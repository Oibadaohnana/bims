//! The crew: the room's simulation, aboard the ship.
//!
//! This is the fifth stage of [`crate::World::step`], and it is not a copy
//! of the room — it *is* the room. `bims::game::Game` is the Bims with
//! their needs and errands, the galley, the heads, the bay and the deck, and
//! `bims::aboard` lays one out from the accepted [`ShipDesign`] so that
//! every fixture is where the designer put it. One update per world step,
//! on the world's clock: the room's own `update` takes real seconds at 1x,
//! and a world step is exactly one sixtieth of one.
//!
//! # Where a Bim is
//!
//! In **design world units**, about the design's origin, `y` growing down
//! the grid — the coordinates the ship's own parts are in, so that the
//! ship's position, heading and acceleration do not reach them. The room
//! draws itself in those units too, and the ship painter turns the whole
//! picture with the hull.
//!
//! # Bim *i* at bunk *i*
//!
//! Bunks in id order, and ids only ever climb and are never reissued, so
//! the pairing is the same on every client and survives every edit that
//! did not touch the bunks. `bims::aboard::starts` is that rule.
//!
//! # What is not here yet
//!
//! The room has [`bims::room::BERTHS`] beds and as many seats, so at most
//! two of a crew are simulated — a Bim's index is its berth and its seat,
//! and the room has two of each. The cold store is stocked from the cargo
//! when the world opens and is the room's from then on; what the crew eat
//! and grow does not yet come back off the manifest.

use bims::game::Game as Room;
use shipdesign::ShipDesign;
use worldgen::math::{DVec2, dvec2};

use crate::data;

/// A world step, in the room's own unit: real seconds at 1x.
const STEP_SECONDS: f32 = (data::STEP_MINUTES / time::MINUTES_PER_SECOND) as f32;

/// The room aboard, and the crew in it.
pub struct Aboard {
    pub room: Room,
}

impl Aboard {
    /// The room laid out from `design`, with `crew` Bims at their bunks.
    pub fn new(design: &ShipDesign, crew: u32, seed: u64) -> Aboard {
        let mut room = bims::aboard::game_aboard(design, crew as usize, seed);
        // Drawn once before the first step, so the ship view has the room in
        // it from its first frame rather than from its first step.
        room.render();
        Aboard { room }
    }

    /// One step of the crew: what stage 5 does. The simulation only — the
    /// room is drawn by [`Aboard::render`] when the ship is, not every step.
    pub fn step(&mut self) {
        self.room.simulate(STEP_SECONDS);
    }

    /// Draw the room as it stands. Called by the ship painter once a frame.
    pub fn render(&mut self) {
        self.room.render();
    }

    /// How many are simulated. Not always the crew count — see the module
    /// note.
    pub fn count(&self) -> u32 {
        self.room.crew_count()
    }

    /// Where one of them is, in design world units.
    pub fn position(&self, who: u32) -> DVec2 {
        if who >= self.count() {
            return DVec2::ZERO;
        }
        let p = self.room.bim_pos(who as usize);
        dvec2(p.x as f64, p.y as f64)
    }

    /// The room's clock, in game minutes. It is the world's clock read
    /// through the room, and the two agreeing is what
    /// `the_crew_keep_the_world_s_clock` holds.
    pub fn minutes(&self) -> f64 {
        self.room.clock_minutes() as f64
    }
}

impl core::fmt::Debug for Aboard {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "Aboard({} crew, {:.1} min)",
            self.count(),
            self.minutes()
        )
    }
}
