//! The heads: a sealed compartment in the corner of the deck, with a powered
//! door, a toilet and a washbasin.
//!
//! It is the only part of the ship that changes shape. The door is a solid
//! when it is shut and thin air when it is open, which is why the pathfinder
//! keeps two grids rather than one — see [`crate::nav::Maps`].

use crate::draw::{Color, DrawList};
use crate::math::{PI, Rect, TAU, Vec2, clamp, lerp, vec2};
use crate::room::{DECK_SEAM, GLOW, GLOW_DIM, HULL, PANEL, PANEL_EDGE, PANEL_LIT, STEEL, WARN};

/// Thickness of the compartment's own bulkheads.
const WALL: f32 = 16.0;
/// The doorway, and so the sliding panel that fills it. Wide enough that a
/// body still has room either side once the pathfinder has inflated the walls.
const DOORWAY: f32 = 84.0;
/// How far along the north bulkhead the doorway sits.
const DOOR_OFFSET: f32 = 84.0;
/// How far out from a fixture the Bim stands to use it.
const STAND_OFF: f32 = 34.0;

/// How fast the door panels travel, in fractions of open per second.
const DOOR_RATE: f32 = 2.6;
/// How long the bowl glows after a flush, and the tap runs after a wash.
const FLUSH_TIME: f32 = 2.2;

const BOWL: Color = Color::rgb(0.80, 0.84, 0.87);
const BOWL_RIM: Color = Color::rgb(0.62, 0.68, 0.72);
const WATER: Color = Color::rgb(0.24, 0.52, 0.66);
const BASIN: Color = Color::rgb(0.74, 0.79, 0.82);
/// A deck plate a shade off the main one, so the compartment reads as a
/// separate room even where the hull provides two of its walls.
const BATH_DECK: Color = Color::rgb(0.17, 0.20, 0.24);

pub struct Bath {
    /// The whole compartment, bulkheads included.
    pub shell: Rect,
    /// The deck inside it.
    pub inner: Rect,
    /// The three bulkhead runs. The doorway is the gap in the north one; the
    /// ship's own hull closes the other two sides.
    walls: [Rect; 3],
    /// The opening, and the panels that fill it when shut.
    pub door: Rect,
    pub toilet: Rect,
    pub sink: Rect,

    /// 0 shut, 1 fully retracted into the bulkhead. Animated towards `target`.
    pub open: f32,
    target: f32,
    pub locked: bool,

    /// Seconds left of the cistern refilling, and of the tap running.
    flush: f32,
    tap: f32,
    time: f32,
}

impl Bath {
    /// Tucked into the bottom-right corner, using two walls of the hull so
    /// only two bulkheads have to be built.
    pub fn new(interior: Rect) -> Bath {
        let shell = Rect::from_corners(
            vec2(interior.max.x - 214.0, interior.max.y - 194.0),
            interior.max,
        );
        let inner = Rect::from_corners(shell.min + vec2(WALL, WALL), shell.max);

        let door = Rect::from_min_size(
            vec2(shell.min.x + DOOR_OFFSET, shell.min.y),
            vec2(DOORWAY, WALL),
        );
        let walls = [
            // North, either side of the doorway.
            Rect::from_corners(shell.min, vec2(door.min.x, shell.min.y + WALL)),
            Rect::from_corners(
                vec2(door.max.x, shell.min.y),
                vec2(shell.max.x, shell.min.y + WALL),
            ),
            // West, running down from under the north bulkhead.
            Rect::from_corners(
                vec2(shell.min.x, shell.min.y + WALL),
                vec2(shell.min.x + WALL, shell.max.y),
            ),
        ];

        // The pan stands against the hull on the far side, cistern to the
        // wall; the basin is set into a shelf beside the door.
        let toilet = Rect::from_min_size(
            vec2(shell.max.x - 62.0, shell.min.y + 96.0),
            vec2(62.0, 76.0),
        );
        let sink = Rect::from_min_size(vec2(inner.min.x + 6.0, inner.min.y), vec2(64.0, 30.0));

        Bath {
            shell,
            inner,
            walls,
            door,
            toilet,
            sink,
            open: 0.0,
            target: 0.0,
            locked: false,
            flush: 0.0,
            tap: 0.0,
            time: 0.0,
        }
    }

    // --- what the rest of the game asks about ----------------------------

    /// The bulkheads. Fixed, unlike the door.
    pub fn solids(&self) -> [Rect; 3] {
        self.walls
    }

    /// The doorway, when there is a door across it to walk around.
    pub fn closed_door(&self) -> Option<Rect> {
        if self.is_open() {
            None
        } else {
            Some(self.door)
        }
    }

    /// Reads the target rather than the animation: a door on its way open is
    /// one the Bim can already plan a route through, and waiting for the
    /// panels to finish would stall the step that just opened it.
    pub fn is_open(&self) -> bool {
        self.target > 0.5
    }

    pub fn set_open(&mut self, open: bool) {
        // A locked door stays shut; unlock it first.
        if open && self.locked {
            return;
        }
        self.target = if open { 1.0 } else { 0.0 };
    }

    /// Locking also shuts it — a locked door standing open is not locked.
    pub fn set_locked(&mut self, locked: bool) {
        self.locked = locked;
        if locked {
            self.target = 0.0;
        }
    }

    pub fn flush(&mut self) {
        self.flush = FLUSH_TIME;
    }

    pub fn run_tap(&mut self, seconds: f32) {
        self.tap = seconds;
    }

    /// Outside the door, on the deck, and just inside it.
    pub fn outside_station(&self) -> Vec2 {
        vec2(self.door.center().x, self.shell.min.y - STAND_OFF)
    }

    pub fn inside_station(&self) -> Vec2 {
        vec2(self.door.center().x, self.door.max.y + STAND_OFF)
    }

    /// Where the Bim sits, and where it stands to turn round first.
    pub fn toilet_seat(&self) -> Vec2 {
        vec2(self.toilet.min.x + 18.0, self.toilet.center().y)
    }

    pub fn toilet_station(&self) -> Vec2 {
        self.toilet_seat() - vec2(48.0, 0.0)
    }

    pub fn basin_pos(&self) -> Vec2 {
        vec2(self.sink.center().x, self.sink.max.y - 13.0)
    }

    pub fn sink_station(&self) -> Vec2 {
        vec2(self.sink.center().x, self.sink.max.y + STAND_OFF)
    }

    /// Which fixture in here is under a point, using the codes in `room.rs`.
    pub fn hit(&self, p: Vec2) -> u32 {
        if self.toilet.expand(4.0).contains(p) {
            crate::room::HIT_TOILET
        } else if self.door.expand(8.0).contains(p) {
            crate::room::HIT_DOOR
        } else {
            crate::room::HIT_NONE
        }
    }

    pub fn update(&mut self, dt: f32) {
        self.time += dt;
        let step = DOOR_RATE * dt;
        self.open += clamp(self.target - self.open, -step, step);
        self.flush = (self.flush - dt).max(0.0);
        self.tap = (self.tap - dt).max(0.0);
    }

    // --- drawing ---------------------------------------------------------

    pub fn draw(&self, list: &mut DrawList) {
        self.draw_shell(list);
        self.draw_toilet(list);
        self.draw_sink(list);
        self.draw_door(list);
    }

    fn draw_shell(&self, list: &mut DrawList) {
        // A deck plate of its own, so the compartment reads as a separate
        // room even where the bulkhead is the hull.
        list.rect(self.inner.center(), self.inner.size(), 0.0, 0.0, BATH_DECK);
        list.stroke_rect(
            self.inner.center(),
            self.inner.size() - vec2(3.0, 3.0),
            0.0,
            0.0,
            2.0,
            GLOW.alpha(0.13),
        );
        let mut y = self.inner.min.y + 44.0;
        while y < self.inner.max.y {
            list.line(
                vec2(self.inner.min.x, y),
                vec2(self.inner.max.x, y),
                1.0,
                DECK_SEAM,
            );
            y += 44.0;
        }

        for wall in &self.walls {
            list.rect(wall.center(), wall.size(), 0.0, 3.0, PANEL);
            // A lit strip along the inward face, the way a corridor is lit.
            let lit = if wall.width() > wall.height() {
                Rect::from_corners(vec2(wall.min.x, wall.max.y - 3.5), wall.max)
            } else {
                Rect::from_corners(vec2(wall.max.x - 3.5, wall.min.y), wall.max)
            };
            list.rect(lit.center(), lit.size(), 0.0, 1.0, GLOW_DIM);
            list.stroke_rect(wall.center(), wall.size(), 0.0, 3.0, 1.0, PANEL_EDGE);
        }
    }

    /// Two leaves parting in the middle, each sliding away under the bulkhead
    /// it meets. A status lamp beside them reads locked at a glance.
    fn draw_door(&self, list: &mut DrawList) {
        let d = self.door;
        // The opening itself: a dark slot, lit down both jambs, so that an
        // open door is plainly a gap and not just a lighter piece of wall.
        list.rect(d.center(), d.size() + vec2(6.0, 2.0), 0.0, 0.0, HULL);
        for side in [-1.0f32, 1.0] {
            list.rect(
                vec2(d.center().x + side * (d.width() * 0.5 + 1.5), d.center().y),
                vec2(3.0, d.height()),
                0.0,
                1.0,
                GLOW.alpha(0.55),
            );
        }

        let leaf = d.width() * 0.5;
        for side in [-1.0f32, 1.0] {
            let shut = d.center().x + side * leaf * 0.5;
            let at = vec2(lerp(shut, shut + side * leaf, self.open), d.center().y);
            // Each leaf is drawn a shade above the bulkhead and outlined, so
            // the join down the middle stays visible against it.
            list.rect(
                at,
                vec2(leaf, d.height() - 2.0),
                0.0,
                2.0,
                STEEL.alpha(0.55),
            );
            list.rect(at, vec2(leaf - 5.0, d.height() - 7.0), 0.0, 2.0, PANEL_LIT);
            // Chevrons on the leading edge, pointing the way it opens, and
            // going red when the door is locked.
            for i in 0..2 {
                list.rect(
                    vec2(at.x + side * (leaf * 0.5 - 7.0 - i as f32 * 7.0), at.y),
                    vec2(2.5, d.height() - 9.0),
                    0.0,
                    1.0,
                    if self.locked { WARN } else { GLOW },
                );
            }
        }

        // Status lamp on the bulkhead beside the opening, pulsing when locked.
        let lamp = vec2(d.max.x + 13.0, d.center().y);
        let pulse = 0.65 + 0.35 * (self.time * 3.4).sin();
        let (colour, strength) = if self.locked {
            (WARN, pulse)
        } else {
            (GLOW, 0.8)
        };
        list.circle(lamp, 13.0, colour.alpha(0.18 * strength));
        list.circle(lamp, 7.0, colour.alpha(strength));
    }

    /// A pan seen from directly above is a stack of ovals, which on its own
    /// reads as a porthole. What makes it a toilet is the cistern standing
    /// proud of it against the wall, and the seat being a ring with a gap at
    /// the front rather than a closed disc.
    fn draw_toilet(&self, list: &mut DrawList) {
        let t = self.toilet;
        let seat = self.toilet_seat();

        // Cistern, moulded into the hull behind and taller than the pan so it
        // shows at both ends.
        let cistern = vec2(t.max.x - 16.0, t.center().y);
        list.rect(cistern, vec2(32.0, t.height()), 0.0, 6.0, PANEL_LIT);
        list.stroke_rect(cistern, vec2(32.0, t.height()), 0.0, 6.0, 1.0, PANEL_EDGE);
        // Flush plate on it, lit, and brighter while it is running.
        let churn = self.flush / FLUSH_TIME;
        list.rect(
            cistern,
            vec2(14.0, 26.0),
            0.0,
            3.0,
            GLOW.alpha(0.35 + 0.55 * churn),
        );

        // Pan: a pedestal, wider at the front than where it meets the cistern.
        list.ellipse(seat + vec2(6.0, 0.0), vec2(44.0, 60.0), 0.0, BOWL_RIM);
        list.ellipse(seat, vec2(46.0, 52.0), 0.0, BOWL_RIM);
        list.ellipse(seat, vec2(40.0, 46.0), 0.0, BOWL);

        // The seat: a ring, with the gap at the front the way a seat is cut.
        list.stroke_ellipse(seat, vec2(31.0, 37.0), 0.0, 6.0, BOWL_RIM);
        list.rect(seat - vec2(16.0, 0.0), vec2(9.0, 13.0), 0.0, 2.0, BOWL);

        // Water, lit from under the rim, and swirling after a flush.
        list.ellipse(seat, vec2(24.0, 30.0), 0.0, WATER);
        if churn > 0.0 {
            for i in 0..4 {
                let a = self.time * 7.0 + i as f32 * (TAU / 4.0);
                let at = seat + Vec2::from_angle(a) * (8.0 * churn);
                list.circle(at, 5.0 + 4.0 * churn, GLOW.alpha(0.35 * churn));
            }
            list.stroke_ellipse(seat, vec2(31.0, 37.0), 0.0, 2.5, GLOW.alpha(0.8 * churn));
        }
    }

    fn draw_sink(&self, list: &mut DrawList) {
        let s = self.sink;
        list.rect(s.center(), s.size(), 0.0, 5.0, PANEL_LIT);
        list.rect(
            vec2(s.center().x, s.max.y - 2.0),
            vec2(s.width(), 4.0),
            0.0,
            2.0,
            PANEL_EDGE,
        );

        let basin = self.basin_pos();
        list.ellipse(basin, vec2(44.0, 26.0), 0.0, BOWL_RIM);
        list.ellipse(basin, vec2(37.0, 20.0), 0.0, BASIN);
        list.ellipse(basin, vec2(12.0, 7.0), 0.0, BOWL_RIM);

        // Tap: a spout reaching over the basin from the wall side.
        let spout = vec2(basin.x, s.min.y + 8.0);
        list.rect(spout, vec2(9.0, 16.0), 0.0, 3.0, STEEL);
        list.circle(spout + vec2(0.0, 7.0), 8.0, STEEL);

        if self.tap <= 0.0 {
            return;
        }
        // Running water: a column from the spout and a ring spreading in the
        // basin, both moving so it reads as flowing rather than painted on.
        list.rect(
            vec2(basin.x, (spout.y + basin.y) * 0.5 + 3.0),
            vec2(5.0, basin.y - spout.y),
            0.0,
            2.0,
            GLOW.alpha(0.55),
        );
        for i in 0..3 {
            let t = (self.time * 1.7 + i as f32 / 3.0) % 1.0;
            list.stroke_ellipse(
                basin,
                vec2(16.0 + 22.0 * t, 9.0 + 13.0 * t),
                0.0,
                1.5,
                GLOW.alpha(0.45 * (1.0 - t)),
            );
        }
        // A little splash at the point of impact.
        for i in 0..4 {
            let a = self.time * 5.0 + i as f32 * (TAU / 4.0) + PI * 0.25;
            let at = basin + Vec2::from_angle(a) * vec2(9.0, 5.0).len();
            list.circle(at, 3.0, GLOW.alpha(0.3));
        }
    }
}
