//! The world clock.
//!
//! Everything in the game that is measured in wall-clock time — how long a nap
//! lasts, when it gets dark — goes through here, so there is one definition of
//! how fast a day passes rather than a rate copied into each caller.
//!
//! The clock runs off the same `dt` as the rest of the simulation, which means
//! the speed slider carries it along too: at 12x a day takes two minutes.

use crate::math::smoothstep;

/// How many game minutes pass in one real second at 1x. One a second makes a
/// full day 24 minutes of real time, and a six-hour sleep six of them — long
/// enough to feel like a night, short enough to sit through on the slider.
pub const MINUTES_PER_SECOND: f32 = 1.0;

/// Minutes in a day, and in an hour. Spelled out because the conversions read
/// better than the numbers.
pub const HOUR: f32 = 60.0;
pub const DAY: f32 = 24.0 * HOUR;

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

    /// How bright it is outside, 0 at night to 1 in the day.
    pub fn daylight(&self) -> f32 {
        let hour = self.minutes / HOUR;
        let up = smoothstep((hour - DAWN.0) / (DAWN.1 - DAWN.0));
        let down = smoothstep((hour - DUSK.0) / (DUSK.1 - DUSK.0));
        up - down
    }
}
