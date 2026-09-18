//! A powered door in a bulkhead, as a ship design places them.
//!
//! The classic room has one door — the heads' — and it is worked by hand:
//! the Bim walks to the panel, and the pathfinder keeps a grid for each of
//! its two states (`crate::nav::Maps`). A designed ship has a door in every
//! bulkhead, and a grid per combination of them is not a thing, so these
//! are **sliding doors that open by themselves**: a body walking up to one
//! opens it, and it shuts a moment after the doorway is clear. The
//! pathfinder walks straight through an unlocked one, open or shut, because
//! it will be open by the time the body gets there.
//!
//! What the player can do to one is the bathroom door's vocabulary — open,
//! close, lock, unlock — and it is done the bathroom door's way: the Bim
//! walks over and works it, through [`crate::room::Switch::Door`]. *Open*
//! holds it open, so it no longer shuts itself; *close* hands it back to
//! itself; *lock* shuts it and makes it a solid, which is the one state the
//! pathfinder has to know about, and the room rebuilds its grids when one
//! changes. A door that is told to lock while somebody is in the opening
//! waits for them: the panels never close on a body, and the lock takes
//! the moment the doorway is clear.
//!
//! Nothing here knows what a tile is. A door is a rect and which way it
//! runs, and `aboard.rs` is what works those out from a design.

use crate::draw::DrawList;
use crate::math::{Rect, Vec2, clamp, lerp, vec2};
use crate::room::{GLOW, HULL, PANEL_LIT, STEEL, WARN};

/// How fast the leaves travel, in fractions of open per second. The
/// bathroom door's rate.
const RATE: f32 = 2.6;
/// How far from the opening a body has to be for the door to open for it.
pub const REACH: f32 = 64.0;
/// How long the doorway has to be clear before the door shuts itself.
const SHUT_AFTER: f32 = 1.2;
/// How far from the opening a body counts as *in* it, for the panels not to
/// close on anyone and for a lock to wait.
const IN_THE_WAY: f32 = 22.0;

/// What the player asks of a door. Carried by the switch rather than read
/// off the door when the hand arrives, for the reason the bathroom door's
/// orders are.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Order {
    /// Hold it open: it stops shutting itself until told otherwise.
    Open,
    /// Let it look after itself again.
    Close,
    /// Shut and locked: a solid, until unlocked.
    Lock,
    Unlock,
}

impl Order {
    /// The number that crosses the wasm boundary.
    pub fn from_code(code: u32) -> Option<Order> {
        match code {
            0 => Some(Order::Open),
            1 => Some(Order::Close),
            2 => Some(Order::Lock),
            3 => Some(Order::Unlock),
            _ => None,
        }
    }
}

pub struct Door {
    /// The opening: the two tiles the design put the door in, a run along
    /// the bulkhead one tile deep.
    pub rect: Rect,
    /// Whether the leaves slide along `x` — a door in a bulkhead that runs
    /// east–west — or along `y`. The long side of `rect`.
    pub along_x: bool,
    /// 0 shut, 1 fully drawn back into the bulkhead. Animated.
    pub open: f32,
    /// Held open by the player, so it does not shut itself.
    pub held: bool,
    pub locked: bool,
    /// Seconds since the doorway was last clear; the door shuts itself when
    /// it reaches [`SHUT_AFTER`].
    clear_for: f32,
    /// A picture clock for the lamp.
    time: f32,
}

impl Door {
    pub fn new(rect: Rect, along_x: bool) -> Door {
        Door {
            rect,
            along_x,
            open: 0.0,
            held: false,
            locked: false,
            clear_for: SHUT_AFTER,
            time: 0.0,
        }
    }

    /// Whether the pathfinder may plan through it. Locked is the only no:
    /// an unlocked door opens for whoever comes.
    pub fn passable(&self) -> bool {
        !self.locked
    }

    /// Whether a body walks into it right now: locked and the leaves shut
    /// far enough to be in the way. A locked door still drawing back —
    /// somebody was in the opening — blocks nobody until it has.
    pub fn blocks(&self) -> bool {
        self.locked && self.open < 0.5
    }

    pub fn is_open(&self) -> bool {
        self.open > 0.5
    }

    /// The unit step across the door: the way a body walks through it.
    pub fn through(&self) -> Vec2 {
        if self.along_x {
            vec2(0.0, 1.0)
        } else {
            vec2(1.0, 0.0)
        }
    }

    /// Where a body stands to work the panel: just outside the opening, on
    /// whichever side `from` is.
    pub fn station(&self, from: Vec2, stand_off: f32) -> Vec2 {
        let across = self.through();
        let side = if (from - self.rect.center()).dot(across) < 0.0 {
            -1.0
        } else {
            1.0
        };
        let half = if self.along_x {
            self.rect.height() / 2.0
        } else {
            self.rect.width() / 2.0
        };
        self.rect.center() + across * (side * (half + stand_off))
    }

    /// The player's order, applied the moment the Bim's hand reaches the
    /// panel. Opening a locked door is refused — unlock it first, which the
    /// menu says — and locking one takes its hold off: a locked door held
    /// open is not locked.
    pub fn order(&mut self, order: Order) {
        match order {
            Order::Open => {
                if !self.locked {
                    self.held = true;
                }
            }
            Order::Close => self.held = false,
            Order::Lock => {
                self.held = false;
                self.locked = true;
            }
            Order::Unlock => self.locked = false,
        }
    }

    /// One frame. `bodies` is where everybody in the room is standing: the
    /// door opens for anyone within reach, waits for anyone in the opening,
    /// and shuts itself once the doorway has been clear a moment.
    pub fn update(&mut self, dt: f32, bodies: &[Vec2]) {
        self.time += dt;
        let near = bodies.iter().any(|&p| self.rect.expand(REACH).contains(p));
        let in_the_way = bodies
            .iter()
            .any(|&p| self.rect.expand(IN_THE_WAY).contains(p));
        if near {
            self.clear_for = 0.0;
        } else {
            self.clear_for += dt;
        }
        let target = if self.locked {
            // Shut, once nobody is standing in it.
            if in_the_way { 1.0 } else { 0.0 }
        } else if self.held || near || self.clear_for < SHUT_AFTER {
            1.0
        } else {
            0.0
        };
        let step = RATE * dt;
        self.open += clamp(target - self.open, -step, step);
    }

    /// Two leaves parting in the middle, each sliding away under the
    /// bulkhead it meets, with the lamp beside the opening reading locked
    /// or held at a glance — the bathroom door's picture, laid along
    /// whichever way this one runs.
    pub fn draw(&self, list: &mut DrawList) {
        let d = self.rect;
        let c = d.center();
        // `u` runs along the leaves' travel and `v` across the opening.
        let (u, v) = if self.along_x {
            (vec2(1.0, 0.0), vec2(0.0, 1.0))
        } else {
            (vec2(0.0, 1.0), vec2(1.0, 0.0))
        };
        // The opening runs the length of the door and the bulkhead is as
        // deep as the door is across it: a door is two tiles along and one
        // deep, so the two are not the same number.
        let (width, across) = if self.along_x {
            (d.width(), d.height())
        } else {
            (d.height(), d.width())
        };
        let deep = across * 0.34;
        let rot = if self.along_x {
            0.0
        } else {
            core::f32::consts::FRAC_PI_2
        };

        // The opening: a dark slot across the bulkhead line, lit down both
        // jambs, so that an open door is plainly a gap.
        list.rect(c, vec2(width + 2.0, deep + 4.0), rot, 0.0, HULL);
        for side in [-1.0f32, 1.0] {
            list.rect(
                c + u * (side * (width * 0.5 + 1.5)),
                vec2(3.0, deep + 2.0),
                rot,
                1.0,
                GLOW.alpha(0.55),
            );
        }

        let leaf = width * 0.5;
        let lamp_colour = if self.locked {
            WARN
        } else if self.held {
            GLOW.alpha(0.9)
        } else {
            GLOW
        };
        for side in [-1.0f32, 1.0] {
            let shut = side * leaf * 0.5;
            let at = c + u * lerp(shut, shut + side * leaf, self.open);
            list.rect(at, vec2(leaf, deep), rot, 2.0, STEEL.alpha(0.55));
            list.rect(at, vec2(leaf - 5.0, deep - 5.0), rot, 2.0, PANEL_LIT);
            // Chevrons on the leading edge, pointing the way it opens, red
            // when the door is locked.
            for i in 0..2 {
                list.rect(
                    at + u * (side * (leaf * 0.5 - 6.0 - i as f32 * 6.0)),
                    vec2(2.5, deep - 7.0),
                    rot,
                    1.0,
                    if self.locked { WARN } else { GLOW },
                );
            }
        }

        // The lamp on the bulkhead beside the opening, pulsing when locked.
        let lamp = c + u * (width * 0.5 + 9.0) + v * (deep * 0.5 + 6.0);
        let pulse = 0.65 + 0.35 * (self.time * 3.4).sin();
        let strength = if self.locked { pulse } else { 0.8 };
        list.circle(lamp, 9.0, lamp_colour.alpha(0.18 * strength));
        list.circle(lamp, 4.5, lamp_colour.alpha(strength));
    }
}
