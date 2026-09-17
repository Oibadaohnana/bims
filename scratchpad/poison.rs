// Food poisoning: a meal cooked in a dirty galley makes the Bim ill.
//
// Every tile within two of the hob with a real mess on it — a wetting or
// worse, not the stain a meal leaves — is a one-in-five
// chance the meal is bad, rolled the moment the pot finishes cooking or a
// bowl is filled. A Bim that
// eats from a bad pot is ill for two days: the restroom need runs three
// times as fast, and reaching nothing is the accident straight away rather
// than an hour of holding on.
//
// Read off the pot and the clocks, not off where the Bim is seen standing —
// see the note at the top of `priority.rs` for why.

include!("modules.rs");

use game::Game;
use math::vec2;
use memory::What;

const STEP: f32 = 1.0 / 60.0;
const FRAMES_PER_HOUR: u32 = 60 * 60;
const FRAMES_PER_DAY: u32 = 24 * FRAMES_PER_HOUR;

/// Where the food and restroom needs sit in `Need::ALL`, which is how
/// `spend_for_probe` and `need_level` index them.
const FOOD: u32 = 1;
const RESTROOM: u32 = 2;

/// The deck's tile, and how far round the hob the galley is judged.
const TILE: f32 = filth::TILE;
const REACH: i32 = 2;

fn main() {
    let mut fails = 0;
    macro_rules! check {
        ($what:expr, $ok:expr, $extra:expr) => {
            if $ok { println!("  ok   {}", $what); }
            else { println!("  FAIL {} — {}", $what, $extra); fails += 1; }
        };
        ($what:expr, $ok:expr) => { check!($what, $ok, "") };
    }

    /// Empty the food need and run until the Bim has eaten, saying whether
    /// the food was ever bad while it did.
    fn one_meal(game: &mut Game) -> (bool, bool) {
        game.spend_for_probe(0, FOOD, 1.0);
        let (mut bad_pot, mut ate) = (false, false);
        for _ in 0..(4 * FRAMES_PER_HOUR) {
            game.update(STEP);
            bad_pot |= game.food_is_bad();
            if game.need_level(0, FOOD) > 0.9 {
                ate = true;
                break;
            }
        }
        (bad_pot, ate)
    }

    // --- a clean galley cooks clean -----------------------------------------

    let mut game = Game::new(3, 960.0, 640.0);
    let (bad, ate) = one_meal(&mut game);
    check!("the Bim eats", ate);
    check!("and the food was never bad", !bad);
    check!("and nobody is ill", game.poisoning(0) == 0.0 && game.poisoning(1) == 0.0);

    // --- a filthy one very likely does not ---------------------------------
    //
    // Every tile round the hob fouled: a dozen and more rolls at one in
    // five, so one clean pot in a run of five seeds would be a fluke, and
    // five would be the roll not happening.

    let mut poisoned = 0;
    for seed in 1..=5u64 {
        let mut game = Game::new(seed, 960.0, 640.0);
        let hob = game.pot_pos_for_probe();
        for dr in -REACH..=REACH {
            for dc in -REACH..=REACH {
                game.foul_for_probe(hob + vec2(dc as f32 * TILE, dr as f32 * TILE));
            }
        }
        let (bad, ate) = one_meal(&mut game);
        if ate && bad && game.poisoning(0) > 0.0 {
            poisoned += 1;
        }
    }
    check!(
        "a meal out of a fouled galley makes the cook ill, most seeds",
        poisoned >= 3,
        format!("{poisoned} of 5")
    );

    // --- what being ill does ----------------------------------------------------
    //
    // Staged by hand, since the roll is the galley's business. Autonomy off
    // and the timetable wiped, so nobody goes to the heads or to bed and
    // what is measured is the drain and the accident, not the errand.

    let mut game = Game::new(4, 960.0, 640.0);
    game.set_autonomous(false);
    for hour in 0..24 {
        game.set_schedule_slot(hour, 0);
    }
    game.poison_for_probe(0);
    check!(
        "the illness is two days long",
        (game.poisoning(0) - 48.0).abs() < 0.01,
        game.poisoning(0)
    );
    check!(
        "and is written down",
        (0..game.memory_len(0)).any(|i| game.memory_what(0, i) == What::FoodPoisoning.code())
    );

    // Both start the hour with the same level; the ill one loses three
    // times what the well one does.
    let (a0, a1) = (game.need_level(0, RESTROOM), game.need_level(1, RESTROOM));
    for _ in 0..(FRAMES_PER_HOUR / 2) {
        game.update(STEP);
    }
    let (b0, b1) = (game.need_level(0, RESTROOM), game.need_level(1, RESTROOM));
    let (drop0, drop1) = (a0 - b0, a1 - b1);
    check!(
        "the restroom need runs three times as fast",
        drop1 > 0.0 && (drop0 / drop1 - 3.0).abs() < 0.05,
        format!("{drop0} against {drop1}")
    );

    // Reaching nothing is the accident, at once: no hour of holding on.
    let before = (0..game.memory_len(0))
        .filter(|&i| game.memory_what(0, i) == What::Accident.code())
        .count();
    game.spend_for_probe(0, RESTROOM, 1.0);
    for _ in 0..(2 * 60) {
        game.update(STEP);
    }
    let after = (0..game.memory_len(0))
        .filter(|&i| game.memory_what(0, i) == What::Accident.code())
        .count();
    check!("empty is an accident within two minutes", after > before, format!("{before}→{after}"));

    // And it passes. Two days on, well again.
    for _ in 0..(2 * FRAMES_PER_DAY + FRAMES_PER_HOUR) {
        game.update(STEP);
    }
    check!("two days on the Bim is well", game.poisoning(0) == 0.0, game.poisoning(0));

    if fails > 0 {
        println!("\n{fails} FAILED");
        std::process::exit(1);
    }
    println!("\nall passed");
}
