//! A design, and the one function that changes one.
//!
//! [`apply`] is the only way a [`ShipDesign`] ever gains or loses a part. It
//! takes a design and hands back a new one, so a rejected edit cannot leave a
//! half-changed design behind, and every rule about what may be placed where
//! is in one place rather than spread through whatever is driving the UI this
//! week. The page does not mutate a design; it calls this.

use crate::budget::Budget;
use crate::parts::{Layer, PartKind, Rotation, covered, footprint};

/// One part, placed. `origin` is the top-left tile of the **turned**
/// footprint, so a part's origin is where you clicked whichever way round it
/// is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PlacedPart {
    pub id: u32,
    pub kind: PartKind,
    pub origin: (u32, u32),
    pub rotation: Rotation,
}

impl PlacedPart {
    /// Every tile this part covers.
    pub fn tiles(&self) -> Vec<(u32, u32)> {
        covered(self.kind, self.rotation)
            .into_iter()
            .map(|(dx, dy)| (self.origin.0 + dx, self.origin.1 + dy))
            .collect()
    }

    /// Where a Bim stands to use it, in tile coordinates. Signed, because a
    /// use spot can fall outside the build area — which [`crate::validate`]
    /// reports rather than clamping away.
    pub fn use_spots(&self) -> Vec<(i32, i32)> {
        crate::parts::use_spots(self.kind, self.rotation)
            .into_iter()
            .map(|(dx, dy)| (self.origin.0 as i32 + dx, self.origin.1 as i32 + dy))
            .collect()
    }

    pub fn layer(&self) -> Layer {
        self.kind.def().layer
    }
}

/// A ship, as designed. The build area is square and fixed at Start.
///
/// `next_id` only ever climbs: a removed part's id is never handed out again,
/// so an Edit in flight that names it is refused rather than landing on
/// something else. Ids are deliberately **not** hashed — see [`design_hash`].
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ShipDesign {
    pub build_area: u32,
    pub parts: Vec<PlacedPart>,
    pub next_id: u32,
}

impl ShipDesign {
    /// An empty design in a `build_area` x `build_area` square of tiles.
    pub fn new(build_area: u32) -> ShipDesign {
        ShipDesign {
            build_area,
            parts: Vec::new(),
            // Ids start at 1 so that 0 can mean "nothing here" across the
            // wasm boundary, where there are no options.
            next_id: 1,
        }
    }

    pub fn part(&self, id: u32) -> Option<&PlacedPart> {
        self.parts.iter().find(|p| p.id == id)
    }

    pub fn count(&self, kind: PartKind) -> u32 {
        self.parts.iter().filter(|p| p.kind == kind).count() as u32
    }

    /// The occupancy grid: which part, if any, is in each tile of each layer.
    ///
    /// Built fresh from `parts` every time rather than kept alongside it. A
    /// cached grid is a second source of truth, and the whole of this crate
    /// is one source of truth about what a ship is.
    pub fn grid(&self) -> Grid {
        let mut grid = Grid::new(self.build_area);
        for part in &self.parts {
            for tile in part.tiles() {
                grid.set(part.layer(), tile, part.id);
            }
        }
        grid
    }

    /// Whether a tile is inside the build area at all.
    pub fn holds(&self, (x, y): (i32, i32)) -> bool {
        x >= 0 && y >= 0 && (x as u32) < self.build_area && (y as u32) < self.build_area
    }
}

/// Which part id is in each tile of each layer. `0` is empty.
///
/// A flat `Vec` indexed by `y * side + x`, not a map: nothing here may depend
/// on iteration order, and a design has to hash the same on two machines.
pub struct Grid {
    side: u32,
    floor: Vec<u32>,
    object: Vec<u32>,
}

impl Grid {
    fn new(side: u32) -> Grid {
        let cells = (side as usize) * (side as usize);
        Grid {
            side,
            floor: vec![0; cells],
            object: vec![0; cells],
        }
    }

    fn at(&self, (x, y): (u32, u32)) -> usize {
        (y as usize) * (self.side as usize) + (x as usize)
    }

    fn set(&mut self, layer: Layer, tile: (u32, u32), id: u32) {
        let i = self.at(tile);
        match layer {
            Layer::Floor => self.floor[i] = id,
            Layer::Object => self.object[i] = id,
        }
    }

    pub fn side(&self) -> u32 {
        self.side
    }

    pub fn inside(&self, (x, y): (i32, i32)) -> bool {
        x >= 0 && y >= 0 && (x as u32) < self.side && (y as u32) < self.side
    }

    /// The part in `tile` on `layer`, or 0. Out of bounds is 0 as well — the
    /// callers that care about the difference ask [`Grid::inside`] first.
    pub fn get(&self, layer: Layer, (x, y): (i32, i32)) -> u32 {
        if !self.inside((x, y)) {
            return 0;
        }
        let i = self.at((x as u32, y as u32));
        match layer {
            Layer::Floor => self.floor[i],
            Layer::Object => self.object[i],
        }
    }

    pub fn has_floor(&self, tile: (i32, i32)) -> bool {
        self.get(Layer::Floor, tile) != 0
    }

    pub fn occupied(&self, tile: (i32, i32)) -> bool {
        self.get(Layer::Floor, tile) != 0 || self.get(Layer::Object, tile) != 0
    }
}

/// The only two things that can happen to a design.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Edit {
    Place {
        kind: PartKind,
        origin: (u32, u32),
        rotation: Rotation,
    },
    Remove {
        part_id: u32,
    },
}

/// Why an edit was refused.
///
/// The discriminants cross the wasm boundary and index `EDIT_LINES` in
/// `web/ship.js`, so they are written out and not renumbered. `0` is not a
/// variant: it is "no error", which is what the export returns on success.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum EditError {
    /// Some of the footprint falls outside the build area.
    OutOfBounds = 1,
    /// Another object is already standing there.
    ObjectOverlap = 2,
    /// There is no deck under part of the footprint.
    MissingFloor = 3,
    /// There is already deck plating in one of those tiles.
    DuplicateFloor = 4,
    /// The stockpile will not cover it.
    Unaffordable = 5,
    /// No part has that id. A removal that raced another removal.
    NoSuchPart = 6,
    /// Deck plating with something standing on it. Take the thing off first.
    FloorUnderObject = 7,
    /// The host sent a number that is not a part kind or a rotation.
    ///
    /// [`apply`] never returns this — its `Edit` is already typed. The wasm
    /// layer does, and it lives here so that the reasons a player can be
    /// given are one table rather than two.
    BadCode = 8,
    /// The design phase is over: everybody accepted and the ship is settled.
    ///
    /// Also never returned by [`apply`], and here for the same reason. Whose
    /// design phase it is and whether it has finished is a question about a
    /// session, which this crate deliberately knows nothing about.
    Locked = 9,
}

impl EditError {
    pub fn code(self) -> u32 {
        self as u32
    }
}

/// Place or remove a part, or say why not.
///
/// The design is handed back whole rather than mutated, so a refusal cannot
/// leave anything half done. A removal **refunds the whole cost** — there is
/// no wastage in the design phase, because nothing has been built yet; the
/// stockpile is only being promised.
pub fn apply(design: &ShipDesign, budget: &Budget, edit: Edit) -> Result<ShipDesign, EditError> {
    match edit {
        Edit::Place {
            kind,
            origin,
            rotation,
        } => place(design, budget, kind, origin, rotation),
        Edit::Remove { part_id } => remove(design, part_id),
    }
}

fn place(
    design: &ShipDesign,
    budget: &Budget,
    kind: PartKind,
    origin: (u32, u32),
    rotation: Rotation,
) -> Result<ShipDesign, EditError> {
    let def = kind.def();
    let (w, h) = footprint(kind, rotation);

    // In bounds, checked in u64 so a wild origin cannot wrap into looking
    // legal. The build area is square, so one side does for both.
    let far_x = origin.0 as u64 + w as u64;
    let far_y = origin.1 as u64 + h as u64;
    if far_x > design.build_area as u64 || far_y > design.build_area as u64 {
        return Err(EditError::OutOfBounds);
    }

    let grid = design.grid();
    let tiles: Vec<(u32, u32)> = covered(kind, rotation)
        .into_iter()
        .map(|(dx, dy)| (origin.0 + dx, origin.1 + dy))
        .collect();

    for &(x, y) in &tiles {
        let tile = (x as i32, y as i32);
        match def.layer {
            Layer::Floor => {
                if grid.has_floor(tile) {
                    return Err(EditError::DuplicateFloor);
                }
            }
            Layer::Object => {
                if grid.get(Layer::Object, tile) != 0 {
                    return Err(EditError::ObjectOverlap);
                }
                if def.requires_floor && !grid.has_floor(tile) {
                    return Err(EditError::MissingFloor);
                }
            }
        }
    }

    if !budget.affords(design, def.cost) {
        return Err(EditError::Unaffordable);
    }

    let mut next = design.clone();
    next.parts.push(PlacedPart {
        id: next.next_id,
        kind,
        origin,
        rotation,
    });
    next.next_id += 1;
    Ok(next)
}

fn remove(design: &ShipDesign, part_id: u32) -> Result<ShipDesign, EditError> {
    let Some(part) = design.part(part_id).copied() else {
        return Err(EditError::NoSuchPart);
    };

    // Deck plating with something standing on it stays. Otherwise a rectangle
    // drag over the galley would take the floor out from under the hob and
    // leave it hanging — which `validate` would then report as a fault in a
    // ship nobody meant to change that way.
    if part.layer() == Layer::Floor {
        let grid = design.grid();
        for (x, y) in part.tiles() {
            if grid.get(Layer::Object, (x as i32, y as i32)) != 0 {
                return Err(EditError::FloorUnderObject);
            }
        }
    }

    let mut next = design.clone();
    next.parts.retain(|p| p.id != part_id);
    Ok(next)
}

/// A design's identity: the same layout gives the same number whatever order
/// it was built in, on any machine.
///
/// An Accept is recorded against one of these, so two players accepting have
/// to be accepting the same ship. That makes three things load-bearing:
///
/// - **Part ids are not hashed.** Build the galley then the heads, or the
///   heads then the galley, and the ids differ while the ship does not.
/// - **The parts are sorted first**, by `(origin.y, origin.x, kind)` — with
///   rotation as a last tiebreak, so the order is total even for a design
///   that somehow holds two parts of one kind at one origin.
/// - **It is written out by hand.** FNV-1a over little-endian `u32`s, no
///   `Hash` derive and no `DefaultHasher`: those are explicitly allowed to
///   differ between builds, and this number crosses between machines.
pub fn design_hash(design: &ShipDesign) -> u64 {
    let mut keys: Vec<(u32, u32, u32, u32)> = design
        .parts
        .iter()
        .map(|p| (p.origin.1, p.origin.0, p.kind.code(), p.rotation.code()))
        .collect();
    keys.sort_unstable();

    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = OFFSET;
    let mut eat = |value: u32| {
        for byte in value.to_le_bytes() {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(PRIME);
        }
    };

    // The build area is in the hash too: the same parts in a bigger square
    // are a different ship, and an Accept must not carry across a resize.
    eat(design.build_area);
    for (y, x, kind, rotation) in keys {
        eat(x);
        eat(y);
        eat(kind);
        eat(rotation);
    }
    hash
}
