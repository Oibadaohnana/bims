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

/// What a ship can be built from and carry. Six, no production chains, and
/// no recipe **here** — an id and what a unit of it weighs is the whole of
/// what this crate needs. What a given part is made of is
/// `shipdesign::PartDef::recipe`, which is a fact about a part, and this
/// crate deliberately knows nothing about parts. A part weighs its recipe
/// added up out of the masses below, so building one moves mass from the hold
/// into the hull without changing the total.
///
/// The first four are materials and the last two are food. Food is here
/// rather than somewhere of its own because it is **cargo**: it is bought at
/// a station, it is stowed in a cold store, and the engines have to push it
/// like anything else. What makes it food rather than metal is where it is
/// stowed and what a Bim does with it, and neither of those is this crate's
/// business — see `economy::storage`.
///
/// The discriminants are written out because they cross the wasm boundary as
/// numbers one day, and a reordered enum must not silently renumber a save.
/// **0 to 3 are fixed** and a new resource is appended.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum ResourceId {
    Ore = 0,
    Metal = 1,
    Fuel = 2,
    Components = 3,
    Vegetable = 4,
    Tofu = 5,
    /// The rare one. Sold only at mining outposts, mined only from rich
    /// belts, and wanted only by the emitter — so a crew that never fights
    /// never needs any, and a crew that does has a reason to visit a belt.
    Galvum = 6,
    /// The rare tier of component: metal, components and galvum at a
    /// workbench, and what a turret, a shield, a mining laser and a sensor
    /// array are made of. Made, never sold.
    Emitter = 7,
    /// A pressure suit. Worn for a walk outside — mining ore off a belt —
    /// and kept in a suit locker, which is the locker class of storage. A
    /// resource rather than a thing with a state of its own, until a suit
    /// wears out.
    Suit = 8,
    /// A laser handgun: two components and an emitter at the armoury.
    /// What a Bim will carry into a fight. Made, never sold.
    Handgun = 9,
    /// A vest: metal and components at the armoury. Worn into a fight.
    /// Made, never sold.
    Vest = 10,
    /// A medkit: vegetables and a component at the armoury. What treating a
    /// wound will use up.
    Medkit = 11,
}

impl ResourceId {
    /// Every resource, in discriminant order. `ALL[id as usize].id == id`,
    /// which [`ResourceId::def`] relies on and [`defs_are_sound`] checks.
    pub const ALL: [ResourceId; 12] = [
        ResourceId::Ore,
        ResourceId::Metal,
        ResourceId::Fuel,
        ResourceId::Components,
        ResourceId::Vegetable,
        ResourceId::Tofu,
        ResourceId::Galvum,
        ResourceId::Emitter,
        ResourceId::Suit,
        ResourceId::Handgun,
        ResourceId::Vest,
        ResourceId::Medkit,
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
/// lighter than either and components are light and fiddly. A crate of
/// vegetables and a block of tofu are lighter again — a week's meals for two
/// weighs less than one girder, which is the relation that matters.
///
/// The two made things are pinned to their recipes rather than to taste:
/// crafting conserves mass, so an emitter weighs exactly the metal, the two
/// components and the galvum that went into it, and four components weigh
/// one metal. `shipdesign::recipes` is where those recipes live and
/// `every_recipe_conserves_mass` there is what holds this column to them.
pub static RESOURCES: [ResourceDef; 12] = [
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
    ResourceDef {
        id: ResourceId::Vegetable,
        mass_per_unit: 0.5,
    },
    ResourceDef {
        id: ResourceId::Tofu,
        mass_per_unit: 0.5,
    },
    ResourceDef {
        id: ResourceId::Galvum,
        mass_per_unit: 4.0,
    },
    ResourceDef {
        id: ResourceId::Emitter,
        mass_per_unit: 16.0,
    },
    ResourceDef {
        id: ResourceId::Suit,
        mass_per_unit: 6.0,
    },
    // The three the armoury makes weigh their recipes, like the emitter.
    ResourceDef {
        id: ResourceId::Handgun,
        mass_per_unit: 20.0,
    },
    ResourceDef {
        id: ResourceId::Vest,
        mass_per_unit: 36.0,
    },
    ResourceDef {
        id: ResourceId::Medkit,
        mass_per_unit: 3.0,
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
