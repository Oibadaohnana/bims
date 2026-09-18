//! Mass conservation: a part weighs what it is made of, and building one
//! moves that weight rather than making it.
//!
//! Every part has a [`recipe`](crate::parts::PartDef::recipe) and
//! [`crate::parts::part_mass`] is that recipe added up. Nothing else says
//! what a part weighs. That one decision is what this module is about,
//! because it makes construction a **move**: the materials were in the hold
//! weighing something, they are in the wall weighing the same, and the ship
//! did not get heavier or lighter on the way.
//!
//! # The contract
//!
//! - **What a ship weighs** is the sum of [`crate::parts::part_mass`] over
//!   its parts, plus what is in the hold, plus the materials delivered to
//!   construction sites, plus the crew aboard. Sites do not exist yet — see
//!   *What is not here* — but they are in the sum so that the day they
//!   arrive they are not a hole in it.
//! - **Construction moves exactly the recipe** out of the hold, or out of the
//!   site it was delivered to, and into the part. Total mass is unchanged.
//! - **Deconstruction returns exactly the recipe**, all of it, to the hold or
//!   to a site. Total mass is unchanged. **There is no loss.** Scrapping at a
//!   penalty is a design decision nobody has taken, and a rule that quietly
//!   ate a tenth of every wall would be discovered as a ship that mysteriously
//!   lightens every time it is rebuilt.
//! - **Total mass changes through four things and no others**: trading while
//!   docked, fuel burnt by the engines, food eaten or grown, and crew joining
//!   or leaving. Anything else that changes it is a bug, and this list is the
//!   thing to check it against.
//! - **Money is only usable while docked at a station.** That is where euros
//!   and materials can be swapped for each other, in either direction. The
//!   design phase is docked at the spawn station, which is why everything in
//!   it is instant and paid for in euros; away from a station, what gets built
//!   comes out of the hold or does not get built.
//!
//! # Where the mass is, which is not the same question
//!
//! Total mass is conserved. **The centre of mass is not**, and neither is the
//! inertia about it: a part is at its tile centres, and the cargo and the crew
//! are at the centre of mass itself for want of anywhere better to put them.
//! So welding the hold's metal into an engine at the stern moves the centre of
//! mass aft without changing a gram of the total. Nothing reads that yet —
//! [`crate::mass`] is a single figure — and it is written down here so that
//! whatever does read it knows the total was the only thing promised.
//!
//! # What is not here
//!
//! Loose items on the deck, scrap, and any attempt to balance an instant
//! part's price against what its materials cost. [`build_from_cargo`] and
//! [`deconstruct_to_cargo`] are the **rule**, not the mechanism: the
//! construction step in `world` drives the first with a Bim and a site in
//! the middle of it — materials hauled to a site are *reserved* in the hold,
//! never taken out of it, so the sum above holds at every step, and the
//! whole recipe leaves the hold in the one call that puts the part down.
//! Nothing calls the second yet.

use economy::{Money, storage};
use physics::ResourceId;

use crate::budget::Budget;
use crate::design::{CARGO_SLOTS, Edit, EditError, ShipDesign, apply};
use crate::parts::PartKind;

/// Materials already welded into the ship: units of each resource, indexed by
/// [`ResourceId`], summed over every part's recipe.
///
/// `u64` rather than `u32` because this one can genuinely get large — a
/// thousand tiles of frame is a thousand recipes — and a hold's worth of
/// arithmetic wrapping round into nothing is exactly the kind of quiet wrong
/// answer the rest of this crate is written to avoid.
///
/// It is the other half of [`ShipDesign::cargo`]: together they are every
/// material aboard, and `bound + carried` only changes when the ship trades.
pub fn bound_materials(design: &ShipDesign) -> [u64; CARGO_SLOTS] {
    let mut held = [0u64; CARGO_SLOTS];
    for part in &design.parts {
        for &(id, units) in part.kind.def().recipe {
            held[id as usize] = held[id as usize].saturating_add(units as u64);
        }
    }
    held
}

/// What [`bound_materials`] weighs. The same figure as
/// [`crate::hull_mass`], reached from the other side — every part is its
/// recipe, so adding the recipes up and adding the parts up are one sum
/// written twice, and a test that they agree is a test that nothing has
/// grown a second idea of what a part weighs.
pub fn bound_mass(design: &ShipDesign) -> f64 {
    let held = bound_materials(design);
    ResourceId::ALL
        .iter()
        .map(|&id| held[id as usize] as f64 * id.mass_per_unit())
        .sum()
}

/// A pool nothing can exhaust.
///
/// Construction is paid for in **materials**, not in euros — the whole point
/// of the rule is that it works with a station nowhere in sight. The
/// placement still goes through [`apply`], because the rules about what may
/// stand where are not to be written twice, and this is what stops the money
/// half of `apply` having an opinion about a transaction money is not part
/// of.
fn free() -> Budget {
    Budget::new(Money::MAX)
}

/// What building `edit` costs in materials: the part's recipe for a
/// placement, and for deck plating the deck's recipe plus the frame's when
/// the tile has no frame yet — [`Edit::Plate`] lays both. Empty for
/// anything that is not construction. The construction step reads this to
/// know what to haul to a site, and [`build_from_cargo`] spends exactly it,
/// so the two cannot disagree about what a wall is made of.
pub fn recipe_for(design: &ShipDesign, edit: Edit) -> Vec<(ResourceId, u32)> {
    match edit {
        Edit::Place { kind, .. } => kind.def().recipe.to_vec(),
        Edit::Plate { origin } => {
            let tile = (origin.0 as i32, origin.1 as i32);
            let mut recipe = PartKind::Floor.def().recipe.to_vec();
            if !design.grid().has_structure(tile) {
                for &(id, units) in PartKind::Structure.def().recipe {
                    match recipe.iter_mut().find(|(r, _)| *r == id) {
                        Some((_, have)) => *have += units,
                        None => recipe.push((id, units)),
                    }
                }
            }
            recipe
        }
        Edit::Remove { .. } | Edit::Buy { .. } | Edit::Sell { .. } => Vec::new(),
    }
}

/// Build a part out of what is in the hold.
///
/// The placement rules are [`apply`]'s, unchanged. What is different is the
/// payment: the recipe comes out of [`ShipDesign::cargo`] rather than the
/// price coming out of the pool, and the ship weighs exactly what it weighed
/// before.
///
/// Only [`Edit::Place`] and [`Edit::Plate`] mean anything here — the other
/// three are not construction — and anything else is [`EditError::BadCode`],
/// the same answer the boundary gives for an edit it cannot make sense of.
///
/// Geometry is checked before materials, the way [`apply`] checks it before
/// money: "that will not fit there" is the more useful of the two answers
/// when both are true.
pub fn build_from_cargo(design: &ShipDesign, edit: Edit) -> Result<ShipDesign, EditError> {
    if !matches!(edit, Edit::Place { .. } | Edit::Plate { .. }) {
        return Err(EditError::BadCode);
    }

    // What it costs is read off the design *before* the edit: plating a
    // bare tile lays the frame as well, and the frame is there afterwards.
    let recipe = recipe_for(design, edit);
    let mut next = apply(design, &free(), edit)?;

    // Every material checked before any is spent: half a recipe taken out of
    // the hold for a part that was then refused is mass vanishing.
    if recipe.iter().any(|&(id, units)| next.carrying(id) < units) {
        return Err(EditError::MaterialsShort);
    }
    for &(id, units) in &recipe {
        next.cargo[id as usize] -= units;
    }
    Ok(next)
}

/// Take a part off and put its materials back in the hold.
///
/// The whole recipe, every time. The removal rules are [`apply`]'s — what is
/// standing on it, what is stored in it — and what is added here is that the
/// materials have to have somewhere to go: a ship whose only shelf was the
/// part being taken off cannot hold the metal that shelf was made of, and
/// that is [`EditError::NoRoomAboard`] rather than a quiet loss.
///
/// Room is measured against the design **after** the removal, which is the
/// case that makes the rule bite: taking off a shelf takes its capacity with
/// it.
pub fn deconstruct_to_cargo(design: &ShipDesign, part_id: u32) -> Result<ShipDesign, EditError> {
    let kind = design.part(part_id).ok_or(EditError::NoSuchPart)?.kind;
    let mut next = apply(design, &free(), Edit::Remove { part_id })?;

    // Asked again for each material rather than once for the recipe: metal
    // and components are both shelved, so what the first one takes up is
    // room the second one has not got.
    for &(id, units) in kind.def().recipe {
        let class = storage(id);
        let room = next.capacity(class).saturating_sub(next.stored(class));
        if units > room {
            return Err(EditError::NoRoomAboard);
        }
        next.cargo[id as usize] += units;
    }
    Ok(next)
}
