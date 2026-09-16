//! What the designed ship weighs, and what that does to its engines.
//!
//! All four answers come out of `physics`, which is the point of `physics`
//! being its own crate: the world generator already measures its systems with
//! the same arithmetic, and a builder that worked out acceleration its own way
//! would be quoting a trip nobody could fly.
//!
//! Nothing here is shown during the design phase. There is no consumer for it
//! until flight, and a number on screen that nothing depends on is a number
//! that quietly goes wrong. It is computed and tested now so that the day
//! flight arrives, the ship it is handed already weighs something sensible.

use physics::{EngineSpec, Facing, Mass, MassError};

use crate::design::ShipDesign;
use crate::parts::{PartKind, part_mass};

/// Every part welded to the hull, added up. The frame, the plating and
/// everything standing on it — but **not** what is in the hold; that is
/// [`ShipDesign::manifest`] and it is added separately in [`ship_mass`].
///
/// A part weighs its recipe, through [`part_mass`], which is what makes this
/// figure and the manifest two readings of the same materials: build a wall
/// out of the hold and this goes up by exactly what the manifest goes down
/// by. See [`crate::materials`].
///
/// What is left in the pool is not in either. Money is not cargo.
pub fn hull_mass(design: &ShipDesign) -> f64 {
    design.parts.iter().map(|p| part_mass(p.kind)).sum()
}

/// The engines, as `physics` wants them: a thrust and a direction of push.
///
/// The direction comes from the part's rotation through
/// [`crate::parts::Rotation::facing`], which is a placeholder — see the note
/// there. The thrusts are placeholders too; the *shape* is not.
pub fn engines(design: &ShipDesign) -> Vec<EngineSpec> {
    design
        .parts
        .iter()
        .filter(|p| p.kind == PartKind::Engine)
        .map(|p| EngineSpec {
            thrust: p.kind.def().thrust,
            facing: p.rotation.facing(),
        })
        .collect()
}

/// What the ship weighs: the hull, the crew aboard, and **what is in the
/// hold**.
///
/// The cargo is bought during the design phase and it is aboard from the
/// moment it is bought — a player who fills the tanks has a heavier ship and
/// should see the acceleration say so before they accept it, not after.
///
/// An empty design comes back as [`MassError::HullTooLight`] rather than as
/// zero, because `physics` refuses a ship that weighs nothing — one would
/// accelerate infinitely, and rounding it up would hide whatever lost the
/// parts.
pub fn ship_mass(design: &ShipDesign, crew_count: u32) -> Result<Mass, MassError> {
    physics::ship_mass(hull_mass(design), &design.manifest(), crew_count)
}

/// How hard the ship accelerates along one of its own axes, or `None` if it
/// does not weigh enough to be a ship yet.
///
/// No engines on the axis is `0.0`, which is a real answer: it is what makes
/// `physics::travel_days` refuse to quote a trip the ship could not stop at
/// the end of.
pub fn acceleration(design: &ShipDesign, crew_count: u32, axis: Facing) -> Option<f64> {
    let mass = ship_mass(design, crew_count).ok()?;
    Some(physics::axis_acceleration(&engines(design), mass, axis))
}
