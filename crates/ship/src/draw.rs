//! The bridge to the renderer, again.
//!
//! **This is a deliberate copy** of the room's `crates/game/src/draw.rs`, not
//! a shared module. The *format* is shared — twelve floats a shape, in the
//! order below — because one replay loop in JavaScript can then paint either
//! page. The *code* is not, because the room is a crate the designer has no
//! business importing: nothing here may reach into `room.rs`, `nav.rs` or
//! `task.rs`, and an import of `draw` would be the first crack in that.
//!
//! If the format ever changes it changes in two files, and the host reads the
//! stride at runtime from whichever wasm it loaded so the two can differ
//! while that is happening.

/// Floats per shape: kind, x, y, w, h, rot, radius, line, r, g, b, a.
pub const STRIDE: usize = 12;

pub const KIND_RECT: f32 = 0.0;
pub const KIND_ELLIPSE: f32 = 1.0;

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

impl DrawList {
    pub fn new() -> DrawList {
        DrawList {
            data: Vec::with_capacity(4096 * STRIDE),
        }
    }

    pub fn clear(&mut self) {
        self.data.clear();
    }

    pub fn as_ptr(&self) -> *const f32 {
        self.data.as_ptr()
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// `x`/`y` are the centre and `w`/`h` the full size, both in world units.
    /// `rot` spins the shape about its own centre; `radius` rounds rectangle
    /// corners and is ignored by ellipses; `line` strokes instead of filling.
    #[allow(clippy::too_many_arguments)]
    pub fn push(
        &mut self,
        kind: f32,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        rot: f32,
        radius: f32,
        line: f32,
        c: Color,
    ) {
        self.data
            .extend_from_slice(&[kind, x, y, w, h, rot, radius, line, c.r, c.g, c.b, c.a]);
    }

    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, radius: f32, c: Color) {
        self.push(KIND_RECT, x, y, w, h, 0.0, radius, FILLED, c);
    }

    pub fn stroke_rect(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        radius: f32,
        line: f32,
        c: Color,
    ) {
        self.push(KIND_RECT, x, y, w, h, 0.0, radius, line, c);
    }

    pub fn ellipse(&mut self, x: f32, y: f32, w: f32, h: f32, c: Color) {
        self.push(KIND_ELLIPSE, x, y, w, h, 0.0, 0.0, FILLED, c);
    }

    /// A rectangle given by its corners rather than its centre, which is how
    /// tile geometry comes out.
    pub fn box_between(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, radius: f32, c: Color) {
        self.rect(
            (x0 + x1) / 2.0,
            (y0 + y1) / 2.0,
            x1 - x0,
            y1 - y0,
            radius,
            c,
        );
    }

    pub fn stroke_between(
        &mut self,
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        radius: f32,
        line: f32,
        c: Color,
    ) {
        self.stroke_rect(
            (x0 + x1) / 2.0,
            (y0 + y1) / 2.0,
            x1 - x0,
            y1 - y0,
            radius,
            line,
            c,
        );
    }
}
