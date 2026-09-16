// Dirt that spreads: where it comes from, and how it gets about.
//
// Two new things to hold down. The dirty jobs — cooking, planting, lifting a
// crop — flick something onto the deck around the Bim as each step of them
// finishes, and boots carry what is already down from one tile to the next.
// Neither is an accident, which makes them the first mess aboard that nobody
// had to have a bad day to make.
//
// The arithmetic half is checked by hand, because it is the half that has to
// be exact: a quarter of the tile, moved rather than copied. The rest is a day
// aboard, where what matters is that the galley goes grubby on its own and the
// crew still get on top of it.

include!("modules.rs");

use bim::{CREW, PLAYER};
use filth::{BASELINE, Filth, Mess, TILE};
use game::Game;
use math::{Rect, vec2};
use rng::Rng;

const STEP: f32 = 1.0 / 60.0;
const FRAMES_PER_DAY: u32 = 24 * 60 * 60;

/// Every bit of dirt on the deck added up, in the tiles' own units. Spreading
/// *moves* dirt, so this is the number that must never grow while a Bim is
/// only walking about.
fn total_dirt(game: &Game) -> f32 {
    let mut sum = 0.0;
    let mut y = TILE * 0.5;
    while y < room::ROOM_H {
        let mut x = TILE * 0.5;
        while x < room::ROOM_W {
            sum += BASELINE - game.tile_filth(vec2(x, y));
            x += TILE;
        }
        y += TILE;
    }
    sum
}

/// Is there grime — the mess a dirty job makes, as against an accident —
/// anywhere on the deck right now?
fn grime_anywhere(game: &Game) -> bool {
    let mut y = TILE * 0.5;
    while y < room::ROOM_H {
        let mut x = TILE * 0.5;
        while x < room::ROOM_W {
            if game.spot_mess(x, y) == Mess::Grime.code() {
                return true;
            }
            x += TILE;
        }
        y += TILE;
    }
    false
}

fn deck() -> Filth {
    Filth::new(Rect::from_corners(
        vec2(0.0, 0.0),
        vec2(room::ROOM_W, room::ROOM_H),
    ))
}

fn main() {
    let mut fails = 0;
    macro_rules! check {
        ($what:expr, $ok:expr, $extra:expr) => {
            if $ok { println!("  ok   {}", $what); }
            else { println!("  FAIL {} — {}", $what, $extra); fails += 1; }
        };
        ($what:expr, $ok:expr) => { check!($what, $ok, "") };
    }

    // --- a quarter of a tile, moved ----------------------------------------
    //
    // The whole rule, done by hand. A boot out of a dirty tile takes a quarter
    // of what is on it to the tile it steps to, and the tile behind is a
    // quarter lighter for it. Conserved, not copied: a Bim pacing the galley
    // must not be able to multiply the deck's filth.

    let mut tiles = deck();
    let here = vec2(300.0, 300.0);
    let next = here + vec2(TILE, 0.0);
    tiles.sick_on(here);
    let start = BASELINE - tiles.at(here);

    // A quarter of the crossings do it, so roll until one does rather than
    // asserting on any one call. Each failed roll leaves the deck untouched,
    // which is itself worth knowing.
    let mut rng = Rng::new(4);
    let mut tries = 0;
    while !tiles.track(here, next, &mut rng) {
        tries += 1;
        assert!(tries < 1000, "track never fired in a thousand crossings");
        check!(
            "a crossing that does not spread leaves both tiles alone",
            (BASELINE - tiles.at(here) - start).abs() < 0.001 && tiles.at(next) >= BASELINE,
            tries
        );
        if tries > 2 {
            break;
        }
    }
    while !tiles.track(here, next, &mut rng) {}

    let behind = BASELINE - tiles.at(here);
    let ahead = BASELINE - tiles.at(next);
    check!(
        "the tile ahead gets a quarter of the dirt",
        (ahead - start * 0.25).abs() < 0.01,
        format!("{ahead:.2} of {start:.2}")
    );
    check!(
        "and the tile behind is exactly that much lighter",
        (behind + ahead - start).abs() < 0.01,
        format!("{behind:.2} + {ahead:.2} vs {start:.2}")
    );
    check!(
        "the smear is the same kind of mess as what it came off",
        tiles.kind_at(next) == Mess::Sick,
        tiles.kind_at(next).code()
    );

    // And on from there: a quarter of a quarter, so a trail thins fast enough
    // to die out rather than working its way across the ship.
    let third = next + vec2(TILE, 0.0);
    while !tiles.track(next, third, &mut rng) {}
    let far = BASELINE - tiles.at(third);
    check!(
        "and a step further on carries a quarter of that",
        (far - start * 0.0625).abs() < 0.05,
        format!("{far:.2} of {start:.2}")
    );

    // --- what does not happen ----------------------------------------------

    let mut tiles = deck();
    tiles.sick_on(here);
    let before = tiles.at(here);
    let mut rng = Rng::new(9);
    let mut moved = false;
    for _ in 0..500 {
        moved |= tiles.track(here, here + vec2(2.0, 2.0), &mut rng);
    }
    check!(
        "a step that stays on the same tile spreads nothing",
        !moved && tiles.at(here) == before
    );

    // A clean deck must cost nothing at all — and in particular must not draw
    // from `rng`. Every roll aboard comes off one stream, so a die thrown on a
    // frame that used to throw none reshuffles every later outcome in the run,
    // and the crew walk about on clean deck nearly all the time.
    let mut tiles = deck();
    let mut used = Rng::new(21);
    let mut spare = Rng::new(21);
    for i in 0..500 {
        let from = vec2(200.0 + (i % 5) as f32 * TILE, 200.0);
        tiles.track(from, from + vec2(TILE, 0.0), &mut used);
    }
    check!(
        "walking a clean deck draws nothing from the stream",
        used.unit() == spare.unit()
    );

    // --- a dirty job leaves something behind --------------------------------

    let mut tiles = deck();
    let mut rng = Rng::new(3);
    check!(
        "nothing is flicked where a body cannot go",
        !tiles.spatter(here, &mut rng, |_| false) && tiles.dirty_tiles() == 0
    );
    check!("and the deck is still clean", tiles.dirty_tiles() == 0);

    check!("a dirty job marks the deck", tiles.spatter(here, &mut rng, |_| true));
    check!(
        "with exactly one tile's worth",
        tiles.dirty_tiles() == 1,
        tiles.dirty_tiles()
    );
    // Somewhere in the block around the Bim, not necessarily underfoot.
    let mut stained = None;
    for dr in -2..=2 {
        for dc in -2..=2 {
            let at = here + vec2(dc as f32 * TILE, dr as f32 * TILE);
            if tiles.at(at) < BASELINE {
                stained = Some((at, tiles.kind_at(at)));
            }
        }
    }
    let (at, kind) = stained.expect("nothing was marked");
    check!(
        "within a tile of where the work was done",
        (at.x - here.x).abs() <= TILE * 1.5 && (at.y - here.y).abs() <= TILE * 1.5,
        format!("{at:?} vs {here:?}")
    );
    check!("and it reads as grime, not as an accident", kind == Mess::Grime);
    check!(
        "grime is worth the broom",
        tiles.worst_tile(here, |_| true).is_some()
    );
    // Least bad of the lot: grime tracked over a fouled tile must not talk the
    // readout back down.
    tiles.sick_on(at);
    tiles.spatter(at, &mut rng, |t| t == at);
    check!("but never talks a worse mess down", tiles.kind_at(at) == Mess::Sick);

    // --- boots leave a trail ------------------------------------------------
    //
    // Walk a Bim out of a fouled tile and across the open deck. The tiles it
    // crosses on the way should pick something up — nobody sweeping, nobody
    // having an accident, just dirt moved by a pair of feet.

    let mut game = Game::new(6, 960.0, 640.0);
    // Nobody decides anything for themselves: what is measured here is a walk
    // and nothing else. Recruited, so neither wanders off it either.
    game.set_autonomous(false);
    for w in 0..CREW {
        game.recruit_for_probe(w, true);
    }
    // Along the open middle of the deck. The room is `room::ROOM_W` wide, not
    // the canvas, and its far end is hemmed in by the counter and the second
    // bunk — a walk out to that is a wander and not a walk.
    //
    // A band of fouled deck across the middle of the lane rather than one
    // tile, because which row a body settles into is not something a probe
    // gets to decide: it is nudged by the other Bim, by the push-out, by a
    // waypoint being half a tile off. A band is crossed whichever row it uses.
    let west = vec2(180.0, 420.0);
    let east_end = vec2(560.0, 420.0);
    // Four rows deep, because ordering a Bim to `y` does not put it on that
    // row: the route is smoothed, the push-out nudges it, and it settles a
    // whole tile below where it was sent. A band one row deep missed the lane
    // entirely and the walk crossed nothing but clean deck.
    for x in (260..480).step_by(TILE as usize) {
        for y in (360..520).step_by(TILE as usize) {
            let at = vec2(x as f32, y as f32);
            if game.spot_at(at.x, at.y) == room::SPOT_DECK && game.can_reach_for_probe(PLAYER, at) {
                game.foul_for_probe(at);
            }
        }
    }
    let tiles_before = game.dirty_tiles();
    let dirt_before = total_dirt(&game);
    check!(
        "there is a band of fouled deck to cross",
        tiles_before >= 4,
        tiles_before
    );

    game.put_for_probe(PLAYER, west);
    // Back and forth, not once across. A crossing spreads a quarter of the
    // time, so one lap would be a coin toss dressed up as an assertion — and
    // the next change to land anywhere near the RNG stream would flip it.
    //
    // Turned round on *arrival*, measured as a distance, and never on
    // `is_walking`: a Bim that has just been handed a route is still turning
    // to face it and does not read as walking yet, so a loop that re-orders
    // whenever `is_walking` is false hands it a fresh route every frame and
    // it stands on the spot for ever, laps counting merrily up.
    let mut laps = 0;
    let mut target = east_end;
    game.send_for_probe(PLAYER, target);
    let mut patience = 0;
    for _ in 0..(40 * 60 * 60) {
        game.update(STEP);
        patience += 1;
        if (game.bim_pos(PLAYER) - target).len() < TILE * 0.5 || patience > 60 * 60 {
            target = if target == east_end { west } else { east_end };
            if !game.send_for_probe(PLAYER, target) {
                break;
            }
            patience = 0;
            laps += 1;
            if laps > 20 {
                break;
            }
        }
    }
    check!("it walked the deck", laps > 20, laps);
    check!(
        "walking through a mess spreads it",
        game.dirty_tiles() > tiles_before,
        format!("{} tiles, was {tiles_before}", game.dirty_tiles())
    );
    // And the whole point of moving it rather than copying it: a Bim pacing
    // the deck must not be able to make filth out of nothing.
    check!(
        "and never makes more of it than there was",
        total_dirt(&game) <= dirt_before + 0.5,
        format!("{:.0} vs {dirt_before:.0}", total_dirt(&game))
    );
    // The trail is fainter than what it came off, tile for tile: what spreads
    // is a share, so nothing downstream is ever as bad as its source.
    let mut worst_trail = 0.0f32;
    for x in (140..280).step_by(TILE as usize / 2) {
        worst_trail = worst_trail.max(BASELINE - game.tile_filth(vec2(x as f32, 420.0)));
    }
    check!(
        "and what it carried out is fainter than the band",
        worst_trail < 110.0,
        format!("{worst_trail:.1}")
    );

    // --- a day's work is dirty work ----------------------------------------
    //
    // Nobody is told to do anything. Cooking, planting and lifting on their own
    // have to put something on the deck — and the crew have to get it off
    // again, or a week aboard ends buried.
    //
    // Watched by the *kind* rather than by the count. An accident leaves tiles
    // too, and the crew are free to have one over three days; `Mess::Grime` is
    // the only thing on the deck that nobody had a bad day to make, so it is
    // the one reading that says this is the new machinery running.

    for seed in [1, 11, 42] {
        let mut game = Game::new(seed, 960.0, 640.0);
        let mut ever_dirty = 0;
        let mut ever_grime = false;
        for f in 0..(3 * FRAMES_PER_DAY) {
            game.update(STEP);
            ever_dirty = ever_dirty.max(game.dirty_tiles());
            // Sweeping takes it away again, so the deck has to be read as the
            // run goes rather than at the end of it. Once a minute or so is
            // plenty and keeps the sweep cheap.
            if f % 600 == 0 && !ever_grime {
                ever_grime = grime_anywhere(&game);
            }
        }
        check!(
            format!("seed {seed}: three days aboard dirty the deck by themselves"),
            ever_grime && ever_dirty > 0,
            format!("{ever_dirty} tiles at worst, grime seen: {ever_grime}")
        );
        check!(
            format!("seed {seed}: and the crew stay on top of it"),
            game.dirty_tiles() <= 6,
            game.dirty_tiles()
        );
        println!(
            "       seed {seed}: {} dirty tiles at worst, {} left after three days",
            ever_dirty,
            game.dirty_tiles()
        );
    }

    println!();
    if fails > 0 { println!("{fails} FAILED"); std::process::exit(1); }
    println!("all passed");
}
