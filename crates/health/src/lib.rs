//! What is wrong with a body, what it costs, and when it kills.
//!
//! One [`HealthState`] per Bim. It holds health points, an accumulated
//! radiation dose and a cancer if there is one, and [`update`] moves all of
//! that on by a number of game minutes given what the world is doing to the
//! body this instant. It renders nothing, exports nothing to wasm, knows
//! nothing about Bims and nothing about tiles — the play phase will hold one
//! of these per crew member and work out the [`Exposure`] from
//! `shipdesign::ExposureMap` and where the Bim is standing.
//!
//! It compiles for native and for `wasm32-unknown-unknown` and has to give
//! the same answers on both, because a native server will one day have to
//! agree with the browser about who survived a walk outside. The native half
//! of that check is `tests.rs`; the wasm half is that the crate is built for
//! wasm32 by `nix flake check` along with everything else.
//!
//! # This is not the room's `health.rs`
//!
//! `crates/game/src/health.rs` is the behaviour test room's hunger-and-sleep
//! bar. Nothing here imports it and nothing here is ported from it — that
//! room is a separate game that happens to live in the same repository. The
//! one thing taken from it is a lesson, and it is written out again in
//! `state.rs`: **health that mends every frame can undo a killing blow
//! before anything checks whether the body is dead.**
//!
//! Hunger, sleep loss and filth are meant to arrive here eventually, as
//! further [`Condition`] variants. They have not, and adding them is its own
//! step with its own decisions about how the room's clocks translate.
//!
//! # How it is built
//!
//! - **Everything is a condition.** A [`Condition`] is a damage rate,
//!   whether it stops the body mending, and what it does to how fast the
//!   body works and walks. Radiation and cancer are the first two kinds, and
//!   there is no third mechanism waiting to be discovered: [`update`] adds up
//!   the conditions in force and does not know what any of them is.
//! - **Radiation is a dose, not a switch.** It rises while a body is in the
//!   open and falls while it is under cover, at half the rate; what it does
//!   depends on how much of it there is. The meter stays up until it is back
//!   at nothing, so a player can watch a Bim come clear.
//! - **Cancer is permanent.** The first time the dose reaches
//!   [`data::CANCER`] the body has it, and there is no cure in this step — no
//!   medical bay exists to build. What makes it dangerous is less the damage
//!   than that **nothing mends while it is there**.
//! - **Any step length gives the same answer.** [`update`] cuts the interval
//!   at every boundary it crosses and applies a constant rate in closed form
//!   to each piece, so a frame, an hour and a day of catching up all agree.
//!   That is not a nicety: the browser and the native server will not be
//!   stepping in the same rhythm.
//! - **Death is final.** [`update`] on a dead body changes nothing and says
//!   nothing.
//!
//! # What is deliberately absent
//!
//! Treatment of any kind, a medical bay, anything to do with Bims, any UI or
//! meter drawing, hunger, sleep and filth, and radiation that differs by star
//! system — the last of those is [`Exposure::Exposed`]'s `intensity` waiting
//! for somebody to pass something other than 1.0.

pub mod condition;
pub mod data;
pub mod event;
pub mod state;

pub use condition::{CancerStage, Condition, Conditions, Effect, Effects, RadiationStage};
pub use data::{CANCER, CRITICAL, MAX_HEALTH, SICKNESS, data_is_sound};
pub use event::HealthEvent;
pub use state::{
    CancerState, Exposure, HealthState, conditions, effects, meter_visible, net_health_rate, update,
};

#[cfg(test)]
mod tests;
