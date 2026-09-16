//! What a trip has to be true of.
//!
//! Two kinds of test here and they check different things. Most of them build
//! a [`Dynamics`] by hand — a mass, an acceleration, an alpha — because the
//! arithmetic is what is under test and a real ship's numbers would only make
//! the sums harder to read. The last few use
//! `shipdesign::fixture::flyer`, because the two placeholder constants in
//! [`data`] are chosen against *that ship* and a scenario, and a test with
//! hand-picked numbers in it could not tell you whether they were still any
//! good.

use shipdesign::fixture::{FLYER_FUEL, flyer};
use worldgen::math::{DVec2, dvec2};

use crate::angle;
use crate::data;
use crate::dynamics::{Dynamics, dynamics};
use crate::plan::{
    Effort, Phase, Plan, PlanError, Spin, Target, abort, effort_at, fuel_burned_at, plan_trip,
    state_at,
};

/// Square roots and trigonometry throughout, so nothing is compared with
/// `==`: that would be asserting how the optimiser ordered a multiplication.
fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-6 * a.abs().max(b.abs()).max(1.0)
}

/// A ship made of numbers. `a_backward` of zero is the ordinary case — one
/// engine, pointing forward — and is what makes a flip the only way to stop.
fn made_up(a_forward: f64, a_backward: f64, alpha: f64) -> Dynamics {
    Dynamics {
        mass: physics::Mass::new(1000.0).unwrap(),
        centre_of_mass: DVec2::ZERO,
        inertia: 1.0,
        a_forward,
        a_backward,
        alpha,
        forward_engines: 1,
        backward_engines: if a_backward > 0.0 { 1 } else { 0 },
        has_helm: true,
        has_airlock: true,
    }
}

/// A trip due north, which is the easy direction to read a position in: the
/// arrival point is straight up the `y` axis.
fn northward(dynamics: &Dynamics, distance: f64, heading: f64) -> Plan {
    plan_trip(
        dynamics,
        DVec2::ZERO,
        heading,
        Target::Point(dvec2(0.0, distance)),
        dvec2(0.0, distance),
        f64::MAX,
    )
    .expect("that trip should have planned")
}

// --- which way round everything is ---------------------------------------

#[test]
fn a_heading_of_nought_is_north_and_it_grows_clockwise() {
    let north = angle::facing(0.0);
    assert!(close(north.x, 0.0) && close(north.y, 1.0));
    let east = angle::facing(std::f64::consts::FRAC_PI_2);
    assert!(close(east.x, 1.0) && close(east.y, 0.0));
    let south = angle::facing(std::f64::consts::PI);
    assert!(close(south.x, 0.0) && close(south.y, -1.0));

    assert!(close(angle::bearing(dvec2(0.0, 5.0)), 0.0));
    assert!(close(
        angle::bearing(dvec2(5.0, 0.0)),
        std::f64::consts::FRAC_PI_2
    ));
}

/// The design grid's up is the ship's Forward, and the grid's `y` grows down.
/// Both halves of that are in one function, so both are checked at once.
#[test]
fn the_design_grid_is_turned_the_way_the_ship_is_pointing() {
    let up = dvec2(0.0, -1.0);
    let right = dvec2(1.0, 0.0);

    let at_rest = angle::rotate_design(up, 0.0);
    assert!(
        close(at_rest.x, 0.0) && close(at_rest.y, 1.0),
        "up is north"
    );
    let starboard = angle::rotate_design(right, 0.0);
    assert!(
        close(starboard.x, 1.0) && close(starboard.y, 0.0),
        "the grid's right is east",
    );

    let turned = angle::rotate_design(up, std::f64::consts::FRAC_PI_2);
    assert!(
        close(turned.x, 1.0) && close(turned.y, 0.0),
        "at a heading of a quarter turn the nose points east",
    );
}

/// It is its own inverse, which is what the pointer arithmetic leans on.
#[test]
fn turning_a_point_twice_puts_it_back() {
    for &heading in &[0.0, 0.7, std::f64::consts::FRAC_PI_2, 3.9, -2.1] {
        let there = angle::rotate_design(dvec2(37.0, -14.0), heading);
        let back = angle::unrotate_design(there, heading);
        assert!(close(back.x, 37.0) && close(back.y, -14.0), "{heading}");
    }
}

#[test]
fn the_shortest_way_round_is_the_short_way() {
    let turn = std::f64::consts::TAU;
    assert!(close(angle::shortest(0.1, turn - 0.1), -0.2));
    assert!(close(angle::shortest(turn - 0.1, 0.1), 0.2));
    assert!(close(angle::wrap(turn + 1.0), 1.0));
}

// --- what a design does when you push it ----------------------------------

/// A four-tile square of frame, so the centre of mass and the inertia can be
/// worked out by hand and compared. Every tile weighs the same, so the centre
/// is the middle of the square.
#[test]
fn a_symmetric_design_has_its_weight_in_the_middle() {
    use shipdesign::parts::TILE;
    use shipdesign::{Budget, Edit, PartKind, Rotation, ShipDesign, apply};

    let budget = Budget::new(1_000_000);
    let mut design = ShipDesign::new(8);
    for (x, y) in [(2, 2), (3, 2), (2, 3), (3, 3)] {
        design = apply(
            &design,
            &budget,
            Edit::Place {
                kind: PartKind::Structure,
                origin: (x, y),
                rotation: Rotation::R0,
            },
        )
        .unwrap();
    }

    let d = dynamics(&design, 0).expect("four tiles of frame is a ship");
    let middle = 3.0 * TILE as f64;
    assert!(close(d.centre_of_mass.x, middle), "{:?}", d.centre_of_mass);
    assert!(close(d.centre_of_mass.y, middle));

    // Every tile centre is half a tile from the middle on each axis, so the
    // square of the distance is half a tile squared, twice.
    let m = shipdesign::part_mass(PartKind::Structure);
    let arm2 = 2.0 * (TILE as f64 / 2.0).powi(2);
    assert!(
        close(d.inertia, 4.0 * m * arm2 + data::INERTIA_FLOOR),
        "{}",
        d.inertia,
    );
    assert!(close(d.alpha, 0.0), "no thrusters, no turning");
    assert!(close(d.a_forward, 0.0), "no engines, no pushing");
}

/// The mass is `shipdesign`'s and not a second opinion about it.
#[test]
fn a_ships_mass_is_the_one_shipdesign_says_it_is() {
    for crew in [1u32, 4] {
        let design = flyer(crew);
        let d = dynamics(&design, crew).unwrap();
        assert_eq!(d.mass, shipdesign::ship_mass(&design, crew).unwrap());
        assert!(d.a_forward > 0.0, "the flyer has a forward engine");
        assert_eq!(d.a_backward, 0.0, "and deliberately no backward one");
        assert!(d.alpha > 0.0, "and four thrusters");
        assert!(d.has_helm && d.has_airlock);
    }
}

#[test]
fn a_ship_with_no_thrusters_cannot_turn() {
    let mut stripped = flyer(1);
    stripped
        .parts
        .retain(|p| p.kind != shipdesign::PartKind::Thruster);
    let d = dynamics(&stripped, 1).unwrap();
    assert_eq!(d.alpha, 0.0);
    assert!(!d.can_rotate());
}

// --- the shape of a trip ---------------------------------------------------

/// The whole of the flip plan, checked end to end: it starts at rest, it
/// finishes at rest on the arrival point, nothing jumps at a phase boundary,
/// and the total is exactly what the four phases add up to.
#[test]
fn a_flip_trip_starts_and_ends_at_rest_on_the_arrival_point() {
    let d = made_up(0.05, 0.0, 1e-3);
    let plan = northward(&d, 400_000.0, 2.0);
    let total = plan.duration();

    let start = state_at(&plan, 0.0);
    assert!(close(start.speed, 0.0));
    assert_eq!(start.phase, Phase::Align);

    let end = state_at(&plan, total);
    assert!(close(end.speed, 0.0), "ended at {}", end.speed);
    assert!(
        end.position.distance(plan.arrival) < 1e-6,
        "{:?} against {:?}",
        end.position,
        plan.arrival,
    );
    assert_eq!(end.phase, Phase::Arrived);

    // Every phase happens, in order, and each is entered where the last one
    // left off. A jump in position, velocity or heading at a boundary is the
    // one failure a closed-form plan can have that a single end-to-end
    // assertion would not see.
    let mut at = 0.0;
    let mut seen = Vec::new();
    for segment in &plan.segments {
        seen.push(segment.phase);
        let before = state_at(&plan, at + segment.duration - 1e-7);
        let after = state_at(&plan, at + segment.duration + 1e-7);
        assert!(
            before.position.distance(after.position) < 1e-3,
            "{:?} jumped in position",
            segment.phase,
        );
        assert!(
            (before.speed - after.speed).abs() < 1e-3,
            "{:?} jumped in speed",
            segment.phase,
        );
        assert!(
            angle::shortest(before.heading, after.heading).abs() < 1e-3,
            "{:?} jumped in heading",
            segment.phase,
        );
        at += segment.duration;
    }
    assert_eq!(
        seen,
        vec![Phase::Align, Phase::Burn, Phase::Flip, Phase::Brake]
    );
    assert!(close(at, total));
}

/// With no turn to make and no flip to pay for, the trip is exactly the one
/// `physics` quotes — which is the formula the world generator laid every
/// system out against.
#[test]
fn without_a_flip_the_time_is_the_one_physics_quotes() {
    // Equal accelerations both ways, so no flip is wanted and the backward
    // engines brake straight away.
    let d = made_up(0.05, 0.05, 1e-3);
    let distance = 400_000.0;
    let plan = northward(&d, distance, 0.0);
    let want = time::minutes(physics::travel_days(distance, 0.05, 0.05).unwrap());
    assert!(
        close(plan.duration(), want),
        "{} against {want}",
        plan.duration()
    );
    assert_eq!(
        plan.segments.len(),
        2,
        "no align and no flip leaves a burn and a brake",
    );
}

/// The same identity from the other side: a ship whose thrusters are so quick
/// that the flip costs no time at all takes exactly as long as `physics`
/// quotes for equal accelerations both ways.
///
/// Worth having as well as the test above, because the two reach the answer
/// through different arithmetic — that one solves the two-legged burn, this
/// one solves the quadratic with the coast in it and watches the coast go to
/// nothing.
#[test]
fn a_flip_of_no_length_costs_no_time() {
    let a = 0.05;
    let distance = 400_000.0;
    let want = time::minutes(physics::travel_days(distance, a, a).unwrap());

    // No backward engine, so the flip is the only way to stop, and an alpha
    // large enough that half a turn is over before it began.
    let d = made_up(a, 0.0, 1e9);
    let plan = northward(&d, distance, 0.0);
    let flip = plan
        .segments
        .iter()
        .find(|s| s.phase == Phase::Flip)
        .expect("it had to turn round to stop");
    assert!(flip.duration < 1e-3, "the flip took {}", flip.duration);
    assert!(
        close(plan.duration(), want),
        "{} against {want}",
        plan.duration(),
    );
}

/// The align turns the short way round and finishes pointing at the target.
#[test]
fn an_align_turns_the_short_way_and_ends_on_the_bearing() {
    let d = made_up(0.05, 0.0, 1e-3);
    // Aimed due north from a heading just *west* of north: the short way is
    // clockwise, a small positive turn.
    let plan = northward(&d, 400_000.0, -0.6);
    let align = plan.segments[0];
    assert_eq!(align.phase, Phase::Align);
    assert!(align.spin.delta() > 0.0, "it should turn clockwise");
    assert!(align.spin.delta().abs() < std::f64::consts::PI);

    let after = state_at(&plan, align.duration);
    assert!(
        angle::shortest(after.heading, plan.bearing).abs() <= data::ALIGN_TOLERANCE,
        "ended on {} rather than {}",
        after.heading,
        plan.bearing,
    );
    assert!(close(after.speed, 0.0), "an align does not move the ship");

    // And a ship already pointing at the target does not turn at all.
    let straight = northward(&d, 400_000.0, 0.0);
    assert_eq!(straight.segments[0].phase, Phase::Burn);
}

/// Which brake is picked is a question about which is quicker, asked every
/// time rather than settled once.
#[test]
fn the_quicker_brake_is_the_one_that_is_used() {
    let distance = 400_000.0;

    // Strong backward engines and lazy thrusters: no reason to turn round.
    let backward = made_up(0.05, 0.05, 1e-5);
    let plan = northward(&backward, distance, 0.0);
    assert!(
        !plan.segments.iter().any(|s| s.phase == Phase::Flip),
        "a ship that can brake on its tail should not flip",
    );

    // Feeble backward engines and quick thrusters: turning round is worth it.
    let flipper = made_up(0.05, 0.0005, 1e-2);
    let plan = northward(&flipper, distance, 0.0);
    assert!(
        plan.segments.iter().any(|s| s.phase == Phase::Flip),
        "turning round was quicker and should have been chosen",
    );

    // And with nothing pushing backward, a flip is the only option there is.
    let only = made_up(0.05, 0.0, 1e-3);
    let plan = northward(&only, distance, 0.0);
    assert!(plan.segments.iter().any(|s| s.phase == Phase::Flip));
}

/// A thruster burns nothing, so the fuel curve is flat through every turn and
/// rises only under the engines.
#[test]
fn fuel_is_burnt_by_engines_and_by_nothing_else() {
    let d = made_up(0.05, 0.0, 1e-3);
    let plan = northward(&d, 400_000.0, 2.0);
    let total = plan.duration();

    assert!(close(fuel_burned_at(&plan, 0.0), 0.0));
    assert!(
        close(fuel_burned_at(&plan, total), plan.fuel_required),
        "{} against {}",
        fuel_burned_at(&plan, total),
        plan.fuel_required,
    );

    let mut at = 0.0;
    for segment in &plan.segments {
        let before = fuel_burned_at(&plan, at);
        let after = fuel_burned_at(&plan, at + segment.duration);
        if segment.engines == 0 {
            assert!(
                close(before, after),
                "{:?} burnt {} units with nothing lit",
                segment.phase,
                after - before,
            );
        } else {
            assert!(after > before, "{:?} burnt nothing", segment.phase);
        }
        at += segment.duration;
    }
}

/// Every refusal, each from the smallest case that produces it.
#[test]
fn a_trip_that_cannot_be_flown_says_which_way_it_cannot() {
    let there = dvec2(0.0, 400_000.0);
    let plan =
        |d: &Dynamics, fuel: f64| plan_trip(d, DVec2::ZERO, 0.0, Target::Point(there), there, fuel);

    let mut no_helm = made_up(0.05, 0.0, 1e-3);
    no_helm.has_helm = false;
    assert_eq!(plan(&no_helm, f64::MAX), Err(PlanError::NoHelm));

    assert_eq!(
        plan(&made_up(0.0, 0.05, 1e-3), f64::MAX),
        Err(PlanError::NoForwardEngine),
    );

    // Nothing to turn with and no backward engine: it could set off and would
    // never stop, so it is refused rather than flown.
    assert_eq!(
        plan(&made_up(0.05, 0.0, 0.0), f64::MAX),
        Err(PlanError::CannotRotate),
    );
    // And the same ship pointed somewhere else cannot even aim.
    let d = made_up(0.05, 0.05, 0.0);
    assert_eq!(
        plan_trip(&d, DVec2::ZERO, 1.0, Target::Point(there), there, f64::MAX),
        Err(PlanError::CannotRotate),
    );

    assert_eq!(
        plan(&made_up(0.05, 0.0, 1e-3), 0.0),
        Err(PlanError::NotEnoughFuel)
    );

    // Inside the radius round the target is already there.
    let d = made_up(0.05, 0.0, 1e-3);
    assert_eq!(
        plan_trip(
            &d,
            DVec2::ZERO,
            0.0,
            Target::Station(0),
            dvec2(0.0, data::ARRIVAL_RADIUS_STATION / 2.0),
            f64::MAX,
        ),
        Err(PlanError::AlreadyThere),
    );
    assert_eq!(
        plan_trip(
            &d,
            DVec2::ZERO,
            0.0,
            Target::Point(DVec2::ZERO),
            DVec2::ZERO,
            f64::MAX
        ),
        Err(PlanError::AlreadyThere),
    );
}

/// The arrival point is short of the target, by however much that kind of
/// target wants keeping clear of.
#[test]
fn a_trip_stops_short_of_what_it_is_aimed_at() {
    let d = made_up(0.05, 0.0, 1e-3);
    let there = dvec2(0.0, 400_000.0);
    for (target, radius) in [
        (Target::Station(3), data::ARRIVAL_RADIUS_STATION),
        (Target::Body(1), data::ARRIVAL_RADIUS_BODY),
        (Target::Point(there), 0.0),
    ] {
        let plan = plan_trip(&d, DVec2::ZERO, 0.0, target, there, f64::MAX).unwrap();
        assert!(close(plan.arrival.distance(there), radius), "{target:?}");
    }
}

/// Docking is a station **and** a way out of the ship. Without an airlock a
/// trip to a station ends alongside it.
#[test]
fn docking_wants_an_airlock_as_well_as_a_station() {
    let there = dvec2(0.0, 400_000.0);
    let with = made_up(0.05, 0.0, 1e-3);
    assert!(
        plan_trip(&with, DVec2::ZERO, 0.0, Target::Station(0), there, f64::MAX)
            .unwrap()
            .docks
    );
    let mut without = with;
    without.has_airlock = false;
    assert!(
        !plan_trip(
            &without,
            DVec2::ZERO,
            0.0,
            Target::Station(0),
            there,
            f64::MAX
        )
        .unwrap()
        .docks
    );
    // And a point is never a dock, however many airlocks there are.
    assert!(
        !plan_trip(
            &with,
            DVec2::ZERO,
            0.0,
            Target::Point(there),
            there,
            f64::MAX
        )
        .unwrap()
        .docks
    );
}

// --- giving up -------------------------------------------------------------

/// An abort during the burn and an abort mid-flip both end at rest, further
/// along the line than they started and never behind it. Reversing would be
/// the one thing "come to rest along the current line" must not do.
#[test]
fn an_abort_ends_at_rest_without_going_backwards() {
    let d = made_up(0.05, 0.0, 1e-3);
    let plan = northward(&d, 400_000.0, 0.0);
    let flip_starts: f64 = plan.segments[0].duration;

    for (when, what) in [
        (flip_starts / 2.0, "during the burn"),
        (flip_starts + plan.segments[1].duration / 2.0, "mid-flip"),
    ] {
        let was = state_at(&plan, when);
        let stop = abort(&plan, when);
        assert!(stop.aborting);
        let end = state_at(&stop, stop.duration());
        assert!(close(end.speed, 0.0), "{what}: ended at {}", end.speed);

        // Still on the line, and further along it.
        let moved = end.position.sub(was.position);
        let along = moved.x * plan.direction.x + moved.y * plan.direction.y;
        assert!(along >= -1e-6, "{what}: went backwards by {along}");
        assert!(
            close(moved.length(), along.abs()),
            "{what}: came off the line",
        );
        assert!(
            end.position.distance(stop.arrival) < 1e-6,
            "{what}: the plan and the walk disagree about where it stopped",
        );
    }
}

/// Aborting while still turning to face the target is the one case with
/// nothing to brake. It takes the spin out and holds.
#[test]
fn aborting_an_align_stops_the_turn_and_nothing_else() {
    let d = made_up(0.05, 0.0, 1e-3);
    let plan = northward(&d, 400_000.0, 2.5);
    let when = plan.segments[0].duration / 3.0;
    let was = state_at(&plan, when);
    let stop = abort(&plan, when);

    assert!(stop.duration() > 0.0, "there was a spin to take out");
    let end = state_at(&stop, stop.duration());
    assert!(close(end.speed, 0.0));
    assert!(
        end.position.distance(was.position) < 1e-6,
        "an align does not move the ship, and neither does stopping one",
    );
    assert!(
        close(end.heading, stop.final_heading()),
        "the plan and the walk disagree about where it finished pointing",
    );
    assert!(close(stop.fuel_required, fuel_burned_at(&plan, when)));
}

/// What the abort inherits. The fuel already burnt by the trip it replaced is
/// carried, or the hold would only ever be charged for the last leg.
#[test]
fn an_abort_carries_the_fuel_the_trip_had_already_burnt() {
    let d = made_up(0.05, 0.0, 1e-3);
    let plan = northward(&d, 400_000.0, 0.0);
    let when = plan.segments[0].duration * 0.8;
    let burnt = fuel_burned_at(&plan, when);
    assert!(burnt > 0.0);

    let stop = abort(&plan, when);
    assert!(close(fuel_burned_at(&stop, 0.0), burnt));
    assert!(
        stop.fuel_required > burnt,
        "the braking costs something too"
    );
    assert!(close(
        fuel_burned_at(&stop, stop.duration()),
        stop.fuel_required
    ));
}

// --- the two placeholder constants, against the ship they were chosen for --

/// Four thrusters, one real hull, half a circle, and the two game hours the
/// design step asked for. This is what `PartDef::torque_thrust` is set by.
#[test]
fn four_thrusters_flip_the_reference_inside_two_hours() {
    let d = dynamics(&flyer(4), 4).unwrap();
    let flip = Spin::swing(std::f64::consts::PI, d.alpha);
    assert!(
        flip.duration() <= 120.0,
        "a flip took {} minutes, which is too long to be worth doing",
        flip.duration(),
    );
    assert!(
        flip.duration() > 1.0,
        "a flip took {} minutes, which is not a manoeuvre at all",
        flip.duration(),
    );
}

/// One full tank, and the longest hop the world generator will ever put
/// between two things in a system. This is what
/// [`data::FUEL_PER_ENGINE_MINUTE`] is set by: a ship that cannot cross its
/// own system is a ship with nowhere to go.
#[test]
fn one_full_tank_crosses_the_longest_reference_hop() {
    let design = flyer(4);
    let d = dynamics(&design, 4).unwrap();
    let hop = worldgen::data::reference_distance(worldgen::data::TRAVEL_BAND.max_days)
        .expect("the reference ship has a longest hop");

    let there = dvec2(0.0, hop);
    let plan = plan_trip(
        &d,
        DVec2::ZERO,
        0.0,
        Target::Station(0),
        there,
        FLYER_FUEL as f64,
    )
    .expect("the flyer should be able to cross its own system");

    assert!(
        plan.fuel_required <= FLYER_FUEL as f64,
        "the longest hop wanted {} units and the tank holds {FLYER_FUEL}",
        plan.fuel_required,
    );
    assert!(
        plan.fuel_required >= FLYER_FUEL as f64 * 0.2,
        "the longest hop only wanted {} units of {FLYER_FUEL}: fuel is not a \
         decision at that price",
        plan.fuel_required,
    );

    // And it is not an absurd amount of time, either: the reference ship takes
    // fourteen days over this and the flyer is a great deal heavier, but a
    // trip nobody would sit through is a trip nobody takes.
    let days = time::days(plan.duration());
    assert!(days < 400.0, "the longest hop took {days} days");
}

/// One more time with the real fixture, because everything above that used a
/// real ship used it standing still. A trip planned from a heading the ship
/// was left on, to somewhere off to the side, with a fixed amount of fuel, is
/// the whole of what the world will ever ask for.
#[test]
fn the_flyer_can_actually_be_flown_somewhere() {
    let d = dynamics(&flyer(2), 2).unwrap();
    let there = dvec2(3_000_000.0, -1_500_000.0);
    let plan = plan_trip(
        &d,
        dvec2(120.0, -40.0),
        2.9,
        Target::Body(2),
        there,
        FLYER_FUEL as f64,
    )
    .expect("a trip across a system");

    let end = state_at(&plan, plan.duration());
    assert!(close(end.speed, 0.0));
    assert!(end.position.distance(plan.arrival) < 1e-3);
    assert!(close(
        end.position.distance(there),
        data::ARRIVAL_RADIUS_BODY
    ));
    assert!(!plan.docks, "a body is not somewhere to dock");
}

/// Not an assertion — a readout, for whoever next has to move one of the
/// placeholder numbers. Run with `--nocapture`.
#[test]
fn what_the_fixture_actually_flies_like() {
    let d = dynamics(&flyer(4), 4).unwrap();
    let hop = worldgen::data::reference_distance(worldgen::data::TRAVEL_BAND.max_days).unwrap();
    let there = dvec2(0.0, hop);
    let plan = plan_trip(&d, DVec2::ZERO, 0.0, Target::Station(0), there, f64::MAX).unwrap();
    println!(
        "mass {:.0}  a_forward {:.5}  inertia {:.3e}  alpha {:.3e}\n\
         flip {:.1} min  longest hop {:.1} days, {:.1} units of fuel of {FLYER_FUEL}",
        d.mass.get(),
        d.a_forward,
        d.inertia,
        d.alpha,
        Spin::swing(std::f64::consts::PI, d.alpha).duration(),
        time::days(plan.duration()),
        plan.fuel_required,
    );
}

// --- what the ship is doing to itself --------------------------------------

/// The effort is the plan read a second way, and it has to agree with the
/// first: the engines are lit exactly while the speed is changing, the
/// thrusters push exactly while the rate is, and the sign of the push is the
/// sign of the change. A picture of the exhaust is built on this and nothing
/// else would notice it drifting.
#[test]
fn the_effort_is_the_derivative_of_the_state() {
    let d = dynamics(&flyer(4), 4).unwrap();
    // Off at a right angle, so there is a real turn to align and a flip
    // to brake with — the flyer has no backward engine.
    let plan = plan_trip(
        &d,
        DVec2::ZERO,
        0.0,
        Target::Point(dvec2(400_000.0, 0.0)),
        dvec2(400_000.0, 0.0),
        FLYER_FUEL as f64,
    )
    .expect("the flyer should fly there");

    let mut seen_engines = false;
    let mut seen_thrusters = false;
    let mut begun = 0.0;
    for segment in &plan.segments {
        // Well inside the segment, and either side of a swing's halfway
        // point rather than on it: the derivative is read across a short
        // gap, and a gap over an edge reads as nothing in particular.
        let gap = (segment.duration * 0.02).min(0.05);
        for share in [0.1, 0.3, 0.45, 0.55, 0.7, 0.9] {
            let t = begun + segment.duration * share;
            let effort = effort_at(&plan, t);
            let before = state_at(&plan, t - gap);
            let after = state_at(&plan, t + gap);
            let speed_change = after.speed - before.speed;
            assert_eq!(
                effort.engines > 0,
                speed_change.abs() > 1e-9,
                "at {t}: engines {} while the speed moved by {speed_change}",
                effort.engines,
            );
            if effort.engines > 0 {
                assert!(
                    effort.accel * speed_change > 0.0,
                    "at {t}: pushing the wrong way"
                );
                seen_engines = true;
            } else {
                assert_eq!(effort.accel, 0.0);
            }
            let rate_change = plan_rate(&plan, t + gap) - plan_rate(&plan, t - gap);
            assert_eq!(
                effort.alpha != 0.0,
                rate_change.abs() > 1e-12,
                "at {t}: alpha {} while the rate moved by {rate_change}",
                effort.alpha,
            );
            if effort.alpha != 0.0 {
                assert!(
                    effort.alpha * rate_change > 0.0,
                    "at {t}: turning the wrong way"
                );
                seen_thrusters = true;
            }
        }
        begun += segment.duration;
    }
    assert!(
        seen_engines && seen_thrusters,
        "the trip should have both burnt and turned"
    );
    assert_eq!(effort_at(&plan, begun + 1.0), Effort::NONE);
    assert_eq!(effort_at(&plan, -1.0), Effort::NONE);
}

/// The angular rate `minutes` into a plan, read off the segments the way
/// `state_at` reads the heading.
fn plan_rate(plan: &Plan, minutes: f64) -> f64 {
    let mut left = minutes;
    for segment in &plan.segments {
        if left < segment.duration {
            return segment.spin.rate_at(left);
        }
        left -= segment.duration;
    }
    0.0
}
