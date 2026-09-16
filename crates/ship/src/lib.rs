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

pub mod camera;
pub mod draw;
pub mod editor;
pub mod game;
pub mod paint;
pub mod starfield;
pub mod view;
pub mod world_paint;

use editor::Editor;
use flight::Target;
use game::{Game, ViewMode};
use physics::{Facing, ResourceId};
use shipdesign::ShipDesign;
use shipdesign::parts::{PartKind, footprint};
use shipdesign::validate::Severity;
use shipdesign::{Money, Storage, TILE, storage, trade_price};
use world::world::Command;
use world::{Speed, WorldEvent};
use worldgen::{GalaxyType, Node};

/// wasm is single-threaded and the host drives every call, so one global is
/// both sufficient and safe in practice. Same arrangement as the room's.
static mut EDITOR: Option<Editor> = None;
static mut GAME: Option<Game> = None;
static mut LIST: Option<draw::DrawList> = None;

/// What the lobby asked for, kept from `ship_init` until Accept hands it to
/// the world. Two numbers, because that is all a galaxy is.
static mut SEED: u64 = world::data::DEFAULT_SEED;
static mut GALAXY: u32 = 0;

fn editor() -> &'static mut Editor {
    unsafe {
        (*(&raw mut EDITOR))
            .as_mut()
            .expect("ship_init was not called")
    }
}

/// The game, once there is one. `None` for the whole of the design phase,
/// which is why almost every export below has a two-armed match in it rather
/// than a second set of exports: "how much fuel is aboard" is one question
/// whichever half of the page is up.
fn game() -> Option<&'static mut Game> {
    unsafe { (*(&raw mut GAME)).as_mut() }
}

/// The live ship: the game's if there is one, the design being laid out if
/// there is not.
fn design() -> &'static ShipDesign {
    match game() {
        Some(game) => &game.world.ship.design,
        None => &editor().design,
    }
}

/// Open the world with the accepted design. Called the moment the last Accept
/// lands and at no other time.
fn start_game() {
    if game().is_some() {
        return;
    }
    let (seed, galaxy) = unsafe { (SEED, GALAXY) };
    let galaxy_type = GalaxyType::ALL
        .get(galaxy as usize)
        .copied()
        .unwrap_or(GalaxyType::SpiralTwoArm);
    let editor = editor();
    let Some(design) = editor.finish_design().cloned() else {
        return;
    };
    let money = editor.budget.remaining(&editor.design);
    let started = Game::start(
        design,
        money,
        editor.players,
        editor.local,
        seed,
        galaxy_type,
        editor.view.width,
        editor.view.height,
    );
    unsafe { GAME = started }
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
///
/// `seed_hi`/`seed_lo` and `galaxy` are the world the game will open in, and
/// they are **temporary in exactly one way**: today they come off the query
/// string with a fixed default behind them, and when the lobby's World tab
/// exists they will come off that instead. Nothing else about them changes —
/// a galaxy has always been a seed and a type.
#[unsafe(no_mangle)]
pub extern "C" fn ship_init(
    build_area: u32,
    money_per_bim_hi: u32,
    money_per_bim_lo: u32,
    players: u32,
    local_slot: u32,
    seed_hi: u32,
    seed_lo: u32,
    galaxy: u32,
    width: f32,
    height: f32,
) {
    unsafe {
        SEED = ((seed_hi as u64) << 32) | seed_lo as u64;
        GALAXY = galaxy;
        GAME = None;
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

/// The seed a page opened with no lobby behind it gets, in the two halves
/// everything 64-bit crosses in.
///
/// Exported rather than written down in `web/ship.js` so there is **one** copy
/// of it: a default that exists in two places is a default that will disagree
/// with itself, and two players in the same lobby would end up in two
/// galaxies. It goes when the lobby's World tab arrives.
#[unsafe(no_mangle)]
pub extern "C" fn ship_default_seed_hi() -> u32 {
    (world::data::DEFAULT_SEED >> 32) as u32
}

#[unsafe(no_mangle)]
pub extern "C" fn ship_default_seed_lo() -> u32 {
    world::data::DEFAULT_SEED as u32
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
    if let Some(game) = game() {
        game.resize(width, height);
    }
}

/// Rebuild the shape buffer.
///
/// A redraw and **never a step**: the world is advanced by `ship_world_step`,
/// which the page calls however many times a frame is worth. Folding the two
/// together would tie the simulation to the display's refresh rate, which is
/// the one thing a fixed step exists to avoid.
#[unsafe(no_mangle)]
pub extern "C" fn ship_render() {
    match game() {
        Some(game) => world_paint::paint(game, list()),
        None => paint::paint(editor(), list()),
    }
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

// One transform for both halves of the page, so `web/ship.js` has one paint
// loop rather than two. What changes is what the origin *is*: the corner of
// the build area during the design phase, and the ship itself once the game
// has started — see `crates/ship/src/camera.rs`.

#[unsafe(no_mangle)]
pub extern "C" fn ship_view_scale() -> f32 {
    match game() {
        Some(game) => game.camera().scale(),
        None => editor().view.scale(),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ship_view_x() -> f32 {
    match game() {
        Some(game) => game.camera().offset_x(),
        None => editor().view.offset_x(),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ship_view_y() -> f32 {
    match game() {
        Some(game) => game.camera().offset_y(),
        None => editor().view.offset_y(),
    }
}

/// Shove the view by a screen-pixel delta. Middle-drag and WASD both come
/// through here.
#[unsafe(no_mangle)]
pub extern "C" fn ship_pan(dx: f32, dy: f32) {
    match game() {
        Some(game) => game.camera_mut().pan(dx, dy),
        None => editor().view.pan(dx, dy),
    }
}

/// Zoom about a point on the canvas. `factor` is a multiplier the host works
/// out from the wheel, so no exponential has to be linked into the wasm for
/// the sake of a scroll.
#[unsafe(no_mangle)]
pub extern "C" fn ship_zoom(at_x: f32, at_y: f32, factor: f32) {
    match game() {
        Some(game) => game.camera_mut().zoom(at_x, at_y, factor),
        None => editor().view.zoom(at_x, at_y, factor),
    }
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

/// What is left of it.
///
/// Never negative; `apply` refuses anything that would take it there, and
/// during the design phase it is **derived from the design** every time rather
/// than decremented as parts go down.
///
/// Once the game has started it is the world's figure instead — the same
/// money, carried across at Accept and not converted into anything, and now
/// spent and earned at stations rather than derived from a ship.
#[unsafe(no_mangle)]
pub extern "C" fn ship_remaining_hi() -> u32 {
    hi(remaining())
}

#[unsafe(no_mangle)]
pub extern "C" fn ship_remaining_lo() -> u32 {
    lo(remaining())
}

fn remaining() -> Money {
    match game() {
        Some(game) => game.world.money,
        None => {
            let editor = editor();
            editor.budget.remaining(&editor.design)
        }
    }
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

/// Units of it aboard the **live** ship: the design being laid out, or the one
/// the game is flying. One question, one export.
#[unsafe(no_mangle)]
pub extern "C" fn ship_cargo(resource: u32) -> u32 {
    with_resource(resource, |id| design().carrying(id))
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
    with_storage(class, |c| design().capacity(c))
}

/// How much of it is taken up by what is aboard.
#[unsafe(no_mangle)]
pub extern "C" fn ship_storage_used(class: u32) -> u32 {
    with_storage(class, |c| design().stored(c))
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
/// The last Accept is what opens the world. There is no separate "start"
/// call: the design phase ending and the game beginning are one event, and two
/// exports for it would be two things that can disagree about which ship got
/// handed over.
#[unsafe(no_mangle)]
pub extern "C" fn ship_accept(slot: u32, hash_hi: u32, hash_lo: u32) -> u32 {
    let hash = ((hash_hi as u64) << 32) | hash_lo as u64;
    let took = editor().accept(slot, hash);
    if editor().phase == editor::Phase::Game {
        start_game();
    }
    took as u32
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
/// being laid out, `Phase::Game` once everybody has accepted it.
///
/// **One export, not three.** "Is it finished", "may I still edit" and "which
/// phase is it" are the same question, and three ways of asking it is three
/// things that can disagree. `PHASE_DESIGN` in `web/ship.js` is the host's
/// half of the pair.
#[unsafe(no_mangle)]
pub extern "C" fn ship_phase() -> u32 {
    editor().phase as u32
}

// --- what the ship is, in either phase ------------------------------------
//
// The handoff screen showed these before there was a game to hand to. They
// are now the game's own readouts and they read the **live** ship, which is
// the whole reason they were computed and looked at early: the numbers flight
// would one day want are the numbers flight now uses.

#[unsafe(no_mangle)]
pub extern "C" fn ship_part_total() -> u32 {
    design().parts.len() as u32
}

/// What the ship weighs, crew and cargo included. `0.0` for a design too
/// light to be a ship — `physics` refuses one rather than quoting an infinite
/// acceleration.
#[unsafe(no_mangle)]
pub extern "C" fn ship_mass() -> f64 {
    match game() {
        Some(game) => game.world.ship.dynamics.mass.get(),
        None => {
            let editor = editor();
            paint::debug_mass(&editor.design, editor.players)
        }
    }
}

/// Acceleration along one of the ship's own axes, in world units per game
/// minute squared. `axis` is a `physics::Facing`.
#[unsafe(no_mangle)]
pub extern "C" fn ship_acceleration(axis: u32) -> f64 {
    let axis = *Facing::ALL.get(axis as usize).unwrap_or(&Facing::Forward);
    let crew = match game() {
        Some(game) => game.world.ship.crew_count,
        None => editor().players,
    };
    shipdesign::acceleration(design(), crew, axis).unwrap_or(0.0)
}

// --- the game -------------------------------------------------------------
//
// Everything below is `None` until the last Accept lands, and every one of
// them answers with a nought rather than reaching into an editor that is no
// longer the point. A page that asks about a trip during the design phase is
// asking a question with no answer, not making a mistake.

/// Whether the world is open. The page shows the game view on this.
#[unsafe(no_mangle)]
pub extern "C" fn ship_world_ready() -> u32 {
    game().is_some() as u32
}

/// Advance the world by exactly one step.
///
/// **The page decides how many.** It keeps an accumulator, works out what a
/// frame is worth at the effective speed, and calls this that many times —
/// see `MAX_STEPS_PER_FRAME` in `web/ship.js`. Putting the accumulator in here
/// would give the wasm an opinion about real time, which it has no way to
/// measure.
#[unsafe(no_mangle)]
pub extern "C" fn ship_world_step() {
    if let Some(game) = game() {
        game.step();
    }
}

/// How many steps a second of real time is worth at 1x.
///
/// The page turns a frame into steps with it. It is a fact about the world
/// rather than about the browser, so it comes from here: a 60 written down in
/// `web/ship.js` would be a second copy of `world::data::STEP_MINUTES` waiting
/// to disagree with the first.
#[unsafe(no_mangle)]
pub extern "C" fn ship_steps_per_second() -> f64 {
    game()
        .map(|g| g.world.steps_per_second())
        .unwrap_or(time::MINUTES_PER_SECOND / world::data::STEP_MINUTES)
}

/// How many steps have been taken. The stamp a command carries.
///
/// `f64` rather than the `u64` it really is: the boundary carries `u32`, and a
/// double holds a whole step count exactly for rather longer than anybody will
/// be playing.
#[unsafe(no_mangle)]
pub extern "C" fn ship_world_steps() -> f64 {
    game().map(|g| g.world.steps as f64).unwrap_or(0.0)
}

#[unsafe(no_mangle)]
pub extern "C" fn ship_world_day() -> u32 {
    game().map(|g| g.world.day()).unwrap_or(0)
}

/// Minutes into the day. The host turns it into a clock face; no strings
/// cross this boundary.
#[unsafe(no_mangle)]
pub extern "C" fn ship_world_minutes() -> f64 {
    game().map(|g| g.world.minutes_into_day()).unwrap_or(0.0)
}

// --- how fast ---------------------------------------------------------------

#[unsafe(no_mangle)]
pub extern "C" fn ship_speed_count() -> u32 {
    Speed::ALL.len() as u32
}

/// What a speed is worth, as a plain multiplier. `0` is the pause.
#[unsafe(no_mangle)]
pub extern "C" fn ship_speed_multiplier(code: u32) -> u32 {
    Speed::from_code(code).map(|s| s.multiplier()).unwrap_or(0)
}

/// What this player has asked for. The panel shows every slot's, which is what
/// makes "why are we crawling" answerable without anybody having to say so.
#[unsafe(no_mangle)]
pub extern "C" fn ship_speed_request(slot: u32) -> u32 {
    game().map(|g| g.requested(slot).code()).unwrap_or(0)
}

/// What the world is actually running at: the slowest request there is.
#[unsafe(no_mangle)]
pub extern "C" fn ship_effective_speed() -> u32 {
    game()
        .map(|g| g.world.effective_speed().code())
        .unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn ship_cmd_speed(slot: u32, code: u32) {
    let Some(speed) = Speed::from_code(code) else {
        return;
    };
    if let Some(game) = game() {
        game.send(Command::SetSpeed { slot, speed });
    }
}

// --- where the ship is ------------------------------------------------------

#[unsafe(no_mangle)]
pub extern "C" fn ship_world_x() -> f64 {
    game().map(|g| g.world.ship.position().x).unwrap_or(0.0)
}

#[unsafe(no_mangle)]
pub extern "C" fn ship_world_y() -> f64 {
    game().map(|g| g.world.ship.position().y).unwrap_or(0.0)
}

/// Radians, 0 at north and growing clockwise. See `flight::angle`.
#[unsafe(no_mangle)]
pub extern "C" fn ship_world_heading() -> f64 {
    game().map(|g| g.world.ship.heading).unwrap_or(0.0)
}

/// World units a game minute. Zero unless the ship is actually under way.
#[unsafe(no_mangle)]
pub extern "C" fn ship_world_speed() -> f64 {
    game()
        .and_then(|g| g.world.trip_state())
        .map(|s| s.speed)
        .unwrap_or(0.0)
}

/// Docked, holding, or travelling — `world::ShipState`'s own codes.
#[unsafe(no_mangle)]
pub extern "C" fn ship_world_state() -> u32 {
    game().map(|g| g.world.ship.state.code()).unwrap_or(0)
}

/// Which part of the trip it is in — a `flight::Phase`, indexing `PHASE_NAMES`.
#[unsafe(no_mangle)]
pub extern "C" fn ship_trip_phase() -> u32 {
    game()
        .map(|game| world_paint::phase_code(game))
        .unwrap_or(0)
}

/// Whether the trip under way is a **stop** rather than a trip.
///
/// A redirect is an abort followed by a departure, so for a while the ship is
/// braking towards nowhere in particular. "Braking" is the true answer and
/// "Stopping" is the useful one: a player who has just asked to go somewhere
/// else wants to know that is what is happening.
#[unsafe(no_mangle)]
pub extern "C" fn ship_trip_aborting() -> u32 {
    game()
        .and_then(|g| g.world.plan())
        .is_some_and(|plan| plan.aborting) as u32
}

/// The station it is tied to, **plus one**, or 0 for nowhere. There is no
/// negative `u32` to say "nothing" with, and station ids start at zero.
#[unsafe(no_mangle)]
pub extern "C" fn ship_docked_at() -> u32 {
    match game().map(|g| &g.world.ship.state) {
        Some(world::ShipState::Docked { station }) => station + 1,
        _ => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ship_fuel_aboard() -> u32 {
    game().map(|g| g.world.ship.fuel_aboard()).unwrap_or(0)
}

/// Fuel spoken for by the trip under way. It cannot be sold and no other trip
/// may be planned against it.
#[unsafe(no_mangle)]
pub extern "C" fn ship_fuel_reserved() -> u32 {
    game().map(|g| g.world.ship.reserved_fuel).unwrap_or(0)
}

/// Whose route is on the map, **plus one**, or 0 if nobody has set one.
#[unsafe(no_mangle)]
pub extern "C" fn ship_destination_by() -> u32 {
    game()
        .and_then(|g| g.world.ship.destination_set_by)
        .map(|slot| slot + 1)
        .unwrap_or(0)
}

/// 0 in open space, 1 alongside something.
#[unsafe(no_mangle)]
pub extern "C" fn ship_frame_kind() -> u32 {
    game().map(|g| g.world.ship.frame.code()).unwrap_or(0)
}

/// What it is alongside: 0 for a body and 1 for a station, matching the map
/// list below. Meaningless while `ship_frame_kind` is 0.
#[unsafe(no_mangle)]
pub extern "C" fn ship_frame_node_kind() -> u32 {
    match game().and_then(|g| g.world.ship.frame.node()) {
        Some(Node::Body(_)) => 0,
        Some(Node::Station(_)) => 1,
        None => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ship_frame_node_id() -> u32 {
    match game().and_then(|g| g.world.ship.frame.node()) {
        Some(Node::Body(id)) | Some(Node::Station(id)) => id,
        None => 0,
    }
}

/// How far the crew can see, in world units. Eyesight, or the sensor arrays.
#[unsafe(no_mangle)]
pub extern "C" fn ship_detection_range() -> f64 {
    game().map(|g| g.world.detection_range()).unwrap_or(0.0)
}

/// The world's identity, for a client one day comparing itself against a
/// server. In two halves like every other 64-bit number here.
#[unsafe(no_mangle)]
pub extern "C" fn ship_world_checksum_hi() -> u32 {
    (game().map(|g| g.world.checksum()).unwrap_or(0) >> 32) as u32
}

#[unsafe(no_mangle)]
pub extern "C" fn ship_world_checksum_lo() -> u32 {
    game().map(|g| g.world.checksum()).unwrap_or(0) as u32
}

// --- the map -----------------------------------------------------------------
//
// Only what the crew have found. Undiscovered things are not in the list at
// all rather than being in it and hidden: a host that could count them could
// leak how much system there is left to explore.

#[unsafe(no_mangle)]
pub extern "C" fn ship_map_count() -> u32 {
    game().map(|g| g.world.discovered.len() as u32).unwrap_or(0)
}

fn map_node(i: u32) -> Option<Node> {
    game()?.world.discovered.get(i as usize).copied()
}

/// 0 for a body, 1 for a station.
#[unsafe(no_mangle)]
pub extern "C" fn ship_map_kind(i: u32) -> u32 {
    match map_node(i) {
        Some(Node::Body(_)) => 0,
        Some(Node::Station(_)) => 1,
        None => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ship_map_id(i: u32) -> u32 {
    match map_node(i) {
        Some(Node::Body(id)) | Some(Node::Station(id)) => id,
        None => 0,
    }
}

/// What sort of thing it is: a `worldgen::BodyKind` or `StationKind`, which
/// the host reads against `BODY_KIND_NAMES` or `STATION_KIND_NAMES` depending
/// on `ship_map_kind`.
#[unsafe(no_mangle)]
pub extern "C" fn ship_map_type(i: u32) -> u32 {
    let Some(game) = game() else { return 0 };
    match map_node(i) {
        Some(Node::Body(id)) => game
            .world
            .system
            .body(id)
            .map(|b| b.kind as u32)
            .unwrap_or(0),
        Some(Node::Station(id)) => game
            .world
            .system
            .station(id)
            .map(|s| s.kind as u32)
            .unwrap_or(0),
        None => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ship_map_x(i: u32) -> f64 {
    map_position(i).map(|at| at.x).unwrap_or(0.0)
}

#[unsafe(no_mangle)]
pub extern "C" fn ship_map_y(i: u32) -> f64 {
    map_position(i).map(|at| at.y).unwrap_or(0.0)
}

fn map_position(i: u32) -> Option<worldgen::math::DVec2> {
    let game = game()?;
    game.world.system.absolute_position(map_node(i)?)
}

/// Which discovered thing a click on the map is near enough to, **plus one**,
/// or 0 for empty space — which is a perfectly good place to fly to.
#[unsafe(no_mangle)]
pub extern "C" fn ship_map_pick(x: f32, y: f32, slop: f32) -> u32 {
    let Some(game) = game() else { return 0 };
    let Some(node) = game.pick(x, y, slop) else {
        return 0;
    };
    game.world
        .discovered
        .iter()
        .position(|&n| n == node)
        .map(|i| i as u32 + 1)
        .unwrap_or(0)
}

/// Where a point on the canvas is, in the system.
#[unsafe(no_mangle)]
pub extern "C" fn ship_map_point_x(x: f32, y: f32) -> f64 {
    game().map(|g| g.point_at(x, y).x).unwrap_or(0.0)
}

#[unsafe(no_mangle)]
pub extern "C" fn ship_map_point_y(x: f32, y: f32) -> f64 {
    game().map(|g| g.point_at(x, y).y).unwrap_or(0.0)
}

// --- planning a trip ----------------------------------------------------------
//
// A preview is worked out for the **local player only** and is never a
// command: two players hovering over different planets must not be an argument
// about where the ship is going.

fn target_of(kind: u32, id: u32) -> Target {
    if kind == 1 {
        Target::Station(id)
    } else {
        Target::Body(id)
    }
}

/// Quote a trip to the `i`th thing on the map.
#[unsafe(no_mangle)]
pub extern "C" fn ship_preview_node(i: u32) {
    let Some(node) = map_node(i) else { return };
    let target = match node {
        Node::Body(id) => Target::Body(id),
        Node::Station(id) => Target::Station(id),
    };
    if let Some(game) = game() {
        game.preview(target);
    }
}

/// Quote a trip to a bare point in space.
#[unsafe(no_mangle)]
pub extern "C" fn ship_preview_point(x: f64, y: f64) {
    if let Some(game) = game() {
        game.preview(Target::Point(worldgen::math::dvec2(x, y)));
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ship_preview_clear() {
    if let Some(game) = game() {
        game.clear_preview();
    }
}

/// 0 for nothing quoted, 1 for a trip, 2 for a refusal. Three answers rather
/// than two, because "no preview yet" and "that trip cannot be flown" are
/// different things to show.
#[unsafe(no_mangle)]
pub extern "C" fn ship_preview_state() -> u32 {
    match game().and_then(|g| g.preview.as_ref()) {
        None => 0,
        Some(Ok(_)) => 1,
        Some(Err(_)) => 2,
    }
}

/// A `flight::PlanError`, indexing `PLAN_ERRORS`. Meaningless unless
/// `ship_preview_state` is 2.
#[unsafe(no_mangle)]
pub extern "C" fn ship_preview_error() -> u32 {
    match game().and_then(|g| g.preview.as_ref()) {
        Some(Err(why)) => why.code(),
        _ => 0,
    }
}

/// The whole trip, in game minutes, **stopping first included**.
#[unsafe(no_mangle)]
pub extern "C" fn ship_preview_minutes() -> f64 {
    preview_field(|p| p.minutes)
}

/// How much of that is coming to rest before setting off. Zero from a
/// standstill, and often most of the bill when a trip is already under way.
#[unsafe(no_mangle)]
pub extern "C" fn ship_preview_stopping() -> f64 {
    preview_field(|p| p.stopping)
}

#[unsafe(no_mangle)]
pub extern "C" fn ship_preview_fuel() -> f64 {
    preview_field(|p| p.fuel)
}

/// Whether arriving means docking. A station with no airlock to get out of is
/// a station to hold beside.
#[unsafe(no_mangle)]
pub extern "C" fn ship_preview_docks() -> u32 {
    preview_field(|p| u32::from(p.docks) as f64) as u32
}

fn preview_field(read: impl Fn(&world::Preview) -> f64) -> f64 {
    match game().and_then(|g| g.preview.as_ref()) {
        Some(Ok(preview)) => read(preview),
        _ => 0.0,
    }
}

// --- orders --------------------------------------------------------------------
//
// Queued rather than applied. A command lands at a **step**, the same step for
// everybody, which is the whole reason `web/ship.js` stamps each one with
// `ship_world_steps() + 1` on its way through `net`.

#[unsafe(no_mangle)]
pub extern "C" fn ship_cmd_confirm_node(slot: u32, kind: u32, id: u32) {
    if let Some(game) = game() {
        game.send(Command::Confirm {
            slot,
            target: target_of(kind, id),
        });
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ship_cmd_confirm_point(slot: u32, x: f64, y: f64) {
    if let Some(game) = game() {
        game.send(Command::Confirm {
            slot,
            target: Target::Point(worldgen::math::dvec2(x, y)),
        });
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ship_cmd_abort(slot: u32) {
    if let Some(game) = game() {
        game.send(Command::Abort { slot });
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ship_cmd_buy(slot: u32, resource: u32, units: u32) {
    let Some(resource) = ResourceId::ALL.get(resource as usize).copied() else {
        return;
    };
    if let Some(game) = game() {
        game.send(Command::Buy {
            slot,
            resource,
            units,
        });
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ship_cmd_sell(slot: u32, resource: u32, units: u32) {
    let Some(resource) = ResourceId::ALL.get(resource as usize).copied() else {
        return;
    };
    if let Some(game) = game() {
        game.send(Command::Sell {
            slot,
            resource,
            units,
        });
    }
}

// --- what happened -------------------------------------------------------------
//
// The same arrangement as the room's diary: a code and a number, with
// `EVENT_LINES` in `web/ship.js` holding the sentences. An event whose code
// has no line there is **dropped from the page** rather than shown blank, and
// what catches a missing one is the row count against `ship_event_count`.

#[unsafe(no_mangle)]
pub extern "C" fn ship_event_count() -> u32 {
    game().map(|g| g.events.len() as u32).unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn ship_event_code(i: u32) -> u32 {
    event(i).map(WorldEvent::code).unwrap_or(0)
}

/// The one number the sentence needs. Which number it is depends on the code,
/// exactly as `MEMORY_LINES` works in the room.
#[unsafe(no_mangle)]
pub extern "C" fn ship_event_value(i: u32) -> f64 {
    event(i).map(|e| e.value() as f64).unwrap_or(0.0)
}

fn event(i: u32) -> Option<WorldEvent> {
    game()?.events.get(i as usize).copied()
}

/// Drop everything the page has read. Called once a frame after the rows are
/// drawn — an event is a thing that happened once.
#[unsafe(no_mangle)]
pub extern "C" fn ship_events_clear() {
    if let Some(game) = game() {
        game.events.clear();
    }
}

// --- the two views ---------------------------------------------------------------

#[unsafe(no_mangle)]
pub extern "C" fn ship_view_mode() -> u32 {
    game().map(|g| g.mode as u32).unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn ship_set_view_mode(mode: u32) {
    if let Some(game) = game() {
        game.set_mode(if mode == 1 {
            ViewMode::Map
        } else {
            ViewMode::Ship
        });
    }
}

/// The pointer, over the ship view. Turned back through the heading, so a
/// click lands on the tile it looks like it landed on at any heading.
#[unsafe(no_mangle)]
pub extern "C" fn ship_game_hover(x: f32, y: f32) {
    if let Some(game) = game() {
        game.hover = Some(game.tile_at(x, y));
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ship_game_leave() {
    if let Some(game) = game() {
        game.hover = None;
    }
}

/// Whether the pointer is over the hull at all. The two below are meaningless
/// when it is not, and there is no negative `u32` to say so with.
#[unsafe(no_mangle)]
pub extern "C" fn ship_game_tile_inside() -> u32 {
    match game().and_then(|g| g.hover.map(|t| (g, t))) {
        Some((game, tile)) => game.world.ship.design.holds(tile) as u32,
        None => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ship_game_tile_x() -> i32 {
    game().and_then(|g| g.hover).map(|t| t.0).unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn ship_game_tile_y() -> i32 {
    game().and_then(|g| g.hover).map(|t| t.1).unwrap_or(0)
}

// --- the self check -------------------------------------------------------

/// Every bit set means the wasm build agrees with the native one.
pub const SELF_CHECK_ALL: u32 = 0b1111111;

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
    // And the **world**: a fixed scenario, stepped a fixed number of times,
    // checksummed. This is the one that catches a target that disagrees about
    // flying rather than about building — `crates/world`'s
    // `the_reference_run_comes_out_at_the_number_it_is_pinned_to` is the other
    // half, and a drift fails exactly one of the two.
    if world::fixture::reference_run() == world::fixture::REFERENCE_CHECKSUM {
        bits |= 1 << 6;
    }
    bits
}

/// The world checksum as this target computes it, so a failed
/// [`ship_self_check`] can be read rather than guessed at.
#[unsafe(no_mangle)]
pub extern "C" fn ship_reference_checksum_hi() -> u32 {
    (world::fixture::reference_run() >> 32) as u32
}

#[unsafe(no_mangle)]
pub extern "C" fn ship_reference_checksum_lo() -> u32 {
    world::fixture::reference_run() as u32
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

#[cfg(test)]
mod tests;
