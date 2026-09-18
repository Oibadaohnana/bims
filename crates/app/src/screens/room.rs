//! The behaviour test room: the Bims on a deck, on their own.
//!
//! The room fits itself to the window and the crew's panels float over
//! it. What is here is the deck: the canvas, the pointer over it, the
//! speed, and the status line — the panels are `crew.rs`, shared with the
//! ship, which runs this same room aboard.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};
use bims::game::Game;
use bims::room::{HIT_DOOR, HIT_SHIP_DOOR};

use crate::canvas::{Pointer, canvas_painter, paint_shapes, rect_of, root_ui};
use crate::crew::{CLICK_SLOP, CrewPanels};
use crate::format::{clock_text, span_text};
use crate::names::*;
use crate::shapes::View;
use crate::{Screen, theme};

/// The simulation always advances in steps of this size, whatever the
/// display refresh rate or the speed multiplier. Fixed steps keep the walk
/// cycle, the cooking timers and the collision push-out behaving
/// identically at 1x and 24x.
const SIM_STEP: f32 = 1.0 / 60.0;

/// Ceiling on steps per frame. Without it, a window that was minimised for
/// a minute would try to catch up in one frame and lock up.
const MAX_STEPS_PER_FRAME: u32 = 32;

/// Fastest the simulation will run. At 24x a game day goes by in a minute.
const MAX_SPEED: u32 = 24;

/// Largest wall-clock gap we believe. Anything longer is a stall.
const MAX_FRAME_DT: f32 = 1.0 / 20.0;

/// How long a refusal stays on the status line, in seconds.
const REFUSAL_SECONDS: f64 = 4.0;

/// The bubble over a talking Bim. Sits above the name, so the two do not
/// fight.
const BUBBLE_SIZE: f32 = 12.0;
const BUBBLE_LIFT: f32 = 68.0;

pub struct RoomPlugin;

impl Plugin for RoomPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(Screen::Room), open)
            .add_systems(EguiPrimaryContextPass, frame.run_if(in_state(Screen::Room)));
    }
}

#[derive(Resource)]
pub struct RoomScreen {
    game: Game,
    panels: CrewPanels,
    speed: u32,
    backlog: f32,
    /// Where the pointer is over the deck, in room coordinates.
    hover_at: Option<Vec2>,
    /// A marquee under way: where the press landed, in window points.
    drag_from: Option<egui::Pos2>,
    /// A refused order, and when to stop saying so.
    refusal: Option<(String, f64)>,
    size: Vec2,
}

fn open(mut commands: Commands, window: Single<&Window>) {
    let seed = rand_seed();
    let size = Vec2::new(window.width().max(64.0), window.height().max(64.0));
    let game = Game::new(seed, size.x, size.y);
    let panels = CrewPanels::new(crate::crew::player(), game.crew_count());
    let mut screen = RoomScreen {
        game,
        panels,
        speed: 1,
        backlog: 0.0,
        hover_at: None,
        drag_from: None,
        refusal: None,
        size,
    };
    // Somebody has to be picked to begin with, or the game opens with a
    // blank right-hand side and no hint that clicking a Bim is what fills it.
    screen.game.select_group(1);
    commands.insert_resource(screen);
}

/// A random `u64`, so each run wanders somewhere new.
pub fn rand_seed() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u64(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0),
    );
    h.finish()
}

fn frame(mut contexts: EguiContexts, mut screen: ResMut<RoomScreen>, time: Res<Time>) -> Result {
    let ctx = contexts.ctx_mut()?.clone();
    let screen = &mut *screen;
    let dt = time.delta_secs().min(MAX_FRAME_DT);

    // Speed is applied by running more fixed steps, never by stretching
    // one: a 24x step would move the Bim several times its own body in a
    // frame, and it would walk through the counter.
    screen.backlog += dt * screen.speed as f32;
    let mut steps = 0;
    while screen.backlog >= SIM_STEP && steps < MAX_STEPS_PER_FRAME {
        screen.game.simulate(SIM_STEP);
        screen.backlog -= SIM_STEP;
        steps += 1;
    }
    if screen.backlog > SIM_STEP * MAX_STEPS_PER_FRAME as f32 {
        screen.backlog = 0.0; // gave up catching up
    }

    screen.panels.begin_frame();
    let now = ctx.input(|i| i.time);
    let player = screen.panels.player;
    let name = |who: u32| crew_name(who);

    // --- the header --------------------------------------------------------
    let mut root = root_ui(&ctx);
    egui::Panel::top("room-header").show(&mut root, |ui| {
        ui.horizontal(|ui| {
            ui.heading("Bims");
            let minutes = screen.game.clock_minutes();
            ui.label(format!(
                "Day {} · {}",
                screen.game.clock_day(),
                clock_text(minutes)
            ));
            if screen.game.is_recruited() {
                ui.label(egui::RichText::new("◆ Recruited — orders only").color(theme::ACCENT));
            }
            ui.separator();
            let status = status_line(screen, now, minutes);
            let refusing = matches!(&screen.refusal, Some((_, until)) if now < *until);
            ui.label(egui::RichText::new(status).color(if refusing {
                theme::WARN
            } else {
                theme::MUTED
            }));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(format!("{}x", screen.speed));
                ui.add(egui::Slider::new(&mut screen.speed, 1..=MAX_SPEED).show_value(false));
                theme::asks(ui, "Speed", SPEED_TIP);
            });
        });
    });

    // --- the deck ------------------------------------------------------------
    let canvas = rect_of(root.available_rect_before_wrap());
    let size = canvas.size();
    if size != screen.size && size.x > 0.0 && size.y > 0.0 {
        screen.size = size;
        screen.game.resize(size.x, size.y);
        screen.panels.close_menu();
    }
    let view = View {
        scale: screen.game.view_scale(),
        offset: {
            let o = screen.game.view_offset();
            Vec2::new(o.x, o.y)
        },
    };

    let pointer = Pointer::read(&ctx);
    let on_deck = pointer.on(canvas).map(|p| view.to_world(p));
    let keys = !ctx.egui_wants_keyboard_input();

    // The pointer over the deck, asked about every frame rather than only
    // when it moves: the hob gets lit under a stationary pointer.
    if let Some(p) = pointer.pos.map(|p| view.to_world(p - canvas.min)) {
        if pointer.pos.is_some_and(|p| canvas.contains(p)) {
            screen.hover_at = Some(p);
        } else {
            screen.hover_at = None;
        }
    } else {
        screen.hover_at = None;
    }

    if let Some(p) = on_deck {
        if pointer.secondary_pressed {
            screen.panels.close_menu();
            let fixture = screen.game.hit_at(p.x, p.y);
            if fixture != 0 {
                // Right-clicking a fixture opens its menu.
                screen.panels.open_menu(
                    fixture,
                    egui::pos2(pointer.pos.unwrap().x, pointer.pos.unwrap().y),
                );
            }
            if fixture == 0 || fixture == HIT_DOOR || fixture == HIT_SHIP_DOOR {
                // On bare floor a right-click is an order — and on a door
                // it is both: the menu, and a walk into the doorway, which
                // is somewhere a Bim may stand.
                let code = screen.game.order_move(p.x, p.y);
                if let Some(refused) = order_refused(code) {
                    screen.refusal = Some((refused.into(), now + REFUSAL_SECONDS));
                }
            }
        }
        if pointer.primary_pressed {
            screen.panels.close_menu();
            screen.drag_from = pointer.pos.map(|p| egui::pos2(p.x, p.y));
            screen.game.drag_begin(p.x, p.y);
        }
    }
    if screen.drag_from.is_some() {
        if let Some(p) = pointer.pos.map(|p| view.to_world(p - canvas.min)) {
            screen.game.drag_update(p.x, p.y);
            if pointer.primary_released {
                let from = screen.drag_from.take().unwrap();
                let fixture = screen.game.drag_end(p.x, p.y);
                let here = pointer.pos.unwrap();
                let moved = (egui::pos2(here.x, here.y) - from).length() > CLICK_SLOP;
                // A click that landed on a fixture opens its menu instead of
                // selecting.
                if fixture != 0 && !moved {
                    screen.panels.open_menu(fixture, egui::pos2(here.x, here.y));
                }
            }
        } else if pointer.primary_released || pointer.pos.is_none() {
            screen.drag_from = None;
            screen.game.drag_cancel();
        }
    }

    if keys {
        ctx.input(|i| {
            if i.key_pressed(egui::Key::C) {
                screen.game.select_group(1);
            }
            if i.key_pressed(egui::Key::R) {
                screen.game.toggle_recruited();
            }
            if i.key_pressed(egui::Key::Escape) {
                if screen.panels.menu_open() {
                    screen.panels.close_menu();
                } else {
                    screen.game.clear_selection();
                }
            }
        });
    }

    // --- the panels, floating over the deck ----------------------------------
    let left = egui::pos2(canvas.min.x + 10.0, canvas.min.y + 10.0);
    egui::Area::new(egui::Id::new("room-left"))
        .fixed_pos(left)
        .order(egui::Order::Middle)
        .show(&ctx, |ui| {
            // What the pointer is over.
            let (thing, on_it) = match screen.hover_at {
                Some(p) => {
                    let (_, thing, on_it) = CrewPanels::spot_readout(&screen.game, p.x, p.y);
                    (thing, on_it)
                }
                None => ("—".into(), String::new()),
            };
            panel_frame().show(ui, |ui| {
                ui.set_min_width(200.0);
                ui.label(thing);
                if !on_it.is_empty() {
                    ui.label(egui::RichText::new(on_it).small().color(theme::MUTED));
                }
            });
            let mut drawn = false;
            let frame = panel_frame();
            let response = frame.show(ui, |ui| {
                ui.set_min_width(200.0);
                drawn = screen.panels.agendas(ui, &screen.game, &name);
            });
            if !drawn {
                // Nothing on: the box goes rather than sitting there empty.
                let _ = response;
            }
        });

    egui::Area::new(egui::Id::new("room-tray"))
        .anchor(egui::Align2::LEFT_BOTTOM, egui::vec2(10.0, -10.0))
        .order(egui::Order::Middle)
        .show(&ctx, |ui| {
            panel_frame().show(ui, |ui| {
                // No ship under the room, so no Ship tab to draw under the tabs.
                let _ = screen.panels.tray(ui, &mut screen.game, None, &name);
            });
        });

    egui::Area::new(egui::Id::new("room-side"))
        .anchor(
            egui::Align2::RIGHT_TOP,
            egui::vec2(-10.0, canvas.min.y + 10.0),
        )
        .order(egui::Order::Middle)
        .show(&ctx, |ui| {
            panel_frame().show(ui, |ui| {
                ui.set_min_width(240.0);
                if !screen.panels.side(ui, &mut screen.game, &name) {
                    ui.label(egui::RichText::new("Click a Bim to look at it.").color(theme::MUTED));
                }
            });
        });

    screen.panels.menu(&ctx, &mut screen.game, &name);
    screen.panels.inventory_window(&ctx, &screen.game, &name);
    screen.panels.end_frame(&mut screen.game);

    // --- painting --------------------------------------------------------------
    screen.game.render();
    let painter = canvas_painter(&ctx, canvas);
    paint_shapes(&painter, canvas, view, screen.game.shapes());

    // The names and the bubbles, over the deck and under the panels. Text
    // is the app's: the shape buffer has rectangles and ellipses in it and
    // nothing else.
    for who in 0..screen.game.crew_count() {
        let at = screen.game.bim_pos(who as usize);
        let head = view.to_canvas(Vec2::new(at.x, at.y - theme::NAME_LIFT)) + canvas.min;
        let color = if who as usize == player {
            theme::YOURS
        } else {
            theme::THEIRS
        };
        theme::name_over(&painter, egui::pos2(head.x, head.y), &name(who), color);
        let topic = screen.game.chat_topic(who as usize);
        if topic != 0 {
            let said = format!("{}…", chat_topic(topic));
            let at = view.to_canvas(Vec2::new(at.x, at.y - BUBBLE_LIFT)) + canvas.min;
            bubble(&painter, egui::pos2(at.x, at.y), &said);
        }
    }
    Ok(())
}

/// The status line is the player's Bim and nobody else's: it is where an
/// order lands and where a refusal is said, and Kate takes no orders.
fn status_line(screen: &RoomScreen, now: f64, minutes: f32) -> String {
    let game = &screen.game;
    let player = screen.panels.player;
    let me = crew_name(player as u32);
    if let Some((refused, until)) = &screen.refusal
        && now < *until
    {
        return refused.clone();
    }
    if !game.is_alive(player) {
        return format!("{me} has died.");
    }
    let resting = game.rest_left(player);
    if resting > 0.0 {
        return format!(
            "In bed · {} to go, up at {}",
            span_text(resting),
            clock_text(minutes + resting)
        );
    }
    // A stall looks like the game has hung unless it says what it is.
    if game.is_napping(player) {
        return format!("{me} has dropped off where he stands…");
    }
    if game.is_stalled(player) {
        return format!("{me} has lost the thread of it…");
    }
    if let Some(doing) = activity_line(game.activity(player)) {
        return doing.into();
    }
    if game.selected_count() != 0 {
        return format!("{me} selected — right-click the floor to send him there");
    }
    IDLE_HINT.into()
}

/// A panel floating over the deck, in the pages' own translucent green.
pub fn panel_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(egui::Color32::from_rgba_unmultiplied(20, 29, 25, 230))
        .stroke(egui::Stroke::new(1.0, theme::LINE))
        .corner_radius(6.0)
        .inner_margin(8.0)
}

/// What one of them is saying, in a bubble over its head. Only one of them
/// ever has a bubble up — they take turns — so two never overlap.
fn bubble(painter: &egui::Painter, at: egui::Pos2, said: &str) {
    let font = egui::FontId::proportional(BUBBLE_SIZE);
    let galley = painter.layout_no_wrap(said.to_string(), font.clone(), theme::INK);
    let pad = 8.0;
    let size = egui::vec2(galley.size().x + pad * 2.0, BUBBLE_SIZE + pad * 1.6);
    let rect = egui::Rect::from_center_size(at, size);
    painter.rect(
        rect,
        7.0,
        egui::Color32::from_rgba_unmultiplied(20, 29, 25, 240),
        egui::Stroke::new(
            1.0,
            egui::Color32::from_rgba_unmultiplied(127, 209, 168, 140),
        ),
        egui::StrokeKind::Outside,
    );
    // The tail, pointing down at whoever is talking.
    let bottom = rect.max.y - 1.0;
    painter.add(egui::Shape::convex_polygon(
        vec![
            egui::pos2(at.x - 5.0, bottom),
            egui::pos2(at.x + 5.0, bottom),
            egui::pos2(at.x, bottom + 8.0),
        ],
        egui::Color32::from_rgba_unmultiplied(20, 29, 25, 240),
        egui::Stroke::NONE,
    ));
    painter.galley(rect.min + egui::vec2(pad, pad * 0.8), galley, theme::INK);
}
