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
mod dish;
mod draw;
mod filth;
mod game;
mod health;
mod hydro;
mod manager;
mod math;
mod nav;
mod needs;
mod rng;
mod room;
mod schedule;
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

/// Right-click on the ground: send the selection to that spot, opening a door
/// on the way if that is what it takes.
///
/// Returns what became of it: 0 ignored, 1 on its way, 2 going to open a door
/// first, 3 blocked by a locked door, 4 no route there at all. The host says
/// so for the last two, because an order that silently does nothing reads as a
/// broken click.
#[unsafe(no_mangle)]
pub extern "C" fn bims_order_move(x: f32, y: f32) -> u32 {
    game().order_move(x, y)
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

/// Game minutes before the hob gives up and turns itself off, or 0 when it
/// is off already or has something actually cooking on it.
#[unsafe(no_mangle)]
pub extern "C" fn bims_stove_idle_left() -> f32 {
    game().stove_idle_left()
}

#[unsafe(no_mangle)]
pub extern "C" fn bims_toggle_fridge() {
    game().toggle_fridge();
}

#[unsafe(no_mangle)]
pub extern "C" fn bims_toggle_stove() {
    game().toggle_stove();
}

/// Start a meal of whichever recipe the Bim fancies.
#[unsafe(no_mangle)]
pub extern "C" fn bims_make_food() {
    game().make_food();
}

/// A stew: two vegetables chopped, then cooked in the pot.
#[unsafe(no_mangle)]
pub extern "C" fn bims_make_stew() {
    game().cook(room::Dish::Stew);
}

/// A bowl: tofu chopped, tipped in with the salad, and eaten cold.
#[unsafe(no_mangle)]
pub extern "C" fn bims_make_bowl() {
    game().cook(room::Dish::Bowl);
}

// --- the cold store -----------------------------------------------------
//
// Two counts rather than one, because the two are not interchangeable: a stew
// is two vegetables, a bowl is a block of tofu with a salad beside it. The
// hydroponic bay is the only thing that puts any back.

#[unsafe(no_mangle)]
pub extern "C" fn bims_store_veg() -> u32 {
    game().store_veg()
}

#[unsafe(no_mangle)]
pub extern "C" fn bims_store_tofu() -> u32 {
    game().store_tofu()
}

/// Helpings left in the pot on the hob. A stew is cooked once and eaten twice,
/// so this is what says whether the Bim has to cook at all.
#[unsafe(no_mangle)]
pub extern "C" fn bims_pot_servings() -> u32 {
    game().pot_servings()
}

/// How many a fresh pot holds.
#[unsafe(no_mangle)]
pub extern "C" fn bims_pot_capacity() -> u32 {
    game().pot_capacity()
}

/// Go and have what is left in the pot rather than cooking something new.
#[unsafe(no_mangle)]
pub extern "C" fn bims_eat_leftovers() {
    game().eat_leftovers();
}

// --- the hydroponic bay -------------------------------------------------
//
// Five trays. What goes in them is decided by the manager's target — the bay
// plants whichever of greens and soy the store is furthest behind on — unless
// the player has put a standing order on it, which overrides the lot.
//
// Automation is a setting, like the timetable: the deciding is the player's
// and the *doing* is still the Bim's, one tray at a time, on foot.

#[unsafe(no_mangle)]
pub extern "C" fn bims_hydro_spots() -> u32 {
    game().hydro_spots()
}

/// What is growing in tray `i`: 0 empty, 1 greens, 2 soy.
#[unsafe(no_mangle)]
pub extern "C" fn bims_hydro_crop(i: u32) -> u32 {
    game().hydro_crop(i)
}

/// How far along tray `i` is, 0 to 1. Greens take a day, soy a day and a half.
#[unsafe(no_mangle)]
pub extern "C" fn bims_hydro_growth(i: u32) -> f32 {
    game().hydro_growth(i)
}

/// How many trays are ready to lift.
#[unsafe(no_mangle)]
pub extern "C" fn bims_hydro_ripe() -> u32 {
    game().hydro_ripe()
}

#[unsafe(no_mangle)]
pub extern "C" fn bims_hydro_automated() -> u32 {
    game().hydro_automated() as u32
}

#[unsafe(no_mangle)]
pub extern "C" fn bims_set_hydro_automated(on: u32) {
    game().set_hydro_automated(on != 0);
}

/// The standing order: 0 none — follow the target — 1 greens, 2 soy.
#[unsafe(no_mangle)]
pub extern "C" fn bims_hydro_forced() -> u32 {
    game().hydro_forced()
}

#[unsafe(no_mangle)]
pub extern "C" fn bims_set_hydro_forced(crop: u32) {
    game().set_hydro_forced(crop);
}

/// Non-zero while the store is at target and the bay is holding what it has:
/// nothing growing, nothing planted, nothing lifted.
#[unsafe(no_mangle)]
pub extern "C" fn bims_hydro_hibernating() -> u32 {
    game().hydro_hibernating() as u32
}

// --- the manager --------------------------------------------------------
//
// What the place is told to keep in stock. One number for now — food, in
// units of two thirds greens to one third soy — and everything automated
// works to it rather than being told separately.

#[unsafe(no_mangle)]
pub extern "C" fn bims_food_target() -> u32 {
    game().food_target()
}

#[unsafe(no_mangle)]
pub extern "C" fn bims_set_food_target(units: u32) {
    game().set_food_target(units);
}

/// The two halves of that target, as counts of the actual things.
#[unsafe(no_mangle)]
pub extern "C" fn bims_target_veg() -> u32 {
    game().target_veg()
}

#[unsafe(no_mangle)]
pub extern "C" fn bims_target_tofu() -> u32 {
    game().target_tofu()
}

/// The most the manager will accept, so the host's input agrees with it.
#[unsafe(no_mangle)]
pub extern "C" fn bims_food_target_max() -> u32 {
    manager::MOST
}

// --- keeping body and soul together -------------------------------------

/// Health out of 100. At zero the Bim is dead and nothing moves again.
#[unsafe(no_mangle)]
pub extern "C" fn bims_health() -> f32 {
    game().health()
}

#[unsafe(no_mangle)]
pub extern "C" fn bims_max_health() -> f32 {
    health::MAX_HEALTH
}

/// 0 when well fed, then 1, 2, 3 for mild, ordinary and extreme malnutrition.
#[unsafe(no_mangle)]
pub extern "C" fn bims_malnutrition() -> u32 {
    game().malnutrition()
}

/// 0 wide awake, then 1, 2, 3 for sleepy, sleep-deprived and past it. The
/// stages slow errands down by fumbling them; the last one also drops the Bim
/// where it stands for a quarter of an hour at a time.
#[unsafe(no_mangle)]
pub extern "C" fn bims_drowsiness() -> u32 {
    game().drowsiness()
}

/// Non-zero while the Bim has nodded off on its feet.
#[unsafe(no_mangle)]
pub extern "C" fn bims_is_napping() -> u32 {
    game().is_napping() as u32
}

/// Non-zero while it is standing there having lost the thread of its errand.
#[unsafe(no_mangle)]
pub extern "C" fn bims_is_stalled() -> u32 {
    game().is_stalled() as u32
}

/// How many times the errand running now has had to be started over.
#[unsafe(no_mangle)]
pub extern "C" fn bims_task_fumbles() -> u32 {
    game().task_fumbles()
}

#[unsafe(no_mangle)]
pub extern "C" fn bims_is_alive() -> u32 {
    game().is_alive() as u32
}

// --- the dishwasher -----------------------------------------------------

/// Dirty plates stacked in it, and how many it holds. The host reads the
/// capacity rather than repeating it.
#[unsafe(no_mangle)]
pub extern "C" fn bims_dishwasher_loaded() -> u32 {
    game().dishwasher_loaded()
}

#[unsafe(no_mangle)]
pub extern "C" fn bims_dishwasher_capacity() -> u32 {
    game().dishwasher_capacity()
}

/// Game minutes left of the running cycle, or 0 when it is not running.
#[unsafe(no_mangle)]
pub extern "C" fn bims_dishwasher_cycle_left() -> f32 {
    game().dishwasher_cycle_left()
}

/// Start a cycle without waiting for the rack to fill.
#[unsafe(no_mangle)]
pub extern "C" fn bims_run_dishwasher() {
    game().run_dishwasher();
}

// --- the agenda ---------------------------------------------------------
//
// The chain running now, followed by every chain that was put down to make
// way for something else, in the order they will be picked back up. The host
// draws this as a checklist; all it needs per row is a code, how far through
// it is, and whether it is the one actually running.

#[unsafe(no_mangle)]
pub extern "C" fn bims_agenda_len() -> u32 {
    game().agenda_len()
}

/// Which errand row `i` is: 1 a meal, 2 the cooker, 3 a nap, 4 a sleep,
/// 5 the heads. Zero when `i` is past the end.
#[unsafe(no_mangle)]
pub extern "C" fn bims_agenda_job(i: u32) -> u32 {
    game().agenda_job(i)
}

/// How far through row `i` is, 0 to 1. A queued row keeps the progress it had
/// when it was interrupted.
#[unsafe(no_mangle)]
pub extern "C" fn bims_agenda_progress(i: u32) -> f32 {
    game().agenda_progress(i)
}

/// Non-zero for the row that is actually running, which is only ever row 0.
#[unsafe(no_mangle)]
pub extern "C" fn bims_agenda_active(i: u32) -> u32 {
    game().agenda_active(i)
}

// --- what the Bim wants -------------------------------------------------
//
// Four levels, each 1 when comfortable and 0 when desperate, in a fixed order:
// rest, food, restroom, cleanliness. A level below the threshold is what sets
// the matching errand going on its own — where there is one. Cleanliness has
// no errand behind it yet and does not run down with the clock either; it
// follows the state of the deck the Bim is standing on.

#[unsafe(no_mangle)]
pub extern "C" fn bims_need_count() -> u32 {
    game().need_count()
}

#[unsafe(no_mangle)]
pub extern "C" fn bims_need_level(i: u32) -> f32 {
    game().need_level(i)
}

/// The level at which the Bim goes and does something about it. The host reads
/// it rather than repeating it, so the bar and the behaviour cannot disagree.
#[unsafe(no_mangle)]
pub extern "C" fn bims_need_threshold() -> f32 {
    game().need_threshold()
}

// --- mess ---------------------------------------------------------------
//
// What a need being left unmet actually looks like. Both of these are stages
// rather than levels — 0 for nothing wrong, then 1, 2, 3 — because both are
// reached by a clock rather than by a bar running out.

/// How badly the Bim needs the heads: 0 comfortable, 1 fidgeting, 2 close to
/// an accident, 3 holding on and about to lose.
#[unsafe(no_mangle)]
pub extern "C" fn bims_urge() -> u32 {
    game().urge()
}

/// How far gone it is for want of a clean place to stand: 0 fine, 1 walking
/// carefully, 2 moving away from the mess, 3 being sick in it.
#[unsafe(no_mangle)]
pub extern "C" fn bims_discomfort() -> u32 {
    game().discomfort()
}

/// How filthy the Bim itself is, 0 clean to 1 covered.
#[unsafe(no_mangle)]
pub extern "C" fn bims_bim_filth() -> f32 {
    game().bim_filth()
}

/// How much of the deck is dirty, 0 to 1, for the host's readout.
#[unsafe(no_mangle)]
pub extern "C" fn bims_deck_filth() -> f32 {
    game().deck_filth()
}

// --- the timetable ------------------------------------------------------
//
// Twenty-four slots, one an hour. Only sleep is timetabled so far; every other
// hour is "anything", which leaves the Bim deciding for itself as before.

#[unsafe(no_mangle)]
pub extern "C" fn bims_schedule_hours() -> u32 {
    schedule::HOURS as u32
}

/// What hour `hour` is set aside for: 0 anything, 1 sleep.
#[unsafe(no_mangle)]
pub extern "C" fn bims_schedule_slot(hour: u32) -> u32 {
    game().schedule_slot(hour)
}

#[unsafe(no_mangle)]
pub extern "C" fn bims_set_schedule_slot(hour: u32, slot: u32) {
    game().set_schedule_slot(hour, slot);
}

/// Rested above this, a scheduled sleep is passed over. The host reads it so
/// its explanation cannot drift from the behaviour.
#[unsafe(no_mangle)]
pub extern "C" fn bims_schedule_ignore_above() -> f32 {
    game().schedule_ignore_above()
}

// --- deciding for itself ------------------------------------------------

// --- under direct orders ------------------------------------------------

/// Recruit the Bim, or let it go. Recruited, it starts nothing by itself — no
/// errand from a need, no sleep from the timetable, nothing off the queue, and
/// no pottering about — but does everything it is told, and everything going
/// without does to it still bites.
#[unsafe(no_mangle)]
pub extern "C" fn bims_toggle_recruited() {
    game().toggle_recruited();
}

#[unsafe(no_mangle)]
pub extern "C" fn bims_is_recruited() -> u32 {
    game().is_recruited() as u32
}

/// Non-zero while the Bim starts errands on its own when it has nothing on.
#[unsafe(no_mangle)]
pub extern "C" fn bims_is_autonomous() -> u32 {
    game().is_autonomous() as u32
}

#[unsafe(no_mangle)]
pub extern "C" fn bims_set_autonomous(on: u32) {
    game().set_autonomous(on != 0);
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

/// Send the Bim to the door panel and work it. There is a panel each side of
/// the bulkhead; it uses whichever it is nearest.
#[unsafe(no_mangle)]
pub extern "C" fn bims_toggle_door() {
    game().toggle_door();
}

/// Locking shuts the door as well; a locked door standing open is not locked.
#[unsafe(no_mangle)]
pub extern "C" fn bims_toggle_door_lock() {
    game().toggle_door_lock();
}

/// Whether the Bim could set off for the heads. A locked door only stops one
/// on the wrong side of it; already in there, it starts at the pan.
#[unsafe(no_mangle)]
pub extern "C" fn bims_can_use_toilet() -> u32 {
    game().can_use_toilet() as u32
}

/// Use the heads and wash afterwards. Does nothing when it cannot be reached,
/// so the host greys the menu item out to match.
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
