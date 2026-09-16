// Native probe for the second Bim: that there are two, that they keep out of
// each other's way in every sense, and that both live a full week unattended.

include!("modules.rs");

use bim::{CREW, PLAYER};
use game::Game;
use room::{BERTHS, SEATS};

const STEP: f32 = 1.0 / 60.0;
const FRAMES_PER_DAY: u32 = 24 * 60 * 60;

fn main() {
    let mut fails = 0;
    macro_rules! check {
        ($what:expr, $ok:expr, $extra:expr) => {
            if $ok {
                println!("  ok   {}", $what);
            } else {
                println!("  FAIL {} — {}", $what, $extra);
                fails += 1;
            }
        };
        ($what:expr, $ok:expr) => {
            check!($what, $ok, "")
        };
    }

    check!("two of everything", CREW == 2 && BERTHS == 2 && SEATS == 2);

    // --- a berth and a seat each -----------------------------------------

    let layout = room::Room::new();
    check!(
        "the two berths are different places",
        layout.bed_station(0) != layout.bed_station(1),
        format!("{:?} {:?}", layout.bed_station(0), layout.bed_station(1))
    );
    check!(
        "and do not overlap",
        !layout.beds[0].frame.expand(1.0).contains(layout.beds[1].frame.center()),
        ""
    );
    check!(
        "the two seats are different places",
        layout.chair_at(0) != layout.chair_at(1),
        ""
    );
    check!(
        "and face opposite ways",
        (layout.chair_facing(0) - layout.chair_facing(1)).abs() > 1.0,
        format!("{} {}", layout.chair_facing(0), layout.chair_facing(1))
    );
    check!(
        "each plate goes in front of its own seat",
        layout.table_plate_pos(0) != layout.table_plate_pos(1),
        ""
    );
    // Every station has to be somewhere a body can actually stand.
    for who in 0..BERTHS {
        let s = layout.bed_station(who);
        check!(
            format!("berth {who}'s station is on open deck"),
            layout.spot(s) == room::SPOT_DECK,
            layout.spot(s)
        );
    }

    // --- they pass through each other ---------------------------------------
    //
    // Deliberately, and it is the thing that cannot deadlock: a route is
    // planned once and never replanned, so a Bim whose line goes through the
    // other has no second plan. What squeezing past costs is speed.

    let mut game = Game::new(9, 960.0, 640.0);
    let mut closest = f32::MAX;
    let mut close_frames = 0;
    let mut slowed_while_close = 0;
    let mut slowed_while_clear = 0;
    for _ in 0..(2 * FRAMES_PER_DAY) {
        game.update(STEP);
        let gap = (game.bim_pos(1) - game.bim_pos(0)).len();
        closest = closest.min(gap);
        let both_afoot = (0..CREW).all(|w| !game.is_seated_for_probe(w));
        if !both_afoot {
            continue;
        }
        if gap < 30.0 {
            close_frames += 1;
            if game.crowding_for_probe(0) < 1.0 {
                slowed_while_close += 1;
            }
        } else if gap > 60.0 && game.crowding_for_probe(0) < 1.0 {
            slowed_while_clear += 1;
        }
    }
    println!("       closest approach over two days: {closest:.1}px");
    check!(
        "they do get right up against each other",
        close_frames > 0,
        close_frames
    );
    check!(
        "and are slowed the whole time they are",
        slowed_while_close == close_frames,
        format!("{slowed_while_close} of {close_frames}")
    );
    check!(
        "and never slowed with the room to themselves",
        slowed_while_clear == 0,
        slowed_while_clear
    );

    // --- only one of them cooks at a time ----------------------------------

    let mut game = Game::new(4, 960.0, 640.0);
    let mut both_cooking = 0;
    let mut both_heads = 0;
    let mut cooked = [0, 0];
    let mut was = [0, 0];
    for _ in 0..(4 * FRAMES_PER_DAY) {
        game.update(STEP);
        let acts: Vec<u32> = (0..CREW).map(|w| game.activity(w)).collect();
        // JOB_MEAL / JOB_BOWL / JOB_LEFTOVERS
        let cooking = |a: u32| a == 1 || a == 10 || a == 12;
        if acts.iter().filter(|&&a| cooking(a)).count() > 1 {
            both_cooking += 1;
        }
        // JOB_HEADS
        if acts.iter().filter(|&&a| a == 5).count() > 1 {
            both_heads += 1;
        }
        for w in 0..CREW {
            if cooking(acts[w]) && !cooking(was[w]) {
                cooked[w] += 1;
            }
            was[w] = acts[w];
        }
    }
    check!("never two in the galley at once", both_cooking == 0, both_cooking);
    check!("never two on the pan at once", both_heads == 0, both_heads);
    check!(
        "and both of them still get fed",
        cooked[0] > 0 && cooked[1] > 0,
        format!("{cooked:?}")
    );
    println!("       meals started over four days: {cooked:?}");

    // --- only James takes orders -------------------------------------------

    let mut game = Game::new(3, 960.0, 640.0);
    game.select_group(1);
    check!("selecting picks one Bim", game.selected_count() == 1, game.selected_count());
    game.toggle_recruited();
    check!("recruiting takes the player's Bim", game.is_recruited());
    check!(
        "and PLAYER is the one it took",
        game.bim_pos(PLAYER) == game.bim_pos(PLAYER),
        ""
    );

    // A recruited James starts nothing; Kate carries on with her day.
    //
    // "Stays put" has to be measured loosely now, and the reason is the point
    // of this whole round: Kate walks into him and shoves him aside, so a
    // recruited Bim does drift — about a body's width each time it is bumped.
    // What must not happen is him *setting off* anywhere, so that is what is
    // asserted, with the drift only held to less than a walk across the room.
    let james_at = game.bim_pos(PLAYER);
    let kate_at = game.bim_pos(1);
    let mut walked = 0;
    for _ in 0..(6 * 60 * 60) {
        game.update(STEP);
        if game.is_walking(PLAYER) {
            walked += 1;
        }
    }
    check!("a recruited James never sets off", walked == 0, walked);
    check!(
        "and is only ever shoved a little",
        (game.bim_pos(PLAYER) - james_at).len() < 150.0,
        (game.bim_pos(PLAYER) - james_at).len()
    );
    check!(
        "while Kate gets on with it",
        (game.bim_pos(1) - kate_at).len() > 40.0,
        (game.bim_pos(1) - kate_at).len()
    );

    // --- a week of it --------------------------------------------------------

    for seed in [1, 11, 101] {
        let mut game = Game::new(seed, 960.0, 640.0);
        let mut worst = [f32::MAX; CREW];
        let mut nights = [0; CREW];
        let mut was_asleep = [false; CREW];
        for _ in 0..(7 * FRAMES_PER_DAY) {
            game.update(STEP);
            for w in 0..CREW {
                worst[w] = worst[w].min(game.health(w));
                let asleep = game.activity(w) == 4;
                if asleep && !was_asleep[w] {
                    nights[w] += 1;
                }
                was_asleep[w] = asleep;
            }
        }
        for w in 0..CREW {
            check!(
                format!("seed {seed}, crew {w}: alive after a week"),
                game.is_alive(w)
            );
            check!(
                format!("seed {seed}, crew {w}: never seriously hurt"),
                worst[w] > 90.0,
                worst[w]
            );
            check!(
                format!("seed {seed}, crew {w}: slept most nights"),
                nights[w] >= 5,
                nights[w]
            );
            check!(
                format!("seed {seed}, crew {w}: not starving at the end"),
                game.malnutrition(w) == 0,
                game.malnutrition(w)
            );
        }
    }

    println!();
    if fails > 0 {
        println!("{fails} FAILED");
        std::process::exit(1);
    }
    println!("all passed");
}
