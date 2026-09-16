//! What there is to spend.
//!
//! Every Bim brings money and all of it goes into **one pool**. There is one
//! ship and it is built together, so there is nothing for a per-player purse
//! to mean: any part anybody places comes out of the same figure. The pool
//! itself is [`economy::starting_pool`]'s answer, worked out once when the
//! design phase opens and fixed for the whole of it.
//!
//! During the design phase a part is paid for out of the pool and a removal
//! puts the **whole** price back, because nothing has actually been welded
//! yet — the design phase is a promise, and the promise can be withdrawn in
//! full. Goods bought at the station work the same way: nothing has left the
//! dock, so a sale hands back what it cost.
//!
//! **Both come out of the one figure.** A ship and a hold full of fuel are
//! paid for from the same pool, so a player who spends everything on hull has
//! nothing to load it with — which is the whole of the decision the design
//! phase asks anybody to make.
//!
//! **Money only works at a station.** Euros buy parts and goods because the
//! design phase takes place **docked at the spawn station**, where there is
//! somebody to buy them from. Away from one the pool is still there and still
//! the crew's, and it buys nothing: what gets built then comes out of the
//! hold, by the rule in [`crate::materials`].
//!
//! **What is left over is not aboard.** It is money, not cargo: it does not
//! count towards what the ship weighs, and nothing in [`crate::mass`] has
//! ever heard of it. The goods do weigh something, and `mass` knows about
//! those.

use economy::{Money, trade_value};
use physics::ResourceId;

use crate::design::ShipDesign;

/// What there is to spend, and what a design has spent of it.
///
/// The budget holds the pool and nothing that moves. "Remaining" is always
/// **derived** from a design rather than kept alongside it and decremented,
/// so a rejected edit, a removal and a replayed edit stream cannot drift
/// apart from what is actually on the ship.
///
/// `given` is the one other figure, and it is fixed too: the price of
/// whatever the design phase *opened* with. A design phase that starts on a
/// ship already laid out — see [`Budget::with_gift`] — did not buy that ship
/// out of the crew's pool, so its price is added to what there is to spend
/// and what is left starts at exactly what the crew brought. Taking a given
/// part off puts its price in hand like any removal does; a gift is a gift.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Budget {
    pub pool: Money,
    /// The price of what was there before anybody spent anything. Zero for
    /// a design phase that opened on an empty grid.
    pub given: Money,
}

impl Budget {
    pub fn new(pool: Money) -> Budget {
        Budget { pool, given: 0 }
    }

    /// A pool with a ship already on the grid: what `preset` costs is given,
    /// so [`Budget::remaining`] of `preset` is the whole pool.
    pub fn with_gift(pool: Money, preset: &ShipDesign) -> Budget {
        Budget {
            pool,
            given: Budget::spent(preset),
        }
    }

    /// What the design has cost: every part's price, plus what every unit of
    /// cargo was bought for.
    ///
    /// Saturating, and only as a backstop: a design built through
    /// [`crate::design::apply`] cannot get near [`Money::MAX`], and one that
    /// arrived from somewhere the rules were not applied is a design whose
    /// spending should read as "more than there is" rather than wrap round
    /// into looking like free parts.
    pub fn spent(design: &ShipDesign) -> Money {
        let parts = design.parts.iter().fold(0, |sum: Money, part| {
            sum.saturating_add(part.kind.def().price)
        });
        ResourceId::ALL.iter().fold(parts, |sum, &id| {
            let value = trade_value(id, design.carrying(id)).unwrap_or(Money::MAX);
            sum.saturating_add(value)
        })
    }

    /// What is left. Never negative — [`crate::design::apply`] refuses
    /// anything that would take it there, and the saturation here is the
    /// backstop for a design that arrived from somewhere that did not.
    pub fn remaining(&self, design: &ShipDesign) -> Money {
        self.pool
            .saturating_add(self.given)
            .saturating_sub(Budget::spent(design))
    }

    /// Whether one more thing at `price` fits in what is left.
    pub fn affords(&self, design: &ShipDesign, price: Money) -> bool {
        self.remaining(design) >= price
    }
}
