//! A mining site: the asteroids about a belt, as tiles a Bim mines on foot.
//!
//! A belt is a point on the map. Hold station at one and it becomes a
//! **place**: a field of asteroids laid out round the ship, each a blob of
//! tiles on the ship's own tile grid, close enough that a walk to the
//! nearest is minutes rather than hours. The ship is what the rocks are laid
//! out about — the field is generated when the ship first comes to rest at
//! the belt, from the belt's own seed, in the frame the ship is holding in —
//! so a suited Bim walks the same grid outside that it walks inside, and a
//! click on a rock is the same tile arithmetic as a click on the deck.
//!
//! # What an asteroid is made of
//!
//! Skin and core. Every tile within [`CORE_DEPTH`] of the outside is bare
//! [`Rock::Stone`], and only the tiles deeper than that are the ore — iron
//! on most of them, [`Rock::Galvum`] on about one asteroid in ten
//! ([`GALVUM_SHARE`]). So an asteroid has to be dug into, three tiles deep,
//! before it gives anything worth having, and which of them is the purple
//! one is visible from outside: the core shows through.
//!
//! # What the crew are told
//!
//! Nothing is mined that the player has not **marked**. A mark is a
//! [`crate::world::Command`], so two players' ships agree about which
//! rocks are wanted, and the room is handed the marked tiles that still
//! stand, every step, with every rock tile as something to walk round —
//! see `World::eva_offer`. What a tile yields when it is gone is
//! [`yield_of`], and it is the world's to add to the hold.

use shipdesign::parts::TILE;
use worldgen::rng::{Purpose, Rng, mix, seed_for};

/// What a tile of an asteroid is made of. The discriminants are what the
/// painter and the readout are indexed by.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum Rock {
    /// The skin: bare rock, worth next to nothing.
    Stone = 0,
    /// Iron ore, in the core of most asteroids.
    Iron = 1,
    /// Galvum, in the core of the rare one.
    Galvum = 2,
}

impl Rock {
    pub fn code(self) -> u32 {
        self as u32
    }
}

/// One tile of an asteroid, in the ship's design frame: tile `(x, y)` is
/// where design tile `(x, y)` would be, and a rock is off the hull so the
/// coordinates run negative and past the build area both.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct RockTile {
    pub x: i32,
    pub y: i32,
    pub kind: Rock,
}

/// The asteroids about one belt, and what the crew have been told to do
/// about them.
#[derive(Clone, PartialEq, Debug)]
pub struct MiningSite {
    /// The belt's body id.
    pub belt: u32,
    /// Every tile still standing, sorted by `(y, x)` so the checksum and
    /// the picture walk it the same way everywhere. A mined tile is gone.
    pub tiles: Vec<RockTile>,
    /// The tiles the crew are to mine, in the order they were marked. Only
    /// tiles that stand; a mined tile's mark goes with it.
    pub marked: Vec<(i32, i32)>,
}

/// How many asteroids a site has, least and most.
pub const ASTEROIDS: (u32, u32) = (8, 12);
/// An asteroid's half-width, in tiles, least and most.
pub const ASTEROID_RADIUS: (i32, i32) = (4, 7);
/// How deep the skin is: every tile nearer the outside than this is stone,
/// and the ore begins at this depth.
pub const CORE_DEPTH: u32 = 3;
/// What share of a site's asteroids carry galvum rather than iron.
pub const GALVUM_SHARE: f64 = 0.10;
/// How far the nearest rock keeps from the hull, in tiles, and how much
/// further out the furthest may lie. Close: the point of the site is that a
/// walk to the rock is minutes.
pub const CLEARANCE: i32 = 4;
pub const SPREAD: i32 = 18;

/// What one tile gives up when it is mined, as a resource and units of it.
pub fn yield_of(kind: Rock) -> (physics::ResourceId, u32) {
    match kind {
        Rock::Stone => (physics::ResourceId::Rock, 2),
        Rock::Iron => (physics::ResourceId::Ore, 2),
        Rock::Galvum => (physics::ResourceId::Galvum, 1),
    }
}

/// The middle of a rock tile, in design world units — the room's units.
pub fn tile_middle(x: i32, y: i32) -> (f64, f64) {
    let t = TILE as f64;
    ((x as f64 + 0.5) * t, (y as f64 + 0.5) * t)
}

impl MiningSite {
    /// The site for `belt`, laid out about a hull whose tiles span
    /// `hull` — `(x0, y0, x1, y1)`, inclusive — from the galaxy's seed.
    ///
    /// Deterministic in every draw: the same belt about the same hull is
    /// the same field for two players. A stream of its own
    /// ([`Purpose::MiningSite`]), so it moved nothing else in the galaxy.
    pub fn generate(
        galaxy_seed: u64,
        star_id: u32,
        belt: u32,
        hull: (i32, i32, i32, i32),
    ) -> MiningSite {
        let seed = seed_for(
            galaxy_seed,
            star_id,
            worldgen::GENERATOR_VERSION,
            Purpose::MiningSite,
        );
        let mut rng = Rng::new(seed ^ mix(belt as u64));

        let (x0, y0, x1, y1) = hull;
        let cx = (x0 + x1) as f64 / 2.0;
        let cy = (y0 + y1) as f64 / 2.0;
        let half_w = (x1 - x0 + 1) as f64 / 2.0;
        let half_h = (y1 - y0 + 1) as f64 / 2.0;
        // Everything of the hull lies inside this circle about its middle.
        let hull_radius = (half_w * half_w + half_h * half_h).sqrt().ceil() as i32;

        let count = ASTEROIDS.0 + rng.below(ASTEROIDS.1 - ASTEROIDS.0 + 1);
        // A tenth of them carry galvum: the whole tenths for certain, the
        // remainder as a chance, so a site of twelve has one and one in
        // five has a second.
        let whole = (count as f64 * GALVUM_SHARE).floor() as u32;
        let remainder = count as f64 * GALVUM_SHARE - whole as f64;
        let galvum = whole + u32::from(rng.chance(remainder));
        let mut rich: Vec<u32> = Vec::new();
        while (rich.len() as u32) < galvum.min(count) {
            let pick = rng.below(count);
            if !rich.contains(&pick) {
                rich.push(pick);
            }
        }

        // Place each asteroid: a middle in a ring about the hull, clear of
        // the hull and of every asteroid placed before it. A placement
        // that will not fit after a few tries is a rock the site does
        // without — the draws are made either way, so the stream stays in
        // step.
        let mut placed: Vec<(i32, i32, i32)> = Vec::new();
        let mut tiles: Vec<RockTile> = Vec::new();
        for i in 0..count {
            let radius = ASTEROID_RADIUS.0
                + rng.below((ASTEROID_RADIUS.1 - ASTEROID_RADIUS.0 + 1) as u32) as i32;
            let squash = rng.range(0.6, 1.0);
            let mut spot = None;
            for _ in 0..24 {
                let near = (hull_radius + CLEARANCE + radius) as f64;
                let distance = rng.range(near, near + SPREAD as f64);
                let angle = rng.angle();
                let ax = (cx + distance * angle.cos()).round() as i32;
                let ay = (cy + distance * angle.sin()).round() as i32;
                let clear = placed.iter().all(|&(px, py, pr)| {
                    let dx = (px - ax) as f64;
                    let dy = (py - ay) as f64;
                    (dx * dx + dy * dy).sqrt() >= (pr + radius + 2) as f64
                });
                if spot.is_none() && clear {
                    spot = Some((ax, ay));
                }
            }
            // The ragged edge is drawn per tile whether or not the rock is
            // placed, for the same reason.
            let mut blob: Vec<(i32, i32)> = Vec::new();
            for dy in -radius..=radius {
                for dx in -radius..=radius {
                    let nx = dx as f64 / radius as f64;
                    let ny = dy as f64 / (radius as f64 * squash);
                    let norm = nx * nx + ny * ny;
                    let edge = 1.0 - 0.3 * rng.unit();
                    if norm <= edge {
                        blob.push((dx, dy));
                    }
                }
            }
            let Some((ax, ay)) = spot else {
                continue;
            };
            placed.push((ax, ay, radius));
            let core = if rich.contains(&i) {
                Rock::Galvum
            } else {
                Rock::Iron
            };
            for (&(dx, dy), depth) in blob.iter().zip(depths(&blob, radius)) {
                tiles.push(RockTile {
                    x: ax + dx,
                    y: ay + dy,
                    kind: if depth >= CORE_DEPTH {
                        core
                    } else {
                        Rock::Stone
                    },
                });
            }
        }
        tiles.sort_by_key(|t| (t.y, t.x));
        tiles.dedup_by_key(|t| (t.y, t.x));
        MiningSite {
            belt,
            tiles,
            marked: Vec::new(),
        }
    }

    /// The tile standing at `(x, y)`, if one is.
    pub fn at(&self, x: i32, y: i32) -> Option<Rock> {
        self.tiles
            .iter()
            .find(|t| t.x == x && t.y == y)
            .map(|t| t.kind)
    }

    /// Whether `(x, y)` is marked to be mined.
    pub fn is_marked(&self, x: i32, y: i32) -> bool {
        self.marked.contains(&(x, y))
    }

    /// Mark a standing tile to be mined, or unmark a marked one. A tile
    /// that is not there is refused: there is nothing to mark.
    pub fn toggle_mark(&mut self, x: i32, y: i32) -> bool {
        if self.at(x, y).is_none() {
            return false;
        }
        if let Some(i) = self.marked.iter().position(|&m| m == (x, y)) {
            self.marked.remove(i);
        } else {
            self.marked.push((x, y));
        }
        true
    }

    pub fn clear_marks(&mut self) {
        self.marked.clear();
    }

    /// The marked tiles, in the order they were marked. What the room is
    /// handed to walk to.
    pub fn targets(&self) -> impl Iterator<Item = (i32, i32)> + '_ {
        self.marked.iter().copied()
    }

    /// Take a tile out: what it was made of, or `None` if it was not there.
    /// Its mark goes with it.
    pub fn mine(&mut self, x: i32, y: i32) -> Option<Rock> {
        let i = self.tiles.iter().position(|t| t.x == x && t.y == y)?;
        let kind = self.tiles.remove(i).kind;
        self.marked.retain(|&m| m != (x, y));
        Some(kind)
    }
}

/// How many tiles in from the outside each tile of a blob is, four ways:
/// one on the edge, two beside the edge, and so on inwards. A flood from
/// the outside in over a grid of the blob's own size, so it is one walk of
/// the rock rather than one per tile.
fn depths(blob: &[(i32, i32)], radius: i32) -> Vec<u32> {
    let side = (2 * radius + 1) as usize;
    let index = |(dx, dy): (i32, i32)| ((dy + radius) as usize) * side + (dx + radius) as usize;
    let mut inside = vec![false; side * side];
    for &p in blob {
        inside[index(p)] = true;
    }
    let mut depth = vec![0u32; side * side];
    let outside = |(x, y): (i32, i32), inside: &[bool]| {
        x < -radius || x > radius || y < -radius || y > radius || !inside[index((x, y))]
    };
    let mut ring: Vec<(i32, i32)> = blob
        .iter()
        .copied()
        .filter(|&(x, y)| {
            [(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)]
                .into_iter()
                .any(|n| outside(n, &inside))
        })
        .collect();
    for &p in &ring {
        depth[index(p)] = 1;
    }
    let mut at = 1;
    while !ring.is_empty() {
        at += 1;
        let mut next = Vec::new();
        for &(x, y) in &ring {
            for n in [(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)] {
                if !outside(n, &inside) && depth[index(n)] == 0 {
                    depth[index(n)] = at;
                    next.push(n);
                }
            }
        }
        ring = next;
    }
    // A tile the flood never reached is the middle of a rock with no
    // outside, which cannot happen; it reads as deep.
    blob.iter()
        .map(|&p| {
            let d = depth[index(p)];
            if d == 0 { u32::MAX } else { d }
        })
        .collect()
}
