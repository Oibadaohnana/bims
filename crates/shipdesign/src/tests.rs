//! Native tests for the design rules.
//!
//! These run under `cargo test --target <native> -p shipdesign`, which is
//! what `nix flake check` does. The crate compiles for wasm as well, and
//! `crates/ship`'s `ship_self_check` export is the other half of the one
//! thing a native test cannot answer on its own: whether the two targets
//! hash a design the same way.

use physics::{Facing, ResourceId};

use crate::budget::{BASE_STOCKPILE, Budget, PER_MILLE, RESOURCE_COUNT};
use crate::design::{Edit, EditError, ShipDesign, apply, design_hash};
use crate::fixture::{CREWS, REFERENCE_HASH, REFERENCE_PARTS, reference};
use crate::mass::{acceleration, hull_mass, ship_mass};
use crate::parts::{
    Layer, PartKind, Rotation, TILE, covered, defs_are_sound, footprint, use_spots,
};
use crate::validate::{IssueCode, REQUIRED, Severity, has_errors, validate};

/// A budget with plenty in it, for the tests that are not about money.
fn rich() -> Budget {
    Budget::new([u32::MAX; RESOURCE_COUNT])
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

/// A square of deck, which almost everything else needs under it.
fn floored(side: u32, from: (u32, u32), to: (u32, u32)) -> ShipDesign {
    let mut design = ShipDesign::new(side);
    for y in from.1..to.1 {
        for x in from.0..to.0 {
            design = put(design, PartKind::Floor, (x, y));
        }
    }
    design
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
        assert!(def.mass > 0.0, "{kind:?} weighs nothing");
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
    }
    assert_eq!(TILE, 52);
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
        Ok(65)
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
fn most_things_need_deck_under_them_and_a_wall_does_not() {
    let design = floored(8, (0, 0), (4, 4));
    assert_eq!(
        place(&design, &rich(), PartKind::Hob, (5, 5), Rotation::R0),
        Err(EditError::MissingFloor)
    );
    // Half on, half off is still off.
    assert_eq!(
        place(&design, &rich(), PartKind::Worktop, (3, 1), Rotation::R0),
        Err(EditError::MissingFloor)
    );
    assert!(place(&design, &rich(), PartKind::Wall, (5, 5), Rotation::R0).is_ok());
}

#[test]
fn deck_cannot_be_laid_twice() {
    let design = floored(8, (0, 0), (4, 4));
    assert_eq!(
        place(&design, &rich(), PartKind::Floor, (2, 2), Rotation::R0),
        Err(EditError::DuplicateFloor)
    );
    assert!(place(&design, &rich(), PartKind::Floor, (4, 4), Rotation::R0).is_ok());
}

#[test]
fn what_the_stockpile_will_not_cover_is_refused() {
    // Three metal: two floor tiles, and then nothing.
    let budget = Budget::new([0, 3, 0, 0]);
    let mut design = ShipDesign::new(8);
    design = place(&design, &budget, PartKind::Floor, (0, 0), Rotation::R0).unwrap();
    design = place(&design, &budget, PartKind::Floor, (1, 0), Rotation::R0).unwrap();
    design = place(&design, &budget, PartKind::Floor, (2, 0), Rotation::R0).unwrap();
    assert_eq!(
        place(&design, &budget, PartKind::Floor, (3, 0), Rotation::R0),
        Err(EditError::Unaffordable)
    );
    // A hob needs components as well as metal, and there are none at all.
    assert_eq!(
        place(&design, &budget, PartKind::Hob, (0, 0), Rotation::R0),
        Err(EditError::Unaffordable)
    );
    assert_eq!(budget.remaining(&design), [0, 0, 0, 0]);
}

#[test]
fn deck_with_something_standing_on_it_stays_put() {
    let mut design = floored(8, (0, 0), (8, 8));
    design = put(design, PartKind::Hob, (3, 3));
    let under = design.grid().get(Layer::Floor, (3, 3));
    assert_ne!(under, 0);
    assert_eq!(
        apply(&design, &rich(), Edit::Remove { part_id: under }),
        Err(EditError::FloorUnderObject)
    );
    // Take the hob off and the plating comes up.
    let hob = design.grid().get(Layer::Object, (3, 3));
    let bare = apply(&design, &rich(), Edit::Remove { part_id: hob }).unwrap();
    assert!(apply(&bare, &rich(), Edit::Remove { part_id: under }).is_ok());
}

#[test]
fn removing_something_that_is_not_there_is_refused() {
    let design = floored(8, (0, 0), (2, 2));
    assert_eq!(
        apply(&design, &rich(), Edit::Remove { part_id: 999 }),
        Err(EditError::NoSuchPart)
    );
    // And the id is not reissued, so the second removal of one part is a
    // refusal rather than a hit on whatever took its place.
    let first = design.parts[0].id;
    let gone = apply(&design, &rich(), Edit::Remove { part_id: first }).unwrap();
    let back = place(&gone, &rich(), PartKind::Floor, (0, 0), Rotation::R0).unwrap();
    assert_ne!(back.parts.last().unwrap().id, first);
    assert_eq!(
        apply(&back, &rich(), Edit::Remove { part_id: first }),
        Err(EditError::NoSuchPart)
    );
}

// --- the budget -----------------------------------------------------------

#[test]
fn a_removal_hands_back_exactly_what_the_part_cost() {
    let budget = Budget::from_factor(PER_MILLE);
    let empty = ShipDesign::new(10);
    let before = budget.remaining(&empty);

    let floored = place(&empty, &budget, PartKind::Floor, (2, 2), Rotation::R0).unwrap();
    let with = place(&floored, &budget, PartKind::ColdStore, (2, 2), Rotation::R0).unwrap();
    let cost = budget.remaining(&floored);
    let after_place = budget.remaining(&with);
    for &(id, units) in PartKind::ColdStore.def().cost {
        assert_eq!(after_place[id as usize] + units, cost[id as usize]);
    }

    let store = with.grid().get(Layer::Object, (2, 2));
    let without = apply(&with, &budget, Edit::Remove { part_id: store }).unwrap();
    assert_eq!(budget.remaining(&without), cost);

    let bare = apply(
        &without,
        &budget,
        Edit::Remove {
            part_id: without.parts[0].id,
        },
    )
    .unwrap();
    assert_eq!(budget.remaining(&bare), before);
}

#[test]
fn remaining_never_goes_under_nothing() {
    // A design that arrived from somewhere the rules were not applied.
    let budget = Budget::new([0, 1, 0, 0]);
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
    assert_eq!(Budget::spent(&design)[ResourceId::Metal as usize], 5);
    assert_eq!(budget.remaining(&design), [0, 0, 0, 0]);
    assert!(!budget.affords(&design, PartKind::Floor.def().cost));
}

#[test]
fn the_lobby_factor_scales_the_stockpile() {
    assert_eq!(Budget::from_factor(PER_MILLE).stockpile, BASE_STOCKPILE);
    let half = Budget::from_factor(500).stockpile;
    let double = Budget::from_factor(2000).stockpile;
    for i in 0..RESOURCE_COUNT {
        assert_eq!(half[i], BASE_STOCKPILE[i] / 2);
        assert_eq!(double[i], BASE_STOCKPILE[i] * 2);
    }
    assert_eq!(Budget::from_factor(0).stockpile, [0; RESOURCE_COUNT]);
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

    // One tile of deck out on its own in the corner.
    let stray = put(design, PartKind::Floor, (19, 19));
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

    // Deck, then the galley.
    let mut one = empty.clone();
    for x in 2..6 {
        one = place(&one, &budget, PartKind::Floor, (x, 2), Rotation::R0).unwrap();
        one = place(&one, &budget, PartKind::Floor, (x, 3), Rotation::R0).unwrap();
    }
    one = place(&one, &budget, PartKind::Hob, (2, 2), Rotation::R0).unwrap();
    one = place(&one, &budget, PartKind::ColdStore, (5, 3), Rotation::R90).unwrap();

    // The same ship, laid out backwards, with a part placed and taken off
    // again in the middle of it.
    let mut two = empty.clone();
    for x in (2..6).rev() {
        two = place(&two, &budget, PartKind::Floor, (x, 3), Rotation::R0).unwrap();
        two = place(&two, &budget, PartKind::Floor, (x, 2), Rotation::R0).unwrap();
    }
    two = place(&two, &budget, PartKind::ColdStore, (5, 3), Rotation::R90).unwrap();
    two = place(&two, &budget, PartKind::Basin, (4, 2), Rotation::R0).unwrap();
    let basin = two.grid().get(Layer::Object, (4, 2));
    two = apply(&two, &budget, Edit::Remove { part_id: basin }).unwrap();
    two = place(&two, &budget, PartKind::Hob, (2, 2), Rotation::R0).unwrap();

    // The hob is one part in one place in both, and carries a different id in
    // each — which is the whole reason ids stay out of the hash.
    let hob = |d: &ShipDesign| d.grid().get(Layer::Object, (2, 2));
    assert_ne!(hob(&one), hob(&two), "the ids really do differ");
    assert_eq!(design_hash(&one), design_hash(&two));
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
fn the_hull_weighs_what_its_parts_weigh() {
    let design = put(
        put(floored(10, (2, 2), (6, 6)), PartKind::Hob, (2, 2)),
        PartKind::Engine,
        (4, 2),
    );
    // Sixteen floor tiles, a hob and an engine — added up by hand.
    let want = 16.0 * 5.0 + 30.0 + 400.0;
    assert_eq!(hull_mass(&design), want);

    // Two crew aboard and nothing in the hold.
    let mass = ship_mass(&design, 2).unwrap();
    assert_eq!(mass.get(), want + 2.0 * physics::PLAYER_MASS);

    // The stockpile left over is the station's and weighs nothing here.
    let budget = Budget::from_factor(PER_MILLE);
    assert!(budget.remaining(&design).iter().any(|&left| left > 0));
    assert_eq!(ship_mass(&design, 2).unwrap().get(), mass.get());
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
