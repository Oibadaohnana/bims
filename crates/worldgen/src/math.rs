//! A point, at full width.
//!
//! The room's `Vec2` is `f32` and lives in the game crate, which is right for
//! a compartment forty tiles across and wrong for a galaxy: a system is a
//! hundred million units wide and there are fifty thousand light years of
//! galaxy around it, so a single-precision position loses metres at the far
//! end and whole hops at the further one.
//!
//! Everything out here is therefore `f64`, and it stays `f64` all the way
//! into the simulation. **Narrowing happens at draw time and nowhere else**:
//! rendering will subtract the ship's position first and convert what is left
//! — a screenful, never more — so the `f32` only ever holds a small number.
//! Nothing here may be reworked on the assumption that positions move to keep
//! the ship centred; they do not.

#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct DVec2 {
    pub x: f64,
    pub y: f64,
}

pub fn dvec2(x: f64, y: f64) -> DVec2 {
    DVec2 { x, y }
}

impl DVec2 {
    pub const ZERO: DVec2 = DVec2 { x: 0.0, y: 0.0 };

    /// A unit vector at `angle` radians, scaled by `length`. The one way
    /// anything here turns a direction and a distance into a position.
    pub fn polar(angle: f64, length: f64) -> DVec2 {
        DVec2 {
            x: angle.cos() * length,
            y: angle.sin() * length,
        }
    }

    pub fn add(self, other: DVec2) -> DVec2 {
        DVec2 {
            x: self.x + other.x,
            y: self.y + other.y,
        }
    }

    pub fn sub(self, other: DVec2) -> DVec2 {
        DVec2 {
            x: self.x - other.x,
            y: self.y - other.y,
        }
    }

    pub fn scale(self, by: f64) -> DVec2 {
        DVec2 {
            x: self.x * by,
            y: self.y * by,
        }
    }

    pub fn length(self) -> f64 {
        self.length_squared().sqrt()
    }

    /// Squared length, for the comparisons that do not need the square root —
    /// which is most of them, and worth having when a layout check runs over
    /// every pair of nodes in a system.
    pub fn length_squared(self) -> f64 {
        self.x * self.x + self.y * self.y
    }

    pub fn distance(self, other: DVec2) -> f64 {
        self.sub(other).length()
    }
}
