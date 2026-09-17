//! The numbers the world is generated from.
//!
//! All placeholders, and all load-bearing. Every one of them either decides
//! what a seed produces or decides whether a layout is legal, so the list at
//! the bottom of this comment is not a formality: change any of these and the
//! same seed gives a different galaxy, which means saves and lobbies
//! disagreeing about where anything is.
//!
//! **A change to any of the following is a [`crate::GENERATOR_VERSION`] bump:**
//! [`TRAVEL_BAND`], [`REFERENCE_SHIP`] and anything it is built from —
//! `physics`'s `PLAYER_MASS` and the resource masses it carries — the day
//! length in the `time` crate, the travel-time formula itself, and the
//! desolation mapping below.
//!
//! The station shares, the salvage and the hazards are *not* on that list on
//! purpose: they change what is in a system without changing whether a layout
//! passes validation, so they can be tuned while the world stays the shape it
//! was.

use physics::{EngineSpec, Facing, Mass, ResourceId};

/// How far apart things in a system are allowed to be, **in days for the
/// reference ship**. Not in distance: a distance means nothing to a player
/// and everything to a generator, and days are what the constraint is
/// actually about — a system you can cross in an afternoon has no geography,
/// and one that takes a month to cross has too much.
#[derive(Clone, Copy, Debug)]
pub struct TravelBand {
    pub min_days: f64,
    pub max_days: f64,
}

pub const TRAVEL_BAND: TravelBand = TravelBand {
    min_days: 1.0,
    max_days: 14.0,
};

/// The ship every layout is measured against.
///
/// **Fixed.** Not the player's ship, not derived from the build area or from
/// how many people are in the lobby — the world has to come out the same for
/// a seed whatever the lobby settings say, and a reference ship that moved
/// with them would quietly make a four-player galaxy a different galaxy.
///
/// The values are chosen so the derived forward acceleration is exactly
/// `1.0`: two engines of a thousand against two thousand of mass. That is
/// arbitrary and it is also convenient — it makes a one-day hop 518400 world
/// units, which is a number that can be held in the head while reading a
/// layout dump.
#[derive(Clone, Copy, Debug)]
pub struct ReferenceShip {
    pub engine_count: u32,
    pub engine_thrust: f64,
    pub facing: Facing,
    pub hull_mass: f64,
    pub crew_count: u32,
    /// Carried as a manifest rather than as a lump of mass so it goes through
    /// the same `physics::ship_mass` every other ship does. A second way of
    /// weighing a ship is a second thing to keep in step.
    pub cargo: [(ResourceId, u32); 1],
    /// The reference ship turns round to brake, so its deceleration is its
    /// acceleration. **If the flight step ever forbids rotation this becomes
    /// false, every hop in every system gets longer, and the generator
    /// version has to move with it.**
    pub can_flip: bool,
}

pub const REFERENCE_SHIP: ReferenceShip = ReferenceShip {
    engine_count: 2,
    engine_thrust: 1000.0,
    facing: Facing::Forward,
    hull_mass: 1000.0,
    crew_count: 2,
    cargo: [(ResourceId::Metal, 100)],
    can_flip: true,
};

impl ReferenceShip {
    /// What it weighs, through the ordinary mass function.
    ///
    /// Panics if the constants above do not describe a ship that can exist.
    /// That is deliberate: this is a compile-time-ish fact about a constant,
    /// not a runtime condition, and a generator that quietly carried on with
    /// a nonsense reference would produce a nonsense galaxy in silence.
    pub fn mass(&self) -> Mass {
        physics::ship_mass(self.hull_mass, &self.cargo, self.crew_count)
            .expect("the reference ship's constants must describe a real ship")
    }

    pub fn engines(&self) -> Vec<EngineSpec> {
        (0..self.engine_count)
            .map(|_| EngineSpec {
                thrust: self.engine_thrust,
                facing: self.facing,
            })
            .collect()
    }

    /// Acceleration along the axis it thrusts on. **Derived, never stored** —
    /// storing it would be a second copy of the masses to keep in step, and
    /// the whole reason the physics contract is a crate is to not have those.
    pub fn acceleration(&self) -> f64 {
        physics::axis_acceleration(&self.engines(), self.mass(), self.facing)
    }

    /// What it slows down with. The same, while it can turn round; nothing at
    /// all if it cannot and has no engine facing backwards — and a ship that
    /// cannot stop gets no travel time, which is exactly what should happen.
    pub fn deceleration(&self) -> f64 {
        if self.can_flip {
            self.acceleration()
        } else {
            physics::axis_acceleration(&self.engines(), self.mass(), Facing::Backward)
        }
    }
}

/// How long the reference ship takes to cross `distance`, in days. The one
/// question the layout checks ask.
pub fn reference_days(distance: f64) -> Option<f64> {
    let ship = REFERENCE_SHIP;
    physics::travel_days(distance, ship.acceleration(), ship.deceleration())
}

/// How far apart two things have to be for the reference ship to take `days`
/// over the trip. The one answer the layout *builder* needs.
pub fn reference_distance(days: f64) -> Option<f64> {
    let ship = REFERENCE_SHIP;
    physics::travel_distance(days, ship.acceleration(), ship.deceleration())
}

// --- how run-down a system is ------------------------------------------

/// Desolation is drawn from this shape. Raising a uniform draw to a power
/// above one leans it towards zero, so most systems are middling and the
/// genuinely abandoned ones are rare — which is what makes finding one worth
/// anything.
const DESOLATION_SKEW: f64 = 1.6;

/// Draw a system's desolation. In `[0, 1]`, **stored and never displayed**:
/// it is a generator input, and a number on the screen saying how bad a place
/// is would do the exploring for the player.
pub fn desolation(u: f64) -> f64 {
    u.clamp(0.0, 1.0).powf(DESOLATION_SKEW)
}

/// What desolation means for how spread out a system is: nothing at all at
/// zero, and the full span at one.
///
/// This is the *target* hop, not the hop. The actual distance between two
/// neighbours is drawn between the minimum and this, so a desolate system is
/// on average emptier rather than uniformly stretched.
pub fn target_hop_days(desolation: f64) -> f64 {
    let TravelBand { min_days, max_days } = TRAVEL_BAND;
    let d = desolation.clamp(0.0, 1.0);
    (min_days + (max_days - min_days) * d).min(max_days)
}

// --- stations -----------------------------------------------------------

/// What fraction of systems have a station at all. A target rather than a
/// quota — each system rolls against it on its own. It was a quarter, and
/// the galaxy read as empty: a map where most stars are somewhere you cannot
/// start and a system with one dock is a system with nowhere to go.
pub const STATION_SHARE: f64 = 0.6;

/// Once a system has a station, the chance of a second, and then of a
/// third. Each only where there is a body of the right kind free for it —
/// [`parent_suits`] and "at most one station per parent body" still hold —
/// so a one-planet system stays a one-station system whatever these say.
pub const MORE_STATIONS: [f64; 2] = [0.55, 0.3];

/// Relays want somewhere nobody goes. A system at or above this is a
/// candidate; below it, a relay is not sited there.
pub const RELAY_DESOLATION: f64 = 0.6;

/// What a station is.
///
/// The discriminants cross the wasm boundary as numbers, so they are written
/// out and must not be reordered — the host's name table is indexed by them,
/// exactly as `SPOT_NAMES` and `JOB_NAMES` are.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(u32)]
pub enum StationKind {
    /// In orbit of somewhere people live. The ordinary case.
    Orbital = 0,
    /// Hung off a gas giant, cracking its atmosphere.
    Refinery = 1,
    /// Bolted to a belt.
    MiningOutpost = 2,
    /// Nobody aboard. What is left is worth taking.
    Derelict = 3,
    /// Deep space, on its own, listening.
    Relay = 4,
}

impl StationKind {
    pub const ALL: [StationKind; 5] = [
        StationKind::Orbital,
        StationKind::Refinery,
        StationKind::MiningOutpost,
        StationKind::Derelict,
        StationKind::Relay,
    ];

    /// Whether a station of this kind has this to sell.
    ///
    /// The whole of the rule, and the only place it is written down. A
    /// derelict sells nothing — there is nobody aboard to sell it. Galvum
    /// is the mining outposts' alone, which is what makes one somewhere
    /// worth flying to. An emitter is never sold: it is made at a
    /// workbench out of galvum, and a station that sold finished emitters
    /// would make the galvum pointless. Every station **buys** anything;
    /// this is only about what is on the shelf.
    pub fn sells(self, resource: physics::ResourceId) -> bool {
        use physics::ResourceId;
        match (self, resource) {
            (StationKind::Derelict, _) => false,
            // Made at the armoury and the workbench; nobody stocks them.
            (_, ResourceId::Emitter | ResourceId::Handgun | ResourceId::Vest) => false,
            (StationKind::MiningOutpost, ResourceId::Galvum) => true,
            (_, ResourceId::Galvum) => false,
            _ => true,
        }
    }
}

/// What a body is.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(u32)]
pub enum BodyKind {
    RockyPlanet = 0,
    GasGiant = 1,
    IceWorld = 2,
    /// Measured as a single point, however wide it looks on a map. A belt
    /// that was a ring would make "how far apart are these two things" a
    /// different question for every pair, and the layout checks would have to
    /// answer it a different way for every pair too.
    AsteroidBelt = 3,
}

impl BodyKind {
    pub const ALL: [BodyKind; 4] = [
        BodyKind::RockyPlanet,
        BodyKind::GasGiant,
        BodyKind::IceWorld,
        BodyKind::AsteroidBelt,
    ];

    /// How often each turns up when a system is being filled in. Weights, not
    /// probabilities — they are normalised where they are used.
    pub fn weight(self) -> f64 {
        match self {
            BodyKind::RockyPlanet => 4.0,
            BodyKind::GasGiant => 2.5,
            BodyKind::IceWorld => 2.0,
            BodyKind::AsteroidBelt => 2.5,
        }
    }
}

/// Which bodies a kind of station can be built on.
///
/// This is the whole of the matching rule and the only place it is written
/// down. A refinery hangs off a gas giant because that is what it refines; an
/// outpost is on a belt because that is what it mines; a relay is out in deep
/// space on its own; and a derelict can be anywhere, because whatever it was
/// for stopped mattering a long time ago.
pub fn parent_suits(kind: StationKind, parent: Option<BodyKind>) -> bool {
    match (kind, parent) {
        (StationKind::Refinery, Some(BodyKind::GasGiant)) => true,
        (StationKind::MiningOutpost, Some(BodyKind::AsteroidBelt)) => true,
        (StationKind::Orbital, Some(BodyKind::RockyPlanet | BodyKind::IceWorld)) => true,
        (StationKind::Relay, None) => true,
        (StationKind::Derelict, _) => true,
        _ => false,
    }
}

/// What one walk outside brings back from a belt.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct BeltYield {
    /// Units of ore a suited Bim gathers in one walk.
    pub ore: u32,
    /// Whether the belt has galvum in it: one unit a walk, on top.
    pub galvum: bool,
}

/// How much ore a walk gathers, least and most, and what share of belts
/// carry galvum. Placeholders: a walk is an hour and a half outside, and
/// eight to twelve ore is four to six metal, a wall and a half.
pub const BELT_ORE: (u32, u32) = (8, 12);
pub const GALVUM_SHARE: f64 = 0.35;

/// What a belt yields. Off a stream of its own — [`Purpose::BeltYield`] —
/// seeded by the galaxy, the star and the body, so it is the same for two
/// players and it moved nothing else when it arrived. A body that is not a
/// belt yields nothing.
pub fn belt_yield(galaxy_seed: u64, star_id: u32, body_id: u32, kind: BodyKind) -> BeltYield {
    if kind != BodyKind::AsteroidBelt {
        return BeltYield {
            ore: 0,
            galvum: false,
        };
    }
    let seed = crate::rng::seed_for(
        galaxy_seed,
        star_id,
        crate::GENERATOR_VERSION,
        crate::rng::Purpose::BeltYield,
    );
    let mut rng = crate::rng::Rng::new(seed ^ crate::rng::mix(body_id as u64));
    let ore = BELT_ORE.0 + rng.below(BELT_ORE.1 - BELT_ORE.0 + 1);
    let galvum = rng.chance(GALVUM_SHARE);
    BeltYield { ore, galvum }
}

/// What is left to be pulled out of the wreck.
///
/// Only derelicts have any, by default — and "by default" is this function:
/// somewhere to hang an exception without going hunting through the
/// generator for the place the number is decided.
pub fn salvage_sites(kind: StationKind, u: f64) -> u32 {
    match kind {
        StationKind::Derelict => 3 + (u * 6.0) as u32,
        _ => 0,
    }
}

/// What can be wrong with a station.
///
/// **Stored and never displayed.** The map generator places these; the
/// preview does not mention them, because a player who can read the hazards
/// off the star map before setting out is not exploring.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(u32)]
pub enum HazardKind {
    Radiation = 0,
    /// A hull that is open to space.
    Breach = 1,
    Fire = 2,
    /// Something growing, or something spilt.
    Contamination = 3,
}

impl HazardKind {
    pub const ALL: [HazardKind; 4] = [
        HazardKind::Radiation,
        HazardKind::Breach,
        HazardKind::Fire,
        HazardKind::Contamination,
    ];
}

/// How many kinds of thing tend to be wrong with each kind of station, and
/// how many sites of each. A working station has the odd problem; a derelict
/// is nothing but.
pub fn hazard_pressure(kind: StationKind) -> f64 {
    match kind {
        StationKind::Orbital => 0.15,
        StationKind::Refinery => 0.45,
        StationKind::MiningOutpost => 0.35,
        StationKind::Derelict => 1.0,
        StationKind::Relay => 0.25,
    }
}

// A station used to have stores, and a blueprint used to carry them. Both
// went when the crew started bringing **money** rather than starting with
// material: what a ship costs is now a price in euros out of one shared pool
// — see `crates/economy` and `crates/shipdesign` — and a station holding
// crates of metal was a starting condition dressed up as a fact about the
// world. Trading is a separate thing that does not exist yet, and when it
// does, what a station will sell is a price list rather than a stockpile.

#[cfg(test)]
mod tests {
    use super::*;

    /// The defaults are chosen to make this exactly one. If it ever is not,
    /// every distance in every system has quietly changed scale.
    #[test]
    fn the_reference_ship_accelerates_at_one() {
        let a = REFERENCE_SHIP.acceleration();
        assert!((a - 1.0).abs() < 1e-12, "acceleration was {a}");
        assert_eq!(REFERENCE_SHIP.deceleration(), a);
    }

    #[test]
    fn a_day_is_the_distance_it_looks() {
        let d = reference_distance(1.0).unwrap();
        assert!((d - 518_400.0).abs() < 1e-6, "a one-day hop was {d}");
        assert!((reference_days(d).unwrap() - 1.0).abs() < 1e-12);
    }

    #[test]
    fn the_travel_band_makes_sense() {
        assert!(TRAVEL_BAND.min_days > 0.0);
        assert!(TRAVEL_BAND.max_days > TRAVEL_BAND.min_days);
    }

    #[test]
    fn desolation_maps_onto_the_whole_band() {
        assert!((target_hop_days(0.0) - TRAVEL_BAND.min_days).abs() < 1e-12);
        assert!((target_hop_days(1.0) - TRAVEL_BAND.max_days).abs() < 1e-12);
        // And never out of it, whatever it is handed.
        for &d in &[-1.0, 0.0, 0.3, 1.0, 2.0] {
            let t = target_hop_days(d);
            assert!(
                t >= TRAVEL_BAND.min_days && t <= TRAVEL_BAND.max_days,
                "{t}"
            );
        }
    }

    #[test]
    fn desolation_stays_in_its_range_and_leans_low() {
        let mut rng = crate::rng::Rng::new(1);
        let mut high = 0;
        for _ in 0..10_000 {
            let d = desolation(rng.unit());
            assert!((0.0..=1.0).contains(&d));
            if d > 0.5 {
                high += 1;
            }
        }
        // A uniform draw would put half of them above a half.
        assert!(high < 4_000, "{high} systems in ten thousand were desolate");
    }

    /// A belt yields something and nothing else does; the same belt yields
    /// the same thing every time, and about a third of them have galvum.
    #[test]
    fn a_belt_yields_the_same_thing_every_time_and_only_a_belt_yields() {
        let a = belt_yield(7, 3, 2, BodyKind::AsteroidBelt);
        assert_eq!(a, belt_yield(7, 3, 2, BodyKind::AsteroidBelt));
        assert!(a.ore >= BELT_ORE.0 && a.ore <= BELT_ORE.1, "{a:?}");
        for kind in [
            BodyKind::RockyPlanet,
            BodyKind::GasGiant,
            BodyKind::IceWorld,
        ] {
            assert_eq!(belt_yield(7, 3, 2, kind).ore, 0);
            assert!(!belt_yield(7, 3, 2, kind).galvum);
        }
        let mut rich = 0;
        for body in 0..1000u32 {
            if belt_yield(7, body, 1, BodyKind::AsteroidBelt).galvum {
                rich += 1;
            }
        }
        assert!(
            (250..=450).contains(&rich),
            "{rich} rich belts in a thousand"
        );
    }

    /// What is on the shelf where: galvum only at an outpost, an emitter
    /// nowhere, nothing at a derelict, and everything else everywhere
    /// somebody lives.
    #[test]
    fn what_each_kind_of_station_sells() {
        use physics::ResourceId;
        for kind in StationKind::ALL {
            for resource in ResourceId::ALL {
                let want = match (kind, resource) {
                    (StationKind::Derelict, _) => false,
                    (_, ResourceId::Emitter | ResourceId::Handgun | ResourceId::Vest) => false,
                    (StationKind::MiningOutpost, ResourceId::Galvum) => true,
                    (_, ResourceId::Galvum) => false,
                    _ => true,
                };
                assert_eq!(kind.sells(resource), want, "{kind:?} {resource:?}");
            }
        }
        assert!(StationKind::MiningOutpost.sells(ResourceId::Galvum));
        assert!(!StationKind::Orbital.sells(ResourceId::Galvum));
        assert!(!StationKind::MiningOutpost.sells(ResourceId::Emitter));
        assert!(StationKind::Relay.sells(ResourceId::Fuel));
        assert!(StationKind::Orbital.sells(ResourceId::Medkit));
        assert!(!StationKind::Orbital.sells(ResourceId::Handgun));
    }

    /// The matching rule, both ways round: what each kind accepts, and what
    /// it refuses. The refusals are the half that would go unnoticed.
    #[test]
    fn stations_only_sit_where_they_belong() {
        use BodyKind::*;
        use StationKind::*;
        assert!(parent_suits(Refinery, Some(GasGiant)));
        assert!(!parent_suits(Refinery, Some(RockyPlanet)));
        assert!(!parent_suits(Refinery, None));
        assert!(parent_suits(MiningOutpost, Some(AsteroidBelt)));
        assert!(!parent_suits(MiningOutpost, Some(IceWorld)));
        assert!(parent_suits(Orbital, Some(RockyPlanet)));
        assert!(parent_suits(Orbital, Some(IceWorld)));
        assert!(!parent_suits(Orbital, Some(GasGiant)));
        assert!(!parent_suits(Orbital, None));
        assert!(parent_suits(Relay, None));
        assert!(!parent_suits(Relay, Some(RockyPlanet)));
        for &b in &BodyKind::ALL {
            assert!(parent_suits(Derelict, Some(b)));
        }
        assert!(parent_suits(Derelict, None));
    }

    #[test]
    fn only_derelicts_have_salvage_by_default() {
        for &k in &StationKind::ALL {
            let n = salvage_sites(k, 0.9);
            if k == StationKind::Derelict {
                assert!(n > 0);
            } else {
                assert_eq!(n, 0, "{k:?} had salvage");
            }
        }
    }
}
