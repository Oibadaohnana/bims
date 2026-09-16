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
use shipdesign::parts::PartKind;
use shipdesign::{ShipDesign, design_hash};
use worldgen::math::DVec2;
use worldgen::{Galaxy, GalaxyType, Node, StarSystem};

use crate::crew::Aboard;
use crate::data;
use crate::event::{Refusal, WorldEvent};
use crate::frame::{self, Frame};
use crate::speed::{self, Speed};

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
}

impl ShipState {
    /// The number that crosses the wasm boundary.
    pub fn code(&self) -> u32 {
        match self {
            ShipState::Docked { .. } => 0,
            ShipState::Holding => 1,
            ShipState::Travelling { .. } => 2,
        }
    }
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
    /// Where a redirect is going once the ship has stopped. Only ever the
    /// **latest** confirmed target: two Confirms in one step leave one.
    pub pending: Option<(u32, Target)>,
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
    /// What the crew have seen, shared between all of them and never
    /// forgotten. Sorted, so a checksum over it means something.
    pub discovered: Vec<Node>,
    /// One per player, in slot order. The world runs at the slowest of them.
    pub speed_requests: Vec<Speed>,
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
        let at = system
            .absolute_position(Node::Station(station_id))
            .ok_or(StartError::NoSuchStation)?;

        let dynamics = flight::dynamics(&design, players).map_err(StartError::NotAShip)?;
        let aboard = Aboard::new(&design, players, seed);
        let mut ship = Ship {
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
        };
        ship.set_position(at);

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
            discovered: Vec::new(),
            // Everybody starts at real time. Anything else would have the
            // world already moving before the first player had looked at it.
            speed_requests: vec![Speed::Real; players as usize],
        };

        // The whole system, charted. The crew picked this dock off the
        // lobby's chart of this very system — every planet and every station
        // on it — and a map that then hid what they had just been looking at
        // would be a map with nothing on it to fly to. Discovery is for what
        // the chart does not show: the systems beyond this one, when there
        // is a way there, and anything a sweep turns up on the way.
        world.discovered = world.system.nodes();
        world.discovered.sort_by_key(node_key);
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

        // 3. Flight.
        let was = self.ship.position();
        self.fly(&mut events);
        let now = self.ship.position();

        // 4. What that brought into range.
        self.discover_along(was, now, &mut events);
        self.settle_frame(&mut events);

        // 5. Crew: the room's own update, aboard, on this clock. Bims live
        //    in ship-design coordinates and the ship's position, rotation and
        //    acceleration do not reach them. See `crates/world/src/crew.rs`.
        self.aboard.step();

        // 6. Construction. Also empty. What goes here builds and pulls apart
        //    through `shipdesign::materials`, out of what is aboard, and asks
        //    `World::can_modify_part` before it touches anything.

        // 7. Health and radiation. Also empty. The design's exposure map —
        //    `shipdesign::exposure` — is the radiation input, and
        //    `crates/health` is the body it goes into.

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
            | Command::Sell { slot, .. } => slot,
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

        self.ship.state = match station {
            Some(id) => ShipState::Docked { station: id },
            None => ShipState::Holding,
        };
        // Docking is the one place the ship is *put* somewhere rather than
        // flown there: the approach ends a station radius short, and going
        // alongside closes the gap.
        if let Some(id) = station
            && let Some(at) = self.system.absolute_position(Node::Station(id))
        {
            self.ship.set_position(at);
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
        } else {
            events.push(WorldEvent::Arrived { station });
        }
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

        let under_way = match &self.ship.state {
            ShipState::Travelling { plan, departed } => Some((plan.clone(), *departed)),
            _ => None,
        };
        match under_way {
            Some((plan, departed)) => {
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
            None => self.set_off(slot, target, events),
        }
    }

    /// An Abort. Unlike a redirect it leaves nothing pending.
    fn give_up(&mut self, slot: u32, events: &mut Vec<WorldEvent>) {
        let under_way = match &self.ship.state {
            ShipState::Travelling { plan, departed } => Some((plan.clone(), *departed)),
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
            other => self.system.absolute_position(other.node()?),
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
        if travelling && matches!(kind, PartKind::Engine | PartKind::Thruster) {
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

    /// Whether this player may give the ship an order.
    ///
    /// **Always true in this step**, and it is a function rather than nothing
    /// at all because it is the single place a later step will require that
    /// player's Bim to be standing at a Helm use spot. Confirm and Abort ask
    /// it; speed and trading deliberately do not.
    pub fn can_command(&self, slot: u32) -> bool {
        slot < self.speed_requests.len() as u32
    }

    // --- trading ------------------------------------------------------------

    fn buy(&mut self, slot: u32, resource: ResourceId, units: u32, events: &mut Vec<WorldEvent>) {
        if !matches!(self.ship.state, ShipState::Docked { .. }) {
            events.push(refused(slot, Refusal::NotDocked));
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
        // short somewhere there is nothing to buy.
        let aboard = self.ship.design.carrying(resource);
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
            ShipState::Docked { station } => Node::Station(*station),
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
    /// `web/ship.js` would be a second copy of [`data::STEP_MINUTES`] waiting
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

    /// Settle the local frame, for a probe that has just moved the ship.
    pub fn settle_frame_for_probe(&mut self) -> Vec<WorldEvent> {
        let mut events = Vec::new();
        self.settle_frame(&mut events);
        events
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

/// Where the **simulation** starts: the lowest star id with a station in its
/// system, and the lowest station id in that system.
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
        if let Some(station) = system.stations.iter().map(|s| s.id).min() {
            return Some((star.id, station));
        }
    }
    None
}
