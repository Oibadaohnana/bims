//! What a ship is made of and what may be done to one.
//!
//! This crate is the **design phase**: a tile grid, a table of parts, one
//! function that changes a design, one that says what is wrong with it, and
//! one that gives it an identity two players can both accept. It renders
//! nothing and exports nothing to wasm — `crates/ship` beside it is the
//! cdylib that draws a design on a canvas, and the play phase will one day
//! fly the same design. Neither may have its own idea of what a legal ship is.
//!
//! It compiles for native and for `wasm32-unknown-unknown`, and
//! [`design_hash`] has to give the **same number on both**. That is why
//! nothing in the data or the hash is a `usize` or a float, and why no
//! `HashMap` is iterated anywhere in here.
//!
//! # The two phases
//!
//! **Design** — this crate. Placing and removing is instant, paid out of the
//! station's stockpile, checked by [`validate`], and finished when every
//! player has accepted the same [`design_hash`]. No Bims exist.
//!
//! **Play** — stage 5, not written. Bims spawn, and every later change has to
//! be constructed or deconstructed by one of them.
//!
//! They are two phases of **one ship**. The data model here is the one play
//! uses; it is not a separate editor format that gets converted.
//!
//! # The contract for stage 5
//!
//! Four promises this crate makes, or asks for, and that the play phase has
//! to keep. None of them is implemented here.
//!
//! - **Every walkable tile must actually be walkable.** [`validate`] passes a
//!   design whose use spots are all reachable over floor tiles whose object
//!   layer is empty or non-blocking — including one-tile corridors and
//!   doorways. The room's navigation **cannot be assumed to satisfy that**:
//!   it is a 10-unit cell grid that inflates every obstacle by a
//!   `BODY_MARGIN` of 23, over a tile that is [`parts::TILE`] = 52 units. A
//!   one-tile gap between two walls is 52 units wide with 23 taken off each
//!   side, which leaves 6 — and the centre-sampled line test in `nav.rs`
//!   already has a known failure mode at exactly that kind of clearance (see
//!   CLAUDE.md, "A route the body cannot hold to"). So **stage 5 needs
//!   tile-based navigation, or has to prove the existing one walks every
//!   design this crate accepts**. Accepting a ship the crew cannot cross
//!   would look like a Bim frozen mid-errand, which is the hardest failure
//!   aboard to diagnose.
//! - **A use spot is where a Bim stands to use a part.** Not where the part
//!   is. The chain that walks to a cold store walks to one of
//!   [`parts::use_spots`], and the design was validated on exactly that.
//! - **Bim `i` spawns at bunk `i`** — bunks in part-id order, players in
//!   lobby-slot order. Which is why ids only ever climb and a removed one is
//!   never reissued.
//! - **After Accept, nothing is instant.** Every change is a Bim's work:
//!   construction or deconstruction. The stockpile stops being a budget and
//!   becomes something that has to be hauled.
//!
//! # What is deliberately absent
//!
//! Power, oxygen and airtightness, engine exhaust clearance, ship rotation,
//! construction labour, hauling, undo, and the final art. A part has a mass,
//! a cost, a footprint and somewhere to stand — and nothing else, because
//! every field that exists is a field something has to keep true.

pub mod budget;
pub mod design;
pub mod fixture;
pub mod mass;
pub mod parts;
pub mod validate;

pub use budget::{BASE_STOCKPILE, Budget, RESOURCE_COUNT};
pub use design::{Edit, EditError, Grid, PlacedPart, ShipDesign, apply, design_hash};
pub use mass::{acceleration, hull_mass, ship_mass};
pub use parts::{Layer, PartDef, PartKind, Rotation, TILE};
pub use validate::{Issue, IssueCode, Severity, has_errors, validate};

#[cfg(test)]
mod tests;
