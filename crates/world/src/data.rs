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
/// to this step — see the note on `MAX_STEPS_PER_FRAME` in `crates/app/src/screens/game.rs`,
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

/// How near the ship has to come to a station's hull for the people living
/// there to be simulated: fifty tiles. Nearer than the local frame by a long
/// way, because a room is a whole simulation and a station seen from the far
/// side of its frame is a shape, not a place. Measured from the ship's
/// position to the station's hull, not its centre, so a big station is not
/// further away than a small one at the same door. The way out is
/// [`LOCAL_HYSTERESIS`] further.
pub const RESIDENTS_RANGE: f64 = 50.0 * shipdesign::TILE as f64;

/// What a station's people keep in their cold store, a head: the targets
/// their own manager works to — the bay planted to keep the greens and the
/// soy up, stew cooked ahead for the shelf. Theirs, not the crew's: the
/// Management tab is the crew's own and reaches nobody ashore.
pub const RESIDENT_VEG_EACH: u32 = 100;
pub const RESIDENT_TOFU_EACH: u32 = 50;
pub const RESIDENT_STEW_EACH: u32 = 2;

/// How far out a station is drawn as a hull in the ship view rather than
/// left to the map. Twice the local frame: far enough that a station comes
/// into the picture as a speck and grows, rather than appearing.
pub const STATION_VISIBLE: f64 = 2.0 * LOCAL_RADIUS_STATION;

/// How long the ship takes to push off its berth once everybody is where
/// they belong, and how long it takes to come alongside once a trip has
/// ended, in game minutes. Both are read off the clock in closed form —
/// see `World::cast_off` and `World::come_alongside` — so a browser at 24x
/// and a server catching up put the ship in the same place.
pub const UNDOCK_MINUTES: f64 = 3.0;
pub const DOCK_MINUTES: f64 = 5.0;

/// How long the ship waits at the berth for the station's people to go
/// ashore and its own to come back aboard before it leaves without them.
/// Whoever is still on the wrong side of the airlock then is put where the
/// room puts a body with no floor under it — see `crew::Aboard::unjoined`.
pub const CASTING_OFF_LIMIT: f64 = 30.0;

/// How far a crew member may stand from the helm's seat and still be at the
/// helm: a tile, which is where a route to the seat can be relied on to
/// leave a body, and no further.
pub const HELM_REACH: f64 = shipdesign::TILE as f64;

/// How far inside a door the people going through it are sent, in tiles:
/// the corridor just inside a station's port, and the deck just inside the
/// ship's.
pub const ASHORE_TILES: f64 = 2.5;

/// How long one tile of rock takes to mine, in game minutes, with the Bim
/// standing beside it with a pick. A walk is as many of these as there are
/// marked rocks it can get to, on top of the suit and the airlock either
/// end and the walk out to each.
pub const MINE_TILE_MINUTES: f64 = 12.0;

/// What the suit lets through, as a multiplier on the open-air dose rate
/// in `crates/health`: a quarter, so a walk is about twenty-two minutes'
/// worth of dose, and the dose comes off at half a unit a minute inside.
pub const SUIT_INTENSITY: f64 = 0.25;

/// The dose above which a Bim is not sent out again: half the critical
/// line. Two walks back to back are fine; a third waits for the dose to
/// come off. This is what bounds a walk outside — there is no air gauge.
pub const EVA_DOSE_LIMIT: f64 = health::CRITICAL / 2.0;

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

/// How long putting a part together takes, in game minutes: this much
/// whatever it is, plus this much a unit of what it is made of. A wall of
/// two metal is six minutes beside it; a heavy engine, two hundred and
/// fifty units, a little over two hours. The carrying is on top, a load
/// at a time — see [`HAUL_LOAD`].
pub const BUILD_MINUTES_BASE: f64 = 5.0;
pub const BUILD_MINUTES_PER_UNIT: f64 = 0.5;

/// How many units of one material a Bim carries to a construction site in
/// one trip. A wall is one trip; the heavy engine's hundred and fifty
/// metal is eight.
pub const HAUL_LOAD: u32 = 20;
