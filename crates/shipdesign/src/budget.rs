//! What there is to build with.
//!
//! The stockpile is the station's, not the ship's. During the design phase
//! parts are paid for out of it and a removal puts the whole cost back,
//! because nothing has actually been welded yet — the design phase is a
//! promise, and the promise can be withdrawn in full.
//!
//! **What is left over stays at the station.** It is not cargo, it is not
//! aboard, and it does not count towards what the ship weighs. Loading is a
//! separate thing that does not exist yet; see [`crate::mass`].

use physics::ResourceId;

use crate::design::ShipDesign;

/// How many resources there are. `physics::ResourceId::ALL.len()`, written
/// out because it indexes arrays here and an array length has to be a
/// constant.
pub const RESOURCE_COUNT: usize = 4;

/// What a station holds at ×1, per [`ResourceId`], in discriminant order.
///
/// **Placeholder.** Nothing costs Ore yet; it is in the stockpile because a
/// station that mines has some, and a resource that never appears in a
/// remaining-stores readout is a resource nobody notices has gone missing.
pub static BASE_STOCKPILE: [u32; RESOURCE_COUNT] = [400, 2400, 200, 800];

/// The lobby's stockpile factor, as thousandths.
///
/// The factor the player picks is ×0.5, ×1 or ×2, and it arrives from the
/// lobby as a number in a query string. It is carried across the wasm
/// boundary as an integer rather than a float on purpose: two players have to
/// end up with the same stockpile down to the unit, and `(base as f64 * 0.5)`
/// is a promise about rounding that nobody made.
pub const PER_MILLE: u32 = 1000;

/// What there is to spend, and what a design has spent of it.
///
/// The budget holds only the stockpile. "Remaining" is always **derived** from
/// a design rather than kept alongside it and decremented, so a rejected edit,
/// a removal and a replayed edit stream cannot drift apart from what is
/// actually on the ship.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Budget {
    pub stockpile: [u32; RESOURCE_COUNT],
}

impl Budget {
    pub fn new(stockpile: [u32; RESOURCE_COUNT]) -> Budget {
        Budget { stockpile }
    }

    /// The stockpile the lobby's factor buys. `factor_permille` is 500, 1000
    /// or 2000 for the three settings; anything else scales the same way.
    ///
    /// Integer arithmetic throughout, in `u64` so a large factor cannot wrap.
    pub fn from_factor(factor_permille: u32) -> Budget {
        let mut stockpile = [0u32; RESOURCE_COUNT];
        for (i, &base) in BASE_STOCKPILE.iter().enumerate() {
            let scaled = base as u64 * factor_permille as u64 / PER_MILLE as u64;
            stockpile[i] = scaled.min(u32::MAX as u64) as u32;
        }
        Budget { stockpile }
    }

    pub fn stock(&self, id: ResourceId) -> u32 {
        self.stockpile[id as usize]
    }

    /// What every part in the design cost, added up. `u64`, because this is
    /// the one figure a design loaded from somewhere else could have made
    /// absurd, and a wrapping subtraction below it would read as free parts.
    pub fn spent(design: &ShipDesign) -> [u64; RESOURCE_COUNT] {
        let mut spent = [0u64; RESOURCE_COUNT];
        for part in &design.parts {
            for &(id, units) in part.kind.def().cost {
                spent[id as usize] += units as u64;
            }
        }
        spent
    }

    /// What is left. Never negative — [`crate::design::apply`] refuses
    /// anything that would take it there, and the saturation here is the
    /// backstop for a design that arrived from somewhere that did not.
    pub fn remaining(&self, design: &ShipDesign) -> [u32; RESOURCE_COUNT] {
        let spent = Budget::spent(design);
        let mut left = [0u32; RESOURCE_COUNT];
        for i in 0..RESOURCE_COUNT {
            left[i] = (self.stockpile[i] as u64).saturating_sub(spent[i]) as u32;
        }
        left
    }

    /// Whether one more thing costing `cost` fits in what is left.
    pub fn affords(&self, design: &ShipDesign, cost: &[(ResourceId, u32)]) -> bool {
        let left = self.remaining(design);
        cost.iter().all(|&(id, units)| left[id as usize] >= units)
    }
}
