//! The Bim: a top-down character that decides where to go on its own, unless
//! a task is telling it what to do.

use crate::draw::{Color, DrawList};
use crate::math::{PI, Rect, TAU, Vec2, angle_lerp, approach, clamp, lerp, vec2, wrap_angle};
use crate::rng::Rng;
use crate::room::{GRIP, STEEL, draw_knife, draw_plate, draw_spoon};

// --- behaviour tuning ---------------------------------------------------

/// How sharply the body swings towards the heading it wants.
const TURN_RATE: f32 = 5.5;
/// Pixels per second squared, used both to speed up and to slow down.
const ACCEL: f32 = 220.0;
/// Odds that a finished walk is followed by a pause rather than another walk.
const PAUSE_CHANCE: f32 = 0.28;
/// Odds that a new walk picks a wholly new direction instead of a gentle turn.
const REVERSAL_CHANCE: f32 = 0.15;
/// Speed when marching to a spot a task or the player picked.
const MARCH_SPEED: f32 = 96.0;
/// How close counts as arrived at the final waypoint.
const ARRIVE_RADIUS: f32 = 5.0;
/// How close counts as having rounded an intermediate corner. Looser than
/// arrival so a multi-leg route flows instead of stuttering at every turn.
const WAYPOINT_RADIUS: f32 = 11.0;
/// Distance over which the Bim eases down on its approach.
const SLOWDOWN_RADIUS: f32 = 70.0;
/// How long it stands still on arrival before wandering off again.
const ARRIVE_SETTLE: f32 = 0.9;
/// How much bigger the Bim is drawn than the original sprite. Everything about
/// the body — parts, arm reach, where held items sit — goes through this, so the
/// proportions against the pot and the table stay as designed.
pub const BODY_SCALE: f32 = 1.45;
/// How far the body centre is kept clear of walls and furniture.
pub const BODY_MARGIN: f32 = 23.0;
/// How close a click or marquee has to come to count as touching the Bim.
const PICK_RADIUS: f32 = 26.0;
/// How fast the walls talk the Bim out of a plan that points at them.
const INTENT_RATE: f32 = 3.0;
/// How far from a wall the Bim starts turning back, as a fraction of the
/// smaller room dimension so a narrow room is not entirely "edge".
const EDGE_MARGIN_FRAC: f32 = 0.16;
const EDGE_MARGIN_MIN: f32 = 40.0;
const EDGE_MARGIN_MAX: f32 = 120.0;
/// How far from a piece of furniture the Bim starts going round it.
const AVOID_RANGE: f32 = 54.0;

/// One up-and-down of the knife, and one trip of the fork to the mouth. Tasks
/// count their steps in these units so the animation and the state agree.
pub const CHOP_PERIOD: f32 = 0.34;
pub const SCOOP_PERIOD: f32 = 0.62;
pub const BITE_PERIOD: f32 = 0.95;

// --- look ---------------------------------------------------------------

const SHIRT: Color = Color::rgb(0.33, 0.58, 0.85);
const SLEEVE: Color = Color::rgb(0.27, 0.49, 0.74);
const SKIN: Color = Color::rgb(0.91, 0.73, 0.55);
const NOSE: Color = Color::rgb(0.82, 0.62, 0.45);
const HAIR: Color = Color::rgb(0.23, 0.17, 0.12);
const BOOT: Color = Color::rgb(0.24, 0.18, 0.13);
const OUTLINE: Color = Color::rgba(0.05, 0.08, 0.07, 0.55);
const SHADOW: Color = Color::rgba(0.0, 0.0, 0.0, 0.20);
const VEG: Color = Color::rgb(0.44, 0.68, 0.24);
const VEG_DARK: Color = Color::rgb(0.30, 0.50, 0.16);
/// The Zs that float off a sleeping Bim.
const SLEEP_Z: Color = Color::rgb(0.80, 0.90, 0.95);
/// Shared with the trail and the order marker, so everything the player is
/// steering reads as one colour.
pub const ACCENT: Color = Color::rgb(0.50, 0.82, 0.66);

#[derive(Clone, Copy, PartialEq)]
enum Activity {
    Walking,
    Pausing,
    /// Heading for a spot the player or a task picked, rather than one it chose.
    Marching,
}

/// Something in the Bim's hands.
#[derive(Clone, Copy, PartialEq)]
pub enum Held {
    Nothing,
    Vegetable,
    Slices,
    Knife,
    Spoon,
    /// A plate, carrying how full it is.
    Plate(f32),
}

/// What the hands are busy doing. Each one drives its own arm animation.
#[derive(Clone, Copy, PartialEq)]
pub enum Action {
    None,
    /// Arms out in front: opening a door, setting something down, reaching in.
    Reach,
    /// The knife going up and down on the board.
    Chop,
    /// The spoon swinging from the pot across to the plate.
    Serve,
    /// Cutlery going from the plate to the mouth and back.
    Eat,
    /// Out cold, arms tucked in, breathing slowly.
    Sleep,
    /// Both hands together under the tap, turning over one another.
    Wash,
}

/// One turn of the hands under the tap.
const SCRUB_PERIOD: f32 = 0.55;

/// How long one breath takes while asleep, in seconds.
const BREATH_PERIOD: f32 = 5.4;

pub struct Character {
    pub pos: Vec2,
    /// The direction the body actually faces, in radians.
    pub heading: f32,
    pub speed: f32,

    /// Where the Bim *intends* to go, before steering is applied.
    intent: f32,
    target_speed: f32,

    activity: Activity,
    /// Seconds left before the current activity is reconsidered.
    timer: f32,

    /// Advances with distance travelled, so the walk cycle never slides.
    stride: f32,
    /// Advances with time; drives idle breathing and looking around.
    idle: f32,

    /// Waypoints left to walk, from the pathfinder. Overrides the wander until
    /// the last one is reached.
    path: Vec<Vec2>,
    pub selected: bool,
    /// Animates the selection ring so a picked Bim reads at a glance.
    select_pulse: f32,

    /// While scripted, the Bim does nothing of its own accord — a task is
    /// driving it, and it stands still between instructions.
    scripted: bool,
    /// A direction to turn to on the spot, used between scripted steps.
    face_target: Option<f32>,
    seated: bool,

    main: Held,
    tool: Held,
    action: Action,
    action_phase: f32,
}

impl Character {
    pub fn new(pos: Vec2, rng: &mut Rng) -> Character {
        let heading = rng.range(0.0, TAU);
        let mut c = Character {
            pos,
            heading,
            speed: 0.0,
            intent: heading,
            target_speed: 0.0,
            activity: Activity::Pausing,
            timer: 0.0,
            stride: 0.0,
            idle: rng.range(0.0, 100.0),
            path: Vec::new(),
            selected: false,
            select_pulse: 0.0,
            scripted: false,
            face_target: None,
            seated: false,
            main: Held::Nothing,
            tool: Held::Nothing,
            action: Action::None,
            action_phase: 0.0,
        };
        c.begin_walk(rng);
        c
    }

    pub fn is_walking(&self) -> bool {
        self.activity != Activity::Pausing && self.speed > 4.0
    }

    /// Radius used for picking and marquee selection.
    pub fn pick_radius(&self) -> f32 {
        PICK_RADIUS
    }

    /// True once an ordered walk has finished, so a task can move on.
    pub fn arrived(&self) -> bool {
        self.path.is_empty()
    }

    // --- orders and scripting -------------------------------------------

    /// Walk the given route, then go back to whatever it was doing before.
    ///
    /// An empty route means the pathfinder found nowhere to go, so the Bim
    /// deliberately does not enter the marching state — a task waiting on
    /// `arrived` would otherwise wait forever.
    pub fn follow_path(&mut self, route: Vec<Vec2>) {
        self.path = route;
        self.timer = 0.0;
        self.face_target = None;
        if !self.path.is_empty() {
            self.activity = Activity::Marching;
        }
    }

    /// Hand control to a task, or give it back.
    pub fn set_scripted(&mut self, on: bool) {
        self.scripted = on;
        if on {
            self.path.clear();
            self.activity = Activity::Pausing;
            self.timer = 0.0;
        } else {
            self.face_target = None;
            self.seated = false;
            self.main = Held::Nothing;
            self.tool = Held::Nothing;
            self.action = Action::None;
            self.timer = 0.0;
        }
    }

    pub fn is_scripted(&self) -> bool {
        self.scripted
    }

    /// Turn on the spot to face `angle`.
    pub fn face(&mut self, angle: f32) {
        self.face_target = Some(angle);
    }

    /// True once a `face` has very nearly finished.
    pub fn facing_settled(&self) -> bool {
        match self.face_target {
            None => true,
            Some(a) => wrap_angle(a - self.heading).abs() < 0.12,
        }
    }

    pub fn sit(&mut self, at: Vec2, facing: f32) {
        self.pos = at;
        self.seated = true;
        self.speed = 0.0;
        self.target_speed = 0.0;
        self.face_target = Some(facing);
    }

    /// Lie down on a bed. Mechanically this is sitting — the Bim is put where
    /// the furniture says and holds still until told otherwise — but saying so
    /// at the call site is worth the three lines.
    pub fn lie(&mut self, at: Vec2, facing: f32) {
        self.sit(at, facing);
    }

    pub fn stand(&mut self) {
        self.seated = false;
    }

    /// Get up and end up standing at `at`, which is how you leave a bed: the
    /// Bim was lying in the middle of it, and the floor beside it is the only
    /// place it can actually stand.
    pub fn stand_at(&mut self, at: Vec2) {
        self.pos = at;
        self.stand();
    }

    pub fn hold_main(&mut self, item: Held) {
        self.main = item;
    }

    pub fn hold_tool(&mut self, item: Held) {
        self.tool = item;
    }

    pub fn main_held(&self) -> Held {
        self.main
    }

    pub fn set_action(&mut self, action: Action) {
        if self.action != action {
            self.action = action;
            self.action_phase = 0.0;
        }
    }

    // --- behaviour ------------------------------------------------------

    fn begin_walk(&mut self, rng: &mut Rng) {
        self.activity = Activity::Walking;
        self.timer = rng.range(1.0, 3.4);
        // Most course changes are small; now and then the Bim changes its mind
        // completely. That mix is what reads as "wandering" rather than "jitter".
        let turn = if rng.chance(REVERSAL_CHANCE) {
            rng.signed() * PI
        } else {
            rng.gaussian() * 0.8
        };
        self.intent = wrap_angle(self.intent + turn);
        self.target_speed = rng.range(34.0, 78.0);
    }

    fn begin_pause(&mut self, rng: &mut Rng) {
        self.activity = Activity::Pausing;
        self.timer = rng.range(0.4, 1.8);
        self.target_speed = 0.0;
    }

    /// How hard the room is pushing the Bim around — walls it is too close to
    /// and furniture it is about to walk into — and which way. `None` once it
    /// is out in open floor.
    fn avoid_push(&self, interior: Rect, solids: &[Rect]) -> Option<(f32, f32)> {
        let margin = clamp(
            interior.width().min(interior.height()) * EDGE_MARGIN_FRAC,
            EDGE_MARGIN_MIN,
            EDGE_MARGIN_MAX,
        );
        let mut push = Vec2::ZERO;
        if self.pos.x < interior.min.x + margin {
            push.x += (interior.min.x + margin - self.pos.x) / margin;
        }
        if self.pos.x > interior.max.x - margin {
            push.x -= (self.pos.x - (interior.max.x - margin)) / margin;
        }
        if self.pos.y < interior.min.y + margin {
            push.y += (interior.min.y + margin - self.pos.y) / margin;
        }
        if self.pos.y > interior.max.y - margin {
            push.y -= (self.pos.y - (interior.max.y - margin)) / margin;
        }

        // Furniture pushes too, so the Bim walks around the table rather than
        // bumping along it.
        for solid in solids {
            let away = self.pos - solid.nearest(self.pos);
            let distance = away.len();
            if distance < AVOID_RANGE {
                let strength = 1.0 - distance / AVOID_RANGE;
                let dir = if distance > 0.01 {
                    away * (1.0 / distance)
                } else {
                    vec2(0.0, 1.0)
                };
                push += dir * (strength * 1.6);
            }
        }

        let inward = push.normalize_or_zero();
        if inward == Vec2::ZERO {
            return None;
        }
        // Ease in, so the turn begins as a suggestion and ends as a decision.
        // A straight linear blend leaves the Bim skimming along the wall.
        let t = clamp(push.len(), 0.0, 1.0);
        Some((t * t * (3.0 - 2.0 * t), inward.angle()))
    }

    /// The Bim's own plans: pick a new leg when the current one runs out, and
    /// turn away from walls and furniture. Returns the heading it wants.
    fn wander(&mut self, dt: f32, interior: Rect, solids: &[Rect], rng: &mut Rng) -> f32 {
        self.timer -= dt;
        if self.timer <= 0.0 {
            match self.activity {
                Activity::Pausing => self.begin_walk(rng),
                Activity::Walking => {
                    if rng.chance(PAUSE_CHANCE) {
                        self.begin_pause(rng)
                    } else {
                        self.begin_walk(rng)
                    }
                }
                Activity::Marching => {}
            }
        }

        match self.avoid_push(interior, solids) {
            None => self.intent,
            Some((strength, inward)) => {
                // Talk the Bim out of its plan as well as its heading. Steering
                // the heading alone would send it straight back at the wall the
                // moment it came clear, and it would hug the edge for ages.
                self.intent = angle_lerp(self.intent, inward, approach(INTENT_RATE * strength, dt));
                angle_lerp(self.intent, inward, strength)
            }
        }
    }

    /// Walk the planned route, waypoint by waypoint. Obstacle steering is
    /// deliberately skipped here: the path was planned around the furniture
    /// already, so steering could only argue with it.
    fn follow_order(&mut self) -> f32 {
        let Some(&target) = self.path.first() else {
            self.activity = Activity::Pausing;
            self.timer = 0.0;
            return self.intent;
        };

        let last_leg = self.path.len() == 1;
        let to = target - self.pos;
        let distance = to.len();

        // Corners are rounded rather than stopped at, so a long route flows
        // instead of stuttering at every turn.
        let reached = if last_leg {
            ARRIVE_RADIUS
        } else {
            WAYPOINT_RADIUS
        };
        if distance <= reached {
            self.path.remove(0);
            if self.path.is_empty() {
                // Arrived. Stand for a beat, then go back to wandering.
                self.activity = Activity::Pausing;
                self.timer = if self.scripted { 0.0 } else { ARRIVE_SETTLE };
                self.target_speed = 0.0;
            }
            return self.intent;
        }

        self.intent = to.angle();
        // Ease off on the final approach so it settles on the spot instead of
        // overshooting and circling back. Intermediate corners keep full speed.
        self.target_speed = if last_leg {
            MARCH_SPEED * clamp(distance / SLOWDOWN_RADIUS, 0.3, 1.0)
        } else {
            MARCH_SPEED
        };
        self.intent
    }

    /// Stand where you are, turning to any direction a task asked for.
    fn hold_still(&mut self) -> f32 {
        self.target_speed = 0.0;
        self.face_target.unwrap_or(self.heading)
    }

    pub fn update(&mut self, dt: f32, interior: Rect, solids: &[Rect], rng: &mut Rng) {
        // A task outranks a player order, which outranks the Bim's own plans.
        let goal = if self.seated {
            self.hold_still()
        } else if self.activity == Activity::Marching {
            self.follow_order()
        } else if self.scripted {
            self.hold_still()
        } else {
            self.wander(dt, interior, solids, rng)
        };
        self.heading = angle_lerp(self.heading, goal, approach(TURN_RATE, dt));

        // Ease the speed so starts and stops have weight.
        let step = ACCEL * dt;
        self.speed += clamp(self.target_speed - self.speed, -step, step);

        if !self.seated {
            self.pos += Vec2::from_angle(self.heading) * (self.speed * dt);
            // Keep clear of the walls, then shove out of anything walked into.
            self.pos = interior.expand(-BODY_MARGIN).nearest(self.pos);
            for solid in solids {
                if let Some(out) = solid.push_out(self.pos, BODY_MARGIN) {
                    self.pos += out;
                }
            }
        }

        self.stride = (self.stride + self.speed * dt * 0.10) % TAU;
        self.idle += dt;
        self.select_pulse = (self.select_pulse + dt * 2.2) % TAU;
        self.action_phase += dt;
    }

    // --- rendering ------------------------------------------------------

    /// Forward reach of each arm, and where a held tool sits, for the current
    /// action. Local frame: `+x` is forward, `+y` is the Bim's right.
    fn pose(&self, swing: f32, moving: f32) -> Pose {
        let p = self.action_phase;
        match self.action {
            Action::None => Pose {
                left: -swing * 5.0 * moving,
                right: swing * 5.0 * moving,
                tool: vec2(15.0, 12.0),
                tool_rot: 0.0,
                reach: 0.0,
            },
            Action::Reach => {
                // Ramp the arms out over a moment rather than snapping straight.
                let r = clamp(p / 0.25, 0.0, 1.0);
                Pose {
                    left: 7.0 * r,
                    right: 7.0 * r,
                    tool: vec2(14.0 + 12.0 * r, 10.0),
                    tool_rot: 0.0,
                    reach: r,
                }
            }
            Action::Chop => {
                // One stroke per CHOP_PERIOD: down fast, back up.
                let t = (p / CHOP_PERIOD) % 1.0;
                let drop = (t * TAU).sin();
                Pose {
                    left: 6.0,
                    right: 12.0 + drop * 5.0,
                    // Offset to the working hand so the blade clears the head.
                    tool: vec2(26.0 + drop * 5.0, 10.0),
                    tool_rot: -0.5 + drop * 0.5,
                    reach: 1.0,
                }
            }
            Action::Serve => {
                // Sweep across from the pot to the plate and back.
                let t = (p / SCOOP_PERIOD) % 1.0;
                let sweep = (t * TAU).sin();
                Pose {
                    left: 4.0,
                    right: 12.0,
                    // Wide enough to visibly travel from the pot to the plate.
                    tool: vec2(29.0, sweep * 19.0),
                    tool_rot: sweep * 0.4,
                    reach: 1.0,
                }
            }
            Action::Wash => {
                // Hands held together out in front, working over each other:
                // one arm forward as the other comes back, in a small circle.
                let turn = p * TAU / SCRUB_PERIOD;
                Pose {
                    left: 9.0 + turn.sin() * 2.5,
                    right: 9.0 - turn.sin() * 2.5,
                    tool: vec2(20.0, 0.0),
                    tool_rot: 0.0,
                    reach: 1.0,
                }
            }
            Action::Sleep => {
                // Arms in at the sides, lifting a little with each breath.
                let breath = (p * TAU / BREATH_PERIOD).sin();
                Pose {
                    left: -3.0 + breath,
                    right: -3.0 + breath,
                    tool: vec2(15.0, 12.0),
                    tool_rot: 0.0,
                    reach: 0.0,
                }
            }
            Action::Eat => {
                // Out to the plate, back to the mouth.
                let t = (p / BITE_PERIOD) % 1.0;
                let near = ((t * TAU).cos() * 0.5 + 0.5).powf(1.4);
                Pose {
                    left: 5.0,
                    right: 8.0 + near * 4.0,
                    tool: vec2(lerp(26.0, 11.0, near), 5.0),
                    tool_rot: near * 0.6,
                    reach: 0.6,
                }
            }
        }
    }

    pub fn draw(&self, list: &mut DrawList) {
        let swing = self.stride.sin();
        let moving = clamp(self.speed / 60.0, 0.0, 1.0);
        // Breathing while still, a light bounce while walking.
        let bob = lerp(
            (self.idle * 1.8).sin() * 0.4,
            (self.stride * 2.0).cos() * 0.8,
            moving,
        );
        // Idle glancing about; suppressed once the Bim is busy or going somewhere.
        let look = if self.action == Action::None {
            (self.idle * 0.9).sin() * 0.30 * (1.0 - moving)
        } else {
            0.0
        };
        let pose = self.pose(swing, moving);

        if self.selected {
            // A ring on the ground under the Bim, breathing gently so it stays
            // legible against the floor.
            let pulse = (1.0 + self.select_pulse.sin() * 0.04) * BODY_SCALE;
            list.circle(self.pos, 40.0 * pulse, ACCENT.alpha(0.10));
            list.ring(self.pos, 40.0 * pulse, 2.5, ACCENT.alpha(0.85));
        }

        // Cast under the body and turned with it, so the halo always fits.
        list.ellipse(
            self.pos + vec2(0.0, 4.5 * BODY_SCALE),
            vec2(28.0, 36.0) * BODY_SCALE,
            self.heading,
            SHADOW,
        );

        let mut b = list.brush(self.pos, self.heading, BODY_SCALE);

        // Boots, under the body: one strides forward as the other trails. A
        // seated Bim tucks them in.
        if !self.seated {
            for side in [-1.0f32, 1.0] {
                let step = swing * 8.0 * side * moving;
                b.ellipse(vec2(step, 7.0 * side), vec2(13.5, 9.0), 0.0, BOOT);
            }
        }

        // Torso: broad across the shoulders, shallow front to back, with a dark
        // rim behind it so the silhouette holds up against any floor colour.
        // Asleep the chest rises and falls; it is the only thing moving, so
        // without it the Bim reads as switched off rather than resting.
        let breath = match self.action {
            Action::Sleep => 1.0 + 0.035 * (self.action_phase * TAU / BREATH_PERIOD).sin(),
            _ => 1.0,
        };
        b.ellipse(Vec2::ZERO, vec2(25.0, 33.0) * breath, 0.0, OUTLINE);
        b.ellipse(Vec2::ZERO, vec2(22.0, 30.0) * breath, 0.0, SHIRT);

        // Arms: swinging while walking, reaching or working otherwise.
        for (side, forward) in [(-1.0f32, pose.left), (1.0f32, pose.right)] {
            let at = vec2(forward - 1.0, 13.5 * side);
            b.ellipse(at, vec2(11.5, 11.5), 0.0, OUTLINE);
            b.ellipse(at, vec2(9.5, 9.5), 0.0, SLEEVE);
        }

        // Head assembly, pivoting about the neck. Seen from above it is mostly
        // hair, with the face and nose showing at the leading edge.
        let pivot = vec2(2.5 + bob * 0.3, 0.0);
        let at = |local: Vec2| pivot + local.rotate(look);

        b.ellipse(at(Vec2::ZERO), vec2(15.5, 15.5), 0.0, OUTLINE);
        b.ellipse(at(Vec2::ZERO), vec2(13.5, 13.5), 0.0, SKIN);
        b.ellipse(at(vec2(-2.0, 0.0)), vec2(11.0, 13.0), look, HAIR);
        b.ellipse(at(vec2(5.6, 0.0)), vec2(4.0, 3.2), look, NOSE);

        self.draw_held(list, pose);

        if self.action == Action::Sleep {
            self.draw_zs(list);
        }
    }

    /// Zs drifting up off a sleeping Bim, each one rising and fading as the
    /// next sets off. Placed in the room rather than on the body, so they go
    /// the same way whichever way the Bim is lying.
    fn draw_zs(&self, list: &mut DrawList) {
        const COUNT: usize = 3;
        let from = self.pos + vec2(52.0, -18.0);
        for i in 0..COUNT {
            let t = (self.action_phase / BREATH_PERIOD + i as f32 / COUNT as f32) % 1.0;
            let at = from + vec2(20.0 * t, -38.0 * t);
            // In and out again, so none of them pops.
            let fade = (t * PI).sin();
            draw_z(list, at, 11.0 + 9.0 * t, SLEEP_Z.alpha(0.85 * fade));
        }
    }

    /// Whatever is in the hands, placed in front of the body.
    fn draw_held(&self, list: &mut DrawList, pose: Pose) {
        let to_world = |local: Vec2| self.pos + (local * BODY_SCALE).rotate(self.heading);

        match self.main {
            Held::Nothing => {}
            Held::Vegetable => {
                let at = to_world(vec2(18.0 + pose.reach * 13.0, -6.0));
                list.ellipse(at, vec2(30.0, 17.0), self.heading, VEG);
                list.ellipse(
                    at - Vec2::from_angle(self.heading) * 13.0,
                    vec2(8.0, 12.0),
                    self.heading,
                    VEG_DARK,
                );
            }
            Held::Slices => {
                // A handful of rounds, cupped in both hands.
                for i in 0..5 {
                    let at = to_world(vec2(
                        16.0 + pose.reach * 13.0 + (i % 2) as f32 * 7.0,
                        -8.0 + i as f32 * 4.0,
                    ));
                    list.circle(at, 9.0, VEG);
                    list.circle(at, 4.0, VEG_DARK);
                }
            }
            Held::Plate(fill) => {
                draw_plate(
                    list,
                    to_world(vec2(23.0 + pose.reach * 5.0, 0.0)),
                    fill,
                    1.0,
                );
            }
            Held::Knife => draw_knife(list, to_world(vec2(20.0, -6.0)), self.heading),
            Held::Spoon => draw_spoon(list, to_world(vec2(20.0, -6.0)), self.heading),
        }

        let tool_at = to_world(pose.tool);
        let tool_rot = self.heading + pose.tool_rot;
        match self.tool {
            Held::Knife => draw_knife(list, tool_at, tool_rot),
            Held::Spoon => draw_spoon(list, tool_at, tool_rot),
            // A plain fork, for eating at the table.
            Held::Slices => {
                list.rect(tool_at, vec2(22.0, 4.0), tool_rot, 2.0, STEEL);
                list.rect(
                    tool_at + Vec2::from_angle(tool_rot) * -10.0,
                    vec2(8.0, 6.0),
                    tool_rot,
                    2.0,
                    GRIP,
                );
            }
            _ => {}
        }
    }
}

/// A letter Z, drawn from the three strokes you would write it with.
fn draw_z(list: &mut DrawList, at: Vec2, size: f32, c: Color) {
    let h = size * 0.5;
    let line = (size * 0.17).max(1.2);
    list.line(at + vec2(-h, -h), at + vec2(h, -h), line, c);
    list.line(at + vec2(h, -h), at + vec2(-h, h), line, c);
    list.line(at + vec2(-h, h), at + vec2(h, h), line, c);
}

/// Arm and tool placement for one frame of an action.
#[derive(Clone, Copy)]
struct Pose {
    /// Forward offset of the left arm, in local pixels.
    left: f32,
    right: f32,
    /// Where a held tool sits, in the local frame.
    tool: Vec2,
    tool_rot: f32,
    /// 0 to 1, how far the arms are extended. Held items ride along with it.
    reach: f32,
}
