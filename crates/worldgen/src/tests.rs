//! The native half of the cross-target check.
//!
//! [`crate::fixture`] pins what the reference galaxy's checksum comes out at.
//! This compares the native build against it; `lobby_galaxy_checksum_hi`/`_lo`
//! in `crates/lobby` is the wasm build's answer to the same question, read by
//! `scratchpad/builder-check.mjs` against the same constants. A target whose
//! arithmetic drifted fails exactly one of the two.

use crate::fixture::{REFERENCE_CHECKSUMS, REFERENCE_SEED, reference};
use crate::galaxy::GalaxyType;

#[test]
fn the_reference_galaxy_comes_out_at_the_numbers_it_is_pinned_to() {
    for &t in &GalaxyType::ALL {
        let got = reference(t).checksum();
        assert_eq!(
            got, REFERENCE_CHECKSUMS[t as usize],
            "{t:?} under seed {REFERENCE_SEED:#x} came out at {got:#018x}. If the \
             generator was meant to change, this is a GENERATOR_VERSION bump and \
             the pinned checksums in fixture.rs move with it."
        );
    }
}

/// A checksum that did not notice anything would pass the test above with
/// the wrong numbers written down.
#[test]
fn the_checksum_notices_a_different_galaxy() {
    let a = reference(GalaxyType::Round).checksum();
    let b = crate::Galaxy::new(REFERENCE_SEED + 1, GalaxyType::Round).checksum();
    let c = reference(GalaxyType::Spiral).checksum();
    assert_ne!(a, b, "a different seed should show");
    assert_ne!(a, c, "a different type should show");
}

/// The four pinned numbers are four *different* numbers, or a type is not
/// reaching the generator.
#[test]
fn every_type_is_pinned_to_its_own_number() {
    for (i, a) in REFERENCE_CHECKSUMS.iter().enumerate() {
        for b in &REFERENCE_CHECKSUMS[i + 1..] {
            assert_ne!(a, b);
        }
    }
}
