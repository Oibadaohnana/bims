//! Money, and the only sums anybody is allowed to do with it.
//!
//! Nobody starts with stores any more. Each Bim brings **money**, all of it
//! goes into **one shared pool**, and everything in the design phase is
//! bought out of that pool: the parts of the ship, and the goods the crew
//! load into it. This crate is that pool's arithmetic and nothing else — it
//! renders nothing, it knows no parts, and its only dependency is `physics`,
//! for [`ResourceId`].
//!
//! # What a station will sell, and where it goes
//!
//! [`trade_price`] is what a unit of a resource costs, and it is **the same
//! at every station**. That is deliberate and it is temporary: prices that
//! differ by where you are is a trading game, and a trading game wants a
//! market, a reason to fly and somewhere to sell — none of which exists. One
//! price list is the honest placeholder, and the shape of the call is what a
//! per-station one would need anyway.
//!
//! Supply is unlimited. What bounds a purchase is **the ship**: goods are
//! stowed, and [`storage`] says in what — food in a cold store, fuel in a
//! tank, everything else on a shelf. A ship with nowhere to put a thing
//! cannot buy it.
//!
//! Three things it is built around:
//!
//! - **A [`Money`] is a whole number of euros.** Never a float. Two players
//!   have to end up with the same pool down to the last euro, and `0.1 + 0.2`
//!   is a promise nobody made. The euro sign and the digit grouping live in
//!   the host — no strings cross the wasm boundary, and none are made here.
//! - **Every sum is checked.** Overflow is an [`EconomyError`], never a wrap
//!   and never a saturation. A wrap would hand somebody a fortune and a
//!   saturation would quietly make two different lobbies agree; both are
//!   worse than a refusal that says so.
//! - **It compiles for native and for wasm32 and gives the same answers.**
//!   There is no `usize`, no float and no hashing in here for exactly that
//!   reason. The wasm half of the check is `ship_self_check` in
//!   `crates/ship`; the native half is the tests below.

use physics::ResourceId;

/// Whole euros. There are no cents: a part costs what it costs, and a
/// fractional euro is a rounding rule two machines could disagree about.
pub type Money = u64;

/// What a lone player gets on top of their own money.
///
/// A ship is a ship whether one person or four are paying for it — the hull,
/// the galley and the heads cost the same — so somebody playing alone would
/// otherwise be building a quarter of a ship. The bonus is a **placeholder**
/// like every other number in the game so far, and it is deliberately a lump
/// rather than a multiplier: it is meant to cover the fixed part of a ship,
/// and the fixed part does not scale.
pub const SOLO_BONUS: Money = 20_000;

/// Why a sum could not be done.
///
/// The discriminants are written out because they will one day cross the wasm
/// boundary the way [`crate::Money`] does; `0` is left free for "nothing went
/// wrong", which is the shape every other code in this workspace has.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum EconomyError {
    /// A game with nobody in it. There is no pool to build one.
    NoPlayers = 1,
    /// The sum does not fit in a [`Money`]. A refusal rather than a wrap.
    Overflow = 2,
}

impl EconomyError {
    pub fn code(self) -> u32 {
        self as u32
    }
}

/// What the crew have between them at the start of the design phase.
///
/// Everybody's money goes into **one pool**: there is one ship and it is
/// built together, so there is nothing for a per-player purse to mean. A lone
/// player gets [`SOLO_BONUS`] on top, because the ship they have to build is
/// the same size as everybody else's.
///
/// Checked throughout. `player_count == 0` is an error rather than an empty
/// pool — a game with no players is a question nobody meant to ask, and
/// answering "nothing" would let it start.
pub fn starting_pool(money_per_bim: Money, player_count: u32) -> Result<Money, EconomyError> {
    if player_count == 0 {
        return Err(EconomyError::NoPlayers);
    }
    let brought = money_per_bim
        .checked_mul(player_count as Money)
        .ok_or(EconomyError::Overflow)?;
    if player_count == 1 {
        brought
            .checked_add(SOLO_BONUS)
            .ok_or(EconomyError::Overflow)
    } else {
        Ok(brought)
    }
}

/// `a + b`, or a refusal. The one way two amounts of money are added.
pub fn add(a: Money, b: Money) -> Result<Money, EconomyError> {
    a.checked_add(b).ok_or(EconomyError::Overflow)
}

// --- what a station sells, and where it goes ------------------------------

/// Where a resource is stowed aboard.
///
/// Not a part: several parts can be a shelf between them, and the question a
/// purchase asks is "is there room in the **class**", not "is there room in
/// that cupboard". `PartDef::capacity` in `shipdesign` is what says which
/// part provides which class and how much of it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum Storage {
    /// Racking. Ore, metal, components — anything that keeps.
    Shelf = 0,
    /// Fuel, which is not going on a shelf.
    FuelTank = 1,
    /// Food, which goes off.
    ColdStore = 2,
}

impl Storage {
    /// Every class, in discriminant order. The host builds one capacity
    /// readout per entry.
    pub const ALL: [Storage; 3] = [Storage::Shelf, Storage::FuelTank, Storage::ColdStore];

    pub fn code(self) -> u32 {
        self as u32
    }

    pub fn from_code(code: u32) -> Option<Storage> {
        Storage::ALL.get(code as usize).copied()
    }
}

/// What one unit costs, in whole euros. **Placeholder**, and the same at
/// every station — see the module note.
///
/// A `match` rather than a table so that a new [`ResourceId`] is a compile
/// error here rather than a resource that is quietly free.
pub fn trade_price(resource: ResourceId) -> Money {
    match resource {
        ResourceId::Ore => 20,
        ResourceId::Metal => 60,
        ResourceId::Fuel => 40,
        ResourceId::Components => 150,
        ResourceId::Vegetable => 8,
        ResourceId::Tofu => 12,
    }
}

/// What `units` of it cost, or a refusal. Checked like every other sum here:
/// a hand-typed order for four billion vegetables is a refusal, not a wrap
/// into somebody being paid to take them.
pub fn trade_value(resource: ResourceId, units: u32) -> Result<Money, EconomyError> {
    trade_price(resource)
        .checked_mul(units as Money)
        .ok_or(EconomyError::Overflow)
}

/// Where a resource is stowed. Same reason for the `match` as above.
pub fn storage(resource: ResourceId) -> Storage {
    match resource {
        ResourceId::Ore | ResourceId::Metal | ResourceId::Components => Storage::Shelf,
        ResourceId::Fuel => Storage::FuelTank,
        ResourceId::Vegetable | ResourceId::Tofu => Storage::ColdStore,
    }
}

/// Everything in `amounts`, added up, or a refusal if it does not fit.
pub fn total(amounts: impl IntoIterator<Item = Money>) -> Result<Money, EconomyError> {
    amounts.into_iter().try_fold(0, add)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lone_player_gets_the_bonus_and_a_crew_does_not() {
        assert_eq!(starting_pool(100_000, 1), Ok(120_000));
        assert_eq!(starting_pool(100_000, 2), Ok(200_000));
        assert_eq!(starting_pool(100_000, 3), Ok(300_000));
        assert_eq!(starting_pool(100_000, 4), Ok(400_000));
    }

    /// The three the lobby offers, at each crew size it can have.
    #[test]
    fn the_lobby_presets_all_add_up() {
        for &each in &[50_000, 100_000, 200_000] {
            assert_eq!(starting_pool(each, 1), Ok(each + SOLO_BONUS));
            for players in 2..=4 {
                assert_eq!(starting_pool(each, players), Ok(each * players as Money));
            }
        }
    }

    #[test]
    fn a_game_with_nobody_in_it_is_refused() {
        assert_eq!(starting_pool(100_000, 0), Err(EconomyError::NoPlayers));
        assert_eq!(starting_pool(0, 0), Err(EconomyError::NoPlayers));
    }

    /// Never a wrap and never a saturation: both would be a lobby quietly
    /// disagreeing with the one next to it about what there is to spend.
    #[test]
    fn overflow_is_an_error_rather_than_a_fortune() {
        assert_eq!(starting_pool(Money::MAX, 2), Err(EconomyError::Overflow));
        // The multiply fits and the bonus is what tips it over.
        assert_eq!(starting_pool(Money::MAX, 1), Err(EconomyError::Overflow));
        assert_eq!(
            starting_pool(Money::MAX - SOLO_BONUS, 1),
            Ok(Money::MAX),
            "the largest pool there is still has to be reachable",
        );
        assert_eq!(add(Money::MAX, 1), Err(EconomyError::Overflow));
        assert_eq!(total([Money::MAX, 1]), Err(EconomyError::Overflow));
    }

    /// Every resource has a price and somewhere to put it. A `match` makes
    /// that a compile error rather than a test failure, and this is here for
    /// the part the compiler cannot check: that no price is nought — a free
    /// resource is one a player would buy an unbounded amount of.
    #[test]
    fn every_resource_is_priced_and_stowable() {
        for &id in ResourceId::ALL.iter() {
            assert!(trade_price(id) > 0, "{id:?} is free");
            assert!(Storage::ALL.contains(&storage(id)), "{id:?}");
        }
        assert_eq!(trade_price(ResourceId::Ore), 20);
        assert_eq!(trade_price(ResourceId::Metal), 60);
        assert_eq!(trade_price(ResourceId::Fuel), 40);
        assert_eq!(trade_price(ResourceId::Components), 150);
        assert_eq!(trade_price(ResourceId::Vegetable), 8);
        assert_eq!(trade_price(ResourceId::Tofu), 12);

        assert_eq!(storage(ResourceId::Ore), Storage::Shelf);
        assert_eq!(storage(ResourceId::Metal), Storage::Shelf);
        assert_eq!(storage(ResourceId::Components), Storage::Shelf);
        assert_eq!(storage(ResourceId::Fuel), Storage::FuelTank);
        assert_eq!(storage(ResourceId::Vegetable), Storage::ColdStore);
        assert_eq!(storage(ResourceId::Tofu), Storage::ColdStore);
    }

    #[test]
    fn an_order_is_priced_by_the_unit_and_cannot_wrap() {
        assert_eq!(trade_value(ResourceId::Metal, 0), Ok(0));
        assert_eq!(trade_value(ResourceId::Metal, 1), Ok(60));
        assert_eq!(trade_value(ResourceId::Metal, 100), Ok(6_000));
        assert_eq!(trade_value(ResourceId::Tofu, 250), Ok(3_000));
        // The largest order the type can express still has to be answerable,
        // and the one that does not fit has to be a refusal.
        assert_eq!(
            trade_value(ResourceId::Components, u32::MAX),
            Ok(150 * u32::MAX as Money),
        );
        assert!(Storage::from_code(3).is_none());
        assert_eq!(Storage::from_code(2), Some(Storage::ColdStore));
    }

    #[test]
    fn nothing_is_nothing() {
        assert_eq!(starting_pool(0, 4), Ok(0));
        assert_eq!(starting_pool(0, 1), Ok(SOLO_BONUS));
        assert_eq!(total([]), Ok(0));
        assert_eq!(total([1, 2, 3]), Ok(6));
    }
}
