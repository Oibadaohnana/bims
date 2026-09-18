//! The look: the pages' palette, egui's visuals set to it once, and the
//! handful of widgets every screen draws the same way.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};

/// The window behind everything: the void the ship sits in.
pub const VOID: Color = Color::srgb(0.047, 0.071, 0.063);

pub const INK: egui::Color32 = egui::Color32::from_rgb(0xe7, 0xef, 0xe9);
pub const MUTED: egui::Color32 = egui::Color32::from_rgb(0x8f, 0xa8, 0x9a);
pub const ACCENT: egui::Color32 = egui::Color32::from_rgb(0x7f, 0xd1, 0xa8);
pub const WARN: egui::Color32 = egui::Color32::from_rgb(0xfa, 0x73, 0x52);
pub const BAD: egui::Color32 = egui::Color32::from_rgb(0xff, 0x8f, 0x7a);
pub const CAUTION: egui::Color32 = egui::Color32::from_rgb(0xff, 0xd7, 0xa6);
pub const GRAVE: egui::Color32 = egui::Color32::from_rgb(0xff, 0xd1, 0x5a);
pub const PANEL: egui::Color32 = egui::Color32::from_rgb(0x18, 0x21, 0x1d);
pub const PANEL_DEEP: egui::Color32 = egui::Color32::from_rgb(0x0c, 0x12, 0x10);
pub const RAISED: egui::Color32 = egui::Color32::from_rgb(0x22, 0x33, 0x2b);
pub const RAISED_ON: egui::Color32 = egui::Color32::from_rgb(0x4a, 0x6d, 0x5c);
pub const LINE: egui::Color32 = egui::Color32::from_rgb(0x2a, 0x3b, 0x33);
pub const YOURS: egui::Color32 = ACCENT;
pub const THEIRS: egui::Color32 = INK;
pub const SLEEP: egui::Color32 = egui::Color32::from_rgb(0x3f, 0x6e, 0xa8);
pub const ANY: egui::Color32 = egui::Color32::from_rgb(0x4a, 0x55, 0x60);
pub const NAME_STROKE: egui::Color32 = egui::Color32::from_rgba_premultiplied(6, 10, 9, 217);

/// The name over a Bim's head: how big, and how far above the body it
/// sits. The lift is in room units and scaled with the view, so the name
/// stays over the head at any window size; the size is in points, because
/// type that scaled with the room would go illegible on a small window.
pub const NAME_SIZE: f32 = 13.5;
pub const NAME_LIFT: f32 = 46.0;

pub struct ThemePlugin;

impl Plugin for ThemePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(EguiPrimaryContextPass, style.run_if(run_once));
    }
}

fn style(mut contexts: EguiContexts) -> Result {
    let ctx = contexts.ctx_mut()?;
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = PANEL;
    visuals.window_fill = PANEL;
    visuals.extreme_bg_color = PANEL_DEEP;
    visuals.faint_bg_color = RAISED;
    visuals.override_text_color = Some(INK);
    visuals.hyperlink_color = ACCENT;
    visuals.selection.bg_fill = RAISED_ON;
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, LINE);
    visuals.widgets.inactive.bg_fill = RAISED;
    visuals.widgets.inactive.weak_bg_fill = RAISED;
    visuals.widgets.hovered.bg_fill = RAISED_ON;
    visuals.widgets.hovered.weak_bg_fill = RAISED_ON;
    visuals.widgets.active.bg_fill = RAISED_ON;
    visuals.widgets.active.weak_bg_fill = RAISED_ON;
    visuals.window_stroke = egui::Stroke::new(1.0, LINE);
    visuals.window_corner_radius = egui::CornerRadius::same(6);
    visuals.menu_corner_radius = egui::CornerRadius::same(6);
    ctx.set_visuals(visuals);
    ctx.all_styles_mut(|style| {
        // The type, a size up from egui's own throughout: the panels are read
        // at arm's length over a deck, and the default is sized for a form.
        // The scale setting on the Esc sheet ([`ui_scale_row`]) multiplies
        // everything on top of this.
        use egui::{FontFamily, FontId, TextStyle};
        style.text_styles = [
            (
                TextStyle::Small,
                FontId::new(10.5, FontFamily::Proportional),
            ),
            (TextStyle::Body, FontId::new(14.5, FontFamily::Proportional)),
            (
                TextStyle::Button,
                FontId::new(14.5, FontFamily::Proportional),
            ),
            (
                TextStyle::Heading,
                FontId::new(21.0, FontFamily::Proportional),
            ),
            (
                TextStyle::Monospace,
                FontId::new(14.0, FontFamily::Monospace),
            ),
        ]
        .into();
        style.spacing.item_spacing = egui::vec2(6.0, 4.0);
        style.spacing.button_padding = egui::vec2(8.0, 3.0);
        // Tooltips are asked for, never stumbled into: long enough that
        // crossing a panel sets nothing off, short enough that asking feels
        // like no wait at all.
        style.interaction.tooltip_delay = 0.3;
        style.interaction.tooltip_grace_time = 0.1;
    });
    Ok(())
}

// --- widgets -----------------------------------------------------------------

/// A section heading, the pages' `h2`.
pub fn heading(ui: &mut egui::Ui, text: &str) {
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(text.to_uppercase())
            .small()
            .color(MUTED)
            .strong(),
    );
}

/// A word that already names the thing, underlined, with its explanation
/// hanging off it. The word is the affordance.
pub fn asks(ui: &mut egui::Ui, word: &str, tip: &str) -> egui::Response {
    ui.add(egui::Label::new(egui::RichText::new(word).underline()).sense(egui::Sense::hover()))
        .on_hover_cursor(egui::CursorIcon::Help)
        .on_hover_text(tip)
}

/// A "?" to put beside something that has no word of its own to underline.
pub fn question_mark(ui: &mut egui::Ui, tip: &str) -> egui::Response {
    ui.add(egui::Button::new(egui::RichText::new("?").small()).min_size(egui::vec2(17.0, 17.0)))
        .on_hover_cursor(egui::CursorIcon::Help)
        .on_hover_text(tip)
}

/// A bar: a fraction of a strip, with the track behind it.
pub fn bar(ui: &mut egui::Ui, width: f32, fraction: f32, fill: egui::Color32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 8.0), egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 3.0, RAISED);
    let mut done = rect;
    done.set_width(rect.width() * fraction.clamp(0.0, 1.0));
    painter.rect_filled(done, 3.0, fill);
    response
}

/// A button that is marked as the one in force.
pub fn toggle(ui: &mut egui::Ui, on: bool, text: impl Into<egui::WidgetText>) -> egui::Response {
    ui.add(toggle_button(on, text))
}

/// The same, as a widget, for a caller that wants to `add_enabled` it.
pub fn toggle_button(on: bool, text: impl Into<egui::WidgetText>) -> egui::Button<'static> {
    let text: egui::WidgetText = text.into();
    if on {
        egui::Button::new(text).fill(RAISED_ON)
    } else {
        egui::Button::new(text)
    }
}

/// A big button, the pages' `.big`.
pub fn big(ui: &mut egui::Ui, text: &str, enabled: bool) -> egui::Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(egui::RichText::new(text).size(17.0)).min_size(egui::vec2(130.0, 32.0)),
    )
}

/// The sizes the whole window can be shown at, as multiples of the
/// display's own: egui's zoom factor, which scales the type, the panels and
/// the canvas between them alike, and which bevy_egui reads back so the
/// pointer lands where it looks. One is the display as it is.
pub const UI_SCALES: [(f32, &str); 5] = [
    (1.0, "100%"),
    (1.25, "125%"),
    (1.5, "150%"),
    (1.75, "175%"),
    (2.0, "200%"),
];

/// The UI scale, chosen: a row of the sizes with the one in force marked.
/// On the Esc sheet, and nowhere else — it is the one setting there is.
pub fn ui_scale_row(ui: &mut egui::Ui) {
    let now = ui.ctx().zoom_factor();
    let mut pick = None;
    ui.horizontal(|ui| {
        for (scale, label) in UI_SCALES {
            if toggle(ui, (now - scale).abs() < 0.01, label).clicked() {
                pick = Some(scale);
            }
        }
    });
    if let Some(scale) = pick {
        ui.ctx().set_zoom_factor(scale);
    }
}

/// A swatch of colour, for a palette row or a legend.
pub fn swatch(ui: &mut egui::Ui, color: egui::Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 2.0, color);
}

pub fn ship_color32(c: ship::draw::Color) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(
        (c.r * 255.0) as u8,
        (c.g * 255.0) as u8,
        (c.b * 255.0) as u8,
        (c.a * 255.0) as u8,
    )
}

/// A name over a head, on the background layer so it sits over the deck
/// and under every panel. Stroked first in the void's own darkness, so it
/// stays legible over a pale part as readily as over the floor.
pub fn name_over(painter: &egui::Painter, at: egui::Pos2, name: &str, color: egui::Color32) {
    let font = egui::FontId::proportional(NAME_SIZE);
    for (dx, dy) in [
        (-1.0, 0.0),
        (1.0, 0.0),
        (0.0, -1.0),
        (0.0, 1.0),
        (-1.0, -1.0),
        (1.0, 1.0),
    ] {
        painter.text(
            at + egui::vec2(dx, dy),
            egui::Align2::CENTER_BOTTOM,
            name,
            font.clone(),
            NAME_STROKE,
        );
    }
    painter.text(at, egui::Align2::CENTER_BOTTOM, name, font, color);
}
