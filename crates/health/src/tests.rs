//! Native tests for the health model.
//!
//! These run under `cargo test --target <native> -p health`, which is what
//! `nix flake check` does. The crate compiles for wasm as well and has to
//! give the same answers there; nothing in here is a `usize` or a hash, so
//! the only wasm half of the check is that it builds, which the flake's
//! `--workspace` wasm build covers.
//!
//! The three scenarios are the requirement rather than the numbers in
//! `data.rs`. If a value there moves, what should be argued about is whether
//! these three still describe the game that is wanted:
//!
//! - **A** — a shift outside. Costs health, all of it comes back.
//! - **B** — a day outside. Survivable, and the body carries a cancer for
//!   the rest of its life.
//! - **C** — that cancer, left alone. Fatal in under six weeks.

use crate::condition::{CancerStage, Condition, RadiationStage};
use crate::data::{
    CANCER, CANCER_ADVANCED_DAMAGE, CANCER_EARLY_DAMAGE, CANCER_TERMINAL_AT, CANCER_TERMINAL_DAYS,
    CANCER_TERMINAL_MOVE, CANCER_TERMINAL_WORK, CRITICAL, CRITICAL_DAMAGE, DOSE_DECAY, MAX_HEALTH,
    MEND, RAD_RATE, SICKNESS, SICKNESS_DAMAGE, SICKNESS_MOVE, SICKNESS_WORK, data_is_sound,
};
use crate::event::HealthEvent::{
    self, BelowCritical, CancerAdvanced, CancerOnset, CancerTerminal, CriticalDose, Died,
    DoseCleared, RadiationDetected, RadiationSickness, SicknessSubsided,
};
use crate::state::{
    CancerState, Exposure, HealthState, conditions, effects, meter_visible, net_health_rate, update,
};
use time::{DAY, HOUR};

/// Everything here is arithmetic on rates in the hundredths, so a tolerance
/// this tight is still thousands of times larger than the drift a few
/// thousand steps of `f64` accumulate.
const EPSILON: f64 = 1e-6;

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= EPSILON
}

/// One stretch of time under one exposure. A scenario is a list of them.
type Leg = (f64, Exposure);

/// Run `minutes` under one exposure in steps of `dt`, collecting the events.
/// `dt` of [`f64::INFINITY`] is the whole thing in one call.
fn run(state: &mut HealthState, minutes: f64, exposure: Exposure, dt: f64) -> Vec<HealthEvent> {
    let mut events = Vec::new();
    let mut left = minutes;
    while left > 0.0 {
        let step = dt.min(left);
        events.extend(update(state, step, exposure));
        left -= step;
    }
    events
}

fn play(start: HealthState, legs: &[Leg], dt: f64) -> (HealthState, Vec<HealthEvent>) {
    let mut state = start;
    let mut events = Vec::new();
    for &(minutes, exposure) in legs {
        events.extend(run(&mut state, minutes, exposure, dt));
    }
    (state, events)
}

fn open() -> Exposure {
    Exposure::normal()
}

fn under_cover() -> Exposure {
    Exposure::Shielded
}

/// Minutes under cover to shed `dose`. The scenarios run until the dose is
/// gone rather than for a round number of hours, because "until it is clear"
/// is the thing being asserted.
fn to_shed(dose: f64) -> f64 {
    dose / DOSE_DECAY
}

/// A shift outside: eight hours in the open, then inside until clear.
fn scenario_a() -> Vec<Leg> {
    let dose = 8.0 * HOUR * RAD_RATE;
    vec![(8.0 * HOUR, open()), (to_shed(dose), under_cover())]
}

/// A day outside, then inside until clear.
fn scenario_b() -> Vec<Leg> {
    let dose = DAY * RAD_RATE;
    vec![(DAY, open()), (to_shed(dose), under_cover())]
}

/// Cancer from minute zero, under cover the whole time, for forty days.
fn scenario_c() -> Vec<Leg> {
    vec![(40.0 * DAY, under_cover())]
}

#[test]
fn the_numbers_are_ordered_the_way_the_model_reads_them() {
    assert!(data_is_sound());
}

#[test]
fn the_meter_comes_up_on_the_first_minute_and_goes_when_the_dose_does() {
    let mut state = HealthState::new();
    assert!(
        !meter_visible(&state),
        "nothing to show before anything happened"
    );

    update(&mut state, 1.0, open());
    assert!(close(state.dose, RAD_RATE));
    assert!(meter_visible(&state));

    // Half off, and still up: the meter is how a player watches it go.
    update(&mut state, 1.0, under_cover());
    assert!(close(state.dose, RAD_RATE - DOSE_DECAY));
    assert!(meter_visible(&state));

    // Exactly nothing, and away it goes.
    update(&mut state, 1.0, under_cover());
    assert_eq!(state.dose, 0.0, "the decay stops at nothing, not near it");
    assert!(!meter_visible(&state));
}

#[test]
fn a_dose_never_goes_negative_however_long_the_body_is_under_cover() {
    let mut state = HealthState::new();
    update(&mut state, 30.0, open());
    update(&mut state, 100.0 * DAY, under_cover());
    assert_eq!(state.dose, 0.0);

    // And the same in small steps, which is the other way round to get it
    // wrong: a decay applied per step with no floor under it.
    let mut state = HealthState::new();
    update(&mut state, 30.0, open());
    run(&mut state, DAY, under_cover(), 1.0);
    assert_eq!(state.dose, 0.0);
}

#[test]
fn scenario_a_a_shift_outside_costs_health_and_all_of_it_comes_back() {
    let (state, events) = play(HealthState::new(), &scenario_a(), 1.0);

    assert!(!state.dead, "eight hours outside must not be fatal");
    assert!(
        state.cancer.is_none(),
        "eight hours is well short of a cancer"
    );
    assert_eq!(state.dose, 0.0);
    assert!(
        close(state.points, MAX_HEALTH),
        "health should be back to full, not {}",
        state.points
    );
    assert!(!events.contains(&Died));
    assert!(!events.contains(&CancerOnset));
    assert_eq!(*events.last().unwrap(), DoseCleared);
}

#[test]
fn scenario_a_does_damage_on_the_way_through() {
    // Full health at the end is only worth asserting if the body was ever
    // hurt. Read the low-water mark rather than the last sample — the same
    // trap as the room's long-run probes.
    let mut state = HealthState::new();
    let mut worst = MAX_HEALTH;
    for &(minutes, exposure) in &scenario_a() {
        let mut left = minutes;
        while left > 0.0 {
            let step = 1.0_f64.min(left);
            update(&mut state, step, exposure);
            worst = worst.min(state.points);
            left -= step;
        }
    }
    assert!(worst < MAX_HEALTH - 1.0, "barely a scratch: {worst}");
}

#[test]
fn scenario_b_a_day_outside_is_survivable_and_leaves_a_cancer() {
    let (state, events) = play(HealthState::new(), &scenario_b(), 1.0);

    assert!(!state.dead, "a day outside must be survivable");
    assert_eq!(state.dose, 0.0);
    let cancer = state.cancer.expect("a day outside reaches the cancer dose");
    assert!(
        close(cancer.minutes_since_onset, to_shed(DAY * RAD_RATE)),
        "the cancer began when the dose peaked, not when it cleared"
    );
    assert!(state.points > 0.0);
    assert!(
        state.points < MAX_HEALTH,
        "nothing mends with a cancer in it"
    );
    assert!(events.contains(&CancerOnset));
    assert!(events.contains(&RadiationSickness));
    assert!(events.contains(&SicknessSubsided));
    assert!(!events.contains(&Died));
}

#[test]
fn scenario_c_a_cancer_left_alone_kills_in_under_six_weeks() {
    let mut state = HealthState::with_cancer();

    run(&mut state, CANCER_TERMINAL_DAYS * DAY, under_cover(), 1.0);
    assert!(
        !state.dead,
        "a body should still be alive when the cancer turns terminal"
    );
    assert_eq!(state.cancer_stage(), Some(CancerStage::Terminal));

    let (state, events) = play(HealthState::with_cancer(), &scenario_c(), 1.0);
    assert!(
        state.dead,
        "forty days of cancer with no treatment is fatal"
    );
    assert_eq!(state.points, 0.0);
    assert_eq!(
        events,
        vec![CancerAdvanced, CancerTerminal, Died],
        "nothing else happens to a body under cover"
    );
}

#[test]
fn nothing_mends_while_the_dose_is_critical_or_a_cancer_is_present() {
    let hurt = |extra: fn(&mut HealthState)| {
        let mut state = HealthState::new();
        state.points = MAX_HEALTH / 2.0;
        extra(&mut state);
        state
    };

    // A dose below critical is on the meter and does nothing else.
    let mut elevated = hurt(|s| s.dose = CRITICAL / 2.0);
    assert!(close(net_health_rate(&elevated), MEND));
    update(&mut elevated, 10.0, under_cover());
    assert!(elevated.points > MAX_HEALTH / 2.0);

    // At critical it is damage, and no mending underneath it.
    let critical = hurt(|s| s.dose = CRITICAL);
    assert!(close(net_health_rate(&critical), -CRITICAL_DAMAGE));

    let ill = hurt(|s| s.dose = SICKNESS);
    assert!(close(net_health_rate(&ill), -SICKNESS_DAMAGE));

    // A cancer suppresses it with no dose at all, which is most of what
    // makes a cancer worth avoiding.
    let mut ill = hurt(|s| {
        s.cancer = Some(CancerState {
            minutes_since_onset: 0.0,
        })
    });
    assert!(close(net_health_rate(&ill), -CANCER_EARLY_DAMAGE));
    let before = ill.points;
    update(&mut ill, DAY, under_cover());
    assert!(ill.points < before, "a cancer never mends on its own");

    // Both at once add up rather than one winning.
    let both = hurt(|s| {
        s.dose = SICKNESS;
        s.cancer = Some(CancerState {
            minutes_since_onset: CANCER_TERMINAL_AT / 2.0,
        });
    });
    assert!(close(
        net_health_rate(&both),
        -(SICKNESS_DAMAGE + CANCER_ADVANCED_DAMAGE)
    ));

    // And a body with nothing wrong and nothing to mend is reported as
    // going nowhere rather than as mending into a ceiling.
    assert_eq!(net_health_rate(&HealthState::new()), 0.0);
}

#[test]
fn death_is_final_and_mending_in_the_same_update_does_not_undo_it() {
    // Enough dose to be doing damage, and little enough health that the body
    // dies well before the dose falls below critical — leaving most of the
    // interval as time it would have spent mending.
    let mut state = HealthState::new();
    state.points = 0.5;
    state.dose = 200.0;

    let events = update(&mut state, 1000.0, under_cover());
    assert!(state.dead, "the damage got there first");
    assert_eq!(state.points, 0.0, "mending must not lift a dead body");
    assert_eq!(
        events.iter().filter(|&&e| e == Died).count(),
        1,
        "said once"
    );
    assert_eq!(*events.last().unwrap(), Died, "and said last");

    // Nothing moves afterwards, whatever is done to it.
    let dead = state;
    let after = update(&mut state, DAY, open());
    assert!(after.is_empty(), "a dead body has nothing to report");
    assert_eq!(state, dead, "and nothing about it changes");
}

#[test]
fn a_crossing_is_reported_once_each_way_however_many_times_it_happens() {
    // Out and back in twice: 200 minutes outside takes the dose past
    // critical, and 400 under cover takes it back to nothing.
    let out_and_back = [(200.0, open()), (to_shed(200.0), under_cover())];
    let legs: Vec<Leg> = out_and_back
        .iter()
        .chain(out_and_back.iter())
        .copied()
        .collect();
    let (state, events) = play(HealthState::new(), &legs, 1.0);

    assert_eq!(state.dose, 0.0);
    assert_eq!(
        events,
        vec![
            RadiationDetected,
            CriticalDose,
            BelowCritical,
            DoseCleared,
            RadiationDetected,
            CriticalDose,
            BelowCritical,
            DoseCleared,
        ]
    );
}

#[test]
fn one_long_step_says_everything_it_passed_through() {
    // A day in the open in a single call. The events are the ladder, in
    // order — not just the band it ended up in.
    let mut state = HealthState::new();
    let events = update(&mut state, DAY, open());
    assert_eq!(
        events,
        vec![
            RadiationDetected,
            CriticalDose,
            RadiationSickness,
            CancerOnset
        ]
    );
    assert!(close(state.dose, CANCER));
}

#[test]
fn a_step_of_any_length_gives_the_same_answer() {
    for (name, start, legs) in [
        ("A", HealthState::new(), scenario_a()),
        ("B", HealthState::new(), scenario_b()),
        ("C", HealthState::with_cancer(), scenario_c()),
    ] {
        let (minutely, minutely_events) = play(start, &legs, 1.0);
        let (hourly, hourly_events) = play(start, &legs, HOUR);
        let (whole, whole_events) = play(start, &legs, f64::INFINITY);

        for (other, other_events, dt) in [
            (hourly, hourly_events, "an hour"),
            (whole, whole_events, "the whole leg"),
        ] {
            assert_eq!(
                minutely.dead, other.dead,
                "scenario {name} disagrees about death at {dt} a step"
            );
            assert!(
                close(minutely.points, other.points),
                "scenario {name} at {dt} a step: {} health, not {}",
                other.points,
                minutely.points
            );
            assert!(
                close(minutely.dose, other.dose),
                "scenario {name} at {dt} a step: {} dose, not {}",
                other.dose,
                minutely.dose
            );
            match (minutely.cancer, other.cancer) {
                (None, None) => {}
                (Some(a), Some(b)) => assert!(
                    close(a.minutes_since_onset, b.minutes_since_onset),
                    "scenario {name} at {dt} a step: cancer of a different age"
                ),
                _ => panic!("scenario {name} at {dt} a step disagrees about a cancer"),
            }
            assert_eq!(
                minutely_events, other_events,
                "scenario {name} at {dt} a step: a different story"
            );
        }
    }
}

#[test]
fn effects_are_one_when_nothing_is_wrong_and_multiply_when_things_are() {
    let healthy = effects(&HealthState::new());
    assert_eq!(healthy.work_speed, 1.0);
    assert_eq!(healthy.move_speed, 1.0);
    assert!(conditions(&HealthState::new()).is_empty());

    // A dose that is only on the meter slows nothing down.
    let mut elevated = HealthState::new();
    elevated.dose = CRITICAL / 2.0;
    assert_eq!(effects(&elevated).work_speed, 1.0);
    assert_eq!(effects(&elevated).move_speed, 1.0);
    assert_eq!(conditions(&elevated).len(), 1);

    let mut both = HealthState::new();
    both.dose = SICKNESS;
    both.cancer = Some(CancerState {
        minutes_since_onset: CANCER_TERMINAL_AT,
    });
    let effects = effects(&both);
    assert!(close(
        effects.work_speed,
        SICKNESS_WORK * CANCER_TERMINAL_WORK
    ));
    assert!(close(
        effects.move_speed,
        SICKNESS_MOVE * CANCER_TERMINAL_MOVE
    ));

    let listed: Vec<Condition> = conditions(&both).iter().collect();
    assert_eq!(
        listed,
        vec![
            Condition::Radiation(RadiationStage::RadiationSickness),
            Condition::Cancer(CancerStage::Terminal),
        ]
    );
}

#[test]
fn a_band_is_closed_at_the_bottom_and_reverses_at_the_same_line() {
    assert_eq!(RadiationStage::of(0.0), RadiationStage::None);
    assert_eq!(
        RadiationStage::of(f64::MIN_POSITIVE),
        RadiationStage::Elevated
    );
    assert_eq!(RadiationStage::of(CRITICAL), RadiationStage::Critical);
    assert_eq!(
        RadiationStage::of(SICKNESS),
        RadiationStage::RadiationSickness
    );

    // Coming down: exactly on the line is still in the band, and the event
    // only fires once it is past.
    let mut state = HealthState::new();
    state.dose = CRITICAL + DOSE_DECAY;
    let events = update(&mut state, 1.0, under_cover());
    assert_eq!(state.dose, CRITICAL);
    assert!(events.is_empty(), "still critical, so nothing to say yet");

    let events = update(&mut state, 1.0, under_cover());
    assert_eq!(events, vec![BelowCritical]);
}

#[test]
fn cancer_begins_the_first_time_the_dose_reaches_it_and_never_leaves() {
    let mut state = HealthState::new();
    state.dose = CANCER - 1.0;
    let events = update(&mut state, 1.0, open());
    assert_eq!(events, vec![CancerOnset]);
    assert_eq!(state.cancer_stage(), Some(CancerStage::Early));

    // Back to nothing, and it is still there — there is no cure to call.
    run(&mut state, to_shed(CANCER), under_cover(), HOUR);
    assert_eq!(state.dose, 0.0);
    assert!(state.cancer.is_some());

    // And going out again does not start a second one.
    let events = run(&mut state, DAY, open(), HOUR);
    assert!(!events.contains(&CancerOnset));
}
