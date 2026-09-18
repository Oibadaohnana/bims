//! Bims, on a desk.
//!
//! The one binary. It opens a window, and every frame it asks whichever
//! screen is up to step its simulation, lay out its panels and paint its
//! shapes; the shapes go into one mesh (`canvas.rs`) and the panels are egui.
//! Nothing in here decides anything about the game — the room, the rules,
//! the world and the galaxy are the crates beside this one, and this crate
//! is the window, the pointer and the words.
//!
//! Six things to run, and each is a name rather than a flag:
//!
//! ```text
//! bims               the whole game in order — menu, setup or lobby, world
//!                    and station, ship design, then the world docked where
//!                    you said
//! bims simulation    straight into the world on the playtest ship
//! bims design        straight into the yard, the playtest ship given, docked
//!                    where the simulation docks
//! bims room          the behaviour test room — Bims on a deck
//! bims test          the simulation somewhere else each time — docked at a
//!                    random station somebody lives on, in a random galaxy
//! bims combat        the simulation docked at a hostile station: the people
//!                    living there are enemies, and a recruited crew member
//!                    shoots at any it can see
//! bims --self-check  the pinned constants, checked, and the verdict printed
//! ```

mod canvas;
mod crew;
mod dev;
mod format;
mod names;
mod screens;
mod shapes;
mod theme;

use bevy::prelude::*;
use bevy::window::PresentMode;
use bevy_egui::EguiPlugin;

/// Which of the six things this process is.
#[derive(Resource, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Launch {
    Game,
    Simulation,
    Design,
    Room,
    Test,
    Combat,
}

/// Which screen is up. One at a time, and the whole game is a walk through
/// them in order: the menu, then setup or a lobby, then the designer, then
/// the game the last Accept opens. The room and the simulation start
/// further along.
#[derive(States, Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum Screen {
    #[default]
    Menu,
    Setup,
    Lobby,
    /// Nowhere to start: the lobby named no station, or one the galaxy has
    /// not got. A screen that says so and a way back, never a different
    /// dock.
    Lost,
    Design,
    Game,
    Room,
}

fn usage() -> ! {
    eprintln!("usage: bims [game|simulation|design|room|test|combat|--self-check]");
    std::process::exit(2)
}

fn main() {
    let launch = match std::env::args().nth(1).as_deref() {
        None | Some("game") => Launch::Game,
        Some("simulation") => Launch::Simulation,
        Some("design") => Launch::Design,
        Some("room") => Launch::Room,
        Some("test") => Launch::Test,
        // Accepted as a flag too, since that is how it was first asked for.
        Some("combat") | Some("--combat") => Launch::Combat,
        Some("--self-check") => {
            let bits = ship::session::self_check();
            let all = ship::session::SELF_CHECK_ALL;
            println!("self check: {bits:#010b} of {all:#010b}");
            std::process::exit(if bits == all { 0 } else { 1 });
        }
        Some(_) => usage(),
    };

    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "Bims".into(),
            present_mode: PresentMode::AutoVsync,
            resolution: dev::window_resolution(),
            mode: dev::window_mode(),
            // The panels want their room: the designer's palette and
            // checks, the game's controls, and the floating panels over
            // the deck between them.
            resize_constraints: bevy::window::WindowResizeConstraints {
                min_width: 1100.0,
                min_height: 700.0,
                ..default()
            },
            visible: dev::smoke_frames().is_none(),
            ..default()
        }),
        ..default()
    }))
    .add_plugins(EguiPlugin::default())
    .insert_resource(ClearColor(theme::VOID))
    .insert_resource(launch)
    .init_state::<Screen>()
    .add_plugins((
        canvas::CanvasPlugin,
        dev::DevPlugin,
        theme::ThemePlugin,
        screens::builder::BuilderPlugin,
        screens::designer::DesignerPlugin,
        screens::game::GamePlugin,
        screens::room::RoomPlugin,
    ))
    .add_systems(Startup, open);
    app.run();
}

/// Where to start, by what was asked for. The menu is the default state;
/// the others skip straight to their screen. The yard is handed what the
/// setup screen would have handed it: the defaults, and the simulation's
/// dock for a spawn.
fn open(launch: Res<Launch>, mut commands: Commands, mut next: ResMut<NextState<Screen>>) {
    match *launch {
        Launch::Game => {}
        Launch::Simulation | Launch::Test | Launch::Combat => next.set(Screen::Game),
        Launch::Design => {
            let mut settings = screens::builder::Settings::default();
            settings.seed = world::data::DEFAULT_SEED;
            settings.spawn = ship::session::pick_dock(settings.seed, settings.galaxy, 0);
            commands.insert_resource(screens::designer::Start(settings));
            next.set(Screen::Design);
        }
        Launch::Room => next.set(Screen::Room),
    }
}
