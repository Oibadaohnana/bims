//! What the World tab holds: one galaxy, every system in it, the star being
//! looked at, and the marks the page has asked for.
//!
//! # "Has a station" is answered by generating the system
//!
//! [`worldgen::Galaxy::designation_for`] only knows about the five stars
//! *promised* a station. A quarter of the rest roll one on their own, and a
//! promised one can still lose it — a station with nowhere in its own system
//! to fly to is pruned. So the only way to answer "which stars can the game
//! start at" that agrees with what [`worldgen::Galaxy::system`] will actually
//! build is to build them, once per galaxy, and keep the answer. A thousand
//! systems is a few dozen draws each; it is done once, when the seed or the
//! type changes, and never again.
//!
//! The inspected system is **not** read from that cache. It is generated
//! afresh from `Galaxy::system`, which costs nothing and means the harness's
//! comparison of "reported as having a station" against "what inspecting it
//! lists" is a comparison of two paths rather than of one path with itself.
//!
//! # A crew cannot start at an enemy's
//!
//! Some of the stations somebody lives on are hostile
//! (`StationBlueprint::hostile`): docked there the crew are the enemy and
//! the people aboard shoot. The map still shows the star as having a
//! station — it does, and it is somewhere to fly to — but the start has to
//! be somewhere else, so `can_start` is kept beside `has_station`, and the
//! two ways a start gets picked ([`Lobby::random_start`] and
//! [`Lobby::can_start_at`], which the page asks before it offers "Start
//! here") both refuse a hostile station. The rule is here rather than on the
//! page so that a page and a server agree about it.

use worldgen::{Galaxy, GalaxyType, StarSystem};

use crate::diagram::{self, Placed};
use crate::draw::DrawList;
use crate::preview::{self, Marks, Ping, Preview};

/// At most this many pings on screen. A suggestion a second from four
/// guests is still legible; a hundred would be a screen of rings.
const MAX_PINGS: usize = 8;

pub struct Lobby {
    pub galaxy: Galaxy,
    /// Indexed by star id.
    pub has_station: Vec<bool>,
    /// Indexed by star id: has a station a crew can start at — one that is
    /// not hostile. A subset of `has_station`.
    pub can_start: Vec<bool>,
    pub checksum: u64,
    pub preview: Preview,
    pub hovered: Option<u32>,
    /// The pending start, as the page last said: a star and a station in it.
    pub spawn: Option<(u32, u32)>,
    pub pings: Vec<Ping>,
    /// The star whose system is in the side panel, and the system itself.
    pub inspected: Option<(u32, StarSystem)>,
    /// Where the last diagram put everything, for the host's labels.
    pub placed: Placed,
}

impl Lobby {
    pub fn new(seed: u64, galaxy_type: GalaxyType, width: f32, height: f32) -> Lobby {
        let galaxy = Galaxy::new(seed, galaxy_type);
        let systems = galaxy.every_system();
        let has_station = systems.iter().map(|s| !s.stations.is_empty()).collect();
        let can_start = systems
            .iter()
            .map(|s| s.stations.iter().any(|st| !st.hostile))
            .collect();
        let checksum = worldgen::galaxy_checksum(&galaxy, &systems);
        let preview = Preview::new(width, height, &galaxy.stars);
        Lobby {
            galaxy,
            has_station,
            can_start,
            checksum,
            preview,
            hovered: None,
            spawn: None,
            pings: Vec::new(),
            inspected: None,
            placed: Placed::default(),
        }
    }

    /// A different seed or type is a different galaxy; the same pair is the
    /// one already here, and nothing moves. Returns whether it changed.
    ///
    /// Everything else goes with the old galaxy: the spawn, the pings and the
    /// inspected system all name a star of it, and the camera is fitted
    /// again because a new galaxy is a new map, not a rearranged one.
    pub fn set_world(&mut self, seed: u64, galaxy_type: GalaxyType) -> bool {
        if self.galaxy.seed == seed && self.galaxy.galaxy_type == galaxy_type {
            return false;
        }
        *self = Lobby::new(seed, galaxy_type, self.preview.width, self.preview.height);
        true
    }

    pub fn hover(&mut self, x: f32, y: f32) {
        self.hovered = self.preview.pick(&self.galaxy.stars, x, y);
    }

    /// Open a star's system in the side panel. A star that is not in this
    /// galaxy closes it.
    pub fn inspect(&mut self, star: u32) {
        self.inspected = self.galaxy.system(star).map(|s| (star, s));
    }

    /// Whether a station of the inspected system is somebody else's: ringed
    /// in red on the diagram, and not somewhere a crew can start. `false`
    /// when nothing is inspected or the id is not a station of it.
    pub fn station_hostile(&self, station: u32) -> bool {
        self.inspected
            .as_ref()
            .and_then(|(_, system)| system.station(station))
            .is_some_and(|s| s.hostile)
    }

    /// Whether the game may start at this station of this star: it exists,
    /// and it is not hostile. The page asks before it offers "Start here",
    /// and a start handed in from elsewhere is checked the same way.
    ///
    /// Generates the system rather than reading the inspected one, because
    /// the star asked about is not always the one open in the panel.
    pub fn can_start_at(&self, star: u32, station: u32) -> bool {
        self.galaxy
            .system(star)
            .and_then(|s| s.station(station).map(|st| !st.hostile))
            .unwrap_or(false)
    }

    /// A random start: a star that can be started at, and one of its
    /// stations that is not hostile. `roll` is the page's own randomness —
    /// the star off the low word, the station off the high one, which is
    /// how the page picked before the rule moved here. `None` only in a
    /// galaxy with nowhere to start at all.
    pub fn random_start(&self, roll: u64) -> Option<(u32, u32)> {
        let stars: Vec<u32> = (0..self.galaxy.stars.len() as u32)
            .filter(|&s| self.can_start.get(s as usize).copied().unwrap_or(false))
            .collect();
        if stars.is_empty() {
            return None;
        }
        let star = stars[(roll % stars.len() as u64) as usize];
        let system = self.galaxy.system(star)?;
        let open: Vec<u32> = system
            .stations
            .iter()
            .filter(|st| !st.hostile)
            .map(|st| st.id)
            .collect();
        if open.is_empty() {
            return None;
        }
        Some((star, open[((roll >> 32) % open.len() as u64) as usize]))
    }

    pub fn ping(&mut self, star: u32) {
        if self.galaxy.star(star).is_none() {
            return;
        }
        if self.pings.len() >= MAX_PINGS {
            self.pings.remove(0);
        }
        self.pings.push(Ping { star, age: 0.0 });
    }

    /// Let `dt` seconds go by. Only the pings care.
    pub fn advance(&mut self, dt: f32) {
        if !(dt > 0.0) || !dt.is_finite() {
            return;
        }
        for ping in &mut self.pings {
            ping.age += dt;
        }
        self.pings.retain(|p| p.age < preview::PING_SECONDS);
    }

    /// Whether anything is still moving, so the host can leave the canvas
    /// alone between pointer events when nothing is.
    pub fn animating(&self) -> bool {
        !self.pings.is_empty()
    }

    pub fn paint(&self, list: &mut DrawList) {
        let marks = Marks {
            hovered: self.hovered,
            spawn: self.spawn.map(|(star, _)| star),
            pings: &self.pings,
        };
        preview::paint(
            &self.preview,
            &self.galaxy.stars,
            &self.has_station,
            &marks,
            list,
        );
    }

    /// Paint the inspected system into `list`, fitted to a panel of this
    /// size, and remember where everything landed.
    pub fn paint_system(&mut self, width: f32, height: f32, list: &mut DrawList) {
        let Some((star, system)) = &self.inspected else {
            list.clear();
            self.placed = Placed::default();
            return;
        };
        let spawn = match self.spawn {
            Some((s, station)) if s == *star => Some(station),
            _ => None,
        };
        self.placed = diagram::paint(system, spawn, width, height, list);
    }
}
