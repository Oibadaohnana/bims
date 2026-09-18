//! A docked ship and its station as **one grid**, so that one room — one
//! deck, one navigation grid — holds both and the crew can walk through
//! the airlocks from one to the other.
//!
//! The ship stays at its own coordinates, shifted by a whole number of
//! tiles so nothing is negative; the station is turned into the ship's
//! frame by however many quarter turns the berth put between them and
//! shifted likewise. Both are laid down again through `shipdesign::apply`,
//! so the joined design is one the rules admit — and the one tile of open
//! space between the two hulls where the collars meet is decked, which is
//! the whole of the passage.
//!
//! The berth is what makes this a matter of whole tiles: two ports are
//! mated face to face, both faces sit on tile seams, and both outward
//! steps are axis-aligned, so the ship's heading at the berth is a quarter
//! turn and the offset between the grids is an integer.
//!
//! # What is lost in the joining
//!
//! The room has one galley, one heads, one bay and one locker, and takes
//! the **first** of each by id — which is the ship's, since the ship is
//! laid down first. The station's galley is furniture to walk round while
//! the ship is docked. Its residents are **not** in the joined room: they
//! keep a room of their own on the station's design, where that galley is
//! the galley — see `World::join_rooms` — so nothing of theirs is lost;
//! what the crew cannot do is use the station's fixtures from this deck.

use flight::angle;
use shipdesign::parts::{Rotation, TILE, covered};
use shipdesign::{Budget, Edit, Money, PartKind, ShipDesign, apply, dock};
use worldgen::math::{DVec2, dvec2};

use crate::station::{Berth, Station};

/// The two designs as one.
#[derive(Clone, Debug)]
pub struct Joined {
    pub design: ShipDesign,
    /// Where the ship's tile (0, 0) sits in the joined grid, in tiles.
    pub ship_at: (u32, u32),
    /// Where a station design point lands in the joined grid: the image of
    /// the station's origin, in design units, and the station's two axes as
    /// unit steps in the joined grid. A station point `(x, y)` is at
    /// `origin + x * ex + y * ey`.
    pub station_origin: DVec2,
    pub station_ex: DVec2,
    pub station_ey: DVec2,
}

impl Joined {
    /// A ship design point, in the joined grid's units.
    pub fn ship_shift(&self) -> DVec2 {
        dvec2(
            self.ship_at.0 as f64 * TILE as f64,
            self.ship_at.1 as f64 * TILE as f64,
        )
    }

    /// A station design point, in the joined grid's units.
    pub fn from_station(&self, p: DVec2) -> DVec2 {
        self.station_origin
            .add(self.station_ex.scale(p.x))
            .add(self.station_ey.scale(p.y))
    }
}

/// Lay the two down as one grid. `None` when either has no port — there is
/// nothing to join by — or the berth is not a quarter turn, which cannot
/// happen for two axis-aligned ports and is refused rather than rounded.
pub fn join(ship: &ShipDesign, com: DVec2, station: &Station, berth: &Berth) -> Option<Joined> {
    let port = dock::port(ship)?;
    station.port()?;
    let t = TILE as f64;
    let ship_anchor = berth.position.sub(angle::rotate_design(com, berth.heading));

    // A station design point into the ship's design frame: through the
    // system and back.
    let to_ship = |p: DVec2| -> DVec2 {
        let system = station.to_system(p);
        angle::unrotate_design(system.sub(ship_anchor), berth.heading)
    };
    let origin = to_ship(DVec2::ZERO);
    let ex = to_ship(dvec2(t, 0.0)).sub(origin).scale(1.0 / t);
    let ey = to_ship(dvec2(0.0, t)).sub(origin).scale(1.0 / t);
    let axis = |v: DVec2| -> Option<(i32, i32)> {
        let (x, y) = (v.x.round() as i32, v.y.round() as i32);
        ((v.x - x as f64).abs() < 1e-6 && (v.y - y as f64).abs() < 1e-6 && x.abs() + y.abs() == 1)
            .then_some((x, y))
    };
    let (ax, ay) = (axis(ex)?, axis(ey)?);
    // Quarter turns clockwise on a y-down grid: `+x` goes to `+y` on the
    // first, to `-x` on the second, to `-y` on the third.
    let turns = match ax {
        (1, 0) => 0,
        (0, 1) => 1,
        (-1, 0) => 2,
        _ => 3,
    };
    // The origin lands on a tile corner, to rounding.
    let (ox, oy) = ((origin.x / t).round() as i32, (origin.y / t).round() as i32);
    if (origin.x / t - ox as f64).abs() > 1e-6 || (origin.y / t - oy as f64).abs() > 1e-6 {
        return None;
    }
    // A station tile's corner in ship tiles. A tile is a unit square whose
    // image is a unit square with a different min corner; the min corner is
    // the least of the four images.
    let corner =
        |x: i32, y: i32| -> (i32, i32) { (ox + x * ax.0 + y * ay.0, oy + x * ax.1 + y * ay.1) };
    let tile = |x: i32, y: i32| -> (i32, i32) {
        let corners = [
            corner(x, y),
            corner(x + 1, y),
            corner(x, y + 1),
            corner(x + 1, y + 1),
        ];
        let mx = corners.iter().map(|c| c.0).min().unwrap();
        let my = corners.iter().map(|c| c.1).min().unwrap();
        (mx, my)
    };

    // The extent of both, in ship tiles, and the shift that puts the least
    // corner one tile in from the edge.
    let side = station.design.build_area as i32;
    let station_corners = [
        tile(0, 0),
        tile(side - 1, 0),
        tile(0, side - 1),
        tile(side - 1, side - 1),
    ];
    let min_x = station_corners.iter().map(|c| c.0).min().unwrap().min(0);
    let min_y = station_corners.iter().map(|c| c.1).min().unwrap().min(0);
    let max_x = station_corners
        .iter()
        .map(|c| c.0)
        .max()
        .unwrap()
        .max(ship.build_area as i32 - 1);
    let max_y = station_corners
        .iter()
        .map(|c| c.1)
        .max()
        .unwrap()
        .max(ship.build_area as i32 - 1);
    let shift = (1 - min_x, 1 - min_y);
    let area = ((max_x - min_x + 3).max(max_y - min_y + 3)) as u32;

    let budget = Budget::new(Money::MAX);
    let mut design = ShipDesign::new(area);
    let mut put = |kind: PartKind, origin: (i32, i32), rotation: Rotation| {
        if origin.0 < 0 || origin.1 < 0 {
            return;
        }
        if let Ok(next) = apply(
            &design,
            &budget,
            Edit::Place {
                kind,
                origin: (origin.0 as u32, origin.1 as u32),
                rotation,
            },
        ) {
            design = next;
        }
    };

    // The ship first, at its own coordinates plus the shift, so its parts
    // have the lowest ids and its fixtures are the room's.
    for part in &ship.parts {
        put(
            part.kind,
            (
                part.origin.0 as i32 + shift.0,
                part.origin.1 as i32 + shift.1,
            ),
            part.rotation,
        );
    }
    // Then the station, turned. A part's tiles are turned one by one and
    // its origin is the least of them; its rotation gains the quarter
    // turns. `covered` is checked against the turned tiles, so a mismatch
    // in the turning arithmetic is a part left out, not a part put down
    // wrong.
    for part in &station.design.parts {
        let turned: Vec<(i32, i32)> = part
            .tiles()
            .into_iter()
            .map(|(x, y)| tile(x as i32, y as i32))
            .collect();
        let origin = (
            turned.iter().map(|c| c.0).min().unwrap(),
            turned.iter().map(|c| c.1).min().unwrap(),
        );
        let rotation = Rotation::ALL[(part.rotation as usize + turns) % 4];
        let mut expect: Vec<(i32, i32)> = covered(part.kind, rotation)
            .into_iter()
            .map(|(dx, dy)| (origin.0 + dx as i32, origin.1 + dy as i32))
            .collect();
        let mut got = turned.clone();
        expect.sort_unstable();
        got.sort_unstable();
        debug_assert_eq!(expect, got, "{:?} turned {turns} times", part.kind);
        if expect != got {
            continue;
        }
        put(
            part.kind,
            (origin.0 + shift.0, origin.1 + shift.1),
            rotation,
        );
    }
    // The passage: the tile beyond each of the ship's airlock tiles, where
    // the two collars meet, decked so a body can cross it.
    let airlock = ship.part(port.part_id)?;
    for (x, y) in airlock.tiles() {
        let gap = (
            x as i32 + port.outward.0 + shift.0,
            y as i32 + port.outward.1 + shift.1,
        );
        put(PartKind::Structure, gap, Rotation::R0);
        put(PartKind::Floor, gap, Rotation::R0);
    }
    // And the ship's cargo, which is what the room stocks its cold store
    // from.
    for (resource, units) in ship.manifest() {
        if let Ok(next) = apply(&design, &budget, Edit::Buy { resource, units }) {
            design = next;
        }
    }

    // Where a station point lands in the joined grid, in design units: the
    // origin's image, shifted, and the axes.
    let station_origin = dvec2((ox + shift.0) as f64 * t, (oy + shift.1) as f64 * t);
    Some(Joined {
        design,
        ship_at: (shift.0 as u32, shift.1 as u32),
        station_origin,
        station_ex: dvec2(ax.0 as f64, ax.1 as f64),
        station_ey: dvec2(ay.0 as f64, ay.1 as f64),
    })
}
