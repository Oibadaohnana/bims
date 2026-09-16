//! Drawing a design, as rectangles and ellipses.
//!
//! Placeholder art throughout: one colour per kind and a bar showing which
//! way it faces. The room's fixtures are drawn properly, tile by tile, in
//! `crates/game/src/room.rs`, and **none of that is reused** — a designer
//! showing a ship at eight pixels a tile wants a legible block, not a
//! hand-drawn hob, and the two would have to be kept in step for no gain.
//!
//! The palette is the repo's: the deck greys and the cyan glow out of
//! `room.rs`, so the designer and the room look like one game.
//!
//! The colours are also what the host paints its palette swatches with —
//! they cross the boundary as numbers through `ship_part_color_*`, so the
//! button for a hob cannot end up a different colour from the hob.

use shipdesign::parts::{Layer, PartKind, Rotation, footprint, use_spots};
use shipdesign::validate::Severity;
use shipdesign::{ShipDesign, TILE};

use crate::draw::{Color, DrawList};
use crate::editor::Editor;

const VOID: Color = Color::rgb(0.03, 0.04, 0.05);
const AREA: Color = Color::rgb(0.07, 0.09, 0.11);
const AREA_EDGE: Color = Color::rgba(0.38, 0.86, 0.95, 0.35);
const SEAM: Color = Color::rgba(0.55, 0.85, 0.95, 0.07);
const DECK: Color = Color::rgb(0.13, 0.15, 0.18);
const DECK_EDGE: Color = Color::rgba(0.55, 0.85, 0.95, 0.10);

const GLOW: Color = Color::rgb(0.38, 0.86, 0.95);
const WARN: Color = Color::rgb(0.98, 0.45, 0.32);
const GOOD: Color = Color::rgb(0.50, 0.90, 0.60);
const SPOT: Color = Color::rgba(0.98, 0.82, 0.35, 0.85);

/// One colour per [`PartKind`], indexed by discriminant. `PARTS` order, and
/// the same order the palette is built in.
///
/// Index 0 is the deck, which is drawn as a tile rather than as an object; it
/// is in the table anyway so the palette button for it has a swatch.
pub static PART_COLORS: [Color; 15] = [
    Color::rgb(0.13, 0.15, 0.18), // Floor
    Color::rgb(0.30, 0.34, 0.40), // Wall
    Color::rgb(0.38, 0.86, 0.95), // Door
    Color::rgb(0.85, 0.42, 0.22), // Engine
    Color::rgb(0.52, 0.57, 0.70), // Bunk
    Color::rgb(0.56, 0.63, 0.70), // ColdStore
    Color::rgb(0.52, 0.57, 0.63), // Worktop
    Color::rgb(0.72, 0.40, 0.28), // Hob
    Color::rgb(0.44, 0.56, 0.62), // Dishwasher
    Color::rgb(0.40, 0.45, 0.52), // Table
    Color::rgb(0.30, 0.36, 0.44), // Chair
    Color::rgb(0.76, 0.80, 0.84), // Toilet
    Color::rgb(0.64, 0.72, 0.78), // Basin
    Color::rgb(0.40, 0.66, 0.30), // HydroBay
    Color::rgb(0.50, 0.42, 0.30), // BroomLocker
];

/// Tile coordinates to world units.
fn world(tile: i32) -> f32 {
    tile as f32 * TILE as f32
}

/// The whole frame.
pub fn paint(editor: &Editor, list: &mut DrawList) {
    list.clear();

    let span = world(editor.design.build_area as i32);
    let margin = world(4);
    list.box_between(-margin, -margin, span + margin, span + margin, 0.0, VOID);
    list.box_between(0.0, 0.0, span, span, 0.0, AREA);

    seams(editor, list, span);
    deck(editor, list);
    objects(editor, list);
    faults(editor, list);
    pointed_at(editor, list);
    ghost(editor, list);
}

/// The tile grid. Faint, and drawn across the whole build area rather than
/// only where there is deck: it is the thing that says where you may build.
fn seams(editor: &Editor, list: &mut DrawList, span: f32) {
    let line = 1.0;
    for i in 0..=editor.design.build_area {
        let at = world(i as i32);
        list.box_between(at - line / 2.0, 0.0, at + line / 2.0, span, 0.0, SEAM);
        list.box_between(0.0, at - line / 2.0, span, at + line / 2.0, 0.0, SEAM);
    }
    // The edge of what may be built on, said louder than the seams.
    list.stroke_between(0.0, 0.0, span, span, 0.0, 2.0, AREA_EDGE);
}

fn deck(editor: &Editor, list: &mut DrawList) {
    let t = TILE as f32;
    for part in &editor.design.parts {
        if part.layer() != Layer::Floor {
            continue;
        }
        for (x, y) in part.tiles() {
            let (x0, y0) = (world(x as i32) + 1.0, world(y as i32) + 1.0);
            list.box_between(x0, y0, x0 + t - 2.0, y0 + t - 2.0, 2.0, DECK);
            list.stroke_between(x0, y0, x0 + t - 2.0, y0 + t - 2.0, 2.0, 1.0, DECK_EDGE);
        }
    }
}

fn objects(editor: &Editor, list: &mut DrawList) {
    for part in &editor.design.parts {
        if part.layer() == Layer::Floor {
            continue;
        }
        let (w, h) = footprint(part.kind, part.rotation);
        let inset = 3.0;
        let x0 = world(part.origin.0 as i32) + inset;
        let y0 = world(part.origin.1 as i32) + inset;
        let x1 = x0 + world(w as i32) - 2.0 * inset;
        let y1 = y0 + world(h as i32) - 2.0 * inset;
        let color = PART_COLORS[part.kind as usize];
        list.box_between(x0, y0, x1, y1, 5.0, color);
        facing_bar(part.kind, part.rotation, (x0, y0, x1, y1), list);
    }
}

/// A darker bar along the side a part is approached from.
///
/// Placeholder art has to say *something* about rotation or a turned engine
/// looks identical to an upright one and the `R` key appears to do nothing.
/// The side is worked out from the first use spot, so it is the same fact the
/// validator checks rather than a second opinion about which way round a part
/// is.
fn facing_bar(
    kind: PartKind,
    rotation: Rotation,
    (x0, y0, x1, y1): (f32, f32, f32, f32),
    list: &mut DrawList,
) {
    let spots = use_spots(kind, rotation);
    let Some(&(sx, sy)) = spots.first() else {
        return;
    };
    // The use spot in tiles from the part's own centre, so which side it is
    // on falls out of the sign of the bigger component.
    let (w, h) = footprint(kind, rotation);
    let dx = sx as f32 + 0.5 - w as f32 / 2.0;
    let dy = sy as f32 + 0.5 - h as f32 / 2.0;
    let thick = 6.0;
    let dark = Color::rgba(0.0, 0.0, 0.0, 0.40);
    if dx.abs() >= dy.abs() {
        if dx >= 0.0 {
            list.box_between(x1 - thick, y0, x1, y1, 0.0, dark);
        } else {
            list.box_between(x0, y0, x0 + thick, y1, 0.0, dark);
        }
    } else if dy >= 0.0 {
        list.box_between(x0, y1 - thick, x1, y1, 0.0, dark);
    } else {
        list.box_between(x0, y0, x1, y0 + thick, 0.0, dark);
    }
}

/// What is wrong, rung on the deck.
///
/// Every issue's tiles are outlined all the time — a fault you have to hover
/// to discover is a fault nobody finds — and the one being pointed at in the
/// list is filled as well.
fn faults(editor: &Editor, list: &mut DrawList) {
    let t = TILE as f32;
    for (i, issue) in editor.issues().iter().enumerate() {
        let shade = match issue.severity {
            Severity::Error => WARN,
            Severity::Warning => GLOW,
        };
        let focused = editor.focus == Some(i);
        for &(x, y) in &issue.tiles {
            let (x0, y0) = (world(x as i32), world(y as i32));
            if focused {
                list.box_between(x0, y0, x0 + t, y0 + t, 3.0, shade.alpha(0.30));
            }
            list.stroke_between(
                x0 + 2.0,
                y0 + 2.0,
                x0 + t - 2.0,
                y0 + t - 2.0,
                3.0,
                if focused { 4.0 } else { 2.0 },
                shade,
            );
        }
    }
}

/// The part under the pointer: its outline, and the tiles a Bim would stand
/// in to use it.
///
/// Use spots are shown **only** while a part is pointed at. They are the one
/// piece of the model that has no other way of being seen, and drawing them
/// permanently would fill the deck with markers.
fn pointed_at(editor: &Editor, list: &mut DrawList) {
    let id = editor.hovered_part();
    if id == 0 || editor.drag.is_some() {
        return;
    }
    let Some(part) = editor.design.part(id) else {
        return;
    };
    // The ring exists to introduce the use spots, so a part with none has
    // nothing to say and gets nothing. That is the deck and the walls — and
    // ringing every deck tile the pointer crossed was a light flashing on and
    // off across the whole grid for no information at all. What the pointer is
    // over is in the readout, in words.
    if part.use_spots().is_empty() {
        return;
    }
    let t = TILE as f32;
    for (x, y) in part.tiles() {
        let (x0, y0) = (world(x as i32), world(y as i32));
        list.stroke_between(
            x0 + 1.0,
            y0 + 1.0,
            x0 + t - 1.0,
            y0 + t - 1.0,
            3.0,
            2.0,
            GLOW,
        );
    }
    for (sx, sy) in part.use_spots() {
        let cx = world(sx) + t / 2.0;
        let cy = world(sy) + t / 2.0;
        list.ellipse(cx, cy, t * 0.34, t * 0.34, SPOT);
    }
}

/// What the tool would do if you clicked now.
///
/// Green for takes, red for refused — and during a drag, every tile of the
/// drag rather than only the one under the pointer, because a drag that only
/// showed its far end would be a rectangle nobody could see the size of.
fn ghost(editor: &Editor, list: &mut DrawList) {
    if editor.phase != crate::editor::Phase::Design {
        return;
    }
    let t = TILE as f32;

    if let Some(drag) = editor.drag {
        let color = if drag.removing { WARN } else { GOOD };
        for (x, y) in editor.drag_tiles() {
            let (x0, y0) = (world(x as i32), world(y as i32));
            list.box_between(x0, y0, x0 + t, y0 + t, 2.0, color.alpha(0.22));
            list.stroke_between(
                x0 + 1.0,
                y0 + 1.0,
                x0 + t - 1.0,
                y0 + t - 1.0,
                2.0,
                2.0,
                color,
            );
        }
        return;
    }

    let Some((hover, (w, h))) = editor.ghost_box() else {
        return;
    };
    let ok = editor.ghost_ok();
    let color = if ok { GOOD } else { WARN };
    let x0 = world(hover.0);
    let y0 = world(hover.1);
    let x1 = x0 + world(w as i32);
    let y1 = y0 + world(h as i32);
    list.box_between(x0, y0, x1, y1, 4.0, color.alpha(0.20));
    list.stroke_between(x0 + 1.0, y0 + 1.0, x1 - 1.0, y1 - 1.0, 4.0, 2.0, color);
    // Which way it is turned, and where whoever uses it will stand.
    if ok {
        facing_bar(editor.tool, editor.ghost, (x0, y0, x1, y1), list);
        for (dx, dy) in use_spots(editor.tool, editor.ghost) {
            let cx = world(hover.0 + dx) + t / 2.0;
            let cy = world(hover.1 + dy) + t / 2.0;
            list.ellipse(cx, cy, t * 0.30, t * 0.30, SPOT);
        }
    }
}

/// What the hull would weigh if this design were built. Not drawn; the
/// painter is simply where the world-unit helpers are, and the placeholder
/// handoff screen wants the number.
pub fn debug_mass(design: &ShipDesign, crew: u32) -> f64 {
    shipdesign::ship_mass(design, crew)
        .map(|m| m.get())
        .unwrap_or(0.0)
}
