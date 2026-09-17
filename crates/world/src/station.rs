//! A station is a place: a hull on a grid, at a position, with a door.
//!
//! The world generator hands over a [`StationBlueprint`] — a kind, a
//! position and a `map_seed` — and this module turns it into a
//! [`ShipDesign`] the way the designer would have: the same parts, the same
//! rules, through [`shipdesign::apply`]. That buys three things at once.
//! The ship painter draws a station with the code it draws the ship with,
//! so a station sits in the same picture as the hull rather than beside it
//! as an icon. The room (`bims::aboard`) lays a station out exactly as it
//! lays a ship out, so the people living there are the room's Bims with the
//! room's needs and errands. And [`shipdesign::dock::port`] finds the
//! station's airlock the way it finds the ship's, which is what makes
//! docking airlock to airlock one piece of arithmetic asked twice.
//!
//! # Deterministic, and the same on both targets
//!
//! Everything here is drawn from the blueprint's `map_seed` through
//! `worldgen::rng`, in integers, and placed through `apply` — so a station
//! is the same station on a native server and in a browser, and the ship
//! docks in the same place on both to the unit.
//!
//! # A station does not turn
//!
//! Its heading is nought, always. North is up on every station's grid, so a
//! station's design coordinates go into the system through
//! [`flight::angle::rotate_design`] at zero: the flip between the grid's
//! y-down and the system's y-up and nothing else.
//!
//! # What a station is not, yet
//!
//! It is not a solid the ship cannot fly through. The ship docks *beside*
//! it — [`Station::berth`] puts the hull outside the station's, airlock to
//! airlock — and a trip to it ends outside its hull, but a trip *past* one
//! is a straight line whatever is in the way. Collision is the next step
//! and this is the shape it will need: a hull with a real extent, at a real
//! position.

use flight::angle;
use shipdesign::dock::{self, Port};
use shipdesign::parts::TILE;
use shipdesign::{Budget, Edit, Money, PartKind, Rotation, ShipDesign, apply};
use worldgen::math::{DVec2, dvec2};
use worldgen::rng::Rng;
use worldgen::system::StationBlueprint;
use worldgen::{Node, StarSystem, StationKind};

/// Layouts already built, by kind and seed. A layout is a pure function of
/// the two, and building one is two thousand edits through `apply`, each of
/// which rebuilds the grid — cheap enough once, and a world opens every
/// station of its system at once. Looked up only, never walked, so the
/// order in it decides nothing.
static BUILT: std::sync::Mutex<Vec<((StationKind, u64), ShipDesign)>> =
    std::sync::Mutex::new(Vec::new());

/// How many tiles across a station's build area is, by kind. The hull fills
/// it bar a one-tile margin. Big next to a ship — the playtest ship is twenty
/// — because a station is where ships go, not a ship.
pub fn side_of(kind: StationKind) -> u32 {
    match kind {
        StationKind::Orbital => 40,
        StationKind::Refinery => 36,
        StationKind::MiningOutpost => 32,
        StationKind::Derelict => 34,
        StationKind::Relay => 26,
    }
}

/// How many people live aboard, by kind. The room simulates at most
/// [`bims::room::BERTHS`], a relay is a lonely posting, and nobody is left
/// on a derelict — which is why one never gets a room at all.
pub fn residents_of(kind: StationKind) -> u32 {
    match kind {
        StationKind::Derelict => 0,
        StationKind::Relay => 1,
        _ => bims::room::BERTHS as u32,
    }
}

/// Where a ship goes to be docked at a station: its centre of mass and its
/// heading, with the two airlocks' outer faces touching.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Berth {
    pub position: DVec2,
    pub heading: f64,
}

/// One station of the system, as a place.
#[derive(Clone, PartialEq, Debug)]
pub struct Station {
    pub id: u32,
    pub kind: StationKind,
    /// The layout: what the painter draws, what the room lays out, and what
    /// the port is found in.
    pub design: ShipDesign,
    /// Where design tile (0, 0) sits in the system. The grid's centre is on
    /// the blueprint's position.
    pub anchor: DVec2,
    /// The seed the residents' room is opened with.
    pub map_seed: u64,
}

impl Station {
    /// Build the station the blueprint describes, standing at `at`.
    pub fn build(blueprint: &StationBlueprint, at: DVec2) -> Station {
        let design = layout(blueprint.kind, blueprint.map_seed);
        let half = design.build_area as f64 * TILE as f64 / 2.0;
        Station {
            id: blueprint.id,
            kind: blueprint.kind,
            anchor: at.sub(angle::rotate_design(dvec2(half, half), 0.0)),
            design,
            map_seed: blueprint.map_seed,
        }
    }

    /// Every station of a system, in id order.
    pub fn all_of(system: &StarSystem) -> Vec<Station> {
        system
            .stations
            .iter()
            .filter_map(|blueprint| {
                let at = system.absolute_position(Node::Station(blueprint.id))?;
                Some(Station::build(blueprint, at))
            })
            .collect()
    }

    pub fn residents(&self) -> u32 {
        residents_of(self.kind)
    }

    /// A design point — world units about the grid's origin, `y` down — in
    /// the system.
    pub fn to_system(&self, design: DVec2) -> DVec2 {
        self.anchor.add(angle::rotate_design(design, 0.0))
    }

    /// The middle of the grid, in the system: the blueprint's position.
    pub fn centre(&self) -> DVec2 {
        let half = self.design.build_area as f64 * TILE as f64 / 2.0;
        self.to_system(dvec2(half, half))
    }

    /// How far from the centre the hull reaches, at most: the half-diagonal
    /// of the grid. A circle the hull is certainly inside, for "how near is
    /// the ship to the station" without walking the tiles.
    pub fn radius(&self) -> f64 {
        self.design.build_area as f64 * TILE as f64 * core::f64::consts::FRAC_1_SQRT_2
    }

    /// How far a point is from the station's hull, at least: distance to the
    /// centre less the radius, and never negative.
    pub fn clearance(&self, from: DVec2) -> f64 {
        (from.distance(self.centre()) - self.radius()).max(0.0)
    }

    pub fn port(&self) -> Option<Port> {
        dock::port(&self.design)
    }

    /// The outer face of the station's airlock, in the system, and the way
    /// it opens, as a unit vector. The way out for a ship pushing off.
    pub fn face(&self) -> Option<(DVec2, DVec2)> {
        let port = self.port()?;
        let (fx, fy) = port.face();
        let outward = dvec2(port.outward.0 as f64, port.outward.1 as f64);
        Some((
            self.to_system(dvec2(fx, fy)),
            angle::rotate_design(outward, 0.0),
        ))
    }

    /// Where `ship` docks: its centre of mass and heading with its airlock's
    /// outer face on the station's, opening the other way.
    ///
    /// A ship with no port cannot dock, and gets a berth all the same — held
    /// off the station's door by its own size, pointing north — because a
    /// world still opens with a ship alongside whether or not it can go
    /// aboard, and "beside the door" is where alongside is.
    pub fn berth(&self, ship: &ShipDesign, centre_of_mass: DVec2) -> Option<Berth> {
        let (face, outward) = self.face()?;
        let Some(port) = dock::port(ship) else {
            let reach = ship.build_area as f64 * TILE as f64 * core::f64::consts::FRAC_1_SQRT_2;
            return Some(Berth {
                position: face.add(outward.scale(reach + TILE as f64)),
                heading: 0.0,
            });
        };
        // The ship's door has to open the way the station's does not. Its
        // outward step has a bearing at heading nought; the heading is what
        // turns that bearing onto the opposite of the station's.
        let step = dvec2(port.outward.0 as f64, port.outward.1 as f64);
        let at_zero = angle::bearing(angle::rotate_design(step, 0.0));
        let heading = angle::wrap(angle::bearing(outward.scale(-1.0)) - at_zero);
        // Then the ship is placed so that its door's outer face lands on the
        // station's: the face is a design offset from the centre of mass,
        // turned through that heading.
        let (fx, fy) = port.face();
        let offset = angle::rotate_design(dvec2(fx, fy).sub(centre_of_mass), heading);
        Some(Berth {
            position: face.sub(offset),
            heading,
        })
    }
}

// --- the layout ---------------------------------------------------------------

/// The station's design, from its kind and its seed.
///
/// One plan for all of them, sized by kind and dressed by the seed. The
/// hull is a square with its corners cut back three tiles in
/// [`PartKind::DiagonalOutsideWall`] pieces, the port in the west skin and
/// the array on the north. Inside, two corridors three tiles wide cross in
/// the middle — the west one runs in from the port — and the four
/// quarters between them are rooms: the galley and mess to the north-west,
/// the crew's quarters with the heads along their north wall to the
/// north-east, hydroponics to the south-west, engineering to the
/// south-east. Every room has a doorway onto each corridor, two tiles
/// wide, and every fixture stands with two clear tiles in front of it,
/// because the room's navigation cannot walk a one-tile gap (see
/// `crates/shipdesign`'s module note) — a bulkhead here is only ever
/// where a body can still get round it with a tile to spare. A derelict
/// is the same hull with holes in it and nobody home.
///
/// The seed decides how many bays, shelves and batteries there are and
/// nothing else about the shape, so two seeds are two stations without
/// either being a different building.
pub fn layout(kind: StationKind, map_seed: u64) -> ShipDesign {
    if let Ok(built) = BUILT.lock()
        && let Some((_, design)) = built.iter().find(|(key, _)| *key == (kind, map_seed))
    {
        return design.clone();
    }
    let design = build_layout(kind, map_seed);
    if let Ok(mut built) = BUILT.lock() {
        built.push(((kind, map_seed), design.clone()));
    }
    design
}

/// How many tiles each corner of the hull is cut back by.
const CHAMFER: u32 = 3;

/// Whether a tile of a station's grid is on the frame, and if it is on a
/// cut, which way the corner piece there faces: `None` off the station,
/// `Some(None)` a plain tile, `Some(Some(r))` a corner piece turned `r`.
/// The solid half of each piece faces the middle of the station, which is
/// what seals the corner.
fn outline(side: u32, x: u32, y: u32) -> Option<Option<Rotation>> {
    let last = side - 2;
    if x < 1 || x > last || y < 1 || y > last {
        return None;
    }
    let corners = [
        ((x - 1) + (y - 1), Rotation::R270),
        ((last - x) + (y - 1), Rotation::R0),
        ((x - 1) + (last - y), Rotation::R180),
        ((last - x) + (last - y), Rotation::R90),
    ];
    for (distance, turn) in corners {
        if distance < CHAMFER {
            return None;
        }
        if distance == CHAMFER {
            return Some(Some(turn));
        }
    }
    Some(None)
}

fn build_layout(kind: StationKind, map_seed: u64) -> ShipDesign {
    let side = side_of(kind);
    let budget = Budget::new(Money::MAX);
    let mut design = ShipDesign::new(side);
    let mut rng = Rng::new(map_seed);

    let put = |design: &mut ShipDesign, kind: PartKind, origin: (u32, u32), rotation| {
        if let Ok(next) = apply(
            design,
            &budget,
            Edit::Place {
                kind,
                origin,
                rotation,
            },
        ) {
            *design = next;
        }
    };
    let take = |design: &mut ShipDesign, tile: (u32, u32)| {
        let standing = design
            .grid()
            .get(shipdesign::Layer::Object, (tile.0 as i32, tile.1 as i32));
        if standing != 0
            && let Ok(next) = apply(design, &budget, Edit::Remove { part_id: standing })
        {
            *design = next;
        }
    };

    // Frame, deck and skin over the outline: plating along the straight
    // runs, corner pieces on the cuts, deck inside.
    let last = side - 2;
    for y in 1..=last {
        for x in 1..=last {
            let Some(cut) = outline(side, x, y) else {
                continue;
            };
            put(&mut design, PartKind::Structure, (x, y), Rotation::R0);
            let skin = x == 1 || x == last || y == 1 || y == last;
            match cut {
                Some(turn) => put(&mut design, PartKind::DiagonalOutsideWall, (x, y), turn),
                None if skin => put(&mut design, PartKind::OutsideWall, (x, y), Rotation::R0),
                None => put(&mut design, PartKind::Floor, (x, y), Rotation::R0),
            }
        }
    }

    // The port: two tiles of the west skin, decked, with the airlock on
    // them. The array in the north skin.
    let mid = side / 2;
    for tile in [(1, mid - 1), (1, mid)] {
        take(&mut design, tile);
        put(&mut design, PartKind::Floor, tile, Rotation::R0);
    }
    put(&mut design, PartKind::Airlock, (1, mid - 1), Rotation::R0);
    take(&mut design, (mid, 1));
    put(&mut design, PartKind::SensorArray, (mid, 1), Rotation::R0);

    // The bulkheads: the walls either side of the two corridors, with the
    // crossing left open and a two-tile doorway into each room from each
    // corridor. The doorways sit where the room meets the crossing, except
    // hydroponics' north one, which is at the west end so the bays can run
    // along that wall.
    let (near, far) = (mid - 2, mid + 2);
    let doors: [((u32, u32), (u32, u32)); 8] = [
        // (from, to) inclusive, each a run of two tiles in a wall.
        ((near, mid - 4), (near, mid - 3)), // galley, east wall
        ((mid - 4, near), (mid - 3, near)), // galley, south wall
        ((far, mid - 4), (far, mid - 3)),   // quarters, west wall
        ((mid + 3, near), (mid + 4, near)), // quarters, south wall
        ((near, mid + 3), (near, mid + 4)), // hydroponics, east wall
        ((2, far), (3, far)),               // hydroponics, north wall
        ((far, mid + 3), (far, mid + 4)),   // engineering, west wall
        ((mid + 3, far), (mid + 4, far)),   // engineering, north wall
    ];
    let in_door = |x: u32, y: u32| {
        doors
            .iter()
            .any(|&((x0, y0), (x1, y1))| x >= x0 && x <= x1 && y >= y0 && y <= y1)
    };
    for y in 2..last {
        for x in 2..last {
            let on_row = (y == near || y == far) && !(x >= mid - 1 && x <= mid + 1);
            let on_column = (x == near || x == far) && !(y >= mid - 1 && y <= mid + 1);
            if !(on_row || on_column) || outline(side, x, y) != Some(None) || in_door(x, y) {
                continue;
            }
            put(&mut design, PartKind::Wall, (x, y), Rotation::R0);
        }
    }
    // Each doorway is one door, two tiles along its wall: turned for a run
    // along a row, upright for one down a column.
    for &((x0, y0), (x1, _)) in &doors {
        let rotation = if x1 > x0 { Rotation::R90 } else { Rotation::R0 };
        put(&mut design, PartKind::Door, (x0, y0), rotation);
    }

    // The galley and mess, north-west: the galley along the north wall
    // from where the cut ends, worked from the row below; a table with a
    // chair a side under it, and a second table in a room deep enough.
    let (x0, y0, y1) = (2u32, 2u32, mid - 3);
    for (kind, x) in [
        (PartKind::ColdStore, x0 + 2),
        (PartKind::Worktop, x0 + 3),
        (PartKind::Hob, x0 + 5),
        (PartKind::Dishwasher, x0 + 6),
    ] {
        put(&mut design, kind, (x, y0), Rotation::R0);
    }
    let mut table_y = y0 + 4;
    while table_y + 1 <= y1 - 2 {
        put(
            &mut design,
            PartKind::Table,
            (x0 + 3, table_y),
            Rotation::R0,
        );
        put(
            &mut design,
            PartKind::Chair,
            (x0 + 3, table_y + 1),
            Rotation::R0,
        );
        put(
            &mut design,
            PartKind::Chair,
            (x0 + 4, table_y + 1),
            Rotation::R0,
        );
        table_y += 4;
        if table_y + 1 > y1 - 2 || y1 - y0 < 12 {
            break;
        }
    }

    // The quarters, north-east: the heads along the north wall, from the
    // corner nearest the corridor, and the bunks down the east skin, each
    // used from the tile west of it.
    let (x0, y0, x1, y1) = (mid + 3, 2u32, last - 1, mid - 3);
    for (kind, x) in [
        (PartKind::Toilet, x0),
        (PartKind::Basin, x0 + 1),
        (PartKind::Shower, x0 + 2),
    ] {
        put(&mut design, kind, (x, y0), Rotation::R0);
    }
    let mut bunk_y = y0 + 3;
    while bunk_y + 1 <= y1 {
        put(&mut design, PartKind::Bunk, (x1, bunk_y), Rotation::R0);
        bunk_y += 3;
    }

    // Hydroponics, south-west: runs of six trays across the room, one
    // every three rows from two below the north wall — so the row a run is
    // worked from and the row behind it are clear — as many as the seed
    // likes and the room holds, more on a bigger station, which feeds
    // more, and side by side where the room is wide enough for two. The
    // runs start three tiles in from the west wall, and the broom locker
    // stands against that wall under the doorway with the two-tile gangway
    // between it and the runs: a run ending diagonally against the locker
    // pinched the tile it is worked from to a sliver, which is the
    // corner-to-corner rule again.
    let (x0, y0, x1, y1) = (2u32, mid + 3, mid - 3, last - 1);
    let bays = 1 + rng.below(3) + (side - 26) / 7;
    let columns = ((x1 - x0 - 1) / 7).max(1);
    for i in 0..bays {
        let (column, row) = (i % columns, i / columns);
        let at = (x0 + 3 + column * 7, y0 + 2 + row * 3);
        // Three rows off the south wall, not two: the corner piece of the
        // chamfer and the end of a run two tiles from it diagonally leave
        // the tile between them a sliver, and the rows under the run were
        // deck nobody could get to.
        if at.1 + 3 > y1 {
            break;
        }
        put(&mut design, PartKind::HydroBay, at, Rotation::R0);
    }
    put(
        &mut design,
        PartKind::BroomLocker,
        (x0, y0 + 2),
        Rotation::R0,
    );

    // Engineering, south-east: the reactor and life support along the
    // north wall clear of the doorways, with the batteries in the slot
    // between them; the tank down the west wall, far enough below the
    // doorways that the way past the reactor's corner is two tiles wide —
    // two solids a tile apart corner to corner leave a diagonal gap the
    // navigation will not squeeze through, and the whole room was cut off
    // by exactly that once; and shelves stood two tiles off the south wall
    // so they are worked from a gangway.
    let (x0, y0, x1, y1) = (mid + 3, mid + 3, last - 1, last - 1);
    put(&mut design, PartKind::Reactor, (x0 + 3, y0), Rotation::R0);
    put(
        &mut design,
        PartKind::LifeSupport,
        (x0 + 6, y0),
        Rotation::R0,
    );
    let batteries = rng.below(3);
    for i in 0..batteries {
        put(
            &mut design,
            PartKind::Battery,
            (x0 + 5, y0 + i),
            Rotation::R0,
        );
    }
    // A big station runs a second reactor, beyond life support.
    if x0 + 10 <= x1 {
        put(&mut design, PartKind::Reactor, (x0 + 9, y0), Rotation::R0);
    }
    put(&mut design, PartKind::FuelTank, (x0, y0 + 4), Rotation::R0);
    let shelves = 2 + rng.below(4) + (side - 26) / 5;
    let mut shelf_x = x0 + 3;
    let mut shelf_y = y1 - 2;
    for _ in 0..shelves {
        if shelf_x > last - 3 {
            // A second row, four tiles up, in a room deep enough for the
            // gangway between the two.
            if shelf_y != y1 - 2 || y1 - y0 < 12 {
                break;
            }
            shelf_x = x0 + 3;
            shelf_y = y1 - 6;
        }
        put(
            &mut design,
            PartKind::Shelf,
            (shelf_x, shelf_y),
            Rotation::R0,
        );
        shelf_x += 2;
    }

    // A derelict has lost some of its skin — not the port, and not the
    // array, which is what a passing ship still picks up. A roll that
    // lands on a corner the cut took off takes nothing, and is still a
    // roll, so the stream stays in step.
    if kind == StationKind::Derelict {
        let holes = 6 + rng.below(6);
        for _ in 0..holes {
            let along = 2 + rng.below(last - 3);
            let tile = match rng.below(4) {
                0 => (along, 1),
                1 => (along, last),
                2 => (1, along),
                _ => (last, along),
            };
            if tile == (1, mid - 1) || tile == (1, mid) || tile == (mid, 1) {
                continue;
            }
            take(&mut design, tile);
        }
    }

    design
}
