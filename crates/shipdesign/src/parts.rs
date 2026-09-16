//! What a ship can be built out of.
//!
//! One table, [`PARTS`], with one entry per [`PartKind`] in discriminant
//! order. Every number in it is a **placeholder** — nothing here has been
//! balanced against anything, and the masses and costs exist so that the
//! rules in [`crate::design`] and [`crate::validate`] have something real to
//! be exercised against.
//!
//! What is *not* a placeholder is the shape of the table. The discriminants
//! cross the wasm boundary as numbers and will one day be in a save file, so
//! they are written out and **never renumbered**: a new part is appended, a
//! retired one leaves a hole.

use physics::ResourceId;

/// World units to a tile side. Fixed, and the one place it is written down.
///
/// The room's own grid is nothing to do with this — that is a 10-unit
/// navigation grid over hand-placed furniture. A ship is tiles.
pub const TILE: u32 = 52;

/// Which of a tile's two slots a part sits in.
///
/// A tile holds at most one of each: the deck plating under your boots, and
/// the one thing standing on it. Walls are `Object` rather than a third
/// layer, because a wall and a bunk are equally "the thing in this tile" and
/// two of them in one tile is equally nonsense.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum Layer {
    /// The deck plating. Only [`PartKind::Floor`].
    Floor = 0,
    /// Everything that stands on it, walls included.
    Object = 1,
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
}

impl PartKind {
    /// Every kind, in discriminant order. `ALL[k as usize] == k`, which
    /// [`PartKind::def`] relies on and [`defs_are_sound`] checks.
    pub const ALL: [PartKind; 15] = [
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
    /// **Placeholder.** Grid "up" — a part at [`Rotation::R0`] — is
    /// [`physics::Facing::Forward`], and the rest follow it round clockwise.
    /// The mapping is arbitrary in the one way that matters: whether ships
    /// rotate, and how a ship's own frame lines up with the grid it was drawn
    /// on, is undecided. `physics` deliberately refuses to assume an answer
    /// and neither does anything else; this is the placeholder that lets the
    /// mass and acceleration arithmetic be written and tested now.
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
/// `web/ship.js` is where the words live and this is only the numbers.
#[derive(Clone, Copy, Debug)]
pub struct PartDef {
    pub kind: PartKind,
    /// Tiles across and down, **unrotated**. [`footprint`] turns this and a
    /// [`Rotation`] into the tiles actually covered.
    pub footprint: (u32, u32),
    pub layer: Layer,
    /// Whether a body may pass through the tile. A door does not block; nor
    /// does a chair, which is a seat rather than an obstacle and has to be
    /// stood on to be used.
    pub blocks_movement: bool,
    /// Whether every tile of the footprint needs deck plating under it.
    /// Everything on the object layer except a wall: a wall is hull.
    pub requires_floor: bool,
    /// Tile offsets from the origin, **unrotated**, where a Bim stands to use
    /// the part. They rotate with it. Offsets are signed because most of them
    /// are outside the footprint — you stand *beside* a cold store.
    pub use_spots: &'static [(i32, i32)],
    pub cost: &'static [(ResourceId, u32)],
    /// Strictly greater than zero. A part that weighs nothing is a part that
    /// makes the ship accelerate for free.
    pub mass: f64,
    /// Greater than zero for [`PartKind::Engine`] and nothing else.
    pub thrust: f64,
}

/// The table. Placeholder numbers throughout; see the module note.
///
/// Read the `use_spots` column as the whole of what the play phase will be
/// told about how a part is approached — [`crate::validate`] already insists
/// every one of them is floor a body can stand on and that they can all reach
/// each other, so a design that passes here is one the crew can work.
pub static PARTS: [PartDef; 15] = [
    PartDef {
        kind: PartKind::Floor,
        footprint: (1, 1),
        layer: Layer::Floor,
        blocks_movement: false,
        requires_floor: false,
        use_spots: &[],
        cost: &[(ResourceId::Metal, 1)],
        mass: 5.0,
        thrust: 0.0,
    },
    PartDef {
        kind: PartKind::Wall,
        footprint: (1, 1),
        layer: Layer::Object,
        blocks_movement: true,
        // A wall is hull. It is the one thing on the object layer that can
        // stand where there is no deck, which is what lets a ship be walled
        // before it is floored.
        requires_floor: false,
        use_spots: &[],
        cost: &[(ResourceId::Metal, 2)],
        mass: 8.0,
        thrust: 0.0,
    },
    PartDef {
        kind: PartKind::Door,
        footprint: (1, 1),
        layer: Layer::Object,
        blocks_movement: false,
        requires_floor: true,
        // Nobody *uses* a door; they walk through it. It earns its keep in
        // the reachability check, which lets a route pass through one.
        use_spots: &[],
        cost: &[(ResourceId::Metal, 2), (ResourceId::Components, 1)],
        mass: 6.0,
        thrust: 0.0,
    },
    PartDef {
        kind: PartKind::Engine,
        footprint: (2, 3),
        layer: Layer::Object,
        blocks_movement: true,
        requires_floor: true,
        // Beside the middle of the left-hand side. Asymmetric on purpose:
        // it is the part the rotation test pins by hand.
        use_spots: &[(-1, 1)],
        cost: &[
            (ResourceId::Metal, 40),
            (ResourceId::Fuel, 10),
            (ResourceId::Components, 20),
        ],
        mass: 400.0,
        thrust: 500.0,
    },
    PartDef {
        kind: PartKind::Bunk,
        footprint: (1, 2),
        layer: Layer::Object,
        blocks_movement: true,
        requires_floor: true,
        use_spots: &[(-1, 0)],
        cost: &[(ResourceId::Metal, 6), (ResourceId::Components, 2)],
        mass: 30.0,
        thrust: 0.0,
    },
    PartDef {
        kind: PartKind::ColdStore,
        footprint: (1, 1),
        layer: Layer::Object,
        blocks_movement: true,
        requires_floor: true,
        use_spots: &[(0, 1)],
        cost: &[(ResourceId::Metal, 8), (ResourceId::Components, 6)],
        mass: 60.0,
        thrust: 0.0,
    },
    PartDef {
        kind: PartKind::Worktop,
        footprint: (2, 1),
        layer: Layer::Object,
        blocks_movement: true,
        requires_floor: true,
        use_spots: &[(0, 1), (1, 1)],
        cost: &[(ResourceId::Metal, 6)],
        mass: 25.0,
        thrust: 0.0,
    },
    PartDef {
        kind: PartKind::Hob,
        footprint: (1, 1),
        layer: Layer::Object,
        blocks_movement: true,
        requires_floor: true,
        use_spots: &[(0, 1)],
        cost: &[(ResourceId::Metal, 5), (ResourceId::Components, 4)],
        mass: 30.0,
        thrust: 0.0,
    },
    PartDef {
        kind: PartKind::Dishwasher,
        footprint: (1, 1),
        layer: Layer::Object,
        blocks_movement: true,
        requires_floor: true,
        use_spots: &[(0, 1)],
        cost: &[(ResourceId::Metal, 6), (ResourceId::Components, 6)],
        mass: 45.0,
        thrust: 0.0,
    },
    PartDef {
        kind: PartKind::Table,
        footprint: (2, 1),
        layer: Layer::Object,
        blocks_movement: true,
        requires_floor: true,
        // One side, not both. A table with four use spots makes every
        // fixture design need a gangway round it, which is a rule nobody
        // asked for.
        use_spots: &[(0, 1), (1, 1)],
        cost: &[(ResourceId::Metal, 5)],
        mass: 20.0,
        thrust: 0.0,
    },
    PartDef {
        kind: PartKind::Chair,
        footprint: (1, 1),
        layer: Layer::Object,
        // A seat is stood on, not walked round. Its own tile is its use
        // spot, and a blocking part whose use spot is itself could never
        // validate.
        blocks_movement: false,
        requires_floor: true,
        use_spots: &[(0, 0)],
        cost: &[(ResourceId::Metal, 2)],
        mass: 8.0,
        thrust: 0.0,
    },
    PartDef {
        kind: PartKind::Toilet,
        footprint: (1, 1),
        layer: Layer::Object,
        blocks_movement: true,
        requires_floor: true,
        use_spots: &[(0, 1)],
        cost: &[(ResourceId::Metal, 5), (ResourceId::Components, 3)],
        mass: 25.0,
        thrust: 0.0,
    },
    PartDef {
        kind: PartKind::Basin,
        footprint: (1, 1),
        layer: Layer::Object,
        blocks_movement: true,
        requires_floor: true,
        use_spots: &[(0, 1)],
        cost: &[(ResourceId::Metal, 4), (ResourceId::Components, 2)],
        mass: 15.0,
        thrust: 0.0,
    },
    PartDef {
        kind: PartKind::HydroBay,
        footprint: (2, 2),
        layer: Layer::Object,
        blocks_movement: true,
        requires_floor: true,
        use_spots: &[(0, 2)],
        cost: &[(ResourceId::Metal, 12), (ResourceId::Components, 10)],
        mass: 80.0,
        thrust: 0.0,
    },
    PartDef {
        kind: PartKind::BroomLocker,
        footprint: (1, 1),
        layer: Layer::Object,
        blocks_movement: true,
        requires_floor: true,
        use_spots: &[(0, 1)],
        cost: &[(ResourceId::Metal, 3)],
        mass: 10.0,
        thrust: 0.0,
    },
];

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
pub fn use_spots(kind: PartKind, rotation: Rotation) -> Vec<(i32, i32)> {
    let def = kind.def();
    def.use_spots
        .iter()
        .map(|&spot| turn(spot, def.footprint, rotation))
        .collect()
}

/// Whether the table above holds together: one entry per kind, in order, each
/// weighing something, thrust on engines and nowhere else, a footprint with
/// area in it, and nothing on the floor layer that is not the floor.
pub fn defs_are_sound() -> bool {
    if PARTS.len() != PartKind::ALL.len() {
        return false;
    }
    PartKind::ALL.iter().enumerate().all(|(i, &kind)| {
        let def = &PARTS[i];
        let engine = kind == PartKind::Engine;
        def.kind == kind
            && def.mass > 0.0
            && def.mass.is_finite()
            && def.thrust.is_finite()
            && def.thrust >= 0.0
            && (def.thrust > 0.0) == engine
            && def.footprint.0 > 0
            && def.footprint.1 > 0
            && (def.layer == Layer::Floor) == (kind == PartKind::Floor)
            && def.cost.iter().all(|&(_, units)| units > 0)
    })
}
