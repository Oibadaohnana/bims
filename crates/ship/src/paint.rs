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

use shipdesign::parts::{Layer, PartKind, Rotation, footprint, is_diagonal, use_spots};
use shipdesign::validate::Severity;
use shipdesign::{Grid, PlacedPart, ShipDesign, TILE};

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
/// An engine's exhaust on the deck, while building: the flame's colour.
const FLAME: Color = Color::rgb(1.0, 0.62, 0.22);

/// One colour per [`PartKind`], indexed by discriminant. `PARTS` order, and
/// the same order the palette is built in.
///
/// Index 0 is the deck and index 15 is the frame; both are drawn as tiles
/// rather than as objects, and both are in the table anyway so the palette
/// buttons for them have swatches.
pub static PART_COLORS: [Color; 35] = [
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
    Color::rgb(0.22, 0.24, 0.27), // Structure — the frame, darker than deck
    Color::rgb(0.46, 0.52, 0.58), // OutsideWall — hull, paler than a wall
    Color::rgb(0.38, 0.72, 0.86), // Helm
    Color::rgb(0.86, 0.62, 0.24), // Reactor
    Color::rgb(0.74, 0.66, 0.22), // PowerConduit
    Color::rgb(0.62, 0.58, 0.30), // Battery
    Color::rgb(0.40, 0.48, 0.58), // FuelTank
    Color::rgb(0.34, 0.62, 0.52), // LifeSupport
    Color::rgb(0.58, 0.68, 0.74), // Airlock
    Color::rgb(0.70, 0.74, 0.80), // SensorArray
    Color::rgb(0.48, 0.44, 0.36), // Shelf
    Color::rgb(0.60, 0.70, 0.76), // Shower
    Color::rgb(0.82, 0.52, 0.30), // Thruster — the engine's orange, paler
    Color::rgb(0.90, 0.32, 0.18), // HeavyEngine — the engine's orange, deeper
    Color::rgb(0.30, 0.34, 0.40), // DiagonalWall — the wall's grey
    Color::rgb(0.46, 0.52, 0.58), // DiagonalOutsideWall — the hull's
    Color::rgb(0.80, 0.42, 0.20), // Smelter — the glow of it
    Color::rgb(0.56, 0.50, 0.38), // Workbench
    Color::rgb(0.78, 0.80, 0.84), // SuitLocker — suit-white
    Color::rgb(0.42, 0.38, 0.44), // Armoury — gunmetal
];

/// The frame, drawn as the tile under everything. Dimmer than the deck and
/// without its edge, so a floored tile still reads as floored.
const FRAME: Color = Color::rgb(0.10, 0.11, 0.13);
const FRAME_EDGE: Color = Color::rgba(0.55, 0.85, 0.95, 0.06);

/// A conduit: a thin run across the tile rather than a block, because
/// something else can be standing on the same tile.
const CONDUIT: Color = Color::rgba(0.74, 0.66, 0.22, 0.75);

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
    frame(editor, list);
    deck(editor, list);
    conduit(editor, list);
    objects(editor, list);
    // Before the faults, so an issue outline is still legible over it, and
    // after the parts, so it reads as a wash over the ship rather than as
    // something underneath it.
    radiation(editor, list);
    exhausts(editor, list);
    faults(editor, list);
    pointed_at(editor, list);
    ghost(editor, list);
}

/// Where every engine's exhaust goes, drawn on the deck while building:
/// the row of tiles straight behind its bell, washed the flame's colour
/// with a plume tapering aft, so which way an engine fires and how far is
/// never a guess. Over something of the ship — an engine inside the hull,
/// firing into a room — the wash is the warning colour with a cross on
/// every tile it would cook, the same fault `IssueCode::ExhaustBlocked`
/// reports in the checks panel. It is drawn for every engine, not only the
/// one the pointer is on, because the point is to see it *before* the row
/// in the panel.
fn exhausts(editor: &Editor, list: &mut DrawList) {
    let grid = editor.design.grid();
    for part in &editor.design.parts {
        if !part.kind.def().pushes() {
            continue;
        }
        exhaust_marks(part, &grid, list, 1.0);
    }
}

/// The marks for one engine. `strength` is how loud: full for a placed
/// engine, fainter for the ghost of one.
fn exhaust_marks(part: &PlacedPart, grid: &Grid, list: &mut DrawList, strength: f32) {
    use shipdesign::validate::exhaust_tiles;
    let t = TILE as f32;
    let blocked = shipdesign::exhaust_blocked(part, grid);
    let color = if blocked { WARN } else { FLAME };
    // Aft as a unit step, from the difference between an exhaust tile and
    // the engine's own tile beside it — the exhaust tiles are exactly one
    // step behind the bell.
    let tiles = exhaust_tiles(part);
    let Some(&(ex, ey)) = tiles.first() else {
        return;
    };
    let own = part.tiles();
    let aft = own
        .iter()
        .map(|&(x, y)| (ex - x as i32, ey - y as i32))
        .find(|&(dx, dy)| dx.abs() + dy.abs() == 1)
        .unwrap_or((0, 1));
    for &(x, y) in &tiles {
        let inside = grid.inside((x, y));
        let cooked = inside && grid.get(Layer::Structure, (x, y)) != 0;
        let x0 = world(x);
        let y0 = world(y);
        // The wash: the tile behind the bell, and a fainter one beyond it
        // where the plume reaches. Both drawn even off the build area — the
        // plume goes into space, and that is what "clear" looks like.
        list.box_between(x0, y0, x0 + t, y0 + t, 2.0, color.alpha(0.30 * strength));
        let (x2, y2) = (world(x + aft.0), world(y + aft.1));
        list.box_between(x2, y2, x2 + t, y2 + t, 2.0, color.alpha(0.12 * strength));
        // A flame tongue down the middle of the tile, pointing aft.
        let (cx, cy) = (x0 + t / 2.0, y0 + t / 2.0);
        let (ax, ay) = (aft.0 as f32, aft.1 as f32);
        list.line(
            cx - ax * t * 0.35,
            cy - ay * t * 0.35,
            cx + ax * t * 0.45,
            cy + ay * t * 0.45,
            5.0,
            color.alpha(0.85 * strength),
        );
        list.line(
            cx + ax * t * 0.45,
            cy + ay * t * 0.45,
            cx + ax * t * 0.9,
            cy + ay * t * 0.9,
            2.5,
            color.alpha(0.55 * strength),
        );
        if cooked {
            // A cross over the tile it would burn.
            let m = 10.0;
            list.line(
                x0 + m,
                y0 + m,
                x0 + t - m,
                y0 + t - m,
                3.0,
                WARN.alpha(strength),
            );
            list.line(
                x0 + t - m,
                y0 + m,
                x0 + m,
                y0 + t - m,
                3.0,
                WARN.alpha(strength),
            );
        }
    }
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

/// The frame, under everything. Drawn for its own tiles whether or not
/// there is deck on them: a half-built ship is mostly bare frame and it has
/// to be visible to be built on.
fn frame(editor: &Editor, list: &mut DrawList) {
    let t = TILE as f32;
    let grid = editor.design.grid();
    for part in &editor.design.parts {
        if part.layer() != Layer::Structure {
            continue;
        }
        for (x, y) in part.tiles() {
            let (x0, y0) = (world(x as i32), world(y as i32));
            // Under a corner piece the frame is drawn as the same half of the
            // tile: the whole tile is framed as far as the rules go, but a
            // square of frame poking out past a chamfer would read as a hole
            // in the hull rather than as the edge of it.
            if let Some(rotation) = crate::hull::diagonal_at(&editor.design, &grid, (x, y)) {
                let c = crate::hull::corner(rotation);
                list.triangle(x0 + t / 2.0, y0 + t / 2.0, t, t, c.rot, FRAME);
                continue;
            }
            list.box_between(x0, y0, x0 + t, y0 + t, 0.0, FRAME);
            list.stroke_between(x0, y0, x0 + t, y0 + t, 0.0, 1.0, FRAME_EDGE);
        }
    }
}

/// What runs through a tile rather than filling it. A cross, so it reads as
/// a run of conduit and is still visible under whatever is standing on it.
fn conduit(editor: &Editor, list: &mut DrawList) {
    let t = TILE as f32;
    let thick = 4.0;
    for part in &editor.design.parts {
        if part.layer() != Layer::Utility {
            continue;
        }
        for (x, y) in part.tiles() {
            let (x0, y0) = (world(x as i32), world(y as i32));
            let (cx, cy) = (x0 + t / 2.0, y0 + t / 2.0);
            list.box_between(x0, cy - thick / 2.0, x0 + t, cy + thick / 2.0, 0.0, CONDUIT);
            list.box_between(cx - thick / 2.0, y0, cx + thick / 2.0, y0 + t, 0.0, CONDUIT);
        }
    }
}

/// Where the outside can see in, washed over the tiles it reaches.
///
/// **Not tied to the issue list.** Every other highlight on this page waits
/// for a pointer to rest on a row; this one does not, because a player who
/// has not looked at the checks panel is exactly the player about to accept a
/// ship with a hole in it.
fn radiation(editor: &Editor, list: &mut DrawList) {
    let t = TILE as f32;
    for &(x, y) in editor.exposed().tiles() {
        let (x0, y0) = (world(x as i32), world(y as i32));
        list.box_between(x0, y0, x0 + t, y0 + t, 0.0, WARN.alpha(0.22));
    }
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
        if part.layer() != Layer::Object {
            continue;
        }
        let (w, h) = footprint(part.kind, part.rotation);
        let inset = 3.0;
        let x0 = world(part.origin.0 as i32) + inset;
        let y0 = world(part.origin.1 as i32) + inset;
        let x1 = x0 + world(w as i32) - 2.0 * inset;
        let y1 = y0 + world(h as i32) - 2.0 * inset;
        let color = PART_COLORS[part.kind as usize];
        if is_diagonal(part.kind) {
            let c = crate::hull::corner(part.rotation);
            list.triangle(
                (x0 + x1) / 2.0,
                (y0 + y1) / 2.0,
                x1 - x0,
                y1 - y0,
                c.rot,
                color,
            );
            continue;
        }
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
        // Radiation already has the whole deck washed in `radiation` above.
        // Outlining its tiles as well would put a cyan box round every tile
        // of an unhulled ship — a grid of markers over a wash, saying the
        // same thing twice and making neither legible. Resting on the row
        // still rings them, which is what the highlight is for.
        if issue.code == shipdesign::IssueCode::RadiationExposure.code() && !focused {
            continue;
        }
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
    // A corner piece's ghost is the corner it would fill, so `R` visibly
    // walks it round the tile — a square ghost would make the key look dead.
    if is_diagonal(editor.tool) {
        let c = crate::hull::corner(editor.ghost);
        let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
        list.triangle(cx, cy, x1 - x0, y1 - y0, c.rot, color.alpha(0.35));
        list.push(
            crate::draw::KIND_TRIANGLE,
            cx,
            cy,
            x1 - x0 - 2.0,
            y1 - y0 - 2.0,
            c.rot,
            0.0,
            2.0,
            color,
        );
        return;
    }
    list.box_between(x0, y0, x1, y1, 4.0, color.alpha(0.20));
    list.stroke_between(x0 + 1.0, y0 + 1.0, x1 - 1.0, y1 - 1.0, 4.0, 2.0, color);
    // Which way it is turned, and where whoever uses it will stand. A part
    // used from any side marks only the ring tiles a body could stand on,
    // or an engine against the hull would be ringed with spots in the wall.
    if ok {
        facing_bar(editor.tool, editor.ghost, (x0, y0, x1, y1), list);
        let grid = editor.design.grid();
        let any = shipdesign::parts::any_side_will_do(editor.tool);
        for (dx, dy) in use_spots(editor.tool, editor.ghost) {
            let tile = (hover.0 + dx, hover.1 + dy);
            if any && (grid.get(Layer::Floor, tile) == 0 || grid.get(Layer::Object, tile) != 0) {
                continue;
            }
            let cx = world(tile.0) + t / 2.0;
            let cy = world(tile.1) + t / 2.0;
            list.ellipse(cx, cy, t * 0.30, t * 0.30, SPOT);
        }
    }
    // And where its exhaust would go, if it has one — whether or not the
    // ghost is placeable, since where the flame goes is the thing to know
    // before moving it: the wash says "firing into the ship" before the
    // part is ever put down.
    if editor.tool.def().pushes() && hover.0 >= 0 && hover.1 >= 0 {
        let would = PlacedPart {
            id: 0,
            kind: editor.tool,
            origin: (hover.0 as u32, hover.1 as u32),
            rotation: editor.ghost,
        };
        exhaust_marks(&would, &editor.design.grid(), list, 0.8);
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
