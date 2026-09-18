//! What a ship can be built out of.
//!
//! One table, [`PARTS`], with one entry per [`PartKind`] in discriminant
//! order. Every number in it is a **placeholder** — nothing here has been
//! balanced against anything, and the recipes and prices exist so that the
//! rules in [`crate::design`] and [`crate::validate`] have something real to
//! be exercised against.
//!
//! What is *not* a placeholder is the shape of the table. The discriminants
//! cross the wasm boundary as numbers and will one day be in a save file, so
//! they are written out and **never renumbered**: a new part is appended, a
//! retired one leaves a hole.
//!
//! # A part is made of something, and weighs what it is made of
//!
//! [`PartDef::recipe`] is the materials one is built from — metal and
//! components, never ore and never food — and [`part_mass`] is that recipe
//! added up. There is deliberately **no mass column**: a part that weighed
//! something other than its materials would gain or lose mass every time one
//! was built, and the whole of [`crate::materials`] is the promise that it
//! does not.
//!
//! [`PartDef::price`] is the other half and it is **independent**. It is what
//! a finished part costs in euros at a station, where money and materials can
//! be swapped for each other; away from one there is no price, only the
//! recipe. Nothing works one out from the other, and nothing should — an
//! instant part bought at the dock and a part welded up out of the hold are
//! two different transactions that happen to end in the same wall.
//!
//! # Four layers, and what holds what up
//!
//! A tile holds at most one part per [`Layer`], and a part can say what has
//! to be there already through [`PartDef::requires`]:
//!
//! - **Structure** is the frame the ship is built on. It needs nothing under
//!   it and everything else is over it, directly or through the deck.
//! - **Floor** is the deck plating you walk on. It needs structure.
//! - **Object** is the one thing standing in the tile — a wall, a bunk, an
//!   engine. Most need deck; a wall, an outside wall, a sensor array and a
//!   thruster are hull and stand straight on structure.
//! - **Utility** is what runs *through* a tile without filling it: conduit.
//!   It needs structure and nothing stands on it.
//!
//! # Shielding
//!
//! [`PartDef::shields`] is what keeps the radiation out — see
//! [`crate::validate::exposure`]. It is a property of the **part**, not of
//! the hull: an outside wall shields, a plain internal wall does not, and a
//! door does not, so a ship walled in ordinary walls is a ship the crew are
//! being cooked in.

use economy::{Money, Storage};
use physics::ResourceId;

/// World units to a tile side. Fixed, and the one place it is written down.
///
/// The room's own grid is nothing to do with this — that is a 10-unit
/// navigation grid over hand-placed furniture. A ship is tiles.
pub const TILE: u32 = 52;

/// What one reactor makes, in units a minute. Placeholder, pinned to one
/// outcome: the playtest ship's nine consumers — six systems, the smelter,
/// the workbench and the drug lab — draw a hundred and seventeen between
/// them, so one reactor runs the ship as it comes and a fourth bench is a
/// second reactor or a brownout.
pub const REACTOR_OUTPUT: f64 = 120.0;

/// What one battery holds: half an hour of a reactor's output. Enough to
/// ride out a reactor going down between two stations and not enough to
/// run a ship on.
pub const BATTERY_CHARGE: f64 = 30.0 * REACTOR_OUTPUT;

/// Which of a tile's four slots a part sits in.
///
/// A tile holds at most one of each. Walls are `Object` rather than a layer
/// of their own, because a wall and a bunk are equally "the thing standing in
/// this tile" and two of them in one tile is equally nonsense.
///
/// The discriminants cross the wasm boundary and **0 and 1 are fixed** — they
/// were the whole of the enum before there was structure under the deck.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum Layer {
    /// The deck plating. Only [`PartKind::Floor`].
    Floor = 0,
    /// Everything that stands on it, walls included.
    Object = 1,
    /// The frame the whole ship is built on. Under everything, holds nothing
    /// up on its own, and the layer [`crate::validate`] asks about when it
    /// wants to know whether a ship is in one piece.
    Structure = 2,
    /// What runs *through* a tile rather than filling it: conduit. Something
    /// can stand on the same tile, and a body can walk over it.
    Utility = 3,
}

impl Layer {
    /// Every layer, in discriminant order. `ALL[l as usize] == l`, which the
    /// occupancy grid relies on.
    pub const ALL: [Layer; 4] = [
        Layer::Floor,
        Layer::Object,
        Layer::Structure,
        Layer::Utility,
    ];

    pub fn code(self) -> u32 {
        self as u32
    }

    pub fn from_code(code: u32) -> Option<Layer> {
        Layer::ALL.get(code as usize).copied()
    }
}

/// Everything that can be placed.
///
/// The discriminants are explicit and permanent — see the module note. The
/// order is also the order of [`PARTS`] and of the palette the host draws.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum PartKind {
    Floor = 0,
    Wall = 1,
    Door = 2,
    Engine = 3,
    Bunk = 4,
    ColdStore = 5,
    Worktop = 6,
    Hob = 7,
    Dishwasher = 8,
    Table = 9,
    Chair = 10,
    Toilet = 11,
    Basin = 12,
    HydroBay = 13,
    BroomLocker = 14,
    /// The frame. Everything else is built on top of it.
    Structure = 15,
    /// Hull plating. A wall that keeps the radiation out.
    OutsideWall = 16,
    Helm = 17,
    Reactor = 18,
    PowerConduit = 19,
    Battery = 20,
    FuelTank = 21,
    LifeSupport = 22,
    Airlock = 23,
    SensorArray = 24,
    Shelf = 25,
    Shower = 26,
    /// A manoeuvring thruster. What turns the ship, and the only thing that
    /// does — the main engines push through the centre of mass and never spin
    /// it.
    Thruster = 27,
    /// The big main engine: five times the push of an [`PartKind::Engine`]
    /// for three and a half times the weight, and a fuel bill to match. What
    /// moves a heavy ship in reasonable time; on a light one it is mostly
    /// engine.
    HeavyEngine = 28,
    /// A plain wall cut across the corner of its tile: a right-angled
    /// triangle filling the half of the tile its [`Rotation`] names — see
    /// [`solid_corner`]. What lets a bulkhead turn a corner at forty-five
    /// degrees, the way the hull pieces of every spacecraft in every game of
    /// this kind do. It fills the whole tile as far as the rules are
    /// concerned: one object a tile, a body cannot pass, and nothing else
    /// stands there. Only the picture is a triangle.
    DiagonalWall = 29,
    /// The same cut across the hull: an [`PartKind::OutsideWall`] as a
    /// triangle, so a ship can have a pointed bow and chamfered corners and
    /// still keep the radiation out. It seals its whole tile — the
    /// exposure fill is four-neighbour, and a staircase of these touching
    /// corner to corner is as tight as a straight run.
    DiagonalOutsideWall = 30,
    /// Two ore in, one metal out, the slag vented. The first workstation,
    /// and the one that draws most — see [`crate::recipes`].
    Smelter = 31,
    /// Metal into components, and metal, components and galvum into an
    /// emitter. See [`crate::recipes`].
    Workbench = 32,
    /// Where the pressure suits hang: the locker class of storage, two of
    /// them. Where a walk outside starts and ends.
    SuitLocker = 33,
    /// A bench and a locker in one: where handguns, vests and medkits are
    /// made — [`crate::recipes`] — and where they are kept, four of them.
    Armoury = 34,
    /// The bench where fibre is rolled into bandages — [`crate::recipes`].
    /// The workbench's size and its habits: worked from the tile below,
    /// draws, and a body sees over nothing of it.
    DrugLab = 35,
}

impl PartKind {
    /// Every kind, in discriminant order. `ALL[k as usize] == k`, which
    /// [`PartKind::def`] relies on and [`defs_are_sound`] checks.
    pub const ALL: [PartKind; 36] = [
        PartKind::Floor,
        PartKind::Wall,
        PartKind::Door,
        PartKind::Engine,
        PartKind::Bunk,
        PartKind::ColdStore,
        PartKind::Worktop,
        PartKind::Hob,
        PartKind::Dishwasher,
        PartKind::Table,
        PartKind::Chair,
        PartKind::Toilet,
        PartKind::Basin,
        PartKind::HydroBay,
        PartKind::BroomLocker,
        PartKind::Structure,
        PartKind::OutsideWall,
        PartKind::Helm,
        PartKind::Reactor,
        PartKind::PowerConduit,
        PartKind::Battery,
        PartKind::FuelTank,
        PartKind::LifeSupport,
        PartKind::Airlock,
        PartKind::SensorArray,
        PartKind::Shelf,
        PartKind::Shower,
        PartKind::Thruster,
        PartKind::HeavyEngine,
        PartKind::DiagonalWall,
        PartKind::DiagonalOutsideWall,
        PartKind::Smelter,
        PartKind::Workbench,
        PartKind::SuitLocker,
        PartKind::Armoury,
        PartKind::DrugLab,
    ];

    /// The number that crosses the wasm boundary. No strings do.
    pub fn code(self) -> u32 {
        self as u32
    }

    /// Back from that number. `None` for anything that is not a kind, which
    /// is what a host sending nonsense looks like.
    pub fn from_code(code: u32) -> Option<PartKind> {
        PartKind::ALL.get(code as usize).copied()
    }

    pub fn def(self) -> &'static PartDef {
        &PARTS[self as usize]
    }
}

/// A quarter turn. Clockwise, because the grid's `y` grows downwards and
/// clockwise is what that makes of "the next one round".
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum Rotation {
    R0 = 0,
    R90 = 1,
    R180 = 2,
    R270 = 3,
}

impl Rotation {
    pub const ALL: [Rotation; 4] = [Rotation::R0, Rotation::R90, Rotation::R180, Rotation::R270];

    pub fn code(self) -> u32 {
        self as u32
    }

    pub fn from_code(code: u32) -> Option<Rotation> {
        Rotation::ALL.get(code as usize).copied()
    }

    /// The next quarter turn clockwise. What the `R` key does to the ghost.
    pub fn next(self) -> Rotation {
        Rotation::ALL[((self as usize) + 1) % 4]
    }

    /// Which way an engine at this rotation pushes the ship.
    ///
    /// Grid "up" — a part at [`Rotation::R0`] — is
    /// [`physics::Facing::Forward`], and the rest follow it round clockwise.
    /// **This is no longer arbitrary.** The flight step decided it: the ship's
    /// Forward *is* the design grid's up, so at heading 0 the design is drawn
    /// on screen exactly as it was laid out, and a ship that turns to a
    /// heading of π/2 has the top of its grid pointing east. Changing this
    /// mapping now would turn every ship anybody has drawn through a quarter
    /// circle.
    ///
    /// Only `Forward` and `Backward` are flown: the autopilot burns along the
    /// start–arrival line and turns with thrusters, so an engine bolted
    /// sideways contributes nothing but its weight. That is a warning on the
    /// design rather than a refusal — see [`crate::validate`].
    pub fn facing(self) -> physics::Facing {
        match self {
            Rotation::R0 => physics::Facing::Forward,
            Rotation::R90 => physics::Facing::Right,
            Rotation::R180 => physics::Facing::Backward,
            Rotation::R270 => physics::Facing::Left,
        }
    }
}

/// One part, as data.
///
/// No name: no strings cross the wasm boundary, so `PART_NAMES` in
/// `crates/app/src/names.rs` is where the words live and this is only the numbers.
#[derive(Clone, Copy, Debug)]
pub struct PartDef {
    pub kind: PartKind,
    /// Tiles across and down, **unrotated**. [`footprint`] turns this and a
    /// [`Rotation`] into the tiles actually covered.
    pub footprint: (u32, u32),
    pub layer: Layer,
    /// Whether a body may pass through the tile. A door does not block; nor
    /// does a chair, which is a seat rather than an obstacle and has to be
    /// stood on to be used; nor does an airlock, which is a door with a
    /// hull rating.
    pub blocks_movement: bool,
    /// What every tile of the footprint has to hold already, if anything.
    /// Deck for most things, structure for the frame's own plating and for
    /// the hull parts that stand straight on it, and `None` for structure
    /// itself, which is what everything else is built on.
    pub requires: Option<Layer>,
    /// Tile offsets from the origin, **unrotated**, where a Bim stands to use
    /// the part. They rotate with it. Offsets are signed because most of them
    /// are outside the footprint — you stand *beside* a cold store.
    pub use_spots: &'static [(i32, i32)],
    /// What one costs, in whole euros out of the crew's shared pool.
    /// Strictly greater than zero: a part that is free is a part the pool has
    /// no opinion about, and the whole of the design phase is the pool having
    /// an opinion.
    pub price: Money,
    /// Whether the part keeps radiation out — see
    /// [`crate::validate::exposure`]. Hull, essentially: the outside wall,
    /// the airlock, the sensor array, the thruster and the engine block. A
    /// plain internal wall does not, and neither does a door.
    pub shields: bool,
    /// What this part holds, if it holds anything: a class of storage and how
    /// many units of it. `None` for everything that is not a container.
    pub capacity: Option<(Storage, u32)>,
    /// What the part is **made of**: units of each material, and nothing
    /// else. Never empty, and only [`ResourceId::Metal`],
    /// [`ResourceId::Components`] and [`ResourceId::Emitter`] — ore and
    /// galvum are what those are made from, fuel is burnt and the food is
    /// eaten, so none of them belongs in a wall.
    ///
    /// There is no separate mass. [`part_mass`] adds the recipe up, so a
    /// part weighs exactly what went into it and building one moves mass
    /// from the hold into the hull without changing the total — see the
    /// contract in [`crate::materials`].
    pub recipe: &'static [(ResourceId, u32)],
    /// Greater than zero for the main engines — [`PartKind::Engine`] and
    /// [`PartKind::HeavyEngine`] — and nothing else. [`PartDef::pushes`] is
    /// the question to ask; nothing should list the kinds.
    ///
    /// A main engine's push goes through the ship's **centre of mass**
    /// whatever tile it is bolted to, so it produces no torque. That is a
    /// simplification and a deliberate one: engines placed off the centreline
    /// would otherwise spin a ship that a player laid out symmetrically to the
    /// eye and not to the gram, and there is nothing they could do about it.
    pub thrust: f64,
    /// Greater than zero for [`PartKind::Thruster`] and nothing else;
    /// [`PartDef::turns`] asks it.
    ///
    /// Force, not torque: what it becomes depends on how far the thruster is
    /// from the centre of mass, which is `flight::dynamics`'s arithmetic and
    /// not a fact about the part. A thruster fires **either way round**, so
    /// one of them turns the ship in both directions and four of them turn it
    /// faster; there is no left thruster and no right one.
    pub torque_thrust: f64,
    /// Power, in units a minute: **positive** on the reactor, which makes
    /// it, **negative** on what draws it, and nought on everything else.
    /// [`PartDef::supplies`] and [`PartDef::draws`] are the questions;
    /// nothing should list the kinds. A part is powered when a tile of it
    /// carries conduit on a network with a reactor — see [`crate::power`].
    ///
    /// A consumer's draw is constant whether or not anybody is using it.
    /// Placeholder, like the rest: a hob that only drew while lit is a
    /// draw the world would have to ask the room about every step.
    pub power: f64,
    /// What the part can hold in power, in units — a minute of a one-unit
    /// draw is one unit. Greater than nought on the battery and nothing
    /// else; [`PartDef::stores`] asks it.
    pub charge: f64,
}

impl PartDef {
    /// Whether this makes power: the reactor. The play phase's brownout
    /// rule and the validator's network both ask this.
    pub fn supplies(&self) -> bool {
        self.power > 0.0
    }

    /// Whether this draws power, and so is something that can be
    /// unpowered.
    pub fn draws(&self) -> bool {
        self.power < 0.0
    }

    /// Whether this holds power: the battery.
    pub fn stores(&self) -> bool {
        self.charge > 0.0
    }

    /// Whether this is a main engine — something the autopilot burns along
    /// the start–arrival line. There are two sizes of them, and everything
    /// that wants "the engines" — the validator, the dynamics, the fuel
    /// bill, the painter — asks this rather than naming either kind, so a
    /// third size is one row in the table.
    pub fn pushes(&self) -> bool {
        self.thrust > 0.0
    }

    /// Whether this is a manoeuvring thruster. The other half of the
    /// exclusive pair; see [`PartDef::torque_thrust`].
    pub fn turns(&self) -> bool {
        self.torque_thrust > 0.0
    }

    /// Whether a line of sight stops at this part.
    ///
    /// The whole of the rule, and the only place it is written down.
    /// Everything a body cannot walk through stands in the way of its eyes
    /// too — a wall, a shelf, a reactor, a tank, the engines — except the
    /// **low** furniture a Bim sees over: a bunk, the worktop, the hob, a
    /// dishwasher, a table, a toilet, a basin, a tray of crops, the helm's
    /// console, a battery on the deck. A door is a wall while its leaves
    /// are shut and nothing while they are open, and which it is at the
    /// moment is the room's to know; this says what it is shut. Nothing
    /// that is walked over blocks sight.
    pub fn blocks_sight(&self) -> bool {
        match self.kind {
            PartKind::Door => true,
            PartKind::Bunk
            | PartKind::Worktop
            | PartKind::Hob
            | PartKind::Dishwasher
            | PartKind::Table
            | PartKind::Toilet
            | PartKind::Basin
            | PartKind::HydroBay
            | PartKind::Helm
            | PartKind::Battery => false,
            _ => self.blocks_movement,
        }
    }
}

/// The table. Placeholder numbers throughout; see the module note.
///
/// Read the `use_spots` column as the whole of what the play phase will be
/// told about how a part is approached — [`crate::validate`] already insists
/// every one of them is floor a body can stand on and that they can all reach
/// each other, so a design that passes here is one the crew can work.
pub static PARTS: [PartDef; 36] = [
    PartDef {
        kind: PartKind::Floor,
        footprint: (1, 1),
        layer: Layer::Floor,
        blocks_movement: false,
        requires: Some(Layer::Structure),
        use_spots: &[],
        price: 50,
        shields: false,
        capacity: None,
        recipe: &[(ResourceId::Metal, 1)],
        thrust: 0.0,
        torque_thrust: 0.0,
        power: 0.0,
        charge: 0.0,
    },
    PartDef {
        kind: PartKind::Wall,
        footprint: (1, 1),
        layer: Layer::Object,
        blocks_movement: true,
        // A wall is hull. It is the one thing on the object layer that can
        // stand where there is no deck, which is what lets a ship be walled
        // before it is floored.
        requires: Some(Layer::Structure),
        use_spots: &[],
        price: 100,
        shields: false,
        capacity: None,
        recipe: &[(ResourceId::Metal, 2)],
        thrust: 0.0,
        torque_thrust: 0.0,
        power: 0.0,
        charge: 0.0,
    },
    PartDef {
        kind: PartKind::Door,
        // Two tiles along the bulkhead, one deep, like the airlock: a
        // doorway the room's navigation can walk (a one-tile gap it cannot;
        // see the crate's module note), and one part for it rather than two
        // doors side by side. Which way it runs is its rotation — `R0`
        // stands in a bulkhead running north–south, `R90` in one running
        // east–west — and nothing reads the neighbours to guess.
        footprint: (1, 2),
        layer: Layer::Object,
        blocks_movement: false,
        requires: Some(Layer::Floor),
        // Nobody *uses* a door; they walk through it. It earns its keep in
        // the reachability check, which lets a route pass through one.
        use_spots: &[],
        price: 400,
        shields: false,
        capacity: None,
        recipe: &[(ResourceId::Metal, 2), (ResourceId::Components, 2)],
        thrust: 0.0,
        torque_thrust: 0.0,
        power: -1.0,
        charge: 0.0,
    },
    PartDef {
        kind: PartKind::Engine,
        footprint: (2, 3),
        layer: Layer::Object,
        blocks_movement: true,
        requires: Some(Layer::Floor),
        // Beside the middle of the left-hand side. Asymmetric on purpose:
        // it is the part the rotation test pins by hand.
        use_spots: &[(-1, 1)],
        price: 20_000,
        shields: true,
        capacity: None,
        recipe: &[(ResourceId::Metal, 40), (ResourceId::Components, 40)],
        thrust: 2_000.0,
        torque_thrust: 0.0,
        power: 0.0,
        charge: 0.0,
    },
    PartDef {
        kind: PartKind::Bunk,
        footprint: (1, 2),
        layer: Layer::Object,
        blocks_movement: true,
        requires: Some(Layer::Floor),
        use_spots: &[(-1, 0)],
        price: 800,
        shields: false,
        capacity: None,
        recipe: &[(ResourceId::Metal, 3), (ResourceId::Components, 2)],
        thrust: 0.0,
        torque_thrust: 0.0,
        power: 0.0,
        charge: 0.0,
    },
    PartDef {
        kind: PartKind::ColdStore,
        footprint: (1, 1),
        layer: Layer::Object,
        blocks_movement: true,
        requires: Some(Layer::Floor),
        use_spots: &[(0, 1)],
        price: 1_500,
        shields: false,
        capacity: Some((Storage::ColdStore, 100)),
        recipe: &[(ResourceId::Metal, 6), (ResourceId::Components, 6)],
        thrust: 0.0,
        torque_thrust: 0.0,
        power: -5.0,
        charge: 0.0,
    },
    PartDef {
        kind: PartKind::Worktop,
        footprint: (2, 1),
        layer: Layer::Object,
        blocks_movement: true,
        requires: Some(Layer::Floor),
        use_spots: &[(0, 1), (1, 1)],
        price: 600,
        shields: false,
        capacity: None,
        recipe: &[(ResourceId::Metal, 3)],
        thrust: 0.0,
        torque_thrust: 0.0,
        power: 0.0,
        charge: 0.0,
    },
    PartDef {
        kind: PartKind::Hob,
        footprint: (1, 1),
        layer: Layer::Object,
        blocks_movement: true,
        requires: Some(Layer::Floor),
        use_spots: &[(0, 1)],
        price: 1_200,
        shields: false,
        capacity: None,
        recipe: &[(ResourceId::Metal, 3), (ResourceId::Components, 3)],
        thrust: 0.0,
        torque_thrust: 0.0,
        power: 0.0,
        charge: 0.0,
    },
    PartDef {
        kind: PartKind::Dishwasher,
        footprint: (1, 1),
        layer: Layer::Object,
        blocks_movement: true,
        requires: Some(Layer::Floor),
        use_spots: &[(0, 1)],
        price: 900,
        shields: false,
        capacity: None,
        recipe: &[(ResourceId::Metal, 5), (ResourceId::Components, 3)],
        thrust: 0.0,
        torque_thrust: 0.0,
        power: 0.0,
        charge: 0.0,
    },
    PartDef {
        kind: PartKind::Table,
        footprint: (2, 1),
        layer: Layer::Object,
        blocks_movement: true,
        requires: Some(Layer::Floor),
        // One side, not both. A table with four use spots makes every
        // fixture design need a gangway round it, which is a rule nobody
        // asked for.
        use_spots: &[(0, 1), (1, 1)],
        price: 400,
        shields: false,
        capacity: None,
        recipe: &[(ResourceId::Metal, 2)],
        thrust: 0.0,
        torque_thrust: 0.0,
        power: 0.0,
        charge: 0.0,
    },
    PartDef {
        kind: PartKind::Chair,
        footprint: (1, 1),
        layer: Layer::Object,
        // A seat is stood on, not walked round. Its own tile is its use
        // spot, and a blocking part whose use spot is itself could never
        // validate.
        blocks_movement: false,
        requires: Some(Layer::Floor),
        use_spots: &[(0, 0)],
        price: 150,
        shields: false,
        capacity: None,
        recipe: &[(ResourceId::Metal, 1)],
        thrust: 0.0,
        torque_thrust: 0.0,
        power: 0.0,
        charge: 0.0,
    },
    PartDef {
        kind: PartKind::Toilet,
        footprint: (1, 1),
        layer: Layer::Object,
        blocks_movement: true,
        requires: Some(Layer::Floor),
        use_spots: &[(0, 1)],
        price: 1_000,
        shields: false,
        capacity: None,
        recipe: &[(ResourceId::Metal, 3), (ResourceId::Components, 1)],
        thrust: 0.0,
        torque_thrust: 0.0,
        power: 0.0,
        charge: 0.0,
    },
    PartDef {
        kind: PartKind::Basin,
        footprint: (1, 1),
        layer: Layer::Object,
        blocks_movement: true,
        requires: Some(Layer::Floor),
        use_spots: &[(0, 1)],
        price: 500,
        shields: false,
        capacity: None,
        recipe: &[(ResourceId::Metal, 2)],
        thrust: 0.0,
        torque_thrust: 0.0,
        power: 0.0,
        charge: 0.0,
    },
    PartDef {
        kind: PartKind::HydroBay,
        // A run of six trays, one a tile, worked from the row above them —
        // the room's bay has six trays and stands the Bim along one side,
        // and each tile of this is one of them.
        footprint: (6, 1),
        layer: Layer::Object,
        blocks_movement: true,
        requires: Some(Layer::Floor),
        use_spots: &[(0, -1), (1, -1), (2, -1), (3, -1), (4, -1), (5, -1)],
        price: 4_000,
        shields: false,
        capacity: None,
        recipe: &[(ResourceId::Metal, 12), (ResourceId::Components, 12)],
        thrust: 0.0,
        torque_thrust: 0.0,
        power: -15.0,
        charge: 0.0,
    },
    PartDef {
        kind: PartKind::BroomLocker,
        footprint: (1, 1),
        layer: Layer::Object,
        blocks_movement: true,
        requires: Some(Layer::Floor),
        use_spots: &[(0, 1)],
        price: 150,
        shields: false,
        capacity: None,
        recipe: &[(ResourceId::Metal, 1), (ResourceId::Components, 1)],
        thrust: 0.0,
        torque_thrust: 0.0,
        power: 0.0,
        charge: 0.0,
    },
    // --- the frame, and the hull on it ------------------------------------
    PartDef {
        kind: PartKind::Structure,
        footprint: (1, 1),
        layer: Layer::Structure,
        // Nothing stands on structure: it is under everything, and a body
        // walks over the deck laid on it rather than over the frame.
        blocks_movement: false,
        // The only part that needs nothing. Everything else is over it.
        requires: None,
        use_spots: &[],
        price: 50,
        shields: false,
        capacity: None,
        recipe: &[(ResourceId::Metal, 2)],
        thrust: 0.0,
        torque_thrust: 0.0,
        power: 0.0,
        charge: 0.0,
    },
    PartDef {
        kind: PartKind::OutsideWall,
        footprint: (1, 1),
        layer: Layer::Object,
        blocks_movement: true,
        // Hull, like a plain wall: it stands on the frame with no deck
        // needed, which is what lets a ship be skinned before it is floored.
        requires: Some(Layer::Structure),
        use_spots: &[],
        price: 200,
        shields: true,
        capacity: None,
        recipe: &[(ResourceId::Metal, 4), (ResourceId::Components, 1)],
        thrust: 0.0,
        torque_thrust: 0.0,
        power: 0.0,
        charge: 0.0,
    },
    // --- systems -----------------------------------------------------------
    PartDef {
        kind: PartKind::Helm,
        footprint: (2, 1),
        layer: Layer::Object,
        blocks_movement: true,
        requires: Some(Layer::Floor),
        // One seat at it. A second would mean two Bims flying one ship,
        // which is a decision the play phase has not taken.
        use_spots: &[(0, 1)],
        price: 5_000,
        shields: false,
        capacity: None,
        recipe: &[(ResourceId::Metal, 4), (ResourceId::Components, 20)],
        thrust: 0.0,
        torque_thrust: 0.0,
        power: -5.0,
        charge: 0.0,
    },
    PartDef {
        kind: PartKind::Reactor,
        footprint: (2, 2),
        layer: Layer::Object,
        blocks_movement: true,
        requires: Some(Layer::Floor),
        // Nobody works a reactor by hand: it makes power the moment it is
        // on a network — `crate::power` — and no chain walks to one, so it
        // has nowhere to stand and wants none.
        use_spots: &[],
        price: 12_000,
        shields: false,
        capacity: None,
        recipe: &[(ResourceId::Metal, 30), (ResourceId::Components, 30)],
        thrust: 0.0,
        torque_thrust: 0.0,
        power: REACTOR_OUTPUT,
        charge: 0.0,
    },
    PartDef {
        kind: PartKind::PowerConduit,
        footprint: (1, 1),
        layer: Layer::Utility,
        blocks_movement: false,
        requires: Some(Layer::Structure),
        use_spots: &[],
        price: 20,
        shields: false,
        capacity: None,
        recipe: &[(ResourceId::Metal, 1)],
        thrust: 0.0,
        torque_thrust: 0.0,
        power: 0.0,
        charge: 0.0,
    },
    PartDef {
        kind: PartKind::Battery,
        footprint: (1, 1),
        layer: Layer::Object,
        blocks_movement: true,
        requires: Some(Layer::Floor),
        use_spots: &[],
        price: 3_000,
        shields: false,
        capacity: None,
        recipe: &[(ResourceId::Metal, 4), (ResourceId::Components, 10)],
        thrust: 0.0,
        torque_thrust: 0.0,
        power: 0.0,
        charge: BATTERY_CHARGE,
    },
    PartDef {
        kind: PartKind::FuelTank,
        footprint: (2, 2),
        layer: Layer::Object,
        blocks_movement: true,
        requires: Some(Layer::Floor),
        // Somewhere to stand to fill it, below the left-hand column, the
        // same way the bay is approached.
        use_spots: &[(0, 2)],
        price: 4_000,
        shields: false,
        capacity: Some((Storage::FuelTank, 200)),
        recipe: &[(ResourceId::Metal, 10)],
        thrust: 0.0,
        torque_thrust: 0.0,
        power: 0.0,
        charge: 0.0,
    },
    PartDef {
        kind: PartKind::LifeSupport,
        footprint: (2, 2),
        layer: Layer::Object,
        blocks_movement: true,
        requires: Some(Layer::Floor),
        use_spots: &[],
        price: 6_000,
        shields: false,
        capacity: None,
        recipe: &[(ResourceId::Metal, 8), (ResourceId::Components, 12)],
        thrust: 0.0,
        torque_thrust: 0.0,
        power: -20.0,
        charge: 0.0,
    },
    PartDef {
        kind: PartKind::Airlock,
        footprint: (1, 2),
        layer: Layer::Object,
        // A way out, so a way through: it is a door with a hull rating, and
        // the reachability check walks it like one.
        blocks_movement: false,
        requires: Some(Layer::Floor),
        use_spots: &[],
        price: 3_000,
        shields: true,
        capacity: None,
        recipe: &[(ResourceId::Metal, 8), (ResourceId::Components, 6)],
        thrust: 0.0,
        torque_thrust: 0.0,
        power: 0.0,
        charge: 0.0,
    },
    PartDef {
        kind: PartKind::SensorArray,
        footprint: (1, 1),
        layer: Layer::Object,
        blocks_movement: true,
        // Bolted to the frame on the outside, like the hull it sits in.
        requires: Some(Layer::Structure),
        use_spots: &[],
        price: 4_000,
        shields: true,
        capacity: None,
        recipe: &[(ResourceId::Metal, 3), (ResourceId::Components, 12)],
        thrust: 0.0,
        torque_thrust: 0.0,
        power: -10.0,
        charge: 0.0,
    },
    // --- crew ---------------------------------------------------------------
    PartDef {
        kind: PartKind::Shelf,
        footprint: (1, 1),
        layer: Layer::Object,
        blocks_movement: true,
        requires: Some(Layer::Floor),
        use_spots: &[(0, 1)],
        price: 300,
        shields: false,
        capacity: Some((Storage::Shelf, 100)),
        recipe: &[(ResourceId::Metal, 2)],
        thrust: 0.0,
        torque_thrust: 0.0,
        power: 0.0,
        charge: 0.0,
    },
    PartDef {
        kind: PartKind::Shower,
        footprint: (1, 1),
        layer: Layer::Object,
        blocks_movement: true,
        requires: Some(Layer::Floor),
        use_spots: &[(0, 1)],
        price: 1_200,
        shields: false,
        capacity: None,
        recipe: &[(ResourceId::Metal, 3), (ResourceId::Components, 2)],
        thrust: 0.0,
        torque_thrust: 0.0,
        power: 0.0,
        charge: 0.0,
    },
    PartDef {
        kind: PartKind::Thruster,
        footprint: (1, 1),
        layer: Layer::Object,
        blocks_movement: true,
        // Bolted to the frame on the outside, like the hull and the sensor
        // array. A thruster inside the ship would be pushing against its own
        // hull — and standing it on the frame is also what puts it out at the
        // edges, which is where a lever arm comes from.
        requires: Some(Layer::Structure),
        use_spots: &[],
        price: 2_500,
        // It is hull, so it keeps the radiation out like the rest of the
        // skin. A ring of thrusters with gaps between them is still gaps.
        shields: true,
        capacity: None,
        recipe: &[(ResourceId::Metal, 4), (ResourceId::Components, 6)],
        thrust: 0.0,
        // Placeholder, and picked against a scenario rather than out of the
        // air: four of these on the flyable fixture's hull turn it through
        // half a circle in about a hundred and ten game minutes, inside the
        // two hours the flight step asks for.
        // `flight`'s `four_thrusters_flip_the_reference_inside_two_hours` is
        // what pins it, and `what_the_fixture_actually_flies_like` beside it
        // prints the numbers for whoever has to move this next.
        torque_thrust: 1_000.0,
        power: 0.0,
        charge: 0.0,
    },
    PartDef {
        kind: PartKind::HeavyEngine,
        footprint: (3, 4),
        layer: Layer::Object,
        blocks_movement: true,
        requires: Some(Layer::Floor),
        // Beside the middle of the left-hand side, as the small one is.
        use_spots: &[(-1, 1)],
        price: 75_000,
        shields: true,
        capacity: None,
        // Three and a half times an `Engine` for five times the push, so it
        // is the better engine *per tonne* — which is the point of it: a
        // ship heavy enough to want one has hull and cargo to move, and a
        // small engine on a big hull crawls. The fuel bill is per unit of
        // thrust (`flight::data::FUEL_PER_THRUST_MINUTE`), so it also burns
        // five times as much a minute, and a fast trip is a dear one.
        recipe: &[(ResourceId::Metal, 150), (ResourceId::Components, 100)],
        thrust: 10_000.0,
        torque_thrust: 0.0,
        power: 0.0,
        charge: 0.0,
    },
    // --- the corners ---------------------------------------------------------
    PartDef {
        kind: PartKind::DiagonalWall,
        footprint: (1, 1),
        layer: Layer::Object,
        blocks_movement: true,
        // Hull like the straight wall: it stands on the frame with no deck
        // needed, so a bulkhead can be drawn before the deck is laid.
        requires: Some(Layer::Structure),
        use_spots: &[],
        price: 100,
        shields: false,
        capacity: None,
        recipe: &[(ResourceId::Metal, 2)],
        thrust: 0.0,
        torque_thrust: 0.0,
        power: 0.0,
        charge: 0.0,
    },
    PartDef {
        kind: PartKind::DiagonalOutsideWall,
        footprint: (1, 1),
        layer: Layer::Object,
        blocks_movement: true,
        requires: Some(Layer::Structure),
        use_spots: &[],
        price: 200,
        shields: true,
        capacity: None,
        recipe: &[(ResourceId::Metal, 4), (ResourceId::Components, 1)],
        thrust: 0.0,
        torque_thrust: 0.0,
        power: 0.0,
        charge: 0.0,
    },
    // --- workstations -------------------------------------------------------
    //
    // Worked from the tile below, like the galley, and each is a bench the
    // room's craft chain stands a Bim at — `bims::room::Bench`. Both draw,
    // so both want conduit under them, and both stop in a brownout.
    PartDef {
        kind: PartKind::Smelter,
        footprint: (2, 2),
        layer: Layer::Object,
        blocks_movement: true,
        requires: Some(Layer::Floor),
        use_spots: &[(0, 2)],
        price: 8_000,
        shields: false,
        capacity: None,
        recipe: &[(ResourceId::Metal, 20), (ResourceId::Components, 10)],
        thrust: 0.0,
        torque_thrust: 0.0,
        power: -40.0,
        charge: 0.0,
    },
    PartDef {
        kind: PartKind::Workbench,
        footprint: (2, 1),
        layer: Layer::Object,
        blocks_movement: true,
        requires: Some(Layer::Floor),
        use_spots: &[(0, 1)],
        price: 3_000,
        shields: false,
        capacity: None,
        recipe: &[(ResourceId::Metal, 6), (ResourceId::Components, 4)],
        thrust: 0.0,
        torque_thrust: 0.0,
        power: -15.0,
        charge: 0.0,
    },
    PartDef {
        kind: PartKind::SuitLocker,
        footprint: (1, 1),
        layer: Layer::Object,
        blocks_movement: true,
        requires: Some(Layer::Floor),
        use_spots: &[(0, 1)],
        price: 1_500,
        shields: false,
        capacity: Some((Storage::Locker, 2)),
        recipe: &[(ResourceId::Metal, 3), (ResourceId::Components, 2)],
        thrust: 0.0,
        torque_thrust: 0.0,
        power: 0.0,
        charge: 0.0,
    },
    PartDef {
        kind: PartKind::Armoury,
        footprint: (2, 1),
        layer: Layer::Object,
        blocks_movement: true,
        requires: Some(Layer::Floor),
        use_spots: &[(0, 1)],
        price: 6_000,
        shields: false,
        capacity: Some((Storage::Locker, 4)),
        recipe: &[(ResourceId::Metal, 10), (ResourceId::Components, 8)],
        thrust: 0.0,
        torque_thrust: 0.0,
        power: -10.0,
        charge: 0.0,
    },
    // The drug lab: the workbench's footprint and use spot, a lighter draw
    // — a press and a steriliser rather than a lathe — and, like the
    // armoury, a cabinet of its own for what it makes: locker class, since
    // that is where a bandage is kept, and a bigger one than the armoury's
    // because a dressing is small.
    PartDef {
        kind: PartKind::DrugLab,
        footprint: (2, 1),
        layer: Layer::Object,
        blocks_movement: true,
        requires: Some(Layer::Floor),
        use_spots: &[(0, 1)],
        price: 4_000,
        shields: false,
        capacity: Some((Storage::Locker, 6)),
        recipe: &[(ResourceId::Metal, 5), (ResourceId::Components, 6)],
        thrust: 0.0,
        torque_thrust: 0.0,
        power: -5.0,
        charge: 0.0,
    },
];

/// Whether a part is one of the two cut across its tile. What the painters
/// and the drag vocabulary ask, so a third kind of corner is one row here.
pub fn is_diagonal(kind: PartKind) -> bool {
    matches!(kind, PartKind::DiagonalWall | PartKind::DiagonalOutsideWall)
}

/// Which corner of its tile a diagonal wall's right angle is in, as a step
/// in each axis with `y` down: [`Rotation::R0`] is the **south-west** corner
/// and the rest follow clockwise, the way [`Rotation::next`] turns. The
/// solid half of the tile is the half that corner is in; the hypotenuse
/// faces the corner opposite, and the two sides of the tile that touch the
/// right angle are the two the wall runs the full length of.
///
/// This is the whole of what "which way round" means for a corner piece.
/// Everything that draws one reads it, so a chamfer that was meant to face
/// the bow cannot be drawn facing the stern by one painter and not another.
pub fn solid_corner(rotation: Rotation) -> (i32, i32) {
    match rotation {
        Rotation::R0 => (-1, 1),
        Rotation::R90 => (-1, -1),
        Rotation::R180 => (1, -1),
        Rotation::R270 => (1, 1),
    }
}

/// Which way a door's leaves slide: along `x` when it stands in a bulkhead
/// running east–west, along `y` in one running north–south. It is the long
/// side of the turned footprint and nothing else — a door is two tiles
/// along the bulkhead and one deep, so the rotation says which — and the
/// room and the painter both read it here rather than guessing off the
/// neighbours, which is how a door in a doorway of nothing came to be drawn
/// one way and walked the other.
pub fn door_slides_along_x(rotation: Rotation) -> bool {
    let (w, h) = footprint(PartKind::Door, rotation);
    w > h
}

/// What a part weighs: its recipe, added up.
///
/// **There is no other answer.** A part does not carry a mass of its own
/// beside the materials it is made of, because the two could then disagree —
/// and a part that weighs more than what went into it is mass appearing out
/// of nothing every time one is built. See [`crate::materials`] for what
/// that buys.
///
/// Strictly positive, which [`defs_are_sound`] checks: every recipe has
/// something in it and every material weighs something.
pub fn part_mass(kind: PartKind) -> f64 {
    kind.def()
        .recipe
        .iter()
        .map(|&(id, units)| units as f64 * id.mass_per_unit())
        .sum()
}

/// The footprint a part covers once it is turned: tiles across and down.
///
/// A quarter turn swaps them; a half turn does not.
pub fn footprint(kind: PartKind, rotation: Rotation) -> (u32, u32) {
    let (w, h) = kind.def().footprint;
    match rotation {
        Rotation::R0 | Rotation::R180 => (w, h),
        Rotation::R90 | Rotation::R270 => (h, w),
    }
}

/// Turn one offset within (or beside) a part's unrotated `w` x `h` box.
///
/// This is the whole of the rotation arithmetic and both the footprint and
/// the use spots go through it, which is the point: a footprint tile and a
/// use spot two tiles outside the part have to end up in the same relation to
/// each other after a turn as before it.
///
/// Clockwise, with `y` growing downwards: what was to the west ends up to the
/// north.
pub fn turn(offset: (i32, i32), (w, h): (u32, u32), rotation: Rotation) -> (i32, i32) {
    let (x, y) = offset;
    let (w, h) = (w as i32, h as i32);
    match rotation {
        Rotation::R0 => (x, y),
        Rotation::R90 => (h - 1 - y, x),
        Rotation::R180 => (w - 1 - x, h - 1 - y),
        Rotation::R270 => (y, w - 1 - x),
    }
}

/// Every tile a part covers, as offsets from its origin, once turned.
pub fn covered(kind: PartKind, rotation: Rotation) -> Vec<(u32, u32)> {
    let (w, h) = kind.def().footprint;
    let mut out = Vec::with_capacity((w * h) as usize);
    for y in 0..h {
        for x in 0..w {
            let (tx, ty) = turn((x as i32, y as i32), (w, h), rotation);
            // Every footprint offset lands back inside the turned box, so
            // these cannot be negative. `covered` is the one place that is
            // true, which is why use spots stay signed.
            out.push((tx as u32, ty as u32));
        }
    }
    out
}

/// Where a Bim stands to use the part, as offsets from its origin, once
/// turned. Signed: most of them are outside the footprint.
///
/// A main engine is the exception: it is worked on from **any** side, so
/// its spots are every tile ringing its footprint (corners left out — you
/// cannot reach the housing from a corner), and the rules ask for *one* of
/// them to be standable rather than all of them. See
/// [`any_side_will_do`]. Everything else stands where its table row says.
pub fn use_spots(kind: PartKind, rotation: Rotation) -> Vec<(i32, i32)> {
    let def = kind.def();
    if any_side_will_do(kind) {
        let (w, h) = (def.footprint.0 as i32, def.footprint.1 as i32);
        let mut ring = Vec::with_capacity(2 * (w + h) as usize);
        for y in 0..h {
            ring.push((-1, y));
            ring.push((w, y));
        }
        for x in 0..w {
            ring.push((x, -1));
            ring.push((x, h));
        }
        return ring
            .into_iter()
            .map(|spot| turn(spot, def.footprint, rotation))
            .collect();
    }
    def.use_spots
        .iter()
        .map(|&spot| turn(spot, def.footprint, rotation))
        .collect()
}

/// Whether a part is used from whichever side a body can get at, rather
/// than from the spots its table row names: the main engines, which are
/// bolted to the stern with hull round three sides of them and worked on
/// from whatever side is left. The validator wants **one** of the ring's
/// tiles to be deck for such a part, not all of them.
pub fn any_side_will_do(kind: PartKind) -> bool {
    kind.def().pushes()
}

/// Whether a consumer keeps running on what the reactor makes when the
/// battery is flat: life support, and the doors. Everything else that
/// draws stops in a brownout. A part named here has to draw, which
/// [`defs_are_sound`] checks, because an essential that drew nothing would
/// be a list that means nothing.
pub fn essential(kind: PartKind) -> bool {
    matches!(kind, PartKind::LifeSupport | PartKind::Door)
}

/// Whether one recipe holds together: something in it, only materials, a
/// real number of each, and no resource named twice — two entries for metal
/// would be a part whose weight depends on which one a reader stopped at.
fn recipe_is_sound(recipe: &'static [(ResourceId, u32)]) -> bool {
    !recipe.is_empty()
        && recipe.iter().enumerate().all(|(i, &(id, units))| {
            let material = matches!(
                id,
                ResourceId::Metal | ResourceId::Components | ResourceId::Emitter
            );
            let once = !recipe[..i].iter().any(|&(seen, _)| seen == id);
            material && units > 0 && once
        })
}

/// Whether the table above holds together: one entry per kind, in order, each
/// made of something and costing something, thrust on engines and nowhere
/// else, turning force on thrusters and nowhere else, a footprint with area in
/// it, exactly one part on the floor layer and exactly one on the structure
/// layer, nothing requiring its own layer, and no container that holds
/// nothing.
pub fn defs_are_sound() -> bool {
    if PARTS.len() != PartKind::ALL.len() {
        return false;
    }
    PartKind::ALL.iter().enumerate().all(|(i, &kind)| {
        let def = &PARTS[i];
        let engine = matches!(kind, PartKind::Engine | PartKind::HeavyEngine);
        let thruster = kind == PartKind::Thruster;
        def.kind == kind
            && recipe_is_sound(def.recipe)
            && part_mass(kind) > 0.0
            && part_mass(kind).is_finite()
            && def.thrust.is_finite()
            && def.thrust >= 0.0
            && def.pushes() == engine
            // The two are exclusive on purpose. A part that both pushed and
            // turned would make "which engines are burning" — and therefore
            // the fuel bill — a different question for every design.
            && def.torque_thrust.is_finite()
            && def.torque_thrust >= 0.0
            && def.turns() == thruster
            // Power is the same shape: made by the reactor, held by the
            // battery, drawn by what `essential` names among others, and
            // never two of those on one part.
            && def.power.is_finite()
            && def.supplies() == (kind == PartKind::Reactor)
            && def.charge.is_finite()
            && def.charge >= 0.0
            && def.stores() == (kind == PartKind::Battery)
            && !(def.supplies() && def.stores())
            && (!essential(kind) || def.draws())
            && def.footprint.0 > 0
            && def.footprint.1 > 0
            && (def.layer == Layer::Floor) == (kind == PartKind::Floor)
            && (def.layer == Layer::Structure) == (kind == PartKind::Structure)
            // A part standing on its own layer could never be placed: the
            // tile it needs is the tile it would fill.
            && def.requires != Some(def.layer)
            // Structure is the bottom of the stack, and the only thing that
            // may need nothing under it.
            && (def.requires.is_none() == (kind == PartKind::Structure))
            && def.price > 0
            && def.capacity.is_none_or(|(_, units)| units > 0)
            // Nothing a body walks through stops its eyes, bar a door with
            // its leaves shut; and every wall, inside or out, stops them.
            && (!def.blocks_sight() || def.blocks_movement || kind == PartKind::Door)
            && (!matches!(
                kind,
                PartKind::Wall
                    | PartKind::OutsideWall
                    | PartKind::DiagonalWall
                    | PartKind::DiagonalOutsideWall
            ) || def.blocks_sight())
    })
}
