//! The world, and the one loop that advances it.
//!
//! # One clock
//!
//! There is a single [`World::step`] and it moves everything by exactly
//! [`data::STEP_MINUTES`]. The ship's flight, what the crew have seen, and —
//! when they arrive — the crew themselves, construction and health all happen
//! inside it, in a fixed order, at the same instant. Nothing in this crate may
//! grow a clock of its own or a loop of its own; a second one is two
//! simulations that will disagree, and the failure reads as a ship that is in
//! two places.
//!
//! The order inside a step is written out in [`World::step`]. The crew are
//! in it now — spawned at their bunks when the world opens, stepped in the
//! fifth stage — and the last two stages are **empty on purpose**: they are
//! where construction and health go, and they are there so that the shape of
//! the loop is decided while it is still small.
//!
//! # Where the ship is
//!
//! Two numbers and a rule. [`Ship::anchor`] is where design tile (0, 0) sits
//! in the system, [`Ship::heading`] is which way the ship is pointing, and the
//! ship's *position* — the thing a trip moves and the camera is centred on —
//! is the **centre of mass**, worked out from those two through
//! [`flight::angle::rotate_design`].
//!
//! That way round rather than the other because of what
//! [`World::on_ship_changed`] has to promise: welding a shelf to the stern
//! moves the centre of mass, and it must not move the *hull*. If the position
//! were stored and the anchor derived, every wall anybody built would shove
//! the whole ship sideways through space.
//!
//! # What changes a ship's mass
//!
//! Four things, and this is the list to check anything new against: trading
//! while docked, fuel burnt by the engines, food eaten or grown, and crew
//! joining or leaving. Construction and deconstruction move materials from the
//! hold into the hull and back — `shipdesign::materials` is that contract —
//! and change the centre of mass and the inertia without changing the total.

use economy::{Money, Storage, storage, trade_value};
use flight::{Dynamics, Phase, Plan, PlanError, Target, angle};
use physics::ResourceId;
use shipdesign::parts::{PartKind, Rotation};
use shipdesign::{CARGO_SLOTS, ShipDesign, design_hash};
use worldgen::math::DVec2;
use worldgen::{Galaxy, GalaxyType, Node, StarSystem};

use bims::sight::Stance;

use crate::build::{self, BuildSite, SiteRefusal};
use crate::crew::{Aboard, Residents};
use crate::data;
use crate::event::{Refusal, WorldEvent};
use crate::frame::{self, Frame};
use crate::mining::{self, MiningSite};
use crate::speed::{self, Speed};
use crate::station::{Berth, Station};

/// What a player can ask the world to do.
///
/// Every one of them carries the slot that sent it, because every one of them
/// can be sent by anybody: there is one ship and the crew fly it together.
/// Which of them a player is *allowed* to send is [`World::can_command`], and
/// today the answer is always yes.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Command {
    /// Fly there. While a trip is under way this is a redirect: the ship stops
    /// first and then sets off again.
    Confirm {
        slot: u32,
        target: Target,
    },
    /// Stop.
    Abort {
        slot: u32,
    },
    SetSpeed {
        slot: u32,
        speed: Speed,
    },
    Buy {
        slot: u32,
        resource: ResourceId,
        units: u32,
    },
    Sell {
        slot: u32,
        resource: ResourceId,
        units: u32,
    },
    /// Keep so many of `resource` made — see [`World::set_craft_target`].
    /// A command rather than a setting because the benches work to it and
    /// two players' ships have to agree about what the benches are doing.
    SetCraftTarget {
        slot: u32,
        resource: ResourceId,
        units: u32,
    },
    /// Mark a rock tile at the mining site to be mined, or unmark a marked
    /// one — `(x, y)` in the ship's design tiles, where the site's rocks
    /// are. A command because the crew work to the marks and every
    /// player's ship has to agree about which rocks are wanted.
    MarkRock {
        slot: u32,
        x: i32,
        y: i32,
    },
    /// Take every mark off.
    ClearMarks {
        slot: u32,
    },
    /// Lay out a part to be built: a construction site at `origin`, turned
    /// by `rotation`, for the crew to carry the materials to and put
    /// together — see [`crate::build`]. A command because the crew work to
    /// the sites and every player's ship has to agree about what is being
    /// built where. Refused while the ship is not at rest, and where the
    /// part would not go.
    PlaceSite {
        slot: u32,
        kind: PartKind,
        origin: (u32, u32),
        rotation: Rotation,
    },
    /// Take a site away again. Whatever was carried to it was only ever
    /// spoken for and is the hold's again.
    CancelSite {
        slot: u32,
        site: u32,
    },
}

/// What the ship is doing.
#[derive(Clone, PartialEq, Debug)]
pub enum ShipState {
    /// Alongside a station, where money works. The centre of mass sits on the
    /// station's own position.
    Docked { station: u32 },
    /// Stopped, somewhere. Not docked: a station with no airlock is somewhere
    /// to hold beside rather than somewhere to go aboard.
    Holding,
    /// Under way. The plan is the whole of the trip, and `departed` is the
    /// clock reading it began at — the two of them together are the ship's
    /// position at any moment, worked out rather than accumulated.
    Travelling { plan: Plan, departed: f64 },
    /// A trip confirmed at a station. The ship is still at its berth with
    /// the rooms joined, the station's people are going ashore and its own
    /// coming back aboard, and it casts off once they have — or once
    /// [`data::CASTING_OFF_LIMIT`] has gone by since `since`, whoever is
    /// still on the wrong side. `Ship::pending` is where it is going.
    CastingOff { station: u32, since: f64 },
    /// Pushing off the berth, straight out of the station's door: from
    /// `from`, `along` the way the door opens, for
    /// [`data::UNDOCK_MINUTES`] from `began`. The trip is planned once the
    /// ship is clear, from where it has got to, so the turn towards the
    /// target is the plan's own align phase.
    Undocking {
        station: u32,
        from: DVec2,
        along: DVec2,
        began: f64,
    },
    /// Coming alongside, over [`data::DOCK_MINUTES`] from `began`: from
    /// where the trip ended to `hold`, the point in front of the station's
    /// door the push-off ends at, turning onto the berth's heading on the
    /// way; then straight in along the door's line to the berth, and
    /// tied up at the end of it.
    Docking {
        station: u32,
        from: DVec2,
        from_heading: f64,
        hold: DVec2,
        began: f64,
    },
}

impl ShipState {
    /// The number that crosses the wasm boundary.
    pub fn code(&self) -> u32 {
        match self {
            ShipState::Docked { .. } => 0,
            ShipState::Holding => 1,
            ShipState::Travelling { .. } => 2,
            ShipState::CastingOff { .. } => 3,
            ShipState::Undocking { .. } => 4,
            ShipState::Docking { .. } => 5,
        }
    }

    /// The station the ship is tied up at with the rooms joined — docked,
    /// or casting off and not yet clear of the berth. What the painter
    /// draws the airlocks mated for.
    pub fn alongside(&self) -> Option<u32> {
        match self {
            ShipState::Docked { station } | ShipState::CastingOff { station, .. } => Some(*station),
            _ => None,
        }
    }

    /// The station the ship is at, tied up or on its way on or off the
    /// berth. What the local frame and the residents' room are about.
    pub fn station(&self) -> Option<u32> {
        match self {
            ShipState::Docked { station }
            | ShipState::CastingOff { station, .. }
            | ShipState::Undocking { station, .. }
            | ShipState::Docking { station, .. } => Some(*station),
            _ => None,
        }
    }
}

/// Nought to one, with no jolt at either end: how far along a push-off or
/// a docking the ship is at `t` of `over` minutes.
fn eased(t: f64, over: f64) -> f64 {
    let t = (t / over).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The ship: the design, where it is, and what it is doing.
#[derive(Clone, PartialEq, Debug)]
pub struct Ship {
    /// **The live ship.** The play phase changes this one; there is no second
    /// copy anywhere and no editor format that gets converted into it.
    pub design: ShipDesign,
    pub crew_count: u32,
    /// Worked out from the design, and only ever by
    /// [`World::on_ship_changed`].
    pub dynamics: Dynamics,
    /// Where design tile (0, 0) is, in system coordinates. See the module
    /// note: this is stored and the position is derived, not the other way
    /// about.
    pub anchor: DVec2,
    pub heading: f64,
    pub state: ShipState,
    /// Fuel spoken for by the trip under way. It cannot be sold, and it does
    /// not count towards what another trip could be planned with.
    pub reserved_fuel: u32,
    /// Who set the destination. The route line on the map is drawn in their
    /// colour, which is the whole of what it is for.
    pub destination_set_by: Option<u32>,
    pub frame: Frame,
    /// Where a redirect is going once the ship has stopped, or where a
    /// departure is going once the ship is clear of its berth. Only ever
    /// the **latest** confirmed target: two Confirms in one step leave one.
    pub pending: Option<(u32, Target)>,
    /// What is in the batteries, in power units — never more than what
    /// the wired batteries hold. Full when the world opens, and moved by
    /// [`World::run_power`] alone; see [`Power`].
    pub charge: f64,
}

impl Ship {
    /// Which centre of mass the hull is hung from at this instant.
    ///
    /// A trip keeps the one it was planned with, so a design change halfway
    /// does not move a ship that is already flying — see
    /// [`World::on_ship_changed`].
    fn live_centre(&self) -> DVec2 {
        match &self.state {
            ShipState::Travelling { plan, .. } => plan.dynamics.centre_of_mass,
            _ => self.dynamics.centre_of_mass,
        }
    }

    /// Where the ship is: its centre of mass, in system coordinates.
    pub fn position(&self) -> DVec2 {
        self.anchor
            .add(angle::rotate_design(self.live_centre(), self.heading))
    }

    /// Put the centre of mass there, by moving the anchor under it.
    fn set_position(&mut self, at: DVec2) {
        self.anchor = at.sub(angle::rotate_design(self.live_centre(), self.heading));
    }

    pub fn fuel_aboard(&self) -> u32 {
        self.design.carrying(ResourceId::Fuel)
    }

    /// Fuel nobody has spoken for. What a new trip may be planned against.
    pub fn unreserved_fuel(&self) -> u32 {
        self.fuel_aboard().saturating_sub(self.reserved_fuel)
    }
}

/// Why a world could not be started.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StartError {
    /// The star or the station the game was told to start at is not in this
    /// galaxy. The lobby names both, and a world does **not** fall back to
    /// some other dock when they are wrong: two players handed different
    /// spawns would be two players in two places, and a wrong one is a bug
    /// to show rather than to paper over.
    NoSuchStation,
    /// The accepted design does not describe a ship that can exist.
    NotAShip(flight::DynamicsError),
}

/// One star system, the ship in it, and the clock they both run on.
///
/// Not `Clone` and not `PartialEq`: the room aboard is neither, and a
/// world is compared by its [`World::checksum`] — which is the comparison
/// two machines will make, and the one a test should make too.
#[derive(Debug)]
pub struct World {
    pub clock_minutes: f64,
    /// How many steps have been taken. The stamp a command carries, and the
    /// one number that is an integer rather than a float — a count of steps
    /// is the only honest way to say "this command applies *there*".
    pub steps: u64,
    pub galaxy_seed: u64,
    pub galaxy_type: GalaxyType,
    pub star_id: u32,
    pub system: StarSystem,
    /// What is left of the pool the ship was designed against. It buys
    /// nothing away from a station.
    pub money: Money,
    pub ship: Ship,
    /// The people aboard, and the room they live in: the room's whole
    /// simulation, laid out on this ship, one Bim per player at their own
    /// bunk from the moment the world opened. See [`crate::crew`].
    pub aboard: Aboard,
    /// Every station of the system as a place: a hull on a grid at a
    /// position, with a door the ship docks by. Built once, in id order.
    /// See [`crate::station`].
    pub stations: Vec<Station>,
    /// The station the ship is near, if it is near one, as a room: the
    /// room's whole simulation again, laid out on that station with the
    /// people who live there in it, opened when the ship comes within
    /// [`data::RESIDENTS_RANGE`] of the hull and closed when it leaves. One
    /// at a time — the nearest. A derelict's room has nobody in it. See
    /// [`crate::crew`].
    pub residents: Option<Residents>,
    /// The ship's power over its live networks, worked out from the parts
    /// once per change to them — `on_ship_changed` — rather than once a
    /// step: it is a union-find over every tile of the grid, and the
    /// parts change when something is built and not otherwise.
    power_budget: shipdesign::PowerBudget,
    /// What the crew have seen, shared between all of them and never
    /// forgotten. Sorted, so a checksum over it means something.
    pub discovered: Vec<Node>,
    /// How many of each resource the crew are to keep made, by
    /// `ResourceId` — the player's standing instruction to the benches,
    /// the way the manager's stew target is to the galley. A recipe is on
    /// offer while the hold has fewer of its output than this. Nought until
    /// somebody asks: a workshop that started the game smelting the ore
    /// down would move every probe that pins the first morning.
    pub craft_targets: [u32; CARGO_SLOTS],
    /// Each crew member's body, by slot — `crates/health`: the dose and
    /// what it has done. Stage 8 of the step. Only the suit doses anybody
    /// yet: a walk outside is exposure at [`data::SUIT_INTENSITY`], and
    /// under cover the dose comes off. The room's own hunger and sleep are
    /// not in here; see the crate's note.
    pub health: Vec<health::HealthState>,
    /// One per player, in slot order. The world runs at the slowest of them.
    pub speed_requests: Vec<Speed>,
    /// The asteroids about every belt the ship has held station at, one
    /// site a belt in belt order, laid out the first time the ship came
    /// to rest there and kept — a mined tile stays mined. See
    /// [`crate::mining`].
    pub sites: Vec<MiningSite>,
    /// Bumped every time a site's rocks change, so the room rebuilds its
    /// outside grid then and only then.
    pub site_version: u64,
    /// What the walk under way has mined so far — rock, ore, galvum — to
    /// be said all at once when the Bim comes back in.
    walk_tally: (u32, u32, u32),
    /// The parts laid out to be built and not built yet, in the order they
    /// were laid out — which is the order the crew take them in. See
    /// [`crate::build`]. In `world_checksum` whole.
    pub builds: Vec<BuildSite>,
    /// The next site's id. Only ever climbs, like a part's.
    pub next_site: u32,
    /// The station the crew set out from: the one place they are at home,
    /// and friendly unless it is on the list below.
    pub home: u32,
    /// The stations whose people are enemies, sorted by id. Nothing puts a
    /// station here but [`World::set_hostile`] — the `combat` command, so
    /// far — and everywhere else is neutral. See [`World::stance`].
    pub hostile: Vec<u32>,
}

impl World {
    /// Open a world with the accepted ship docked at a station.
    ///
    /// `star_id` and `station_id` are the spawn the lobby chose — see
    /// [`StartError::NoSuchStation`] for why a wrong one is an error and not
    /// a fallback. [`spawn`] picks one for the simulation, which has no lobby.
    ///
    /// `money` is what was left of the design phase's pool. It is not
    /// converted into anything and it is not spent at Accept: it is what the
    /// crew have in hand, and it is spendable only while docked.
    pub fn start(
        design: ShipDesign,
        money: Money,
        players: u32,
        seed: u64,
        galaxy_type: GalaxyType,
        star_id: u32,
        station_id: u32,
    ) -> Result<World, StartError> {
        let players = players.max(1);
        let galaxy = Galaxy::new(seed, galaxy_type);
        let system = galaxy.system(star_id).ok_or(StartError::NoSuchStation)?;
        let stations = Station::all_of(&system);
        if !stations.iter().any(|s| s.id == station_id) {
            return Err(StartError::NoSuchStation);
        }

        let dynamics = flight::dynamics(&design, players).map_err(StartError::NotAShip)?;
        let aboard = Aboard::new(&design, players, seed);
        let design_for_charge = design.clone();
        let ship = Ship {
            design,
            crew_count: players,
            dynamics,
            anchor: DVec2::ZERO,
            heading: 0.0,
            state: ShipState::Docked {
                station: station_id,
            },
            reserved_fuel: 0,
            destination_set_by: None,
            frame: Frame::Local(Node::Station(station_id)),
            pending: None,
            // Full: the ship has been sitting at a station's dock.
            charge: shipdesign::power_budget(&design_for_charge).storage,
        };

        let mut world = World {
            clock_minutes: 0.0,
            steps: 0,
            galaxy_seed: seed,
            galaxy_type,
            star_id,
            system,
            money,
            ship,
            aboard,
            stations,
            residents: None,
            power_budget: shipdesign::power_budget(&design_for_charge),
            discovered: Vec::new(),
            craft_targets: [0; CARGO_SLOTS],
            health: vec![health::HealthState::new(); players as usize],
            // Everybody starts at real time. Anything else would have the
            // world already moving before the first player had looked at it.
            speed_requests: vec![Speed::Real; players as usize],
            sites: Vec::new(),
            site_version: 0,
            walk_tally: (0, 0, 0),
            builds: Vec::new(),
            next_site: 1,
            home: station_id,
            hostile: Vec::new(),
        };

        // The whole system, charted. The crew picked this dock off the
        // lobby's chart of this very system — every planet and every station
        // on it — and a map that then hid what they had just been looking at
        // would be a map with nothing on it to fly to. Discovery is for what
        // the chart does not show: the systems beyond this one, when there
        // is a way there, and anything a sweep turns up on the way.
        world.discovered = world.system.nodes();
        world.discovered.sort_by_key(node_key);
        // Alongside from the first step: the ship at the station's door, and
        // whoever lives there already up and about.
        world.dock_at(station_id);
        world.settle_residents();
        Ok(world)
    }

    /// Forget the chart: only the dock is known, plus whatever the sensors
    /// reach from it. For probes of discovery, which otherwise have nothing
    /// left to discover in a system that opens charted.
    pub fn uncharted_for_probe(&mut self) {
        self.discovered.clear();
        if let ShipState::Docked { station } = self.ship.state {
            self.discovered.push(Node::Station(station));
        }
        let here = self.ship.position();
        self.discover_along(here, here, &mut Vec::new());
    }

    // --- the step ---------------------------------------------------------

    /// Advance the world by exactly one [`data::STEP_MINUTES`].
    ///
    /// **The order below is the contract.** Later steps add their systems at
    /// the numbered places and nowhere else; what they must not do is
    /// introduce a second clock or a second loop, because then there are two
    /// answers to "what time is it aboard".
    ///
    /// Commands go first so that a command stamped for this step lands before
    /// anything moves — a Confirm and the departure it causes are the same
    /// instant, not one step apart.
    pub fn step(&mut self, commands: &[Command]) -> Vec<WorldEvent> {
        let mut events = Vec::new();

        // 1. What the players asked for, in the order it arrived.
        for &command in commands {
            self.apply(command, &mut events);
        }

        // 2. The clock. Everything below reads it; nothing below sets it.
        self.clock_minutes += data::STEP_MINUTES;
        self.steps += 1;

        // 3. Flight — and the two moves either side of a trip that are not
        //    flown: pushing off the berth and coming alongside.
        let was = self.ship.position();
        self.fly(&mut events);
        self.cast_off(&mut events);
        self.come_alongside(&mut events);
        let now = self.ship.position();

        // 4. What that brought into range.
        self.discover_along(was, now, &mut events);
        self.settle_frame(&mut events);
        self.settle_residents();
        self.settle_site();

        // 5. Crew: the room's own update, aboard, on this clock. Bims live
        //    in ship-design coordinates and the ship's position, rotation and
        //    acceleration do not reach them. See `crates/world/src/crew.rs`.
        //    The station's residents, when the ship is near one, are the
        //    same room again on the same clock.
        //
        //    Away from a berth the ship wants somebody at the helm, and the
        //    room offers that as a job. Not while casting off: the crew are
        //    being walked home then, and the helm can wait until the ship is
        //    clear. Docked, the post the job set is lifted.
        let wants_helm = !matches!(
            self.ship.state,
            ShipState::Docked { .. } | ShipState::CastingOff { .. }
        );
        let seat = if wants_helm { self.helm_spot() } else { None };
        self.aboard.set_helm(seat);
        //    And what the benches are wanted for, worked out fresh from the
        //    hold, the targets and the power; then, after the step, what
        //    they finished, moved through the hold.
        let orders = self.craft_orders();
        self.aboard.room.set_craft_orders(orders);
        let eva = self.eva_offer();
        self.aboard.room.set_eva(eva);
        //    And the construction sites, what each still wants, and who may
        //    go out to one beyond the hull. What the room did about them is
        //    read in stage 7.
        let builds = self.build_orders();
        let suit_ok = self.suit_ok();
        self.aboard.room.set_build_orders(builds, suit_ok);
        self.aboard.step();
        if let Some(residents) = &mut self.residents {
            residents.aboard.step();
        }
        self.visit(&mut events);
        for recipe in self.aboard.room.take_crafted() {
            self.finish_craft(recipe, &mut events);
        }
        for at in self.aboard.room.take_mined() {
            self.finish_tile(at);
        }
        for _ in 0..self.aboard.room.take_walks() {
            self.finish_walk(&mut events);
        }

        // 6. Power: what the reactors made this step against what the
        //    wired consumers drew, into or out of the batteries. What is
        //    running in a brownout is `World::powered`, read by whatever
        //    draws — nothing aboard reads it yet.
        self.run_power();

        // 7. Construction: what the crew did at the sites this step — a load
        //    taken off a shelf, a load put down, a load given up, a part put
        //    together — moved through the hold. Building goes through
        //    `shipdesign::materials`, out of what is aboard, and asks
        //    `World::can_modify_part` first. See `crate::build`.
        for (site, resource, units) in self.aboard.room.take_picked() {
            self.finish_pick(site, resource, units);
        }
        for site in self.aboard.room.take_dropped() {
            self.finish_drop(site);
        }
        for site in self.aboard.room.take_returned() {
            self.finish_return(site);
        }
        for site in self.aboard.room.take_built() {
            self.finish_build(site, &mut events);
        }

        // 8. Health and radiation: each crew member's body, dosed while it
        //    is outside in a suit and sheltered otherwise. The design's
        //    exposure map — `shipdesign::exposure` — is the other input
        //    and is not wired yet.
        self.run_health(&mut events);

        events
    }

    /// Everything a command can do. Split out from [`World::step`] so the
    /// order of the step reads as an order rather than as a wall of matches.
    fn apply(&mut self, command: Command, events: &mut Vec<WorldEvent>) {
        let slot = match command {
            Command::Confirm { slot, .. }
            | Command::Abort { slot }
            | Command::SetSpeed { slot, .. }
            | Command::Buy { slot, .. }
            | Command::Sell { slot, .. }
            | Command::SetCraftTarget { slot, .. }
            | Command::MarkRock { slot, .. }
            | Command::ClearMarks { slot }
            | Command::PlaceSite { slot, .. }
            | Command::CancelSite { slot, .. } => slot,
        };

        match command {
            // Speed is not an order to the ship, so it does not go through the
            // helm: a player who is nowhere near the controls may still say
            // they want to watch this bit slowly.
            Command::SetSpeed { speed, .. } => self.request_speed(slot, speed),
            Command::Confirm { target, .. } => {
                if !self.can_command(slot) {
                    events.push(refused(slot, Refusal::NotAtTheHelm));
                    return;
                }
                self.confirm(slot, target, events);
            }
            Command::Abort { .. } => {
                if !self.can_command(slot) {
                    events.push(refused(slot, Refusal::NotAtTheHelm));
                    return;
                }
                self.give_up(slot, events);
            }
            Command::Buy {
                resource, units, ..
            } => self.buy(slot, resource, units, events),
            Command::Sell {
                resource, units, ..
            } => self.sell(slot, resource, units, events),
            Command::SetCraftTarget {
                resource, units, ..
            } => self.set_craft_target(resource, units),
            Command::MarkRock { x, y, .. } => self.mark_rock(x, y),
            Command::ClearMarks { .. } => self.clear_marks(),
            Command::PlaceSite {
                kind,
                origin,
                rotation,
                ..
            } => self.place_site(slot, kind, origin, rotation, events),
            Command::CancelSite { site, .. } => self.cancel_site(slot, site, events),
        }
    }

    // --- flying ------------------------------------------------------------

    /// Where the trip has got to, and what to do when it is over.
    fn fly(&mut self, events: &mut Vec<WorldEvent>) {
        let ShipState::Travelling { plan, departed } = &self.ship.state else {
            return;
        };
        let (plan, departed) = (plan.clone(), *departed);
        let elapsed = self.clock_minutes - departed;
        let total = plan.duration();
        let state = flight::state_at(&plan, elapsed.min(total));

        // Heading first: the position is worked out through it.
        self.ship.heading = state.heading;
        self.ship.set_position(state.position);
        if elapsed < total {
            return;
        }
        self.finish(&plan, events);
    }

    /// A plan has run out. Take the fuel, release what is left of the
    /// reservation, put the ship down, and pick up a redirect if one is
    /// waiting.
    fn finish(&mut self, plan: &Plan, events: &mut Vec<WorldEvent>) {
        // The fuel actually burnt, which for an abort includes what the trip
        // it replaced had already got through.
        let burnt = units_of(plan.fuel_required)
            .min(self.ship.reserved_fuel)
            .min(self.ship.fuel_aboard());
        self.ship.design.cargo[ResourceId::Fuel as usize] -= burnt;
        self.ship.reserved_fuel = 0;

        let station = plan
            .docks
            .then(|| match plan.target {
                Target::Station(id) => Some(id),
                _ => None,
            })
            .flatten();

        // The approach ends a station radius short of the berth, and going
        // alongside closes the gap — a slide and a turn over
        // `DOCK_MINUTES`, ending airlock to airlock. A station with no door
        // to dock by is simply where the ship is put.
        let hold = station.and_then(|id| self.hold_point(id));
        self.ship.state = match (station, hold) {
            (Some(id), Some(hold)) => ShipState::Docking {
                station: id,
                from: self.ship.position(),
                from_heading: self.ship.heading,
                hold,
                began: self.clock_minutes,
            },
            (Some(id), None) => ShipState::Docked { station: id },
            (None, _) => ShipState::Holding,
        };
        if let ShipState::Docked { station: id } = self.ship.state {
            self.dock_at(id);
        }
        if plan.aborting {
            self.ship.destination_set_by = None;
        }
        self.on_ship_changed();

        if plan.aborting {
            // A redirect: the stop was only ever the first half of it.
            if let Some((slot, target)) = self.ship.pending.take() {
                self.set_off(slot, target, events);
                return;
            }
        } else if let ShipState::Docking { station, .. } = self.ship.state {
            events.push(WorldEvent::Docking { station });
        } else {
            events.push(WorldEvent::Arrived { station });
        }
    }

    /// The two halves of leaving a station. Waiting at the berth for
    /// everybody to be on their own side of the airlock — the station's
    /// people are sent ashore and the crew called back every step, and
    /// nobody is teleported — and then the push-off, read off the clock.
    /// The trip is planned from wherever the push-off ends; a plan that
    /// will not go leaves the ship holding there, as it would anywhere.
    fn cast_off(&mut self, events: &mut Vec<WorldEvent>) {
        match self.ship.state.clone() {
            ShipState::CastingOff { station, since } => {
                self.aboard.send_everybody_home(&self.ship.design);
                let overdue = self.clock_minutes - since >= data::CASTING_OFF_LIMIT;
                if !self.aboard.everybody_home(&self.ship.design) && !overdue {
                    return;
                }
                let from = self.ship.position();
                let along = self.way_out(station).unwrap_or(DVec2 { x: 0.0, y: 1.0 });
                self.unjoin_rooms();
                self.ship.state = ShipState::Undocking {
                    station,
                    from,
                    along,
                    began: self.clock_minutes,
                };
                let slot = self.ship.pending.map(|(slot, _)| slot).unwrap_or(0);
                events.push(WorldEvent::Undocking { slot });
            }
            ShipState::Undocking {
                from, along, began, ..
            } => {
                let t = self.clock_minutes - began;
                let reach = self.undock_distance() * eased(t, data::UNDOCK_MINUTES);
                self.ship.set_position(from.add(along.scale(reach)));
                if t < data::UNDOCK_MINUTES {
                    return;
                }
                self.ship.state = ShipState::Holding;
                match self.ship.pending.take() {
                    Some((slot, target)) => self.set_off(slot, target, events),
                    None => self.ship.destination_set_by = None,
                }
            }
            _ => {}
        }
    }

    /// How far the ship pushes off before it turns: its own span, so a
    /// turn in place clears the station's hull whichever way it goes.
    fn undock_distance(&self) -> f64 {
        self.ship.design.build_area as f64 * shipdesign::TILE as f64
    }

    /// The last stretch of a trip to a station, read off the clock, in two
    /// halves of [`data::DOCK_MINUTES`]: to the hold point in front of the
    /// door, turning onto the berth's heading on the way, and then straight
    /// in along the door's line — the push-off backwards. Tied up at the
    /// end: `dock_at` sets the ship down exactly and joins the rooms.
    fn come_alongside(&mut self, events: &mut Vec<WorldEvent>) {
        let ShipState::Docking {
            station,
            from,
            from_heading,
            hold,
            began,
        } = self.ship.state.clone()
        else {
            return;
        };
        let t = self.clock_minutes - began;
        let half = data::DOCK_MINUTES / 2.0;
        if let Some(berth) = self.berth_at(station) {
            if t < half {
                let f = eased(t, half);
                let turn = angle::shortest(from_heading, berth.heading);
                self.ship.heading = angle::wrap(from_heading + turn * f);
                self.ship.set_position(from.add(hold.sub(from).scale(f)));
            } else {
                let f = eased(t - half, half);
                self.ship.heading = berth.heading;
                self.ship
                    .set_position(hold.add(berth.position.sub(hold).scale(f)));
            }
        }
        if t < data::DOCK_MINUTES {
            return;
        }
        self.ship.state = ShipState::Docked { station };
        self.dock_at(station);
        events.push(WorldEvent::Arrived {
            station: Some(station),
        });
    }

    /// A Confirm. From rest it is a departure; under way it is a redirect,
    /// which is a stop followed by a departure.
    fn confirm(&mut self, slot: u32, target: Target, events: &mut Vec<WorldEvent>) {
        if let Some(node) = target.node()
            && !self.discovered.contains(&node)
        {
            events.push(WorldEvent::PlanFailed {
                slot,
                error: PlanError::TargetUndiscovered,
            });
            return;
        }
        // The ship does not move while it is built on — see `crate::build`.
        // Asked of a ship at rest only: one already under way is being
        // redirected, and nothing is built on it in the meantime anyway.
        if self.at_rest() && self.under_construction() {
            events.push(refused(slot, Refusal::UnderConstruction));
            return;
        }

        match self.ship.state.clone() {
            ShipState::Travelling { plan, departed } => {
                self.ship.pending = Some((slot, target));
                self.ship.destination_set_by = Some(slot);
                if plan.aborting {
                    // Already stopping. Only the latest target is kept, and it
                    // has just been kept; there is nothing else to do.
                    return;
                }
                self.begin_abort(&plan, self.clock_minutes - departed);
                events.push(WorldEvent::Aborted { slot });
            }
            ShipState::Docked { station } => {
                // Refused now rather than after everybody has been sent
                // ashore for nothing: what would not fly from the berth
                // will not fly from a ship's length outside it either.
                if let Err(error) = self.plan_from_here(target) {
                    events.push(WorldEvent::PlanFailed { slot, error });
                    return;
                }
                self.ship.pending = Some((slot, target));
                self.ship.destination_set_by = Some(slot);
                self.ship.state = ShipState::CastingOff {
                    station,
                    since: self.clock_minutes,
                };
                events.push(WorldEvent::CastingOff { slot });
            }
            // Still leaving: only where to is changed.
            ShipState::CastingOff { .. } | ShipState::Undocking { .. } => {
                self.ship.pending = Some((slot, target));
                self.ship.destination_set_by = Some(slot);
            }
            ShipState::Docking { .. } => {
                events.push(refused(slot, Refusal::ComingAlongside));
            }
            ShipState::Holding => self.set_off(slot, target, events),
        }
    }

    /// An Abort. Unlike a redirect it leaves nothing pending. At the berth
    /// it is the departure called off — the ship stays tied up; pushing
    /// off, the push-off finishes and the ship holds there.
    fn give_up(&mut self, slot: u32, events: &mut Vec<WorldEvent>) {
        let under_way = match &self.ship.state {
            ShipState::Travelling { plan, departed } => Some((plan.clone(), *departed)),
            ShipState::CastingOff { station, .. } => {
                self.ship.state = ShipState::Docked { station: *station };
                self.ship.pending = None;
                self.ship.destination_set_by = None;
                events.push(WorldEvent::Aborted { slot });
                return;
            }
            ShipState::Undocking { .. } => {
                self.ship.pending = None;
                self.ship.destination_set_by = None;
                events.push(WorldEvent::Aborted { slot });
                return;
            }
            _ => None,
        };
        let Some((plan, departed)) = under_way else {
            events.push(refused(slot, Refusal::NotTravelling));
            return;
        };
        self.ship.pending = None;
        if plan.aborting {
            return;
        }
        self.begin_abort(&plan, self.clock_minutes - departed);
        events.push(WorldEvent::Aborted { slot });
    }

    fn begin_abort(&mut self, plan: &Plan, elapsed: f64) {
        let stop = flight::abort(plan, elapsed);
        // The reservation becomes the stop's bill, which covers what has
        // already been burnt as well as the braking still to come.
        self.ship.reserved_fuel = units_of(stop.fuel_required).min(self.ship.fuel_aboard());
        self.ship.state = ShipState::Travelling {
            plan: stop,
            departed: self.clock_minutes,
        };
    }

    /// Plan a trip from where the ship is standing, and go.
    fn set_off(&mut self, slot: u32, target: Target, events: &mut Vec<WorldEvent>) {
        match self.plan_from_here(target) {
            Ok(plan) => {
                self.ship.reserved_fuel = units_of(plan.fuel_required);
                self.ship.destination_set_by = Some(slot);
                self.ship.pending = None;
                self.ship.state = ShipState::Travelling {
                    plan,
                    departed: self.clock_minutes,
                };
                self.unjoin_rooms();
                self.leave_site(events);
                events.push(WorldEvent::Departed { slot });
            }
            Err(error) => {
                self.ship.pending = None;
                self.ship.destination_set_by = None;
                events.push(WorldEvent::PlanFailed { slot, error });
            }
        }
    }

    fn plan_from_here(&self, target: Target) -> Result<Plan, PlanError> {
        if self.ship.fuel_aboard() == 0 {
            return Err(PlanError::NoFuelAboard);
        }
        let at = self
            .target_position(target)
            .ok_or(PlanError::TargetUndiscovered)?;
        flight::plan_trip(
            &self.ship.dynamics,
            self.ship.position(),
            self.ship.heading,
            target,
            at,
            self.ship.unreserved_fuel() as f64,
        )
    }

    /// Where a target is in the system.
    pub fn target_position(&self, target: Target) -> Option<DVec2> {
        match target {
            Target::Point(at) => Some(at),
            // A station is flown to by its door, not by its middle: the
            // trip is aimed at the hold point in front of it and ends a
            // station radius short of that, and coming alongside covers
            // the rest — so the ship never comes to rest inside a station.
            Target::Station(id) => self
                .hold_point(id)
                .or_else(|| self.system.absolute_position(Node::Station(id))),
            other => self.system.absolute_position(other.node()?),
        }
    }

    // --- stations ------------------------------------------------------------

    pub fn station(&self, id: u32) -> Option<&Station> {
        self.stations.iter().find(|s| s.id == id)
    }

    /// Where this ship docks at that station: airlock to airlock, outside
    /// its hull. `None` for a station that is not there or has no door.
    pub fn berth_at(&self, id: u32) -> Option<Berth> {
        self.station(id)?
            .berth(&self.ship.design, self.ship.dynamics.centre_of_mass)
    }

    /// The point in front of that station's door where a push-off ends and
    /// a docking begins its last straight run: the berth, a ship's span out
    /// along the way the door opens. `None` where there is no berth.
    pub fn hold_point(&self, id: u32) -> Option<DVec2> {
        let berth = self.berth_at(id)?;
        let out = self.way_out(id)?;
        Some(berth.position.add(out.scale(self.undock_distance())))
    }

    /// The way out of that station's door: the way the door opens, or, for
    /// a station with no door, straight away from its middle.
    fn way_out(&self, id: u32) -> Option<DVec2> {
        let station = self.station(id)?;
        if let Some((_, out)) = station.face() {
            return Some(out);
        }
        let away = self.ship.position().sub(station.centre());
        let len = away.length();
        (len > 0.0).then(|| away.scale(1.0 / len))
    }

    /// Put the ship at its berth: the one place, with [`World::start`], that
    /// it is set down rather than flown. Heading first, because the position
    /// is worked out through it.
    fn dock_at(&mut self, id: u32) {
        let Some(berth) = self.berth_at(id) else {
            if let Some(at) = self.system.absolute_position(Node::Station(id)) {
                self.ship.set_position(at);
            }
            return;
        };
        self.ship.heading = berth.heading;
        self.ship.set_position(berth.position);
        self.join_rooms(id, berth);
    }

    /// Docked: the ship's room and the station's become one deck, so the
    /// crew can walk through the airlocks. See [`crate::docking`]. The
    /// station's people are **not** in it: they keep their own room, laid
    /// out on the station's design with the station's galley, heads, bunks
    /// and benches for fixtures, and go on living there on their own
    /// timetable, under their own manager. The joined room is the ship's
    /// fixtures and the ship's crew, with the station's deck to walk on and
    /// the station's fixtures as furniture to walk round.
    fn join_rooms(&mut self, id: u32, berth: Berth) {
        let Some(station) = self.station(id) else {
            return;
        };
        let Some(joined) = crate::docking::join(
            &self.ship.design,
            self.ship.dynamics.centre_of_mass,
            station,
            &berth,
        ) else {
            return;
        };
        let (design, count, seed) = (
            station.design.clone(),
            station.residents(),
            station.map_seed,
        );
        // The residents' room: the one already open, or opened now if the
        // ship arrived faster than the room did.
        let residents = match self.residents.take() {
            Some(residents) if residents.station == id => residents,
            _ => Residents::open(id, &design, count, seed, self.clock_minutes),
        };
        self.drop_loads();
        let ship_seed = self.galaxy_seed ^ self.steps;
        let crew = std::mem::replace(&mut self.aboard, Aboard::new(&self.ship.design, 1, 0))
            .room
            .take_crew();
        self.aboard = Aboard::joined(
            joined,
            &self.ship.design,
            &design,
            crew,
            ship_seed,
            self.clock_minutes,
        );
        let mut residents = residents;
        // Its doors are the joined room's to draw — one picture of each,
        // in one state — and the joined room is told where the residents
        // are every step (`visit`) so its doors open for them too.
        residents.aboard.room.set_doors_drawn(false);
        // And its fog is the joined room's, over both decks: this room
        // draws none, and its people only where the crew can see them.
        residents.aboard.room.set_fog(bims::sight::Fog::None);
        self.residents = Some(residents);
        self.apply_stances();
    }

    /// Whose a station is, to the crew: home is friendly, a station on the
    /// hostile list is hostile, and everywhere else is neutral.
    pub fn stance(&self, station: u32) -> Stance {
        if self.hostile.binary_search(&station).is_ok() {
            Stance::Hostile
        } else if station == self.home {
            Stance::Friendly
        } else {
            Stance::Neutral
        }
    }

    /// Make a station's people enemies, or not. The rooms that are open
    /// on it are told at once.
    pub fn set_hostile(&mut self, station: u32, hostile: bool) {
        match (self.hostile.binary_search(&station), hostile) {
            (Err(i), true) => self.hostile.insert(i, station),
            (Ok(i), false) => {
                self.hostile.remove(i);
            }
            _ => {}
        }
        self.apply_stances();
    }

    /// Tell every room open on a station whose it is: the residents' room
    /// its own stance, for its fog and the ring under each of its people,
    /// and the joined deck which of its tiles are the station's. Asked
    /// whenever a room opens or a stance changes.
    fn apply_stances(&mut self) {
        let docked = self.ship.state.station();
        let ashore = self.residents.as_ref().map(|r| self.stance(r.station));
        if let (Some(residents), Some(stance)) = (&mut self.residents, ashore) {
            residents.aboard.room.set_stance(stance);
            residents
                .aboard
                .room
                .set_hostile_bodies(stance == Stance::Hostile);
        }
        let foreign = match (self.aboard.station_box, docked) {
            (Some((lo, hi)), Some(station)) if self.aboard.is_joined() => Some((
                bims::math::Rect::from_corners(
                    bims::math::vec2(lo.x as f32, lo.y as f32),
                    bims::math::vec2(hi.x as f32, hi.y as f32),
                ),
                self.stance(station),
            )),
            _ => None,
        };
        match foreign {
            Some((rect, stance)) => self.aboard.room.set_foreign(Some(rect), stance),
            None => self.aboard.room.set_foreign(None, Stance::Neutral),
        }
    }

    /// Docked, the residents walk about in their own room and the joined
    /// room draws the doors: it is told where they are, in its own units,
    /// so a door opens for a resident walking through it the way it does
    /// for the crew. Once a step, after both rooms have moved.
    fn visit(&mut self, events: &mut Vec<WorldEvent>) {
        if !self.aboard.is_joined() {
            return;
        }
        let visitors: Vec<DVec2> = match &self.residents {
            Some(residents) => (0..residents.aboard.count())
                .map(|who| residents.aboard.position(who))
                .collect(),
            None => Vec::new(),
        };
        self.aboard.visit(&visitors);
        // And which of them the crew can see, for their own room to draw.
        // Off the last trace, which is a frame's rather than a step's —
        // nobody crosses a bulkhead in a sixtieth of a minute.
        let seen = self.aboard.seen(&visitors);
        if let Some(residents) = &mut self.residents {
            residents.aboard.room.set_seen(&seen);
        }
        // The fight: a hostile station's people are the crew's targets, at
        // those same positions, and whatever landed on one since the last
        // step comes off the body it belongs to. The rooms are two, so the
        // shot is fired in one and the wound is in the other.
        let hostile = self
            .residents
            .as_ref()
            .is_some_and(|r| self.stance(r.station) == Stance::Hostile);
        let hits = self.aboard.room.take_hits();
        let Some(residents) = self.residents.as_mut().filter(|_| hostile) else {
            self.aboard.room.set_hostiles(Vec::new());
            return;
        };
        let room = &mut residents.aboard.room;
        for (who, damage) in hits {
            if who >= room.crew_count() as usize || !room.is_alive(who) {
                continue;
            }
            let before = room.health(who);
            room.wound(who, damage);
            if before > 0.0 && room.health(who) <= 0.0 {
                events.push(WorldEvent::EnemyDown {
                    station: residents.station,
                    who: who as u32,
                });
            }
        }
        let alive: Vec<bool> = (0..room.crew_count())
            .map(|who| room.is_alive(who as usize) && room.health(who as usize) > 0.0)
            .collect();
        let targets = self.aboard.hostiles(&visitors, &alive);
        self.aboard.room.set_hostiles(targets);
    }

    /// Under way again: the ship's room is the ship's alone. The residents
    /// were in their own room throughout and go on in it, drawing their
    /// own doors again, until the ship is out of range.
    fn unjoin_rooms(&mut self) {
        if !self.aboard.is_joined() {
            return;
        }
        // A room taken apart drops every errand, a load in somebody's arms
        // with it: whatever was on its way to a site is the hold's again.
        self.drop_loads();
        let seed = self.galaxy_seed ^ self.steps;
        let old = std::mem::replace(&mut self.aboard, Aboard::new(&self.ship.design, 1, 0));
        self.aboard = old.unjoined(&self.ship.design, seed, self.clock_minutes);
        if let Some(residents) = &mut self.residents {
            residents.aboard.room.set_doors_drawn(true);
            residents.aboard.room.set_fog(bims::sight::Fog::All);
        }
    }

    /// The nearest station, and how far the ship is from its hull.
    fn nearest_station(&self) -> Option<(&Station, f64)> {
        let here = self.ship.position();
        self.stations
            .iter()
            .map(|s| (s, s.clearance(here)))
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
    }

    /// Open the station's room when the ship comes within range of its hull,
    /// and close it when the ship leaves. Every station gets one — a
    /// derelict's has nobody in it, and is opened all the same because the
    /// room is what has the pictures of the fixtures. The way out is
    /// further than the way in, the same hysteresis as the local frame and
    /// for the same reason: a ship holding on the line must not open and
    /// close a whole room every step.
    fn settle_residents(&mut self) {
        // Docked, the station is in the ship's room and its own room is the
        // one `join_rooms` left for its pictures. Nothing to settle — and
        // not while the ship is casting off either, since the rooms are
        // still one until it does.
        if matches!(
            self.ship.state,
            ShipState::Docked { .. } | ShipState::CastingOff { .. }
        ) {
            return;
        }
        let near = self
            .nearest_station()
            .map(|(s, clearance)| (s.id, clearance, s.residents(), s.map_seed));
        if let Some(residents) = &self.residents {
            let same = near.filter(|&(id, _, _, _)| id == residents.station);
            let keep = same.is_some_and(|(_, clearance, _, _)| {
                clearance <= data::RESIDENTS_RANGE * data::LOCAL_HYSTERESIS
            });
            if keep {
                return;
            }
            self.residents = None;
        }
        if let Some((id, clearance, count, seed)) = near
            && clearance <= data::RESIDENTS_RANGE
            && let Some(station) = self.station(id)
        {
            self.residents = Some(Residents::open(
                id,
                &station.design,
                count,
                seed,
                self.clock_minutes,
            ));
            // Whose it is: its fog is black for a stranger's, and its
            // people are ringed for an enemy's.
            self.apply_stances();
        }
    }

    /// What a trip would cost, without committing to it.
    ///
    /// Worked out by the page for the local player only, and never a command:
    /// two players hovering over different planets must not be an argument
    /// about where the ship is going.
    pub fn preview(&self, target: Target) -> Result<Preview, PlanError> {
        if let Some(node) = target.node()
            && !self.discovered.contains(&node)
        {
            return Err(PlanError::TargetUndiscovered);
        }

        // Under way, the honest quote includes stopping first — which is what
        // a redirect actually does, and it is often most of the bill.
        let (stopping, from, heading, spent) = match &self.ship.state {
            ShipState::Travelling { plan, departed } => {
                let stop = flight::abort(plan, self.clock_minutes - departed);
                let end = flight::state_at(&stop, stop.duration());
                (
                    stop.duration(),
                    end.position,
                    end.heading,
                    stop.fuel_required,
                )
            }
            _ => (0.0, self.ship.position(), self.ship.heading, 0.0),
        };

        if self.ship.fuel_aboard() == 0 {
            return Err(PlanError::NoFuelAboard);
        }
        let at = self
            .target_position(target)
            .ok_or(PlanError::TargetUndiscovered)?;
        let spare = (self.ship.fuel_aboard() as f64 - spent).max(0.0);
        let plan = flight::plan_trip(&self.ship.dynamics, from, heading, target, at, spare)?;
        Ok(Preview {
            minutes: stopping + plan.duration(),
            stopping,
            fuel: spent + plan.fuel_required,
            docks: plan.docks,
        })
    }

    // --- the ship changing --------------------------------------------------

    /// Everything derived from the design, redone.
    ///
    /// **The one door.** Trading goes through it, a plan ending goes through
    /// it, and construction will go through it. What it promises is that the
    /// anchor and the heading do not move: the hull stays exactly where it was
    /// in the system and on the screen, and it is the *centre of mass* — the
    /// ship's position — that shifts when weight is added to one end.
    ///
    /// A trip already under way keeps the dynamics it was planned with, so
    /// this does not alter an arrival that has been promised. It is the next
    /// plan that flies differently.
    pub fn on_ship_changed(&mut self) {
        if let Ok(dynamics) = flight::dynamics(&self.ship.design, self.ship.crew_count) {
            self.ship.dynamics = dynamics;
        }
        // A battery taken off takes what was in it; one put on arrives
        // empty. Either way the charge cannot exceed what is there to hold
        // it.
        self.power_budget = shipdesign::power_budget(&self.ship.design);
        let storage = self.power_budget.storage;
        if self.ship.charge > storage {
            self.ship.charge = storage;
        }
    }

    /// Stage 6 of [`World::step`]: the reactors' output less the wired
    /// consumers' draw, over one step, into the batteries and clamped to
    /// what they hold. Closed form off the step length, so a browser at
    /// 24x and a server catching up land on the same charge.
    ///
    /// The draw is charged in full whether or not the ship is browned
    /// out: what stops in a brownout is the consumers, and what they would
    /// have drawn was never there to take. The clamp at nought *is* the
    /// brownout.
    fn run_power(&mut self) {
        let budget = self.power_budget;
        let net = (budget.supply - budget.draw) * data::STEP_MINUTES;
        self.ship.charge = (self.ship.charge + net).clamp(0.0, budget.storage);
    }

    // --- making things -------------------------------------------------------

    /// Set what the crew are to keep made of `resource`. Clamped to what the
    /// shelf could hold, since a target past that is one the benches would
    /// never reach.
    pub fn set_craft_target(&mut self, resource: ResourceId, units: u32) {
        let most = self.ship.design.capacity(storage(resource));
        self.craft_targets[resource as usize] = units.min(most);
    }

    pub fn craft_target(&self, resource: ResourceId) -> u32 {
        self.craft_targets[resource as usize]
    }

    /// Whether one of `recipe` could be made out of the hold right now:
    /// every input aboard and not spoken for by a construction site, and
    /// room in the output's class for the output once the inputs are out
    /// of it.
    fn can_make(&self, recipe: &shipdesign::Recipe) -> bool {
        let design = &self.ship.design;
        let inputs_aboard = recipe
            .inputs
            .iter()
            .all(|&(id, units)| self.free(id) >= units);
        let class = storage(recipe.output.0);
        let freed: u32 = recipe
            .inputs
            .iter()
            .filter(|&&(id, _)| storage(id) == class)
            .map(|&(_, units)| units)
            .sum();
        let after = design
            .stored(class)
            .saturating_sub(freed)
            .saturating_add(recipe.output.1);
        inputs_aboard && after <= design.capacity(class)
    }

    /// Every recipe the benches are wanted for this step, one order per
    /// bench of its station: the hold short of the target, one makeable,
    /// the station wired and running. In recipe order, which is what the
    /// room picks from.
    pub fn craft_orders(&self) -> Vec<bims::game::Order> {
        let mut orders = Vec::new();
        for (i, recipe) in shipdesign::RECIPES.iter().enumerate() {
            let (output, units) = recipe.output;
            // What is aboard plus what is on the bench: a chain already
            // making one counts, or the target would be overshot by one
            // for every step the first one took.
            let coming = self.aboard.room.crafts_under_way(i as u32) * units;
            if self.ship.design.carrying(output) + coming >= self.craft_targets[output as usize] {
                continue;
            }
            if !self.can_make(recipe) || !self.powered(recipe.station) {
                continue;
            }
            for (bench, b) in self.aboard.room.benches().iter().enumerate() {
                if b.kind == recipe.station.code() {
                    orders.push(bims::game::Order {
                        recipe: i as u32,
                        bench,
                        minutes: recipe.minutes as f32,
                    });
                }
            }
        }
        orders
    }

    /// A Bim finished `recipe`: the inputs out of the hold and the output
    /// in, if the hold still allows it — the ore may have been sold while
    /// the Bim stood at the smelter — and an event either way. The mass
    /// moves with the cargo; `on_ship_changed` is what notices.
    fn finish_craft(&mut self, recipe: u32, events: &mut Vec<WorldEvent>) {
        let Some(r) = shipdesign::RECIPES.get(recipe as usize) else {
            return;
        };
        if !self.can_make(r) {
            events.push(WorldEvent::CraftLost { recipe });
            return;
        }
        for &(id, units) in r.inputs {
            self.ship.design.cargo[id as usize] -= units;
        }
        self.ship.design.cargo[r.output.0 as usize] += r.output.1;
        self.on_ship_changed();
        events.push(WorldEvent::Crafted { recipe });
    }

    // --- building -------------------------------------------------------------

    /// Whether the ship is standing still: docked, or holding station.
    /// The only time a site may be laid out or worked — see
    /// [`crate::build`].
    pub fn at_rest(&self) -> bool {
        matches!(
            self.ship.state,
            ShipState::Docked { .. } | ShipState::Holding
        )
    }

    /// Whether anything is being built: a site with something carried to
    /// it, or a Bim on the way to one. What refuses a Confirm — the ship
    /// does not move while it is built on.
    pub fn under_construction(&self) -> bool {
        self.builds.iter().any(|s| s.begun()) || self.aboard.room.building_under_way()
    }

    /// Units of `resource` the sites have spoken for, delivered or in
    /// somebody's arms.
    pub fn reserved(&self, resource: ResourceId) -> u32 {
        self.builds
            .iter()
            .fold(0u32, |sum, s| sum.saturating_add(s.reserved(resource)))
    }

    /// Units of `resource` aboard that nothing has claimed: what may be
    /// sold, smelted, or carried to another site. Every hand that reaches
    /// for the hold asks this rather than the raw count.
    pub fn free(&self, resource: ResourceId) -> u32 {
        self.ship
            .design
            .carrying(resource)
            .saturating_sub(self.reserved(resource))
    }

    pub fn site(&self, id: u32) -> Option<&BuildSite> {
        self.builds.iter().find(|s| s.id == id)
    }

    /// The design with every pending site already on it, in order, each
    /// that will go. What a new site is checked against, so a wall laid
    /// out on deck that is itself laid out goes: the deck will be there by
    /// the time the wall is built, since the crew take the sites in order.
    fn design_with_sites(&self) -> ShipDesign {
        let free = shipdesign::Budget::new(Money::MAX);
        let mut design = self.ship.design.clone();
        for site in &self.builds {
            if let Ok(next) = shipdesign::apply(&design, &free, site.edit()) {
                design = next;
            }
        }
        design
    }

    /// Whether a site for `kind` at `origin` turned `rotation` may be laid
    /// out: the ship at rest, the part going where the rules say it may on
    /// the ship as it will be once the pending sites are built, and the
    /// ship as it would then be raising no error the ship does not raise
    /// already — a wall across the spot the hob is worked from is a crew
    /// that starve in front of it, and the design phase would have refused
    /// it too. `Err` is why: a refusal, or the code of the first new fault
    /// as [`crate::event::Refusal::WontFit`] with `issue` set.
    pub fn can_place_site(
        &self,
        kind: PartKind,
        origin: (u32, u32),
        rotation: Rotation,
    ) -> Result<(), SiteRefusal> {
        if !self.at_rest() {
            return Err(SiteRefusal::UnderWay);
        }
        let site = BuildSite::new(0, kind, origin, rotation);
        let before = self.design_with_sites();
        let after =
            match shipdesign::apply(&before, &shipdesign::Budget::new(Money::MAX), site.edit()) {
                Ok(after) => after,
                Err(why) => return Err(SiteRefusal::WontFit(why.code())),
            };
        let crew = self.ship.crew_count;
        let errors = |design: &ShipDesign| -> Vec<u32> {
            shipdesign::validate(design, crew)
                .into_iter()
                .filter(|i| i.severity == shipdesign::Severity::Error)
                .map(|i| i.code)
                .collect()
        };
        let already = errors(&before);
        if let Some(&code) = errors(&after).iter().find(|c| !already.contains(c)) {
            return Err(SiteRefusal::Fault(code));
        }
        Ok(())
    }

    fn place_site(
        &mut self,
        slot: u32,
        kind: PartKind,
        origin: (u32, u32),
        rotation: Rotation,
        events: &mut Vec<WorldEvent>,
    ) {
        match self.can_place_site(kind, origin, rotation) {
            Ok(()) => {}
            Err(SiteRefusal::UnderWay) => {
                events.push(refused(slot, Refusal::UnderWay));
                return;
            }
            Err(SiteRefusal::WontFit(_) | SiteRefusal::Fault(_)) => {
                events.push(refused(slot, Refusal::WontFit));
                return;
            }
        }
        let id = self.next_site;
        self.next_site += 1;
        self.builds.push(BuildSite::new(id, kind, origin, rotation));
        events.push(WorldEvent::SitePlaced { site: id, kind });
    }

    fn cancel_site(&mut self, slot: u32, site: u32, events: &mut Vec<WorldEvent>) {
        let Some(at) = self.builds.iter().position(|s| s.id == site) else {
            events.push(refused(slot, Refusal::NoSuchSite));
            return;
        };
        let gone = self.builds.remove(at);
        events.push(WorldEvent::SiteCancelled { kind: gone.kind });
    }

    /// Every site, one order each, this step: what it still wants carried
    /// — the first material short of its recipe that the hold has any free
    /// of, a load of it — and, with everything there, that it is to be
    /// built and how long that takes. A site wanting neither is still on
    /// the list — a load may be on its way to it, and the walk has to find
    /// it — with nothing to start at it: short of something the hold has
    /// none of, or a wall on deck that is itself still a site, waiting for
    /// the deck. Nothing while the ship is not at rest. In the order the
    /// sites were laid out, which is what the room picks from.
    pub fn build_orders(&self) -> Vec<bims::game::Build> {
        if !self.at_rest() {
            return Vec::new();
        }
        let design = &self.ship.design;
        let free = shipdesign::Budget::new(Money::MAX);
        let t = shipdesign::TILE as f32;
        let offset = self.aboard.offset;
        self.builds
            .iter()
            .map(|site| {
                let recipe = site.recipe(design);
                let tiles = site
                    .tiles()
                    .into_iter()
                    .map(|(x, y)| {
                        bims::math::Rect::from_min_size(
                            bims::math::vec2(
                                x as f32 * t + offset.x as f32,
                                y as f32 * t + offset.y as f32,
                            ),
                            bims::math::vec2(t, t),
                        )
                    })
                    .collect();
                let haul = recipe.iter().find_map(|&(id, _)| {
                    let short = site.short(design, id);
                    let load = short.min(data::HAUL_LOAD).min(self.free(id));
                    (load > 0).then_some((id as u32, load))
                });
                // Only a part that would go down now is worth putting
                // together: one on a site that is itself waiting is not,
                // and nor is one `can_modify_part` says to leave.
                let buildable = site.stocked(design)
                    && self.can_modify_part(site.kind)
                    && shipdesign::apply(design, &free, site.edit()).is_ok();
                let minutes = if buildable {
                    build::build_minutes(&recipe) as f32
                } else {
                    0.0
                };
                bims::game::Build {
                    site: site.id,
                    tiles,
                    haul,
                    minutes,
                }
            })
            .collect()
    }

    /// Who may put a suit on and go out to a site beyond the hull, by
    /// slot: alive, under the dose limit — the same line a walk to mine
    /// draws — and a suit aboard to wear. The airlock itself is the room's
    /// to find.
    fn suit_ok(&self) -> Vec<bool> {
        let suit = self.ship.design.carrying(ResourceId::Suit) > 0;
        self.health
            .iter()
            .map(|h| suit && !h.dead && h.dose < data::EVA_DOSE_LIMIT)
            .collect()
    }

    /// A Bim took a load off a shelf for `site`: as much of `units` of
    /// `resource` as the hold has free is now in its arms — spoken for, not
    /// moved. A site that has gone gets nothing, and the Bim carries a
    /// crate of nothing to nowhere, which the room sorts out at its next
    /// walk.
    fn finish_pick(&mut self, site: u32, resource: u32, units: u32) {
        let Some(resource) = ResourceId::ALL.get(resource as usize).copied() else {
            return;
        };
        let got = units.min(self.free(resource));
        if let Some(site) = self.builds.iter_mut().find(|s| s.id == site) {
            site.carrying[resource as usize] += got;
        }
    }

    /// A load arrived: everything in transit to the site is delivered.
    fn finish_drop(&mut self, site: u32) {
        if let Some(site) = self.builds.iter_mut().find(|s| s.id == site) {
            for (delivered, carrying) in site.delivered.iter_mut().zip(site.carrying.iter_mut()) {
                *delivered += core::mem::take(carrying);
            }
        }
    }

    /// A load was given up short of the site: the hold's again.
    fn finish_return(&mut self, site: u32) {
        if let Some(site) = self.builds.iter_mut().find(|s| s.id == site) {
            site.carrying = [0; CARGO_SLOTS];
        }
    }

    /// Every load in somebody's arms, the hold's again: for a room being
    /// taken apart, whose crew drop everything they were carrying without
    /// the room saying so.
    fn drop_loads(&mut self) {
        for site in &mut self.builds {
            site.carrying = [0; CARGO_SLOTS];
        }
    }

    /// A Bim put a site together: the part goes down and its recipe comes
    /// out of the hold in one go, if the rules still allow it — the deck
    /// under it may have been laid out and cancelled since, the metal sold
    /// — and an event either way. The site is finished with whatever
    /// happened: a part that will not go is a site to lay out again, not
    /// one to stand at for ever. The room is laid out again under the
    /// crew with the part in it.
    fn finish_build(&mut self, site: u32, events: &mut Vec<WorldEvent>) {
        let Some(at) = self.builds.iter().position(|s| s.id == site) else {
            return;
        };
        let site = self.builds.remove(at);
        if !self.can_modify_part(site.kind) {
            events.push(WorldEvent::BuildLost { kind: site.kind });
            return;
        }
        // The reservation is this site's own, and it is gone with the site
        // — so the recipe is checked against what is free of *every other*
        // site's claim, which is what `free` says now that it is out of
        // the list.
        let recipe = site.recipe(&self.ship.design);
        if recipe.iter().any(|&(id, units)| self.free(id) < units) {
            events.push(WorldEvent::BuildLost { kind: site.kind });
            return;
        }
        match shipdesign::build_from_cargo(&self.ship.design, site.edit()) {
            Ok(next) => {
                self.ship.design = next;
                self.on_ship_changed();
                self.relayout_room();
                events.push(WorldEvent::Built { kind: site.kind });
            }
            Err(_) => events.push(WorldEvent::BuildLost { kind: site.kind }),
        }
    }

    /// The room aboard laid out again on the ship as it now is — with the
    /// station's deck joined to it, if it is docked — keeping the crew,
    /// their errands and everything else that is state. See
    /// `Aboard::relayout`. The residents' room is the station's and is
    /// not touched.
    fn relayout_room(&mut self) {
        let joined = self.ship.state.alongside().and_then(|id| {
            let berth = self.berth_at(id)?;
            let station = self.station(id)?;
            crate::docking::join(
                &self.ship.design,
                self.ship.dynamics.centre_of_mass,
                station,
                &berth,
            )
        });
        match joined {
            Some(joined) if self.aboard.is_joined() => self.aboard.relayout(joined.design),
            _ => {
                let design = self.ship.design.clone();
                self.aboard.relayout(design);
            }
        }
    }

    // --- a walk outside ------------------------------------------------------

    /// The belt whose frame the ship is in, if it is in one, whatever the
    /// ship is doing there.
    fn belt_here(&self) -> Option<&worldgen::Body> {
        let Frame::Local(Node::Body(id)) = self.ship.frame else {
            return None;
        };
        self.system
            .body(id)
            .filter(|b| b.kind == worldgen::BodyKind::AsteroidBelt)
    }

    /// The belt the ship is holding at, if it is: at rest in the local
    /// frame of a body of that kind.
    fn belt_alongside(&self) -> Option<&worldgen::Body> {
        if self.ship.state != ShipState::Holding {
            return None;
        }
        self.belt_here()
    }

    /// The hull's tiles, as the box they span — `(x0, y0, x1, y1)`,
    /// inclusive — which is what a mining site is laid out clear of.
    fn hull_box(&self) -> (i32, i32, i32, i32) {
        let mut span: Option<(i32, i32, i32, i32)> = None;
        for part in &self.ship.design.parts {
            for (x, y) in part.tiles() {
                let (x, y) = (x as i32, y as i32);
                span = Some(match span {
                    None => (x, y, x, y),
                    Some((x0, y0, x1, y1)) => (x0.min(x), y0.min(y), x1.max(x), y1.max(y)),
                });
            }
        }
        span.unwrap_or((0, 0, 0, 0))
    }

    /// Stage 4's last word: the ship has come to rest at a belt it has not
    /// held at before, so the asteroids about it are laid out now, in the
    /// frame it is holding in. Once per belt — coming back finds the site
    /// as it was left, mined tiles and all.
    fn settle_site(&mut self) {
        let Some(belt) = self.belt_alongside().map(|b| b.id) else {
            return;
        };
        if self.sites.iter().any(|s| s.belt == belt) {
            return;
        }
        let site = MiningSite::generate(self.galaxy_seed, self.star_id, belt, self.hull_box());
        let at = self.sites.partition_point(|s| s.belt < belt);
        self.sites.insert(at, site);
        self.site_version += 1;
    }

    /// The mining site the ship is holding at, if it is at one.
    pub fn site_here(&self) -> Option<&MiningSite> {
        let belt = self.belt_alongside()?.id;
        self.sites.iter().find(|s| s.belt == belt)
    }

    fn site_here_mut(&mut self) -> Option<&mut MiningSite> {
        let belt = self.belt_alongside()?.id;
        self.sites.iter_mut().find(|s| s.belt == belt)
    }

    /// Mark a rock at the site to be mined, or take the mark off again.
    /// Nothing happens away from a site or off a rock.
    fn mark_rock(&mut self, x: i32, y: i32) {
        if let Some(site) = self.site_here_mut()
            && site.toggle_mark(x, y)
        {
            self.site_version += 1;
        }
    }

    fn clear_marks(&mut self) {
        if let Some(site) = self.site_here_mut() {
            site.clear_marks();
            self.site_version += 1;
        }
    }

    /// The ship is leaving the site: the marks come off, since an order to
    /// mine a rock is an order about a place the ship is at, whoever is out
    /// there is brought back in through the door, and what that walk had
    /// mined so far is said now, since it will not come in on its own.
    fn leave_site(&mut self, events: &mut Vec<WorldEvent>) {
        // By the frame, not by the state: the ship is under way by the time
        // this is asked.
        if let Some(belt) = self.belt_here().map(|b| b.id)
            && let Some(site) = self.sites.iter_mut().find(|s| s.belt == belt)
        {
            site.clear_marks();
            self.site_version += 1;
        }
        self.aboard.room.recall_outside();
        if self.walk_tally != (0, 0, 0) {
            self.finish_walk(events);
        }
    }

    /// Whether the crew may walk outside this step, and to what: holding at
    /// a belt with rocks marked, a port to go out by, a suit in the locker,
    /// room on the shelf for what comes back, and each Bim's dose under the
    /// limit. The room is handed every rock as something to walk round and
    /// the marked ones as where to go, in the ship's own units.
    pub fn eva_offer(&self) -> Option<bims::game::Eva> {
        let site = self.site_here()?;
        shipdesign::dock::port(&self.ship.design)?;
        if self.ship.design.carrying(ResourceId::Suit) == 0 {
            return None;
        }
        let design = &self.ship.design;
        if design.stored(Storage::Shelf) >= design.capacity(Storage::Shelf) {
            return None;
        }
        let t = shipdesign::TILE as f32;
        let middle = |(x, y): (i32, i32)| {
            let (mx, my) = mining::tile_middle(x, y);
            bims::math::vec2(mx as f32, my as f32)
        };
        Some(bims::game::Eva {
            allowed: self
                .health
                .iter()
                .map(|h| !h.dead && h.dose < data::EVA_DOSE_LIMIT)
                .collect(),
            targets: site.targets().map(middle).collect(),
            rocks: site
                .tiles
                .iter()
                .map(|r| {
                    let m = middle((r.x, r.y));
                    bims::math::Rect::from_min_size(
                        bims::math::vec2(m.x - t / 2.0, m.y - t / 2.0),
                        bims::math::vec2(t, t),
                    )
                })
                .collect(),
            version: self.site_version,
            tile_minutes: data::MINE_TILE_MINUTES as f32,
        })
    }

    /// A Bim mined the rock whose middle is `at`, in the ship's units: the
    /// tile comes out of the site and what it yields goes on the shelf, as
    /// much of it as fits, counted towards what the walk brings back. A
    /// tile that is not there — mined by the other one, or the ship has
    /// left — yields nothing.
    fn finish_tile(&mut self, at: (f32, f32)) {
        let t = shipdesign::TILE as f32;
        let (x, y) = ((at.0 / t).floor() as i32, (at.1 / t).floor() as i32);
        let Some(kind) = self.site_here_mut().and_then(|s| s.mine(x, y)) else {
            return;
        };
        self.site_version += 1;
        let (resource, units) = mining::yield_of(kind);
        let design = &mut self.ship.design;
        let room = design
            .capacity(Storage::Shelf)
            .saturating_sub(design.stored(Storage::Shelf));
        let got = units.min(room);
        design.cargo[resource as usize] += got;
        match kind {
            mining::Rock::Stone => self.walk_tally.0 += got,
            mining::Rock::Iron => self.walk_tally.1 += got,
            mining::Rock::Galvum => self.walk_tally.2 += got,
        }
        self.on_ship_changed();
    }

    /// A walk came back: say what it brought, all at once.
    fn finish_walk(&mut self, events: &mut Vec<WorldEvent>) {
        let (rock, ore, galvum) = core::mem::take(&mut self.walk_tally);
        events.push(WorldEvent::Mined { rock, ore, galvum });
    }

    /// Stage 8: each body, one step on, exposed while outside in the suit
    /// and sheltered otherwise. Closed form in the health crate, so a
    /// server catching up on an hour lands where a browser did.
    fn run_health(&mut self, events: &mut Vec<WorldEvent>) {
        for who in 0..self.health.len() {
            let exposure = if self.aboard.room.is_outside(who) {
                health::Exposure::Exposed {
                    intensity: data::SUIT_INTENSITY,
                }
            } else {
                health::Exposure::Shielded
            };
            for event in health::update(&mut self.health[who], data::STEP_MINUTES, exposure) {
                events.push(WorldEvent::Health {
                    who: who as u32,
                    event,
                });
            }
        }
    }

    /// The ship's power, as the crew would read it off a panel.
    pub fn power(&self) -> Power {
        let budget = self.power_budget;
        Power {
            supply: budget.supply,
            draw: budget.draw,
            storage: budget.storage,
            charge: self.ship.charge,
        }
    }

    /// Whether a consumer of this kind is running: some part of that kind
    /// is on a live network, and either the ship is not browned out or the
    /// kind is one of the essentials, which run off the reactor's own
    /// output when the batteries are flat. A kind that draws nothing is
    /// running by definition; a kind that draws and is not aboard is not.
    ///
    /// Asked per **kind** rather than per part, because what asks it is a
    /// chain deciding whether the smelter works, and a chain has a kind in
    /// hand and not an id.
    pub fn powered(&self, kind: PartKind) -> bool {
        let def = kind.def();
        if !def.draws() {
            return true;
        }
        let wired = self
            .ship
            .design
            .parts
            .iter()
            .any(|p| p.kind == kind && shipdesign::is_powered(&self.ship.design, p.id));
        wired && (!self.power().brownout() || shipdesign::essential(kind))
    }

    /// Whether the construction step may touch a part of this kind.
    ///
    /// **It has to be asked before any construction or deconstruction.** Two
    /// things it refuses, and both of them are about a promise already made:
    /// an engine or a thruster taken off mid-flight would change a trip that
    /// has been quoted, and a fuel tank taken off could leave a reservation
    /// with nowhere to sit.
    pub fn can_modify_part(&self, kind: PartKind) -> bool {
        let travelling = matches!(self.ship.state, ShipState::Travelling { .. });
        if travelling && (kind.def().pushes() || kind.def().turns()) {
            return false;
        }
        if let Some((Storage::FuelTank, units)) = kind.def().capacity {
            let left = self
                .ship
                .design
                .capacity(Storage::FuelTank)
                .saturating_sub(units);
            if left < self.ship.reserved_fuel {
                return false;
            }
        }
        true
    }

    /// Whether this player may give the ship an order: their crew member
    /// has to be standing at the helm. Confirm and Abort ask it; speed and
    /// trading deliberately do not.
    pub fn can_command(&self, slot: u32) -> bool {
        slot < self.speed_requests.len() as u32 && self.at_the_helm(slot)
    }

    /// The seat at the helm: the middle of the first helm's use spot, in
    /// the ship's design units. `None` for a ship with no helm, which
    /// nobody can fly from anywhere.
    pub fn helm_spot(&self) -> Option<DVec2> {
        let helm = self
            .ship
            .design
            .parts
            .iter()
            .filter(|p| p.kind == PartKind::Helm)
            .min_by_key(|p| p.id)?;
        let &(x, y) = helm.use_spots().first()?;
        let t = shipdesign::TILE as f64;
        Some(DVec2 {
            x: (x as f64 + 0.5) * t,
            y: (y as f64 + 0.5) * t,
        })
    }

    /// Whether that player's crew member is at the helm: within
    /// [`data::HELM_REACH`] of the seat. Slot *i* is Bim *i*, the same
    /// pairing as the bunks.
    pub fn at_the_helm(&self, slot: u32) -> bool {
        if slot >= self.aboard.crew_count() {
            return false;
        }
        self.helm_spot()
            .is_some_and(|seat| self.aboard.position(slot).distance(seat) <= data::HELM_REACH)
    }

    /// Send that player's crew member to the helm, to stand there until
    /// sent elsewhere. A room order like any other, not a command: it
    /// crosses no seam and lands on nobody else's screen. False when there
    /// is no helm or no way to it.
    pub fn order_to_helm(&mut self, slot: u32) -> bool {
        if slot >= self.aboard.crew_count() {
            return false;
        }
        let Some(seat) = self.helm_spot() else {
            return false;
        };
        let at = seat.add(self.aboard.offset);
        self.aboard
            .room
            .send_to(slot as usize, bims::math::vec2(at.x as f32, at.y as f32))
    }

    /// Lift the post [`World::order_to_helm`] set, so that player's crew
    /// member goes back about its errands. The page's Confirm walks the
    /// Bim to the seat, gives the order once it is there, and then lets it
    /// go — under way the helm is a job the room hands out itself. A room
    /// order like the walk: it crosses no seam.
    pub fn stand_down(&mut self, slot: u32) {
        if slot < self.aboard.crew_count() {
            self.aboard.room.stand_down(slot as usize);
        }
    }

    /// Stand that player's crew member at the helm this instant. For the
    /// probes: a test of a trip is not a test of the walk to the seat.
    pub fn man_the_helm_for_probe(&mut self, slot: u32) {
        let Some(seat) = self.helm_spot() else {
            return;
        };
        let at = seat.add(self.aboard.offset);
        self.aboard
            .room
            .post_for_probe(slot as usize, bims::math::vec2(at.x as f32, at.y as f32));
    }

    // --- trading ------------------------------------------------------------

    fn buy(&mut self, slot: u32, resource: ResourceId, units: u32, events: &mut Vec<WorldEvent>) {
        let ShipState::Docked { station } = self.ship.state else {
            events.push(refused(slot, Refusal::NotDocked));
            return;
        };
        if !self
            .station(station)
            .is_some_and(|s| s.stock.sells(resource))
        {
            events.push(refused(slot, Refusal::NotSoldHere));
            return;
        }
        let Ok(value) = trade_value(resource, units) else {
            events.push(refused(slot, Refusal::Unaffordable));
            return;
        };
        if value > self.money {
            events.push(refused(slot, Refusal::Unaffordable));
            return;
        }
        let class = storage(resource);
        let wanted = self.ship.design.stored(class).saturating_add(units);
        if wanted > self.ship.design.capacity(class) {
            events.push(refused(slot, Refusal::NoRoomAboard));
            return;
        }
        self.money -= value;
        self.ship.design.cargo[resource as usize] += units;
        self.on_ship_changed();
        events.push(WorldEvent::Traded {
            slot,
            resource,
            units: units as i64,
        });
    }

    fn sell(&mut self, slot: u32, resource: ResourceId, units: u32, events: &mut Vec<WorldEvent>) {
        if !matches!(self.ship.state, ShipState::Docked { .. }) {
            events.push(refused(slot, Refusal::NotDocked));
            return;
        }
        // Reserved fuel is not the crew's to sell. It is spoken for by a trip
        // that has been committed to, and selling it would leave the ship
        // short somewhere there is nothing to buy. Nor is what the
        // construction sites have claimed — see `free`.
        let aboard = self.free(resource);
        let sellable = if resource == ResourceId::Fuel {
            aboard.saturating_sub(self.ship.reserved_fuel)
        } else {
            aboard
        };
        if units > sellable {
            events.push(refused(slot, Refusal::NotAboard));
            return;
        }
        let Ok(value) = trade_value(resource, units) else {
            events.push(refused(slot, Refusal::SumTooBig));
            return;
        };
        let Ok(money) = economy::add(self.money, value) else {
            events.push(refused(slot, Refusal::SumTooBig));
            return;
        };
        self.money = money;
        self.ship.design.cargo[resource as usize] -= units;
        self.on_ship_changed();
        events.push(WorldEvent::Traded {
            slot,
            resource,
            units: -(units as i64),
        });
    }

    // --- looking out of the window ------------------------------------------

    /// How far the crew can see: their own eyes, or the sensor arrays, and
    /// whichever of those reaches further.
    pub fn detection_range(&self) -> f64 {
        let arrays = self.ship.design.count(PartKind::SensorArray) as f64;
        let radar = (data::RADAR_RANGE_PER_SENSOR * arrays).min(data::RADAR_RANGE_MAX);
        data::VISION_RANGE.max(radar)
    }

    /// Everything that came within range of the stretch just travelled.
    ///
    /// Tested against the **segment**, not against the endpoints. At 24x a
    /// step can be a long way, and a station passed in the middle of one would
    /// otherwise be missed entirely — the ship would fly straight through
    /// somewhere and nobody would have seen it.
    fn discover_along(&mut self, from: DVec2, to: DVec2, events: &mut Vec<WorldEvent>) {
        let range = self.detection_range();
        let mut found = Vec::new();
        for node in self.system.nodes() {
            if self.discovered.contains(&node) {
                continue;
            }
            let Some(at) = self.system.absolute_position(node) else {
                continue;
            };
            if segment_distance(from, to, at) <= range {
                found.push(node);
            }
        }
        if found.is_empty() {
            return;
        }
        for node in found {
            self.discovered.push(node);
            events.push(WorldEvent::Discovered { node });
        }
        // Sorted rather than in the order they happened to be seen: the
        // checksum runs over this, and two clients that found the same two
        // things in one step must not disagree about the list.
        self.discovered.sort_by_key(node_key);
    }

    /// Which node the view is about, if it is about one at all.
    fn frame_candidate(&self) -> Option<(Node, f64, f64)> {
        let node = match &self.ship.state {
            ShipState::Docked { station }
            | ShipState::CastingOff { station, .. }
            | ShipState::Undocking { station, .. }
            | ShipState::Docking { station, .. } => Node::Station(*station),
            ShipState::Holding => self.nearest_discovered()?,
            ShipState::Travelling { plan, departed } => {
                // The last leg of a trip aimed at somewhere, and only that.
                // Passing something on the way is not arriving at it.
                let phase = flight::state_at(plan, self.clock_minutes - departed).phase;
                if phase != Phase::Brake {
                    return None;
                }
                plan.target.node()?
            }
        };
        let at = self.system.absolute_position(node)?;
        Some((
            node,
            at.distance(self.ship.position()),
            data::local_radius(node),
        ))
    }

    fn nearest_discovered(&self) -> Option<Node> {
        let here = self.ship.position();
        self.discovered
            .iter()
            .copied()
            .filter_map(|node| self.system.absolute_position(node).map(|at| (node, at)))
            .min_by(|a, b| {
                let (da, db) = (a.1.distance(here), b.1.distance(here));
                da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(node, _)| node)
    }

    fn settle_frame(&mut self, events: &mut Vec<WorldEvent>) {
        let now = frame::settle(self.ship.frame, self.frame_candidate());
        if now != self.ship.frame {
            self.ship.frame = now;
            events.push(WorldEvent::FrameChanged { frame: now });
        }
    }

    // --- readouts -----------------------------------------------------------

    /// What the world is actually running at: the slowest request there is.
    pub fn effective_speed(&self) -> Speed {
        speed::effective(&self.speed_requests)
    }

    /// Record what a player wants the world to run at.
    ///
    /// **The one control that does not wait for a step**, and it has to be:
    /// at a pause no steps are taken at all, so a queued speed change would
    /// never be applied and a pause could never be lifted. It is safe to be
    /// the exception because it changes nothing a step would have changed —
    /// not the clock, not the ship, not what anybody is carrying, only how
    /// fast the caller is expected to turn the crank.
    ///
    /// [`Command::SetSpeed`] goes through here too, so there is one
    /// implementation and two ways in rather than two implementations.
    pub fn request_speed(&mut self, slot: u32, speed: Speed) {
        if let Some(request) = self.speed_requests.get_mut(slot as usize) {
            *request = speed;
        }
    }

    /// How many steps a second of real time is worth at 1x.
    ///
    /// The page needs it to turn a frame into steps, and it is a fact about
    /// the world rather than about the browser — a 60 written down in
    /// `crates/app/src/screens/game.rs` would be a second copy of [`data::STEP_MINUTES`] waiting
    /// to disagree with the first.
    pub fn steps_per_second(&self) -> f64 {
        time::MINUTES_PER_SECOND / data::STEP_MINUTES
    }

    /// The whole day count, and the minutes into it. The host formats; no
    /// strings cross the boundary.
    pub fn day(&self) -> u32 {
        (self.clock_minutes / time::DAY) as u32
    }

    pub fn minutes_into_day(&self) -> f64 {
        self.clock_minutes % time::DAY
    }

    /// Where the trip has got to, for a caller that wants to draw it.
    pub fn trip_state(&self) -> Option<flight::State> {
        match &self.ship.state {
            ShipState::Travelling { plan, departed } => {
                Some(flight::state_at(plan, self.clock_minutes - departed))
            }
            _ => None,
        }
    }

    /// What the ship is doing to itself — the engines lit and the thrusters
    /// pushing — for a caller that wants to draw the exhaust. Nothing while
    /// docked or holding.
    pub fn effort(&self) -> flight::Effort {
        match &self.ship.state {
            ShipState::Travelling { plan, departed } => {
                flight::effort_at(plan, self.clock_minutes - departed)
            }
            _ => flight::Effort::NONE,
        }
    }

    pub fn plan(&self) -> Option<&Plan> {
        match &self.ship.state {
            ShipState::Travelling { plan, .. } => Some(plan),
            _ => None,
        }
    }

    /// How far through the trip the ship is, for a caller that wants to
    /// draw a bar: minutes flown and minutes the whole trip takes, the
    /// first never past the second. `None` when not `Travelling` — the
    /// push-off and the docking are their own states with their own
    /// lengths, and a stop is a trip too.
    pub fn trip_progress(&self) -> Option<(f64, f64)> {
        match &self.ship.state {
            ShipState::Travelling { plan, departed } => {
                let total = plan.duration();
                Some(((self.clock_minutes - departed).clamp(0.0, total), total))
            }
            _ => None,
        }
    }

    /// A number two clients can compare to find out whether they have drifted
    /// apart. See [`crate::checksum`] for what goes into it and why the floats
    /// are rounded on the way.
    pub fn checksum(&self) -> u64 {
        crate::checksum::world_checksum(self)
    }

    /// The design's identity, for a caller that wants to show it.
    pub fn design_hash(&self) -> u64 {
        design_hash(&self.ship.design)
    }

    // --- seams for probes ---------------------------------------------------

    /// Run the discovery pass over a stretch the ship did not actually fly.
    ///
    /// For probes. The alternative is a test that has to arrange a real trip
    /// past a body whose position is whatever the seed happened to put it at,
    /// which would be a test of the generator dressed up as a test of
    /// discovery. Same reason the room has `put_for_probe`.
    pub fn discover_for_probe(&mut self, from: DVec2, to: DVec2) -> Vec<WorldEvent> {
        let mut events = Vec::new();
        self.discover_along(from, to, &mut events);
        events
    }

    /// Put the ship's centre of mass somewhere, without flying it there.
    ///
    /// For probes, and for one thing in particular: a ship that is holding
    /// station does not move at all, so there is no other way to walk it
    /// across a local frame's boundary and back.
    pub fn put_for_probe(&mut self, at: DVec2) {
        self.ship.set_position(at);
    }

    /// Let go of the dock without flying anywhere: holding, with the ship's
    /// room its own again. For probes of what happens away from a station.
    pub fn undock_for_probe(&mut self) {
        self.ship.state = ShipState::Holding;
        self.unjoin_rooms();
    }

    /// Settle the local frame, for a probe that has just moved the ship.
    pub fn settle_frame_for_probe(&mut self) -> Vec<WorldEvent> {
        let mut events = Vec::new();
        self.settle_frame(&mut events);
        events
    }

    /// Stage a fight: the station the ship is tied to made hostile, the
    /// first crew member recruited and stood just inside its port, and the
    /// first of its people stood a few tiles down the corridor from them —
    /// the state a fight is looked at in without walking the station for
    /// one. `false`, and nothing moved, away from a berth or at a station
    /// nobody lives on. For probes and for `BIMS_FIGHT` in the app.
    pub fn stage_fight_for_probe(&mut self) -> bool {
        let Some(station) = self.ship.state.station() else {
            return false;
        };
        let Some(ashore) = self.aboard.ashore else {
            return false;
        };
        let Some(port) = self.station(station).and_then(|s| s.port()) else {
            return false;
        };
        if !self
            .residents
            .as_ref()
            .is_some_and(|r| r.aboard.count() > 0)
        {
            return false;
        }
        self.set_hostile(station, true);
        // The resident: four tiles further in than the crew member, along
        // the corridor the port opens onto, in the station's own frame.
        let reach = (data::ASHORE_TILES + 4.0) * shipdesign::TILE as f64;
        let there = bims::math::vec2(
            (port.centre.0 - port.outward.0 as f64 * reach) as f32,
            (port.centre.1 - port.outward.1 as f64 * reach) as f32,
        );
        if let Some(residents) = &mut self.residents {
            residents.aboard.room.put_for_probe(0, there);
        }
        // The crew member: just inside the station's door, under orders.
        self.aboard
            .room
            .put_for_probe(0, bims::math::vec2(ashore.x as f32, ashore.y as f32));
        self.aboard.room.recruit_for_probe(0, true);
        true
    }

    /// Let go of the dock and hold at the first belt of this system, its
    /// site laid out — the state a walk outside is looked at in. `false`,
    /// and nothing moved, when the system has no belt. For probes and for
    /// `BIMS_AT_BELT` in the app.
    pub fn hold_at_belt_for_probe(&mut self) -> bool {
        let Some(belt) = self
            .system
            .bodies
            .iter()
            .find(|b| b.kind == worldgen::BodyKind::AsteroidBelt)
            .cloned()
        else {
            return false;
        };
        self.undock_for_probe();
        self.put_for_probe(belt.position);
        self.settle_frame_for_probe();
        self.settle_site();
        true
    }
}

/// The ship's power at this instant: units a minute in and out, and what
/// the batteries hold and have.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Power {
    pub supply: f64,
    pub draw: f64,
    pub storage: f64,
    pub charge: f64,
}

impl Power {
    /// Whether the optional consumers have stopped: more drawn than made,
    /// and nothing left in the batteries to cover the difference. A ship
    /// with no batteries and a short network is browned out for good.
    pub fn brownout(&self) -> bool {
        self.draw > self.supply && self.charge <= 0.0
    }
}

/// What a trip would cost. Never a command — see [`World::preview`].
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Preview {
    /// The whole thing, stopping included.
    pub minutes: f64,
    /// How much of that is coming to rest first. Zero from a standstill.
    pub stopping: f64,
    pub fuel: f64,
    pub docks: bool,
}

fn refused(slot: u32, why: Refusal) -> WorldEvent {
    WorldEvent::Refused { slot, why }
}

/// Fuel is bought and stowed in whole units, so a bill of 148.6 costs 149.
/// Rounding down would let a trip burn fuel nobody had.
fn units_of(amount: f64) -> u32 {
    if !amount.is_finite() || amount <= 0.0 {
        return 0;
    }
    amount.ceil().min(u32::MAX as f64) as u32
}

/// A total order over nodes, for keeping [`World::discovered`] sorted.
/// Bodies before stations, ids ascending — the same order
/// `StarSystem::nodes` hands them out in.
pub fn node_key(node: &Node) -> (u32, u32) {
    match node {
        Node::Body(id) => (0, *id),
        Node::Station(id) => (1, *id),
    }
}

/// [`segment_distance`], for the probe that checks the geometry directly
/// rather than through a trip whose shape the seed decides.
pub fn segment_distance_for_probe(from: DVec2, to: DVec2, point: DVec2) -> f64 {
    segment_distance(from, to, point)
}

/// How near a point comes to a line segment. The one piece of geometry
/// discovery needs, and it is a segment rather than a line because the ship
/// travelled a stretch and not a whole axis.
fn segment_distance(from: DVec2, to: DVec2, point: DVec2) -> f64 {
    let along = to.sub(from);
    let len2 = along.length_squared();
    if len2 <= 0.0 {
        return from.distance(point);
    }
    let t = (point.sub(from).x * along.x + point.sub(from).y * along.y) / len2;
    let t = t.clamp(0.0, 1.0);
    from.add(along.scale(t)).distance(point)
}

/// Any dock somebody lives on, anywhere in the galaxy: the `pick`-th of
/// them, in star then station order, wrapping round. For `nix run .#test`,
/// which wants a *different* place each time and a place a crew can live —
/// so a random roll from the page turns into a random system, and the same
/// roll into the same one.
///
/// Every system is generated to answer it, which is what the lobby's World
/// tab does too; a galaxy is a few hundred stars and it takes a moment.
pub fn spawn_anywhere(galaxy: &Galaxy, pick: u64) -> Option<(u32, u32)> {
    let docks: Vec<(u32, u32)> = galaxy
        .every_system()
        .iter()
        .flat_map(|system| {
            system
                .stations
                .iter()
                .filter(|s| crate::station::residents_of(s.kind) > 0)
                .map(move |s| (system.star_id, s.id))
        })
        .collect();
    if docks.is_empty() {
        return None;
    }
    Some(docks[(pick % docks.len() as u64) as usize])
}

/// Where the **simulation** starts: the lowest star id with a station
/// somebody lives on, and the lowest such station in that system.
///
/// Somebody lives on it, because a crew that opens docked at a derelict
/// opens beside a wreck with nobody aboard — which is a place to salvage,
/// not a place to start from. `crate::station::residents_of` is what says
/// who lives where.
///
/// The game proper starts where the lobby's World tab said, and that pair
/// comes into [`World::start`] from outside. This is for `nix run
/// .#simulation`, which has no lobby in front of it and wants the same dock
/// every time — and for the fixtures, for the same reason.
pub fn spawn(galaxy: &Galaxy) -> Option<(u32, u32)> {
    for star in &galaxy.stars {
        let Some(system) = galaxy.system(star.id) else {
            continue;
        };
        if let Some(station) = system
            .stations
            .iter()
            .filter(|s| crate::station::residents_of(s.kind) > 0)
            .map(|s| s.id)
            .min()
        {
            return Some((star.id, station));
        }
    }
    None
}
