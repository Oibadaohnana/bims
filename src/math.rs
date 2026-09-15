//! Minimal vector maths. Bims builds with nothing but `rustc`, so everything
//! it needs lives here rather than in a crate.

use core::ops::{Add, AddAssign, Mul, Neg, Sub};

pub const PI: f32 = core::f32::consts::PI;
pub const TAU: f32 = core::f32::consts::TAU;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

pub const fn vec2(x: f32, y: f32) -> Vec2 {
    Vec2 { x, y }
}

impl Vec2 {
    pub const ZERO: Vec2 = vec2(0.0, 0.0);

    /// Unit vector pointing along `angle` radians (0 = screen-right, +y = down).
    pub fn from_angle(angle: f32) -> Vec2 {
        vec2(angle.cos(), angle.sin())
    }

    pub fn len(self) -> f32 {
        (self.x * self.x + self.y * self.y).sqrt()
    }

    pub fn angle(self) -> f32 {
        self.y.atan2(self.x)
    }

    /// Turned a quarter turn: the same length, at right angles to itself.
    pub fn perp(self) -> Vec2 {
        vec2(-self.y, self.x)
    }

    pub fn normalize_or_zero(self) -> Vec2 {
        let l = self.len();
        if l > 1e-6 {
            self * (1.0 / l)
        } else {
            Vec2::ZERO
        }
    }

    /// Rotate clockwise on screen (y grows downwards) by `angle` radians.
    pub fn rotate(self, angle: f32) -> Vec2 {
        let (s, c) = (angle.sin(), angle.cos());
        vec2(self.x * c - self.y * s, self.x * s + self.y * c)
    }

    pub fn lerp(self, other: Vec2, t: f32) -> Vec2 {
        self + (other - self) * t
    }
}

impl Add for Vec2 {
    type Output = Vec2;
    fn add(self, r: Vec2) -> Vec2 {
        vec2(self.x + r.x, self.y + r.y)
    }
}

impl Sub for Vec2 {
    type Output = Vec2;
    fn sub(self, r: Vec2) -> Vec2 {
        vec2(self.x - r.x, self.y - r.y)
    }
}

impl Mul<f32> for Vec2 {
    type Output = Vec2;
    fn mul(self, s: f32) -> Vec2 {
        vec2(self.x * s, self.y * s)
    }
}

impl Neg for Vec2 {
    type Output = Vec2;
    fn neg(self) -> Vec2 {
        vec2(-self.x, -self.y)
    }
}

impl AddAssign for Vec2 {
    fn add_assign(&mut self, r: Vec2) {
        *self = *self + r;
    }
}

pub fn clamp(v: f32, lo: f32, hi: f32) -> f32 {
    if v < lo {
        lo
    } else if v > hi {
        hi
    } else {
        v
    }
}

pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Ease 0..1 in and out. Used wherever a linear blend reads as mechanical:
/// the steering away from a wall, and the light going down at dusk.
pub fn smoothstep(t: f32) -> f32 {
    let t = clamp(t, 0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Fold an angle into `(-PI, PI]` so differences stay meaningful.
pub fn wrap_angle(mut a: f32) -> f32 {
    while a > PI {
        a -= TAU;
    }
    while a <= -PI {
        a += TAU;
    }
    a
}

/// Interpolate between angles the short way around the circle.
pub fn angle_lerp(from: f32, to: f32, t: f32) -> f32 {
    wrap_angle(from + wrap_angle(to - from) * t)
}

/// Framerate-independent easing factor: the fraction of the remaining
/// distance to cover this frame when closing at `rate` per second.
pub fn approach(rate: f32, dt: f32) -> f32 {
    1.0 - (-rate * dt).exp()
}

/// An axis-aligned box. Used for furniture footprints, marquee selection and
/// the collision push-out.
#[derive(Clone, Copy, Debug)]
pub struct Rect {
    pub min: Vec2,
    pub max: Vec2,
}

#[allow(dead_code)]
impl Rect {
    /// Build from two opposite corners dragged in either direction.
    pub fn from_corners(a: Vec2, b: Vec2) -> Rect {
        Rect {
            min: vec2(a.x.min(b.x), a.y.min(b.y)),
            max: vec2(a.x.max(b.x), a.y.max(b.y)),
        }
    }

    pub fn from_min_size(min: Vec2, size: Vec2) -> Rect {
        Rect {
            min,
            max: min + size,
        }
    }

    pub fn from_center_size(center: Vec2, size: Vec2) -> Rect {
        let half = size * 0.5;
        Rect {
            min: center - half,
            max: center + half,
        }
    }

    pub fn center(self) -> Vec2 {
        (self.min + self.max) * 0.5
    }

    pub fn size(self) -> Vec2 {
        self.max - self.min
    }

    pub fn width(self) -> f32 {
        self.max.x - self.min.x
    }

    pub fn height(self) -> f32 {
        self.max.y - self.min.y
    }

    /// Grow (or, with a negative amount, shrink) the box on every side.
    pub fn expand(self, by: f32) -> Rect {
        Rect {
            min: self.min - vec2(by, by),
            max: self.max + vec2(by, by),
        }
    }

    pub fn contains(self, p: Vec2) -> bool {
        p.x >= self.min.x && p.x <= self.max.x && p.y >= self.min.y && p.y <= self.max.y
    }

    /// The point of the box closest to `p` — inside the box, that is `p` itself.
    pub fn nearest(self, p: Vec2) -> Vec2 {
        vec2(
            clamp(p.x, self.min.x, self.max.x),
            clamp(p.y, self.min.y, self.max.y),
        )
    }

    /// True when a circle touches the box. A click is just a box of zero size,
    /// so the same test covers both picking and marquee selection.
    pub fn touches_circle(self, center: Vec2, radius: f32) -> bool {
        (center - self.nearest(center)).len() <= radius
    }

    /// Shortest shove that moves a circle centred at `p` clear of the box, or
    /// `None` if it is already clear. Used to keep the Bim out of the furniture.
    pub fn push_out(self, p: Vec2, radius: f32) -> Option<Vec2> {
        let grown = self.expand(radius);
        if !grown.contains(p) {
            return None;
        }
        // Leave by whichever side is nearest: the smallest of the four depths.
        let left = p.x - grown.min.x;
        let right = grown.max.x - p.x;
        let up = p.y - grown.min.y;
        let down = grown.max.y - p.y;
        let least = left.min(right).min(up).min(down);
        Some(if least == left {
            vec2(-left, 0.0)
        } else if least == right {
            vec2(right, 0.0)
        } else if least == up {
            vec2(0.0, -up)
        } else {
            vec2(0.0, down)
        })
    }
}
