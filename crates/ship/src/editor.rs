//! The design phase as a state machine: one design, one pool of money, one
//! camera, and everybody's Accept.
//!
//! Every rule about what may be placed where is in `shipdesign` and none of
//! it is repeated here. What is here is the part that only makes sense with a
//! pointer in it — which tool is picked, which way the ghost is turned, what
//! a drag covers — plus the two pieces of bookkeeping that belong to the
//! *session* rather than to the design: who has accepted, and whether the
//! phase is over.
//!
//! # Accepts, and what clears them
//!
//! An Accept is recorded **against a hash**, never against "the design". Any
//! successful edit by anybody changes the hash and clears every Accept, which
//! is the whole of the protocol: there is no way to be holding an Accept for
//! a ship that is no longer on screen.

use physics::ResourceId;
use shipdesign::design::{Edit, EditError};
use shipdesign::parts::{Layer, PartKind, Rotation, footprint};
use shipdesign::validate::{ExposureMap, Issue};
use shipdesign::{
    Budget, Money, ShipDesign, apply, design_hash, exposure, starting_pool, validate,
};

use crate::view::View;

/// Which half of the page's life it is in.
///
/// Two, and there is deliberately no third. "Is it finished", "may I still
/// edit" and "has the game started" are the same question asked three ways,
/// and three exports answering it would be three things that can disagree —
/// there were three for about an hour once and the boundary check is what
/// said so.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum Phase {
    /// Placing and removing, instant and free to undo by removal.
    Design = 0,
    /// Everybody accepted the same ship, the design is settled, and the game
    /// has it. Editing is locked; from here on the world's clock is running.
    ///
    /// The code is unchanged from when this was called `Finished` and meant
    /// "waiting for a play phase that does not exist yet" — it crosses the
    /// wasm boundary, and `PHASE_DESIGN` in `web/ship.js` is the other half of
    /// the pair.
    Game = 1,
}

/// A pointer held down over the grid.
///
/// What it covers depends on the tool, and that is worked out on demand
/// rather than accumulated as the pointer moves: a drag is a *from* and a
/// *to*, so dragging back over your own path takes tiles off again, which is
/// what every other rectangle tool in the world does.
#[derive(Clone, Copy, Debug)]
pub struct Drag {
    pub from: (i32, i32),
    pub to: (i32, i32),
    /// A right-drag, which takes things off rather than putting them on.
    pub removing: bool,
}

pub struct Editor {
    pub design: ShipDesign,
    pub budget: Budget,
    pub view: View,

    /// Frozen at Start, both of them. The crew that will board is the players
    /// in the lobby, so `validate` is asked for that many bunks and chairs.
    pub players: u32,
    pub local: u32,

    /// The hash each player has accepted for, or `None`. Indexed by slot.
    accepts: Vec<Option<u64>>,
    pub phase: Phase,

    pub tool: PartKind,
    pub ghost: Rotation,
    /// The tile under the pointer, or `None` when it is off the canvas.
    /// Signed: a pointer outside the build area is outside it.
    pub hover: Option<(i32, i32)>,
    pub drag: Option<Drag>,
    /// The issue whose tiles are being pointed at in the list, if any.
    pub focus: Option<usize>,

    issues: Vec<Issue>,
    /// Where the outside can see in. Kept beside the issues and refreshed
    /// with them: the painter tints it every frame and working it out sixty
    /// times a second for a ship nobody is changing would be a flood fill a
    /// frame for nothing.
    exposed: ExposureMap,
    hash: u64,
}

impl Editor {
    /// Open a design phase.
    ///
    /// The pool is worked out **here and once**: every Bim's money, plus the
    /// lone player's bonus, through the one function in `economy` that a
    /// native server would use as well. A pool that could not be worked out —
    /// a crew of nobody, or a figure so large it overflows — is nothing to
    /// spend rather than a panic: a cdylib that panics aborts, and an abort
    /// tells the player nothing at all.
    pub fn new(
        area: u32,
        money_per_bim: Money,
        players: u32,
        local: u32,
        width: f32,
        height: f32,
    ) -> Editor {
        let players = players.max(1);
        let design = ShipDesign::new(area.max(1));
        let hash = design_hash(&design);
        let issues = validate(&design, players);
        let exposed = exposure(&design);
        Editor {
            design,
            budget: Budget::new(starting_pool(money_per_bim, players).unwrap_or(0)),
            view: View::new(area.max(1), width, height),
            players,
            local: local.min(players - 1),
            accepts: vec![None; players as usize],
            phase: Phase::Design,
            tool: PartKind::Floor,
            ghost: Rotation::R0,
            hover: None,
            drag: None,
            focus: None,
            issues,
            exposed,
            hash,
        }
    }

    /// An editor whose design phase is already over: `design` laid out,
    /// accepted by everybody, and settled. What the simulation opens with —
    /// there is no design phase to skip if there was never one, and every
    /// export that reads the editor still has one to read.
    pub fn settled(
        design: ShipDesign,
        players: u32,
        local: u32,
        width: f32,
        height: f32,
    ) -> Editor {
        let mut editor = Editor::new(design.build_area, 0, players, local, width, height);
        editor.design = design;
        editor.refresh();
        for slot in &mut editor.accepts {
            *slot = Some(editor.hash);
        }
        editor.phase = Phase::Game;
        editor
    }

    /// Start the design phase on `design` rather than on an empty grid, as
    /// a gift: the crew's pool is what they brought, and what is already
    /// there cost them nothing — see [`Budget::with_gift`]. Everything is
    /// still theirs to change.
    pub fn give(&mut self, design: ShipDesign) {
        self.budget = Budget::with_gift(self.budget.pool, &design);
        self.design = design;
        self.refresh();
    }

    pub fn hash(&self) -> u64 {
        self.hash
    }

    pub fn issues(&self) -> &[Issue] {
        &self.issues
    }

    /// Which tiles the outside can see into. What the painter tints, and
    /// what stage 5 will one day be handed.
    pub fn exposed(&self) -> &ExposureMap {
        &self.exposed
    }

    pub fn has_errors(&self) -> bool {
        shipdesign::has_errors(&self.issues)
    }

    /// Everything derived from the design, redone. Called after an edit and
    /// nowhere else — validation walks the whole grid and is not something to
    /// do sixty times a second for a ship nobody is changing.
    fn refresh(&mut self) {
        self.hash = design_hash(&self.design);
        self.issues = validate(&self.design, self.players);
        self.exposed = exposure(&self.design);
        self.focus = None;
        // A ship that has changed is a ship nobody has accepted.
        for slot in &mut self.accepts {
            *slot = None;
        }
    }

    /// Put a part down. `0` means it took; anything else is an
    /// [`EditError`] code, which `EDIT_LINES` in `web/ship.js` turns into a
    /// sentence.
    ///
    /// This is the only door in. The host reaches it through `net`, so that
    /// wiring a transport up is a change to that object and not to every
    /// click handler on the page.
    pub fn place(&mut self, kind: u32, x: u32, y: u32, rotation: u32) -> u32 {
        let (Some(kind), Some(rotation)) =
            (PartKind::from_code(kind), Rotation::from_code(rotation))
        else {
            return EditError::BadCode.code();
        };
        self.edit(placing(kind, (x, y), rotation))
    }

    pub fn remove(&mut self, part_id: u32) -> u32 {
        self.edit(Edit::Remove { part_id })
    }

    /// Take goods aboard, or put them back. `0` means it took; anything else
    /// is an [`EditError`] code, exactly as for a part — a purchase is an
    /// edit, so it goes through the same door and clears everybody's Accept
    /// the same way.
    pub fn buy(&mut self, resource: u32, units: u32) -> u32 {
        match ResourceId::ALL.get(resource as usize).copied() {
            Some(resource) => self.edit(Edit::Buy { resource, units }),
            None => EditError::BadCode.code(),
        }
    }

    pub fn sell(&mut self, resource: u32, units: u32) -> u32 {
        match ResourceId::ALL.get(resource as usize).copied() {
            Some(resource) => self.edit(Edit::Sell { resource, units }),
            None => EditError::BadCode.code(),
        }
    }

    fn edit(&mut self, edit: Edit) -> u32 {
        if self.phase != Phase::Design {
            return EditError::Locked.code();
        }
        match apply(&self.design, &self.budget, edit) {
            Ok(next) => {
                self.design = next;
                self.refresh();
                0
            }
            Err(why) => why.code(),
        }
    }

    // --- accepting --------------------------------------------------------

    /// Record `slot`'s Accept, but only for the ship actually on screen.
    ///
    /// Returns whether it was taken. A mismatched hash is refused rather than
    /// quietly accepted against the current design: the whole point of the
    /// hash is that an Accept in flight when somebody else placed a wall is
    /// an Accept for a ship that no longer exists.
    pub fn accept(&mut self, slot: u32, hash: u64) -> bool {
        if self.phase != Phase::Design || slot >= self.players {
            return false;
        }
        if hash != self.hash || self.has_errors() {
            return false;
        }
        self.accepts[slot as usize] = Some(hash);
        if self.everyone_agrees() {
            self.phase = Phase::Game;
        }
        true
    }

    pub fn unaccept(&mut self, slot: u32) {
        if self.phase == Phase::Design && slot < self.players {
            self.accepts[slot as usize] = None;
        }
    }

    pub fn accepted(&self, slot: u32) -> bool {
        self.accepts
            .get(slot as usize)
            .is_some_and(|a| *a == Some(self.hash))
    }

    fn everyone_agrees(&self) -> bool {
        self.accepts.iter().all(|a| *a == Some(self.hash))
    }

    /// The handoff. Stage 5 takes this and spawns a Bim at each bunk; there
    /// is nothing to hand it to yet, so the page shows a placeholder and this
    /// is what it will one day pass on.
    pub fn finish_design(&self) -> Option<&ShipDesign> {
        (self.phase == Phase::Game).then_some(&self.design)
    }

    // --- the pointer ------------------------------------------------------

    pub fn set_tool(&mut self, kind: u32) {
        if let Some(kind) = PartKind::from_code(kind) {
            self.tool = kind;
        }
    }

    pub fn rotate_ghost(&mut self) {
        self.ghost = self.ghost.next();
    }

    pub fn hover_at(&mut self, x: f32, y: f32) {
        let tile = self.view.to_tile(x, y);
        self.hover = Some(tile);
        if let Some(drag) = &mut self.drag {
            drag.to = tile;
        }
    }

    pub fn leave(&mut self) {
        self.hover = None;
    }

    /// Whether the tool would go down where the pointer is. What the ghost's
    /// colour is, and nothing else — the actual placement asks again, because
    /// between a ghost and a click somebody else may have built there.
    pub fn ghost_ok(&self) -> bool {
        let Some((x, y)) = self.hover else {
            return false;
        };
        if self.phase != Phase::Design || x < 0 || y < 0 {
            return false;
        }
        apply(
            &self.design,
            &self.budget,
            placing(self.tool, (x as u32, y as u32), self.ghost),
        )
        .is_ok()
    }

    /// The part under the pointer, or 0. Objects win over the deck beneath
    /// them — the deck is what is left when you point at nothing.
    pub fn hovered_part(&self) -> u32 {
        let Some(tile) = self.hover else { return 0 };
        let grid = self.design.grid();
        let object = grid.get(Layer::Object, tile);
        if object != 0 {
            return object;
        }
        grid.get(Layer::Floor, tile)
    }

    pub fn drag_begin(&mut self, x: f32, y: f32, removing: bool) {
        let tile = self.view.to_tile(x, y);
        self.hover = Some(tile);
        self.drag = Some(Drag {
            from: tile,
            to: tile,
            removing,
        });
    }

    pub fn drag_cancel(&mut self) {
        self.drag = None;
    }

    /// Every tile the current drag covers, in the order the edits should go
    /// out: rows top to bottom, left to right, so a rectangle of deck fills
    /// the way it is read.
    ///
    /// The shape depends on the tool, which is the whole of the drag
    /// vocabulary: a rectangle for the things you fill an area with — frame,
    /// deck, conduit — and for taking things off, a straight line for the
    /// things you draw a run of, which is both kinds of wall, and one tile
    /// for everything else. A cold store is placed, not painted.
    pub fn drag_tiles(&self) -> Vec<(u32, u32)> {
        let Some(drag) = self.drag else {
            return Vec::new();
        };
        let area = matches!(
            self.tool,
            PartKind::Structure | PartKind::Floor | PartKind::PowerConduit
        );
        let run = matches!(self.tool, PartKind::Wall | PartKind::OutsideWall);
        let cells = if drag.removing || area {
            rectangle(drag.from, drag.to)
        } else if run {
            straight_line(drag.from, drag.to)
        } else {
            vec![drag.to]
        };
        cells
            .into_iter()
            .filter(|&t| self.design.holds(t))
            .map(|(x, y)| (x as u32, y as u32))
            .collect()
    }

    /// The parts a removing drag would take off: **the top of each tile's
    /// stack, and only that**. What is standing in a tile comes off before
    /// what runs through it, that before the deck, and the deck before the
    /// frame — one layer a click, so a right-click on a hob takes the hob
    /// and leaves the deck, and a second one takes the deck. A rectangle
    /// peels every tile in it by one.
    ///
    /// The order across tiles is the reason this is worked out here rather
    /// than in the host: objects first, then deck, then frame, or a tile's
    /// deck would be refused as `SupportInUse` because the neighbouring
    /// tile's object, in the same drag, had not come off yet.
    pub fn drag_parts(&self) -> Vec<u32> {
        const TOP_DOWN: [Layer; 4] = [
            Layer::Object,
            Layer::Utility,
            Layer::Floor,
            Layer::Structure,
        ];
        let tiles = self.drag_tiles();
        let grid = self.design.grid();
        let mut picked: Vec<(usize, u32)> = Vec::new();
        for &(x, y) in &tiles {
            for (rank, &layer) in TOP_DOWN.iter().enumerate() {
                let id = grid.get(layer, (x as i32, y as i32));
                if id != 0 {
                    if !picked.iter().any(|&(_, p)| p == id) {
                        picked.push((rank, id));
                    }
                    break;
                }
            }
        }
        picked.sort_unstable();
        picked.into_iter().map(|(_, id)| id).collect()
    }

    /// Where the ghost's footprint would land, as a tile rectangle. Used by
    /// the painter, and by nothing that decides anything.
    pub fn ghost_box(&self) -> Option<((i32, i32), (u32, u32))> {
        let hover = self.hover?;
        Some((hover, footprint(self.tool, self.ghost)))
    }
}

/// Every tile in the rectangle two corners describe, either way round.
fn rectangle(a: (i32, i32), b: (i32, i32)) -> Vec<(i32, i32)> {
    let (x0, x1) = (a.0.min(b.0), a.0.max(b.0));
    let (y0, y1) = (a.1.min(b.1), a.1.max(b.1));
    let mut out = Vec::new();
    for y in y0..=y1 {
        for x in x0..=x1 {
            out.push((x, y));
        }
    }
    out
}

/// A straight run from `a` towards `b` along whichever axis moved further.
///
/// Straight rather than a diagonal staircase on purpose: a wall drawn with a
/// wobble in it is a wall nobody meant, and the two axes are what a bulkhead
/// is ever drawn along.
fn straight_line(a: (i32, i32), b: (i32, i32)) -> Vec<(i32, i32)> {
    let (dx, dy) = ((b.0 - a.0).abs(), (b.1 - a.1).abs());
    let mut out = Vec::new();
    if dx >= dy {
        let (x0, x1) = (a.0.min(b.0), a.0.max(b.0));
        for x in x0..=x1 {
            out.push((x, a.1));
        }
    } else {
        let (y0, y1) = (a.1.min(b.1), a.1.max(b.1));
        for y in y0..=y1 {
            out.push((a.0, y));
        }
    }
    out
}

/// The edit a tool makes. Deck plating is the one tool that is not a plain
/// placement: it lays its own frame — see [`Edit::Plate`] — because the
/// frame and the deck are one thing to a player, and were two clicks.
fn placing(kind: PartKind, origin: (u32, u32), rotation: Rotation) -> Edit {
    match kind {
        PartKind::Floor => Edit::Plate { origin },
        kind => Edit::Place {
            kind,
            origin,
            rotation,
        },
    }
}
