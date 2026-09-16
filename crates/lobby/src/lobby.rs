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
        let checksum = worldgen::galaxy_checksum(&galaxy, &systems);
        let preview = Preview::new(width, height, &galaxy.stars);
        Lobby {
            galaxy,
            has_station,
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
