//! Mess: what is on the deck, what is on the Bim, and what that does to it.
//!
//! The deck is scored tile by tile. A tile starts at [`BASELINE`] and only ever
//! goes down — nothing cleans up yet — with an accident taking one straight to
//! [`FOULED`], the worst there is. The Bim carries its own share of it around
//! separately, because a Bim that soils itself takes the mess with it when it
//! walks away.
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

pub struct Filth {
    cols: usize,
    rows: usize,
    origin: Vec2,
    /// One score per tile, [`FOULED`] to [`BASELINE`].
    tiles: Vec<f32>,

    /// Game minutes the restroom need has been at nothing, and the same for
    /// the cleanliness need. Both start the clock the moment they empty.
    bursting_for: f32,
    filthy_for: f32,
    /// Game minutes since the last bout of sickness.
    since_sick: f32,
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
            bursting_for: 0.0,
            filthy_for: 0.0,
            since_sick: 0.0,
        }
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

    /// Take `cost` off the tile under `at`, never past the worst there is.
    pub fn soil(&mut self, at: Vec2, cost: f32) {
        let (c, r) = self.cell(at);
        if let Some(i) = self.index(c, r) {
            self.tiles[i] = (self.tiles[i] - cost).max(FOULED);
        }
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

    /// Run the clocks and say what, if anything, happened.
    ///
    /// `restroom` and `cleanliness` are the two levels as they stand. Both
    /// clocks are held at nothing while their need is not empty, so seeing to
    /// either one puts the Bim back to the beginning of it.
    pub fn update(
        &mut self,
        minutes: f32,
        restroom: f32,
        cleanliness: f32,
        urge_extreme: bool,
        urge_medium: bool,
        rng: &mut Rng,
    ) -> Mishap {
        let mut out = Mishap::default();

        // The heads, or the deck. Holding on is a matter of how long it has
        // been at nothing rather than of the level, which cannot go lower.
        if urge_extreme {
            self.bursting_for += minutes;
            if self.bursting_for >= HOLDS_FOR {
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

    // --- what each mishap costs ------------------------------------------

    /// Wetting itself: the tile, the Bim, and what it puts the need back to.
    pub fn wet(&mut self, at: Vec2) -> (f32, f32) {
        self.soil(at, WET_COST);
        (WET_ON_BIM, WET_RELIEF)
    }

    /// The worst of it: the tile goes straight to the bottom of the scale.
    pub fn foul(&mut self, at: Vec2) -> (f32, f32) {
        self.soil(at, RUINED);
        (FOULED_ON_BIM, FOULED_RELIEF)
    }

    /// Being sick ruins a tile as thoroughly as an accident does. It is also
    /// the one mess the Bim makes over and over, so a Bim at the worst stage
    /// leaves a trail of fouled tiles behind it as it moves away from each.
    pub fn sick_on(&mut self, at: Vec2) {
        self.soil(at, RUINED);
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
                // Nothing to full, from the baseline down to the worst.
                let deep = ((BASELINE - score) / (BASELINE - FOULED)).clamp(0.0, 1.0);
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
