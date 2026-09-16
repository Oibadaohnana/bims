//! What the player has told the place to keep in stock.
//!
//! One number for now — how much food to hold — and everything automated reads
//! its target off this rather than being told separately. The hydroponic bay is
//! the only thing listening so far; whatever comes next (water, spares, a
//! second Bim's worth of anything) belongs here beside it.
//!
//! The number is in **food units**, and a unit splits two to one: two thirds
//! greens, one third soy. That is the ratio the Bim actually eats at — a stew
//! is two vegetables and a bowl is a block of tofu with a salad — so asking for
//! 99 units puts 66 vegetables and 33 blocks of tofu on the target, and neither
//! runs out while the other is still stacked up.

/// How a food unit divides. Two parts greens to one part soy.
const VEG_PARTS: u32 = 2;
const TOFU_PARTS: u32 = 1;
const PARTS: u32 = VEG_PARTS + TOFU_PARTS;

/// As much as the manager will accept. A hundred units is more than a Bim can
/// eat in a season and well past what five trays can grow.
pub const MOST: u32 = 999;

/// What the place starts out asking for: about what the cold store begins with,
/// so nothing is behind before the first meal is cooked.
const AT_DAWN: u32 = 30;

pub struct Manager {
    food: u32,
}

impl Manager {
    pub fn new() -> Manager {
        Manager { food: AT_DAWN }
    }

    pub fn food_units(&self) -> u32 {
        self.food
    }

    pub fn set_food_units(&mut self, units: u32) {
        self.food = units.min(MOST);
    }

    /// The two halves of that, as counts of the actual things. Greens are
    /// rounded and tofu takes the remainder, so the two always add back up to
    /// the number the player typed rather than drifting a unit either way.
    pub fn veg(&self) -> u32 {
        (self.food * VEG_PARTS).div_ceil(PARTS)
    }

    pub fn tofu(&self) -> u32 {
        self.food - self.veg()
    }

    /// Both at once, which is how the bay asks.
    pub fn stock_target(&self) -> (u32, u32) {
        (self.veg(), self.tofu())
    }
}
