//! What the world has to be true of.
//!
//! Most of these fly the real fixture in the real spawn system, because
//! almost everything here is about how the parts fit together rather than
//! about arithmetic — the arithmetic has its own tests next door in `flight`.
//! Where a scenario would otherwise depend on where the generator happened to
//! put a planet, the probe seams on [`World`] are used instead: see
//! `discover_for_probe` and `put_for_probe`.

use economy::{Money, Storage, trade_price};
use physics::ResourceId;
use shipdesign::fixture::{FLYER_FUEL, flyer};
use shipdesign::parts::PartKind;
use shipdesign::{Budget, Edit, Rotation, ShipDesign, apply, build_from_cargo};
use worldgen::GalaxyType;
use worldgen::math::{DVec2, dvec2};

use crate::data;
use crate::event::{Refusal, WorldEvent};
use crate::fixture::{REFERENCE_MONEY, reference_target, reference_world, simulation_world};
use crate::frame::Frame;
use crate::speed::Speed;
use crate::world::{Command, ShipState, World};
use crate::{PlanError, Target};

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-6 * a.abs().max(b.abs()).max(1.0)
}

/// A world with a named amount of money, for the trading tests.
fn world_with(design: ShipDesign, money: Money, players: u32) -> World {
    simulation_world(design, money, players)
}

fn basic() -> World {
    world_with(flyer(2), REFERENCE_MONEY, 2)
}

/// Somewhere near enough for a trip to finish inside a test.
///
/// A real node in the spawn system is days away, and a test that stepped
/// through one would be stepping five million times. A point target is the
/// same code path with a shorter line.
fn nearby(world: &World, distance: f64) -> Target {
    Target::Point(world.ship.position().add(dvec2(distance, distance / 3.0)))
}

/// Run until the ship is no longer travelling, or give up.
fn until_stopped(world: &mut World, limit: u32) -> Vec<WorldEvent> {
    let mut seen = Vec::new();
    for _ in 0..limit {
        seen.extend(world.step(&[]));
        if !matches!(world.ship.state, ShipState::Travelling { .. }) {
            return seen;
        }
    }
    panic!("the trip never ended");
}

// --- starting ---------------------------------------------------------------

#[test]
fn a_world_opens_docked_at_the_spawn_station_with_the_money_left_over() {
    let world = basic();
    let ShipState::Docked { station } = world.ship.state else {
        panic!("it should start docked, not {:?}", world.ship.state);
    };
    assert_eq!(world.money, REFERENCE_MONEY);
    assert_eq!(world.clock_minutes, 0.0);
    assert_eq!(world.steps, 0);
    assert_eq!(world.ship.heading, 0.0);
    assert_eq!(world.ship.reserved_fuel, 0);

    // The centre of mass is on the station, which is what docked means.
    let at = world
        .system
        .absolute_position(worldgen::Node::Station(station))
        .unwrap();
    assert!(world.ship.position().distance(at) < 1e-9);

    // And the dock the ship is tied to is something the crew have seen —
    // as is everything else in the system, off the lobby's chart.
    assert!(world.discovered.contains(&worldgen::Node::Station(station)));
    assert_eq!(world.discovered.len(), world.system.nodes().len());
    assert_eq!(
        world.ship.frame,
        Frame::Local(worldgen::Node::Station(station))
    );
}

/// The spawn is the **lowest** star with a station and the lowest station in
/// it, so two clients that generated the same galaxy start in the same place.
#[test]
fn the_spawn_is_the_first_dock_in_the_galaxy() {
    let galaxy = worldgen::Galaxy::new(data::DEFAULT_SEED, GalaxyType::SpiralTwoArm);
    let (star, station) = crate::spawn(&galaxy).expect("a galaxy has a station somewhere");
    for earlier in 0..star {
        let system = galaxy.system(earlier).unwrap();
        assert!(system.stations.is_empty(), "star {earlier} had a station");
    }
    let system = galaxy.system(star).unwrap();
    assert_eq!(system.stations.iter().map(|s| s.id).min(), Some(station));

    let world = basic();
    assert_eq!(world.star_id, star);
}

// --- one clock ---------------------------------------------------------------

/// A step is a step whoever calls it. Grouping them the way a frame does
/// changes nothing at all, which is the whole promise the fixed step is for.
#[test]
fn how_the_steps_are_grouped_into_frames_makes_no_difference() {
    let target = |w: &World| nearby(w, 20_000.0);
    let confirm_at = 37u32;
    let total = 400u32;

    let run = |group: u32| {
        let mut world = basic();
        let target = target(&world);
        let mut taken = 0;
        while taken < total {
            let batch = group.min(total - taken);
            for _ in 0..batch {
                let commands: Vec<Command> = if taken == confirm_at {
                    vec![Command::Confirm { slot: 0, target }]
                } else {
                    Vec::new()
                };
                world.step(&commands);
                taken += 1;
            }
        }
        world.checksum()
    };

    let one_at_a_time = run(1);
    for group in [2u32, 7, 24, 60, 400] {
        assert_eq!(
            run(group),
            one_at_a_time,
            "running {group} steps to a frame gave a different world",
        );
    }
}

#[test]
fn a_step_is_always_the_same_length() {
    let mut world = basic();
    for i in 1..=10u32 {
        world.step(&[]);
        assert!(close(world.clock_minutes, data::STEP_MINUTES * i as f64));
        assert_eq!(world.steps, i as u64);
    }
    // And the day rolls over where it should.
    assert_eq!(world.day(), 0);
}

/// Stepping the ship through a whole trip puts it where the closed-form plan
/// said it would be, at the time the plan said. If those two ever disagree,
/// every promise the helm panel makes is a lie.
#[test]
fn stepping_a_trip_lands_where_the_plan_said_it_would() {
    let mut world = basic();
    let target = nearby(&world, 4_000.0);
    world.step(&[Command::Confirm { slot: 0, target }]);

    // The departure is the clock reading the **command** landed at, which is
    // one step before the clock the step it arrived in finished on: commands
    // are applied before the clock advances, on purpose.
    let ShipState::Travelling { plan, departed } = &world.ship.state else {
        panic!("the trip should have been planned");
    };
    let (arrival, duration, departed) = (plan.arrival, plan.duration(), *departed);

    until_stopped(&mut world, 200_000);
    assert!(
        world.ship.position().distance(arrival) < 1.0,
        "ended {} units from the arrival point",
        world.ship.position().distance(arrival),
    );
    // Within one step of the quoted time: the loop cannot stop mid-step, so
    // the arrival lands on the first step at or past it.
    let took = world.clock_minutes - departed;
    assert!(
        took >= duration && took < duration + data::STEP_MINUTES * 2.0,
        "took {took} minutes against a quote of {duration}",
    );
    assert_eq!(world.ship.state, ShipState::Holding);
}

// --- what a ship change does, and does not do -------------------------------

/// The conservation contract, in the world rather than in `shipdesign`:
/// welding a wall out of the hold changes where the weight is and not how
/// much of it there is, and it does not move the hull a millimetre.
#[test]
fn building_a_part_out_of_the_hold_moves_no_mass_and_no_hull() {
    // The flyer has no shelf, so there is nowhere to keep the materials. One
    // shelf and a hundred units of metal is what a construction step would
    // actually be working from.
    let budget = Budget::new(10_000_000);
    let mut design = flyer(2);
    design = apply(
        &design,
        &budget,
        Edit::Place {
            kind: PartKind::Shelf,
            origin: (7, 9),
            rotation: Rotation::R0,
        },
    )
    .expect("a shelf on bare deck");
    design = apply(
        &design,
        &budget,
        Edit::Buy {
            resource: ResourceId::Metal,
            units: 60,
        },
    )
    .expect("metal onto the shelf");

    let mut world = world_with(design, 10_000, 2);
    let before_mass = world.ship.dynamics.mass.get();
    let before_anchor = world.ship.anchor;
    let before_centre = world.ship.dynamics.centre_of_mass;
    let hull_before: Vec<DVec2> = hull_positions(&world);

    // A wall at the far end of the ship, which is where the centre of mass
    // will be dragged.
    world.ship.design = build_from_cargo(
        &world.ship.design,
        Edit::Place {
            kind: PartKind::Wall,
            origin: (16, 16),
            rotation: Rotation::R0,
        },
    )
    .expect("the hold had the metal for a wall");
    world.on_ship_changed();

    assert!(
        close(world.ship.dynamics.mass.get(), before_mass),
        "mass went from {before_mass} to {}",
        world.ship.dynamics.mass.get(),
    );
    assert_eq!(world.ship.anchor, before_anchor, "the anchor moved");
    assert_eq!(hull_positions(&world), hull_before, "the hull moved");
    assert_ne!(
        world.ship.dynamics.centre_of_mass, before_centre,
        "a wall at one end should have pulled the weight towards it",
    );
    // And it moved *towards the wall*, which is down and to the right of
    // where it was in design coordinates.
    assert!(world.ship.dynamics.centre_of_mass.x > before_centre.x);
    assert!(world.ship.dynamics.centre_of_mass.y > before_centre.y);
}

/// Every hull tile, in system coordinates. What must not move when the ship
/// changes shape.
fn hull_positions(world: &World) -> Vec<DVec2> {
    use flight::angle;
    use shipdesign::parts::TILE;
    let mut out = Vec::new();
    for part in &world.ship.design.parts {
        if part.kind != PartKind::OutsideWall {
            continue;
        }
        for (x, y) in part.tiles() {
            let design = dvec2(
                (x as f64 + 0.5) * TILE as f64,
                (y as f64 + 0.5) * TILE as f64,
            );
            out.push(
                world
                    .ship
                    .anchor
                    .add(angle::rotate_design(design, world.ship.heading)),
            );
        }
    }
    out
}

/// A design change mid-flight does not move an arrival that has already been
/// promised. The trip keeps the dynamics it was planned with; the *next* one
/// gets the new ones.
#[test]
fn changing_the_ship_under_way_does_not_move_the_arrival() {
    let mut world = basic();
    let target = nearby(&world, 12_000.0);
    world.step(&[Command::Confirm { slot: 0, target }]);
    let (arrival, duration) = {
        let plan = world.plan().unwrap();
        (plan.arrival, plan.duration())
    };
    let was = world.ship.dynamics;

    for _ in 0..500 {
        world.step(&[]);
    }
    // A wall, welded on out of nowhere — the point is the dynamics changing,
    // not where the metal came from.
    world.ship.design = apply(
        &world.ship.design,
        &Budget::new(10_000_000),
        Edit::Place {
            kind: PartKind::Wall,
            origin: (16, 16),
            rotation: Rotation::R0,
        },
    )
    .unwrap();
    world.on_ship_changed();
    assert_ne!(
        world.ship.dynamics, was,
        "the ship should fly differently now"
    );

    let plan = world.plan().unwrap();
    assert_eq!(
        plan.dynamics, was,
        "the trip kept the ship it was planned for"
    );
    assert!(close(plan.duration(), duration));
    assert!(plan.arrival.distance(arrival) < 1e-9);
}

#[test]
fn engines_and_thrusters_are_not_to_be_touched_in_flight() {
    let mut world = basic();
    for kind in [PartKind::Engine, PartKind::Thruster] {
        assert!(world.can_modify_part(kind), "{kind:?} while docked");
    }

    let target = nearby(&world, 12_000.0);
    world.step(&[Command::Confirm { slot: 0, target }]);
    for kind in [PartKind::Engine, PartKind::Thruster] {
        assert!(!world.can_modify_part(kind), "{kind:?} while travelling");
    }
    // Everything else is still fair game — a bunk does not change a trip.
    assert!(world.can_modify_part(PartKind::Bunk));

    until_stopped(&mut world, 200_000);
    assert!(
        world.can_modify_part(PartKind::Engine),
        "holding, not flying"
    );
}

/// The other half of the same rule: a tank cannot come off while what is in
/// it is spoken for.
#[test]
fn a_tank_holding_a_reservation_cannot_be_taken_off() {
    let mut world = basic();
    assert!(
        world.can_modify_part(PartKind::FuelTank),
        "nothing is reserved yet",
    );

    let target = nearby(&world, 12_000.0);
    world.step(&[Command::Confirm { slot: 0, target }]);
    assert!(world.ship.reserved_fuel > 0);
    assert!(
        !world.can_modify_part(PartKind::FuelTank),
        "the only tank is holding the reservation",
    );
}

// --- fuel ---------------------------------------------------------------------

#[test]
fn confirming_reserves_the_fuel_and_arriving_spends_it() {
    let mut world = basic();
    let aboard = world.ship.fuel_aboard();
    assert_eq!(aboard, FLYER_FUEL);

    let target = nearby(&world, 4_000.0);
    world.step(&[Command::Confirm { slot: 0, target }]);
    let wanted = world.plan().unwrap().fuel_required;
    assert!(wanted > 0.0);
    assert_eq!(world.ship.reserved_fuel, wanted.ceil() as u32);
    assert_eq!(
        world.ship.unreserved_fuel(),
        aboard - world.ship.reserved_fuel,
    );
    // Reserving does not burn anything: it is still all aboard.
    assert_eq!(world.ship.fuel_aboard(), aboard);

    let spent = world.ship.reserved_fuel;
    until_stopped(&mut world, 200_000);
    assert_eq!(world.ship.fuel_aboard(), aboard - spent);
    assert_eq!(world.ship.reserved_fuel, 0, "the reservation was released");
}

/// Reserved fuel is not the crew's to sell. It has been promised to a trip
/// that is already under way, and there is nowhere out there to buy more.
#[test]
fn reserved_fuel_cannot_be_sold() {
    let mut world = basic();
    let target = nearby(&world, 4_000.0);
    // Confirmed, then stopped again, so the ship is docked with a reservation
    // still standing — which is the only way to be docked and reserved at
    // once, and exactly the case the rule is about.
    world.step(&[Command::Confirm { slot: 0, target }]);
    let reserved = world.ship.reserved_fuel;
    assert!(reserved > 0);

    // While travelling, trading is refused for a different reason entirely.
    let events = world.step(&[Command::Sell {
        slot: 0,
        resource: ResourceId::Fuel,
        units: 1,
    }]);
    assert!(refused_with(&events, Refusal::NotDocked));
    assert_eq!(world.ship.fuel_aboard(), FLYER_FUEL);
}

/// An abort charges for what was actually burnt — the part of the trip that
/// happened, plus the braking — and hands the rest of the reservation back.
#[test]
fn an_abort_only_costs_what_it_actually_burnt() {
    let mut world = basic();
    let target = nearby(&world, 40_000.0);
    world.step(&[Command::Confirm { slot: 0, target }]);
    let whole_trip = world.ship.reserved_fuel;

    for _ in 0..4_000 {
        world.step(&[]);
    }
    let events = world.step(&[Command::Abort { slot: 1 }]);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, WorldEvent::Aborted { slot: 1 })),
        "{events:?}",
    );
    let stopping = world.ship.reserved_fuel;
    assert!(
        stopping < whole_trip,
        "giving up should cost less than going: {stopping} against {whole_trip}",
    );

    until_stopped(&mut world, 200_000);
    assert_eq!(world.ship.state, ShipState::Holding);
    assert_eq!(world.ship.reserved_fuel, 0);
    assert_eq!(world.ship.fuel_aboard(), FLYER_FUEL - stopping);
    assert_eq!(world.ship.destination_set_by, None);
}

#[test]
fn a_ship_with_no_fuel_is_not_going_anywhere() {
    let dry = apply(
        &flyer(2),
        &Budget::new(10_000_000),
        Edit::Sell {
            resource: ResourceId::Fuel,
            units: FLYER_FUEL,
        },
    )
    .unwrap();
    let mut world = world_with(dry, REFERENCE_MONEY, 2);
    let target = nearby(&world, 4_000.0);
    assert_eq!(world.preview(target), Err(PlanError::NoFuelAboard));

    let events = world.step(&[Command::Confirm { slot: 0, target }]);
    assert!(
        events.iter().any(|e| matches!(
            e,
            WorldEvent::PlanFailed {
                error: PlanError::NoFuelAboard,
                ..
            }
        )),
        "{events:?}",
    );
    assert!(matches!(world.ship.state, ShipState::Docked { .. }));
}

// --- redirecting ----------------------------------------------------------------

/// A Confirm while the ship is under way stops it first and then sets off
/// again. Two separate plans, one after the other, and the ship is at rest in
/// between — which is the only shape a trip has, so a redirect does not need
/// a mechanism of its own.
#[test]
fn a_confirm_under_way_stops_first_and_then_goes() {
    let mut world = basic();
    let first = nearby(&world, 40_000.0);
    world.step(&[Command::Confirm {
        slot: 0,
        target: first,
    }]);
    for _ in 0..4_000 {
        world.step(&[]);
    }
    assert_eq!(world.plan().unwrap().target, first);

    let second = Target::Point(world.ship.position().add(dvec2(-8_000.0, 2_000.0)));
    world.step(&[Command::Confirm {
        slot: 1,
        target: second,
    }]);
    assert!(world.plan().unwrap().aborting, "it should be stopping");
    assert_eq!(world.ship.pending, Some((1, second)));
    assert_eq!(
        world.ship.destination_set_by,
        Some(1),
        "the route is theirs now"
    );

    // It comes to rest, and the moment it does the second trip begins — with
    // no gap, and without the player having to ask twice.
    let mut set_off = false;
    for _ in 0..200_000 {
        let events = world.step(&[]);
        if events
            .iter()
            .any(|e| matches!(e, WorldEvent::Departed { .. }))
        {
            set_off = true;
            break;
        }
    }
    assert!(set_off, "the redirect never set off");
    assert_eq!(world.plan().unwrap().target, second);
    assert!(!world.plan().unwrap().aborting);
    assert_eq!(world.ship.pending, None);

    until_stopped(&mut world, 400_000);
    let want = match second {
        Target::Point(at) => at,
        _ => unreachable!(),
    };
    assert!(world.ship.position().distance(want) < 1.0);
}

/// Only the latest confirmed target is kept. Two in one step is one trip, and
/// it is the second one's.
#[test]
fn two_confirms_in_one_step_keep_the_later_one() {
    let mut world = basic();
    let first = nearby(&world, 40_000.0);
    let second = Target::Point(world.ship.position().add(dvec2(-30_000.0, 0.0)));
    world.step(&[
        Command::Confirm {
            slot: 0,
            target: first,
        },
        Command::Confirm {
            slot: 1,
            target: second,
        },
    ]);

    // The first set off and the second turned it into a redirect. A ship one
    // step out of the dock has no speed and no spin to take out, so the stop
    // is of no length and the second trip is already under way — which is the
    // redirect working, not a shortcut round it.
    assert_eq!(world.ship.destination_set_by, Some(1));
    let going_to = world.ship.pending.map(|(_, t)| t).unwrap_or_else(|| {
        let plan = world.plan().expect("it should be flying somewhere");
        assert!(!plan.aborting, "an abort with nothing pending goes nowhere");
        plan.target
    });
    assert_eq!(going_to, second);
    assert_ne!(going_to, first);
}

/// A redirect whose second leg cannot be flown leaves the ship holding where
/// it stopped, and says why.
#[test]
fn a_redirect_that_cannot_be_flown_holds_and_says_so() {
    let mut world = basic();
    let first = nearby(&world, 40_000.0);
    world.step(&[Command::Confirm {
        slot: 0,
        target: first,
    }]);
    for _ in 0..4_000 {
        world.step(&[]);
    }
    // Somewhere nobody has seen. A node the generator put in this system but
    // that is beyond the ship's sensors — with the chart forgotten first,
    // because a system opens fully charted.
    world.uncharted_for_probe();
    let unseen = world
        .system
        .nodes()
        .into_iter()
        .find(|n| !world.discovered.contains(n));
    let Some(unseen) = unseen else {
        return; // this seed's system is small enough to see all at once
    };
    let target = match unseen {
        worldgen::Node::Body(id) => Target::Body(id),
        worldgen::Node::Station(id) => Target::Station(id),
    };
    let events = world.step(&[Command::Confirm { slot: 0, target }]);
    assert!(
        events.iter().any(|e| matches!(
            e,
            WorldEvent::PlanFailed {
                error: PlanError::TargetUndiscovered,
                ..
            }
        )),
        "{events:?}",
    );
    // And the first trip carries on, untouched.
    assert_eq!(world.plan().unwrap().target, first);
    assert!(!world.plan().unwrap().aborting);
}

// --- trading ---------------------------------------------------------------------

#[test]
fn buying_costs_money_and_makes_the_ship_heavier() {
    let mut world = world_with(flyer(2), 100_000, 2);
    // Somewhere to put it first: the flyer has a fuel tank and a cold store
    // and no racking at all.
    world.ship.design = apply(
        &world.ship.design,
        &Budget::new(10_000_000),
        Edit::Place {
            kind: PartKind::Shelf,
            origin: (7, 9),
            rotation: Rotation::R0,
        },
    )
    .unwrap();
    world.on_ship_changed();

    let money = world.money;
    let mass = world.ship.dynamics.mass.get();
    let events = world.step(&[Command::Buy {
        slot: 1,
        resource: ResourceId::Metal,
        units: 10,
    }]);

    assert!(events.iter().any(|e| matches!(
        e,
        WorldEvent::Traded {
            resource: ResourceId::Metal,
            units: 10,
            ..
        }
    )));
    assert_eq!(world.money, money - 10 * trade_price(ResourceId::Metal));
    assert_eq!(world.ship.design.carrying(ResourceId::Metal), 10);
    assert!(close(
        world.ship.dynamics.mass.get(),
        mass + 10.0 * ResourceId::Metal.mass_per_unit(),
    ));

    // And selling puts every euro of it back.
    world.step(&[Command::Sell {
        slot: 0,
        resource: ResourceId::Metal,
        units: 10,
    }]);
    assert_eq!(world.money, money);
    assert!(close(world.ship.dynamics.mass.get(), mass));
}

#[test]
fn what_cannot_be_paid_for_or_stowed_is_refused() {
    let mut world = world_with(flyer(2), 100, 2);
    // No money.
    let events = world.step(&[Command::Buy {
        slot: 0,
        resource: ResourceId::Components,
        units: 10,
    }]);
    assert!(refused_with(&events, Refusal::Unaffordable));

    // Money, but nowhere to put it: the flyer has no racking.
    world.money = 1_000_000;
    assert_eq!(world.ship.design.capacity(Storage::Shelf), 0);
    let events = world.step(&[Command::Buy {
        slot: 0,
        resource: ResourceId::Metal,
        units: 1,
    }]);
    assert!(refused_with(&events, Refusal::NoRoomAboard));

    // And selling what is not there.
    let events = world.step(&[Command::Sell {
        slot: 0,
        resource: ResourceId::Ore,
        units: 1,
    }]);
    assert!(refused_with(&events, Refusal::NotAboard));
}

/// Money works at a dock and nowhere else. That is `shipdesign::materials`'
/// rule, and this is the world keeping it.
#[test]
fn nothing_is_bought_or_sold_away_from_a_station() {
    let mut world = world_with(flyer(2), 1_000_000, 2);
    let target = nearby(&world, 4_000.0);
    world.step(&[Command::Confirm { slot: 0, target }]);

    for command in [
        Command::Buy {
            slot: 0,
            resource: ResourceId::Vegetable,
            units: 1,
        },
        Command::Sell {
            slot: 0,
            resource: ResourceId::Vegetable,
            units: 1,
        },
    ] {
        let events = world.step(&[command]);
        assert!(refused_with(&events, Refusal::NotDocked), "{command:?}");
    }

    until_stopped(&mut world, 200_000);
    assert_eq!(world.ship.state, ShipState::Holding);
    let events = world.step(&[Command::Buy {
        slot: 0,
        resource: ResourceId::Vegetable,
        units: 1,
    }]);
    assert!(
        refused_with(&events, Refusal::NotDocked),
        "holding is not docked either",
    );
}

fn refused_with(events: &[WorldEvent], want: Refusal) -> bool {
    events
        .iter()
        .any(|e| matches!(e, WorldEvent::Refused { why, .. } if *why == want))
}

// --- looking out of the window ----------------------------------------------------

/// Something that comes within range of the stretch just travelled is seen;
/// something that never does is not. The **stretch**, not the endpoints: at
/// the top speed a step is a long way, and a station passed in the middle of
/// one would otherwise be missed entirely.
#[test]
fn what_comes_within_range_of_the_track_is_seen() {
    let mut world = basic();
    let range = world.detection_range();
    assert!(range > 0.0);

    let here = world.ship.position();
    let out = here.add(dvec2(range * 20.0, 0.0));

    // A point half way along the track and just inside the range, and another
    // beside the same track and well outside it.
    let seen_from = here.add(dvec2(range * 10.0, range * 0.5));
    let unseen_from = here.add(dvec2(range * 10.0, range * 3.0));
    assert!(crate::world::segment_distance_for_probe(here, out, seen_from) <= range);
    assert!(crate::world::segment_distance_for_probe(here, out, unseen_from) > range);

    // Neither endpoint is anywhere near the near one, which is the half that
    // an endpoint-only test would get wrong.
    assert!(here.distance(seen_from) > range);
    assert!(out.distance(seen_from) > range);
}

#[test]
fn flying_past_something_is_what_discovers_it() {
    let mut world = basic();
    world.uncharted_for_probe();
    let unseen: Vec<worldgen::Node> = world
        .system
        .nodes()
        .into_iter()
        .filter(|n| !world.discovered.contains(n))
        .collect();
    let Some(&node) = unseen.first() else {
        return; // everything was in range from the dock
    };
    let at = world.system.absolute_position(node).unwrap();

    // Standing still beside it is enough; the track is a point.
    let events = world.discover_for_probe(at, at);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, WorldEvent::Discovered { node: n } if *n == node)),
        "{events:?}",
    );
    assert!(world.discovered.contains(&node));

    // And it is never forgotten, and never listed twice.
    let again = world.discover_for_probe(at, at);
    assert!(again.is_empty());
    assert_eq!(world.discovered.iter().filter(|&&n| n == node).count(), 1,);
}

#[test]
fn sensor_arrays_are_what_make_a_system_visible() {
    let world = basic();
    let with = world.detection_range();

    let mut blind = flyer(2);
    blind.parts.retain(|p| p.kind != PartKind::SensorArray);
    let world = world_with(blind, REFERENCE_MONEY, 2);
    assert_eq!(world.detection_range(), data::VISION_RANGE);
    assert!(with > world.detection_range());
}

// --- the local frame -----------------------------------------------------------

/// In at the radius, out at a quarter again, and nothing at all in between.
/// A ship sitting exactly on the line must not strobe.
#[test]
fn the_local_frame_has_a_hysteresis_and_uses_it() {
    let mut world = basic();
    let ShipState::Docked { station } = world.ship.state else {
        unreachable!()
    };
    let node = worldgen::Node::Station(station);
    let at = world.system.absolute_position(node).unwrap();
    let radius = data::local_radius(node);

    // Undock, so the ship is holding rather than tied on.
    world.ship.state = ShipState::Holding;
    world.put_for_probe(at);
    world.settle_frame_for_probe();
    assert_eq!(world.ship.frame, Frame::Local(node));

    // Drifting between the two radii changes nothing, however often.
    for i in 0..10 {
        let out = radius * (1.0 + 0.2 * (i % 2) as f64);
        world.put_for_probe(at.add(dvec2(out, 0.0)));
        assert!(
            world.settle_frame_for_probe().is_empty(),
            "the frame flipped at {out}",
        );
        assert_eq!(world.ship.frame, Frame::Local(node));
    }

    // Past the outer radius, once.
    world.put_for_probe(at.add(dvec2(radius * data::LOCAL_HYSTERESIS * 1.01, 0.0)));
    let events = world.settle_frame_for_probe();
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(world.ship.frame, Frame::Space);

    // And back in takes the inner one: the outer radius is not enough.
    world.put_for_probe(at.add(dvec2(radius * 1.1, 0.0)));
    assert!(world.settle_frame_for_probe().is_empty());
    assert_eq!(world.ship.frame, Frame::Space);
    world.put_for_probe(at.add(dvec2(radius * 0.9, 0.0)));
    let events = world.settle_frame_for_probe();
    assert_eq!(events.len(), 1);
    assert_eq!(world.ship.frame, Frame::Local(node));
}

/// The frame is about the view. It must not reach the clock or the speed —
/// which is easy to say and easy to break, so it is checked.
#[test]
fn the_frame_changing_does_not_touch_the_clock_or_the_speed() {
    let mut world = basic();
    world.step(&[Command::SetSpeed {
        slot: 0,
        speed: Speed::Ten,
    }]);
    let speed = world.effective_speed();
    let target = nearby(&world, 40_000.0);

    // The Confirm is inside the loop, because the frame change it causes —
    // undocking — happens in that very step. Counting from afterwards would
    // count nothing and the test would be asserting that nothing happened.
    let mut changes = 0;
    let mut last = world.clock_minutes;
    for i in 0..8_000 {
        let commands: Vec<Command> = if i == 0 {
            vec![Command::Confirm { slot: 0, target }]
        } else {
            Vec::new()
        };
        let events = world.step(&commands);
        assert!(
            close(world.clock_minutes - last, data::STEP_MINUTES),
            "a step was not a step",
        );
        last = world.clock_minutes;
        assert_eq!(world.effective_speed(), speed);
        changes += events
            .iter()
            .filter(|e| matches!(e, WorldEvent::FrameChanged { .. }))
            .count();
    }
    assert!(
        changes > 0,
        "leaving the dock should have changed the frame"
    );
}

// --- speed ---------------------------------------------------------------------

#[test]
fn the_world_runs_at_the_slowest_request() {
    let mut world = basic();
    world.step(&[
        Command::SetSpeed {
            slot: 0,
            speed: Speed::Top,
        },
        Command::SetSpeed {
            slot: 1,
            speed: Speed::Triple,
        },
    ]);
    assert_eq!(world.effective_speed(), Speed::Triple);

    world.step(&[Command::SetSpeed {
        slot: 1,
        speed: Speed::Paused,
    }]);
    assert_eq!(world.effective_speed(), Speed::Paused);
    assert_eq!(world.effective_speed().multiplier(), 0);

    // A slot nobody is in cannot change it.
    world.step(&[Command::SetSpeed {
        slot: 9,
        speed: Speed::Top,
    }]);
    assert_eq!(world.effective_speed(), Speed::Paused);
}

// --- permissions ------------------------------------------------------------------

/// Everyone can do everything, today. What the function is *for* is being the
/// one place a later step makes Confirm and Abort want a Bim at the helm.
#[test]
fn anybody_aboard_can_give_the_ship_an_order() {
    let mut world = basic();
    assert!(world.can_command(0));
    assert!(world.can_command(1));
    assert!(!world.can_command(2), "there are only two of them");

    let target = nearby(&world, 12_000.0);
    let events = world.step(&[Command::Confirm { slot: 1, target }]);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, WorldEvent::Departed { slot: 1 }))
    );
    assert_eq!(world.ship.destination_set_by, Some(1));

    let events = world.step(&[Command::Confirm { slot: 5, target }]);
    assert!(refused_with(&events, Refusal::NotAtTheHelm));
}

// --- the cross-target number -------------------------------------------------------

/// The scenario `ship_self_check` runs in wasm, run here in native. Both
/// compare against the same written-down constant; a target whose arithmetic
/// drifted fails exactly one of the two.
#[test]
fn the_reference_run_comes_out_at_the_number_it_is_pinned_to() {
    assert_eq!(
        crate::fixture::reference_run(),
        crate::fixture::REFERENCE_CHECKSUM,
    );
    // And it is the same every time it is asked for, which is the other half
    // of what a checksum is worth.
    assert_eq!(
        crate::fixture::reference_run(),
        crate::fixture::reference_run()
    );
}

/// A checksum that did not notice anything would pass every test above.
#[test]
fn the_checksum_notices_a_world_that_has_moved() {
    let mut world = basic();
    let was = world.checksum();
    world.step(&[]);
    assert_ne!(world.checksum(), was, "a step should show");

    let mut spent = basic();
    spent.money -= 1;
    assert_ne!(spent.checksum(), basic().checksum(), "a euro should show");

    let mut confirmed = basic();
    let target = nearby(&confirmed, 12_000.0);
    confirmed.step(&[Command::Confirm { slot: 0, target }]);
    let mut idle = basic();
    idle.step(&[]);
    assert_ne!(confirmed.checksum(), idle.checksum(), "a trip should show");
}

// --- where a world starts ----------------------------------------------------

/// The lobby names the spawn, and a world starts exactly there — any star
/// with a station, any station in it — docked, with that dock discovered.
#[test]
fn a_world_starts_at_the_station_it_was_told_to() {
    let galaxy = worldgen::Galaxy::new(data::DEFAULT_SEED, GalaxyType::SpiralTwoArm);
    // Not the simulation's spawn: the *last* dock in the galaxy, so a world
    // that quietly fell back to `spawn` would show up.
    let (star, station) = galaxy
        .stars
        .iter()
        .rev()
        .find_map(|s| {
            let system = galaxy.system(s.id)?;
            let last = system.stations.iter().map(|st| st.id).max()?;
            Some((s.id, last))
        })
        .expect("a galaxy has a station somewhere");
    assert_ne!((star, station), crate::spawn(&galaxy).unwrap());

    let world = World::start(
        flyer(2),
        REFERENCE_MONEY,
        2,
        data::DEFAULT_SEED,
        GalaxyType::SpiralTwoArm,
        star,
        station,
    )
    .expect("a real station is somewhere to start");
    assert_eq!(world.star_id, star);
    assert_eq!(world.ship.state, ShipState::Docked { station });
    assert!(world.discovered.contains(&worldgen::Node::Station(station)));
}

/// A spawn that is not there is an error and never a different dock.
#[test]
fn a_spawn_that_does_not_exist_is_refused_rather_than_replaced() {
    let galaxy = worldgen::Galaxy::new(data::DEFAULT_SEED, GalaxyType::SpiralTwoArm);
    let empty = galaxy
        .stars
        .iter()
        .find(|s| galaxy.system(s.id).unwrap().stations.is_empty())
        .expect("most stars have no station")
        .id;
    let (star, _) = crate::spawn(&galaxy).unwrap();
    let start = |star, station| {
        World::start(
            flyer(2),
            REFERENCE_MONEY,
            2,
            data::DEFAULT_SEED,
            GalaxyType::SpiralTwoArm,
            star,
            station,
        )
    };
    assert_eq!(
        start(empty, 0).err(),
        Some(crate::StartError::NoSuchStation)
    );
    assert_eq!(
        start(star, 999).err(),
        Some(crate::StartError::NoSuchStation)
    );
    assert_eq!(
        start(u32::MAX, 0).err(),
        Some(crate::StartError::NoSuchStation)
    );
}

/// The simulation's ship, at the simulation's dock, with the simulation's
/// purse: a trip to the nearest thing the crew can see can be planned with
/// the fuel aboard, or the playtest is a ship that cannot leave the dock.
#[test]
fn the_playtest_ship_can_fly_somewhere_from_the_simulation_spawn() {
    use shipdesign::fixture::playtest_ship;
    let world = simulation_world(playtest_ship(), data::SIMULATION_MONEY, 1);
    assert_eq!(world.money, data::SIMULATION_MONEY);
    assert_eq!(world.ship.crew_count, 1);
    let ShipState::Docked { station: dock } = world.ship.state else {
        panic!("not docked");
    };
    assert_eq!(
        world.ship.fuel_aboard(),
        world.ship.design.capacity(Storage::FuelTank)
    );

    // The nearest node that is somewhere to *go*. At this spawn nothing but
    // the dock's own parent body is in sight, and that is where the ship
    // already is — the planner says `AlreadyThere` — so the trip is to the
    // next thing out, revealed through the probe seam the way a sensor sweep
    // would reveal it, and it has to be flyable on the tank.
    let here = world.ship.position();
    let mut nodes: Vec<worldgen::Node> = world
        .system
        .nodes()
        .into_iter()
        .filter(|&n| n != worldgen::Node::Station(dock))
        .collect();
    nodes.sort_by(|a, b| {
        let da = world.system.absolute_position(*a).unwrap().distance(here);
        let db = world.system.absolute_position(*b).unwrap().distance(here);
        da.partial_cmp(&db).unwrap()
    });
    let mut world = world;
    let mut quote = None;
    for node in nodes {
        let at = world.system.absolute_position(node).unwrap();
        world.discover_for_probe(at, at);
        let target = match node {
            worldgen::Node::Body(id) => Target::Body(id),
            worldgen::Node::Station(id) => Target::Station(id),
        };
        match world.preview(target) {
            Err(PlanError::AlreadyThere) => continue,
            other => {
                quote = Some(other.expect("the trip should plan"));
                break;
            }
        }
    }
    let quote = quote.expect("the spawn system has somewhere to fly to");
    assert!(quote.minutes > 0.0);
    assert!(
        quote.fuel <= world.ship.unreserved_fuel() as f64,
        "the trip wants {} fuel and there are {} aboard",
        quote.fuel,
        world.ship.unreserved_fuel()
    );
}

// --- the crew ----------------------------------------------------------------

/// The crew are aboard from the first step, and they are the room's Bims:
/// one per player, each at their own bunk — Bim *i* at bunk *i* in id
/// order — standing on its use spot, which is deck.
#[test]
fn everybody_spawns_at_their_own_bunk() {
    let world = basic();
    assert_eq!(world.aboard.count(), 2);
    let mut bunks: Vec<_> = world
        .ship
        .design
        .parts
        .iter()
        .filter(|p| p.kind == PartKind::Bunk)
        .collect();
    bunks.sort_by_key(|p| p.id);
    let grid = world.ship.design.grid();
    let mut seen = Vec::new();
    for who in 0..world.aboard.count() {
        let spot = bunks[who as usize].use_spots()[0];
        let at = world.aboard.position(who);
        let want = bims::aboard::tile_middle(spot.0, spot.1);
        assert!(
            (at.x - want.x as f64).abs() < 1e-3 && (at.y - want.y as f64).abs() < 1e-3,
            "bim {who} at {at:?}, bunk {who} is used from {want:?}"
        );
        assert_ne!(
            grid.get(shipdesign::Layer::Floor, spot),
            0,
            "bim {who} is not on deck"
        );
        assert_eq!(
            grid.get(shipdesign::Layer::Object, spot),
            0,
            "bim {who} is inside something"
        );
        assert!(!seen.contains(&spot), "two Bims in one place");
        seen.push(spot);
    }
}

/// Stage 5 runs on the world's clock and nobody else's: the room's clock
/// aboard reads exactly the world's, step for step, and a Bim that was put
/// somewhere else shows in the checksum.
#[test]
fn the_crew_keep_the_world_s_clock() {
    let mut world = basic();
    let mut other = basic();
    // The room's day starts at eight in the morning, so its clock reads
    // ahead of the world's by a fixed offset; what has to agree is how far
    // each has moved.
    let dawn = world.aboard.minutes();
    for _ in 0..10 {
        world.step(&[]);
        other.step(&[]);
    }
    // To a thousandth of a minute: the room keeps its clock in `f32`, and
    // at eight in the morning an `f32` minute is good to about that.
    assert!(
        ((world.aboard.minutes() - dawn) - world.clock_minutes).abs() < 1e-3,
        "room {} vs world {}",
        world.aboard.minutes() - dawn,
        world.clock_minutes
    );
    assert_eq!(world.checksum(), other.checksum());
    let was = other.aboard.position(0);
    other
        .aboard
        .room
        .put_for_probe(0, bims::math::vec2(was.x as f32 + 60.0, was.y as f32));
    assert_ne!(
        world.checksum(),
        other.checksum(),
        "a Bim that moved should show"
    );
}

/// The room aboard is the room: left to themselves for a game day the
/// crew walk about, and the deck they walk is the design's — nobody ends
/// up standing in a wall or off the ship.
#[test]
fn the_crew_live_aboard() {
    use shipdesign::fixture::playtest_ship;
    let mut world = simulation_world(playtest_ship(), data::SIMULATION_MONEY, 1);
    assert_eq!(world.aboard.count(), 1);
    let start = world.aboard.position(0);
    let mut moved = false;
    let mut farthest = 0.0f64;
    let grid = world.ship.design.grid();
    let tile = shipdesign::parts::TILE as f64;
    // Six game hours: long enough to be hungry, cook and eat.
    for _ in 0..(6 * 60 * 60) {
        world.step(&[]);
        let at = world.aboard.position(0);
        let d = at.distance(start);
        farthest = farthest.max(d);
        if d > tile {
            moved = true;
        }
        let (tx, ty) = ((at.x / tile).floor() as i32, (at.y / tile).floor() as i32);
        assert_ne!(
            grid.get(shipdesign::Layer::Floor, (tx, ty)),
            0,
            "the Bim is off the deck at tile ({tx}, {ty}) after {} minutes",
            world.clock_minutes
        );
    }
    assert!(moved, "the Bim never went anywhere; farthest {farthest}");
}
