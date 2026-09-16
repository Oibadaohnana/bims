//! One world, opened the same way every time.
//!
//! Here rather than in the tests for the reason `shipdesign::fixture` is:
//! **two targets have to agree about it.** The browser runs
//! [`World::step`](crate::World::step) in wasm and the native server that will
//! one day be authoritative runs the same loop for x86, and the only way to
//! find out whether they agree is to run a fixed scenario on both and compare
//! each against the same written-down number.
//!
//! The native end is `tests.rs`; the wasm end is `ship_self_check` in
//! `crates/ship`, which the node harness reads. Both compare against
//! [`REFERENCE_CHECKSUM`]. If one target's arithmetic ever drifts from the
//! other's, exactly one of those two fails.

use shipdesign::fixture::flyer;
use worldgen::GalaxyType;

use crate::data;
use crate::world::{Command, World};
use crate::{Speed, Target};

/// What the reference crew have left over from the design phase.
pub const REFERENCE_MONEY: economy::Money = 40_000;

/// How many steps [`reference_run`] takes. Ten game minutes at 1x, which is
/// long enough to get a trip planned, confirmed and turning and short enough
/// that the wasm half of the check does not hold up a page load.
pub const REFERENCE_STEPS: u32 = 600;

/// What [`reference_run`] comes out at.
///
/// Pinned rather than computed, for the same reason `REFERENCE_HASH` is: a
/// test comparing two computed values would pass happily while both were
/// wrong. Update it only when the scenario below is meant to change.
pub const REFERENCE_CHECKSUM: u64 = 0x_34bc_d68c_bb1d_86d3;

/// A world with the flyable fixture docked at the spawn station.
pub fn reference_world() -> World {
    World::start(
        flyer(2),
        REFERENCE_MONEY,
        2,
        data::DEFAULT_SEED,
        GalaxyType::SpiralTwoArm,
    )
    .expect("the default seed should have somewhere to spawn")
}

/// Somewhere in the spawn system that is not where the ship is standing.
///
/// The lowest-numbered node that is not the dock, so the scenario does not
/// depend on which of them the generator happened to put nearest.
pub fn reference_target(world: &World) -> Target {
    let docked = match &world.ship.state {
        crate::ShipState::Docked { station } => Some(*station),
        _ => None,
    };
    for node in world.system.nodes() {
        match node {
            worldgen::Node::Station(id) if Some(id) == docked => continue,
            worldgen::Node::Body(id) => return Target::Body(id),
            worldgen::Node::Station(id) => return Target::Station(id),
        }
    }
    Target::Point(worldgen::math::dvec2(0.0, 0.0))
}

/// The scenario: open a world, confirm a trip, and run for
/// [`REFERENCE_STEPS`].
///
/// Everything a checksum is meant to catch is in it — a plan made, fuel
/// reserved, a heading turning through a trigonometric function, the local
/// frame changing as the ship leaves the dock.
pub fn reference_run() -> u64 {
    let mut world = reference_world();
    let target = reference_target(&world);

    // Both players ask for the top speed, so the effective speed is a
    // decision that was actually taken rather than the default.
    world.step(&[
        Command::SetSpeed {
            slot: 0,
            speed: Speed::Top,
        },
        Command::SetSpeed {
            slot: 1,
            speed: Speed::Top,
        },
        Command::Confirm { slot: 1, target },
    ]);
    for _ in 1..REFERENCE_STEPS {
        world.step(&[]);
    }
    world.checksum()
}
