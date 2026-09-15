//! What the Bim wants, and when it wants it.
//!
//! Four levels, each running from 1 (comfortable) down to 0. A level falling
//! past [`URGENT`] is what sets the matching errand going; doing the thing
//! fills it back up. Everything else about the Bim's day follows from these
//! numbers.
//!
//! Three of them run on the clock. The fourth, cleanliness, has no clock and no
//! errand behind it: it follows the state of the deck the Bim is standing on
//! and of the Bim itself, and what it does when it runs out is in `filth.rs`.
//!
//! The rates are not picked by eye. Each one is written as the drop from full
//! to the trigger, divided by how long that is supposed to take, so the daily
//! counts are readable in the arithmetic rather than buried in a decimal.
//!
//! Nothing drains while the Bim is asleep. That is a deliberate choice and it
//! is what makes the counts come out exactly: if the restroom need ran down
//! through the night, six hours in bed would end well past the trigger, the Bim
//! would get up needing the heads immediately, and the three-a-day would drift
//! into four. Sleeping through the night is the whole point of sleeping.

use crate::clock::{DAY, HOUR, MINUTES_PER_SECOND};
use crate::task::SLEEP_MINUTES;

/// The level an errand starts at, and the level a finished one leaves behind.
pub const URGENT: f32 = 0.10;
const FULL: f32 = 1.0;
/// The span a need travels between being seen to and needing seeing to again.
const SPAN: f32 = FULL - URGENT;

/// How much of the day the Bim is up and about.
const WAKING: f32 = DAY - SLEEP_MINUTES;

/// The Bim's own day is an hour longer than the ship's.
///
/// With rest draining at exactly the rate that empties it in one day, bedtime
/// lands on the same hour for ever, which is both unlifelike and dull: the
/// Bim has no rhythm of its own, only the clock's. Draining a little slower
/// gives it a body clock that free-runs — bedtime walks an hour later each
/// day and it keeps resettling — which is roughly what an animal left in the
/// dark does, and it is what makes a timetable worth having: the schedule is
/// something to pull a drifting Bim back onto, rather than a restatement of
/// what it was going to do anyway.
///
/// Only rest drifts. Food and the restroom need still drain per waking minute,
/// so the Bim eats twice and visits three times in a waking day whatever hour
/// it happens to start.
const DRIFT: f32 = 1.0 * HOUR;
const BODY_DAY: f32 = DAY + DRIFT;
const BODY_WAKING: f32 = BODY_DAY - SLEEP_MINUTES;

/// How often each need should come up in a day.
const SLEEPS_PER_DAY: f32 = 1.0;
const MEALS_PER_DAY: f32 = 2.0;
const VISITS_PER_DAY: f32 = 3.0;

/// Waking minutes each errand costs, from the Bim setting off to the need
/// being full again. These are measured off the chains, not guessed at, and
/// the distinction that matters is *to full* rather than *to the end*: a meal
/// runs on for another ten minutes stacking the dishwasher, and the Bim is
/// already getting hungry again through all of it.
///
/// The cost has to come out of the slot. A cycle is the draining plus the
/// doing, so dividing the waking day by the number of helpings and using the
/// whole of it as the drain leaves no room for the errands themselves and the
/// day comes up short.
/// A stew costs about 57 and a bowl about 32; the Bim picks between them at
/// random, so the mean is what the day is built on.
const MEAL_COST: f32 = 44.5;
const VISIT_COST: f32 = 14.0;
const TO_BED_COST: f32 = 6.0;

/// Per game minute *awake*: each need falls from full to urgent exactly once
/// per slot, and the slots tile the waking day with room for the errands.
const REST_DRAIN: f32 = SPAN / (BODY_WAKING / SLEEPS_PER_DAY - TO_BED_COST);
const FOOD_DRAIN: f32 = SPAN / (WAKING / MEALS_PER_DAY - MEAL_COST);
const RESTROOM_DRAIN: f32 = SPAN / (WAKING / VISITS_PER_DAY - VISIT_COST);

/// Roughly how long the act of putting each need right takes, in game minutes.
/// Recovery is spread over that so a bar visibly fills while the Bim is doing
/// the thing rather than jumping when the chain ends. Filling a moment early
/// is harmless: the level clamps, and this way `needs.rs` does not have to
/// know the exact length of a step in `task.rs`.
const EATING: f32 = 6.0;
const RELIEF: f32 = 5.0;

/// Sleeping still covers exactly the span over exactly the six hours; it is
/// only the run-down that drifts, not the night itself.
const REST_RECOVER: f32 = SPAN / SLEEP_MINUTES;
/// A meal fills the whole range whatever it started at, as does a visit.
const FOOD_RECOVER: f32 = FULL / EATING;
const RESTROOM_RECOVER: f32 = FULL / RELIEF;

/// Where a day starts. The Bim is rested, having just got up, and the other
/// two are part-used so that the first morning has something in it rather than
/// eight quiet hours: the heads at about ten, a meal at about half twelve.
const REST_AT_DAWN: f32 = 1.0;
const FOOD_AT_DAWN: f32 = 0.55;
const RESTROOM_AT_DAWN: f32 = 0.40;
/// A clean Bim in a clean room. This one only moves if something makes a mess.
const CLEAN_AT_DAWN: f32 = 1.0;

/// How badly the Bim needs the heads, read straight off the level.
///
/// The Bim sets off for the pan at [`URGENT`], so the two worse states only
/// come up when it cannot get there — shut in, under orders, or with autonomy
/// off. That is the whole point of them: they are what going without looks
/// like when the errand is not available.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Urge {
    None,
    Mild,
    Medium,
    Extreme,
}

/// Where each urge starts. Mild is well before the Bim would go of its own
/// accord, so the fidgeting reads as a warning rather than a surprise.
const MILD_URGE: f32 = 0.25;
const EXTREME_URGE: f32 = 0.001;

impl Urge {
    fn of(level: f32) -> Urge {
        if level <= EXTREME_URGE {
            Urge::Extreme
        } else if level < URGENT {
            Urge::Medium
        } else if level < MILD_URGE {
            Urge::Mild
        } else {
            Urge::None
        }
    }

    /// 0 comfortable, then 1, 2, 3. The host names them.
    pub fn stage(self) -> u32 {
        self as u32
    }

    /// The chance per game minute of hopping about on the spot. Nought while
    /// the Bim is comfortable, and it does it more the worse it gets.
    pub fn fidget_chance(self) -> f32 {
        match self {
            Urge::None => 0.0,
            Urge::Mild => 1.0 / 6.0,
            Urge::Medium | Urge::Extreme => 1.0 / 2.0,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Need {
    Rest,
    Food,
    Restroom,
    /// Not a clock like the others: this one follows the state of the room the
    /// Bim is standing in and of the Bim itself. See `filth.rs`.
    Cleanliness,
}

impl Need {
    /// In the order they are shown, and the order the host indexes them by.
    pub const ALL: [Need; 4] = [Need::Rest, Need::Food, Need::Restroom, Need::Cleanliness];

    pub fn from_index(i: u32) -> Option<Need> {
        Need::ALL.get(i as usize).copied()
    }

    fn drain(self) -> f32 {
        match self {
            Need::Rest => REST_DRAIN,
            Need::Food => FOOD_DRAIN,
            Need::Restroom => RESTROOM_DRAIN,
            // Time alone does not make a Bim dirty; filth does.
            Need::Cleanliness => 0.0,
        }
    }

    fn recover(self) -> f32 {
        match self {
            Need::Rest => REST_RECOVER,
            Need::Food => FOOD_RECOVER,
            Need::Restroom => RESTROOM_RECOVER,
            Need::Cleanliness => 0.0,
        }
    }
}

pub struct Needs {
    levels: [f32; Need::ALL.len()],
}

impl Needs {
    pub fn new() -> Needs {
        Needs {
            levels: [REST_AT_DAWN, FOOD_AT_DAWN, RESTROOM_AT_DAWN, CLEAN_AT_DAWN],
        }
    }

    /// How badly it needs the heads.
    pub fn urge(&self) -> Urge {
        Urge::of(self.level(Need::Restroom))
    }

    /// Put a need back where an accident leaves it: relieving itself where it
    /// stands is still relief, however much worse everything else gets.
    pub fn refill(&mut self, need: Need, to: f32) {
        self.levels[need as usize] = to.clamp(0.0, FULL);
    }

    /// Bring a need down by a flat amount, stopping at nothing. Being sick
    /// empties the stomach whatever was in it.
    pub fn spend(&mut self, need: Need, amount: f32) {
        let level = &mut self.levels[need as usize];
        *level = (*level - amount).max(0.0);
    }

    /// Cleanliness moving, in level per game minute: positive freshens, and
    /// negative is the room and the Bim's own state working on it.
    pub fn scrub(&mut self, minutes: f32, rate: f32) {
        let level = &mut self.levels[Need::Cleanliness as usize];
        *level = (*level + rate * minutes).clamp(0.0, FULL);
    }

    pub fn level(&self, need: Need) -> f32 {
        self.levels[need as usize]
    }

    /// `restoring` is whichever need the Bim is actually seeing to this
    /// instant — mid-doze, mid-mouthful, sat on the pan — or none.
    /// `tiring` multiplies how fast the Bim runs out of rest — malnutrition
    /// doubles it and then trebles it, so a starving Bim needs more sleep.
    pub fn update(&mut self, dt: f32, restoring: Option<Need>, tiring: f32) {
        let minutes = dt * MINUTES_PER_SECOND;

        // Asleep, nothing is spent at all; see the note at the top.
        if restoring != Some(Need::Rest) {
            for need in Need::ALL {
                let rate = need.drain() * if need == Need::Rest { tiring } else { 1.0 };
                let level = &mut self.levels[need as usize];
                *level = (*level - rate * minutes).max(0.0);
            }
        }

        if let Some(need) = restoring {
            let level = &mut self.levels[need as usize];
            *level = (*level + need.recover() * minutes).min(FULL);
        }
    }

    /// Everything past the trigger, emptiest first. A list rather than a
    /// single answer because the Bim may not be able to do anything about the
    /// most pressing one — an empty fridge, a locked door — and the ones
    /// behind it should not be held up by that.
    pub fn urgent(&self) -> Vec<Need> {
        let mut out: Vec<Need> = Need::ALL
            .into_iter()
            .filter(|&need| self.level(need) < URGENT)
            .collect();
        out.sort_by(|&a, &b| self.level(a).total_cmp(&self.level(b)));
        out
    }
}
