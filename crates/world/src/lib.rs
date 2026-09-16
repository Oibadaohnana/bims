//! The game world: a star system, the ship in it, and everything aboard, all
//! advancing on **one clock**.
//!
//! The design phase is the only phase before this one. After Accept, a
//! [`World`] opens with the accepted ship docked at the spawn station and what
//! was left of the pool in the crew's hands, and from then on there is one
//! simulation and one loop: [`World::step`].
//!
//! It renders nothing and exports nothing to wasm. `crates/ship` draws it and
//! `web/ship.js` steps it, the same way `crates/shipdesign` is the rules and
//! `crates/ship` is the pointer. The split is not tidiness — a native server
//! has to run exactly this loop and reach exactly the same world, and two
//! implementations of it would be two different games.
//!
//! # The contract for the steps that come after this one
//!
//! Written down here because the whole point of the loop's shape is that the
//! things which do not exist yet have somewhere to go that is already decided.
//!
//! - **Bims live in ship-design tile coordinates.** The ship's position, its
//!   rotation and its acceleration do not reach them: a Bim walking to the
//!   galley is walking across a grid, and whether that grid happens to be
//!   pointing north or east is the renderer's business and nobody else's.
//! - **Every ship change goes through [`World::on_ship_changed`].** It is the
//!   one place the dynamics are recomputed, and what it promises — the anchor
//!   and the hull do not move, the centre of mass does — is what stops a
//!   welded wall shoving the ship sideways through space.
//! - **Construction and deconstruction follow the mass conservation
//!   contract** in `shipdesign::materials`, use only what is aboard, and ask
//!   [`World::can_modify_part`] first.
//! - **Money is used only while docked.** Away from a station there is nobody
//!   to buy from, and what gets built comes out of the hold or does not get
//!   built.
//! - **Crew, construction and health run inside [`World::step`]**, at the
//!   numbered extension points, on this clock. Not on a second one.
//! - **The design's exposure map is the radiation input.**
//!   `shipdesign::exposure` says which tiles the outside can see into, and
//!   `crates/health` says what standing in one does to a body.
//!
//! # What is deliberately absent
//!
//! Networking, interstellar travel, moving bodies, gravity, collisions, power,
//! oxygen, station interiors, prices that differ by where you are, manual
//! flight, and any speed above [`data::TOP_SPEED`].

pub mod checksum;
pub mod crew;
pub mod data;
pub mod event;
pub mod fixture;
pub mod frame;
pub mod speed;
pub mod world;

pub use checksum::world_checksum;
pub use event::{Refusal, WorldEvent};
pub use frame::Frame;
pub use speed::Speed;
pub use world::{Command, Preview, Ship, ShipState, StartError, World, spawn};

// The three things a caller of this crate wants from the ones underneath it,
// re-exported so it does not have to depend on all four for the sake of a
// type: a target to fly to, a reason it could not be, and where a trip has
// got to.
pub use flight::{Phase, PlanError, Target};

#[cfg(test)]
mod tests;
