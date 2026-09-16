//! The ship designer, as the browser sees it.
//!
//! The design phase runs here: `web/ship.html` hands this module a canvas
//! size and the numbers the lobby chose, then each frame asks it to rebuild a
//! shape buffer and reads that buffer straight out of wasm memory.
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
//! **No strings cross it.** The parts, the prices, the reasons an edit was
//! refused and the faults in a design are all numbers, and `PART_NAMES`,
//! `EDIT_LINES` and `ISSUE_LINES` in `web/ship.js` are where the words live.
//! Adding a part is an enum variant, a name in that file, and the range check
//! in the tests.
//!
//! **Money crosses in two halves.** A [`shipdesign::Money`] is a `u64` and
//! the boundary carries `u32`, so every money export is a `_hi` and a `_lo`
//! — the same arrangement `ship_hash_hi`/`ship_hash_lo` has been using for
//! the design hash. The host puts the two back together; the euro sign and
//! the digit grouping are its business and are never made in here.

mod draw;
mod editor;
mod paint;
mod view;

use editor::Editor;
use physics::{Facing, ResourceId};
use shipdesign::parts::{PartKind, footprint};
use shipdesign::validate::Severity;
use shipdesign::{Money, Storage, TILE, storage, trade_price};

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
/// `build_area` is tiles a side and `money_per_bim` is what each of the crew
/// brings, in whole euros and in the two halves money always crosses in. Both,
/// with `players` and `local_slot`, come off the query string
/// `web/builder.js` navigated with, and every one of them is a number because
/// nothing else crosses this boundary.
///
/// The pool is `economy::starting_pool` of those two, worked out in
/// [`editor::Editor::new`] so that the wasm and a native server arrive at it
/// the same way.
#[unsafe(no_mangle)]
pub extern "C" fn ship_init(
    build_area: u32,
    money_per_bim_hi: u32,
    money_per_bim_lo: u32,
    players: u32,
    local_slot: u32,
    width: f32,
    height: f32,
) {
    unsafe {
        EDITOR = Some(Editor::new(
            build_area,
            money(money_per_bim_hi, money_per_bim_lo),
            players,
            local_slot,
            width,
            height,
        ))
    }
}

/// Two `u32` halves back into one [`Money`]. The other direction is
/// [`hi`] and [`lo`] below.
fn money(hi: u32, lo: u32) -> Money {
    ((hi as Money) << 32) | lo as Money
}

/// The top half of a money figure, for an export that hands one out.
fn hi(amount: Money) -> u32 {
    (amount >> 32) as u32
}

fn lo(amount: Money) -> u32 {
    amount as u32
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

/// What one of them costs, in whole euros. In two halves, like every other
/// money export.
///
/// The palette shows it, and so does the readout by the pointer.
#[unsafe(no_mangle)]
pub extern "C" fn ship_part_price_hi(kind: u32) -> u32 {
    hi(with_kind(kind, |k| k.def().price))
}

#[unsafe(no_mangle)]
pub extern "C" fn ship_part_price_lo(kind: u32) -> u32 {
    lo(with_kind(kind, |k| k.def().price))
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

// --- the money ------------------------------------------------------------

/// What the crew have between them: everybody's money, plus the lone
/// player's bonus. Fixed for the whole phase.
#[unsafe(no_mangle)]
pub extern "C" fn ship_pool_hi() -> u32 {
    hi(editor().budget.pool)
}

#[unsafe(no_mangle)]
pub extern "C" fn ship_pool_lo() -> u32 {
    lo(editor().budget.pool)
}

/// What is left of it. Never negative; `apply` refuses anything that would
/// take it there, and it is **derived from the design** every time rather
/// than decremented as parts go down.
#[unsafe(no_mangle)]
pub extern "C" fn ship_remaining_hi() -> u32 {
    let editor = editor();
    hi(editor.budget.remaining(&editor.design))
}

#[unsafe(no_mangle)]
pub extern "C" fn ship_remaining_lo() -> u32 {
    let editor = editor();
    lo(editor.budget.remaining(&editor.design))
}

// --- the station's goods, and the hold ------------------------------------
//
// Supply is unlimited and every station charges the same; what bounds a
// purchase is the pool and the ship. All three of those are `economy` and
// `shipdesign` — nothing here decides anything, it only asks.

#[unsafe(no_mangle)]
pub extern "C" fn ship_resource_count() -> u32 {
    ResourceId::ALL.len() as u32
}

/// What a unit of it costs at the station.
#[unsafe(no_mangle)]
pub extern "C" fn ship_trade_price_hi(resource: u32) -> u32 {
    hi(with_resource(resource, trade_price))
}

#[unsafe(no_mangle)]
pub extern "C" fn ship_trade_price_lo(resource: u32) -> u32 {
    lo(with_resource(resource, trade_price))
}

/// Units of it aboard.
#[unsafe(no_mangle)]
pub extern "C" fn ship_cargo(resource: u32) -> u32 {
    let editor = editor();
    with_resource(resource, |id| editor.design.carrying(id))
}

/// Which class of storage it is stowed in — a `Storage` code, which indexes
/// `STORAGE_NAMES` in `web/ship.js`.
#[unsafe(no_mangle)]
pub extern "C" fn ship_storage_of(resource: u32) -> u32 {
    with_resource(resource, |id| storage(id).code())
}

#[unsafe(no_mangle)]
pub extern "C" fn ship_storage_count() -> u32 {
    Storage::ALL.len() as u32
}

/// How much of that class the ship has, over every part that provides it.
#[unsafe(no_mangle)]
pub extern "C" fn ship_storage_capacity(class: u32) -> u32 {
    let editor = editor();
    with_storage(class, |c| editor.design.capacity(c))
}

/// How much of it is taken up by what is aboard.
#[unsafe(no_mangle)]
pub extern "C" fn ship_storage_used(class: u32) -> u32 {
    let editor = editor();
    with_storage(class, |c| editor.design.stored(c))
}

/// Take goods aboard. `0` means it took; anything else is an `EditError`
/// code and indexes `EDIT_LINES`, the same as a placement.
#[unsafe(no_mangle)]
pub extern "C" fn ship_buy(resource: u32, units: u32) -> u32 {
    editor().buy(resource, units)
}

#[unsafe(no_mangle)]
pub extern "C" fn ship_sell(resource: u32, units: u32) -> u32 {
    editor().sell(resource, units)
}

fn with_resource<T: Default>(resource: u32, f: impl FnOnce(ResourceId) -> T) -> T {
    ResourceId::ALL
        .get(resource as usize)
        .copied()
        .map(f)
        .unwrap_or_default()
}

fn with_storage<T: Default>(class: u32, f: impl FnOnce(Storage) -> T) -> T {
    Storage::from_code(class).map(f).unwrap_or_default()
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

/// How many tiles the outside can see into.
///
/// The tint itself is painted in here — it is on whenever the map is not
/// empty, whatever the issue list is doing — so this is for the page's own
/// readout and for the harness. `ship_issue_tile_count` says the same thing
/// about the warning row; this says it without going through the list.
#[unsafe(no_mangle)]
pub extern "C" fn ship_exposed_count() -> u32 {
    editor().exposed().len() as u32
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
pub const SELF_CHECK_ALL: u32 = 0b111111;

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
    // The draw format, and the money arithmetic: a lone player's pool is
    // their own money plus the bonus, and a crew's is nothing but their own.
    // It is the one sum two machines have to agree on down to the euro.
    let solo = economy::starting_pool(100_000, 1) == Ok(120_000);
    let crew = economy::starting_pool(100_000, 4) == Ok(400_000);
    let none = economy::starting_pool(100_000, 0).is_err();
    if draw::STRIDE == 12 && solo && crew && none {
        bits |= 1 << 4;
    }
    // The reference is carrying what it is meant to carry, and the sealed
    // hull is sealed. Both are in the hash now, and both are the kind of
    // thing that could come out differently on a target with a different
    // idea of what a `u32` sums to.
    let design = reference(4);
    let stowed = shipdesign::fixture::REFERENCE_CARGO
        .iter()
        .all(|&(id, units)| design.carrying(id) == units);
    if stowed && shipdesign::exposure(&design).is_empty() {
        bits |= 1 << 5;
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
