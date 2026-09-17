//! What a design does when you push it: how heavy it is, where its weight
//! sits, how hard it accelerates and how fast it turns.
//!
//! All of it is derived from the design and **nothing here is stored on one**.
//! A ship that gains a wall is a different ship to fly, and the one call that
//! says so is [`dynamics`] — `world::World::on_ship_changed` is the only place
//! that is meant to be called from, precisely so there is one moment at which
//! a ship's flying changes rather than a scattering of them.
//!
//! # Two simplifications, both deliberate
//!
//! - **A main engine produces no torque**, wherever it is bolted. Its push
//!   goes through the centre of mass. The alternative is a ship that spins
//!   because a player laid it out symmetrically to the eye and not to the
//!   gram, with nothing on the page to say why and nothing they could do about
//!   it.
//! - **Cargo and crew sit at the centre of mass.** They count towards the
//!   total — an engine has to push them — and they move neither the centre of
//!   mass nor the inertia, because where a crate is stowed and where a Bim is
//!   standing are questions this crate has no way to ask.
//!
//! Both are written down here rather than discovered later: they are the two
//! things somebody adding a cargo-position model would have to undo.

use physics::{Facing, Mass, MassError};
use shipdesign::parts::{PartKind, TILE};
use shipdesign::{ShipDesign, part_mass};
use worldgen::math::{DVec2, dvec2};

use crate::data;

/// Why a design could not be flown.
///
/// One variant, and it is `physics`'s: a ship that weighs nothing accelerates
/// infinitely, and everything below divides by the mass.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DynamicsError {
    /// The design does not describe a ship that can exist. See
    /// [`physics::MassError`].
    NotAShip(MassError),
}

/// Everything about a design that a trip needs, worked out once.
///
/// A [`crate::Plan`] keeps a copy of the one it was planned with and flies it
/// to the end. That is what makes a trip's arrival time a promise: welding a
/// shelf on halfway does not move the ship's arrival, it changes what the
/// *next* trip will be like.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Dynamics {
    /// Hull, cargo and crew. `physics::ship_mass`'s answer and no other.
    pub mass: Mass,
    /// Where the weight sits, in **design coordinates**: world units from the
    /// top-left corner of tile (0, 0), with `y` growing downwards the way the
    /// grid does. The rendering and the anchor arithmetic both turn it into
    /// system coordinates; nothing stores it in those.
    pub centre_of_mass: DVec2,
    /// Moment of inertia about the centre of mass, with
    /// [`data::INERTIA_FLOOR`] under it.
    pub inertia: f64,
    /// Along the ship's nose, in world units per game minute squared.
    pub a_forward: f64,
    /// Along its tail. Zero for a ship with no backward engine, which is the
    /// ordinary case and is what makes a flip worth doing.
    pub a_backward: f64,
    /// Angular acceleration from every thruster at once, in radians per game
    /// minute squared. Zero without thrusters, which is a ship that can only
    /// fly the heading it was left on.
    pub alpha: f64,
    /// How many engines burn when the ship is pushing forward, and how many
    /// when it is pushing back. What the painter lights; the fuel bill is
    /// per unit of thrust and comes off the acceleration instead.
    pub forward_engines: u32,
    pub backward_engines: u32,
    /// Whether there is anywhere to fly it from.
    pub has_helm: bool,
    /// Whether there is a way off it — an airlock that opens onto space, a
    /// port — which is the difference between docking at a station and
    /// holding station beside one. An airlock buried in the deck is not one.
    pub has_airlock: bool,
}

impl Dynamics {
    /// What one minute of burning costs at `accel` along the ship's line.
    ///
    /// Fuel goes as **thrust**, not as a count of engines: a heavy engine
    /// pushing five times as hard burns five times as much, or it would be
    /// the only engine worth having. The thrust is the acceleration times
    /// the mass, which is why this takes the segment's `accel` rather than
    /// its engine count — the count is what the painter lights, and the two
    /// are read off the same segment. Nothing while coasting or turning.
    pub fn fuel_per_minute(&self, accel: f64) -> f64 {
        data::FUEL_PER_THRUST_MINUTE * accel.abs() * self.mass.get()
    }

    /// Whether the ship can turn at all.
    pub fn can_rotate(&self) -> bool {
        self.alpha > 0.0
    }
}

/// Work out how a design flies.
///
/// Called on every change to the ship and nowhere else — see the module note.
pub fn dynamics(design: &ShipDesign, crew_count: u32) -> Result<Dynamics, DynamicsError> {
    let mass = shipdesign::ship_mass(design, crew_count).map_err(DynamicsError::NotAShip)?;

    // The centre of mass is over the **parts**. Cargo and crew are at it by
    // definition, so adding them in would be adding the same point to its own
    // average — a no-op dressed up as arithmetic.
    let mut hull = 0.0;
    let mut moment = DVec2::ZERO;
    for part in &design.parts {
        let m = part_mass(part.kind);
        moment = moment.add(tile_centre(part).scale(m));
        hull += m;
    }
    if hull <= 0.0 {
        // `ship_mass` refuses a hull under `MIN_HULL_MASS`, so this is only
        // reachable if that floor is ever lowered to nothing. Answering with
        // a divide by zero would be a centre of mass of NaN and a ship drawn
        // nowhere at all.
        return Err(DynamicsError::NotAShip(MassError::HullTooLight));
    }
    let centre_of_mass = moment.scale(1.0 / hull);

    let mut inertia = data::INERTIA_FLOOR;
    let mut torque = 0.0;
    for part in &design.parts {
        let arm = tile_centre(part).distance(centre_of_mass);
        inertia += part_mass(part.kind) * arm * arm;
    }
    for part in &design.parts {
        let def = part.kind.def();
        if def.torque_thrust > 0.0 {
            // A thruster sitting exactly on the centre of mass has no lever
            // and does nothing, which falls out of the arithmetic rather than
            // being a special case.
            torque += def.torque_thrust * tile_centre(part).distance(centre_of_mass);
        }
    }

    let engines = shipdesign::mass::engines(design);
    let count = |facing: Facing| {
        design
            .parts
            .iter()
            .filter(|p| p.kind.def().pushes() && p.rotation.facing() == facing)
            .count() as u32
    };

    Ok(Dynamics {
        mass,
        centre_of_mass,
        inertia,
        a_forward: physics::axis_acceleration(&engines, mass, Facing::Forward),
        a_backward: physics::axis_acceleration(&engines, mass, Facing::Backward),
        alpha: torque / inertia,
        forward_engines: count(Facing::Forward),
        backward_engines: count(Facing::Backward),
        has_helm: design.count(PartKind::Helm) > 0,
        has_airlock: shipdesign::port(design).is_some(),
    })
}

/// The middle of a part's footprint, in design world units.
///
/// The average of its tile centres rather than the middle of its bounding
/// box: they are the same for every footprint in the table today, and they
/// stop being the same the first time a part is not a rectangle.
fn tile_centre(part: &shipdesign::PlacedPart) -> DVec2 {
    let tiles = part.tiles();
    let n = tiles.len().max(1) as f64;
    let mut sum = DVec2::ZERO;
    for (x, y) in tiles {
        sum = sum.add(dvec2(
            (x as f64 + 0.5) * TILE as f64,
            (y as f64 + 0.5) * TILE as f64,
        ));
    }
    sum.scale(1.0 / n)
}
