//! What a Bim remembers of its days.
//!
//! A diary rather than a log, and a short one: nothing goes in unless it
//! actually mattered. Accidents, illness, going hungry, going without sleep,
//! going without company, and what it saw happen to the other one — including
//! the other one dying. A day in which the crew simply got on with their work
//! leaves **no entry at all**, which is the intended reading of an empty page.
//!
//! It used to keep the day's work too, and the result was a wall of "Used the
//! toilet." to scroll past before reaching the one line worth having.
//!
//! **No strings live here.** Every entry is a day, a time, a code and one
//! number, and the host turns those into a sentence — the same rule the rest
//! of this boundary keeps. A Bim that remembers "I was sick in the galley"
//! remembers `(day 4, 18:22, WasSick, 0)`, and "in the galley" is the host's
//! wording of nothing at all.

/// How many entries a Bim keeps. Long enough to hold a good few days, short
/// enough that a Bim left running for a game year does not grow without bound
/// — the oldest fall off the front. Memory is finite; so is this.
pub const KEEP: usize = 320;

/// What happened. The host names each of these; the numbers are the contract.
///
/// **Only things worth remembering are in here.** The diary used to keep the
/// day's work as well — every meal, every trip to the heads, every tray of the
/// bay — and the result was a page of "Used the toilet." that a player had to
/// scroll past to find the one line that mattered. A day in which nothing went
/// wrong now leaves no entry at all, and that is the intended reading: an
/// empty diary means a good week.
///
/// The numbers start at 20 because the block below 20 was the day's work, and
/// the codes are the contract across the boundary — reusing them would make an
/// old saved diary say something new.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum What {
    /// Did not make it to the heads. `detail` 0 wet itself, 1 worse.
    Accident = 20,
    /// Brought up what was in it.
    WasSick = 21,
    /// Dropped off standing up.
    NoddedOff = 22,
    /// Going hungry got worse. `detail` is the stage, 1 to 3.
    Hungrier = 23,
    /// Going without sleep got worse. `detail` is the stage, 1 to 3.
    Wearier = 24,
    /// Saw one of the others have an accident. `detail` is which of the crew.
    SawAccident = 25,
    /// Saw one of the others be sick. `detail` is which of the crew.
    SawSickness = 26,
    /// A low moment, for want of anybody to talk to. `detail` is how many
    /// whole days it has been. These are what "depressed memories" means: the
    /// diary of a Bim nobody has spoken to fills up with them.
    FeltLow = 27,
    /// Stopped where it stood and sat down on the deck.
    BrokeDown = 28,
    /// Hurt itself. `detail` is the health it cost.
    HurtSelf = 29,
    /// One of the others died. `detail` is which of the crew. Written into
    /// everybody else's diary and not into the dead one's: a Bim does not
    /// record its own end, and there would be nobody to read it back.
    CrewDied = 30,
}

impl What {
    pub fn code(self) -> u32 {
        self as u32
    }
}

/// One line of the diary.
#[derive(Clone, Copy)]
pub struct Moment {
    pub day: u32,
    /// Minutes since midnight, when it happened.
    pub at: f32,
    pub what: What,
    pub detail: u32,
}

pub struct Memory {
    kept: Vec<Moment>,
}

impl Memory {
    pub fn new() -> Memory {
        Memory {
            kept: Vec::with_capacity(KEEP),
        }
    }

    /// Remember something. The oldest is forgotten once the book is full.
    pub fn note(&mut self, day: u32, at: f32, what: What, detail: u32) {
        if self.kept.len() >= KEEP {
            self.kept.remove(0);
        }
        self.kept.push(Moment {
            day,
            at,
            what,
            detail,
        });
    }

    pub fn len(&self) -> usize {
        self.kept.len()
    }

    /// Entry `i`, oldest first. The host reads them in order and groups them
    /// by day; showing the newest day at the top is its business.
    pub fn at(&self, i: usize) -> Option<Moment> {
        self.kept.get(i).copied()
    }

    /// Whether anything of a kind has been remembered. For the probes.
    #[allow(dead_code)]
    pub fn any(&self, what: What) -> bool {
        self.kept.iter().any(|m| m.what == what)
    }
}
