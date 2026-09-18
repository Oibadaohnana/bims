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

use bims::sight::Stance;
use flight::Phase;
use shipdesign::parts::{Layer, PartKind, Rotation, TILE};
use shipdesign::{Grid, ShipDesign};
use world::ShipState;
use worldgen::math::{DVec2, dvec2};
use worldgen::{BodyKind, Node, StationKind};

use crate::draw::{Color, DrawList};
use crate::game::{Game, Overlay, ViewMode};
use crate::hull;
use crate::paint::PART_COLORS;
use crate::starfield::FIELD;

const VOID: Color = Color::rgb(0.02, 0.03, 0.04);
const FRAME: Color = Color::rgb(0.10, 0.11, 0.13);
const DECK: Color = Color::rgb(0.13, 0.15, 0.18);
const GLOW: Color = Color::rgb(0.38, 0.86, 0.95);
const STAR: Color = Color::rgb(0.98, 0.88, 0.55);
const MUTED: Color = Color::rgba(0.55, 0.85, 0.95, 0.22);
/// The electricity overlay: a part on a live network, and one that is not.
const LIVE: Color = Color::rgb(0.50, 0.90, 0.60);
const DEAD: Color = Color::rgb(0.98, 0.45, 0.32);

/// What the rocks of a mining site are drawn in, by `world::Rock` code:
/// stone, iron ore and galvum. Stone is the dull brown of the belt's icon,
/// iron a silver that catches the light, galvum the purple nothing else
/// aboard is. Told apart at a glance, which is the point: which asteroid
/// is the rare one shows through its skin.
pub static ROCK_COLORS: [Color; 3] = [
    Color::rgb(0.42, 0.36, 0.30),
    Color::rgb(0.72, 0.74, 0.78),
    Color::rgb(0.62, 0.32, 0.82),
];
/// The seam between one rock tile and the next, and the lit edge on each.
const ROCK_SEAM: Color = Color::rgba(0.0, 0.0, 0.0, 0.35);
const ROCK_LIT: Color = Color::rgba(1.0, 1.0, 1.0, 0.10);
/// A rock marked to be mined: ringed in the warm colour an order is.
const MARK: Color = Color::rgb(0.98, 0.72, 0.35);
/// A construction site: the blueprint's blue, and how far through the
/// part's own picture shows for a site and for the blueprint in hand.
const BLUEPRINT: Color = Color::rgb(0.45, 0.72, 1.0);
const SITE_FADE: f32 = 0.55;
const GHOST_FADE: f32 = 0.50;
/// Where whoever uses a part would stand, as the designer marks it.
const SPOT: Color = Color::rgba(0.98, 0.82, 0.35, 0.85);

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
    // the camera — which is not at all unless the view is head up. The
    // planet the ship is at is the ground under it; the stations are drawn
    // where they are, already turned, by `stations`.
    let out_there = list.len();
    starfield(game, list);
    local_node(game, list);
    list.turn_from(out_there, game.camera_turn() as f32);
    stations(game, list);

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

    // The rocks of the mining site, if the ship is at one. In the ship's
    // frame — they were laid out on its tile grid — so they go through the
    // same turn the hull does; and under it, since they are outside it.
    rocks(game, &mut ship);

    // Under everything: the rim that makes the hull a body against the
    // stars, and the exhaust, which shows where it clears the stern and
    // never over the deck.
    hull::shadow(&mut ship, design, &grid);
    hull::exhaust(&mut ship, design, &grid, firing, centre, game.frame);

    // The parts the room aboard draws for itself — the galley, the heads,
    // the table, the bunks, the bay, the locker — are left to it: it has the
    // pictures. The airlock is drawn open while the ship is docked, because
    // it is mated to the station's and a way through.
    let rooms = bims::aboard::drawn_by_room(design);
    let mated = game.mated_airlock().map(|id| (id, game.airlock_ajar));
    hull_tiles(&mut ship, design, &grid, firing, &rooms, mated);
    // What is laid out to be built, over the deck it will stand on: each
    // site as the part's own picture, shown through, in the blueprint's
    // blue, with how much of it has arrived along the bottom. And the
    // blueprint in the player's hand, over the tile the pointer is on.
    sites(game, &grid, &mut ship);
    blueprint(game, &grid, &mut ship);
    hull::lights(&mut ship, design, &grid, game.frame);

    // The tile under the pointer, rung. Part of the ship, so turned with it
    // — and a rock under the pointer while the player is marking rocks,
    // which is on the same grid.
    let tile = TILE as f32;
    let over_rock = game.marking
        && game
            .hover
            .is_some_and(|(x, y)| game.world.site_here().is_some_and(|s| s.at(x, y).is_some()));
    if let Some((x, y)) = game.hover
        && (design.holds((x, y)) || over_rock)
    {
        let m = signed_tile_middle(x, y);
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
    // The room's picture is in the room's own units, which are the ship's
    // shifted by the ship's offset into them — nothing while the ship is
    // on its own, the join's shift while it is docked.
    let offset = game.world.aboard.offset;
    let room_centre = (centre.0 + offset.x as f32, centre.1 + offset.y as f32);
    list.append_turned(game.world.aboard.room.shapes(), room_centre, turn);
    // The electricity overlay, over the lot — the room's fixtures included,
    // since a galley that draws power is rung as much as a reactor is — in
    // the ship's frame and turned with it like the hull.
    if game.overlay == Overlay::Electricity {
        let mut over = DrawList::default();
        electricity(&mut over, design, &grid);
        list.append_turned(over.shapes(), centre, turn);
    }
}

/// The middle of a tile that may be off the hull, in design world units.
fn signed_tile_middle(x: i32, y: i32) -> DVec2 {
    dvec2(
        (x as f64 + 0.5) * TILE as f64,
        (y as f64 + 0.5) * TILE as f64,
    )
}

/// The mining site's rocks, a tile each, in the ship's frame: every tile
/// standing, in its kind's colour with a seam round it and a lit edge, and
/// the marked ones ringed. Nothing while the ship is not at a site.
fn rocks(game: &Game, ship: &mut DrawList) {
    let Some(site) = game.world.site_here() else {
        return;
    };
    let tile = TILE as f32;
    for rock in &site.tiles {
        let m = signed_tile_middle(rock.x, rock.y);
        let (x, y) = (m.x as f32, m.y as f32);
        let color = ROCK_COLORS[rock.kind.code() as usize % ROCK_COLORS.len()];
        list_rock(ship, x, y, tile, color);
    }
    for &(x, y) in &site.marked {
        let m = signed_tile_middle(x, y);
        let (x, y) = (m.x as f32, m.y as f32);
        ship.push(
            crate::draw::KIND_RECT,
            x,
            y,
            tile - 4.0,
            tile - 4.0,
            0.0,
            4.0,
            0.0,
            MARK.alpha(0.35),
        );
        ship.push(
            crate::draw::KIND_RECT,
            x,
            y,
            tile - 4.0,
            tile - 4.0,
            0.0,
            4.0,
            4.0,
            MARK,
        );
    }
}

/// One tile of rock: the seam is the tile, the rock is inset into it, and a
/// lighter sliver along its top-left edge is the light on it.
fn list_rock(list: &mut DrawList, x: f32, y: f32, tile: f32, color: Color) {
    list.push(
        crate::draw::KIND_RECT,
        x,
        y,
        tile,
        tile,
        0.0,
        0.0,
        0.0,
        ROCK_SEAM,
    );
    list.push(
        crate::draw::KIND_RECT,
        x,
        y,
        tile - 3.0,
        tile - 3.0,
        0.0,
        5.0,
        0.0,
        color,
    );
    list.push(
        crate::draw::KIND_RECT,
        x - tile * 0.12,
        y - tile * 0.12,
        tile * 0.5,
        tile * 0.5,
        0.0,
        6.0,
        0.0,
        ROCK_LIT,
    );
}

/// The part a site or a blueprint is for, as its own picture, faded into
/// `list`: the hull's or the fittings' picture where there is one, the
/// deck's tile for plating, and the part's colour as a block otherwise —
/// the same three askings as `hull_tiles`, so a ghost of a thing looks
/// like the thing.
fn faded_part(
    list: &mut DrawList,
    design: &ShipDesign,
    grid: &Grid,
    kind: PartKind,
    origin: (u32, u32),
    rotation: Rotation,
    alpha: f32,
) {
    let tile = TILE as f32;
    let part = shipdesign::PlacedPart {
        id: 0,
        kind,
        origin,
        rotation,
    };
    let mut picture = DrawList::default();
    let drawn = kind != PartKind::Floor
        && (hull::part(&mut picture, &part, grid, hull::Firing::NONE, None)
            || crate::fittings::part(&mut picture, &part));
    if !drawn {
        let color = match kind {
            PartKind::Floor => DECK,
            PartKind::Structure => FRAME,
            _ => PART_COLORS[kind as usize],
        };
        for (x, y) in part.tiles() {
            let m = tile_middle(x, y);
            picture.push(
                crate::draw::KIND_RECT,
                m.x as f32,
                m.y as f32,
                tile - 3.0,
                tile - 3.0,
                0.0,
                4.0,
                0.0,
                color,
            );
        }
    }
    let _ = design;
    list.append_faded(picture.shapes(), alpha);
}

/// The box round a part's tiles, in design units: `(x0, y0, x1, y1)`.
fn part_box(kind: PartKind, origin: (u32, u32), rotation: Rotation) -> (f32, f32, f32, f32) {
    let t = TILE as f32;
    let (w, h) = shipdesign::parts::footprint(kind, rotation);
    let x0 = origin.0 as f32 * t;
    let y0 = origin.1 as f32 * t;
    (x0, y0, x0 + w as f32 * t, y0 + h as f32 * t)
}

/// Every construction site, over the deck: the part shown through in the
/// blueprint's blue, ringed, with a bar along its foot for how much of
/// what it is made of has been carried to it. In the ship's frame, since
/// a site is a tile of the ship.
fn sites(game: &Game, grid: &Grid, ship: &mut DrawList) {
    let design = &game.world.ship.design;
    for site in &game.world.builds {
        faded_part(
            ship,
            design,
            grid,
            site.kind,
            site.origin,
            site.rotation,
            SITE_FADE,
        );
        let (x0, y0, x1, y1) = part_box(site.kind, site.origin, site.rotation);
        ship.box_between(x0, y0, x1, y1, 3.0, BLUEPRINT.alpha(0.16));
        ship.stroke_between(x0 + 1.5, y0 + 1.5, x1 - 1.5, y1 - 1.5, 3.0, 2.0, BLUEPRINT);
        // What has arrived, as a share of what it is made of.
        let recipe = site.recipe(design);
        let wanted: u32 = recipe.iter().map(|&(_, u)| u).sum();
        let there: u32 = recipe
            .iter()
            .map(|&(id, u)| site.delivered[id as usize].min(u))
            .sum();
        if wanted > 0 && there > 0 {
            let share = there as f32 / wanted as f32;
            let w = (x1 - x0 - 8.0) * share;
            ship.rect(
                x0 + 4.0 + w / 2.0,
                y1 - 6.0,
                w,
                4.0,
                0.0,
                if there >= wanted { LIVE } else { BLUEPRINT },
            );
        }
    }
}

/// The blueprint in the player's hand, over the tile the pointer is on:
/// the part shown through, ringed in the live colour where it would go
/// and the warning colour where it would not — the world's own answer,
/// asked through `Game::ghost_check`. Nothing with no tool in hand, and
/// nothing in the map view, where a tile means nothing.
fn blueprint(game: &Game, grid: &Grid, ship: &mut DrawList) {
    let Some((kind, rotation)) = game.placing else {
        return;
    };
    let Some((x, y)) = game.hover else {
        return;
    };
    if x < 0 || y < 0 {
        return;
    }
    let origin = (x as u32, y as u32);
    let ok = game.ghost_ok();
    let color = if ok { LIVE } else { DEAD };
    faded_part(
        ship,
        &game.world.ship.design,
        grid,
        kind,
        origin,
        rotation,
        GHOST_FADE,
    );
    let (x0, y0, x1, y1) = part_box(kind, origin, rotation);
    ship.box_between(x0, y0, x1, y1, 4.0, color.alpha(0.18));
    ship.stroke_between(x0 + 1.0, y0 + 1.0, x1 - 1.0, y1 - 1.0, 4.0, 2.0, color);
    // Where whoever uses it will stand, as the designer marks it.
    if ok {
        let t = TILE as f32;
        let any = shipdesign::parts::any_side_will_do(kind);
        for (dx, dy) in shipdesign::parts::use_spots(kind, rotation) {
            let tile = (x + dx, y + dy);
            if any && (grid.get(Layer::Floor, tile) == 0 || grid.get(Layer::Object, tile) != 0) {
                continue;
            }
            let m = signed_tile_middle(tile.0, tile.1);
            ship.ellipse(m.x as f32, m.y as f32, t * 0.30, t * 0.30, SPOT);
        }
    }
}

/// Frame, then deck, then what is standing on them — the same order the
/// design phase paints in, so the two views read as one ship. `skip` is
/// what somebody else draws: the parts the room has pictures for. The
/// hull's own working parts have pictures in `hull`; everything else is its
/// colour, a tile at a time.
fn hull_tiles(
    list: &mut DrawList,
    design: &ShipDesign,
    grid: &Grid,
    firing: hull::Firing,
    skip: &[u32],
    open_airlock: Option<(u32, f32)>,
) {
    let tile = TILE as f32;
    // Not the utility layer: the conduit under the deck is the electricity
    // overlay's to draw, and only while that is up — see `electricity`.
    for layer in [Layer::Structure, Layer::Floor, Layer::Object] {
        for part in &design.parts {
            if part.layer() != layer || skip.contains(&part.id) {
                continue;
            }
            if layer == Layer::Object && hull::part(list, part, grid, firing, open_airlock) {
                continue;
            }
            if crate::fittings::part(list, part) {
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
                // The frame under a corner piece is the same half of the
                // tile, as in the design phase: the edge of the ship is the
                // chamfer, not the square behind it.
                if layer == Layer::Structure
                    && let Some(rotation) = hull::diagonal_at(design, grid, (x, y))
                {
                    let c = hull::corner(rotation);
                    list.triangle(m.x as f32, m.y as f32, tile, tile, c.rot, color);
                    continue;
                }
                list.push(
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
}

/// The electricity overlay, over the ship's picture: every part that makes,
/// holds or draws power washed and rung — in the live colour when it is on
/// a network with a reactor, in the warning colour when it is not — and
/// the conduit under the deck drawn on top, the way the design phase draws
/// it, so what is wired to what can be followed. The networks are asked
/// once, for the whole ship, rather than a part at a time.
fn electricity(list: &mut DrawList, design: &ShipDesign, grid: &Grid) {
    let tile = TILE as f32;
    let live: Vec<u32> = shipdesign::networks(design)
        .into_iter()
        .filter(|net| net.live())
        .flat_map(|net| net.parts)
        .collect();
    for part in &design.parts {
        let def = part.kind.def();
        if !(def.supplies() || def.draws() || def.stores()) {
            continue;
        }
        let color = if live.contains(&part.id) { LIVE } else { DEAD };
        for (x, y) in part.tiles() {
            let m = tile_middle(x, y);
            list.rect(m.x as f32, m.y as f32, tile, tile, 0.0, color.alpha(0.18));
            list.stroke_rect(
                m.x as f32,
                m.y as f32,
                tile - 3.0,
                tile - 3.0,
                2.0,
                2.0,
                color,
            );
        }
    }
    for part in &design.parts {
        if part.kind != PartKind::PowerConduit {
            continue;
        }
        for at in part.tiles() {
            let links = crate::fittings::conduit_links(design, grid, at);
            crate::fittings::conduit(list, at, links);
        }
    }
}

/// Every station near enough to be in the picture, drawn where it is and
/// as big as it is, so that one **approaches** rather than appears.
///
/// Three distances, three pictures. Out to `STATION_VISIBLE` a station is a
/// plate the size of its hull with its icon on it — a shape getting nearer,
/// and nothing to simulate. Inside the local frame's radius it is its
/// hull, tile by tile, the parts inside it as their colours. And while the
/// ship is within the residents' range the room aboard it is open, so the
/// fixtures are the room's pictures and the people living there walk about
/// between them — the same way the ship's own room is drawn over its hull.
///
/// A station does not turn, so its picture is turned by the camera alone;
/// the ship's own airlock is mated to the station's while docked, and both
/// are drawn open.
fn stations(game: &Game, list: &mut DrawList) {
    let here = game.world.ship.position();
    let turn = game.camera_turn() as f32;
    let docked = game.world.ship.state.alongside();
    for station in &game.world.stations {
        let clearance = station.clearance(here);
        if clearance > world::data::STATION_VISIBLE {
            continue;
        }
        // Where the station's middle lands: system offset, y flipped, then
        // turned with the camera.
        let offset = station.centre().sub(here);
        let at = crate::game::turned(offset.x as f32, -offset.y as f32, turn);
        let side = station.design.build_area as f32 * TILE as f32;
        let middle = (side / 2.0, side / 2.0);
        let residents = game
            .world
            .residents
            .as_ref()
            .filter(|r| r.station == station.id);

        // Somebody else's station is under a black fog until the crew
        // have looked into it — see `bims::sight` — and its room, which
        // draws that fog, is only open within the residents' range. Out
        // to there it stays the plate it was from further off: a shape
        // and a kind, and nothing of what is inside. The crew's own is
        // its hull from the local frame in, as before.
        let stranger = game.world.stance(station.id) != Stance::Friendly;
        if clearance > world::data::LOCAL_RADIUS_STATION || (stranger && residents.is_none()) {
            let hull = (station.design.build_area as f32 - 2.0) * TILE as f32;
            let plate = if stranger {
                hull::HULL_UNKNOWN
            } else {
                hull::HULL_FAR
            };
            list.push(
                crate::draw::KIND_RECT,
                at.0,
                at.1,
                hull,
                hull,
                turn,
                8.0,
                0.0,
                plate,
            );
            paint_station(list, at.0, at.1, hull * 0.8, station.kind, 6.0);
            continue;
        }

        let grid = station.design.grid();
        // The station's door opens with the ship's: they are one passage.
        let open = docked
            .filter(|&id| id == station.id)
            .and(station.port().map(|p| (p.part_id, game.airlock_ajar)));
        let skip: Vec<u32> = match residents {
            Some(_) => bims::aboard::drawn_by_room(&station.design),
            None => Vec::new(),
        };
        let mut picture = DrawList::default();
        hull::shadow(&mut picture, &station.design, &grid);
        hull_tiles(
            &mut picture,
            &station.design,
            &grid,
            hull::Firing::NONE,
            &skip,
            open,
        );
        hull::lights(&mut picture, &station.design, &grid, game.frame);
        list.append_turned_at(picture.shapes(), middle, turn, at);
        if let Some(residents) = residents {
            list.append_turned_at(residents.aboard.room.shapes(), middle, turn, at);
        }
    }
}

/// Where one of a station's residents lands in the camera's units, for the
/// host's name over their head: the same turn and shift the room's picture
/// went through.
pub fn resident_on_screen(game: &Game, who: u32) -> (f32, f32) {
    let Some(residents) = &game.world.residents else {
        return (0.0, 0.0);
    };
    let Some(station) = game.world.station(residents.station) else {
        return (0.0, 0.0);
    };
    let turn = game.camera_turn();
    let side = station.design.build_area as f64 * TILE as f64;
    let (x, y) = on_screen(
        residents.aboard.position(who),
        dvec2(side / 2.0, side / 2.0),
        turn,
    );
    let offset = station.centre().sub(game.world.ship.position());
    let at = crate::game::turned(offset.x as f32, -offset.y as f32, turn as f32);
    (x + at.0, y + at.1)
}

/// The three parallax layers.
///
/// In screen pixels, turned back into camera units on the way out — a
/// backdrop covers the window rather than the world, so it does not tile four
/// hundred times over when the view is zoomed out. How far each layer has
/// streamed is the field's own clock (`Starfield::advance`, from
/// `ship_render`); this only draws it where it has got to.
fn starfield(game: &Game, list: &mut DrawList) {
    let camera = &game.ship_view;
    let scale = camera.scale().max(1e-9);

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
        let slide = game.stars.slid[i];
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

/// The body the ship is alongside, drawn where it actually is: the ground.
///
/// **Never turned.** A planet does not tip over because the ship holding
/// beside it has rolled. Stations used to be drawn here too, a ring the
/// hull sat inside; they are places now, with hulls of their own, and
/// [`stations`] draws every one in range where it stands.
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
        Node::Station(_) => {}
        Node::Body(id) => {
            // A belt the ship is holding at is drawn as its rocks, tile by
            // tile in the ship's frame — see `rocks` — not as the handful
            // of stones the map's icon is.
            if game.world.site_here().is_some() {
                return;
            }
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
