//! The world: a galaxy of stars, what is in each system, and the station
//! blueprints a later step will build interiors from.
//!
//! One crate, compiled for **both** sides. The client builds it to wasm to
//! draw the star map and preview a system; the native server that will one
//! day be authoritative builds it for x86 and generates exactly the same
//! world from the same seed. Offline play and `./run game` use the same
//! code again, locally. That is the whole reason it is not simply part of the
//! game: two implementations of a generator are two galaxies.
//!
//! # What this step produces, and what it does not
//!
//! It produces a **blueprint** — deterministic data describing a station —
//! and it produces no map. No tiles, no rooms, no items on a deck, no
//! interiors of any kind. Every field on a [`StationBlueprint`] is something
//! the future map generator will either place or honour, and a field that is
//! neither does not belong on it.
//!
//! # The three rules that hold it together
//!
//! 1. **Stars come off one stream, systems off their own.** Reworking what is
//!    inside systems must never move a star, because a player learns the map
//!    long before they learn a system. [`rng::Purpose`] is how that is
//!    enforced rather than merely intended.
//! 2. **A galaxy is a seed, a type and a version.** Nothing else, and no
//!    lobby setting reaches in at all. There was one — a multiplier on what a
//!    station had in its stores — and it went with the stores themselves when
//!    the crew started bringing money instead; `a_seed_and_a_type_are_the_whole_of_the_input`
//!    in [`system`] is what keeps it that way.
//! 3. **Distances are in days, not units.** Every layout rule is written
//!    against [`data::REFERENCE_SHIP`], which is fixed and is not anybody's
//!    actual ship. [`layout`] is the specification; [`system`] is the
//!    generator that has to satisfy it.
//!
//! # Versioning
//!
//! [`GENERATOR_VERSION`] is mixed into every seed, so bumping it regenerates
//! every galaxy. The list of things that force a bump is at the top of
//! [`data`] — it is short, and it is longer than it looks, because the travel
//! formula and the day length are on it.

pub mod checksum;
pub mod data;
pub mod fixture;
pub mod galaxy;
pub mod layout;
pub mod math;
pub mod name;
pub mod rng;
pub mod system;

#[cfg(test)]
mod tests;

pub use checksum::galaxy_checksum;
pub use data::{BodyKind, HazardKind, StationKind, TravelBand};
pub use galaxy::{Galaxy, GalaxyType, Star, StarClass};
pub use layout::Fault;
pub use math::DVec2;
pub use name::Name;
pub use system::{Body, Node, StarSystem, StationBlueprint};

/// Which generation this world was made by.
///
/// Mixed into every seed, so a bump is not a migration — it is a different
/// galaxy for every seed there has ever been. That is the intended effect and
/// the reason the bump list in [`data`] is kept short and explicit: a change
/// that alters where anything is has to be declared, because the alternative
/// is two players on different builds walking around what they both think is
/// the same station.
pub const GENERATOR_VERSION: u32 = 2;
