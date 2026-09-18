//! What the player has told the place to keep in stock.
//!
//! Four numbers — vegetables, blocks of tofu, pots of stew and fibre in the
//! cold store — and everything automated reads its target off this rather
//! than being told separately. The hydroponic bay plants to the first two
//! and the fourth; the galley cooks to the third, one stew out of a
//! vegetable and a block of tofu, and puts it in the cold store beside
//! them. Whatever comes next (water, spares, a second Bim's worth of
//! anything) belongs here beside them.
//!
//! They are set **separately**. They used to be one number in food units
//! that split two to one, which was the ratio a Bim ate at; a stew on the
//! shelf takes one of each, and a target that was one dial for three things
//! had no honest way to say "more soy". Fibre is not food at all — it is
//! what a bandage is made of — and starts at nought like the stew, so a
//! bay nobody has asked for it grows none.

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
/// And no fibre, for the same reason: a bay that grew it unasked would
/// plant a tray the first morning, and every probe pinned on the bay would
/// move.
const FIBRE_AT_DAWN: u32 = 0;

/// Which of the four a caller means. The codes cross the boundary —
/// `Game::target(kind)` and `Game::set_target(kind, n)` — so they are
/// fixed, and appended to rather than reordered.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Stock {
    Veg = 0,
    Tofu = 1,
    Stew = 2,
    Fibre = 3,
}

impl Stock {
    pub const ALL: [Stock; 4] = [Stock::Veg, Stock::Tofu, Stock::Stew, Stock::Fibre];

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
    fibre: u32,
}

impl Manager {
    pub fn new() -> Manager {
        Manager {
            veg: VEG_AT_DAWN,
            tofu: TOFU_AT_DAWN,
            stew: STEW_AT_DAWN,
            fibre: FIBRE_AT_DAWN,
        }
    }

    pub fn target(&self, which: Stock) -> u32 {
        match which {
            Stock::Veg => self.veg,
            Stock::Tofu => self.tofu,
            Stock::Stew => self.stew,
            Stock::Fibre => self.fibre,
        }
    }

    pub fn set_target(&mut self, which: Stock, count: u32) {
        let slot = match which {
            Stock::Veg => &mut self.veg,
            Stock::Tofu => &mut self.tofu,
            Stock::Stew => &mut self.stew,
            Stock::Fibre => &mut self.fibre,
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

    pub fn fibre(&self) -> u32 {
        self.fibre
    }

    /// The three the bay grows, at once, which is how the bay asks:
    /// vegetables, tofu, fibre.
    pub fn stock_target(&self) -> (u32, u32, u32) {
        (self.veg, self.tofu, self.fibre)
    }
}
