//! Pathfinding around the furniture.
//!
//! The deck is small and almost never changes shape, so grids are built once
//! at startup and every walk — player orders and scripted jobs alike — is
//! planned on one. Obstacles are inflated by the body radius, which means a
//! path that exists on a grid is one the Bim can physically walk without
//! clipping a corner.
//!
//! The one thing that does change is the bathroom door, and [`Maps`] holds a
//! grid for each of its two states rather than rebuilding one as it moves.

use crate::math::{Rect, Vec2, vec2};
use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// Grid resolution in world pixels, in the classic room. Fine enough to slip
/// through the gaps between units, coarse enough that a search is trivial.
const CELL: f32 = 10.0;

/// Cells to a tile in a room laid out on a tile grid — a ship's. An odd
/// number, so a tile's middle is a cell's middle: a one-tile corridor with
/// a body margin of 23 leaves six units free down its centre, and whether
/// any cell centre lands in those six is otherwise down to where the grid
/// happens to start. See [`Nav::tiled`].
const CELLS_PER_TILE: f32 = 5.0;

/// Integer step costs, the usual trick for keeping A* ordering on integers
/// rather than floats: 10 orthogonal, 14 diagonal (√2 ≈ 1.4).
const STRAIGHT: u32 = 10;
const DIAGONAL: u32 = 14;

/// How far the outside grid reaches from the body it is built about, in
/// tiles, every way. A hundred: further than any rock of a site, so a walk
/// is planned whole, and rebuilt about the body once it has moved
/// [`OUTSIDE_RECENTRE`] tiles from the middle. See [`Nav::outside`].
pub const OUTSIDE_RADIUS: i32 = 100;
pub const OUTSIDE_RECENTRE: f32 = 25.0;

pub struct Nav {
    cols: usize,
    rows: usize,
    /// World position of the centre of cell (0, 0).
    origin: Vec2,
    /// The cell's side: [`CELL`] in the classic room, a fifth of a tile
    /// aboard.
    cell: f32,
    blocked: Vec<bool>,
    /// Which connected patch of free cells each cell is in — the same
    /// number for every cell a body could walk between, by the same steps
    /// the search takes — and nought for a blocked one. Labelled once, when
    /// the grid is built, so "can a body get from here to there" is two
    /// lookups rather than a search of the whole grid: the room asks that
    /// question of every shelf, every site and every fixture for every
    /// idle Bim every step, and a search that comes back empty has walked
    /// every cell it could reach first.
    region: Vec<u32>,
}

impl Nav {
    /// Build the grid. A cell is blocked if a body centred on it would overlap
    /// a solid or stick out past the walls.
    pub fn new(interior: Rect, solids: &[Rect], clearance: f32) -> Nav {
        let walkable = interior.expand(-clearance);
        let cols = (walkable.width() / CELL).floor().max(1.0) as usize;
        let rows = (walkable.height() / CELL).floor().max(1.0) as usize;
        // Centre the grid in the walkable area so the margins are even.
        let origin = vec2(
            walkable.min.x + (walkable.width() - (cols - 1) as f32 * CELL) * 0.5,
            walkable.min.y + (walkable.height() - (rows - 1) as f32 * CELL) * 0.5,
        );
        Nav::build(cols, rows, origin, CELL, interior, solids, clearance)
    }

    /// The grid for a room laid out on tiles of `tile`, `interior` being
    /// tile-aligned: [`CELLS_PER_TILE`] cells a tile, phased so that every
    /// tile's middle is a cell's middle. That is what makes a one-tile
    /// corridor walkable — the free strip down its centre is narrower than
    /// a cell, and a grid started anywhere else has a cell centre in it
    /// only by luck, tile by tile.
    pub fn tiled(interior: Rect, solids: &[Rect], clearance: f32, tile: f32) -> Nav {
        let cell = tile / CELLS_PER_TILE;
        let cols = (interior.width() / cell).round().max(1.0) as usize;
        let rows = (interior.height() / cell).round().max(1.0) as usize;
        let origin = interior.min + vec2(cell * 0.5, cell * 0.5);
        Nav::build(cols, rows, origin, cell, interior, solids, clearance)
    }

    /// The grid for the outside of a ship: [`OUTSIDE_RADIUS`] tiles every
    /// way from `centre`, one cell a tile, with `solids` — the hull and the
    /// rocks — the only things in it. A cell a tile rather than five,
    /// because space is wide and the grid is rebuilt as the body moves: a
    /// tile's middle is a body's radius clear of its neighbour's edge, so a
    /// mined-out tile is a cell a body can stand in, and a tunnel a tile
    /// wide is walked. `interior()` is what the grid covers.
    pub fn outside(centre: Vec2, solids: &[Rect], clearance: f32, tile: f32) -> Nav {
        let side = (2 * OUTSIDE_RADIUS + 1) as f32 * tile;
        let corner = vec2(
            (centre.x / tile).floor() * tile - OUTSIDE_RADIUS as f32 * tile,
            (centre.y / tile).floor() * tile - OUTSIDE_RADIUS as f32 * tile,
        );
        let interior = Rect::from_min_size(corner, vec2(side, side));
        let cols = (2 * OUTSIDE_RADIUS + 1) as usize;
        let origin = corner + vec2(tile * 0.5, tile * 0.5);
        // The margin the classic grid keeps from its walls is nothing out
        // here: the edge of the grid is not a wall, only where the grid
        // stops, and a body at the edge is a body a rebuild will follow.
        Nav::build(
            cols,
            cols,
            origin,
            tile,
            interior.expand(clearance),
            solids,
            clearance,
        )
    }

    /// What the grid covers, in world units.
    pub fn interior(&self) -> Rect {
        Rect::from_min_size(
            self.origin - vec2(self.cell * 0.5, self.cell * 0.5),
            vec2(self.cols as f32 * self.cell, self.rows as f32 * self.cell),
        )
    }

    /// The middle of the grid, in world units.
    pub fn middle(&self) -> Vec2 {
        self.interior().center()
    }

    /// Either grid: a cell is blocked if a body centred on it would overlap
    /// a solid or stick out past the walls.
    fn build(
        cols: usize,
        rows: usize,
        origin: Vec2,
        cell: f32,
        interior: Rect,
        solids: &[Rect],
        clearance: f32,
    ) -> Nav {
        let walkable = interior.expand(-clearance);
        let mut nav = Nav {
            cols,
            rows,
            origin,
            cell,
            blocked: vec![false; cols * rows],
            region: Vec::new(),
        };
        // The classic grid is cut to the walkable area and never has a cell
        // outside it; the tiled one covers the whole interior, so the
        // margin's cells are blocked here.
        for r in 0..rows {
            for c in 0..cols {
                if !walkable.contains(nav.centre(c, r)) {
                    nav.blocked[r * cols + c] = true;
                }
            }
        }
        // Solid by solid over the cells its inflated box can reach, rather
        // than cell by cell over every solid: the same question asked of
        // the same cells, so the grid comes out identical, and a ship's
        // room with seven hundred solids builds in a few thousand tests
        // instead of sixty million. It matters because a ship's grids are
        // rebuilt whenever one of its doors is locked or unlocked.
        for s in solids {
            let box_ = s.expand(clearance);
            let c0 = ((box_.min.x - origin.x) / cell).floor().max(0.0) as usize;
            let r0 = ((box_.min.y - origin.y) / cell).floor().max(0.0) as usize;
            let c1 = ((box_.max.x - origin.x) / cell).ceil().max(0.0) as usize;
            let r1 = ((box_.max.y - origin.y) / cell).ceil().max(0.0) as usize;
            for r in r0..=r1.min(rows.saturating_sub(1)) {
                for c in c0..=c1.min(cols.saturating_sub(1)) {
                    if box_.contains(nav.centre(c, r)) {
                        nav.blocked[r * cols + c] = true;
                    }
                }
            }
        }
        nav.label();
        nav
    }

    /// Number the patches: a flood from every free cell not yet numbered,
    /// stepping exactly as [`Nav::search`] does — eight ways, and never
    /// diagonally between two blocked cells — so two cells share a number
    /// exactly when a search between them would succeed.
    fn label(&mut self) {
        let n = self.cols * self.rows;
        self.region = vec![0; n];
        let mut next = 0u32;
        let mut stack: Vec<usize> = Vec::new();
        for start in 0..n {
            if self.blocked[start] || self.region[start] != 0 {
                continue;
            }
            next += 1;
            self.region[start] = next;
            stack.push(start);
            while let Some(here) = stack.pop() {
                let (c, r) = (here % self.cols, here / self.cols);
                let steps: Vec<(usize, usize)> = self.steps_from(c, r).collect();
                for (nc, nr) in steps {
                    let i = nr * self.cols + nc;
                    if self.region[i] == 0 {
                        self.region[i] = next;
                        stack.push(i);
                    }
                }
            }
        }
    }

    /// The free cells a body may step to from `(c, r)`: the eight
    /// neighbours, less the blocked ones and less a diagonal that would
    /// squeeze between two blocked cells. The one rule, for the search and
    /// the labelling both.
    fn steps_from(&self, c: usize, r: usize) -> impl Iterator<Item = (usize, usize)> + '_ {
        [
            (1isize, 0isize),
            (-1, 0),
            (0, 1),
            (0, -1),
            (1, 1),
            (1, -1),
            (-1, 1),
            (-1, -1),
        ]
        .into_iter()
        .filter_map(move |(dc, dr)| {
            let nc = c as isize + dc;
            let nr = r as isize + dr;
            if nc < 0 || nr < 0 || nc >= self.cols as isize || nr >= self.rows as isize {
                return None;
            }
            let (nc, nr) = (nc as usize, nr as usize);
            if self.is_blocked(nc, nr) {
                return None;
            }
            if dc != 0 && dr != 0 && (self.is_blocked(nc, r) || self.is_blocked(c, nr)) {
                return None;
            }
            Some((nc, nr))
        })
    }

    /// Whether a body could walk from `from` to `to` at all: what
    /// [`Nav::path`] would find a route for, answered without finding it.
    /// Both ends are snapped to the nearest free cell exactly as `path`
    /// snaps them, so the two agree.
    pub fn can_reach(&self, from: Vec2, to: Vec2) -> bool {
        let (c0, r0) = self.cell_at(self.nearest_free(from));
        let (c1, r1) = self.cell_at(self.nearest_free(to));
        let a = self.region[r0 * self.cols + c0];
        a != 0 && a == self.region[r1 * self.cols + c1]
    }

    fn centre(&self, c: usize, r: usize) -> Vec2 {
        self.origin + vec2(c as f32 * self.cell, r as f32 * self.cell)
    }

    /// Nearest cell to a world point, clamped into the grid.
    fn cell_at(&self, p: Vec2) -> (usize, usize) {
        let c = ((p.x - self.origin.x) / self.cell).round();
        let r = ((p.y - self.origin.y) / self.cell).round();
        (
            c.clamp(0.0, (self.cols - 1) as f32) as usize,
            r.clamp(0.0, (self.rows - 1) as f32) as usize,
        )
    }

    fn is_blocked(&self, c: usize, r: usize) -> bool {
        self.blocked[r * self.cols + c]
    }

    /// Can a body stand here?
    pub fn is_free(&self, p: Vec2) -> bool {
        let (c, r) = self.cell_at(p);
        !self.is_blocked(c, r)
    }

    /// The closest standable point to `p`. Used to make an order onto the table
    /// mean "the floor beside the table" rather than nothing at all.
    pub fn nearest_free(&self, p: Vec2) -> Vec2 {
        let (c0, r0) = self.cell_at(p);
        if !self.is_blocked(c0, r0) {
            return self.centre(c0, r0);
        }
        // Expanding rings outwards; the room is small enough that this is cheap.
        let reach = self.cols.max(self.rows);
        for radius in 1..=reach {
            let mut best: Option<(f32, Vec2)> = None;
            for dr in -(radius as isize)..=(radius as isize) {
                for dc in -(radius as isize)..=(radius as isize) {
                    // Only the perimeter of this ring; the inside was searched already.
                    if dc.abs() != radius as isize && dr.abs() != radius as isize {
                        continue;
                    }
                    let c = c0 as isize + dc;
                    let r = r0 as isize + dr;
                    if c < 0 || r < 0 || c >= self.cols as isize || r >= self.rows as isize {
                        continue;
                    }
                    let (c, r) = (c as usize, r as usize);
                    if self.is_blocked(c, r) {
                        continue;
                    }
                    let at = self.centre(c, r);
                    let d = (at - p).len();
                    if best.is_none_or(|(bd, _)| d < bd) {
                        best = Some((d, at));
                    }
                }
            }
            if let Some((_, at)) = best {
                return at;
            }
        }
        p
    }

    /// The middles of the free cells within `radius` of `centre`, one every
    /// `spacing` units along each axis (rounded to whole cells, never
    /// fewer than one), for something that wants places to stand rather
    /// than a route — the enemy's tactics, which score every spot it could
    /// shoot from. Tile-spaced rather than cell-spaced because a ship's
    /// grid is five cells a tile and the fight is worked out on tiles: at
    /// a weapon's twelve-tile reach the lattice is a few hundred spots,
    /// where every cell would be fourteen thousand, each a trace of sight.
    /// The lattice is phased on the grid, not on `centre`, so two calls a
    /// step apart offer the same spots and a choice made once is offered
    /// again — and on the middle cell of each run, which on a tiled grid
    /// is the tile's middle.
    pub fn free_cells_within(&self, centre: Vec2, radius: f32, spacing: f32) -> Vec<Vec2> {
        let step = (spacing / self.cell).round().max(1.0) as usize;
        let span = (radius / self.cell).ceil() as isize;
        let (c0, r0) = self.cell_at(centre);
        let mut out = Vec::new();
        for r in (r0 as isize - span).max(0)..=(r0 as isize + span).min(self.rows as isize - 1) {
            let r = r as usize;
            if r % step != step / 2 {
                continue;
            }
            for c in (c0 as isize - span).max(0)..=(c0 as isize + span).min(self.cols as isize - 1)
            {
                let c = c as usize;
                if c % step != step / 2 || self.is_blocked(c, r) {
                    continue;
                }
                let at = self.centre(c, r);
                if (at - centre).len() <= radius {
                    out.push(at);
                }
            }
        }
        out
    }

    /// Waypoints from `from` to `to`, not including `from`. Empty if there is
    /// nowhere to go; a single point when the way is already clear.
    pub fn path(&self, from: Vec2, to: Vec2) -> Vec<Vec2> {
        let goal = self.nearest_free(to);
        if self.line_clear(from, goal) {
            return vec![goal];
        }

        // `from` can be inside an inflated obstacle after a push-out, so plan
        // from the nearest cell the search can actually reach.
        let start = self.cell_at(self.nearest_free(from));
        let end = self.cell_at(goal);
        // Two patches: no route, and no point walking every cell of one to
        // find that out.
        if self.region[start.1 * self.cols + start.0] != self.region[end.1 * self.cols + end.0] {
            return Vec::new();
        }
        let Some(cells) = self.search(start, end) else {
            return Vec::new();
        };

        let mut points: Vec<Vec2> = cells.iter().map(|&(c, r)| self.centre(c, r)).collect();
        // Finish on the real target rather than the centre of its cell.
        if let Some(last) = points.last_mut() {
            *last = goal;
        }
        self.smooth(from, points)
    }

    fn search(&self, start: (usize, usize), end: (usize, usize)) -> Option<Vec<(usize, usize)>> {
        let n = self.cols * self.rows;
        let idx = |c: usize, r: usize| r * self.cols + c;
        let heuristic = |c: usize, r: usize| {
            let dc = c.abs_diff(end.0) as u32;
            let dr = r.abs_diff(end.1) as u32;
            // Octile distance: diagonals where they help, straights for the rest.
            DIAGONAL * dc.min(dr) + STRAIGHT * (dc.max(dr) - dc.min(dr))
        };

        let mut cost = vec![u32::MAX; n];
        let mut came = vec![usize::MAX; n];
        let mut open = BinaryHeap::new();

        cost[idx(start.0, start.1)] = 0;
        open.push(Reverse((
            heuristic(start.0, start.1),
            idx(start.0, start.1),
        )));

        while let Some(Reverse((_, here))) = open.pop() {
            let (c, r) = (here % self.cols, here / self.cols);
            if (c, r) == end {
                let mut route = vec![(c, r)];
                let mut at = here;
                while came[at] != usize::MAX {
                    at = came[at];
                    route.push((at % self.cols, at / self.cols));
                }
                route.reverse();
                return Some(route);
            }

            for (nc, nr) in self.steps_from(c, r) {
                let diagonal = nc != c && nr != r;
                let step = if diagonal { DIAGONAL } else { STRAIGHT };
                let next = cost[here].saturating_add(step);
                if next < cost[idx(nc, nr)] {
                    cost[idx(nc, nr)] = next;
                    came[idx(nc, nr)] = here;
                    open.push(Reverse((next + heuristic(nc, nr), idx(nc, nr))));
                }
            }
        }
        None
    }

    /// True when a body can walk straight from `a` to `b`.
    /// Whether a body can actually walk the straight line from `a` to `b`.
    ///
    /// Sampled along the line *and* a half-cell either side of it, because a
    /// cell is marked blocked by its centre alone: a line can pass within half
    /// a cell of an obstacle and still find every sample free. The Bim then
    /// walks that line, the collision push-out shoves it back, and where the
    /// push is exactly opposite the walk — a waypoint straight through the
    /// corner of the table — the two cancel and the Bim stands there for ever,
    /// marching on the spot, with the chain waiting on an arrival that cannot
    /// come. That is not a hypothetical: it stood against the table for ten
    /// hours of game time with a trip to the heads on its agenda.
    ///
    /// The width costs two extra samples per step and buys a route the body
    /// can hold to without the physics arguing with it.
    fn line_clear(&self, a: Vec2, b: Vec2) -> bool {
        let span = b - a;
        let steps = (span.len() / (self.cell * 0.4)).ceil().max(1.0) as usize;
        let side = span.normalize_or_zero().perp() * (self.cell * 0.5);
        (0..=steps).all(|i| {
            let p = a.lerp(b, i as f32 / steps as f32);
            self.is_free(p) && self.is_free(p + side) && self.is_free(p - side)
        })
    }

    /// Drop every waypoint that can be skipped without hitting anything. A raw
    /// grid route staircases along diagonals; this turns it back into the few
    /// straight legs a person would actually walk.
    fn smooth(&self, from: Vec2, points: Vec<Vec2>) -> Vec<Vec2> {
        let mut out: Vec<Vec2> = Vec::new();
        let mut anchor = from;
        let mut i = 0;
        while i < points.len() {
            // Farthest point still in sight of the current anchor.
            let mut furthest = i;
            for j in (i..points.len()).rev() {
                if self.line_clear(anchor, points[j]) {
                    furthest = j;
                    break;
                }
            }
            out.push(points[furthest]);
            anchor = points[furthest];
            i = furthest + 1;
        }
        out
    }
}

/// Both versions of the deck: with the bathroom door shut, and with it open.
///
/// One grid was enough while the room never changed shape, and until the heads
/// were built it never did. The trouble with rebuilding a single grid when the
/// door moves is timing: a task opens the door and asks for a route through it
/// in the same step, so a rebuild anywhere in the frame loop is always one
/// frame late and the route comes back empty. Two grids built once, and the
/// choice made at the moment a path is asked for, cannot be late.
pub struct Maps {
    open: Nav,
    shut: Nav,
    /// The outside of the hull, while there is an outside to walk: built
    /// about whoever is out there by `Game::refresh_outside`, and `None` in
    /// a room with nothing outside it. See [`Nav::outside`].
    outside: Option<Nav>,
}

impl Maps {
    /// `solids` is everything fixed; `door` is the panel that only sometimes
    /// stands in the way. `tile` is the room's tile where it has one — a
    /// ship's — and the grids are then [`Nav::tiled`]; `None` is the
    /// classic room's grid.
    pub fn new(
        interior: Rect,
        solids: &[Rect],
        door: Rect,
        clearance: f32,
        tile: Option<f32>,
    ) -> Maps {
        let mut with_door = solids.to_vec();
        with_door.push(door);
        let grid = |solids: &[Rect]| match tile {
            Some(tile) => Nav::tiled(interior, solids, clearance, tile),
            None => Nav::new(interior, solids, clearance),
        };
        Maps {
            open: grid(solids),
            shut: grid(&with_door),
            outside: None,
        }
    }

    pub fn pick(&self, door_open: bool) -> &Nav {
        if door_open { &self.open } else { &self.shut }
    }

    /// The grid a body is on: the outside's while it is out there, the
    /// deck's otherwise. A body outside with no outside grid — the ship has
    /// left the site under it — gets the deck's, which has no route for it
    /// and says so, the way a shut door does.
    pub fn for_body(&self, outside: bool, door_open: bool) -> &Nav {
        match (outside, &self.outside) {
            (true, Some(nav)) => nav,
            _ => self.pick(door_open),
        }
    }

    pub fn outside(&self) -> Option<&Nav> {
        self.outside.as_ref()
    }

    pub fn set_outside(&mut self, nav: Option<Nav>) {
        self.outside = nav;
    }

    pub fn take_outside(&mut self) -> Option<Nav> {
        self.outside.take()
    }
}
