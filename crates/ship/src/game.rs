//! The game, as the browser sees it: a world, two cameras, and the pointer.
//!
//! Every rule is next door in `crates/world` and `crates/flight`, which render
//! nothing and know nothing about a canvas. Nothing in here decides whether a
//! trip can be flown or what a step does; it asks, it draws the answer, and it
//! passes the player's orders along.
//!
//! # The page drives the clock, and that is deliberate
//!
//! [`Game::step`] advances the world by exactly one step and nothing else
//! decides how many of those happen. `crates/app/src/screens/game.rs` keeps an accumulator, works
//! out how many steps a frame is worth at the effective speed, and calls this
//! that many times. Putting the accumulator in here would mean the wasm had an
//! opinion about real time, which is the one thing it has no way to measure.
//!
//! # Commands are queued, not applied
//!
//! A command from the page is put on a list and handed to the **next** step.
//! That is what makes the stamp `crates/app/src/screens/game.rs` puts on it mean something: a
//! command applies at a step, the same step for everybody, and a transport
//! that arrives late has something to compare against. Applying one the
//! instant a button is pressed would work perfectly and would be impossible to
//! wire a network into later.

use flight::{PlanError, Target, angle};
use shipdesign::parts::{PartKind, Rotation, TILE};
use shipdesign::{Money, ShipDesign};
use world::world::Command;
use world::{Preview, SiteRefusal, Speed, World, WorldEvent};
use worldgen::GalaxyType;
use worldgen::math::{DVec2, dvec2};

use crate::camera::Camera;
use crate::starfield::Starfield;

/// Which picture is being drawn.
///
/// The discriminants cross the wasm boundary. Two of them, and there is no
/// third: a "ship view that is also a map" is how you end up with a map you
/// cannot read and a ship you cannot see.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum ViewMode {
    /// The ship itself, at tile scale, turned to its heading.
    Ship = 0,
    /// The system: what the crew have found, and where the ship is in it.
    Map = 1,
}

/// Furthest in and out the ship view will go. The same three-times-life-size
/// ceiling the design phase has, so the two look like one game.
const SHIP_MIN_SCALE: f32 = 0.02;
const SHIP_MAX_SCALE: f32 = 3.0;

/// And for the map, where the units are whole systems. A hundred million
/// units across a canvas is about `1e-5`, so this brackets it either side by a
/// couple of orders of magnitude.
const MAP_MIN_SCALE: f32 = 1e-8;
const MAP_MAX_SCALE: f32 = 1e-2;

/// How near a body has to be to the passage for the airlock doors to open,
/// in design units: a tile and a half, which is about a body's walk before
/// it reaches the door.
const AIRLOCK_HAIL: f64 = 1.5 * TILE as f64;

/// How much of the way to open or shut the door moves each frame.
const AIRLOCK_EASE: f32 = 0.18;

/// How much of the canvas the whole of the discovered system should fill when
/// the map is first opened.
const MAP_FIT: f32 = 0.8;

/// What the ship view is drawn to show, over and above the ship: the tray's
/// View tab. A view setting like `Game::head_up` — this window's own, read
/// by the painter and by nothing that decides anything.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Overlay {
    /// The ship as it is: the deck, the parts, the crew. The conduit under
    /// the deck is not drawn, because at the game's scale a run through
    /// every powered room is wiring over the picture.
    #[default]
    Plain,
    /// Electricity: the conduit is drawn, and everything that makes, holds
    /// or draws power is rung — lit if it is on a live network, warned if
    /// it is not.
    Electricity,
}

pub struct Game {
    pub world: World,
    pub mode: ViewMode,
    pub ship_view: Camera,
    pub map_view: Camera,
    /// Which player this browser is.
    pub local: u32,
    /// What the last batch of steps threw up, waiting for the page to read it.
    /// Drained rather than kept: an event is a thing that happened once.
    pub events: Vec<WorldEvent>,
    /// Orders waiting for the next step — see the module note.
    pub queued: Vec<Command>,
    /// The last plan the local player asked about. Never a command, and never
    /// shared: two players hovering over different planets is not an argument
    /// about where the ship is going.
    pub preview: Option<Result<Preview, PlanError>>,
    /// What the preview is *of*, so the map can ring it. Set and cleared
    /// with the preview and read by nothing that decides anything.
    pub aimed: Option<Target>,
    /// The design tile under the pointer, in the ship view. Signed: a pointer
    /// off the hull is off it rather than on the nearest edge.
    pub hover: Option<(i32, i32)>,
    /// Head up rather than north up: the ship is drawn the way it was laid
    /// out and the sky, the station alongside and the map turn round it
    /// instead. A view setting and nothing else — the heading is what it is,
    /// and nothing that decides anything reads this. Off by default, because
    /// a fixed sky is what makes a flip legible; see `camera.rs`.
    pub head_up: bool,
    /// Whether the player is marking rocks: the Actions tab's Mine tool is
    /// on, so a rock under the pointer is rung. A tool setting, this
    /// window's own; a mark itself is a command and crosses the seam.
    pub marking: bool,
    /// What the ship view is showing over the ship — the tray's View tab.
    /// A view setting like `head_up`, this window's own.
    pub overlay: Overlay,
    /// The part the player is about to lay out, and which way round: the
    /// Build tab's tool, drawn as a blueprint under the pointer. A tool
    /// setting, this window's own, like `marking`; the site itself is a
    /// command and crosses the seam.
    pub placing: Option<(PartKind, Rotation)>,
    /// Whether the blueprint under the pointer would go, worked out once
    /// per tile it is over rather than every frame — `validate` walks the
    /// whole ship. `(kind, tile, rotation)` it was asked for, and the
    /// answer.
    ghost_check: Option<((PartKind, (i32, i32), Rotation), Result<(), SiteRefusal>)>,
    /// Whether the ship view follows the crew member the player steers, or
    /// has been let go to be dragged anywhere — `Camera::set_loose`. Off by
    /// default — the game opens on the whole ship, free to be dragged, and
    /// `F` or the View panel tethers it; a view setting like `head_up`, this
    /// window's own, and nothing that decides anything reads it.
    pub follow: bool,
    /// Frames drawn. What the exhaust flickers and the running lights blink
    /// off — a picture clock, counted by the render and by nothing that
    /// decides anything. It does not stop at a pause, which is right: a
    /// paused flame still burns.
    pub frame: u32,
    pub stars: Starfield,
    /// How far the mated airlocks stand open, 0 shut to 1 wide. A picture
    /// clock like `frame`: it eases towards open while somebody is at the
    /// door and shut when nobody is, and nothing that decides anything
    /// reads it — the passage is walkable whatever the door looks like.
    pub airlock_ajar: f32,
}

impl Game {
    /// Open the game with the accepted design.
    ///
    /// Returns `None` when the world will not start, which in practice means
    /// a galaxy with nowhere to spawn or a design that does not weigh enough
    /// to be a ship. A cdylib that panicked here would abort, and an abort
    /// tells the player nothing at all.
    pub fn start(
        design: ShipDesign,
        money: Money,
        players: u32,
        local: u32,
        seed: u64,
        galaxy_type: GalaxyType,
        star: u32,
        station: u32,
        width: f32,
        height: f32,
    ) -> Option<Game> {
        let world = World::start(design, money, players, seed, galaxy_type, star, station).ok()?;
        let mut game = Game {
            world,
            mode: ViewMode::Ship,
            ship_view: Camera::new(width, height, 1.0, SHIP_MIN_SCALE, SHIP_MAX_SCALE),
            map_view: Camera::new(width, height, 1e-5, MAP_MIN_SCALE, MAP_MAX_SCALE),
            local: local.min(players.saturating_sub(1)),
            events: Vec::new(),
            queued: Vec::new(),
            preview: None,
            aimed: None,
            hover: None,
            head_up: false,
            follow: false,
            marking: false,
            overlay: Overlay::default(),
            placing: None,
            ghost_check: None,
            frame: 0,
            airlock_ajar: 0.0,
            stars: Starfield::new(seed),
        };
        game.fit_ship();
        game.fit_map();
        game.ship_view.set_loose(true);
        Some(game)
    }

    pub fn resize(&mut self, width: f32, height: f32) {
        self.ship_view.resize(width, height);
        self.map_view.resize(width, height);
    }

    /// The canvas is this big, and the views are to start again from it:
    /// the whole hull in the ship view, everything found in the map. For a
    /// host that only learns the canvas's size after the game has opened —
    /// the desktop lays its panels out around the window it was given —
    /// and wants the opening view the game would have had at that size.
    pub fn fit(&mut self, width: f32, height: f32) {
        self.resize(width, height);
        self.fit_ship();
        self.fit_map();
    }

    /// The camera the page should be painting through.
    pub fn camera(&self) -> &Camera {
        match self.mode {
            ViewMode::Ship => &self.ship_view,
            ViewMode::Map => &self.map_view,
        }
    }

    pub fn camera_mut(&mut self) -> &mut Camera {
        match self.mode {
            ViewMode::Ship => &mut self.ship_view,
            ViewMode::Map => &mut self.map_view,
        }
    }

    /// How far the camera is turned, in the screen's sense — the angle
    /// everything that is *out there* is drawn through, and the one the ship's
    /// own heading is drawn on top of.
    ///
    /// Nothing, north up: the camera never turns and the ship does. Head up,
    /// it is the heading undone, so the ship comes out at `heading +
    /// camera_turn() == 0` — square to the window — and the world turns the
    /// other way by exactly as much. Everything that draws or reads back the
    /// ship goes through [`Game::ship_turn`], everything that draws or reads
    /// back the world goes through this alone, and there is no third thing.
    pub fn camera_turn(&self) -> f64 {
        if self.head_up {
            -self.world.ship.heading
        } else {
            0.0
        }
    }

    /// The angle the ship is drawn through: its heading, less however far the
    /// camera has turned to keep it upright.
    pub fn ship_turn(&self) -> f64 {
        self.world.ship.heading + self.camera_turn()
    }

    /// What is lit this frame: the plan's effort read against the heading.
    /// Nothing while docked or holding.
    pub fn firing(&self) -> crate::hull::Firing {
        match self.world.plan() {
            Some(plan) => crate::hull::Firing::of(
                self.world.effort(),
                self.world.ship.heading,
                plan.direction,
            ),
            None => crate::hull::Firing::NONE,
        }
    }

    /// The ship's airlock while it is mated to a station's: docked, and the
    /// ship has a port. What the painter draws open. `None` otherwise.
    pub fn mated_airlock(&self) -> Option<u32> {
        let station = self.world.ship.state.alongside()?;
        self.world.station(station)?.port()?;
        shipdesign::port(&self.world.ship.design).map(|p| p.part_id)
    }

    /// Put the crew member the player steers in the middle of the ship view:
    /// the camera's focus is where they stand, in the camera's units about
    /// the ship, so the view follows them off the ship and into a station
    /// and can never be panned until they are off the edge. Once a frame,
    /// from `ship_render`. The map is left about the ship, and a view let
    /// go (`follow` off) is left wherever it was dragged.
    pub fn follow_player(&mut self) {
        let who = bims::bim::PLAYER as u32;
        if !self.follow || who >= self.world.aboard.count() {
            return;
        }
        let (x, y) = crate::world_paint::crew_on_screen(self, who);
        self.ship_view.set_focus(x, y);
    }

    /// Stream the sky on by however much world time has passed since the
    /// last frame, at the ship's speed. Once a frame, from `ship_render`;
    /// a picture clock, and nothing that decides anything reads it.
    pub fn stream_sky(&mut self) {
        let velocity = self
            .world
            .trip_state()
            .map(|state| state.velocity)
            .unwrap_or(worldgen::math::DVec2::ZERO);
        self.stars.advance(velocity, self.world.clock_minutes);
    }

    /// Put the crew member the player steers in the middle of the ship view
    /// now, once, whatever the camera is doing: a tethered view has its
    /// shove undone, a loose one is brought back to them and left loose.
    pub fn centre_on_player(&mut self) {
        let who = bims::bim::PLAYER as u32;
        if who >= self.world.aboard.count() {
            return;
        }
        let (x, y) = crate::world_paint::crew_on_screen(self, who);
        self.ship_view.recentre(x, y);
    }

    /// Follow the crew member the player steers, or stop: the ship view is
    /// let loose so a drag takes it anywhere, and tethered again it snaps
    /// back to them on the next frame.
    pub fn set_follow(&mut self, on: bool) {
        self.follow = on;
        self.ship_view.set_loose(!on);
    }

    /// Move the airlock door on one frame: open while anybody in the room
    /// is within [`AIRLOCK_HAIL`] of the passage, shut otherwise, easing
    /// either way so it reads as a door and not a switch. Docked with the
    /// rooms joined, or it stays shut.
    pub fn tick_airlock(&mut self) {
        let want = self.someone_at_the_door();
        let target = if want { 1.0 } else { 0.0 };
        self.airlock_ajar += (target - self.airlock_ajar) * AIRLOCK_EASE;
        if (self.airlock_ajar - target).abs() < 0.005 {
            self.airlock_ajar = target;
        }
    }

    fn someone_at_the_door(&self) -> bool {
        if self.mated_airlock().is_none() || !self.world.aboard.is_joined() {
            return false;
        }
        let Some(port) = shipdesign::port(&self.world.ship.design) else {
            return false;
        };
        // The passage, in the room's units: the ship's door face, plus the
        // ship's offset into the joined room.
        let (fx, fy) = port.face();
        let door = dvec2(fx, fy).add(self.world.aboard.offset);
        (0..self.world.aboard.count()).any(|who| {
            let at = self
                .world
                .aboard
                .position(who)
                .add(self.world.aboard.offset);
            at.distance(door) <= AIRLOCK_HAIL
        })
    }

    /// Switch views. Each camera keeps its own scale between visits: a map
    /// zoomed in on the dock is still zoomed in on the dock when it is
    /// opened again — it is centred on the ship whatever it is doing, so
    /// what the ship has got up to in the meantime is on it anyway.
    pub fn set_mode(&mut self, mode: ViewMode) {
        self.mode = mode;
    }

    /// Start the ship view showing the whole hull.
    fn fit_ship(&mut self) {
        let span = self.world.ship.design.build_area as f32 * TILE as f32;
        let fit = (self.ship_view.width / span).min(self.ship_view.height / span);
        self.ship_view.set_scale(fit * 0.9);
    }

    /// Start the map showing everything the crew have found.
    ///
    /// Measured from the ship, because the ship is what the camera is centred
    /// on — a fit worked out from the star would put the ship off the edge the
    /// moment it flew anywhere.
    fn fit_map(&mut self) {
        let here = self.world.ship.position();
        let mut furthest = self.world.detection_range();
        // The star is always there to be seen, and it is the origin.
        furthest = furthest.max(here.length());
        for &node in &self.world.discovered {
            if let Some(at) = self.world.system.absolute_position(node) {
                furthest = furthest.max(at.distance(here));
            }
        }
        let half = (self.map_view.width.min(self.map_view.height) / 2.0) as f64;
        let scale = (half * MAP_FIT as f64 / furthest.max(1.0)) as f32;
        self.map_view.set_scale(scale);
    }

    // --- the clock ----------------------------------------------------------

    /// One step of the world, with whatever the page has queued.
    ///
    /// Events pile up across a frame's worth of steps and the page drains them
    /// once; a step that produced nothing adds nothing.
    pub fn step(&mut self) {
        let commands = std::mem::take(&mut self.queued);
        let (hash, sites, state) = (
            self.world.design_hash(),
            self.world.builds.len(),
            self.world.ship.state.code(),
        );
        let events = self.world.step(&commands);
        self.events.extend(events);
        // The ship may have changed under a still pointer — a part built, a
        // site laid out, the ship set off — and then the blueprint's answer
        // is asked again. Only then: the answer is a validation of the
        // whole ship, and at 24x this runs many times a frame.
        if hash != self.world.design_hash()
            || sites != self.world.builds.len()
            || state != self.world.ship.state.code()
        {
            self.ghost_check = None;
        }
    }

    /// Queue an order for the next step.
    ///
    /// With one exception, and it is `world`'s rather than this module's: a
    /// speed request is applied straight away, because at a pause there are no
    /// steps for a queued one to land on and the pause could never be lifted.
    /// See [`World::request_speed`].
    pub fn send(&mut self, command: Command) {
        if let Command::SetSpeed { slot, speed } = command {
            self.world.request_speed(slot, speed);
            return;
        }
        self.queued.push(command);
    }

    /// What step a command queued now will apply at. The stamp `crates/app/src/screens/game.rs`
    /// puts on every message, and the thing a transport would compare.
    pub fn next_step(&self) -> u64 {
        self.world.steps + 1
    }

    // --- the pointer ---------------------------------------------------------

    /// A point on the canvas, as a design tile.
    ///
    /// The ship is drawn turned, so this is the turn read backwards: screen
    /// offset from the middle, back through the heading, back into the grid.
    /// It is the same arithmetic `world_paint` uses forwards, which is what
    /// makes a click land on the tile it looks like it landed on at every
    /// heading rather than only at zero.
    pub fn tile_at(&self, x: f32, y: f32) -> (i32, i32) {
        let design = self.design_point_at(x, y);
        (
            (design.x / TILE as f64).floor() as i32,
            (design.y / TILE as f64).floor() as i32,
        )
    }

    /// A point on the canvas, in design world units — the tile arithmetic
    /// above before the floor, and the room's own coordinates aboard, since a
    /// design unit is a room unit (`bims::aboard`). What a click on the deck
    /// is handed to the room as: a marquee corner, an order, a fixture.
    pub fn design_point_at(&self, x: f32, y: f32) -> DVec2 {
        let (vx, vy) = self.ship_view.to_view(x, y);
        // Screen is y-down and the system is y-up, so the screen offset is
        // turned back into system space before it is turned back into the
        // grid. Through the angle the ship was *drawn* at, which is the
        // heading less the camera's turn — head up, that is nothing at all.
        let system = dvec2(vx as f64, -(vy as f64));
        angle::unrotate_design(system, self.ship_turn())
            .add(self.world.ship.dynamics.centre_of_mass)
    }

    /// A point on the canvas, as a position in the system. What a click on the
    /// map means.
    pub fn point_at(&self, x: f32, y: f32) -> DVec2 {
        let (vx, vy) = self.map_view.to_view(x, y);
        // Back through the camera's turn before the y-flip, since the turn
        // was made on screen, after it.
        let (vx, vy) = turned(vx, vy, -self.camera_turn() as f32);
        self.world
            .ship
            .position()
            .add(dvec2(vx as f64, -(vy as f64)))
    }

    /// The discovered node a click on the map is near enough to count as
    /// picking, if any.
    ///
    /// Measured in **screen pixels** rather than in world units, because the
    /// thing being aimed at is an icon: at a map scale where a system fits on
    /// a laptop, a world-unit tolerance is either the whole screen or a
    /// thousandth of a pixel.
    pub fn pick(&self, x: f32, y: f32, slop: f32) -> Option<worldgen::Node> {
        let here = self.world.ship.position();
        let mut best: Option<(f32, worldgen::Node)> = None;
        for &node in &self.world.discovered {
            let Some(at) = self.world.system.absolute_position(node) else {
                continue;
            };
            let offset = at.sub(here);
            // Where the map put it: y flipped, then turned with the camera.
            let (ox, oy) = turned(offset.x as f32, -offset.y as f32, self.camera_turn() as f32);
            let sx = self.map_view.offset_x() + ox * self.map_view.scale();
            let sy = self.map_view.offset_y() + oy * self.map_view.scale();
            let away = ((sx - x).powi(2) + (sy - y).powi(2)).sqrt();
            if away <= slop && best.is_none_or(|(near, _)| away < near) {
                best = Some((away, node));
            }
        }
        best.map(|(_, node)| node)
    }

    /// Work out what a trip would cost, for the local player's panel.
    ///
    /// Asked again every frame while the player is aiming at something, rather
    /// than once when they click. A quote goes stale the instant the ship
    /// moves — and while a trip is already under way most of the bill is
    /// *stopping first*, which shrinks as the ship slows. A number that was
    /// right when it was worked out and is wrong now is worse than no number.
    pub fn preview(&mut self, target: Target) {
        self.preview = Some(self.world.preview(target));
        self.aimed = Some(target);
    }

    pub fn clear_preview(&mut self) {
        self.preview = None;
        self.aimed = None;
    }

    /// Whether the blueprint under the pointer would go where it is, and
    /// why not if not — the world's `can_place_site`, asked once per tile
    /// the pointer crosses and remembered, since it validates the whole
    /// ship. `None` with no tool in hand or the pointer off the hull's
    /// grid. The answer goes stale when the ship changes under a still
    /// pointer, which `step` clears for.
    pub fn ghost_check(&mut self) -> Option<Result<(), SiteRefusal>> {
        let (kind, rotation) = self.placing?;
        let tile = self.hover?;
        let key = (kind, tile, rotation);
        if let Some((asked, answer)) = self.ghost_check
            && asked == key
        {
            return Some(answer);
        }
        let answer = if tile.0 < 0 || tile.1 < 0 {
            Err(SiteRefusal::WontFit(
                shipdesign::EditError::OutOfBounds.code(),
            ))
        } else {
            self.world
                .can_place_site(kind, (tile.0 as u32, tile.1 as u32), rotation)
        };
        self.ghost_check = Some((key, answer));
        Some(answer)
    }

    /// The last answer, if it is about the tile the pointer is on now —
    /// for a readout with no hand to ask with. A frame behind the pointer
    /// at most.
    pub fn ghost_answer(&self) -> Option<Result<(), SiteRefusal>> {
        let (kind, rotation) = self.placing?;
        let tile = self.hover?;
        self.ghost_check
            .as_ref()
            .filter(|(asked, _)| *asked == (kind, tile, rotation))
            .map(|(_, answer)| *answer)
    }

    /// Whether the last answer was that the blueprint would go — for the
    /// painter, which has no hand to ask with; `Session::render` asks
    /// first.
    pub fn ghost_ok(&self) -> bool {
        self.ghost_check
            .as_ref()
            .is_some_and(|(_, answer)| answer.is_ok())
    }

    /// Put the blueprint's tool in hand, or down, and turn it. The check
    /// is forgotten with the tool.
    pub fn set_placing(&mut self, placing: Option<(PartKind, Rotation)>) {
        self.placing = placing;
        self.ghost_check = None;
    }

    pub fn rotate_placing(&mut self) {
        if let Some((kind, rotation)) = self.placing {
            self.set_placing(Some((kind, rotation.next())));
        }
    }

    /// The construction site under `tile`, if one is laid out over it.
    pub fn site_at(&self, tile: (i32, i32)) -> Option<&world::BuildSite> {
        if tile.0 < 0 || tile.1 < 0 {
            return None;
        }
        let tile = (tile.0 as u32, tile.1 as u32);
        self.world.builds.iter().find(|s| s.tiles().contains(&tile))
    }

    /// The speed this player has asked for.
    pub fn requested(&self, slot: u32) -> Speed {
        self.world
            .speed_requests
            .get(slot as usize)
            .copied()
            .unwrap_or(Speed::Paused)
    }
}

/// A screen offset turned about the origin by `angle`, in the screen's own
/// sense — the same matrix `world_paint` turns a tile with and the canvas's
/// `rotate()` applies, so a positive angle is clockwise on a y-down screen.
pub fn turned(x: f32, y: f32, angle: f32) -> (f32, f32) {
    let (s, c) = (angle.sin(), angle.cos());
    (x * c - y * s, x * s + y * c)
}
