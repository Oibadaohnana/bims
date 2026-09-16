//! The stars behind the ship.
//!
//! **Cosmetic, and nothing else.** Not the galaxy's stars, not the system's,
//! not anything a player can fly to or point at: three layers of specks that
//! slide the other way when the ship moves, so that a ship under acceleration
//! reads as moving rather than as sitting in a black rectangle. Nothing in the
//! simulation has ever heard of them.
//!
//! Two things about it are worth knowing before changing any of it.
//!
//! - **The speed mapping is logarithmic and clamped.** Real velocities here
//!   run from nothing to tens of thousands of units a minute over one trip. A
//!   linear mapping gives a field that does not move for the first hour and
//!   then tears across the screen in a blur; a logarithm with a ceiling on it
//!   gives something that reads as "faster" at every point in between, which
//!   is the whole of what it is for.
//! - **It wraps.** Each layer is a square tile of stars repeated for ever, so
//!   the field never runs out however far the ship goes. Anything that stops
//!   it wrapping is a starfield that empties.

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

/// The furthest the field will slide, in screen pixels. Past this it looks the
/// same however fast the ship is going, which is the clamp's job.
const SCROLL_LIMIT: f64 = 900.0;

/// What a speed of this much reads as "flat out". Under it the mapping is a
/// logarithm; over it, the clamp.
const FAST: f64 = 20_000.0;

/// How much the field slides with the ship's **position** rather than its
/// speed, in screen pixels per world unit. Tiny, and there so that a ship
/// holding station at two ends of a system does not have the same sky.
const DRIFT: f64 = 2e-4;

pub struct Starfield {
    pub layers: Vec<Vec<Speck>>,
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
        Starfield { layers }
    }

    pub fn factor(layer: usize) -> f64 {
        LAYER_FACTORS[layer.min(LAYER_FACTORS.len() - 1)]
    }

    /// How far the field has slid, in screen pixels, given where the ship is
    /// and how fast it is going.
    ///
    /// Opposite the velocity, because the stars are what is standing still.
    /// The magnitude is the clamped logarithm described in the module note.
    /// Both terms are in **screen** axes already: system `+y` is north and
    /// screen `y` grows downwards, so the sign on the second component is the
    /// flip and not a mistake.
    pub fn scroll(position: DVec2, velocity: DVec2) -> DVec2 {
        let speed = velocity.length();
        let mapped = if speed > 0.0 {
            ((1.0 + speed).ln() / (1.0 + FAST).ln()).min(1.0)
        } else {
            0.0
        };
        let along = if speed > 0.0 {
            velocity.scale(1.0 / speed)
        } else {
            DVec2::ZERO
        };
        let slide = along.scale(-mapped * SCROLL_LIMIT);
        let drift = position.scale(-DRIFT);
        let total = slide.add(drift);
        dvec2(total.x, -total.y)
    }
}
