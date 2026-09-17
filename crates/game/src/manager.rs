//! What the player has told the place to keep in stock.
//!
//! Three numbers — vegetables, blocks of tofu and pots of stew in the cold
//! store — and everything automated reads its target off this rather than
//! being told separately. The hydroponic bay plants to the first two; the
//! galley cooks to the third, one stew out of a vegetable and a block of
//! tofu, and puts it in the cold store beside them. Whatever comes next
//! (water, spares, a second Bim's worth of anything) belongs here beside
//! them.
//!
//! The three are set **separately** now. They used to be one number in food
//! units that split two to one, which was the ratio a Bim ate at; a stew on
//! the shelf takes one of each, and a target that was one dial for three
//! things had no honest way to say "more soy".

/// As much as the manager will accept of anything. A hundred is more than a
/// Bim can eat in a season and well past what six trays can grow.
pub const MOST: u32 = 999;

/// What the place starts out asking for: about what the cold store begins
/// with, so nothing is behind before the first meal is cooked. Two to one,
/// the ratio the Bim eats at.
const VEG_AT_DAWN: u32 = 20;
const TOFU_AT_DAWN: u32 = 10;
/// No stew until somebody asks for it. The galley would otherwise start the
/// game by cooking the store down, and every probe that pins where the crew
/// are on the first morning would move.
const STEW_AT_DAWN: u32 = 0;

/// Which of the three a caller means. The codes cross the wasm boundary —
/// `bims_target(kind)` and `bims_set_target(kind, n)` — so they are fixed.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Stock {
    Veg = 0,
    Tofu = 1,
    Stew = 2,
}

impl Stock {
    pub const ALL: [Stock; 3] = [Stock::Veg, Stock::Tofu, Stock::Stew];

    pub fn code(self) -> u32 {
        self as u32
    }

    pub fn from_code(code: u32) -> Option<Stock> {
        Stock::ALL.get(code as usize).copied()
    }
}

pub struct Manager {
    veg: u32,
    tofu: u32,
    stew: u32,
}

impl Manager {
    pub fn new() -> Manager {
        Manager {
            veg: VEG_AT_DAWN,
            tofu: TOFU_AT_DAWN,
            stew: STEW_AT_DAWN,
        }
    }

    pub fn target(&self, which: Stock) -> u32 {
        match which {
            Stock::Veg => self.veg,
            Stock::Tofu => self.tofu,
            Stock::Stew => self.stew,
        }
    }

    pub fn set_target(&mut self, which: Stock, count: u32) {
        let slot = match which {
            Stock::Veg => &mut self.veg,
            Stock::Tofu => &mut self.tofu,
            Stock::Stew => &mut self.stew,
        };
        *slot = count.min(MOST);
    }

    pub fn veg(&self) -> u32 {
        self.veg
    }

    pub fn tofu(&self) -> u32 {
        self.tofu
    }

    pub fn stew(&self) -> u32 {
        self.stew
    }

    /// The two the bay grows, at once, which is how the bay asks.
    pub fn stock_target(&self) -> (u32, u32) {
        (self.veg, self.tofu)
    }
}
