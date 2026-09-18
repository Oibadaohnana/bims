//! Going without, and what it costs: food on one side, sleep on the other.
//!
//! An empty stomach is not itself harmful — the Bim can be hungry for a while
//! and simply be hungry. What does the damage is staying that way, so the
//! measure here is *time spent with nothing in it* rather than the hunger
//! level, and the three stages of malnutrition are thresholds on that clock.
//!
//! Only the last stage costs health. The first two are a warning: a Bim that
//! is merely slow and tiring easily can still be fed and will come right.
//!
//! # The body is three parts, and the blood is a fourth number
//!
//! Health is not one bar but three — the head, the body and the legs, each
//! with a base of its own ([`Part::max`]: 5, 75 and 20, a hundred all told)
//! — and what the panel calls health is the three added up. A shot lands
//! on one part ([`Part::HIT_ODDS`]: one in twenty the head, three in four
//! the body, one in five the legs) and takes the weapon's damage off that
//! part alone. The head or the body at nothing is death. The legs at
//! nothing is a **leg lost**: the Bim goes on, on the one it has left, its
//! leg health starts again from the base, and the second time the legs
//! reach nothing there are none left. Starvation and mending run over all
//! three in proportion, so the total behaves exactly as the one bar did.
//!
//! Beside the three, **blood**: a hundred points, and every hit opens a
//! wound that bleeds [`BLEED_PER_WOUND`] of it an hour until it is dressed
//! — so ten open wounds bleed a Bim out in an hour. Under half, the Bim
//! walks at half its pace; under [`OUT_AT`], it is out cold where it
//! stands; at nothing it is dead. A bandage ([`Health::bandage`]) closes
//! every wound on one part, and blood comes back on its own once nothing
//! is open. Armour will one day stop a wound opening; nothing does yet.

use crate::clock::{DAY, HOUR};

pub const MAX_HEALTH: f32 = 100.0;

/// The blood a body has, full.
pub const MAX_BLOOD: f32 = 100.0;

/// Blood lost an hour by each open wound.
pub const BLEED_PER_WOUND: f32 = 10.0;

/// Below this share of its blood the Bim walks at half its pace, and
/// below the second it is out cold.
pub const SLOWED_AT: f32 = 0.5;
pub const OUT_AT: f32 = 0.3;

/// How fast blood comes back once nothing is bleeding: from nothing to
/// full in two days, the same as health.
const BLOOD_RECOVER: f32 = MAX_BLOOD / (2.0 * DAY);

/// How a lost leg slows the walk: half on one leg, a crawl on none.
const ONE_LEG_PACE: f32 = 0.5;
const NO_LEGS_PACE: f32 = 0.25;

/// Where a shot lands. The codes are the app's: the three armour slots
/// are in the same order.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Part {
    Head = 0,
    Body = 1,
    Legs = 2,
}

impl Part {
    pub const ALL: [Part; 3] = [Part::Head, Part::Body, Part::Legs];

    /// The odds a shot lands on each, in `ALL` order. They add to one.
    pub const HIT_ODDS: [f32; 3] = [0.05, 0.75, 0.20];

    pub fn code(self) -> u32 {
        self as u32
    }

    pub fn from_code(code: u32) -> Option<Part> {
        Part::ALL.get(code as usize).copied()
    }

    /// The part's whole health. The three add to [`MAX_HEALTH`].
    pub fn max(self) -> f32 {
        match self {
            Part::Head => 5.0,
            Part::Body => 75.0,
            Part::Legs => 20.0,
        }
    }

    /// Which part a roll of `unit` (0 to 1) lands on, by [`Part::HIT_ODDS`].
    pub fn hit_by(unit: f32) -> Part {
        let mut edge = 0.0;
        for (i, odds) in Part::HIT_ODDS.iter().enumerate() {
            edge += odds;
            if unit < edge {
                return Part::ALL[i];
            }
        }
        Part::Legs
    }
}

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
    /// The head, the body and the legs, in [`Part::ALL`] order.
    parts: [f32; 3],
    /// How many legs are gone: none, one, or both.
    legs_lost: u32,
    blood: f32,
    /// Open wounds on each part, bleeding until dressed.
    wounds: [u32; 3],
}

impl Health {
    pub fn new() -> Health {
        Health {
            starved: 0.0,
            sleepless: 0.0,
            parts: [Part::Head.max(), Part::Body.max(), Part::Legs.max()],
            legs_lost: 0,
            blood: MAX_BLOOD,
            wounds: [0; 3],
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

    /// The three parts added up: what the panel's one bar shows.
    pub fn points(&self) -> f32 {
        self.parts.iter().sum()
    }

    /// One part's health.
    pub fn part(&self, part: Part) -> f32 {
        self.parts[part as usize]
    }

    pub fn blood(&self) -> f32 {
        self.blood
    }

    /// Open wounds on one part.
    pub fn wounds(&self, part: Part) -> u32 {
        self.wounds[part as usize]
    }

    /// Open wounds all told.
    pub fn bleeding(&self) -> u32 {
        self.wounds.iter().sum()
    }

    pub fn legs_lost(&self) -> u32 {
        self.legs_lost
    }

    /// Dead: the head or the body at nothing, or bled out.
    pub fn is_dead(&self) -> bool {
        self.parts[Part::Head as usize] <= 0.0
            || self.parts[Part::Body as usize] <= 0.0
            || self.blood <= 0.0
    }

    /// Out cold for want of blood. Not dead — that is [`Health::is_dead`].
    pub fn unconscious(&self) -> bool {
        !self.is_dead() && self.blood < MAX_BLOOD * OUT_AT
    }

    /// How fast it walks for what the fight has done to it, as a fraction
    /// of its usual pace: the legs it has left, and the blood.
    pub fn pace(&self) -> f32 {
        let legs = match self.legs_lost {
            0 => 1.0,
            1 => ONE_LEG_PACE,
            _ => NO_LEGS_PACE,
        };
        let blood = if self.blood < MAX_BLOOD * SLOWED_AT {
            0.5
        } else {
            1.0
        };
        legs * blood
    }

    /// A shot landing on `part`: the damage off that part, and a wound
    /// opened on it. The legs at nothing is a leg lost and the leg health
    /// started again; the second time, there are none left and the legs
    /// stay at nothing. `true` when a leg went.
    pub fn shot(&mut self, part: Part, damage: f32) -> bool {
        let i = part as usize;
        self.wounds[i] += 1;
        let mut leg_lost = false;
        match part {
            Part::Head | Part::Body => {
                self.parts[i] = (self.parts[i] - damage).max(0.0);
            }
            Part::Legs => {
                if self.legs_lost < 2 {
                    self.parts[i] = (self.parts[i] - damage).max(0.0);
                    if self.parts[i] <= 0.0 {
                        self.legs_lost += 1;
                        leg_lost = true;
                        self.parts[i] = if self.legs_lost < 2 {
                            Part::Legs.max()
                        } else {
                            0.0
                        };
                    }
                }
            }
        }
        leg_lost
    }

    /// Dress every wound on one part. `true` when there was one to dress.
    pub fn bandage(&mut self, part: Part) -> bool {
        let i = part as usize;
        let had = self.wounds[i] > 0;
        self.wounds[i] = 0;
        had
    }

    /// Take a flat amount off the body, never past nothing. Hunger works on
    /// the health bar over hours; this is for the things that happen all at
    /// once — so far, a Bim nobody has spoken to in a week hurting itself.
    pub fn hurt(&mut self, points: f32) {
        let body = &mut self.parts[Part::Body as usize];
        *body = (*body - points).max(0.0);
    }

    /// The end of it, by the Bim's own hand. Kept apart from [`Health::hurt`]
    /// with everything left of the bar taken at once, so that what happened is
    /// legible here rather than being a subtraction that happened to reach
    /// zero.
    pub fn give_up(&mut self) {
        self.parts = [0.0; 3];
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
        // Nothing comes back from nothing. Health mends on its own while the
        // Bim is fed, and without this the bar taken to zero by anything
        // *sudden* — a Bim hurting itself, a Bim giving up — is back above
        // zero on the very next frame, before `Game` has looked at it. The
        // death then simply never happens: the run ends with a Bim whose
        // health reads nought and who is still walking about.
        if self.is_dead() {
            return;
        }
        // The blood: out through every open wound, back on its own once
        // nothing is open. Bleeding to nothing is a death like any other,
        // and the check at the top of the next tick is what says so.
        let open = self.bleeding();
        if open > 0 {
            let loss = open as f32 * BLEED_PER_WOUND / HOUR * minutes;
            self.blood = (self.blood - loss).max(0.0);
        } else {
            self.blood = (self.blood + BLOOD_RECOVER * minutes).min(MAX_BLOOD);
        }
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
        // Over the three parts in proportion to their size, so the total
        // goes from full to nothing in a day the way the one bar did. Legs
        // that are gone do not grow back.
        for part in Part::ALL {
            let i = part as usize;
            if part == Part::Legs && self.legs_lost >= 2 {
                self.parts[i] = 0.0;
                continue;
            }
            let share = part.max() / MAX_HEALTH;
            self.parts[i] = (self.parts[i] + change * share * minutes).clamp(0.0, part.max());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_parts_add_to_a_hundred_and_the_odds_to_one() {
        let total: f32 = Part::ALL.iter().map(|p| p.max()).sum();
        assert_eq!(total, MAX_HEALTH);
        let odds: f32 = Part::HIT_ODDS.iter().sum();
        assert!((odds - 1.0).abs() < 1e-6);
        assert_eq!(Part::hit_by(0.0), Part::Head);
        assert_eq!(Part::hit_by(0.049), Part::Head);
        assert_eq!(Part::hit_by(0.05), Part::Body);
        assert_eq!(Part::hit_by(0.799), Part::Body);
        assert_eq!(Part::hit_by(0.8), Part::Legs);
        assert_eq!(Part::hit_by(0.999), Part::Legs);
    }

    #[test]
    fn a_head_shot_kills_and_a_body_shot_wears_it_down() {
        let mut h = Health::new();
        assert!(!h.shot(Part::Body, 12.0));
        assert_eq!(h.part(Part::Body), 63.0);
        assert_eq!(h.points(), 88.0);
        assert!(!h.is_dead());
        h.shot(Part::Head, 12.0);
        assert!(h.is_dead());
    }

    #[test]
    fn the_legs_go_one_at_a_time_and_the_walk_slows_with_them() {
        let mut h = Health::new();
        assert_eq!(h.pace(), 1.0);
        assert!(!h.shot(Part::Legs, 12.0));
        assert!(h.shot(Part::Legs, 12.0), "the second shot takes the leg");
        assert_eq!(h.legs_lost(), 1);
        assert_eq!(h.part(Part::Legs), Part::Legs.max(), "started again");
        assert_eq!(h.pace(), ONE_LEG_PACE);
        assert!(!h.is_dead());
        h.shot(Part::Legs, 12.0);
        assert!(h.shot(Part::Legs, 12.0));
        assert_eq!(h.legs_lost(), 2);
        assert_eq!(h.part(Part::Legs), 0.0);
        assert_eq!(h.pace(), NO_LEGS_PACE);
        assert!(!h.is_dead(), "no legs is not dead");
        // And nothing grows back.
        h.update(DAY, 1.0, 1.0, false);
        assert_eq!(h.part(Part::Legs), 0.0);
    }

    #[test]
    fn ten_wounds_bleed_a_bim_out_in_an_hour_and_a_bandage_stops_it() {
        let mut h = Health::new();
        for _ in 0..10 {
            h.shot(Part::Body, 1.0);
        }
        assert_eq!(h.bleeding(), 10);
        h.update(HOUR * 0.5, 1.0, 1.0, false);
        assert!((h.blood() - 50.0).abs() < 1e-3, "{}", h.blood());
        assert_eq!(h.pace(), 1.0, "half is not under half");
        h.update(1.0, 1.0, 1.0, false);
        assert_eq!(h.pace(), 0.5);
        assert!(!h.unconscious());
        h.update(HOUR * 0.25, 1.0, 1.0, false);
        assert!(h.unconscious(), "{}", h.blood());
        assert!(!h.is_dead());
        assert!(h.bandage(Part::Body));
        assert_eq!(h.bleeding(), 0);
        assert!(!h.bandage(Part::Body), "nothing left to dress");
        let before = h.blood();
        h.update(HOUR, 1.0, 1.0, false);
        assert!(h.blood() > before, "blood comes back once nothing is open");

        let mut h = Health::new();
        for _ in 0..10 {
            h.shot(Part::Body, 1.0);
        }
        h.update(HOUR, 1.0, 1.0, false);
        assert!(h.is_dead(), "bled out");
    }

    #[test]
    fn starvation_still_takes_a_day_over_the_whole_of_it() {
        let mut h = Health::new();
        // Up to the last stage — the stage is read after the minutes are
        // added, so the step that crosses the line already drains — then a
        // day on nothing.
        h.update(EXTREME_AT - 1.0, 0.0, 1.0, false);
        assert!(!h.is_dead());
        assert_eq!(h.points(), MAX_HEALTH);
        h.update(DAY * 0.5, 0.0, 1.0, false);
        assert!((h.points() - 50.0).abs() < 0.5, "{}", h.points());
        assert!(!h.is_dead());
        h.update(DAY * 0.5 + 1.0, 0.0, 1.0, false);
        assert!(h.is_dead());
    }
}
