//! Whether a system is laid out legally, and the two distances that decide it.
//!
//! The rules are about **days**, not distance, and they are two:
//!
//! - **Minimum.** No two nodes closer than [`data::TRAVEL_BAND`]`.min_days`
//!   apart. A station and the body it is attached to are the one exception —
//!   they are meant to be on top of each other.
//! - **Maximum.** Every node reachable from every other through hops of at
//!   most `max_days`. Not a promise that any two things are within a
//!   fortnight of each other: a system can be longer than that end to end,
//!   as long as there is somewhere to stop on the way.
//!
//! Both are measured with [`data::REFERENCE_SHIP`], which is fixed and has
//! nothing to do with whatever the players end up flying. The checks are here
//! rather than in the generator because they are the *specification* of a
//! legal system — the generator has to satisfy them and the tests have to be
//! able to say whether it did, and those want the same code.

use crate::data;
use crate::system::{Node, StarSystem};

/// Nodes closer together than this are too close. Cached arithmetic, not a
/// new rule: it is `min_days` run through the reference ship.
pub fn min_separation() -> f64 {
    data::reference_distance(data::TRAVEL_BAND.min_days).expect("the reference ship must fly")
}

/// The longest single hop allowed to count as an edge.
pub fn max_hop() -> f64 {
    data::reference_distance(data::TRAVEL_BAND.max_days).expect("the reference ship must fly")
}

/// What is wrong with a system.
#[derive(Clone, PartialEq, Debug)]
pub enum Fault {
    /// Two nodes a player could not tell apart from a day's flying.
    TooClose {
        a: Node,
        b: Node,
        days: f64,
    },
    /// A node with no chain of short-enough hops to the rest of the system.
    /// Carries the whole stranded group, because one node cut off and nine
    /// cut off together are different bugs.
    Stranded(Vec<Node>),
    /// A station sharing a parent body with another.
    ParentTaken {
        station: u32,
        parent_body: u32,
    },
    /// A station attached to something it does not belong on.
    WrongParent(u32),
    /// A station with nowhere in its own system to fly to.
    Marooned(u32),
}

/// Everything wrong with a system, or an empty list.
///
/// Returns all of them rather than the first: a generator bug usually shows
/// as a handful of related faults, and stopping at the first one hides the
/// shape of it.
pub fn faults(system: &StarSystem) -> Vec<Fault> {
    let mut found = Vec::new();
    let nodes = system.nodes();
    let min = min_separation();

    // --- nothing too close to anything else ---------------------------
    for (i, &a) in nodes.iter().enumerate() {
        for &b in &nodes[i + 1..] {
            if system.attached(a, b) {
                continue; // a station and its own parent body
            }
            let (Some(pa), Some(pb)) = (system.absolute_position(a), system.absolute_position(b))
            else {
                continue;
            };
            let gap = pa.distance(pb);
            if gap < min {
                found.push(Fault::TooClose {
                    a,
                    b,
                    // The days are what the rule is in, so that is what the
                    // fault reports — a distance would have to be converted
                    // by whoever read it.
                    days: data::reference_days(gap).unwrap_or(0.0),
                });
            }
        }
    }

    // --- and everything reachable from everything else -----------------
    if let Some(stranded) = stranded_group(system, &nodes) {
        found.push(Fault::Stranded(stranded));
    }

    // --- stations sit where they are allowed to ------------------------
    let mut parents_used: Vec<u32> = Vec::new();
    for s in &system.stations {
        let parent_kind = s.parent_body.and_then(|id| system.body(id)).map(|b| b.kind);
        if s.parent_body.is_some() && parent_kind.is_none() {
            found.push(Fault::WrongParent(s.id));
        } else if !data::parent_suits(s.kind, parent_kind) {
            found.push(Fault::WrongParent(s.id));
        }
        if let Some(parent) = s.parent_body {
            if parents_used.contains(&parent) {
                found.push(Fault::ParentTaken {
                    station: s.id,
                    parent_body: parent,
                });
            }
            parents_used.push(parent);
        }
        // Somewhere to go that is not the thing it is bolted to.
        let has_somewhere = system
            .nodes()
            .iter()
            .any(|&n| n != Node::Station(s.id) && !system.attached(n, Node::Station(s.id)));
        if !has_somewhere {
            found.push(Fault::Marooned(s.id));
        }
    }

    found
}

/// The group of nodes cut off from node zero, if there is one.
///
/// A plain flood fill over "is this hop short enough". Systems have single
/// figures of nodes, so the quadratic edge test costs nothing and is easier
/// to be sure of than anything cleverer.
fn stranded_group(system: &StarSystem, nodes: &[Node]) -> Option<Vec<Node>> {
    if nodes.len() < 2 {
        return None;
    }
    let reach = max_hop();
    let mut seen = vec![false; nodes.len()];
    let mut queue = vec![0usize];
    seen[0] = true;
    while let Some(i) = queue.pop() {
        for j in 0..nodes.len() {
            if seen[j] {
                continue;
            }
            let (Some(a), Some(b)) = (
                system.absolute_position(nodes[i]),
                system.absolute_position(nodes[j]),
            ) else {
                continue;
            };
            if a.distance(b) <= reach {
                seen[j] = true;
                queue.push(j);
            }
        }
    }
    let cut_off: Vec<Node> = nodes
        .iter()
        .zip(seen.iter())
        .filter(|&(_, &s)| !s)
        .map(|(&n, _)| n)
        .collect();
    (!cut_off.is_empty()).then_some(cut_off)
}

/// Whether a system is legal. The generator's own tests use [`faults`] so a
/// failure says what was wrong; this is for everything else.
pub fn is_legal(system: &StarSystem) -> bool {
    faults(system).is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::TRAVEL_BAND;

    #[test]
    fn the_two_distances_are_the_band_in_units() {
        assert!((data::reference_days(min_separation()).unwrap() - TRAVEL_BAND.min_days).abs() < 1e-9);
        assert!((data::reference_days(max_hop()).unwrap() - TRAVEL_BAND.max_days).abs() < 1e-9);
        assert!(max_hop() > min_separation());
    }
}
