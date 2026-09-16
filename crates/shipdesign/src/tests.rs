//! Native tests for the design rules.
//!
//! These run under `cargo test --target <native> -p shipdesign`, which is
//! what `nix flake check` does. The crate compiles for wasm as well, and
//! `crates/ship`'s `ship_self_check` export is the other half of the one
//! thing a native test cannot answer on its own: whether the two targets
//! hash a design the same way.

use economy::{Money, Storage, trade_price, trade_value};
use physics::{Facing, ResourceId};

use crate::budget::Budget;
use crate::design::{CARGO_SLOTS, Edit, EditError, ShipDesign, apply, design_hash};
use crate::fixture::{CREWS, REFERENCE_HASH, REFERENCE_PARTS, REFERENCE_POOL, reference};
use crate::mass::{acceleration, hull_mass, ship_mass};
use crate::materials::{bound_mass, bound_materials, build_from_cargo, deconstruct_to_cargo};
use crate::parts::{
    Layer, PartKind, Rotation, TILE, covered, defs_are_sound, footprint, part_mass, use_spots,
};
use crate::validate::{IssueCode, REQUIRED, Severity, exposure, has_errors, validate};

/// A budget with plenty in it, for the tests that are not about money.
fn rich() -> Budget {
    Budget::new(Money::MAX)
}

fn place(
    design: &ShipDesign,
    budget: &Budget,
    kind: PartKind,
    origin: (u32, u32),
    rotation: Rotation,
) -> Result<ShipDesign, EditError> {
    apply(
        design,
        budget,
        Edit::Place {
            kind,
            origin,
            rotation,
        },
    )
}

/// Place and insist it took. Used where the placement is scaffolding rather
/// than the thing under test.
fn put(design: ShipDesign, kind: PartKind, origin: (u32, u32)) -> ShipDesign {
    place(&design, &rich(), kind, origin, Rotation::R0)
        .unwrap_or_else(|e| panic!("{kind:?} at {origin:?} was refused: {e:?}"))
}

/// A square of frame, which everything needs under it.
fn framed(side: u32, from: (u32, u32), to: (u32, u32)) -> ShipDesign {
    let mut design = ShipDesign::new(side);
    for y in from.1..to.1 {
        for x in from.0..to.0 {
            design = put(design, PartKind::Structure, (x, y));
        }
    }
    design
}

/// A square of frame with deck on it, which almost everything else needs
/// under it in turn.
fn floored(side: u32, from: (u32, u32), to: (u32, u32)) -> ShipDesign {
    let mut design = framed(side, from, to);
    for y in from.1..to.1 {
        for x in from.0..to.0 {
            design = put(design, PartKind::Floor, (x, y));
        }
    }
    design
}

/// Buy, and insist it took.
fn bought(design: &ShipDesign, budget: &Budget, resource: ResourceId, units: u32) -> ShipDesign {
    apply(design, budget, Edit::Buy { resource, units })
        .unwrap_or_else(|e| panic!("{units} of {resource:?} was refused: {e:?}"))
}

/// Every issue code a design raises, whatever the severity, in the order
/// `validate` produced them.
fn all_codes(design: &ShipDesign, crew: u32) -> Vec<u32> {
    validate(design, crew).into_iter().map(|i| i.code).collect()
}

fn codes(design: &ShipDesign, crew: u32) -> Vec<u32> {
    let mut out: Vec<u32> = validate(design, crew)
        .into_iter()
        .filter(|i| i.severity == Severity::Error)
        .map(|i| i.code)
        .collect();
    out.sort_unstable();
    out
}

// --- the tables -----------------------------------------------------------

#[test]
fn the_part_table_holds_together() {
    assert!(defs_are_sound());
    assert_eq!(PartKind::Floor.def().kind, PartKind::Floor);
    assert_eq!(PartKind::BroomLocker.def().kind, PartKind::BroomLocker);
}

/// The table is indexed by discriminant, so an entry out of order is an
/// engine that weighs what a chair does. `defs_are_sound` checks it; this
/// says out loud what "in order" means.
#[test]
fn the_table_is_in_discriminant_order() {
    for (i, &kind) in PartKind::ALL.iter().enumerate() {
        assert_eq!(kind.code(), i as u32);
        assert_eq!(crate::parts::PARTS[i].kind, kind);
        assert_eq!(PartKind::from_code(i as u32), Some(kind));
    }
    assert_eq!(PartKind::from_code(PartKind::ALL.len() as u32), None);
}

#[test]
fn only_the_engine_pushes_and_everything_has_weight() {
    for &kind in PartKind::ALL.iter() {
        let def = kind.def();
        assert!(part_mass(kind) > 0.0, "{kind:?} weighs nothing");
        if kind == PartKind::Engine {
            assert!(def.thrust > 0.0, "the engine does not push");
        } else {
            assert_eq!(def.thrust, 0.0, "{kind:?} pushes the ship");
        }
    }
}

#[test]
fn the_floor_is_the_only_thing_on_the_floor_layer() {
    for &kind in PartKind::ALL.iter() {
        let floor = kind == PartKind::Floor;
        assert_eq!(kind.def().layer == Layer::Floor, floor, "{kind:?}");
        let frame = kind == PartKind::Structure;
        assert_eq!(kind.def().layer == Layer::Structure, frame, "{kind:?}");
    }
    assert_eq!(TILE, 52);
}

/// Which layer holds what up. Written out rather than derived, because the
/// whole of the stacking rule is this column of the table and a part that
/// needs the wrong thing is a part that can be built in mid-air.
#[test]
fn every_part_needs_what_it_is_meant_to_need() {
    // The frame needs nothing; it is what everything else stands on.
    assert_eq!(PartKind::Structure.def().requires, None);
    // Hull and frame-mounted things stand straight on the frame.
    for kind in [
        PartKind::Floor,
        PartKind::Wall,
        PartKind::OutsideWall,
        PartKind::SensorArray,
        PartKind::PowerConduit,
    ] {
        assert_eq!(
            kind.def().requires,
            Some(Layer::Structure),
            "{kind:?} should stand on the frame",
        );
    }
    // Everything else wants deck under it.
    for &kind in PartKind::ALL.iter() {
        let on_frame = matches!(
            kind,
            PartKind::Structure
                | PartKind::Floor
                | PartKind::Wall
                | PartKind::OutsideWall
                | PartKind::SensorArray
                | PartKind::PowerConduit
        );
        if !on_frame {
            assert_eq!(kind.def().requires, Some(Layer::Floor), "{kind:?}");
        }
    }
    assert_eq!(Layer::from_code(3), Some(Layer::Utility));
    assert_eq!(Layer::from_code(4), None);
}

/// What keeps the radiation out, and what holds goods. Both are written out
/// here as well as in the table: a part that quietly stopped shielding is a
/// crew being cooked with nothing on the page to say so.
#[test]
fn shielding_and_storage_are_where_they_are_meant_to_be() {
    let shielding: Vec<PartKind> = PartKind::ALL
        .iter()
        .copied()
        .filter(|k| k.def().shields)
        .collect();
    assert_eq!(
        shielding,
        vec![
            PartKind::Engine,
            PartKind::OutsideWall,
            PartKind::Airlock,
            PartKind::SensorArray,
        ],
    );

    let holding: Vec<(PartKind, (Storage, u32))> = PartKind::ALL
        .iter()
        .copied()
        .filter_map(|k| k.def().capacity.map(|c| (k, c)))
        .collect();
    assert_eq!(
        holding,
        vec![
            (PartKind::ColdStore, (Storage::ColdStore, 100)),
            (PartKind::FuelTank, (Storage::FuelTank, 200)),
            (PartKind::Shelf, (Storage::Shelf, 100)),
        ],
    );
}

/// A door is a way through and so is an airlock; a wall is not. What
/// `walkable` asks, said out loud.
#[test]
fn what_a_body_can_walk_through() {
    for kind in [PartKind::Door, PartKind::Chair, PartKind::Airlock] {
        assert!(!kind.def().blocks_movement, "{kind:?} should be walkable");
    }
    for kind in [PartKind::Wall, PartKind::OutsideWall, PartKind::Engine] {
        assert!(kind.def().blocks_movement, "{kind:?} should block");
    }
}

/// The cargo array is as long as there are resources. It is a fixed-size
/// array because it is hashed, and a fixed size is a thing that can drift.
#[test]
fn cargo_is_the_right_length() {
    assert_eq!(CARGO_SLOTS, ResourceId::ALL.len());
    assert_eq!(ShipDesign::new(4).cargo.len(), CARGO_SLOTS);
}

// --- rotation -------------------------------------------------------------

/// The engine is 2 x 3 with one use spot beside its middle-left, which makes
/// it the part where a rotation bug cannot hide. Every tile below is worked
/// out by hand, not by running the code and writing down what came out.
#[test]
fn all_four_turns_of_an_asymmetric_part_land_where_they_should() {
    let at = (10u32, 10u32);
    let tiles = |rotation| {
        let mut out: Vec<(u32, u32)> = covered(PartKind::Engine, rotation)
            .into_iter()
            .map(|(dx, dy)| (at.0 + dx, at.1 + dy))
            .collect();
        out.sort_unstable();
        out
    };
    let spots = |rotation| {
        use_spots(PartKind::Engine, rotation)
            .into_iter()
            .map(|(dx, dy)| (at.0 as i32 + dx, at.1 as i32 + dy))
            .collect::<Vec<_>>()
    };

    // Upright: two across, three down. You stand to the west, level with the
    // middle row.
    assert_eq!(footprint(PartKind::Engine, Rotation::R0), (2, 3));
    assert_eq!(
        tiles(Rotation::R0),
        vec![(10, 10), (10, 11), (10, 12), (11, 10), (11, 11), (11, 12)]
    );
    assert_eq!(spots(Rotation::R0), vec![(9, 11)]);

    // A quarter turn clockwise: three across, two down, and west has become
    // north — over the middle column.
    assert_eq!(footprint(PartKind::Engine, Rotation::R90), (3, 2));
    assert_eq!(
        tiles(Rotation::R90),
        vec![(10, 10), (10, 11), (11, 10), (11, 11), (12, 10), (12, 11)]
    );
    assert_eq!(spots(Rotation::R90), vec![(11, 9)]);

    // Half turn: the same six tiles, the use spot swung round to the east.
    assert_eq!(footprint(PartKind::Engine, Rotation::R180), (2, 3));
    assert_eq!(tiles(Rotation::R180), tiles(Rotation::R0));
    assert_eq!(spots(Rotation::R180), vec![(12, 11)]);

    // Three quarters: the same box as R90, the use spot to the south.
    assert_eq!(footprint(PartKind::Engine, Rotation::R270), (3, 2));
    assert_eq!(tiles(Rotation::R270), tiles(Rotation::R90));
    assert_eq!(spots(Rotation::R270), vec![(11, 12)]);
}

#[test]
fn r_goes_round_and_comes_back() {
    let mut r = Rotation::R0;
    for _ in 0..4 {
        r = r.next();
    }
    assert_eq!(r, Rotation::R0);
    assert_eq!(Rotation::R0.facing(), Facing::Forward);
    assert_eq!(Rotation::R90.facing(), Facing::Right);
    assert_eq!(Rotation::R180.facing(), Facing::Backward);
    assert_eq!(Rotation::R270.facing(), Facing::Left);
}

// --- apply ----------------------------------------------------------------

#[test]
fn a_part_hanging_off_the_edge_is_refused() {
    let design = floored(8, (0, 0), (8, 8));
    // The engine is three tiles deep upright, so an origin at y = 6 puts its
    // last row outside an 8-tile square.
    assert_eq!(
        place(&design, &rich(), PartKind::Engine, (3, 6), Rotation::R0),
        Err(EditError::OutOfBounds)
    );
    // Turned, it is only two deep and fits — but is then too wide.
    assert_eq!(
        place(&design, &rich(), PartKind::Engine, (3, 6), Rotation::R90).map(|d| d.parts.len()),
        // Sixty-four tiles of frame, sixty-four of deck, and the engine.
        Ok(129)
    );
    assert_eq!(
        place(&design, &rich(), PartKind::Engine, (6, 3), Rotation::R90),
        Err(EditError::OutOfBounds)
    );
    assert_eq!(
        place(&design, &rich(), PartKind::Floor, (8, 0), Rotation::R0),
        Err(EditError::OutOfBounds)
    );
}

#[test]
fn two_objects_cannot_stand_in_one_tile() {
    let design = put(floored(8, (0, 0), (8, 8)), PartKind::Hob, (3, 3));
    assert_eq!(
        place(&design, &rich(), PartKind::Basin, (3, 3), Rotation::R0),
        Err(EditError::ObjectOverlap)
    );
    // Overlapping by one tile of a longer footprint counts too.
    assert_eq!(
        place(&design, &rich(), PartKind::Worktop, (2, 3), Rotation::R0),
        Err(EditError::ObjectOverlap)
    );
    assert!(place(&design, &rich(), PartKind::Basin, (4, 3), Rotation::R0).is_ok());
}

#[test]
fn most_things_need_deck_under_them_and_hull_only_needs_frame() {
    // Frame over the whole square, deck over a corner of it.
    let mut design = framed(8, (0, 0), (8, 8));
    for y in 0..4 {
        for x in 0..4 {
            design = put(design, PartKind::Floor, (x, y));
        }
    }
    assert_eq!(
        place(&design, &rich(), PartKind::Hob, (5, 5), Rotation::R0),
        Err(EditError::MissingFloor)
    );
    // Half on, half off is still off.
    assert_eq!(
        place(&design, &rich(), PartKind::Worktop, (3, 1), Rotation::R0),
        Err(EditError::MissingFloor)
    );
    // Hull stands on bare frame.
    assert!(place(&design, &rich(), PartKind::Wall, (5, 5), Rotation::R0).is_ok());
    assert!(
        place(
            &design,
            &rich(),
            PartKind::OutsideWall,
            (5, 5),
            Rotation::R0
        )
        .is_ok()
    );
    assert!(
        place(
            &design,
            &rich(),
            PartKind::SensorArray,
            (5, 5),
            Rotation::R0
        )
        .is_ok()
    );
    assert!(
        place(
            &design,
            &rich(),
            PartKind::PowerConduit,
            (5, 5),
            Rotation::R0
        )
        .is_ok()
    );
}

/// Nothing at all goes down on a tile with no frame in it — not the deck,
/// not the hull, not a conduit. The frame is the first thing built and the
/// only thing that needs nothing.
#[test]
fn nothing_is_built_without_the_frame_under_it() {
    let bare = ShipDesign::new(8);
    for kind in [
        PartKind::Floor,
        PartKind::Wall,
        PartKind::OutsideWall,
        PartKind::SensorArray,
        PartKind::PowerConduit,
    ] {
        assert_eq!(
            place(&bare, &rich(), kind, (2, 2), Rotation::R0),
            Err(EditError::MissingStructure),
            "{kind:?} went down on nothing",
        );
    }
    // And with the frame there, every one of them does.
    let frame = framed(8, (2, 2), (3, 3));
    for kind in [
        PartKind::Floor,
        PartKind::Wall,
        PartKind::OutsideWall,
        PartKind::SensorArray,
        PartKind::PowerConduit,
    ] {
        assert!(
            place(&frame, &rich(), kind, (2, 2), Rotation::R0).is_ok(),
            "{kind:?} would not stand on the frame",
        );
    }
    // The frame itself needs nothing and goes anywhere inside the area.
    assert!(place(&bare, &rich(), PartKind::Structure, (7, 7), Rotation::R0).is_ok());
}

/// One part per layer per tile. The deck and the object layer have said so
/// since before there were four; the other two say it in their own words.
#[test]
fn a_layer_holds_one_thing_and_the_layers_do_not_collide() {
    let design = floored(8, (0, 0), (8, 8));
    assert_eq!(
        place(&design, &rich(), PartKind::Structure, (2, 2), Rotation::R0),
        Err(EditError::LayerOccupied)
    );
    let wired = put(design.clone(), PartKind::PowerConduit, (2, 2));
    assert_eq!(
        place(
            &wired,
            &rich(),
            PartKind::PowerConduit,
            (2, 2),
            Rotation::R0
        ),
        Err(EditError::LayerOccupied)
    );
    // A conduit and a hob share a tile happily: different layers, and the
    // conduit runs under the thing standing on it.
    assert!(place(&wired, &rich(), PartKind::Hob, (2, 2), Rotation::R0).is_ok());
}

#[test]
fn deck_cannot_be_laid_twice() {
    let design = put(floored(8, (0, 0), (4, 4)), PartKind::Structure, (4, 4));
    assert_eq!(
        place(&design, &rich(), PartKind::Floor, (2, 2), Rotation::R0),
        Err(EditError::DuplicateFloor)
    );
    // The frame is there and the deck is not, so this one takes.
    assert!(place(&design, &rich(), PartKind::Floor, (4, 4), Rotation::R0).is_ok());
    // And the frame cannot be laid twice either — it says so in its own
    // words, because it is not the deck.
    assert_eq!(
        place(&design, &rich(), PartKind::Structure, (4, 4), Rotation::R0),
        Err(EditError::LayerOccupied)
    );
}

#[test]
fn what_the_pool_will_not_cover_is_refused() {
    // Three tiles of frame at fifty each, and then nothing.
    let budget = Budget::new(150);
    let mut design = ShipDesign::new(8);
    for x in 0..3 {
        design = place(&design, &budget, PartKind::Structure, (x, 0), Rotation::R0).unwrap();
    }
    assert_eq!(budget.remaining(&design), 0);
    assert_eq!(
        place(&design, &budget, PartKind::Structure, (3, 0), Rotation::R0),
        Err(EditError::Unaffordable)
    );

    // And something dearer is refused while there is still money, rather
    // than when the pool is empty: what is left has to cover the whole price.
    // The frame and the deck go down first, so what the engine is refused
    // for is the price and not the plating under it.
    let budget = Budget::new(1_000);
    let mut design = ShipDesign::new(8);
    for y in 0..3 {
        for x in 0..2 {
            design = place(&design, &budget, PartKind::Structure, (x, y), Rotation::R0).unwrap();
            design = place(&design, &budget, PartKind::Floor, (x, y), Rotation::R0).unwrap();
        }
    }
    assert_eq!(budget.remaining(&design), 400);
    assert!(PartKind::Engine.def().price > 400);
    assert_eq!(
        place(&design, &budget, PartKind::Engine, (0, 0), Rotation::R0),
        Err(EditError::Unaffordable)
    );
    // Exactly what is left is still affordable: it is `>=`, not `>`.
    assert!(budget.affords(&design, 400));
    assert!(!budget.affords(&design, 401));
}

#[test]
fn what_is_underneath_something_stays_put() {
    let mut design = floored(8, (0, 0), (8, 8));
    design = put(design, PartKind::Hob, (3, 3));
    let grid = design.grid();
    let frame = grid.get(Layer::Structure, (3, 3));
    let deck = grid.get(Layer::Floor, (3, 3));
    let hob = grid.get(Layer::Object, (3, 3));
    assert!(frame != 0 && deck != 0 && hob != 0);

    // The deck is holding the hob up and the frame is holding the deck up.
    // Neither comes out from under what is standing on it.
    for id in [frame, deck] {
        assert_eq!(
            apply(&design, &rich(), Edit::Remove { part_id: id }),
            Err(EditError::SupportInUse),
        );
    }

    // Take them off in order and each comes up in its turn.
    let no_hob = apply(&design, &rich(), Edit::Remove { part_id: hob }).unwrap();
    assert_eq!(
        apply(&no_hob, &rich(), Edit::Remove { part_id: frame }),
        Err(EditError::SupportInUse),
    );
    let no_deck = apply(&no_hob, &rich(), Edit::Remove { part_id: deck }).unwrap();
    assert!(apply(&no_deck, &rich(), Edit::Remove { part_id: frame }).is_ok());
}

/// The same rule for everything that stands on the frame, not only the deck.
#[test]
fn the_frame_under_hull_and_conduit_stays_put() {
    for kind in [
        PartKind::Wall,
        PartKind::OutsideWall,
        PartKind::SensorArray,
        PartKind::PowerConduit,
    ] {
        let design = put(framed(8, (2, 2), (4, 4)), kind, (2, 2));
        let frame = design.grid().get(Layer::Structure, (2, 2));
        assert_eq!(
            apply(&design, &rich(), Edit::Remove { part_id: frame }),
            Err(EditError::SupportInUse),
            "the frame came out from under a {kind:?}",
        );
        // And a frame tile with nothing on it comes up.
        let spare = design.grid().get(Layer::Structure, (3, 3));
        assert!(apply(&design, &rich(), Edit::Remove { part_id: spare }).is_ok());
    }
}

#[test]
fn removing_something_that_is_not_there_is_refused() {
    let design = floored(8, (0, 0), (2, 2));
    assert_eq!(
        apply(&design, &rich(), Edit::Remove { part_id: 999 }),
        Err(EditError::NoSuchPart)
    );
    // And the id is not reissued, so the second removal of one part is a
    // refusal rather than a hit on whatever took its place. The deck rather
    // than the frame, because the frame has the deck standing on it.
    let deck = design.grid().get(Layer::Floor, (0, 0));
    let gone = apply(&design, &rich(), Edit::Remove { part_id: deck }).unwrap();
    let back = place(&gone, &rich(), PartKind::Floor, (0, 0), Rotation::R0).unwrap();
    assert_ne!(back.parts.last().unwrap().id, deck);
    assert_eq!(
        apply(&back, &rich(), Edit::Remove { part_id: deck }),
        Err(EditError::NoSuchPart)
    );
}

// --- the budget -----------------------------------------------------------

#[test]
fn a_removal_hands_back_exactly_what_the_part_cost() {
    let budget = Budget::new(REFERENCE_POOL);
    let empty = ShipDesign::new(10);
    let before = budget.remaining(&empty);
    assert_eq!(before, REFERENCE_POOL);

    let frame = place(&empty, &budget, PartKind::Structure, (2, 2), Rotation::R0).unwrap();
    let floored = place(&frame, &budget, PartKind::Floor, (2, 2), Rotation::R0).unwrap();
    let laid = budget.remaining(&floored);
    assert_eq!(
        laid + PartKind::Floor.def().price + PartKind::Structure.def().price,
        before,
    );

    let with = place(&floored, &budget, PartKind::ColdStore, (2, 2), Rotation::R0).unwrap();
    assert_eq!(
        budget.remaining(&with) + PartKind::ColdStore.def().price,
        laid
    );

    let store = with.grid().get(Layer::Object, (2, 2));
    let without = apply(&with, &budget, Edit::Remove { part_id: store }).unwrap();
    assert_eq!(budget.remaining(&without), laid);

    // Off in the order they went on: the deck is holding nothing up now, and
    // the frame is holding the deck up until it is gone.
    let deck = without.grid().get(Layer::Floor, (2, 2));
    let bare = apply(&without, &budget, Edit::Remove { part_id: deck }).unwrap();
    let nothing = apply(
        &bare,
        &budget,
        Edit::Remove {
            part_id: bare.grid().get(Layer::Structure, (2, 2)),
        },
    )
    .unwrap();
    assert_eq!(budget.remaining(&nothing), before);
    assert_eq!(Budget::spent(&nothing), 0);
}

#[test]
fn remaining_never_goes_under_nothing() {
    // A design that arrived from somewhere the rules were not applied.
    let budget = Budget::new(60);
    let mut design = ShipDesign::new(10);
    for x in 0..5 {
        design.parts.push(crate::design::PlacedPart {
            id: design.next_id,
            kind: PartKind::Floor,
            origin: (x, 0),
            rotation: Rotation::R0,
        });
        design.next_id += 1;
    }
    assert_eq!(Budget::spent(&design), 5 * PartKind::Floor.def().price);
    assert_eq!(budget.remaining(&design), 0);
    assert!(!budget.affords(&design, PartKind::Floor.def().price));
    // Not "everything is free again", which is what a wrap would read as.
    assert!(!budget.affords(&design, 1));
}

/// The price list, written out here as well as in the table, so that moving
/// one is a decision taken twice rather than a typo nobody notices. Every
/// part costs something: a free part is one the pool has no opinion about.
#[test]
fn every_part_has_the_price_it_is_meant_to_have() {
    let want: [(PartKind, Money); 15] = [
        (PartKind::Floor, 50),
        (PartKind::Wall, 100),
        (PartKind::Door, 400),
        (PartKind::Engine, 20_000),
        (PartKind::Bunk, 800),
        (PartKind::ColdStore, 1_500),
        (PartKind::Worktop, 600),
        (PartKind::Hob, 1_200),
        (PartKind::Dishwasher, 900),
        (PartKind::Table, 400),
        (PartKind::Chair, 150),
        (PartKind::Toilet, 1_000),
        (PartKind::Basin, 500),
        (PartKind::HydroBay, 4_000),
        (PartKind::BroomLocker, 150),
    ];
    for (kind, price) in want {
        assert_eq!(kind.def().price, price, "{kind:?}");
    }
    for &kind in PartKind::ALL.iter() {
        assert!(kind.def().price > 0, "{kind:?} is free");
    }
}

/// What the lobby's presets actually buy. Not a rule — prices and the
/// presets are both placeholders — but a solo player who cannot afford the
/// reference ship is a design phase nobody can finish, and that is worth
/// finding out here rather than in a browser.
#[test]
fn a_lone_player_can_afford_the_reference_ship() {
    let pool = economy::starting_pool(100_000, 1).unwrap();
    assert_eq!(pool, 120_000);
    for &crew in CREWS.iter() {
        let spent = Budget::spent(&reference(crew));
        assert!(spent > 0);
        assert!(
            spent <= economy::starting_pool(100_000, crew.max(1)).unwrap(),
            "the reference for {crew} costs {spent}",
        );
    }
}

// --- the hold, and the station ---------------------------------------------

/// A ship with a shelf, a tank and a cold store on it, with room to spare.
fn with_holds() -> ShipDesign {
    let mut design = floored(12, (1, 1), (11, 11));
    design = put(design, PartKind::Shelf, (2, 2));
    design = put(design, PartKind::FuelTank, (4, 2));
    design = put(design, PartKind::ColdStore, (8, 2));
    design
}

#[test]
fn what_the_ship_can_hold_is_the_sum_of_what_is_on_it() {
    let empty = floored(12, (1, 1), (11, 11));
    for class in Storage::ALL {
        assert_eq!(empty.capacity(class), 0, "{class:?}");
        assert_eq!(empty.stored(class), 0, "{class:?}");
    }

    let design = with_holds();
    assert_eq!(design.capacity(Storage::Shelf), 100);
    assert_eq!(design.capacity(Storage::FuelTank), 200);
    assert_eq!(design.capacity(Storage::ColdStore), 100);

    // Two shelves are twice the shelf.
    let more = put(design, PartKind::Shelf, (2, 4));
    assert_eq!(more.capacity(Storage::Shelf), 200);
}

#[test]
fn buying_fills_the_right_hold_and_costs_the_price() {
    let budget = Budget::new(REFERENCE_POOL);
    let design = with_holds();
    let before = budget.remaining(&design);

    let stocked = bought(&design, &budget, ResourceId::Metal, 10);
    assert_eq!(stocked.carrying(ResourceId::Metal), 10);
    assert_eq!(
        budget.remaining(&stocked) + trade_value(ResourceId::Metal, 10).unwrap(),
        before,
    );
    // Metal is racking, so it is the shelf that filled up and not the tank.
    assert_eq!(stocked.stored(Storage::Shelf), 10);
    assert_eq!(stocked.stored(Storage::FuelTank), 0);
    assert_eq!(stocked.stored(Storage::ColdStore), 0);

    // Food and fuel go to their own classes, and two resources sharing a
    // class share the room.
    let full = bought(
        &bought(&stocked, &budget, ResourceId::Tofu, 30),
        &budget,
        ResourceId::Vegetable,
        20,
    );
    assert_eq!(full.stored(Storage::ColdStore), 50);
    let fuelled = bought(&full, &budget, ResourceId::Fuel, 150);
    assert_eq!(fuelled.stored(Storage::FuelTank), 150);
}

#[test]
fn a_sale_hands_back_exactly_what_the_goods_cost() {
    let budget = Budget::new(REFERENCE_POOL);
    let design = with_holds();
    let before = budget.remaining(&design);

    let stocked = bought(&design, &budget, ResourceId::Components, 40);
    assert!(budget.remaining(&stocked) < before);

    let sold = apply(
        &stocked,
        &budget,
        Edit::Sell {
            resource: ResourceId::Components,
            units: 40,
        },
    )
    .unwrap();
    assert_eq!(sold.carrying(ResourceId::Components), 0);
    assert_eq!(budget.remaining(&sold), before, "the refund was not whole");
    assert_eq!(sold.cargo, design.cargo);

    // Half back is half back.
    let half = apply(
        &stocked,
        &budget,
        Edit::Sell {
            resource: ResourceId::Components,
            units: 15,
        },
    )
    .unwrap();
    assert_eq!(half.carrying(ResourceId::Components), 25);
    assert_eq!(
        budget.remaining(&half),
        before - trade_value(ResourceId::Components, 25).unwrap(),
    );
}

#[test]
fn what_cannot_be_paid_for_or_stowed_is_refused() {
    let design = with_holds();

    // Nothing in the pool but what the ship already cost: the parts are
    // bought, so there is nothing left for the shopping.
    let broke = Budget::new(Budget::spent(&design));
    assert_eq!(broke.remaining(&design), 0);
    assert_eq!(
        apply(
            &design,
            &broke,
            Edit::Buy {
                resource: ResourceId::Ore,
                units: 1
            }
        ),
        Err(EditError::CargoUnaffordable),
    );

    // Money enough for five and an order for six.
    let thin = Budget::new(Budget::spent(&design) + 5 * trade_price(ResourceId::Ore));
    assert!(
        apply(
            &design,
            &thin,
            Edit::Buy {
                resource: ResourceId::Ore,
                units: 5
            }
        )
        .is_ok()
    );
    assert_eq!(
        apply(
            &design,
            &thin,
            Edit::Buy {
                resource: ResourceId::Ore,
                units: 6
            }
        ),
        Err(EditError::CargoUnaffordable),
    );

    // Room for a hundred on the shelf and an order for a hundred and one —
    // with money for both, so what refuses it is the ship and not the pool.
    let rich = Budget::new(REFERENCE_POOL);
    assert!(
        apply(
            &design,
            &rich,
            Edit::Buy {
                resource: ResourceId::Ore,
                units: 100
            }
        )
        .is_ok()
    );
    assert_eq!(
        apply(
            &design,
            &rich,
            Edit::Buy {
                resource: ResourceId::Ore,
                units: 101
            }
        ),
        Err(EditError::NoRoomAboard),
    );

    // And the class is shared: eighty units of ore leaves twenty for metal.
    let part_full = bought(&design, &rich, ResourceId::Ore, 80);
    assert!(
        apply(
            &part_full,
            &rich,
            Edit::Buy {
                resource: ResourceId::Metal,
                units: 20
            }
        )
        .is_ok()
    );
    assert_eq!(
        apply(
            &part_full,
            &rich,
            Edit::Buy {
                resource: ResourceId::Metal,
                units: 21
            }
        ),
        Err(EditError::NoRoomAboard),
    );

    // A ship with no tank cannot take fuel at all, however much money there
    // is: there is nowhere to put it.
    let tankless = floored(12, (1, 1), (11, 11));
    assert_eq!(
        apply(
            &tankless,
            &rich,
            Edit::Buy {
                resource: ResourceId::Fuel,
                units: 1
            }
        ),
        Err(EditError::NoRoomAboard),
    );
}

#[test]
fn selling_what_is_not_aboard_is_refused() {
    let budget = Budget::new(REFERENCE_POOL);
    let design = bought(&with_holds(), &budget, ResourceId::Tofu, 10);
    for units in [11, 100, u32::MAX] {
        assert_eq!(
            apply(
                &design,
                &budget,
                Edit::Sell {
                    resource: ResourceId::Tofu,
                    units
                }
            ),
            Err(EditError::NotAboard),
            "{units} were sold out of ten",
        );
    }
    // A different resource in the same class is not this one.
    assert_eq!(
        apply(
            &design,
            &budget,
            Edit::Sell {
                resource: ResourceId::Vegetable,
                units: 1
            }
        ),
        Err(EditError::NotAboard),
    );
    assert!(
        apply(
            &design,
            &budget,
            Edit::Sell {
                resource: ResourceId::Tofu,
                units: 10
            }
        )
        .is_ok()
    );
}

#[test]
fn a_hold_with_something_in_it_cannot_be_taken_off() {
    let budget = Budget::new(REFERENCE_POOL);
    // Two shelves, a hundred each, and a hundred and fifty units aboard.
    let mut design = put(with_holds(), PartKind::Shelf, (2, 4));
    design = bought(&design, &budget, ResourceId::Ore, 150);

    let shelf = design.grid().get(Layer::Object, (2, 2));
    assert_eq!(
        apply(&design, &budget, Edit::Remove { part_id: shelf }),
        Err(EditError::StorageInUse),
        "a shelf came off under a hundred and fifty units of ore",
    );

    // Sell fifty and one shelf is spare, so it comes off.
    let lighter = apply(
        &design,
        &budget,
        Edit::Sell {
            resource: ResourceId::Ore,
            units: 50,
        },
    )
    .unwrap();
    assert!(apply(&lighter, &budget, Edit::Remove { part_id: shelf }).is_ok());

    // The tank and the cold store are empty, so they were never in the way.
    let tank = design.grid().get(Layer::Object, (4, 2));
    assert!(apply(&design, &budget, Edit::Remove { part_id: tank }).is_ok());
}

// --- radiation -------------------------------------------------------------

/// A sealed box: frame and deck inside, outside wall the whole way round.
/// `gap` replaces the middle of the top wall, which is what every test below
/// varies.
fn hull(gap: Option<PartKind>) -> ShipDesign {
    let mut design = framed(12, (2, 2), (10, 10));
    for y in 3..9 {
        for x in 3..9 {
            design = put(design, PartKind::Floor, (x, y));
        }
    }
    for i in 2..10 {
        for at in [(i, 2), (i, 9), (2, i), (9, i)] {
            if design.grid().get(Layer::Object, (at.0 as i32, at.1 as i32)) == 0 {
                design = put(design, PartKind::OutsideWall, at);
            }
        }
    }
    if let Some(kind) = gap {
        // Clear whatever the part's own footprint covers and give it deck if
        // it wants deck — the hull ring has none. Sized off the footprint
        // rather than assumed to be one tile: the engine is two by three and
        // an airlock is one by two, and the point of the test is that all of
        // them seal.
        let (w, h) = footprint(kind, Rotation::R0);
        for dy in 0..h {
            for dx in 0..w {
                let at = (5 + dx, 2 + dy);
                let there = design.grid().get(Layer::Object, (at.0 as i32, at.1 as i32));
                if there != 0 {
                    design = apply(&design, &rich(), Edit::Remove { part_id: there }).unwrap();
                }
                if kind.def().requires == Some(Layer::Floor)
                    && !design.grid().has_floor((at.0 as i32, at.1 as i32))
                {
                    design = put(design, PartKind::Floor, at);
                }
            }
        }
        design = put(design, kind, (5, 2));
    }
    design
}

#[test]
fn a_sealed_hull_lets_nothing_in() {
    let design = hull(None);
    let map = exposure(&design);
    assert!(
        map.is_empty(),
        "a closed hull was exposed at {:?}",
        map.tiles(),
    );
    assert!(!all_codes(&design, 0).contains(&IssueCode::RadiationExposure.code()));
}

#[test]
fn a_hole_in_the_hull_exposes_what_is_behind_it() {
    // A plain wall and a door are not hull: they hold a body in and let the
    // radiation through, which is the whole distinction the part table draws.
    for leaky in [PartKind::Wall, PartKind::Door] {
        let design = hull(Some(leaky));
        let map = exposure(&design);
        assert!(!map.is_empty(), "{leaky:?} sealed the ship");
        // The room behind it is exposed, not merely the gap.
        assert!(map.contains((5, 3)), "{leaky:?}: the room was not reached");
        assert!(map.contains((8, 8)), "{leaky:?}: the far corner was missed");
        // And it is the first thing the page is told about.
        assert_eq!(
            all_codes(&design, 0).first(),
            Some(&IssueCode::RadiationExposure.code()),
            "{leaky:?}",
        );
        // A warning, never a refusal. The box has no galley in it and so
        // has errors of its own; what matters is that this is not one of
        // them.
        let severity = validate(&design, 0)
            .into_iter()
            .find(|i| i.code == IssueCode::RadiationExposure.code())
            .map(|i| i.severity);
        assert_eq!(severity, Some(Severity::Warning), "{leaky:?}");
    }

    // The hull parts seal it again. An engine is a block of machinery, an
    // airlock is a door with a hull rating, a sensor array is bolted through
    // the skin — all three keep it out.
    for sealing in [PartKind::Engine, PartKind::Airlock, PartKind::SensorArray] {
        let design = hull(Some(sealing));
        assert!(
            exposure(&design).is_empty(),
            "{sealing:?} let the radiation in",
        );
    }
}

/// A shielding part is never exposed itself: the fill cannot enter it, which
/// is the hull doing its job rather than a special case in the code.
#[test]
fn the_hull_itself_is_not_what_is_being_irradiated() {
    let design = hull(Some(PartKind::Wall));
    let map = exposure(&design);
    for (x, y) in [(2u32, 2u32), (9, 9), (4, 2)] {
        assert!(
            !map.contains((x as i32, y as i32)),
            "the outside wall at {x},{y} was called exposed",
        );
    }
    // The plain wall standing in the gap *is* exposed — it is not hull.
    assert!(map.contains((5, 2)));
}

/// Shielding parts that meet only at a corner seal that corner. Four-
/// neighbour only, and this is what that buys: a hull drawn as a staircase
/// does not leak at every step of it.
#[test]
fn a_diagonal_join_does_not_leak() {
    // A diamond of four outside walls round one tile. No two of them share
    // an edge — every join is a corner — and the tile in the middle is
    // nevertheless sealed.
    let mut design = framed(8, (0, 0), (8, 8));
    for at in [(3, 2), (2, 3), (4, 3), (3, 4)] {
        design = put(design, PartKind::OutsideWall, at);
    }
    let map = exposure(&design);
    assert!(
        !map.contains((3, 3)),
        "the radiation went through a corner join",
    );
    // The frame all round it is reached, including the gaps between the
    // walls, so the fill is running and is simply not getting in.
    for open in [(2, 2), (4, 4), (2, 4), (4, 2), (7, 7)] {
        assert!(map.contains(open), "the fill never reached {open:?}");
    }
}

/// A part can be shielded and still be worked from a tile that is not. The
/// warning names it, because the Bim standing there is the one being cooked.
#[test]
fn a_part_worked_from_the_open_is_named() {
    let mut design = framed(8, (0, 0), (8, 8));
    for y in 0..8 {
        for x in 0..8 {
            design = put(design, PartKind::Floor, (x, y));
        }
    }
    design = put(design, PartKind::Hob, (4, 4));
    let hob = design.grid().get(Layer::Object, (4, 4));

    let issues = validate(&design, 0);
    let radiation = issues
        .iter()
        .find(|i| i.code == IssueCode::RadiationExposure.code())
        .expect("an unwalled ship was not reported");
    assert_eq!(radiation.severity, Severity::Warning);
    assert!(radiation.parts.contains(&hob));
    // The hob's own tile is not in the map — a hob does not shield, so the
    // fill went through it — but this is the use spot at (4, 5) either way.
    assert!(exposure(&design).contains((4, 5)));
}

// --- what a ship says about itself -----------------------------------------

#[test]
fn a_ship_with_no_helm_and_no_food_says_so_without_refusing() {
    let mut design = reference(1);
    design.parts.retain(|p| p.kind != PartKind::Helm);
    design.cargo = [0; CARGO_SLOTS];

    let issues = validate(&design, 1);
    assert!(!has_errors(&issues));
    let codes: Vec<u32> = issues.iter().map(|i| i.code).collect();
    assert!(codes.contains(&IssueCode::NoHelm.code()), "{codes:?}");
    assert!(codes.contains(&IssueCode::NoFoodAboard.code()), "{codes:?}");

    // One unit of either is food aboard. It is what is *in* the ship that
    // counts, not what the ship could hold.
    let budget = Budget::new(REFERENCE_POOL);
    let fed = bought(&design, &budget, ResourceId::Tofu, 1);
    assert!(!all_codes(&fed, 1).contains(&IssueCode::NoFoodAboard.code()));
}

// --- validation -----------------------------------------------------------

#[test]
fn the_reference_ship_is_valid_for_the_crew_it_was_built_for() {
    for (i, &crew) in CREWS.iter().enumerate() {
        let design = reference(crew);
        assert_eq!(
            design.parts.len() as u32,
            REFERENCE_PARTS[i],
            "the reference for {crew} crew lost or gained a part",
        );
        let issues = validate(&design, crew);
        assert!(
            !has_errors(&issues),
            "reference for {crew}: {:?}",
            issues
                .iter()
                .filter(|i| i.severity == Severity::Error)
                .collect::<Vec<_>>()
        );
        // It has an engine, a bay and a locker, so the only warning left is
        // the one about pushing on every axis.
        let warnings: Vec<u32> = issues.iter().map(|i| i.code).collect();
        assert_eq!(warnings, vec![IssueCode::NoEngineOnAxis.code()]);
    }
}

/// One test over the whole required list rather than seven, so a part added
/// to `REQUIRED` without an error code of its own cannot slip through.
#[test]
fn taking_out_any_required_fixture_is_an_error() {
    let crew = 4;
    let whole = reference(crew);
    for &(kind, code) in REQUIRED.iter() {
        let mut stripped = whole.clone();
        stripped.parts.retain(|p| p.kind != kind);
        assert!(
            stripped.parts.len() < whole.parts.len(),
            "the reference has no {kind:?} to take out",
        );
        assert!(
            codes(&stripped, crew).contains(&code.code()),
            "removing every {kind:?} did not raise {code:?}: {:?}",
            codes(&stripped, crew),
        );
    }
}

#[test]
fn a_bed_and_a_seat_each_or_it_is_an_error() {
    let design = reference(4);
    assert_eq!(codes(&design, 4), Vec::<u32>::new());
    assert_eq!(
        codes(&design, 5),
        vec![
            IssueCode::TooFewBunks.code(),
            IssueCode::TooFewChairs.code()
        ],
    );
    let mut no_bunks = design.clone();
    no_bunks.parts.retain(|p| p.kind != PartKind::Bunk);
    assert!(codes(&no_bunks, 4).contains(&IssueCode::TooFewBunks.code()));
}

#[test]
fn a_ship_in_two_pieces_is_an_error() {
    let design = reference(1);
    assert!(!codes(&design, 1).contains(&IssueCode::Disconnected.code()));

    // One tile of frame out on its own in the corner. The frame is what the
    // check walks: a ship is its structure, and everything else stands on
    // that.
    let stray = put(design, PartKind::Structure, (19, 19));
    assert!(codes(&stray, 1).contains(&IssueCode::Disconnected.code()));

    let issue = validate(&stray, 1)
        .into_iter()
        .find(|i| i.code == IssueCode::Disconnected.code())
        .unwrap();
    // It points at the stray rather than at the ship.
    assert_eq!(issue.tiles, vec![(19, 19)]);
    assert_eq!(issue.parts.len(), 1);
}

#[test]
fn a_wall_where_a_bim_has_to_stand_is_an_error() {
    let design = reference(1);
    assert!(!codes(&design, 1).contains(&IssueCode::UseSpotBlocked.code()));

    // The cold store is at (3, 3) facing down the room, so (3, 4) is where
    // whoever opens it stands.
    let store = *design
        .parts
        .iter()
        .find(|p| p.kind == PartKind::ColdStore)
        .unwrap();
    assert_eq!(store.use_spots(), vec![(3, 4)]);

    let blocked = put(design, PartKind::Wall, (3, 4));
    let issue = validate(&blocked, 1)
        .into_iter()
        .find(|i| i.code == IssueCode::UseSpotBlocked.code())
        .expect("a walled-in cold store was not reported");
    assert_eq!(issue.tiles, vec![(3, 4)]);
    assert_eq!(issue.parts, vec![store.id]);
}

/// Two fittings either side of a bulkhead. Through a door they can reach each
/// other; through a wall they cannot — and the check has to be able to tell
/// the difference, or every ship with an internal door is refused.
#[test]
fn a_door_is_a_way_through_and_a_wall_is_not() {
    let split = |gap: PartKind| {
        let mut design = floored(10, (1, 1), (9, 9));
        // A bulkhead across the middle with one tile left for the gap.
        for x in 1..9 {
            if x != 4 {
                design = put(design, PartKind::Wall, (x, 5));
            }
        }
        design = put(design, gap, (4, 5));
        // A cold store in the north half and a toilet in the south, each
        // facing into its own half.
        design = put(design, PartKind::ColdStore, (2, 2));
        design = put(design, PartKind::Toilet, (2, 7));
        design
    };

    let through = split(PartKind::Door);
    assert!(
        !codes(&through, 0).contains(&IssueCode::UseSpotsCutOff.code()),
        "a door was not a way through: {:?}",
        codes(&through, 0),
    );

    let shut = split(PartKind::Wall);
    assert!(
        codes(&shut, 0).contains(&IssueCode::UseSpotsCutOff.code()),
        "a solid bulkhead was walked through: {:?}",
        codes(&shut, 0),
    );
}

/// Engines are the design's business, not the validator's: none at all, or
/// none that can stop the ship, is something to be told rather than stopped.
#[test]
fn everything_about_engines_is_only_a_warning() {
    let mut design = reference(1);
    design.parts.retain(|p| p.kind != PartKind::Engine);
    let issues = validate(&design, 1);
    assert!(!has_errors(&issues));
    let codes: Vec<u32> = issues.iter().map(|i| i.code).collect();
    assert!(codes.contains(&IssueCode::NoEngine.code()));
    assert!(!codes.contains(&IssueCode::NoEngineOnAxis.code()));

    // One engine facing each way, on bare deck with room round them — the
    // reference is too full to stand four of them in, and this check is about
    // the axes and nothing else.
    let mut all_round = floored(20, (1, 1), (19, 19));
    for (i, &rotation) in Rotation::ALL.iter().enumerate() {
        all_round = place(
            &all_round,
            &rich(),
            PartKind::Engine,
            (2 + 4 * i as u32, 4),
            rotation,
        )
        .expect("an engine would not stand on bare deck");
    }
    let codes: Vec<u32> = validate(&all_round, 0).iter().map(|i| i.code).collect();
    assert!(!codes.contains(&IssueCode::NoEngine.code()), "{codes:?}");
    assert!(
        !codes.contains(&IssueCode::NoEngineOnAxis.code()),
        "{codes:?}"
    );
}

#[test]
fn a_ship_with_no_bay_and_no_locker_says_so_without_refusing() {
    let mut design = reference(1);
    design
        .parts
        .retain(|p| p.kind != PartKind::HydroBay && p.kind != PartKind::BroomLocker);
    let issues = validate(&design, 1);
    assert!(!has_errors(&issues));
    let codes: Vec<u32> = issues.iter().map(|i| i.code).collect();
    assert!(codes.contains(&IssueCode::NoHydroBay.code()));
    assert!(codes.contains(&IssueCode::NoBroomLocker.code()));
}

// --- the hash -------------------------------------------------------------

#[test]
fn the_same_ship_built_two_ways_hashes_the_same() {
    let budget = rich();
    let empty = ShipDesign::new(10);

    // Frame, deck, then the galley, then the shopping.
    let mut one = empty.clone();
    for x in 2..6 {
        for y in 2..4 {
            one = place(&one, &budget, PartKind::Structure, (x, y), Rotation::R0).unwrap();
            one = place(&one, &budget, PartKind::Floor, (x, y), Rotation::R0).unwrap();
        }
    }
    one = place(&one, &budget, PartKind::Hob, (2, 2), Rotation::R0).unwrap();
    one = place(&one, &budget, PartKind::ColdStore, (5, 3), Rotation::R90).unwrap();
    one = bought(&one, &budget, ResourceId::Tofu, 7);
    one = bought(&one, &budget, ResourceId::Vegetable, 3);

    // The same ship, laid out backwards, with a part placed and taken off
    // again in the middle of it, and the shopping done in the other order
    // and partly undone.
    let mut two = empty.clone();
    for x in (2..6).rev() {
        for y in (2..4).rev() {
            two = place(&two, &budget, PartKind::Structure, (x, y), Rotation::R0).unwrap();
        }
        for y in (2..4).rev() {
            two = place(&two, &budget, PartKind::Floor, (x, y), Rotation::R0).unwrap();
        }
    }
    two = place(&two, &budget, PartKind::ColdStore, (5, 3), Rotation::R90).unwrap();
    two = place(&two, &budget, PartKind::Basin, (4, 2), Rotation::R0).unwrap();
    let basin = two.grid().get(Layer::Object, (4, 2));
    two = apply(&two, &budget, Edit::Remove { part_id: basin }).unwrap();
    two = place(&two, &budget, PartKind::Hob, (2, 2), Rotation::R0).unwrap();
    two = bought(&two, &budget, ResourceId::Vegetable, 9);
    two = apply(
        &two,
        &budget,
        Edit::Sell {
            resource: ResourceId::Vegetable,
            units: 6,
        },
    )
    .unwrap();
    two = bought(&two, &budget, ResourceId::Tofu, 7);

    // The hob is one part in one place in both, and carries a different id in
    // each — which is the whole reason ids stay out of the hash.
    let hob = |d: &ShipDesign| d.grid().get(Layer::Object, (2, 2));
    assert_ne!(hob(&one), hob(&two), "the ids really do differ");
    assert_eq!(one.cargo, two.cargo);
    assert_eq!(design_hash(&one), design_hash(&two));

    // And a different manifest is a different ship, whatever the parts say.
    let heavier = bought(&one, &budget, ResourceId::Tofu, 1);
    assert_ne!(design_hash(&heavier), design_hash(&one));
}

#[test]
fn any_change_at_all_moves_the_hash() {
    let budget = rich();
    let base = put(
        put(floored(10, (2, 2), (6, 6)), PartKind::Hob, (3, 3)),
        PartKind::Basin,
        (4, 3),
    );
    let was = design_hash(&base);

    // A part added.
    let added = put(base.clone(), PartKind::Chair, (5, 5));
    assert_ne!(design_hash(&added), was);

    // A part taken away.
    let hob = base.grid().get(Layer::Object, (3, 3));
    let fewer = apply(&base, &budget, Edit::Remove { part_id: hob }).unwrap();
    assert_ne!(design_hash(&fewer), was);

    // The same part, turned.
    let turned = place(&fewer, &budget, PartKind::Hob, (3, 3), Rotation::R90).unwrap();
    assert_ne!(design_hash(&turned), was);
    assert_ne!(design_hash(&turned), design_hash(&base));

    // The same part, moved.
    let moved = place(&fewer, &budget, PartKind::Hob, (3, 4), Rotation::R0).unwrap();
    assert_ne!(design_hash(&moved), was);

    // The same parts in a bigger square.
    let mut wider = base.clone();
    wider.build_area = 12;
    assert_ne!(design_hash(&wider), was);

    // And putting it back gives the old number again.
    let back = place(&fewer, &budget, PartKind::Hob, (3, 3), Rotation::R0).unwrap();
    assert_eq!(design_hash(&back), was);
}

/// The pinned half of the cross-target check. The other half is
/// `ship_self_check` in `crates/ship`, which computes the same hashes in
/// wasm and compares them against these same constants — see
/// `scratchpad/ship-check.mjs`. A target that hashed differently would fail
/// exactly one of the two.
#[test]
fn the_reference_hashes_to_the_number_it_is_pinned_to() {
    for (i, &crew) in CREWS.iter().enumerate() {
        assert_eq!(
            design_hash(&reference(crew)),
            REFERENCE_HASH[i],
            "the reference design for {crew} crew has moved",
        );
    }
    assert_ne!(REFERENCE_HASH[0], REFERENCE_HASH[1]);
}

// --- mass and thrust ------------------------------------------------------

#[test]
fn the_hull_weighs_what_its_parts_weigh_and_the_hold_adds_to_it() {
    let design = put(
        put(floored(10, (2, 2), (6, 6)), PartKind::Hob, (2, 2)),
        PartKind::Engine,
        (4, 2),
    );
    // Sixteen tiles of frame, sixteen of deck, a hob and an engine — added
    // up by hand out of the recipes, which is the only place a mass comes
    // from now: frame is 2 metal, deck 1, a hob 3 metal and 3 components, an
    // engine 40 and 40, at 8 a unit for metal and 2 for components.
    let want = 16.0 * 16.0 + 16.0 * 8.0 + 30.0 + 400.0;
    assert_eq!(hull_mass(&design), want);

    // Two crew aboard and nothing in the hold.
    let mass = ship_mass(&design, 2).unwrap();
    assert_eq!(mass.get(), want + 2.0 * physics::PLAYER_MASS);

    // What is left in the pool is money rather than cargo, and weighs
    // nothing at all.
    let budget = Budget::new(REFERENCE_POOL);
    assert!(budget.remaining(&design) > 0);
    assert_eq!(ship_mass(&design, 2).unwrap().get(), mass.get());

    // What is *in the hold* does weigh something, and the engines have to
    // push it from the moment it is bought.
    let stocked = bought(
        &put(design.clone(), PartKind::Shelf, (5, 5)),
        &budget,
        ResourceId::Metal,
        20,
    );
    let with_shelf = want + part_mass(PartKind::Shelf);
    assert_eq!(hull_mass(&stocked), with_shelf, "cargo is not hull");
    assert_eq!(
        ship_mass(&stocked, 2).unwrap().get(),
        with_shelf + 20.0 * ResourceId::Metal.mass_per_unit() + 2.0 * physics::PLAYER_MASS,
    );
}

#[test]
fn an_axis_with_nothing_pushing_it_accelerates_at_nothing() {
    let design = put(
        put(floored(10, (2, 2), (6, 6)), PartKind::Engine, (2, 2)),
        PartKind::Hob,
        (5, 5),
    );
    let mass = ship_mass(&design, 1).unwrap().get();
    let forward = acceleration(&design, 1, Facing::Forward).unwrap();
    assert!((forward - 500.0 / mass).abs() < 1e-12, "{forward}");
    for axis in [Facing::Backward, Facing::Left, Facing::Right] {
        assert_eq!(acceleration(&design, 1, axis), Some(0.0));
    }
    // A design with nothing in it is not a ship, and says so rather than
    // dividing by nothing.
    assert_eq!(acceleration(&ShipDesign::new(10), 1, Facing::Forward), None);
}

#[test]
fn two_engines_on_one_axis_add_up() {
    let mut design = floored(10, (2, 2), (8, 8));
    design = put(design, PartKind::Engine, (2, 2));
    design = put(design, PartKind::Engine, (5, 2));
    let mass = ship_mass(&design, 0).unwrap().get();
    let forward = acceleration(&design, 0, Facing::Forward).unwrap();
    assert!((forward - 1000.0 / mass).abs() < 1e-12, "{forward}");
}

// --- materials, and mass that is moved rather than made -------------------

/// A yard to build one part in: frame, deck over all but the outermost ring
/// of it, and two shelves with enough metal and components in them for the
/// heaviest recipe there is.
///
/// The bare ring of frame is what the deck plating and the hull parts want —
/// structure with nothing on it. Two shelves rather than one on purpose:
/// deconstructing a shelf has to find room for the metal *that shelf was
/// made of*, and with one there would be nowhere to put it. That case has a
/// test of its own below.
fn yard() -> ShipDesign {
    let mut design = framed(12, (1, 1), (11, 11));
    for y in 2..11 {
        for x in 2..11 {
            design = put(design, PartKind::Floor, (x, y));
        }
    }
    design = put(design, PartKind::Shelf, (2, 2));
    design = put(design, PartKind::Shelf, (3, 2));
    design = bought(&design, &rich(), ResourceId::Metal, 40);
    bought(&design, &rich(), ResourceId::Components, 40)
}

/// Somewhere in [`yard`] that this kind can legally go.
fn yard_spot(kind: PartKind) -> (u32, u32) {
    match kind.def().requires {
        // The frame needs nothing under it, and the tile outside the frame
        // is the one tile with nothing in it at all.
        None => (0, 0),
        // Deck plating and hull stand straight on the frame. The bare ring
        // is frame with no deck on it, which is what the plating wants and
        // what the hull is happy with.
        Some(Layer::Structure) => (1, 3),
        // Everything else wants deck, and the middle of the yard is clear
        // for the largest footprint in the table.
        _ => (5, 5),
    }
}

fn build(design: &ShipDesign, kind: PartKind) -> Result<ShipDesign, EditError> {
    build_from_cargo(
        design,
        Edit::Place {
            kind,
            origin: yard_spot(kind),
            rotation: Rotation::R0,
        },
    )
}

/// Every recipe is materials and only materials. Ore is what metal is
/// refined from, fuel is burnt and the other two are eaten — none of them is
/// something a wall is made of, and a recipe that named one would be a part
/// nobody could build out of anything they mined.
#[test]
fn a_part_is_made_of_metal_and_components_and_nothing_else() {
    for &kind in PartKind::ALL.iter() {
        let recipe = kind.def().recipe;
        assert!(!recipe.is_empty(), "{kind:?} is made of nothing");
        for &(id, units) in recipe {
            assert!(
                id == ResourceId::Metal || id == ResourceId::Components,
                "{kind:?} is made of {id:?}",
            );
            assert!(units > 0, "{kind:?} wants no {id:?}");
        }
        let named_twice = recipe
            .iter()
            .enumerate()
            .any(|(i, &(id, _))| recipe[..i].iter().any(|&(seen, _)| seen == id));
        assert!(!named_twice, "{kind:?} names a material twice");
        assert!(part_mass(kind) > 0.0, "{kind:?} weighs nothing");
    }
}

/// Three recipes added up by hand. Metal is 8 a unit and components are 2,
/// and a part weighs what went into it and nothing else — so these are the
/// numbers that catch `part_mass` quietly growing a second term.
#[test]
fn a_part_weighs_what_it_is_made_of() {
    assert_eq!(part_mass(PartKind::Engine), 40.0 * 8.0 + 40.0 * 2.0);
    assert_eq!(part_mass(PartKind::Engine), 400.0);
    assert_eq!(part_mass(PartKind::Wall), 2.0 * 8.0);
    assert_eq!(part_mass(PartKind::Wall), 16.0);
    assert_eq!(part_mass(PartKind::Helm), 4.0 * 8.0 + 20.0 * 2.0);
    assert_eq!(part_mass(PartKind::Helm), 72.0);
}

/// The whole of the contract, for every part there is: what comes out of the
/// hold and what goes into the wall weigh the same, so the ship's mass does
/// not move.
#[test]
fn building_a_part_out_of_the_hold_does_not_change_what_the_ship_weighs() {
    let yard = yard();
    let before = ship_mass(&yard, 2).unwrap().get();
    for &kind in PartKind::ALL.iter() {
        let built = build(&yard, kind).unwrap_or_else(|e| panic!("{kind:?} was refused: {e:?}"));
        let after = ship_mass(&built, 2).unwrap().get();
        assert!(
            (after - before).abs() < 1e-9,
            "{kind:?}: {before} became {after}",
        );
        // And it went the way round it is meant to: the hull gained exactly
        // the part, so the hold lost exactly the part.
        assert!(
            (hull_mass(&built) - hull_mass(&yard) - part_mass(kind)).abs() < 1e-9,
            "{kind:?}",
        );
        assert_eq!(built.parts.len(), yard.parts.len() + 1, "{kind:?}");
    }
}

/// And back again. Deconstructing what was just built returns the hold to
/// exactly what it held before — every unit of it, for every part in the
/// table — which is the "no loss" half of the rule.
#[test]
fn taking_a_part_off_puts_every_material_back() {
    let yard = yard();
    let before = ship_mass(&yard, 2).unwrap().get();
    for &kind in PartKind::ALL.iter() {
        let built = build(&yard, kind).unwrap_or_else(|e| panic!("{kind:?} was refused: {e:?}"));
        let id = built.parts.last().unwrap().id;
        let back = deconstruct_to_cargo(&built, id).unwrap_or_else(|e| panic!("{kind:?}: {e:?}"));
        assert_eq!(back.cargo, yard.cargo, "{kind:?} came back short");
        assert!(
            (ship_mass(&back, 2).unwrap().get() - before).abs() < 1e-9,
            "{kind:?}",
        );
        assert_eq!(back.parts.len(), yard.parts.len(), "{kind:?}");
    }
}

/// An empty hold builds nothing, and it is refused whole — not placed and
/// then paid for, which would be a wall standing there for free.
#[test]
fn building_without_the_materials_is_refused() {
    let bare = floored(12, (1, 1), (11, 11));
    for kind in [PartKind::Wall, PartKind::Engine, PartKind::Hob] {
        assert_eq!(
            build_from_cargo(
                &bare,
                Edit::Place {
                    kind,
                    origin: (5, 5),
                    rotation: Rotation::R0,
                },
            ),
            Err(EditError::MaterialsShort),
            "{kind:?}",
        );
    }

    // One unit short is still short: it is the whole recipe or nothing.
    let yard = yard();
    let engine = build(&yard, PartKind::Engine).unwrap();
    assert_eq!(engine.carrying(ResourceId::Metal), 0);
    assert_eq!(
        build(&engine, PartKind::Wall),
        Err(EditError::MaterialsShort),
    );

    // The placement rules are still `apply`'s, and they are asked first: a
    // part that will not fit is told so rather than told to go shopping.
    assert_eq!(
        build_from_cargo(
            &bare,
            Edit::Place {
                kind: PartKind::Hob,
                origin: (11, 11),
                rotation: Rotation::R0,
            },
        ),
        Err(EditError::MissingFloor),
    );

    // And nothing but a placement is construction.
    assert_eq!(
        build_from_cargo(&yard, Edit::Remove { part_id: 1 }),
        Err(EditError::BadCode),
    );
    assert_eq!(
        build_from_cargo(
            &yard,
            Edit::Buy {
                resource: ResourceId::Metal,
                units: 1,
            },
        ),
        Err(EditError::BadCode),
    );
}

/// Materials have to have somewhere to go. A ship with no shelf cannot take
/// a hob apart, and the one whose only shelf *is* the part coming off cannot
/// either — the room is measured after the removal, which is the case that
/// makes the rule bite.
#[test]
fn a_deconstruction_with_nowhere_to_put_the_materials_is_refused() {
    let bare = put(floored(12, (1, 1), (11, 11)), PartKind::Hob, (5, 5));
    let hob = bare.parts.last().unwrap().id;
    assert_eq!(
        deconstruct_to_cargo(&bare, hob),
        Err(EditError::NoRoomAboard),
    );

    let one_shelf = put(floored(12, (1, 1), (11, 11)), PartKind::Shelf, (2, 2));
    let shelf = one_shelf.parts.last().unwrap().id;
    assert_eq!(
        deconstruct_to_cargo(&one_shelf, shelf),
        Err(EditError::NoRoomAboard),
        "the shelf cannot hold the metal it is made of once it is off",
    );
    // With a second shelf to put it on, the same removal is fine.
    let two = put(one_shelf, PartKind::Shelf, (3, 2));
    assert_eq!(
        deconstruct_to_cargo(&two, shelf).map(|d| d.carrying(ResourceId::Metal)),
        Ok(2),
    );

    // A part that is not there is `NoSuchPart`, the same as a removal is.
    assert_eq!(
        deconstruct_to_cargo(&bare, 9_999),
        Err(EditError::NoSuchPart),
    );
    // And the removal rules still hold: the deck under the hob is holding it
    // up, whatever the materials would do.
    let deck = bare.grid().get(Layer::Floor, (5, 5));
    assert_eq!(
        deconstruct_to_cargo(&bare, deck),
        Err(EditError::SupportInUse),
    );
}

/// What is welded in and what is in the hold are one stock of materials.
/// Building moves units from one column to the other and changes neither
/// total — the same statement as the mass one, in the units a hauling step
/// will want.
#[test]
fn what_is_welded_in_and_what_is_in_the_hold_are_one_stock() {
    let yard = yard();
    let built = build(&yard, PartKind::Engine).unwrap();

    let before = bound_materials(&yard);
    let after = bound_materials(&built);
    assert_eq!(
        after[ResourceId::Metal as usize] - before[ResourceId::Metal as usize],
        40,
    );
    assert_eq!(
        after[ResourceId::Components as usize] - before[ResourceId::Components as usize],
        40,
    );
    for &id in ResourceId::ALL.iter() {
        let i = id as usize;
        assert_eq!(
            after[i] + built.carrying(id) as u64,
            before[i] + yard.carrying(id) as u64,
            "{id:?} was made or lost",
        );
    }

    // Nothing is made of food or ore, so those columns stay empty however
    // the ship is built.
    for id in [
        ResourceId::Ore,
        ResourceId::Fuel,
        ResourceId::Vegetable,
        ResourceId::Tofu,
    ] {
        assert_eq!(after[id as usize], 0, "{id:?} is welded into something");
    }

    // The two ways of weighing the hull are one sum written twice.
    for design in [yard, built, reference(4)] {
        assert!((bound_mass(&design) - hull_mass(&design)).abs() < 1e-9);
    }
}
