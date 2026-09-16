//! Flying a ship from one place in a system to another.
//!
//! Three things, and nothing else: what a design does when you push it
//! ([`dynamics`]), the trip that takes it somewhere ([`plan_trip`]), and which
//! way round everything is ([`angle`]). It renders nothing, exports nothing to
//! wasm, and holds no state — a [`Plan`] is made once and then *read* at a
//! time, by whoever is asking.
//!
//! # Why it is its own crate
//!
//! The same three answers are wanted in three places that must not each work
//! them out their own way. The browser flies the ship. The native server that
//! will one day be authoritative flies the same ship and has to put it in the
//! same place to the unit. And `crates/world` — the loop both of those run —
//! asks for an arrival time before anybody commits to a trip, which is the
//! same arithmetic asked a different way.
//!
//! It is also why nothing here integrates. See [`plan`]'s note: a plan
//! evaluated at 3.5 minutes gives the same answer whether it was asked once or
//! two hundred and ten times, which is the only way a page stepping at 24x and
//! a server catching up on a day can agree.
//!
//! # What is deliberately absent
//!
//! Manual flight, collisions, gravity, moving bodies, a speed limit, engine
//! exhaust, power, and any notion of a ship that is not a rigid body. A trip
//! is a straight line between two points that do not move, because in this
//! world nothing in a system moves except the ship.

pub mod angle;
pub mod data;
pub mod dynamics;
pub mod plan;

pub use dynamics::{Dynamics, DynamicsError, dynamics};
pub use plan::{
    Effort, Phase, Plan, PlanError, Segment, Spin, State, Target, abort, effort_at, fuel_burned_at,
    plan_trip, state_at,
};

#[cfg(test)]
mod tests;
