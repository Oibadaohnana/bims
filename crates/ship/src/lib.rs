//! The ship designer and the game it starts.
//!
//! The design phase runs here: the app hands [`Session::design`] a canvas
//! size and the numbers the lobby chose, then each frame asks it to rebuild
//! a shape buffer and paints that. The format is the room's twelve floats a
//! shape, so one replay loop paints either half of the game — and this crate
//! imports the room only to run it *aboard*, through `crates/world`.
//!
//! Every rule is next door in `shipdesign`, which renders nothing and knows
//! nothing about a pointer. Nothing in here decides whether a part may be
//! placed; it asks.
//!
//! # The boundary
//!
//! **No strings.** The parts, the prices, the reasons an edit was refused
//! and the faults in a design are all numbers, and `PART_NAMES`,
//! `EDIT_LINES` and `ISSUE_LINES` in `crates/app/src/names.rs` are where
//! the words live. Adding a part is an enum variant, a name in that file,
//! and the range check in the tests. A native server will one day run
//! `shipdesign` and `world` and it has no words to say; keeping the words
//! out of here is what keeps that true.

pub mod camera;
pub mod draw;
pub mod editor;
pub mod fittings;
pub mod game;
pub mod hull;
pub mod paint;
pub mod session;
pub mod starfield;
pub mod view;
pub mod world_paint;

pub use session::{NONE, Preset, Session};

#[cfg(test)]
mod tests;
