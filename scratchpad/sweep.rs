// Sweeping up: the one errand that undoes a mess.
//
// Everything else aboard either makes a mess or gets out of its way. This is
// the chain that puts it right — broom out of its locker, a few tiles at a
// time, broom away — and the thing to check is that it actually converges:
// a deck that has been fouled ends up clean, and not by teleporting the dirt
// away but by a Bim walking to each tile with a broom in its hands.

include!("modules.rs");

use bim::{CREW, PLAYER};
use game::Game;
use math::vec2;
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

    // --- one tile, by hand --------------------------------------------------

    let mut deck = filth::Filth::new(math::Rect::from_corners(
        vec2(0.0, 0.0),
        vec2(room::ROOM_W, room::ROOM_H),
    ));
    let spot = vec2(300.0, 300.0);
    check!("a clean deck wants no sweeping", deck.dirty_tiles() == 0, deck.dirty_tiles());
    check!("and offers no tile", deck.worst_tile(spot, |_| true).is_none());
    deck.sick_on(spot);
    check!("a fouled tile wants sweeping", deck.dirty_tiles() == 1, deck.dirty_tiles());
    check!("and is the one offered", deck.worst_tile(spot, |_| true).is_some());
    deck.sweep(spot);
    check!("sweeping it puts it right", deck.dirty_tiles() == 0, deck.dirty_tiles());
    check!("and takes the stain with it", deck.kind_at(spot) == filth::Mess::None);
    check!("so nothing is offered again", deck.worst_tile(spot, |_| true).is_none());

    // The nearer of two, when there is a choice, so the Bim works outwards
    // rather than crossing the room for the worst one every time.
    deck.sick_on(vec2(120.0, 120.0));
    deck.sick_on(vec2(760.0, 500.0));
    let from = vec2(140.0, 140.0);
    let picked = deck.worst_tile(from, |_| true).unwrap();
    check!(
        "of two as bad as each other it picks the nearer",
        (picked - from).len() < 300.0,
        format!("{picked:?}")
    );

    // --- the whole errand ---------------------------------------------------
    //
    // Foul a stretch of deck and leave them to it. Nobody is told to clean;
    // it is what they do with time they have nothing better to spend.

    let mut game = Game::new(5, 960.0, 640.0);
    let before = mess_up(&mut game, 14);
    check!("the deck starts filthy", before >= 10, before);

    let mut ever_held_broom = false;
    let mut ever_swept = false;
    let mut cleaned_by = 0;
    for f in 0..(4 * FRAMES_PER_DAY) {
        game.update(STEP);
        for w in 0..CREW {
            if game.holds_broom_for_probe(w) {
                ever_held_broom = true;
            }
            // JOB_CLEAN
            if game.activity(w) == 13 {
                ever_swept = true;
            }
        }
        if game.dirty_tiles() == 0 {
            cleaned_by = f;
            break;
        }
    }
    check!("somebody gets the broom out", ever_held_broom);
    check!("and it reads as sweeping", ever_swept);
    check!(
        "and the deck comes up clean",
        game.dirty_tiles() == 0,
        game.dirty_tiles()
    );
    println!(
        "       {before} fouled tiles cleared in {:.0} game minutes",
        cleaned_by as f32 * STEP
    );
    // Not instant: it is a Bim walking to each tile with a broom, so it has to
    // take the better part of an hour rather than a frame or two.
    check!(
        "by walking there, not by magic",
        cleaned_by as f32 * STEP > 60.0,
        cleaned_by
    );

    // --- and it is *not* remembered ------------------------------------------
    //
    // Sweeping used to go in the diary, tile count and all. It does not any
    // more: the diary keeps only what went wrong, and a deck that got swept is
    // the opposite of that. Asserted rather than merely dropped, because
    // "nothing was written down" is exactly the sort of thing that comes back
    // by accident the next time somebody adds a `What`.

    let mut entries = 0;
    for w in 0..CREW {
        entries += game.memory_len(w);
    }
    check!(
        "a clean-up is not worth a diary entry",
        entries == 0,
        format!("{entries} entries")
    );

    // --- one broom ----------------------------------------------------------

    let mut game = Game::new(8, 960.0, 640.0);
    mess_up(&mut game, 20);
    let mut both_at_once = 0;
    for _ in 0..(3 * FRAMES_PER_DAY) {
        game.update(STEP);
        if (0..CREW).filter(|&w| game.holds_broom_for_probe(w)).count() > 1 {
            both_at_once += 1;
        }
    }
    check!("there is only one broom", both_at_once == 0, both_at_once);

    // --- it is the last thing they do ---------------------------------------
    //
    // Sweeping must never come before eating, sleeping or the heads. Foul the
    // deck *and* starve them, and the meal has to win.

    let mut game = Game::new(2, 960.0, 640.0);
    mess_up(&mut game, 20);
    // Empty enough to be past the food threshold from the off.
    game.spend_for_probe(PLAYER, 1, 1.0);
    let mut swept_before_eating = false;
    let mut ate = false;
    for _ in 0..(FRAMES_PER_DAY / 2) {
        game.update(STEP);
        match game.activity(PLAYER) {
            13 if !ate => swept_before_eating = true,
            1 | 10 | 12 => ate = true,
            _ => {}
        }
        if ate {
            break;
        }
    }
    check!("a hungry Bim eats before it sweeps", !swept_before_eating);
    check!("and does eat", ate);

    // --- a week with a broom ------------------------------------------------

    for seed in [1, 11, 101] {
        let mut game = Game::new(seed, 960.0, 640.0);
        let mut worst = [f32::MAX; CREW];
        for _ in 0..(7 * FRAMES_PER_DAY) {
            game.update(STEP);
            for w in 0..CREW {
                worst[w] = worst[w].min(game.health(w));
            }
        }
        for w in 0..CREW {
            check!(
                format!("seed {seed}, crew {w}: alive and well after a week"),
                game.is_alive(w) && worst[w] > 90.0,
                worst[w]
            );
        }
        println!("       seed {seed}: {} dirty tiles left after a week", game.dirty_tiles());
    }

    println!();
    if fails > 0 { println!("{fails} FAILED"); std::process::exit(1); }
    println!("all passed");
}

/// Foul a run of tiles across the open deck and say how many took.
fn mess_up(game: &mut Game, want: u32) -> u32 {
    let mut done = 0;
    for y in (60..540).step_by(filth::TILE as usize) {
        for x in (60..800).step_by(filth::TILE as usize) {
            if done >= want {
                return game.dirty_tiles();
            }
            let at = vec2(x as f32, y as f32);
            // Only tiles a body can actually get to. Some of the deck is
            // deck and still unreachable — the corner past the end of the
            // counter, hemmed in by the bunk — and fouling one of those would
            // be staging a mess nobody can sweep and then calling it a bug.
            if game.spot_at(at.x, at.y) == room::SPOT_DECK && game.can_reach_for_probe(0, at) {
                game.foul_for_probe(at);
                done += 1;
            }
        }
    }
    game.dirty_tiles()
}
