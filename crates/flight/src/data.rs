//! The numbers a trip is flown by, kept apart from the arithmetic.
//!
//! Placeholders, like every other table in this workspace — but two of them
//! are placeholders **against a scenario** rather than against nothing, and
//! that is the only thing that makes them checkable:
//!
//! - [`FUEL_PER_ENGINE_MINUTE`] is set so that the flyable fixture
//!   (`shipdesign::fixture::flyer`) can cross the world generator's longest
//!   reference hop on one full tank and still have something left. A ship that
//!   cannot reach the far side of its own system is a ship with nowhere to go.
//! - `PartDef::torque_thrust` — which lives next door in `shipdesign`, because
//!   it is a fact about a part — is set so that four thrusters on the same
//!   fixture turn it through half a circle inside two game hours. A flip that
//!   takes a day would make braking by turning round a worse deal than a
//!   backward engine in every case, and the choice between them would stop
//!   being a choice.
//!
//! Both are pinned by tests rather than by comments: see
//! `one_full_tank_crosses_the_longest_reference_hop` and
//! `four_thrusters_flip_the_reference_inside_two_hours`.

/// How close to a station a trip finishes.
///
/// The arrival point is this far **short** of the target along the approach
/// line, so a ship that arrives has not flown into the thing it was aiming at.
pub const ARRIVAL_RADIUS_STATION: f64 = 3_000.0;

/// The same for a body, which is a great deal bigger than a station even
/// though the generator stores it as a point. Flying to a gas giant means
/// flying to somewhere near it.
pub const ARRIVAL_RADIUS_BODY: f64 = 15_000.0;

/// How near the bearing counts as pointing at it.
///
/// A trip whose target is already within this of the ship's nose skips the
/// align phase entirely rather than turning through a thousandth of a radian,
/// which would be a rotation phase of no length that the plan walker would
/// have to special-case anyway.
pub const ALIGN_TOLERANCE: f64 = 0.01;

/// Units of fuel one burning engine gets through in a game minute.
///
/// See the module note: this is chosen against the longest reference hop, not
/// out of the air. Engines that are not burning — during an align, during a
/// flip, while holding, while docked — use nothing at all, and thrusters use
/// nothing ever.
pub const FUEL_PER_ENGINE_MINUTE: f64 = 0.0015;

/// The floor under a ship's moment of inertia.
///
/// Strictly greater than zero because [`crate::Dynamics::alpha`] divides by
/// it. A ship small enough for this to matter is one part welded to nothing,
/// which is not a ship — but a division by zero is an infinite angular
/// acceleration and a heading of NaN, and a NaN heading is a ship that
/// disappears rather than an error anybody can read.
pub const INERTIA_FLOOR: f64 = 1.0;

/// Below this, a distance is not a trip and a speed is not motion.
///
/// One world unit is about a fiftieth of a tile, so this is well under the
/// width of a bulkhead — it is here to keep a plan of no length out of the
/// segment walker, not to be a tolerance anybody flies to.
pub const STILL: f64 = 1e-6;
