//! The camera the game view is drawn through.
//!
//! **North is up and it never rotates.** Not in the ship view, not on the map,
//! not while the ship is turning end over end. A camera that followed the
//! heading would be the one that made a flip legible and every other moment
//! unreadable — you could not tell which way you were going because "which
//! way" would always look the same.
//!
//! So the *ship* rotates on screen instead, drawn through its heading, and
//! everything else — the starfield, a station drawn alongside, the map — stays
//! square to the window.
//!
//! That is the default and not the only way: `Game::head_up` is the player
//! asking for the other one, the ship held square and the world turned round
//! it, and it is done in `world_paint` by turning what is out there after the
//! fact rather than by anything in here. This camera still never rotates;
//! `Game::camera_turn` is the whole of the difference.
//!
//! The transform is the same one the design phase's [`crate::view::View`]
//! hands out (`screen = world * scale + offset`), so `web/ship.js` paints
//! either page with one loop. The difference is what the origin is: the design
//! phase's is the corner of the build area, and this one is **the ship**. That
//! is what makes the camera centred on it without anything having to follow
//! anything.

use crate::view::clamp;

/// One camera: how far in, and how far the player has shoved it.
pub struct Camera {
    pub width: f32,
    pub height: f32,
    scale: f32,
    /// Screen pixels away from the middle. Clamped so the thing at the origin
    /// — the ship — is always somewhere on the canvas: a view that can be
    /// dragged until the ship is off the edge is a view a player gets lost in.
    pan_x: f32,
    pan_y: f32,
    min_scale: f32,
    max_scale: f32,
}

/// How much of the canvas the pan may take the middle out to, as a fraction
/// of each half. Nine tenths, so the ship is always at least a sliver inside
/// the edge rather than exactly on it.
const PAN_LIMIT: f32 = 0.9;

impl Camera {
    pub fn new(width: f32, height: f32, scale: f32, min_scale: f32, max_scale: f32) -> Camera {
        let mut camera = Camera {
            width: width.max(1.0),
            height: height.max(1.0),
            scale,
            pan_x: 0.0,
            pan_y: 0.0,
            min_scale,
            max_scale,
        };
        camera.settle();
        camera
    }

    pub fn resize(&mut self, width: f32, height: f32) {
        self.width = width.max(1.0);
        self.height = height.max(1.0);
        self.settle();
    }

    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// Where the origin lands on the canvas. The middle, shoved by the pan.
    pub fn offset_x(&self) -> f32 {
        self.width / 2.0 + self.pan_x
    }

    pub fn offset_y(&self) -> f32 {
        self.height / 2.0 + self.pan_y
    }

    pub fn pan(&mut self, dx: f32, dy: f32) {
        self.pan_x += dx;
        self.pan_y += dy;
        self.settle();
    }

    /// Zoom about a point on the canvas, so whatever is under the pointer
    /// stays under it. `factor` arrives already worked out — an exponential is
    /// a lot of binary to link in for the sake of a scroll wheel.
    pub fn zoom(&mut self, at_x: f32, at_y: f32, factor: f32) {
        if !(factor > 0.0) || !factor.is_finite() {
            return;
        }
        let before = self.scale;
        self.scale = clamp(before * factor, self.min_scale, self.max_scale);
        if self.scale == before {
            return;
        }
        let ratio = self.scale / before;
        self.pan_x = at_x - (at_x - self.offset_x()) * ratio - self.width / 2.0;
        self.pan_y = at_y - (at_y - self.offset_y()) * ratio - self.height / 2.0;
        self.settle();
    }

    /// Put the camera back where it is allowed to be.
    pub fn settle(&mut self) {
        self.scale = clamp(self.scale, self.min_scale, self.max_scale);
        let (x, y) = (self.width / 2.0 * PAN_LIMIT, self.height / 2.0 * PAN_LIMIT);
        self.pan_x = clamp(self.pan_x, -x, x);
        self.pan_y = clamp(self.pan_y, -y, y);
    }

    /// Set the scale without moving what is in the middle. What a view does
    /// when it is first opened, or when it has been asked to fit something.
    pub fn set_scale(&mut self, scale: f32) {
        self.scale = scale;
        self.settle();
    }

    /// A point on the canvas, in the camera's own units — which are world
    /// units about the ship, with `y` still growing downwards the way a screen
    /// does. Turning that into a design tile or a system position is the
    /// caller's job, and it is a different job in each view.
    pub fn to_view(&self, x: f32, y: f32) -> (f32, f32) {
        (
            (x - self.offset_x()) / self.scale,
            (y - self.offset_y()) / self.scale,
        )
    }
}
