//! One sitting at the ship: the design phase, and the game the last Accept
//! turns it into.
//!
//! A [`Session`] is what `crates/app` owns for the whole of `ship`'s life —
//! an [`Editor`] from the first frame, a [`Game`] from the moment everybody
//! has accepted. Almost every question about the ship is one question
//! whichever half it is in — "how much fuel is aboard" is asked of the
//! design being laid out and of the ship that is flying — which is why the
//! two-armed reads below live here rather than in the app: the app should
//! not have to know which phase it is in to ask.
//!
//! Nothing here decides anything. It asks `shipdesign` and `world`.

use flight::Target;
use physics::{Facing, ResourceId};
use shipdesign::parts::{Layer, PartKind, footprint};
use shipdesign::{Money, ShipDesign, TILE, storage, trade_price};
use worldgen::{GalaxyType, Node};

use crate::draw::{Color, DrawList};
use crate::editor::{Editor, Phase};
use crate::game::Game;
use crate::{paint, world_paint};

/// "The lobby did not say." What a star or a station id is when there is
/// none — `u32::MAX`, the same value the lobby uses for nothing, because
/// star 0 and station 0 both exist.
pub const NONE: u32 = u32::MAX;

/// What a design phase opens on. The playtest ship unless asked for an
/// empty grid, because a player who wanted an empty grid can clear one and
/// a player who wanted a ship cannot conjure one.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Preset {
    Empty,
    Playtest,
}

/// World units to a tile side, for the app's readouts. The geometry is all
/// done in here.
pub const TILE_UNITS: u32 = TILE;

pub fn galaxy_type(code: u32) -> GalaxyType {
    GalaxyType::ALL
        .get(code as usize)
        .copied()
        .unwrap_or(GalaxyType::SpiralTwoArm)
}

/// The spawn the lobby gave, or none. Both halves have to be there — a star
/// with no station is no more a spawn than nothing at all.
pub fn spawn_from(star: u32, station: u32) -> Option<(u32, u32)> {
    (star != NONE && station != NONE).then_some((star, station))
}

/// A dock somebody lives on, anywhere in the galaxy the seed and type name:
/// the `roll`-th of them, wrapping. For the `test` command — the simulation
/// somewhere else each time — where the app rolls the number and which dock
/// the roll lands on is this crate's, so the same roll on the same seed is
/// the same place on every machine.
pub fn pick_dock(seed: u64, galaxy: u32, roll: u64) -> Option<(u32, u32)> {
    world::spawn_anywhere(&worldgen::Galaxy::new(seed, galaxy_type(galaxy)), roll)
}

pub struct Session {
    pub editor: Editor,
    /// The game, once there is one. `None` for the whole of the design
    /// phase.
    pub game: Option<Game>,
    /// What the lobby asked for, kept from the first frame until Accept
    /// hands it to the world. A galaxy is a seed and a type; where in it the
    /// game starts is a star and a station, and `None` when the lobby did
    /// not say — which is an error screen, never a different dock.
    pub seed: u64,
    pub galaxy: u32,
    pub spawn: Option<(u32, u32)>,
    list: DrawList,
}

impl Session {
    /// Open a design phase.
    ///
    /// `build_area` is tiles a side and `money_per_bim` is what each of the
    /// crew brings, in whole euros. The pool is `economy::starting_pool` of
    /// that and `players`, worked out in [`Editor::new`] so that this and a
    /// native server arrive at it the same way.
    ///
    /// `seed` and `galaxy` are the world the game will open in, and `spawn`
    /// is where in it. The spawn deliberately has **no default**: `None` is
    /// a screen that says so and a way back to the lobby, never a game
    /// somewhere else. [`Session::spawn_ok`] is how the app finds out.
    #[allow(clippy::too_many_arguments)]
    pub fn design(
        build_area: u32,
        money_per_bim: Money,
        players: u32,
        local_slot: u32,
        seed: u64,
        galaxy: u32,
        spawn: Option<(u32, u32)>,
        preset: Preset,
        width: f32,
        height: f32,
    ) -> Session {
        let mut editor = Editor::new(
            build_area,
            money_per_bim,
            players,
            local_slot,
            width,
            height,
        );
        if preset == Preset::Playtest
            && let Some(given) = shipdesign::fixture::playtest_ship_on(build_area)
        {
            editor.give(given);
        }
        editor.market = spawn.and_then(|(star, station)| {
            worldgen::Galaxy::new(seed, galaxy_type(galaxy))
                .system(star)
                .and_then(|system| system.station(station).map(|s| s.stock))
        });
        Session {
            editor,
            game: None,
            seed,
            galaxy,
            spawn,
            list: DrawList::new(),
        }
    }

    /// Skip the design phase and open the world on the playtest ship: what
    /// the `simulation` command is.
    ///
    /// One player, slot 0, [`shipdesign::fixture::playtest_ship`] already
    /// settled, [`world::data::SIMULATION_MONEY`] in hand. The spawn is the
    /// one given when there is one and the simulation's default when there
    /// is not — the lowest star with a station, which is the one place
    /// [`world::spawn`] is still used. Every other number is fixed on
    /// purpose: a playtest that opened somewhere different each time would
    /// be a playtest of nothing.
    pub fn simulate(
        seed: u64,
        galaxy: u32,
        spawn: Option<(u32, u32)>,
        width: f32,
        height: f32,
    ) -> Session {
        let spawn =
            spawn.or_else(|| world::spawn(&worldgen::Galaxy::new(seed, galaxy_type(galaxy))));
        let design = shipdesign::fixture::playtest_ship();
        let editor = Editor::settled(design.clone(), 1, 0, width, height);
        let game = spawn.and_then(|(star, station)| {
            Game::start(
                design,
                world::data::SIMULATION_MONEY,
                1,
                0,
                seed,
                galaxy_type(galaxy),
                star,
                station,
                width,
                height,
            )
        });
        Session {
            editor,
            game,
            seed,
            galaxy,
            spawn,
            list: DrawList::new(),
        }
    }

    /// Whether the spawn the session was given is a station this galaxy
    /// has: both halves present, the star in the galaxy, the station in its
    /// system. Asked once, before a design phase is shown — a player who
    /// laid out a ship for an hour and then learnt at Accept that there was
    /// nowhere to put it would be right to be cross.
    pub fn spawn_ok(&self) -> bool {
        let Some((star, station)) = self.spawn else {
            return false;
        };
        worldgen::Galaxy::new(self.seed, galaxy_type(self.galaxy))
            .system(star)
            .is_some_and(|system| system.station(station).is_some())
    }

    /// Hold at the first belt of the system with its mining site laid out,
    /// rather than docked — see `World::hold_at_belt_for_probe`. `false`
    /// when there is no belt, or no world.
    pub fn hold_at_belt_for_probe(&mut self) -> bool {
        self.game
            .as_mut()
            .is_some_and(|g| g.world.hold_at_belt_for_probe())
    }

    /// Stage a fight at the dock — see `World::stage_fight_for_probe`.
    pub fn stage_fight_for_probe(&mut self) -> bool {
        self.game
            .as_mut()
            .is_some_and(|g| g.world.stage_fight_for_probe())
    }

    /// Make the station the ship is tied to an enemy's: what the `combat`
    /// command does to the simulation's dock. False with no world, or
    /// away from a berth.
    pub fn make_dock_hostile(&mut self) -> bool {
        let Some(game) = &mut self.game else {
            return false;
        };
        let Some(station) = game.world.ship.state.station() else {
            return false;
        };
        game.world.set_hostile(station, true);
        true
    }

    /// The simulation's spawn for the seed and type the session opened
    /// with: the lowest star with a station, and the lowest station in it.
    pub fn simulation_spawn(&self) -> Option<(u32, u32)> {
        world::spawn(&worldgen::Galaxy::new(self.seed, galaxy_type(self.galaxy)))
    }

    /// Open the world with the accepted design. Called the moment the last
    /// Accept lands and at no other time.
    ///
    /// With no spawn there is no world: the app checked [`Session::spawn_ok`]
    /// at the start and showed the error screen instead of a design phase,
    /// so this is only reached without one if something went round that
    /// check — and then the honest outcome is still no game rather than a
    /// game somewhere else.
    fn start_game(&mut self) {
        if self.game.is_some() {
            return;
        }
        let Some((star, station)) = self.spawn else {
            return;
        };
        let Some(design) = self.editor.finish_design().cloned() else {
            return;
        };
        let money = self.editor.budget.remaining(&self.editor.design);
        self.game = Game::start(
            design,
            money,
            self.editor.players,
            self.editor.local,
            self.seed,
            galaxy_type(self.galaxy),
            star,
            station,
            self.editor.view.width,
            self.editor.view.height,
        );
    }

    /// The live ship: the game's if there is one, the design being laid out
    /// if there is not.
    pub fn design_ref(&self) -> &ShipDesign {
        match &self.game {
            Some(game) => &game.world.ship.design,
            None => &self.editor.design,
        }
    }

    pub fn resize(&mut self, width: f32, height: f32) {
        self.editor.view.resize(width, height);
        if let Some(game) = &mut self.game {
            game.resize(width, height);
        }
    }

    /// Resize, and start the views again from that size — the whole build
    /// area, the whole hull, everything found. What a host does the first
    /// time it knows how big its canvas really is.
    pub fn fit(&mut self, width: f32, height: f32) {
        self.editor.view.fit(width, height);
        if let Some(game) = &mut self.game {
            game.fit(width, height);
        }
    }

    /// Rebuild the shape buffer and hand it back.
    ///
    /// A redraw and **never a step**: the world is advanced by
    /// [`Session::world_step`], which the app calls however many times a
    /// frame is worth. Folding the two together would tie the simulation to
    /// the display's refresh rate, which is the one thing a fixed step
    /// exists to avoid.
    pub fn render(&mut self) -> &[f32] {
        match &mut self.game {
            Some(game) => {
                // The room aboard draws itself once a frame, here, and not
                // once a step: at 24x that is one picture rather than
                // twenty-four.
                game.world.aboard.render();
                // And the station's room, whose people are always in it now:
                // drawn once a frame the same way, or they stand in the
                // picture at their bunks while their names walk about.
                if let Some(residents) = &mut game.world.residents {
                    residents.aboard.render();
                }
                game.frame = game.frame.wrapping_add(1);
                game.tick_airlock();
                game.stream_sky();
                game.follow_player();
                // Whether the blueprint in hand would go where the pointer
                // is, asked before the painter colours it.
                game.ghost_check();
                world_paint::paint(game, &mut self.list);
            }
            None => paint::paint(&self.editor, &mut self.list),
        }
        self.list.shapes()
    }

    // --- the camera -------------------------------------------------------
    //
    // One transform for both halves, so the app has one paint loop rather
    // than two. What changes is what the origin *is*: the corner of the
    // build area during the design phase, and the ship itself once the game
    // has started — see `camera.rs`.

    pub fn view_scale(&self) -> f32 {
        match &self.game {
            Some(game) => game.camera().scale(),
            None => self.editor.view.scale(),
        }
    }

    pub fn view_offset(&self) -> (f32, f32) {
        match &self.game {
            Some(game) => (game.camera().offset_x(), game.camera().offset_y()),
            None => (self.editor.view.offset_x(), self.editor.view.offset_y()),
        }
    }

    /// Shove the view by a screen-pixel delta. Middle-drag and WASD both
    /// come through here.
    pub fn pan(&mut self, dx: f32, dy: f32) {
        match &mut self.game {
            Some(game) => game.camera_mut().pan(dx, dy),
            None => self.editor.view.pan(dx, dy),
        }
    }

    /// Zoom about a point on the canvas by a multiplier.
    pub fn zoom(&mut self, at_x: f32, at_y: f32, factor: f32) {
        match &mut self.game {
            Some(game) => game.camera_mut().zoom(at_x, at_y, factor),
            None => self.editor.view.zoom(at_x, at_y, factor),
        }
    }

    // --- the palette ------------------------------------------------------

    /// Tiles across and down, at the ghost's current rotation — so a
    /// palette row can show what it is about to put down rather than what
    /// it would be upright.
    pub fn part_size(&self, kind: PartKind) -> (u32, u32) {
        footprint(kind, self.editor.ghost)
    }

    /// The colour the part is drawn in. The palette swatches are painted
    /// with these, so a button cannot end up a different colour from the
    /// thing it places.
    pub fn part_color(kind: PartKind) -> Color {
        paint::PART_COLORS[kind as usize]
    }

    /// Whether the pointer is over the build area at all.
    pub fn hover_inside(&self) -> bool {
        self.editor
            .hover
            .is_some_and(|t| self.editor.design.holds(t))
    }

    /// The kind of a part by id, off the **live** ship — the design being
    /// laid out, or the one flying — so the game's readout can name what
    /// the pointer is over the same way the designer's does.
    pub fn part_kind(&self, part_id: u32) -> Option<PartKind> {
        self.design_ref().part(part_id).map(|p| p.kind)
    }

    // --- the money --------------------------------------------------------

    /// What is left of the pool.
    ///
    /// Never negative; `apply` refuses anything that would take it there,
    /// and during the design phase it is **derived from the design** every
    /// time rather than decremented as parts go down.
    ///
    /// Once the game has started it is the world's figure instead — the
    /// same money, carried across at Accept and not converted into
    /// anything, and now spent and earned at stations rather than derived
    /// from a ship.
    pub fn remaining(&self) -> Money {
        match &self.game {
            Some(game) => game.world.money,
            None => self.editor.budget.remaining(&self.editor.design),
        }
    }

    // --- the station's goods, and the hold --------------------------------

    /// Whether the station the ship is at sells `resource`: the design
    /// phase's spawn station, or the one the ship is docked at — and false
    /// anywhere else, since there is nobody to buy from.
    pub fn sold_here(&self, resource: ResourceId) -> bool {
        match &self.game {
            Some(g) => match g.world.ship.state {
                world::ShipState::Docked { station } => g
                    .world
                    .station(station)
                    .is_some_and(|s| s.stock.sells(resource)),
                _ => false,
            },
            None => self.editor.sells(resource),
        }
    }

    pub fn trade_price(resource: ResourceId) -> Money {
        trade_price(resource)
    }

    /// Units of it aboard the **live** ship.
    pub fn cargo(&self, resource: ResourceId) -> u32 {
        self.design_ref().carrying(resource)
    }

    pub fn storage_of(resource: ResourceId) -> shipdesign::Storage {
        storage(resource)
    }

    pub fn storage_capacity(&self, class: shipdesign::Storage) -> u32 {
        self.design_ref().capacity(class)
    }

    pub fn storage_used(&self, class: shipdesign::Storage) -> u32 {
        self.design_ref().stored(class)
    }

    // --- accepting --------------------------------------------------------

    /// Record a player's Accept against a hash. `true` if it was taken.
    ///
    /// Refused when the hash is not the design's — an Accept in flight when
    /// somebody else placed a wall is an Accept for a ship that no longer
    /// exists — and refused while there are errors. The last Accept is what
    /// opens the world. There is no separate "start" call: the design phase
    /// ending and the game beginning are one event, and two ways to do it
    /// would be two things that can disagree about which ship got handed
    /// over.
    pub fn accept(&mut self, slot: u32, hash: u64) -> bool {
        let took = self.editor.accept(slot, hash);
        if self.editor.phase == Phase::Game {
            self.start_game();
        }
        took
    }

    /// Whether the ship can still be changed.
    pub fn designing(&self) -> bool {
        self.editor.phase == Phase::Design
    }

    /// Whether the world is open.
    pub fn playing(&self) -> bool {
        !self.designing() && self.game.is_some()
    }

    // --- what the ship is, in either phase --------------------------------

    /// What the ship weighs, crew and cargo included. `0.0` for a design too
    /// light to be a ship — `physics` refuses one rather than quoting an
    /// infinite acceleration.
    pub fn mass(&self) -> f64 {
        match &self.game {
            Some(game) => game.world.ship.dynamics.mass.get(),
            None => paint::debug_mass(&self.editor.design, self.editor.players),
        }
    }

    /// Acceleration along one of the ship's own axes, in world units per
    /// game minute squared.
    pub fn acceleration(&self, axis: Facing) -> f64 {
        let crew = match &self.game {
            Some(game) => game.world.ship.crew_count,
            None => self.editor.players,
        };
        shipdesign::acceleration(self.design_ref(), crew, axis).unwrap_or(0.0)
    }

    // --- the game ---------------------------------------------------------

    /// Advance the world by exactly one step.
    ///
    /// **The app decides how many.** It keeps an accumulator, works out what
    /// a frame is worth at the effective speed, and calls this that many
    /// times. Putting the accumulator in here would give the simulation an
    /// opinion about real time, which it has no way to measure.
    pub fn world_step(&mut self) {
        if let Some(game) = &mut self.game {
            game.step();
        }
    }

    /// How many steps a second of real time is worth at 1x. A fact about
    /// the world rather than about the window, so it comes from here.
    pub fn steps_per_second(&self) -> f64 {
        self.game
            .as_ref()
            .map(|g| g.world.steps_per_second())
            .unwrap_or(time::MINUTES_PER_SECOND / world::data::STEP_MINUTES)
    }

    /// Where a Bim is, in the ship view's camera units about the ship —
    /// what the view offset and scale turn into a canvas pixel, the same way
    /// the shapes are. `None` for a Bim that is not there.
    pub fn crew_on_screen(&self, who: u32) -> Option<(f32, f32)> {
        let game = self.game.as_ref()?;
        if who >= game.world.aboard.crew_count() {
            return None;
        }
        Some(world_paint::crew_on_screen(game, who))
    }

    /// How many residents are being simulated. Nought away from any
    /// station, and nought at a derelict. Docked or not, they are in the
    /// station's own room.
    pub fn resident_count(&self) -> u32 {
        let Some(game) = &self.game else {
            return 0;
        };
        game.world
            .residents
            .as_ref()
            .map(|r| r.aboard.count())
            .unwrap_or(0)
    }

    /// Which station they live on, or `None` for nobody.
    pub fn resident_station(&self) -> Option<u32> {
        let game = self.game.as_ref()?;
        game.world.residents.as_ref().map(|r| r.station)
    }

    /// Where a resident is, in the ship view's camera units about the ship.
    pub fn resident_on_screen(&self, who: u32) -> Option<(f32, f32)> {
        let game = self.game.as_ref()?;
        if who >= self.resident_count() {
            return None;
        }
        // A name over nobody: a resident the crew cannot see is not drawn,
        // and a name walking about on its own would give them away.
        let drawn = game
            .world
            .residents
            .as_ref()
            .is_some_and(|r| r.aboard.room.body_seen(who as usize));
        if !drawn {
            return None;
        }
        Some(world_paint::resident_on_screen(game, who))
    }

    /// The station the ship is tied to, or `None` for nowhere.
    pub fn docked_at(&self) -> Option<u32> {
        match self.game.as_ref().map(|g| &g.world.ship.state) {
            Some(world::ShipState::Docked { station }) => Some(*station),
            _ => None,
        }
    }

    // --- the map ----------------------------------------------------------
    //
    // Only what the crew have found. Undiscovered things are not in the list
    // at all rather than being in it and hidden.

    pub fn map_node(&self, i: usize) -> Option<Node> {
        self.game.as_ref()?.world.discovered.get(i).copied()
    }

    /// Where the `i`th thing on the map is, or `None` when the crew have not
    /// found anything with that index.
    pub fn map_position(&self, i: usize) -> Option<worldgen::math::DVec2> {
        let game = self.game.as_ref()?;
        game.world.system.absolute_position(self.map_node(i)?)
    }

    /// What sort of thing the `i`th discovered thing is: a
    /// `worldgen::BodyKind` or `StationKind` discriminant, read against the
    /// app's name table for the node's kind.
    pub fn map_type(&self, node: Node) -> u32 {
        let Some(game) = &self.game else { return 0 };
        match node {
            Node::Body(id) => game
                .world
                .system
                .body(id)
                .map(|b| b.kind as u32)
                .unwrap_or(0),
            Node::Station(id) => game
                .world
                .system
                .station(id)
                .map(|s| s.kind as u32)
                .unwrap_or(0),
        }
    }

    /// Where a node sits in the discovered list, if it is there.
    pub fn map_index_of(&self, node: Node) -> Option<usize> {
        self.game
            .as_ref()?
            .world
            .discovered
            .iter()
            .position(|&n| n == node)
    }

    /// Quote a trip to a node or a point. Worked out for the **local player
    /// only** and never a command: two players hovering over different
    /// planets must not be an argument about where the ship is going.
    pub fn preview(&mut self, target: Target) {
        if let Some(game) = &mut self.game {
            game.preview(target);
        }
    }

    pub fn clear_preview(&mut self) {
        if let Some(game) = &mut self.game {
            game.clear_preview();
        }
    }

    // --- the two views ----------------------------------------------------

    /// The part under the pointer in the ship view, or `None` — the top of
    /// the tile, the way the designer's `hovered_part` answers: what is
    /// standing there, else the conduit through it, else the deck, else the
    /// frame.
    pub fn game_hovered_part(&self) -> Option<u32> {
        let game = self.game.as_ref()?;
        let tile = game.hover?;
        let grid = game.world.ship.design.grid();
        [
            Layer::Object,
            Layer::Utility,
            Layer::Floor,
            Layer::Structure,
        ]
        .into_iter()
        .map(|layer| grid.get(layer, tile))
        .find(|&id| id != 0)
    }

    /// Whether the pointer is over the hull at all.
    pub fn game_tile_inside(&self) -> bool {
        match self.game.as_ref().and_then(|g| g.hover.map(|t| (g, t))) {
            Some((game, tile)) => game.world.ship.design.holds(tile),
            None => false,
        }
    }

    // --- the room aboard --------------------------------------------------

    /// The room the world is stepping, for the room's panels to act on.
    pub fn room(&mut self) -> Option<&mut bims::game::Game> {
        self.game.as_mut().map(|g| &mut g.world.aboard.room)
    }

    pub fn room_ref(&self) -> Option<&bims::game::Game> {
        self.game.as_ref().map(|g| &g.world.aboard.room)
    }

    /// A canvas point, in the room's coordinates aboard. Only meaningful
    /// once there is a world; nought before.
    ///
    /// Every room coordinate aboard is a design world unit, and this is the
    /// pointer read back through the ship's camera and heading — the same
    /// arithmetic `Game::tile_at` floors.
    pub fn room_point(&self, x: f32, y: f32) -> (f32, f32) {
        match &self.game {
            Some(g) => {
                let p = g.design_point_at(x, y);
                (
                    (p.x + g.world.aboard.offset.x) as f32,
                    (p.y + g.world.aboard.offset.y) as f32,
                )
            }
            None => (0.0, 0.0),
        }
    }
}

// --- the self check -------------------------------------------------------

/// Every bit set means this build agrees with the pinned constants.
pub const SELF_CHECK_ALL: u32 = 0b11111111;

/// Does *this build* get the same answers the fixtures pin?
///
/// `design_hash` is what an Accept is recorded against, so it has to be
/// identical on every machine in a game and on the native server that will
/// one day be authoritative. The fixtures in `shipdesign::fixture` and
/// `world::fixture` are the written-down answers; `crates/shipdesign/src/tests.rs`
/// and `crates/world/src/tests.rs` check them one at a time, and this checks
/// them all at once, as a bitmask so a failure says which half.
pub fn self_check() -> u32 {
    use shipdesign::fixture::{CREWS, REFERENCE_HASH, REFERENCE_PARTS, reference};

    let mut bits = 0;
    if shipdesign::parts::defs_are_sound() {
        bits |= 1;
    }
    for (i, &crew) in CREWS.iter().enumerate() {
        let design = reference(crew);
        let right = shipdesign::design_hash(&design) == REFERENCE_HASH[i]
            && design.parts.len() as u32 == REFERENCE_PARTS[i];
        if right {
            bits |= 1 << (i + 1);
        }
    }
    if !shipdesign::has_errors(&shipdesign::validate(&reference(4), 4)) {
        bits |= 1 << 3;
    }
    // The draw format, and the money arithmetic: a lone player's pool is
    // their own money plus the bonus, and a crew's is nothing but their own.
    // It is the one sum two machines have to agree on down to the euro.
    let solo = economy::starting_pool(100_000, 1) == Ok(120_000);
    let crew = economy::starting_pool(100_000, 4) == Ok(400_000);
    let none = economy::starting_pool(100_000, 0).is_err();
    if crate::draw::STRIDE == 12 && solo && crew && none {
        bits |= 1 << 4;
    }
    // The reference is carrying what it is meant to carry, and the sealed
    // hull is sealed.
    let design = reference(4);
    let stowed = shipdesign::fixture::REFERENCE_CARGO
        .iter()
        .all(|&(id, units)| design.carrying(id) == units);
    if stowed && shipdesign::exposure(&design).is_empty() {
        bits |= 1 << 5;
    }
    // And the **world**: a fixed scenario, stepped a fixed number of times,
    // checksummed.
    if world::fixture::reference_run() == world::fixture::REFERENCE_CHECKSUM {
        bits |= 1 << 6;
    }
    // And the simulation's ship, for the same reason as the reference.
    let playtest = shipdesign::fixture::playtest_ship();
    if shipdesign::design_hash(&playtest) == shipdesign::fixture::PLAYTEST_HASH
        && playtest.parts.len() as u32 == shipdesign::fixture::PLAYTEST_PARTS
    {
        bits |= 1 << 7;
    }
    bits
}
