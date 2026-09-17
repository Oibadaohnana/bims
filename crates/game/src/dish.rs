//! The dishwasher: a unit in the galley counter front.
//!
//! It holds ten plates. The tenth one in starts a cycle, and the cycle runs on
//! the world clock rather than on anything the Bim is doing, so the Bim is free
//! the whole two hours it takes.
//!
//! Plates are counted in two places on purpose. `loaded` is the dirty stack
//! waiting to go round; `washing` is what is going round now. Separating them
//! means a plate cleared away *during* a cycle joins the next load instead of
//! vanishing when this one finishes.

use crate::clock::{HOUR, MINUTES_PER_SECOND};
use crate::draw::{Color, DrawList};
use crate::math::{Rect, TAU, Vec2, clamp, lerp, vec2};
use crate::room::{GLOW, PANEL, PANEL_EDGE, PANEL_LIT, STEEL, WARN};

/// How many plates it stows before it has to be run.
pub const CAPACITY: u32 = 10;
/// How long a cycle takes, in game minutes.
pub const CYCLE_MINUTES: f32 = 2.0 * HOUR;

/// How fast the door drops open, in fractions of open per second.
const DOOR_RATE: f32 = 2.4;
/// How far the door travels out into the room when it opens.
const DOOR_TRAVEL: f32 = 26.0;

const CAVITY: Color = Color::rgb(0.09, 0.11, 0.13);
const RACK: Color = Color::rgb(0.42, 0.48, 0.54);

pub struct Dishwasher {
    /// The door, as a face on the front of the counter run.
    pub face: Rect,
    /// The whole appliance, where it stands on its own: a ship's dishwasher
    /// is one tile, and the room draws that tile, so the body is drawn here
    /// with the door on its front. `None` in the classic room, where it is
    /// set into the counter run and the counter is the body.
    pub body: Option<Rect>,

    /// Dirty plates waiting for a cycle.
    pub loaded: u32,
    /// Plates in the cycle running now; zero when it is not running.
    pub washing: u32,
    /// Game minutes left of that cycle.
    pub cycle_left: f32,

    /// 0 shut, 1 hanging open. Animated towards `target`.
    pub open: f32,
    target: f32,
    time: f32,
}

impl Dishwasher {
    /// Set into the counter front, in the run between the drawer and the hob.
    pub fn new(counter: Rect) -> Dishwasher {
        Dishwasher::at(Rect::from_min_size(
            vec2(counter.min.x + 292.0, counter.max.y - 22.0),
            vec2(70.0, 20.0),
        ))
    }

    /// A dishwasher with its door face wherever a layout puts it.
    pub fn at(face: Rect) -> Dishwasher {
        Dishwasher {
            face,
            body: None,
            loaded: 0,
            washing: 0,
            cycle_left: 0.0,
            open: 0.0,
            target: 0.0,
            time: 0.0,
        }
    }

    pub fn is_running(&self) -> bool {
        self.cycle_left > 0.0
    }

    /// True once there is no room for another plate, which is what makes the
    /// Bim reach for the button instead of walking away.
    pub fn is_full(&self) -> bool {
        self.loaded >= CAPACITY
    }

    pub fn set_open(&mut self, open: bool) {
        self.target = if open { 1.0 } else { 0.0 };
    }

    /// Stack one more plate. Silently caps: the chain checks `is_full` for
    /// itself, and a dropped plate is better than an eleventh one.
    pub fn stack(&mut self) {
        self.loaded = (self.loaded + 1).min(CAPACITY);
    }

    /// Begin a cycle with whatever is loaded. Refused when there is nothing in
    /// it, or when one is already running — in which case the stack simply
    /// waits, and the next Bim to finish a meal tries again.
    pub fn start(&mut self) {
        if self.loaded == 0 || self.is_running() {
            return;
        }
        self.washing = self.loaded;
        self.loaded = 0;
        self.cycle_left = CYCLE_MINUTES;
    }

    /// How far through the cycle it is, 0 to 1.
    pub fn progress(&self) -> f32 {
        if !self.is_running() {
            return 0.0;
        }
        1.0 - self.cycle_left / CYCLE_MINUTES
    }

    pub fn update(&mut self, dt: f32) {
        self.time += dt;
        let step = DOOR_RATE * dt;
        self.open += clamp(self.target - self.open, -step, step);

        if self.cycle_left > 0.0 {
            self.cycle_left = (self.cycle_left - dt * MINUTES_PER_SECOND).max(0.0);
            if self.cycle_left == 0.0 {
                // Done: the clean rack goes back into the galley stores.
                self.washing = 0;
            }
        }
    }

    // --- drawing ---------------------------------------------------------

    pub fn draw(&self, list: &mut DrawList) {
        let f = self.face;
        let drop = self.open * DOOR_TRAVEL;

        // The appliance itself, the whole tile, where it stands alone.
        if let Some(b) = self.body {
            list.rect(b.center(), b.size(), 0.0, 3.0, PANEL);
            list.stroke_rect(b.center(), b.size(), 0.0, 3.0, 1.0, PANEL_EDGE.alpha(0.6));
        }

        // The cavity behind the door, with a rack and whatever is stacked in
        // it, revealed as the door comes down.
        if self.open > 0.02 {
            let cavity = Rect::from_corners(
                vec2(f.min.x + 3.0, f.min.y - 2.0),
                vec2(f.max.x - 3.0, f.max.y + drop),
            );
            list.rect(cavity.center(), cavity.size(), 0.0, 3.0, CAVITY);
            for row in 0..2 {
                let y = lerp(cavity.min.y + 6.0, cavity.max.y - 6.0, row as f32);
                list.line(
                    vec2(cavity.min.x + 5.0, y),
                    vec2(cavity.max.x - 5.0, y),
                    1.5,
                    RACK.alpha(self.open),
                );
            }
            // Plates on edge in the rack, as many as are actually in there.
            let shown = self.loaded.max(self.washing).min(CAPACITY);
            for i in 0..shown {
                let x = lerp(
                    cavity.min.x + 8.0,
                    cavity.max.x - 8.0,
                    i as f32 / (CAPACITY - 1) as f32,
                );
                list.rect(
                    vec2(x, cavity.center().y),
                    vec2(3.0, cavity.height() - 10.0),
                    0.0,
                    1.5,
                    STEEL.alpha(0.75 * self.open),
                );
            }
        }

        // The door itself, hanging down into the room as it opens.
        let door = vec2(f.center().x, f.center().y + drop);
        list.rect(door, f.size() + vec2(0.0, 4.0), 0.0, 3.0, PANEL_LIT);
        list.stroke_rect(door, f.size() + vec2(0.0, 4.0), 0.0, 3.0, 1.0, PANEL_EDGE);
        // Handle bar across the top of it.
        list.rect(
            door + vec2(0.0, -5.0),
            vec2(f.width() - 16.0, 3.0),
            0.0,
            1.5,
            STEEL,
        );

        // A pip per plate it can hold, so the load is readable from across the
        // room without opening anything. Bright for a dirty plate waiting,
        // dim for one already going round, dark for an empty slot — which
        // means a cycle still looks full rather than looking empty.
        for i in 0..CAPACITY {
            let x = lerp(
                door.x - f.width() * 0.5 + 7.0,
                door.x + f.width() * 0.5 - 7.0,
                i as f32 / (CAPACITY - 1) as f32,
            );
            let at = vec2(x, door.y + 4.5);
            if i < self.loaded {
                list.circle(at, 5.5, GLOW.alpha(0.9));
            } else if i < self.loaded + self.washing {
                list.circle(at, 5.0, GLOW.alpha(0.35));
            } else {
                list.circle(at, 4.5, PANEL_EDGE.alpha(0.55));
            }
        }

        if !self.is_running() {
            return;
        }
        // Running: a bar filling along the bottom edge, and the whole door
        // breathing, so a cycle is obvious at a glance from anywhere.
        let pulse = 0.55 + 0.45 * (self.time * 2.2).sin();
        list.stroke_rect(
            door,
            f.size() + vec2(4.0, 8.0),
            0.0,
            4.0,
            1.5,
            GLOW.alpha(0.30 + 0.35 * pulse),
        );
        let run = f.width() - 12.0;
        let done = run * self.progress();
        list.rect(
            vec2(
                door.x - run * 0.5 + done * 0.5,
                door.y + f.height() * 0.5 + 0.5,
            ),
            vec2(done.max(1.0), 3.0),
            0.0,
            1.5,
            GLOW,
        );
        // Water going round behind the door.
        for i in 0..3 {
            let t = (self.time * 0.9 + i as f32 / 3.0) % 1.0;
            let a = t * TAU;
            let at = door + Vec2::from_angle(a) * vec2(f.width() * 0.28, f.height() * 0.22).len();
            list.circle(at, 4.0, GLOW.alpha(0.20 * (1.0 - t)));
        }
        // Nearly done: the bar tips warm, the way an appliance chirps at you.
        if self.progress() > 0.94 {
            list.circle(
                door + vec2(f.width() * 0.5 - 7.0, -5.0),
                6.0,
                WARN.alpha(pulse),
            );
        }
    }
}
