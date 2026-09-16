//! What is in a system: its bodies, and the stations built on them.
//!
//! Generated **lazily and from a hash**, never from a running stream. Ask for
//! star 400's system and you get the same answer whether it is the first
//! system you have looked at or the nine hundredth, on a client or on a
//! server — see [`crate::rng`] for why that is worth the trouble.
//!
//! # This step places nothing
//!
//! A [`StationBlueprint`] is not a map. There are no tiles here, no rooms, no
//! items on the deck — only the facts a later map generator will need before
//! it can make any of those: what kind of station it is, how much of it is
//! wrecked, and one seed to build the interior from. Every field earns its
//! place by being something that generator either places or honours.
//!
//! # How a system gets its shape
//!
//! Constructively, outwards, one body at a time. The first goes down at a
//! seeded distance from the star; each one after it is a **hop** from
//! somewhere already placed — far enough to be a real trip, near enough to
//! reach — and is rejected and re-drawn if it lands too close to anything
//! else. Building it this way rather than scattering and checking afterwards
//! is what makes the maximum rule free: every body arrives within one hop of
//! the system it is joining, so the whole thing is connected before anybody
//! checks.

use crate::data::{self, BodyKind, HazardKind, StationKind};
use crate::galaxy::Galaxy;
use crate::layout;
use crate::math::{DVec2, dvec2};
use crate::name::{self, Name};
use crate::rng::{Purpose, Rng};

// No lobby setting reaches world generation at all any more.
//
// There used to be one — a multiplier on what a station had in its stores —
// and it went when the crew started bringing **money** instead of starting
// with stores. A galaxy is now identified by its seed, its type and the
// generator version and by **nothing else**, which is a stronger version of
// the rule `WorldSettings` was written to keep: two players whose lobbies
// disagree about anything at all are still looking at the same stars, the
// same planets and the same stations in the same places. Anything that wants
// to reintroduce a setting here has to answer why it is a fact about the
// world rather than a starting condition.

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Body {
    pub id: u32,
    pub kind: BodyKind,
    /// Relative to its star, which is the origin of the system.
    pub position: DVec2,
    pub name: Name,
}

/// Everything a map generator will need to build a station, and nothing else.
#[derive(Clone, PartialEq, Debug)]
pub struct StationBlueprint {
    pub id: u32,
    pub kind: StationKind,
    /// The body it is attached to. `None` means deep space, and then
    /// `position` is relative to the star instead.
    pub parent_body: Option<u32>,
    pub position: DVec2,
    pub name: Name,
    /// Wrecks to pull apart. Only derelicts have any — see
    /// [`data::salvage_sites`], which is where an exception would go.
    pub salvage_sites: u32,
    /// **Stored, never displayed.** The map generator places these; a preview
    /// that listed them would let a player rule a station out without going.
    pub hazard_sites: Vec<(HazardKind, u32)>,
    /// For the map generator, and derived rather than drawn, so a station's
    /// interior does not move when the station beside it gains a hazard.
    pub map_seed: u64,
}

/// One thing in a system that can be flown to.
///
/// The star is deliberately **not** one. Nothing lands on a star, nothing is
/// built on one, and counting it as a node would put a minimum separation
/// between the star and its own innermost planet for no reason anybody could
/// name.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Node {
    Body(u32),
    Station(u32),
}

#[derive(Clone, PartialEq, Debug)]
pub struct StarSystem {
    pub star_id: u32,
    /// How abandoned it is, in `[0, 1]`. **Stored, never displayed** — it is
    /// an input to how spread out the system is and how likely a relay is to
    /// be sited here, and a number on the screen would do the exploring.
    pub desolation: f64,
    pub bodies: Vec<Body>,
    pub stations: Vec<StationBlueprint>,
}

impl StarSystem {
    pub fn body(&self, id: u32) -> Option<&Body> {
        self.bodies.iter().find(|b| b.id == id)
    }

    pub fn station(&self, id: u32) -> Option<&StationBlueprint> {
        self.stations.iter().find(|s| s.id == id)
    }

    /// Everything that can be flown to, bodies first.
    pub fn nodes(&self) -> Vec<Node> {
        self.bodies
            .iter()
            .map(|b| Node::Body(b.id))
            .chain(self.stations.iter().map(|s| Node::Station(s.id)))
            .collect()
    }

    /// Where a node is, with its parent chain added up.
    ///
    /// **The chain stops at the star.** A body is stored relative to its
    /// star and a station relative to its parent body, so summing those is
    /// well defined and this does it. Adding the star's own position on top
    /// would be assuming the galaxy is one continuous plane that systems sit
    /// in, and that is not decided — interstellar travel is a separate future
    /// technology and may well not be a matter of flying across a map at all.
    /// So: positions within a system, measured from the star. Nothing here
    /// mixes the two scales, and nothing downstream should either.
    pub fn absolute_position(&self, node: Node) -> Option<DVec2> {
        match node {
            Node::Body(id) => self.body(id).map(|b| b.position),
            Node::Station(id) => {
                let s = self.station(id)?;
                match s.parent_body {
                    None => Some(s.position),
                    Some(parent) => Some(self.body(parent)?.position.add(s.position)),
                }
            }
        }
    }

    /// Whether these two are a station and the body it is bolted to — the one
    /// pair the minimum separation does not apply to.
    pub fn attached(&self, a: Node, b: Node) -> bool {
        let bolted = |station: Node, body: Node| match (station, body) {
            (Node::Station(s), Node::Body(b)) => {
                self.station(s).and_then(|s| s.parent_body) == Some(b)
            }
            _ => false,
        };
        bolted(a, b) || bolted(b, a)
    }
}

impl Galaxy {
    /// The system around a star, generated on the spot.
    ///
    /// Returns `None` for a star that is not in this galaxy. Nothing is
    /// cached: it is a few dozen draws and a handful of allocations, and a
    /// cache would be one more thing that could disagree with the server.
    pub fn system(&self, star_id: u32) -> Option<StarSystem> {
        self.star(star_id)?;
        Some(generate(self, star_id))
    }
}

/// How many bodies a system can have. One is allowed and is a bleak little
/// place; the upper end is where a system stops being legible on a preview.
const MIN_BODIES: u32 = 1;
const MAX_BODIES: u32 = 7;

/// Tries at placing one body before it is given up on.
///
/// Giving up drops that body and carries on with the rest, which is right:
/// a system that has run out of room for an eighth planet is a system with
/// seven planets, not a generator failure. Looping until it fitted would hang
/// on a system whose minimum hop leaves nowhere legal to stand.
const PLACEMENT_TRIES: u32 = 32;

/// Angles tried when every drawn one was blocked. Evenly spaced, starting
/// from the best guess — see [`site`], where the sweep is.
const SWEEP_STEPS: u32 = 64;

/// How far off its parent body a station sits, as a fraction of the minimum
/// separation. Small — it is in orbit, not in the next postcode — but not
/// zero: a station at exactly its parent's position would make "which of
/// these did I click" unanswerable.
const STATION_ORBIT: f64 = 0.02;

fn generate(galaxy: &Galaxy, star_id: u32) -> StarSystem {
    let (seed, version) = (galaxy.seed, galaxy.generator_version);
    let promised = galaxy.designation_for(star_id);

    let mut rng = Rng::stream(seed, star_id, version, Purpose::Desolation);
    let mut desolation = data::desolation(rng.unit());
    // A system promised a relay is a system a relay would be sited in. The
    // threshold is a preference rather than a rule, and this is the one place
    // that leans on it.
    if promised == Some(StationKind::Relay) {
        desolation = desolation.max(data::RELAY_DESOLATION);
    }

    let bodies = place_bodies(seed, star_id, version, desolation, promised);
    let stations = place_stations(seed, star_id, version, desolation, promised, &bodies);

    StarSystem {
        star_id,
        desolation,
        bodies,
        stations,
    }
}

/// Place the bodies, outwards from the first.
fn place_bodies(
    seed: u64,
    star_id: u32,
    version: u32,
    desolation: f64,
    promised: Option<StationKind>,
) -> Vec<Body> {
    let mut rng = Rng::stream(seed, star_id, version, Purpose::Bodies);
    let min_gap = layout::min_separation();
    let target_days = data::target_hop_days(desolation);

    let wanted = MIN_BODIES + rng.below(MAX_BODIES - MIN_BODIES + 1);
    let mut placed: Vec<(BodyKind, DVec2)> = Vec::new();

    // The first body: a seeded distance out from the star. The star is not a
    // node, so this distance is not a constraint — it is only what stops
    // every system starting at the same radius.
    let first = data::reference_distance(rng.range(
        data::TRAVEL_BAND.min_days,
        target_days.max(data::TRAVEL_BAND.min_days),
    ))
    .unwrap_or(min_gap);
    placed.push((draw_kind(&mut rng), DVec2::polar(rng.angle(), first)));

    for _ in 1..wanted {
        for _ in 0..PLACEMENT_TRIES {
            // A hop from somewhere already there — which is what makes the
            // system connected without anyone checking afterwards.
            let anchor = placed[rng.below(placed.len() as u32) as usize].1;
            let days = rng
                .range(data::TRAVEL_BAND.min_days, target_days)
                .clamp(data::TRAVEL_BAND.min_days, data::TRAVEL_BAND.max_days);
            let Some(hop) = data::reference_distance(days) else {
                break;
            };
            let candidate = anchor.add(DVec2::polar(rng.angle(), hop));
            if placed
                .iter()
                .all(|&(_, p)| p.distance(candidate) >= min_gap)
            {
                placed.push((draw_kind(&mut rng), candidate));
                break;
            }
        }
    }

    // A system promised a station needs somewhere to put it. Swapping the
    // outermost body's kind is enough and costs nothing: it moves no
    // position, so the layout the checks passed is the layout that ships.
    if let Some(kind) = promised {
        ensure_parent_for(kind, &mut placed, &mut rng);
    }

    // Numbered from the star outwards, so a body's name says where it is.
    placed.sort_by(|a, b| {
        a.1.length()
            .partial_cmp(&b.1.length())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    placed
        .into_iter()
        .enumerate()
        .map(|(i, (kind, position))| Body {
            id: i as u32,
            kind,
            position,
            // The ordinal is one-based: the host prints it as a roman
            // numeral, and there is no planet nought.
            name: Name::body(i as u16 + 1),
        })
        .collect()
}

/// Make sure a body of the sort this station kind needs is present, by
/// changing one body's kind rather than by adding or moving anything.
fn ensure_parent_for(kind: StationKind, placed: &mut [(BodyKind, DVec2)], rng: &mut Rng) {
    if placed
        .iter()
        .any(|&(b, _)| data::parent_suits(kind, Some(b)))
    {
        return;
    }
    let suitable: Vec<BodyKind> = BodyKind::ALL
        .into_iter()
        .filter(|&b| data::parent_suits(kind, Some(b)))
        .collect();
    let Some(&want) = rng.pick(&suitable) else {
        return; // a relay wants no parent at all, and needs nothing here
    };
    let which = rng.below(placed.len() as u32) as usize;
    if let Some(slot) = placed.get_mut(which) {
        slot.0 = want;
    }
}

fn draw_kind(rng: &mut Rng) -> BodyKind {
    let total: f64 = BodyKind::ALL.iter().map(|b| b.weight()).sum();
    let mut roll = rng.unit() * total;
    for &b in &BodyKind::ALL {
        if roll < b.weight() {
            return b;
        }
        roll -= b.weight();
    }
    BodyKind::RockyPlanet
}

/// Site the stations, if there are any.
fn place_stations(
    seed: u64,
    star_id: u32,
    version: u32,
    desolation: f64,
    promised: Option<StationKind>,
    bodies: &[Body],
) -> Vec<StationBlueprint> {
    let mut rng = Rng::stream(seed, star_id, version, Purpose::Stations);

    // Most of the galaxy is empty. A promised system is the exception, and it
    // still rolls first so the stream stays in step with an unpromised one.
    let rolled = rng.chance(data::STATION_SHARE);
    let mut wanted: Vec<StationKind> = Vec::new();
    if let Some(kind) = promised {
        wanted.push(kind);
    } else if rolled {
        if let Some(kind) = pick_kind(&mut rng, desolation, bodies, &[]) {
            wanted.push(kind);
        }
    }
    // A second one, now and then, and only where there is room for it. This
    // is what gives "at most one station per parent body" something to bite
    // on — with one station a system it could never be broken.
    if !wanted.is_empty() && rng.chance(0.12) {
        if let Some(kind) = pick_kind(&mut rng, desolation, bodies, &wanted) {
            wanted.push(kind);
        }
    }

    let mut built: Vec<StationBlueprint> = Vec::new();
    for kind in wanted {
        let taken: Vec<u32> = built.iter().filter_map(|s| s.parent_body).collect();
        let Some((parent, position)) = site(&mut rng, kind, bodies, &built, &taken) else {
            continue;
        };
        let id = built.len() as u32;
        built.push(furnish(seed, star_id, version, id, kind, parent, position));
    }

    // A station with nowhere in its own system to fly to is a station the
    // game has nothing to do with, so it is not there — a lone planet with a
    // yard bolted to it and not one other thing to visit is a start screen
    // with no game behind it.
    //
    // The count is: every body that is not its own parent, plus every other
    // station. Pruning repeats because removing one station is exactly what
    // can strand the next.
    loop {
        let doomed = built.iter().position(|s| {
            let bodies_to_visit = bodies.len() - usize::from(s.parent_body.is_some());
            bodies_to_visit + built.len() - 1 == 0
        });
        match doomed {
            Some(i) => {
                built.remove(i);
            }
            None => break,
        }
    }
    for (i, s) in built.iter_mut().enumerate() {
        s.id = i as u32;
    }
    built
}

/// Which kind of station could stand here, weighted by what the system has.
fn pick_kind(
    rng: &mut Rng,
    desolation: f64,
    bodies: &[Body],
    already: &[StationKind],
) -> Option<StationKind> {
    let free_parent = |kind: StationKind| {
        bodies
            .iter()
            .any(|b| data::parent_suits(kind, Some(b.kind)))
    };
    let candidates: Vec<StationKind> = StationKind::ALL
        .into_iter()
        .filter(|&k| !already.contains(&k))
        .filter(|&k| match k {
            // A relay wants nowhere, and wants nowhere *quiet*.
            StationKind::Relay => desolation >= data::RELAY_DESOLATION,
            other => free_parent(other),
        })
        .collect();
    rng.pick(&candidates).copied()
}

/// Where a station of this kind goes: which body it hangs off, and where it
/// sits relative to it.
///
/// Returns `None` when there is nowhere legal, which is a perfectly ordinary
/// outcome — a second station in a one-planet system has nowhere to go.
fn site(
    rng: &mut Rng,
    kind: StationKind,
    bodies: &[Body],
    built: &[StationBlueprint],
    taken: &[u32],
) -> Option<(Option<u32>, DVec2)> {
    let min_gap = layout::min_separation();

    // Deep space: placed like a body, a hop from something already there,
    // and held to the same minimum as everything else.
    if kind == StationKind::Relay {
        let anchors: Vec<DVec2> = bodies.iter().map(|b| b.position).collect();
        for _ in 0..PLACEMENT_TRIES {
            let anchor = *rng.pick(&anchors)?;
            let days = rng.range(data::TRAVEL_BAND.min_days, data::TRAVEL_BAND.max_days);
            let hop = data::reference_distance(days)?;
            let candidate = anchor.add(DVec2::polar(rng.angle(), hop));
            if clear(candidate, bodies, built, min_gap) {
                return Some((None, candidate));
            }
        }
        return outside_everything(bodies);
    }

    // Attached: a free body of a kind this station belongs on.
    let choices: Vec<u32> = bodies
        .iter()
        .filter(|b| data::parent_suits(kind, Some(b.kind)))
        .filter(|b| !taken.contains(&b.id))
        .map(|b| b.id)
        .collect();
    let parent_id = *rng.pick(&choices)?;
    let parent = bodies.iter().find(|b| b.id == parent_id)?;

    // Close in, and on whichever side keeps it clear of everything else. The
    // offset is small enough that this almost always takes on the first try —
    // it only matters where two bodies are a whisker over the minimum apart.
    for _ in 0..PLACEMENT_TRIES {
        let offset = DVec2::polar(rng.angle(), min_gap * STATION_ORBIT);
        let absolute = parent.position.add(offset);
        let clear_of_bodies = bodies
            .iter()
            .filter(|b| b.id != parent_id)
            .all(|b| b.position.distance(absolute) >= min_gap);
        if clear_of_bodies && clear_of_stations(absolute, bodies, built, min_gap) {
            return Some((Some(parent_id), offset));
        }
    }

    // Every angle drawn was blocked, which only happens where something else
    // is sitting a whisker over the minimum from this body. Sweep instead of
    // drawing: start on the far side of the parent from whatever is nearest —
    // the best single guess there is — and go round from there.
    //
    // **The sweep checks, it does not assume.** Standing opposite the nearest
    // thing clears *that* one and can walk the station a fifth of a percent
    // closer to a third body, which is a `TooClose` of about 0.99 days
    // against a minimum of 1.0: legal-looking, invisible, and exactly the
    // fault this used to produce before the sweep was here.
    let nearest = bodies
        .iter()
        .filter(|b| b.id != parent_id)
        .map(|b| b.position)
        .min_by(|a, b| {
            let (da, db) = (a.distance(parent.position), b.distance(parent.position));
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        });
    let start = match nearest {
        Some(near) => {
            let away = parent.position.sub(near);
            if away.length() == 0.0 {
                0.0
            } else {
                away.y.atan2(away.x)
            }
        }
        None => 0.0,
    };
    for i in 0..SWEEP_STEPS {
        let angle = start + std::f64::consts::TAU * i as f64 / SWEEP_STEPS as f64;
        let offset = DVec2::polar(angle, min_gap * STATION_ORBIT);
        let absolute = parent.position.add(offset);
        let clear_of_bodies = bodies
            .iter()
            .filter(|b| b.id != parent_id)
            .all(|b| b.position.distance(absolute) >= min_gap);
        if clear_of_bodies && clear_of_stations(absolute, bodies, built, min_gap) {
            return Some((Some(parent_id), offset));
        }
    }

    // Hemmed in from every side. There is nowhere legal to bolt it on, which
    // is the same ordinary outcome as a second station in a one-planet
    // system: the station is simply not there. A system with a body pair that
    // tight has barely room for the pair.
    None
}

/// Somewhere in this system that is certainly clear of everything in it:
/// straight out past the outermost body.
///
/// The arithmetic is worth spelling out, because it is what makes this a
/// guarantee rather than another try. The point sits at radius `R + hop`
/// where `R` is the outermost body's; so its distance from any body at
/// radius `r <= R` is at least `hop`, and `hop` is most of a maximum-length
/// trip — comfortably over the minimum separation, and short enough that the
/// relay is still connected to the body it was measured from.
fn outside_everything(bodies: &[Body]) -> Option<(Option<u32>, DVec2)> {
    let outermost = bodies.iter().max_by(|a, b| {
        a.position
            .length()
            .partial_cmp(&b.position.length())
            .unwrap_or(std::cmp::Ordering::Equal)
    })?;
    let hop = layout::max_hop() * 0.95;
    let out = outermost.position;
    let length = out.length();
    let direction = if length == 0.0 {
        dvec2(1.0, 0.0)
    } else {
        out.scale(1.0 / length)
    };
    Some((None, out.add(direction.scale(hop))))
}

fn clear(at: DVec2, bodies: &[Body], built: &[StationBlueprint], min_gap: f64) -> bool {
    bodies.iter().all(|b| b.position.distance(at) >= min_gap)
        && clear_of_stations(at, bodies, built, min_gap)
}

fn clear_of_stations(at: DVec2, bodies: &[Body], built: &[StationBlueprint], min_gap: f64) -> bool {
    built.iter().all(|s| {
        let base = match s.parent_body {
            None => DVec2::ZERO,
            Some(id) => match bodies.iter().find(|b| b.id == id) {
                Some(b) => b.position,
                None => return true,
            },
        };
        base.add(s.position).distance(at) >= min_gap
    })
}

/// Fill in everything about a station that is not where it is.
#[allow(clippy::too_many_arguments)]
fn furnish(
    seed: u64,
    star_id: u32,
    version: u32,
    id: u32,
    kind: StationKind,
    parent_body: Option<u32>,
    position: DVec2,
) -> StationBlueprint {
    let base = Rng::stream(seed, star_id, version, Purpose::StationContents);
    // Branched by id rather than drawn in sequence: the second station's
    // stores must not change because the first one gained a hazard.
    let mut rng = base.branch(id as u64);

    let salvage_sites = data::salvage_sites(kind, rng.unit());

    let pressure = data::hazard_pressure(kind);
    let hazard_sites: Vec<(HazardKind, u32)> = HazardKind::ALL
        .into_iter()
        .filter_map(|hazard| {
            rng.chance(pressure)
                .then(|| (hazard, 1 + rng.below((pressure * 4.0).ceil() as u32)))
        })
        .collect();

    StationBlueprint {
        id,
        kind,
        parent_body,
        position,
        name: station_name(&base, id),
        salvage_sites,
        hazard_sites,
        // Its own stream, so an interior does not change when anything about
        // the station outside it does.
        map_seed: Rng::stream(seed, star_id, version, Purpose::MapSeed)
            .branch(id as u64)
            .next_u64(),
    }
}

fn station_name(base: &Rng, id: u32) -> Name {
    let mut rng = base.branch(0x_5741_4e41_0000_0000 ^ id as u64);
    Name::station(
        rng.below(name::STATION_WORDS as u32) as u16,
        rng.below(900) as u16 + 1,
        rng.below(9) as u16 + 1,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::galaxy::{GalaxyType, STAR_COUNT};
    use crate::layout;
    use std::collections::HashSet;

    fn every_system(seed: u64, t: GalaxyType) -> (Galaxy, Vec<StarSystem>) {
        let g = Galaxy::new(seed, t);
        let systems = (0..STAR_COUNT).map(|id| g.system(id).unwrap()).collect();
        (g, systems)
    }

    #[test]
    fn a_system_is_the_same_however_often_it_is_asked_for() {
        let g = Galaxy::new(808, GalaxyType::Spiral);
        for id in [0, 1, 17, 500, 999] {
            let a = g.system(id).unwrap();
            let b = g.system(id).unwrap();
            assert_eq!(a, b);
        }
        assert!(g.system(STAR_COUNT).is_none());
    }

    /// The reason there is no shared stream. Looking at one system must not
    /// change the next, in any order, on either side of the wire.
    #[test]
    fn the_order_systems_are_asked_for_in_does_not_matter() {
        let g = Galaxy::new(1234, GalaxyType::Elliptical);
        let forwards: Vec<_> = (0..40).map(|id| g.system(id).unwrap()).collect();
        let mut backwards: Vec<_> = (0..40).rev().map(|id| g.system(id).unwrap()).collect();
        backwards.reverse();
        assert_eq!(forwards, backwards);
        // And asking for one in the middle on its own gives the same answer.
        assert_eq!(g.system(21).unwrap(), forwards[21]);
    }

    /// A system is a seed, a star and a version, and **nothing a lobby can
    /// pick**. There used to be one setting that reached this far — a
    /// multiplier on a station's stores — and it went with the stores
    /// themselves; what is left is a generator whose whole input is the three
    /// numbers `Galaxy` was built with. Two galaxies of the same seed and
    /// type are the same galaxy, whatever the lobbies around them said.
    #[test]
    fn a_seed_and_a_type_are_the_whole_of_the_input() {
        let one = Galaxy::new(55, GalaxyType::SpiralTwoArm);
        let two = Galaxy::new(55, GalaxyType::SpiralTwoArm);
        for id in 0..120 {
            assert_eq!(
                one.system(id).unwrap(),
                two.system(id).unwrap(),
                "star {id}"
            );
        }
    }

    #[test]
    fn every_system_in_a_galaxy_is_laid_out_legally() {
        for &t in &GalaxyType::ALL {
            let (_, systems) = every_system(2024, t);
            for s in &systems {
                let faults = layout::faults(s);
                assert!(faults.is_empty(), "{t:?} star {}: {faults:?}", s.star_id);
            }
        }
    }

    /// Several seeds, because a layout rule that holds for one galaxy and not
    /// the next is not holding at all.
    #[test]
    fn layouts_are_legal_across_seeds() {
        for seed in [0u64, 1, 7, 42, 99, 1000, u64::MAX] {
            let (_, systems) = every_system(seed, GalaxyType::Spiral);
            let bad: Vec<_> = systems
                .iter()
                .filter(|s| !layout::is_legal(s))
                .map(|s| (s.star_id, layout::faults(s)))
                .collect();
            assert!(bad.is_empty(), "seed {seed}: {bad:?}");
        }
    }

    #[test]
    fn a_system_has_bodies_and_not_too_many() {
        let (_, systems) = every_system(11, GalaxyType::Round);
        for s in &systems {
            assert!(
                (MIN_BODIES as usize..=MAX_BODIES as usize).contains(&s.bodies.len()),
                "star {} had {} bodies",
                s.star_id,
                s.bodies.len()
            );
            // Ids are the index, and the ordinal counts outwards from one.
            for (i, b) in s.bodies.iter().enumerate() {
                assert_eq!(b.id as usize, i);
                assert_eq!(b.name.part, i as u16 + 1);
            }
        }
    }

    #[test]
    fn about_a_quarter_of_systems_have_a_station() {
        let (_, systems) = every_system(3, GalaxyType::Spiral);
        let with = systems.iter().filter(|s| !s.stations.is_empty()).count();
        let share = with as f64 / systems.len() as f64;
        assert!(
            (0.18..0.32).contains(&share),
            "{share} of systems had a station"
        );
    }

    /// The galaxy-wide promise. Every kind exists somewhere, in every galaxy.
    #[test]
    fn every_galaxy_has_one_of_every_kind_of_station() {
        for seed in [0u64, 5, 123, 77_777] {
            for &t in &GalaxyType::ALL {
                let (_, systems) = every_system(seed, t);
                let kinds: HashSet<StationKind> = systems
                    .iter()
                    .flat_map(|s| s.stations.iter().map(|st| st.kind))
                    .collect();
                for &k in &StationKind::ALL {
                    assert!(kinds.contains(&k), "seed {seed} {t:?} had no {k:?}");
                }
            }
        }
    }

    #[test]
    fn relays_are_out_in_the_quiet_and_on_their_own() {
        let (_, systems) = every_system(64, GalaxyType::Elliptical);
        let mut seen = 0;
        for s in &systems {
            for st in s.stations.iter().filter(|s| s.kind == StationKind::Relay) {
                assert_eq!(st.parent_body, None);
                assert!(
                    s.desolation >= data::RELAY_DESOLATION,
                    "a relay at star {} where desolation is {}",
                    s.star_id,
                    s.desolation
                );
                seen += 1;
            }
        }
        assert!(seen > 0, "no relays at all to check");
    }

    #[test]
    fn a_station_is_only_ever_on_something_it_belongs_on() {
        for &t in &GalaxyType::ALL {
            let (_, systems) = every_system(404, t);
            for s in &systems {
                for st in &s.stations {
                    let parent = st.parent_body.and_then(|id| s.body(id)).map(|b| b.kind);
                    assert!(
                        data::parent_suits(st.kind, parent),
                        "{:?} on {parent:?} at star {}",
                        st.kind,
                        s.star_id
                    );
                }
            }
        }
    }

    #[test]
    fn no_two_stations_share_a_body() {
        let (_, systems) = every_system(2, GalaxyType::Spiral);
        for s in &systems {
            let parents: Vec<u32> = s.stations.iter().filter_map(|st| st.parent_body).collect();
            let unique: HashSet<_> = parents.iter().collect();
            assert_eq!(unique.len(), parents.len(), "star {}", s.star_id);
        }
    }

    #[test]
    fn a_station_always_has_somewhere_to_fly_to() {
        for seed in [8u64, 800, 80_000] {
            let (_, systems) = every_system(seed, GalaxyType::Round);
            for s in &systems {
                for st in &s.stations {
                    let elsewhere = s
                        .nodes()
                        .into_iter()
                        .filter(|&n| n != Node::Station(st.id))
                        .filter(|&n| !s.attached(n, Node::Station(st.id)))
                        .count();
                    assert!(elsewhere > 0, "station {} at star {}", st.id, s.star_id);
                }
            }
        }
    }

    #[test]
    fn a_blueprint_carries_what_a_map_generator_will_want() {
        let (_, systems) = every_system(17, GalaxyType::Spiral);
        let mut map_seeds = HashSet::new();
        let mut checked = 0;
        for s in &systems {
            for st in &s.stations {
                assert!(map_seeds.insert(st.map_seed), "two stations, one map seed");
                if st.kind == StationKind::Derelict {
                    assert!(st.salvage_sites > 0);
                } else {
                    assert_eq!(st.salvage_sites, 0);
                }
                for &(_, count) in &st.hazard_sites {
                    assert!(count > 0, "a hazard with no sites");
                }
                checked += 1;
            }
        }
        assert!(checked > 100, "only {checked} stations to check");
    }

    /// Derelicts should be the wrecks and orbitals should mostly be fine.
    #[test]
    fn a_derelict_is_in_a_worse_state_than_a_working_station() {
        let (_, systems) = every_system(19, GalaxyType::Spiral);
        let mean = |kind: StationKind| {
            let all: Vec<f64> = systems
                .iter()
                .flat_map(|s| s.stations.iter())
                .filter(|s| s.kind == kind)
                .map(|s| s.hazard_sites.len() as f64)
                .collect();
            all.iter().sum::<f64>() / all.len().max(1) as f64
        };
        assert!(
            mean(StationKind::Derelict) > mean(StationKind::Orbital) * 2.0,
            "derelict {} vs orbital {}",
            mean(StationKind::Derelict),
            mean(StationKind::Orbital)
        );
    }

    #[test]
    fn positions_add_up_through_the_parent_chain() {
        let g = Galaxy::new(6, GalaxyType::Spiral);
        for id in 0..300 {
            let s = g.system(id).unwrap();
            for st in &s.stations {
                let at = s.absolute_position(Node::Station(st.id)).unwrap();
                match st.parent_body {
                    None => assert_eq!(at, st.position),
                    Some(parent) => {
                        let p = s.body(parent).unwrap().position;
                        assert_eq!(at, p.add(st.position));
                        // And it is in orbit rather than in the next county.
                        assert!(st.position.length() < layout::min_separation());
                    }
                }
            }
        }
    }

    /// Desolate systems are more spread out than busy ones. This is the only
    /// thing desolation does to a layout, so if it stops being true the
    /// mapping has quietly come unhooked.
    #[test]
    fn desolate_systems_are_emptier() {
        let (_, systems) = every_system(21, GalaxyType::Spiral);
        let span = |s: &StarSystem| {
            s.bodies
                .iter()
                .map(|b| b.position.length())
                .fold(0.0, f64::max)
        };
        let busy: Vec<f64> = systems
            .iter()
            .filter(|s| s.desolation < 0.2 && s.bodies.len() >= 4)
            .map(span)
            .collect();
        let empty: Vec<f64> = systems
            .iter()
            .filter(|s| s.desolation > 0.7 && s.bodies.len() >= 4)
            .map(span)
            .collect();
        let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len().max(1) as f64;
        assert!(!busy.is_empty() && !empty.is_empty());
        assert!(
            mean(&empty) > mean(&busy) * 2.0,
            "desolate {} vs busy {}",
            mean(&empty),
            mean(&busy)
        );
    }
}
