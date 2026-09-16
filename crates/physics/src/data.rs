//! The numbers, kept apart from the arithmetic that uses them.
//!
//! Everything here is a **placeholder**. Nothing in this file has been
//! balanced against anything; the values exist so the functions in `lib.rs`
//! have something to be exercised with, and so the world generator has a
//! reference ship to measure its layouts against. Expect all of them to move.
//!
//! What must *not* move quietly is the relationship between them. The world
//! generator validates every system layout by asking how many days a trip
//! takes, and that answer is built out of these masses. Changing one changes
//! which layouts are legal, which changes the world a seed produces — so a
//! change here is a `generator_version` bump, not a tweak.

/// What a crew member weighs, engines included in nothing: a body on the ship
/// is mass the engines have to push and that is the whole of its role here.
pub const PLAYER_MASS: f64 = 100.0;

/// The floor under a ship's hull and structure.
///
/// A ship with no mass accelerates infinitely, so the contract refuses one.
/// This is the configurable floor rather than a clamp: a hull below it is a
/// validation error and the caller is told, because a ship that weighs
/// nothing is a bug upstream and quietly rounding it up would hide that.
pub const MIN_HULL_MASS: f64 = 1.0;

/// What a ship can be built from and carry. Four, no production chains, and
/// no recipe anywhere — an id and what a unit of it weighs is the whole of
/// what this step needs.
///
/// The discriminants are written out because they cross the wasm boundary as
/// numbers one day, and a reordered enum must not silently renumber a save.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum ResourceId {
    Ore = 0,
    Metal = 1,
    Fuel = 2,
    Components = 3,
}

impl ResourceId {
    /// Every resource, in discriminant order. `ALL[id as usize].id == id`,
    /// which [`ResourceId::def`] relies on and [`defs_are_sound`] checks.
    pub const ALL: [ResourceId; 4] = [
        ResourceId::Ore,
        ResourceId::Metal,
        ResourceId::Fuel,
        ResourceId::Components,
    ];

    pub fn def(self) -> &'static ResourceDef {
        &RESOURCES[self as usize]
    }

    /// What one unit of it weighs. The one number the mass function wants.
    pub fn mass_per_unit(self) -> f64 {
        self.def().mass_per_unit
    }
}

/// A resource, as data. There is deliberately nothing else on it: no name (no
/// strings cross the wasm boundary — the host holds the words), no recipe, no
/// stack size.
#[derive(Clone, Copy, Debug)]
pub struct ResourceDef {
    pub id: ResourceId,
    /// Strictly greater than zero. A resource that weighs nothing would let a
    /// ship carry an unbounded cargo for free.
    pub mass_per_unit: f64,
}

/// The table. Ore is the raw rock, metal is what it refines to, fuel is
/// lighter than either and components are light and fiddly.
pub static RESOURCES: [ResourceDef; 4] = [
    ResourceDef {
        id: ResourceId::Ore,
        mass_per_unit: 10.0,
    },
    ResourceDef {
        id: ResourceId::Metal,
        mass_per_unit: 8.0,
    },
    ResourceDef {
        id: ResourceId::Fuel,
        mass_per_unit: 5.0,
    },
    ResourceDef {
        id: ResourceId::Components,
        mass_per_unit: 2.0,
    },
];

/// Whether the table above holds together: one entry per id, in order, each
/// weighing something. The unit tests assert it, and so can anything that
/// loads a resource table from somewhere else later.
pub fn defs_are_sound() -> bool {
    RESOURCES.len() == ResourceId::ALL.len()
        && ResourceId::ALL.iter().enumerate().all(|(i, &id)| {
            RESOURCES[i].id == id
                && RESOURCES[i].mass_per_unit > 0.0
                && RESOURCES[i].mass_per_unit.is_finite()
        })
}
