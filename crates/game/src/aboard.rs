//! The room, laid out from a ship design.
//!
//! This is where the two halves of the game meet: `crates/game` is the
//! Bims — their needs, their errands, the galley and the heads and the bay,
//! and the drawing of all of it — and `shipdesign` is the ship the player
//! laid out. Everything the room's errands walk to is a rect on a
//! [`Room`], and everything the designer placed is a part on a tile grid;
//! this module turns the second into the first, and nothing else in the
//! crate has heard of a `ShipDesign`.
//!
//! It is the one module the native probes do **not** stand up — they
//! declare the crate's modules by `#[path]` and link no other crate — which
//! is why the rest of the room takes a [`Layout`] of plain rects rather than
//! a design, and why this file is not in `scratchpad/modules.rs`.
//!
//! # What maps to what
//!
//! | part | the room's |
//! | --- | --- |
//! | cold store | fridge |
//! | worktop | counter, with the board and the drawer on it |
//! | hob | stove |
//! | dishwasher | dishwasher |
//! | table, chairs | table, seats |
//! | bunks | berths, in id order — Bim *i* sleeps in bunk *i* |
//! | broom locker | locker |
//! | hydroponic bay | bay |
//! | toilet, basin | the heads, with no bulkheads of their own |
//! | anything else a body cannot walk through | a solid the nav grid avoids |
//!
//! One of each, the first by id where the design has more. Every fixture is
//! used **from the south** — the Bim stands below it, as it stands below the
//! galley in the classic room — so a part turned to face another way is
//! used from the wrong side, and a part with a wall to its south is one the
//! crew cannot get at. That is the honest limit of this step, and it is
//! written down here rather than fixed, because fixing it is the room's
//! stations learning a direction each.
//!
//! # Units
//!
//! Room units are ship-design world units: a tile is [`TILE`] of both, so a
//! design coordinate *is* a room coordinate and nothing is scaled on the way
//! across. The room's `interior` is the deck's bounding box, and every tile
//! inside that box that is not deck is a solid, so an L-shaped ship does not
//! get a room that thinks the missing corner is floor.

use physics::ResourceId;
use shipdesign::parts::{Layer, TILE};
use shipdesign::{PartKind, PlacedPart, ShipDesign};

use crate::game::Game;
use crate::math::{Rect, Vec2, vec2};
use crate::room::Layout;

/// A tile's rect, in room units.
fn tile_rect(x: i32, y: i32) -> Rect {
    let t = TILE as f32;
    Rect::from_min_size(vec2(x as f32 * t, y as f32 * t), vec2(t, t))
}

/// The middle of a tile.
pub fn tile_middle(x: i32, y: i32) -> Vec2 {
    let t = TILE as f32;
    vec2((x as f32 + 0.5) * t, (y as f32 + 0.5) * t)
}

/// A part's footprint as one rect.
fn part_rect(part: &PlacedPart) -> Rect {
    let tiles = part.tiles();
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
    for &(x, y) in &tiles {
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
    }
    if tiles.is_empty() {
        return tile_rect(part.origin.0 as i32, part.origin.1 as i32);
    }
    Rect::from_corners(
        tile_rect(x0 as i32, y0 as i32).min,
        tile_rect(x1 as i32, y1 as i32).max,
    )
}

/// Every part of a kind, in id order.
fn of_kind(design: &ShipDesign, kind: PartKind) -> Vec<&PlacedPart> {
    let mut parts: Vec<&PlacedPart> = design.parts.iter().filter(|p| p.kind == kind).collect();
    parts.sort_by_key(|p| p.id);
    parts
}

/// The room's layout for a design.
///
/// A design the designer accepted has every fixture the room walks to —
/// `REQUIRED` in `shipdesign::validate` is exactly that list — and one it
/// did not accept is not this module's problem: a fixture that is missing
/// is put on the worktop, and the worktop, if *that* is missing, on the
/// first deck tile, so the room stands up rather than panicking in a cdylib.
pub fn layout_of(design: &ShipDesign) -> Layout {
    let t = TILE as f32;
    let bounds = Rect::from_min_size(
        Vec2::ZERO,
        vec2(design.build_area as f32 * t, design.build_area as f32 * t),
    );

    // The deck's bounding box, and every tile in it that is not deck.
    let floors = of_kind(design, PartKind::Floor);
    let interior = floors
        .iter()
        .map(|p| part_rect(p))
        .reduce(|a, b| {
            Rect::from_corners(
                vec2(a.min.x.min(b.min.x), a.min.y.min(b.min.y)),
                vec2(a.max.x.max(b.max.x), a.max.y.max(b.max.y)),
            )
        })
        .unwrap_or_else(|| tile_rect(0, 0));
    let grid = design.grid();
    let mut others: Vec<Rect> = Vec::new();
    let (x0, y0) = ((interior.min.x / t) as i32, (interior.min.y / t) as i32);
    let (x1, y1) = ((interior.max.x / t) as i32, (interior.max.y / t) as i32);
    for y in y0..y1 {
        for x in x0..x1 {
            if grid.get(Layer::Floor, (x, y)) == 0 {
                others.push(tile_rect(x, y));
            }
        }
    }

    let first = |kind: PartKind| of_kind(design, kind).first().map(|p| part_rect(p));
    let fallback = first(PartKind::Worktop).unwrap_or_else(|| tile_rect(x0, y0));
    let one = |kind: PartKind| first(kind).unwrap_or(fallback);

    // Everything else a body cannot walk through: parts the room has no
    // fixture for — an engine, a helm, a tank, a shelf, a wall — and only
    // those that block. A door, a chair and an airlock are walked onto.
    const MAPPED: [PartKind; 11] = [
        PartKind::ColdStore,
        PartKind::Worktop,
        PartKind::Hob,
        PartKind::Dishwasher,
        PartKind::Table,
        PartKind::Chair,
        PartKind::Bunk,
        PartKind::BroomLocker,
        PartKind::HydroBay,
        PartKind::Toilet,
        PartKind::Basin,
    ];
    for part in &design.parts {
        let def = part.kind.def();
        if def.layer != Layer::Object || !def.blocks_movement {
            continue;
        }
        // The first of each mapped kind is the fixture; any second one is
        // furniture in the way.
        let is_fixture = MAPPED.contains(&part.kind)
            && of_kind(design, part.kind).first().map(|p| p.id) == Some(part.id);
        if is_fixture {
            continue;
        }
        others.push(part_rect(part));
    }

    Layout {
        bounds,
        interior,
        counter: one(PartKind::Worktop),
        fridge: one(PartKind::ColdStore),
        stove: one(PartKind::Hob),
        dishwasher: one(PartKind::Dishwasher),
        table: one(PartKind::Table),
        chairs: of_kind(design, PartKind::Chair)
            .iter()
            .map(|p| part_rect(p).center())
            .collect(),
        beds: of_kind(design, PartKind::Bunk)
            .iter()
            .map(|p| part_rect(p))
            .collect(),
        locker: one(PartKind::BroomLocker),
        bay: one(PartKind::HydroBay),
        toilet: one(PartKind::Toilet),
        sink: one(PartKind::Basin),
        others,
        veg: design.carrying(ResourceId::Vegetable),
        tofu: design.carrying(ResourceId::Tofu),
    }
}

/// The parts the room draws for itself — its fixtures — so a painter that
/// draws the rest of the ship as tiles can leave these to the room. The
/// same rule as `layout_of`: the first of each mapped kind, by id.
pub fn drawn_by_room(design: &ShipDesign) -> Vec<u32> {
    [
        PartKind::ColdStore,
        PartKind::Worktop,
        PartKind::Hob,
        PartKind::Dishwasher,
        PartKind::Table,
        PartKind::Chair,
        PartKind::Bunk,
        PartKind::BroomLocker,
        PartKind::HydroBay,
        PartKind::Toilet,
        PartKind::Basin,
    ]
    .into_iter()
    .flat_map(|kind| {
        let parts = of_kind(design, kind);
        // Every chair and every bunk is drawn, up to the room's two of each;
        // of everything else, the first.
        let drawn = match kind {
            PartKind::Chair | PartKind::Bunk => crate::room::BERTHS,
            _ => 1,
        };
        parts
            .into_iter()
            .take(drawn)
            .map(|p| p.id)
            .collect::<Vec<_>>()
    })
    .collect()
}

/// Where each of the crew starts: on their own bunk's use spot, Bim *i* at
/// bunk *i* in id order. A Bim past the last bunk starts on the first deck
/// tile, which cannot happen to a design the designer accepted.
pub fn starts(design: &ShipDesign, crew: usize) -> Vec<Vec2> {
    let bunks = of_kind(design, PartKind::Bunk);
    let fallback = of_kind(design, PartKind::Floor)
        .first()
        .map(|p| tile_middle(p.origin.0 as i32, p.origin.1 as i32))
        .unwrap_or(Vec2::ZERO);
    (0..crew)
        .map(|who| match bunks.get(who) {
            Some(bunk) => bunk
                .use_spots()
                .first()
                .map(|&(x, y)| tile_middle(x, y))
                .unwrap_or_else(|| part_rect(bunk).center()),
            None => fallback,
        })
        .collect()
}

/// The room's game, aboard this ship, with `crew` Bims at their bunks.
pub fn game_aboard(design: &ShipDesign, crew: usize, seed: u64) -> Game {
    let layout = layout_of(design);
    let (w, h) = (layout.bounds.width(), layout.bounds.height());
    Game::with_layout(layout, seed, &starts(design, crew), w, h)
}
