//! One number that says whether two builds generated the same galaxy.
//!
//! The lobby draws the star map in wasm and a native server will one day
//! generate the same galaxy from the same seed, and the only cheap way to
//! find out that the two have drifted — a `powf` that rounds differently, a
//! `sin` out of a different libm — is to run a fixed seed on both and compare
//! each against the same written-down number. [`crate::fixture`] holds the
//! numbers; `crates/worldgen/src/tests.rs` is the native end and
//! `lobby_galaxy_checksum` in `crates/lobby` is the wasm end, read by
//! `scratchpad/builder-check.mjs`.
//!
//! # What goes in
//!
//! Everything a player can see or click: every star's position, class and
//! name, and — because "does this star have a station" has to be answered by
//! **generating the system** rather than by looking at the designations —
//! every system's bodies and stations, with their kinds, parents, positions
//! and names, what each station stocks and whose side it is on. A galaxy
//! whose stars all matched and whose stations did not would put two
//! players' spawn pickers on different stations — or one player's crew
//! ashore at a station the other's is shooting its way out of.
//!
//! # Why the floats are rounded
//!
//! The same reason `world::checksum` gives: `sin`, `cos` and `powf` come out
//! of the platform's libm natively and Rust's own on `wasm32-unknown-unknown`,
//! and those may differ in the last bit. Positions go onto a grid on the way
//! in — a hundredth of a light year across the galaxy, a thousandth of a unit
//! inside a system — which is far finer than any real divergence and far
//! coarser than a rounding one.

use crate::galaxy::Galaxy;
use crate::system::StarSystem;

/// FNV-1a, written out by hand. Not a `Hash` derive and not `DefaultHasher`:
/// those are explicitly allowed to differ between builds, and this number
/// crosses between machines.
struct Fnv(u64);

const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const PRIME: u64 = 0x0000_0100_0000_01b3;

impl Fnv {
    fn new() -> Fnv {
        Fnv(OFFSET)
    }

    fn eat(&mut self, value: u64) {
        for byte in value.to_le_bytes() {
            self.0 ^= byte as u64;
            self.0 = self.0.wrapping_mul(PRIME);
        }
    }

    /// A float, onto a grid first. See the module note.
    fn eat_rounded(&mut self, value: f64, scale: f64) {
        let quantised = if value.is_finite() {
            (value * scale).round()
        } else {
            f64::MAX
        };
        self.eat(quantised.to_bits());
    }
}

/// A hundredth of a light year. The galaxy is fifty thousand across.
const GALAXY_GRID: f64 = 100.0;

/// A thousandth of a world unit. A system is a hundred million across.
const SYSTEM_GRID: f64 = 1_000.0;

/// The checksum of a galaxy and the systems generated from it.
///
/// `systems` has to be every star's system in id order — which is what the
/// lobby holds anyway, because it needs the lot to say which stars have a
/// station. [`Galaxy::checksum`] generates them for a caller that has not.
pub fn galaxy_checksum(galaxy: &Galaxy, systems: &[StarSystem]) -> u64 {
    let mut hash = Fnv::new();

    hash.eat(galaxy.seed);
    hash.eat(galaxy.generator_version as u64);
    hash.eat(galaxy.galaxy_type as u64);
    hash.eat(galaxy.stars.len() as u64);
    for star in &galaxy.stars {
        hash.eat(star.id as u64);
        hash.eat_rounded(star.position.x, GALAXY_GRID);
        hash.eat_rounded(star.position.y, GALAXY_GRID);
        hash.eat(star.star_class as u64);
        hash.eat(star.name.word as u64);
        hash.eat(star.name.number as u64);
        hash.eat(star.name.part as u64);
    }

    hash.eat(systems.len() as u64);
    for system in systems {
        hash.eat(system.star_id as u64);
        hash.eat(system.bodies.len() as u64);
        for body in &system.bodies {
            hash.eat(body.id as u64);
            hash.eat(body.kind as u64);
            hash.eat_rounded(body.position.x, SYSTEM_GRID);
            hash.eat_rounded(body.position.y, SYSTEM_GRID);
            hash.eat(body.name.part as u64);
        }
        hash.eat(system.stations.len() as u64);
        for station in &system.stations {
            hash.eat(station.id as u64);
            hash.eat(station.kind as u64);
            hash.eat(station.parent_body.map(u64::from).unwrap_or(u64::MAX));
            hash.eat_rounded(station.position.x, SYSTEM_GRID);
            hash.eat_rounded(station.position.y, SYSTEM_GRID);
            hash.eat(station.name.word as u64);
            hash.eat(station.name.number as u64);
            hash.eat(station.name.part as u64);
            // The shelf and the side, which a player sees the moment they
            // dock: two builds whose stations stood in the same places and
            // disagreed about which of them shoot would be two galaxies.
            hash.eat(station.stock.0 as u64);
            hash.eat(u64::from(station.hostile));
        }
    }

    hash.0
}

impl Galaxy {
    /// Every system, in star id order. What the lobby holds so it can say
    /// which stars have a station without asking the designations, which
    /// only know about the five promised ones.
    pub fn every_system(&self) -> Vec<StarSystem> {
        self.stars
            .iter()
            .map(|s| {
                self.system(s.id)
                    .expect("a star of this galaxy has a system")
            })
            .collect()
    }

    /// [`galaxy_checksum`], generating the systems on the way.
    pub fn checksum(&self) -> u64 {
        galaxy_checksum(self, &self.every_system())
    }
}
