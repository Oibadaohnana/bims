//! The bridge to the renderer, a third time.
//!
//! **This is a deliberate copy** of the room's `crates/game/src/draw.rs` and
//! the designer's `crates/ship/src/draw.rs`, not a shared module. The
//! *format* is shared — twelve floats a shape, in the order below — because
//! one replay loop in JavaScript can then paint any of the three pages. The
//! *code* is not, because the room and the designer are crates the lobby has
//! no business importing: this one knows about a galaxy and nothing else, and
//! an import of either `draw` would be the first crack in that.
//!
//! If the format ever changes it changes in three files, and each host reads
//! the stride at runtime from whichever wasm it loaded so they can differ
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

    /// A line between two points, as a thin rectangle turned to lie along it.
    ///
    /// The format has rectangles and ellipses and nothing else — see the
    /// module note — so a line is a rectangle with the `rot` field doing the
    /// work. The map's route line is the only thing that wants one, and it
    /// wants one badly enough to be worth the four lines of trigonometry.
    pub fn line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, width: f32, c: Color) {
        let (dx, dy) = (x1 - x0, y1 - y0);
        let length = (dx * dx + dy * dy).sqrt();
        if length <= 0.0 {
            return;
        }
        self.push(
            KIND_RECT,
            (x0 + x1) / 2.0,
            (y0 + y1) / 2.0,
            length,
            width,
            dy.atan2(dx),
            0.0,
            FILLED,
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
