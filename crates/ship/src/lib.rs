//! The ship designer, as the browser sees it.
//!
//! The design phase runs here: `web/ship.html` hands this module a canvas
//! size and the four numbers the lobby chose, then each frame asks it to
//! rebuild a shape buffer and reads that buffer straight out of wasm memory.
//! The format is the room's twelve floats a shape, so one replay loop paints
//! either page — but this is a **separate cdylib** and imports nothing from
//! `crates/game`.
//!
//! Every rule is next door in `shipdesign`, which renders nothing and knows
//! nothing about a pointer. Nothing in here decides whether a part may be
//! placed; it asks.
//!
//! # The boundary
//!
//! **No strings cross it.** The parts, the resources, the reasons an edit was
//! refused and the faults in a design are all numbers, and `PART_NAMES`,
//! `RESOURCE_NAMES`, `EDIT_LINES` and `ISSUE_LINES` in `web/ship.js` are
//! where the words live. Adding a part is an enum variant, a name in that
//! file, and the range check in the tests.

mod draw;
mod editor;
mod paint;
mod view;

use editor::Editor;
use physics::{Facing, ResourceId};
use shipdesign::parts::{PartKind, footprint};
use shipdesign::validate::Severity;
use shipdesign::{RESOURCE_COUNT, TILE};

/// wasm is single-threaded and the host drives every call, so one global is
/// both sufficient and safe in practice. Same arrangement as the room's.
static mut EDITOR: Option<Editor> = None;
static mut LIST: Option<draw::DrawList> = None;

fn editor() -> &'static mut Editor {
    unsafe {
        (*(&raw mut EDITOR))
            .as_mut()
            .expect("ship_init was not called")
    }
}

fn list() -> &'static mut draw::DrawList {
    unsafe {
        let slot = &mut *(&raw mut LIST);
        if slot.is_none() {
            *slot = Some(draw::DrawList::new());
        }
        slot.as_mut().unwrap()
    }
}

// --- starting up ----------------------------------------------------------

/// Floats per shape in the draw buffer. The host walks the slice with this
/// rather than a constant of its own.
#[unsafe(no_mangle)]
pub extern "C" fn ship_stride() -> usize {
    draw::STRIDE
}

/// World units to a tile side. The host wants it for nothing but its own
/// readouts; the geometry is all done in here.
#[unsafe(no_mangle)]
pub extern "C" fn ship_tile() -> u32 {
    TILE
}

/// Open a design phase.
///
/// `build_area` is tiles a side and `stock_permille` is the lobby's stockpile
/// factor in thousandths — ×0.5 arrives as 500. Both, with `players` and
/// `local_slot`, come off the query string `web/builder.js` navigated with,
/// and all four are numbers because nothing else crosses this boundary.
#[unsafe(no_mangle)]
pub extern "C" fn ship_init(
    build_area: u32,
    stock_permille: u32,
    players: u32,
    local_slot: u32,
    width: f32,
    height: f32,
) {
    unsafe {
        EDITOR = Some(Editor::new(
            build_area,
            stock_permille,
            players,
            local_slot,
            width,
            height,
        ))
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ship_resize(width: f32, height: f32) {
    editor().view.resize(width, height);
}

/// Rebuild the shape buffer. Nothing moves on its own here — there is no
/// simulation — so this is a redraw rather than a step, and it is called once
/// a frame because the ghost follows the pointer.
#[unsafe(no_mangle)]
pub extern "C" fn ship_render() {
    let editor = editor();
    paint::paint(editor, list());
}

/// Pointer to the current frame's shapes. Only valid until the next
/// [`ship_render`], and memory growth can move it, so re-read it every frame.
#[unsafe(no_mangle)]
pub extern "C" fn ship_draw_ptr() -> *const f32 {
    list().as_ptr()
}

#[unsafe(no_mangle)]
pub extern "C" fn ship_draw_len() -> usize {
    list().len()
}

// --- the camera -----------------------------------------------------------

#[unsafe(no_mangle)]
pub extern "C" fn ship_view_scale() -> f32 {
    editor().view.scale()
}

#[unsafe(no_mangle)]
pub extern "C" fn ship_view_x() -> f32 {
    editor().view.offset_x()
}

#[unsafe(no_mangle)]
pub extern "C" fn ship_view_y() -> f32 {
    editor().view.offset_y()
}

/// Shove the view by a screen-pixel delta. Middle-drag and WASD both come
/// through here.
#[unsafe(no_mangle)]
pub extern "C" fn ship_pan(dx: f32, dy: f32) {
    editor().view.pan(dx, dy);
}

/// Zoom about a point on the canvas. `factor` is a multiplier the host works
/// out from the wheel, so no exponential has to be linked into the wasm for
/// the sake of a scroll.
#[unsafe(no_mangle)]
pub extern "C" fn ship_zoom(at_x: f32, at_y: f32, factor: f32) {
    editor().view.zoom(at_x, at_y, factor);
}

// --- the palette ----------------------------------------------------------

#[unsafe(no_mangle)]
pub extern "C" fn ship_part_count() -> u32 {
    PartKind::ALL.len() as u32
}

/// Tiles across, at the ghost's current rotation — so a palette row can show
/// what it is about to put down rather than what it would be upright.
#[unsafe(no_mangle)]
pub extern "C" fn ship_part_w(kind: u32) -> u32 {
    with_kind(kind, |k| footprint(k, editor().ghost).0)
}

#[unsafe(no_mangle)]
pub extern "C" fn ship_part_h(kind: u32) -> u32 {
    with_kind(kind, |k| footprint(k, editor().ghost).1)
}

/// What one of them costs in `resource`. Zero for a resource it does not use.
#[unsafe(no_mangle)]
pub extern "C" fn ship_part_cost(kind: u32, resource: u32) -> u32 {
    with_kind(kind, |k| {
        k.def()
            .cost
            .iter()
            .find(|&&(id, _)| id as u32 == resource)
            .map(|&(_, units)| units)
            .unwrap_or(0)
    })
}

/// The colour the part is drawn in, as a byte. The palette swatches are
/// painted with these, so a button cannot end up a different colour from the
/// thing it places.
#[unsafe(no_mangle)]
pub extern "C" fn ship_part_color(kind: u32, channel: u32) -> u32 {
    with_kind(kind, |k| {
        let c = paint::PART_COLORS[k as usize];
        let v = match channel {
            0 => c.r,
            1 => c.g,
            _ => c.b,
        };
        (v * 255.0) as u32
    })
}

fn with_kind<T: Default>(kind: u32, f: impl FnOnce(PartKind) -> T) -> T {
    PartKind::from_code(kind).map(f).unwrap_or_default()
}

// --- the tool under the pointer -------------------------------------------

#[unsafe(no_mangle)]
pub extern "C" fn ship_set_tool(kind: u32) {
    editor().set_tool(kind);
}

#[unsafe(no_mangle)]
pub extern "C" fn ship_tool() -> u32 {
    editor().tool.code()
}

/// `R`. Turns the ghost a quarter clockwise; does not touch anything placed.
#[unsafe(no_mangle)]
pub extern "C" fn ship_rotate_ghost() {
    editor().rotate_ghost();
}

#[unsafe(no_mangle)]
pub extern "C" fn ship_ghost_rotation() -> u32 {
    editor().ghost.code()
}

#[unsafe(no_mangle)]
pub extern "C" fn ship_hover(x: f32, y: f32) {
    editor().hover_at(x, y);
}

#[unsafe(no_mangle)]
pub extern "C" fn ship_leave() {
    editor().leave();
}

/// Whether the pointer is over the build area at all. The tile readouts below
/// are meaningless when it is not, and there is no negative `u32` to say so
/// with.
#[unsafe(no_mangle)]
pub extern "C" fn ship_hover_inside() -> u32 {
    let editor = editor();
    editor.hover.is_some_and(|t| editor.design.holds(t)) as u32
}

#[unsafe(no_mangle)]
pub extern "C" fn ship_hover_x() -> u32 {
    editor().hover.map(|t| t.0.max(0) as u32).unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn ship_hover_y() -> u32 {
    editor().hover.map(|t| t.1.max(0) as u32).unwrap_or(0)
}

/// Whether the tool would go down where the pointer is.
#[unsafe(no_mangle)]
pub extern "C" fn ship_ghost_ok() -> u32 {
    editor().ghost_ok() as u32
}

/// The part under the pointer, or 0. Objects win over the deck beneath them.
#[unsafe(no_mangle)]
pub extern "C" fn ship_hovered_part() -> u32 {
    editor().hovered_part()
}

#[unsafe(no_mangle)]
pub extern "C" fn ship_part_kind(part_id: u32) -> u32 {
    editor()
        .design
        .part(part_id)
        .map(|p| p.kind.code())
        .unwrap_or(0)
}

// --- drags ----------------------------------------------------------------
//
// A drag is geometry, not an edit. The host asks what tiles it covers and
// then sends each one through `net` as its own Edit, which is what keeps the
// transport seam honest: there is no bulk operation for a network to have to
// learn about later.

#[unsafe(no_mangle)]
pub extern "C" fn ship_drag_begin(x: f32, y: f32, removing: u32) {
    editor().drag_begin(x, y, removing != 0);
}

#[unsafe(no_mangle)]
pub extern "C" fn ship_drag_update(x: f32, y: f32) {
    editor().hover_at(x, y);
}

#[unsafe(no_mangle)]
pub extern "C" fn ship_drag_cancel() {
    editor().drag_cancel();
}

#[unsafe(no_mangle)]
pub extern "C" fn ship_dragging() -> u32 {
    editor().drag.is_some() as u32
}

#[unsafe(no_mangle)]
pub extern "C" fn ship_drag_removing() -> u32 {
    editor().drag.is_some_and(|d| d.removing) as u32
}

#[unsafe(no_mangle)]
pub extern "C" fn ship_drag_tile_count() -> u32 {
    editor().drag_tiles().len() as u32
}

#[unsafe(no_mangle)]
pub extern "C" fn ship_drag_tile_x(i: u32) -> u32 {
    editor()
        .drag_tiles()
        .get(i as usize)
        .map(|t| t.0)
        .unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn ship_drag_tile_y(i: u32) -> u32 {
    editor()
        .drag_tiles()
        .get(i as usize)
        .map(|t| t.1)
        .unwrap_or(0)
}

/// What a right-drag would take off, objects before deck. In that order
/// because the other way round every tile with something standing on it is
/// refused and the drag looks half broken.
#[unsafe(no_mangle)]
pub extern "C" fn ship_drag_part_count() -> u32 {
    editor().drag_parts().len() as u32
}

#[unsafe(no_mangle)]
pub extern "C" fn ship_drag_part(i: u32) -> u32 {
    editor().drag_parts().get(i as usize).copied().unwrap_or(0)
}

// --- editing --------------------------------------------------------------

/// Place a part. `0` means it took; anything else is an `EditError` code and
/// indexes `EDIT_LINES` in `web/ship.js`.
#[unsafe(no_mangle)]
pub extern "C" fn ship_place(kind: u32, x: u32, y: u32, rotation: u32) -> u32 {
    editor().place(kind, x, y, rotation)
}

#[unsafe(no_mangle)]
pub extern "C" fn ship_remove(part_id: u32) -> u32 {
    editor().remove(part_id)
}

// --- the stockpile --------------------------------------------------------

#[unsafe(no_mangle)]
pub extern "C" fn ship_resource_count() -> u32 {
    RESOURCE_COUNT as u32
}

#[unsafe(no_mangle)]
pub extern "C" fn ship_stock(resource: u32) -> u32 {
    editor()
        .budget
        .stockpile
        .get(resource as usize)
        .copied()
        .unwrap_or(0)
}

/// What is left of the stockpile. Never negative; `apply` refuses anything
/// that would take it there.
#[unsafe(no_mangle)]
pub extern "C" fn ship_remaining(resource: u32) -> u32 {
    let editor = editor();
    editor
        .budget
        .remaining(&editor.design)
        .get(resource as usize)
        .copied()
        .unwrap_or(0)
}

// --- what is wrong with it ------------------------------------------------

#[unsafe(no_mangle)]
pub extern "C" fn ship_issue_count() -> u32 {
    editor().issues().len() as u32
}

/// 0 for an error, 1 for a warning — `Severity`'s own discriminants.
#[unsafe(no_mangle)]
pub extern "C" fn ship_issue_severity(i: u32) -> u32 {
    editor()
        .issues()
        .get(i as usize)
        .map(|issue| match issue.severity {
            Severity::Error => 0,
            Severity::Warning => 1,
        })
        .unwrap_or(0)
}

/// The `IssueCode`, which indexes `ISSUE_LINES` in `web/ship.js`.
#[unsafe(no_mangle)]
pub extern "C" fn ship_issue_code(i: u32) -> u32 {
    editor()
        .issues()
        .get(i as usize)
        .map(|issue| issue.code)
        .unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn ship_issue_tile_count(i: u32) -> u32 {
    editor()
        .issues()
        .get(i as usize)
        .map(|issue| issue.tiles.len() as u32)
        .unwrap_or(0)
}

/// Whether anything in the list blocks Accept.
#[unsafe(no_mangle)]
pub extern "C" fn ship_has_errors() -> u32 {
    editor().has_errors() as u32
}

/// Ring one issue's tiles brighter than the rest, while the pointer rests on
/// its row. A highlight, not a tooltip: nothing is said, and it goes the
/// moment the pointer moves.
#[unsafe(no_mangle)]
pub extern "C" fn ship_focus_issue(i: u32) {
    let editor = editor();
    editor.focus = (i < editor.issues().len() as u32).then_some(i as usize);
}

#[unsafe(no_mangle)]
pub extern "C" fn ship_clear_focus() {
    editor().focus = None;
}

// --- accepting ------------------------------------------------------------

#[unsafe(no_mangle)]
pub extern "C" fn ship_players() -> u32 {
    editor().players
}

#[unsafe(no_mangle)]
pub extern "C" fn ship_local_slot() -> u32 {
    editor().local
}

/// The design's identity, in two halves because the boundary carries `u32`.
/// The host puts them back together only to hand them to `ship_accept`.
#[unsafe(no_mangle)]
pub extern "C" fn ship_hash_hi() -> u32 {
    (editor().hash() >> 32) as u32
}

#[unsafe(no_mangle)]
pub extern "C" fn ship_hash_lo() -> u32 {
    editor().hash() as u32
}

/// Record a player's Accept against a hash. `1` if it was taken.
///
/// Refused when the hash is not the design's — an Accept in flight when
/// somebody else placed a wall is an Accept for a ship that no longer exists
/// — and refused while there are errors.
#[unsafe(no_mangle)]
pub extern "C" fn ship_accept(slot: u32, hash_hi: u32, hash_lo: u32) -> u32 {
    let hash = ((hash_hi as u64) << 32) | hash_lo as u64;
    editor().accept(slot, hash) as u32
}

#[unsafe(no_mangle)]
pub extern "C" fn ship_unaccept(slot: u32) {
    editor().unaccept(slot);
}

#[unsafe(no_mangle)]
pub extern "C" fn ship_accepted(slot: u32) -> u32 {
    editor().accepted(slot) as u32
}

/// Which half of the page's life it is in: `Phase::Design` while the ship is
/// being laid out, `Phase::Finished` once everybody has accepted it.
///
/// **One export, not three.** "Is it finished", "may I still edit" and "which
/// phase is it" are the same question, and three ways of asking it is three
/// things that can disagree. `PHASE_DESIGN` in `web/ship.js` is the host's
/// half of the pair.
#[unsafe(no_mangle)]
pub extern "C" fn ship_phase() -> u32 {
    editor().phase as u32
}

/// The handoff to the play phase.
///
/// Stage 5 takes [`editor::Editor::finish_design`] in Rust — the design does
/// not cross the boundary, and this is only here to say whether there is one
/// to take. `ship_phase` is how the page asks the same thing.
pub fn finished_design() -> Option<&'static shipdesign::ShipDesign> {
    editor().finish_design()
}

// --- the handoff ----------------------------------------------------------
//
// Nothing consumes these yet. They are what the placeholder screen shows so
// that the numbers flight will one day want are computed and looked at now
// rather than discovered to be wrong later.

#[unsafe(no_mangle)]
pub extern "C" fn ship_part_total() -> u32 {
    editor().design.parts.len() as u32
}

/// What the finished ship weighs, crew aboard and nothing in the hold. `0.0`
/// for a design too light to be a ship — `physics` refuses one rather than
/// quoting an infinite acceleration.
#[unsafe(no_mangle)]
pub extern "C" fn ship_mass() -> f64 {
    let editor = editor();
    paint::debug_mass(&editor.design, editor.players)
}

/// Acceleration along one of the ship's own axes, in world units per game
/// minute squared. `axis` is a `physics::Facing`.
#[unsafe(no_mangle)]
pub extern "C" fn ship_acceleration(axis: u32) -> f64 {
    let editor = editor();
    let axis = *Facing::ALL.get(axis as usize).unwrap_or(&Facing::Forward);
    shipdesign::acceleration(&editor.design, editor.players, axis).unwrap_or(0.0)
}

// --- the self check -------------------------------------------------------

/// Every bit set means the wasm build agrees with the native one.
pub const SELF_CHECK_ALL: u32 = 0b11111;

/// What a native `cargo test` cannot answer: does *this target* get the same
/// answers?
///
/// `design_hash` is what an Accept is recorded against, so it has to be
/// identical on the wasm the players run and on the native server that will
/// one day be authoritative. Both ends compare the reference design against
/// the same pinned constants in `shipdesign::fixture`; the native end is
/// `crates/shipdesign/src/tests.rs` and this is the other. A target whose
/// arithmetic drifted would fail exactly one of the two.
///
/// Read by `scratchpad/ship-check.mjs`. A bitmask rather than a bool so a
/// failure says which half.
#[unsafe(no_mangle)]
pub extern "C" fn ship_self_check() -> u32 {
    use shipdesign::fixture::{CREWS, REFERENCE_HASH, REFERENCE_PARTS, reference};

    let mut bits = 0;
    if shipdesign::parts::defs_are_sound() {
        bits |= 1;
    }
    for (i, &crew) in CREWS.iter().enumerate() {
        let design = reference(crew);
        let right = shipdesign::design_hash(&design) == REFERENCE_HASH[i]
            && design.parts.len() as u32 == REFERENCE_PARTS[i];
        if right {
            bits |= 1 << (i + 1);
        }
    }
    if !shipdesign::has_errors(&shipdesign::validate(&reference(4), 4)) {
        bits |= 1 << 3;
    }
    if draw::STRIDE == 12 && RESOURCE_COUNT == ResourceId::ALL.len() {
        bits |= 1 << 4;
    }
    bits
}

/// The reference design's hash as this target computes it, so a failed
/// [`ship_self_check`] can be read rather than guessed at.
#[unsafe(no_mangle)]
pub extern "C" fn ship_reference_hash_hi(crew: u32) -> u32 {
    (shipdesign::design_hash(&shipdesign::fixture::reference(crew)) >> 32) as u32
}

#[unsafe(no_mangle)]
pub extern "C" fn ship_reference_hash_lo(crew: u32) -> u32 {
    shipdesign::design_hash(&shipdesign::fixture::reference(crew)) as u32
}
