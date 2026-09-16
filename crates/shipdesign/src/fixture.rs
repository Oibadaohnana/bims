//! The reference design: one ship, built the same way every time.
//!
//! It is in the library rather than in the tests because two targets have to
//! agree about it. [`design_hash`](crate::design_hash) is what an Accept is
//! recorded against, so it has to come out identical on the native server that
//! will one day be authoritative and in the wasm the players are running — and
//! the only way to find out is to compute it on both and compare each against
//! the same written-down number.
//!
//! The native end is `tests.rs`; the wasm end is `ship_self_check` in
//! `crates/ship`, which the node harness reads. Both compare against
//! [`REFERENCE_HASH`] below. If one target's arithmetic ever drifts from the
//! other's, exactly one of those two fails.

use economy::Money;
use physics::ResourceId;

use crate::budget::Budget;
use crate::design::{Edit, ShipDesign, apply};
use crate::parts::{Layer, PartKind, Rotation};

/// Tiles a side. Small enough to read, big enough to hold a working ship.
pub const AREA: u32 = 20;

/// The crew sizes the reference is built for, in the order
/// [`REFERENCE_HASH`] and [`REFERENCE_PARTS`] are indexed.
pub const CREWS: [u32; 2] = [1, 4];

/// What the reference is built with. Deliberately far more than it needs:
/// this fixture is about the hash and the rules, and a reference ship that
/// ran out of money halfway would be a reference ship whose parts count moved
/// every time a price was tuned.
pub const REFERENCE_POOL: Money = 10_000_000;

/// What [`reference`] hashes to, for each of [`CREWS`].
///
/// Pinned rather than computed: these are the numbers that catch a target
/// hashing differently, and a test that compares two computed values would
/// pass happily while both were wrong. Update them only when the reference
/// design itself is meant to change.
pub const REFERENCE_HASH: [u64; 2] = [0x5a7f_fb5a_98ee_38e9, 0xdb7f_99a6_e7bc_3312];

/// What [`reference`] is carrying, whatever the crew size: a few days of
/// vegetables and tofu, bought through [`apply`] like everything else.
///
/// Pinned for the same reason the hash is — the cargo is *in* the hash, so a
/// target that added up a manifest differently would show up here rather
/// than as an unexplained number.
pub const REFERENCE_CARGO: [(ResourceId, u32); 2] =
    [(ResourceId::Vegetable, 40), (ResourceId::Tofu, 20)];

/// How many parts [`reference`] ends up with, for each of [`CREWS`].
///
/// [`reference`] skips an edit that does not take rather than panicking — a
/// panic in a cdylib is an abort and tells nobody anything. This is what
/// notices the skip instead.
pub const REFERENCE_PARTS: [u32; 2] = [661, 667];

/// The reference ship: a framed, floored, hull-plated compartment with a
/// galley along the top, a table and chairs, bunks down the right, a helm, an
/// engine, a bay and a locker, and a few days' food in the cold store. Valid
/// for `crew`, with no errors, no missing fixture and **no radiation getting
/// in** — the hull is outside wall the whole way round.
///
/// Built through [`apply`] like anything else, so it is a ship the rules
/// admit rather than a hand-assembled `Vec` that might not be. That goes for
/// the cargo as well: the food is bought, not written into the array.
pub fn reference(crew: u32) -> ShipDesign {
    let budget = Budget::new(REFERENCE_POOL);
    let mut design = ShipDesign::new(AREA);

    let put = |design: &mut ShipDesign, kind: PartKind, origin, rotation| {
        if let Ok(next) = apply(
            design,
            &budget,
            Edit::Place {
                kind,
                origin,
                rotation,
            },
        ) {
            *design = next;
        }
    };

    // The frame first, and it has to reach everywhere anything else goes:
    // under the deck, and under the hull ring one tile outside it.
    for y in 1..19 {
        for x in 1..19 {
            put(&mut design, PartKind::Structure, (x, y), Rotation::R0);
        }
    }

    // Then the deck, inside the ring.
    for y in 2..18 {
        for x in 2..18 {
            put(&mut design, PartKind::Floor, (x, y), Rotation::R0);
        }
    }

    // Hull round the outside of it. Outside wall rather than plain wall:
    // plain wall does not shield, and a ship skinned in it is a ship whose
    // crew are being cooked. The corners are placed twice and the second
    // attempt is refused, which is the loop being simple rather than a bug.
    for i in 1..19 {
        put(&mut design, PartKind::OutsideWall, (i, 1), Rotation::R0);
        put(&mut design, PartKind::OutsideWall, (i, 18), Rotation::R0);
        put(&mut design, PartKind::OutsideWall, (1, i), Rotation::R0);
        put(&mut design, PartKind::OutsideWall, (18, i), Rotation::R0);
    }

    // The galley and the heads along row 3, everything facing down the room,
    // so every use spot is the open row below them.
    put(&mut design, PartKind::ColdStore, (3, 3), Rotation::R0);
    put(&mut design, PartKind::Worktop, (5, 3), Rotation::R0);
    put(&mut design, PartKind::Hob, (8, 3), Rotation::R0);
    put(&mut design, PartKind::Dishwasher, (10, 3), Rotation::R0);
    put(&mut design, PartKind::Toilet, (12, 3), Rotation::R0);
    put(&mut design, PartKind::Basin, (14, 3), Rotation::R0);
    put(&mut design, PartKind::BroomLocker, (16, 3), Rotation::R0);

    put(&mut design, PartKind::Table, (4, 6), Rotation::R0);
    put(&mut design, PartKind::Helm, (7, 6), Rotation::R0);
    put(&mut design, PartKind::HydroBay, (10, 10), Rotation::R0);
    put(&mut design, PartKind::Engine, (7, 13), Rotation::R0);

    // A bed and a seat each. The chairs go on the table's own use spots where
    // there are two of them and beside it after that — a chair is walked onto
    // rather than round, so sitting one in a doorway of a use spot is fine.
    for i in 0..crew {
        put(&mut design, PartKind::Bunk, (14, 8 + 2 * i), Rotation::R0);
        put(
            &mut design,
            PartKind::Chair,
            (4 + i % 2, 7 + 2 * (i / 2)),
            Rotation::R0,
        );
    }

    // And something to eat. Bought through `apply` like everything else, so
    // the cold store's capacity is what bounds it — and so the cargo half of
    // `design_hash` is pinned by the same cross-target check the parts are.
    for (resource, units) in REFERENCE_CARGO {
        if let Ok(next) = apply(&design, &budget, Edit::Buy { resource, units }) {
            design = next;
        }
    }

    design
}

/// Where the flyer's four thrusters and its sensor array go, as hull tiles
/// they **replace**.
///
/// Mid-edge rather than at the corners, and all four of them, because what a
/// thruster is worth is its distance from the centre of mass and a corner is
/// no further out than the middle of a side on a square hull. Replacing the
/// plating rather than standing beside it is the only option there is: a
/// thruster is hull, it stands on the frame, and a tile holds one object.
const THRUSTER_TILES: [(u32, u32); 4] = [(9, 1), (9, 18), (1, 9), (18, 9)];
const SENSOR_TILE: (u32, u32) = (5, 1);

/// How much fuel [`flyer`] leaves the dock with: a full tank.
pub const FLYER_FUEL: u32 = 200;

/// The reference ship again, with everything a trip actually needs.
///
/// [`reference`] is a ship you can **live** on and it is deliberately not one
/// you can fly: it has an engine and nothing else, so it raises every one of
/// the flight warnings and is exactly the fixture those warnings are tested
/// against. This is the other one — four thrusters to turn with, an airlock
/// to dock through, a sensor array to see with, a tank and two hundred units
/// of fuel to burn — and it is what `flight` and `world` measure their
/// scenarios against.
///
/// Its hash is deliberately **not** pinned. [`REFERENCE_HASH`] is about two
/// targets agreeing; this one is about a trip being flyable, and pinning a
/// second number would only mean a second thing to update whenever a placement
/// here moved.
pub fn flyer(crew: u32) -> ShipDesign {
    let budget = Budget::new(REFERENCE_POOL);
    let mut design = reference(crew);

    // The hull comes off first. Both replacements shield, so the skin is
    // still closed when they go back on — which `exposure` is asked about in
    // `a_flyer_is_still_sealed`.
    let swap = |design: &mut ShipDesign, tile: (u32, u32), kind: PartKind| {
        let standing = design
            .grid()
            .get(Layer::Object, (tile.0 as i32, tile.1 as i32));
        if standing != 0
            && let Ok(next) = apply(design, &budget, Edit::Remove { part_id: standing })
        {
            *design = next;
        }
        if let Ok(next) = apply(
            design,
            &budget,
            Edit::Place {
                kind,
                origin: tile,
                rotation: Rotation::R0,
            },
        ) {
            *design = next;
        }
    };

    for tile in THRUSTER_TILES {
        swap(&mut design, tile, PartKind::Thruster);
    }
    swap(&mut design, SENSOR_TILE, PartKind::SensorArray);

    for (kind, origin) in [(PartKind::Airlock, (16, 8)), (PartKind::FuelTank, (2, 12))] {
        if let Ok(next) = apply(
            &design,
            &budget,
            Edit::Place {
                kind,
                origin,
                rotation: Rotation::R0,
            },
        ) {
            design = next;
        }
    }

    if let Ok(next) = apply(
        &design,
        &budget,
        Edit::Buy {
            resource: ResourceId::Fuel,
            units: FLYER_FUEL,
        },
    ) {
        design = next;
    }

    design
}

// --- the playtest ship ------------------------------------------------------

/// What [`playtest_ship`] hashes to. Pinned for the reason [`REFERENCE_HASH`]
/// is: `ship_self_check` computes it in wasm and `tests.rs` natively, and a
/// target that hashed the simulation's ship differently would start a
/// different simulation. Update it only when the ship below is meant to
/// change.
pub const PLAYTEST_HASH: u64 = 0xd235_be32_b183_42e0;

/// How many parts [`playtest_ship`] ends up with. What notices a placement
/// that was quietly refused — the builder skips rather than panics, for the
/// reason [`REFERENCE_PARTS`] gives.
pub const PLAYTEST_PARTS: u32 = 665;

/// Where the playtest ship's thrusters go: hull tiles near the four corners,
/// which they **replace**, as the flyer's do.
const PLAYTEST_THRUSTERS: [(u32, u32); 4] = [(2, 1), (17, 1), (2, 18), (17, 18)];

/// The hull tiles the airlock stands in: one column of the right-hand skin,
/// two tall. They get deck first, because an airlock stands on deck, and
/// the airlock shields, so the hull is as closed as it was.
const PLAYTEST_AIRLOCK: [(u32, u32); 2] = [(18, 9), (18, 10)];

/// What the playtest ship carries besides a full tank: enough metal and
/// components to build with, and a few days of food. Bought through
/// [`apply`], so the shelf and the cold store are what bound it.
pub const PLAYTEST_CARGO: [(ResourceId, u32); 5] = [
    (ResourceId::Fuel, 200),
    (ResourceId::Metal, 60),
    (ResourceId::Components, 40),
    (ResourceId::Vegetable, 40),
    (ResourceId::Tofu, 20),
];

/// The ship `nix run .#simulation` opens with: one of everything a crew of
/// one needs to live and to fly, on a twenty-tile grid, with a full tank and
/// a stocked hold. Valid for one with **no errors and no warnings**.
///
/// It is not [`flyer`] for one crew, though it is close to it, because a
/// playtest ship is meant to be changed as the game grows — a shower, a
/// shelf, materials to build with — without moving the reference that two
/// targets are compared on. Built through [`apply`] like everything else.
pub fn playtest_ship() -> ShipDesign {
    let budget = Budget::new(REFERENCE_POOL);
    let mut design = ShipDesign::new(AREA);

    let put = |design: &mut ShipDesign, kind: PartKind, origin, rotation| {
        if let Ok(next) = apply(
            design,
            &budget,
            Edit::Place {
                kind,
                origin,
                rotation,
            },
        ) {
            *design = next;
        }
    };
    let take = |design: &mut ShipDesign, tile: (u32, u32)| {
        let standing = design
            .grid()
            .get(Layer::Object, (tile.0 as i32, tile.1 as i32));
        if standing != 0
            && let Ok(next) = apply(design, &budget, Edit::Remove { part_id: standing })
        {
            *design = next;
        }
    };

    // Frame under everything, deck inside, hull round the outside — the
    // reference's shape exactly.
    for y in 1..19 {
        for x in 1..19 {
            put(&mut design, PartKind::Structure, (x, y), Rotation::R0);
        }
    }
    for y in 2..18 {
        for x in 2..18 {
            put(&mut design, PartKind::Floor, (x, y), Rotation::R0);
        }
    }
    for i in 1..19 {
        put(&mut design, PartKind::OutsideWall, (i, 1), Rotation::R0);
        put(&mut design, PartKind::OutsideWall, (i, 18), Rotation::R0);
        put(&mut design, PartKind::OutsideWall, (1, i), Rotation::R0);
        put(&mut design, PartKind::OutsideWall, (18, i), Rotation::R0);
    }

    // The hull's working parts, in place of plating: thrusters at the
    // corners, an array forward, and an airlock in the starboard skin.
    for tile in PLAYTEST_THRUSTERS {
        take(&mut design, tile);
        put(&mut design, PartKind::Thruster, tile, Rotation::R0);
    }
    take(&mut design, (9, 1));
    put(&mut design, PartKind::SensorArray, (9, 1), Rotation::R0);
    for tile in PLAYTEST_AIRLOCK {
        take(&mut design, tile);
        put(&mut design, PartKind::Floor, tile, Rotation::R0);
    }
    put(
        &mut design,
        PartKind::Airlock,
        PLAYTEST_AIRLOCK[0],
        Rotation::R0,
    );

    // The galley and the heads along the top, facing down the room.
    put(&mut design, PartKind::ColdStore, (3, 3), Rotation::R0);
    put(&mut design, PartKind::Worktop, (5, 3), Rotation::R0);
    put(&mut design, PartKind::Hob, (8, 3), Rotation::R0);
    put(&mut design, PartKind::Dishwasher, (10, 3), Rotation::R0);
    put(&mut design, PartKind::Toilet, (12, 3), Rotation::R0);
    put(&mut design, PartKind::Basin, (14, 3), Rotation::R0);
    put(&mut design, PartKind::BroomLocker, (16, 3), Rotation::R0);

    // Living space in the middle, stores down the sides, the engine aft.
    put(&mut design, PartKind::Table, (4, 6), Rotation::R0);
    put(&mut design, PartKind::Chair, (4, 7), Rotation::R0);
    put(&mut design, PartKind::Helm, (7, 6), Rotation::R0);
    put(&mut design, PartKind::Bunk, (14, 8), Rotation::R0);
    put(&mut design, PartKind::Shower, (16, 6), Rotation::R0);
    put(&mut design, PartKind::Shelf, (16, 12), Rotation::R0);
    put(&mut design, PartKind::HydroBay, (10, 10), Rotation::R0);
    put(&mut design, PartKind::FuelTank, (2, 12), Rotation::R0);
    put(&mut design, PartKind::Engine, (7, 14), Rotation::R0);

    for (resource, units) in PLAYTEST_CARGO {
        if let Ok(next) = apply(&design, &budget, Edit::Buy { resource, units }) {
            design = next;
        }
    }

    design
}

/// [`playtest_ship`] laid out in the middle of a bigger build area — what
/// the design phase opens with, so nobody starts from an empty grid.
///
/// The same parts at the same places, shifted by half the difference, and
/// the same cargo, all put down through [`apply`] again so the result is a
/// ship the rules admit on *that* grid rather than a copy with its numbers
/// changed. `None` for an area the ship does not fit, which is an empty
/// grid for the player rather than half a ship.
///
/// Its hash is not pinned: it is [`PLAYTEST_HASH`]'s ship moved, and the
/// part count says whether every part came across.
pub fn playtest_ship_on(area: u32) -> Option<ShipDesign> {
    if area < AREA {
        return None;
    }
    let shift = (area - AREA) / 2;
    let budget = Budget::new(REFERENCE_POOL);
    let source = playtest_ship();
    let mut design = ShipDesign::new(area);
    // In id order, which is placement order: the frame went down before the
    // deck and the deck before what stands on it, and ids only ever climb.
    for part in &source.parts {
        if let Ok(next) = apply(
            &design,
            &budget,
            Edit::Place {
                kind: part.kind,
                origin: (part.origin.0 + shift, part.origin.1 + shift),
                rotation: part.rotation,
            },
        ) {
            design = next;
        }
    }
    for (resource, units) in PLAYTEST_CARGO {
        if let Ok(next) = apply(&design, &budget, Edit::Buy { resource, units }) {
            design = next;
        }
    }
    Some(design)
}
