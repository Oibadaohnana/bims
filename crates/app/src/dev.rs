//! Running the game with nobody at the keyboard.
//!
//! `BIMS_SMOKE_FRAMES=n` runs `n` frames and exits, which is how a build is
//! checked to open a window and draw without anybody watching it; add
//! `BIMS_SCREENSHOT=file.png` and the frame thirty before the end is saved
//! there, which is how a change to a screen is *looked at* from a terminal.
//! `BIMS_WINDOW=1400x900` asks for a window size and `BIMS_FULLSCREEN=1`
//! for the whole screen; a tiling desktop ignores the first, so the second
//! is how a screenshot comes out at the size a player would have.
//!
//! `BIMS_POINTER="40:move:600,250;60:click:600,250;90:right:300,400"` moves
//! and clicks the pointer at those frames, in logical pixels from the
//! window's top left — the way a harness drives a screen. Move at least a
//! frame before clicking: egui hit-tests a click against the widgets laid
//! out on the previous frame. `press`, `move`, `release` (and `rpress`,
//! `rrelease` for the right button) across several frames are a drag.
//! `wheel` and `wheelup` at a point are a notch of the wheel, which zooms.
//! `BIMS_KEYS="60:Escape,90:M"` presses keys.
//!
//! `BIMS_AT_BELT=1` opens the simulation holding at a belt, its mining site
//! laid out, instead of docked. `BIMS_FIGHT=1` opens it with a fight staged
//! at the dock: the station hostile, the crew member recruited inside its
//! door and one of its people down the corridor.

use bevy::diagnostic::{DiagnosticsStore, FrameCount, FrameTimeDiagnosticsPlugin};
use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyCode, KeyboardInput};
use bevy::input::mouse::{MouseButton, MouseButtonInput, MouseScrollUnit, MouseWheel};
use bevy::prelude::*;
use bevy::window::{CursorMoved, WindowEvent, WindowResolution};

/// `BIMS_AT_BELT=1` opens the simulation holding at the spawn system's belt
/// with its mining site laid out, rather than docked — how a change to the
/// outside is looked at without flying there first.
pub fn at_belt() -> bool {
    std::env::var("BIMS_AT_BELT").as_deref() == Ok("1")
}

/// `BIMS_FIGHT=1` opens the simulation with the dock made hostile, the
/// crew member recruited just inside the station's door and one of the
/// station's people a few tiles down the corridor — how a fight is looked
/// at without walking the station for one.
pub fn fight() -> bool {
    std::env::var("BIMS_FIGHT").as_deref() == Ok("1")
}

pub fn smoke_frames() -> Option<u32> {
    std::env::var("BIMS_SMOKE_FRAMES")
        .ok()
        .and_then(|v| v.parse().ok())
}

/// `BIMS_FULLSCREEN=1` asks for the whole screen. A tiling desktop gives a
/// window whatever tile it likes and ignores the size it asked for, so this
/// is how a smoke run gets a picture at the size a player would actually
/// have.
pub fn window_mode() -> bevy::window::WindowMode {
    use bevy::window::{MonitorSelection, VideoModeSelection, WindowMode};
    match std::env::var("BIMS_FULLSCREEN").as_deref() {
        Ok("1") => WindowMode::BorderlessFullscreen(MonitorSelection::Current),
        Ok("2") => WindowMode::Fullscreen(MonitorSelection::Current, VideoModeSelection::Current),
        _ => WindowMode::Windowed,
    }
}

pub fn window_resolution() -> WindowResolution {
    let default = WindowResolution::new(1400, 900);
    let Ok(spec) = std::env::var("BIMS_WINDOW") else {
        return default;
    };
    let Some((w, h)) = spec.split_once('x') else {
        return default;
    };
    match (w.parse::<u32>(), h.parse::<u32>()) {
        (Ok(w), Ok(h)) if w > 0 && h > 0 => WindowResolution::new(w, h),
        _ => default,
    }
}

pub struct DevPlugin;

impl Plugin for DevPlugin {
    fn build(&self, app: &mut App) {
        if smoke_frames().is_some() {
            app.add_plugins(FrameTimeDiagnosticsPlugin::default())
                .add_systems(Update, smoke_exit);
        }
        if std::env::var("BIMS_POINTER").is_ok() || std::env::var("BIMS_KEYS").is_ok() {
            app.add_systems(Update, scripted_input);
        }
    }
}

fn smoke_exit(
    mut commands: Commands,
    frames: Res<FrameCount>,
    diagnostics: Res<DiagnosticsStore>,
    window: Single<&Window>,
    mut exit: MessageWriter<AppExit>,
    mut shot_taken: Local<bool>,
) {
    let Some(limit) = smoke_frames() else { return };
    if frames.0 + 30 >= limit
        && !*shot_taken
        && let Ok(path) = std::env::var("BIMS_SCREENSHOT")
    {
        *shot_taken = true;
        use bevy::render::view::screenshot::{Screenshot, save_to_disk};
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(path));
    }
    if frames.0 >= limit {
        // The frame time goes in the line because a screen that draws
        // slowly is a bug no picture shows, and the window size because the
        // desktop decides it and a scripted pointer is relative to it.
        let avg_ms = diagnostics
            .get(&FrameTimeDiagnosticsPlugin::FRAME_TIME)
            .and_then(|d| d.average())
            .unwrap_or(f64::NAN);
        println!(
            "smoke: {} frames, {avg_ms:.1} ms a frame, window {}x{}",
            frames.0,
            window.width(),
            window.height()
        );
        exit.write(AppExit::Success);
    }
}

/// Pointer input goes out as `WindowEvent`s, the stream bevy_egui reads,
/// *and* into `Window::cursor_position`, so a scripted click lands on an
/// egui widget or on the canvas exactly as a real one would.
fn scripted_input(
    frames: Res<FrameCount>,
    mut keyboard: MessageWriter<KeyboardInput>,
    mut window_events: MessageWriter<WindowEvent>,
    mut mouse: MessageWriter<MouseButtonInput>,
    mut wheels: MessageWriter<MouseWheel>,
    mut window: Single<(Entity, &mut Window)>,
) {
    let (window_entity, window) = &mut *window;
    let window_entity = *window_entity;

    let at_this_frame = |item: &str| -> Option<String> {
        let (at, rest) = item.split_once(':')?;
        (at.trim().parse::<u32>() == Ok(frames.0)).then(|| rest.trim().to_string())
    };

    for item in std::env::var("BIMS_KEYS").unwrap_or_default().split(',') {
        let Some(name) = at_this_frame(item) else {
            continue;
        };
        let (key_code, logical) = match name.as_str() {
            "Escape" | "Esc" => (KeyCode::Escape, Key::Escape),
            "Enter" => (KeyCode::Enter, Key::Enter),
            "1" => (KeyCode::Digit1, Key::Character("1".into())),
            "2" => (KeyCode::Digit2, Key::Character("2".into())),
            "3" => (KeyCode::Digit3, Key::Character("3".into())),
            "4" => (KeyCode::Digit4, Key::Character("4".into())),
            other if other.len() == 1 => {
                let ch = other.chars().next().unwrap().to_ascii_lowercase();
                let code = match ch {
                    'a' => KeyCode::KeyA,
                    'd' => KeyCode::KeyD,
                    'f' => KeyCode::KeyF,
                    'm' => KeyCode::KeyM,
                    'n' => KeyCode::KeyN,
                    'r' => KeyCode::KeyR,
                    's' => KeyCode::KeyS,
                    'w' => KeyCode::KeyW,
                    _ => continue,
                };
                (code, Key::Character(ch.to_string().into()))
            }
            _ => continue,
        };
        for state in [ButtonState::Pressed, ButtonState::Released] {
            let input = KeyboardInput {
                key_code,
                logical_key: logical.clone(),
                state,
                text: None,
                repeat: false,
                window: window_entity,
            };
            keyboard.write(input.clone());
            window_events.write(WindowEvent::KeyboardInput(input));
        }
    }

    for item in std::env::var("BIMS_POINTER").unwrap_or_default().split(';') {
        let Some(spec) = at_this_frame(item) else {
            continue;
        };
        let Some((what, xy)) = spec.split_once(':') else {
            continue;
        };
        let Some((x, y)) = xy.split_once(',') else {
            continue;
        };
        let (Ok(x), Ok(y)) = (x.trim().parse::<f32>(), y.trim().parse::<f32>()) else {
            continue;
        };
        let position = Vec2::new(x, y);
        // Wayland refuses to move a cursor it does not hold; the event is
        // what egui reads, so the refusal is nothing to report.
        window.set_cursor_position(Some(position));
        window_events.write(WindowEvent::CursorMoved(CursorMoved {
            window: window_entity,
            position,
            delta: None,
        }));
        // `press`/`release` are the two halves of a click on their own,
        // for a drag: press at one frame, move at the next few, release.
        const CLICK: &[ButtonState] = &[ButtonState::Pressed, ButtonState::Released];
        // `wheel` is one notch of the wheel down at that point, which zooms
        // the canvas out; `wheelup` a notch up.
        if let Some(notch) = match what.trim() {
            "wheel" => Some(-1.0),
            "wheelup" => Some(1.0),
            _ => None,
        } {
            let wheel = MouseWheel {
                unit: MouseScrollUnit::Line,
                x: 0.0,
                y: notch,
                window: window_entity,
                phase: bevy::input::touch::TouchPhase::Moved,
            };
            wheels.write(wheel);
            window_events.write(WindowEvent::MouseWheel(wheel));
            continue;
        }
        let (button, states): (MouseButton, &[ButtonState]) = match what.trim() {
            "move" => continue,
            "click" => (MouseButton::Left, CLICK),
            "right" => (MouseButton::Right, CLICK),
            "middle" => (MouseButton::Middle, CLICK),
            "press" => (MouseButton::Left, &[ButtonState::Pressed]),
            "release" => (MouseButton::Left, &[ButtonState::Released]),
            "rpress" => (MouseButton::Right, &[ButtonState::Pressed]),
            "rrelease" => (MouseButton::Right, &[ButtonState::Released]),
            _ => continue,
        };
        // Both streams: Bevy's own input reads the typed message, and
        // bevy_egui reads the ordered `WindowEvent` batch.
        for &state in states {
            let input = MouseButtonInput {
                button,
                state,
                window: window_entity,
            };
            mouse.write(input);
            window_events.write(WindowEvent::MouseButtonInput(input));
        }
    }
}
