//! The stars behind the ship.
//!
//! **Cosmetic, and nothing else.** Not the galaxy's stars, not the system's,
//! not anything a player can fly to or point at: three layers of specks that
//! stream the other way while the ship moves, so that a ship under way
//! reads as moving rather than as sitting in a black rectangle. Nothing in
//! the simulation has ever heard of them.
//!
//! Three things about it are worth knowing before changing any of it.
//!
//! - **It streams at a rate, on the world's clock.** The field is a picture
//!   clock like `Game::frame`: [`Starfield::advance`] moves it on by the
//!   ship's speed times how much world time has passed since the last
//!   frame, so a ship at a steady speed has stars going past it steadily, a
//!   pause holds them, and 24x is twenty-four times the stream. It used to
//!   be a *displacement* — the field shifted by an amount that depended on
//!   the speed — which is a picture that stands still at any constant
//!   speed, and a shifted still picture is indistinguishable from an
//!   unshifted one. Nothing that decides anything reads it.
//! - **The speed mapping is logarithmic and clamped.** Real velocities here
//!   run from nothing to tens of thousands of units a minute over one trip. A
//!   linear mapping gives a field that does not move for the first hour and
//!   then tears across the screen in a blur; a logarithm with a ceiling on it
//!   gives something that reads as "faster" at every point in between, which
//!   is the whole of what it is for.
//! - **It wraps.** Each layer is a square tile of stars repeated for ever, so
//!   the field never runs out however far the ship goes — and each layer's
//!   stream is wrapped to its tile as it goes, so the numbers never grow.
//!   Anything that stops it wrapping is a starfield that empties.

use worldgen::math::{DVec2, dvec2};
use worldgen::rng::Rng;

/// One speck.
///
/// Its position is in **screen pixels**, not world units. A backdrop is a
/// backdrop: it covers the canvas whatever the zoom, and a field measured in
/// world units would tile four hundred times over at the far end of the zoom
/// range and once at the near end.
#[derive(Clone, Copy)]
pub struct Speck {
    /// Where it is inside its layer's tile.
    pub at: DVec2,
    pub size: f32,
    pub brightness: f32,
}

/// How many layers, how far apart they read, and how many specks each has.
///
/// Three, because two do not read as depth and four is a lot of rectangles a
/// frame for something nobody is looking at. The factors are how much each
/// layer scrolls: the near one moves a whole unit per unit, the far one barely
/// at all.
const LAYER_FACTORS: [f64; 3] = [1.0, 0.45, 0.18];
const LAYER_SPECKS: [u32; 3] = [90, 130, 170];

/// The side of the square each layer tiles, in screen pixels. Two or three
/// repeats cover an ordinary window, which is few enough that the pattern is
/// not obvious and few enough to draw.
pub const FIELD: f64 = 700.0;

/// How fast the near layer streams when the ship is flat out, in screen
/// pixels per game minute — which at 1x is per real second. Past `FAST` it
/// streams no faster however fast the ship is going, which is the clamp's
/// job; at 24x it is twenty-four times this, and that is meant.
const STREAM: f64 = 90.0;

/// What a speed of this much reads as "flat out". Under it the mapping is a
/// logarithm; over it, the clamp.
const FAST: f64 = 20_000.0;

pub struct Starfield {
    pub layers: Vec<Vec<Speck>>,
    /// How far each layer has streamed, in screen pixels, wrapped to the
    /// tile. A picture clock: advanced by [`Starfield::advance`] and read by
    /// the painter and nothing else.
    pub slid: [DVec2; 3],
    /// The world's clock as of the last advance, so the next one knows how
    /// much time went by. `None` until the first frame, or the first frame
    /// would stream the whole of the clock so far.
    clock: Option<f64>,
}

impl Starfield {
    /// Built from the world's own seed, so the same galaxy has the same sky.
    ///
    /// Its own branch of the seed rather than a stream with a `Purpose`:
    /// `worldgen::rng::Purpose` is the list of things the **world** is
    /// generated from, and a decoration has no business being on it.
    pub fn new(seed: u64) -> Starfield {
        let mut rng = Rng::new(seed ^ 0x_5354_4152_4649_454c);
        let layers = LAYER_SPECKS
            .iter()
            .map(|&count| {
                (0..count)
                    .map(|_| Speck {
                        at: dvec2(rng.range(0.0, FIELD), rng.range(0.0, FIELD)),
                        // The near layer is not the bright one: a bright speck
                        // that scrolls fast reads as a scratch on the screen.
                        size: rng.range(0.8, 2.2) as f32,
                        brightness: rng.range(0.18, 0.75) as f32,
                    })
                    .collect()
            })
            .collect();
        Starfield {
            layers,
            slid: [DVec2::ZERO; 3],
            clock: None,
        }
    }

    pub fn factor(layer: usize) -> f64 {
        LAYER_FACTORS[layer.min(LAYER_FACTORS.len() - 1)]
    }

    /// How fast the near layer streams, in screen pixels per game minute,
    /// given how fast the ship is going.
    ///
    /// Opposite the velocity, because the stars are what is standing still.
    /// The magnitude is the clamped logarithm described in the module note.
    /// In **screen** axes already: system `+y` is north and screen `y` grows
    /// downwards, so the sign on the second component is the flip and not a
    /// mistake.
    pub fn rate(velocity: DVec2) -> DVec2 {
        let speed = velocity.length();
        if !(speed > 0.0) {
            return DVec2::ZERO;
        }
        let mapped = ((1.0 + speed).ln() / (1.0 + FAST).ln()).min(1.0);
        let along = velocity.scale(1.0 / speed);
        let rate = along.scale(-mapped * STREAM);
        dvec2(rate.x, -rate.y)
    }

    /// Stream the field on to the world's clock reading `now`, at the rate
    /// the ship's `velocity` gives. Once a frame, from `ship_render`: the
    /// time that has passed is read off the clock rather than off the
    /// frame, so a pause holds the sky, a fast-forward streams it faster,
    /// and a frame that stepped the world twice streams it twice as far.
    pub fn advance(&mut self, velocity: DVec2, now: f64) {
        let minutes = match self.clock {
            Some(then) => (now - then).max(0.0),
            None => 0.0,
        };
        self.clock = Some(now);
        if minutes == 0.0 {
            return;
        }
        let rate = Starfield::rate(velocity);
        for (i, slid) in self.slid.iter_mut().enumerate() {
            let step = rate.scale(minutes * Starfield::factor(i));
            *slid = dvec2(
                (slid.x + step.x).rem_euclid(FIELD),
                (slid.y + step.y).rem_euclid(FIELD),
            );
        }
    }
}
