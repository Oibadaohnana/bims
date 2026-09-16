//! The galaxy preview: a camera over the star field, the pick under the
//! pointer, and the shapes that draw it.
//!
//! **Everything comes out in screen pixels.** The room and the designer emit
//! shapes in world units and hand the host a scale and an offset to paint
//! them through; this one does the projection itself and reports a transform
//! of one. A galaxy is fifty thousand light years across and a star on it is
//! two pixels wide whatever the zoom, so nearly every shape here is sized in
//! pixels already — projecting the *position* and leaving the rest alone is
//! the arithmetic that wants doing, and doing it in one place keeps the pick
//! and the paint agreeing about where a star is.
//!
//! North is up and the camera never rotates, for the same reason the game
//! view's does not: a map you learn is a map that holds still.

use worldgen::galaxy::{GALAXY_RADIUS, Star, StarClass};

use crate::draw::{Color, DrawList};

/// How far the pointer may be from a star and still be on it, in pixels.
///
/// Generous, because a star is two pixels wide. In a dense core at full
/// zoom-out there is always something inside this, and the nearest wins —
/// which is what keeps hovering usable there rather than a lottery.
pub const PICK_RADIUS: f32 = 12.0;

/// Furthest in, as a multiple of the fit. Sixty-four times is a light year
/// across a few pixels: enough to pull two stars a minimum separation apart
/// well clear of each other.
const MAX_ZOOM: f32 = 64.0;

/// How much of the canvas the galaxy takes up when it is fitted. Not all of
/// it: a rim star flush against the edge reads as a map that was cut off.
const FIT_FILL: f32 = 0.92;

/// How far off the canvas the galaxy's centre may be dragged, as a fraction
/// of the canvas — measured from the galaxy's *edge*, so at full zoom the rim
/// can be brought to the middle of the screen and the map can still not be
/// lost altogether.
const PAN_KEEP: f32 = 0.1;

/// How long a ping is visible, in seconds.
pub const PING_SECONDS: f32 = 1.8;

/// Branchless, and deliberately not `f32::clamp`: that one panics when its
/// bounds cross, and the panic path drags Rust's formatting machinery into
/// the wasm. Same reason the room and the designer each carry one.
pub fn clamp(v: f32, lo: f32, hi: f32) -> f32 {
    if v < lo {
        lo
    } else if v > hi {
        hi
    } else {
        v
    }
}

/// The camera: how far in, and how far the map has been shoved.
///
/// `screen = centre + pan + galaxy * scale`, with `y` flipped so that north
/// is up. [`Preview::to_screen`] and [`Preview::to_galaxy`] are that line and
/// its inverse, and nothing else in the crate turns one into the other.
pub struct Preview {
    pub width: f32,
    pub height: f32,
    /// Half the galaxy's span, in galaxy units: how far out the furthest
    /// star sits. Measured off the stars rather than assumed from
    /// `GALAXY_RADIUS`, because the off-arm scatter throws a few past it.
    extent: f32,
    scale: f32,
    pan_x: f32,
    pan_y: f32,
}

impl Preview {
    pub fn new(width: f32, height: f32, stars: &[Star]) -> Preview {
        let extent = stars
            .iter()
            .map(|s| s.position.x.abs().max(s.position.y.abs()) as f32)
            .fold(0.0, f32::max)
            .max(GALAXY_RADIUS as f32 * 0.5);
        let mut preview = Preview {
            width: width.max(1.0),
            height: height.max(1.0),
            extent,
            scale: 0.0,
            pan_x: 0.0,
            pan_y: 0.0,
        };
        preview.scale = preview.fit_scale();
        preview.settle();
        preview
    }

    pub fn resize(&mut self, width: f32, height: f32) {
        let fitted = self.scale == self.fit_scale();
        self.width = width.max(1.0);
        self.height = height.max(1.0);
        // A map that was showing the whole galaxy keeps showing the whole
        // galaxy when the panel changes size; one that was zoomed in stays
        // where it was looking.
        if fitted {
            self.scale = self.fit_scale();
        }
        self.settle();
    }

    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// The scale at which the whole galaxy fits the canvas.
    pub fn fit_scale(&self) -> f32 {
        self.width.min(self.height) * FIT_FILL / (2.0 * self.extent)
    }

    /// How far in the camera is, from nothing at the fit to one at the
    /// furthest. What the star sizes grow with.
    pub fn zoom_level(&self) -> f32 {
        let fit = self.fit_scale();
        if fit <= 0.0 {
            return 0.0;
        }
        clamp((self.scale / fit).ln() / MAX_ZOOM.ln(), 0.0, 1.0)
    }

    pub fn pan(&mut self, dx: f32, dy: f32) {
        self.pan_x += dx;
        self.pan_y += dy;
        self.settle();
    }

    /// Zoom about a point on the canvas, so whatever is under the pointer
    /// stays under it. `factor` arrives already worked out — the host does
    /// the exponential, exactly as it does for the designer.
    pub fn zoom(&mut self, at_x: f32, at_y: f32, factor: f32) {
        if !(factor > 0.0) || !factor.is_finite() {
            return;
        }
        let before = self.scale;
        let fit = self.fit_scale();
        self.scale = clamp(before * factor, fit, fit * MAX_ZOOM);
        if self.scale == before {
            return;
        }
        let ratio = self.scale / before;
        let (cx, cy) = (self.width / 2.0, self.height / 2.0);
        self.pan_x = at_x - (at_x - cx - self.pan_x) * ratio - cx;
        self.pan_y = at_y - (at_y - cy - self.pan_y) * ratio - cy;
        self.settle();
    }

    /// Put the camera back where it is allowed to be.
    fn settle(&mut self) {
        let fit = self.fit_scale();
        self.scale = clamp(self.scale, fit, fit * MAX_ZOOM);
        // The galaxy's edge may go as far as `PAN_KEEP` of the canvas from
        // the far side, and no further: some of it is always on screen.
        let reach = self.extent * self.scale;
        let x = self.width * (0.5 - PAN_KEEP) + reach;
        let y = self.height * (0.5 - PAN_KEEP) + reach;
        self.pan_x = clamp(self.pan_x, -x, x);
        self.pan_y = clamp(self.pan_y, -y, y);
    }

    /// Where a galaxy position lands on the canvas.
    pub fn to_screen(&self, x: f64, y: f64) -> (f32, f32) {
        (
            self.width / 2.0 + self.pan_x + x as f32 * self.scale,
            self.height / 2.0 + self.pan_y - y as f32 * self.scale,
        )
    }

    /// The galaxy position under a point on the canvas.
    pub fn to_galaxy(&self, sx: f32, sy: f32) -> (f64, f64) {
        (
            ((sx - self.width / 2.0 - self.pan_x) / self.scale) as f64,
            (-(sy - self.height / 2.0 - self.pan_y) / self.scale) as f64,
        )
    }

    /// The star under a point on the canvas: the nearest one within
    /// [`PICK_RADIUS`] pixels, or none.
    ///
    /// Measured on the *screen*, not in galaxy units, so the radius is the
    /// same size under the finger at every zoom. A thousand projections per
    /// pointer move is nothing.
    pub fn pick(&self, stars: &[Star], sx: f32, sy: f32) -> Option<u32> {
        let mut best: Option<(u32, f32)> = None;
        let limit = PICK_RADIUS * PICK_RADIUS;
        for star in stars {
            let (x, y) = self.to_screen(star.position.x, star.position.y);
            let d = (x - sx) * (x - sx) + (y - sy) * (y - sy);
            if d <= limit && best.is_none_or(|(_, b)| d < b) {
                best = Some((star.id, d));
            }
        }
        best.map(|(id, _)| id)
    }

    fn on_canvas(&self, x: f32, y: f32, margin: f32) -> bool {
        x >= -margin && x <= self.width + margin && y >= -margin && y <= self.height + margin
    }
}

/// A suggestion, drawn as a ring spreading out from a star and fading.
#[derive(Clone, Copy)]
pub struct Ping {
    pub star: u32,
    /// Seconds since it was made.
    pub age: f32,
}

/// What is drawn over the stars: the pointer's pick, the pending spawn, and
/// any pings.
pub struct Marks<'a> {
    pub hovered: Option<u32>,
    pub spawn: Option<u32>,
    pub pings: &'a [Ping],
}

const VOID: Color = Color::rgb(0.03, 0.05, 0.05);
const HOVER: Color = Color::rgb(0.50, 0.82, 0.66);
const SPAWN: Color = Color::rgb(1.0, 0.86, 0.45);
const PING: Color = Color::rgb(0.55, 0.80, 0.95);

/// How much of a star without a station shows. Dimmed rather than hidden:
/// it can still be inspected, and a map with holes in it reads as a map that
/// failed to load.
const DIM: f32 = 0.30;

/// A star's colour, by class: the classical sequence, blue to red.
fn star_color(class: StarClass) -> Color {
    match class {
        StarClass::O => Color::rgb(0.62, 0.72, 1.0),
        StarClass::B => Color::rgb(0.72, 0.80, 1.0),
        StarClass::A => Color::rgb(0.92, 0.94, 1.0),
        StarClass::F => Color::rgb(1.0, 0.97, 0.88),
        StarClass::G => Color::rgb(1.0, 0.92, 0.66),
        StarClass::K => Color::rgb(1.0, 0.76, 0.50),
        StarClass::M => Color::rgb(1.0, 0.56, 0.44),
    }
}

/// A star's radius on screen, in pixels, at the fit. Hot stars are bigger,
/// because the map should look like a sky and not like a scatter plot.
fn star_radius(class: StarClass) -> f32 {
    match class {
        StarClass::O => 3.2,
        StarClass::B => 2.8,
        StarClass::A => 2.4,
        StarClass::F => 2.1,
        StarClass::G => 2.0,
        StarClass::K => 1.8,
        StarClass::M => 1.5,
    }
}

/// Paint the whole preview.
///
/// `has_station` is indexed by star id. Stars are drawn in id order and the
/// marks on top of all of them, so a ring is never under a neighbour.
pub fn paint(
    preview: &Preview,
    stars: &[Star],
    has_station: &[bool],
    marks: &Marks,
    list: &mut DrawList,
) {
    list.clear();
    list.rect(
        preview.width / 2.0,
        preview.height / 2.0,
        preview.width,
        preview.height,
        0.0,
        VOID,
    );

    // Stars grow a little as the camera goes in, so a zoomed-in map is not a
    // field of the same specks further apart.
    let grow = 1.0 + 1.5 * preview.zoom_level();

    for star in stars {
        let (x, y) = preview.to_screen(star.position.x, star.position.y);
        if !preview.on_canvas(x, y, 8.0) {
            continue;
        }
        let r = star_radius(star.star_class) * grow;
        let lit = has_station.get(star.id as usize).copied().unwrap_or(false);
        let color = star_color(star.star_class);
        if lit {
            // A soft halo, so the stars worth going to can be found by eye
            // before they are found by hovering.
            list.ellipse(x, y, r * 5.0, r * 5.0, color.alpha(0.10));
            list.ellipse(x, y, r * 2.0, r * 2.0, color);
        } else {
            list.ellipse(x, y, r * 2.0, r * 2.0, color.alpha(DIM));
        }
    }

    if let Some(id) = marks.hovered.and_then(|id| stars.get(id as usize)) {
        let (x, y) = preview.to_screen(id.position.x, id.position.y);
        list.push(
            crate::draw::KIND_ELLIPSE,
            x,
            y,
            18.0,
            18.0,
            0.0,
            0.0,
            1.5,
            HOVER,
        );
    }

    if let Some(star) = marks.spawn.and_then(|id| stars.get(id as usize)) {
        let (x, y) = preview.to_screen(star.position.x, star.position.y);
        // A ring and a diamond: two shapes nothing else on the map uses, so
        // the start is the one thing that cannot be mistaken for a star.
        list.push(
            crate::draw::KIND_ELLIPSE,
            x,
            y,
            24.0,
            24.0,
            0.0,
            0.0,
            2.0,
            SPAWN,
        );
        list.push(
            crate::draw::KIND_RECT,
            x,
            y,
            26.0,
            26.0,
            core::f32::consts::FRAC_PI_4,
            0.0,
            1.0,
            SPAWN.alpha(0.7),
        );
    }

    for ping in marks.pings {
        let Some(star) = stars.get(ping.star as usize) else {
            continue;
        };
        let t = clamp(ping.age / PING_SECONDS, 0.0, 1.0);
        if t >= 1.0 {
            continue;
        }
        let (x, y) = preview.to_screen(star.position.x, star.position.y);
        let d = 10.0 + 50.0 * t;
        list.push(
            crate::draw::KIND_ELLIPSE,
            x,
            y,
            d,
            d,
            0.0,
            0.0,
            2.0,
            PING.alpha(1.0 - t),
        );
    }
}
