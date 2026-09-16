//! Names, as numbers.
//!
//! **No strings cross the wasm boundary.** That rule is not relaxed for a
//! thousand stars — it is the reason there are a thousand of them and no
//! table of a thousand words anywhere. A [`Name`] is three small integers and
//! the host turns them into something to read, exactly as `SPOT_NAMES` and
//! `CREW_NAMES` already do for the room.
//!
//! # What the host has to hold
//!
//! Two word tables, and their lengths have to match [`STAR_WORDS`] and
//! [`STATION_WORDS`]. If the generator hands over `word: 40` and the host's
//! table has thirty entries, the name comes out blank or as `undefined` —
//! which is the same failure a missing `MEMORY_LINES` entry gives, and it
//! reads as the star not existing rather than as a table being short. A
//! generator that grows its tables therefore has to grow the host's in the
//! same commit.
//!
//! # How each kind is read
//!
//! | thing | rendered as | example |
//! | --- | --- | --- |
//! | star | word, a dash, the number | `Tanis-284` |
//! | body | its star's name, then a roman numeral for `part` | `Tanis-284 III` |
//! | station | word, the number, and `part` as a mark if there is one | `Cordell Yard 7` |
//!
//! A body has no word of its own on purpose: a planet named independently of
//! its star reads as a second system. The host already has the star when it
//! draws the system, so the one number is all it is short of.

/// Slots in the host's table of star words. Every `Name::word` a star gets is
/// below this.
pub const STAR_WORDS: u16 = 48;

/// Slots in the host's table of station words.
pub const STATION_WORDS: u16 = 32;

/// "There is no number here." Not zero: a station can legitimately be mark 0
/// and a star could be catalogued at nought.
pub const NO_NUMBER: u16 = u16::MAX;

/// A name, in three numbers. See the table above for how each kind of thing
/// is read out of them.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Name {
    /// Index into whichever of the host's word tables suits the thing named.
    /// [`NO_NUMBER`] when the thing borrows its star's word, as a body does.
    pub word: u16,
    /// The catalogue number.
    pub number: u16,
    /// A further designation: which planet out from the star, which mark of
    /// station. [`NO_NUMBER`] when there is none.
    pub part: u16,
}

impl Name {
    pub fn star(word: u16, number: u16) -> Name {
        Name {
            word,
            number,
            part: NO_NUMBER,
        }
    }

    /// The `n`th body out from its star, counting from one — which is what
    /// makes it a roman numeral on the page rather than an array index.
    pub fn body(ordinal: u16) -> Name {
        Name {
            word: NO_NUMBER,
            number: NO_NUMBER,
            part: ordinal,
        }
    }

    pub fn station(word: u16, number: u16, mark: u16) -> Name {
        Name {
            word,
            number,
            part: mark,
        }
    }
}

/// How many distinct star names there are to go round. A power of two because
/// the shuffle below is a multiplication modulo it, and that is a bijection
/// only when the modulus is a power of two and the multiplier is odd.
const NAME_SPACE: u32 = 1 << 16;

/// A star's name, and **no two stars share one**.
///
/// Uniqueness is by construction rather than by rejection: the star's id is
/// run through an odd multiply-and-offset modulo a power of two, which
/// permutes `0..NAME_SPACE` without collisions, and the result is split into
/// a word and a catalogue number. So it is still a seeded scatter — a
/// different galaxy names its stars differently — but no galaxy ever has two
/// `Tanis-284`, which on a map that is clicked rather than read would be a
/// genuine trap.
///
/// It holds for `star_id < NAME_SPACE`. A galaxy of more than 65536 stars
/// would start repeating, which is why [`crate::galaxy::STAR_COUNT`] is
/// checked against it.
pub fn star_name(galaxy_seed: u64, star_id: u32, generator_version: u32) -> Name {
    let shuffle = crate::rng::seed_for(
        galaxy_seed,
        0,
        generator_version,
        crate::rng::Purpose::StarField,
    );
    // Odd, so the multiplication is invertible modulo a power of two.
    let odd = (shuffle | 1) as u32;
    let offset = (shuffle >> 32) as u32;
    let slot = star_id.wrapping_mul(odd).wrapping_add(offset) % NAME_SPACE;

    let word = (slot % STAR_WORDS as u32) as u16;
    let number = (slot / STAR_WORDS as u32) as u16 + 1;
    Name::star(word, number)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn no_two_stars_share_a_name() {
        for &seed in &[0u64, 1, 42, 0xdead_beef_cafe_f00d] {
            let mut seen = HashSet::new();
            for id in 0..crate::galaxy::STAR_COUNT {
                assert!(
                    seen.insert(star_name(seed, id, crate::GENERATOR_VERSION)),
                    "seed {seed} repeated a name at star {id}"
                );
            }
        }
    }

    #[test]
    fn a_word_is_always_one_the_host_has() {
        for id in 0..crate::galaxy::STAR_COUNT {
            let n = star_name(7, id, crate::GENERATOR_VERSION);
            assert!(n.word < STAR_WORDS, "star {id} wanted word {}", n.word);
            assert_ne!(n.number, NO_NUMBER);
        }
    }

    /// Different galaxies name the same star differently, or the seed is not
    /// reaching the naming at all.
    #[test]
    fn a_different_galaxy_names_its_stars_differently() {
        let a: Vec<_> = (0..64)
            .map(|id| star_name(1, id, crate::GENERATOR_VERSION))
            .collect();
        let b: Vec<_> = (0..64)
            .map(|id| star_name(2, id, crate::GENERATOR_VERSION))
            .collect();
        assert_ne!(a, b);
    }

    /// The whole point of naming from the star field's stream and not from
    /// the system's: what is in a system must not touch what its star is
    /// called, or every rename would move every planet.
    #[test]
    fn a_name_is_the_same_whatever_else_is_asked_for() {
        let before = star_name(3, 11, crate::GENERATOR_VERSION);
        let _ =
            crate::rng::Rng::stream(3, 11, crate::GENERATOR_VERSION, crate::rng::Purpose::Bodies)
                .next_u64();
        assert_eq!(before, star_name(3, 11, crate::GENERATOR_VERSION));
    }
}
