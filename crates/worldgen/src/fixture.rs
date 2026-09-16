//! One galaxy, generated the same way every time, and what it comes out at.
//!
//! Here rather than in the tests for the reason `shipdesign::fixture` and
//! `world::fixture` are: **two targets have to agree about it.** The lobby
//! generates the galaxy in wasm to draw the star map, and the native server
//! that will one day be authoritative generates it for x86; the only way to
//! find out whether they agree is to run a fixed seed on both and compare each
//! against the same written-down number.
//!
//! The native end is `tests.rs`; the wasm end is `lobby_galaxy_checksum_hi`
//! and `_lo` in `crates/lobby`, which `scratchpad/builder-check.mjs` reads
//! and compares against the constants below — parsed out of this file, so the
//! harness and the test are looking at one copy of the number. If one
//! target's arithmetic ever drifts from the other's, exactly one of those two
//! fails.

use crate::galaxy::{Galaxy, GalaxyType};

/// The seed every pinned number below was generated from. Chosen to have both
/// halves non-zero, so a lobby that dropped the high word — the boundary
/// carries a seed as two `u32`s — would come out at a different galaxy.
pub const REFERENCE_SEED: u64 = 0x_4c4f_4242_0000_0007;

/// What [`Galaxy::checksum`] comes out at for [`REFERENCE_SEED`] under each
/// galaxy type, indexed by `GalaxyType as usize`.
///
/// Pinned rather than computed: a test comparing two computed values would
/// pass happily while both were wrong. Update these only when the generator
/// is meant to change — which is a [`crate::GENERATOR_VERSION`] bump, and
/// the test that reads them says so.
pub const REFERENCE_CHECKSUMS: [u64; 4] = [
    0x_8a32_6d51_25f4_89af,
    0x_9810_d098_4d09_7d82,
    0x_63e7_ef75_dbcc_d268,
    0x_dbcd_0e4d_59d8_ab20,
];

/// The reference galaxy of one type.
pub fn reference(galaxy_type: GalaxyType) -> Galaxy {
    Galaxy::new(REFERENCE_SEED, galaxy_type)
}
