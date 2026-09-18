//! The galley's fixtures, one struct each, so a room can have several of
//! any of them.
//!
//! The room used to keep the whole galley as loose fields — one hob's
//! heat, one board's slices, one fridge's door — and every chain step read
//! them directly, which is exactly what made a second hob a coloured block
//! nobody could cook on. Now each fixture carries its own state and the
//! room holds a list of each kind; a chain **picks** one when it first
//! needs one — the closest that nobody else has — and keeps it for the
//! rest of that errand (`task::Picks`). What is *shared* stays on the
//! room: the cold store's counts, the drawer's plate count, the dish being
//! cooked. A cold store is a class of storage, not a box, and a plate is a
//! plate whichever drawer it came out of.

use crate::math::{Rect, Vec2, vec2};
use crate::room::Cut;

/// A run of worktop with a chopping board and a drawer on it.
#[derive(Clone, Debug)]
pub struct Worktop {
    pub frame: Rect,
    pub board: Rect,
    pub drawer: Rect,
    /// 0 shut, 1 wide open. Animated towards `drawer_target`.
    pub drawer_open: f32,
    pub drawer_target: f32,
    /// The two sides of the chopping board — the left, then the right — and
    /// what is on each: the tofu goes on the left and a vegetable on the
    /// right, and the second thing chopped for one meal takes whichever is
    /// free, so a stew's two things sit side by side until both go in the
    /// pot together. See [`Cut`].
    pub board_sides: [Cut; 2],
    /// Which side the knife is at work on.
    pub board_cutting: usize,
    pub knife_on_board: bool,
}

impl Worktop {
    pub fn new(frame: Rect, board: Rect, drawer: Rect) -> Worktop {
        Worktop {
            frame,
            board,
            drawer,
            drawer_open: 0.0,
            drawer_target: 0.0,
            board_sides: [Cut::default(); 2],
            board_cutting: 0,
            knife_on_board: false,
        }
    }
}

/// A hob with its one pot.
#[derive(Clone, Debug)]
pub struct Hob {
    pub frame: Rect,
    pub on: bool,
    /// Lags `on` so the burner glows up and fades rather than snapping.
    pub heat: f32,
    /// Game minutes the hob has been lit with nothing cooking on it.
    pub idle: f32,
    /// How full the pot is, and how far along the cooking is.
    pub pot_contents: f32,
    /// Helpings left in the pot. A stew is cooked once and eaten twice, so
    /// this is what says whether the Bim has to cook at all.
    pub pot_servings: u32,
    pub pot_cooked: f32,
    /// Whether what was last made on this hob — the pot, or a bowl off the
    /// board beside it — was made in a dirty galley and will make whoever
    /// eats it ill. Rolled by the game the moment the pot finishes cooking
    /// or a bowl is filled — see `Game::judge_the_food` — and cleared when
    /// the pot is filled afresh.
    pub food_bad: bool,
    /// Set the frame the pot finishes cooking or a bowl is filled, for the
    /// game to take and roll on.
    pub judge_food: bool,
    /// A plate on the worktop beside the hob, being served or waiting to be
    /// picked up, carrying how full it is.
    pub plate_on_counter: Option<f32>,
}

impl Hob {
    pub fn new(frame: Rect) -> Hob {
        Hob {
            frame,
            on: false,
            heat: 0.0,
            idle: 0.0,
            pot_contents: 0.0,
            pot_servings: 0,
            pot_cooked: 0.0,
            food_bad: false,
            judge_food: false,
            plate_on_counter: None,
        }
    }

    /// Where a plate sits while being served: on the worktop beside the
    /// hob, to its left.
    pub fn serving_pos(&self) -> Vec2 {
        vec2(self.frame.min.x - 24.0, self.frame.min.y + 34.0)
    }
}

/// A cold store's door. What is *in* the cold store is the room's: one
/// count, whichever box it is taken from.
#[derive(Clone, Debug)]
pub struct Fridge {
    pub frame: Rect,
    /// 0 shut, 1 wide open. Animated towards `target`.
    pub door: f32,
    pub target: f32,
}

impl Fridge {
    pub fn new(frame: Rect) -> Fridge {
        Fridge {
            frame,
            door: 0.0,
            target: 0.0,
        }
    }
}

/// A broom locker, with a broom in it or not.
#[derive(Clone, Debug)]
pub struct Locker {
    pub frame: Rect,
    /// Whether the broom is out of it. Drawn, and nothing else: a broom in
    /// a Bim's hands is not in the cupboard.
    pub broom_out: bool,
}

impl Locker {
    pub fn new(frame: Rect) -> Locker {
        Locker {
            frame,
            broom_out: false,
        }
    }
}

/// The closest of some fixtures to `from` that is not in `taken`, by
/// index. How every chain picks: nearest first, and the next when that
/// one is somebody else's.
pub fn closest_free(
    frames: impl Iterator<Item = Rect>,
    from: Vec2,
    taken: impl Fn(usize) -> bool,
) -> Option<usize> {
    let mut best: Option<(f32, usize)> = None;
    for (i, frame) in frames.enumerate() {
        if taken(i) {
            continue;
        }
        let d = (frame.center() - from).len();
        if best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, i));
        }
    }
    best.map(|(_, i)| i)
}
