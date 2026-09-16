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
use crate::parts::PartKind;

/// Every part welded to the hull, added up.
///
/// The remaining stockpile is **not** in here. It stays at the station: a
/// design is a promise about what to build, not a manifest of what is aboard,
/// and loading cargo is a thing that does not exist yet.
pub fn hull_mass(design: &ShipDesign) -> f64 {
    design.parts.iter().map(|p| p.kind.def().mass).sum()
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

/// What the ship weighs with its crew aboard and nothing in the hold.
///
/// An empty design comes back as [`MassError::HullTooLight`] rather than as
/// zero, because `physics` refuses a ship that weighs nothing — one would
/// accelerate infinitely, and rounding it up would hide whatever lost the
/// parts.
pub fn ship_mass(design: &ShipDesign, crew_count: u32) -> Result<Mass, MassError> {
    physics::ship_mass(hull_mass(design), &[], crew_count)
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
