//! The front of the game: the start menu, the setup screen, and the lobby.
//!
//! The first screens a player meets, and deliberately separate from the
//! room and the designer: the builder deals in settings and has no `Game`,
//! and the room deals in a `Game` and has no menus. Only the World tab
//! touches the galaxy, through `crates/lobby`.
//!
//! The multiplayer half is surface only. Nothing here opens a socket:
//! [`Net`] is a local stand-in with the shape a transport will have, and it
//! is the one seam to replace. Every part of the UI that will eventually be
//! told something by the network is already told it by `net` instead, so
//! wiring one up means implementing `net` and not touching the screens.
//!
//! What the settings come out as, and the only things that leave this
//! screen, are plain numbers: a count of euros, a tile count, a seed, a
//! type and two ids. Nothing but numbers cross into the simulation.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};
use lobby::{Lobby, NONE};

use crate::canvas::{paint_shapes, rect_of, root_ui};
use crate::format::{euros, roman};
use crate::names::*;
use crate::shapes::View;
use crate::{Screen, theme};

/// What each of the crew brings, in whole euros. It all goes into one pool.
const MONEY: [(&str, u64); 3] = [("Lean", 50_000), ("Standard", 100_000), ("Full", 200_000)];

/// Starting ship, in tiles a side.
const SHIPS: [(&str, u32, &str); 3] = [
    ("Small", 30, "30 × 30"),
    ("Standard", 40, "40 × 40"),
    ("Large", 60, "60 × 60"),
];

/// The shape of the galaxy, by `worldgen::GalaxyType`.
const GALAXIES: [(&str, &str); 4] = [
    ("Two-arm spiral", "Two arms, easy to read"),
    ("Spiral", "Four arms, busier"),
    ("Elliptical", "A squashed cloud"),
    ("Round", "Dense in the middle"),
];

const DEFAULT_MONEY: u64 = 100_000;
const DEFAULT_SHIP: u32 = 40;
const DEFAULT_GALAXY: u32 = 0;

/// Berths in a lobby. Four is a guess at the eventual crew ceiling.
const LOBBY_SLOTS: usize = 4;

/// Room codes are read out loud down a phone line, so the alphabet leaves
/// out the pairs that are heard wrong: O/0, I/1.
const CODE_ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
const CODE_LENGTH: usize = 6;

/// Who you are until there are accounts. The room calls crew 0 James.
const YOU: &str = "James";

/// How long a passing remark stays on screen, in seconds.
const NOTE_SECONDS: f64 = 4.0;

/// A press that moves less than this before it lets go is a click on a
/// star, not a drag of the map.
const DRAG_SLOP: f32 = 4.0;

const LOBBY_SAID: &str = "Nobody can reach this lobby yet.";

/// What a game would be started with. One object, shared by every settings
/// tool on the screen: the solo screen and the lobby cannot disagree about
/// what was picked. `spawn` is `None` until a station has been picked, and
/// its absence is what disables Start.
#[derive(Resource, Clone, Debug)]
pub struct Settings {
    pub money_per_bim: u64,
    pub ship: u32,
    pub seed: u64,
    pub galaxy: u32,
    pub spawn: Option<(u32, u32)>,
    /// How many players, and which slot is this one — the lobby's, or one
    /// and nought.
    pub players: u32,
    pub slot: u32,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            money_per_bim: DEFAULT_MONEY,
            ship: DEFAULT_SHIP,
            seed: crate::screens::room::rand_seed(),
            galaxy: DEFAULT_GALAXY,
            spawn: None,
            players: 1,
            slot: 0,
        }
    }
}

struct Player {
    name: String,
    host: bool,
    you: bool,
}

/// The network that is not there yet. Same shape a real one will have:
/// you ask it for things, and it tells you what happened. Nothing in the
/// screens reaches past it.
#[derive(Default)]
struct Net {
    code: Option<String>,
    host: bool,
    players: Vec<Player>,
}

impl Net {
    /// Open a lobby. Locally this only mints a code and seats you; with a
    /// transport behind it, it is the call that creates the room.
    fn create(&mut self) -> Result<String, String> {
        let code = make_code();
        self.code = Some(code.clone());
        self.host = true;
        self.players = vec![Player {
            name: YOU.into(),
            host: true,
            you: true,
        }];
        Ok(code)
    }

    /// Walk into somebody else's lobby. There is nothing to walk into yet,
    /// and saying so is better than pretending: the field, the button and
    /// the refusal are all real, only the wire is missing.
    fn join(&mut self, code: &str) -> Result<(), String> {
        if !is_code(code) {
            return Err("That is not a room code.".into());
        }
        Err(format!(
            "No way to reach {code} yet — nothing is listening."
        ))
    }

    /// The host changed a setting. With a transport this is the broadcast.
    fn push(&self, _what: &Settings) {}

    fn leave(&mut self) {
        self.code = None;
        self.host = false;
        self.players.clear();
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Setup,
    World,
}

/// A passing remark on a line, and when it goes.
struct Remark {
    text: String,
    warn: bool,
    until: f64,
}

impl Remark {
    fn say(text: impl Into<String>, warn: bool, now: f64) -> Option<Remark> {
        Some(Remark {
            text: text.into(),
            warn,
            until: now + NOTE_SECONDS,
        })
    }

    fn show(slot: &mut Option<Remark>, ui: &mut egui::Ui, now: f64, back: &str) {
        if let Some(r) = slot
            && now > r.until
        {
            *slot = None;
        }
        match slot {
            Some(r) => {
                ui.label(egui::RichText::new(&r.text).color(if r.warn {
                    theme::WARN
                } else {
                    theme::MUTED
                }));
            }
            None => {
                ui.label(egui::RichText::new(back).color(theme::MUTED));
            }
        }
    }
}

#[derive(Resource)]
pub struct BuilderScreen {
    net: Net,
    tab: Tab,
    seed_text: String,
    join_code: String,
    join_note: Option<Remark>,
    world_note: Option<Remark>,
    lobby_said: Option<Remark>,
    /// The galaxy: built once, moved between the two tools. There is one
    /// galaxy, one camera over it and one canvas.
    lobby: Lobby,
    inspected: Option<u32>,
    /// A press on the preview: where it started, and whether it has become
    /// a drag.
    pressed: Option<egui::Pos2>,
    dragging: bool,
    preview_size: Vec2,
    system_list: lobby::draw::DrawList,
    galaxy_list: lobby::draw::DrawList,
}

pub struct BuilderPlugin;

impl Plugin for BuilderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Settings>()
            .add_systems(
                Startup,
                open.run_if(|l: Res<crate::Launch>| *l == crate::Launch::Game),
            )
            .add_systems(
                EguiPrimaryContextPass,
                frame.run_if(|s: Res<State<Screen>>| {
                    matches!(s.get(), Screen::Menu | Screen::Setup | Screen::Lobby)
                }),
            );
    }
}

fn open(mut commands: Commands, settings: Res<Settings>) {
    let lobby = Lobby::new(
        settings.seed,
        ship::session::galaxy_type(settings.galaxy),
        560.0,
        400.0,
    );
    commands.insert_resource(BuilderScreen {
        net: Net::default(),
        tab: Tab::Setup,
        seed_text: settings.seed.to_string(),
        join_code: String::new(),
        join_note: None,
        world_note: None,
        lobby_said: None,
        lobby,
        inspected: None,
        pressed: None,
        dragging: false,
        preview_size: Vec2::new(560.0, 400.0),
        system_list: lobby::draw::DrawList::new(),
        galaxy_list: lobby::draw::DrawList::new(),
    });
}

fn frame(
    mut contexts: EguiContexts,
    mut screen: ResMut<BuilderScreen>,
    mut settings: ResMut<Settings>,
    state: Res<State<Screen>>,
    mut next: ResMut<NextState<Screen>>,
    mut commands: Commands,
    time: Res<Time>,
) -> Result {
    let ctx = contexts.ctx_mut()?.clone();
    let screen = &mut *screen;
    let settings = &mut *settings;
    let now = ctx.input(|i| i.time);
    screen.lobby.advance(time.delta_secs().min(0.1));
    let mut root = root_ui(&ctx);
    let mut go: Option<Screen> = None;
    let mut start = false;

    match state.get() {
        Screen::Menu => {
            egui::CentralPanel::default().show(&mut root, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(ui.available_height() * 0.25);
                    ui.label(egui::RichText::new("Bims").size(40.0).strong());
                    ui.label(
                        egui::RichText::new("A ship, a crew, and whatever they can grow.")
                            .color(theme::MUTED),
                    );
                    ui.add_space(20.0);
                    ui.horizontal(|ui| {
                        ui.add_space((ui.available_width() - 260.0).max(0.0) / 2.0);
                        if theme::big(ui, "Play", true).clicked() {
                            go = Some(Screen::Setup);
                        }
                        if theme::big(ui, "Create lobby", true).clicked() {
                            match screen.net.create() {
                                Ok(_) => go = Some(Screen::Lobby),
                                Err(why) => screen.join_note = Remark::say(why, true, now),
                            }
                        }
                    });
                    ui.add_space(20.0);
                    ui.horizontal(|ui| {
                        ui.add_space((ui.available_width() - 300.0).max(0.0) / 2.0);
                        ui.label(egui::RichText::new("Join with a code").color(theme::MUTED));
                        let field = ui.add(
                            egui::TextEdit::singleline(&mut screen.join_code)
                                .char_limit(CODE_LENGTH)
                                .hint_text("——————")
                                .desired_width(90.0),
                        );
                        let submitted =
                            field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        if ui.button("Join").clicked() || submitted {
                            let typed = screen.join_code.trim().to_uppercase();
                            if let Err(why) = screen.net.join(&typed) {
                                screen.join_note = Remark::say(why, true, now);
                            }
                        }
                    });
                    Remark::show(&mut screen.join_note, ui, now, "");
                });
            });
        }
        Screen::Setup => {
            egui::Panel::top("setup-head").show(&mut root, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("< Back").clicked() {
                        go = Some(Screen::Menu);
                    }
                    ui.heading("Game setup");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(egui::RichText::new("One crew, no lobby").color(theme::MUTED));
                    });
                });
            });
            egui::Panel::bottom("setup-foot").show(&mut root, |ui| {
                ui.horizontal(|ui| {
                    let why = start_refusal(settings);
                    ui.label(egui::RichText::new(why.unwrap_or("")).color(theme::CAUTION));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if theme::big(ui, "Start", why.is_none()).clicked() {
                            start = true;
                        }
                    });
                });
            });
            egui::CentralPanel::default().show(&mut root, |ui| {
                tool(ui, screen, settings, true, now);
            });
        }
        Screen::Lobby => {
            egui::Panel::top("lobby-head").show(&mut root, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("< Leave").clicked() {
                        screen.net.leave();
                        go = Some(Screen::Menu);
                    }
                    ui.heading("Lobby");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Copy").clicked()
                            && let Some(code) = &screen.net.code
                        {
                            ui.ctx().copy_text(code.clone());
                            screen.lobby_said = Remark::say(format!("Copied {code}."), false, now);
                        }
                        ui.label(
                            egui::RichText::new(
                                screen.net.code.clone().unwrap_or_else(|| "——————".into()),
                            )
                            .size(18.0)
                            .strong(),
                        );
                        ui.label(egui::RichText::new("Room code").color(theme::MUTED));
                    });
                });
            });
            egui::Panel::bottom("lobby-foot").show(&mut root, |ui| {
                ui.horizontal(|ui| {
                    Remark::show(&mut screen.lobby_said, ui, now, LOBBY_SAID);
                    let why = start_refusal(settings);
                    ui.label(egui::RichText::new(why.unwrap_or("")).color(theme::CAUTION));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if theme::big(ui, "Start", why.is_none()).clicked() {
                            start = true;
                        }
                    });
                });
            });
            egui::Panel::left("lobby-crew")
                .default_size(220.0)
                .show(&mut root, |ui| {
                    theme::heading(ui, "Crew");
                    for i in 0..LOBBY_SLOTS {
                        ui.horizontal(|ui| match screen.net.players.get(i) {
                            Some(p) => {
                                theme::swatch(ui, theme::ACCENT);
                                ui.label(if p.you {
                                    format!("{} (you)", p.name)
                                } else {
                                    p.name.clone()
                                });
                                ui.label(
                                    egui::RichText::new(if p.host { "Host" } else { "Crew" })
                                        .small()
                                        .color(theme::MUTED),
                                );
                            }
                            None => {
                                theme::swatch(ui, theme::RAISED);
                                ui.label(egui::RichText::new("Open").color(theme::MUTED));
                                ui.label(
                                    egui::RichText::new("Waiting").small().color(theme::MUTED),
                                );
                            }
                        });
                    }
                    ui.label(
                        egui::RichText::new(if screen.net.host {
                            "Read the code out to whoever is joining."
                        } else {
                            "The host decides the settings."
                        })
                        .small()
                        .color(theme::MUTED),
                    );
                });
            let editable = screen.net.host;
            egui::CentralPanel::default().show(&mut root, |ui| {
                tool(ui, screen, settings, editable, now);
            });
        }
        _ => {}
    }

    if start && start_refusal(settings).is_none() {
        // Start hands the game over to the designer. What crosses is the
        // numbers and nothing else: the money each Bim brings, the build
        // area in tiles, how many players there are, which slot you are,
        // the seed, the galaxy type, and the star and station the game
        // starts at.
        settings.players = if screen.net.code.is_some() {
            screen.net.players.len().max(1) as u32
        } else {
            1
        };
        settings.slot = 0;
        commands.insert_resource(crate::screens::designer::Start(settings.clone()));
        go = Some(Screen::Design);
    }
    if let Some(screen) = go {
        next.set(screen);
    }
    Ok(())
}

/// Why Start cannot be pressed, or `None` if it can.
fn start_refusal(settings: &Settings) -> Option<&'static str> {
    if settings.spawn.is_none() {
        return Some("Pick a station to start at on the World tab.");
    }
    None
}

/// The settings tool: one tabbed panel, shown on its own for a solo game
/// and inside the lobby. Both write to the same `Settings`.
fn tool(
    ui: &mut egui::Ui,
    screen: &mut BuilderScreen,
    settings: &mut Settings,
    editable: bool,
    now: f64,
) {
    ui.horizontal(|ui| {
        for (tab, label) in [(Tab::Setup, "Game setup"), (Tab::World, "World")] {
            if theme::toggle(ui, screen.tab == tab, label).clicked() {
                screen.tab = tab;
            }
        }
    });
    ui.separator();
    match screen.tab {
        Tab::Setup => {
            choice_row(
                ui,
                "Money per Bim",
                "What each of you brings; it all goes into one pool",
                editable,
                &MONEY.map(|(label, amount)| (label, euros(amount), amount)),
                &mut settings.money_per_bim,
            );
            choice_row(
                ui,
                "Ship size",
                "Tiles a side",
                editable,
                &SHIPS.map(|(label, tiles, sub)| (label, sub.to_string(), tiles)),
                &mut settings.ship,
            );
            screen.net.push(settings);
        }
        Tab::World => world(ui, screen, settings, editable, now),
    }
}

/// A row of mutually exclusive choices, each one a number written into the
/// setting. A guest in somebody else's lobby watches the settings rather
/// than setting them.
fn choice_row<T: PartialEq + Copy>(
    ui: &mut egui::Ui,
    name: &str,
    note: &str,
    editable: bool,
    options: &[(&str, String, T)],
    value: &mut T,
) {
    ui.add_space(6.0);
    ui.label(egui::RichText::new(name).strong());
    ui.label(egui::RichText::new(note).small().color(theme::MUTED));
    ui.horizontal(|ui| {
        for (label, sub, v) in options {
            let on = *value == *v;
            let text = format!("{label}\n{sub}");
            let response = ui.add_enabled(editable, {
                let b = egui::Button::new(text).min_size(egui::vec2(120.0, 40.0));
                if on { b.fill(theme::RAISED_ON) } else { b }
            });
            if response.clicked() && editable {
                *value = *v;
            }
        }
    });
}

/// The World tab: the seed, the galaxy type, the galaxy itself, and the
/// system beside it. A start is chosen here, not explored: no distances,
/// no travel times, nothing about what a station is like.
fn world(
    ui: &mut egui::Ui,
    screen: &mut BuilderScreen,
    settings: &mut Settings,
    editable: bool,
    now: f64,
) {
    // The header names the start, or says there is none yet.
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Start at").strong());
        match settings.spawn.and_then(|(star, station)| {
            place_name(&mut screen.lobby, screen.inspected, star, station)
        }) {
            Some(name) => ui.label(egui::RichText::new(name).color(theme::ACCENT)),
            None => {
                ui.label(egui::RichText::new("nowhere yet — pick a station").color(theme::MUTED))
            }
        };
    });

    // The seed: a decimal field for a u64.
    ui.add_space(4.0);
    ui.label(egui::RichText::new("Seed").strong());
    ui.label(
        egui::RichText::new("A whole number; the same one is the same galaxy")
            .small()
            .color(theme::MUTED),
    );
    let mut new_seed: Option<u64> = None;
    ui.horizontal(|ui| {
        let field = ui.add_enabled(
            editable,
            egui::TextEdit::singleline(&mut screen.seed_text).desired_width(220.0),
        );
        let submitted = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if ui.add_enabled(editable, egui::Button::new("Set")).clicked() || submitted {
            match parse_seed(&screen.seed_text) {
                Some(seed) => new_seed = Some(seed),
                None => {
                    screen.world_note = Remark::say(
                        "That is not a seed: a whole number, up to twenty digits.",
                        true,
                        now,
                    );
                    screen.seed_text = settings.seed.to_string();
                }
            }
        }
        if ui
            .add_enabled(editable, egui::Button::new("New seed"))
            .clicked()
        {
            new_seed = Some(crate::screens::room::rand_seed());
        }
    });

    // The galaxy type.
    ui.add_space(4.0);
    ui.label(egui::RichText::new("Galaxy").strong());
    ui.label(
        egui::RichText::new("Its shape; a thousand stars either way")
            .small()
            .color(theme::MUTED),
    );
    let mut new_galaxy: Option<u32> = None;
    ui.horizontal(|ui| {
        for (i, (label, sub)) in GALAXIES.iter().enumerate() {
            let on = settings.galaxy == i as u32;
            let response = ui.add_enabled(editable, {
                let b =
                    egui::Button::new(format!("{label}\n{sub}")).min_size(egui::vec2(120.0, 40.0));
                if on { b.fill(theme::RAISED_ON) } else { b }
            });
            if response.clicked() && !on {
                new_galaxy = Some(i as u32);
            }
        }
    });

    // The seed or the type moved: a different galaxy, and nothing chosen
    // in the old one means anything in it.
    let mut changed = false;
    if let Some(seed) = new_seed
        && seed != settings.seed
    {
        settings.seed = seed;
        changed = true;
    }
    if let Some(galaxy) = new_galaxy {
        settings.galaxy = galaxy;
        changed = true;
    }
    if changed {
        screen.seed_text = settings.seed.to_string();
        settings.spawn = None;
        screen.inspected = None;
        screen
            .lobby
            .set_world(settings.seed, ship::session::galaxy_type(settings.galaxy));
        screen.lobby.spawn = None;
        screen.net.push(settings);
    }

    ui.label(
        egui::RichText::new("Drag to pan, scroll to zoom, click a star to look at its system. Dim stars have no station.")
            .small()
            .color(theme::MUTED),
    );

    // The preview on the left, the system on the right.
    ui.add_space(4.0);
    let total = ui.available_size();
    let card_w = 300.0_f32.min(total.x * 0.4);
    let preview_w = (total.x - card_w - 12.0).max(200.0);
    let preview_h = (total.y - 50.0).clamp(200.0, 600.0);
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(preview_w);
            let (rect, response) = ui.allocate_exact_size(
                egui::vec2(preview_w, preview_h),
                egui::Sense::click_and_drag(),
            );
            preview(ui, screen, settings, rect, &response);
            ui.horizontal(|ui| {
                let said = match screen.lobby.hovered {
                    Some(star) => {
                        let s = screen.lobby.galaxy.star(star);
                        s.map(|s| {
                            format!(
                                "{} · class {} · {}",
                                star_name(s.name),
                                STAR_CLASS_NAMES
                                    .get(s.star_class as usize)
                                    .copied()
                                    .unwrap_or("?"),
                                if screen
                                    .lobby
                                    .has_station
                                    .get(star as usize)
                                    .copied()
                                    .unwrap_or(false)
                                {
                                    "has a station"
                                } else {
                                    "no station"
                                }
                            )
                        })
                        .unwrap_or_default()
                    }
                    None => String::new(),
                };
                ui.label(egui::RichText::new(said).small().color(theme::MUTED));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add_enabled(editable, egui::Button::new("Random start"))
                        .clicked()
                    {
                        pick_random_start(screen, settings);
                    }
                });
            });
            Remark::show(&mut screen.world_note, ui, now, "");
        });
        ui.vertical(|ui| {
            ui.set_width(card_w);
            system_card(ui, screen, settings, editable, card_w);
        });
    });
}

/// The galaxy, painted into `rect`: pan by dragging, zoom with the wheel,
/// click a star to open it.
fn preview(
    ui: &egui::Ui,
    screen: &mut BuilderScreen,
    settings: &Settings,
    rect: egui::Rect,
    response: &egui::Response,
) {
    let size = Vec2::new(rect.width(), rect.height());
    if size != screen.preview_size {
        screen.preview_size = size;
        screen.lobby.preview.resize(size.x, size.y);
    }
    let local = |p: egui::Pos2| (p.x - rect.min.x, p.y - rect.min.y);
    if response.drag_started_by(egui::PointerButton::Primary)
        || response.drag_started_by(egui::PointerButton::Middle)
    {
        screen.pressed = response.interact_pointer_pos();
        screen.dragging = response.drag_started_by(egui::PointerButton::Middle);
    }
    if response.dragged() {
        let delta = response.drag_delta();
        let moved = screen
            .pressed
            .zip(response.interact_pointer_pos())
            .is_some_and(|(a, b)| (a - b).length() > DRAG_SLOP);
        if screen.dragging || moved {
            screen.dragging = true;
            screen.lobby.preview.pan(delta.x, delta.y);
        }
    }
    if let Some(p) = response.hover_pos() {
        let (x, y) = local(p);
        screen.lobby.hover(x, y);
        let scroll = response.ctx.input(|i| i.smooth_scroll_delta.y);
        if scroll != 0.0 {
            screen
                .lobby
                .preview
                .zoom(x, y, crate::canvas::zoom_factor(scroll));
            screen.lobby.hover(x, y);
        }
    } else if !response.dragged() {
        screen.lobby.hovered = None;
    }
    let was_click = response.clicked() || (response.drag_stopped() && !screen.dragging);
    if response.drag_stopped() || response.clicked() {
        if was_click && let Some(p) = response.interact_pointer_pos() {
            let (x, y) = local(p);
            screen.lobby.hover(x, y);
            if let Some(star) = screen.lobby.hovered {
                inspect(screen, star);
            }
        }
        screen.pressed = None;
        screen.dragging = false;
    }
    screen.lobby.spawn = settings.spawn;
    screen.lobby.paint(&mut screen.galaxy_list);
    paint_shapes(
        ui.painter(),
        rect_of(rect),
        View::PIXELS,
        screen.galaxy_list.shapes(),
    );
    // The frame round it.
    ui.painter().rect_stroke(
        rect,
        4.0,
        egui::Stroke::new(1.0, theme::LINE),
        egui::StrokeKind::Outside,
    );
}

fn inspect(screen: &mut BuilderScreen, star: u32) {
    screen.lobby.inspect(star);
    screen.inspected = screen.lobby.inspected.as_ref().map(|(id, _)| *id);
}

/// The side panel: the star that is open, its bodies and its stations,
/// with a diagram of the system.
fn system_card(
    ui: &mut egui::Ui,
    screen: &mut BuilderScreen,
    settings: &mut Settings,
    editable: bool,
    width: f32,
) {
    let Some(star) = screen.inspected else {
        ui.label(egui::RichText::new("Pick a star").strong());
        return;
    };
    let base = screen
        .lobby
        .galaxy
        .star(star)
        .map(|s| star_name(s.name))
        .unwrap_or_default();
    let class = screen
        .lobby
        .galaxy
        .star(star)
        .map(|s| {
            STAR_CLASS_NAMES
                .get(s.star_class as usize)
                .copied()
                .unwrap_or("?")
        })
        .unwrap_or("?");
    ui.label(egui::RichText::new(format!("{base} · class {class}")).strong());
    let diagram_h = 180.0;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, diagram_h), egui::Sense::hover());
    screen.lobby.spawn = settings.spawn;
    screen
        .lobby
        .paint_system(rect.width(), rect.height(), &mut screen.system_list);
    paint_shapes(
        ui.painter(),
        rect_of(rect),
        View::PIXELS,
        screen.system_list.shapes(),
    );

    let Some((_, system)) = screen.lobby.inspected.as_ref() else {
        return;
    };
    // The labels on the diagram. Text is the app's — the buffer holds
    // rectangles and ellipses and nothing else — so it goes on after the
    // shapes, where the lobby says each thing landed.
    let painter = ui.painter().with_clip_rect(rect);
    for (i, body) in system.bodies.iter().enumerate() {
        if let Some(&(x, y)) = screen.lobby.placed.bodies.get(i) {
            painter.text(
                rect.min + egui::vec2(x + 9.0, y - 9.0),
                egui::Align2::LEFT_CENTER,
                roman(body.name.part as u32),
                egui::FontId::proportional(11.0),
                theme::MUTED,
            );
        }
    }
    for (i, station) in system.stations.iter().enumerate() {
        if let Some(&(x, y)) = screen.lobby.placed.stations.get(i) {
            painter.text(
                rect.min + egui::vec2(x + 9.0, y + 9.0),
                egui::Align2::LEFT_CENTER,
                STATION_KIND_NAMES
                    .get(station.kind as usize)
                    .copied()
                    .unwrap_or("Station"),
                egui::FontId::proportional(11.0),
                theme::INK,
            );
        }
    }

    let bodies: Vec<(String, &'static str)> = system
        .bodies
        .iter()
        .map(|b| {
            (
                format!("{base} {}", roman(b.name.part as u32)),
                BODY_KIND_NAMES
                    .get(b.kind as usize)
                    .copied()
                    .unwrap_or("Body"),
            )
        })
        .collect();
    let stations: Vec<(String, String)> = system
        .stations
        .iter()
        .map(|s| {
            let parent = match s.parent_body {
                Some(p) if p != NONE => system
                    .body(p)
                    .map(|b| format!("{base} {}", roman(b.name.part as u32)))
                    .unwrap_or_else(|| "deep space".into()),
                _ => "deep space".into(),
            };
            (
                station_name(s.name),
                format!(
                    "{} · {parent}",
                    STATION_KIND_NAMES
                        .get(s.kind as usize)
                        .copied()
                        .unwrap_or("Station")
                ),
            )
        })
        .collect();
    // Whatever is left under the diagram, and the list scrolls in it: the
    // preview beside it decides how tall the tab is.
    let left = ui.available_height().max(80.0);
    egui::ScrollArea::vertical()
        .max_height(left)
        .show(ui, |ui| {
            for (name, kind) in bodies {
                ui.horizontal(|ui| {
                    ui.label(name);
                    ui.label(egui::RichText::new(kind).small().color(theme::MUTED));
                });
            }
            ui.separator();
            if stations.is_empty() {
                ui.label(
                    egui::RichText::new("No station here — nowhere to start from.")
                        .color(theme::MUTED),
                );
            }
            for (i, (name, whereabouts)) in stations.into_iter().enumerate() {
                let on = settings.spawn == Some((star, i as u32));
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new(name).color(if on {
                            theme::ACCENT
                        } else {
                            theme::INK
                        }));
                        ui.label(egui::RichText::new(whereabouts).small().color(theme::MUTED));
                    });
                    let label = if editable {
                        if on { "Starting here" } else { "Start here" }
                    } else {
                        "Suggest"
                    };
                    if ui
                        .add_enabled(!(editable && on), egui::Button::new(label))
                        .clicked()
                    {
                        if editable {
                            settings.spawn = Some((star, i as u32));
                            screen.lobby.spawn = settings.spawn;
                            screen.net.push(settings);
                        } else {
                            // A guest pointing at a station: a ring on the map
                            // for everybody, and a word about who.
                            screen.lobby.ping(star);
                        }
                    }
                });
            }
        });
}

/// A random station among every star that has one.
fn pick_random_start(screen: &mut BuilderScreen, settings: &mut Settings) {
    let with_station: Vec<u32> = (0..screen.lobby.galaxy.stars.len() as u32)
        .filter(|&s| {
            screen
                .lobby
                .has_station
                .get(s as usize)
                .copied()
                .unwrap_or(false)
        })
        .collect();
    if with_station.is_empty() {
        return;
    }
    let roll = crate::screens::room::rand_seed();
    let star = with_station[(roll % with_station.len() as u64) as usize];
    inspect(screen, star);
    let count = screen
        .lobby
        .inspected
        .as_ref()
        .map(|(_, s)| s.stations.len())
        .unwrap_or(0);
    if count == 0 {
        return;
    }
    let station = ((roll >> 32) % count as u64) as u32;
    settings.spawn = Some((star, station));
    screen.lobby.spawn = settings.spawn;
    screen.net.push(settings);
}

/// What a station at a star is called, "Cordell Yard 7 at Tanis-284". Reads
/// the inspected system if it is the right one and opens the other for a
/// moment otherwise — the name is the galaxy's to give, and the screen
/// keeps no copy of any.
fn place_name(
    lobby: &mut Lobby,
    inspected: Option<u32>,
    star: u32,
    station: u32,
) -> Option<String> {
    let name = |lobby: &Lobby| -> Option<String> {
        let (_, system) = lobby.inspected.as_ref()?;
        let s = system.station(station)?;
        let star = lobby.galaxy.star(star)?;
        Some(format!(
            "{} at {}",
            station_name(s.name),
            star_name(star.name)
        ))
    };
    if lobby.inspected.as_ref().map(|(id, _)| *id) == Some(star) {
        return name(lobby);
    }
    lobby.inspect(star);
    let found = name(lobby);
    lobby.inspect(inspected.unwrap_or(NONE));
    found
}

/// What was typed, as a seed — or `None` if it is not one. Every digit and
/// nothing else, and no bigger than a u64 holds.
fn parse_seed(text: &str) -> Option<u64> {
    let trimmed: String = text
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '_')
        .collect();
    if trimmed.is_empty() || !trimmed.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    trimmed.parse().ok()
}

fn make_code() -> String {
    let roll = crate::screens::room::rand_seed();
    (0..CODE_LENGTH)
        .map(|i| CODE_ALPHABET[((roll >> (i * 8)) as usize) % CODE_ALPHABET.len()] as char)
        .collect()
}

fn is_code(text: &str) -> bool {
    text.len() == CODE_LENGTH && text.bytes().all(|b| CODE_ALPHABET.contains(&b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_seed_is_digits_and_nothing_else() {
        assert_eq!(parse_seed("42"), Some(42));
        assert_eq!(parse_seed(" 1 000 "), Some(1000));
        assert_eq!(parse_seed("18446744073709551615"), Some(u64::MAX));
        assert_eq!(parse_seed("18446744073709551616"), None);
        assert_eq!(parse_seed("abc"), None);
        assert_eq!(parse_seed(""), None);
    }

    #[test]
    fn a_room_code_is_six_of_the_readable_letters() {
        let code = make_code();
        assert!(is_code(&code), "{code}");
        assert!(!is_code("O0I1AB"));
        assert!(!is_code("ABCDE"));
    }
}
