//! Deterministic randomness, drawn from a hash rather than from a stream.
//!
//! This is the part of the world generator that most wants getting right, and
//! the rule is short: **there is no global RNG here**. A shared stream would
//! mean the numbers a system gets depend on how many systems were generated
//! before it, which is a different thing on a client that has looked at four
//! stars and a server that has looked at nine hundred. Every draw instead
//! starts from a seed hashed out of what the thing *is* —
//! `(galaxy_seed, star_id, generator_version, purpose)` — so the same star
//! generates the same system whoever asks and in whatever order.
//!
//! [`Purpose`] is what keeps two draws about the same star apart. Without it,
//! the stream that names a star and the stream that scatters its planets are
//! the same stream, and adding a syllable to the naming would move every
//! planet in the galaxy. With it, each is its own sequence and the others do
//! not feel it — which is the whole of "changing system generation must never
//! move stars", written down as a type.
//!
//! The mixer is SplitMix64's finalizer: cheap, well-tested and — the part
//! that matters — the same arithmetic on every target, because it is all
//! integer. A float-based hash would give the wasm client and the native
//! server leave to disagree in the last bit, and one bit is a different
//! galaxy.

/// What a stream is *for*. Drawing two different kinds of thing from one
/// stream ties them together for ever; this is how they are kept apart.
///
/// The discriminants are written out because they are part of the world's
/// identity: renumbering one silently regenerates every galaxy, so a change
/// here is a `generator_version` bump.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u64)]
pub enum Purpose {
    /// Where the stars are, and what they are. Galaxy-wide, drawn once.
    StarField = 1,
    /// Which systems are promised a station of a given kind.
    Designations = 2,
    /// How run-down a system is.
    Desolation = 3,
    /// What bodies a system has, and where.
    Bodies = 4,
    /// Whether a system has a station, of what kind, and where.
    Stations = 5,
    /// What a station has in its stores, and what is wrong with it.
    StationContents = 6,
    /// The seed handed on to the map generator that will build an interior.
    MapSeed = 7,
    /// What a belt yields to a walk outside. Its own stream, so adding it
    /// moved nothing else in the galaxy.
    BeltYield = 8,
}

/// SplitMix64's finalizer. Takes a counter-ish input to a well-spread output,
/// which is what lets the seeds below be built by simple mixing rather than
/// by a proper hash function.
pub fn mix(mut x: u64) -> u64 {
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
    x ^ (x >> 31)
}

/// The seed for one stream. Order matters and each part is mixed in on its
/// own, so two parts swapping values does not give the same answer.
pub fn seed_for(galaxy_seed: u64, star_id: u32, generator_version: u32, purpose: Purpose) -> u64 {
    let mut h = mix(galaxy_seed);
    h = mix(h ^ mix(star_id as u64).wrapping_add(0x9e3779b97f4a7c15));
    h = mix(h ^ mix(generator_version as u64).wrapping_add(0xc2b2ae3d27d4eb4f));
    mix(h ^ mix(purpose as u64).wrapping_add(0x165667b19e3779f9))
}

/// A stream of numbers. SplitMix64 proper — one addition and a mix per draw,
/// no state to get wrong, and it never repeats inside any run this generator
/// will ever make.
#[derive(Clone, Debug)]
pub struct Rng {
    state: u64,
}

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng { state: mix(seed) }
    }

    /// The stream for one purpose within one system. Every draw in this crate
    /// starts here or from [`Rng::branch`].
    pub fn stream(galaxy_seed: u64, star_id: u32, generator_version: u32, purpose: Purpose) -> Rng {
        Rng::new(seed_for(galaxy_seed, star_id, generator_version, purpose))
    }

    /// A stream hanging off this one, told apart by `tag`.
    ///
    /// For the things there are several of within one purpose — the second
    /// station's map seed, the fourth body's name — where drawing them in
    /// sequence would mean removing the third one shifts everything after it.
    /// This does not advance `self`.
    pub fn branch(&self, tag: u64) -> Rng {
        Rng::new(self.state ^ mix(tag).wrapping_add(0x9e3779b97f4a7c15))
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e3779b97f4a7c15);
        mix(self.state)
    }

    /// Uniform in `[0, 1)`. Fifty-three bits, which is every bit an `f64`
    /// mantissa has — the world is measured in hundreds of millions of units
    /// and a coarser draw would show as banding in the layouts.
    pub fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    pub fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.unit()
    }

    /// A whole number in `0..n`, and 0 for an empty range. Taken off the top
    /// bits by multiplication rather than by remainder, which would lean on
    /// the low bits and bias the result towards the start of the range.
    pub fn below(&mut self, n: u32) -> u32 {
        if n == 0 {
            return 0;
        }
        ((self.next_u64() >> 32) * n as u64 >> 32) as u32
    }

    pub fn chance(&mut self, p: f64) -> bool {
        self.unit() < p
    }

    /// An angle, all the way round.
    pub fn angle(&mut self) -> f64 {
        self.range(0.0, core::f64::consts::TAU)
    }

    /// Roughly standard-normal, by Irwin–Hall with four draws. Good enough
    /// for scattering stars off an arm and cheap enough to do a few thousand
    /// times; nothing here needs a real Gaussian.
    pub fn gaussian(&mut self) -> f64 {
        let sum = self.unit() + self.unit() + self.unit() + self.unit();
        (sum - 2.0) * 1.732_050_807_568_877_2
    }

    /// One of `items`, or `None` if there are none.
    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> Option<&'a T> {
        if items.is_empty() {
            return None;
        }
        items.get(self.below(items.len() as u32) as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stream_is_the_same_every_time() {
        let draw = || {
            let mut r = Rng::stream(12345, 7, 1, Purpose::Bodies);
            (0..8).map(|_| r.next_u64()).collect::<Vec<_>>()
        };
        assert_eq!(draw(), draw());
    }

    /// The point of `Purpose`. Same galaxy, same star, different question —
    /// and the answers must not be the same numbers.
    #[test]
    fn purposes_do_not_share_a_stream() {
        let mut a = Rng::stream(1, 1, 1, Purpose::Bodies);
        let mut b = Rng::stream(1, 1, 1, Purpose::Stations);
        assert_ne!(a.next_u64(), b.next_u64());
    }

    #[test]
    fn the_parts_of_a_seed_do_not_commute() {
        assert_ne!(
            seed_for(1, 2, 3, Purpose::Bodies),
            seed_for(2, 1, 3, Purpose::Bodies)
        );
        assert_ne!(
            seed_for(1, 2, 3, Purpose::Bodies),
            seed_for(1, 2, 4, Purpose::Bodies)
        );
    }

    #[test]
    fn neighbouring_stars_are_not_neighbouring_streams() {
        let first = |id| Rng::stream(99, id, 1, Purpose::Bodies).next_u64();
        let a = first(41);
        let b = first(42);
        // A weak hash would leave these close together; a good one leaves
        // them unrelated. Anything sharing a whole top byte would be a smell.
        assert_ne!(a >> 56, b >> 56);
    }

    #[test]
    fn branching_does_not_move_the_parent() {
        let r = Rng::new(7);
        let mut parent = r.clone();
        let _ = r.branch(3);
        let mut again = r.clone();
        assert_eq!(parent.next_u64(), again.next_u64());
        assert_ne!(r.branch(3).next_u64(), r.branch(4).next_u64());
    }

    #[test]
    fn unit_stays_inside_its_range() {
        let mut r = Rng::new(3);
        let (mut lo, mut hi) = (1.0f64, 0.0f64);
        for _ in 0..100_000 {
            let u = r.unit();
            assert!((0.0..1.0).contains(&u));
            lo = lo.min(u);
            hi = hi.max(u);
        }
        // It should actually cover the range rather than hugging the middle.
        assert!(lo < 0.001 && hi > 0.999, "{lo}..{hi}");
    }

    #[test]
    fn below_is_bounded_and_covers_its_range() {
        let mut r = Rng::new(5);
        let mut seen = [0u32; 6];
        for _ in 0..60_000 {
            let n = r.below(6);
            assert!(n < 6);
            seen[n as usize] += 1;
        }
        assert_eq!(r.below(0), 0);
        // Ten thousand expected in each bucket; a badly biased draw shows up
        // long before this is tight.
        assert!(seen.iter().all(|&c| c > 8_000 && c < 12_000), "{seen:?}");
    }
}
