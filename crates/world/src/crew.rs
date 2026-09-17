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

use bims::bim::Bim;
use bims::character::Uniform;
use bims::game::Game as Room;
use bims::math::vec2;
use shipdesign::parts::{Layer, TILE};
use shipdesign::{ShipDesign, dock};
use worldgen::math::{DVec2, dvec2};

use crate::data;
use crate::docking::Joined;

/// A world step, in the room's own unit: real seconds at 1x.
const STEP_SECONDS: f32 = (data::STEP_MINUTES / time::MINUTES_PER_SECOND) as f32;

/// The room aboard, and the crew in it.
///
/// Docked, it is the ship **and the station** as one room — see
/// [`crate::docking`] — with the ship's crew first and the station's
/// residents after them, and the ship's own grid sitting `offset` into the
/// room's. Everything that reads a position through here gets it in the
/// **ship's** frame, whichever room it is; only the painter and the pointer
/// need the offset, to put the room's picture and the room's coordinates
/// where the ship is.
pub struct Aboard {
    pub room: Room,
    /// Where the ship's design origin sits in the room's grid, in design
    /// units. Nought for a ship on its own.
    pub offset: DVec2,
    /// How many of the room's Bims are the ship's crew. Indices below it
    /// are the crew, from it the residents of the station docked to.
    pub crew: u32,
    /// The design the room is laid out on: the ship's, or the joined one.
    pub design: ShipDesign,
    /// Where the two sides of the airlock lead, in the room's units, while
    /// the rooms are joined: the corridor just inside the station's door,
    /// and the deck just inside the ship's. Where the station's people are
    /// sent before the ship casts off, and where its own are called back
    /// to. Neither for a ship on its own.
    pub ashore: Option<DVec2>,
    pub gangway: Option<DVec2>,
}

impl Aboard {
    /// The room laid out from `design`, with `crew` Bims at their bunks.
    pub fn new(design: &ShipDesign, crew: u32, seed: u64) -> Aboard {
        let mut room = bims::aboard::game_aboard(design, crew as usize, seed);
        // Drawn once before the first step, so the ship view has the room in
        // it from its first frame rather than from its first step.
        room.render();
        let crew = room_crew(&room);
        Aboard {
            room,
            offset: DVec2::ZERO,
            crew,
            design: design.clone(),
            ashore: None,
            gangway: None,
        }
    }

    /// The ship and its station as one room, with the ship's crew and the
    /// station's residents in it — each where they were standing in their
    /// own room, now in the joined one. Both crews come from
    /// `Game::take_crew`, so nobody is mid-errand: a chain aimed at a
    /// fixture in a room that no longer exists is not a chain worth keeping.
    pub fn joined(
        joined: Joined,
        ship: &ShipDesign,
        station: &ShipDesign,
        crew: Vec<Bim>,
        residents: Vec<Bim>,
        seed: u64,
        minutes: f64,
    ) -> Aboard {
        let layout = bims::aboard::layout_of(&joined.design);
        let (w, h) = (layout.bounds.width(), layout.bounds.height());
        let mut room = Room::with_layout(layout, seed, &[], w, h);
        let shift = joined.ship_shift();
        // Either side of the passage: a few tiles in from each door, along
        // the way it opens, read backwards.
        let inside = |port: dock::Port| {
            let reach = crate::data::ASHORE_TILES * TILE as f64;
            dvec2(
                port.centre.0 - port.outward.0 as f64 * reach,
                port.centre.1 - port.outward.1 as f64 * reach,
            )
        };
        let gangway = dock::port(ship).map(|p| inside(p).add(shift));
        let ashore = dock::port(station).map(|p| joined.from_station(inside(p)));
        room.adopt(crew, vec2(shift.x as f32, shift.y as f32));
        let crew = room_crew(&room);
        // A resident stood at a station point `p` now stands at the image
        // of `p`; `adopt` takes one shift, so residents are moved to the
        // joined frame first, one by one, and shifted by nothing.
        let moved: Vec<Bim> = residents
            .into_iter()
            .map(|mut bim| {
                let p = bim.character.pos;
                let at = joined.from_station(dvec2(p.x as f64, p.y as f64));
                bim.character.stand_at(vec2(at.x as f32, at.y as f32));
                bim
            })
            .collect();
        room.adopt(moved, bims::math::Vec2::ZERO);
        room.wind_clock(minutes as f32);
        room.render();
        Aboard {
            room,
            offset: shift,
            crew,
            design: joined.design,
            ashore,
            gangway,
        }
    }

    /// Take the room apart again: the ship's crew back into a room of the
    /// ship alone, and the residents handed back to whoever wants them.
    /// Anybody of the crew still on the station's deck is stood at their
    /// bunk — `adopt` does that for a position the new room has no floor
    /// under — which is the ship leaving without waiting.
    pub fn unjoined(self, ship: &ShipDesign, seed: u64, minutes: f64) -> (Aboard, Vec<Bim>) {
        let crew_count = self.crew as usize;
        let offset = self.offset;
        let mut everybody = self.room.take_crew();
        let residents = everybody.split_off(crew_count.min(everybody.len()));
        let layout = bims::aboard::layout_of(ship);
        let (w, h) = (layout.bounds.width(), layout.bounds.height());
        let mut room = Room::with_layout(layout, seed, &[], w, h);
        room.adopt(everybody, vec2(-offset.x as f32, -offset.y as f32));
        room.wind_clock(minutes as f32);
        room.render();
        let crew = room_crew(&room);
        (
            Aboard {
                room,
                offset: DVec2::ZERO,
                crew,
                design: ship.clone(),
                ashore: None,
                gangway: None,
            },
            residents,
        )
    }

    /// Whether one of them is on the ship: standing on a tile of the
    /// ship's own frame, or in the passage between the two collars. Asked
    /// of the ship's design rather than the joined one, because the joined
    /// one is the station too.
    pub fn on_ship(&self, who: u32, ship: &ShipDesign) -> bool {
        let p = self.position(who);
        let t = TILE as f64;
        let tile = ((p.x / t).floor() as i32, (p.y / t).floor() as i32);
        if ship.grid().get(Layer::Structure, tile) != 0 {
            return true;
        }
        // The passage: the tile beyond each airlock tile, the way it opens.
        dock::port(ship)
            .and_then(|port| ship.part(port.part_id).map(|a| (port, a.tiles())))
            .is_some_and(|(port, tiles)| {
                tiles
                    .iter()
                    .any(|&(x, y)| (x as i32 + port.outward.0, y as i32 + port.outward.1) == tile)
            })
    }

    /// Everybody to their own side of the airlock: the station's people
    /// ashore, the ship's back aboard. Sent once each, and again only when
    /// an errand has taken one back across — a Bim handed a fresh route
    /// every step never moves. The station's people are *posted* ashore,
    /// since they are let go with the station anyway; the crew are only
    /// walked back, or they would stand at the airlock for the rest of the
    /// voyage. Whether they are all there yet is [`Aboard::everybody_home`];
    /// nothing here waits.
    pub fn send_everybody_home(&mut self, ship: &ShipDesign) {
        let near = |a: Option<bims::math::Vec2>, b: bims::math::Vec2| {
            a.is_some_and(|a| (a - b).len() <= TILE as f32)
        };
        for who in 0..self.count() {
            let crew = who < self.crew;
            let on_ship = self.on_ship(who, ship);
            let (belongs, to) = if crew {
                (on_ship, self.gangway)
            } else {
                (!on_ship, self.ashore)
            };
            let Some(to) = to else {
                continue;
            };
            let to = vec2(to.x as f32, to.y as f32);
            if belongs {
                continue;
            }
            let who = who as usize;
            if crew {
                // Already on the way: leave it be.
                if near(self.room.destination_for_probe(who), to) {
                    continue;
                }
                self.room.walk_to(who, to);
            } else {
                // Posted there already — to within the snap a route makes —
                // and on its way or standing: leave it be.
                if near(self.room.post_of(who), to) && !self.room.is_busy(who) {
                    continue;
                }
                self.room.send_to(who, to);
            }
        }
    }

    /// Whether the crew are all on the ship and the station's people all
    /// off it. True of a ship on its own.
    pub fn everybody_home(&self, ship: &ShipDesign) -> bool {
        (0..self.count()).all(|who| self.on_ship(who, ship) == (who < self.crew))
    }

    /// Whether this is the ship and a station as one room.
    pub fn is_joined(&self) -> bool {
        self.offset != DVec2::ZERO || self.count() > self.crew
    }

    /// The ship's own crew: the first `crew` of the room's Bims.
    pub fn crew_count(&self) -> u32 {
        self.crew
    }

    /// The station's residents aboard the joined room, if it is one.
    pub fn resident_count(&self) -> u32 {
        self.count().saturating_sub(self.crew)
    }

    /// One step of the crew: what stage 5 does. The simulation only — the
    /// room is drawn by [`Aboard::render`] when the ship is, not every step.
    pub fn step(&mut self) {
        self.room.simulate(STEP_SECONDS);
    }

    /// Tell the room where the helm is while the ship wants somebody at it,
    /// in the ship's design units, or that it does not. The room offers the
    /// helm as a job off this — see `bims::work::Job::Helm`.
    pub fn set_helm(&mut self, seat: Option<DVec2>) {
        self.room.set_helm(seat.map(|s| {
            let at = s.add(self.offset);
            vec2(at.x as f32, at.y as f32)
        }));
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

    /// Where one of them is, in the **ship's** design world units — the
    /// room's, less the ship's offset into it.
    pub fn position(&self, who: u32) -> DVec2 {
        if who >= self.count() {
            return DVec2::ZERO;
        }
        let p = self.room.bim_pos(who as usize);
        dvec2(p.x as f64, p.y as f64).sub(self.offset)
    }

    /// Whether one of them is standing on a deck tile of the room's design.
    pub fn on_deck(&self, who: u32) -> bool {
        let p = self.room.bim_pos(who as usize);
        let t = shipdesign::TILE as f32;
        let tile = ((p.x / t).floor() as i32, (p.y / t).floor() as i32);
        self.design.grid().get(shipdesign::Layer::Floor, tile) != 0
    }

    /// The room's clock, in game minutes. It is the world's clock read
    /// through the room, and the two agreeing is what
    /// `the_crew_keep_the_world_s_clock` holds.
    pub fn minutes(&self) -> f64 {
        self.room.clock_minutes() as f64
    }
}

fn room_crew(room: &Room) -> u32 {
    room.crew_count()
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

/// The people living on a station: the room again, laid out on the
/// station's design, opened when the ship comes near and closed when it
/// leaves. See `World::settle_residents`.
///
/// Opened rather than kept: a station's room is not simulated while nobody
/// is there to see it, and when the ship comes back the residents start
/// afresh at their bunks. What they were doing an hour ago is not state the
/// world carries — it is the honest limit of this step, and it is why a
/// derelict, with nobody aboard, never gets a room at all.
pub struct Residents {
    pub station: u32,
    pub aboard: Aboard,
}

impl Residents {
    /// The station's room, with its people at their bunks and its clock
    /// wound on to the world's, so a station reached at noon is not at
    /// breakfast.
    pub fn open(
        station: u32,
        design: &ShipDesign,
        count: u32,
        seed: u64,
        minutes: f64,
    ) -> Residents {
        let mut aboard = Aboard::new(design, count, seed);
        aboard.room.wind_clock(minutes as f32);
        // The station's coverall, so that on a joined deck who is going
        // ashore can be told from who is staying.
        for who in 0..aboard.count() {
            aboard.room.set_uniform(who as usize, Uniform::Station);
        }
        aboard.room.render();
        Residents { station, aboard }
    }

    /// Everybody in it, out of it: for joining them to the ship's room.
    pub fn take(self) -> Vec<Bim> {
        self.aboard.room.take_crew()
    }
}

impl core::fmt::Debug for Residents {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Residents(station {}, {:?})", self.station, self.aboard)
    }
}
