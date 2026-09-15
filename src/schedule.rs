//! A day's timetable, one slot per hour.
//!
//! Only sleep is timetabled for now; every other hour is left as *anything*,
//! which means the Bim carries on choosing for itself from what it wants. So
//! the schedule is not a rota the Bim obeys to the letter — it is a standing
//! instruction to go to bed at a particular time of day, and one it is allowed
//! to decline.
//!
//! It declines when it is not tired enough to be worth it. A Bim sent to bed
//! at nine tenths rested would be up again almost at once, which is worse than
//! not going: it is a walk to the bunk, a climb, and a walk back.

use crate::clock::HOUR;
use crate::task::SLEEP_MINUTES;

/// The night the timetable starts out with: ten at night, running as long as a
/// night actually is, so the block ends where the Bim would be getting up
/// anyway. Derived from `SLEEP_MINUTES` rather than written out, so the two
/// cannot drift apart.
const NIGHT_FROM: u32 = 22;

/// What an hour is set aside for.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    /// Whatever the Bim decides; the ordinary run of the day.
    Anything,
    Sleep,
}

/// Rested above this, a scheduled sleep is ignored — that block, not for good.
pub const IGNORE_ABOVE: f32 = 0.80;
/// Rested to this, the Bim gets up, whatever the timetable says.
pub const WAKE_AT: f32 = 1.0;

pub const HOURS: usize = 24;

pub struct Schedule {
    hours: [Slot; HOURS],
    /// False once this block of sleep has been acted on, so a six-hour block
    /// sends the Bim to bed once rather than six times. Re-armed by any hour
    /// that is not a sleep hour.
    armed: bool,
}

impl Schedule {
    /// Starts with a night painted in, from ten. Everything else is left as
    /// anything, so outside those hours the Bim's own needs run the day.
    ///
    /// A default block does real work here rather than being decoration: the
    /// Bim's body clock free-runs an hour slow, and this is what it gets pulled
    /// back onto on the days it is tired enough to accept.
    pub fn new() -> Schedule {
        let mut schedule = Schedule {
            hours: [Slot::Anything; HOURS],
            armed: true,
        };
        let night = (SLEEP_MINUTES / HOUR) as u32;
        for i in 0..night {
            schedule.set((NIGHT_FROM + i) % HOURS as u32, Slot::Sleep);
        }
        schedule
    }

    pub fn slot(&self, hour: u32) -> Slot {
        self.hours[hour as usize % HOURS]
    }

    pub fn set(&mut self, hour: u32, slot: Slot) {
        self.hours[hour as usize % HOURS] = slot;
    }

    /// Whether the clock has just walked into a block of scheduled sleep that
    /// has not been acted on yet.
    ///
    /// Armed on the way in rather than on the hour boundary, so painting sleep
    /// onto the hour it is already is takes effect straight away instead of
    /// waiting for the next one.
    pub fn due(&mut self, minutes: f32) -> bool {
        let hour = (minutes / 60.0) as u32 % HOURS as u32;
        if self.slot(hour) != Slot::Sleep {
            self.armed = true;
            return false;
        }
        if !self.armed {
            return false;
        }
        self.armed = false;
        true
    }
}
