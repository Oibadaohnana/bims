// Native probe: drives Game and Filth directly, no browser and no wasm.
//
// Three things this round added that only the simulation can answer for: that
// a tile remembers what was spilt on it and keeps the worst of it, that the
// readout Game hands the host reports a real accident, and that the rest
// trigger actually sends the Bim to bed when the timetable will not.

include!("modules.rs");

use filth::{Filth, Mess};
use bim::PLAYER;
use game::Game;
use math::{Rect, vec2};
use room::{ROOM_H, ROOM_W, SPOT_DECK, SPOT_HEADS_DECK};

const STEP: f32 = 1.0 / 60.0;
/// Frames in a game day: one game minute a second at 1x, sixty frames a second.
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

    // --- a tile remembers what landed on it ------------------------------

    let deck = Rect::from_corners(vec2(0.0, 0.0), vec2(ROOM_W, ROOM_H));
    let mut tiles = Filth::new(deck);
    let here = vec2(200.0, 200.0);
    let next = vec2(200.0 + filth::TILE, 200.0);

    check!("a clean tile has nothing on it", tiles.kind_at(here) == Mess::None);
    check!("a clean tile is not fouled", tiles.depth_at(here) == 0.0, tiles.depth_at(here));

    tiles.wet(here);
    check!("a wet tile says wet", tiles.kind_at(here) == Mess::Wet);
    let wet = tiles.depth_at(here);
    check!("a wet tile is part fouled", wet > 0.0 && wet < 1.0, wet);

    // Being sick on a tile already wet leaves it reading as the worse of the
    // two, not the later of the two.
    tiles.sick_on(here);
    check!("sick on a wet tile reads as sick", tiles.kind_at(here) == Mess::Sick);
    check!("and it is fully fouled", tiles.depth_at(here) >= 1.0, tiles.depth_at(here));

    // And a later, lesser mess does not talk it back down.
    tiles.wet(here);
    check!("a wet patch does not undo sick", tiles.kind_at(here) == Mess::Sick);

    check!("the next tile along is still clean", tiles.kind_at(next) == Mess::None);

    // --- the readout reports a real accident -----------------------------
    //
    // Autonomy off, so the Bim never takes itself to the heads: the restroom
    // need empties, it holds on for an hour, and then it does not.

    let mut game = Game::new(3, 960.0, 640.0);
    game.set_autonomous(false);
    let mut fouled = None;
    for _ in 0..(2 * FRAMES_PER_DAY) {
        game.update(STEP);
        let p = game.bim_pos(PLAYER);
        if game.spot_mess(p.x, p.y) != 0 {
            fouled = Some((p, game.spot_mess(p.x, p.y), game.spot_mess_depth(p.x, p.y)));
            break;
        }
    }
    match fouled {
        Some((p, kind, depth)) => {
            check!("an accident leaves something under the Bim", kind >= 1 && kind <= 3, kind);
            check!("and the tile is fouled to match", depth > 0.0, depth);
            let spot = game.spot_at(p.x, p.y);
            check!(
                "and it happened somewhere that counts as deck",
                spot == SPOT_DECK || spot == SPOT_HEADS_DECK,
                spot
            );
        }
        None => {
            println!("  FAIL nothing was ever spilt in two days");
            fails += 1;
        }
    }

    // --- the rest trigger sends the Bim to bed ---------------------------
    //
    // Every hour painted as "anything", so nothing but the trigger can put the
    // Bim in bed. Before this round that meant it never went at all.

    let mut game = Game::new(7, 960.0, 640.0);
    for hour in 0..24 {
        game.set_schedule_slot(hour, 0);
    }
    check!("rest is watched by default", game.need_trigger_on(0));
    check!(
        "and at a tenth by default",
        (game.need_trigger(0) - 0.10).abs() < 1e-6,
        game.need_trigger(0)
    );

    let mut went_to_bed = false;
    for _ in 0..(3 * FRAMES_PER_DAY) {
        game.update(STEP);
        // JOB_SLEEP
        if game.activity(PLAYER) == 4 {
            went_to_bed = true;
            break;
        }
    }
    check!(
        "the rest trigger puts the Bim to bed",
        went_to_bed,
        game.need_level(PLAYER, 0)
    );

    // Switched off, it never does — and the need still empties, which is the
    // point: switching a trigger off stops the errand, not the draining.
    let mut game = Game::new(7, 960.0, 640.0);
    for hour in 0..24 {
        game.set_schedule_slot(hour, 0);
    }
    game.set_need_trigger_on(0, false);
    let mut slept = false;
    let mut lowest = f32::MAX;
    for _ in 0..(3 * FRAMES_PER_DAY) {
        game.update(STEP);
        lowest = lowest.min(game.need_level(PLAYER, 0));
        if game.activity(PLAYER) == 4 {
            slept = true;
            break;
        }
    }
    check!("switched off, it never does", !slept, game.need_level(PLAYER, 0));
    // Not "emptied", and not the level at the end either. A Bim that far gone
    // nods off where it stands, and a nod-off claws back about as much rest as
    // the wait for it costs — so the level does not settle anywhere in
    // particular, it oscillates, and where it happens to be on the last frame
    // is an accident of how the day fell out. (It moved from 0.08 to 0.33 on a
    // change that only made the Bim walk further.) The low-water mark is the
    // claim worth making: the need really did run past the level the Bim would
    // have acted on, and it still never went to bed.
    check!(
        "the need still fell past the trigger",
        lowest < game.need_trigger(0),
        lowest
    );
    check!("and it is paying for it", game.drowsiness(PLAYER) > 0, game.drowsiness(PLAYER));

    // --- the harvest is carried, not teleported --------------------------
    //
    // A ripe tray must leave the store where it was until the Bim has walked
    // the plant up the room and put it in. What this watches for is the gap:
    // the moment the tray empties, and the later moment the store goes up.

    let mut game = Game::new(5, 960.0, 640.0);
    // Ask for far more than is aboard, so the bay is working from the start.
    game.set_target(manager::Stock::Veg, manager::MOST);
    game.set_target(manager::Stock::Tofu, manager::MOST);

    let mut lifted_at = None;
    let mut stored_at = None;
    let mut ripe_before = game.hydro_ripe();
    let mut store_before = game.store_veg() + game.store_tofu();
    let mut carried_past = 0;
    for frame in 0..(6 * FRAMES_PER_DAY) {
        game.update(STEP);
        let ripe_now = game.hydro_ripe();
        let store_now = game.store_veg() + game.store_tofu();
        // A tray that was ripe and is not: something came out of it.
        if ripe_now < ripe_before && lifted_at.is_none() {
            lifted_at = Some(frame);
        }
        if store_now > store_before && lifted_at.is_some() && stored_at.is_none() {
            stored_at = Some(frame);
        }
        if lifted_at.is_some() && stored_at.is_none() {
            carried_past += 1;
        }
        ripe_before = ripe_now;
        store_before = store_now;
        if stored_at.is_some() {
            break;
        }
    }

    match (lifted_at, stored_at) {
        (Some(lifted), Some(stored)) => {
            check!("a ripe tray gets lifted", true);
            check!("and the store only goes up afterwards", stored > lifted, stored);
            // Far enough for a walk across the room and a fridge door, not
            // the one or two frames a rounding error would give.
            check!(
                "with a real walk in between",
                carried_past > 60,
                format!("{carried_past} frames")
            );
        }
        _ => {
            println!("  FAIL nothing was ever lifted and stored — {lifted_at:?} {stored_at:?}");
            fails += 1;
        }
    }

    // --- the highlight ----------------------------------------------------
    //
    // Every code the readout can name is either a place with a rect or one of
    // the two that are everywhere — deck and bulkhead — and the ring has to
    // land on the thing it names.

    let layout = room::Room::new();
    let mut ringed = 0;
    for spot in 0..=15 {
        match layout.spot_rect(spot) {
            Some(area) => {
                ringed += 1;
                let back = layout.spot(area.center());
                check!(
                    format!("spot {spot} rings itself"),
                    back == spot,
                    format!("centre reads as {back}")
                );
            }
            None => check!(
                format!("spot {spot} has no single place, rightly"),
                spot == room::SPOT_NOTHING || spot == SPOT_DECK || spot == room::SPOT_BULKHEAD
                    || spot == SPOT_HEADS_DECK,
                spot
            ),
        }
    }
    check!("most things have a place to ring", ringed >= 10, ringed);

    // --- a week of it, with everything left as it comes ------------------
    //
    // The rest trigger is new, and a new reason to go to bed is a new way to
    // be in bed instead of eating. This is the check that it has not quietly
    // starved the Bim: default settings, default timetable, nobody touching
    // anything, for a week.

    for seed in [1, 11, 101] {
        let mut game = Game::new(seed, 960.0, 640.0);
        let mut worst_health = f32::MAX;
        let mut nights = 0;
        let mut was_asleep = false;
        for _ in 0..(7 * FRAMES_PER_DAY) {
            game.update(STEP);
            worst_health = worst_health.min(game.health(PLAYER));
            let asleep = game.activity(PLAYER) == 4;
            if asleep && !was_asleep {
                nights += 1;
            }
            was_asleep = asleep;
            if !game.is_alive(PLAYER) {
                break;
            }
        }
        check!(format!("seed {seed}: alive after a week"), game.is_alive(PLAYER));
        check!(
            format!("seed {seed}: never seriously hurt"),
            worst_health > 90.0,
            worst_health
        );
        check!(
            format!("seed {seed}: slept most nights"),
            nights >= 5,
            nights
        );
        check!(
            format!("seed {seed}: not starving at the end"),
            game.malnutrition(PLAYER) == 0,
            game.malnutrition(PLAYER)
        );
    }

    println!();
    if fails > 0 {
        println!("{fails} FAILED");
        std::process::exit(1);
    }
    println!("all passed");
}
