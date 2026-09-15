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

/// Grid resolution in world pixels. Fine enough to slip through the gaps
/// between units, coarse enough that a search is trivial.
const CELL: f32 = 10.0;

/// Integer step costs, the usual trick for keeping A* ordering on integers
/// rather than floats: 10 orthogonal, 14 diagonal (√2 ≈ 1.4).
const STRAIGHT: u32 = 10;
const DIAGONAL: u32 = 14;

pub struct Nav {
    cols: usize,
    rows: usize,
    /// World position of the centre of cell (0, 0).
    origin: Vec2,
    blocked: Vec<bool>,
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

        let mut nav = Nav {
            cols,
            rows,
            origin,
            blocked: vec![false; cols * rows],
        };
        for r in 0..rows {
            for c in 0..cols {
                let p = nav.centre(c, r);
                let hit = solids.iter().any(|s| s.expand(clearance).contains(p));
                nav.blocked[r * cols + c] = hit;
            }
        }
        nav
    }

    fn centre(&self, c: usize, r: usize) -> Vec2 {
        self.origin + vec2(c as f32 * CELL, r as f32 * CELL)
    }

    /// Nearest cell to a world point, clamped into the grid.
    fn cell_at(&self, p: Vec2) -> (usize, usize) {
        let c = ((p.x - self.origin.x) / CELL).round();
        let r = ((p.y - self.origin.y) / CELL).round();
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

            for (dc, dr) in [
                (1isize, 0isize),
                (-1, 0),
                (0, 1),
                (0, -1),
                (1, 1),
                (1, -1),
                (-1, 1),
                (-1, -1),
            ] {
                let nc = c as isize + dc;
                let nr = r as isize + dr;
                if nc < 0 || nr < 0 || nc >= self.cols as isize || nr >= self.rows as isize {
                    continue;
                }
                let (nc, nr) = (nc as usize, nr as usize);
                if self.is_blocked(nc, nr) {
                    continue;
                }
                // No squeezing diagonally between two blocked cells.
                let diagonal = dc != 0 && dr != 0;
                if diagonal
                    && (self.is_blocked((c as isize + dc) as usize, r)
                        || self.is_blocked(c, (r as isize + dr) as usize))
                {
                    continue;
                }

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
    fn line_clear(&self, a: Vec2, b: Vec2) -> bool {
        let span = b - a;
        let steps = (span.len() / (CELL * 0.4)).ceil().max(1.0) as usize;
        (0..=steps).all(|i| self.is_free(a.lerp(b, i as f32 / steps as f32)))
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
}

impl Maps {
    /// `solids` is everything fixed; `door` is the panel that only sometimes
    /// stands in the way.
    pub fn new(interior: Rect, solids: &[Rect], door: Rect, clearance: f32) -> Maps {
        let mut with_door = solids.to_vec();
        with_door.push(door);
        Maps {
            open: Nav::new(interior, solids, clearance),
            shut: Nav::new(interior, &with_door, clearance),
        }
    }

    pub fn pick(&self, door_open: bool) -> &Nav {
        if door_open { &self.open } else { &self.shut }
    }
}
