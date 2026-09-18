//! Arms, armour, and the shots in the air.
//!
//! Every Bim carries a [`Gear`]: three armour slots, empty so far, and a
//! weapon slot with the hand laser everybody is issued. A Bim under orders
//! — recruited — is in **combat mode**: it draws the weapon and, whenever
//! an enemy is in range and in its own line of sight (`crate::sight`, the
//! peek round a wall included), fires. The shot is a [`Bolt`]: a thing
//! that flies, at the weapon's speed, until it reaches a body, a wall or
//! the end of its range. Nothing here decides who is an enemy — the world
//! hands the room the targets (`Game::set_hostiles`) and takes the hits
//! back (`Game::take_hits`), since the enemies live in a room of their own.
//!
//! # Shots and bolts: the fight is drawn in one room
//!
//! The crew and a station's people are in two rooms, and a bolt that flew
//! in both would be two bolts. So a **friendly** bolt hits the targets the
//! world named, and a **hostile** bolt — red — hits this room's own bodies;
//! and a room whose bodies are hostile (`Game::set_hostile_bodies`) fires
//! no bolts at all but records a [`Shot`], which the world carries into
//! the crew's room and fires there as a hostile bolt (`Game::enemy_fire`).
//! Every bolt therefore flies, lands and is drawn on the crew's deck, and
//! the wound is applied where the body is: a hit on a target goes out
//! through [`Combat::take_hits`] for the world to carry to the residents'
//! room, a hit on one of our own goes onto [`Combat::wounds_taken`] and
//! the game applies it itself. A [`Hit`] carries the **part** it landed
//! on, rolled from the combat stream the instant the bolt reaches the body
//! ([`crate::health::Part::hit_by`]).
//!
//! # The enemy's tactics
//!
//! An enemy knows where the crew are — the world tells its room, the way
//! it tells the crew's — and [`Tactics::stand`] is where it chooses to
//! stand: a spot in range of a crew member from which it can shoot, and
//! for choice one **against a wall** where only the peek round it sees the
//! target, so the target's own shot has a wall in the way. Cover first,
//! then the open at the greatest distance the weapon reaches, less a
//! little for every tile of walking; the spot it stands on already gets a
//! bonus so it does not dither between two as good as each other.
//!
//! No strings: a weapon is a code and its stats are numbers, and the app
//! names them (`WEAPON_NAMES`, `ARMOUR_NAMES` in `crates/app/src/names.rs`).
//!
//! # Accuracy is a number at ten tiles
//!
//! A weapon's `accuracy` is its odds of a hit on a body **ten tiles away**
//! — [`ACCURACY_RANGE`] — and the odds fall off with distance from there:
//! `accuracy ^ (tiles / 10)`, so a 70% pistol is about 84% at five tiles
//! and 49% at twenty. A shot that is going to hit is aimed at the body; one
//! that is going to miss is aimed a little wide of it and flies past,
//! which is what a miss should look like. The roll is drawn from the
//! combat's own stream, so a fight re-rolls nothing in the rest of the
//! room — see `crates/game/CLAUDE.md` on what re-rolling does to probes.
//!
//! # Time
//!
//! The room's `dt` is real seconds at 1x, so a weapon's speed and fire rate
//! are per real second at 1x and the world's 24x is twenty-four times as
//! fast, the way everything else aboard is.

use crate::draw::{Color, DrawList};
use crate::health::Part;
use crate::math::Vec2;
use crate::nav::Nav;
use crate::rng::Rng;
use crate::room::TILE;
use crate::sight::Sight;

/// The distance a weapon's accuracy is quoted at, in tiles.
pub const ACCURACY_RANGE: f32 = 10.0;

/// How near a bolt has to pass a body's middle to hit it, in room units:
/// the body's own half-width, near enough.
pub const HIT_RADIUS: f32 = 17.0;

/// How wide of the body a miss is aimed, in room units: past the hit
/// radius by a clear margin, so a miss visibly whistles by.
const MISS_BY: f32 = HIT_RADIUS + 22.0;

/// How long a bolt is drawn, in room units, and how thick.
const BOLT_LENGTH: f32 = 42.0;
const BOLT_CORE: f32 = 3.5;
const BOLT_GLOW: f32 = 11.0;

/// How long the flash where a bolt lands lasts, in seconds.
const SPARK_LIFE: f32 = 0.18;

/// Friendly fire is blue and an enemy's is red — always, so a glance at
/// the air says who is shooting whom.
pub const FRIENDLY_BOLT: Color = Color::rgb(0.40, 0.72, 1.0);
pub const HOSTILE_BOLT: Color = Color::rgb(1.0, 0.28, 0.22);
const BOLT_CORE_WHITE: Color = Color::rgb(0.92, 0.97, 1.0);

/// What a Bim can shoot with. The discriminants are the codes the app
/// names, written out so a reordering cannot renumber anything; `0` is
/// "nothing in the slot", as everywhere else.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum WeaponKind {
    /// The hand laser everybody starts with.
    LaserPistol = 1,
}

impl WeaponKind {
    pub const ALL: [WeaponKind; 1] = [WeaponKind::LaserPistol];

    pub fn code(self) -> u32 {
        self as u32
    }

    /// The weapon's numbers. One table, here, for a native server to
    /// agree with.
    pub fn stats(self) -> WeaponStats {
        match self {
            WeaponKind::LaserPistol => WeaponStats {
                range: 12.0,
                accuracy: 0.70,
                speed: 18.0,
                damage: 12.0,
                fire_rate: 1.5,
            },
        }
    }
}

/// What a weapon does, in the units the app prints them in.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct WeaponStats {
    /// How far it reaches, in tiles. Nothing beyond is shot at, and a bolt
    /// dies there.
    pub range: f32,
    /// Odds of a hit at [`ACCURACY_RANGE`] tiles, 0 to 1.
    pub accuracy: f32,
    /// How fast the shot flies, in tiles a second.
    pub speed: f32,
    /// Health points taken off per shot that lands.
    pub damage: f32,
    /// Shots a second.
    pub fire_rate: f32,
}

impl WeaponStats {
    /// What it could do a second with every shot landing.
    pub fn dps(&self) -> f32 {
        self.fire_rate * self.damage
    }

    /// Odds of a hit on a body `tiles` away. See the module note.
    pub fn hit_chance(&self, tiles: f32) -> f32 {
        self.accuracy.powf(tiles.max(0.0) / ACCURACY_RANGE)
    }

    /// Range and speed in room units.
    pub fn reach(&self) -> f32 {
        self.range * TILE
    }

    fn pace(&self) -> f32 {
        self.speed * TILE
    }
}

/// What a Bim can wear. Nothing yet: the slots exist so the inventory has
/// somewhere to put a helmet when there is one. An empty enum, so that the
/// name table pins to nought and grows with it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ArmourKind {}

impl ArmourKind {
    pub fn code(self) -> u32 {
        match self {}
    }
}

/// What one Bim has on it: three armour slots, top to bottom, and the
/// weapon in its hand.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Gear {
    pub head: Option<ArmourKind>,
    pub body: Option<ArmourKind>,
    pub legs: Option<ArmourKind>,
    pub weapon: Option<WeaponKind>,
}

impl Gear {
    /// What everybody is issued: nothing to wear, and a hand laser.
    pub fn issued() -> Gear {
        Gear {
            weapon: Some(WeaponKind::LaserPistol),
            ..Gear::default()
        }
    }
}

/// One shot in the air.
#[derive(Clone, Copy, Debug)]
pub struct Bolt {
    pub pos: Vec2,
    /// Room units a second.
    pub vel: Vec2,
    /// Room units it may still fly.
    pub left: f32,
    pub damage: f32,
    /// Fired by an enemy: red, and looking for this room's own bodies
    /// rather than for the targets. See the module note.
    pub hostile: bool,
}

/// A bolt that reached a body: whose — an index into the targets for a
/// friendly bolt, into this room's own crew for a hostile one — where on
/// it, and how hard.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Hit {
    pub who: usize,
    pub part: Part,
    pub damage: f32,
}

/// A shot an enemy took, for the world to carry into the crew's room and
/// fire there. Where from — the eye it was aimed from, which for a peek
/// is beside the body — where at, and with what.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Shot {
    pub from: Vec2,
    pub at: Vec2,
    pub weapon: WeaponKind,
}

/// Where a bolt ended, briefly lit.
#[derive(Clone, Copy, Debug)]
struct Spark {
    pos: Vec2,
    age: f32,
    hostile: bool,
}

/// The fight, as the room keeps it: the enemies the world named, the bolts
/// flying, and the hits that landed since the world last asked.
pub struct Combat {
    /// Where the enemies stand, index for index with whoever the world
    /// says they are; `None` for one that is down. Empty in peace.
    targets: Vec<Option<Vec2>>,
    pub bolts: Vec<Bolt>,
    sparks: Vec<Spark>,
    /// Every friendly bolt that landed on a target, for the world to carry
    /// to the body it belongs to.
    hits: Vec<Hit>,
    /// Every hostile bolt that landed on one of this room's own bodies.
    /// The game applies these itself, every step, and keeps a copy for
    /// the world to log.
    pub wounds_taken: Vec<Hit>,
    /// The shots this room's people took while its bodies were hostile —
    /// recorded here rather than flown, for the world to fire in the
    /// crew's room. See the module note.
    pub shots: Vec<Shot>,
    rng: Rng,
}

impl Combat {
    pub fn new(seed: u64) -> Combat {
        Combat {
            targets: Vec::new(),
            bolts: Vec::new(),
            sparks: Vec::new(),
            hits: Vec::new(),
            wounds_taken: Vec::new(),
            shots: Vec::new(),
            // Its own stream: a fight must not re-roll the room.
            rng: Rng::new(seed ^ 0xC0B_A7),
        }
    }

    pub fn set_targets(&mut self, targets: Vec<Option<Vec2>>) {
        self.targets = targets;
    }

    pub fn targets(&self) -> &[Option<Vec2>] {
        &self.targets
    }

    pub fn take_hits(&mut self) -> Vec<Hit> {
        std::mem::take(&mut self.hits)
    }

    pub fn take_shots(&mut self) -> Vec<Shot> {
        std::mem::take(&mut self.shots)
    }

    /// A shot taken but not flown: what a hostile room's people do
    /// instead of firing, so the bolt flies where the crew are.
    pub fn shoot(&mut self, from: Vec2, at: Vec2, weapon: WeaponKind) {
        self.shots.push(Shot { from, at, weapon });
    }

    /// The nearest enemy a shooter standing at `from` can see, within the
    /// weapon's range: which one, the eye it is seen from — the body, or the
    /// peek beside a wall — and where it stands.
    pub fn aim(
        &self,
        sight: &Sight,
        from: Vec2,
        stats: &WeaponStats,
    ) -> Option<(usize, Vec2, Vec2)> {
        let reach = stats.reach();
        let mut best: Option<(f32, usize, Vec2, Vec2)> = None;
        for (i, target) in self.targets.iter().enumerate() {
            let Some(at) = *target else {
                continue;
            };
            let d = (at - from).len();
            if d > reach || best.is_some_and(|b| b.0 <= d) {
                continue;
            }
            if let Some(eye) = sight.sees_from(from, at) {
                best = Some((d, i, eye, at));
            }
        }
        best.map(|(_, i, eye, at)| (i, eye, at))
    }

    /// A shot from `from` at a body at `at`. Whether it will hit is rolled
    /// now, against the distance; a miss is aimed wide.
    pub fn fire(&mut self, from: Vec2, at: Vec2, stats: &WeaponStats, hostile: bool) {
        let to = at - from;
        let tiles = to.len() / TILE;
        let hits = self.rng.chance(stats.hit_chance(tiles));
        let aim = if hits {
            at
        } else {
            let side = if self.rng.chance(0.5) { 1.0 } else { -1.0 };
            at + to.normalize_or_zero().perp() * (MISS_BY * side)
        };
        let dir = (aim - from).normalize_or_zero();
        if dir == Vec2::ZERO {
            return;
        }
        self.bolts.push(Bolt {
            pos: from,
            vel: dir * stats.pace(),
            left: stats.reach(),
            damage: stats.damage,
            hostile,
        });
    }

    /// Fly every bolt on by `dt`: into a body, into a wall, or out to the
    /// end of its range, whichever comes first. A friendly bolt looks for
    /// the targets; a hostile one for `bodies`, this room's own — each
    /// Bim's position while it is alive and on the deck, conscious or
    /// not, and `None` for one dead or outside. Where a bolt reaches a
    /// body the part it lands on is rolled then, from the combat stream.
    pub fn step(&mut self, dt: f32, sight: &Sight, bodies: &[Option<Vec2>]) {
        for s in &mut self.sparks {
            s.age += dt;
        }
        self.sparks.retain(|s| s.age < SPARK_LIFE);

        let targets = &self.targets;
        let rng = &mut self.rng;
        let mut landed: Vec<Hit> = Vec::new();
        let mut taken: Vec<Hit> = Vec::new();
        let mut sparks: Vec<Spark> = Vec::new();
        self.bolts.retain_mut(|bolt| {
            let mut flight = bolt.vel * dt;
            let mut span = flight.len();
            if span > bolt.left {
                flight = flight * (bolt.left / span.max(1e-6));
                span = bolt.left;
            }
            let from = bolt.pos;
            let to = from + flight;
            // The first thing along the way: a wall, or a body. Each as a
            // fraction of this step's flight.
            let mut stop: Option<(f32, Option<usize>)> = None;
            if let Some(wall) = sight.first_opaque_along(from, to) {
                stop = Some(((wall - from).len() / span.max(1e-6), None));
            }
            let looking_for: &[Option<Vec2>] = if bolt.hostile { bodies } else { targets };
            for (i, body) in looking_for.iter().enumerate() {
                let Some(body) = *body else {
                    continue;
                };
                if let Some(t) = along(from, to, body, HIT_RADIUS)
                    && stop.is_none_or(|(s, _)| t < s)
                {
                    stop = Some((t, Some(i)));
                }
            }
            match stop {
                Some((t, who)) => {
                    let at = from + flight * t;
                    if let Some(who) = who {
                        let hit = Hit {
                            who,
                            part: Part::hit_by(rng.unit()),
                            damage: bolt.damage,
                        };
                        if bolt.hostile {
                            taken.push(hit);
                        } else {
                            landed.push(hit);
                        }
                    }
                    sparks.push(Spark {
                        pos: at,
                        age: 0.0,
                        hostile: bolt.hostile,
                    });
                    false
                }
                None => {
                    bolt.pos = to;
                    bolt.left -= span;
                    if bolt.left <= 0.0 {
                        // Spent: it fades where it got to, without a flash.
                        return false;
                    }
                    true
                }
            }
        });
        self.hits.extend(landed);
        self.wounds_taken.extend(taken);
        self.sparks.extend(sparks);
    }

    /// Whether anything is in the air or lit.
    pub fn quiet(&self) -> bool {
        self.bolts.is_empty() && self.sparks.is_empty()
    }

    /// The bolts and the sparks. Over the fog — a shot is always seen.
    pub fn draw(&self, list: &mut DrawList) {
        for bolt in &self.bolts {
            let dir = bolt.vel.normalize_or_zero();
            let head = bolt.pos;
            let tail = head - dir * BOLT_LENGTH;
            let colour = if bolt.hostile {
                HOSTILE_BOLT
            } else {
                FRIENDLY_BOLT
            };
            list.line(tail, head, BOLT_GLOW, colour.alpha(0.30));
            list.line(tail, head, BOLT_CORE + 2.0, colour.alpha(0.85));
            list.line(
                tail + dir * (BOLT_LENGTH * 0.35),
                head,
                BOLT_CORE,
                BOLT_CORE_WHITE,
            );
        }
        for spark in &self.sparks {
            let t = 1.0 - spark.age / SPARK_LIFE;
            let colour = if spark.hostile {
                HOSTILE_BOLT
            } else {
                FRIENDLY_BOLT
            };
            list.circle(spark.pos, 8.0 + 14.0 * (1.0 - t), colour.alpha(0.55 * t));
            list.circle(spark.pos, 5.0 * t, BOLT_CORE_WHITE.alpha(0.9 * t));
        }
    }
}

/// Where along the segment `a`–`b`, as a fraction, a circle of `radius` at
/// `centre` is first touched, if it is.
fn along(a: Vec2, b: Vec2, centre: Vec2, radius: f32) -> Option<f32> {
    let d = b - a;
    let len2 = d.dot(d);
    if len2 <= 1e-9 {
        return ((centre - a).len() <= radius).then_some(0.0);
    }
    // The nearest point of the line to the centre, clamped to the segment.
    let t = ((centre - a).dot(d) / len2).clamp(0.0, 1.0);
    let near = a + d * t;
    if (centre - near).len() > radius {
        return None;
    }
    // Back off along the segment to where the circle's edge is crossed, so
    // the flash sits on the body's edge rather than in its middle.
    let back = (radius * radius - (centre - near).dot(centre - near))
        .max(0.0)
        .sqrt()
        / len2.sqrt();
    Some((t - back).max(0.0))
}

/// What a candidate stand is worth to the enemy, before the walk to it is
/// taken off: cover with a peek that sees the target, or the open with the
/// target in plain sight, each plus the distance to the target so that the
/// furthest spot of its kind wins. See the module note.
const COVER_SCORE: f32 = 2000.0;
const OPEN_SCORE: f32 = 1000.0;
/// What a tile of walking costs against those.
const WALK_COST_PER_TILE: f32 = 4.0;
/// What the spot the enemy already stands on is worth over a fresh one,
/// so that two spots as good as each other do not have it walking
/// between them every plan.
const STAY_BONUS: f32 = 60.0;

/// The enemy's choice of where to stand. See the module note.
pub struct Tactics;

impl Tactics {
    /// Where a body at `from`, armed with `stats`, should stand to shoot
    /// at `targets` — `None` for a target that is down — or `None` when
    /// nowhere in reach of any of them can be shot from.
    ///
    /// The candidates are a tile-spaced lattice of free cells within the
    /// weapon's reach of each target (`Nav::free_cells_within`), kept to
    /// the ones the body can walk to, plus the spot it stands on now. Each
    /// is scored against every target in reach of it by what
    /// `Sight::eyes_from` sees from there: the body's own eye blind to the
    /// target but a peek beside a wall seeing it is cover, [`COVER_SCORE`]
    /// plus the distance; the body's eye seeing it is the open,
    /// [`OPEN_SCORE`] plus the distance; neither is nothing. The best
    /// target's score stands for the spot, less [`WALK_COST_PER_TILE`] a
    /// tile from `from`, plus [`STAY_BONUS`] for the spot it is on.
    pub fn stand(
        sight: &Sight,
        nav: &Nav,
        from: Vec2,
        targets: &[Option<Vec2>],
        stats: &WeaponStats,
    ) -> Option<Vec2> {
        let reach = stats.reach();
        let here = nav.nearest_free(from);
        let mut lattice: Vec<Vec2> = targets
            .iter()
            .flatten()
            .flat_map(|&target| nav.free_cells_within(target, reach, TILE))
            .collect();
        // Two targets in reach of one another offer the same spots
        // twice, and a spot scored twice is a trace wasted: the lattice is
        // the grid's, so equal spots are equal to the bit.
        lattice.sort_by(|a, b| {
            (a.y, a.x)
                .partial_cmp(&(b.y, b.x))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        lattice.dedup_by(|a, b| (*a - *b).len() <= 1e-3);
        let mut candidates: Vec<Vec2> = vec![here];
        candidates.extend(
            lattice
                .into_iter()
                .filter(|&c| (c - here).len() > 1e-3 && nav.can_reach(from, c)),
        );
        let mut best: Option<(f32, Vec2)> = None;
        for (i, &c) in candidates.iter().enumerate() {
            let Some(view) = Tactics::view_from(sight, c, targets, reach) else {
                continue;
            };
            let walk = (c - from).len() / TILE * WALK_COST_PER_TILE;
            let stay = if i == 0 { STAY_BONUS } else { 0.0 };
            let score = view - walk + stay;
            if best.is_none_or(|(b, _)| score > b) {
                best = Some((score, c));
            }
        }
        best.map(|(_, at)| at)
    }

    /// The best a spot is worth against any one target in reach of it,
    /// by what is seen from there. `None` when no target can be shot at
    /// from the spot.
    fn view_from(sight: &Sight, c: Vec2, targets: &[Option<Vec2>], reach: f32) -> Option<f32> {
        let eyes = sight.eyes_from(c);
        let mut best: Option<f32> = None;
        for target in targets.iter().flatten() {
            let d = (*target - c).len();
            if d > reach {
                continue;
            }
            let tile = sight.tile_of(*target);
            let mut body_sees = false;
            let mut peek_sees = false;
            for eye in &eyes {
                if !eye.admits(tile) || !sight.clear_line(eye.at, tile) {
                    continue;
                }
                if eye.is_peek() {
                    peek_sees = true;
                } else {
                    body_sees = true;
                }
            }
            let score = if !body_sees && peek_sees {
                COVER_SCORE + d
            } else if body_sees {
                OPEN_SCORE + d
            } else {
                continue;
            };
            if best.is_none_or(|b| score > b) {
                best = Some(score);
            }
        }
        best
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::BODY_MARGIN;
    use crate::math::{Rect, vec2};

    /// A room twenty tiles by ten with one wall down its middle, from the
    /// top to three tiles short of the bottom, so the two halves join
    /// through the gap. Tile-aligned, the way a ship's room is.
    fn walled_room() -> (Sight, Nav) {
        let wall = Rect::from_min_size(vec2(10.0 * TILE, 0.0), vec2(TILE, 7.0 * TILE));
        room_with(&[wall])
    }

    fn room_with(solids: &[Rect]) -> (Sight, Nav) {
        let interior = Rect::from_min_size(Vec2::ZERO, vec2(20.0 * TILE, 10.0 * TILE));
        let sight = Sight::new(interior, interior, TILE, solids, &[]);
        let nav = Nav::tiled(interior, solids, BODY_MARGIN, TILE);
        (sight, nav)
    }

    fn middle(x: f32, y: f32) -> Vec2 {
        vec2((x + 0.5) * TILE, (y + 0.5) * TILE)
    }

    /// Whether the body's own eye at `c` sees `target`, and whether a peek
    /// beside a wall does: the two halves of what the tactics score.
    fn views(sight: &Sight, c: Vec2, target: Vec2) -> (bool, bool) {
        let tile = sight.tile_of(target);
        let mut body = false;
        let mut peek = false;
        for eye in sight.eyes_from(c) {
            if eye.admits(tile) && sight.clear_line(eye.at, tile) {
                if eye.is_peek() {
                    peek = true;
                } else {
                    body = true;
                }
            }
        }
        (body, peek)
    }

    #[test]
    fn the_enemy_takes_cover_where_there_is_some_and_the_open_at_range_where_there_is_none() {
        let (sight, nav) = walled_room();
        let stats = WeaponKind::LaserPistol.stats();
        // The target on the right of the wall, the enemy on the left.
        let target = middle(14.0, 8.0);
        let from = middle(3.0, 2.0);
        let stand = Tactics::stand(&sight, &nav, from, &[Some(target)], &stats)
            .expect("somewhere in reach can be shot from");
        let (body, peek) = views(&sight, stand, target);
        assert!(
            !body && peek,
            "cover: the body's eye blind to the target and a peek seeing it, at {stand:?}"
        );
        assert!((stand - target).len() <= stats.reach());
        // The spot is against the wall — a tile beside it.
        let (x, _) = sight.tile_of(stand);
        assert!(x == 9 || x == 11, "against the wall, not {x}");

        // A room with nothing in it: nowhere to take cover, so the open at
        // the greatest distance the weapon reaches — furthest, but in reach.
        let (sight, nav) = room_with(&[]);
        let target = middle(15.0, 8.5);
        let from = middle(17.0, 9.0);
        let stand = Tactics::stand(&sight, &nav, from, &[Some(target)], &stats).unwrap();
        let (body, _) = views(&sight, stand, target);
        assert!(body, "the open");
        let d = (stand - target).len();
        assert!(d <= stats.reach() && d > stats.reach() - 2.0 * TILE, "{d}");

        // Nothing named: nowhere to stand.
        assert!(Tactics::stand(&sight, &nav, from, &[None], &stats).is_none());
    }

    #[test]
    fn a_hostile_bolt_finds_our_own_bodies_and_a_friendly_one_the_targets() {
        let (sight, _) = walled_room();
        let stats = WeaponKind::LaserPistol.stats();
        let mut combat = Combat::new(7);
        let ours = middle(15.0, 8.0);
        let theirs = middle(15.0, 2.0);
        combat.set_targets(vec![Some(theirs)]);
        // Straight down the corridor at each, from four tiles off, until
        // one lands: the roll is the combat stream's and a miss flies wide.
        let mut own = 0;
        let mut hits = 0;
        for _ in 0..40 {
            combat.fire(middle(19.0, 8.0), ours, &stats, true);
            combat.fire(middle(19.0, 2.0), theirs, &stats, false);
            for _ in 0..60 {
                combat.step(0.05, &sight, &[Some(ours)]);
            }
            own += combat.wounds_taken.len();
            combat.wounds_taken.clear();
            hits += combat.take_hits().len();
        }
        assert!(
            own > 20 && own < 40,
            "{own} of 40 hostile bolts landed on us"
        );
        assert!(
            hits > 20 && hits < 40,
            "{hits} of 40 friendly bolts landed on them"
        );

        // A hostile bolt never touches a target, nor a friendly one us.
        combat.fire(middle(19.0, 2.0), theirs, &stats, true);
        combat.fire(middle(19.0, 8.0), ours, &stats, false);
        for _ in 0..60 {
            combat.step(0.05, &sight, &[Some(ours)]);
        }
        assert!(combat.wounds_taken.is_empty() && combat.take_hits().is_empty());
        assert!(combat.quiet());

        // A shot is recorded, not flown.
        combat.shoot(ours, theirs, WeaponKind::LaserPistol);
        assert!(combat.bolts.is_empty());
        let shots = combat.take_shots();
        assert_eq!(shots.len(), 1);
        assert_eq!(shots[0].at, theirs);
        assert!(combat.take_shots().is_empty());
    }
}
