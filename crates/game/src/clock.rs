//! The world clock.
//!
//! Everything in the game that is measured in wall-clock time — how long a nap
//! lasts, when it gets dark — goes through here, so there is one definition of
//! how fast a day passes rather than a rate copied into each caller.
//!
//! The clock runs off the same `dt` as the rest of the simulation, which means
//! the speed slider carries it along too: at 24x a day takes one minute.

use crate::math::smoothstep;

/// How many game minutes pass in one real second at 1x. One a second makes a
/// full day 24 minutes of real time, and a six-hour sleep six of them — long
/// enough to feel like a night, short enough to sit through on the slider.
///
/// Minutes in a day and in an hour come with it. All three are the `time`
/// crate's, restated here as `f32` because the room is `f32` throughout —
/// a conversion of one definition rather than a second one. The world
/// generator reads the same constants at full width, so a day is the same
/// length on both sides of the game.
pub const MINUTES_PER_SECOND: f32 = crate::time::MINUTES_PER_SECOND as f32;
pub const HOUR: f32 = crate::time::HOUR as f32;
pub const DAY: f32 = crate::time::DAY as f32;

/// The year the game opens on, and the ship's calendar.
///
/// Twelve months of the lengths everyone knows, and **no leap years** — 365
/// days, every year, for ever. That is a simplification and it is deliberate:
/// a leap day buys nothing here and costs a special case in every piece of
/// arithmetic that turns a day count into a date, including the one that works
/// out how old somebody is.
pub const START_YEAR: u32 = 2400;
pub const DAYS_IN_YEAR: u32 = 365;
const MONTH_DAYS: [u32; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

/// A day of the year, 0 to 364, as a month (1 to 12) and a date in it.
pub fn month_and_date(day_of_year: u32) -> (u32, u32) {
    let mut left = day_of_year % DAYS_IN_YEAR;
    for (i, &len) in MONTH_DAYS.iter().enumerate() {
        if left < len {
            return (i as u32 + 1, left + 1);
        }
        left -= len;
    }
    // Unreachable while the months add up to a year, which the constant above
    // is the whole definition of.
    (12, 31)
}

/// The Bim's day starts here.
const WAKING_HOUR: f32 = 8.0;

/// When the light comes up and goes down again. Between each pair the room
/// eases from one to the other rather than switching.
const DAWN: (f32, f32) = (5.5, 7.5);
const DUSK: (f32, f32) = (19.0, 21.5);

/// Real seconds a span of game minutes takes at 1x.
pub fn seconds(minutes: f32) -> f32 {
    minutes / MINUTES_PER_SECOND
}

pub struct Clock {
    /// Minutes since midnight, fractional.
    minutes: f32,
    day: u32,
}

impl Clock {
    pub fn new() -> Clock {
        Clock {
            minutes: WAKING_HOUR * HOUR,
            day: 1,
        }
    }

    pub fn advance(&mut self, dt: f32) {
        self.minutes += dt * MINUTES_PER_SECOND;
        while self.minutes >= DAY {
            self.minutes -= DAY;
            self.day += 1;
        }
    }

    /// Minutes since midnight. The host formats this; no strings cross the
    /// wasm boundary.
    pub fn minutes(&self) -> f32 {
        self.minutes
    }

    pub fn day(&self) -> u32 {
        self.day
    }

    /// Days since the game opened, counting the first as 0. The date
    /// arithmetic wants an offset; `day` is what the player is shown.
    fn elapsed(&self) -> u32 {
        self.day.saturating_sub(1)
    }

    /// The date aboard: the year, and the day of that year counting from 0.
    /// The game opens on the first of January in [`START_YEAR`].
    pub fn year(&self) -> u32 {
        START_YEAR + self.elapsed() / DAYS_IN_YEAR
    }

    pub fn day_of_year(&self) -> u32 {
        self.elapsed() % DAYS_IN_YEAR
    }

    /// How bright it is outside, 0 at night to 1 in the day.
    pub fn daylight(&self) -> f32 {
        let hour = self.minutes / HOUR;
        let up = smoothstep((hour - DAWN.0) / (DAWN.1 - DAWN.0));
        let down = smoothstep((hour - DUSK.0) / (DUSK.1 - DUSK.0));
        up - down
    }
}
