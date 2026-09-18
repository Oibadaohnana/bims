//! The lobby's World tab.
//!
//! The app hands [`Lobby::new`] a seed, a galaxy type and a canvas size; it
//! builds the galaxy — and every system in it, see [`lobby::Lobby`] — and
//! then answers questions about it and draws it. The draw format is the
//! room's twelve floats a shape, so the app's one replay loop paints this
//! as it paints the deck. It imports nothing from `crates/game` or
//! `crates/ship`: the lobby is the screen in front of the game, and has no
//! ship and no room.
//!
//! **Nothing here decides anything.** Which star has a station, where a body
//! is, what a station is called — every answer is `worldgen`'s, and the
//! native server will give the same one. What is here is a camera, a pick
//! under the pointer, and a buffer of shapes.
//!
//! **No strings.** A star's name is three numbers out of `worldgen::name`,
//! and the words are the app's — `STAR_WORDS` in `crates/app/src/names.rs`.
//! That is the same rule the room and the designer keep, and for the same
//! reason: a server has no words to say, and a table of them in two places
//! is a table that disagrees with itself.

pub mod diagram;
pub mod draw;
pub mod lobby;
pub mod preview;

pub use lobby::Lobby;

/// "There is no such thing." What an id answer is when the thing asked
/// about is not there — no star under the pointer, no parent body, no
/// system open. `u32::MAX` rather than zero, because star 0, body 0 and
/// station 0 all exist.
pub const NONE: u32 = u32::MAX;

#[cfg(test)]
mod tests;
