//! Which way round everything is.
//!
//! One convention, written down once, because there are three places that
//! would otherwise each invent their own and two of them would be wrong: the
//! plan, the world's anchor arithmetic, and the renderer.
//!
//! - **A heading is 0 at north and grows clockwise.** π/2 is east.
//! - **North is the system's +y.** So a heading `h` points along
//!   `(sin h, cos h)` in system coordinates, which is what [`facing`] returns.
//! - **The ship's Forward is the design grid's up**, and the grid's `y` grows
//!   *downwards*. So a design offset `(dx, dy)` has a forward component of
//!   `-dy` and a starboard component of `dx`, and [`rotate_design`] is that
//!   turned into system coordinates.
//!
//! The one consequence worth spelling out, because the renderer depends on it:
//! at heading 0 a design offset `(dx, dy)` lands at system offset `(dx, -dy)`,
//! which is the design drawn exactly as it was laid out once the screen's
//! y-down is taken back off. See `crates/ship/src/world_paint.rs`.

use worldgen::math::{DVec2, dvec2};

/// An angle in `(-π, π]`. Every difference between two headings goes through
/// here, so "turn 350° left" is never the answer to "turn 10° right".
pub fn wrap(angle: f64) -> f64 {
    let turn = std::f64::consts::TAU;
    let mut a = angle % turn;
    if a > std::f64::consts::PI {
        a -= turn;
    }
    if a <= -std::f64::consts::PI {
        a += turn;
    }
    a
}

/// The shortest way round from one heading to another, signed: positive is
/// clockwise.
pub fn shortest(from: f64, to: f64) -> f64 {
    wrap(to - from)
}

/// The unit vector a heading points along, in system coordinates.
pub fn facing(heading: f64) -> DVec2 {
    dvec2(heading.sin(), heading.cos())
}

/// The heading that points along a vector. Zero for the zero vector, which is
/// a direction nobody asked for rather than a NaN everybody inherits.
pub fn bearing(along: DVec2) -> f64 {
    if along.x == 0.0 && along.y == 0.0 {
        return 0.0;
    }
    along.x.atan2(along.y)
}

/// A design-space offset — world units, `y` down, measured from the ship's
/// own centre of mass — as a system-space offset at this heading.
///
/// **It is its own inverse**, which is worth knowing before anybody writes a
/// second function to undo it. The map is a rotation *composed with* the flip
/// between the grid's y-down and the system's y-up, and a rotation composed
/// with a reflection is a reflection: apply it twice and you are back where
/// you started. [`unrotate_design`] exists only so that call sites read the
/// way they mean.
pub fn rotate_design(offset: DVec2, heading: f64) -> DVec2 {
    let (s, c) = (heading.sin(), heading.cos());
    dvec2(offset.x * c - offset.y * s, -offset.x * s - offset.y * c)
}

/// A system-space offset back into design space. See [`rotate_design`]: this
/// is the same function, named for the direction it is being read in.
pub fn unrotate_design(offset: DVec2, heading: f64) -> DVec2 {
    rotate_design(offset, heading)
}
