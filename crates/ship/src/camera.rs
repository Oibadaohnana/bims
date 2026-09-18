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
//! hands out (`screen = world * scale + offset`), so `crates/app` paints
//! either page with one loop. The difference is what the origin is: the design
//! phase's is the corner of the build area, and this one is **the ship** —
//! everything in the ship view is drawn in the camera's units about the
//! ship's centre of mass.
//!
//! What sits in the *middle* of the window is a different question, and it
//! is [`Camera::focus`]: a point in those same units that the pan is
//! measured from and clamped about. In the ship view it is the crew member
//! the player steers, set every frame (`Game::follow_player`), so the view
//! follows them off the ship and through a station's airlock and can never
//! be dragged until they are off the edge. On the map it is nought — the
//! ship.
//!
//! That is the camera **tethered**, and it can also be let [`loose`]
//! (`Game::follow` off): then nobody sets the focus, a pan moves the focus
//! itself and nothing clamps it — a free camera, for looking at the far
//! end of a station while the crew are busy at this one. The two are one
//! camera and one transform; which of the pan and the focus a drag moves is
//! the whole difference, so a view let loose stays exactly where it was
//! and a view tethered again snaps back to its subject on the next frame.
//!
//! [`loose`]: Camera::set_loose

use crate::view::clamp;

/// One camera: how far in, and how far the player has shoved it.
pub struct Camera {
    pub width: f32,
    pub height: f32,
    scale: f32,
    /// Screen pixels away from the middle. Clamped so the thing at the focus
    /// is always somewhere on the canvas: a view that can be dragged until
    /// the thing it is about is off the edge is a view a player gets lost in.
    pan_x: f32,
    pan_y: f32,
    /// What the pan is measured from, in the camera's own units: the point
    /// that sits in the middle of the window when the pan is nought. The
    /// origin unless somebody says otherwise — see the module note.
    focus_x: f32,
    focus_y: f32,
    /// Whether the focus is nobody's: a pan then moves the focus rather
    /// than the clamped shove, and the view goes wherever it is dragged.
    /// See the module note.
    loose: bool,
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
            focus_x: 0.0,
            focus_y: 0.0,
            loose: false,
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

    /// Where the origin lands on the canvas: the middle shoved by the pan,
    /// less however far the focus is from the origin — so the focus is what
    /// lands in the middle.
    pub fn offset_x(&self) -> f32 {
        self.width / 2.0 + self.pan_x - self.focus_x * self.scale
    }

    pub fn offset_y(&self) -> f32 {
        self.height / 2.0 + self.pan_y - self.focus_y * self.scale
    }

    /// What the view is about, in the camera's units. The pan is left alone,
    /// so a view shoved a little off its subject stays shoved a little off
    /// its subject as the subject moves.
    pub fn set_focus(&mut self, x: f32, y: f32) {
        if x.is_finite() && y.is_finite() {
            self.focus_x = x;
            self.focus_y = y;
        }
    }

    pub fn focus(&self) -> (f32, f32) {
        (self.focus_x, self.focus_y)
    }

    /// Put `(x, y)` in the middle of the canvas: the focus set there and the
    /// shove undone. Works tethered or loose, and changes neither.
    pub fn recentre(&mut self, x: f32, y: f32) {
        self.set_focus(x, y);
        self.pan_x = 0.0;
        self.pan_y = 0.0;
    }

    /// Let the camera go, or tether it again. Letting it go moves nothing:
    /// whatever shove the pan was holding is folded into the focus, so the
    /// view stays put and a drag from here on carries the focus with it.
    /// Tethered again, the pan starts from nought about whatever focus is
    /// next set.
    pub fn set_loose(&mut self, loose: bool) {
        if loose && !self.loose {
            self.absorb_pan();
        }
        self.loose = loose;
    }

    pub fn is_loose(&self) -> bool {
        self.loose
    }

    /// Fold the pan into the focus. The offset comes out the same, so
    /// nothing on screen moves.
    fn absorb_pan(&mut self) {
        self.focus_x -= self.pan_x / self.scale;
        self.focus_y -= self.pan_y / self.scale;
        self.pan_x = 0.0;
        self.pan_y = 0.0;
    }

    pub fn pan(&mut self, dx: f32, dy: f32) {
        if self.loose {
            // A free camera: the focus goes where it is dragged, unclamped.
            self.focus_x -= dx / self.scale;
            self.focus_y -= dy / self.scale;
            return;
        }
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
        // What is under the pointer, in the camera's units, before the
        // change; it has to be under the pointer after it too.
        let (ux, uy) = self.to_view(at_x, at_y);
        self.scale = clamp(before * factor, self.min_scale, self.max_scale);
        if self.scale == before {
            return;
        }
        // `at = offset + u * scale` and `offset = middle + pan - focus * scale`,
        // solved for the pan.
        self.pan_x = at_x - (ux - self.focus_x) * self.scale - self.width / 2.0;
        self.pan_y = at_y - (uy - self.focus_y) * self.scale - self.height / 2.0;
        if self.loose {
            // Nothing to clamp about: the shove goes into the focus instead.
            self.absorb_pan();
        } else {
            self.settle();
        }
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
