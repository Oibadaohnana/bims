//! Where the shapes land, and where the pointer is.
//!
//! A screen paints into **canvases**: rectangles of the window, each with
//! its own view and its own shape buffer. The room's screen has one, the
//! whole of the window between the panels; the lobby's World tab has two,
//! the galaxy and the system diagram, laid out inside an egui panel like
//! any other widget. Each is tessellated into an egui mesh and painted on
//! whichever layer the screen says — the background for a canvas between
//! the panels, the panel's own for one inside it — and egui clips it to
//! the rectangle. Bevy draws nothing but the clear colour.
//!
//! The pointer is read out of egui rather than out of Bevy's input, so that
//! a click on a panel is the panel's and a click beside it is the canvas's
//! by the same rule egui uses to decide — `is_pointer_over_area` — and so a
//! text field that has the keyboard keeps it.

use bevy::prelude::*;
use bevy_egui::egui;

use crate::shapes::{Rect, ShapeBuf, View};

pub struct CanvasPlugin;

impl Plugin for CanvasPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup);
    }
}

/// The one camera: it clears the window to the void, and egui draws on it.
/// Nothing else is ever spawned.
fn setup(mut commands: Commands) {
    commands.spawn((Camera2d, bevy_egui::PrimaryEguiContext));
}

/// Paint `shapes`, in world units under `view`, into `rect` on `painter`'s
/// layer — clipped to the rect, so a canvas inside a panel stays inside it
/// and one beside the panels stays out of them.
pub fn paint_shapes(painter: &egui::Painter, rect: Rect, view: View, shapes: &[f32]) {
    let mut buf = ShapeBuf::new(rect, painter.pixels_per_point());
    buf.replay(shapes, view);
    if buf.is_empty() {
        return;
    }
    painter
        .with_clip_rect(egui_rect(rect))
        .add(buf.into_shape());
}

/// The background layer, clipped to a canvas: where a screen whose canvas
/// is the window between the panels paints, and puts its words after.
pub fn canvas_painter(ctx: &egui::Context, rect: Rect) -> egui::Painter {
    ctx.layer_painter(egui::LayerId::background())
        .with_clip_rect(egui_rect(rect))
}

// --- the pointer -------------------------------------------------------------

/// The pointer this frame, as egui saw it. Positions are window pixels.
#[derive(Clone, Copy, Debug, Default)]
pub struct Pointer {
    pub pos: Option<Vec2>,
    /// Over a panel, a window or a menu: the canvas is not to act on it.
    pub over_ui: bool,
    pub primary_pressed: bool,
    pub primary_released: bool,
    pub secondary_pressed: bool,
    pub secondary_released: bool,
    pub middle_down: bool,
    pub middle_pressed: bool,
    /// Wheel, in points, up positive.
    pub scroll: f32,
}

impl Pointer {
    pub fn read(ctx: &egui::Context) -> Pointer {
        use egui::PointerButton::{Middle, Primary, Secondary};
        // Asked before the input lock is taken: both of these lock the
        // context themselves, and egui's lock is not reentrant.
        let over_ui = ctx.is_pointer_over_egui() || ctx.egui_wants_pointer_input();
        ctx.input(|i| Pointer {
            pos: i.pointer.latest_pos().map(|p| Vec2::new(p.x, p.y)),
            over_ui,
            primary_pressed: i.pointer.button_pressed(Primary),
            primary_released: i.pointer.button_released(Primary),
            secondary_pressed: i.pointer.button_pressed(Secondary),
            secondary_released: i.pointer.button_released(Secondary),
            middle_down: i.pointer.button_down(Middle),
            middle_pressed: i.pointer.button_pressed(Middle),
            // The raw wheel events rather than the smoothed delta: a zoom
            // wants the notch, not a scroll area's easing.
            scroll: i
                .raw
                .events
                .iter()
                .map(|e| match e {
                    egui::Event::MouseWheel { unit, delta, .. } => match unit {
                        egui::MouseWheelUnit::Point => delta.y,
                        egui::MouseWheelUnit::Line => delta.y * 40.0,
                        egui::MouseWheelUnit::Page => delta.y * 400.0,
                    },
                    _ => 0.0,
                })
                .sum(),
        })
    }

    /// Where the pointer is inside `rect`, or `None` when it is outside it
    /// or over a panel.
    pub fn on(&self, rect: Rect) -> Option<Vec2> {
        let p = self.pos?;
        (!self.over_ui && rect.contains(p)).then(|| p - rect.min)
    }
}

/// How much one point of wheel is worth as a zoom factor.
pub const ZOOM_PER_POINT: f32 = 0.003;

pub fn zoom_factor(scroll: f32) -> f32 {
    (scroll * ZOOM_PER_POINT).exp()
}

/// A `Ui` over the whole window, on the background layer: what the panels
/// are shown into, and what is left of it afterwards is the canvas.
pub fn root_ui(ctx: &egui::Context) -> egui::Ui {
    egui::Ui::new(
        ctx.clone(),
        egui::Id::new("viewport"),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(ctx.viewport_rect()),
    )
}

/// The egui rect as ours.
pub fn rect_of(r: egui::Rect) -> Rect {
    Rect::new(Vec2::new(r.min.x, r.min.y), Vec2::new(r.max.x, r.max.y))
}

pub fn egui_rect(r: Rect) -> egui::Rect {
    egui::Rect::from_min_max(egui::pos2(r.min.x, r.min.y), egui::pos2(r.max.x, r.max.y))
}
