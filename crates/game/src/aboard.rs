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
//! | hydroponic bay | bay, six trays along it, worked from its use spots' side |
//! | toilet, basin | the heads, with no bulkheads of their own |
//! | shower | the shower, used from its use spot; a solid the ship draws |
//! | smelter, workbench | a bench each, used from its use spot; solids the ship draws |
//! | suit locker | where a walk outside starts and ends; a solid the ship draws |
//! | shelf | where a load for a construction site is picked up; a solid the ship draws |
//! | airlock (the port) | the gangway inside it and the spot outside it, for a walk |
//! | door | a powered door — see `crate::door` |
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
use shipdesign::parts::{Layer, TILE, door_slides_along_x};
use shipdesign::{PartKind, PlacedPart, ShipDesign};

use crate::game::Game;
use crate::math::{Rect, Vec2, vec2};
use crate::room::{Bench, Layout, More, Still};

/// How far inside the port the gangway is, in tiles — the same distance the
/// world sends the crew back to before casting off — and how far beyond the
/// end of the collar a body on a walk outside is held.
const GANGWAY_TILES: f32 = 2.5;
const OUTSIDE_TILES: f32 = 1.0;

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
    // And what stops a line of sight: the same tiles, and the parts that
    // do, below. A door is not here — see `doors`.
    let mut opaque: Vec<Rect> = Vec::new();
    let mut extras: Vec<(Still, Rect)> = Vec::new();
    let mut more = More::default();
    let (x0, y0) = ((interior.min.x / t) as i32, (interior.min.y / t) as i32);
    let (x1, y1) = ((interior.max.x / t) as i32, (interior.max.y / t) as i32);
    for y in y0..y1 {
        for x in x0..x1 {
            if grid.get(Layer::Floor, (x, y)) == 0 {
                others.push(tile_rect(x, y));
                opaque.push(tile_rect(x, y));
            }
        }
    }

    let first = |kind: PartKind| of_kind(design, kind).first().map(|p| part_rect(p));
    let fallback = first(PartKind::Worktop).unwrap_or_else(|| tile_rect(x0, y0));
    let one = |kind: PartKind| first(kind).unwrap_or(fallback);

    // Which side the bay is worked from: which edge of its frame its first
    // use spot lies beyond. **Beyond the edge, not off the centre** — the
    // first spot is at the *end* of a run of six, and measured from the
    // middle of the run it read as off the end rather than off the side,
    // which laid the trays across the bay in six strips instead of one a
    // tile. North when there is no bay to ask.
    let side_of = |p: &PlacedPart| {
        let spot = *p.use_spots().first()?;
        let at = tile_middle(spot.0, spot.1);
        let frame = part_rect(p);
        Some(if at.x < frame.min.x {
            vec2(-1.0, 0.0)
        } else if at.x > frame.max.x {
            vec2(1.0, 0.0)
        } else if at.y > frame.max.y {
            vec2(0.0, 1.0)
        } else {
            vec2(0.0, -1.0)
        })
    };
    let bay_side = of_kind(design, PartKind::HydroBay)
        .first()
        .and_then(|p| side_of(p))
        .unwrap_or(vec2(0.0, -1.0));

    // The doors, each with the way its leaves slide: along the bulkhead it
    // stands in, which is the long side of the part — a door is two tiles
    // along and one deep, so its rotation says which, and the painter reads
    // the same function.
    let doors: Vec<(Rect, bool)> = of_kind(design, PartKind::Door)
        .iter()
        .map(|p| (part_rect(p), door_slides_along_x(p.rotation)))
        .collect();

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
        if def.layer == Layer::Object && def.blocks_sight() && part.kind != PartKind::Door {
            opaque.push(part_rect(part));
        }
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
        // A second of a kind the room has a picture for is drawn as one,
        // standing still. A chair and a bunk are never here: the room
        // seats and beds every one of them.
        // Every second of a kind the room works is worked too: the rest
        // go to the room as fixtures of their own, and a chain picks the
        // closest free one. A table and a basin are the two the room only
        // draws — every chair is seated already, and a basin is a heads'
        // half, paired with the toilet nearest it below.
        let frame = part_rect(part);
        match part.kind {
            PartKind::Worktop => more.worktops.push(frame),
            PartKind::Hob => more.hobs.push(frame),
            PartKind::ColdStore => more.fridges.push(frame),
            PartKind::Dishwasher => more.dishwashers.push(frame),
            PartKind::BroomLocker => more.lockers.push(frame),
            PartKind::Table => extras.push((Still::Table, frame)),
            PartKind::Basin => extras.push((Still::Basin, frame)),
            PartKind::HydroBay => {
                let side = side_of(part).unwrap_or(vec2(0.0, -1.0));
                more.bays.push((frame, side));
            }
            PartKind::Toilet => {
                let nearest = of_kind(design, PartKind::Basin)
                    .iter()
                    .map(|b| part_rect(b))
                    .min_by(|a, b| {
                        let da = (a.center() - frame.center()).len();
                        let db = (b.center() - frame.center()).len();
                        da.partial_cmp(&db).unwrap_or(core::cmp::Ordering::Equal)
                    })
                    .unwrap_or(frame);
                more.heads.push((frame, nearest));
            }
            _ => {}
        }
    }

    // The shower: its footprint, and the spot in front of it off its use
    // spot, turned with the part. It stays a solid in `others` — the room
    // has no picture for it and the ship painter draws it.
    let shower_of = |p: &PlacedPart| {
        let at = p
            .use_spots()
            .first()
            .map(|&(x, y)| tile_middle(x, y))
            .unwrap_or_else(|| part_rect(p).center() + vec2(0.0, t));
        (part_rect(p), at)
    };
    let showers = of_kind(design, PartKind::Shower);
    let shower = showers.first().map(|p| shower_of(p));
    more.showers = showers.iter().skip(1).map(|p| shower_of(p)).collect();

    let helm = first(PartKind::Helm);

    // The suit locker, worked from its use spot like the shower, and the
    // port: the deck a couple of tiles inside it, which is where a walk
    // outside sets out from and comes back to, and a spot a tile beyond its
    // face, which is where the body is held while it is out.
    let suit_locker = of_kind(design, PartKind::SuitLocker).first().map(|p| {
        let at = p
            .use_spots()
            .first()
            .map(|&(x, y)| tile_middle(x, y))
            .unwrap_or_else(|| part_rect(p).center() + vec2(0.0, t));
        (part_rect(p), at)
    });
    // The hull, every tile of it, for the grid a body walks outside on: a
    // suited Bim goes round the ship, not through it. Structure is the
    // layer everything stands on, so a tile with structure is a tile of
    // the ship.
    let area = design.build_area as i32;
    let mut hull: Vec<Rect> = Vec::new();
    for y in 0..area {
        for x in 0..area {
            if grid.get(Layer::Structure, (x, y)) != 0 {
                hull.push(tile_rect(x, y));
            }
        }
    }
    let port = shipdesign::dock::port(design);
    let gangway = port.as_ref().map(|p| {
        vec2(
            p.centre.0 as f32 - p.outward.0 as f32 * GANGWAY_TILES * t,
            p.centre.1 as f32 - p.outward.1 as f32 * GANGWAY_TILES * t,
        )
    });
    let outside = port.as_ref().map(|p| {
        let face = p.face();
        vec2(
            face.0 as f32 + p.outward.0 as f32 * OUTSIDE_TILES * t,
            face.1 as f32 + p.outward.1 as f32 * OUTSIDE_TILES * t,
        )
    });

    // The shelves, every one, worked from its use spot like a bench: where
    // a load of materials for a construction site is picked up. Solids in
    // `others` still; the ship draws them.
    let shelves: Vec<(Rect, Vec2)> = of_kind(design, PartKind::Shelf)
        .iter()
        .map(|p| {
            let at = p
                .use_spots()
                .first()
                .map(|&(x, y)| tile_middle(x, y))
                .unwrap_or_else(|| part_rect(p).center() + vec2(0.0, t));
            (part_rect(p), at)
        })
        .collect();

    // The workstations: every part with a recipe made at it, in id order,
    // each with the spot in front of it off its use spot, turned with the
    // part. They stay solids in `others`; the ship draws them.
    let benches: Vec<Bench> = design
        .parts
        .iter()
        .filter(|p| shipdesign::recipes::at(p.kind).next().is_some())
        .map(|p| Bench {
            kind: p.kind.code(),
            frame: part_rect(p),
            at: p
                .use_spots()
                .first()
                .map(|&(x, y)| tile_middle(x, y))
                .unwrap_or_else(|| part_rect(p).center() + vec2(0.0, t)),
        })
        .collect();

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
        bay_side,
        toilet: one(PartKind::Toilet),
        sink: one(PartKind::Basin),
        shower,
        helm,
        benches,
        suit_locker,
        gangway,
        outside,
        hull,
        shelves,
        others,
        opaque,
        more,
        extras,
        doors,
        airlocks: of_kind(design, PartKind::Airlock)
            .iter()
            .map(|p| part_rect(p))
            .collect(),
        veg: design.carrying(ResourceId::Vegetable),
        tofu: design.carrying(ResourceId::Tofu),
    }
}

/// The parts the room draws for itself — its fixtures — so a painter that
/// draws the rest of the ship as tiles can leave these to the room. Every
/// part of every kind the room has a picture for: the first of each is
/// the fixture the room works, and the rest it draws standing still
/// (`Layout::extras`), so none of them is a coloured block.
pub fn drawn_by_room(design: &ShipDesign) -> Vec<u32> {
    [
        PartKind::Door,
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
        of_kind(design, kind)
            .into_iter()
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
