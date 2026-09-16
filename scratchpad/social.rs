// Company: the fifth need, and what going without it does.
//
// Two halves, and they want testing very differently.
//
// The **ordinary** half is two Bims who talk to each other twice a day and are
// never lonely. That runs itself: start a game, leave it a week, and check
// nobody ever reaches the first stage.
//
// The **other** half takes ten game days to reach and is the part nobody will
// ever see by accident. Waiting for it is not an option — so the solitude
// clock is pinned forward with `leave_alone_for_probe`, re-pinned every frame
// so that the other Bim striking up a conversation cannot quietly reset it,
// and what is watched is the fallout. Pinning rather than switching autonomy
// off matters: a Bim with autonomy off does not eat either, and a run that
// starves its subject is measuring starvation.

include!("modules.rs");

use bim::{CREW, PLAYER};
use clock::{DAY, HOUR};
use game::Game;
use math::vec2;
use memory::What;
use rng::Rng;
use social::{Loneliness, Solitude};

const STEP: f32 = 1.0 / 60.0;
const FRAMES_PER_DAY: u32 = 24 * 60 * 60;
/// Where company sits in `Need::ALL`, which is how the host indexes it.
const COMPANY: u32 = 4;
const JOB_CHAT: u32 = game::JOB_CHAT;

fn main() {
    let mut fails = 0;
    macro_rules! check {
        ($what:expr, $ok:expr, $extra:expr) => {
            if $ok { println!("  ok   {}", $what); }
            else { println!("  FAIL {} — {}", $what, $extra); fails += 1; }
        };
        ($what:expr, $ok:expr) => { check!($what, $ok, "") };
    }

    // --- the clock, by hand -------------------------------------------------

    let mut rng = Rng::new(5);
    let mut alone = Solitude::new();
    check!("a Bim in company is not lonely", alone.stage() == Loneliness::None);
    check!("and works at its usual rate", alone.stage().works_at() == 1.0);

    for (days, want) in [
        (2.9, Loneliness::None),
        (3.0, Loneliness::Desocialized),
        (4.9, Loneliness::Desocialized),
        (5.0, Loneliness::Severe),
        (6.9, Loneliness::Severe),
        (7.0, Loneliness::Isolated),
        (30.0, Loneliness::Isolated),
    ] {
        alone.set_alone_for(days * DAY);
        check!(
            format!("{days} days alone is stage {}", want.stage()),
            alone.stage() == want,
            alone.stage().stage()
        );
    }

    alone.set_alone_for(4.0 * DAY);
    check!("a Bim gone without drags", alone.stage().works_at() < 1.0);
    check!(
        "by a tenth, and no more",
        (alone.stage().works_at() - 0.90).abs() < 0.001,
        alone.stage().works_at()
    );
    alone.talked();
    check!("and a word puts it all back", alone.stage() == Loneliness::None);
    check!("right back to nothing", alone.alone_for() == 0.0, alone.alone_for());

    // Giving up: nothing at all before the tenth day, then three per cent an
    // hour and three more for every day after.
    for (days, want) in [(9.9, 0.0), (10.0, 0.03), (10.9, 0.03), (11.0, 0.06), (12.0, 0.09)] {
        alone.set_alone_for(days * DAY);
        check!(
            format!("at {days} days the chance of giving up is {want} an hour"),
            (alone.despair_per_hour() - want).abs() < 0.0005,
            alone.despair_per_hour()
        );
    }

    // --- what each stage actually does, counted -----------------------------
    //
    // Run a day of frames at each stage with the clock pinned, and count.

    let a_day = |days: f32, sitting_allowed: bool, rng: &mut Rng| {
        let mut alone = Solitude::new();
        let (mut brooded, mut broke, mut hurt, mut gave_up) = (0, 0, 0, 0);
        for _ in 0..FRAMES_PER_DAY {
            alone.set_alone_for(days * DAY);
            let out = alone.update(STEP * clock::MINUTES_PER_SECOND, sitting_allowed, rng);
            brooded += out.brooded as u32;
            broke += out.broke_down as u32;
            hurt += out.hurt_itself as u32;
            gave_up += out.gave_up as u32;
        }
        (brooded, broke, hurt, gave_up)
    };

    let quiet = a_day(1.0, true, &mut rng);
    check!("a Bim with company has no bad days", quiet == (0, 0, 0, 0), format!("{quiet:?}"));

    // Six a day at four hours apart, give or take the one that falls exactly
    // on the boundary of the count: a day of frames is 1440 minutes and the
    // sixth brood is due at minute 1440, which is the frame after the last one.
    let low = a_day(3.5, true, &mut rng);
    check!(
        "desocialized: broods every four hours and nothing worse",
        (5..=6).contains(&low.0) && low.1 == 0 && low.2 == 0 && low.3 == 0,
        format!("{low:?}")
    );

    let severe = a_day(5.5, true, &mut rng);
    check!(
        "severe: still broods, and now sits down roughly every five hours",
        (5..=6).contains(&severe.0) && (2..=9).contains(&severe.1) && severe.2 == 0,
        format!("{severe:?}")
    );
    let seated = a_day(5.5, false, &mut rng);
    check!(
        "and never sits down when there is nowhere to sit down",
        seated.1 == 0,
        format!("{seated:?}")
    );

    let isolated = a_day(7.5, true, &mut rng);
    check!(
        "isolated: hurts itself every four hours",
        (5..=6).contains(&isolated.2) && isolated.3 == 0,
        format!("{isolated:?}")
    );

    let despairing = a_day(10.5, true, &mut rng);
    check!(
        "and past ten days it may stop altogether",
        despairing.3 > 0,
        format!("{despairing:?}")
    );

    // --- a week aboard, left to themselves -----------------------------------

    for seed in [1, 7, 42] {
        let mut game = Game::new(seed, 960.0, 640.0);
        let mut chats = [0u32; CREW];
        let mut talking = [false; CREW];
        let mut worst = [0.0f32; CREW];
        let mut lowest_bar = [1.0f32; CREW];
        let mut bubbles = 0;
        for _ in 0..(7 * FRAMES_PER_DAY) {
            game.update(STEP);
            for w in 0..CREW {
                let now = game.activity(w) == JOB_CHAT;
                if now && !talking[w] {
                    chats[w] += 1;
                }
                talking[w] = now;
                worst[w] = worst[w].max(game.days_alone(w));
                lowest_bar[w] = lowest_bar[w].min(game.need_level(w, COMPANY));
                if game.chat_topic(w) != 0 {
                    bubbles += 1;
                }
            }
        }
        for w in 0..CREW {
            check!(
                format!("seed {seed}, crew {w}: gets talked to, about twice a day"),
                (10..=20).contains(&chats[w]),
                chats[w]
            );
            check!(
                format!("seed {seed}, crew {w}: and is never lonely for it"),
                worst[w] < 3.0 && game.loneliness(w) == 0,
                format!("{:.2} days at worst", worst[w])
            );
            check!(
                format!("seed {seed}, crew {w}: the bar goes down and comes back"),
                lowest_bar[w] < 0.5 && game.need_level(w, COMPANY) > 0.0,
                format!("{:.2} at worst", lowest_bar[w])
            );
        }
        // Exactly one of them holds the floor at a time: two bubbles over two
        // Bims a body's width apart would sit on top of each other.
        check!(format!("seed {seed}: somebody is visibly saying something"), bubbles > 0);
        // And both remember it.
        for w in 0..CREW {
            check!(
                format!("seed {seed}, crew {w}: writes the conversations down"),
                (0..game.memory_len(w)).any(|i| game.memory_what(w, i) == What::Talked.code())
            );
        }
        println!("       seed {seed}: {chats:?} conversations in a week");
    }

    // --- one of them left out of it ------------------------------------------

    for (days, name) in [(3.5, "desocialized"), (5.5, "severe"), (7.5, "isolated")] {
        let mut game = Game::new(11, 960.0, 640.0);
        let mut sat = false;
        for _ in 0..(2 * FRAMES_PER_DAY) {
            // Re-pinned every frame: the other Bim will come over for a word
            // and that would put the clock straight back to nothing.
            game.leave_alone_for_probe(PLAYER, days);
            game.update(STEP);
            sat |= game.broken_down_for_probe(PLAYER);
        }
        let diary = |what: What| {
            (0..game.memory_len(PLAYER)).filter(|&i| game.memory_what(PLAYER, i) == what.code()).count()
        };
        check!(
            format!("{name}: the stage is what the clock says"),
            game.loneliness(PLAYER) > 0,
            game.loneliness(PLAYER)
        );
        check!(
            format!("{name}: two days of it fill the diary with low moments"),
            diary(What::FeltLow) >= 8,
            diary(What::FeltLow)
        );
        check!(
            format!("{name}: sits down on the deck only past five days"),
            sat == (days >= 5.0),
            format!("sat: {sat}")
        );
        check!(
            format!("{name}: hurts itself only past seven"),
            (diary(What::HurtSelf) > 0) == (days >= 7.0),
            diary(What::HurtSelf)
        );
        if days >= 7.0 {
            check!(
                format!("{name}: and it costs health"),
                game.health(PLAYER) < 100.0,
                game.health(PLAYER)
            );
        }
        // The other one is fine throughout: this is a clock on a body, and
        // pinning one Bim's must not reach the other's.
        check!(
            format!("{name}: the other one is untouched"),
            game.loneliness(1) == 0 && game.health(1) > 90.0,
            format!("stage {} health {:.0}", game.loneliness(1), game.health(1))
        );
    }

    // --- and past ten days it ends ------------------------------------------

    let mut died = 0;
    for seed in [3, 13, 23] {
        let mut game = Game::new(seed, 960.0, 640.0);
        for _ in 0..(3 * FRAMES_PER_DAY) {
            game.leave_alone_for_probe(PLAYER, 10.5);
            game.update(STEP);
            if !game.is_alive(PLAYER) {
                died += 1;
                break;
            }
        }
    }
    // Three per cent an hour is a median of about a day, so surviving three of
    // them is a one-in-a-thousand run: all three lasting would mean the roll
    // is not happening at all.
    check!("past ten days alone, a Bim gives up", died >= 2, format!("{died} of 3"));

    // --- and the tenth longer ------------------------------------------------
    //
    // Measured as a walk, because a walk is the one thing whose length can be
    // read off a frame count without anything else in the way.

    let clear = timed_walk(0.0);
    let dragging = timed_walk(4.0);
    let ratio = dragging as f32 / clear as f32;
    check!(
        "a Bim with nobody to talk to walks a tenth slower",
        (ratio - 1.0 / 0.90).abs() < 0.06,
        format!("{clear} frames clear, {dragging} lonely — {ratio:.3}x")
    );
    println!("       {clear} frames clear, {dragging} lonely ({ratio:.3}x)");

    println!();
    if fails > 0 { println!("{fails} FAILED"); std::process::exit(1); }
    println!("all passed");
}

/// Frames for one Bim to walk a fixed line across the open deck, with its
/// solitude clock pinned at `days`.
///
/// Both crew are recruited and autonomy is off, so nothing else moves and
/// nothing else is being measured. The clock is still pinned every frame —
/// `bear_the_solitude` runs before the errand half of the frame and is not
/// gated on autonomy, which is exactly what makes this readable.
fn timed_walk(days: f32) -> u32 {
    let mut game = Game::new(6, 960.0, 640.0);
    game.set_autonomous(false);
    for w in 0..CREW {
        game.recruit_for_probe(w, true);
    }
    let from = game.put_for_probe(PLAYER, vec2(180.0, 420.0));
    let to = vec2(560.0, 420.0);
    assert!(game.send_for_probe(PLAYER, to), "no route for the walk");
    for f in 0..(2 * 60 * 60) {
        game.leave_alone_for_probe(PLAYER, days);
        game.update(STEP);
        if (game.bim_pos(PLAYER) - to).len() < 8.0 {
            return f;
        }
    }
    let _ = (from, HOUR);
    panic!("the walk never finished");
}
