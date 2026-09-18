//! Bims — the room.
//!
//! The Bims' simulation: two characters on a deck, their needs, their errands
//! and the fixtures they use, and the drawing of all of it as a flat list of
//! shapes. Nothing in here knows what a window is. `crates/app` owns a
//! [`game::Game`] — on its own as the behaviour test room, or aboard the
//! designed ship through `crates/world` — steps it, and turns
//! [`game::Game::shapes`] into pixels. The panels, the menus and every word on
//! screen are the app's; what crosses out of here is numbers and shapes, and
//! that is deliberate: a native server will one day run this same room and
//! it has no words to say either.

// Public, because the ship game runs this room aboard the designed ship —
// `world` reaches `aboard`, `game`, `room` and `math` — and the app drives
// it through `game` and names what it is told through `room`'s codes.
pub mod aboard;
pub mod bath;
pub mod bim;
pub mod character;
pub mod clock;
pub mod combat;
pub mod dish;
pub mod door;
pub mod draw;
pub mod filth;
pub mod galley;
pub mod game;
pub mod health;
pub mod hydro;
pub mod manager;
pub mod math;
pub mod memory;
pub mod nav;
pub mod needs;
pub mod rng;
pub mod room;
pub mod schedule;
pub mod sight;
pub mod social;
pub mod task;
pub mod work;

/// The shared `time` crate, pulled into the crate root so every module reaches
/// it as `crate::time`. That spelling is deliberate: the native probes in
/// `scratchpad/` declare the crate's modules by `#[path]` and link nothing at
/// all, so they stand a plain `mod time;` over the same file in its place.
/// Written as `time::` it would resolve only through the extern prelude, and
/// every probe would need a `--extern` and a build step to go with it.
pub(crate) use ::time;
