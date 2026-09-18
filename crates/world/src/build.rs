//! Construction: a part of the ship laid out by the player and put
//! together by the crew, out of what is in the hold.
//!
//! A [`BuildSite`] is a part that is not there yet — a kind, a tile and a
//! turn, the same three numbers a placement in the design phase is — and
//! what has been carried to it. The crew haul its recipe to it a load at a
//! time off the shelves and then put it together beside it, and when they
//! have, the part is placed through `shipdesign::build_from_cargo` exactly
//! as the design phase would have placed it, paid for in materials. Stage 7
//! of [`crate::World::step`] is where all of that lands.
//!
//! # The materials never leave the hold until the part goes down
//!
//! A load "carried to a site" is a **reservation** on the hold, not a move
//! out of it: [`BuildSite::delivered`] and [`BuildSite::carrying`] count
//! units the crew have spoken for, and the count on the shelf is untouched
//! until `build_from_cargo` takes the whole recipe in one go. That keeps
//! the contract in `shipdesign::materials` true at every step — the ship
//! weighs its parts plus its hold, and nothing is in transit weighing
//! nothing — and it means a site given up costs nothing to give up. What
//! the reservation does is keep the metal from being sold or smelted out
//! from under the site: [`crate::World::free`] is what is aboard less what
//! the sites have claimed, and every hand that reaches for the hold asks
//! that rather than the raw count.
//!
//! # The ship does not move while it is built on, and is not built on while it moves
//!
//! Both halves are the world's to keep. A site is only placed, and the
//! crew only sent to one, while the ship is at rest — docked or holding —
//! and a Confirm is refused while any site has something carried to it or
//! a Bim on the way to it. A blueprint with nothing done at it does not
//! hold the ship: it is a plan, and a plan can wait.

use physics::ResourceId;
use shipdesign::parts::{PartKind, Rotation};
use shipdesign::{CARGO_SLOTS, Edit, ShipDesign, recipe_for};

/// One part waiting to be built.
#[derive(Clone, PartialEq, Debug)]
pub struct BuildSite {
    /// Its identity. Ids only ever climb and are never reissued, so a
    /// command naming a site that has since been built or cancelled is
    /// refused rather than landing on another.
    pub id: u32,
    pub kind: PartKind,
    pub origin: (u32, u32),
    pub rotation: Rotation,
    /// Units of each resource carried to the site so far, by `ResourceId`.
    /// A reservation on the hold — see the module note.
    pub delivered: [u32; CARGO_SLOTS],
    /// Units in somebody's arms on the way to it, the same way.
    pub carrying: [u32; CARGO_SLOTS],
}

impl BuildSite {
    pub fn new(id: u32, kind: PartKind, origin: (u32, u32), rotation: Rotation) -> BuildSite {
        BuildSite {
            id,
            kind,
            origin,
            rotation,
            delivered: [0; CARGO_SLOTS],
            carrying: [0; CARGO_SLOTS],
        }
    }

    /// The edit that puts the part down: deck plating lays its own frame,
    /// the way the designer's plating tool does.
    pub fn edit(&self) -> Edit {
        match self.kind {
            PartKind::Floor => Edit::Plate {
                origin: self.origin,
            },
            kind => Edit::Place {
                kind,
                origin: self.origin,
                rotation: self.rotation,
            },
        }
    }

    /// What it is made of, against `design` — plating a bare tile costs
    /// the frame as well as the deck.
    pub fn recipe(&self, design: &ShipDesign) -> Vec<(ResourceId, u32)> {
        recipe_for(design, self.edit())
    }

    /// Every tile the part will cover.
    pub fn tiles(&self) -> Vec<(u32, u32)> {
        shipdesign::parts::covered(self.kind, self.rotation)
            .into_iter()
            .map(|(dx, dy)| (self.origin.0 + dx, self.origin.1 + dy))
            .collect()
    }

    /// Units of `resource` still to be carried to it, against `design`.
    pub fn short(&self, design: &ShipDesign, resource: ResourceId) -> u32 {
        let wanted = self
            .recipe(design)
            .iter()
            .find(|&&(id, _)| id == resource)
            .map(|&(_, units)| units)
            .unwrap_or(0);
        wanted.saturating_sub(self.delivered[resource as usize] + self.carrying[resource as usize])
    }

    /// Whether everything it is made of is there.
    pub fn stocked(&self, design: &ShipDesign) -> bool {
        self.recipe(design)
            .iter()
            .all(|&(id, units)| self.delivered[id as usize] >= units)
    }

    /// Whether anything has been done at it: a load carried to it or on
    /// its way. What holds the ship at its berth.
    pub fn begun(&self) -> bool {
        self.delivered.iter().any(|&u| u > 0) || self.carrying.iter().any(|&u| u > 0)
    }

    /// Units of `resource` spoken for by this site, delivered or in
    /// transit.
    pub fn reserved(&self, resource: ResourceId) -> u32 {
        self.delivered[resource as usize] + self.carrying[resource as usize]
    }
}

/// How long putting a part together takes, in game minutes: a base plus a
/// share per unit of materials, so a wall is a few minutes and a heavy
/// engine an afternoon. Placeholder, like every other number aboard.
pub fn build_minutes(recipe: &[(ResourceId, u32)]) -> f64 {
    let units: u32 = recipe.iter().map(|&(_, units)| units).sum();
    crate::data::BUILD_MINUTES_BASE + crate::data::BUILD_MINUTES_PER_UNIT * units as f64
}

/// Why a site may not be laid out where it was asked for. What
/// [`crate::World::can_place_site`] answers, for a blueprint to be drawn
/// red before anything is sent; the command itself comes back as a
/// [`crate::Refusal`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SiteRefusal {
    /// The ship is not at rest.
    UnderWay,
    /// The rules would not put the part there: a `shipdesign::EditError`
    /// code.
    WontFit(u32),
    /// The part would go, and the ship would then have a fault it has not
    /// got now: a `shipdesign::IssueCode` code.
    Fault(u32),
}
