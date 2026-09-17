//! Where the power goes: which parts are on a network with a reactor.
//!
//! A part has a [`PartDef::power`](crate::parts::PartDef::power) figure —
//! made by the reactor, drawn by what consumes — and it only counts if the
//! part is **wired**: a tile of its footprint carries a
//! [`PartKind::PowerConduit`] on the utility layer, and that conduit is on a
//! network that reaches a reactor. The utility layer exists so that a
//! conduit can run through the tile of the thing it feeds, which is why
//! there is no adjacency rule here: under it or not at all.
//!
//! A **network** is one connected run of conduit — four-neighbour, the same
//! flood as the radiation fill and the connectivity check, over a different
//! layer — plus one more join: two conduit tiles under the **same part** are
//! on one network whether or not the run between them is laid. The part is
//! the join. Without that a reactor with conduit under two of its tiles from
//! two runs would be two half-reactors, each making the whole of its output.
//! Only a part with a power figure joins — a table the run passes under
//! is not a wire.
//!
//! What this hands back is read in two places and has to be read the same
//! way in both: [`crate::validate`] warns about a consumer on no live
//! network and about a network drawing more than it makes, and the world
//! runs the battery on [`budget`] every step. Nothing here renders, and
//! nothing here decides what happens in a brownout — that is the world's,
//! and `parts::essential` is what it reads.

use crate::design::ShipDesign;
use crate::parts::{Layer, PartKind};

/// One connected run of conduit and everything wired to it.
#[derive(Clone, PartialEq, Debug)]
pub struct Network {
    /// The conduit tiles, in row order.
    pub tiles: Vec<(u32, u32)>,
    /// Every part **with a power figure** standing on one of them —
    /// reactors, batteries and consumers — by id, ascending. The frame and
    /// the deck under the run, and a table it passes beneath, are not on the
    /// network in any sense that matters.
    pub parts: Vec<u32>,
    /// Units a minute made, over the reactors on it.
    pub supply: f64,
    /// Units a minute drawn, over the consumers on it. Positive.
    pub draw: f64,
    /// Units held at most, over the batteries on it.
    pub storage: f64,
}

impl Network {
    /// Whether anything on it is powered at all: it has a reactor.
    pub fn live(&self) -> bool {
        self.supply > 0.0
    }

    /// Whether it draws more than it makes. A battery only delays that;
    /// the world's brownout is what it becomes.
    pub fn short(&self) -> bool {
        self.draw > self.supply
    }
}

/// The ship's power as one figure each, summed over the **live** networks.
///
/// Networks are pooled here on purpose. Two reactors on two runs each with
/// its own consumers are, to the crew, one ship with so much power; the
/// per-network question — is *this* run short — is the validator's, and it
/// asks [`networks`] directly.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Budget {
    pub supply: f64,
    pub draw: f64,
    pub storage: f64,
}

/// Every network of the design, in order of their first tile in row order.
pub fn networks(design: &ShipDesign) -> Vec<Network> {
    let grid = design.grid();
    let side = grid.side();
    let at = |(x, y): (u32, u32)| (y as usize) * (side as usize) + x as usize;
    let conduit = |tile: (i32, i32)| {
        let id = grid.get(Layer::Utility, tile);
        id != 0
            && design
                .part(id)
                .is_some_and(|p| p.kind == PartKind::PowerConduit)
    };

    // Union-find over the grid: every conduit tile joins its four
    // neighbours, and then every part joins the conduit tiles under it.
    let mut parent: Vec<usize> = (0..(side as usize) * (side as usize)).collect();
    fn root(parent: &mut [usize], i: usize) -> usize {
        let mut i = i;
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    fn unite(parent: &mut [usize], a: usize, b: usize) {
        let (a, b) = (root(parent, a), root(parent, b));
        if a != b {
            // The lower index wins, so a network is named by its first tile.
            let (lo, hi) = if a < b { (a, b) } else { (b, a) };
            parent[hi] = lo;
        }
    }

    for y in 0..side {
        for x in 0..side {
            let tile = (x as i32, y as i32);
            if !conduit(tile) {
                continue;
            }
            for next in [(tile.0 + 1, tile.1), (tile.0, tile.1 + 1)] {
                if conduit(next) {
                    unite(&mut parent, at((x, y)), at((next.0 as u32, next.1 as u32)));
                }
            }
        }
    }
    let electrical = |part: &crate::design::PlacedPart| {
        let def = part.kind.def();
        def.supplies() || def.draws() || def.stores()
    };
    for part in design.parts.iter().filter(|p| electrical(p)) {
        let wired: Vec<(u32, u32)> = part
            .tiles()
            .into_iter()
            .filter(|&(x, y)| conduit((x as i32, y as i32)))
            .collect();
        for pair in wired.windows(2) {
            unite(&mut parent, at(pair[0]), at(pair[1]));
        }
    }

    // Gather. Networks come out in the order of their root, which is their
    // first tile in row order; the tiles within each are in row order too.
    let mut roots: Vec<usize> = Vec::new();
    let mut nets: Vec<Network> = Vec::new();
    for y in 0..side {
        for x in 0..side {
            if !conduit((x as i32, y as i32)) {
                continue;
            }
            let r = root(&mut parent, at((x, y)));
            let i = match roots.iter().position(|&seen| seen == r) {
                Some(i) => i,
                None => {
                    roots.push(r);
                    nets.push(Network {
                        tiles: Vec::new(),
                        parts: Vec::new(),
                        supply: 0.0,
                        draw: 0.0,
                        storage: 0.0,
                    });
                    nets.len() - 1
                }
            };
            nets[i].tiles.push((x, y));
        }
    }
    for part in design.parts.iter().filter(|p| electrical(p)) {
        let Some(tile) = part
            .tiles()
            .into_iter()
            .find(|&(x, y)| conduit((x as i32, y as i32)))
        else {
            continue;
        };
        let r = root(&mut parent, at(tile));
        let i = roots
            .iter()
            .position(|&seen| seen == r)
            .expect("a wired tile is on a network");
        let net = &mut nets[i];
        net.parts.push(part.id);
        let def = part.kind.def();
        if def.supplies() {
            net.supply += def.power;
        } else if def.draws() {
            net.draw -= def.power;
        }
        net.storage += def.charge;
    }
    for net in &mut nets {
        net.parts.sort_unstable();
    }
    nets
}

/// Whether `part_id` is on a live network. True for anything standing
/// over a live run whether or not it draws — the question is about the
/// wiring, not the part.
pub fn is_powered(design: &ShipDesign, part_id: u32) -> bool {
    networks(design)
        .iter()
        .any(|net| net.live() && net.parts.contains(&part_id))
}

/// Every consumer not on a live network, by id, ascending. What the
/// validator points at.
pub fn unpowered(design: &ShipDesign) -> Vec<u32> {
    let nets = networks(design);
    let mut out: Vec<u32> = design
        .parts
        .iter()
        .filter(|p| p.kind.def().draws())
        .filter(|p| !nets.iter().any(|n| n.live() && n.parts.contains(&p.id)))
        .map(|p| p.id)
        .collect();
    out.sort_unstable();
    out
}

/// The ship's power over its live networks. See [`Budget`].
pub fn budget(design: &ShipDesign) -> Budget {
    let mut out = Budget {
        supply: 0.0,
        draw: 0.0,
        storage: 0.0,
    };
    for net in networks(design).iter().filter(|n| n.live()) {
        out.supply += net.supply;
        out.draw += net.draw;
        out.storage += net.storage;
    }
    out
}
