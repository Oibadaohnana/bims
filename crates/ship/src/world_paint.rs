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
//! # What is never turned with the ship
//!
//! The starfield and anything drawn because it is *out there* — a station
//! alongside, everything on the map. Those are in the world, and the world
//! does not tip over when the ship does.
//!
//! # Head up
//!
//! The one exception is the player's to switch on: `Game::head_up` turns the
//! *camera* instead, by the heading undone, so the ship is drawn the way it
//! was laid out and the sky and the map turn round it. Nothing above changes
//! shape — the ship goes through `Game::ship_turn`, which is the heading plus
//! the camera's turn, and everything out there is pushed square to the
//! window as before and then turned by `Game::camera_turn` after the fact
//! (`DrawList::turn_from`). North up, both turns are what they always were.

use flight::Phase;
use shipdesign::parts::{Layer, TILE};
use world::ShipState;
use worldgen::math::{DVec2, dvec2};
use worldgen::{BodyKind, Node, StationKind};

use crate::draw::{Color, DrawList};
use crate::game::{Game, ViewMode};
use crate::hull;
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

/// Where one of the crew lands in the camera's units, for the host's name
/// over their head. The same arithmetic the room's picture is turned with.
pub fn crew_on_screen(game: &Game, who: u32) -> (f32, f32) {
    on_screen(
        game.world.aboard.position(who),
        game.world.ship.dynamics.centre_of_mass,
        game.ship_turn(),
    )
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

    // Everything out there, drawn square to the window and then turned with
    // the camera — which is not at all unless the view is head up.
    let out_there = list.len();
    starfield(game, list);
    // Whatever the ship is alongside, under the hull: a station the ship is
    // docked to has the hull inside its ring, and a planet is the ground.
    local_node(game, list);
    list.turn_from(out_there, game.camera_turn() as f32);

    // The ship, drawn in its own frame — design units about the design's
    // origin, the grid it was laid out in — and turned with it at the end.
    // One turn for the whole picture, so a picture made of many shapes only
    // has to be right the once.
    let design = &game.world.ship.design;
    let grid = design.grid();
    let firing = game.firing();
    let centre = game.world.ship.dynamics.centre_of_mass;
    let centre = (centre.x as f32, centre.y as f32);
    let mut ship = DrawList::default();

    // Under everything: the rim that makes the hull a body against the
    // stars, and the exhaust, which shows where it clears the stern and
    // never over the deck.
    hull::shadow(&mut ship, design, &grid);
    hull::exhaust(&mut ship, design, &grid, firing, centre, game.frame);

    // Frame, then deck, then what is standing on them — the same order the
    // design phase paints in, so the two views read as one ship. The parts
    // the room aboard draws for itself — the galley, the heads, the table,
    // the bunks, the bay, the locker — are left to it: it has the pictures.
    // The hull's own working parts have pictures of their own in `hull`.
    let rooms = bims::aboard::drawn_by_room(design);
    let tile = TILE as f32;
    for layer in [
        Layer::Structure,
        Layer::Floor,
        Layer::Object,
        Layer::Utility,
    ] {
        for part in &design.parts {
            if part.layer() != layer || rooms.contains(&part.id) {
                continue;
            }
            if layer == Layer::Object && hull::part(&mut ship, part, &grid, firing) {
                continue;
            }
            let color = match layer {
                Layer::Structure => FRAME,
                Layer::Floor => DECK,
                _ => PART_COLORS[part.kind as usize],
            };
            let inset = if layer == Layer::Object { 3.0 } else { 0.0 };
            for (x, y) in part.tiles() {
                let m = tile_middle(x, y);
                ship.push(
                    crate::draw::KIND_RECT,
                    m.x as f32,
                    m.y as f32,
                    tile - inset,
                    tile - inset,
                    0.0,
                    if layer == Layer::Object { 4.0 } else { 0.0 },
                    0.0,
                    color,
                );
            }
        }
    }
    hull::lights(&mut ship, design, &grid, game.frame);

    // The tile under the pointer, rung. Part of the ship, so turned with it.
    if let Some((x, y)) = game.hover
        && design.holds((x, y))
    {
        let m = tile_middle(x as u32, y as u32);
        ship.push(
            crate::draw::KIND_RECT,
            m.x as f32,
            m.y as f32,
            tile,
            tile,
            0.0,
            3.0,
            2.0,
            GLOW,
        );
    }

    let turn = game.ship_turn() as f32;
    list.append_turned(ship.shapes(), centre, turn);
    // The room aboard — its fixtures, the deck's mess, the crew and what
    // they are carrying — as the room drew it, over the hull and turned the
    // same way. The room's draw buffer is the same twelve floats a shape as
    // this one, since the format is shared across all three cdylibs. Over
    // the ship's picture rather than in it because it is a separate buffer;
    // over the lights and the hover ring too, which touches nothing the
    // room draws.
    list.append_turned(game.world.aboard.room.shapes(), centre, turn);
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

    // The screen pixels to cover. The window, unless the field is about to be
    // turned round the ship: then a square about the ship's own pixel wide
    // enough to reach the furthest corner, or the corners would be bare
    // after the turn. Worked out from the corners rather than the diagonal
    // because the pan has the ship off the middle.
    let (x0, y0, x1, y1) = if game.head_up {
        let (ox, oy) = (camera.offset_x() as f64, camera.offset_y() as f64);
        let reach = ox
            .max(camera.width as f64 - ox)
            .hypot(oy.max(camera.height as f64 - oy));
        (ox - reach, oy - reach, ox + reach, oy + reach)
    } else {
        (0.0, 0.0, camera.width as f64, camera.height as f64)
    };
    // The tile the first repeat sits in, and how many repeats reach the far
    // side. A repeat is `FIELD` wide, so the one holding `x0` starts at the
    // multiple of `FIELD` at or below it.
    let (first_x, first_y) = ((x0 / FIELD).floor() as i32, (y0 / FIELD).floor() as i32);
    let across = ((x1 - x0) / FIELD).ceil() as i32 + 1;
    let down = ((y1 - y0) / FIELD).ceil() as i32 + 1;

    for (i, layer) in game.stars.layers.iter().enumerate() {
        let slide = scroll.scale(Starfield::factor(i));
        for speck in layer {
            let base = dvec2(
                (speck.at.x + slide.x).rem_euclid(FIELD),
                (speck.at.y + slide.y).rem_euclid(FIELD),
            );
            for tx in first_x..first_x + across {
                for ty in first_y..first_y + down {
                    let px = base.x + tx as f64 * FIELD;
                    let py = base.y + ty as f64 * FIELD;
                    if px < x0 || px > x1 || py < y0 || py > y1 {
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
    // Sized against the hull: a station is something the ship fits inside
    // the ring of, and a planet is something the ship is a speck against.
    let hull = game.world.ship.design.build_area as f32 * TILE as f32;
    match node {
        Node::Station(id) => {
            if let Some(station) = game.world.system.station(id) {
                paint_station(list, x, y, hull * 1.6, station.kind, 6.0);
            }
        }
        Node::Body(id) => {
            if let Some(body) = game.world.system.body(id) {
                paint_body(list, x, y, hull * 4.0, body.kind, 6.0);
            }
        }
    }
}

// --- what a planet and a station look like --------------------------------------
//
// One drawing of each kind, used at two scales: a few pixels across on the
// map, and tiles across when the ship is alongside. Everything is ellipses
// and rectangles, because that is all the draw format has, and everything is
// worked out from `size` — the diameter — so the same picture reads at both.
// `thin` is a line width in the caller's units, since a hairline on the map
// is a different number from a hairline alongside.

const RIM: Color = Color::rgba(0.02, 0.03, 0.04, 0.55);
const LIGHT: Color = Color::rgba(1.0, 1.0, 1.0, 0.16);
const SHADE: Color = Color::rgba(0.0, 0.0, 0.0, 0.18);

fn disc(list: &mut DrawList, x: f32, y: f32, d: f32, color: Color) {
    list.ellipse(x, y, d, d, color);
}

fn ring(list: &mut DrawList, x: f32, y: f32, w: f32, h: f32, rot: f32, thin: f32, color: Color) {
    list.push(crate::draw::KIND_ELLIPSE, x, y, w, h, rot, 0.0, thin, color);
}

/// A planet or a belt, `size` across, centred on `(x, y)`.
pub fn paint_body(list: &mut DrawList, x: f32, y: f32, size: f32, kind: BodyKind, thin: f32) {
    let color = body_color(Some(kind));
    let r = size / 2.0;
    match kind {
        BodyKind::RockyPlanet => {
            disc(list, x, y, size, color);
            // Continents: three darker patches, and a lit limb.
            for (dx, dy, dw, dh) in [
                (-0.25, -0.15, 0.40, 0.28),
                (0.20, 0.10, 0.34, 0.36),
                (-0.05, 0.36, 0.26, 0.18),
            ] {
                list.ellipse(x + dx * r, y + dy * r, dw * size, dh * size, SHADE);
            }
            list.ellipse(x - 0.3 * r, y - 0.3 * r, 0.42 * size, 0.42 * size, LIGHT);
            ring(list, x, y, size, size, 0.0, thin, RIM);
        }
        BodyKind::GasGiant => {
            disc(list, x, y, size, color);
            // Bands across the face, darker towards the poles, then a ring
            // seen a little from above.
            for (dy, dh, dark) in [
                (-0.55, 0.16, 0.22),
                (-0.15, 0.12, 0.10),
                (0.30, 0.18, 0.16),
                (0.65, 0.12, 0.24),
            ] {
                let half = (1.0f32 - dy * dy).max(0.0).sqrt();
                list.ellipse(
                    x,
                    y + dy * r,
                    half * size * 0.98,
                    dh * size,
                    SHADE.alpha(dark),
                );
            }
            list.ellipse(x - 0.3 * r, y - 0.35 * r, 0.4 * size, 0.3 * size, LIGHT);
            ring(list, x, y, size, size, 0.0, thin, RIM);
            ring(
                list,
                x,
                y,
                size * 1.9,
                size * 0.42,
                -0.3,
                thin * 1.5,
                color.alpha(0.55),
            );
        }
        BodyKind::IceWorld => {
            disc(list, x, y, size, color);
            // A bright cap and a bright limb: the whole thing reads as glare.
            list.ellipse(
                x,
                y - 0.62 * r,
                0.6 * size,
                0.3 * size,
                Color::rgba(1.0, 1.0, 1.0, 0.45),
            );
            list.ellipse(
                x + 0.2 * r,
                y + 0.25 * r,
                0.3 * size,
                0.2 * size,
                SHADE.alpha(0.12),
            );
            list.ellipse(x - 0.3 * r, y - 0.2 * r, 0.4 * size, 0.4 * size, LIGHT);
            ring(list, x, y, size, size, 0.0, thin, RIM);
        }
        BodyKind::AsteroidBelt => {
            // Measured as a point, drawn as a handful of rocks about it — at
            // fixed angles and sizes, so it holds still between frames.
            for (i, scale) in [0.9f32, 0.6, 1.0, 0.5, 0.75, 0.55, 0.8]
                .into_iter()
                .enumerate()
            {
                let a = i as f32 * core::f32::consts::TAU / 7.0 + 0.5;
                let (dx, dy) = (a.cos() * 0.62 * r, a.sin() * 0.55 * r);
                let d = 0.22 * size * scale;
                list.push(
                    crate::draw::KIND_ELLIPSE,
                    x + dx,
                    y + dy,
                    d,
                    d * 0.75,
                    a,
                    0.0,
                    0.0,
                    color,
                );
            }
        }
    }
}

/// A station, `size` across, centred on `(x, y)`.
pub fn paint_station(list: &mut DrawList, x: f32, y: f32, size: f32, kind: StationKind, thin: f32) {
    let color = station_color(Some(kind));
    let r = size / 2.0;
    match kind {
        StationKind::Orbital => {
            // A wheel: a ring, a hub, and four spokes.
            ring(list, x, y, size, size, 0.0, thin * 2.0, color);
            disc(list, x, y, 0.36 * size, color);
            for i in 0..2 {
                let a = i as f32 * core::f32::consts::FRAC_PI_2;
                list.push(
                    crate::draw::KIND_RECT,
                    x,
                    y,
                    size,
                    thin * 1.2,
                    a,
                    0.0,
                    0.0,
                    color.alpha(0.8),
                );
            }
        }
        StationKind::Refinery => {
            // Two tanks side by side, a stack between them, and a flare.
            for dx in [-0.42f32, 0.42] {
                list.ellipse(x + dx * r, y + 0.2 * r, 0.52 * size, 0.56 * size, color);
                ring(
                    list,
                    x + dx * r,
                    y + 0.2 * r,
                    0.52 * size,
                    0.56 * size,
                    0.0,
                    thin,
                    RIM,
                );
            }
            list.rect(
                x,
                y - 0.2 * r,
                0.16 * size,
                0.9 * size,
                thin,
                color.alpha(0.9),
            );
            disc(
                list,
                x,
                y - 0.72 * r,
                0.24 * size,
                Color::rgb(1.0, 0.72, 0.30),
            );
        }
        StationKind::MiningOutpost => {
            // A rig set into its rock: a square turned on its corner, with
            // the rubble it is working beside it.
            list.push(
                crate::draw::KIND_RECT,
                x,
                y,
                0.6 * size,
                0.6 * size,
                core::f32::consts::FRAC_PI_4,
                thin,
                0.0,
                color,
            );
            ring(
                list,
                x,
                y,
                0.6 * size,
                0.6 * size,
                core::f32::consts::FRAC_PI_4,
                thin,
                RIM,
            );
            for (dx, dy, d) in [(0.55f32, -0.35, 0.28), (0.62, 0.3, 0.2), (-0.6, 0.4, 0.22)] {
                disc(
                    list,
                    x + dx * r,
                    y + dy * r,
                    d * size,
                    body_color(Some(BodyKind::AsteroidBelt)),
                );
            }
        }
        StationKind::Derelict => {
            // A wheel that has come apart: the ring dim and broken — drawn
            // as three arcs' worth of short straight pieces, since the
            // format has no arcs and painting a gap over it would paint
            // over whatever is underneath — a dark hub, and a scatter of
            // what came off.
            let pieces = 10;
            for i in 0..pieces {
                if i == 2 || i == 3 {
                    continue; // the bite
                }
                let a = i as f32 * core::f32::consts::TAU / pieces as f32;
                let (px, py) = (x + a.cos() * r, y + a.sin() * r);
                let len = core::f32::consts::TAU * r / pieces as f32 * 0.85;
                list.push(
                    crate::draw::KIND_RECT,
                    px,
                    py,
                    thin * 2.0,
                    len,
                    a,
                    0.0,
                    0.0,
                    color.alpha(0.7),
                );
            }
            disc(list, x, y, 0.3 * size, color.alpha(0.5));
            ring(
                list,
                x,
                y,
                0.3 * size,
                0.3 * size,
                0.0,
                thin,
                color.alpha(0.7),
            );
            for (dx, dy, d) in [
                (0.9f32, -0.95, 0.12),
                (1.15, -0.55, 0.09),
                (0.65, -1.2, 0.08),
            ] {
                disc(list, x + dx * r, y + dy * r, d * size, color);
            }
        }
        StationKind::Relay => {
            // A hub, a mast, and a dish looking out.
            disc(list, x, y + 0.2 * r, 0.34 * size, color);
            list.rect(x, y - 0.2 * r, thin * 1.5, 0.6 * size, 0.0, color);
            ring(
                list,
                x,
                y - 0.55 * r,
                0.7 * size,
                0.34 * size,
                0.0,
                thin * 1.6,
                color,
            );
            disc(list, x, y - 0.55 * r, 0.1 * size, color);
        }
    }
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

    // The whole of the system is out there, so the whole of it is turned with
    // the camera at the end — square to the window unless the view is head
    // up. The marker for the ship is drawn after, through its own turn.
    let out_there = list.len();

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

    // Orbits under everything: a faint ring through each known body, so the
    // map reads as a system rather than as dots. A belt's is a little
    // stronger, because a belt *is* its orbit.
    let thin = (1.0 / scale) as f32;
    for &node in &game.world.discovered {
        let Node::Body(id) = node else { continue };
        let Some(body) = game.world.system.body(id) else {
            continue;
        };
        let d = (body.position.length() * 2.0) as f32;
        let strength = if body.kind == BodyKind::AsteroidBelt {
            0.30
        } else {
            0.12
        };
        ring(list, sx, sy, d, d, 0.0, thin, MUTED.alpha(strength));
    }

    // The bodies first and the stations over them, because a station sits
    // in orbit of its body and at this scale that is on top of it. Sized in
    // pixels, like the icons they are: a planet drawn to scale would be a
    // fraction of one.
    let size = (26.0 / scale) as f32;
    for &node in &game.world.discovered {
        let Node::Body(id) = node else { continue };
        let Some(at) = game.world.system.absolute_position(node) else {
            continue;
        };
        let (x, y) = place(at);
        if let Some(body) = game.world.system.body(id) {
            paint_body(list, x, y, size, body.kind, thin);
        }
    }
    for &node in &game.world.discovered {
        let Node::Station(id) = node else { continue };
        let Some(at) = game.world.system.absolute_position(node) else {
            continue;
        };
        let (x, y) = place(at);
        if let Some(station) = game.world.system.station(id) {
            paint_station(list, x, y, size * 0.75, station.kind, thin * 1.5);
        }
    }

    // What the helm is aimed at, ringed, so a click has visibly landed on
    // the thing and not beside it.
    let aimed_at = match game.aimed {
        Some(flight::Target::Body(id)) => game.world.system.absolute_position(Node::Body(id)),
        Some(flight::Target::Station(id)) => game.world.system.absolute_position(Node::Station(id)),
        Some(flight::Target::Point(p)) => Some(p),
        None => None,
    };
    if let Some(at) = aimed_at {
        let (x, y) = place(at);
        let d = (26.0 / scale) as f32;
        ring(list, x, y, d, d, 0.0, (1.5 / scale) as f32, GLOW.alpha(0.9));
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

    list.turn_from(out_there, game.camera_turn() as f32);

    // The ship, pointing where it is pointing — a little hull with fins, so
    // it says which way round it is and that it is the ship. Head up, that
    // is straight up, and it is the map that says where north went.
    hull::marker(list, game.ship_turn() as f32, (18.0 / scale) as f32, GLOW);
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
