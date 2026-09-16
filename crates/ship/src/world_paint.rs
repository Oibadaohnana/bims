//! Drawing the game: the ship at tile scale, and the system on a map.
//!
//! # The one piece of geometry that matters
//!
//! The camera never rotates, so the **ship** does. A point in the design grid
//! lands on screen at
//!
//! ```text
//! screen = R(heading) · (design − centre_of_mass)
//! ```
//!
//! where `R` turns clockwise in a y-down coordinate system — which is exactly
//! what the canvas's own `rotate()` does, so each tile is emitted with `rot`
//! set to the heading and the host turns it.
//!
//! That is not a second opinion about which way round the ship is. It falls
//! out of `flight::angle::rotate_design` and the screen's y-flip — a rotation
//! composed with the flip between the grid's y-down and the system's y-up. The
//! tests next door are what keep the two in step:
//! `a_screen_point_maps_back_to_the_tile_it_is_over` reads this arithmetic
//! backwards at four headings, and it fails the moment a sign here moves.
//!
//! # What is never turned
//!
//! The starfield and anything drawn because it is *out there* — a station
//! alongside, everything on the map. Those are in the world, and the world
//! does not tip over when the ship does.

use flight::Phase;
use shipdesign::parts::{Layer, TILE};
use world::ShipState;
use worldgen::math::{DVec2, dvec2};
use worldgen::{BodyKind, Node, StationKind};

use crate::draw::{Color, DrawList};
use crate::game::{Game, ViewMode};
use crate::paint::PART_COLORS;
use crate::starfield::{FIELD, Starfield};

const VOID: Color = Color::rgb(0.02, 0.03, 0.04);
const FRAME: Color = Color::rgb(0.10, 0.11, 0.13);
const DECK: Color = Color::rgb(0.13, 0.15, 0.18);
const GLOW: Color = Color::rgb(0.38, 0.86, 0.95);
const STAR: Color = Color::rgb(0.98, 0.88, 0.55);
const MUTED: Color = Color::rgba(0.55, 0.85, 0.95, 0.22);

/// One colour per lobby slot. The route line is drawn in the colour of
/// whoever set the destination, which is the whole of what
/// `destination_set_by` is for.
pub static PLAYER_COLORS: [Color; 4] = [
    Color::rgb(0.38, 0.86, 0.95),
    Color::rgb(0.98, 0.72, 0.35),
    Color::rgb(0.55, 0.90, 0.60),
    Color::rgb(0.85, 0.58, 0.92),
];

pub fn player_color(slot: u32) -> Color {
    PLAYER_COLORS[(slot as usize) % PLAYER_COLORS.len()]
}

/// The whole frame, whichever view is up.
pub fn paint(game: &Game, list: &mut DrawList) {
    list.clear();
    match game.mode {
        ViewMode::Ship => paint_ship(game, list),
        ViewMode::Map => paint_map(game, list),
    }
}

// --- the ship ---------------------------------------------------------------

/// A design point, in the camera's units about the ship.
fn on_screen(design: DVec2, centre: DVec2, heading: f64) -> (f32, f32) {
    let d = design.sub(centre);
    let (s, c) = (heading.sin(), heading.cos());
    ((d.x * c - d.y * s) as f32, (d.x * s + d.y * c) as f32)
}

/// The middle of a tile, in design world units.
fn tile_middle(x: u32, y: u32) -> DVec2 {
    dvec2(
        (x as f64 + 0.5) * TILE as f64,
        (y as f64 + 0.5) * TILE as f64,
    )
}

fn paint_ship(game: &Game, list: &mut DrawList) {
    let camera = &game.ship_view;
    let scale = camera.scale().max(1e-9);

    // The void first, big enough to cover the canvas at any pan.
    let half_w = camera.width / scale;
    let half_h = camera.height / scale;
    list.rect(0.0, 0.0, half_w * 3.0, half_h * 3.0, 0.0, VOID);

    starfield(game, list);

    let heading = game.world.ship.heading;
    let centre = game.world.ship.dynamics.centre_of_mass;
    let tile = TILE as f32;

    // Frame, then deck, then what is standing on them — the same order the
    // design phase paints in, so the two views read as one ship.
    for layer in [
        Layer::Structure,
        Layer::Floor,
        Layer::Object,
        Layer::Utility,
    ] {
        for part in &game.world.ship.design.parts {
            if part.layer() != layer {
                continue;
            }
            let color = match layer {
                Layer::Structure => FRAME,
                Layer::Floor => DECK,
                _ => PART_COLORS[part.kind as usize],
            };
            let inset = if layer == Layer::Object { 3.0 } else { 0.0 };
            for (x, y) in part.tiles() {
                let (sx, sy) = on_screen(tile_middle(x, y), centre, heading);
                list.push(
                    crate::draw::KIND_RECT,
                    sx,
                    sy,
                    tile - inset,
                    tile - inset,
                    heading as f32,
                    if layer == Layer::Object { 4.0 } else { 0.0 },
                    0.0,
                    color,
                );
            }
        }
    }

    // The tile under the pointer, rung. Turned with the ship, because it is
    // part of the ship.
    if let Some((x, y)) = game.hover
        && game.world.ship.design.holds((x, y))
    {
        let (sx, sy) = on_screen(tile_middle(x as u32, y as u32), centre, heading);
        list.push(
            crate::draw::KIND_RECT,
            sx,
            sy,
            tile,
            tile,
            heading as f32,
            3.0,
            2.0,
            GLOW,
        );
    }

    local_node(game, list);
}

/// The three parallax layers.
///
/// In screen pixels, turned back into camera units on the way out — a
/// backdrop covers the window rather than the world, so it does not tile four
/// hundred times over when the view is zoomed out.
fn starfield(game: &Game, list: &mut DrawList) {
    let camera = &game.ship_view;
    let scale = camera.scale().max(1e-9);
    let velocity = game
        .world
        .trip_state()
        .map(|state| state.velocity)
        .unwrap_or(DVec2::ZERO);
    let scroll = Starfield::scroll(game.world.ship.position(), velocity);

    let across = (camera.width as f64 / FIELD).ceil() as i32 + 1;
    let down = (camera.height as f64 / FIELD).ceil() as i32 + 1;

    for (i, layer) in game.stars.layers.iter().enumerate() {
        let slide = scroll.scale(Starfield::factor(i));
        for speck in layer {
            let base = dvec2(
                (speck.at.x + slide.x).rem_euclid(FIELD),
                (speck.at.y + slide.y).rem_euclid(FIELD),
            );
            for tx in 0..across {
                for ty in 0..down {
                    let px = base.x + tx as f64 * FIELD;
                    let py = base.y + ty as f64 * FIELD;
                    if px > camera.width as f64 || py > camera.height as f64 {
                        continue;
                    }
                    // Screen pixels back into the camera's own units, so the
                    // speck stays the same size on screen at any zoom.
                    let x = (px as f32 - camera.offset_x()) / scale;
                    let y = (py as f32 - camera.offset_y()) / scale;
                    let size = speck.size / scale;
                    list.ellipse(
                        x,
                        y,
                        size,
                        size,
                        Color::rgba(1.0, 1.0, 1.0, speck.brightness),
                    );
                }
            }
        }
    }
}

/// Whatever the ship is alongside, drawn where it actually is.
///
/// **Never turned.** A station does not tip over because the ship it is
/// holding has rolled, and the moment it did the picture would stop saying
/// anything about which way anybody was pointing.
fn local_node(game: &Game, list: &mut DrawList) {
    let Some(node) = game.world.ship.frame.node() else {
        return;
    };
    let Some(at) = game.world.system.absolute_position(node) else {
        return;
    };
    let offset = at.sub(game.world.ship.position());
    // System `+y` is north and the screen's `y` grows down.
    let (x, y) = (offset.x as f32, -offset.y as f32);
    let size = match node {
        Node::Station(_) => TILE as f32 * 6.0,
        Node::Body(_) => TILE as f32 * 30.0,
    };
    list.ellipse(x, y, size, size, MUTED);
    list.push(
        crate::draw::KIND_ELLIPSE,
        x,
        y,
        size,
        size,
        0.0,
        0.0,
        3.0,
        GLOW,
    );
}

// --- the map -----------------------------------------------------------------

fn paint_map(game: &Game, list: &mut DrawList) {
    let camera = &game.map_view;
    let scale = camera.scale().max(1e-30) as f64;
    let here = game.world.ship.position();

    let half_w = camera.width as f64 / scale;
    let half_h = camera.height as f64 / scale;
    list.rect(
        0.0,
        0.0,
        (half_w * 3.0) as f32,
        (half_h * 3.0) as f32,
        0.0,
        VOID,
    );

    // Where a system position lands, in the camera's units about the ship.
    let place = |at: DVec2| {
        let offset = at.sub(here);
        (offset.x as f32, -offset.y as f32)
    };

    // How far the crew can see. Drawn round the ship because that is where the
    // sensors are, and it is the one thing on the map that explains why the
    // rest of it is empty.
    let range = game.world.detection_range() as f32;
    list.push(
        crate::draw::KIND_ELLIPSE,
        0.0,
        0.0,
        range * 2.0,
        range * 2.0,
        0.0,
        0.0,
        (2.0 / scale) as f32,
        MUTED,
    );

    // The star. Always discovered: it is the thing the system is named after
    // and the origin everything else is measured from.
    let (sx, sy) = place(DVec2::ZERO);
    let star = (18.0 / scale) as f32;
    list.ellipse(sx, sy, star, star, STAR);

    for &node in &game.world.discovered {
        let Some(at) = game.world.system.absolute_position(node) else {
            continue;
        };
        let (x, y) = place(at);
        let size = (10.0 / scale) as f32;
        match node {
            Node::Body(id) => {
                let kind = game.world.system.body(id).map(|b| b.kind);
                list.ellipse(x, y, size, size, body_color(kind));
            }
            Node::Station(id) => {
                let kind = game.world.system.station(id).map(|s| s.kind);
                list.rect(x, y, size, size, size * 0.2, station_color(kind));
            }
        }
    }

    // The route, in the colour of whoever set it.
    if let ShipState::Travelling { plan, .. } = &game.world.ship.state {
        let colour = player_color(game.world.ship.destination_set_by.unwrap_or(0));
        let (x0, y0) = place(plan.start);
        let (x1, y1) = place(plan.arrival);
        list.line(x0, y0, x1, y1, (2.0 / scale) as f32, colour);
        let end = (7.0 / scale) as f32;
        list.push(
            crate::draw::KIND_ELLIPSE,
            x1,
            y1,
            end * 2.0,
            end * 2.0,
            0.0,
            0.0,
            (2.0 / scale) as f32,
            colour,
        );
    }

    // The ship, pointing where it is pointing. A rectangle turned to the
    // heading, because the draw format has no triangle and a dot says nothing
    // about which way round anybody is.
    let heading = game.world.ship.heading as f32;
    let long = (16.0 / scale) as f32;
    list.push(
        crate::draw::KIND_RECT,
        0.0,
        0.0,
        long * 0.35,
        long,
        heading,
        0.0,
        0.0,
        GLOW,
    );
}

fn body_color(kind: Option<BodyKind>) -> Color {
    match kind {
        Some(BodyKind::RockyPlanet) => Color::rgb(0.72, 0.56, 0.44),
        Some(BodyKind::GasGiant) => Color::rgb(0.82, 0.70, 0.44),
        Some(BodyKind::IceWorld) => Color::rgb(0.66, 0.84, 0.92),
        Some(BodyKind::AsteroidBelt) => Color::rgb(0.55, 0.55, 0.58),
        None => Color::rgb(0.6, 0.6, 0.6),
    }
}

fn station_color(kind: Option<StationKind>) -> Color {
    match kind {
        Some(StationKind::Orbital) => Color::rgb(0.58, 0.82, 0.90),
        Some(StationKind::Refinery) => Color::rgb(0.86, 0.62, 0.30),
        Some(StationKind::MiningOutpost) => Color::rgb(0.70, 0.66, 0.46),
        Some(StationKind::Derelict) => Color::rgb(0.52, 0.48, 0.50),
        Some(StationKind::Relay) => Color::rgb(0.62, 0.74, 0.92),
        None => Color::rgb(0.6, 0.6, 0.6),
    }
}

/// Which phase to colour a readout by. Not drawn here — the host does the
/// words — but the codes have to come from somewhere and this is where the
/// rest of the view's vocabulary lives.
pub fn phase_code(game: &Game) -> u32 {
    game.world
        .trip_state()
        .map(|state| state.phase.code())
        .unwrap_or(Phase::Arrived.code())
}
