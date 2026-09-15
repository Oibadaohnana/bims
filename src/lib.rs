//! Bims — a 2D top-down game.
//!
//! The whole simulation lives in Rust, compiled to `wasm32-unknown-unknown`.
//! Each frame the host calls [`bims_update`], then reads the resulting shape
//! buffer straight out of wasm memory and paints it to a canvas. The only
//! things crossing the boundary are a few numbers and one pointer, so there
//! is no binding generator anywhere in the build.

mod bath;
mod character;
mod clock;
mod draw;
mod game;
mod math;
mod nav;
mod rng;
mod room;
mod task;

use game::Game;

/// wasm is single-threaded and the host drives every call, so one global is
/// both sufficient and safe in practice.
static mut GAME: Option<Game> = None;

fn game() -> &'static mut Game {
    unsafe {
        (*(&raw mut GAME))
            .as_mut()
            .expect("bims_init was not called")
    }
}

/// Floats per shape in the draw buffer. The host uses this to walk the slice.
#[unsafe(no_mangle)]
pub extern "C" fn bims_stride() -> usize {
    draw::STRIDE
}

/// Start a world `width` x `height` CSS pixels big. `seed_hi`/`seed_lo` come
/// from the host so each page load wanders somewhere new.
#[unsafe(no_mangle)]
pub extern "C" fn bims_init(seed_hi: u32, seed_lo: u32, width: f32, height: f32) {
    let seed = ((seed_hi as u64) << 32) | seed_lo as u64;
    unsafe { GAME = Some(Game::new(seed, width, height)) }
}

#[unsafe(no_mangle)]
pub extern "C" fn bims_resize(width: f32, height: f32) {
    game().resize(width, height);
}

/// Advance the world by `dt` seconds and rebuild the draw buffer.
#[unsafe(no_mangle)]
pub extern "C" fn bims_update(dt: f32) {
    game().update(dt);
}

/// Pointer to the current frame's shapes. Only valid until the next
/// [`bims_update`], and memory growth can move it, so re-read it every frame.
#[unsafe(no_mangle)]
pub extern "C" fn bims_draw_ptr() -> *const f32 {
    game().draw_ptr()
}

/// Number of floats (not shapes) in the current frame's buffer.
#[unsafe(no_mangle)]
pub extern "C" fn bims_draw_len() -> usize {
    game().draw_len()
}

// --- player input -------------------------------------------------------
//
// All coordinates are in the same CSS-pixel space the world was sized in, so
// the host passes canvas-relative positions straight through.

/// Select by control group, as in an RTS. Group 1 is the Bim.
#[unsafe(no_mangle)]
pub extern "C" fn bims_select_group(group: u32) {
    game().select_group(group);
}

#[unsafe(no_mangle)]
pub extern "C" fn bims_clear_selection() {
    game().clear_selection();
}

/// How many characters are currently selected, for the host's status line.
#[unsafe(no_mangle)]
pub extern "C" fn bims_selected_count() -> u32 {
    game().selected_count()
}

/// Begin a marquee selection at the pointer.
#[unsafe(no_mangle)]
pub extern "C" fn bims_drag_begin(x: f32, y: f32) {
    game().drag_begin(x, y);
}

#[unsafe(no_mangle)]
pub extern "C" fn bims_drag_update(x: f32, y: f32) {
    game().drag_update(x, y);
}

/// Finish a marquee and apply the selection. A click is a zero-size marquee.
/// Returns the fixture under a click (see the `HIT_` codes), or 0 for none, so
/// the host knows when to open a menu instead of changing the selection.
#[unsafe(no_mangle)]
pub extern "C" fn bims_drag_end(x: f32, y: f32) -> u32 {
    game().drag_end(x, y)
}

/// Abandon a marquee without changing the selection, e.g. on lost focus.
#[unsafe(no_mangle)]
pub extern "C" fn bims_drag_cancel() {
    game().drag_cancel();
}

/// Which fixture is at a point, or 0 for bare floor. The host asks before
/// acting on a right-click: on furniture that means a menu, anywhere else it
/// means "walk there".
#[unsafe(no_mangle)]
pub extern "C" fn bims_hit_at(x: f32, y: f32) -> u32 {
    game().hit_at(x, y)
}

/// Right-click on the ground: send the selection to that spot.
#[unsafe(no_mangle)]
pub extern "C" fn bims_order_move(x: f32, y: f32) {
    game().order_move(x, y);
}

// --- the view -----------------------------------------------------------
//
// The room is a fixed size in world units. The host scales and centres it in
// whatever canvas it has, and inverts the same transform to turn pointer
// positions back into room coordinates before calling anything above.

#[unsafe(no_mangle)]
pub extern "C" fn bims_view_scale() -> f32 {
    game().view_scale()
}

#[unsafe(no_mangle)]
pub extern "C" fn bims_view_x() -> f32 {
    game().view_offset().x
}

#[unsafe(no_mangle)]
pub extern "C" fn bims_view_y() -> f32 {
    game().view_offset().y
}

// --- fixtures -----------------------------------------------------------
//
// The host draws the menus, so it asks for the state it needs to label them
// rather than strings being passed across the boundary.

#[unsafe(no_mangle)]
pub extern "C" fn bims_fridge_is_open() -> u32 {
    game().fridge_is_open() as u32
}

#[unsafe(no_mangle)]
pub extern "C" fn bims_stove_is_on() -> u32 {
    game().stove_is_on() as u32
}

/// Non-zero while the Bim is working through a task and should be left alone.
#[unsafe(no_mangle)]
pub extern "C" fn bims_is_busy() -> u32 {
    game().is_busy() as u32
}

#[unsafe(no_mangle)]
pub extern "C" fn bims_toggle_fridge() {
    game().toggle_fridge();
}

#[unsafe(no_mangle)]
pub extern "C" fn bims_toggle_stove() {
    game().toggle_stove();
}

/// Start the make-a-meal chain: fridge, board, knife, pot, stove, table.
#[unsafe(no_mangle)]
pub extern "C" fn bims_make_food() {
    game().make_food();
}

// --- the heads ----------------------------------------------------------

#[unsafe(no_mangle)]
pub extern "C" fn bims_door_is_open() -> u32 {
    game().door_is_open() as u32
}

#[unsafe(no_mangle)]
pub extern "C" fn bims_door_is_locked() -> u32 {
    game().door_is_locked() as u32
}

/// The bathroom door is powered: the player works it from anywhere, the way
/// the fridge opens, rather than the Bim walking over to it.
#[unsafe(no_mangle)]
pub extern "C" fn bims_toggle_door() {
    game().toggle_door();
}

/// Locking shuts the door as well; a locked door standing open is not locked.
#[unsafe(no_mangle)]
pub extern "C" fn bims_toggle_door_lock() {
    game().toggle_door_lock();
}

/// Use the heads and wash afterwards. Does nothing through a locked door, so
/// the host greys the menu item out to match.
#[unsafe(no_mangle)]
pub extern "C" fn bims_use_toilet() {
    game().use_toilet();
}

// --- the clock ----------------------------------------------------------
//
// Time is kept in game minutes since midnight. The host does the formatting,
// which keeps strings off the boundary and out of the wasm binary.

#[unsafe(no_mangle)]
pub extern "C" fn bims_clock_minutes() -> f32 {
    game().clock_minutes()
}

/// Days since the game started, counting the first as 1.
#[unsafe(no_mangle)]
pub extern "C" fn bims_clock_day() -> u32 {
    game().clock_day()
}

/// How long each kind of lie-down lasts, in game minutes. The host reads
/// these rather than repeating them, so its menu cannot promise half an hour
/// and get something else.
#[unsafe(no_mangle)]
pub extern "C" fn bims_nap_minutes() -> f32 {
    task::NAP_MINUTES
}

#[unsafe(no_mangle)]
pub extern "C" fn bims_sleep_minutes() -> f32 {
    task::SLEEP_MINUTES
}

/// Game minutes of sleep the Bim has left, or 0 when it is not in bed.
#[unsafe(no_mangle)]
pub extern "C" fn bims_rest_left() -> f32 {
    game().rest_left()
}

/// Send the Bim to bed: up the ladder, under the covers, and out again when
/// the clock says so.
#[unsafe(no_mangle)]
pub extern "C" fn bims_nap() {
    game().rest(task::NAP_MINUTES);
}

#[unsafe(no_mangle)]
pub extern "C" fn bims_sleep() {
    game().rest(task::SLEEP_MINUTES);
}

/// What the Bim is busy with: 0 idle, 1 a meal, 2 the cooker, 3 a lie-down,
/// 4 the heads. The host turns these into words.
#[unsafe(no_mangle)]
pub extern "C" fn bims_activity() -> u32 {
    game().activity()
}

/// Which step of the running task the Bim is on, for a status line.
/// Zero means no task.
#[unsafe(no_mangle)]
pub extern "C" fn bims_task_step() -> u32 {
    game().task_step()
}

/// Where the Bim is, in room coordinates. Read-only; handy for a status
/// readout, and it lets tests probe position without guessing at draw order.
#[unsafe(no_mangle)]
pub extern "C" fn bims_bim_x() -> f32 {
    game().bim_pos().x
}

#[unsafe(no_mangle)]
pub extern "C" fn bims_bim_y() -> f32 {
    game().bim_pos().y
}
