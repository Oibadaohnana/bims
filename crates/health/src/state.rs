//! One body's health, and the one function that moves it.
//!
//! # The order inside an update
//!
//! Dose first, then the stages it puts the body in, then mending, then
//! damage, then the clamp, then the death check — and **death is checked
//! last and is final**. That order is the lesson from the room's
//! `crates/game/src/health.rs`, written down there as a comment and repeated
//! here because it cost a bug: health that mends every frame will otherwise
//! undo a killing blow before anything has looked at whether the body is
//! dead, and the run ends with a Bim reading nought health and still walking
//! about. Here it cannot happen twice over: everything that does damage also
//! suppresses mending, and the moment health reaches nothing the update
//! stops dead rather than carrying on through the rest of the interval.
//!
//! # Why an update is cut into segments
//!
//! [`update`] must give the same answer for one step of a day as for 1440
//! steps of a minute. The play phase will call it every frame with a
//! fraction of a minute, a fast-forward will call it with hours, and a
//! native server catching up on a disconnection will call it with a day —
//! and all three have to agree, or the same ship gives two answers about who
//! survived.
//!
//! Rates are only constant *between* boundaries, so the interval is cut at
//! every boundary it crosses — a dose threshold, a dose reaching nothing,
//! cancer beginning, cancer advancing, health reaching full or nothing — and
//! each piece is applied in closed form at a rate that does not change
//! inside it. Nothing here integrates numerically and nothing here has a
//! fixed internal tick, both of which would make the answer depend on how
//! the caller happened to chop the time up.

use crate::condition::{CancerStage, Condition, Conditions, Effects, RadiationStage};
use crate::data::{CANCER, CRITICAL, DOSE_DECAY, MAX_HEALTH, MEND, RAD_RATE, SICKNESS};
use crate::event::HealthEvent;

/// A cancer, and the only thing worth knowing about one: how long the body
/// has had it. The stage is derived from that rather than stored, so there
/// is no way for the two to disagree.
///
/// There is **no cure and no `treat` function**. Treatment is a later step
/// and a medical bay is a part nobody has drawn; a body that has this has it
/// for good, which is what makes a day in the open a decision rather than an
/// inconvenience.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct CancerState {
    pub minutes_since_onset: f64,
}

/// Everything that is true of one body's health.
///
/// The fields are public because the play phase will want to read every one
/// of them to draw a panel, but [`update`] is what should move them: it is
/// the only thing that keeps `points` inside `0..=MAX_HEALTH`, `dose` at or
/// above nothing, and `dead` in step with `points`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct HealthState {
    /// `0..=MAX_HEALTH`.
    pub points: f64,
    /// Once true, never false again. [`update`] does nothing at all to a
    /// body that is dead.
    pub dead: bool,
    /// Accumulated radiation, in minutes-in-the-open at the default
    /// intensity. Never negative.
    pub dose: f64,
    /// `None` until the day the dose reaches [`CANCER`], and `Some` for ever
    /// after.
    pub cancer: Option<CancerState>,
}

impl HealthState {
    /// A body in full health with nothing wrong with it.
    pub fn new() -> HealthState {
        HealthState {
            points: MAX_HEALTH,
            dead: false,
            dose: 0.0,
            cancer: None,
        }
    }

    /// Full health and a cancer that has just begun. For a body that starts
    /// ill — and for the scenario that asks how long one lasts with nothing
    /// else wrong with it.
    pub fn with_cancer() -> HealthState {
        HealthState {
            cancer: Some(CancerState {
                minutes_since_onset: 0.0,
            }),
            ..HealthState::new()
        }
    }

    pub fn radiation_stage(&self) -> RadiationStage {
        RadiationStage::of(self.dose)
    }

    pub fn cancer_stage(&self) -> Option<CancerStage> {
        self.cancer.map(|c| CancerStage::of(c.minutes_since_onset))
    }
}

impl Default for HealthState {
    fn default() -> HealthState {
        HealthState::new()
    }
}

/// What the world is doing to the body this instant.
///
/// The play phase works this out per Bim per frame from
/// `shipdesign::ExposureMap` and where the Bim is standing; this crate never
/// asks where anybody is.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Exposure {
    /// Under cover. The dose comes off.
    Shielded,
    /// In the open. `intensity` is a multiplier on [`RAD_RATE`], 1.0 being
    /// the default — a hotter system one day is a bigger number here and
    /// nothing else in this crate changes.
    Exposed { intensity: f64 },
}

impl Exposure {
    /// In the open at the usual strength.
    pub fn normal() -> Exposure {
        Exposure::Exposed { intensity: 1.0 }
    }

    /// Dose per minute: positive in the open, negative under cover.
    ///
    /// A negative intensity is treated as none rather than as shelter —
    /// shelter is [`Exposure::Shielded`], and a caller that means "no
    /// radiation here" has to say so, because an exposure of zero holds a
    /// dose where it is rather than letting it come off.
    pub fn dose_rate(self) -> f64 {
        match self {
            Exposure::Shielded => -DOSE_DECAY,
            Exposure::Exposed { intensity } => RAD_RATE * intensity.max(0.0),
        }
    }
}

/// Whether the radiation meter is on screen.
///
/// It stays up the whole time a dose is coming off, not just while the body
/// is in danger: the interesting part of a dose is watching it go, and a
/// meter that vanished at the critical line would leave a player with no way
/// to tell a Bim that is nearly clear from one that has just come inside.
pub fn meter_visible(state: &HealthState) -> bool {
    state.dose > 0.0
}

/// The conditions in force. A body with nothing wrong with it has none.
///
/// This is the state's own reading of itself — a dose of exactly
/// [`CRITICAL`] counts as critical here, because that is what the dose says
/// now. [`update`] asks a slightly different question of a dose that is on
/// its way down; see [`RadiationStage::below`].
pub fn conditions(state: &HealthState) -> Conditions {
    conditions_of(state.radiation_stage(), state.cancer_stage())
}

/// How fast the body works and walks, all its conditions multiplied
/// together. 1.0 and 1.0 when there is nothing wrong.
pub fn effects(state: &HealthState) -> Effects {
    conditions(state).effects()
}

/// Health per minute, as things stand: positive while mending, negative
/// while anything is doing damage.
///
/// It is the *effective* rate, so a body already at full health reads 0.0
/// rather than the mending rate it is not using, and a dead one reads 0.0.
/// A caller drawing an arrow beside the bar wants the number that describes
/// the next minute.
pub fn net_health_rate(state: &HealthState) -> f64 {
    if state.dead {
        return 0.0;
    }
    rate_of(&conditions(state), state.points)
}

/// Move a body on by `dt_minutes` under `exposure`, and say what happened.
///
/// The returned events are every line crossed, in the order they were
/// crossed, once each. A step of any length gives the same state and the
/// same events as any sequence of shorter steps adding up to it.
pub fn update(state: &mut HealthState, dt_minutes: f64, exposure: Exposure) -> Vec<HealthEvent> {
    let mut events = Vec::new();

    // Death is final. Nothing moves, nothing is said, however long the step.
    if state.dead {
        return events;
    }
    // A body handed in at nothing without having been through here — the
    // play phase will have ways of taking health off all at once. It is dead
    // as of now, and it is said once.
    if state.points <= 0.0 {
        state.points = 0.0;
        state.dead = true;
        events.push(HealthEvent::Died);
        return events;
    }
    // Also catches a NaN, which would otherwise make every comparison below
    // false and the loop below run for ever.
    if !(dt_minutes > 0.0) {
        return events;
    }

    let dose_rate = exposure.dose_rate();
    let mut left = dt_minutes;

    while left > 0.0 {
        let was_radiation = state.radiation_stage();
        let was_cancer = state.cancer_stage();

        // The band in force over the piece of time about to be applied,
        // which is not the same question as what the dose reads as now: a
        // dose sitting exactly on a threshold and coming down is already on
        // its way out of that band.
        let band = if dose_rate < 0.0 {
            RadiationStage::below(state.dose)
        } else {
            was_radiation
        };
        let rate = rate_of(&conditions_of(band, was_cancer), state.points);

        // Every boundary this piece could run into, as (when, where). Each
        // is strictly in the future — an edge of no length would be a
        // segment of no length and the loop would not advance.
        let dose_edge = if dose_rate > 0.0 {
            next_threshold_above(state.dose).map(|to| ((to - state.dose) / dose_rate, to))
        } else if dose_rate < 0.0 && state.dose > band.floor() {
            let floor = band.floor();
            Some(((state.dose - floor) / -dose_rate, floor))
        } else {
            None
        };
        let cancer_edge = state.cancer.and_then(|c| {
            CancerStage::of(c.minutes_since_onset)
                .ends_at()
                .map(|to| (to - c.minutes_since_onset, to))
        });
        let health_edge = if rate > 0.0 {
            Some(((MAX_HEALTH - state.points) / rate, MAX_HEALTH))
        } else if rate < 0.0 {
            Some((state.points / -rate, 0.0))
        } else {
            None
        };

        let mut step = left;
        for edge in [dose_edge, cancer_edge, health_edge] {
            if let Some((when, _)) = edge {
                if when < step {
                    step = when;
                }
            }
        }
        // Belt and braces. Every edge above is strictly positive by
        // construction, so this cannot fire — but a hang would be far worse
        // to diagnose than a step quietly cut short.
        debug_assert!(step > 0.0, "an update segment of no length");
        if !(step > 0.0) {
            break;
        }

        // Landing *on* a boundary is written out exactly rather than
        // arrived at by multiplication, so that a threshold reached in one
        // long step and the same threshold reached in sixty short ones are
        // the same number rather than two that differ in the last bit.
        state.dose = match dose_edge {
            Some((when, to)) if when <= step => to,
            _ => (state.dose + dose_rate * step).max(0.0),
        };
        if let Some(c) = state.cancer.as_mut() {
            c.minutes_since_onset = match cancer_edge {
                Some((when, to)) if when <= step => to,
                _ => c.minutes_since_onset + step,
            };
        }
        state.points = match health_edge {
            Some((when, to)) if when <= step => to,
            // Not `f64::clamp`: it panics when its bounds cross, and that
            // panic path drags Rust's formatting machinery into the wasm.
            // Same reason as `crates/game/src/math.rs`.
            _ => (state.points + rate * step).max(0.0).min(MAX_HEALTH),
        };
        left -= step;

        // One rung at a time, in the direction it went. A step long enough
        // to take a body from a clean bill to radiation sickness says all
        // three things on the way rather than only where it ended up.
        radiation_events(was_radiation, state.radiation_stage(), &mut events);

        // Cancer begins the first time the dose reaches the threshold, and
        // that is the whole of it — there is no second way to get it and no
        // way to lose it.
        if state.cancer.is_none() && state.dose >= CANCER {
            state.cancer = Some(CancerState {
                minutes_since_onset: 0.0,
            });
            events.push(HealthEvent::CancerOnset);
        }
        if let (Some(was), Some(now)) = (was_cancer, state.cancer_stage()) {
            cancer_events(was, now, &mut events);
        }

        if state.points <= 0.0 {
            state.points = 0.0;
            state.dead = true;
            events.push(HealthEvent::Died);
            // The rest of the interval does not happen. Mending in what is
            // left of it must never bring a body back.
            return events;
        }
    }

    events
}

/// The conditions a radiation band and a cancer stage make between them.
/// `RadiationStage::None` is not a condition — a body with no dose has
/// nothing wrong with it on that count, and an empty list is what makes
/// [`Conditions::effects`] come out at 1.0.
fn conditions_of(radiation: RadiationStage, cancer: Option<CancerStage>) -> Conditions {
    let mut conditions = Conditions::default();
    if radiation != RadiationStage::None {
        conditions.push(Condition::Radiation(radiation));
    }
    if let Some(stage) = cancer {
        conditions.push(Condition::Cancer(stage));
    }
    conditions
}

/// Health per minute for a given set of conditions.
///
/// Mending is the body's own and runs whenever nothing is stopping it; the
/// rule "no mending while the dose is critical or cancer is present" is
/// [`Effect::suppresses_mending`](crate::condition::Effect::suppresses_mending)
/// on those conditions and is not written out anywhere else. A body already
/// at full health mends at nothing, which keeps the segment loop honest:
/// otherwise it would cut a segment at a boundary it is already standing on.
fn rate_of(conditions: &Conditions, points: f64) -> f64 {
    let mend = if conditions.suppress_mending() || points >= MAX_HEALTH {
        0.0
    } else {
        MEND
    };
    mend - conditions.damage()
}

/// The next dose threshold above `dose`, or `None` past the last one. The
/// list is ascending, which `data_is_sound` checks.
fn next_threshold_above(dose: f64) -> Option<f64> {
    [CRITICAL, SICKNESS, CANCER].into_iter().find(|&t| t > dose)
}

fn radiation_events(from: RadiationStage, to: RadiationStage, out: &mut Vec<HealthEvent>) {
    let mut at = from.rung();
    while at < to.rung() {
        at += 1;
        out.push(match at {
            1 => HealthEvent::RadiationDetected,
            2 => HealthEvent::CriticalDose,
            _ => HealthEvent::RadiationSickness,
        });
    }
    while at > to.rung() {
        out.push(match at {
            3 => HealthEvent::SicknessSubsided,
            2 => HealthEvent::BelowCritical,
            _ => HealthEvent::DoseCleared,
        });
        at -= 1;
    }
}

fn cancer_events(from: CancerStage, to: CancerStage, out: &mut Vec<HealthEvent>) {
    let mut at = from.rung();
    while at < to.rung() {
        at += 1;
        out.push(match at {
            1 => HealthEvent::CancerAdvanced,
            _ => HealthEvent::CancerTerminal,
        });
    }
}
