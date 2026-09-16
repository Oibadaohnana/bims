// The order the work gets done in: does setting a priority change anything?
//
// The panel is checked by `work-check.mjs` — the boxes, the colours, the
// sorting. This is the other half: that the numbers reach the crew. A panel
// that sets a field nobody reads looks identical from the outside, and that is
// the failure this exists to catch.
//
// Two jobs are staged against each other — a filthy deck and an empty stomach
// — and what is read is which one the crew would *choose*, through
// `work_on_offer_for_probe`. Not which one they are seen doing: with two crew,
// one broom and one galley, they take a job each whatever the list says, so
// every arrangement looks identical from outside and the timing measures
// nothing. The first version of this probe measured exactly that and passed.

include!("modules.rs");

use bim::CREW;
use game::Game;
use math::vec2;
use work::{HIGHEST, Job, LOWEST, Priorities};

const STEP: f32 = 1.0 / 60.0;
const FRAMES_PER_DAY: u32 = 24 * 60 * 60;

/// The activity codes the host reads. Named here so a renumbering on that side
/// fails loudly rather than quietly measuring the wrong errand.
const JOB_MEAL: u32 = game::JOB_MEAL;
const JOB_BOWL: u32 = game::JOB_BOWL;
const JOB_LEFTOVERS: u32 = game::JOB_LEFTOVERS;
const JOB_CLEAN: u32 = game::JOB_CLEAN;

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

    // --- the list itself ----------------------------------------------------

    let mut list = Priorities::new();
    check!(
        "every job starts at the same number",
        Job::ALL.iter().all(|&j| list.of(j) == list.of(Job::ALL[0]))
    );
    check!(
        "which is between the ends of the range",
        (HIGHEST..=LOWEST).contains(&list.of(Job::Clean)),
        list.of(Job::Clean)
    );
    check!("and nothing is before anything", !list.before(Job::Clean, Job::Cook));

    // Round the houses and back, one step less important each time.
    let start = list.of(Job::Clean);
    let mut seen = Vec::new();
    for _ in HIGHEST..=LOWEST {
        seen.push(list.cycle(Job::Clean));
    }
    check!(
        "a click is one step less important and wraps at the bottom",
        seen == vec![4, 5, 1, 2, 3],
        format!("{seen:?}")
    );
    check!("and lands back where it began", list.of(Job::Clean) == start);
    check!(
        "cycling one job leaves the others alone",
        Job::ALL.iter().filter(|&&j| j != Job::Clean).all(|&j| list.of(j) == start)
    );

    // Out of range is clamped rather than refused: the number arrives from the
    // host as a bare integer, and a silent no-op would leave the panel showing
    // a setting the ship is not working to.
    list.set(Job::Cook, 0);
    check!("below the range clamps to the top", list.of(Job::Cook) == HIGHEST, list.of(Job::Cook));
    list.set(Job::Cook, 99);
    check!("above it clamps to the bottom", list.of(Job::Cook) == LOWEST, list.of(Job::Cook));

    // Every code the host can send has a job behind it, and nothing past the
    // end does. The host builds its rows off `work_count`, so the two have to
    // agree exactly or it renders a row for a job that is not there.
    for code in 0..Job::ALL.len() as u32 {
        check!(format!("code {code} names a job"), Job::from_code(code).is_some());
    }
    check!(
        "and one past the end names nothing",
        Job::from_code(Job::ALL.len() as u32).is_none()
    );

    // --- a cutting is a cut and a haul --------------------------------------
    //
    // The crop is carried to the cold store in the same chain that lifted it,
    // so a cutting waits on whichever of the two the player set later. Checked
    // directly: staging a ripe tray to watch it takes most of a game day.

    let mut game = Game::new(3, 960.0, 640.0);
    game.set_work_priority(Job::Cut.code(), HIGHEST);
    game.set_work_priority(Job::Haul.code(), LOWEST);
    check!(
        "a cutting with nobody to haul waits on the hauling",
        game.work_waits_on_for_probe(Job::Cut.code()) == LOWEST,
        game.work_waits_on_for_probe(Job::Cut.code())
    );
    game.set_work_priority(Job::Haul.code(), HIGHEST);
    check!(
        "and waits on the cutting once there is",
        game.work_waits_on_for_probe(Job::Cut.code()) == HIGHEST,
        game.work_waits_on_for_probe(Job::Cut.code())
    );
    check!(
        "no other job is dragged about by the hauling row",
        game.work_waits_on_for_probe(Job::Clean.code())
            == game.work_priority(Job::Clean.code())
    );

    // --- the galley against the deck ----------------------------------------
    //
    // What is asked is what the Bim would *choose*. Whether it then gets on
    // with it is a separate question — the galley may be occupied, the broom
    // may be in the other one's hands — and mixing the two in is what made the
    // first go at this probe meaningless: with two crew, one free broom and
    // one free galley, they take a job each whatever the list says, and every
    // arrangement measured the same.

    check!(
        "left equal, the meal is chosen before the deck",
        offered(None) == vec![Job::Cook.code(), Job::Clean.code()],
        format!("{:?}", offered(None))
    );
    check!(
        "cleaning put first, the deck is chosen before the meal",
        offered(Some((Job::Clean, HIGHEST, Job::Cook, LOWEST)))
            == vec![Job::Clean.code(), Job::Cook.code()],
        format!("{:?}", offered(Some((Job::Clean, HIGHEST, Job::Cook, LOWEST))))
    );
    check!(
        "and cooking put first puts it back the other way round",
        offered(Some((Job::Cook, HIGHEST, Job::Clean, LOWEST)))
            == vec![Job::Cook.code(), Job::Clean.code()],
        format!("{:?}", offered(Some((Job::Cook, HIGHEST, Job::Clean, LOWEST))))
    );
    // One step is enough: the numbers are compared, not bucketed.
    check!(
        "and one step apart is enough to decide it",
        offered(Some((Job::Clean, 2, Job::Cook, 3))) == vec![Job::Clean.code(), Job::Cook.code()],
        format!("{:?}", offered(Some((Job::Clean, 2, Job::Cook, 3))))
    );

    // --- the bay against the deck -------------------------------------------
    //
    // The bay cannot be made to ask at the start: its trays come sown, so
    // `wants_work` has nothing to offer until one ripens or one empties. So
    // this one is run forward to the moment it does ask and read there.

    let bay_equal = when_the_bay_asks(None);
    check!(
        "left equal, the bay is chosen before the deck",
        bay_equal.first().copied() != Some(Job::Clean.code()),
        format!("{bay_equal:?}")
    );
    let deck_over_bay = when_the_bay_asks(Some((Job::Clean, HIGHEST, Job::Plant, LOWEST)));
    check!(
        "and cleaning put over the bay is chosen first whatever the bay wants",
        deck_over_bay.first().copied() == Some(Job::Clean.code()),
        format!("{deck_over_bay:?}")
    );

    // --- and the work still gets done ---------------------------------------
    //
    // The ordering is settled above, where it can be read exactly. What is
    // left to check here is that reordering the list does not *lose* a job:
    // whatever the numbers, a filthy deck ends up swept and a hungry Bim ends
    // up fed.
    //
    // There is deliberately no assertion here that last place *arrives* later.
    // It cannot be measured with this crew: there are two of them, the broom
    // and the galley are separate, and so the one that cannot have the job at
    // the top of the list simply takes the one below it — both jobs begin on
    // the same frame under every arrangement. That is the right behaviour and
    // it makes the timing useless as a reading. The first version of this
    // probe asserted on it and passed for the wrong reason.

    for (what, set) in [
        ("left equal", None),
        ("cleaning first", Some((Job::Clean, HIGHEST, Job::Cook, LOWEST))),
        ("cooking first", Some((Job::Cook, HIGHEST, Job::Clean, LOWEST))),
    ] {
        let run = race(set);
        check!(
            format!("{what}: somebody gets the broom out"),
            run.clean.is_some_and(|f| f < 20 * 60 * 60),
            format!("{:?}", run.clean)
        );
        check!(
            format!("{what}: and somebody gets fed"),
            run.meal.is_some_and(|f| f < 20 * 60 * 60),
            format!("{:?}", run.meal)
        );
    }

    // --- but nobody starves for it -------------------------------------------
    //
    // Cooking at the bottom with a filthy deck at the top is the setting that
    // could kill somebody, and it is exactly the setting a player will try.
    // The Bim is allowed to put the meal off; it is not allowed to go without
    // until it is ill.

    for seed in [4, 44] {
        let mut game = Game::new(seed, 960.0, 640.0);
        game.set_work_priority(Job::Cook.code(), LOWEST);
        game.set_work_priority(Job::Clean.code(), HIGHEST);
        let mut worst = [f32::MAX; CREW];
        let mut ate = [false; CREW];
        for day in 0..3 {
            for _ in 0..FRAMES_PER_DAY {
                // Keep the deck wanting the broom all the way through, so the
                // job at the top never runs out and the one at the bottom is
                // never simply the only thing left.
                mess_up(&mut game, 6);
                game.update(STEP);
                for w in 0..CREW {
                    worst[w] = worst[w].min(game.health(w));
                    ate[w] |= matches!(game.activity(w), JOB_MEAL | JOB_BOWL | JOB_LEFTOVERS);
                }
            }
            let _ = day;
        }
        for w in 0..CREW {
            check!(
                format!("seed {seed}, crew {w}: eats even with cooking at the bottom"),
                ate[w]
            );
            check!(
                format!("seed {seed}, crew {w}: and is never made ill by it"),
                worst[w] > 90.0,
                worst[w]
            );
        }
    }

    println!();
    if fails > 0 { println!("{fails} FAILED"); std::process::exit(1); }
    println!("all passed");
}

/// A filthy deck and two empty stomachs, with the two jobs set as given.
///
/// Both jobs are going from the very first frame, which is the point: the bay
/// cannot be staged this way — its trays come sown, so it asks for nothing
/// until one ripens — and a scene where only one job is on offer measures the
/// larder rather than the list.
fn staged(set: Option<(Job, u32, Job, u32)>) -> Game {
    let mut game = Game::new(9, 960.0, 640.0);
    if let Some((a, at, b, bt)) = set {
        game.set_work_priority(a.code(), at);
        game.set_work_priority(b.code(), bt);
    }
    for w in 0..CREW {
        game.spend_for_probe(w, FOOD, 1.0);
    }
    mess_up(&mut game, 20);
    game
}

/// What the crew would choose, most important first. Both are asked and the
/// answers have to agree — there is one list and it is the ship's, so a
/// setting that reached one Bim and not the other is a bug in itself.
fn offered(set: Option<(Job, u32, Job, u32)>) -> Vec<u32> {
    let game = staged(set);
    let first = game.work_on_offer_for_probe(0);
    assert_eq!(first, game.work_on_offer_for_probe(1), "the crew disagree");
    first
}

/// The same reading, taken at the first moment the bay actually asks for
/// something. Run forward rather than staged, because nothing can make a sown
/// tray want sowing again.
fn when_the_bay_asks(set: Option<(Job, u32, Job, u32)>) -> Vec<u32> {
    let mut game = Game::new(9, 960.0, 640.0);
    if let Some((a, at, b, bt)) = set {
        game.set_work_priority(a.code(), at);
        game.set_work_priority(b.code(), bt);
    }
    for _ in 0..(3 * FRAMES_PER_DAY) {
        // The deck is kept dirty throughout, so that when the bay does speak
        // up there is something for it to be weighed against.
        mess_up(&mut game, 6);
        game.update(STEP);
        let on_offer = game.work_on_offer_for_probe(0);
        if on_offer
            .iter()
            .any(|&j| j == Job::Plant.code() || j == Job::Cut.code())
        {
            return on_offer;
        }
    }
    Vec::new()
}

/// When each of the two jobs was first started by anybody.
struct Race {
    meal: Option<u32>,
    clean: Option<u32>,
}

/// Stage the same scene and let it run, watching which job is actually begun.
fn race(set: Option<(Job, u32, Job, u32)>) -> Race {
    let mut game = staged(set);
    let mut out = Race { meal: None, clean: None };
    for f in 0..(2 * FRAMES_PER_DAY) {
        // Topped up as it goes: whichever job wins must not win simply by
        // being the only one still going.
        mess_up(&mut game, 6);
        game.update(STEP);
        for w in 0..CREW {
            match game.activity(w) {
                JOB_MEAL | JOB_BOWL | JOB_LEFTOVERS if out.meal.is_none() => out.meal = Some(f),
                JOB_CLEAN if out.clean.is_none() => out.clean = Some(f),
                _ => {}
            }
        }
        if out.meal.is_some() && out.clean.is_some() {
            break;
        }
    }
    out
}

/// Foul a run of reachable tiles until the deck has at least `want` dirty, and
/// do nothing at all once it has. Cheap enough to call every frame.
fn mess_up(game: &mut Game, want: u32) {
    if game.dirty_tiles() >= want {
        return;
    }
    for y in (60..540).step_by(filth::TILE as usize) {
        for x in (60..800).step_by(filth::TILE as usize) {
            if game.dirty_tiles() >= want {
                return;
            }
            let at = vec2(x as f32, y as f32);
            // Only tiles a body can get to: some of the deck is deck and still
            // unreachable, and a mess there is one nobody can ever sweep.
            if game.spot_at(at.x, at.y) == room::SPOT_DECK && game.can_reach_for_probe(0, at) {
                game.foul_for_probe(at);
            }
        }
    }
}
