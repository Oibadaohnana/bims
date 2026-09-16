//! The numbers the world runs on, kept apart from the loop that uses them.
//!
//! Placeholders, all of them, and the two that are not merely decorative are
//! the ranges: [`VISION_RANGE`] and [`RADAR_RANGE_PER_SENSOR`] decide how much
//! of a system a crew can see, which is the whole of what exploring is in this
//! step. They are set against the world generator's own scale — a one-day hop
//! for the reference ship is 518 400 units — so that eyesight reaches nowhere
//! at all and one sensor array reaches about three days out.

use economy::Money;
use worldgen::Node;

/// How long one step of the world is, in game minutes.
///
/// One sixtieth of a real second at 1x, which is the room's step exactly. The
/// two simulations are separate and always will be, but a player who has
/// learnt what 24x feels like in one should not have to learn it again in the
/// other.
pub const STEP_MINUTES: f64 = time::MINUTES_PER_SECOND / 60.0;

/// How fast the world will run. One constant, and raising it is not a change
/// to this step — see the note on `MAX_STEPS_PER_FRAME` in `web/ship.js`,
/// which has to move with it or the top of the range stops being reachable.
pub const TOP_SPEED: u32 = 24;

/// How far the crew can see with their own eyes.
///
/// Small on purpose. A one-day hop is over half a million units, so this
/// reaches roughly a tenth of the way to the nearest thing — which makes a
/// ship with no sensor array a ship that can only fly to what it is already
/// beside.
pub const VISION_RANGE: f64 = 50_000.0;

/// What each sensor array adds. Three days' travel for the reference ship,
/// give or take, so one of them turns a system from a fog into a map.
pub const RADAR_RANGE_PER_SENSOR: f64 = 1_500_000.0;

/// And what no number of them will reach past. Without a ceiling, a hull
/// covered in arrays would discover every system it entered on the first
/// frame, and there would be nothing left to fly out and look at.
pub const RADAR_RANGE_MAX: f64 = 6_000_000.0;

/// How near something has to be for the view to be *about* it rather than
/// about the space around it.
///
/// A body is a great deal bigger than a station even though the generator
/// stores both as points, so it gets the wider circle.
pub fn local_radius(node: Node) -> f64 {
    match node {
        Node::Station(_) => LOCAL_RADIUS_STATION,
        Node::Body(_) => LOCAL_RADIUS_BODY,
    }
}

pub const LOCAL_RADIUS_STATION: f64 = 20_000.0;
pub const LOCAL_RADIUS_BODY: f64 = 60_000.0;

/// How much further out than the entry radius the exit is. A quarter again.
pub const LOCAL_HYSTERESIS: f64 = 1.25;

/// The galaxy the **simulation** opens in, and the one a page with no lobby
/// behind it falls back to.
///
/// The game proper never uses it: the lobby's World tab picks a seed and a
/// galaxy type and hands both over. `nix run .#simulation` has no lobby and
/// wants the same world every time, so it starts here — with
/// [`crate::spawn`] picking the dock — unless the query says otherwise.
pub const DEFAULT_SEED: u64 = 0x_5749_4e44_4f57_0001;

/// What the simulation's crew have in hand when it opens. A placeholder, in
/// whole euros: enough to buy a hold of something at the first station and
/// not so much that money stops mattering. The game proper gets what the
/// design phase left of the pool instead.
pub const SIMULATION_MONEY: Money = 50_000;
