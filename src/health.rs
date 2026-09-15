//! Going without, and what it costs: food on one side, sleep on the other.
//!
//! An empty stomach is not itself harmful — the Bim can be hungry for a while
//! and simply be hungry. What does the damage is staying that way, so the
//! measure here is *time spent with nothing in it* rather than the hunger
//! level, and the three stages of malnutrition are thresholds on that clock.
//!
//! Only the last stage costs health. The first two are a warning: a Bim that
//! is merely slow and tiring easily can still be fed and will come right.

use crate::clock::{DAY, HOUR};

pub const MAX_HEALTH: f32 = 100.0;

/// Hunger at or below this counts as an empty stomach, and rest at or below it
/// as running on nothing.
const EMPTY: f32 = 0.02;

/// Game minutes of no sleep before each stage of drowsiness sets in. Faster
/// than starvation, because it is: a night missed tells before a meal does.
///
/// The gaps widen — six hours, then eight — so the first stage arrives as a
/// warning with time to act on it, and the last takes a full night of being
/// kept up to reach.
const SLEEPY_AT: f32 = 4.0 * HOUR;
const DEPRIVED_AT: f32 = 10.0 * HOUR;
const WRECKED_AT: f32 = 18.0 * HOUR;

/// Rest above this clears the whole thing. Unlike hunger, which is wound back
/// a little for every mouthful, sleeplessness only lifts when the Bim has
/// properly slept — which is why the fifteen-minute nods-off at the worst
/// stage, worth a few per cent of rest each, never lift it.
const SLEPT_AT: f32 = 0.80;

/// Game minutes on an empty stomach before each stage sets in. A day without
/// food to reach the worst of it; feeding at any point walks it back.
const MILD_AT: f32 = 8.0 * HOUR;
const MODERATE_AT: f32 = 16.0 * HOUR;
const EXTREME_AT: f32 = 24.0 * HOUR;

/// Starvation is undone faster than it sets in, so one meal is visibly worth
/// something rather than being lost in a day-long ledger.
const MEND_RATE: f32 = 3.0;

/// Health goes from full to nothing in a day of extreme malnutrition, and
/// takes two days of eating properly to come back.
const HEALTH_DRAIN: f32 = MAX_HEALTH / DAY;
const HEALTH_RECOVER: f32 = MAX_HEALTH / (2.0 * DAY);

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Malnutrition {
    None,
    Mild,
    Moderate,
    Extreme,
}

impl Malnutrition {
    /// 0 for a well-fed Bim, then 1, 2, 3. The host names them.
    pub fn stage(self) -> u32 {
        match self {
            Malnutrition::None => 0,
            Malnutrition::Mild => 1,
            Malnutrition::Moderate => 2,
            Malnutrition::Extreme => 3,
        }
    }

    /// How fast it walks, as a fraction of its usual pace.
    pub fn pace(self) -> f32 {
        match self {
            Malnutrition::None => 1.0,
            Malnutrition::Mild => 0.85,
            Malnutrition::Moderate => 0.70,
            Malnutrition::Extreme => 0.55,
        }
    }

    /// How much faster it tires: double from the second stage, triple from the
    /// third, so a starving Bim needs more sleep as well as moving worse.
    pub fn tiring(self) -> f32 {
        match self {
            Malnutrition::None | Malnutrition::Mild => 1.0,
            Malnutrition::Moderate => 2.0,
            Malnutrition::Extreme => 3.0,
        }
    }
}

/// How far gone for want of sleep.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Drowsiness {
    None,
    Sleepy,
    Deprived,
    Wrecked,
}

impl Drowsiness {
    /// 0 wide awake, then 1, 2, 3. The host names them.
    pub fn stage(self) -> u32 {
        match self {
            Drowsiness::None => 0,
            Drowsiness::Sleepy => 1,
            Drowsiness::Deprived => 2,
            Drowsiness::Wrecked => 3,
        }
    }

    /// How much longer an errand takes, all told.
    pub fn drag(self) -> f32 {
        match self {
            Drowsiness::None => 1.0,
            Drowsiness::Sleepy => 1.25,
            Drowsiness::Deprived => 1.50,
            Drowsiness::Wrecked => 2.00,
        }
    }

    /// The chance that a step, on finishing, has to be done again.
    ///
    /// A step repeated with probability `p` takes `1 / (1 - p)` times as long
    /// on average, so this is the inverse: the fumbling is random, but how
    /// much it costs over a whole errand is not.
    pub fn fumble(self) -> f32 {
        1.0 - 1.0 / self.drag()
    }

    /// Only at the worst of it does the Bim drop off where it stands.
    pub fn nods_off(self) -> bool {
        self == Drowsiness::Wrecked
    }
}

pub struct Health {
    /// Game minutes of empty stomach, wound back by eating.
    starved: f32,
    /// Game minutes awake on no rest at all, cleared by a proper sleep.
    sleepless: f32,
    points: f32,
}

impl Health {
    pub fn new() -> Health {
        Health {
            starved: 0.0,
            sleepless: 0.0,
            points: MAX_HEALTH,
        }
    }

    pub fn stage(&self) -> Malnutrition {
        if self.starved >= EXTREME_AT {
            Malnutrition::Extreme
        } else if self.starved >= MODERATE_AT {
            Malnutrition::Moderate
        } else if self.starved >= MILD_AT {
            Malnutrition::Mild
        } else {
            Malnutrition::None
        }
    }

    pub fn drowsiness(&self) -> Drowsiness {
        if self.sleepless >= WRECKED_AT {
            Drowsiness::Wrecked
        } else if self.sleepless >= DEPRIVED_AT {
            Drowsiness::Deprived
        } else if self.sleepless >= SLEEPY_AT {
            Drowsiness::Sleepy
        } else {
            Drowsiness::None
        }
    }

    pub fn points(&self) -> f32 {
        self.points
    }

    pub fn is_dead(&self) -> bool {
        self.points <= 0.0
    }

    /// `minutes` is game minutes elapsed, `food` and `rest` the levels now,
    /// and `resting` whether the Bim is actually asleep this instant.
    ///
    /// Starvation keeps running while the Bim sleeps: you do not stop starving
    /// because you are asleep, and a Bim that tires three times as fast spends
    /// more of its day in bed — which is exactly the spiral the third stage of
    /// malnutrition is meant to be. Sleeplessness plainly does not, so that one
    /// only counts waking minutes.
    pub fn update(&mut self, minutes: f32, food: f32, rest: f32, resting: bool) {
        if food <= EMPTY {
            self.starved += minutes;
        } else {
            self.starved = (self.starved - minutes * MEND_RATE).max(0.0);
        }

        if rest > SLEPT_AT {
            // Properly slept. Nothing short of this clears it.
            self.sleepless = 0.0;
        } else if rest <= EMPTY && !resting {
            self.sleepless += minutes;
        }

        let change = if self.stage() == Malnutrition::Extreme {
            -HEALTH_DRAIN
        } else if self.starved <= 0.0 {
            HEALTH_RECOVER
        } else {
            // Malnourished but not yet starving outright: no worse, no better.
            0.0
        };
        self.points = (self.points + change * minutes).clamp(0.0, MAX_HEALTH);
    }
}
