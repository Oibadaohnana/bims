//! Mess: what is on the deck, what is on the Bim, and what that does to it.
//!
//! The deck is scored tile by tile. A tile starts at [`BASELINE`] and goes down
//! as things happen on it, with an accident taking one straight to [`FOULED`],
//! the worst there is. A broom is the one thing that puts it back — see
//! [`Filth::sweep`], and the chain behind it in `task.rs`. The Bim carries its
//! own share of it around separately, because a Bim that soils itself takes the
//! mess with it when it walks away, and no broom reaches that.
//!
//! Mess spreads, by two routes. The dirty jobs — cooking, planting, lifting a
//! crop — flick something onto the deck around the Bim as each step of them
//! finishes ([`Filth::spatter`]). And boots carry what is already down from
//! one tile to the next ([`Filth::track`]): a quarter of the crossings move a
//! quarter of the tile, so a trail thins fast and dies out rather than
//! working its way across the ship.
//!
//! The cleanliness *need* is not a clock like hunger. It follows two things at
//! once: the average of the tiles within [`REACH`] of where the Bim is standing,
//! and how filthy the Bim itself is. Standing in a clean room with clean hands
//! it recovers; anything else and it falls, faster the worse the mess.
//!
//! Everything a mess *does* to the Bim is on a clock rather than a level, the
//! same as malnutrition in `health.rs`: reaching nothing is the start of it,
//! not the end, and each stage is an hour further in.

use crate::clock::HOUR;
use crate::draw::{Color, DrawList};
use crate::math::{Rect, Vec2, vec2};
use crate::rng::Rng;

/// How big one tile of deck is. The same grid the floor is drawn on, so a
/// fouled tile lines up with the seams under it rather than floating between
/// them.
pub const TILE: f32 = 52.0;

/// What a clean tile scores, and the floor under the worst of it. Both are the
/// player-facing numbers rather than a 0..1 fraction: a tile reads as "10" and
/// a fouled one as "-100".
pub const BASELINE: f32 = 10.0;
pub const FOULED: f32 = -100.0;

/// What each mess takes off the tile it lands on. Soiling itself and being
/// sick both take a tile all the way to the bottom of the scale in one go;
/// wetting one is bad without being the worst there is.
const WET_COST: f32 = 35.0;
const RUINED: f32 = BASELINE - FOULED;

/// How far around itself the Bim notices, in tiles. Three either way, so a
/// seven by seven block.
///
/// Widening this *dilutes*: the need follows the average, so one fouled tile
/// among forty-nine weighs less than one among twenty-five. What it buys is
/// reach — a mess three tiles off now counts for something — and it takes more
/// than one accident for the room itself to start telling on the Bim.
pub const REACH: i32 = 3;

/// Cleanliness lost per game minute with the surroundings at exactly nothing:
/// a full bar gone in an hour. Worse than nothing multiplies it — see
/// [`Filth::grinding`] — so standing in a fouled tile is eleven times that.
const GRIND: f32 = 1.0 / 60.0;
/// What the Bim's own state is worth on top, at maximum filth.
const OWN_GRIND: f32 = GRIND * 5.0;
/// Clean surroundings and clean hands put it back, four hours to full. Nothing
/// scrubs the Bim itself yet, so this only runs once it is clean again.
const FRESHEN: f32 = 1.0 / (4.0 * 60.0);

/// How far below spotless a tile has to be before it is worth getting the
/// broom out. Small: the only things that mark the deck take it a long way
/// down, so in practice this separates "somebody was sick here" from
/// "nothing has happened here" rather than grading dirt.
const WORTH_SWEEPING: f32 = 1.0;

/// How far down a tile has to be before it spoils the food made beside it:
/// as far as a wetting takes one, which is also where a second spatter on
/// the same stain lands. Deliberately **not** [`WORTH_SWEEPING`]: a meal
/// flicks a stain or two onto the deck on its way — see `JOB_MESSES` — and
/// judged by those, the cook's own chopping would poison the cook, every
/// meal, on a deck nobody had done anything to. A wetting, an accident, a
/// bout of sickness, or a galley left to go grubby is what counts.
const SPOILS_FOOD: f32 = BASELINE - WET_COST;

/// What a dirty job flicks onto a tile beside it, and how often a finished
/// step of one does it at all.
///
/// Well short of an accident: a single spatter reads as a stain rather than a
/// ruined tile, and it takes a good many of them in one place before the
/// cleanliness need takes any notice. It is comfortably past
/// [`WORTH_SWEEPING`], though, so the broom has something to come out for —
/// the galley and the bay go grubby on their own now, which is the first
/// mess aboard that nobody had an accident to make.
const GRIME_COST: f32 = 14.0;
pub const JOB_MESSES: f32 = 0.35;

/// Walking it about. A boot crossing a dirty tile has [`SPREAD_CHANCE`] of
/// taking [`SPREAD_SHARE`] of what is on it onto the tile it steps to.
///
/// The dirt *moves*: the tile behind loses exactly what the tile ahead gains.
/// Copying it would let a Bim pacing the galley multiply the deck's filth
/// without limit; moving it means a trail thins as it lengthens — a quarter,
/// then a sixteenth — and falls under [`WORTH_SWEEPING`] on its own after
/// three or four steps, which is what stops a single accident eventually
/// reaching every tile aboard.
const SPREAD_CHANCE: f32 = 0.25;
const SPREAD_SHARE: f32 = 0.25;

/// How much a pixel of walking counts against a tile when choosing which to
/// sweep next, in units of the 0-to-1 dirt score. At this rate the width of
/// the compartment is worth about a third of a tile's filthiness — enough
/// that the Bim works outwards from where it is standing rather than crossing
/// the room for the single worst tile every time.
const DISTANCE_TELLS: f32 = 0.0004;

/// Game minutes at the extreme urge before the Bim stops holding it.
const HOLDS_FOR: f32 = HOUR;
/// Game minutes between one bout of sickness and the next.
const SICK_EVERY: f32 = 0.5 * HOUR;
/// The chance per game hour of not quite making it, at the middle urge.
const WETS_PER_HOUR: f32 = 0.10;

/// How much hunger comes back up with it: half the food bar, in the bar's own
/// units, taken off what is in there rather than scaled by it. So 92% becomes
/// 42%, and anything at or below half full simply ends up empty.
pub const SICK_COSTS_FOOD: f32 = 0.50;

/// What the Bim is left covered in. Wetting itself is bad; the other is total.
const WET_ON_BIM: f32 = 0.45;
const FOULED_ON_BIM: f32 = 1.0;

/// Relief is relief: soiling itself empties the Bim as thoroughly as the pan
/// would, and wetting itself takes the edge off without finishing the job.
const WET_RELIEF: f32 = 0.45;
const FOULED_RELIEF: f32 = 1.0;

// --- how the mess is drawn ----------------------------------------------

/// Filth is the one thing aboard with no blue in it at all, so it reads as
/// out of place on a deck that is otherwise grey and cyan.
const MESS: Color = Color::rgb(0.31, 0.24, 0.11);
const MESS_DARK: Color = Color::rgb(0.20, 0.15, 0.07);
/// The alpha a fouled tile reaches. Short of opaque: the deck seams should
/// still show through, or the tile reads as a hole in the floor.
const MESS_ALPHA: f32 = 0.72;

/// How far gone the Bim is for want of a clean place to stand.
///
/// Reached by the clock, not the level: the cleanliness need hitting nothing
/// starts it, and each stage is an hour further into that.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Discomfort {
    None,
    Mild,
    Uncomfortable,
    Extreme,
}

impl Discomfort {
    /// 0 comfortable, then 1, 2, 3. The host names them.
    pub fn stage(self) -> u32 {
        self as u32
    }

    /// How fast it walks, as a fraction of its usual pace: picking its way
    /// around the mess rather than striding through it.
    pub fn pace(self) -> f32 {
        match self {
            Discomfort::None => 1.0,
            _ => 0.90,
        }
    }

    /// Whether the Bim would rather be standing somewhere else.
    pub fn flees(self) -> bool {
        self >= Discomfort::Uncomfortable
    }

    /// Whether it is being sick on the deck at intervals.
    pub fn sickens(self) -> bool {
        self == Discomfort::Extreme
    }
}

/// What is on a tile, as against how bad it is.
///
/// The score is one number and always will be: everything the simulation does
/// with a tile — the average around the Bim, how fast that grinds it down,
/// which way it walks to get clear — reads that number and nothing else. But a
/// player pointing at a tile is asking a different question, and "-65" is no
/// answer to it. So the worst thing that has landed on each tile is remembered
/// beside the score, for the readout and for nothing else.
///
/// They are ordered worst-last on purpose, because a tile keeps the worst it
/// has had rather than the latest: [`Filth::soil`] takes the greater of the
/// two, so grime tracked over a fouled tile does not talk it back down to
/// grime.
///
/// [`Mess::Grime`] is the one that is not an accident. Cooking, planting and
/// lifting all flick something onto the deck around them, and boots carry it
/// on from there — see [`Filth::spatter`] and [`Filth::track`].
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Mess {
    None,
    Grime,
    Wet,
    Soiled,
    Sick,
}

impl Mess {
    /// 0 clean, then one per kind. The host names them.
    pub fn code(self) -> u32 {
        self as u32
    }
}

/// What a mess did this frame. The game applies these — it is the only thing
/// that knows where the Bim is standing and what its needs are.
#[derive(Default, Clone, Copy)]
pub struct Mishap {
    /// Did not quite make it: wet itself where it stands.
    pub wet: bool,
    /// Could hold it no longer. The worst there is.
    pub fouled: bool,
    /// Brought up what was in it.
    pub sick: bool,
}

/// How long *one Bim* has been going without, and what that is about to cost
/// it.
///
/// These are clocks on a body, not on the deck, and keeping them apart from
/// [`Filth`] matters: the deck is shared and there is one of it, while every
/// Bim holds on for its own hour and stands in the mess for its own two. They
/// lived on `Filth` when there was one Bim aboard and it made no difference.
/// With two it made every difference — `Filth::update` ran once per crew
/// member per frame on the same object, so whichever Bim was comfortable
/// zeroed the other's clock on the way past and the hour was never reached.
/// Nobody ever had an accident, and nobody was ever sick.
pub struct Ordeal {
    /// Game minutes the restroom need has been at nothing, and the same for
    /// the cleanliness need. Both start the clock the moment they empty.
    bursting_for: f32,
    filthy_for: f32,
    /// Game minutes since the last bout of sickness.
    since_sick: f32,
}

impl Ordeal {
    pub fn new() -> Ordeal {
        Ordeal {
            bursting_for: 0.0,
            filthy_for: 0.0,
            since_sick: 0.0,
        }
    }

    /// Run the clocks and say what, if anything, happened.
    ///
    /// `restroom` and `cleanliness` are this Bim's two levels as they stand.
    /// Both clocks are held at nothing while their need is not empty, so
    /// seeing to either one puts the Bim back to the beginning of it.
    pub fn update(
        &mut self,
        minutes: f32,
        restroom: f32,
        cleanliness: f32,
        urge_extreme: bool,
        urge_medium: bool,
        purging: bool,
        rng: &mut Rng,
    ) -> Mishap {
        let mut out = Mishap::default();

        // The heads, or the deck. Holding on is a matter of how long it has
        // been at nothing rather than of the level, which cannot go lower.
        // Food poisoning is the exception: there is no holding on, and the
        // need reaching nothing is the accident, at once.
        if urge_extreme {
            self.bursting_for += minutes;
            if purging || self.bursting_for >= HOLDS_FOR {
                self.bursting_for = 0.0;
                out.fouled = true;
            }
        } else {
            self.bursting_for = 0.0;
            // One in ten an hour, at the middle urge: not every trip that is
            // left too late ends badly, but enough of them do.
            if urge_medium && rng.chance(minutes / HOUR * WETS_PER_HOUR) {
                out.wet = true;
            }
        }
        let _ = restroom;

        // Standing in it. The three stages are an hour apart, and the clock
        // only runs while the need is at nothing.
        if cleanliness <= 0.0 {
            self.filthy_for += minutes;
        } else {
            self.filthy_for = 0.0;
            self.since_sick = 0.0;
        }
        if self.discomfort().sickens() {
            self.since_sick += minutes;
            if self.since_sick >= SICK_EVERY {
                self.since_sick = 0.0;
                out.sick = true;
            }
        }

        out
    }

    pub fn discomfort(&self) -> Discomfort {
        if self.filthy_for <= 0.0 {
            Discomfort::None
        } else if self.filthy_for < HOUR {
            Discomfort::Mild
        } else if self.filthy_for < 2.0 * HOUR {
            Discomfort::Uncomfortable
        } else {
            Discomfort::Extreme
        }
    }

    /// Game minutes this Bim has been holding on at the extreme urge. For the
    /// probes, which need to see the clock actually running rather than infer
    /// it from an accident that may be an hour off.
    #[allow(dead_code)]
    pub fn bursting_for(&self) -> f32 {
        self.bursting_for
    }
}

pub struct Filth {
    cols: usize,
    rows: usize,
    origin: Vec2,
    /// One score per tile, [`FOULED`] to [`BASELINE`].
    tiles: Vec<f32>,
    /// The worst thing that has landed on each tile, beside its score. Nothing
    /// in the simulation reads this — see [`Mess`].
    kinds: Vec<Mess>,
}

impl Filth {
    pub fn new(interior: Rect) -> Filth {
        let cols = (interior.width() / TILE).ceil() as usize + 1;
        let rows = (interior.height() / TILE).ceil() as usize + 1;
        Filth {
            cols,
            rows,
            origin: interior.min,
            tiles: vec![BASELINE; cols * rows],
            kinds: vec![Mess::None; cols * rows],
        }
    }

    /// The same deck over a different interior — one that grew a tile when
    /// deck was laid, or shrank — with every mess kept where it lies. Tiles
    /// carry across by position, so a stain by the hob stays by the hob;
    /// what the new interior does not reach is gone, and what it reaches
    /// afresh is clean.
    pub fn resized(&self, interior: Rect) -> Filth {
        let mut next = Filth::new(interior);
        for r in 0..next.rows {
            for c in 0..next.cols {
                let at = next.centre(c as i32, r as i32);
                let (oc, or) = self.cell(at);
                if let (Some(from), Some(to)) = (self.index(oc, or), next.index(c as i32, r as i32))
                {
                    next.tiles[to] = self.tiles[from];
                    next.kinds[to] = self.kinds[from];
                }
            }
        }
        next
    }

    fn cell(&self, at: Vec2) -> (i32, i32) {
        (
            ((at.x - self.origin.x) / TILE).floor() as i32,
            ((at.y - self.origin.y) / TILE).floor() as i32,
        )
    }

    fn index(&self, c: i32, r: i32) -> Option<usize> {
        if c < 0 || r < 0 || c as usize >= self.cols || r as usize >= self.rows {
            return None;
        }
        Some(r as usize * self.cols + c as usize)
    }

    /// The middle of tile `(c, r)`.
    fn centre(&self, c: i32, r: i32) -> Vec2 {
        self.origin + vec2((c as f32 + 0.5) * TILE, (r as f32 + 0.5) * TILE)
    }

    /// What the tile under `at` scores. Read by the probes, which is the only
    /// way to see one tile rather than the average around the Bim.
    #[allow(dead_code)]
    pub fn at(&self, at: Vec2) -> f32 {
        let (c, r) = self.cell(at);
        self.index(c, r).map_or(BASELINE, |i| self.tiles[i])
    }

    /// What is on the tile under `at`. Only the readout asks.
    pub fn kind_at(&self, at: Vec2) -> Mess {
        let (c, r) = self.cell(at);
        self.index(c, r).map_or(Mess::None, |i| self.kinds[i])
    }

    /// How far down the tile under `at` has been taken, 0 clean to 1 fouled.
    /// The score said as a fraction, which is what a readout wants.
    pub fn depth_at(&self, at: Vec2) -> f32 {
        ((BASELINE - self.at(at)) / (BASELINE - FOULED)).clamp(0.0, 1.0)
    }

    /// Put one tile back the way it was.
    ///
    /// All the way, in one go: a tile is swept or it is not, and a broom that
    /// left a tile half fouled would mean the Bim coming back to the same spot
    /// over and over while the need it is trying to mend barely moves. The
    /// *time* it takes is the chain's business — see `Step::Sweep` — not a
    /// matter of chipping away at the score.
    ///
    /// The kind goes with it. A tile that has been swept has nothing on it, so
    /// the readout must not still be calling it vomit.
    pub fn sweep(&mut self, at: Vec2) {
        let (c, r) = self.cell(at);
        if let Some(i) = self.index(c, r) {
            self.tiles[i] = BASELINE;
            self.kinds[i] = Mess::None;
        }
    }

    /// The tile most worth sweeping from where the Bim is standing, or `None`
    /// when there is nothing it can get to.
    ///
    /// Worst first, but not *only* worst: a tile twice as bad on the far side
    /// of the compartment is not worth the walk when there is one underfoot,
    /// so distance counts against a tile at a rate that lets a merely grubby
    /// one nearby win over a fouled one a room away.
    ///
    /// `can_get_to` is what keeps the whole errand from spinning. Some of the
    /// deck is deck and still out of reach — the corner past the end of the
    /// counter, hemmed in by the bunk — and a mess there would be picked as
    /// the worst tile for ever, with the Bim fetching the broom, failing the
    /// walk, giving up and starting again. `Filth` has no idea where a body
    /// fits, so the question is asked of whoever does.
    pub fn worst_tile(&self, from: Vec2, can_get_to: impl Fn(Vec2) -> bool) -> Option<Vec2> {
        let mut best: Option<(f32, Vec2)> = None;
        for r in 0..self.rows as i32 {
            for c in 0..self.cols as i32 {
                let Some(i) = self.index(c, r) else { continue };
                if self.tiles[i] >= BASELINE - WORTH_SWEEPING {
                    continue;
                }
                let at = self.centre(c, r);
                let dirt = (BASELINE - self.tiles[i]) / (BASELINE - FOULED);
                let away = (at - from).len();
                let score = dirt - away * DISTANCE_TELLS;
                if best.is_some_and(|(had, _)| score <= had) {
                    continue;
                }
                // Asked last, because it is the expensive one.
                if can_get_to(at) {
                    best = Some((score, at));
                }
            }
        }
        best.map(|(_, at)| at)
    }

    /// How many tiles are dirty enough to be worth a broom.
    pub fn dirty_tiles(&self) -> u32 {
        self.tiles
            .iter()
            .filter(|&&t| t < BASELINE - WORTH_SWEEPING)
            .count() as u32
    }

    /// How many tiles within `reach` tiles of `at` — a square, the tile under
    /// it included — are messy enough to spoil food, at [`SPOILS_FOOD`].
    /// What the galley is judged by: a mess beside the hob is a mess in the
    /// food.
    pub fn dirty_tiles_within(&self, at: Vec2, reach: i32) -> u32 {
        let (c, r) = self.cell(at);
        let mut n = 0;
        for dr in -reach..=reach {
            for dc in -reach..=reach {
                if let Some(i) = self.index(c + dc, r + dr)
                    && self.tiles[i] <= SPOILS_FOOD
                {
                    n += 1;
                }
            }
        }
        n
    }

    /// Take `cost` off the tile under `at`, never past the worst there is, and
    /// remember what did it.
    ///
    /// A tile keeps the worst it has had rather than the latest: being sick on
    /// one already wet does not make it a wet one, and the score has gone to
    /// the bottom of the scale either way.
    pub fn soil(&mut self, at: Vec2, cost: f32, what: Mess) {
        let (c, r) = self.cell(at);
        if let Some(i) = self.index(c, r) {
            self.tiles[i] = (self.tiles[i] - cost).max(FOULED);
            self.kinds[i] = self.kinds[i].max(what);
        }
    }

    /// Flick something onto one of the nine tiles around `at`.
    ///
    /// The *job* is dirty, not one exact spot, so what lands goes on a tile
    /// picked out of the block around the Bim rather than always underfoot: a
    /// week of cooking spreads a patch across the galley instead of wearing
    /// one hole in the deck in front of the stove.
    ///
    /// `can_get_to` is the same question [`Filth::worst_tile`] asks, and it is
    /// here for the same reason. Parts of the deck are deck and still out of
    /// reach — under the lip of the counter, the corner past the bunk — and a
    /// stain flicked into one of those is a stain the broom never gets to and
    /// the deck keeps for good. Nothing is put anywhere a body cannot go.
    ///
    /// Says whether anything landed, which is all a probe needs.
    pub fn spatter(&mut self, at: Vec2, rng: &mut Rng, can_get_to: impl Fn(Vec2) -> bool) -> bool {
        let (c0, r0) = self.cell(at);
        let mut choices = [vec2(0.0, 0.0); 9];
        let mut found = 0;
        for dr in -1..=1 {
            for dc in -1..=1 {
                if self.index(c0 + dc, r0 + dr).is_none() {
                    continue;
                }
                let tile = self.centre(c0 + dc, r0 + dr);
                if !can_get_to(tile) {
                    continue;
                }
                choices[found] = tile;
                found += 1;
            }
        }
        if found == 0 {
            return false;
        }
        let tile = choices[rng.below(found as u32) as usize];
        self.soil(tile, GRIME_COST, Mess::Grime);
        true
    }

    /// Walk dirt from the tile a Bim was on onto the tile it has stepped to.
    ///
    /// Handed where a body was at the top of the frame and where it is now.
    /// Nothing happens at all unless that step crossed a tile boundary *and*
    /// the tile behind had something on it worth carrying — which is what
    /// keeps a clean deck from drawing anything from `rng`, and a clean deck
    /// is what most of the probes run on.
    ///
    /// What comes with it is a share of the score and the name that goes with
    /// it: a boot out of a fouled tile leaves a smear of the same thing, not
    /// a fresh kind of mess.
    pub fn track(&mut self, from: Vec2, to: Vec2, rng: &mut Rng) -> bool {
        let (fc, fr) = self.cell(from);
        let (tc, tr) = self.cell(to);
        if (fc, fr) == (tc, tr) {
            return false;
        }
        let (Some(behind), Some(ahead)) = (self.index(fc, fr), self.index(tc, tr)) else {
            return false;
        };
        let dirt = BASELINE - self.tiles[behind];
        if dirt < WORTH_SWEEPING || !rng.chance(SPREAD_CHANCE) {
            return false;
        }
        let moved = dirt * SPREAD_SHARE;
        self.tiles[behind] += moved;
        self.tiles[ahead] = (self.tiles[ahead] - moved).max(FOULED);
        self.kinds[ahead] = self.kinds[ahead].max(self.kinds[behind]);
        true
    }

    /// The average score of the tiles within `REACH` of `at`, the block clipped
    /// to the deck. This is what the cleanliness need actually follows.
    pub fn around(&self, at: Vec2) -> f32 {
        let (c, r) = self.cell(at);
        let mut total = 0.0;
        let mut count = 0.0;
        for dr in -REACH..=REACH {
            for dc in -REACH..=REACH {
                if let Some(i) = self.index(c + dc, r + dr) {
                    total += self.tiles[i];
                    count += 1.0;
                }
            }
        }
        if count == 0.0 {
            BASELINE
        } else {
            total / count
        }
    }

    /// How much of the deck has something on it, weighted by how bad each tile
    /// is: 0 spotless, 1 every tile fouled.
    pub fn dirty_share(&self) -> f32 {
        if self.tiles.is_empty() {
            return 0.0;
        }
        let span = BASELINE - FOULED;
        let sum: f32 = self
            .tiles
            .iter()
            .map(|&t| ((BASELINE - t) / span).clamp(0.0, 1.0))
            .sum();
        sum / self.tiles.len() as f32
    }

    /// Where the Bim would rather be: the middle of the cleanest tile within
    /// `look` tiles that is better than where it is standing now. `None` when
    /// nothing nearby is any better.
    pub fn somewhere_cleaner(&self, from: Vec2, look: i32) -> Option<Vec2> {
        let (c, r) = self.cell(from);
        let here = self.around(from);
        let mut best = here;
        let mut found = None;
        for dr in -look..=look {
            for dc in -look..=look {
                let (tc, tr) = (c + dc, r + dr);
                if self.index(tc, tr).is_none() {
                    continue;
                }
                let spot = self.centre(tc, tr);
                let score = self.around(spot);
                // A clear improvement only, and the nearer of two equals.
                if score > best + 0.5 {
                    best = score;
                    found = Some(spot);
                }
            }
        }
        found
    }

    /// How fast the cleanliness need is moving, per game minute: negative
    /// while there is mess about, positive once everything is clean again.
    ///
    /// `own` is how filthy the Bim itself is, 0 to 1. The room's share doubles
    /// every ten points below nothing, so a fouled tile underfoot is eleven
    /// times the rate of a merely joyless one.
    pub fn grinding(&self, at: Vec2, own: f32) -> f32 {
        let around = self.around(at);
        let from_room = if around <= 0.0 {
            GRIND * (1.0 + -around / 10.0)
        } else {
            0.0
        };
        let from_bim = OWN_GRIND * own;
        if from_room == 0.0 && from_bim == 0.0 {
            FRESHEN
        } else {
            -(from_room + from_bim)
        }
    }

    // --- what each mishap costs ------------------------------------------

    /// Wetting itself: the tile, the Bim, and what it puts the need back to.
    pub fn wet(&mut self, at: Vec2) -> (f32, f32) {
        self.soil(at, WET_COST, Mess::Wet);
        (WET_ON_BIM, WET_RELIEF)
    }

    /// The worst of it: the tile goes straight to the bottom of the scale.
    pub fn foul(&mut self, at: Vec2) -> (f32, f32) {
        self.soil(at, RUINED, Mess::Soiled);
        (FOULED_ON_BIM, FOULED_RELIEF)
    }

    /// Being sick ruins a tile as thoroughly as an accident does. It is also
    /// the one mess the Bim makes over and over, so a Bim at the worst stage
    /// leaves a trail of fouled tiles behind it as it moves away from each.
    pub fn sick_on(&mut self, at: Vec2) {
        self.soil(at, RUINED, Mess::Sick);
    }

    // --- drawing ----------------------------------------------------------

    /// Under everything: the mess is on the deck, so the Bim walks over it.
    pub fn draw(&self, list: &mut DrawList) {
        for r in 0..self.rows as i32 {
            for c in 0..self.cols as i32 {
                let Some(i) = self.index(c, r) else { continue };
                let score = self.tiles[i];
                if score >= BASELINE {
                    continue;
                }
                // Nothing to full, from the baseline down to the worst — and
                // then square-rooted, which is the difference between seeing
                // the deck go grubby and not.
                //
                // The score is linear and has to stay that way; what a stain
                // *looks* like is another question. A dirty job takes 14 off a
                // tile out of a possible 110, and at a flat eighth of the
                // alpha a fouled tile gets, that is a shade nobody can see
                // against the deck. The curve keeps the order — a fouled tile
                // is still much darker than a smear — while giving the faint
                // end of the range somewhere to be.
                let deep = ((BASELINE - score) / (BASELINE - FOULED))
                    .clamp(0.0, 1.0)
                    .sqrt();
                let at = self.centre(c, r);
                list.rect(
                    at,
                    vec2(TILE, TILE),
                    0.0,
                    6.0,
                    MESS.alpha(MESS_ALPHA * deep * 0.55),
                );
                // A few blobs on top, placed off the tile's own coordinates so
                // they stay put between frames without anything being stored.
                let blobs = (deep * 5.0).ceil() as i32;
                for b in 0..blobs {
                    let h = (c * 73 + r * 149 + b * 31) as f32;
                    let off = vec2((h * 0.37).sin(), (h * 0.71).cos()) * (TILE * 0.30);
                    let size = TILE * (0.12 + 0.10 * (h * 0.53).sin().abs());
                    list.ellipse(
                        at + off,
                        vec2(size, size * 0.8),
                        h,
                        MESS_DARK.alpha(MESS_ALPHA * deep),
                    );
                }
            }
        }
    }
}
