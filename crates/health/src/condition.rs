//! What is wrong with a body, and what each of those costs it.
//!
//! Health is not driven by a pile of special cases: it is driven by a **list
//! of conditions**, and a condition is four numbers — how much health it
//! takes per minute, whether it stops the body mending, and what it does to
//! how fast the body works and walks. [`crate::update`] never asks "is this
//! Bim irradiated"; it asks for the conditions in force and adds them up.
//!
//! Radiation and cancer are the first two kinds. Hunger, sleep loss and
//! filth are meant to arrive as further variants of [`Condition`] with rows
//! of their own in [`Condition::effect`], and nothing else should have to
//! change when they do. They are **not** ported here: the room's
//! `crates/game/src/health.rs` still owns those, and moving them is its own
//! step.
//!
//! Two conventions the rest of the crate leans on:
//!
//! - **A band is closed at the bottom.** A dose of exactly [`CRITICAL`] is
//!   critical, exactly [`SICKNESS`] is sickness. [`RadiationStage::of`] is
//!   the one place that is decided, and reversing it is the same table read
//!   backwards — there is no separate ladder for a dose coming down.
//! - **Stages are ordered and adjacent.** [`RadiationStage::rung`] numbers
//!   them 0 to 3 and a crossing only ever moves one rung at a time, which is
//!   what lets a single update that goes from nothing to sickness emit the
//!   two events in between rather than one event naming where it ended up.

use crate::data::{
    CANCER_ADVANCED_AT, CANCER_ADVANCED_DAMAGE, CANCER_ADVANCED_MOVE, CANCER_ADVANCED_WORK,
    CANCER_EARLY_DAMAGE, CANCER_EARLY_MOVE, CANCER_EARLY_WORK, CANCER_TERMINAL_AT,
    CANCER_TERMINAL_DAMAGE, CANCER_TERMINAL_MOVE, CANCER_TERMINAL_WORK, CRITICAL, CRITICAL_DAMAGE,
    SICKNESS, SICKNESS_DAMAGE, SICKNESS_MOVE, SICKNESS_WORK,
};

/// How irradiated a body is, read off its dose.
///
/// `Elevated` is deliberately a stage with no effect whatever. It is what
/// keeps the meter on screen and the warning honest: a Bim that has been out
/// there is carrying something, and the player can watch it come off.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum RadiationStage {
    None,
    Elevated,
    Critical,
    RadiationSickness,
}

impl RadiationStage {
    /// The stage a dose reads as. Bands are closed at the bottom, so exactly
    /// [`CRITICAL`] is already critical.
    pub fn of(dose: f64) -> RadiationStage {
        if dose >= SICKNESS {
            RadiationStage::RadiationSickness
        } else if dose >= CRITICAL {
            RadiationStage::Critical
        } else if dose > 0.0 {
            RadiationStage::Elevated
        } else {
            RadiationStage::None
        }
    }

    /// The stage in force over the instant *after* now, for a dose that is
    /// falling.
    ///
    /// The bands being closed at the bottom is right for the state a dose
    /// reads as and wrong for the rate to apply next: a dose sitting exactly
    /// on [`CRITICAL`] and coming down is critical *now* and elevated for
    /// every instant after, so an update segment starting there must be
    /// charged the elevated rate or it would charge critical damage over an
    /// interval the body was never critical for. Without this the segment
    /// would also be of no length at all, and the loop would not advance.
    pub(crate) fn below(dose: f64) -> RadiationStage {
        if dose > SICKNESS {
            RadiationStage::RadiationSickness
        } else if dose > CRITICAL {
            RadiationStage::Critical
        } else if dose > 0.0 {
            RadiationStage::Elevated
        } else {
            RadiationStage::None
        }
    }

    /// The dose at the bottom of this band. Where a falling dose leaves it,
    /// and therefore where an update segment has to be cut.
    pub(crate) fn floor(self) -> f64 {
        match self {
            RadiationStage::None => 0.0,
            RadiationStage::Elevated => 0.0,
            RadiationStage::Critical => CRITICAL,
            RadiationStage::RadiationSickness => SICKNESS,
        }
    }

    /// 0 for a clean body, then 1, 2, 3. The host names them; this is what
    /// makes "one rung at a time" something the code can walk.
    pub fn rung(self) -> u32 {
        match self {
            RadiationStage::None => 0,
            RadiationStage::Elevated => 1,
            RadiationStage::Critical => 2,
            RadiationStage::RadiationSickness => 3,
        }
    }
}

/// How far along the cancer is. There is no stage before `Early` — a body
/// either has it or does not, which is what [`Option`] on the state says —
/// and there is no way back from any of them.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum CancerStage {
    Early,
    Advanced,
    Terminal,
}

impl CancerStage {
    /// The stage at `minutes` since it began.
    pub fn of(minutes_since_onset: f64) -> CancerStage {
        if minutes_since_onset >= CANCER_TERMINAL_AT {
            CancerStage::Terminal
        } else if minutes_since_onset >= CANCER_ADVANCED_AT {
            CancerStage::Advanced
        } else {
            CancerStage::Early
        }
    }

    /// When this stage gives way to the next, in minutes since onset, or
    /// `None` for the last one. Where an update segment has to be cut.
    pub(crate) fn ends_at(self) -> Option<f64> {
        match self {
            CancerStage::Early => Some(CANCER_ADVANCED_AT),
            CancerStage::Advanced => Some(CANCER_TERMINAL_AT),
            CancerStage::Terminal => None,
        }
    }

    /// 0, 1, 2. As with [`RadiationStage::rung`], so a long update can walk
    /// the stages it passed through rather than jumping to the last.
    pub fn rung(self) -> u32 {
        match self {
            CancerStage::Early => 0,
            CancerStage::Advanced => 1,
            CancerStage::Terminal => 2,
        }
    }
}

/// Something wrong with a body. One variant per kind, each carrying its own
/// stage — a body has at most one radiation stage and at most one cancer
/// stage, so the list is short and fixed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Condition {
    Radiation(RadiationStage),
    Cancer(CancerStage),
}

impl Condition {
    /// What this condition costs, per minute and as a multiplier. **This is
    /// the whole of what a condition does** — there is no other place a
    /// stage is consulted, which is what makes adding hunger later a row
    /// here rather than a branch in [`crate::update`].
    pub fn effect(self) -> Effect {
        match self {
            // Carrying a dose that is doing no harm yet. On the meter, and
            // nothing else: it must not suppress mending, or a Bim would
            // never come right after a walk outside.
            Condition::Radiation(RadiationStage::None | RadiationStage::Elevated) => Effect {
                damage: 0.0,
                suppresses_mending: false,
                work_speed: 1.0,
                move_speed: 1.0,
            },
            Condition::Radiation(RadiationStage::Critical) => Effect {
                damage: CRITICAL_DAMAGE,
                suppresses_mending: true,
                work_speed: 1.0,
                move_speed: 1.0,
            },
            Condition::Radiation(RadiationStage::RadiationSickness) => Effect {
                damage: SICKNESS_DAMAGE,
                suppresses_mending: true,
                work_speed: SICKNESS_WORK,
                move_speed: SICKNESS_MOVE,
            },
            Condition::Cancer(CancerStage::Early) => Effect {
                damage: CANCER_EARLY_DAMAGE,
                suppresses_mending: true,
                work_speed: CANCER_EARLY_WORK,
                move_speed: CANCER_EARLY_MOVE,
            },
            Condition::Cancer(CancerStage::Advanced) => Effect {
                damage: CANCER_ADVANCED_DAMAGE,
                suppresses_mending: true,
                work_speed: CANCER_ADVANCED_WORK,
                move_speed: CANCER_ADVANCED_MOVE,
            },
            Condition::Cancer(CancerStage::Terminal) => Effect {
                damage: CANCER_TERMINAL_DAMAGE,
                suppresses_mending: true,
                work_speed: CANCER_TERMINAL_WORK,
                move_speed: CANCER_TERMINAL_MOVE,
            },
        }
    }
}

/// What one condition does. Damage is health per minute and is never
/// negative — a condition that *mended* would be a treatment, and there is
/// no treatment in this step.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Effect {
    pub damage: f64,
    /// Whether the body's own mending stops while this is in force. Anything
    /// that does damage sets it; the rule "no mending while a dose is
    /// critical or cancer is present" is these flags and nothing else.
    pub suppresses_mending: bool,
    pub work_speed: f64,
    pub move_speed: f64,
}

/// What a body's conditions do to it, all multiplied together. 1.0 is
/// normal, and a healthy body gets exactly 1.0 for both.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Effects {
    pub work_speed: f64,
    pub move_speed: f64,
}

/// The conditions in force, in a fixed array rather than a `Vec`.
///
/// [`crate::update`] asks for this once per segment of every update of every
/// Bim of every frame, and a body can have at most one of each kind — so the
/// allocation would buy nothing. Growing it is one number here.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Conditions {
    items: [Option<Condition>; Conditions::ROOM],
    len: u32,
}

impl Conditions {
    /// One per kind of condition there can be. Adding hunger means adding
    /// one here as well as a variant.
    const ROOM: usize = 2;

    pub(crate) fn push(&mut self, condition: Condition) {
        let at = self.len as usize;
        debug_assert!(
            at < Self::ROOM,
            "more conditions at once than there is room for"
        );
        if at < Self::ROOM {
            self.items[at] = Some(condition);
            self.len += 1;
        }
    }

    pub fn len(&self) -> u32 {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn iter(&self) -> impl Iterator<Item = Condition> + '_ {
        self.items[..self.len as usize].iter().flatten().copied()
    }

    /// Health lost per minute to all of them together.
    pub fn damage(&self) -> f64 {
        self.iter().map(|c| c.effect().damage).sum()
    }

    /// Whether anything in force stops the body mending.
    pub fn suppress_mending(&self) -> bool {
        self.iter().any(|c| c.effect().suppresses_mending)
    }

    /// Their multipliers, multiplied. Nothing wrong gives 1.0 and 1.0,
    /// because an empty product is one.
    pub fn effects(&self) -> Effects {
        let mut effects = Effects {
            work_speed: 1.0,
            move_speed: 1.0,
        };
        for condition in self.iter() {
            let effect = condition.effect();
            effects.work_speed *= effect.work_speed;
            effects.move_speed *= effect.move_speed;
        }
        effects
    }
}
