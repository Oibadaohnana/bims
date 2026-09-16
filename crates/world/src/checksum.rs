//! One number that says whether two copies of the world agree.
//!
//! Nothing uses it yet. It is here for the same reason `design_hash` was here
//! before there was a lobby: the moment there is a transport, a client and a
//! server will each be running [`crate::World::step`] and the only cheap way
//! to find out that they have drifted is to compare a number every so often.
//! Writing it now means the *loop* is designed to be comparable rather than
//! being made comparable later.
//!
//! # Why the floats are rounded
//!
//! FNV-1a over the raw bits of an `f64` would be exact, and exactness is
//! precisely the problem. Everything here that is not arithmetic is `sin`,
//! `cos`, `atan2` and `sqrt`; the first three come out of the platform's libm
//! on native and out of Rust's own on `wasm32-unknown-unknown`, and those two
//! are allowed to differ in the last bit. A checksum that noticed a
//! last-bit disagreement would fire constantly and say nothing.
//!
//! So every float is rounded onto a grid on the way in: positions to a
//! thousandth of a world unit, which is a hundred-thousandth of a tile, and
//! angles and speeds to a millionth. A real divergence — a ship that set off
//! and one that did not, a purchase one side made and the other did not — is
//! orders of magnitude larger than either grid. A rounding difference is
//! invisible. That is the trade, and it is the right way round.
//!
//! The one thing that is **not** rounded is the design: `shipdesign` is
//! integers throughout precisely so its hash is exact on both targets, and
//! that hash goes in whole.

use crate::world::{ShipState, World, node_key};

/// FNV-1a, written out by hand.
///
/// Not a `Hash` derive and not `DefaultHasher`, for the reason `design_hash`
/// gives at more length: those are explicitly allowed to differ between
/// builds, and this number crosses between machines.
struct Fnv(u64);

const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const PRIME: u64 = 0x0000_0100_0000_01b3;

impl Fnv {
    fn new() -> Fnv {
        Fnv(OFFSET)
    }

    fn eat(&mut self, value: u64) {
        for byte in value.to_le_bytes() {
            self.0 ^= byte as u64;
            self.0 = self.0.wrapping_mul(PRIME);
        }
    }

    /// A float, onto a grid first. See the module note.
    fn eat_rounded(&mut self, value: f64, scale: f64) {
        let quantised = if value.is_finite() {
            (value * scale).round()
        } else {
            // A NaN or an infinity is a divergence in itself, and it has to
            // hash to *something* rather than to whatever `as i64` does with
            // it on this target.
            f64::MAX
        };
        self.eat(quantised.to_bits());
    }
}

/// A thousandth of a world unit. A tile is 52 units, so this is well under a
/// millimetre in a game whose distances run to a hundred million.
const POSITION_GRID: f64 = 1_000.0;

/// A millionth of a radian, and of a unit per minute.
const FINE_GRID: f64 = 1_000_000.0;

/// Everything about the world that two clients have to agree on.
pub fn world_checksum(world: &World) -> u64 {
    let mut hash = Fnv::new();

    hash.eat(world.steps);
    hash.eat_rounded(world.clock_minutes, FINE_GRID);
    hash.eat(world.galaxy_seed);
    hash.eat(world.galaxy_type as u64);
    hash.eat(world.star_id as u64);
    hash.eat(world.money);

    let ship = &world.ship;
    hash.eat(world.design_hash());
    hash.eat(ship.crew_count as u64);
    hash.eat_rounded(ship.anchor.x, POSITION_GRID);
    hash.eat_rounded(ship.anchor.y, POSITION_GRID);
    hash.eat_rounded(ship.heading, FINE_GRID);
    hash.eat(ship.reserved_fuel as u64);
    hash.eat(ship.destination_set_by.map(u64::from).unwrap_or(u64::MAX));
    hash.eat(ship.frame.code() as u64);
    if let Some(node) = ship.frame.node() {
        let (kind, id) = node_key(&node);
        hash.eat(kind as u64);
        hash.eat(id as u64);
    }

    hash.eat(ship.state.code() as u64);
    match &ship.state {
        ShipState::Docked { station } => hash.eat(*station as u64),
        ShipState::Holding => {}
        ShipState::Travelling { plan, departed } => {
            // The plan is what the ship will *do*, so it goes in rather than
            // only where the ship has got to: two clients agreeing about a
            // position and disagreeing about the trip is the drift this is
            // for.
            hash.eat_rounded(*departed, FINE_GRID);
            hash.eat_rounded(plan.duration(), FINE_GRID);
            hash.eat_rounded(plan.distance, POSITION_GRID);
            hash.eat_rounded(plan.bearing, FINE_GRID);
            hash.eat_rounded(plan.fuel_required, FINE_GRID);
            hash.eat(plan.target.code() as u64);
            hash.eat(u64::from(plan.docks));
            hash.eat(u64::from(plan.aborting));
            hash.eat(plan.segments.len() as u64);
        }
    }

    // The crew: where each of them is, and the room's clock. Not the whole
    // of the room's state — that is a great deal of `f32` arithmetic that
    // two targets will disagree on in the last bit — but a Bim that went
    // somewhere different is a different world, and this is what says so.
    hash.eat(world.aboard.count() as u64);
    for who in 0..world.aboard.count() {
        let at = world.aboard.position(who);
        hash.eat_rounded(at.x, POSITION_GRID);
        hash.eat_rounded(at.y, POSITION_GRID);
    }
    hash.eat_rounded(world.aboard.minutes(), FINE_GRID);

    for node in &world.discovered {
        let (kind, id) = node_key(node);
        hash.eat(kind as u64);
        hash.eat(id as u64);
    }
    for request in &world.speed_requests {
        hash.eat(request.code() as u64);
    }

    hash.0
}
