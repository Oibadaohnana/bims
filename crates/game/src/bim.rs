//! One crew member: everything that belongs to a Bim rather than to the ship.
//!
//! Two of them live aboard and they are not distinguishable in code. Each has
//! its own needs, its own health, its own errand and its own queue of errands
//! put down; each has a berth and a seat at the table that the other never
//! uses. What they share is the room — one galley, one pot, one set of heads —
//! and `game.rs` is where the sharing is arbitrated.
//!
//! The only asymmetry is the player. [`PLAYER`] is the one the mouse steers;
//! the other lives exactly the same day entirely on its own account, which is
//! the whole point of it being there.

use crate::character::{Character, Look};
use crate::clock;
use crate::filth::Ordeal;
use crate::health::Health;
use crate::math::Vec2;
use crate::memory::Memory;
use crate::needs::Needs;
use crate::rng::Rng;
use crate::social::Solitude;
use crate::task::{Saved, Task};

/// Who is aboard. The index is the whole identity: it picks the berth, the
/// seat, the colour and the name the host prints, and it never changes.
pub const CREW: usize = 2;

/// The one the player steers. Orders, selection and recruiting all mean this
/// one; the other takes no instruction from anybody.
pub const PLAYER: usize = 0;

/// How long a footprint lingers, and how far apart they are laid. Per Bim, so
/// two wanders read as two.
pub const TRAIL_LIFE: f32 = 2.2;
pub const TRAIL_INTERVAL: f32 = 0.08;

/// How many finished errands a Bim keeps to make conversation out of. A few
/// hours' worth: what they talk about should be the afternoon they have just
/// had, not something from the week before last.
pub const TALKS_ABOUT: usize = 8;

pub struct Footprint {
    pub pos: Vec2,
    pub age: f32,
}

pub struct Bim {
    pub character: Character,
    /// The errand running now, and everything put down to make way for it.
    pub task: Option<Task>,
    pub queue: Vec<Saved>,
    pub needs: Needs,
    pub health: Health,
    /// How long this Bim has been holding on, and how long it has been
    /// standing in the mess. Its own clocks, not the deck's — see [`Ordeal`].
    pub ordeal: Ordeal,
    /// Game minutes left of having dropped off standing up.
    pub nap_left: f32,
    /// How long this Bim has been without anybody to talk to, and what that
    /// is costing it. Its own clock, not the ship's — see [`Solitude`].
    pub solitude: Solitude,
    /// Game minutes left of sitting on the deck having given up for a bit.
    /// Works exactly like `nap_left`: the frame stops for this Bim while it
    /// runs, and whatever it was in the middle of is still there afterwards.
    pub sad_left: f32,
    /// What it is talking about this instant, as a topic code, or 0. Set when
    /// a chat is arranged and cleared when it ends; the host turns it into a
    /// sentence in a bubble, because no strings cross the boundary.
    pub chat_topic: u32,
    /// The last few errands it finished, as `job_code`s, newest last.
    ///
    /// Small talk and nothing else. A Bim used to have something to say by
    /// reading its own diary back — but the diary now keeps only the things
    /// that went wrong, and a crew whose week has gone well would have had
    /// nothing to say to each other at all. So what it has been *doing* is
    /// kept here instead, where nothing but the conversation reads it, and
    /// forgotten again as fast as it arrives.
    pub lately: Vec<u32>,
    /// Seconds until it next looks for somewhere cleaner to stand.
    pub flee_wait: f32,
    /// Where the player sent it, held back until a door has been opened. Only
    /// ever set on [`PLAYER`]: nobody sends the other one anywhere.
    pub pending_move: Option<Vec2>,
    pub trail: Vec<Footprint>,
    pub trail_timer: f32,

    /// When it was born: a year in [`BORN_FROM`]..=[`BORN_TO`], and a day of
    /// that year. Rolled at the start and never changing, which is the whole
    /// of what a birthday is.
    pub born_year: u32,
    pub born_day: u32,
    /// What it remembers of its days. See `memory.rs`.
    pub memory: Memory,
    /// The worst each of these has been so far, so that going a stage further
    /// can be noticed once rather than every frame it lasts.
    pub worst_hunger: u32,
    pub worst_weariness: u32,
}

/// The years the crew were born in. Everyone aboard is somewhere between
/// twenty and fifty when the game opens in [`clock::START_YEAR`].
pub const BORN_FROM: u32 = 2350;
pub const BORN_TO: u32 = 2380;

impl Bim {
    pub fn new(who: usize, at: Vec2, rng: &mut Rng) -> Bim {
        Bim {
            character: Character::new(at, Look::of(who), rng),
            task: None,
            queue: Vec::new(),
            needs: Needs::new(),
            health: Health::new(),
            ordeal: Ordeal::new(),
            solitude: Solitude::new(),
            nap_left: 0.0,
            sad_left: 0.0,
            chat_topic: 0,
            lately: Vec::new(),
            flee_wait: 0.0,
            pending_move: None,
            trail: Vec::new(),
            trail_timer: 0.0,
            born_year: BORN_FROM + rng.below(BORN_TO - BORN_FROM + 1),
            born_day: rng.below(clock::DAYS_IN_YEAR),
            memory: Memory::new(),
            worst_hunger: 0,
            worst_weariness: 0,
        }
    }

    /// How old it is on the given date, in whole years. A birthday that has
    /// not come round yet this year has not been had.
    pub fn age(&self, year: u32, day_of_year: u32) -> u32 {
        let had_it = day_of_year >= self.born_day;
        year.saturating_sub(self.born_year)
            .saturating_sub(if had_it { 0 } else { 1 })
    }

    pub fn is_alive(&self) -> bool {
        !self.character.is_dead()
    }

    /// Lay and age the footprints behind it. A task is not tracked: during one
    /// the Bim walks where it is told and the prints are only clutter.
    pub fn tick_trail(&mut self, dt: f32) {
        self.trail_timer -= dt;
        if self.trail_timer <= 0.0 && self.character.is_walking() && !self.character.is_scripted() {
            self.trail_timer = TRAIL_INTERVAL;
            self.trail.push(Footprint {
                pos: self.character.pos,
                age: 0.0,
            });
        }
        for f in &mut self.trail {
            f.age += dt;
        }
        self.trail.retain(|f| f.age < TRAIL_LIFE);
    }
}
