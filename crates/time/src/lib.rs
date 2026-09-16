//! How long things take.
//!
//! The **base time unit is the game minute**, and it is a float rather than a
//! tick count — so nothing here needs a conversion between an integer clock
//! and a continuous one, and no caller has to guess a tick rate.
//!
//! Every other part of the game derives its durations from these. `clock.rs`
//! in the simulation restates them as `f32` because the room is `f32`
//! throughout; that is a conversion of one definition, not a second one.
//! Anything that wants to know how long a day is asks here.
//!
//! Changing [`DAY`] changes every travel time the world generator validates
//! its layouts against, so it is one of the constants that forces a
//! `generator_version` bump.

/// The base unit. Spelled out so arithmetic below reads as what it is rather
/// than as bare numbers.
pub const MINUTE: f64 = 1.0;

pub const HOUR: f64 = 60.0 * MINUTE;

/// A day. Twenty-four hours, and the divisor behind every "how many days is
/// that" in the game.
pub const DAY: f64 = 24.0 * HOUR;

/// Game minutes in one real second at 1x. The speed slider multiplies this,
/// which is why it lives with the durations rather than with the renderer.
pub const MINUTES_PER_SECOND: f64 = 1.0;

/// Game minutes as days, and back. Two lines, but they are the two places the
/// division would otherwise be written out by hand.
pub fn days(minutes: f64) -> f64 {
    minutes / DAY
}

pub fn minutes(days: f64) -> f64 {
    days * DAY
}
