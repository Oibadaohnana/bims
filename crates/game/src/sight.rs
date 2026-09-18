//! What the crew can see.
//!
//! Every Bim sees all the way round — there is no cone — and what stops
//! its eyes is what stands in the way: the walls, the hull, the tall parts,
//! a door with its leaves shut. Sight is worked out on the room's tile
//! grid and it is **traced**: a tile is seen when the straight line from a
//! crew member's eyes to the tile's middle crosses nothing opaque on the
//! way, and the tile it stops at is seen too, so a wall is seen from the
//! room it walls. The crew share it — a tile one of them sees, all of
//! them see — because the crew are one crew and a screen is one screen.
//!
//! The mask is a picture and a fact at once: [`Game::render`] draws a fog
//! over every tile nobody sees, and the world asks [`Sight::seen_at`] for
//! whether a body on the station's deck is in anybody's view. It is
//! recomputed only when something that could change it has — an eye
//! crossed a tile, a door shut or opened — since a trace of every tile
//! from every eye is a few hundred thousand steps in a joined room.
//!
//! # Peeking round a wall
//!
//! A Bim standing **against** a wall — the tile beside it opaque — leans
//! out and looks along it: as well as from where it stands, it sees from
//! the free tile either side of it along the wall, and what those extra
//! eyes add is whatever lies **beyond the wall's line**. So a Bim pressed
//! to the corner of a room sees the whole of the room on the other side
//! of the corner, where one standing a tile back sees only the wedge the
//! corner leaves. [`Sight::eyes_from`] is the rule, and [`Sight::sees_from`]
//! asks it for one body and one target — a shot is fired from whichever
//! eye saw the enemy, so a peeking Bim shoots from the peek.
//!
//! # Whose the tiles are
//!
//! Every tile has a [`Stance`]: the crew's own ship is friendly, and a
//! station's tiles are whatever the world says the station is. What the
//! fog looks like over an unseen tile follows from that. A friendly
//! structure is under a **semi** fog — the deck stays readable, since the
//! crew know their own ship — while a neutral or hostile one is **black**
//! where nobody has looked, and **grey** in a ring [`RING`] tiles wide
//! round what is seen, where the structure shows but no body does. The
//! grey stays once earned: what has been looked at is known, and only
//! who is standing there is forgotten.

use crate::draw::{Color, DrawList};
use crate::math::{Rect, Vec2, vec2};

/// The fog over what nobody sees of a friendly structure: the deck under
/// it stays readable, since the ship is the crew's own and they know where
/// the walls are, but whatever is standing there is not drawn.
const FOG: Color = Color::rgba(0.02, 0.04, 0.03, 0.62);
/// The ring round what is seen of somebody else's structure: the walls
/// and the fixtures show through, and nobody standing among them does.
const FOG_GREY: Color = Color::rgba(0.06, 0.07, 0.08, 0.80);
/// The rest of somebody else's structure: nothing.
const FOG_BLACK: Color = Color::rgba(0.0, 0.0, 0.0, 1.0);

/// How far the grey ring reaches past what is seen, in tiles.
pub const RING: i32 = 3;

/// How far an opaque fog rectangle reaches past its tiles on each side, in
/// room units, so that two meeting edge to edge show no seam.
const OVERLAP: f32 = 2.0;

/// Whose a structure is, to the crew. What the fog over it looks like.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[repr(u32)]
pub enum Stance {
    /// The crew's own ship, and anywhere else they are welcome.
    #[default]
    Friendly = 0,
    Neutral = 1,
    /// Whoever lives there is an enemy.
    Hostile = 2,
}

impl Stance {
    pub fn code(self) -> u32 {
        self as u32
    }
}

/// One tile, as the grid sees it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Cell {
    /// A line of sight stops here: a wall, the hull, a tall part, a shut
    /// door, or the outside.
    opaque: bool,
    /// The fog is drawn here when it is not seen: a tile of the hull, or
    /// anywhere inside the classic room's box.
    fogged: bool,
    /// Somebody else's: a station's tile on a joined deck, under the
    /// foreign stance rather than the room's own.
    foreign: bool,
}

/// One place a body looks from: where, and — for a peek — what it may
/// add, as the direction of the wall it is peeking past from the body's
/// own tile.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Eye {
    pub at: Vec2,
    /// The body's own tile and the wall's direction from it, for a peek;
    /// `None` for the body's own eyes, which add everything they see.
    beyond: Option<((i32, i32), (i32, i32))>,
}

impl Eye {
    /// Whether this is a peek beside a wall rather than the body's own
    /// eyes. What the enemy's tactics ask, to tell cover from the open.
    pub fn is_peek(&self) -> bool {
        self.beyond.is_some()
    }

    /// Whether this eye may add the tile: any, for the body's own; past
    /// the wall's line, for a peek.
    pub fn admits(&self, tile: (i32, i32)) -> bool {
        match self.beyond {
            None => true,
            Some(((bx, by), (dx, dy))) => (tile.0 - bx) * dx + (tile.1 - by) * dy >= 1,
        }
    }
}

/// How a fogged tile is drawn.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Veil {
    Semi,
    Grey,
    Black,
}

/// What everybody aboard can see, put together. See the module note.
#[derive(Clone, Debug)]
pub struct Sight {
    /// The grid's corner and its tile, in room units.
    origin: Vec2,
    tile: f32,
    columns: i32,
    rows: i32,
    /// What is always in the way: the walls and the tall parts.
    fixed: Vec<Cell>,
    /// The same with the shut doors added, for the trace to read.
    cells: Vec<Cell>,
    /// Whether anybody sees each tile.
    seen: Vec<bool>,
    /// Whether each tile is within [`RING`] of one that is seen, or ever
    /// was: what has been looked at stays known — the grey is the fog of
    /// war's "explored", and only the bodies in it are forgotten.
    near: Vec<bool>,
    /// What the mask was last worked out from: the eyes' tiles and the
    /// shut doors. When these have not moved, neither has the mask.
    eyes_at: Vec<(i32, i32)>,
    shut: Vec<Rect>,
    /// Whether anything has been traced yet. Until it has, nothing is
    /// seen, which is right for a room nobody is looking into.
    traced: bool,
    /// Whose the room's own tiles are, and whose the foreign ones.
    own: Stance,
    foreign: Stance,
}

impl Sight {
    /// A grid over `bounds` at `tile`, where a line of sight stops at
    /// every one of `opaque` and outside `interior`, and the fog is drawn
    /// over `fogged` — or, with none given, over the whole of `bounds`.
    pub fn new(bounds: Rect, interior: Rect, tile: f32, opaque: &[Rect], fogged: &[Rect]) -> Sight {
        let columns = (bounds.width() / tile).ceil().max(1.0) as i32;
        let rows = (bounds.height() / tile).ceil().max(1.0) as i32;
        let mut sight = Sight {
            origin: bounds.min,
            tile,
            columns,
            rows,
            fixed: Vec::new(),
            cells: Vec::new(),
            seen: vec![false; (columns * rows) as usize],
            near: vec![false; (columns * rows) as usize],
            eyes_at: Vec::new(),
            shut: Vec::new(),
            traced: false,
            own: Stance::Friendly,
            foreign: Stance::Neutral,
        };
        let mut fixed = vec![
            Cell {
                opaque: false,
                fogged: fogged.is_empty(),
                foreign: false,
            };
            (columns * rows) as usize
        ];
        // Outside the deck's box is the outside: nothing to see there and
        // nothing to see through.
        for y in 0..rows {
            for x in 0..columns {
                if !interior.contains(sight.middle(x, y)) {
                    fixed[sight.index(x, y)].opaque = true;
                }
            }
        }
        for rect in opaque {
            sight.mark(&mut fixed, rect, &mut |c| c.opaque = true);
        }
        for rect in fogged {
            sight.mark(&mut fixed, rect, &mut |c| c.fogged = true);
        }
        sight.cells = fixed.clone();
        sight.fixed = fixed;
        sight
    }

    /// Whose the room's own tiles are. A station's room is whatever the
    /// station is; the ship's is the crew's.
    pub fn set_stance(&mut self, stance: Stance) {
        self.own = stance;
    }

    /// Which tiles are somebody else's — a station's on a joined deck,
    /// by its box — and whose. `None` makes every tile the room's own.
    pub fn set_foreign(&mut self, rect: Option<Rect>, stance: Stance) {
        self.foreign = stance;
        for c in &mut self.fixed {
            c.foreign = false;
        }
        if let Some(rect) = rect {
            for y in 0..self.rows {
                for x in 0..self.columns {
                    if rect.contains(self.middle(x, y)) {
                        let i = self.index(x, y);
                        self.fixed[i].foreign = true;
                    }
                }
            }
        }
        // The trace's copy carries the doors; the flag is the same either
        // way, so it is copied across rather than traced afresh.
        for (c, f) in self.cells.iter_mut().zip(&self.fixed) {
            c.foreign = f.foreign;
        }
    }

    fn index(&self, x: i32, y: i32) -> usize {
        (y * self.columns + x) as usize
    }

    fn inside(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && x < self.columns && y < self.rows
    }

    /// Which tile a point is in. Off the grid is a tile off the grid, which
    /// `inside` says no to.
    pub fn tile_of(&self, p: Vec2) -> (i32, i32) {
        (
            ((p.x - self.origin.x) / self.tile).floor() as i32,
            ((p.y - self.origin.y) / self.tile).floor() as i32,
        )
    }

    fn middle(&self, x: i32, y: i32) -> Vec2 {
        self.origin + vec2((x as f32 + 0.5) * self.tile, (y as f32 + 0.5) * self.tile)
    }

    /// Whether a line of sight stops in the tile, as of the last trace:
    /// off the grid counts as stopped.
    fn opaque_at(&self, x: i32, y: i32) -> bool {
        !self.inside(x, y) || self.cells[self.index(x, y)].opaque
    }

    /// Every tile a rectangle covers a real share of: the part of the tile
    /// it takes has to be at least half the rectangle's own width and
    /// height, capped at half a tile. A tile-aligned part covers its tiles
    /// and none beside them, since the sliver it shares with a neighbour is
    /// nothing wide; a thin wall straddling a tile line goes to the tile
    /// most of it is in.
    fn mark(&self, cells: &mut [Cell], rect: &Rect, f: &mut dyn FnMut(&mut Cell)) {
        let (x0, y0) = self.tile_of(rect.min);
        let (x1, y1) = self.tile_of(rect.max - vec2(1e-3, 1e-3));
        let need_w = (rect.width() * 0.5).min(self.tile * 0.5);
        let need_h = (rect.height() * 0.5).min(self.tile * 0.5);
        for y in y0.max(0)..=y1.min(self.rows - 1) {
            for x in x0.max(0)..=x1.min(self.columns - 1) {
                let tile = Rect::from_min_size(
                    self.origin + vec2(x as f32 * self.tile, y as f32 * self.tile),
                    vec2(self.tile, self.tile),
                );
                let w = rect.max.x.min(tile.max.x) - rect.min.x.max(tile.min.x);
                let h = rect.max.y.min(tile.max.y) - rect.min.y.max(tile.min.y);
                if w >= need_w && h >= need_h && w > 0.0 && h > 0.0 {
                    f(&mut cells[self.index(x, y)]);
                }
            }
        }
    }

    /// Where a body standing at `p` looks from: its own eyes, and — with a
    /// wall against it — the peek either side along that wall. See the
    /// module note. Read against the last trace's doors.
    pub fn eyes_from(&self, p: Vec2) -> Vec<Eye> {
        let mut eyes = vec![Eye {
            at: p,
            beyond: None,
        }];
        let (bx, by) = self.tile_of(p);
        if !self.inside(bx, by) {
            return eyes;
        }
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            if !self.opaque_at(bx + dx, by + dy) {
                continue;
            }
            // The wall is there; the tiles either side of the body along
            // it are where it leans out to.
            for (sx, sy) in [(dy, -dx), (-dy, dx)] {
                let (x, y) = (bx + sx, by + sy);
                if self.opaque_at(x, y) {
                    continue;
                }
                let at = self.middle(x, y);
                let eye = Eye {
                    at,
                    beyond: Some(((bx, by), (dx, dy))),
                };
                if !eyes.contains(&eye) {
                    eyes.push(eye);
                }
            }
        }
        eyes
    }

    /// Whether a body at `from` sees the tile a point is in, and from
    /// which eye — its own, or a peek beside a wall. `None` when it does
    /// not.
    pub fn sees_from(&self, from: Vec2, target: Vec2) -> Option<Vec2> {
        let tile = self.tile_of(target);
        if !self.inside(tile.0, tile.1) {
            return None;
        }
        self.eyes_from(from)
            .into_iter()
            .find(|eye| eye.admits(tile) && self.clear_line(eye.at, tile))
            .map(|eye| eye.at)
    }

    /// Work the mask out again from these eyes and these shut doors, if
    /// anything about them has changed since last time. True when it was.
    pub fn observe(&mut self, eyes: &[Vec2], shut: &[Rect]) -> bool {
        let eyes_at: Vec<(i32, i32)> = eyes.iter().map(|&p| self.tile_of(p)).collect();
        if self.traced && eyes_at == self.eyes_at && shut == self.shut {
            return false;
        }
        self.eyes_at = eyes_at;
        self.shut = shut.to_vec();
        self.traced = true;

        // The doors, shut, over the fixed picture.
        let mut cells = self.fixed.clone();
        for door in shut {
            self.mark(&mut cells, door, &mut |c| c.opaque = true);
        }
        self.cells = cells;

        for s in self.seen.iter_mut() {
            *s = false;
        }
        for &body in eyes {
            for eye in self.eyes_from(body) {
                let (ex, ey) = self.tile_of(eye.at);
                if !self.inside(ex, ey) {
                    continue;
                }
                for y in 0..self.rows {
                    for x in 0..self.columns {
                        let i = self.index(x, y);
                        if self.seen[i] || !eye.admits((x, y)) {
                            continue;
                        }
                        if self.clear_line(eye.at, (x, y)) {
                            self.seen[i] = true;
                        }
                    }
                }
            }
        }
        self.widen();
        true
    }

    /// The ring: every tile within [`RING`] of a seen one, in two passes
    /// — along the rows, then down the columns of that.
    fn widen(&mut self) {
        let (w, h) = (self.columns, self.rows);
        let mut rows = vec![false; (w * h) as usize];
        for y in 0..h {
            for x in 0..w {
                let lo = (x - RING).max(0);
                let hi = (x + RING).min(w - 1);
                rows[self.index(x, y)] = (lo..=hi).any(|k| self.seen[self.index(k, y)]);
            }
        }
        for y in 0..h {
            for x in 0..w {
                let lo = (y - RING).max(0);
                let hi = (y + RING).min(h - 1);
                let near = (lo..=hi).any(|k| rows[self.index(x, k)]);
                let i = self.index(x, y);
                self.near[i] |= near;
            }
        }
    }

    /// Whether the straight line from `from` to the middle of tile `to`
    /// crosses no opaque tile before it gets there. The tiles the line
    /// passes through are walked one at a time, whichever grid line it
    /// crosses next — the standard traversal — so a line squeezing between
    /// two opaque tiles set corner to corner still has to pass through one
    /// of them, and is stopped.
    pub fn clear_line(&self, from: Vec2, to: (i32, i32)) -> bool {
        let (mut x, mut y) = self.tile_of(from);
        let (tx, ty) = to;
        if (x, y) == (tx, ty) {
            return true;
        }
        let target = self.middle(tx, ty);
        let d = target - from;
        let step_x: i32 = if d.x > 0.0 { 1 } else { -1 };
        let step_y: i32 = if d.y > 0.0 { 1 } else { -1 };
        // How far along the line (as a fraction of it) the next vertical
        // and horizontal grid lines are, and how much further each one
        // after that is.
        let next_x = self.origin.x + (x + if d.x > 0.0 { 1 } else { 0 }) as f32 * self.tile;
        let next_y = self.origin.y + (y + if d.y > 0.0 { 1 } else { 0 }) as f32 * self.tile;
        let (mut t_x, delta_x) = if d.x.abs() > 1e-6 {
            ((next_x - from.x) / d.x, self.tile / d.x.abs())
        } else {
            (f32::INFINITY, f32::INFINITY)
        };
        let (mut t_y, delta_y) = if d.y.abs() > 1e-6 {
            ((next_y - from.y) / d.y, self.tile / d.y.abs())
        } else {
            (f32::INFINITY, f32::INFINITY)
        };
        // Never more steps than tiles on the way, whatever the arithmetic.
        let most = (tx - x).abs() + (ty - y).abs() + 2;
        for _ in 0..most {
            if t_x < t_y {
                x += step_x;
                t_x += delta_x;
            } else {
                y += step_y;
                t_y += delta_y;
            }
            if (x, y) == (tx, ty) {
                return true;
            }
            if !self.inside(x, y) || self.cells[self.index(x, y)].opaque {
                return false;
            }
        }
        false
    }

    /// Where the straight line from `from` to `to` first enters an opaque
    /// tile, if it does before it gets there: the point on the tile's
    /// edge. The same traversal as [`Sight::clear_line`], to a point
    /// rather than a tile's middle — what stops a shot.
    pub fn first_opaque_along(&self, from: Vec2, to: Vec2) -> Option<Vec2> {
        let (mut x, mut y) = self.tile_of(from);
        let (tx, ty) = self.tile_of(to);
        if (x, y) == (tx, ty) {
            return None;
        }
        let d = to - from;
        let step_x: i32 = if d.x > 0.0 { 1 } else { -1 };
        let step_y: i32 = if d.y > 0.0 { 1 } else { -1 };
        let next_x = self.origin.x + (x + if d.x > 0.0 { 1 } else { 0 }) as f32 * self.tile;
        let next_y = self.origin.y + (y + if d.y > 0.0 { 1 } else { 0 }) as f32 * self.tile;
        let (mut t_x, delta_x) = if d.x.abs() > 1e-6 {
            ((next_x - from.x) / d.x, self.tile / d.x.abs())
        } else {
            (f32::INFINITY, f32::INFINITY)
        };
        let (mut t_y, delta_y) = if d.y.abs() > 1e-6 {
            ((next_y - from.y) / d.y, self.tile / d.y.abs())
        } else {
            (f32::INFINITY, f32::INFINITY)
        };
        let most = (tx - x).abs() + (ty - y).abs() + 2;
        for _ in 0..most {
            // Where the line crosses into the next tile, as a fraction of
            // the whole of it.
            let t = if t_x < t_y {
                let t = t_x;
                x += step_x;
                t_x += delta_x;
                t
            } else {
                let t = t_y;
                y += step_y;
                t_y += delta_y;
                t
            };
            if t >= 1.0 {
                return None;
            }
            if self.opaque_at(x, y) {
                return Some(from + d * t);
            }
            if (x, y) == (tx, ty) {
                return None;
            }
        }
        None
    }

    /// Whether anybody sees the tile a point is in.
    pub fn seen_at(&self, p: Vec2) -> bool {
        let (x, y) = self.tile_of(p);
        self.inside(x, y) && self.seen[self.index(x, y)]
    }

    /// What is drawn over the tile a point is in, as of the last trace:
    /// 0 nothing, 1 the semi fog of a friendly structure, 2 the grey ring
    /// round what is seen of a stranger's, 3 the black beyond it. For the
    /// probes.
    pub fn veil_at(&self, p: Vec2) -> u32 {
        let (x, y) = self.tile_of(p);
        if !self.inside(x, y) {
            return 0;
        }
        match self.veil(self.index(x, y), false) {
            None => 0,
            Some(Veil::Semi) => 1,
            Some(Veil::Grey) => 2,
            Some(Veil::Black) => 3,
        }
    }

    /// How the fog over a tile is drawn, if it is: by whose the tile is,
    /// and — for somebody else's — whether it is within the ring of what
    /// is seen. With `all`, nothing is seen and there is no ring.
    fn veil(&self, i: usize, all: bool) -> Option<Veil> {
        let c = &self.cells[i];
        if !c.fogged || (!all && self.seen[i]) {
            return None;
        }
        let stance = if c.foreign { self.foreign } else { self.own };
        Some(match stance {
            Stance::Friendly => Veil::Semi,
            _ if !all && self.near[i] => Veil::Grey,
            _ => Veil::Black,
        })
    }

    /// The fog, over every fogged tile nobody sees — or, with `all`, over
    /// every fogged tile, for a room nobody is looking into. Each kind of
    /// veil is one pass: neighbouring tiles are drawn as one rectangle
    /// wherever they can be, so the fog is a few shapes rather than a
    /// thousand, and a run that matches the run under it is one shape
    /// taller.
    pub fn draw(&self, list: &mut DrawList, all: bool) {
        for (veil, colour) in [
            (Veil::Semi, FOG),
            (Veil::Grey, FOG_GREY),
            (Veil::Black, FOG_BLACK),
        ] {
            self.draw_runs(list, colour, |i| self.veil(i, all) == Some(veil));
        }
    }

    fn draw_runs(&self, list: &mut DrawList, colour: Color, fogged: impl Fn(usize) -> bool) {
        let fogged = |x: i32, y: i32| fogged(self.index(x, y));
        // Runs along each row, then the same runs in consecutive rows
        // stacked.
        let mut open: Vec<(i32, i32, i32)> = Vec::new(); // (x0, x1, y0)
        for y in 0..self.rows {
            let mut runs: Vec<(i32, i32)> = Vec::new();
            let mut x = 0;
            while x < self.columns {
                if fogged(x, y) {
                    let x0 = x;
                    while x < self.columns && fogged(x, y) {
                        x += 1;
                    }
                    runs.push((x0, x));
                } else {
                    x += 1;
                }
            }
            let mut next: Vec<(i32, i32, i32)> = Vec::new();
            for &(x0, x1) in &runs {
                match open.iter().position(|&(a, b, _)| a == x0 && b == x1) {
                    Some(i) => next.push(open.swap_remove(i)),
                    None => next.push((x0, x1, y)),
                }
            }
            for (x0, x1, y0) in open.drain(..) {
                self.fog_rect(list, colour, x0, x1, y0, y);
            }
            open = next;
        }
        for (x0, x1, y0) in open {
            self.fog_rect(list, colour, x0, x1, y0, self.rows);
        }
    }

    fn fog_rect(&self, list: &mut DrawList, colour: Color, x0: i32, x1: i32, y0: i32, y1: i32) {
        let min = self.origin + vec2(x0 as f32 * self.tile, y0 as f32 * self.tile);
        let size = vec2((x1 - x0) as f32 * self.tile, (y1 - y0) as f32 * self.tile);
        // An opaque rectangle overlaps its neighbours by a hair: every
        // edge is feathered a pixel wide, and two black rectangles meeting
        // edge to edge showed the seam as a lighter line. A translucent
        // one may not, since the overlap would be twice as dark.
        let grow = if colour.a >= 1.0 { OVERLAP } else { 0.0 };
        list.rect(min + size * 0.5, size + vec2(grow, grow), 0.0, 0.0, colour);
    }

    pub fn tiles(&self) -> i32 {
        self.columns * self.rows
    }
}

/// What a room draws of the fog, and whether its bodies are drawn: whose
/// eyes the picture is through.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Fog {
    /// The crew's own: what they see is lit, the rest is fogged, and every
    /// body is the crew's, so every body is drawn.
    #[default]
    Crew,
    /// Nobody's: a room looked at from outside, a station's with the ship
    /// alongside. All of it fogged, and nobody in it drawn.
    All,
    /// Somebody else's: a station's room under a joined deck, whose fog is
    /// the joined room's to draw. No fog here, and a body is drawn only
    /// when the world says it is in the crew's view (`Game::set_seen`).
    None,
}
