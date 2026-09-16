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

use crate::budget::Budget;
use crate::design::{Edit, ShipDesign, apply};
use crate::parts::{PartKind, Rotation};

/// Tiles a side. Small enough to read, big enough to hold a working ship.
pub const AREA: u32 = 20;

/// The crew sizes the reference is built for, in the order
/// [`REFERENCE_HASH`] and [`REFERENCE_PARTS`] are indexed.
pub const CREWS: [u32; 2] = [1, 4];

/// What [`reference`] hashes to, for each of [`CREWS`].
///
/// Pinned rather than computed: these are the numbers that catch a target
/// hashing differently, and a test that compares two computed values would
/// pass happily while both were wrong. Update them only when the reference
/// design itself is meant to change.
pub const REFERENCE_HASH: [u64; 2] = [0x2ebb_ce30_19f2_8045, 0xbb2c_26f2_d176_c03e];

/// How many parts [`reference`] ends up with, for each of [`CREWS`].
///
/// [`reference`] skips an edit that does not take rather than panicking — a
/// panic in a cdylib is an abort and tells nobody anything. This is what
/// notices the skip instead.
pub const REFERENCE_PARTS: [u32; 2] = [336, 342];

/// The reference ship: a floored, walled compartment with a galley along the
/// top, a table and chairs, bunks down the right, an engine, a bay and a
/// locker. Valid for `crew`, with no errors and no missing fixture.
///
/// Built through [`apply`] like anything else, so it is a ship the rules
/// admit rather than a hand-assembled `Vec` that might not be.
pub fn reference(crew: u32) -> ShipDesign {
    let budget = Budget::from_factor(crate::budget::PER_MILLE);
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

    // Deck first: everything but a wall wants plating under it.
    for y in 2..18 {
        for x in 2..18 {
            put(&mut design, PartKind::Floor, (x, y), Rotation::R0);
        }
    }

    // Hull round the outside of it. Walls need no deck, so the ring sits one
    // tile out — and stays 4-connected to the plating, which is what the
    // connectivity check wants.
    for i in 1..19 {
        put(&mut design, PartKind::Wall, (i, 1), Rotation::R0);
        put(&mut design, PartKind::Wall, (i, 18), Rotation::R0);
        put(&mut design, PartKind::Wall, (1, i), Rotation::R0);
        put(&mut design, PartKind::Wall, (18, i), Rotation::R0);
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

    design
}
