//! Company: what the crew get from each other, and what happens without it.
//!
//! Two of them live aboard and nobody else is coming. Talking is the only
//! thing that fills the fifth need, and the only thing that stops the clock in
//! [`Solitude`] — which is not the same clock. The **need** empties in a day
//! and is what sends a Bim looking for the other one; the **solitude clock**
//! runs in days and is what going without actually costs, on the same pattern
//! as malnutrition in `health.rs` and standing in the mess in `filth.rs`:
//! reaching nothing is the start of it, not the end.
//!
//! | alone for | stage | what it does |
//! | --- | --- | --- |
//! | 3 days | desocialized | broods — a low entry in the diary — and everything it does takes a tenth longer |
//! | 5 days | severe | breaks off about every five hours and sits on the deck for ten minutes |
//! | 7 days | isolated | hurts itself every four hours, ten points of health each time |
//! | 10 days | — | 3% an hour of giving up altogether, and three points more for every further day |
//!
//! The stages do not replace each other: an isolated Bim is still brooding and
//! still breaking down, because each stage is the one before it and worse.
//!
//! **This is a clock on a body.** It belongs to a `Bim` and never to anything
//! shared — see the note on `filth::Ordeal`, which is the same lesson learned
//! the expensive way: a per-Bim clock parked on a shared object gets zeroed by
//! whichever Bim is comfortable, and the stages built on it quietly never fire.

use crate::clock::{DAY, HOUR};
use crate::rng::Rng;

/// How long alone before each stage sets in.
const DESOCIALIZED_AT: f32 = 3.0 * DAY;
const SEVERE_AT: f32 = 5.0 * DAY;
const ISOLATED_AT: f32 = 7.0 * DAY;
/// And how long before it stops wanting to go on at all.
const DESPAIRS_AT: f32 = 10.0 * DAY;

/// Game minutes between one low moment and the next. Often enough that a day
/// of it leaves a legible run of entries in the diary, rare enough that the
/// diary is still readable.
const BROODS_EVERY: f32 = 4.0 * HOUR;

/// The chance per game hour of breaking off and sitting down, once it is bad
/// enough for that — "roughly every five hours" rather than on the dot, the
/// same shape as the accident roll in `filth.rs`.
const BREAKS_PER_HOUR: f32 = 1.0 / 5.0;
/// And how long it sits there, in game minutes.
pub const SITS_FOR: f32 = 10.0;

/// Game minutes between one bout of self-harm and the next, and what each
/// costs.
///
/// The arithmetic matters here, because health mends as well as breaks. A
/// well-fed Bim recovers half its health a day, so at six bouts a day this is
/// ten points a day of net damage: three days from the isolated stage to the
/// despair that follows it leaves it worn down and still alive, which is the
/// shape the escalation wants. Make it much faster and nothing ever reaches
/// day ten.
const HURTS_EVERY: f32 = 4.0 * HOUR;
pub const SELF_HARM: f32 = 10.0;

/// The chance per game hour of giving up, once [`DESPAIRS_AT`] has passed,
/// and how much that rises for every further day of it.
const DESPAIR_PER_HOUR: f32 = 0.03;
const DESPAIR_RISE_PER_DAY: f32 = 0.03;

/// How fast a Bim works once it has gone without company: a tenth slower.
/// Applied to the errand chain and to the walking both, so "everything takes
/// longer" means everything.
const DRAGS_AT: f32 = 0.90;

/// How far gone a Bim is for want of anybody to talk to.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Loneliness {
    None,
    /// Low, and slow with it.
    Desocialized,
    /// Breaking off and sitting down.
    Severe,
    /// Hurting itself, and past ten days, worse.
    Isolated,
}

impl Loneliness {
    /// 0 for a Bim with company, then 1, 2, 3. The host names them.
    pub fn stage(self) -> u32 {
        self as u32
    }

    /// How fast it gets on with things, as a fraction of its usual rate.
    /// Multiplies with every other thing that slows a Bim down.
    pub fn works_at(self) -> f32 {
        match self {
            Loneliness::None => 1.0,
            _ => DRAGS_AT,
        }
    }

    /// Whether it is low enough to be writing miserable things in its diary.
    pub fn broods(self) -> bool {
        self >= Loneliness::Desocialized
    }

    /// Whether it breaks off and sits on the deck.
    pub fn breaks_down(self) -> bool {
        self >= Loneliness::Severe
    }

    /// Whether it has begun hurting itself.
    pub fn harms_itself(self) -> bool {
        self >= Loneliness::Isolated
    }
}

/// What being alone did this frame. The game applies these: only it knows
/// where the Bim is standing, what it is in the middle of, and whether there
/// is anything it can do about any of it.
#[derive(Default, Clone, Copy)]
pub struct Fallout {
    /// A low moment worth remembering.
    pub brooded: bool,
    /// Stopped where it stood and sat down.
    pub broke_down: bool,
    /// Hurt itself. Worth [`SELF_HARM`] points of health.
    pub hurt_itself: bool,
    /// Stopped altogether.
    pub gave_up: bool,
}

/// How long *this Bim* has been without company, and what that is about to
/// cost it.
pub struct Solitude {
    /// Game minutes since it last talked to anybody.
    alone_for: f32,
    /// Sub-clocks, so each thing happens at its own interval rather than all
    /// of them landing on the same frame.
    since_brooded: f32,
    since_hurt: f32,
}

impl Solitude {
    pub fn new() -> Solitude {
        Solitude {
            alone_for: 0.0,
            since_brooded: 0.0,
            since_hurt: 0.0,
        }
    }

    /// Somebody talked to it. Everything goes back to the beginning — there
    /// are no half measures here: a conversation is company, and company is
    /// what all of this was the absence of.
    pub fn talked(&mut self) {
        self.alone_for = 0.0;
        self.since_brooded = 0.0;
        self.since_hurt = 0.0;
    }

    /// Game minutes it has been on its own. For the host's readout and for
    /// the probes, which need to see the clock running rather than infer it
    /// from a stage that is three days off.
    pub fn alone_for(&self) -> f32 {
        self.alone_for
    }

    /// Wind the clock forward by hand. For the probes: watching a Bim reach
    /// the last of this in real time is ten game days of frames, and the
    /// interesting part is what happens once it is there.
    #[allow(dead_code)]
    pub fn set_alone_for(&mut self, minutes: f32) {
        self.alone_for = minutes.max(0.0);
    }

    pub fn stage(&self) -> Loneliness {
        if self.alone_for >= ISOLATED_AT {
            Loneliness::Isolated
        } else if self.alone_for >= SEVERE_AT {
            Loneliness::Severe
        } else if self.alone_for >= DESOCIALIZED_AT {
            Loneliness::Desocialized
        } else {
            Loneliness::None
        }
    }

    /// The chance per game hour that it gives up, or zero before the tenth
    /// day. Three per cent to begin with and three more for every day past
    /// it, so the tenth day is survivable and the fortnight is not.
    pub fn despair_per_hour(&self) -> f32 {
        if self.alone_for < DESPAIRS_AT {
            return 0.0;
        }
        let days_past = ((self.alone_for - DESPAIRS_AT) / DAY).floor();
        (DESPAIR_PER_HOUR + DESPAIR_RISE_PER_DAY * days_past).clamp(0.0, 1.0)
    }

    /// Run the clocks and say what, if anything, happened.
    ///
    /// `can_break_down` is the game's veto: a Bim already sitting at the
    /// table or asleep in its bunk cannot sink to the deck, and asking it to
    /// would put it through the furniture.
    pub fn update(&mut self, minutes: f32, can_break_down: bool, rng: &mut Rng) -> Fallout {
        self.alone_for += minutes;
        let mut out = Fallout::default();
        let stage = self.stage();

        if stage.broods() {
            self.since_brooded += minutes;
            if self.since_brooded >= BROODS_EVERY {
                self.since_brooded = 0.0;
                out.brooded = true;
            }
        }

        // Not on a clock: this one is meant to come out of nowhere.
        if stage.breaks_down() && can_break_down && rng.chance(minutes / HOUR * BREAKS_PER_HOUR) {
            out.broke_down = true;
        }

        if stage.harms_itself() {
            self.since_hurt += minutes;
            if self.since_hurt >= HURTS_EVERY {
                self.since_hurt = 0.0;
                out.hurt_itself = true;
            }
        }

        let despair = self.despair_per_hour();
        if despair > 0.0 && rng.chance(minutes / HOUR * despair) {
            out.gave_up = true;
        }

        out
    }
}
