//! The ship physics contract.
//!
//! Three questions, answered by pure functions over plain data: what does a
//! ship weigh, what does that do to its engines, and how long is the trip.
//! There is no ECS here, no buildables, no ship — a ship is whatever the
//! caller hands in, as a hull mass, a cargo manifest and a list of engines.
//!
//! It is its own crate because three different things need the same answers
//! and must not each work them out their own way: the **world generator**
//! needs them now, to check that the bodies it scatters through a system are
//! a sane number of days apart; the **builder** will need them to tell a
//! player what the ship they are welding together will fly like; and the
//! **flight step** will need them to fly it. Two of those three do not exist
//! yet, which is exactly why this is written down as a contract rather than
//! discovered later inside whichever one is built first.
//!
//! # Units
//!
//! Distance is in world units — the same ones positions are measured in, and
//! deliberately not given a name in metres, because nothing yet depends on
//! one. Time is the `time` crate's base unit, the **game minute**. So an
//! acceleration is world units per minute squared, and it is worth saying out
//! loud because the number looks strange otherwise: the reference ship's
//! forward acceleration is `1.0`, which covers half a million units in a day.
//!
//! # What is not here
//!
//! **Rotation.** [`Facing`] is the ship's own frame — forward, backward,
//! left, right — and how those map onto a heading through space depends on
//! whether ships turn, which is not decided. Nothing in this crate assumes an
//! answer, and nothing in it should be made to until there is one.
//!
//! **Anything that changes over time.** Mass is constant for the length of a
//! trip; it is recalculated on discrete events only — a build, a
//! deconstruction, a load, an unload, a crew member joining or leaving — and
//! never continuously. There is no fuel burn, so a ship does not get lighter
//! on the way.

pub mod data;

pub use data::{PLAYER_MASS, RESOURCES, ResourceDef, ResourceId};

/// What went wrong when a ship was weighed.
///
/// A mass that is not strictly positive is a **validation error**, never a
/// clamp. An infinitely light ship accelerates infinitely, and quietly
/// rounding one up to the floor would turn a bug somewhere upstream — a hull
/// that lost its parts, a manifest that came back empty — into a ship that
/// flies suspiciously well and says nothing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MassError {
    /// The hull and structure came in under [`data::MIN_HULL_MASS`].
    HullTooLight,
    /// A figure was negative, infinite or not a number. Usually a sign that
    /// something was subtracted that should not have been.
    NotFinite,
}

/// A ship's mass, once it has been checked.
///
/// The only ways to make one are [`ship_mass`] and [`Mass::new`], and both
/// refuse anything that is not strictly positive. That is the point of the
/// newtype: [`axis_acceleration`] divides by it, and a type that cannot hold
/// a zero is a stronger promise than a comment asking callers not to pass
/// one.
#[derive(Clone, Copy, PartialEq, PartialOrd, Debug)]
pub struct Mass(f64);

impl Mass {
    /// A mass from a number that has already been worked out elsewhere.
    /// Strictly positive and finite, or nothing.
    pub fn new(mass: f64) -> Result<Mass, MassError> {
        if !mass.is_finite() || mass <= 0.0 {
            return Err(MassError::NotFinite);
        }
        Ok(Mass(mass))
    }

    pub fn get(self) -> f64 {
        self.0
    }
}

/// What a cargo manifest weighs: units of each resource times what a unit of
/// it weighs.
///
/// Only what is **physically on the ship** belongs in the manifest. A
/// station's stockpile is not the ship's mass however much of it the crew
/// intend to use — it counts from the moment it is loaded and not before.
pub fn cargo_mass(cargo: &[(ResourceId, u32)]) -> f64 {
    cargo
        .iter()
        .map(|&(id, units)| units as f64 * id.mass_per_unit())
        .sum()
}

/// What a ship weighs: its hull and structure, plus what it is carrying, plus
/// the crew aboard.
///
/// The hull figure is everything welded to it — plating, engines, the fittings
/// inside — as one number, because this step has no buildables to add up.
pub fn ship_mass(
    hull_and_structure: f64,
    cargo: &[(ResourceId, u32)],
    crew_count: u32,
) -> Result<Mass, MassError> {
    if !hull_and_structure.is_finite() {
        return Err(MassError::NotFinite);
    }
    if hull_and_structure < data::MIN_HULL_MASS {
        return Err(MassError::HullTooLight);
    }
    let total = hull_and_structure + cargo_mass(cargo) + crew_count as f64 * PLAYER_MASS;
    Mass::new(total)
}

/// Which way an engine pushes the ship, in the ship's own frame.
///
/// This is the direction of the **push**, not the direction the engine's bell
/// points: an engine that drives the ship forward is `Forward`, whichever way
/// round it is bolted.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum Facing {
    Forward = 0,
    Backward = 1,
    Left = 2,
    Right = 3,
}

impl Facing {
    pub const ALL: [Facing; 4] = [
        Facing::Forward,
        Facing::Backward,
        Facing::Left,
        Facing::Right,
    ];
}

/// One engine: how hard it pushes and which way.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct EngineSpec {
    /// Force, not acceleration. Never negative — an engine that pushes the
    /// other way is a different `facing`, not a negative thrust.
    pub thrust: f64,
    pub facing: Facing,
}

/// What is wrong with an engine.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EngineError {
    /// Negative, infinite or not a number.
    BadThrust,
}

impl EngineSpec {
    /// A checked engine. The fields are public so a data file can be written
    /// as a plain table, but anything reading engines from outside the
    /// program should come through here.
    pub fn new(thrust: f64, facing: Facing) -> Result<EngineSpec, EngineError> {
        if !thrust.is_finite() || thrust < 0.0 {
            return Err(EngineError::BadThrust);
        }
        Ok(EngineSpec { thrust, facing })
    }

    pub fn check(&self) -> Result<(), EngineError> {
        EngineSpec::new(self.thrust, self.facing).map(|_| ())
    }
}

/// How hard the ship accelerates along one of its own axes.
///
/// Only the engines facing that way contribute. An engine pushing left does
/// nothing at all for forward acceleration — there is no vector decomposition
/// here, because in the ship's frame each engine is squarely on one axis.
///
/// No engines on the axis means `0.0`, which is a real answer rather than a
/// missing one: a ship with nothing pushing backward cannot slow down, and
/// [`travel_days`] turns exactly that into a refusal to quote a trip.
pub fn axis_acceleration(engines: &[EngineSpec], ship_mass: Mass, axis: Facing) -> f64 {
    let thrust: f64 = engines
        .iter()
        .filter(|e| e.facing == axis)
        .map(|e| e.thrust)
        .sum();
    thrust / ship_mass.get()
}

/// How many days a trip of `distance` takes, starting and ending at rest.
///
/// The ship accelerates at `a_acc` for the first half of the work and
/// decelerates at `a_dec` for the second. There is no speed cap and no
/// coasting: it is flat out to the midpoint and braking from there.
///
/// ```text
/// t = sqrt( 2 * distance * (a_acc + a_dec) / (a_acc * a_dec) )
/// ```
///
/// `t` comes out in the base time unit — game minutes — and is handed back in
/// days. With equal accelerations it reduces to `2 * sqrt(distance / a)`,
/// which is the form worth remembering and which the tests pin.
///
/// Returns `None` when the trip is not a trip: a ship that cannot accelerate
/// never sets off, and one that cannot decelerate never stops. Neither is a
/// very long journey, so neither gets a number.
///
/// The result is **never stored**. It is derived from masses and thrusts that
/// change whenever the ship does, so a saved copy would be a lie the moment
/// anyone welded a crate to the hull.
pub fn travel_days(distance: f64, a_acc: f64, a_dec: f64) -> Option<f64> {
    if !distance.is_finite() || distance < 0.0 {
        return None;
    }
    if !a_acc.is_finite() || !a_dec.is_finite() || a_acc <= 0.0 || a_dec <= 0.0 {
        return None;
    }
    let minutes = (2.0 * distance * (a_acc + a_dec) / (a_acc * a_dec)).sqrt();
    Some(time::days(minutes))
}

/// The other way round: how far away something has to be for the trip to take
/// `days`.
///
/// This is [`travel_days`] solved for distance, and it exists because the
/// world generator builds systems *outwards* — it decides a body should be a
/// four-day hop from the last one and needs to know where to put it, rather
/// than scattering bodies and measuring afterwards.
///
/// ```text
/// d = t² * a_acc * a_dec / (2 * (a_acc + a_dec))
/// ```
///
/// Same refusals as [`travel_days`]: no acceleration, no answer.
pub fn travel_distance(days: f64, a_acc: f64, a_dec: f64) -> Option<f64> {
    if !days.is_finite() || days < 0.0 {
        return None;
    }
    if !a_acc.is_finite() || !a_dec.is_finite() || a_acc <= 0.0 || a_dec <= 0.0 {
        return None;
    }
    let minutes = time::minutes(days);
    Some(minutes * minutes * a_acc * a_dec / (2.0 * (a_acc + a_dec)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Everything here is compared with a tolerance rather than `==`: these
    /// are square roots, and an exact float equality would be asserting how
    /// the optimiser ordered the multiplications.
    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1.0)
    }

    #[test]
    fn resource_table_holds_together() {
        assert!(data::defs_are_sound());
        assert_eq!(ResourceId::Metal.def().id, ResourceId::Metal);
    }

    #[test]
    fn cargo_is_units_times_unit_mass() {
        let hold = [(ResourceId::Ore, 3), (ResourceId::Components, 10)];
        assert!(close(cargo_mass(&hold), 3.0 * 10.0 + 10.0 * 2.0));
        assert_eq!(cargo_mass(&[]), 0.0);
    }

    #[test]
    fn mass_is_hull_plus_cargo_plus_crew() {
        let m = ship_mass(1000.0, &[(ResourceId::Metal, 100)], 2).unwrap();
        assert!(close(m.get(), 1000.0 + 800.0 + 2.0 * PLAYER_MASS));
    }

    #[test]
    fn an_empty_ship_with_no_crew_still_weighs_its_hull() {
        let m = ship_mass(data::MIN_HULL_MASS, &[], 0).unwrap();
        assert!(close(m.get(), data::MIN_HULL_MASS));
    }

    /// The guard is an error, not a clamp. A hull under the floor comes back
    /// as a refusal — nothing silently rounds it up.
    #[test]
    fn too_light_a_hull_is_refused() {
        assert_eq!(ship_mass(0.0, &[], 4), Err(MassError::HullTooLight));
        assert_eq!(ship_mass(-1.0, &[], 4), Err(MassError::HullTooLight));
        assert_eq!(
            ship_mass(data::MIN_HULL_MASS / 2.0, &[], 4),
            Err(MassError::HullTooLight)
        );
        assert_eq!(ship_mass(f64::NAN, &[], 0), Err(MassError::NotFinite));
        assert_eq!(Mass::new(0.0), Err(MassError::NotFinite));
    }

    #[test]
    fn only_engines_on_the_axis_count() {
        let m = Mass::new(1000.0).unwrap();
        let engines = [
            EngineSpec::new(500.0, Facing::Forward).unwrap(),
            EngineSpec::new(500.0, Facing::Forward).unwrap(),
            EngineSpec::new(9000.0, Facing::Left).unwrap(),
        ];
        assert!(close(axis_acceleration(&engines, m, Facing::Forward), 1.0));
        assert!(close(axis_acceleration(&engines, m, Facing::Left), 9.0));
        assert_eq!(axis_acceleration(&engines, m, Facing::Backward), 0.0);
        assert_eq!(axis_acceleration(&[], m, Facing::Forward), 0.0);
    }

    #[test]
    fn a_backward_engine_is_not_a_negative_forward_one() {
        assert_eq!(
            EngineSpec::new(-1.0, Facing::Forward),
            Err(EngineError::BadThrust)
        );
        assert_eq!(
            EngineSpec::new(f64::INFINITY, Facing::Forward),
            Err(EngineError::BadThrust)
        );
        assert!(EngineSpec::new(0.0, Facing::Forward).is_ok());
    }

    /// The form worth remembering: with the same acceleration both ways the
    /// whole thing collapses to `2 * sqrt(d / a)`.
    #[test]
    fn equal_accelerations_give_two_root_d_over_a() {
        let cases: [(f64, f64); 5] = [
            (1.0, 1.0),
            (518_400.0, 1.0),
            (1e9, 0.5),
            (12_345.0, 3.25),
            (0.25, 100.0),
        ];
        for &(d, a) in &cases {
            let want = time::days(2.0 * (d / a).sqrt());
            assert!(
                close(travel_days(d, a, a).unwrap(), want),
                "d={d} a={a}: {:?} != {want}",
                travel_days(d, a, a)
            );
        }
    }

    /// The reference ship's numbers, checked end to end: a hull of 1000, a
    /// hundred units of metal, two crew and two engines of 1000 each comes
    /// out at exactly 1.0 forward — which is what makes a one-day hop
    /// 518400 units and keeps the world generator's distances legible.
    #[test]
    fn the_reference_arrangement_accelerates_at_one() {
        let m = ship_mass(1000.0, &[(ResourceId::Metal, 100)], 2).unwrap();
        let engines = [
            EngineSpec::new(1000.0, Facing::Forward).unwrap(),
            EngineSpec::new(1000.0, Facing::Forward).unwrap(),
        ];
        let a = axis_acceleration(&engines, m, Facing::Forward);
        assert!(close(a, 1.0), "forward acceleration was {a}");
        assert!(close(travel_days(518_400.0, a, a).unwrap(), 1.0));
    }

    #[test]
    fn a_slower_brake_makes_a_longer_trip() {
        let fast = travel_days(1e6, 1.0, 1.0).unwrap();
        let slow = travel_days(1e6, 1.0, 0.25).unwrap();
        assert!(slow > fast, "{slow} should be longer than {fast}");
    }

    #[test]
    fn no_distance_is_no_time() {
        assert_eq!(travel_days(0.0, 1.0, 1.0), Some(0.0));
    }

    /// A ship that cannot start, or cannot stop, gets no quote at all.
    #[test]
    fn a_ship_that_cannot_stop_gets_no_answer() {
        assert_eq!(travel_days(1000.0, 1.0, 0.0), None);
        assert_eq!(travel_days(1000.0, 0.0, 1.0), None);
        assert_eq!(travel_days(1000.0, -1.0, 1.0), None);
        assert_eq!(travel_days(1000.0, f64::NAN, 1.0), None);
        assert_eq!(travel_days(-1.0, 1.0, 1.0), None);
        assert_eq!(travel_days(f64::INFINITY, 1.0, 1.0), None);
    }

    /// The inverse has to actually invert. The world generator places a body
    /// by asking for a distance and then the layout check asks how long that
    /// takes; if the two disagree by anything at all, a system built exactly
    /// on the minimum hop comes out failing its own minimum.
    #[test]
    fn distance_and_days_are_inverses() {
        let cases: [(f64, f64, f64); 5] = [
            (1.0, 1.0, 1.0),
            (14.0, 1.0, 1.0),
            (3.5, 2.0, 0.5),
            (0.0, 1.0, 1.0),
            (14.0, 0.125, 7.0),
        ];
        for &(days, a1, a2) in &cases {
            let d = travel_distance(days, a1, a2).unwrap();
            let back = travel_days(d, a1, a2).unwrap();
            assert!(close(back, days), "{days} days -> {d} -> {back} days");
        }
        assert_eq!(travel_distance(1.0, 0.0, 1.0), None);
        assert_eq!(travel_distance(-1.0, 1.0, 1.0), None);
    }

    /// Distance and time are the way round they look: four times the distance
    /// is twice the time, because the whole trip is under acceleration.
    #[test]
    fn four_times_the_distance_is_twice_the_time() {
        let one = travel_days(1e6, 1.0, 1.0).unwrap();
        let four = travel_days(4e6, 1.0, 1.0).unwrap();
        assert!(close(four, 2.0 * one));
    }
}
