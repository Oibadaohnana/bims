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
use crate::fixture::{
    CREWS, FLYER_FUEL, REFERENCE_HASH, REFERENCE_PARTS, REFERENCE_POOL, flyer, reference,
};
use crate::mass::{acceleration, hull_mass, ship_mass};
use crate::materials::{bound_mass, bound_materials, build_from_cargo, deconstruct_to_cargo};
use crate::parts::{
    Layer, PartKind, Rotation, TILE, covered, defs_are_sound, footprint, part_mass, use_spots,
};
use crate::validate::{IssueCode, REQUIRED, Severity, exposure, has_errors, validate, walkable};

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
fn only_the_engines_push_and_everything_has_weight() {
    for &kind in PartKind::ALL.iter() {
        let def = kind.def();
        assert!(part_mass(kind) > 0.0, "{kind:?} weighs nothing");
        if matches!(kind, PartKind::Engine | PartKind::HeavyEngine) {
            assert!(def.pushes(), "{kind:?} does not push");
        } else {
            assert!(!def.pushes(), "{kind:?} pushes the ship");
        }
        assert_eq!(def.turns(), kind == PartKind::Thruster, "{kind:?}");
    }
}

/// What the second engine is *for*, said as numbers: more push for the
/// weight than the small one, so a heavy hull is better off with it, and
/// more weight and money than a light hull would want to carry. A heavy
/// engine that was merely bigger — the same push per tonne — would be a
/// choice with nothing in it.
#[test]
fn the_heavy_engine_is_the_better_engine_per_tonne_and_the_worse_one_to_carry() {
    let (small, big) = (PartKind::Engine.def(), PartKind::HeavyEngine.def());
    assert!(
        big.thrust > 4.0 * small.thrust,
        "not much of a heavy engine"
    );
    assert!(part_mass(PartKind::HeavyEngine) > 3.0 * part_mass(PartKind::Engine));
    assert!(
        big.thrust / part_mass(PartKind::HeavyEngine) > small.thrust / part_mass(PartKind::Engine),
        "the heavy engine should push more per tonne of itself"
    );
    assert!(big.price > 3 * small.price);
    let (w, h) = footprint(PartKind::HeavyEngine, Rotation::R0);
    assert!(w * h > 2 * 3, "it should take more deck than the small one");
    // Hull like the small one: shields, and used from beside it.
    assert!(big.shields);
    assert_eq!(big.use_spots, small.use_spots);
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
        PartKind::Thruster,
        PartKind::PowerConduit,
        PartKind::DiagonalWall,
        PartKind::DiagonalOutsideWall,
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
                | PartKind::Thruster
                | PartKind::PowerConduit
                | PartKind::DiagonalWall
                | PartKind::DiagonalOutsideWall
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
            PartKind::Thruster,
            PartKind::HeavyEngine,
            PartKind::DiagonalOutsideWall,
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
            (PartKind::SuitLocker, (Storage::Locker, 2)),
            (PartKind::Armoury, (Storage::Locker, 4)),
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
    for kind in [
        PartKind::Wall,
        PartKind::OutsideWall,
        PartKind::Engine,
        PartKind::DiagonalWall,
        PartKind::DiagonalOutsideWall,
    ] {
        assert!(kind.def().blocks_movement, "{kind:?} should block");
    }
}

/// The two corner pieces are walls in every rule and a triangle only in the
/// picture: the same layer, the same footing, the same footprint, and the
/// hull one shields exactly as the straight hull does. Which corner is solid
/// goes round clockwise with `R`, starting south-west.
#[test]
fn a_diagonal_wall_is_a_wall_in_every_rule_and_a_triangle_in_the_picture() {
    use crate::parts::{is_diagonal, solid_corner};
    for (diagonal, straight) in [
        (PartKind::DiagonalWall, PartKind::Wall),
        (PartKind::DiagonalOutsideWall, PartKind::OutsideWall),
    ] {
        let (d, s) = (diagonal.def(), straight.def());
        assert!(is_diagonal(diagonal));
        assert!(!is_diagonal(straight));
        assert_eq!(d.footprint, (1, 1));
        assert_eq!(d.layer, s.layer);
        assert_eq!(d.requires, s.requires);
        assert_eq!(d.blocks_movement, s.blocks_movement);
        assert_eq!(d.shields, s.shields);
        assert_eq!(d.price, s.price);
        assert_eq!(d.recipe, s.recipe);
        assert!(d.use_spots.is_empty());
    }
    let mut r = Rotation::R0;
    let mut corners = Vec::new();
    for _ in 0..4 {
        corners.push(solid_corner(r));
        r = r.next();
    }
    assert_eq!(corners, vec![(-1, 1), (-1, -1), (1, -1), (1, 1)]);
}

/// The cargo array is as long as there are resources. It is a fixed-size
/// array because it is hashed, and a fixed size is a thing that can drift.
#[test]
fn cargo_is_the_right_length() {
    assert_eq!(CARGO_SLOTS, ResourceId::ALL.len());
    assert_eq!(ShipDesign::new(4).cargo.len(), CARGO_SLOTS);
}

// --- rotation -------------------------------------------------------------

/// The engine is 2 x 3, which makes it the part where a rotation bug in the
/// footprint cannot hide, and the bay is 2 x 2 with one use spot below its
/// left-hand column, which makes it the part where a rotation bug in the
/// use spots cannot hide. Every tile below is worked out by hand, not by
/// running the code and writing down what came out.
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
        use_spots(PartKind::FuelTank, rotation)
            .into_iter()
            .map(|(dx, dy)| (at.0 as i32 + dx, at.1 as i32 + dy))
            .collect::<Vec<_>>()
    };

    // Upright: two across, three down.
    assert_eq!(footprint(PartKind::Engine, Rotation::R0), (2, 3));
    assert_eq!(
        tiles(Rotation::R0),
        vec![(10, 10), (10, 11), (10, 12), (11, 10), (11, 11), (11, 12)]
    );
    // The tank: you stand below its left-hand column.
    assert_eq!(spots(Rotation::R0), vec![(10, 12)]);

    // A quarter turn clockwise: three across, two down; and south has become
    // west, level with the top row.
    assert_eq!(footprint(PartKind::Engine, Rotation::R90), (3, 2));
    assert_eq!(
        tiles(Rotation::R90),
        vec![(10, 10), (10, 11), (11, 10), (11, 11), (12, 10), (12, 11)]
    );
    assert_eq!(spots(Rotation::R90), vec![(9, 10)]);

    // Half turn: the same six tiles; the use spot swung round to the north,
    // over the right-hand column.
    assert_eq!(footprint(PartKind::Engine, Rotation::R180), (2, 3));
    assert_eq!(tiles(Rotation::R180), tiles(Rotation::R0));
    assert_eq!(spots(Rotation::R180), vec![(11, 9)]);

    // Three quarters: the same box as R90, the use spot to the east, level
    // with the bottom row.
    assert_eq!(footprint(PartKind::Engine, Rotation::R270), (3, 2));
    assert_eq!(tiles(Rotation::R270), tiles(Rotation::R90));
    assert_eq!(spots(Rotation::R270), vec![(12, 11)]);
}

/// An engine is worked on from any side: its use spots are the ring round
/// it, corners left out, and they turn with it. Ten of them for a 2 x 3.
#[test]
fn an_engine_is_used_from_the_ring_round_it() {
    let ring = use_spots(PartKind::Engine, Rotation::R0);
    assert_eq!(ring.len(), 10);
    for spot in [(-1, 0), (-1, 2), (2, 1), (0, -1), (1, 3)] {
        assert!(ring.contains(&spot), "{spot:?} is not on the ring");
    }
    for corner in [(-1, -1), (2, -1), (-1, 3), (2, 3)] {
        assert!(!ring.contains(&corner), "{corner:?} is a corner");
    }
    // Turned a quarter, the ring is the ring of the turned box.
    let turned = use_spots(PartKind::Engine, Rotation::R90);
    assert_eq!(turned.len(), 10);
    assert!(turned.contains(&(3, 0)) && turned.contains(&(1, -1)) && turned.contains(&(1, 2)));
    assert!(crate::parts::any_side_will_do(PartKind::Engine));
    assert!(crate::parts::any_side_will_do(PartKind::HeavyEngine));
    assert!(!crate::parts::any_side_will_do(PartKind::HydroBay));
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
    for sealing in [
        PartKind::Engine,
        PartKind::HeavyEngine,
        PartKind::Airlock,
        PartKind::SensorArray,
    ] {
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

/// A hull with its corners cut off at forty-five degrees is sealed: the
/// staircase of diagonal outside walls across each corner keeps the fill
/// out exactly as the straight run it replaces did, and the plain diagonal
/// wall does not — it is a bulkhead, not hull.
#[test]
fn a_chamfered_corner_is_sealed_by_diagonal_hull_and_not_by_diagonal_wall() {
    use crate::parts::solid_corner;
    let chamfered = |kind: PartKind| {
        // The sealed box with its top-left corner cut: (2,2), (3,2) and
        // (2,3) come off the ring, and the cut runs (2,4), (3,3), (4,2) with
        // the solid half of each facing the room — south-east, R270.
        let mut design = hull(None);
        for at in [(2, 2), (3, 2), (2, 3)] {
            let wall = design.grid().get(Layer::Object, at);
            design = apply(&design, &rich(), Edit::Remove { part_id: wall }).unwrap();
            let frame = design.grid().get(Layer::Structure, at);
            design = apply(&design, &rich(), Edit::Remove { part_id: frame }).unwrap();
        }
        for at in [(2u32, 4u32), (3, 3), (4, 2)] {
            let wall = design.grid().get(Layer::Object, (at.0 as i32, at.1 as i32));
            if wall != 0 {
                design = apply(&design, &rich(), Edit::Remove { part_id: wall }).unwrap();
            }
            design = place(&design, &rich(), kind, at, Rotation::R270).unwrap();
        }
        design
    };
    assert_eq!(solid_corner(Rotation::R270), (1, 1));

    let sealed = chamfered(PartKind::DiagonalOutsideWall);
    assert!(
        exposure(&sealed).is_empty(),
        "the radiation came in through the chamfer: {:?}",
        exposure(&sealed).tiles()
    );
    // And the deck inside the cut is still deck a body can stand on.
    assert!(walkable(&sealed, &sealed.grid(), (3, 4)));

    let leaky = chamfered(PartKind::DiagonalWall);
    let map = exposure(&leaky);
    assert!(
        map.contains((3, 4)),
        "a plain diagonal wall kept the radiation out"
    );
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
        // It has a forward engine, a bay, a locker, a helm and food, so what
        // is left is exactly the four things a *trip* wants and living
        // aboard does not: something to turn with, something to burn,
        // somewhere to dock through and something to see with.
        let warnings: Vec<u32> = issues.iter().map(|i| i.code).collect();
        assert_eq!(
            warnings,
            vec![
                IssueCode::NoThruster.code(),
                IssueCode::NoFuelAboard.code(),
                IssueCode::NoAirlock.code(),
                IssueCode::NoSensorArray.code(),
            ],
        );
    }
}

/// The other fixture. [`reference`] is a ship to live on; this is one to fly,
/// and what says so is that every flight warning has gone.
#[test]
fn the_flyer_is_a_ship_a_trip_can_actually_be_planned_for() {
    for &crew in CREWS.iter() {
        let design = flyer(crew);
        let issues = validate(&design, crew);
        assert!(
            !has_errors(&issues),
            "flyer for {crew}: {:?}",
            issues
                .iter()
                .filter(|i| i.severity == Severity::Error)
                .collect::<Vec<_>>()
        );
        let codes: Vec<u32> = issues.iter().map(|i| i.code).collect();
        assert!(codes.is_empty(), "flyer for {crew} still warns: {codes:?}");

        assert_eq!(design.count(PartKind::Thruster), 4);
        assert_eq!(design.count(PartKind::Airlock), 1);
        assert_eq!(design.count(PartKind::SensorArray), 1);
        assert_eq!(design.carrying(ResourceId::Fuel), FLYER_FUEL);
        // A full tank and not a unit more: the tank is what bounds it, and a
        // fixture that quietly bought less would make every fuel scenario
        // measure something other than what it says.
        assert_eq!(design.capacity(Storage::FuelTank), FLYER_FUEL);
    }
}

/// The thrusters and the array went in **place of** hull plating, and both of
/// them shield, so the skin is still closed. If either stopped shielding this
/// is what would notice.
#[test]
fn a_flyer_is_still_sealed() {
    assert!(exposure(&flyer(4)).is_empty());
    // And it is the same ship underneath: seven tiles of plating came off;
    // four thrusters, an array, two tiles of deck with the airlock standing
    // on them, and a tank went on.
    assert_eq!(
        flyer(4).parts.len(),
        reference(4).parts.len() - 7 + 9,
        "the flyer should be the reference with its hull swapped and three parts added",
    );
    assert_eq!(
        flyer(4).count(PartKind::OutsideWall) + 7,
        reference(4).count(PartKind::OutsideWall)
    );
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
        // A bulkhead across the middle with two tiles left for the gap: a
        // door is two tiles along its bulkhead, and it is turned to run
        // along this one.
        for x in 1..9 {
            if x != 4 && x != 5 {
                design = put(design, PartKind::Wall, (x, 5));
            }
        }
        for x in [4, 5] {
            // One door fills both tiles; walls go in one at a time.
            if design.grid().get(Layer::Object, (x, 5)) == 0 {
                design = place(&design, &rich(), gap, (x as u32, 5), Rotation::R90)
                    .unwrap_or_else(|e| panic!("{gap:?} was refused: {e:?}"));
            }
        }
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
/// none that can push the ship along its own nose, is something to be told
/// rather than stopped.
#[test]
fn everything_about_engines_is_only_a_warning() {
    let mut design = reference(1);
    design.parts.retain(|p| p.kind != PartKind::Engine);
    let issues = validate(&design, 1);
    assert!(!has_errors(&issues));
    let codes: Vec<u32> = issues.iter().map(|i| i.code).collect();
    assert!(codes.contains(&IssueCode::NoEngine.code()));
    // No engines at all is one complaint, not two: "and none of them faces
    // forward" about a ship with no engines is a sentence nobody needs.
    assert!(!codes.contains(&IssueCode::NoForwardEngine.code()));

    // What the flight step actually asks for is a **forward** engine. A ship
    // with one pointing every other way is a ship that cannot set off, and
    // the warning says so; add the forward one and it goes.
    let mut sideways = floored(20, (1, 1), (19, 19));
    for (i, &rotation) in [Rotation::R90, Rotation::R180, Rotation::R270]
        .iter()
        .enumerate()
    {
        sideways = place(
            &sideways,
            &rich(),
            PartKind::Engine,
            (2 + 4 * i as u32, 4),
            rotation,
        )
        .expect("an engine would not stand on bare deck");
    }
    let codes: Vec<u32> = validate(&sideways, 0).iter().map(|i| i.code).collect();
    assert!(!codes.contains(&IssueCode::NoEngine.code()), "{codes:?}");
    assert!(
        codes.contains(&IssueCode::NoForwardEngine.code()),
        "{codes:?}"
    );

    let forward = place(&sideways, &rich(), PartKind::Engine, (14, 4), Rotation::R0)
        .expect("an engine would not stand on bare deck");
    let codes: Vec<u32> = validate(&forward, 0).iter().map(|i| i.code).collect();
    assert!(
        !codes.contains(&IssueCode::NoForwardEngine.code()),
        "{codes:?}"
    );
}

/// The four warnings the flight step added, one at a time. Each is about a
/// part the ship can perfectly well be lived on without and cannot leave the
/// dock without, and none of them is an error.
#[test]
fn what_a_trip_wants_is_said_without_being_insisted_on() {
    let whole = flyer(1);
    assert!(validate(&whole, 1).is_empty());

    for (kind, code) in [
        (PartKind::Thruster, IssueCode::NoThruster),
        (PartKind::Airlock, IssueCode::NoAirlock),
        (PartKind::SensorArray, IssueCode::NoSensorArray),
    ] {
        let mut stripped = whole.clone();
        stripped.parts.retain(|p| p.kind != kind);
        let issues = validate(&stripped, 1);
        assert!(!has_errors(&issues), "{kind:?} became an error");
        let codes: Vec<u32> = issues.iter().map(|i| i.code).collect();
        assert!(codes.contains(&code.code()), "{kind:?}: {codes:?}");
    }

    // Fuel is what is aboard rather than what the tank could hold. A full
    // tank sold off is a ship that goes nowhere, and the tank is still there.
    let dry = apply(
        &whole,
        &rich(),
        Edit::Sell {
            resource: ResourceId::Fuel,
            units: FLYER_FUEL,
        },
    )
    .expect("the fuel would not sell");
    let codes: Vec<u32> = validate(&dry, 1).iter().map(|i| i.code).collect();
    assert_eq!(codes, vec![IssueCode::NoFuelAboard.code()]);
    assert_eq!(dry.count(PartKind::FuelTank), 1);
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
            "the reference design for {crew} crew has moved: it now hashes to {:#018x}",
            design_hash(&reference(crew)),
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
    assert!((forward - 2_000.0 / mass).abs() < 1e-12, "{forward}");
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
    assert!((forward - 4_000.0 / mass).abs() < 1e-12, "{forward}");
}

// --- materials, and mass that is moved rather than made -------------------

/// A yard to build one part in: frame, deck over all but the outermost ring
/// of it, and shelves holding **exactly** the heaviest recipe there is — the
/// heavy engine's — which is what lets the "one unit short" case below build
/// that and then be refused a wall.
///
/// The bare ring of frame is what the deck plating and the hull parts want —
/// structure with nothing on it. More than one shelf on purpose:
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
    for x in 2..5 {
        design = put(design, PartKind::Shelf, (x, 2));
    }
    for &(id, units) in PartKind::HeavyEngine.def().recipe {
        design = bought(&design, &rich(), id, units);
    }
    design
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

    // One unit short is still short: it is the whole recipe or nothing. The
    // yard holds exactly the heavy engine's recipe, so building one empties
    // the shelves.
    let yard = yard();
    let engine = build(&yard, PartKind::HeavyEngine).unwrap();
    assert_eq!(engine.carrying(ResourceId::Metal), 0);
    assert_eq!(engine.carrying(ResourceId::Components), 0);
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

// --- the playtest ship ------------------------------------------------------

/// The simulation's ship is one you can live on *and* fly, straight away:
/// no errors, no warnings, and every part it is meant to have.
#[test]
fn the_playtest_ship_is_a_whole_ship_for_one() {
    use crate::fixture::{PLAYTEST_CARGO, PLAYTEST_PARTS, playtest_ship};
    let design = playtest_ship();
    assert_eq!(
        design.parts.len() as u32,
        PLAYTEST_PARTS,
        "the playtest ship lost or gained a part",
    );
    let issues = validate(&design, 1);
    let codes: Vec<u32> = issues.iter().map(|i| i.code).collect();
    assert!(
        codes.is_empty(),
        "the playtest ship still complains: {codes:?}"
    );
    assert!(exposure(&design).is_empty());

    for (kind, want) in [
        (PartKind::Thruster, 4),
        (PartKind::Airlock, 1),
        (PartKind::SensorArray, 1),
        (PartKind::Engine, 1),
        (PartKind::Helm, 1),
        (PartKind::FuelTank, 1),
        (PartKind::Shelf, 2),
        (PartKind::Smelter, 1),
        (PartKind::Workbench, 1),
        (PartKind::SuitLocker, 1),
        (PartKind::ColdStore, 1),
        (PartKind::Worktop, 1),
        (PartKind::Hob, 1),
        (PartKind::Dishwasher, 1),
        (PartKind::Table, 1),
        (PartKind::Chair, 1),
        (PartKind::Bunk, 1),
        (PartKind::Toilet, 1),
        (PartKind::Basin, 1),
        (PartKind::Shower, 1),
        (PartKind::HydroBay, 1),
        (PartKind::BroomLocker, 1),
        (PartKind::Reactor, 1),
        (PartKind::LifeSupport, 1),
        (PartKind::Battery, 1),
        // Five corner pieces a side cut the bow back, and two bulkheads
        // with a two-tile doorway each — one door apiece — make the three
        // compartments.
        (PartKind::DiagonalOutsideWall, 10),
        (PartKind::Door, 2),
        (PartKind::Wall, 24),
        // The spine, bow to reactor, and the branches to every consumer.
        (PartKind::PowerConduit, 53),
    ] {
        assert_eq!(design.count(kind), want, "{kind:?}");
    }
    // The bow is pointed: nothing of the ship in the two corners of the
    // grid the cut takes off, and the bridge is inside the cut.
    let grid = design.grid();
    for tile in [(2, 1), (3, 2), (17, 1), (16, 2), (2, 4), (17, 4)] {
        assert!(!grid.occupied(tile), "{tile:?} should be off the ship");
    }
    assert!(grid.has_floor((9, 4)), "the pilot's spot is deck");
    for (resource, units) in PLAYTEST_CARGO {
        assert_eq!(design.carrying(resource), units, "{resource:?}");
    }
    // A full tank and not a unit less.
    assert_eq!(
        design.carrying(ResourceId::Fuel),
        design.capacity(Storage::FuelTank)
    );
}

/// The other half is `ship_self_check` in `crates/ship`, as for the
/// reference: the simulation's ship has to hash the same on both targets.
#[test]
fn the_playtest_ship_hashes_to_the_number_it_is_pinned_to() {
    use crate::fixture::{PLAYTEST_HASH, playtest_ship};
    let design = playtest_ship();
    assert_eq!(
        design_hash(&design),
        PLAYTEST_HASH,
        "the playtest ship has moved: it now hashes to {:#018x} with {} parts",
        design_hash(&design),
        design.parts.len()
    );
    assert_ne!(PLAYTEST_HASH, REFERENCE_HASH[0]);
}

/// The ship the design phase opens with is the playtest ship, whole, on the
/// lobby's grid: every part came across, it is as valid there as it was on
/// its own, and it is given rather than bought.
#[test]
fn the_playtest_ship_moves_onto_a_bigger_grid_whole() {
    use crate::fixture::{AREA, PLAYTEST_CARGO, PLAYTEST_PARTS, playtest_ship_on};
    for area in [AREA, 30, 40, 60] {
        let design = playtest_ship_on(area).expect("it fits");
        assert_eq!(design.build_area, area);
        assert_eq!(design.parts.len() as u32, PLAYTEST_PARTS, "on {area}");
        let issues = validate(&design, 1);
        assert!(issues.is_empty(), "on {area}: {issues:?}");
        for (resource, units) in PLAYTEST_CARGO {
            assert_eq!(design.carrying(resource), units);
        }
        // Given: the crew's pool is untouched by what was already there.
        let pool = 120_000;
        let budget = Budget::with_gift(pool, &design);
        assert_eq!(budget.remaining(&design), pool);
        // And a part taken off is money in hand, as any removal is.
        let engine = design
            .parts
            .iter()
            .find(|p| p.kind == PartKind::Engine)
            .unwrap()
            .id;
        let fewer = apply(&design, &budget, Edit::Remove { part_id: engine }).unwrap();
        assert_eq!(
            budget.remaining(&fewer),
            pool + PartKind::Engine.def().price
        );
    }
    // Too small a grid is no ship, not half of one.
    assert!(playtest_ship_on(AREA - 1).is_none());
    assert!(playtest_ship_on(8).is_none());
}

/// Deck plating brings its own frame: one edit on a bare tile is structure
/// and floor, priced as both; on a framed tile it is the floor alone; and a
/// refusal of either half leaves nothing behind.
#[test]
fn plating_lays_its_own_frame() {
    let budget = rich();
    let design = ShipDesign::new(10);
    let plated = apply(&design, &budget, Edit::Plate { origin: (3, 3) }).unwrap();
    assert_eq!(plated.count(PartKind::Structure), 1);
    assert_eq!(plated.count(PartKind::Floor), 1);
    assert_eq!(
        Budget::spent(&plated),
        PartKind::Structure.def().price + PartKind::Floor.def().price
    );

    // On frame that is already there, only the deck goes down.
    let framed = place(&design, &budget, PartKind::Structure, (4, 4), Rotation::R0).unwrap();
    let both = apply(&framed, &budget, Edit::Plate { origin: (4, 4) }).unwrap();
    assert_eq!(both.count(PartKind::Structure), 1);
    assert_eq!(both.count(PartKind::Floor), 1);

    // Deck already there: refused as the deck would be, and the frame count
    // does not move.
    assert_eq!(
        apply(&plated, &budget, Edit::Plate { origin: (3, 3) }).err(),
        Some(EditError::DuplicateFloor)
    );
    // Out of bounds, and nothing at all.
    assert_eq!(
        apply(&design, &budget, Edit::Plate { origin: (10, 3) }).err(),
        Some(EditError::OutOfBounds)
    );
    // Too poor for the frame: nothing goes down, not even the frame.
    let broke = Budget::new(PartKind::Structure.def().price);
    assert_eq!(
        apply(&design, &broke, Edit::Plate { origin: (3, 3) }).err(),
        Some(EditError::Unaffordable)
    );
}

// --- the port ---------------------------------------------------------------

/// The flyer's airlock stands in the starboard skin, two tiles tall, and opens
/// to starboard: that is the port, and its face is half a tile outside the
/// hull. The reference has no airlock and so no port.
#[test]
fn the_port_is_the_airlock_and_it_opens_onto_space() {
    let design = flyer(2);
    let port = crate::dock::port(&design).expect("the flyer has an airlock");
    let airlock = design
        .parts
        .iter()
        .find(|p| p.kind == PartKind::Airlock)
        .unwrap();
    assert_eq!(port.part_id, airlock.id);
    assert_eq!(port.outward, (1, 0), "it opens to starboard");
    let t = TILE as f64;
    // Two tiles at (18, 11) and (18, 12): the centre is the seam between them.
    assert_eq!(port.centre, (18.5 * t, 12.0 * t));
    // The face is the end of the collar, half a tile past the skin.
    assert_eq!(port.face(), (19.5 * t, 12.0 * t));

    assert_eq!(crate::dock::port(&reference(2)), None);
}

/// An airlock with hull on every side of it is a door to nowhere, and a
/// design whose only airlock is one has no port rather than a port that
/// opens into its own deck.
#[test]
fn an_airlock_buried_in_the_hull_is_no_port() {
    let mut design = floored(10, (1, 1), (9, 9));
    design = put(design, PartKind::Airlock, (4, 4));
    assert_eq!(crate::dock::port(&design), None);
}

// --- the exhaust --------------------------------------------------------------

/// An engine fires aft, and what is aft of it has to be space. Inside the
/// hull with deck behind it, it is an error; flush with the stern, its
/// bell over the edge of the ship, it is not — and turned, "aft" turns
/// with it.
#[test]
fn an_engine_has_to_fire_into_space() {
    use crate::validate::{exhaust_blocked, exhaust_tiles};
    // A decked square with an engine in the middle: three tiles of deck
    // straight behind it.
    let inside = put(floored(12, (1, 1), (11, 11)), PartKind::Engine, (4, 4));
    let engine = inside
        .parts
        .iter()
        .find(|p| p.kind == PartKind::Engine)
        .unwrap();
    assert_eq!(exhaust_tiles(engine), vec![(4, 7), (5, 7)]);
    assert!(exhaust_blocked(engine, &inside.grid()));
    let codes: Vec<u32> = validate(&inside, 1)
        .into_iter()
        .filter(|i| i.code == IssueCode::ExhaustBlocked.code())
        .map(|i| i.severity as u32)
        .collect();
    assert_eq!(
        codes,
        vec![Severity::Error as u32],
        "an engine in a room is an error"
    );

    // At the stern, its last row on the last row of frame: nothing behind.
    let stern = put(floored(12, (1, 1), (11, 11)), PartKind::Engine, (4, 8));
    let engine = stern
        .parts
        .iter()
        .find(|p| p.kind == PartKind::Engine)
        .unwrap();
    assert_eq!(exhaust_tiles(engine), vec![(4, 11), (5, 11)]);
    assert!(!exhaust_blocked(engine, &stern.grid()));
    assert!(
        !validate(&stern, 1)
            .iter()
            .any(|i| i.code == IssueCode::ExhaustBlocked.code())
    );

    // Turned a quarter, it fires to the west: the same engine at the west
    // edge is fine and in the middle is not.
    let west = place(
        &floored(12, (1, 1), (11, 11)),
        &rich(),
        PartKind::Engine,
        (1, 4),
        Rotation::R90,
    )
    .unwrap();
    let engine = west
        .parts
        .iter()
        .find(|p| p.kind == PartKind::Engine)
        .unwrap();
    assert_eq!(exhaust_tiles(engine), vec![(0, 4), (0, 5)]);
    assert!(!exhaust_blocked(engine, &west.grid()));
    let middle = place(
        &floored(12, (1, 1), (11, 11)),
        &rich(),
        PartKind::Engine,
        (5, 4),
        Rotation::R90,
    )
    .unwrap();
    let engine = middle
        .parts
        .iter()
        .find(|p| p.kind == PartKind::Engine)
        .unwrap();
    assert!(exhaust_blocked(engine, &middle.grid()));

    // And a part that does not push has no exhaust to speak of.
    let bunk = put(floored(12, (1, 1), (11, 11)), PartKind::Bunk, (4, 4));
    let bunk = bunk
        .parts
        .iter()
        .find(|p| p.kind == PartKind::Bunk)
        .unwrap();
    assert!(exhaust_tiles(bunk).is_empty());
}

/// An engine in a corner of the hull, with frame on three sides, still has
/// a use spot: the ring only needs one tile of deck. With deck on no side
/// at all it is a `UseSpotBlocked` like anything else.
#[test]
fn an_engine_needs_one_side_free_not_every_side() {
    // Decked square, engine flush with the stern in the corner: the ring
    // has deck to the west and the north, frame to the east, space to the
    // south.
    let corner = put(floored(12, (1, 1), (11, 11)), PartKind::Engine, (9, 8));
    let issues = validate(&corner, 1);
    assert!(
        !issues
            .iter()
            .any(|i| i.code == IssueCode::UseSpotBlocked.code()),
        "an engine with deck on one side is reachable"
    );
    assert!(
        !issues
            .iter()
            .any(|i| i.code == IssueCode::ExhaustBlocked.code())
    );

    // Walled in on every side but the stern: nowhere to stand.
    let mut walled = corner.clone();
    for tile in [(8u32, 8u32), (8, 9), (8, 10), (9, 7), (10, 7)] {
        walled = put(walled, PartKind::Wall, tile);
    }
    let issues = validate(&walled, 1);
    assert!(
        issues
            .iter()
            .any(|i| i.code == IssueCode::UseSpotBlocked.code()),
        "an engine nobody can get at should say so"
    );
}

// --- power ----------------------------------------------------------------

/// The column, said out loud: who makes it, who holds it, who draws it —
/// and that the essentials are among the drawers, or the brownout rule
/// would be keeping alive something that was never on.
#[test]
fn power_is_made_held_and_drawn_where_it_is_meant_to_be() {
    use crate::parts::{BATTERY_CHARGE, REACTOR_OUTPUT, essential};
    let making: Vec<PartKind> = PartKind::ALL
        .iter()
        .copied()
        .filter(|k| k.def().supplies())
        .collect();
    assert_eq!(making, vec![PartKind::Reactor]);
    assert_eq!(PartKind::Reactor.def().power, REACTOR_OUTPUT);

    let holding: Vec<PartKind> = PartKind::ALL
        .iter()
        .copied()
        .filter(|k| k.def().stores())
        .collect();
    assert_eq!(holding, vec![PartKind::Battery]);
    assert_eq!(PartKind::Battery.def().charge, BATTERY_CHARGE);

    let drawing: Vec<(PartKind, f64)> = PartKind::ALL
        .iter()
        .copied()
        .filter(|k| k.def().draws())
        .map(|k| (k, -k.def().power))
        .collect();
    assert_eq!(
        drawing,
        vec![
            (PartKind::Door, 1.0),
            (PartKind::ColdStore, 5.0),
            (PartKind::HydroBay, 15.0),
            (PartKind::Helm, 5.0),
            (PartKind::LifeSupport, 20.0),
            (PartKind::SensorArray, 10.0),
            (PartKind::Smelter, 40.0),
            (PartKind::Workbench, 15.0),
            (PartKind::Armoury, 10.0),
        ],
    );
    for kind in PartKind::ALL {
        if essential(kind) {
            assert!(
                kind.def().draws(),
                "{kind:?} is essential and draws nothing"
            );
        }
    }
    assert!(essential(PartKind::LifeSupport));
    assert!(essential(PartKind::Door));
    assert!(!essential(PartKind::HydroBay));
}

/// A reactor with conduit under it, a run to a cold store, and a second
/// cold store the run never reaches: the first is powered and the second
/// is what the warning points at. Then the reactor comes off, and the run
/// is a network nobody is on.
#[test]
fn a_consumer_is_powered_by_conduit_under_it_on_a_run_to_a_reactor() {
    use crate::parts::REACTOR_OUTPUT;
    use crate::power::{is_powered, networks, unpowered};
    let mut design = floored(10, (1, 1), (9, 9));
    design = put(design, PartKind::Reactor, (2, 2));
    design = put(design, PartKind::ColdStore, (6, 2));
    design = put(design, PartKind::ColdStore, (6, 6));
    let reactor = design
        .parts
        .iter()
        .find(|p| p.kind == PartKind::Reactor)
        .unwrap()
        .id;
    let near = design
        .parts
        .iter()
        .find(|p| p.kind == PartKind::ColdStore && p.origin == (6, 2))
        .unwrap()
        .id;
    let far = design
        .parts
        .iter()
        .find(|p| p.kind == PartKind::ColdStore && p.origin == (6, 6))
        .unwrap()
        .id;

    // Nothing wired: both in the dark, and there is no network at all.
    assert!(networks(&design).is_empty());
    assert_eq!(unpowered(&design), vec![near, far]);
    assert_eq!(
        all_codes(&design, 0)
            .iter()
            .filter(|&&c| c == IssueCode::Unpowered.code())
            .count(),
        1
    );

    // Conduit along row 2 from beside the reactor to the near store. A
    // run to the store with no reactor on it is not a live network, so the
    // store is unpowered whether or not there is wire under it.
    for x in 4..=6 {
        design = put(design, PartKind::PowerConduit, (x, 2));
    }
    assert_eq!(networks(&design).len(), 1);
    assert!(!networks(&design)[0].live());
    assert_eq!(unpowered(&design), vec![near, far]);

    // One more tile, under the reactor's own footprint, and it is live.
    design = put(design, PartKind::PowerConduit, (3, 2));
    let nets = networks(&design);
    assert_eq!(nets.len(), 1);
    assert!(nets[0].live());
    assert_eq!(nets[0].parts, vec![reactor, near]);
    assert_eq!(nets[0].supply, REACTOR_OUTPUT);
    assert_eq!(nets[0].draw, 5.0);
    assert_eq!(nets[0].storage, 0.0);
    assert!(is_powered(&design, near));
    assert!(!is_powered(&design, far));
    assert_eq!(unpowered(&design), vec![far]);
    let issue = validate(&design, 0)
        .into_iter()
        .find(|i| i.code == IssueCode::Unpowered.code())
        .expect("the far store should be warned about");
    assert_eq!(issue.severity, Severity::Warning);
    assert_eq!(issue.parts, vec![far]);
    assert_eq!(issue.tiles, vec![(6, 6)]);

    // Take the reactor away and the run goes dark; the store is still on
    // it, and still unpowered.
    let dark = apply(&design, &rich(), Edit::Remove { part_id: reactor }).unwrap();
    assert!(!networks(&dark)[0].live());
    assert_eq!(unpowered(&dark), vec![near, far]);
}

/// Two runs of conduit, each under one tile of the reactor and never
/// laid between: the reactor is the join, and it is one network making
/// one reactor's worth — not two making two.
#[test]
fn a_part_joins_the_conduit_under_it_into_one_network() {
    use crate::parts::REACTOR_OUTPUT;
    use crate::power::networks;
    let mut design = floored(10, (1, 1), (9, 9));
    design = put(design, PartKind::Reactor, (4, 4));
    // Left run: down column 4 from the reactor's top-left tile.
    for y in 1..=4 {
        design = put(design, PartKind::PowerConduit, (4, y));
    }
    // Right run: from the reactor's bottom-right tile to the edge.
    for x in 5..=8 {
        design = put(design, PartKind::PowerConduit, (x, 5));
    }
    let nets = networks(&design);
    assert_eq!(nets.len(), 1, "{nets:?}");
    assert_eq!(nets[0].supply, REACTOR_OUTPUT);
    assert_eq!(nets[0].tiles.len(), 8);

    // The same two runs under a fuel tank instead of a reactor are two
    // networks: a tank is not a wire.
    let mut design = floored(10, (1, 1), (9, 9));
    design = put(design, PartKind::FuelTank, (4, 4));
    for y in 1..=4 {
        design = put(design, PartKind::PowerConduit, (4, y));
    }
    for x in 5..=8 {
        design = put(design, PartKind::PowerConduit, (x, 5));
    }
    assert_eq!(networks(&design).len(), 2);
}

/// Six life supports on one reactor draw exactly what it makes and are
/// not short; a seventh is, and the warning is on that run, with
/// everything on it. A battery on the run holds the number and changes
/// nothing about the warning.
#[test]
fn a_network_drawing_more_than_it_makes_is_warned_about_per_run() {
    use crate::parts::REACTOR_OUTPUT;
    use crate::power::{budget, networks};
    let mut design = floored(20, (1, 1), (19, 19));
    design = put(design, PartKind::Reactor, (2, 2));
    for x in 2..=17 {
        design = put(design, PartKind::PowerConduit, (x, 2));
    }
    for i in 0..6u32 {
        design = put(design, PartKind::LifeSupport, (5 + 2 * i, 2));
    }
    assert!(!all_codes(&design, 0).contains(&IssueCode::PowerShort.code()));
    assert!(!all_codes(&design, 0).contains(&IssueCode::Unpowered.code()));
    assert_eq!(budget(&design).draw, 120.0);
    assert_eq!(
        budget(&design).draw,
        REACTOR_OUTPUT,
        "the test leans on this"
    );

    design = put(design, PartKind::LifeSupport, (17, 2));
    let nets = networks(&design);
    assert!(nets[0].short());
    let issue = validate(&design, 0)
        .into_iter()
        .find(|i| i.code == IssueCode::PowerShort.code())
        .expect("a short network should be warned about");
    assert_eq!(issue.severity, Severity::Warning);
    assert_eq!(issue.parts.len(), 8);
    assert_eq!(issue.tiles, nets[0].tiles);
    assert_eq!(budget(&design).draw, 140.0);
    assert_eq!(budget(&design).supply, REACTOR_OUTPUT);

    design = put(design, PartKind::Battery, (2, 5));
    design = put(design, PartKind::PowerConduit, (2, 3));
    design = put(design, PartKind::PowerConduit, (2, 4));
    design = put(design, PartKind::PowerConduit, (2, 5));
    assert!(all_codes(&design, 0).contains(&IssueCode::PowerShort.code()));
    assert_eq!(budget(&design).storage, crate::parts::BATTERY_CHARGE);

    // A second reactor on a run of its own with nothing on it counts for
    // nothing: the budget is over live networks, but the short one is
    // still short.
    design = put(design, PartKind::Reactor, (10, 10));
    design = put(design, PartKind::PowerConduit, (10, 10));
    assert_eq!(budget(&design).supply, 2.0 * REACTOR_OUTPUT);
    assert!(all_codes(&design, 0).contains(&IssueCode::PowerShort.code()));
}

/// Both fixtures are wired: nothing aboard either is in the dark, one
/// network each, and the playtest ship's figures are the ones the
/// reactor's output was chosen against.
#[test]
fn the_fixtures_are_wired() {
    use crate::fixture::playtest_ship;
    use crate::power::{budget, networks, unpowered};
    for &crew in CREWS.iter() {
        for design in [reference(crew), flyer(crew)] {
            assert!(unpowered(&design).is_empty(), "{:?}", unpowered(&design));
            assert_eq!(networks(&design).len(), 1);
            assert!(!networks(&design)[0].short());
        }
    }
    let design = playtest_ship();
    assert!(unpowered(&design).is_empty(), "{:?}", unpowered(&design));
    let nets = networks(&design);
    assert_eq!(nets.len(), 1, "{nets:?}");
    let power = budget(&design);
    assert_eq!(power.supply, 120.0);
    // Life support, the helm, the array, the cold store, the bay, two
    // doors, the smelter and the workbench.
    assert_eq!(power.draw, 112.0);
    assert_eq!(power.storage, crate::parts::BATTERY_CHARGE);
    let codes = all_codes(&design, 1);
    assert!(!codes.contains(&IssueCode::Unpowered.code()));
    assert!(!codes.contains(&IssueCode::PowerShort.code()));
}

// --- recipes --------------------------------------------------------------

/// The table, said out loud: what each makes, at what, and the mass rule —
/// conserved at the workbench, lost only at the smelter, gained nowhere.
#[test]
fn every_recipe_holds_together() {
    use crate::recipes::{RECIPES, at, recipes_are_sound};
    assert!(recipes_are_sound());
    assert_eq!(RECIPES.len(), 6);

    let smelt = &RECIPES[0];
    assert_eq!(smelt.station, PartKind::Smelter);
    assert_eq!(smelt.inputs, &[(ResourceId::Ore, 2)]);
    assert_eq!(smelt.output, (ResourceId::Metal, 1));
    assert!(smelt.vents);
    assert!(
        smelt.output_mass() < smelt.input_mass(),
        "the slag is vented"
    );

    let components = &RECIPES[1];
    assert_eq!(components.station, PartKind::Workbench);
    assert_eq!(components.output, (ResourceId::Components, 4));
    assert!(!components.vents);
    assert_eq!(components.output_mass(), components.input_mass());

    let emitter = &RECIPES[2];
    assert_eq!(emitter.station, PartKind::Workbench);
    assert_eq!(emitter.output, (ResourceId::Emitter, 1));
    assert!(
        emitter
            .inputs
            .iter()
            .any(|&(id, _)| id == ResourceId::Galvum)
    );
    assert_eq!(emitter.output_mass(), emitter.input_mass());

    assert_eq!(at(PartKind::Smelter).count(), 1);
    assert_eq!(at(PartKind::Workbench).count(), 2);
    assert_eq!(at(PartKind::Armoury).count(), 3);
    assert_eq!(at(PartKind::Hob).count(), 0);
    // The three the armoury makes, and what they are made of: the handgun
    // is the one thing that wants an emitter, so it is the one thing that
    // wants galvum.
    let handgun = &RECIPES[3];
    assert_eq!(handgun.output, (ResourceId::Handgun, 1));
    assert!(
        handgun
            .inputs
            .iter()
            .any(|&(id, _)| id == ResourceId::Emitter)
    );
    assert_eq!(handgun.output_mass(), handgun.input_mass());
    assert_eq!(RECIPES[4].output, (ResourceId::Vest, 1));
    assert_eq!(RECIPES[5].output, (ResourceId::Medkit, 1));
    for r in &RECIPES[3..] {
        assert_eq!(r.station, PartKind::Armoury);
        assert!(!r.vents);
        assert_eq!(r.output_mass(), r.input_mass(), "{:?}", r.output);
    }
    // Every station draws, so every one of them stops in a brownout.
    for r in RECIPES.iter() {
        assert!(r.station.def().draws(), "{:?}", r.station);
    }
}
