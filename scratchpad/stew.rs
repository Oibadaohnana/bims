// Stew for the store: does the target make a job, does the job make a stew,
// and does a hungry Bim eat it?
//
// The manager's stew target is the whole of the demand. Below it, with a
// vegetable and a block of tofu on the shelf, `work_on_offer` puts
// `Job::Cook` on the list — stew for the store is the cook row's, beside a
// meal — and the Bim chops one of each, cooks them, packs the pot into a
// tub and puts it in the cold store. At the target the shelf stops asking. And a Bim that is then hungry warms one up rather than
// cooking from raw, which is what keeps the loop going.
//
// Read off the counts and the chosen job, not off where the Bim is seen
// standing — see the note at the top of `priority.rs` for why.

include!("modules.rs");

use game::Game;
use manager::Stock;
use work::Job;

const STEP: f32 = 1.0 / 60.0;
const FRAMES_PER_HOUR: u32 = 60 * 60;

/// Where the food need sits in `Need::ALL`, which is how `spend_for_probe`
/// indexes it.
const FOOD: u32 = 1;

fn main() {
    let mut fails = 0;
    macro_rules! check {
        ($what:expr, $ok:expr, $extra:expr) => {
            if $ok { println!("  ok   {}", $what); }
            else { println!("  FAIL {} — {}", $what, $extra); fails += 1; }
        };
        ($what:expr, $ok:expr) => { check!($what, $ok, "") };
    }

    // --- nothing happens until somebody asks --------------------------------

    let mut game = Game::new(3, 960.0, 640.0);
    check!("the store starts with no stew", game.store_stew() == 0);
    check!("and nobody is asked for any", game.target(Stock::Stew) == 0);
    check!("so the shelf asks for none", !game.wants_stew_for_probe());
    check!(
        "and cooking is not on offer",
        !game.work_on_offer_for_probe(0).contains(&Job::Cook.code()),
        format!("{:?}", game.work_on_offer_for_probe(0))
    );

    // --- a target is a job -------------------------------------------------

    game.set_target(Stock::Stew, 2);
    check!("the target is what was asked", game.target(Stock::Stew) == 2);
    check!(
        "and the other two are untouched",
        game.target(Stock::Veg) == game.target_veg() && game.target(Stock::Tofu) == 10,
        format!("{} {}", game.target(Stock::Veg), game.target(Stock::Tofu))
    );
    check!("short of it, the shelf asks for one", game.wants_stew_for_probe());
    check!(
        "and cooking is on offer to a Bim that is not hungry",
        game.work_on_offer_for_probe(0).contains(&Job::Cook.code()),
        format!("{:?}", game.work_on_offer_for_probe(0))
    );

    // Run until the first stew is on the shelf. A batch is a few game
    // minutes of work; give it a few hours in case a meal or the heads get
    // in first.
    let (veg0, tofu0) = (game.store_veg(), game.store_tofu());
    let mut seen_cooking = false;
    let mut first_at = None;
    for frame in 0..(6 * FRAMES_PER_HOUR) {
        game.update(STEP);
        if game.activity(0) == game::JOB_STEW || game.activity(1) == game::JOB_STEW {
            seen_cooking = true;
        }
        if game.store_stew() >= 1 {
            first_at = Some(frame);
            break;
        }
    }
    check!("somebody is seen cooking for the store", seen_cooking);
    check!("and a stew reaches the shelf", first_at.is_some());
    check!(
        "made of one vegetable and one block of tofu",
        game.store_veg() + 1 <= veg0 && game.store_tofu() + 1 <= tofu0,
        format!("veg {veg0}→{}, tofu {tofu0}→{}", game.store_veg(), game.store_tofu())
    );
    check!(
        "and the pot is empty afterwards, not left for leftovers",
        game.pot_servings() == 0,
        game.pot_servings()
    );

    // --- and stops at the target -----------------------------------------------

    let mut at_target = None;
    for frame in 0..(12 * FRAMES_PER_HOUR) {
        game.update(STEP);
        if game.store_stew() >= 2 {
            at_target = Some(frame);
            break;
        }
    }
    check!("the second one follows", at_target.is_some());
    // The row itself may still be on offer — it is hunger's as well — so
    // what is read is the shelf's half of it.
    check!("and at the target the shelf stops asking", !game.wants_stew_for_probe());
    // Give it a while: nothing should push the count past the target.
    let mut most = game.store_stew();
    for _ in 0..(6 * FRAMES_PER_HOUR) {
        game.update(STEP);
        most = most.max(game.store_stew());
    }
    check!("nobody cooks a third", most <= 2, most);

    // --- a hungry Bim warms one up ------------------------------------------

    // Empty the food need so a meal is wanted now, and take away the other
    // reasons it could be met: no pot on the hob, so the only two answers
    // are the shelf and the knife.
    let stew_before = game.store_stew();
    check!("there is a pot on the shelf to be hungry for", stew_before > 0);
    game.spend_for_probe(0, FOOD, 1.0);
    // Measured from the moment the warming begins, not from now: the Bim
    // may be halfway through cooking a pot *for* the shelf when hunger
    // bites, and finishes that first — so the shelf can be one up before
    // it is one down.
    let mut warmed = false;
    let mut ate = false;
    let mut on_shelf_at_start = stew_before;
    let mut fewest = stew_before;
    for _ in 0..(4 * FRAMES_PER_HOUR) {
        game.update(STEP);
        if warmed {
            fewest = fewest.min(game.store_stew());
        } else if game.activity(0) == game::JOB_REHEAT {
            warmed = true;
            on_shelf_at_start = game.store_stew();
            fewest = on_shelf_at_start;
        }
        if warmed && game.activity(0) == 0 {
            ate = true;
            break;
        }
    }
    check!("the hungry one warms a stew up rather than cooking", warmed);
    check!("and finishes the meal", ate);
    // The other Bim may well have cooked the shelf back up meanwhile — that
    // is the loop working — so what is asserted is that a pot came off it.
    check!(
        "a pot came off the shelf",
        fewest < on_shelf_at_start,
        format!("{on_shelf_at_start}→{fewest}")
    );

    // --- and asks for nothing it cannot make -----------------------------------

    let mut bare = Game::new(3, 960.0, 640.0);
    bare.set_target(Stock::Stew, 5);
    bare.set_target(Stock::Tofu, 0);
    // Eat the tofu down: every bowl takes one. Rather than wait, ask the
    // store directly through the crop it takes.
    while bare.store_tofu() > 0 {
        bare.take_for_probe(hydro::Crop::Soy);
    }
    check!("with no tofu, the target asks for nothing", !bare.wants_stew_for_probe());
    check!(
        "and cooking is not on offer",
        !bare.work_on_offer_for_probe(0).contains(&Job::Cook.code()),
        format!("{:?}", bare.work_on_offer_for_probe(0))
    );

    if fails > 0 {
        println!("\n{fails} FAILED");
        std::process::exit(1);
    }
    println!("\nall passed");
}
