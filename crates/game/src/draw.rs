//! The bridge to the renderer.
//!
//! Rust owns every pixel decision and records it as a flat list of shapes in
//! linear memory; the JavaScript side just replays that list onto a canvas.
//! Keeping the buffer a plain `f32` slice means no glue-code generator and no
//! per-shape call across the wasm boundary.

use crate::math::{Vec2, vec2};

/// Floats per shape: kind, x, y, w, h, rot, radius, line, r, g, b, a.
/// The host reads this at runtime via `bims_stride`, so it can change here
/// without the renderer hardcoding it — but the field *order* below is shared.
pub const STRIDE: usize = 12;

pub const KIND_RECT: f32 = 0.0;
pub const KIND_ELLIPSE: f32 = 1.0;
/// A right-angled triangle, the bottom-left half of its box, spun about
/// the box's centre. The room draws none; the constant is here because the
/// format is one format and the ship's `draw.rs` has it.
#[allow(dead_code)]
pub const KIND_TRIANGLE: f32 = 2.0;

/// A `line` width of zero means fill; anything greater strokes the outline.
const FILLED: f32 = 0.0;

#[derive(Clone, Copy, Debug)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const fn rgb(r: f32, g: f32, b: f32) -> Color {
        Color { r, g, b, a: 1.0 }
    }

    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Color {
        Color { r, g, b, a }
    }

    pub const fn alpha(self, a: f32) -> Color {
        Color { a, ..self }
    }
}

#[derive(Default)]
pub struct DrawList {
    data: Vec<f32>,
}

// Part of the drawing vocabulary the game builds on; not every shape is used yet.
#[allow(dead_code)]
impl DrawList {
    pub fn new() -> DrawList {
        DrawList {
            data: Vec::with_capacity(512 * STRIDE),
        }
    }

    pub fn clear(&mut self) {
        self.data.clear();
    }

    pub fn data(&self) -> &[f32] {
        &self.data
    }

    pub fn as_ptr(&self) -> *const f32 {
        self.data.as_ptr()
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// `center`/`size` are in world pixels; `rot` spins the shape about its
    /// own centre; `radius` rounds rectangle corners and is ignored by
    /// ellipses; `line` strokes instead of filling when non-zero.
    #[allow(clippy::too_many_arguments)]
    pub fn push(
        &mut self,
        kind: f32,
        center: Vec2,
        size: Vec2,
        rot: f32,
        radius: f32,
        line: f32,
        c: Color,
    ) {
        self.data.extend_from_slice(&[
            kind, center.x, center.y, size.x, size.y, rot, radius, line, c.r, c.g, c.b, c.a,
        ]);
    }

    pub fn rect(&mut self, center: Vec2, size: Vec2, rot: f32, radius: f32, c: Color) {
        self.push(KIND_RECT, center, size, rot, radius, FILLED, c);
    }

    pub fn ellipse(&mut self, center: Vec2, size: Vec2, rot: f32, c: Color) {
        self.push(KIND_ELLIPSE, center, size, rot, 0.0, FILLED, c);
    }

    pub fn circle(&mut self, center: Vec2, diameter: f32, c: Color) {
        self.ellipse(center, vec2(diameter, diameter), 0.0, c);
    }

    pub fn stroke_rect(
        &mut self,
        center: Vec2,
        size: Vec2,
        rot: f32,
        radius: f32,
        line: f32,
        c: Color,
    ) {
        self.push(KIND_RECT, center, size, rot, radius, line, c);
    }

    pub fn stroke_ellipse(&mut self, center: Vec2, size: Vec2, rot: f32, line: f32, c: Color) {
        self.push(KIND_ELLIPSE, center, size, rot, 0.0, line, c);
    }

    pub fn ring(&mut self, center: Vec2, diameter: f32, line: f32, c: Color) {
        self.stroke_ellipse(center, vec2(diameter, diameter), 0.0, line, c);
    }

    pub fn line(&mut self, a: Vec2, b: Vec2, thickness: f32, c: Color) {
        let d = b - a;
        self.rect(a.lerp(b, 0.5), vec2(d.len(), thickness), d.angle(), 0.0, c);
    }

    /// Draw in a local frame: handy for an articulated character whose parts
    /// are easiest to describe relative to "forward". `scale` multiplies both
    /// local offsets and sizes, so a whole figure can be resized at one knob.
    pub fn brush(&mut self, origin: Vec2, rot: f32, scale: f32) -> Brush<'_> {
        Brush {
            list: self,
            origin,
            rot,
            scale,
        }
    }
}

/// A local coordinate frame. `+x` is forward, `+y` is the character's right.
pub struct Brush<'a> {
    list: &'a mut DrawList,
    origin: Vec2,
    rot: f32,
    scale: f32,
}

// Part of the drawing vocabulary the game builds on; not every shape is used yet.
#[allow(dead_code)]
impl Brush<'_> {
    /// Local point to world, including the frame's scale.
    pub fn to_world(&self, local: Vec2) -> Vec2 {
        self.origin + (local * self.scale).rotate(self.rot)
    }

    pub fn ellipse(&mut self, local: Vec2, size: Vec2, local_rot: f32, c: Color) {
        let p = self.to_world(local);
        self.list
            .ellipse(p, size * self.scale, self.rot + local_rot, c);
    }

    pub fn rect(&mut self, local: Vec2, size: Vec2, local_rot: f32, radius: f32, c: Color) {
        let p = self.to_world(local);
        self.list.rect(
            p,
            size * self.scale,
            self.rot + local_rot,
            radius * self.scale,
            c,
        );
    }
}
