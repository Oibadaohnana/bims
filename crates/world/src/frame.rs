//! Whether the view is about a place or about the space between places.
//!
//! **It affects the view and nothing else.** Not the clock, not the speed, not
//! the physics — the ship flies exactly the same trip whether the renderer has
//! decided to draw a station beside it or not. That is worth stating because
//! it is the kind of thing that grows teeth: a "local frame" in another game
//! would be a different coordinate system, or a different tick rate, or a
//! place where docking is allowed. Here it is which picture is drawn.
//!
//! A ship is in a node's frame when it is **there for that node** — docked at
//! it, holding beside it, or on the last leg of a trip aimed at it — and
//! within its radius. Flying *past* something on the way somewhere else never
//! changes the frame, however close the pass: the view should be about where
//! you are going, not about whatever you happened to sail by.

use worldgen::Node;

/// Where the view is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Frame {
    /// Between things. What the ship is in for almost all of a trip.
    Space,
    /// At one of them.
    Local(Node),
}

impl Frame {
    /// The number that crosses the wasm boundary: 0 for space, then the node
    /// kind and its id, which are two more exports rather than one packed
    /// number.
    pub fn code(self) -> u32 {
        match self {
            Frame::Space => 0,
            Frame::Local(_) => 1,
        }
    }

    pub fn node(self) -> Option<Node> {
        match self {
            Frame::Space => None,
            Frame::Local(node) => Some(node),
        }
    }
}

/// Which frame the ship should be in now, given where it is, what it is there
/// for, and what it was in a moment ago.
///
/// The hysteresis is the whole of the subtlety. A ship holding station
/// exactly on the entry radius would otherwise flip in and out every step —
/// which is not a wrong answer so much as an unreadable one — so the way out
/// is further than the way in and a ship drifting between the two is left
/// alone.
pub fn settle(was: Frame, candidate: Option<(Node, f64, f64)>) -> Frame {
    // Still in the one it was in? Leaving takes the wider radius.
    if let Frame::Local(node) = was
        && let Some((near, distance, radius)) = candidate
        && near == node
    {
        let out = radius * crate::data::LOCAL_HYSTERESIS;
        return if distance <= out { was } else { Frame::Space };
    }
    // Otherwise entering takes the narrower one.
    match candidate {
        Some((node, distance, radius)) if distance <= radius => Frame::Local(node),
        _ => Frame::Space,
    }
}
