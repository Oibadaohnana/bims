//! The camera: what part of the build area is on screen, and how big.
//!
//! One per local player, and entirely the host's side of things — panning is
//! not an edit, nothing is told about it, and two players looking at
//! different corners of the same ship is the normal case.
//!
//! `screen = world * scale + offset`, which is the transform `web/ship.js`
//! hands straight to the canvas. Everything below exists to keep that
//! transform pointing at the ship: zoom out far enough and the build area is
//! centred, zoom in and it can be dragged about but not off the edge.

use shipdesign::TILE;

/// How far past the edge of the build area you may look, in tiles. Enough to
/// see that the edge *is* the edge, and no more.
const MARGIN_TILES: f32 = 3.0;

/// Furthest in. Three times life size, which is a tile about 150 pixels
/// across — big enough to see what a two-tile part is doing.
const MAX_SCALE: f32 = 3.0;

/// Branchless, and deliberately not `f32::clamp`: that one panics when its
/// bounds cross, and the panic path drags Rust's formatting machinery into
/// the wasm — nineteen kilobytes of it. Same reason `crates/game/src/math.rs`
/// has its own.
pub fn clamp(v: f32, lo: f32, hi: f32) -> f32 {
    if v < lo {
        lo
    } else if v > hi {
        hi
    } else {
        v
    }
}

pub struct View {
    /// The canvas, in CSS pixels.
    pub width: f32,
    pub height: f32,
    /// The build area, in tiles a side.
    pub area: u32,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
}

impl View {
    pub fn new(area: u32, width: f32, height: f32) -> View {
        let mut view = View {
            width: width.max(1.0),
            height: height.max(1.0),
            area,
            scale: 0.0,
            offset_x: 0.0,
            offset_y: 0.0,
        };
        // Start looking at the whole thing. A design phase that opens zoomed
        // into one corner of a 60-tile square is a player wondering where the
        // ship went.
        view.scale = view.fit_scale();
        view.settle();
        view
    }

    pub fn resize(&mut self, width: f32, height: f32) {
        self.width = width.max(1.0);
        self.height = height.max(1.0);
        self.settle();
    }

    pub fn scale(&self) -> f32 {
        self.scale
    }

    pub fn offset_x(&self) -> f32 {
        self.offset_x
    }

    pub fn offset_y(&self) -> f32 {
        self.offset_y
    }

    /// World units across the whole build area.
    fn span(&self) -> f32 {
        self.area as f32 * TILE as f32
    }

    fn margin(&self) -> f32 {
        MARGIN_TILES * TILE as f32
    }

    /// The scale at which the build area plus its margins just fills the
    /// smaller of the two canvas dimensions. Also the furthest out you may
    /// go — there is nothing out there to look at.
    fn fit_scale(&self) -> f32 {
        let want = self.span() + 2.0 * self.margin();
        (self.width / want).min(self.height / want)
    }

    /// Drag the view by a screen-pixel delta. Middle-drag and WASD both land
    /// here, so there is one clamp rather than two.
    pub fn pan(&mut self, dx: f32, dy: f32) {
        self.offset_x += dx;
        self.offset_y += dy;
        self.settle();
    }

    /// Zoom about a point on the canvas, so the tile under the pointer stays
    /// under the pointer.
    ///
    /// `factor` is a multiplier the host works out from the wheel — one
    /// notch is a few per cent. It arrives already worked out because the
    /// alternative is `powf` in here, and exponentials are not something a
    /// small wasm should be carrying for the sake of a scroll wheel.
    pub fn zoom(&mut self, at_x: f32, at_y: f32, factor: f32) {
        if !(factor > 0.0) || !factor.is_finite() {
            return;
        }
        let before = self.scale;
        self.scale = clamp(before * factor, self.fit_scale(), MAX_SCALE);
        if self.scale == before {
            return;
        }
        // Keep the world point under the cursor where it was.
        let ratio = self.scale / before;
        self.offset_x = at_x - (at_x - self.offset_x) * ratio;
        self.offset_y = at_y - (at_y - self.offset_y) * ratio;
        self.settle();
    }

    /// Put the offsets back inside what the scale allows.
    ///
    /// When the whole area fits on screen there is no choice to make and it is
    /// centred; when it does not, the visible window is held inside the area
    /// plus its margin. Written as one range per axis so the "it fits" case
    /// falls out of the range being empty rather than being a separate branch
    /// that could disagree.
    fn settle(&mut self) {
        let fit = self.fit_scale();
        self.scale = clamp(self.scale, fit, MAX_SCALE);
        let (span, margin, scale) = (self.span(), self.margin(), self.scale);
        self.offset_x = clamp_axis(self.offset_x, self.width, span, margin, scale);
        self.offset_y = clamp_axis(self.offset_y, self.height, span, margin, scale);
    }

    /// Canvas pixels to world units.
    pub fn to_world(&self, x: f32, y: f32) -> (f32, f32) {
        (
            (x - self.offset_x) / self.scale,
            (y - self.offset_y) / self.scale,
        )
    }

    /// Canvas pixels to a tile. Signed, and not clamped: a pointer outside the
    /// build area has to read as outside it rather than as the nearest edge,
    /// or a ghost sticks to the border and looks placeable.
    pub fn to_tile(&self, x: f32, y: f32) -> (i32, i32) {
        let (wx, wy) = self.to_world(x, y);
        let tile = TILE as f32;
        ((wx / tile).floor() as i32, (wy / tile).floor() as i32)
    }
}

/// The offset range one axis allows, and `offset` put inside it.
///
/// The window on the world is `[-offset/scale, (canvas - offset)/scale]`.
/// Holding that inside `[-margin, span + margin]` gives the two bounds below.
/// If they cross, the whole area fits and the midpoint is the centred view.
fn clamp_axis(offset: f32, canvas: f32, span: f32, margin: f32, scale: f32) -> f32 {
    let lo = canvas - (span + margin) * scale;
    let hi = margin * scale;
    if lo > hi {
        return (lo + hi) / 2.0;
    }
    clamp(offset, lo, hi)
}
