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

/// Run until the ship is at rest — holding or tied up — or give up. Coming
/// alongside at the end of a trip is part of the trip here.
fn until_stopped(world: &mut World, limit: u32) -> Vec<WorldEvent> {
    let mut seen = Vec::new();
    for _ in 0..limit {
        seen.extend(world.step(&[]));
        if matches!(
            world.ship.state,
            ShipState::Holding | ShipState::Docked { .. }
        ) {
            return seen;
        }
    }
    panic!("the trip never ended");
}

/// How many steps a departure from the berth is allowed: the station's
/// people walking ashore, the crew back, and the push-off. Half an hour.
const LEAVING: u32 = 30 * 60;

/// Confirm a trip from wherever the ship is, with that player's crew member
/// at the helm, and run until it is under way. From a berth that is the
/// whole of casting off — everybody to their own side of the airlock, the
/// push-off — which is what every trip in here begins with; from a hold it
/// is one step. Everything seen on the way comes back.
fn set_off(world: &mut World, slot: u32, target: Target) -> Vec<WorldEvent> {
    world.man_the_helm_for_probe(slot);
    let mut seen = world.step(&[Command::Confirm { slot, target }]);
    for _ in 0..LEAVING {
        if matches!(world.ship.state, ShipState::Travelling { .. }) {
            return seen;
        }
        seen.extend(world.step(&[]));
    }
    panic!("the ship never set off: {:?}", world.ship.state);
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
    assert_eq!(world.ship.reserved_fuel, 0);

    // Docked means alongside: the ship is at its berth, which is beside
    // the station rather than on top of it.
    let berth = world
        .berth_at(station)
        .expect("the spawn station has a door");
    assert!(world.ship.position().distance(berth.position) < 1e-9);
    assert_eq!(world.ship.heading, berth.heading);
    let at = world
        .system
        .absolute_position(worldgen::Node::Station(station))
        .unwrap();
    assert!(
        world.ship.position().distance(at) > world.station(station).unwrap().radius(),
        "the ship is inside the station"
    );

    // And the dock the ship is tied to is something the crew have seen —
    // as is everything else in the system, off the lobby's chart.
    assert!(world.discovered.contains(&worldgen::Node::Station(station)));
    assert_eq!(world.discovered.len(), world.system.nodes().len());
    assert_eq!(
        world.ship.frame,
        Frame::Local(worldgen::Node::Station(station))
    );
}

/// The spawn is the **lowest** star with a station somebody lives on and
/// the lowest such station in it, so two clients that generated the same
/// galaxy start in the same place — and not beside a wreck.
#[test]
fn the_spawn_is_the_first_lived_in_dock_in_the_galaxy() {
    use crate::station::residents_of;
    let galaxy = worldgen::Galaxy::new(data::DEFAULT_SEED, GalaxyType::SpiralTwoArm);
    let (star, station) = crate::spawn(&galaxy).expect("a galaxy has a station somewhere");
    for earlier in 0..star {
        let system = galaxy.system(earlier).unwrap();
        assert!(
            system.stations.iter().all(|s| residents_of(s.kind) == 0),
            "star {earlier} had a station somebody lives on"
        );
    }
    let system = galaxy.system(star).unwrap();
    assert_eq!(
        system
            .stations
            .iter()
            .filter(|s| residents_of(s.kind) > 0)
            .map(|s| s.id)
            .min(),
        Some(station)
    );
    assert!(residents_of(system.station(station).unwrap().kind) > 0);

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
    set_off(&mut world, 0, target);

    // The departure is the clock reading the step the plan was made in
    // began at — the push-off ending and the trip beginning are the same
    // instant, and commands are applied before the clock advances.
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
    set_off(&mut world, 0, target);
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
    for kind in [PartKind::Engine, PartKind::HeavyEngine, PartKind::Thruster] {
        assert!(world.can_modify_part(kind), "{kind:?} while docked");
    }

    let target = nearby(&world, 12_000.0);
    set_off(&mut world, 0, target);
    for kind in [PartKind::Engine, PartKind::HeavyEngine, PartKind::Thruster] {
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
    set_off(&mut world, 0, target);
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
    set_off(&mut world, 0, target);
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
    set_off(&mut world, 0, target);
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
    set_off(&mut world, 0, target);
    let whole_trip = world.ship.reserved_fuel;

    for _ in 0..4_000 {
        world.step(&[]);
    }
    world.man_the_helm_for_probe(1);
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

    // Refused at the berth, before anybody is sent ashore for nothing.
    world.man_the_helm_for_probe(0);
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
    set_off(&mut world, 0, first);
    for _ in 0..4_000 {
        world.step(&[]);
    }
    assert_eq!(world.plan().unwrap().target, first);

    let second = Target::Point(world.ship.position().add(dvec2(-8_000.0, 2_000.0)));
    world.man_the_helm_for_probe(1);
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
    world.man_the_helm_for_probe(0);
    world.man_the_helm_for_probe(1);
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

    // The first began the departure and the second changed where it is
    // going: the ship is still casting off, and the target it will set off
    // for once it is clear is the second one's.
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
    set_off(&mut world, 0, first);
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
    set_off(&mut world, 0, target);

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

/// A dock sells what its kind sells and nothing else: an emitter is never
/// on the shelf, galvum only at an outpost, and selling is open either way.
/// The spawn is whatever kind it is, so the test reads the kind and expects
/// accordingly — the rule itself is pinned in `worldgen`.
#[test]
fn a_station_only_sells_what_its_kind_sells() {
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
    let mut world = world_with(design, 1_000_000, 2);
    let ShipState::Docked { station } = world.ship.state else {
        panic!("a world opens docked");
    };
    let kind = world.station(station).unwrap().kind;
    assert!(kind.sells(ResourceId::Metal));

    let events = world.step(&[Command::Buy {
        slot: 0,
        resource: ResourceId::Emitter,
        units: 1,
    }]);
    assert!(refused_with(&events, Refusal::NotSoldHere), "{events:?}");
    assert_eq!(world.ship.design.carrying(ResourceId::Emitter), 0);

    let events = world.step(&[Command::Buy {
        slot: 0,
        resource: ResourceId::Galvum,
        units: 1,
    }]);
    if kind == worldgen::StationKind::MiningOutpost {
        assert_eq!(world.ship.design.carrying(ResourceId::Galvum), 1);
    } else {
        assert!(refused_with(&events, Refusal::NotSoldHere), "{events:?}");
        assert_eq!(world.ship.design.carrying(ResourceId::Galvum), 0);
    }

    // Metal is on every shelf, and what is aboard sells anywhere with
    // somebody to buy it.
    let events = world.step(&[Command::Buy {
        slot: 0,
        resource: ResourceId::Metal,
        units: 1,
    }]);
    assert!(!refused_with(&events, Refusal::NotSoldHere), "{events:?}");
    assert_eq!(world.ship.design.carrying(ResourceId::Metal), 1);
    let money = world.money;
    world.step(&[Command::Sell {
        slot: 0,
        resource: ResourceId::Metal,
        units: 1,
    }]);
    assert_eq!(world.ship.design.carrying(ResourceId::Metal), 0);
    assert!(world.money > money);
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
    // Straight away from whatever else is nearest, so that drifting out from
    // the station does not drift *into* its parent body's frame instead —
    // which is a fact about where the generator put the planet, not about
    // the hysteresis.
    let away = world
        .system
        .nodes()
        .into_iter()
        .filter(|&other| other != node)
        .filter_map(|other| world.system.absolute_position(other))
        .min_by(|a, b| {
            a.distance(at)
                .partial_cmp(&b.distance(at))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|other| at.sub(other))
        .map(|d| d.scale(1.0 / d.length().max(1e-9)))
        .unwrap_or(dvec2(1.0, 0.0));
    let out_at = |by: f64| at.add(away.scale(by));

    // Undock, so the ship is holding rather than tied on.
    world.ship.state = ShipState::Holding;
    world.put_for_probe(at);
    world.settle_frame_for_probe();
    assert_eq!(world.ship.frame, Frame::Local(node));

    // Drifting between the two radii changes nothing, however often.
    for i in 0..10 {
        let out = radius * (1.0 + 0.2 * (i % 2) as f64);
        world.put_for_probe(out_at(out));
        assert!(
            world.settle_frame_for_probe().is_empty(),
            "the frame flipped at {out}",
        );
        assert_eq!(world.ship.frame, Frame::Local(node));
    }

    // Past the outer radius, once.
    world.put_for_probe(out_at(radius * data::LOCAL_HYSTERESIS * 1.01));
    let events = world.settle_frame_for_probe();
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(world.ship.frame, Frame::Space);

    // And back in takes the inner one: the outer radius is not enough.
    world.put_for_probe(out_at(radius * 1.1));
    assert!(world.settle_frame_for_probe().is_empty());
    assert_eq!(world.ship.frame, Frame::Space);
    world.put_for_probe(out_at(radius * 0.9));
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
    world.man_the_helm_for_probe(0);

    // The Confirm is inside the loop, because the frame change it causes —
    // leaving the dock's local frame — happens during it. Counting from
    // afterwards would count nothing and the test would be asserting that
    // nothing happened.
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

/// The ship is flown from the helm: a Confirm from a player whose crew
/// member is standing anywhere else is refused, and one whose crew member
/// is at the seat begins the departure. Any of the crew, once there.
#[test]
fn the_ship_is_flown_from_the_helm() {
    let mut world = basic();
    let seat = world.helm_spot().expect("the flyer has a helm");
    assert!(!world.can_command(0), "nobody starts at the helm");
    assert!(!world.can_command(1));
    let target = nearby(&world, 12_000.0);
    let events = world.step(&[Command::Confirm { slot: 1, target }]);
    assert!(refused_with(&events, Refusal::NotAtTheHelm), "{events:?}");
    assert!(matches!(world.ship.state, ShipState::Docked { .. }));

    // Walking there — the order the page's button gives — puts the crew
    // member within reach of the seat and leaves them standing there.
    assert!(world.order_to_helm(1), "no route to the helm");
    let mut there = false;
    for _ in 0..LEAVING {
        world.step(&[]);
        if world.at_the_helm(1) {
            there = true;
            break;
        }
    }
    assert!(there, "never reached the helm");
    assert!(world.aboard.position(1).distance(seat) <= data::HELM_REACH);
    assert!(world.can_command(1));
    assert!(!world.can_command(0), "the other one is still elsewhere");
    assert!(!world.can_command(5), "there are only two of them");

    let events = world.step(&[Command::Confirm { slot: 1, target }]);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, WorldEvent::CastingOff { slot: 1 })),
        "{events:?}"
    );
    assert_eq!(world.ship.destination_set_by, Some(1));
    // And they stay at the helm rather than wandering off it.
    for _ in 0..600 {
        world.step(&[]);
    }
    assert!(world.at_the_helm(1), "wandered off the helm");
}

/// Walk one of the station's people onto the ship — in through the
/// airlock to the deck just inside it — so there is somebody to send
/// ashore. Who it was, in the joined room.
fn bring_a_resident_aboard(world: &mut World) -> u32 {
    let who = world.aboard.crew_count();
    assert!(who < world.aboard.count(), "nobody lives here");
    let to = world.aboard.gangway.expect("the ship has a door");
    assert!(
        world
            .aboard
            .room
            .send_for_probe(who as usize, bims::math::vec2(to.x as f32, to.y as f32)),
        "no route onto the ship"
    );
    for _ in 0..LEAVING {
        world.step(&[]);
        if world.aboard.on_ship(who, &world.ship.design) {
            return who;
        }
    }
    panic!("the resident never came aboard");
}

/// Leaving a station is three things in order: the station's people go
/// ashore and the crew come back aboard, the rooms come apart and the ship
/// pushes straight off the berth, and only then is the trip planned — from
/// where the push-off ended, so the turn towards the target is the plan's
/// own align phase. Nobody is teleported off the ship.
#[test]
fn leaving_a_station_sends_everybody_home_and_pushes_off_before_the_trip() {
    let mut world = basic();
    let ShipState::Docked { station } = world.ship.state else {
        panic!("not docked");
    };
    let berth = world.berth_at(station).unwrap();
    let residents = world.aboard.resident_count();
    assert!(residents > 0, "nobody to send ashore");
    let outward = world.station(station).unwrap().face().unwrap().1;
    let target = nearby(&world, 40_000.0);
    let visitor = bring_a_resident_aboard(&mut world);

    world.man_the_helm_for_probe(0);
    let events = world.step(&[Command::Confirm { slot: 0, target }]);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, WorldEvent::CastingOff { slot: 0 })),
        "{events:?}"
    );
    assert!(matches!(world.ship.state, ShipState::CastingOff { .. }));
    assert!(world.aboard.is_joined(), "the rooms came apart too soon");
    assert_eq!(world.ship.pending, Some((0, target)));

    // The station's people walk off the ship; the ship waits at the berth
    // until the last of them is off it, and then it is a room of its own.
    let mut cast_off = None;
    for i in 0..LEAVING {
        assert!(
            world.ship.position().distance(berth.position) < 1e-6,
            "moved while still casting off"
        );
        if i == 0 {
            assert!(world.aboard.on_ship(visitor, &world.ship.design));
        }
        let events = world.step(&[]);
        if events
            .iter()
            .any(|e| matches!(e, WorldEvent::Undocking { .. }))
        {
            cast_off = Some(i);
            break;
        }
    }
    let cast_off = cast_off.expect("never cast off");
    assert!(cast_off > 0, "the walk ashore takes time");
    assert!(
        !world.aboard.on_ship(visitor, &world.ship.design),
        "cast off with the visitor still aboard"
    );
    assert!(matches!(world.ship.state, ShipState::Undocking { .. }));
    assert!(!world.aboard.is_joined());
    assert_eq!(world.aboard.count(), 2, "somebody came along");
    assert!(world.aboard.everybody_home(&world.ship.design));

    // The push-off: straight out the way the door opens, further every
    // step, heading untouched, and the trip planned from where it ends.
    let mut last = 0.0;
    for _ in 0..LEAVING {
        world.step(&[]);
        if matches!(world.ship.state, ShipState::Travelling { .. }) {
            break;
        }
        let away = world.ship.position().sub(berth.position);
        let along = away.x * outward.x + away.y * outward.y;
        assert!(along >= last - 1e-9, "the ship came back");
        assert!((away.length() - along).abs() < 1e-6, "not straight out");
        assert_eq!(world.ship.heading, berth.heading, "turned before clear");
        last = along;
    }
    let ShipState::Travelling { plan, .. } = &world.ship.state else {
        panic!("never set off: {:?}", world.ship.state);
    };
    assert!(last > 0.0, "never pushed off");
    assert!(plan.start.distance(world.ship.position()) < 1e-6);
    assert!(
        plan.start.distance(berth.position) > 5.0 * shipdesign::TILE as f64,
        "planned from the berth"
    );
    assert_eq!(world.ship.pending, None);
}

/// Called off while everybody is still going ashore, the ship stays tied
/// up; called off during the push-off, it holds where the push-off ends.
#[test]
fn a_departure_can_be_called_off() {
    let mut world = basic();
    let ShipState::Docked { station } = world.ship.state else {
        panic!("not docked");
    };
    let target = nearby(&world, 40_000.0);
    bring_a_resident_aboard(&mut world);
    world.man_the_helm_for_probe(0);
    world.step(&[Command::Confirm { slot: 0, target }]);
    assert!(matches!(world.ship.state, ShipState::CastingOff { .. }));
    let events = world.step(&[Command::Abort { slot: 0 }]);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, WorldEvent::Aborted { slot: 0 }))
    );
    assert_eq!(world.ship.state, ShipState::Docked { station });
    assert!(world.aboard.is_joined());
    assert_eq!(world.ship.pending, None);
    assert_eq!(world.ship.destination_set_by, None);

    world.step(&[Command::Confirm { slot: 0, target }]);
    for _ in 0..LEAVING {
        world.step(&[]);
        if matches!(world.ship.state, ShipState::Undocking { .. }) {
            break;
        }
    }
    assert!(matches!(world.ship.state, ShipState::Undocking { .. }));
    world.step(&[Command::Abort { slot: 0 }]);
    until_stopped(&mut world, LEAVING);
    assert_eq!(world.ship.state, ShipState::Holding);
    assert_eq!(world.ship.destination_set_by, None);
    assert!(world.ship.pending.is_none());
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
    confirmed.man_the_helm_for_probe(0);
    confirmed.step(&[Command::Confirm { slot: 0, target }]);
    let mut idle = basic();
    idle.man_the_helm_for_probe(0);
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
    // Two of the crew; the station's residents are in the same room, after.
    assert_eq!(world.aboard.crew_count(), 2);
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
    for who in 0..world.aboard.crew_count() {
        let spot = bunks[who as usize].use_spots()[0];
        let at = world.aboard.position(who);
        let want = bims::aboard::tile_middle(spot.0, spot.1);
        // To a navigation cell: the world opens docked, and joining the
        // ship's room to the station's stands everybody on the nearest
        // cell a body fits in, which beside a bunk is a few units off the
        // spot itself.
        assert!(
            (at.x - want.x as f64).abs() <= 10.0 && (at.y - want.y as f64).abs() <= 10.0,
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
    assert_eq!(world.aboard.crew_count(), 1);
    let start = world.aboard.position(0);
    let mut moved = false;
    let mut farthest = 0.0f64;
    let tile = shipdesign::parts::TILE as f64;
    // Six game hours: long enough to be hungry, cook and eat. The deck is
    // the room's — docked, the ship's and the station's together — and
    // the Bim may well wander across to the station.
    for _ in 0..(6 * 60 * 60) {
        world.step(&[]);
        let at = world.aboard.position(0);
        let d = at.distance(start);
        farthest = farthest.max(d);
        if d > tile {
            moved = true;
        }
        assert!(
            world.aboard.on_deck(0),
            "the Bim is off the deck at {at:?} after {} minutes",
            world.clock_minutes
        );
    }
    assert!(moved, "the Bim never went anywhere; farthest {farthest}");
}

/// A day's grime is put right under the shower: the playtest ship has one,
/// and a Bim whose washing need is emptied goes and stands under it, comes
/// out with the bar full and its own filth washed off. Emptied by hand
/// rather than waited for, because on the clock the first shower is at the
/// end of the waking day and a test that long is a test of everything.
#[test]
fn a_bim_aboard_takes_a_shower_when_a_day_s_grime_has_caught_up_with_it() {
    use bims::game::JOB_SHOWER;
    use shipdesign::fixture::playtest_ship;
    let mut world = simulation_world(playtest_ship(), data::SIMULATION_MONEY, 1);
    // `Need::ALL`'s index of the washing need, as `spend_for_probe` counts.
    const WASHING: u32 = 5;
    let room = &mut world.aboard.room;
    assert!(
        room.need_count() > WASHING,
        "the washing need is on the list"
    );
    room.spend_for_probe(0, WASHING, 1.0);
    assert_eq!(room.need_level(0, WASHING), 0.0);
    let mut showered = false;
    for _ in 0..(3 * 60 * 60) {
        world.step(&[]);
        if world.aboard.room.activity(0) == JOB_SHOWER {
            showered = true;
        }
        if showered && world.aboard.room.activity(0) != JOB_SHOWER {
            break;
        }
    }
    assert!(showered, "the Bim never went for its shower");
    let level = world.aboard.room.need_level(0, WASHING);
    assert!(level > 0.9, "out of the shower the bar reads {level}");
}

/// Under way, the helm is a job on the work list: nobody posted there, and
/// the room posts whoever is free. Here James is stood at the helm for the
/// departure and then ordered elsewhere, which takes the post away — and
/// the job puts somebody back. Tied up again, the post the job set is
/// lifted, and only that one.
#[test]
fn under_way_the_helm_is_a_job_and_somebody_takes_it() {
    use bims::work::Job;
    let mut world = basic();
    let target = nearby(&world, 40_000.0);
    set_off(&mut world, 0, target);
    let room = &mut world.aboard.room;
    assert!(
        !room.work_on_offer_for_probe(1).contains(&Job::Helm.code()),
        "with James posted there the helm is not on offer"
    );
    // Off the helm: an order anywhere else ends the post. Two tiles aft.
    // Selected first, as the page has him from the first paint.
    room.select_group(1);
    let at = room.bim_pos(0);
    let tile = shipdesign::parts::TILE as f32;
    assert!(
        room.order_move(at.x, at.y + 2.0 * tile) != 0,
        "the order was refused"
    );
    assert!(room.post_of(0).is_none());
    let mut posted = None;
    for _ in 0..(2 * 60 * 60) {
        world.step(&[]);
        if let Some(who) = (0..2).find(|&who| world.aboard.room.post_of(who).is_some()) {
            posted = Some(who);
            break;
        }
    }
    let who = posted.expect("nobody took the helm");
    for _ in 0..(60 * 60) {
        if world.at_the_helm(who as u32) {
            break;
        }
        world.step(&[]);
    }
    assert!(world.at_the_helm(who as u32), "posted but never got there");
    // And once the ship no longer wants anybody there, the post goes.
    world.aboard.room.set_helm(None);
    assert!(
        world.aboard.room.post_of(who).is_none(),
        "the job's post outlived the trip"
    );
}

// --- stations ---------------------------------------------------------------

/// Every kind of station, at a handful of seeds, is a place the room can
/// live in: it has a door that opens onto space, the designer's rules find
/// nothing wrong with it for the people who live there, its hull keeps the
/// radiation out unless it is a derelict, and the same seed builds the same
/// station — which is what a native server and a browser agreeing depends
/// on.
#[test]
fn a_station_is_a_place_the_room_can_live_in() {
    use crate::station::{layout, residents_of, side_of};
    use shipdesign::{design_hash, exposure, has_errors, validate};
    for kind in worldgen::StationKind::ALL {
        for seed in [1u64, 7, 0x_5749_4e44_4f57_0001, u64::MAX] {
            let design = layout(kind, seed);
            assert_eq!(design.build_area, side_of(kind));
            assert_eq!(
                design_hash(&design),
                design_hash(&layout(kind, seed)),
                "{kind:?}"
            );
            let port = shipdesign::port(&design).unwrap_or_else(|| panic!("{kind:?} has no port"));
            assert_eq!(
                port.outward,
                (-1, 0),
                "{kind:?}: the port is in the west skin"
            );
            let issues = validate(&design, residents_of(kind));
            assert!(
                !has_errors(&issues),
                "{kind:?} at seed {seed}: {:?}",
                issues.iter().map(|i| i.code).collect::<Vec<_>>()
            );
            if kind != worldgen::StationKind::Derelict {
                assert!(
                    exposure(&design).is_empty(),
                    "{kind:?} lets the radiation in"
                );
            }
        }
    }
    // And two seeds are two stations, not one station twice.
    assert_ne!(
        design_hash(&layout(worldgen::StationKind::Orbital, 1)),
        design_hash(&layout(worldgen::StationKind::Orbital, 2)),
    );
}

/// A station's rooms are rooms the crew can actually walk: from the deck
/// inside the port, the room's own navigation — the inflated grid that
/// cannot pass a one-tile gap — finds a route to every tile a fixture is
/// worked from and to every open tile of deck. Asked of the navigation
/// rather than of `validate`, because the designer's reachability is
/// four-neighbour over tiles and passes doorways the body cannot fit
/// through; a room the residents cannot get into is a room whose fixtures
/// they starve in front of, and it is cheaper to find out here than by
/// watching one stand at a doorway for a game day.
#[test]
fn a_station_s_rooms_can_all_be_walked_from_its_door() {
    use crate::station::layout;
    use shipdesign::validate::walkable;
    let tile = shipdesign::TILE as f32;
    let middle =
        |(x, y): (u32, u32)| bims::math::vec2((x as f32 + 0.5) * tile, (y as f32 + 0.5) * tile);
    for kind in worldgen::StationKind::ALL {
        for seed in [1u64, 7, 0x_5749_4e44_4f57_0001] {
            let design = layout(kind, seed);
            let grid = design.grid();
            let room = bims::room::Room::from_layout(bims::aboard::layout_of(&design));
            let nav = bims::nav::Nav::tiled(
                room.interior,
                &room.solids(),
                bims::character::BODY_MARGIN,
                tile as f32,
            );
            let port = shipdesign::port(&design).unwrap();
            let inside = (
                (port.centre.0 / tile as f64) as u32 + 1,
                (port.centre.1 / tile as f64) as u32,
            );
            let from = nav.nearest_free(middle(inside));
            let mut cut_off = Vec::new();
            for part in &design.parts {
                for spot in part.use_spots() {
                    let spot = (spot.0 as u32, spot.1 as u32);
                    if nav.path(from, middle(spot)).is_empty() {
                        cut_off.push((part.kind, spot));
                    }
                }
            }
            assert!(
                cut_off.is_empty(),
                "{kind:?} at seed {seed}: no route from the door to {cut_off:?}"
            );
            let mut pockets = Vec::new();
            for y in 0..design.build_area as i32 {
                for x in 0..design.build_area as i32 {
                    if walkable(&design, &grid, (x, y))
                        && nav.path(from, middle((x as u32, y as u32))).is_empty()
                    {
                        pockets.push((x, y));
                    }
                }
            }
            assert!(
                pockets.is_empty(),
                "{kind:?} at seed {seed}: deck nobody can get to at {pockets:?}"
            );
        }
    }
}

/// The berth is airlock to airlock: the outer faces of the two doors are on
/// the same point, the ship's opens the way the station's does not, and the
/// hull is outside the station's hull — every tile of it.
#[test]
fn the_ship_docks_airlock_to_airlock_outside_the_station() {
    let world = basic();
    let ShipState::Docked { station } = world.ship.state else {
        panic!("not docked");
    };
    let station = world.station(station).unwrap();
    let tile = shipdesign::TILE as f64;

    // The ship's door, in the system.
    let port = shipdesign::port(&world.ship.design).unwrap();
    let (fx, fy) = port.face();
    let ship_face = world.ship.anchor.add(flight::angle::rotate_design(
        dvec2(fx, fy),
        world.ship.heading,
    ));
    let station_port = station.port().unwrap();
    let (sx, sy) = station_port.face();
    let station_face = station.to_system(dvec2(sx, sy));
    assert!(
        ship_face.distance(station_face) < 1e-6,
        "the doors are {} apart",
        ship_face.distance(station_face)
    );
    // The ship's door opens towards the station: its outward step, turned
    // through the heading, is the station's outward step reversed.
    let ship_out = flight::angle::rotate_design(
        dvec2(port.outward.0 as f64, port.outward.1 as f64),
        world.ship.heading,
    );
    let station_out = flight::angle::rotate_design(
        dvec2(station_port.outward.0 as f64, station_port.outward.1 as f64),
        0.0,
    );
    assert!(
        ship_out.add(station_out).length() < 1e-9,
        "the doors do not face each other"
    );

    // No tile of the ship's frame is over a tile of the station's.
    let grid = station.design.grid();
    for part in &world.ship.design.parts {
        for (x, y) in part.tiles() {
            let middle = dvec2((x as f64 + 0.5) * tile, (y as f64 + 0.5) * tile);
            let at = world
                .ship
                .anchor
                .add(flight::angle::rotate_design(middle, world.ship.heading));
            let on_station = flight::angle::unrotate_design(at.sub(station.anchor), 0.0);
            let t = (
                (on_station.x / tile).floor() as i32,
                (on_station.y / tile).floor() as i32,
            );
            assert_eq!(
                grid.get(shipdesign::Layer::Structure, t),
                0,
                "ship tile ({x}, {y}) is over station tile {t:?}"
            );
        }
    }
}

/// A trip to a station ends at its berth, alongside, with the doors mated —
/// the same arithmetic as the spawn, reached by flying rather than by being
/// put there.
#[test]
fn arriving_at_a_station_docks_at_its_berth() {
    let mut world = basic();
    let here = match world.ship.state {
        ShipState::Docked { station } => station,
        _ => panic!(),
    };
    let there = world.stations.iter().map(|s| s.id).find(|&id| id != here);
    let Some(there) = there else {
        return; // a system with one station has nowhere else to dock
    };
    // Stand the ship near the other station rather than flying for days.
    world.undock_for_probe();
    let berth = world.berth_at(there).unwrap();
    world.put_for_probe(berth.position.add(dvec2(40_000.0, 10_000.0)));
    set_off(&mut world, 0, Target::Station(there));
    let ShipState::Travelling { plan, .. } = &world.ship.state else {
        panic!()
    };
    assert!(plan.docks);
    let arrival = plan.arrival;

    // The trip ends short of the berth, and the ship then comes alongside:
    // sliding and turning onto the berth over `DOCK_MINUTES`, never
    // jumping, and tied up at the end of it.
    let mut docking = false;
    let mut began = None;
    let mut last = None;
    for _ in 0..400_000 {
        let events = world.step(&[]);
        if events
            .iter()
            .any(|e| matches!(e, WorldEvent::Docking { station } if *station == there))
        {
            docking = true;
            assert!(world.ship.position().distance(arrival) < 1.0);
            began = Some(world.clock_minutes);
        }
        if let ShipState::Docking { .. } = world.ship.state {
            if let Some(was) = last {
                let moved = world.ship.position().distance(was);
                assert!(moved < 200.0, "jumped {moved} in one step");
            }
            last = Some(world.ship.position());
        }
        if !matches!(
            world.ship.state,
            ShipState::Travelling { .. } | ShipState::Docking { .. }
        ) {
            break;
        }
    }
    assert!(docking, "never came alongside");
    let took = world.clock_minutes - began.unwrap();
    assert!(
        took >= data::DOCK_MINUTES && took < data::DOCK_MINUTES + 2.0 * data::STEP_MINUTES,
        "took {took} to come alongside"
    );
    assert_eq!(world.ship.state, ShipState::Docked { station: there });
    let berth = world.berth_at(there).unwrap();
    assert!(world.ship.position().distance(berth.position) < 1e-6);
    assert!((world.ship.heading - berth.heading).abs() < 1e-9);
    assert!(world.aboard.is_joined(), "tied up, but the rooms are apart");
}

/// The people on a station are simulated only while the ship is near: within
/// fifty tiles of the hull they are there, further out they are not, and a
/// derelict has nobody at any range.
#[test]
fn residents_are_there_within_fifty_tiles_and_not_beyond() {
    let mut world = basic();
    let lived_in = world
        .stations
        .iter()
        .find(|s| s.residents() > 0)
        .map(|s| (s.id, s.centre(), s.radius(), s.residents()))
        .expect("the spawn system has a station somebody lives on");
    let (id, centre, radius, count) = lived_in;
    // Docked, the station is in the ship's room; this is about the range, so
    // the ship is undocked first.
    world.undock_for_probe();

    // Far away: nobody.
    world.put_for_probe(centre.add(dvec2(radius + data::RESIDENTS_RANGE * 3.0, 0.0)));
    world.step(&[]);
    assert!(
        world.residents.is_none(),
        "residents at three times the range"
    );
    // Docked, the residents were in the ship's room from the first step:
    // the spawn is a station somebody lives on.
    assert_eq!(basic().aboard.resident_count(), count);

    // Inside the range: the room opens, with the residents at their bunks
    // and the day the world's.
    world.put_for_probe(centre.add(dvec2(radius + data::RESIDENTS_RANGE * 0.5, 0.0)));
    world.step(&[]);
    let residents = world.residents.as_ref().expect("residents within range");
    assert_eq!(residents.station, id);
    assert_eq!(residents.aboard.count(), count);
    // Between the ranges: kept.
    world.put_for_probe(centre.add(dvec2(radius + data::RESIDENTS_RANGE * 1.1, 0.0)));
    world.step(&[]);
    assert!(world.residents.is_some(), "dropped inside the hysteresis");
    // And they live: a resident goes somewhere in a couple of hours, and
    // never off the station's deck.
    let grid = world.station(id).unwrap().design.grid();
    let tile = shipdesign::TILE as f64;
    let start = world.residents.as_ref().unwrap().aboard.position(0);
    let mut moved = false;
    for _ in 0..(2 * 60 * 60) {
        world.step(&[]);
        let residents = world.residents.as_ref().unwrap();
        for who in 0..residents.aboard.count() {
            let at = residents.aboard.position(who);
            let t = ((at.x / tile).floor() as i32, (at.y / tile).floor() as i32);
            assert_ne!(
                grid.get(shipdesign::Layer::Floor, t),
                0,
                "resident {who} off the deck at {t:?}"
            );
        }
        if residents.aboard.position(0).distance(start) > tile {
            moved = true;
        }
    }
    assert!(moved, "the residents never went anywhere");

    // Past the way out: gone.
    world.put_for_probe(centre.add(dvec2(radius + data::RESIDENTS_RANGE * 1.5, 0.0)));
    world.step(&[]);
    assert!(world.residents.is_none(), "still there past the hysteresis");

    // A derelict has nobody, however close — but its room opens all the
    // same, because the room is what draws the fixtures, and it runs.
    if let Some(derelict) = world
        .stations
        .iter()
        .find(|s| s.kind == worldgen::StationKind::Derelict)
        .map(|s| s.centre())
    {
        world.put_for_probe(derelict);
        world.step(&[]);
        let room = world
            .residents
            .as_ref()
            .expect("the derelict's room is open");
        assert_eq!(room.aboard.count(), 0, "somebody lives on the derelict");
        for _ in 0..600 {
            world.step(&[]);
        }
        assert!(world.residents.is_some());
    }
}

// --- one room while docked -----------------------------------------------------

/// Docked, the ship and the station are one room: everybody is in it — the
/// crew first, then the residents — on one deck, and the crew can walk
/// through the airlocks onto the station. Under way again, the ship's room
/// is the ship's alone and anybody of the crew who was over there is back
/// aboard.
#[test]
fn docked_the_ship_and_the_station_are_one_room_and_the_crew_can_cross() {
    let mut world = basic();
    let ShipState::Docked { station } = world.ship.state else {
        panic!("not docked");
    };
    let residents = world.station(station).unwrap().residents();
    assert!(residents > 0, "the spawn station has nobody to meet");
    assert!(world.aboard.is_joined(), "the rooms were not joined");
    assert_eq!(world.aboard.crew_count(), 2);
    assert_eq!(world.aboard.resident_count(), residents);
    assert_eq!(world.aboard.count(), 2 + residents);
    for who in 0..world.aboard.count() {
        assert!(world.aboard.on_deck(who), "{who} is not on the deck");
    }
    // The station keeps a room for its pictures, with nobody in it.
    assert_eq!(world.residents.as_ref().map(|r| r.aboard.count()), Some(0));

    // Somewhere on the station's deck: a floor tile of the joined design
    // that is not inside the ship's own square, well away from the join.
    let tile = shipdesign::TILE as f64;
    let ship_side = world.ship.design.build_area as f64 * tile;
    let offset = world.aboard.offset;
    let on_ship = |p: DVec2| {
        p.x >= offset.x
            && p.y >= offset.y
            && p.x < offset.x + ship_side
            && p.y < offset.y + ship_side
    };
    let there = world
        .aboard
        .design
        .parts
        .iter()
        .filter(|p| p.kind == PartKind::Floor)
        .map(|p| {
            dvec2(
                (p.origin.0 as f64 + 0.5) * tile,
                (p.origin.1 as f64 + 0.5) * tile,
            )
        })
        .filter(|&p| !on_ship(p))
        .max_by(|a, b| a.x.partial_cmp(&b.x).unwrap())
        .expect("the station has deck");
    let sent = world
        .aboard
        .room
        .send_for_probe(0, bims::math::vec2(there.x as f32, there.y as f32));
    assert!(sent, "no route from the ship onto the station");
    let mut arrived = false;
    for _ in 0..(20 * 60 * 60) {
        world.step(&[]);
        let p = world.aboard.room.bim_pos(0);
        if !world.aboard.room.is_walking(0) && (p.x as f64 - there.x).abs() < 2.0 * tile {
            arrived = true;
            break;
        }
    }
    assert!(arrived, "the crew member never got across");
    let p = world.aboard.room.bim_pos(0);
    assert!(
        !on_ship(dvec2(p.x as f64, p.y as f64)),
        "arrived, but still on the ship"
    );

    // Leaving: whoever was over there walks back aboard — the ship waits
    // for them — and then it is one room again with the crew in it.
    let target = nearby(&world, 40_000.0);
    world.man_the_helm_for_probe(1);
    world.step(&[Command::Confirm { slot: 1, target }]);
    assert!(world.aboard.is_joined(), "left without them");
    let mut back = false;
    for _ in 0..LEAVING {
        world.step(&[]);
        if !world.aboard.is_joined() {
            break;
        }
        let p = world.aboard.room.bim_pos(0);
        back = back || on_ship(dvec2(p.x as f64, p.y as f64));
    }
    assert!(back, "the crew member was never seen back on the ship");
    assert!(!world.aboard.is_joined());
    assert_eq!(world.aboard.count(), 2);
    assert_eq!(world.aboard.offset, DVec2::ZERO);
    for who in 0..2 {
        assert!(world.aboard.on_deck(who), "{who} is off the ship");
    }
    // And the station's room reopens with its people, since the ship is
    // still within range.
    world.step(&[]);
    assert_eq!(
        world.residents.as_ref().map(|r| r.aboard.count()),
        Some(residents)
    );
}

/// A ship whose airlock is in its bow docks turned a quarter, and the
/// station is turned the other way into the ship's frame: every part of
/// both comes across, plus the two decked tiles of the passage, and the
/// station's door lands beyond the ship's.
#[test]
fn a_station_is_turned_into_the_frame_of_a_ship_docked_side_on() {
    use shipdesign::{Budget, Edit, Rotation, apply};
    let budget = Budget::new(Money::MAX);
    let mut design = flyer(2);
    // The starboard airlock off, and one across the bow instead: two tiles
    // of the north skin peeled, decked, and the airlock turned to lie along
    // it.
    let starboard = shipdesign::port(&design).unwrap().part_id;
    design = apply(&design, &budget, Edit::Remove { part_id: starboard }).unwrap();
    for x in [11u32, 12] {
        let wall = design.grid().get(shipdesign::Layer::Object, (x as i32, 1));
        design = apply(&design, &budget, Edit::Remove { part_id: wall }).unwrap();
        design = apply(
            &design,
            &budget,
            Edit::Place {
                kind: PartKind::Floor,
                origin: (x, 1),
                rotation: Rotation::R0,
            },
        )
        .unwrap();
    }
    design = apply(
        &design,
        &budget,
        Edit::Place {
            kind: PartKind::Airlock,
            origin: (11, 1),
            rotation: Rotation::R90,
        },
    )
    .unwrap();
    let port = shipdesign::port(&design).expect("a port in the bow");
    assert_eq!(port.outward, (0, -1));

    let world = world_with(design.clone(), REFERENCE_MONEY, 2);
    let ShipState::Docked { station } = world.ship.state else {
        panic!("not docked");
    };
    assert!(
        (world.ship.heading.abs() - std::f64::consts::FRAC_PI_2).abs() < 1e-9,
        "docked at {} rather than a quarter turn",
        world.ship.heading
    );
    let station = world.station(station).unwrap();
    let joined = &world.aboard.design;
    assert_eq!(
        joined.parts.len(),
        design.parts.len() + station.design.parts.len() + 4,
        "not every part came across"
    );
    // The passage is floor, and it is where the collars meet.
    let grid = joined.grid();
    let shift = world.aboard.offset;
    let t = shipdesign::TILE as f64;
    for (x, y) in design.part(port.part_id).unwrap().tiles() {
        let gap = (
            (x as f64 + (shift.x / t)) as i32 + port.outward.0,
            (y as f64 + (shift.y / t)) as i32 + port.outward.1,
        );
        assert_ne!(
            grid.get(shipdesign::Layer::Floor, gap),
            0,
            "no deck at {gap:?}"
        );
    }
    assert!(world.aboard.is_joined());
    for who in 0..world.aboard.count() {
        assert!(world.aboard.on_deck(who), "{who} is off the deck");
    }
}

/// `spawn_anywhere` is what `nix run .#test` lands on: some dock somebody
/// lives on, anywhere in the galaxy, different rolls giving different
/// places and the same roll the same place — and never a derelict.
#[test]
fn a_roll_picks_a_lived_in_dock_anywhere_in_the_galaxy() {
    use crate::station::residents_of;
    let galaxy = worldgen::Galaxy::new(data::DEFAULT_SEED, GalaxyType::SpiralTwoArm);
    let mut seen = std::collections::BTreeSet::new();
    for roll in 0..12u64 {
        let (star, station) =
            crate::spawn_anywhere(&galaxy, roll).expect("somebody lives somewhere");
        assert_eq!(crate::spawn_anywhere(&galaxy, roll), Some((star, station)));
        let system = galaxy.system(star).unwrap();
        let blueprint = system.station(station).unwrap();
        assert!(
            residents_of(blueprint.kind) > 0,
            "roll {roll} landed on a {:?}",
            blueprint.kind
        );
        seen.insert((star, station));
    }
    assert!(seen.len() > 1, "every roll landed on the same dock");
    // Roll 0 is the first lived-in dock, which is where the simulation starts.
    assert_eq!(crate::spawn_anywhere(&galaxy, 0), crate::spawn(&galaxy));
    // And a world opens there, docked, with the residents in the room.
    let (star, station) = crate::spawn_anywhere(&galaxy, 5).unwrap();
    let world = World::start(
        flyer(2),
        REFERENCE_MONEY,
        2,
        data::DEFAULT_SEED,
        GalaxyType::SpiralTwoArm,
        star,
        station,
    )
    .expect("a world opens at the picked dock");
    assert_eq!(world.ship.state, ShipState::Docked { station });
    assert!(world.aboard.resident_count() > 0);
}

/// A ship's doors are powered: they open for whoever walks up, and the
/// pathfinder plans straight through them. Locked, one is a wall — the
/// bridge is behind a two-tile doorway on the playtest ship, and with both
/// tiles locked there is no route to the helm; James works the panel by
/// hand, as with the bathroom door, and the lock is undone the same way.
#[test]
fn a_locked_door_is_a_wall_and_an_unlocked_one_is_not() {
    use shipdesign::fixture::playtest_ship;
    let mut world = simulation_world(playtest_ship(), data::SIMULATION_MONEY, 1);
    let tile = shipdesign::TILE as f32;
    let offset = world.aboard.offset;
    assert!(
        world.aboard.room.ship_door_count() >= 2,
        "the playtest ship has two doors, one a doorway"
    );
    // The pilot's spot, in the joined room's coordinates.
    let helm = bims::math::vec2(offset.x as f32 + 9.5 * tile, offset.y as f32 + 4.5 * tile);
    world.aboard.room.recruit_for_probe(0, true);
    assert!(
        world.aboard.room.send_for_probe(0, helm),
        "no route to the helm to begin with"
    );
    // Door 0 is the bridge bulkhead's, the first the design lays, and it is
    // the whole two-tile doorway: locked, the bridge is sealed.
    assert!(!world.aboard.room.ship_door_is_locked(0));
    world.aboard.room.order_door(0, 0, bims::door::Order::Lock);
    let mut budget = 60 * 60 * 5;
    while budget > 0 && !world.aboard.room.ship_door_is_locked(0) {
        world.step(&[]);
        budget -= 1;
    }
    assert!(
        world.aboard.room.ship_door_is_locked(0),
        "the door never got locked"
    );
    let mut budget = 60 * 5;
    while budget > 0 && world.aboard.room.is_walking(0) {
        world.step(&[]);
        budget -= 1;
    }
    assert!(
        !world.aboard.room.send_for_probe(0, helm),
        "a route through a locked door"
    );
    world
        .aboard
        .room
        .order_door(0, 0, bims::door::Order::Unlock);
    let mut budget = 60 * 60 * 5;
    while budget > 0 && world.aboard.room.ship_door_is_locked(0) {
        world.step(&[]);
        budget -= 1;
    }
    assert!(
        !world.aboard.room.ship_door_is_locked(0),
        "the door never got unlocked"
    );
    assert!(
        world.aboard.room.send_for_probe(0, helm),
        "no route through an unlocked door"
    );
}

/// The bay aboard is six trays, one a tile along the run, worked from the
/// side its use spots are on: a tray a tile across for a bay lying
/// east–west and a tray a tile down for one standing north–south. The
/// side used to be read off the part's centre, and the first spot — at the
/// *end* of the run — then read as off the end rather than off the side,
/// which laid the trays across the bay in six strips.
#[test]
fn the_bay_aboard_is_a_tray_a_tile_worked_from_the_spots_side() {
    use bims::aboard::layout_of;
    use bims::hydro::Bay;
    use shipdesign::fixture::playtest_ship;
    use shipdesign::parts::TILE;
    let tile = TILE as f32;

    // The playtest ship's bay lies east–west and is worked from the north.
    let layout = layout_of(&playtest_ship());
    assert_eq!((layout.bay_side.x, layout.bay_side.y), (0.0, -1.0));
    let bay = Bay::at(layout.bay, layout.bay_side);
    for i in 0..6 {
        let stand = bay.station(i);
        let want = layout.bay.min.x + (i as f32 + 0.5) * tile;
        assert!(
            (stand.x - want).abs() < tile / 3.0 && stand.y < layout.bay.min.y,
            "tray {i} is worked from {stand:?}, not a tile along from the north"
        );
    }

    // One standing north–south, worked from the east: the same spots turned.
    let budget = Budget::new(1_000_000);
    let mut design = ShipDesign::new(12);
    for y in 1..11 {
        for x in 1..11 {
            for kind in [PartKind::Structure, PartKind::Floor] {
                design = apply(
                    &design,
                    &budget,
                    Edit::Place {
                        kind,
                        origin: (x, y),
                        rotation: Rotation::R0,
                    },
                )
                .unwrap();
            }
        }
    }
    design = apply(
        &design,
        &budget,
        Edit::Place {
            kind: PartKind::HydroBay,
            origin: (2, 2),
            rotation: Rotation::R90,
        },
    )
    .expect("a turned bay");
    let layout = layout_of(&design);
    assert_eq!((layout.bay_side.x, layout.bay_side.y), (1.0, 0.0));
    let bay = Bay::at(layout.bay, layout.bay_side);
    for i in 0..6 {
        let stand = bay.station(i);
        let want = layout.bay.min.y + (i as f32 + 0.5) * tile;
        assert!(
            (stand.y - want).abs() < tile / 3.0 && stand.x > layout.bay.max.x,
            "tray {i} is worked from {stand:?}, not a tile down from the east"
        );
    }
}

/// Not a test: a picture of a station's navigation grid, for when
/// `a_station_s_rooms_can_all_be_walked_from_its_door` says a tile is cut
/// off and the layout looks fine. Every deck tile is a digit for how much
/// of it a body can stand on, `.` for all and `x` for none, and every part
/// is its first letter. `cargo test -p world nav_map -- --ignored
/// --nocapture`; the kind and seed are the two lines below.
#[test]
#[ignore]
fn nav_map_of_a_station() {
    let (kind, seed) = (worldgen::StationKind::Relay, 1u64);
    let design = crate::station::layout(kind, seed);
    let tile = shipdesign::TILE as f32;
    let room = bims::room::Room::from_layout(bims::aboard::layout_of(&design));
    let nav = bims::nav::Nav::tiled(
        room.interior,
        &room.solids(),
        bims::character::BODY_MARGIN,
        tile,
    );
    let grid = design.grid();
    for y in 0..design.build_area as i32 {
        let row: String = (0..design.build_area as i32)
            .map(|x| {
                let object = grid.get(shipdesign::Layer::Object, (x, y));
                if object != 0 {
                    let kind = design.part(object).unwrap().kind;
                    return format!("{kind:?}").chars().next().unwrap();
                }
                if grid.get(shipdesign::Layer::Floor, (x, y)) == 0 {
                    return ' ';
                }
                let (mut free, mut all) = (0u32, 0u32);
                for i in 0..5 {
                    for j in 0..5 {
                        let p = bims::math::vec2(
                            x as f32 * tile + 6.0 + i as f32 * 10.0,
                            y as f32 * tile + 6.0 + j as f32 * 10.0,
                        );
                        all += 1;
                        free += nav.is_free(p) as u32;
                    }
                }
                match free {
                    0 => 'x',
                    f if f == all => '.',
                    f => char::from_digit(f * 9 / all, 10).unwrap(),
                }
            })
            .collect();
        println!("{y:2} {row}");
    }
}

// --- power -------------------------------------------------------------------

/// The flyer has no battery, so its charge is nought and stays there; it
/// makes more than it draws, so nothing is browned out. Put a battery on a
/// wired tile and it arrives empty and fills at the surplus; put enough
/// on the run to draw more than the reactor makes and it drains at the
/// deficit, and when it is flat the bay stops and life support does not.
/// Every figure is closed form off the step count, which is what a server
/// catching up relies on.
#[test]
fn the_batteries_fill_at_the_surplus_drain_at_the_deficit_and_flat_is_a_brownout() {
    use shipdesign::{BATTERY_CHARGE, REACTOR_OUTPUT};
    let budget = Budget::new(10_000_000);
    let mut world = basic();
    let power = world.power();
    assert_eq!(power.supply, REACTOR_OUTPUT);
    // The flyer's cold store, helm, bay and array.
    assert_eq!(power.draw, 35.0);
    assert_eq!(power.storage, 0.0);
    assert_eq!(power.charge, 0.0);
    assert!(!power.brownout());
    assert!(world.powered(PartKind::HydroBay));
    assert!(
        !world.powered(PartKind::LifeSupport),
        "the flyer has none, so none is running"
    );
    world.step(&[]);
    assert_eq!(world.power().charge, 0.0);

    // A battery on the spine, which runs down column 8: it arrives empty.
    let with_battery = apply(
        &world.ship.design,
        &budget,
        Edit::Place {
            kind: PartKind::Battery,
            origin: (8, 7),
            rotation: Rotation::R0,
        },
    )
    .expect("a battery on the spine");
    world.ship.design = with_battery;
    world.on_ship_changed();
    assert_eq!(world.power().storage, BATTERY_CHARGE);
    assert_eq!(world.power().charge, 0.0);

    // The surplus — the reactor less the flyer's thirty-five — for ten
    // steps.
    let surplus = REACTOR_OUTPUT - 35.0;
    for _ in 0..10 {
        world.step(&[]);
    }
    let expected = surplus * 10.0 * data::STEP_MINUTES;
    assert!(
        close(world.power().charge, expected),
        "{} vs {expected}",
        world.power().charge
    );

    // Run it full, and it stops at full.
    let steps_to_full = (BATTERY_CHARGE / (surplus * data::STEP_MINUTES)).ceil() as u32 + 5;
    for _ in 0..steps_to_full {
        world.step(&[]);
    }
    assert_eq!(world.power().charge, BATTERY_CHARGE);

    // A world opened on this design opens full, as one docked would be.
    let fresh = world_with(world.ship.design.clone(), REFERENCE_MONEY, 2);
    assert_eq!(fresh.power().charge, BATTERY_CHARGE);

    // Four life supports along the spine and a second array on it draw
    // ninety more: a hundred and twenty-five against the reactor, and the
    // battery drains at the difference.
    let mut design = world.ship.design.clone();
    design = apply(
        &design,
        &budget,
        Edit::Place {
            kind: PartKind::SensorArray,
            origin: (9, 13),
            rotation: Rotation::R0,
        },
    )
    .expect("an array on the spine");
    for y in [4u32, 8, 11, 13] {
        design = apply(
            &design,
            &budget,
            Edit::Place {
                kind: PartKind::LifeSupport,
                origin: (7, y),
                rotation: Rotation::R0,
            },
        )
        .unwrap_or_else(|e| panic!("life support at (7, {y}): {e:?}"));
    }
    world.ship.design = design;
    world.on_ship_changed();
    assert_eq!(world.power().draw, 125.0);
    let deficit = 125.0 - REACTOR_OUTPUT;
    assert!(deficit > 0.0);
    assert!(!world.power().brownout());
    assert!(world.powered(PartKind::HydroBay));
    for _ in 0..10 {
        world.step(&[]);
    }
    let expected = BATTERY_CHARGE - deficit * 10.0 * data::STEP_MINUTES;
    assert!(
        close(world.power().charge, expected),
        "{} vs {expected}",
        world.power().charge
    );

    // Flat: the bay and the cold store stop, life support carries on. The
    // last of the charge is skipped rather than drained — at five a minute
    // that is twelve game hours — and three steps' worth is left to go.
    world.ship.charge = deficit * data::STEP_MINUTES * 3.0;
    for _ in 0..10 {
        world.step(&[]);
    }
    assert_eq!(world.power().charge, 0.0);
    assert!(world.power().brownout());
    assert!(!world.powered(PartKind::HydroBay));
    assert!(!world.powered(PartKind::ColdStore));
    assert!(world.powered(PartKind::LifeSupport));
    assert!(
        world.powered(PartKind::Table),
        "a table draws nothing and is always running"
    );

    // Taking the battery off takes what is in it — here nothing — and the
    // charge cannot exceed what is left to hold it.
    let battery = world
        .ship
        .design
        .parts
        .iter()
        .find(|p| p.kind == PartKind::Battery)
        .unwrap()
        .id;
    world.ship.charge = 100.0;
    world.ship.design = apply(
        &world.ship.design,
        &budget,
        Edit::Remove { part_id: battery },
    )
    .unwrap();
    world.on_ship_changed();
    assert_eq!(world.power().charge, 0.0);
}

/// A consumer standing off the run is not running, brownout or no.
#[test]
fn a_consumer_off_the_run_is_not_running() {
    let budget = Budget::new(10_000_000);
    let mut world = basic();
    // Life support on bare deck aft to port, nowhere near the conduit.
    world.ship.design = apply(
        &world.ship.design,
        &budget,
        Edit::Place {
            kind: PartKind::LifeSupport,
            origin: (4, 14),
            rotation: Rotation::R0,
        },
    )
    .expect("life support on bare deck");
    world.on_ship_changed();
    // The only life support aboard is off the run, so none is running.
    assert!(!world.powered(PartKind::LifeSupport));
    assert!(world.powered(PartKind::HydroBay));
    assert_eq!(
        world.power().draw,
        35.0,
        "an unwired consumer draws nothing"
    );
}

// --- making things -----------------------------------------------------------

/// The whole seam, on the playtest ship: a target for metal, ore aboard,
/// the smelter wired — the room is handed an order, a Bim walks to the
/// bench and stands at it for the recipe's length, and the world moves two
/// ore out of the hold and one metal in, with the mass moving with them.
/// Then the target is met and nothing more is made.
#[test]
fn a_target_for_metal_has_a_bim_smelt_ore_at_the_bench() {
    use bims::game::JOB_CRAFT;
    use shipdesign::fixture::playtest_ship;
    let mut world = simulation_world(playtest_ship(), data::SIMULATION_MONEY, 1);
    let ore = world.ship.design.carrying(ResourceId::Ore);
    let metal = world.ship.design.carrying(ResourceId::Metal);
    assert!(ore >= 2);
    assert_eq!(world.craft_target(ResourceId::Metal), 0);
    assert!(world.craft_orders().is_empty(), "nothing is asked for yet");
    assert_eq!(world.aboard.room.benches().len(), 2);
    assert!(world.powered(PartKind::Smelter));

    // One more metal than there is: one smelt's worth.
    world.set_craft_target(ResourceId::Metal, metal + 1);
    let orders = world.craft_orders();
    assert_eq!(orders.len(), 1, "{}", orders.len());
    assert_eq!(orders[0].recipe, 0);
    assert_eq!(orders[0].minutes, 30.0);
    let mass_before = world.ship.dynamics.mass.get();

    // The recipe is half an hour at the bench plus the walk there; give it
    // the morning. The agenda shows the craft errand on the way.
    let mut crafted = false;
    let mut seen_at_bench = false;
    for _ in 0..(4 * 60 * 60) {
        let events = world.step(&[]);
        if world.aboard.room.agenda_len(0) > 0 && world.aboard.room.agenda_job(0, 0) == JOB_CRAFT {
            seen_at_bench = true;
        }
        if events
            .iter()
            .any(|e| matches!(e, WorldEvent::Crafted { recipe: 0 }))
        {
            crafted = true;
            break;
        }
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, WorldEvent::CraftLost { .. })),
            "the ore was there the whole time"
        );
    }
    assert!(crafted, "no metal was made in four hours");
    assert!(seen_at_bench, "the errand never showed on the agenda");
    assert_eq!(world.ship.design.carrying(ResourceId::Ore), ore - 2);
    assert_eq!(world.ship.design.carrying(ResourceId::Metal), metal + 1);
    // Two ore at ten is twenty; one metal is eight. The slag went.
    let mass_after = world.ship.dynamics.mass.get();
    assert!(
        close(mass_before - mass_after, 12.0),
        "mass went from {mass_before} to {mass_after}"
    );

    // Met: no order, and an hour later still one metal more.
    assert!(world.craft_orders().is_empty());
    for _ in 0..(60 * 60) {
        world.step(&[]);
    }
    assert_eq!(world.ship.design.carrying(ResourceId::Metal), metal + 1);
}

/// A recipe wants its inputs and its station's power. With no ore aboard
/// there is no order for metal however high the target; with the smelter
/// unpowered, none either; and an order for components at the workbench
/// is a different bench and is still made.
#[test]
fn an_order_wants_the_inputs_aboard_and_the_bench_powered() {
    use shipdesign::fixture::playtest_ship;
    let budget = Budget::new(10_000_000);
    let mut world = simulation_world(playtest_ship(), data::SIMULATION_MONEY, 1);
    let ore = world.ship.design.carrying(ResourceId::Ore);
    world.ship.design = apply(
        &world.ship.design,
        &budget,
        Edit::Sell {
            resource: ResourceId::Ore,
            units: ore,
        },
    )
    .unwrap();
    world.on_ship_changed();
    world.set_craft_target(ResourceId::Metal, 999);
    assert!(world.craft_orders().is_empty(), "no ore, no smelting");

    // Components out of metal want only the workbench, which has power.
    let components = world.ship.design.carrying(ResourceId::Components);
    world.set_craft_target(ResourceId::Components, components + 4);
    let orders = world.craft_orders();
    assert_eq!(orders.len(), 1);
    assert_eq!(orders[0].recipe, 1);

    // Take the reactor off: nothing is powered, nothing is on offer.
    let reactor = world
        .ship
        .design
        .parts
        .iter()
        .find(|p| p.kind == PartKind::Reactor)
        .unwrap()
        .id;
    world.ship.design = apply(
        &world.ship.design,
        &budget,
        Edit::Remove { part_id: reactor },
    )
    .unwrap();
    world.on_ship_changed();
    assert!(!world.powered(PartKind::Workbench));
    assert!(world.craft_orders().is_empty(), "no power, no work");

    // The target is clamped to what the shelf could hold.
    world.set_craft_target(ResourceId::Components, 10_000);
    assert_eq!(
        world.craft_target(ResourceId::Components),
        world.ship.design.capacity(Storage::Shelf)
    );
}

// --- a walk outside ----------------------------------------------------------

/// A belt of the spawn system, or of a system that has one: what a walk
/// outside is measured at. The simulation's spawn system is whatever the
/// generator made it, so this looks rather than assumes.
fn a_belt(world: &World) -> Option<worldgen::Body> {
    world
        .system
        .bodies
        .iter()
        .find(|b| b.kind == worldgen::BodyKind::AsteroidBelt)
        .cloned()
}

/// The whole seam: holding at a belt with a suit in the locker, the room is
/// told a walk is on, a Bim takes the suit, goes out through the airlock —
/// off the deck, in the suit, dosed while it is out — comes back in with the
/// belt's ore on the shelf, and the dose comes off again. Docked, or with
/// no suit, no walk is on at all.
#[test]
fn a_walk_outside_at_a_belt_brings_ore_back_and_a_dose_with_it() {
    use bims::game::JOB_EVA;
    use shipdesign::fixture::playtest_ship;
    let mut world = simulation_world(playtest_ship(), data::SIMULATION_MONEY, 1);
    assert_eq!(world.ship.design.carrying(ResourceId::Suit), 1);
    assert!(world.aboard.room.is_outside(0) == false);
    assert!(world.eva_offer().is_none(), "docked is not at a belt");

    // Out to a belt of this system, or give up on the scenario honestly.
    let Some(belt) = a_belt(&world) else {
        eprintln!("the spawn system has no belt; nothing to walk out to");
        return;
    };
    world.undock_for_probe();
    world.put_for_probe(belt.position);
    world.settle_frame_for_probe();
    assert_eq!(
        world.ship.frame,
        Frame::Local(worldgen::Node::Body(belt.id))
    );
    let offer = world.eva_offer().expect("a walk is on at a belt");
    assert_eq!(offer.minutes, data::EVA_MINUTES as f32);
    assert_eq!(offer.allowed, vec![true]);

    // Sell the suit and there is no walk; buy it back — not docked, so by
    // hand — and there is.
    world.ship.design.cargo[ResourceId::Suit as usize] = 0;
    assert!(world.eva_offer().is_none(), "no suit, no walk");
    world.ship.design.cargo[ResourceId::Suit as usize] = 1;

    let ore = world.ship.design.carrying(ResourceId::Ore);
    let galvum = world.ship.design.carrying(ResourceId::Galvum);
    let yield_ = worldgen::belt_yield(world.galaxy_seed, world.star_id, belt.id, belt.kind);
    assert!(yield_.ore > 0);

    let mut went_out = false;
    let mut peak_dose = 0.0f64;
    let mut mined = None;
    let mut on_agenda = false;
    // The walk is an hour and a half plus the errand either side; give it
    // the afternoon.
    for _ in 0..(5 * 60 * 60) {
        let events = world.step(&[]);
        if world.aboard.room.agenda_len(0) > 0 && world.aboard.room.agenda_job(0, 0) == JOB_EVA {
            on_agenda = true;
        }
        if world.aboard.room.is_outside(0) {
            went_out = true;
            assert!(!world.aboard.on_deck(0), "outside is off the deck");
        }
        peak_dose = peak_dose.max(world.health[0].dose);
        if let Some(WorldEvent::Mined { ore, galvum }) = events
            .iter()
            .find(|e| matches!(e, WorldEvent::Mined { .. }))
        {
            mined = Some((*ore, *galvum));
            break;
        }
    }
    assert!(on_agenda, "the walk never showed on the agenda");
    assert!(went_out, "the Bim never went outside");
    let (got_ore, got_galvum) = mined.expect("no walk finished in five hours");
    assert_eq!(got_ore, yield_.ore);
    assert_eq!(got_galvum, u32::from(yield_.galvum));
    assert_eq!(
        world.ship.design.carrying(ResourceId::Ore),
        ore + yield_.ore
    );
    assert_eq!(
        world.ship.design.carrying(ResourceId::Galvum),
        galvum + u32::from(yield_.galvum)
    );
    // Dosed out there — about a quarter of the open-air rate for the walk —
    // and never near the critical line.
    let expected = data::EVA_MINUTES * data::SUIT_INTENSITY;
    assert!(
        peak_dose > expected * 0.9 && peak_dose < health::CRITICAL,
        "peak dose {peak_dose}, expected about {expected}"
    );
    // Back in: on the deck, in the coverall, and the dose coming off. The
    // suit goes so the Bim is not straight back out — its dose is well
    // under the limit and it would be.
    assert!(!world.aboard.room.is_outside(0));
    world.ship.design.cargo[ResourceId::Suit as usize] = 0;
    let dose_in = world.health[0].dose;
    for _ in 0..(60 * 60) {
        world.step(&[]);
        assert!(!world.aboard.room.is_outside(0));
    }
    assert!(
        world.health[0].dose < dose_in,
        "the dose should come off inside"
    );
    assert!(!world.health[0].dead);
}

/// A Bim past the dose limit is not sent out; below it, it is.
#[test]
fn a_dosed_bim_is_kept_in() {
    use shipdesign::fixture::playtest_ship;
    let mut world = simulation_world(playtest_ship(), data::SIMULATION_MONEY, 1);
    let Some(belt) = a_belt(&world) else {
        return;
    };
    world.undock_for_probe();
    world.put_for_probe(belt.position);
    world.settle_frame_for_probe();
    world.health[0].dose = data::EVA_DOSE_LIMIT + 1.0;
    assert_eq!(world.eva_offer().unwrap().allowed, vec![false]);
    world.health[0].dose = data::EVA_DOSE_LIMIT - 1.0;
    assert_eq!(world.eva_offer().unwrap().allowed, vec![true]);
}

/// The chain the whole thing was asked for: ore to metal, metal to
/// components, components and galvum to an emitter, and the emitter into a
/// laser handgun at the armoury. Set the targets and the benches do it in
/// order, because a handgun is not on offer until there is an emitter and
/// an emitter is not until there are components — each recipe waits on the
/// one before it without anybody sequencing them.
#[test]
fn a_target_for_a_handgun_runs_the_whole_chain_from_the_hold() {
    use shipdesign::fixture::playtest_ship;
    let budget = Budget::new(10_000_000);
    let mut world = simulation_world(playtest_ship(), data::SIMULATION_MONEY, 1);
    // An armoury amidships on a branch of its own off the spine.
    let mut design = world.ship.design.clone();
    for (kind, origin, rotation) in [
        (PartKind::PowerConduit, (9, 11), Rotation::R0),
        (PartKind::PowerConduit, (10, 11), Rotation::R0),
        (PartKind::PowerConduit, (11, 11), Rotation::R0),
        (PartKind::Armoury, (11, 11), Rotation::R0),
    ] {
        design = apply(
            &design,
            &budget,
            Edit::Place {
                kind,
                origin,
                rotation,
            },
        )
        .unwrap_or_else(|e| panic!("{kind:?} at {origin:?}: {e:?}"));
    }
    // One galvum, by hand: the spawn is not an outpost.
    design.cargo[ResourceId::Galvum as usize] = 1;
    world.ship.design = design;
    world.on_ship_changed();
    assert!(world.powered(PartKind::Armoury));
    assert_eq!(world.aboard.room.benches().len(), 2, "the room was laid out before");
    // The room is laid out again when the rooms come apart, so the armoury
    // is a bench once the ship lets go of the dock.
    world.undock_for_probe();
    assert_eq!(world.aboard.room.benches().len(), 3);

    world.set_craft_target(ResourceId::Emitter, 1);
    world.set_craft_target(ResourceId::Handgun, 1);
    // A handgun wants an emitter, so only the emitter is on offer.
    let orders = world.craft_orders();
    assert_eq!(orders.len(), 1, "{orders:?}");
    assert_eq!(orders[0].recipe, 2);

    let components = world.ship.design.carrying(ResourceId::Components);
    let metal = world.ship.design.carrying(ResourceId::Metal);
    let mut made = Vec::new();
    for _ in 0..(8 * 60 * 60) {
        let events = world.step(&[]);
        for e in &events {
            if let WorldEvent::Crafted { recipe } = e {
                made.push(*recipe);
            }
        }
        if world.ship.design.carrying(ResourceId::Handgun) >= 1 {
            break;
        }
    }
    assert_eq!(made, vec![2, 3], "the emitter and then the handgun");
    assert_eq!(world.ship.design.carrying(ResourceId::Handgun), 1);
    assert_eq!(world.ship.design.carrying(ResourceId::Emitter), 0, "used up");
    assert_eq!(world.ship.design.carrying(ResourceId::Galvum), 0);
    assert_eq!(world.ship.design.carrying(ResourceId::Components), components - 4);
    assert_eq!(world.ship.design.carrying(ResourceId::Metal), metal - 1);
    // A handgun goes in the locker class, beside the suit.
    assert_eq!(world.ship.design.stored(Storage::Locker), 2);
}
