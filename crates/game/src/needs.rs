//! What the Bim wants, and when it wants it.
//!
//! Five levels, each running from 1 (comfortable) down to 0. A level falling
//! past [`URGENT`] is what sets the matching errand going; doing the thing
//! fills it back up. Everything else about the Bim's day follows from these
//! numbers.
//!
//! Four of them run on the clock. The odd one out, cleanliness, has no clock
//! and no errand behind it: it follows the state of the deck the Bim is
//! standing on and of the Bim itself, and what it does when it runs out is in
//! `filth.rs`. Company is the newest and the only one that needs *another Bim*
//! rather than a fixture — what going without it does is in `social.rs`.
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
/// Four times a waking day — the crew are talkative. Deliberately far more
/// often than the solitude clock in `social.rs` needs: that one only wants
/// resetting once in three days, so a bar on this rate leaves a very wide
/// margin before anything starts to go wrong. A Bim that cannot get a word in
/// for a day is uncomfortable, not damaged, and the margin is where that
/// difference lives.
const CHATS_PER_DAY: f32 = 4.0;

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
/// Crossing the room to where the other one is and standing there talking.
const CHAT_COST: f32 = 12.0;

/// Per game minute *awake*: each need falls from full to urgent exactly once
/// per slot, and the slots tile the waking day with room for the errands.
const REST_DRAIN: f32 = SPAN / (BODY_WAKING / SLEEPS_PER_DAY - TO_BED_COST);
const FOOD_DRAIN: f32 = SPAN / (WAKING / MEALS_PER_DAY - MEAL_COST);
/// The restroom need is the exception, and its slot is the *whole* day rather
/// than the waking part of it: it is the one thing that keeps draining while
/// the Bim sleeps, so a night is six hours of it. Derived from the same
/// arithmetic as the others, so three visits a day still means three — count
/// only the waking minutes here and the night quietly adds a fourth.
const RESTROOM_DRAIN: f32 = SPAN / (DAY / VISITS_PER_DAY - VISIT_COST);
/// Company runs on the waking day, like food: nobody is lonely in their sleep.
///
/// The one need whose arithmetic is **not** written against [`URGENT`], and
/// deliberately so. A Bim goes looking for company at [`COMPANY_TRIGGER`] —
/// half a bar rather than a tenth — and a conversation only puts
/// [`CHAT_FILLS`] back rather than filling it, so the span this travels
/// between one chat and the next is that, not the whole bar. Written the same
/// way as the others: the distance covered, over the time it is meant to take.
const COMPANY_DRAIN: f32 = CHAT_FILLS / (WAKING / CHATS_PER_DAY - CHAT_COST);

/// Roughly how long the act of putting each need right takes, in game minutes.
/// Recovery is spread over that so a bar visibly fills while the Bim is doing
/// the thing rather than jumping when the chain ends. Filling a moment early
/// is harmless: the level clamps, and this way `needs.rs` does not have to
/// know the exact length of a step in `task.rs`.
const EATING: f32 = 6.0;
const RELIEF: f32 = 5.0;
/// A conversation. Short — it is two people standing on the deck, not an
/// evening — and what it is worth is spread over exactly the length of it.
const TALKING: f32 = 6.0;

/// Where the company bar sends the Bim looking for the other one, and how much
/// of it a conversation puts back.
///
/// Both are unlike every other need here, and both pull the same way: they
/// talk **often**. Half a bar is a low bar to clear, and a chat clearing only
/// three tenths of it means the next one is never far off. A need that emptied
/// to a tenth and then filled to the brim would have the crew ignore each other
/// for most of a day and then have one long conversation, which is not what two
/// people sharing one compartment do.
pub const COMPANY_TRIGGER: f32 = 0.50;
const CHAT_FILLS: f32 = 0.30;

/// Sleeping still covers exactly the span over exactly the six hours; it is
/// only the run-down that drifts, not the night itself.
const REST_RECOVER: f32 = SPAN / SLEEP_MINUTES;
/// A meal fills the whole range whatever it started at, as does a visit.
const FOOD_RECOVER: f32 = FULL / EATING;
const RESTROOM_RECOVER: f32 = FULL / RELIEF;
/// A word does not fill anybody up the way a meal does. A conversation is
/// worth [`CHAT_FILLS`] of the bar and no more, so the Bim is back looking for
/// the other one before the day is out however many it has already had — which
/// is the point: company is a thing you keep needing, not a tank you top up.
const COMPANY_RECOVER: f32 = CHAT_FILLS / TALKING;

/// Where a day starts. The Bim is rested, having just got up, and the other
/// two are part-used so that the first morning has something in it rather than
/// eight quiet hours: the heads at about ten, a meal at about half twelve.
const REST_AT_DAWN: f32 = 1.0;
const FOOD_AT_DAWN: f32 = 0.55;
const RESTROOM_AT_DAWN: f32 = 0.40;
/// A clean Bim in a clean room. This one only moves if something makes a mess.
const CLEAN_AT_DAWN: f32 = 1.0;
/// They woke up in the same compartment they went to sleep in, so neither is
/// short of company yet.
const COMPANY_AT_DAWN: f32 = 0.85;

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

/// Anything under this and the Bim goes before bed rather than turning in on
/// it. Well above the trigger it would otherwise wait for, because a night is
/// six hours long and the restroom need is the one thing that keeps draining
/// through it: turning in at four fifths ends the night at nothing.
pub const BEFORE_BED: f32 = 0.80;

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
    /// Somebody to talk to. Back on the clock, and filled by the one thing
    /// aboard that needs two Bims — see `social.rs` for what going without it
    /// does, which is a good deal worse than what this bar shows.
    Company,
}

impl Need {
    /// In the order they are shown, and the order the host indexes them by.
    /// Appended to rather than inserted into: the index is the whole contract
    /// across the boundary, and the host's `NEED_NAMES` is read off it.
    pub const ALL: [Need; 5] = [
        Need::Rest,
        Need::Food,
        Need::Restroom,
        Need::Cleanliness,
        Need::Company,
    ];

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
            Need::Company => COMPANY_DRAIN,
        }
    }

    /// Where this need's trigger starts out.
    ///
    /// [`URGENT`] for everything the Bim can only put right by going somewhere
    /// and doing something, which is most of them. Company is the exception:
    /// it is filled by the *other* Bim being free at the same moment, so
    /// waiting until the bar is nearly empty would mean waiting until the one
    /// thing that fixes it is least likely to be available. Asking early and
    /// often is how two people in one compartment actually behave.
    fn trigger_at(self) -> f32 {
        match self {
            Need::Company => COMPANY_TRIGGER,
            _ => URGENT,
        }
    }

    fn recover(self) -> f32 {
        match self {
            Need::Rest => REST_RECOVER,
            Need::Food => FOOD_RECOVER,
            Need::Restroom => RESTROOM_RECOVER,
            Need::Cleanliness => 0.0,
            Need::Company => COMPANY_RECOVER,
        }
    }
}

/// The level at which a need sends the Bim to see to it, and whether it does
/// so at all.
///
/// [`URGENT`] is where every one of these starts, and the derivation of the
/// drain rates above still assumes it: move a trigger and that need's slot no
/// longer tiles the day the way the arithmetic says it does. That is the
/// player's business — a Bim told to eat at half full eats more often, which
/// is the point of being able to say so — but it is why the constant stays
/// where it is rather than becoming a field.
///
/// Switching one off is not the same as setting it to nothing. Off, the need
/// carries on draining and everything going without does to the Bim still
/// bites; all that stops is the Bim going and doing something about it on its
/// own account.
#[derive(Clone, Copy)]
pub struct Trigger {
    pub on: bool,
    pub at: f32,
}

impl Trigger {
    const fn at(level: f32) -> Trigger {
        Trigger {
            on: true,
            at: level,
        }
    }

    /// Whether `level` has fallen past this trigger. Always false while it is
    /// switched off, whatever the level.
    pub fn caught(self, level: f32) -> bool {
        self.on && level < self.at
    }
}

pub struct Needs {
    levels: [f32; Need::ALL.len()],
    triggers: [Trigger; Need::ALL.len()],
}

impl Needs {
    pub fn new() -> Needs {
        Needs {
            levels: [
                REST_AT_DAWN,
                FOOD_AT_DAWN,
                RESTROOM_AT_DAWN,
                CLEAN_AT_DAWN,
                COMPANY_AT_DAWN,
            ],
            triggers: Need::ALL.map(|need| Trigger::at(need.trigger_at())),
        }
    }

    pub fn trigger(&self, need: Need) -> Trigger {
        self.triggers[need as usize]
    }

    /// Move a trigger. The level is clamped to the bar it sits on, so a
    /// figure typed past either end of it cannot hide off the scale.
    pub fn set_trigger_at(&mut self, need: Need, at: f32) {
        self.triggers[need as usize].at = at.clamp(0.0, FULL);
    }

    pub fn set_trigger_on(&mut self, need: Need, on: bool) {
        self.triggers[need as usize].on = on;
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

        let asleep = restoring == Some(Need::Rest);
        for need in Need::ALL {
            // Asleep, the clock stops for hunger — see the note at the top —
            // but *not* for the restroom need. A body does not stop making
            // water because its owner is unconscious, so that one runs all
            // night and the Bim wakes up needing the heads. What keeps the
            // three-a-day from drifting into four is the other end of it: the
            // Bim empties out before it turns in, so the night starts from
            // full rather than from wherever the evening left it.
            if asleep && need != Need::Restroom {
                continue;
            }
            let rate = need.drain() * if need == Need::Rest { tiring } else { 1.0 };
            let level = &mut self.levels[need as usize];
            *level = (*level - rate * minutes).max(0.0);
        }

        if let Some(need) = restoring {
            let level = &mut self.levels[need as usize];
            *level = (*level + need.recover() * minutes).min(FULL);
        }
    }

    /// Everything past its trigger, emptiest first. A list rather than a
    /// single answer because the Bim may not be able to do anything about the
    /// most pressing one — an empty fridge, a locked door — and the ones
    /// behind it should not be held up by that.
    ///
    /// A need whose trigger is switched off is never in here, however empty it
    /// is: that is the whole of what switching one off does.
    pub fn urgent(&self) -> Vec<Need> {
        let mut out: Vec<Need> = Need::ALL
            .into_iter()
            .filter(|&need| self.trigger(need).caught(self.level(need)))
            .collect();
        out.sort_by(|&a, &b| self.level(a).total_cmp(&self.level(b)));
        out
    }
}
