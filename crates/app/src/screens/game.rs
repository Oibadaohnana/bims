//! The game: one world, one clock, and the screen turning the crank on it.
//!
//! The design phase is over by the time any of this runs: the ship is
//! settled, it is docked at the spawn station, and what was left of the pool
//! is in the crew's hands. The crew are the room — the same simulation as
//! the behaviour test room, laid out on the ship — so the crew's panels are
//! `crew.rs`, and what this screen adds is the coordinates: a window point
//! read back through the ship's camera and heading into the room's own
//! units.
//!
//! The world advances in fixed steps and never in stretched ones: a 24x
//! step would move the ship several times its own length and a trip would
//! skip straight past its own braking phase. Speed is more steps, never
//! bigger ones.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};
use bims::room::{HIT_DOOR, HIT_SHIP_DOOR};
use flight::Target;
use physics::{Facing, ResourceId};
use ship::Session;
use ship::game::ViewMode;
use shipdesign::Storage;
use shipdesign::parts::Rotation;
use world::{ShipState, Speed};
use worldgen::Node;

use super::designer::{Net, Order, ShipSession, settings_sheet, trade_rows};
use crate::canvas::{Pointer, canvas_painter, paint_shapes, rect_of, root_ui, zoom_factor};
use crate::crew::{Actions, CLICK_SLOP, Craft, CrewPanels, Tool};
use crate::format::{euros, grouped, spell};
use crate::names::*;
use crate::screens::room::panel_frame;
use crate::shapes::View;
use crate::{Launch, Screen, theme};

/// Ceiling on world steps per frame. It has to be at least `TOP_SPEED * 60
/// / 30`, or the top of the speed range stops being reachable on a display
/// that is keeping up at 30fps and the world quietly runs slower than the
/// button says.
const MAX_STEPS_PER_FRAME: u32 = 64;

/// How many lines of what-just-happened stay on screen.
const LOG_LINES: usize = 4;

/// How near a click has to come to a map icon to count as picking it, in
/// points. Measured on screen rather than in world units: the thing being
/// aimed at is an icon.
const MAP_PICK_SLOP: f32 = 14.0;

/// What the helm is aimed at, for the local player and nobody else.
#[derive(Clone, Copy)]
enum Aim {
    Node(Node),
    Point(f64, f64),
}

impl Aim {
    fn target(self) -> Target {
        match self {
            Aim::Node(Node::Body(id)) => Target::Body(id),
            Aim::Node(Node::Station(id)) => Target::Station(id),
            Aim::Point(x, y) => Target::Point(worldgen::math::dvec2(x, y)),
        }
    }
}

/// An order that wants somebody at the helm, from the strip at the top.
/// The world refuses one from anywhere else, so the screen walks the local
/// player's crew member to the seat first and gives the order the frame it
/// gets there.
#[derive(Clone, Copy)]
enum HelmOrder {
    Fly(Aim),
    Stop,
}

#[derive(Resource)]
pub struct GameScreen {
    net: Net,
    /// The crew's panels: `None` until the world opens, because everything
    /// they read is the room aboard, which does not exist before there is
    /// a world.
    panels: Option<CrewPanels>,
    aimed: Option<Aim>,
    /// An order on its way to the helm: the crew member is walking there,
    /// and it goes through the seam the frame they arrive.
    pending: Option<HelmOrder>,
    /// The order went through: the post at the helm is lifted once the
    /// world has stepped with it, so the crew member goes back to its
    /// errands and the helm is the room's job to fill.
    relieve: bool,
    /// Whether the station's trade window is up.
    trading: bool,
    backlog: f64,
    log: Vec<String>,
    /// Where the pointer is over the canvas, for the readout.
    hover_at: Option<Vec2>,
    /// A marquee under way on the deck: where the press landed.
    marquee_from: Option<Vec2>,
    pan_from: Option<Vec2>,
    sheet: bool,
    size: Vec2,
    /// The speed Space pauses from, for Space to go back to.
    resume: Speed,
}

pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(Screen::Game), open)
            .add_systems(EguiPrimaryContextPass, frame.run_if(in_state(Screen::Game)));
    }
}

fn open(
    mut commands: Commands,
    session: Option<Res<ShipSession>>,
    launch: Res<Launch>,
    window: Single<&Window>,
) {
    let size = Vec2::new(window.width().max(64.0), window.height().max(64.0));
    let (slot, players) = match session {
        Some(session) => (session.0.editor.local, session.0.editor.players),
        None => {
            // No design phase in front of this: the simulation, on the
            // playtest ship. The `test` command is the same somewhere else
            // each time — a random seed, and a dock somebody lives on picked
            // at random across that galaxy.
            let (seed, spawn) = match *launch {
                Launch::Test => {
                    let seed = super::room::rand_seed();
                    let roll = super::room::rand_seed();
                    (seed, ship::session::pick_dock(seed, 0, roll))
                }
                _ => (world::data::DEFAULT_SEED, None),
            };
            let mut session = Session::simulate(seed, 0, spawn, size.x, size.y);
            if crate::dev::at_belt() {
                session.hold_at_belt_for_probe();
            }
            // The `combat` command is the simulation with the dock turned
            // against the crew: the people living there are enemies, and a
            // recruited crew member shoots at any it can see.
            if *launch == Launch::Combat {
                session.make_dock_hostile();
            }
            if crate::dev::fight() {
                session.stage_fight_for_probe();
            }
            let out = (session.editor.local, session.editor.players);
            commands.insert_resource(ShipSession(session));
            out
        }
    };
    commands.insert_resource(GameScreen {
        net: Net { slot, players },
        panels: None,
        aimed: None,
        pending: None,
        relieve: false,
        trading: false,
        backlog: 0.0,
        log: Vec::new(),
        hover_at: None,
        marquee_from: None,
        pan_from: None,
        sheet: false,
        size: Vec2::ZERO,
        resume: Speed::Real,
    });
}

/// What a thing on the map is called. A kind and a number, because a kind
/// is a fixed table and an identity is a number.
fn node_name(session: &Session, node: Node) -> String {
    let kind = session.map_type(node);
    match node {
        Node::Station(id) => format!(
            "{} {id}",
            STATION_KIND_NAMES
                .get(kind as usize)
                .copied()
                .unwrap_or("Station")
        ),
        Node::Body(id) => format!(
            "{} {id}",
            BODY_KIND_NAMES
                .get(kind as usize)
                .copied()
                .unwrap_or("Body")
        ),
    }
}

fn frame(
    mut contexts: EguiContexts,
    mut screen: ResMut<GameScreen>,
    mut session: ResMut<ShipSession>,
    time: Res<Time>,
) -> Result {
    let ctx = contexts.ctx_mut()?.clone();
    let screen = &mut *screen;
    let session = &mut session.0;
    let now = ctx.input(|i| i.time);
    let dt = time.delta_secs().min(super::designer::MAX_FRAME_DT) as f64;
    let mut root = root_ui(&ctx);

    if session.game.is_none() {
        // A world that never opened — a spawn the galaxy has not got.
        egui::CentralPanel::default().show(&mut root, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(ui.available_height() * 0.3);
                ui.label(egui::RichText::new("Nowhere to start").size(28.0).strong());
                ui.label(
                    egui::RichText::new("The galaxy has no station to dock at.")
                        .color(theme::MUTED),
                );
            });
        });
        return Ok(());
    }

    // --- the world, stepped -------------------------------------------------
    let times = session
        .game
        .as_ref()
        .map(|g| g.world.effective_speed().multiplier())
        .unwrap_or(0) as f64;
    screen.backlog += dt * session.steps_per_second() * times;
    let mut steps = 0;
    while screen.backlog >= 1.0 && steps < MAX_STEPS_PER_FRAME {
        session.world_step();
        screen.backlog -= 1.0;
        steps += 1;
    }
    if screen.backlog > MAX_STEPS_PER_FRAME as f64 {
        screen.backlog = 0.0; // gave up catching up
    }
    // What just happened. An event is a thing that happened once, so the
    // list is drained after it is read.
    if let Some(game) = &mut session.game {
        for event in game.events.drain(..) {
            if let Some(line) = event_line(event) {
                screen.log.push(line);
            }
        }
        while screen.log.len() > LOG_LINES {
            screen.log.remove(0);
        }
    }

    // The crew's panels, built once there is a room to read, and rebuilt
    // for whoever is there now: a docking brings the station's residents
    // into the ship's room and leaving takes them out.
    let crew_now = session.room_ref().map(|r| r.crew_count()).unwrap_or(0);
    match &mut screen.panels {
        None => {
            let mut panels = CrewPanels::new(crate::crew::player(), crew_now);
            if let Some(room) = session.room() {
                room.select_group(1);
            }
            panels.begin_frame();
            screen.panels = Some(panels);
        }
        Some(panels) => {
            panels.rebuild_crew(crew_now);
            panels.begin_frame();
        }
    }
    let crew_count = session
        .game
        .as_ref()
        .map(|g| g.world.aboard.crew_count())
        .unwrap_or(0);
    let resident_station = session.resident_station();
    // The room is the crew's; a second crew — another player's, one day —
    // would come after them and be named for where it is from.
    let name = move |who: u32| -> String {
        if who < crew_count {
            crew_name(who)
        } else {
            resident_name(resident_station.unwrap_or(0), who - crew_count)
        }
    };
    let local = screen.net.slot;

    // --- an order on its way to the helm ---------------------------------------
    // The strip at the top walked the crew member to the seat; the order
    // goes through the frame they get there, and the post is lifted once
    // the world has stepped with it — a step later, so they are still at
    // the seat when the command lands.
    let mut orders: Vec<Order> = Vec::new();
    if let Some(game) = &mut session.game {
        if screen.relieve && steps > 0 {
            game.world.stand_down(local);
            screen.relieve = false;
        }
        if let Some(order) = screen.pending
            && game.world.at_the_helm(local)
        {
            orders.push(match order {
                HelmOrder::Fly(aim) => Order::Fly(aim.target()),
                HelmOrder::Stop => Order::Stop,
            });
            if matches!(order, HelmOrder::Fly(_)) {
                screen.aimed = None;
            }
            screen.pending = None;
            screen.relieve = true;
        }
    }

    // --- the canvas ------------------------------------------------------------
    let canvas = rect_of(root.available_rect_before_wrap());
    let size = canvas.size();
    if size != screen.size && size.x > 0.0 && size.y > 0.0 {
        // The first size the canvas is actually seen at is the one the
        // world should have opened on: the whole hull, in view.
        if screen.size == Vec2::ZERO {
            session.fit(size.x, size.y);
        } else {
            session.resize(size.x, size.y);
        }
        screen.size = size;
    }
    let pointer = Pointer::read(&ctx);
    let on_canvas = pointer.on(canvas);
    let here = pointer.pos.map(|p| p - canvas.min);
    let keys = !ctx.egui_wants_keyboard_input() && !screen.sheet;
    let map_up = session
        .game
        .as_ref()
        .is_some_and(|g| g.mode == ViewMode::Map);
    let panels = screen.panels.as_mut().unwrap();

    // The tool in the pointer's hand. Mine only means anything at a
    // mining site, in the ship view: away from one it is put down.
    let (at_site, marked) = session
        .game
        .as_ref()
        .and_then(|g| g.world.site_here())
        .map(|s| (true, s.marked.len()))
        .unwrap_or((false, 0));
    if panels.tool == Some(Tool::Mine) && !at_site {
        panels.tool = None;
    }
    let marking = panels.tool == Some(Tool::Mine) && !map_up;
    if let Some(game) = &mut session.game {
        game.marking = marking;
    }
    // The blueprint in hand — the Build tab's tool — is the ship view's:
    // the painter draws it under the pointer there, and on the map a tile
    // means nothing. The turn it has is kept across a change of part; the
    // world's answer about the tile is kept until the part or the tile
    // moves, so the tool is only set when it changes.
    let building = match panels.tool {
        Some(Tool::Build(kind)) if !map_up => Some(kind),
        _ => None,
    };
    if let Some(game) = &mut session.game
        && game.placing.map(|(kind, _)| kind) != building
    {
        let rotation = game
            .placing
            .map(|(_, rotation)| rotation)
            .unwrap_or(Rotation::R0);
        game.set_placing(building.map(|kind| (kind, rotation)));
    }
    let mut actions = Actions {
        at_site,
        marked,
        reachable: session.room_ref().map(|r| r.rocks_reachable()).unwrap_or(0),
        clear: false,
        overlay: session.game.as_ref().map(|g| g.overlay).unwrap_or_default(),
        crafts: crafts(session),
        keep: Vec::new(),
        at_rest: session.game.as_ref().is_some_and(|g| g.world.at_rest()),
        free: {
            let mut free = [0u32; shipdesign::CARGO_SLOTS];
            if let Some(game) = &session.game {
                for &id in ResourceId::ALL.iter() {
                    free[id as usize] = game.world.free(id);
                }
            }
            free
        },
        sites: sites(session),
        cancel: Vec::new(),
        head_up: session.game.as_ref().is_some_and(|g| g.head_up),
        follow: session.game.as_ref().is_some_and(|g| g.follow),
        docked: session.docked_at().is_some(),
        station: false,
    };

    // Middle drags pan, in either view.
    if let Some(p) = on_canvas
        && pointer.middle_pressed
    {
        screen.pan_from = Some(p);
    }
    if let Some(from) = screen.pan_from {
        match here {
            Some(p) if pointer.middle_down => {
                session.pan(p.x - from.x, p.y - from.y);
                screen.pan_from = Some(p);
            }
            _ => screen.pan_from = None,
        }
    }
    if let Some(p) = on_canvas
        && pointer.scroll != 0.0
    {
        session.zoom(p.x, p.y, zoom_factor(pointer.scroll));
    }

    if map_up {
        screen.hover_at = None;
        if let Some(game) = &mut session.game {
            game.hover = None;
        }
        // Plot a trip to whatever a click on the map landed on — a thing,
        // or the empty space beside it, which is a perfectly good place to
        // go. Aiming is looking; it is Confirm that wants the helm.
        if let Some(p) = on_canvas
            && pointer.primary_pressed
        {
            let game = session.game.as_mut().unwrap();
            screen.aimed = Some(match game.pick(p.x, p.y, MAP_PICK_SLOP) {
                Some(node) => Aim::Node(node),
                None => {
                    let at = game.point_at(p.x, p.y);
                    Aim::Point(at.x, at.y)
                }
            });
        }
    } else {
        // In the ship view the pointer is over the room aboard, and it does
        // what it does on the room's own screen: a left click or a marquee
        // selects a Bim, a right click on the deck sends the one that takes
        // orders there, and a click on a fixture opens its menu. The
        // marquee is drawn on the deck rather than on the glass — it turns
        // with the ship — which is the box the room tests the crew against.
        screen.hover_at = pointer
            .pos
            .filter(|p| canvas.contains(*p))
            .map(|p| p - canvas.min);
        if let Some(game) = &mut session.game {
            game.hover = screen.hover_at.map(|p| game.tile_at(p.x, p.y));
        }
        // With a blueprint in hand the pointer is about laying it out: a
        // click on a tile it would go on sends the site through the seam,
        // one it would not says why in the log — the readout at the top
        // left says it already — and a right-click puts the tool down.
        if let Some(kind) = building {
            if let Some(p) = on_canvas {
                if pointer.primary_pressed
                    && let Some(game) = &mut session.game
                {
                    let (x, y) = game.tile_at(p.x, p.y);
                    match game.ghost_check() {
                        Some(Ok(())) if x >= 0 && y >= 0 => orders.push(Order::Build {
                            kind,
                            x: x as u32,
                            y: y as u32,
                            rotation: game
                                .placing
                                .map(|(_, rotation)| rotation)
                                .unwrap_or(Rotation::R0),
                        }),
                        Some(Err(why)) => screen.log.push(site_refusal_line(why)),
                        _ => {}
                    }
                }
                if pointer.secondary_pressed {
                    panels.tool = None;
                }
            }
        } else if marking {
            // With the Mine tool in hand the pointer is about the rocks and
            // nothing else: a click on one marks it, or unmarks it, through
            // the seam like any order; a right-click puts the tool down.
            // The system's cursor goes and a pick is drawn in its place,
            // below.
            if let Some(p) = on_canvas {
                ctx.set_cursor_icon(egui::CursorIcon::None);
                if pointer.primary_pressed
                    && let Some(game) = &session.game
                {
                    let (x, y) = game.tile_at(p.x, p.y);
                    if game.world.site_here().is_some_and(|s| s.at(x, y).is_some()) {
                        orders.push(Order::Mark { x, y });
                    }
                }
                if pointer.secondary_pressed {
                    panels.tool = None;
                }
            }
        } else if let Some(p) = on_canvas {
            let (rx, ry) = session.room_point(p.x, p.y);
            if pointer.secondary_pressed {
                panels.close_menu();
                if let Some(room) = session.room() {
                    let fixture = room.hit_at(rx, ry);
                    if fixture != 0 {
                        let at = pointer.pos.unwrap();
                        panels.open_menu(fixture, egui::pos2(at.x, at.y));
                    }
                    // A doorway is somewhere to stand as well as something
                    // to work: the door's menu opens *and* the order goes
                    // through, so a right-click on one walks the Bim into
                    // it. Every other fixture keeps the click for its menu.
                    if fixture == 0 || fixture == HIT_SHIP_DOOR || fixture == HIT_DOOR {
                        let code = room.order_move(rx, ry);
                        if let Some(refused) = order_refused(code) {
                            screen.log.push(refused.into());
                        }
                    }
                }
            }
            if pointer.primary_pressed {
                panels.close_menu();
                screen.marquee_from = Some(p);
                if let Some(room) = session.room() {
                    room.drag_begin(rx, ry);
                }
            }
        }
        if let Some(from) = screen.marquee_from {
            match here {
                Some(p) => {
                    let (rx, ry) = session.room_point(p.x, p.y);
                    if let Some(room) = session.room() {
                        room.drag_update(rx, ry);
                        if pointer.primary_released {
                            let fixture = room.drag_end(rx, ry);
                            let moved = (p - from).length() > CLICK_SLOP;
                            if fixture != 0 && !moved {
                                let at = pointer.pos.unwrap();
                                panels.open_menu(fixture, egui::pos2(at.x, at.y));
                            }
                            screen.marquee_from = None;
                        }
                    }
                }
                None => {
                    screen.marquee_from = None;
                    if let Some(room) = session.room() {
                        room.drag_cancel();
                    }
                }
            }
        }
    }

    if keys {
        let mut d = Vec2::ZERO;
        ctx.input(|i| {
            if let Some(game) = &mut session.game {
                if i.key_pressed(egui::Key::M) {
                    game.set_mode(if game.mode == ViewMode::Map {
                        ViewMode::Ship
                    } else {
                        ViewMode::Map
                    });
                }
                if i.key_pressed(egui::Key::N) {
                    game.head_up = !game.head_up;
                }
                if i.key_pressed(egui::Key::F) {
                    let on = !game.follow;
                    game.set_follow(on);
                }
            }
            if !i.modifiers.any() {
                // The speed keys: Space pauses and goes back to what it paused
                // from, the digits pick a speed. Orders, like the buttons.
                if i.key_pressed(egui::Key::Space)
                    && let Some(game) = &session.game
                {
                    let mine = game.requested(screen.net.slot);
                    if mine == Speed::Paused {
                        orders.push(Order::Speed(screen.resume));
                    } else {
                        screen.resume = mine;
                        orders.push(Order::Speed(Speed::Paused));
                    }
                }
                for (key, speed) in [
                    (egui::Key::Num1, Speed::Real),
                    (egui::Key::Num2, Speed::Triple),
                    (egui::Key::Num3, Speed::Ten),
                    (egui::Key::Num4, Speed::Top),
                ] {
                    if i.key_pressed(key) {
                        orders.push(Order::Speed(speed));
                    }
                }
                // C: the crew member you steer, selected and in the middle.
                if i.key_pressed(egui::Key::C) {
                    if let Some(room) = session.room() {
                        room.select_group(1);
                    }
                    if let Some(game) = &mut session.game {
                        game.centre_on_player();
                    }
                }
                // R turns the blueprint in hand; with none, it recruits.
                if i.key_pressed(egui::Key::R) {
                    if building.is_some() {
                        if let Some(game) = &mut session.game {
                            game.rotate_placing();
                        }
                    } else if let Some(room) = session.room() {
                        room.toggle_recruited();
                    }
                }
            }
            if i.key_pressed(egui::Key::Escape) {
                if panels.tool.is_some() {
                    // A tool in hand is put down first, and nothing else
                    // happens.
                    panels.tool = None;
                } else if panels.menu_open() {
                    // A menu, or the trade window, is shut without opening
                    // the sheet.
                    panels.close_menu();
                } else if screen.trading {
                    screen.trading = false;
                } else {
                    screen.aimed = None;
                    screen.sheet = true;
                }
            }
            let step = super::designer::PAN_SPEED * dt as f32;
            if i.key_down(egui::Key::A) {
                d.x += step;
            }
            if i.key_down(egui::Key::D) {
                d.x -= step;
            }
            if i.key_down(egui::Key::W) {
                d.y += step;
            }
            if i.key_down(egui::Key::S) {
                d.y -= step;
            }
        });
        if d != Vec2::ZERO {
            session.pan(d.x, d.y);
        }
    } else if screen.sheet && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        screen.sheet = false;
    }
    if screen.aimed.is_none() {
        session.clear_preview();
    }

    // Everything that changes the ship goes through the seam.
    for order in orders.drain(..) {
        screen.net.order(session, order);
    }

    // --- the left stack: items, the readout, the pointer, the agendas ----------
    let game = session.game.as_ref().unwrap();
    let view_name = match game.mode {
        ViewMode::Ship => "Ship",
        ViewMode::Map => "System map",
    };
    let state = game.world.ship.state.code();
    let phase = if state == 2 {
        if game.world.plan().is_some_and(|p| p.aborting) {
            "Stopping".to_string()
        } else {
            PHASE_NAMES
                .get(ship::world_paint::phase_code(game) as usize)
                .copied()
                .unwrap_or("Under way")
                .to_string()
        }
    } else {
        STATE_NAMES
            .get(state as usize)
            .copied()
            .unwrap_or("Holding")
            .to_string()
    };
    let speed = game.world.trip_state().map(|s| s.speed).unwrap_or(0.0);
    let degrees = (game.world.ship.heading.to_degrees() + 360.0) % 360.0;
    // What the pointer is over, in the ship view: the part under it and the
    // tile, and what the room aboard makes of the same point.
    let rock_under = game.hover.and_then(|(x, y)| {
        let site = game.world.site_here()?;
        Some((site.at(x, y)?, site.is_marked(x, y), x, y))
    });
    // A blueprint in hand names itself and says whether it would go; a
    // site under the pointer names the part and what has reached it.
    let ghost = building.and_then(|kind| Some((kind, game.ghost_answer()?)));
    let site_under = game.hover.and_then(|tile| game.site_at(tile));
    let (hover_thing, hover_on_it) = match screen.hover_at {
        Some(_) if !map_up && ghost.is_some() => {
            let (kind, answer) = ghost.unwrap();
            let tile = game.hover.unwrap_or((0, 0));
            (
                format!("{} · {}, {}", part_name(kind), tile.0, tile.1),
                match answer {
                    Ok(()) => "Click to lay it out".to_string(),
                    Err(why) => site_refusal_line(why),
                },
            )
        }
        Some(_) if !map_up && site_under.is_some() => {
            let site = site_under.unwrap();
            (
                format!("Blueprint · {}", part_name(site.kind)),
                site_progress(site, &game.world.ship.design),
            )
        }
        Some(_) if !map_up && rock_under.is_some() => {
            let (kind, marked, x, y) = rock_under.unwrap();
            let name = ROCK_NAMES
                .get(kind.code() as usize)
                .copied()
                .unwrap_or("Rock");
            (
                format!("{name} · {x}, {y}"),
                if marked { "Marked to be mined" } else { "" }.to_string(),
            )
        }
        Some(p) if !map_up && session.game_tile_inside() => {
            let part = session.game_hovered_part();
            let mut thing = part
                .and_then(|id| session.part_kind(id))
                .map(part_name)
                .unwrap_or("—")
                .to_string();
            let (rx, ry) = session.room_point(p.x, p.y);
            let room = session.room_ref().unwrap();
            let (spot, room_thing, on_it) = CrewPanels::spot_readout(room, rx, ry);
            if !PLAIN_SPOTS.contains(&spot) {
                thing = room_thing;
            }
            let tile = game.hover.unwrap_or((0, 0));
            (format!("{thing} · {}, {}", tile.0, tile.1), on_it)
        }
        _ => (String::new(), String::new()),
    };

    egui::Area::new(egui::Id::new("game-left"))
        .fixed_pos(egui::pos2(canvas.min.x + 10.0, canvas.min.y + 10.0))
        .order(egui::Order::Middle)
        .show(&ctx, |ui| {
            // The name of the game, the clock and the speed, each in a
            // little frame of its own along the top.
            ui.horizontal(|ui| {
                panel_frame().show(ui, |ui| {
                    ui.heading("Bims");
                });
                panel_frame().show(ui, |ui| {
                    clock_panel(ui, session, &screen.net, &mut orders);
                });
                if session.room_ref().is_some_and(|r| r.is_recruited()) {
                    panel_frame().show(ui, |ui| {
                        ui.label(
                            egui::RichText::new("Recruited — orders only").color(theme::ACCENT),
                        );
                    });
                }
            });
            panel_frame().show(ui, |ui| {
                ui.set_max_width(260.0);
                items_panel(ui, session, panels);
            });
            panel_frame().show(ui, |ui| {
                ui.set_max_width(260.0);
                ui.set_min_width(200.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(view_name).strong());
                    ui.label(egui::RichText::new(&phase).color(theme::MUTED));
                });
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(format!("{speed:.1} u/min")).color(theme::MUTED));
                    ui.label(egui::RichText::new(format!("{degrees:.0}°")).color(theme::MUTED));
                });
                if !hover_thing.is_empty() {
                    ui.label(&hover_thing);
                    if !hover_on_it.is_empty() {
                        ui.label(
                            egui::RichText::new(&hover_on_it)
                                .small()
                                .color(theme::MUTED),
                        );
                    }
                }
            });
            if let Some(room) = session.room_ref() {
                let has_agenda = (0..panels.crew_count).any(|w| room.agenda_len(w as usize) > 0);
                if has_agenda {
                    panel_frame().show(ui, |ui| {
                        ui.set_min_width(200.0);
                        panels.agendas(ui, room, &name);
                    });
                }
            }
        });
    for order in orders.drain(..) {
        screen.net.order(session, order);
    }

    // --- the strip across the top: where the ship is, and where it is going ----
    egui::Area::new(egui::Id::new("game-trip"))
        .anchor(
            egui::Align2::CENTER_TOP,
            egui::vec2(
                (canvas.min.x + canvas.max.x) / 2.0 - ctx.viewport_rect().center().x,
                canvas.min.y + 10.0,
            ),
        )
        .order(egui::Order::Middle)
        .show(&ctx, |ui| {
            panel_frame().show(ui, |ui| {
                ui.set_min_width(300.0);
                trip_panel(
                    ui,
                    session,
                    local,
                    map_up,
                    &mut screen.aimed,
                    &mut screen.pending,
                    &mut screen.relieve,
                    &mut screen.log,
                );
            });
        });

    egui::Area::new(egui::Id::new("game-tray"))
        .anchor(
            egui::Align2::LEFT_BOTTOM,
            egui::vec2(canvas.min.x + 10.0, -10.0),
        )
        .order(egui::Order::Middle)
        .show(&ctx, |ui| {
            panel_frame().show(ui, |ui| {
                let ship_tab = match session.room() {
                    Some(room) => panels.tray(ui, room, Some(&mut actions), &name),
                    None => false,
                };
                if ship_tab {
                    ship_panel(ui, session, &screen.net, local, screen.pending.is_some());
                }
            });
        });
    if let Some(game) = &mut session.game {
        game.head_up = actions.head_up;
        if game.follow != actions.follow {
            game.set_follow(actions.follow);
        }
    }
    // The Station button opens the trade window and shuts it again; so
    // does leaving the berth, since there is nobody to trade with then.
    if actions.station {
        screen.trading = !screen.trading;
    }
    if !actions.docked {
        screen.trading = false;
    }
    if screen.trading {
        trade_window(&ctx, session, &mut screen.trading, &mut orders);
    }
    if actions.clear {
        orders.push(Order::ClearMarks);
    }
    for (resource, units) in actions.keep.drain(..) {
        orders.push(Order::Keep { resource, units });
    }
    for site in actions.cancel.drain(..) {
        orders.push(Order::Cancel { site });
    }
    if let Some(game) = &mut session.game {
        game.overlay = actions.overlay;
    }
    for order in orders.drain(..) {
        screen.net.order(session, order);
    }

    // Clear of the left stack on a narrow window, rather than over it.
    let side_x = (canvas.max.x - 270.0).max(canvas.min.x + 290.0);
    egui::Area::new(egui::Id::new("game-side"))
        .fixed_pos(egui::pos2(side_x, canvas.min.y + 10.0))
        .order(egui::Order::Middle)
        .show(&ctx, |ui| {
            if let Some(room) = session.room() {
                let mut drawn = false;
                let frame = panel_frame();
                frame.show(ui, |ui| {
                    ui.set_min_width(240.0);
                    drawn = panels.side(ui, room, &name);
                    if !drawn {
                        ui.label(
                            egui::RichText::new("Click a Bim to look at it.").color(theme::MUTED),
                        );
                    }
                });
            }
        });

    if !screen.log.is_empty() {
        egui::Area::new(egui::Id::new("game-log"))
            .anchor(
                egui::Align2::RIGHT_BOTTOM,
                egui::vec2(
                    -(size.x * 0.0) - 10.0 - (ctx.viewport_rect().max.x - canvas.max.x),
                    -10.0,
                ),
            )
            .order(egui::Order::Middle)
            .show(&ctx, |ui| {
                panel_frame().show(ui, |ui| {
                    for line in &screen.log {
                        ui.label(egui::RichText::new(line).small());
                    }
                });
            });
    }

    if let Some(room) = session.room() {
        panels.menu(&ctx, room, &name);
        panels.inventory_window(&ctx, room, &name);
        panels.end_frame(room);
    }
    if screen.sheet {
        settings_sheet(&ctx, &mut screen.sheet);
    }

    // --- painting ------------------------------------------------------------------
    let view = View {
        scale: session.view_scale(),
        offset: {
            let (x, y) = session.view_offset();
            Vec2::new(x, y)
        },
    };
    let painter = canvas_painter(&ctx, canvas);
    paint_shapes(&painter, canvas, view, session.render());

    // The pick in the pointer's hand, where the system's cursor was.
    if marking && let Some(p) = on_canvas {
        pick_cursor(&painter, egui::pos2(p.x + canvas.min.x, p.y + canvas.min.y));
    }

    // The crew's names, over their heads, where the ship says each Bim
    // landed — the same camera the shapes went through, so a name stays over
    // its head as the ship turns.
    if !map_up {
        for who in 0..crew_count {
            if let Some((x, y)) = session.crew_on_screen(who) {
                let at = view.to_canvas(Vec2::new(x, y)) + canvas.min;
                let at = egui::pos2(at.x, at.y - theme::NAME_LIFT * view.scale);
                let color = if who == local {
                    theme::YOURS
                } else {
                    theme::THEIRS
                };
                theme::name_over(&painter, at, &crew_name(who), color);
            }
        }
        let station = session.resident_station().unwrap_or(0);
        for who in 0..session.resident_count() {
            if let Some((x, y)) = session.resident_on_screen(who) {
                let at = view.to_canvas(Vec2::new(x, y)) + canvas.min;
                let at = egui::pos2(at.x, at.y - theme::NAME_LIFT * view.scale);
                theme::name_over(&painter, at, &resident_name(station, who), theme::THEIRS);
            }
        }
    }
    let _ = now;
    Ok(())
}

/// A pick, drawn at the pointer: the handle running down and to the left
/// of the point, the head across its top, each stroke over a dark one so
/// it reads on the deck and on the void alike.
fn pick_cursor(painter: &egui::Painter, at: egui::Pos2) {
    let handle = [at + egui::vec2(-11.0, 11.0), at + egui::vec2(2.0, -2.0)];
    let head = [at + egui::vec2(-5.0, -7.0), at + egui::vec2(9.0, 5.0)];
    for (width, color) in [
        (5.0, egui::Color32::from_black_alpha(200)),
        (2.5, theme::INK),
    ] {
        painter.line_segment(handle, egui::Stroke::new(width, color));
        painter.line_segment(head, egui::Stroke::new(width + 1.0, color));
    }
    painter.circle_filled(at, 1.5, theme::ACCENT);
}

/// The strip across the top of the canvas: where the ship is, and where it
/// is going. With the map up it is the helm — what is aimed at, the quote
/// for it, and Confirm; with the ship view up it is the trip under way, as
/// a bar and the time left, or the berth. Brake and Abort sit on it either
/// way. The preview is worked out here and is **never** a command: every
/// order here wants the helm, so a press walks the crew member there and
/// the frame reads it through the seam when they arrive.
#[allow(clippy::too_many_arguments)]
fn trip_panel(
    ui: &mut egui::Ui,
    session: &mut Session,
    local: u32,
    map_up: bool,
    aimed: &mut Option<Aim>,
    pending: &mut Option<HelmOrder>,
    relieve: &mut bool,
    log: &mut Vec<String>,
) {
    // Re-quoted every frame while the player is aiming at something. A
    // quote goes stale the moment the ship moves.
    match *aimed {
        None => session.clear_preview(),
        Some(aim) => session.preview(aim.target()),
    }
    let whereabouts = match session.docked_at() {
        Some(station) => format!("Docked · {}", node_name(session, Node::Station(station))),
        None => match session.game.as_ref().unwrap().world.ship.frame.node() {
            Some(node) => format!("Alongside {}", node_name(session, node)),
            None => "Open space".into(),
        },
    };
    let game = session.game.as_ref().unwrap();
    let state = game.world.ship.state.code();
    let aborting = game.world.plan().is_some_and(|p| p.aborting);
    let walking = pending.is_some();
    let mut press: Option<HelmOrder> = None;
    let mut cancel = false;

    if map_up {
        let quoted = match game.preview.as_ref() {
            Some(Err(why)) => {
                ui.label(egui::RichText::new(plan_error(why.code())).color(theme::WARN));
                false
            }
            Some(Ok(p)) => {
                let spare = game
                    .world
                    .ship
                    .fuel_aboard()
                    .saturating_sub(game.world.ship.reserved_fuel);
                let mut rows = vec![
                    ("Going to", describe_aim(session, *aimed)),
                    ("Arrives in", spell(p.minutes)),
                ];
                if p.stopping > 0.0 {
                    rows.push(("Stopping first", spell(p.stopping)));
                }
                rows.push(("Fuel", format!("{} of {spare} spare", p.fuel.ceil())));
                rows.push(("Ends", if p.docks { "Docked" } else { "Holding" }.into()));
                egui::Grid::new("quote")
                    .num_columns(2)
                    .spacing([10.0, 1.0])
                    .show(ui, |ui| {
                        for (label, value) in rows {
                            ui.label(egui::RichText::new(label).small().color(theme::MUTED));
                            ui.label(value);
                            ui.end_row();
                        }
                    });
                true
            }
            None => {
                ui.label(
                    egui::RichText::new(if state == 2 {
                        "Under way. Click somewhere on the map to plot a new trip."
                    } else {
                        "Click somewhere on the map to plot a trip."
                    })
                    .color(theme::MUTED),
                );
                false
            }
        };
        ui.horizontal(|ui| {
            if ui
                .add_enabled(quoted && !walking, egui::Button::new("Confirm"))
                .clicked()
                && let Some(aim) = *aimed
            {
                press = Some(HelmOrder::Fly(aim));
            }
            if ui
                .add_enabled(aimed.is_some(), egui::Button::new("Clear"))
                .clicked()
            {
                *aimed = None;
            }
            brake_buttons(ui, state, aborting, walking, &mut press);
        });
    } else {
        match game.world.trip_progress() {
            Some((done, total)) => {
                let going = match game.world.plan().map(|p| p.target) {
                    Some(Target::Point(_)) | None => "A point in space".to_string(),
                    Some(target) => target
                        .node()
                        .map(|node| node_name(session, node))
                        .unwrap_or_else(|| "A point in space".into()),
                };
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(if aborting { "Stopping" } else { "To" }).strong(),
                    );
                    ui.label(going);
                });
                ui.horizontal(|ui| {
                    theme::bar(ui, 220.0, (done / total.max(1e-9)) as f32, theme::ACCENT);
                    ui.label(
                        egui::RichText::new(format!("{} left", spell(total - done)))
                            .color(theme::MUTED),
                    );
                });
            }
            None => {
                let phase = if (3..=5).contains(&state) {
                    STATE_NAMES
                        .get(state as usize)
                        .copied()
                        .unwrap_or("Under way")
                        .to_string()
                } else {
                    whereabouts.clone()
                };
                ui.label(egui::RichText::new(phase).strong());
            }
        }
        if state >= 2 {
            ui.horizontal(|ui| {
                brake_buttons(ui, state, aborting, walking, &mut press);
            });
        }
    }
    if walking {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(format!("{} is on the way to the helm.", crew_name(local)))
                    .color(theme::ACCENT),
            );
            if ui.button("Cancel").clicked() {
                cancel = true;
            }
        });
    }

    // A press is a walk first. The order waits in `pending` for the crew
    // member to reach the seat; a walk that cannot start — no helm, or no
    // way to it — says so and orders nothing.
    let game = session.game.as_mut().unwrap();
    if let Some(order) = press {
        if game.world.order_to_helm(local) {
            *pending = Some(order);
            *relieve = false;
        } else {
            log.push(format!("{} cannot reach the helm.", crew_name(local)));
        }
    }
    if cancel {
        *pending = None;
        game.world.stand_down(local);
    }
}

/// Brake stops a ship under way — once. Abort calls off a departure while
/// the ship is still casting off or pushing off the berth. The two are the
/// same command at the seam and never both live.
fn brake_buttons(
    ui: &mut egui::Ui,
    state: u32,
    aborting: bool,
    walking: bool,
    press: &mut Option<HelmOrder>,
) {
    if ui
        .add_enabled(
            state == 2 && !aborting && !walking,
            egui::Button::new("Brake"),
        )
        .clicked()
    {
        *press = Some(HelmOrder::Stop);
    }
    if ui
        .add_enabled(
            (state == 3 || state == 4) && !walking,
            egui::Button::new("Abort"),
        )
        .clicked()
    {
        *press = Some(HelmOrder::Stop);
    }
}

/// What the helm is pointed at, and how far off it is. The distance comes
/// from the map rather than from the plan: a plan is a route, and "how far
/// away is it" is a question about the thing.
fn describe_aim(session: &Session, aimed: Option<Aim>) -> String {
    let Some(aim) = aimed else {
        return "Nowhere".into();
    };
    let here = session.game.as_ref().unwrap().world.ship.position();
    match aim {
        Aim::Node(node) => {
            let at = session
                .map_index_of(node)
                .and_then(|i| session.map_position(i))
                .unwrap_or(here);
            let away = ((at.x - here.x).powi(2) + (at.y - here.y).powi(2)).sqrt();
            format!(
                "{} · {} units",
                node_name(session, node),
                grouped(away.round() as u64)
            )
        }
        Aim::Point(x, y) => {
            let away = ((x - here.x).powi(2) + (y - here.y).powi(2)).sqrt();
            format!("A point in space · {} units", grouped(away.round() as u64))
        }
    }
}

/// The day and the clock, with the speed beside them: what you asked for
/// is the button held down, and what the world is running at is the one
/// coloured — a player held at 1x by somebody else needs to be able to see
/// that is what happened, so with more than one player each one's request
/// is listed under.
fn clock_panel(ui: &mut egui::Ui, session: &Session, net: &Net, orders: &mut Vec<Order>) {
    let game = session.game.as_ref().unwrap();
    let minutes = game.world.minutes_into_day();
    let mine = game.requested(net.slot);
    let effective = game.world.effective_speed();
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!(
                "Day {} · {:02}:{:02}",
                game.world.day(),
                (minutes / 60.0).floor() as u32,
                (minutes % 60.0).floor() as u32
            ))
            .strong(),
        );
        ui.add_space(6.0);
        for speed in Speed::ALL {
            let times = speed.multiplier();
            let label = if times == 0 {
                "||".to_string()
            } else {
                format!("{times}×")
            };
            let text = egui::RichText::new(label).color(if speed == effective {
                theme::ACCENT
            } else {
                theme::INK
            });
            if theme::toggle(ui, speed == mine, text).clicked() {
                orders.push(Order::Speed(speed));
            }
        }
    });
    if net.players > 1 {
        ui.horizontal(|ui| {
            for slot in 0..net.players {
                let times = game.requested(slot).multiplier();
                ui.label(
                    egui::RichText::new(format!(
                        "Player {}{} {}",
                        slot + 1,
                        if slot == net.slot { " (you)" } else { "" },
                        if times == 0 {
                            "paused".into()
                        } else {
                            format!("{times}×")
                        }
                    ))
                    .small()
                    .color(theme::MUTED),
                );
            }
        });
    }
}

/// The station's shelf, in a window in the middle of the screen: what it
/// trades, a row a resource, the ones it does not stock dimmed, and the
/// hold's room under. Only while docked — the button that opens it is
/// only there then — and shut by its own cross, by Escape, or by leaving.
fn trade_window(ctx: &egui::Context, session: &Session, open: &mut bool, orders: &mut Vec<Order>) {
    let Some(station) = session.docked_at() else {
        *open = false;
        return;
    };
    let title = node_name(session, Node::Station(station));
    egui::Window::new(title)
        .id(egui::Id::new("trade"))
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .collapsible(false)
        .resizable(false)
        .open(open)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Crew's money")
                        .small()
                        .color(theme::MUTED),
                );
                ui.label(egui::RichText::new(euros(session.remaining())).strong());
            });
            ui.add_space(4.0);
            if let Some((resource, units, buying)) =
                trade_rows(ui, session, true, |s, id| s.cargo(id))
            {
                orders.push(Order::Deal {
                    resource: ResourceId::ALL[resource as usize],
                    units,
                    buying,
                });
            }
        });
}

/// The Ship tab: the helm, and the ship's facts.
fn ship_panel(ui: &mut egui::Ui, session: &Session, net: &Net, local: u32, walking: bool) {
    let game = session.game.as_ref().unwrap();
    theme::heading(ui, "Helm");
    let manned = game.world.at_the_helm(local);
    ui.label(if walking {
        egui::RichText::new(format!("{} is on the way to the helm.", crew_name(local)))
            .color(theme::ACCENT)
    } else if manned {
        egui::RichText::new(format!("{} is at the helm.", crew_name(local))).color(theme::ACCENT)
    } else {
        egui::RichText::new("Nobody of yours is at the helm.").color(theme::MUTED)
    });
    ui.label(
        egui::RichText::new(
            "Open the map (M), click somewhere, and Confirm at the top: a crew member walks to the helm and sets off.",
        )
        .small()
        .color(theme::MUTED),
    );
    facts_panel(ui, session, net);
}

fn facts_panel(ui: &mut egui::Ui, session: &Session, net: &Net) {
    theme::heading(ui, "Ship");
    let game = session.game.as_ref().unwrap();
    let power = game.world.power();
    let batteries = if power.storage > 0.0 {
        format!(
            ", {} of {} stored",
            power.charge.round(),
            power.storage.round()
        )
    } else {
        String::new()
    };
    let power_line = format!(
        "{} drawn of {} made{batteries}{}",
        power.draw.round(),
        power.supply.round(),
        if power.brownout() {
            " — brownout"
        } else {
            ""
        }
    );
    let dosed: Vec<String> = (0..net.players)
        .filter_map(|who| {
            let dose = game.world.health.get(who as usize)?.dose;
            (dose > 0.5).then(|| format!("{} {}", crew_name(who), dose.round()))
        })
        .collect();
    let by = game.world.ship.destination_set_by;
    let at = game.world.ship.position();
    let rows = [
        (
            "Fuel",
            format!(
                "{} aboard, {} held",
                game.world.ship.fuel_aboard(),
                game.world.ship.reserved_fuel
            ),
        ),
        ("Power", power_line),
        (
            "Dose",
            if dosed.is_empty() {
                "none".into()
            } else {
                dosed.join(", ")
            },
        ),
        ("Mass", format!("{:.0}", session.mass())),
        (
            "Acceleration",
            format!("{:.4}", session.acceleration(Facing::Forward)),
        ),
        ("Parts", session.design_ref().parts.len().to_string()),
        ("Exposed tiles", session.editor.exposed().len().to_string()),
        (
            "Position",
            format!(
                "{}, {}",
                grouped(at.x.round().abs() as u64),
                grouped(at.y.round().abs() as u64)
            ),
        ),
        (
            "Found",
            format!("{} in this system", game.world.discovered.len()),
        ),
        (
            "Scanner",
            format!(
                "{} units",
                grouped(game.world.detection_range().round() as u64)
            ),
        ),
        (
            "Route set by",
            by.map(|s| format!("Player {}", s + 1))
                .unwrap_or_else(|| "Nobody".into()),
        ),
        ("Crew", net.players.to_string()),
    ];
    egui::Grid::new("facts")
        .num_columns(2)
        .spacing([10.0, 1.0])
        .show(ui, |ui| {
            for (label, value) in rows {
                ui.label(egui::RichText::new(label).small().color(theme::MUTED));
                ui.label(egui::RichText::new(value).small());
                ui.end_row();
            }
        });
}

/// What is aboard, a row a resource, grouped by where it is stowed. The
/// counts are not all the hold's: the cold store aboard is the room's,
/// stocked off the manifest when the world opens and at every dock, and
/// what is eaten and grown in between never goes back on the manifest —
/// so the two food rows read the room, because that is what the crew can
/// eat. A readout and nothing else: the standing orders for what the
/// benches make are set on the tray's Management tab, beside the cold
/// store's.
fn items_panel(ui: &mut egui::Ui, session: &Session, panels: &mut CrewPanels) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Inventory").strong());
        theme::question_mark(ui, ITEMS_TIP);
    });
    let room = session.room_ref();
    egui::Grid::new("items")
        .num_columns(2)
        .spacing([8.0, 1.0])
        .show(ui, |ui| {
            // The crew's money first: it is what everything under it was
            // bought with, and what the rest of it sells for.
            ui.label("Money");
            ui.label(egui::RichText::new(euros(session.remaining())).color(theme::ACCENT));
            ui.end_row();
            for &class in Storage::ALL.iter() {
                let mut first = true;
                for &id in ResourceId::ALL.iter() {
                    if Session::storage_of(id) != class {
                        continue;
                    }
                    let held = match (id, room) {
                        (ResourceId::Vegetable, Some(r)) => r.store_veg(),
                        (ResourceId::Tofu, Some(r)) => r.store_tofu(),
                        _ => session.cargo(id),
                    };
                    if first {
                        ui.label(
                            egui::RichText::new(
                                STORAGE_NAMES.get(class as usize).copied().unwrap_or(""),
                            )
                            .small()
                            .color(theme::MUTED),
                        );
                        ui.end_row();
                        first = false;
                    }
                    let row = ui.label(egui::RichText::new(resource_name(id)).color(if held > 0 {
                        theme::INK
                    } else {
                        theme::MUTED
                    }));
                    let count =
                        ui.label(egui::RichText::new(held.to_string()).color(if held > 0 {
                            theme::ACCENT
                        } else {
                            theme::MUTED
                        }));
                    if class == Storage::ColdStore {
                        panels.points(&row, bims::room::SPOT_FRIDGE);
                        panels.points(&count, bims::room::SPOT_FRIDGE);
                    }
                    ui.end_row();
                }
                // Then what is made aboard and kept in the same hold: stew, the
                // first thing made rather than bought.
                if class == Storage::ColdStore
                    && let Some(r) = room
                {
                    let held = r.store_stew();
                    let row = ui.label(egui::RichText::new("Stew, ready").color(if held > 0 {
                        theme::INK
                    } else {
                        theme::MUTED
                    }));
                    let count =
                        ui.label(egui::RichText::new(held.to_string()).color(if held > 0 {
                            theme::ACCENT
                        } else {
                            theme::MUTED
                        }));
                    panels.points(&row, bims::room::SPOT_FRIDGE);
                    panels.points(&count, bims::room::SPOT_FRIDGE);
                    ui.end_row();
                }
            }
            // And the plates, which are the galley's rather than any hold's:
            // clean and in the chopping board's drawer, out of the most it
            // holds. The rest are on the table or in the rack.
            if let Some(r) = room {
                ui.label(egui::RichText::new("Drawer").small().color(theme::MUTED));
                ui.end_row();
                let held = r.plates();
                let row = ui.label(egui::RichText::new("Plates").color(if held > 0 {
                    theme::INK
                } else {
                    theme::MUTED
                }));
                let count = ui.label(
                    egui::RichText::new(format!("{held} / {}", r.plate_drawer_capacity())).color(
                        if held > 0 {
                            theme::ACCENT
                        } else {
                            theme::MUTED
                        },
                    ),
                );
                panels.points(&row, bims::room::SPOT_BOARD);
                panels.points(&count, bims::room::SPOT_BOARD);
                ui.end_row();
            }
        });
}

/// What the benches make, a row each for the Management tab: one per
/// resource a recipe outputs, in `ResourceId` order, with what is aboard,
/// where it is kept, the standing order and the most the hold would take.
fn crafts(session: &Session) -> Vec<Craft> {
    let Some(game) = session.game.as_ref() else {
        return Vec::new();
    };
    ResourceId::ALL
        .iter()
        .copied()
        .filter(|&id| shipdesign::RECIPES.iter().any(|r| r.output.0 == id))
        .map(|id| {
            let class = Session::storage_of(id);
            Craft {
                resource: id,
                held: session.cargo(id),
                target: game.world.craft_target(id),
                most: game.world.ship.design.capacity(class),
                kept_in: STORAGE_NAMES.get(class as usize).copied().unwrap_or(""),
                recipe: recipe_lines(id),
            }
        })
        .collect()
}

/// The construction sites laid out, for the Build tab's list, in the order
/// the crew take them.
fn sites(session: &Session) -> Vec<crate::crew::Site> {
    let Some(game) = session.game.as_ref() else {
        return Vec::new();
    };
    let design = &game.world.ship.design;
    game.world
        .builds
        .iter()
        .map(|site| crate::crew::Site {
            id: site.id,
            kind: site.kind,
            at: site.origin,
            progress: site_progress(site, design),
            stocked: site.stocked(design),
        })
        .collect()
}

/// Every recipe that makes `resource`, in words: "2 ore → 1 metal at the
/// smelter, 30 min". Off the table, so the words cannot drift from what the
/// benches do.
fn recipe_lines(resource: ResourceId) -> String {
    let lower = |id: ResourceId| resource_name(id).to_lowercase();
    shipdesign::RECIPES
        .iter()
        .filter(|r| r.output.0 == resource)
        .map(|r| {
            let inputs: Vec<String> = r
                .inputs
                .iter()
                .map(|&(id, units)| format!("{units} {}", lower(id)))
                .collect();
            format!(
                "{} → {} {} at the {}, {} min.",
                inputs.join(" + "),
                r.output.1,
                lower(resource),
                part_name(r.station).to_lowercase(),
                r.minutes
            )
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[allow(dead_code)]
fn unused(_: ShipState) {}
