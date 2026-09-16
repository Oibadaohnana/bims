// The character sheet: who a Bim is, and what it remembers.

include!("modules.rs");

use bim::{BORN_FROM, BORN_TO, CREW, PLAYER};
use game::Game;
use memory::What;

const STEP: f32 = 1.0 / 60.0;
const FRAMES_PER_DAY: u32 = 24 * 60 * 60;

fn main() {
    let mut fails = 0;
    macro_rules! check {
        ($what:expr, $ok:expr, $extra:expr) => {
            if $ok { println!("  ok   {}", $what); }
            else { println!("  FAIL {} — {}", $what, $extra); fails += 1; }
        };
        ($what:expr, $ok:expr) => { check!($what, $ok, "") };
    }

    // --- the calendar ------------------------------------------------------

    let game = Game::new(3, 960.0, 640.0);
    check!(
        "the game opens in 2400",
        game.clock_year() == clock::START_YEAR && clock::START_YEAR == 2400,
        game.clock_year()
    );
    check!(
        "on the first of January",
        game.clock_month() == 1 && game.clock_date() == 1,
        format!("{}/{}", game.clock_date(), game.clock_month())
    );
    // The months have to add up to the year, or every date past February is
    // wrong and the ages with them.
    let mut days = 0;
    let mut seen = vec![];
    for d in 0..clock::DAYS_IN_YEAR {
        let (m, date) = clock::month_and_date(d);
        seen.push((m, date));
        days += 1;
    }
    check!("a year is 365 days", days == 365);
    check!("and ends on the 31st of December", seen[364] == (12, 31), format!("{:?}", seen[364]));
    // Counting from zero: day 30 is the last of January, day 31 the first of
    // February. The month boundary is the thing worth pinning down — get it
    // off by one and every date past January is wrong, ages included.
    check!(
        "the last of January is day 30",
        seen[30] == (1, 31),
        format!("{:?}", seen[30])
    );
    check!(
        "and the first of February is day 31",
        seen[31] == (2, 1),
        format!("{:?}", seen[31])
    );

    // --- born somewhere sensible -------------------------------------------

    for seed in 1..=30u64 {
        let game = Game::new(seed, 960.0, 640.0);
        for w in 0..CREW {
            let y = game.born_year(w);
            if !(BORN_FROM..=BORN_TO).contains(&y) {
                println!("  FAIL seed {seed} crew {w} born in {y}");
                fails += 1;
            }
            let (m, d) = (game.born_month(w), game.born_date(w));
            if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
                println!("  FAIL seed {seed} crew {w} born on {d}/{m}");
                fails += 1;
            }
            // Age has to agree with the birth year to within the birthday.
            let age = game.age(w);
            let rough = clock::START_YEAR - y;
            if age != rough && age + 1 != rough {
                println!("  FAIL seed {seed} crew {w}: born {y}, age {age}");
                fails += 1;
            }
        }
    }
    check!("everyone is born between 2350 and 2380, on a real date", true);
    let game = Game::new(3, 960.0, 640.0);
    for w in 0..CREW {
        println!(
            "       crew {w}: born {}/{}/{}, aged {}",
            game.born_date(w), game.born_month(w), game.born_year(w), game.age(w)
        );
    }
    // Two Bims with the same birthday every game would be a rolled constant.
    let mut same = 0;
    for seed in 1..=30u64 {
        let g = Game::new(seed, 960.0, 640.0);
        if g.born_year(0) == g.born_year(1) && g.born_month(0) == g.born_month(1) {
            same += 1;
        }
    }
    check!("they are not born on the same day every game", same < 5, same);

    // --- the diary fills ---------------------------------------------------

    let mut game = Game::new(5, 960.0, 640.0);
    for _ in 0..(4 * FRAMES_PER_DAY) {
        game.update(STEP);
    }
    for w in 0..CREW {
        let n = game.memory_len(w);
        println!("       crew {w} remembers {n} things");
        check!(format!("crew {w} remembers its days"), n > 10, n);
        // Chronological, so the host can group by day without sorting.
        let mut last = (0, 0.0f32);
        let mut ordered = true;
        for i in 0..n {
            let here = (game.memory_day(w, i), game.memory_at(w, i));
            if here.0 < last.0 || (here.0 == last.0 && here.1 < last.1 - 0.001) {
                ordered = false;
            }
            last = here;
        }
        check!(format!("crew {w}'s diary is in order"), ordered);
        // Every entry has to be a code in one of the two blocks `What` uses:
        // the day's work, and the things that stand out. This only checks the
        // shape — that the host has *words* for each is asserted over in
        // `scratchpad/smoke.mjs`, which can see the table they live in.
        let mut unknown = 0;
        for i in 0..n {
            let what = game.memory_what(w, i);
            if !(1..=10).contains(&what) && !(20..=29).contains(&what) {
                unknown += 1;
            }
        }
        check!(format!("crew {w}'s entries are all nameable"), unknown == 0, unknown);
    }

    // The ordinary run of a day has to be in there.
    let mut game = Game::new(5, 960.0, 640.0);
    let mut kinds = std::collections::BTreeSet::new();
    for _ in 0..(6 * FRAMES_PER_DAY) {
        game.update(STEP);
    }
    for w in 0..CREW {
        for i in 0..game.memory_len(w) {
            kinds.insert(game.memory_what(w, i));
        }
    }
    println!("       kinds of thing remembered over six days: {kinds:?}");
    check!("they remember waking", kinds.contains(&What::Woke.code()));
    check!("they remember eating", kinds.contains(&What::Ate.code()) || kinds.contains(&What::AteLeftovers.code()));
    check!("they remember the heads", kinds.contains(&What::UsedHeads.code()));
    check!("they remember tending the bay", kinds.contains(&What::Tended.code()));

    // --- and the bad days --------------------------------------------------
    //
    // Left to go without, a Bim has to remember the worst of it — and the
    // other has to remember *seeing* it. That is the half of the diary the
    // player asked for: what he saw as well as what he did.

    let mut game = Game::new(6, 960.0, 640.0);
    game.set_autonomous(false);
    for hour in 0..24 {
        game.set_schedule_slot(hour, 0);
    }
    let mut kinds = std::collections::BTreeSet::new();
    let mut notable = 0;
    for _ in 0..(3 * FRAMES_PER_DAY) {
        game.update(STEP);
    }
    for w in 0..CREW {
        for i in 0..game.memory_len(w) {
            kinds.insert(game.memory_what(w, i));
            if game.memory_notable(w, i) {
                notable += 1;
            }
        }
    }
    println!("       kinds after three bad days: {kinds:?}  notable entries: {notable}");
    check!("an accident is remembered", kinds.contains(&What::Accident.code()));
    check!("being sick is remembered", kinds.contains(&What::WasSick.code()));
    check!("going hungry is remembered", kinds.contains(&What::Hungrier.code()));
    check!("going without sleep is remembered", kinds.contains(&What::Wearier.code()));
    check!(
        "and one of them remembers seeing the other's",
        kinds.contains(&What::SawAccident.code()) || kinds.contains(&What::SawSickness.code())
    );
    check!("the bad ones are marked as standing out", notable > 0, notable);

    // The book is bounded: a Bim left running does not grow for ever.
    let mut game = Game::new(6, 960.0, 640.0);
    game.set_autonomous(false);
    for hour in 0..24 {
        game.set_schedule_slot(hour, 0);
    }
    for _ in 0..(20 * FRAMES_PER_DAY) {
        game.update(STEP);
    }
    for w in 0..CREW {
        check!(
            format!("crew {w} forgets the oldest once the book is full"),
            game.memory_len(w) <= memory::KEEP as u32,
            game.memory_len(w)
        );
    }

    // --- selecting ----------------------------------------------------------

    let mut game = Game::new(3, 960.0, 640.0);
    check!("nobody is selected to begin with", game.selected().is_none());
    game.select_group(1);
    check!("group 1 is the player's Bim", game.selected() == Some(PLAYER));
    // Clicking the other one picks her instead — one at a time.
    let kate = game.bim_pos(1);
    game.drag_begin(kate.x, kate.y);
    game.drag_end(kate.x, kate.y);
    check!("clicking the other one selects her", game.selected() == Some(1), format!("{:?}", game.selected()));
    check!("and only her", !game.is_selected(PLAYER));
    // But she still takes no orders.
    let away = game.bim_pos(PLAYER);
    check!(
        "and an order to her is ignored",
        game.order_move(away.x, away.y) == 0,
        game.order_move(away.x, away.y)
    );
    game.clear_selection();
    check!("and Escape puts the panels away", game.selected().is_none());

    println!();
    if fails > 0 { println!("{fails} FAILED"); std::process::exit(1); }
    println!("all passed");
}
