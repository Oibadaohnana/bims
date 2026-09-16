//! What happened during a step.
//!
//! The same arrangement as the room's diary and the designer's issue list, and
//! for the same reason: **no strings cross the wasm boundary**, so an event is
//! a code and a number or two, and `EVENT_LINES` in `web/ship.js` is where the
//! sentences live. An event whose code has no line there is *dropped* from the
//! page rather than shown as a placeholder, and what catches a missing one is
//! the row count against `ship_event_count()`.
//!
//! # Why there are events at all, when there is already a state
//!
//! A caller that wants to draw a fuel gauge reads the state. A caller that
//! wants to *say* "docked at Wana 231-4" has to watch for the crossing, and
//! polling the state for a change misses one that happened and reversed
//! inside a single update — which at 24x is an ordinary thing for a step to
//! contain. `crates/health` is built on the same distinction and its note says
//! the same thing at more length.

use flight::PlanError;
use physics::ResourceId;
use worldgen::Node;

use crate::frame::Frame;

/// One thing that happened, in the order it happened.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum WorldEvent {
    /// A trip began. Undocking, if it began from a station.
    Departed { slot: u32 },
    /// A trip ended. `station` is `Some` when the ship actually docked, which
    /// wants an airlock as well as a station to aim at.
    Arrived { station: Option<u32> },
    /// A trip was given up. The ship is stopping, not stopped.
    Aborted { slot: u32 },
    /// A destination could not be flown to. Carries the reason, which is the
    /// same `PlanError` a preview would have shown.
    PlanFailed { slot: u32, error: PlanError },
    /// Something in the system has been seen for the first time.
    Discovered { node: Node },
    /// The view is now about a particular place, or about space again.
    FrameChanged { frame: Frame },
    /// Goods aboard. `units` is negative for a sale.
    Traded {
        slot: u32,
        resource: ResourceId,
        units: i64,
    },
    /// A command was not carried out. The reason is in the code.
    Refused { slot: u32, why: Refusal },
}

/// Why a command did nothing.
///
/// Separate from [`PlanError`] on purpose: these are about *whether the
/// command was allowed*, and those are about whether the trip could be flown.
/// A player who is told "not enough fuel" when what actually happened is
/// "you are not docked" will go and buy fuel.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum Refusal {
    /// Trading anywhere but at a station. Money only works at a dock — see
    /// `shipdesign::materials` for the rule and why.
    NotDocked = 1,
    /// Not the money for it.
    Unaffordable = 2,
    /// Nowhere aboard to stow it.
    NoRoomAboard = 3,
    /// Selling more than is aboard, or more than is not spoken for: fuel held
    /// against a trip under way is not fuel anybody may sell.
    NotAboard = 4,
    /// That player may not give this order. Always false in this step — see
    /// [`crate::World::can_command`].
    NotAtTheHelm = 5,
    /// An abort with nothing to abort.
    NotTravelling = 6,
    /// The sum would not fit in a `Money`. A refusal rather than a wrap, the
    /// same as everywhere else money is added up — see `crates/economy`.
    SumTooBig = 7,
}

impl Refusal {
    pub fn code(self) -> u32 {
        self as u32
    }
}

impl WorldEvent {
    /// The number that crosses the wasm boundary, indexing `EVENT_LINES`.
    ///
    /// Written out and never renumbered, the same as every other code table
    /// in this workspace.
    pub fn code(self) -> u32 {
        match self {
            WorldEvent::Departed { .. } => 1,
            WorldEvent::Arrived { station: Some(_) } => 2,
            WorldEvent::Arrived { station: None } => 3,
            WorldEvent::Aborted { .. } => 4,
            WorldEvent::PlanFailed { .. } => 5,
            WorldEvent::Discovered { .. } => 6,
            WorldEvent::FrameChanged { .. } => 7,
            WorldEvent::Traded { units, .. } if units >= 0 => 8,
            WorldEvent::Traded { .. } => 9,
            WorldEvent::Refused { .. } => 10,
        }
    }

    /// The one number the sentence needs, if it needs one: a station id, a
    /// reason code, a node id, a number of units. The host knows which of
    /// those it is from the code, exactly as `MEMORY_LINES` does.
    pub fn value(self) -> i64 {
        match self {
            WorldEvent::Departed { slot } | WorldEvent::Aborted { slot } => slot as i64,
            WorldEvent::Arrived { station } => station.map(i64::from).unwrap_or(-1),
            WorldEvent::PlanFailed { error, .. } => error.code() as i64,
            WorldEvent::Discovered { node } => match node {
                Node::Body(id) | Node::Station(id) => id as i64,
            },
            WorldEvent::FrameChanged { frame } => frame.code() as i64,
            WorldEvent::Traded { units, .. } => units,
            WorldEvent::Refused { why, .. } => why.code() as i64,
        }
    }
}
