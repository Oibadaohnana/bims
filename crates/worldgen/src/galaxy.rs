//! The star field: where the stars are, and what they are.
//!
//! This is the only part of world generation that is drawn all at once. There
//! are a thousand stars, each of them two numbers, a name and a class, and
//! holding the lot costs less than one system's worth of bodies — so the map
//! can be drawn, panned and clicked without generating anything.
//!
//! **Nothing in here may depend on what is inside a system.** The whole star
//! field comes off one stream, [`Purpose::StarField`], and a system comes off
//! its own; that separation is what lets the contents of systems be
//! reworked — different planets, different stations, a different number of
//! either — without the map moving under a player who had learnt it. It is
//! also why a star's *name* is generated here rather than with its system.

use crate::data::StationKind;
use crate::math::{DVec2, dvec2};
use crate::name::{self, Name};
use crate::rng::{Purpose, Rng};

/// How many stars there are. Below `name`'s naming space, which is what makes
/// star names unique — [`crate::name::star_name`] says why.
pub const STAR_COUNT: u32 = 1000;

/// How far the galaxy reaches, in light years. Interstellar distance has no
/// bearing on anything yet: crossing between systems is a future technology
/// and deliberately not the same question as crossing one, so this scale and
/// the in-system one do not have to agree about anything.
pub const GALAXY_RADIUS: f64 = 50_000.0;

/// No two stars closer than this. Not physics — a map that is *clicked* needs
/// its targets to be separable, and two stars a pixel apart are one star that
/// sometimes gives you the wrong system.
pub const MIN_STAR_SEPARATION: f64 = 200.0;

/// Tries at placing a star before its minimum separation is given up on.
///
/// Giving up rather than looping is the important half: a galaxy whose core
/// is too crowded to satisfy the rule must still finish generating, and a
/// star placed slightly too close is a worse map than a star that took
/// sixteen goes — but an infinite loop is worse than either.
const PLACEMENT_TRIES: u32 = 16;

/// Where an arm starts, as a fraction of the radius, and how far round it
/// sweeps between there and the rim — in **radians, not turns**: 2.5 of them
/// is about 143°, a little under half the way round.
///
/// That number is why a four-armed galaxy cannot be told from a round one by
/// counting stars per angle: arms are 90° apart and each sweeps 143°, so
/// added up over every radius they cover the whole circle evenly. The
/// structure is there at any *given* radius, which is how the eye sees it and
/// how `the_shapes_are_different_shapes` has to measure it.
const ARM_INNER: f64 = 0.12;
const ARM_SWEEP: f64 = 2.5;

/// The shape of the whole thing.
///
/// Discriminants written out: this crosses the boundary as a number, and it
/// is part of a galaxy's identity — the same seed under a different type is a
/// different galaxy, not a rearranged one.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(u32)]
pub enum GalaxyType {
    /// Two arms, well separated. The one that looks most like a picture of a
    /// galaxy, and the easiest to navigate by eye.
    SpiralTwoArm = 0,
    /// Four arms. Busier, and harder to tell one part of from another.
    Spiral = 1,
    /// A squashed cloud with no structure at all.
    Elliptical = 2,
    /// A round cloud, dense in the middle.
    Round = 3,
}

impl GalaxyType {
    pub const ALL: [GalaxyType; 4] = [
        GalaxyType::SpiralTwoArm,
        GalaxyType::Spiral,
        GalaxyType::Elliptical,
        GalaxyType::Round,
    ];

    /// Arms, for the two that have them.
    fn arms(self) -> u32 {
        match self {
            GalaxyType::SpiralTwoArm => 2,
            GalaxyType::Spiral => 4,
            _ => 0,
        }
    }
}

/// What kind of star it is. The classical sequence, hottest first.
///
/// It does nothing yet — no habitable zones, no output, no colour that
/// anything reads. It is here because the map wants to look like somewhere
/// rather than like a scatter plot, and because a class is the obvious thing
/// for a later step to hang a temperature or a solar yield off.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(u32)]
pub enum StarClass {
    O = 0,
    B = 1,
    A = 2,
    F = 3,
    G = 4,
    K = 5,
    M = 6,
}

impl StarClass {
    pub const ALL: [StarClass; 7] = [
        StarClass::O,
        StarClass::B,
        StarClass::A,
        StarClass::F,
        StarClass::G,
        StarClass::K,
        StarClass::M,
    ];

    /// Rarest first, and steeply: the sky is mostly small red stars, and a
    /// map on which every star is interesting has no interesting stars.
    fn share(self) -> f64 {
        match self {
            StarClass::O => 0.01,
            StarClass::B => 0.04,
            StarClass::A => 0.05,
            StarClass::F => 0.06,
            StarClass::G => 0.09,
            StarClass::K => 0.15,
            StarClass::M => 0.60,
        }
    }

    fn draw(rng: &mut Rng) -> StarClass {
        let mut roll = rng.unit();
        for &c in &StarClass::ALL {
            if roll < c.share() {
                return c;
            }
            roll -= c.share();
        }
        StarClass::M
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Star {
    pub id: u32,
    /// Relative to the galaxy's origin, which is its centre.
    pub position: DVec2,
    pub name: Name,
    pub star_class: StarClass,
}

/// A galaxy: a seed, a shape, and the stars that fall out of them.
///
/// The systems are **not** in here. They are generated on demand from the
/// seed and the star's id — see [`crate::system`] — so this struct stays
/// small enough to hand about freely and a client that has looked at four
/// stars is in exactly the same state as one that has looked at none.
#[derive(Clone, Debug)]
pub struct Galaxy {
    pub seed: u64,
    pub generator_version: u32,
    pub galaxy_type: GalaxyType,
    pub stars: Vec<Star>,
}

impl Galaxy {
    pub fn new(seed: u64, galaxy_type: GalaxyType) -> Galaxy {
        Galaxy::with_version(seed, galaxy_type, crate::GENERATOR_VERSION)
    }

    /// The same, at a stated version. Only generation tests should name a
    /// version — everything else wants the current one, and pinning an old
    /// one is how a client ends up quietly disagreeing with a server.
    pub fn with_version(seed: u64, galaxy_type: GalaxyType, generator_version: u32) -> Galaxy {
        let stars = scatter(seed, galaxy_type, generator_version);
        Galaxy {
            seed,
            generator_version,
            galaxy_type,
            stars,
        }
    }

    pub fn star(&self, id: u32) -> Option<&Star> {
        self.stars.get(id as usize).filter(|s| s.id == id)
    }

    /// Which star is promised a station of each kind.
    ///
    /// Every galaxy has at least one of every kind, and this is how that
    /// promise is kept without generating a thousand systems to find out
    /// whether it already happened to be true. Five stars are picked from the
    /// galaxy seed alone; when one of them is asked for its system, the
    /// station it was promised is built there whether the ordinary roll
    /// wanted one or not.
    ///
    /// Indexed by `StationKind as usize`, so the array and
    /// [`StationKind::ALL`] have to stay in step.
    pub fn designations(&self) -> [u32; StationKind::ALL.len()] {
        designations(self.seed, self.generator_version, self.stars.len() as u32)
    }

    /// What this star is promised, if anything.
    pub fn designation_for(&self, star_id: u32) -> Option<StationKind> {
        let picked = self.designations();
        StationKind::ALL
            .iter()
            .zip(picked.iter())
            .find(|&(_, &id)| id == star_id)
            .map(|(&kind, _)| kind)
    }
}

/// The five promised stars, as a pure function of the galaxy seed.
///
/// Distinct by rejection, which terminates because there are a thousand stars
/// and five to place. The fallback walks forward rather than re-drawing, so
/// even a degenerate galaxy of five stars still gets five different ones.
fn designations(seed: u64, generator_version: u32, star_count: u32) -> [u32; 5] {
    let mut rng = Rng::stream(seed, 0, generator_version, Purpose::Designations);
    let mut picked = [0u32; 5];
    for slot in 0..picked.len() {
        let mut id = rng.below(star_count);
        while picked[..slot].contains(&id) {
            id = (id + 1) % star_count.max(1);
        }
        picked[slot] = id;
    }
    picked
}

/// Place every star.
///
/// One pass, in id order, each star taking its turn from the same stream —
/// so star 400 is where it is because of the 399 before it, and adding a
/// star to the end changes nothing before it.
fn scatter(seed: u64, galaxy_type: GalaxyType, generator_version: u32) -> Vec<Star> {
    let mut rng = Rng::stream(seed, 0, generator_version, Purpose::StarField);
    let mut stars: Vec<Star> = Vec::with_capacity(STAR_COUNT as usize);
    let mut grid = Grid::new();

    // Spirals get their arms rotated as a whole, so two galaxies of the same
    // type do not sit at the same angle.
    let twist = rng.angle();

    for id in 0..STAR_COUNT {
        let mut position = dvec2(0.0, 0.0);
        for _ in 0..PLACEMENT_TRIES {
            position = sample(&mut rng, galaxy_type, twist);
            if grid.clear_of_neighbours(&stars, position) {
                break;
            }
        }
        grid.insert(stars.len(), position);
        stars.push(Star {
            id,
            position,
            name: name::star_name(seed, id, generator_version),
            star_class: StarClass::draw(&mut rng),
        });
    }
    stars
}

/// One candidate position, in whichever shape this galaxy is.
fn sample(rng: &mut Rng, galaxy_type: GalaxyType, twist: f64) -> DVec2 {
    match galaxy_type {
        GalaxyType::Round => {
            // `u.powf(1.5)` rather than `u.sqrt()`: a uniform disc looks flat
            // and empty in the middle, and a galaxy should be dense there.
            let r = GALAXY_RADIUS * rng.unit().powf(1.5);
            DVec2::polar(rng.angle(), r)
        }
        GalaxyType::Elliptical => {
            let r = GALAXY_RADIUS * rng.unit().powf(1.4);
            let p = DVec2::polar(rng.angle(), r);
            // Squashed, then turned, so the long axis is not always across.
            let squashed = dvec2(p.x, p.y * 0.55);
            rotate(squashed, twist)
        }
        GalaxyType::SpiralTwoArm | GalaxyType::Spiral => {
            let arms = galaxy_type.arms();
            // A fifth of the stars are in the bulge. Without it the middle is
            // a hole, because the arms all start at a radius.
            if rng.chance(0.2) {
                let r = GALAXY_RADIUS * 0.18 * rng.unit().powf(1.3);
                return DVec2::polar(rng.angle(), r);
            }
            let arm = rng.below(arms) as f64;
            let along = rng.unit();
            let r = GALAXY_RADIUS * (ARM_INNER + (1.0 - ARM_INNER) * along);
            let winding = ARM_SWEEP * along;
            let angle = twist + arm * core::f64::consts::TAU / arms as f64 + winding;
            // Arms are tight near the middle and fray at the ends, which is
            // what stops the outside looking like four drawn lines.
            let spread = 0.08 + 0.22 * along;
            let p = DVec2::polar(angle + rng.gaussian() * spread, r);
            // And a little off-arm scatter, so the gaps are not empty.
            p.add(dvec2(rng.gaussian(), rng.gaussian()).scale(GALAXY_RADIUS * 0.012))
        }
    }
}

fn rotate(p: DVec2, angle: f64) -> DVec2 {
    let (s, c) = (angle.sin(), angle.cos());
    dvec2(p.x * c - p.y * s, p.x * s + p.y * c)
}

/// A uniform grid over the galaxy, so "is anything too close to here" is a
/// look at nine cells rather than a walk down a thousand stars. At this size
/// the difference is not felt; at ten thousand stars it would be, and the
/// grid is easier to write now than to retrofit.
struct Grid {
    cells: std::collections::HashMap<(i64, i64), Vec<usize>>,
}

impl Grid {
    fn new() -> Grid {
        Grid {
            cells: std::collections::HashMap::new(),
        }
    }

    fn cell_of(p: DVec2) -> (i64, i64) {
        (
            (p.x / MIN_STAR_SEPARATION).floor() as i64,
            (p.y / MIN_STAR_SEPARATION).floor() as i64,
        )
    }

    fn insert(&mut self, index: usize, p: DVec2) {
        self.cells.entry(Grid::cell_of(p)).or_default().push(index);
    }

    fn clear_of_neighbours(&self, stars: &[Star], p: DVec2) -> bool {
        let (cx, cy) = Grid::cell_of(p);
        let limit = MIN_STAR_SEPARATION * MIN_STAR_SEPARATION;
        for dx in -1..=1 {
            for dy in -1..=1 {
                let Some(here) = self.cells.get(&(cx + dx, cy + dy)) else {
                    continue;
                };
                for &i in here {
                    if stars[i].position.sub(p).length_squared() < limit {
                        return false;
                    }
                }
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn a_seed_gives_the_same_galaxy_twice() {
        for &t in &GalaxyType::ALL {
            let a = Galaxy::new(4242, t);
            let b = Galaxy::new(4242, t);
            assert_eq!(a.stars, b.stars);
        }
    }

    #[test]
    fn different_seeds_and_types_give_different_galaxies() {
        let a = Galaxy::new(1, GalaxyType::Spiral);
        let b = Galaxy::new(2, GalaxyType::Spiral);
        let c = Galaxy::new(1, GalaxyType::Round);
        assert_ne!(a.stars, b.stars);
        assert_ne!(a.stars, c.stars);
    }

    #[test]
    fn every_star_is_there_once_and_indexed_by_its_id() {
        let g = Galaxy::new(9, GalaxyType::SpiralTwoArm);
        assert_eq!(g.stars.len(), STAR_COUNT as usize);
        for (i, s) in g.stars.iter().enumerate() {
            assert_eq!(s.id as usize, i);
            assert_eq!(g.star(s.id).unwrap(), s);
        }
        assert!(g.star(STAR_COUNT).is_none());
    }

    #[test]
    fn stars_stay_inside_the_galaxy_and_apart_from_each_other() {
        for &t in &GalaxyType::ALL {
            let g = Galaxy::new(77, t);
            // The off-arm scatter can push a star a little past the radius;
            // what matters is that none of them are flung miles away.
            for s in &g.stars {
                assert!(
                    s.position.length() < GALAXY_RADIUS * 1.3,
                    "{t:?}: star {} at {:?}",
                    s.id,
                    s.position
                );
            }
            let mut too_close = 0;
            for (i, a) in g.stars.iter().enumerate() {
                for b in &g.stars[i + 1..] {
                    if a.position.distance(b.position) < MIN_STAR_SEPARATION {
                        too_close += 1;
                    }
                }
            }
            // Placement gives up after sixteen tries rather than looping, so
            // a crowded core can leave a handful. A handful is the point; a
            // hundred would mean the separation is not being tried for.
            assert!(too_close < 10, "{t:?}: {too_close} pairs of stars too close");
        }
    }

    /// Each shape has to actually be that shape, or "galaxy type" is a label
    /// on four scatters that look the same.
    ///
    /// Spirals are told apart by **unwinding** them first: an arm is a line
    /// in `(angle - sweep * distance out)` even though it is a curve in
    /// angle, so subtracting the winding back off turns the arms into spikes
    /// in a histogram and leaves a shapeless galaxy exactly as shapeless as
    /// it was. Measuring raw angle instead calls a four-armed galaxy round —
    /// see [`ARM_SWEEP`] for why, and note that it is the measurement that is
    /// wrong there rather than the galaxy.
    #[test]
    fn the_shapes_are_different_shapes() {
        let lumpiness = |t: GalaxyType| {
            let g = Galaxy::new(5, t);
            let mut sectors = [0u32; 16];
            for s in &g.stars {
                let r = s.position.length();
                if r < GALAXY_RADIUS * 0.4 {
                    continue; // the bulge is round in every type
                }
                let along = (r / GALAXY_RADIUS - ARM_INNER) / (1.0 - ARM_INNER);
                let unwound = s.position.y.atan2(s.position.x) - ARM_SWEEP * along;
                let a = unwound.rem_euclid(core::f64::consts::TAU);
                let i = ((a / core::f64::consts::TAU) * 16.0) as usize % 16;
                sectors[i] += 1;
            }
            let total: u32 = sectors.iter().sum();
            let mean = total as f64 / 16.0;
            let var = sectors
                .iter()
                .map(|&c| (c as f64 - mean).powi(2))
                .sum::<f64>()
                / 16.0;
            var.sqrt() / mean
        };
        let round = lumpiness(GalaxyType::Round);
        let two = lumpiness(GalaxyType::SpiralTwoArm);
        let four = lumpiness(GalaxyType::Spiral);
        assert!(round < 0.25, "a round galaxy should be even: {round}");
        assert!(two > round * 2.0, "two arms should be lumpy: {two} vs {round}");
        assert!(four > round * 2.0, "four arms should be lumpy: {four} vs {round}");
        // An elliptical is even in angle like a round one, so lumpiness will
        // not separate them: it is told apart by being squashed. Measured
        // along its own axes rather than along x and y, because the squash is
        // turned by a seeded angle and an axis-aligned reading would call a
        // diagonal ellipse round.
        assert!(
            flatness(GalaxyType::Elliptical) < 0.75,
            "an elliptical should be squashed: {}",
            flatness(GalaxyType::Elliptical)
        );
        assert!(
            flatness(GalaxyType::Round) > 0.9,
            "a round galaxy should not be: {}",
            flatness(GalaxyType::Round)
        );
    }

    /// The ratio of a galaxy's short axis to its long one, from the spread of
    /// its stars. One is a circle; a half is squashed flat.
    fn flatness(t: GalaxyType) -> f64 {
        let g = Galaxy::new(5, t);
        let n = g.stars.len() as f64;
        let (mut xx, mut yy, mut xy) = (0.0, 0.0, 0.0);
        for s in &g.stars {
            xx += s.position.x * s.position.x;
            yy += s.position.y * s.position.y;
            xy += s.position.x * s.position.y;
        }
        let (xx, yy, xy) = (xx / n, yy / n, xy / n);
        // The two eigenvalues of the covariance matrix: the variance along
        // the long axis and along the short one.
        let mid = (xx + yy) / 2.0;
        let half = (((xx - yy) / 2.0).powi(2) + xy * xy).sqrt();
        ((mid - half) / (mid + half)).sqrt()
    }

    #[test]
    fn star_classes_are_mostly_small_red_ones() {
        let g = Galaxy::new(3, GalaxyType::Round);
        let m = g
            .stars
            .iter()
            .filter(|s| s.star_class == StarClass::M)
            .count();
        assert!(m > 450 && m < 750, "{m} of a thousand were M");
        // But every class should turn up at least once in a thousand.
        let seen: HashSet<_> = g.stars.iter().map(|s| s.star_class).collect();
        assert_eq!(seen.len(), StarClass::ALL.len(), "{seen:?}");
    }

    #[test]
    fn five_different_stars_are_promised_a_station_each() {
        for &seed in &[0u64, 1, 900, u64::MAX] {
            let g = Galaxy::new(seed, GalaxyType::Spiral);
            let picked = g.designations();
            let unique: HashSet<_> = picked.iter().collect();
            assert_eq!(unique.len(), picked.len(), "{picked:?}");
            for (i, &id) in picked.iter().enumerate() {
                assert!(id < STAR_COUNT);
                assert_eq!(g.designation_for(id), Some(StationKind::ALL[i]));
            }
        }
    }

    /// The load-bearing separation: a star's position and name come off the
    /// star field's stream, so nothing a system does can move them.
    #[test]
    fn systems_cannot_move_stars() {
        let g = Galaxy::new(31, GalaxyType::SpiralTwoArm);
        let before = g.stars.clone();
        for id in [0, 1, 500, 999] {
            let mut r = Rng::stream(g.seed, id, g.generator_version, Purpose::Bodies);
            for _ in 0..1000 {
                r.next_u64();
            }
        }
        assert_eq!(Galaxy::new(31, GalaxyType::SpiralTwoArm).stars, before);
    }
}
