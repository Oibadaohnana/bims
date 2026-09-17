//! Where a design meets another one: its airlock, and which way it faces
//! out.
//!
//! A ship docks at a station **airlock to airlock**. Both are designs on a
//! tile grid, so "where is the door and which way does it open" is one
//! question asked twice, and it is asked here, in integers, so that the
//! native server and the wasm client put the ship in the same place to the
//! unit. `world` turns the answer into a position and a heading; nothing in
//! this crate knows what a position is.
//!
//! The port is the **first** airlock by id, which is the rule the room uses
//! for every fixture it maps one of. A ship with two airlocks docks by the
//! one it built first.

use crate::design::ShipDesign;
use crate::parts::{Layer, PartKind, TILE};

/// One side of a tile, as a step to the neighbour beyond it.
const SIDES: [(i32, i32); 4] = [(0, -1), (1, 0), (0, 1), (-1, 0)];

/// How far an airlock's collar stands out from the skin: half a tile. Two
/// docked airlocks meet collar to collar, so their hulls are a tile apart
/// and the way through is the two collars end to end. The painter draws
/// the collar this long (`hull::part`), and the berth is worked out from
/// the same number, so the picture and the place agree.
pub const PROTRUSION: f64 = TILE as f64 / 2.0;

/// A design's docking port.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Port {
    pub part_id: u32,
    /// The middle of the airlock, in design world units about the design's
    /// origin, `y` down the grid.
    pub centre: (f64, f64),
    /// Which way it opens to space: a unit step in tiles, `y` down. The side
    /// of the airlock with **nothing of the frame beyond it** along its whole
    /// length; an airlock buried in the hull with no such side has no port.
    pub outward: (i32, i32),
}

impl Port {
    /// Where the end of the collar is: the centre pushed to the skin and
    /// then [`PROTRUSION`] beyond it. Two ports are mated when their faces
    /// coincide.
    pub fn face(&self) -> (f64, f64) {
        let reach = TILE as f64 / 2.0 + PROTRUSION;
        (
            self.centre.0 + self.outward.0 as f64 * reach,
            self.centre.1 + self.outward.1 as f64 * reach,
        )
    }
}

/// The design's port, if it has an airlock that opens onto space.
pub fn port(design: &ShipDesign) -> Option<Port> {
    let airlock = design.parts.iter().find(|p| p.kind == PartKind::Airlock)?;
    let grid = design.grid();
    let tiles = airlock.tiles();
    let outward = SIDES.into_iter().find(|&(dx, dy)| {
        tiles
            .iter()
            .all(|&(x, y)| grid.get(Layer::Structure, (x as i32 + dx, y as i32 + dy)) == 0)
    })?;
    let t = TILE as f64;
    let n = tiles.len() as f64;
    let centre = tiles.iter().fold((0.0, 0.0), |acc, &(x, y)| {
        (
            acc.0 + (x as f64 + 0.5) * t / n,
            acc.1 + (y as f64 + 0.5) * t / n,
        )
    });
    Some(Port {
        part_id: airlock.id,
        centre,
        outward,
    })
}
