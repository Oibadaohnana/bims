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
use crate::parts::{PartKind, Rotation};

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
