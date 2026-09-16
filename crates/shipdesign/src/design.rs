//! A design, and the one function that changes one.
//!
//! [`apply`] is the only way a [`ShipDesign`] ever gains or loses a part, or
//! takes on or gives up cargo. It takes a design and hands back a new one, so
//! a rejected edit cannot leave a half-changed design behind, and every rule
//! about what may be placed where is in one place rather than spread through
//! whatever is driving the UI this week. The page does not mutate a design;
//! it calls this.
//!
//! # A design is the ship *and* what is in it
//!
//! [`ShipDesign::cargo`] is units aboard per [`ResourceId`], and it is part
//! of the design rather than something alongside it: it is paid for out of
//! the same pool, it is hashed into the same [`design_hash`], and the play
//! phase is handed one thing and not two. Buying is bounded by the ship —
//! goods are stowed, and a ship with nowhere to put a thing cannot buy it.

use economy::{Storage, storage, trade_value};
use physics::ResourceId;

use crate::budget::Budget;
use crate::parts::{Layer, PartKind, Rotation, covered, footprint};

/// How many resources there are, which is how long [`ShipDesign::cargo`] is.
/// `physics::ResourceId::ALL.len()`, written out because it sizes an array
/// and an array length has to be a constant. `cargo_is_the_right_length`
/// pins the two together.
pub const CARGO_SLOTS: usize = 6;

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
    ///
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

/// A ship, as designed, and what is aboard it. The build area is square and
/// fixed at Start.
///
/// `next_id` only ever climbs: a removed part's id is never handed out again,
/// so an Edit in flight that names it is refused rather than landing on
/// something else. Ids are deliberately **not** hashed — see [`design_hash`].
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ShipDesign {
    pub build_area: u32,
    pub parts: Vec<PlacedPart>,
    pub next_id: u32,
    /// Units aboard, per [`ResourceId`] in discriminant order. Bought and
    /// sold through [`apply`] like everything else, and bounded by what the
    /// ship has to stow it in.
    pub cargo: [u32; CARGO_SLOTS],
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
            cargo: [0; CARGO_SLOTS],
        }
    }

    /// Units of one resource aboard.
    pub fn carrying(&self, resource: ResourceId) -> u32 {
        self.cargo[resource as usize]
    }

    /// How much of a storage class the ship has, over every part that
    /// provides it. Saturating: a design that arrived from somewhere the
    /// rules were not applied should read as "a great deal of room" rather
    /// than wrap round into none.
    pub fn capacity(&self, class: Storage) -> u32 {
        self.parts
            .iter()
            .filter_map(|p| p.kind.def().capacity)
            .filter(|&(c, _)| c == class)
            .fold(0u32, |sum, (_, units)| sum.saturating_add(units))
    }

    /// How much of that class is taken up by what is aboard.
    pub fn stored(&self, class: Storage) -> u32 {
        ResourceId::ALL
            .iter()
            .filter(|&&id| storage(id) == class)
            .fold(0u32, |sum, &id| sum.saturating_add(self.carrying(id)))
    }

    /// The cargo as `physics` wants it, for [`crate::mass`].
    pub fn manifest(&self) -> Vec<(ResourceId, u32)> {
        ResourceId::ALL
            .iter()
            .map(|&id| (id, self.carrying(id)))
            .filter(|&(_, units)| units > 0)
            .collect()
    }

    pub fn part(&self, id: u32) -> Option<&PlacedPart> {
        self.parts.iter().find(|p| p.id == id)
    }

    pub fn count(&self, kind: PartKind) -> u32 {
        self.parts.iter().filter(|p| p.kind == kind).count() as u32
    }

    /// The occupancy grid: which part, if any, is in each tile of each of the
    /// four layers.
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
/// A flat `Vec` per layer, indexed by `y * side + x`, not a map: nothing here
/// may depend on iteration order, and a design has to hash the same on two
/// machines. Indexed by `Layer as usize`, which is why `Layer::ALL` is in
/// discriminant order.
pub struct Grid {
    side: u32,
    layers: [Vec<u32>; Layer::ALL.len()],
}

impl Grid {
    fn new(side: u32) -> Grid {
        let cells = (side as usize) * (side as usize);
        Grid {
            side,
            layers: [
                vec![0; cells],
                vec![0; cells],
                vec![0; cells],
                vec![0; cells],
            ],
        }
    }

    fn at(&self, (x, y): (u32, u32)) -> usize {
        (y as usize) * (self.side as usize) + (x as usize)
    }

    fn set(&mut self, layer: Layer, tile: (u32, u32), id: u32) {
        let i = self.at(tile);
        self.layers[layer as usize][i] = id;
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
        self.layers[layer as usize][i]
    }

    pub fn has_floor(&self, tile: (i32, i32)) -> bool {
        self.get(Layer::Floor, tile) != 0
    }

    pub fn has_structure(&self, tile: (i32, i32)) -> bool {
        self.get(Layer::Structure, tile) != 0
    }

    /// Whether anything at all is in this tile, on any layer.
    pub fn occupied(&self, tile: (i32, i32)) -> bool {
        Layer::ALL.iter().any(|&l| self.get(l, tile) != 0)
    }
}

/// The only four things that can happen to a design.
///
/// Buying and selling are edits like placing and removing, and for the same
/// reason: they are paid for out of the same pool, they change the same
/// [`design_hash`], and an Accept given before a purchase is an Accept for a
/// ship that is now heavier.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Edit {
    Place {
        kind: PartKind,
        origin: (u32, u32),
        rotation: Rotation,
    },
    /// Deck plating with its own frame: [`PartKind::Structure`] on the tile
    /// if there is none yet, and [`PartKind::Floor`] on that. What the
    /// designer's plating tool sends, so that a player lays a deck in one
    /// pass rather than a frame and then a deck — the two are one thing to
    /// anybody who is not the connectivity check. Both prices are paid when
    /// both go down; a tile that already has frame pays for the deck alone.
    Plate {
        origin: (u32, u32),
    },
    Remove {
        part_id: u32,
    },
    /// Take goods aboard at the station's price.
    Buy {
        resource: ResourceId,
        units: u32,
    },
    /// Put them back. The station takes them at the price it sold them for —
    /// nothing has left the dock, so there is nothing to lose on the deal.
    Sell {
        resource: ResourceId,
        units: u32,
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
    /// The pool will not cover the price.
    Unaffordable = 5,
    /// No part has that id. A removal that raced another removal.
    NoSuchPart = 6,
    /// Something standing on this depends on it: deck plating under a hob,
    /// structure under deck plating. Take the thing above off first.
    ///
    /// It was `FloorUnderObject` when the deck was the only thing anything
    /// stood on. The code is unchanged — these cross the wasm boundary — and
    /// one sentence in `EDIT_LINES` covers both.
    SupportInUse = 7,
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
    /// There is no structure under part of the footprint. The frame goes
    /// down first; everything else is built on it.
    MissingStructure = 10,
    /// Something is already in that tile on this part's own layer. The deck
    /// and the object layer have had their own codes since before there were
    /// four layers; this is the other two.
    LayerOccupied = 11,
    /// The pool will not cover the goods.
    CargoUnaffordable = 12,
    /// Nowhere aboard to put them: not enough of that class of storage, or
    /// none at all.
    NoRoomAboard = 13,
    /// Selling more than is actually aboard.
    NotAboard = 14,
    /// Taking that part off would leave the goods in it with nowhere to go.
    /// Sell them first.
    StorageInUse = 15,
    /// Not the materials aboard to build it out of.
    ///
    /// Never returned by [`apply`] either: in the design phase a part is
    /// bought at the station with money, and it is the construction rule in
    /// [`crate::materials`] that spends metal and components instead. The
    /// code lives here with the others so a player is given one table of
    /// reasons rather than two.
    MaterialsShort = 16,
}

impl EditError {
    pub fn code(self) -> u32 {
        self as u32
    }
}

/// Place or remove a part, buy or sell goods, or say why not.
///
/// The design is handed back whole rather than mutated, so a refusal cannot
/// leave anything half done. A removal **refunds the whole price** and a sale
/// hands back the whole trade value — there is no wastage in the design
/// phase, because nothing has been built and nothing has left the dock; the
/// money is only being promised.
pub fn apply(design: &ShipDesign, budget: &Budget, edit: Edit) -> Result<ShipDesign, EditError> {
    match edit {
        Edit::Place {
            kind,
            origin,
            rotation,
        } => place(design, budget, kind, origin, rotation),
        Edit::Plate { origin } => plate(design, budget, origin),
        Edit::Remove { part_id } => remove(design, part_id),
        Edit::Buy { resource, units } => buy(design, budget, resource, units),
        Edit::Sell { resource, units } => sell(design, resource, units),
    }
}

/// What to say when the layer a part needs is empty.
///
/// Two of the four can be required today and they have a sentence each.
/// Nothing requires `Object` or `Utility` — a part standing on a bunk, or on
/// a conduit, is not a thing — and if one ever does it wants a code of its
/// own rather than borrowing this one.
fn missing(layer: Layer) -> EditError {
    match layer {
        Layer::Floor => EditError::MissingFloor,
        Layer::Structure | Layer::Object | Layer::Utility => EditError::MissingStructure,
    }
}

/// What to say when this part's own layer is already taken in a tile.
fn occupied(layer: Layer) -> EditError {
    match layer {
        Layer::Floor => EditError::DuplicateFloor,
        Layer::Object => EditError::ObjectOverlap,
        Layer::Structure | Layer::Utility => EditError::LayerOccupied,
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

    // One part per layer per tile, and whatever this one needs under it has
    // to be there already. Both questions are asked of every tile of the
    // footprint before anything is placed: half a part is not a thing.
    for &(x, y) in &tiles {
        let tile = (x as i32, y as i32);
        if grid.get(def.layer, tile) != 0 {
            return Err(occupied(def.layer));
        }
        if let Some(under) = def.requires
            && grid.get(under, tile) == 0
        {
            return Err(missing(under));
        }
    }

    if !budget.affords(design, def.price) {
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

/// Frame first if the tile has none, then deck — two placements, one edit,
/// and a refusal of either leaves neither behind.
fn plate(
    design: &ShipDesign,
    budget: &Budget,
    origin: (u32, u32),
) -> Result<ShipDesign, EditError> {
    let tile = (origin.0 as i32, origin.1 as i32);
    let framed = if design.grid().get(Layer::Structure, tile) == 0 {
        place(design, budget, PartKind::Structure, origin, Rotation::R0)?
    } else {
        design.clone()
    };
    place(&framed, budget, PartKind::Floor, origin, Rotation::R0)
}

fn remove(design: &ShipDesign, part_id: u32) -> Result<ShipDesign, EditError> {
    let Some(part) = design.part(part_id).copied() else {
        return Err(EditError::NoSuchPart);
    };

    // Anything standing on this stays standing on it: deck plating under a
    // hob, structure under deck plating. Otherwise a rectangle drag over the
    // galley would take the floor out from under the hob and leave it
    // hanging, which `validate` would then report as a fault in a ship nobody
    // meant to change that way.
    //
    // Asked of every layer rather than of the object layer alone, because
    // what depends on what is `PartDef::requires` and not a list kept here.
    let grid = design.grid();
    for (x, y) in part.tiles() {
        let tile = (x as i32, y as i32);
        for layer in Layer::ALL {
            let above = grid.get(layer, tile);
            if above == 0 || above == part_id {
                continue;
            }
            if design
                .part(above)
                .is_some_and(|p| p.kind.def().requires == Some(part.layer()))
            {
                return Err(EditError::SupportInUse);
            }
        }
    }

    // And a hold with something in it stays as well. A shelf taken off a ship
    // carrying a hundred units of ore is a hundred units of ore standing in
    // the corridor; sell them first.
    if let Some((class, units)) = part.kind.def().capacity
        && design.capacity(class).saturating_sub(units) < design.stored(class)
    {
        return Err(EditError::StorageInUse);
    }

    let mut next = design.clone();
    next.parts.retain(|p| p.id != part_id);
    Ok(next)
}

/// Take goods aboard.
///
/// Two things can refuse it, and they are separate on purpose: a player with
/// no money and a player with no shelf need to be told different things.
/// Supply is not one of them — the station has as much as anybody wants; see
/// `economy`'s module note.
fn buy(
    design: &ShipDesign,
    budget: &Budget,
    resource: ResourceId,
    units: u32,
) -> Result<ShipDesign, EditError> {
    // An order too large to price is an order too large to afford. No wrap
    // here and no saturation either: `trade_value` refuses.
    let value = trade_value(resource, units).map_err(|_| EditError::CargoUnaffordable)?;
    if !budget.affords(design, value) {
        return Err(EditError::CargoUnaffordable);
    }

    let class = storage(resource);
    let wanted = design
        .stored(class)
        .checked_add(units)
        .ok_or(EditError::NoRoomAboard)?;
    if wanted > design.capacity(class) {
        return Err(EditError::NoRoomAboard);
    }

    let mut next = design.clone();
    next.cargo[resource as usize] += units;
    Ok(next)
}

/// Put goods back. Refused beyond what is actually aboard rather than
/// clamped: a sale of more than there is means the page and the ship
/// disagree, and that is worth saying out loud.
fn sell(design: &ShipDesign, resource: ResourceId, units: u32) -> Result<ShipDesign, EditError> {
    if units > design.carrying(resource) {
        return Err(EditError::NotAboard);
    }
    let mut next = design.clone();
    next.cargo[resource as usize] -= units;
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
/// - **The cargo is in it too**, after the parts, in `ResourceId` order and
///   at a fixed length. Two players accepting have to be accepting the same
///   ship *and* the same manifest: the goods came out of the shared pool and
///   the engines have to push them.
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
    for &units in design.cargo.iter() {
        eat(units);
    }
    hash
}
