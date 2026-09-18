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
//! **Design** — this crate. Placing, removing, buying and selling are all
//! instant, all paid out of the crew's shared pool of money, checked by
//! [`validate`], and finished when every player has accepted the same
//! [`design_hash`]. No Bims exist.
//!
//! **Play** — `crates/world`. The design phase ends at the last Accept and
//! the world opens with that ship docked at the spawn station; from then on
//! there is one clock and everything aboard runs on it. Bims, construction and
//! health are not wired into it yet, and when they are, every later change has
//! to be constructed or deconstructed by a Bim.
//!
//! They are two phases of **one ship**. The data model here is the one play
//! uses; it is not a separate editor format that gets converted — `world`
//! holds a [`ShipDesign`] and changes it through [`apply`] like anything else.
//!
//! # Which way round a ship is
//!
//! The flight step settled this, and it is a fact about the **design** rather
//! than about the renderer, so it is written down here:
//!
//! - **Forward is the design grid's up.** A [`PartKind::Engine`] at
//!   [`Rotation::R0`] pushes the ship along its own nose, and that is what
//!   [`parts::Rotation::facing`] means. At a heading of 0 the game draws the
//!   design exactly as it was laid out; at a heading of π/2 the top of the
//!   grid is pointing east.
//! - **Only a [`PartKind::Thruster`] turns it.** Main engines push through the
//!   centre of mass and produce no torque wherever they are bolted — see
//!   `flight::dynamics` for why. A thruster's `torque_thrust` becomes a torque
//!   through its distance from the centre of mass, so where one is placed is
//!   the whole of what it is worth.
//! - **The game view rotates the drawn design by the heading**, and rotates
//!   nothing else. The camera is north-up and never turns.
//!
//! # The contract for the play phase
//!
//! Promises this crate makes, or asks for, and that the play phase has to
//! keep. The last three are kept by `crates/world` now; the rest are still
//! waiting on the crew, the construction step and health.
//!
//! - **Every walkable tile must actually be walkable.** [`validate`] passes a
//!   design whose use spots are all reachable over floor tiles whose object
//!   layer is empty or non-blocking — including one-tile corridors and
//!   doorways. The room's navigation keeps that for a **straight** one-tile
//!   gap: aboard, its grid is phased to the tiles (`Nav::tiled`, five cells
//!   a tile), so a tile's middle is always a cell's middle and the six
//!   units a `BODY_MARGIN` of 23 leaves down a 52-unit gap always hold a
//!   cell — `a_one_tile_corridor_can_be_walked` in `crates/world` pins it,
//!   corner included. What it still does **not** walk is a gap that is
//!   only diagonal: two solids touching corner to corner one tile apart
//!   leave nothing a body of that radius fits through, and [`validate`]
//!   does not know that. Accepting a ship the crew cannot cross would look
//!   like a Bim frozen mid-errand, which is the hardest failure aboard to
//!   diagnose.
//! - **A use spot is where a Bim stands to use a part.** Not where the part
//!   is. The chain that walks to a cold store walks to one of
//!   [`parts::use_spots`], and the design was validated on exactly that.
//! - **Bim `i` spawns at bunk `i`** — bunks in part-id order, players in
//!   lobby-slot order. Which is why ids only ever climb and a removed one is
//!   never reissued.
//! - **After Accept, nothing is instant.** Every change is a Bim's work:
//!   construction or deconstruction. Money stops being a budget to draw a
//!   ship against and becomes something that has to be earned and spent
//!   somewhere.
//! - **[`validate::ExposureMap`] is the input for radiation.** Which tiles
//!   the outside can see into is worked out here and handed over; what it
//!   does to a Bim standing in one — over what time, with what effect on
//!   health — is the play phase's and is not decided.
//! - **What is bought is stowed where its class says.** Food in a cold
//!   store, fuel in a tank, everything else on a shelf; `economy::storage` is
//!   the mapping and `PartDef::capacity` is what provides each class. A play
//!   phase that moves a crate of ore into the fridge has broken the contract
//!   the purchase was checked against.
//! - **The money left over carries into the play phase.** It is not spent at
//!   Accept and it is not converted into anything: it is what the crew have
//!   in hand when they undock. It is only **spendable while docked**, and the
//!   design phase is docked at the spawn station — which is the whole reason
//!   everything in it is instant. Out between stations there is nothing to
//!   buy from, and a part comes out of the hold or does not get built.
//! - **Mass is conserved.** A part weighs its recipe and nothing else, so
//!   construction moves materials from the hold into the hull and
//!   deconstruction moves all of them back. [`materials`] is the contract and
//!   the two functions that keep it; the play phase has to build on the same
//!   rule or a ship will change weight by being rebuilt.
//!
//! # What is deliberately absent
//!
//! Oxygen and airtightness, construction labour, hauling, construction
//! sites, scrap, undo, and the final art. A part has a recipe, a price, a
//! footprint, somewhere to stand, a thrust or a turning force since the
//! flight step, and a power figure since [`power`] — and nothing else,
//! because every field that exists is a field something has to keep true.

pub mod budget;
pub mod design;
pub mod dock;
pub mod fixture;
pub mod mass;
pub mod materials;
pub mod parts;
pub mod power;
pub mod recipes;
pub mod validate;

pub use budget::Budget;
pub use design::{CARGO_SLOTS, Edit, EditError, Grid, PlacedPart, ShipDesign, apply, design_hash};
pub use dock::{Port, port};
// Money and what a station sells are the design phase's units, so they are
// re-exported here rather than leaving every caller to depend on `economy`
// for the sake of a type and two lookups.
pub use economy::{Money, Storage, starting_pool, storage, trade_price, trade_value};
pub use mass::{acceleration, hull_mass, ship_mass};
pub use materials::{bound_materials, build_from_cargo, deconstruct_to_cargo, recipe_for};
pub use parts::{
    BATTERY_CHARGE, Layer, PartDef, PartKind, REACTOR_OUTPUT, Rotation, TILE, essential,
    is_diagonal, part_mass, solid_corner,
};
pub use power::{
    Budget as PowerBudget, Network, budget as power_budget, is_powered, networks, unpowered,
};
pub use recipes::{RECIPES, Recipe, recipes_are_sound};
pub use validate::{
    ExposureMap, Issue, IssueCode, Severity, exhaust_blocked, exhaust_tiles, exposure, has_errors,
    validate,
};

#[cfg(test)]
mod tests;
