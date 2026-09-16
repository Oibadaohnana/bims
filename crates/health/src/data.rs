//! The numbers, kept apart from the arithmetic that uses them.
//!
//! Everything here is a **placeholder**. Nothing has been balanced against
//! anything; the values exist so the model in `state.rs` has something to be
//! exercised with. Expect all of them to move.
//!
//! What must not move quietly is the *shape* they make. Three scenarios in
//! `tests.rs` are the requirement rather than these figures: eight hours in
//! the open costs a Bim some health and it all comes back, a day in the open
//! gives it cancer and it survives the dose, and cancer left alone kills in
//! under forty days. Change a number here and those tests say whether the
//! game still works the way it is meant to. [`data_is_sound`] covers the
//! orderings the code itself relies on.
//!
//! Every rate is **per game minute** and every duration is **in game
//! minutes**, because that is the unit [`crate::update`] is handed. The day
//! length comes from the `time` crate, so a longer day stretches all of this
//! together.

use time::DAY;

/// A body in the best health it can be in. Health is a bar rather than a
/// number of hit points, and 100 makes it a percentage without a conversion.
pub const MAX_HEALTH: f64 = 100.0;

/// Dose picked up per minute standing in an unshielded tile, at the default
/// intensity of 1.0. So a dose figure reads as "minutes in the open".
pub const RAD_RATE: f64 = 1.0;

/// Dose shed per minute under cover. Half the rate it goes on, so getting
/// inside is worth something immediately but a day in the open is two days
/// of being ill afterwards.
pub const DOSE_DECAY: f64 = 0.5;

/// Dose at which the body starts taking damage: two hours in the open.
pub const CRITICAL: f64 = 120.0;

/// Dose at which radiation sickness sets in: eight hours in the open.
pub const SICKNESS: f64 = 480.0;

/// Dose at which cancer begins: a full day in the open. Reaching it once is
/// enough and there is no way back — see [`crate::CancerState`].
pub const CANCER: f64 = 1440.0;

/// Health lost per minute at a critical dose. Ten days of it would kill a
/// Bim outright, which nothing else in the game is slow enough to do.
pub const CRITICAL_DAMAGE: f64 = MAX_HEALTH / (10.0 * DAY);

/// Health lost per minute with radiation sickness. Four days to kill.
pub const SICKNESS_DAMAGE: f64 = MAX_HEALTH / (4.0 * DAY);

/// Health mended per minute with nothing wrong. Two days from nothing to
/// full — deliberately faster than any single source of damage, so a Bim
/// that gets out of trouble visibly comes right.
pub const MEND: f64 = MAX_HEALTH / (2.0 * DAY);

/// Days from the onset of cancer to each later stage. Days rather than
/// minutes because that is how long it is thought about; [`CANCER_ADVANCED_AT`]
/// and [`CANCER_TERMINAL_AT`] are the same two numbers in the unit the code
/// works in.
pub const CANCER_ADVANCED_DAYS: f64 = 10.0;
pub const CANCER_TERMINAL_DAYS: f64 = 20.0;

pub const CANCER_ADVANCED_AT: f64 = CANCER_ADVANCED_DAYS * DAY;
pub const CANCER_TERMINAL_AT: f64 = CANCER_TERMINAL_DAYS * DAY;

/// Health lost per minute at each stage of cancer. The early stage is barely
/// there — a hundred days to kill — and it accelerates: what makes cancer
/// dangerous in this model is that nothing mends while it is present, so
/// even a scratch of damage never comes back.
pub const CANCER_EARLY_DAMAGE: f64 = MAX_HEALTH / (100.0 * DAY);
pub const CANCER_ADVANCED_DAMAGE: f64 = MAX_HEALTH / (40.0 * DAY);
pub const CANCER_TERMINAL_DAMAGE: f64 = MAX_HEALTH / (10.0 * DAY);

/// What radiation sickness does to how fast a body works and walks. 1.0 is
/// normal; contributions multiply, so a Bim with sickness *and* cancer is
/// slower than either alone.
pub const SICKNESS_WORK: f64 = 0.75;
pub const SICKNESS_MOVE: f64 = 0.85;

/// What each stage of cancer does to the same two. Early cancer shows no
/// outward sign at all, which is the point of it having a stage before the
/// one that does.
pub const CANCER_EARLY_WORK: f64 = 1.0;
pub const CANCER_EARLY_MOVE: f64 = 1.0;
pub const CANCER_ADVANCED_WORK: f64 = 0.8;
pub const CANCER_ADVANCED_MOVE: f64 = 0.9;
pub const CANCER_TERMINAL_WORK: f64 = 0.5;
pub const CANCER_TERMINAL_MOVE: f64 = 0.7;

/// The orderings the model reads out of this table rather than checking.
///
/// The dose thresholds have to climb, or the stage a dose reads as is not
/// the stage the update advances through and a Bim would skip a band; the
/// cancer stages have to climb for the same reason. Rates have to be
/// positive because their sign is what "damage" and "mending" mean here, and
/// nothing anywhere negates them.
///
/// Same shape as `physics::defs_are_sound` and `shipdesign::defs_are_sound`:
/// a table that is checked once by a test rather than on every lookup.
pub fn data_is_sound() -> bool {
    0.0 < CRITICAL
        && CRITICAL < SICKNESS
        && SICKNESS < CANCER
        && 0.0 < CANCER_ADVANCED_AT
        && CANCER_ADVANCED_AT < CANCER_TERMINAL_AT
        && MAX_HEALTH > 0.0
        && RAD_RATE > 0.0
        && DOSE_DECAY > 0.0
        && MEND > 0.0
        && CRITICAL_DAMAGE > 0.0
        && SICKNESS_DAMAGE > CRITICAL_DAMAGE
        && CANCER_EARLY_DAMAGE > 0.0
        && CANCER_ADVANCED_DAMAGE > CANCER_EARLY_DAMAGE
        && CANCER_TERMINAL_DAMAGE > CANCER_ADVANCED_DAMAGE
}
