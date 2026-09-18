//! The Bim: a top-down character that decides where to go on its own, unless
//! a task is telling it what to do.

use crate::draw::{Color, DrawList};
use crate::math::{PI, Rect, TAU, Vec2, angle_lerp, approach, clamp, lerp, vec2, wrap_angle};
use crate::rng::Rng;
use crate::room::{
    BROOM_HEAD, BROOM_POLE, Dish, GRIP, STEEL, draw_knife, draw_plate, draw_spoon, draw_stew_tub,
};

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

/// The ship's coverall: what every one of the crew wears.
const SHIRT: Color = Color::rgb(0.33, 0.58, 0.85);
const SLEEVE: Color = Color::rgb(0.27, 0.49, 0.74);
/// The station's, worn by everybody who lives there. Far enough from the
/// ship's that who is crew and who is not never has to be told from where
/// they happen to be standing — see [`Uniform`].
const SHIRT_STATION: Color = Color::rgb(0.84, 0.52, 0.24);
const SLEEVE_STATION: Color = Color::rgb(0.70, 0.41, 0.18);
/// The pressure suit, and the visor over the head while it is worn.
const SHIRT_SUIT: Color = Color::rgb(0.86, 0.88, 0.92);
const SLEEVE_SUIT: Color = Color::rgb(0.70, 0.73, 0.79);
const VISOR: Color = Color::rgb(0.55, 0.78, 0.95);
/// The yoke across the shoulders, in the wearer's own colour: the one thing
/// that tells two people in the same coverall apart from directly above.
const TRIM: Color = Color::rgb(0.86, 0.93, 0.96);
const TRIM_B: Color = Color::rgb(0.74, 0.42, 0.62);
const HAIR_B: Color = Color::rgb(0.42, 0.26, 0.13);
const SKIN: Color = Color::rgb(0.91, 0.73, 0.55);
const NOSE: Color = Color::rgb(0.82, 0.62, 0.45);
const HAIR: Color = Color::rgb(0.23, 0.17, 0.12);
const BOOT: Color = Color::rgb(0.24, 0.18, 0.13);
const OUTLINE: Color = Color::rgba(0.05, 0.08, 0.07, 0.55);
const SHADOW: Color = Color::rgba(0.0, 0.0, 0.0, 0.20);
const VEG: Color = Color::rgb(0.44, 0.68, 0.24);
const VEG_DARK: Color = Color::rgb(0.30, 0.50, 0.16);
const TOFU: Color = Color::rgb(0.93, 0.91, 0.82);
const TOFU_EDGE: Color = Color::rgb(0.78, 0.76, 0.66);
/// A crate of materials on the way to a construction site, and the strap
/// round it.
const CRATE: Color = Color::rgb(0.55, 0.47, 0.32);
const CRATE_STRAP: Color = Color::rgb(0.32, 0.27, 0.19);
/// What a Bim that has had an accident is covered in. The same colour as the
/// mess on the deck, so the two read as the same substance.
const GRIME: Color = Color::rgb(0.24, 0.18, 0.09);
/// The Zs that float off a sleeping Bim.
const SLEEP_Z: Color = Color::rgb(0.80, 0.90, 0.95);
/// The same body, with everything warm taken out of it.
const GONE_SHIRT: Color = Color::rgb(0.30, 0.36, 0.42);
const GONE_SLEEVE: Color = Color::rgb(0.25, 0.30, 0.36);
const GONE_SKIN: Color = Color::rgb(0.55, 0.53, 0.50);
const GONE_HAIR: Color = Color::rgb(0.18, 0.17, 0.16);
/// Shared with the trail and the order marker, so everything the player is
/// steering reads as one colour.
pub const ACCENT: Color = Color::rgb(0.50, 0.82, 0.66);
/// A Bim under direct orders. Its own colour, so being recruited reads at a
/// glance and does not have to be told apart from being merely selected.
const COMMAND: Color = Color::rgb(1.0, 0.82, 0.35);
/// An enemy: the ring under one, in the colour its shots are.
const ENEMY: Color = crate::combat::HOSTILE_BOLT;
/// Blood: the blotch on a part with an open wound, and the drops on the
/// deck. Dark, so it reads as blood and not as the enemy's red.
pub const BLOOD: Color = Color::rgb(0.55, 0.05, 0.05);
/// The hand laser, drawn: the body of it, and the emitter at the muzzle in
/// the colour a friendly bolt is.
const GUN: Color = Color::rgb(0.15, 0.17, 0.20);
const GUN_EDGE: Color = Color::rgb(0.42, 0.47, 0.53);
const GUN_LIT: Color = crate::combat::FRIENDLY_BOLT;

/// Whose coverall a Bim is wearing: the ship's or the station's.
///
/// The crew wear one and the people living on a station wear the other, so
/// that with the two rooms joined at the airlock — see `world::docking` — a
/// glance at the deck says who belongs to the ship and who is going ashore
/// when it casts off. Nothing but `draw` reads it; a resident is simulated
/// exactly as a crew member is.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Uniform {
    Crew,
    Station,
    /// A pressure suit, worn for a walk outside and drawn with a visor over
    /// the head. Put on by [`Character::go_outside`] over whichever of the
    /// other two the body wears, and taken off again by
    /// [`Character::come_inside`].
    Suit,
}

impl Uniform {
    fn shirt(self) -> Color {
        match self {
            Uniform::Crew => SHIRT,
            Uniform::Station => SHIRT_STATION,
            Uniform::Suit => SHIRT_SUIT,
        }
    }

    fn sleeve(self) -> Color {
        match self {
            Uniform::Crew => SLEEVE,
            Uniform::Station => SLEEVE_STATION,
            Uniform::Suit => SLEEVE_SUIT,
        }
    }
}

/// Which of the crew this is, as far as the drawing is concerned: the colour
/// of the yoke on the coverall, a hair colour, and whether it is worn long.
///
/// Nothing but `draw` reads it. Two Bims behave identically — that is the
/// point of the second one — so the only thing that distinguishes them in the
/// simulation is the index, and the only thing that distinguishes them on the
/// deck is this. The coverall itself is the [`Uniform`]'s.
#[derive(Clone, Copy, PartialEq)]
pub enum Look {
    /// Pale yoke, cropped hair.
    First,
    /// Mauve yoke, hair down past the collar.
    Second,
}

impl Look {
    /// The index decides it, so a third of the crew would be a third arm here
    /// rather than a change anywhere else.
    pub fn of(who: usize) -> Look {
        if who % 2 == 0 {
            Look::First
        } else {
            Look::Second
        }
    }

    fn trim(self) -> Color {
        match self {
            Look::First => TRIM,
            Look::Second => TRIM_B,
        }
    }

    fn hair(self) -> Color {
        match self {
            Look::First => HAIR,
            Look::Second => HAIR_B,
        }
    }

    /// How far the hair reaches past the head, seen from above. Nothing for
    /// the cropped one; a fall down the back for the other, which is the one
    /// thing that reads as a difference at this scale even in silhouette.
    fn mane(self) -> f32 {
        match self {
            Look::First => 0.0,
            Look::Second => 1.0,
        }
    }
}

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
    /// A block of tofu, which is a vegetable as far as the chain is concerned
    /// and a different shape as far as the eye is.
    Tofu,
    /// What came off the board: rounds of vegetable and cubes of tofu, in
    /// two hands on the way to the pot.
    Chopped {
        rounds: u32,
        cubes: u32,
    },
    /// A plain fork, for eating at the table.
    Fork,
    Knife,
    Spoon,
    /// The broom, out of its locker.
    Broom,
    /// A pick, for a rock outside. In the tool hand, and swung the way the
    /// knife is.
    Pick,
    /// A plate or a bowl, carrying how full it is and which it is.
    Plate(f32, Dish),
    /// A pot of stew in a tub, on its way to the cold store or back from it.
    Stew,
    /// A crate of materials off a shelf, on its way to a construction site.
    /// The room never knows what is in it: the count is the world's.
    Crate,
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
    /// Hopping on the spot: what a Bim that needs the heads does while it
    /// waits, and the only warning the player gets before an accident.
    Fidget,
    /// Both hands on a broom, working it across the deck.
    Sweep,
    /// Doubled over, being sick on the deck.
    Retch,
    /// Talking to the other one: a hand comes up and drops again, the way one
    /// does. The *words* are the host's — no strings cross this boundary — so
    /// all the simulation ever draws is the gesture.
    Talk,
}

/// One turn of the hands under the tap.
const SCRUB_PERIOD: f32 = 0.55;
/// One stroke of the broom across the deck and back.
const SWEEP_PERIOD: f32 = 0.9;

/// One hop on the spot, and one heave.
const HOP_PERIOD: f32 = 0.42;
const HEAVE_PERIOD: f32 = 0.75;
/// One rise and fall of the hand while talking.
const TALK_PERIOD: f32 = 1.1;

/// How long one breath takes while asleep, in seconds.
const BREATH_PERIOD: f32 = 5.4;

pub struct Character {
    pub pos: Vec2,
    /// Which of the crew this is to look at. Read by `draw` and nothing else.
    look: Look,
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

    /// How fast it walks, as a fraction of its usual pace. Hunger slows it.
    pace: f32,
    /// Once dead it does nothing at all, and is drawn where it fell.
    dead: bool,
    /// Dropped off standing up. Holds still, keeps whatever it was carrying
    /// and whatever route it was on, and picks both up again on waking.
    napping: bool,
    /// Under direct orders: it stands where it is put rather than pottering
    /// about, and nothing starts of its own accord.
    recruited: bool,
    /// Where it has been told to stand and stay — the helm, the far side of
    /// an airlock. It holds still there rather than pottering about, goes
    /// off on its errands as usual, and walks back afterwards; only a fresh
    /// order or the room letting it go takes the post away. See
    /// `Game::send_to`.
    post: Option<Vec2>,
    /// Whose coverall it wears. Drawing only.
    uniform: Uniform,
    /// Outside the hull, in a suit. Mechanically this is sitting — put
    /// somewhere by a chain and held there — at a spot beyond the skin,
    /// and `worn` is the coverall to go back into. Read by the world to
    /// dose the body; see `Game::is_outside`.
    outside: bool,
    worn: Uniform,
    /// How filthy the Bim itself is, 0 clean to 1 covered. Kept here rather
    /// than with the deck's own mess because this is the share that walks
    /// away with it.
    filth: f32,
    /// Seconds left of a hop or a heave. Both are short and both end by
    /// themselves, so nothing else has to remember to stop them.
    antic: f32,
    /// Weapon drawn: in combat mode, with something to draw. Drawing only —
    /// `Game::tick_combat` sets it every step from the gear and the orders.
    armed: bool,
    /// An enemy, to whoever is looking: ringed in red under the body.
    /// Drawing only; the world says who is.
    hostile: bool,
    /// Out cold for want of blood: lying where it dropped, alive, doing
    /// nothing until it comes round. Set by `Game::tick_bim` off the
    /// health, the way napping is set off drowsiness; unlike a nap it
    /// drops the route and stands the body up out of whatever it sat in.
    unconscious: bool,
    /// Which parts have an open wound — head, body, legs — for the blotch
    /// drawn on each. Drawing only; `Game::wound` and the bandage set it.
    wounds: [bool; 3],
}

impl Character {
    pub fn new(pos: Vec2, look: Look, rng: &mut Rng) -> Character {
        let heading = rng.range(0.0, TAU);
        let mut c = Character {
            pos,
            look,
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
            pace: 1.0,
            dead: false,
            napping: false,
            recruited: false,
            post: None,
            uniform: Uniform::Crew,
            outside: false,
            worn: Uniform::Crew,
            filth: 0.0,
            antic: 0.0,
            armed: false,
            hostile: false,
            unconscious: false,
            wounds: [false; 3],
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

    /// Move the route the Bim is on, waypoint by waypoint, for a body
    /// carried into another room whose origin is `shift` away — see
    /// `Game::adopt`. A route left in the old room's coordinates is a walk
    /// to somewhere that is not there.
    pub fn shift_route(&mut self, shift: Vec2) {
        for p in &mut self.path {
            *p += shift;
        }
    }

    /// Where the route the Bim is on ends, or `None` if it is not on one.
    ///
    /// A route is planned once and not replanned, so anything that would put
    /// something in the Bim's way — a door shutting itself, say — has to know
    /// where it was going before it does.
    /// The route as it stands, for the probes.
    #[allow(dead_code)]
    pub fn path_for_probe(&self) -> &[Vec2] {
        &self.path
    }

    pub fn destination(&self) -> Option<Vec2> {
        self.path.last().copied()
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

    /// Out through the airlock: standing at `at`, beyond the hull, in the
    /// suit, and from there walking the outside — `Game` hands the body the
    /// outside's grid and solids while it is out, so nothing shoves it back
    /// onto the deck; see `Game::tick_bim`. `pick` is whether it takes the
    /// pick out with it: a walk to mine does, a walk to build does not.
    pub fn go_outside(&mut self, at: Vec2, facing: f32, pick: bool) {
        if !self.outside {
            self.worn = self.uniform;
        }
        self.outside = true;
        self.uniform = Uniform::Suit;
        if pick {
            self.tool = Held::Pick;
        }
        self.stand_at(at);
        self.face(facing);
        self.set_action(Action::Reach);
    }

    /// Back in, standing at `at` — the deck inside the port — in the
    /// coverall that was under the suit. Safe to call on a body that is
    /// not outside; it then does nothing.
    pub fn come_inside(&mut self, at: Vec2) {
        if !self.outside {
            return;
        }
        self.outside = false;
        self.uniform = self.worn;
        // The pick goes with the suit.
        if self.tool == Held::Pick {
            self.tool = Held::Nothing;
        }
        self.stand_at(at);
        self.set_action(Action::None);
    }

    pub fn is_outside(&self) -> bool {
        self.outside
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

    pub fn tool_held(&self) -> Held {
        self.tool
    }

    /// Where the Bim is sitting or lying and which way it faces, if it is on
    /// anything at all. Saved when a task is put down, because standing up is
    /// part of letting go of one.
    pub fn seat(&self) -> Option<(Vec2, f32)> {
        if self.seated {
            Some((self.pos, self.face_target.unwrap_or(self.heading)))
        } else {
            None
        }
    }

    /// How fast it walks, as a fraction of its usual pace.
    pub fn set_pace(&mut self, pace: f32) {
        self.pace = pace;
    }

    /// Drop off on the spot, or come round again. Unlike dying or being sent
    /// to bed this keeps the path and the hands exactly as they were, so the
    /// Bim carries on with whatever it was doing.
    pub fn nod_off(&mut self, napping: bool) {
        self.napping = napping;
        self.speed = 0.0;
        self.target_speed = 0.0;
        self.set_action(if napping { Action::Sleep } else { Action::None });
    }

    pub fn is_napping(&self) -> bool {
        self.napping
    }

    pub fn set_recruited(&mut self, recruited: bool) {
        self.recruited = recruited;
    }

    pub fn is_recruited(&self) -> bool {
        self.recruited
    }

    pub fn set_post(&mut self, post: Option<Vec2>) {
        self.post = post;
    }

    pub fn post(&self) -> Option<Vec2> {
        self.post
    }

    pub fn set_uniform(&mut self, uniform: Uniform) {
        self.uniform = uniform;
    }

    pub fn uniform(&self) -> Uniform {
        self.uniform
    }

    pub fn set_armed(&mut self, armed: bool) {
        self.armed = armed;
    }

    pub fn is_armed(&self) -> bool {
        self.armed
    }

    pub fn set_hostile(&mut self, hostile: bool) {
        self.hostile = hostile;
    }

    /// Out cold, or come round. Going out drops the walk — whatever it
    /// was doing with its hands is the chain's to put down, and `Game`
    /// interrupts the errand first — and it lies where it stood: not
    /// seated, so nothing holds it in a bunk it has fallen out of.
    pub fn knock_out(&mut self, out: bool) {
        self.unconscious = out;
        if out {
            self.path.clear();
            self.activity = Activity::Pausing;
            self.speed = 0.0;
            self.target_speed = 0.0;
            self.seated = false;
            self.antic = 0.0;
            self.set_action(Action::None);
        }
    }

    pub fn is_unconscious(&self) -> bool {
        self.unconscious
    }

    /// Which parts bleed — head, body, legs — for the blotch drawn on each.
    pub fn set_wounds(&mut self, wounds: [bool; 3]) {
        self.wounds = wounds;
    }

    /// It stops where it stands, and stays there.
    pub fn die(&mut self) {
        self.dead = true;
        self.path.clear();
        self.speed = 0.0;
        self.target_speed = 0.0;
        self.seated = false;
        self.main = Held::Nothing;
        self.tool = Held::Nothing;
        self.action = Action::None;
    }

    pub fn is_dead(&self) -> bool {
        self.dead
    }

    /// Sitting or lying: put there by a chain, and not to be shoved about by
    /// anything outside it.
    pub fn is_seated(&self) -> bool {
        self.seated
    }

    pub fn set_action(&mut self, action: Action) {
        if self.action != action {
            self.action = action;
            self.action_phase = 0.0;
        }
    }

    // --- mess -------------------------------------------------------------

    /// How filthy the Bim itself is, 0 to 1.
    pub fn filth(&self) -> f32 {
        self.filth
    }

    /// Cover it in something. Takes the worse of what it has on it already and
    /// what it has just picked up: an accident cannot make a Bim cleaner.
    pub fn soil(&mut self, amount: f32) {
        self.filth = self.filth.max(amount).clamp(0.0, 1.0);
    }

    /// Get some of it off again. Washing at the basin is the only thing that
    /// does this so far, and it is worth what a basin is worth.
    pub fn wash(&mut self, amount: f32) {
        self.filth = (self.filth - amount).max(0.0);
    }

    /// Hop about on the spot for a moment, or be sick. Neither disturbs what
    /// the Bim was doing — they are things that happen *to* it — so both are
    /// refused outright while it is in the middle of a scripted step, where
    /// the pose belongs to the chain.
    pub fn antic(&mut self, action: Action, seconds: f32) {
        if self.scripted || self.seated || self.dead || self.napping || self.unconscious {
            return;
        }
        self.set_action(action);
        self.antic = seconds;
    }

    /// Whether one is running, so the game does not start another over it.
    pub fn in_antic(&self) -> bool {
        self.antic > 0.0
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
        self.target_speed = self.pace * rng.range(34.0, 78.0);
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
        self.target_speed = self.pace
            * if last_leg {
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
        if self.dead || self.napping || self.unconscious {
            // Nothing moves, but the clock still runs so the shadow, the
            // breathing and the selection ring do not freeze mid-pulse.
            self.idle += dt;
            self.select_pulse = (self.select_pulse + dt * 2.2) % TAU;
            self.action_phase += dt;
            return;
        }
        // A task outranks a player order, which outranks the Bim's own plans.
        let goal = if self.seated {
            self.hold_still()
        } else if self.activity == Activity::Marching {
            self.follow_order()
        } else if self.scripted || self.recruited || self.post.is_some() {
            // Recruited, or posted somewhere, it waits to be told. The wander
            // is the one thing it does unprompted, so that is the one thing
            // being under orders takes away.
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

        // A hop or a heave runs itself down and puts the Bim back to standing.
        if self.antic > 0.0 {
            self.antic -= dt;
            if self.antic <= 0.0 {
                self.antic = 0.0;
                if matches!(self.action, Action::Fidget | Action::Retch) {
                    self.set_action(Action::None);
                }
            }
        }
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
            Action::Sweep => {
                // Both hands on the pole, the head of the broom travelling
                // side to side across the deck in front. Seen from above that
                // is the arms swinging together rather than alternately, which
                // is what tells it apart from a walk.
                let swing = (p * TAU / SWEEP_PERIOD).sin();
                Pose {
                    left: 7.0 + swing * 2.0,
                    right: 7.0 - swing * 2.0,
                    tool: vec2(26.0, swing * 16.0),
                    tool_rot: swing * 0.5,
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
            Action::Fidget => {
                // Arms tucked in and swapping, the way you do.
                let turn = (p * TAU / HOP_PERIOD).sin();
                Pose {
                    left: 2.0 + turn * 3.0,
                    right: 2.0 - turn * 3.0,
                    tool: vec2(15.0, 12.0),
                    tool_rot: 0.0,
                    reach: 0.0,
                }
            }
            Action::Retch => {
                // Both arms forward and down, hands on the knees.
                let heave = (p * TAU / HEAVE_PERIOD).sin().abs();
                Pose {
                    left: 6.0 + heave * 3.0,
                    right: 6.0 + heave * 3.0,
                    tool: vec2(18.0, 8.0),
                    tool_rot: 0.0,
                    reach: 0.8,
                }
            }
            Action::Talk => {
                // One hand up and down, the other still. Small: it is a
                // conversation, not a semaphore.
                let wave = (p * TAU / TALK_PERIOD).sin();
                Pose {
                    left: 3.0 + wave * 2.5,
                    right: 1.0,
                    tool: vec2(15.0, 12.0),
                    tool_rot: 0.0,
                    reach: 0.25 + 0.15 * wave.max(0.0),
                }
            }
        }
    }

    pub fn draw(&self, list: &mut DrawList) {
        if self.dead {
            self.draw_fallen(list);
            return;
        }
        self.draw_rings(list);
        if self.unconscious {
            self.draw_lying(list);
            return;
        }
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

        // Hopping on the spot, or doubled over. Seen from above a jump is the
        // figure growing and its shadow shrinking away underneath it, and a
        // heave is the reverse: hunched down and small.
        let (hop, heave) = match self.action {
            Action::Fidget => (
                (self.action_phase * TAU / HOP_PERIOD).sin().max(0.0),
                0.0f32,
            ),
            Action::Retch => (0.0, (self.action_phase * TAU / HEAVE_PERIOD).sin().abs()),
            _ => (0.0, 0.0),
        };
        let lift = 1.0 + 0.16 * hop - 0.10 * heave;
        let scale = BODY_SCALE * lift;

        // Cast under the body and turned with it, so the halo always fits.
        // It pulls in and darkens as the Bim leaves the deck.
        list.ellipse(
            self.pos + vec2(0.0, (4.5 + 7.0 * hop) * BODY_SCALE),
            vec2(28.0, 36.0) * BODY_SCALE * (1.0 - 0.22 * hop),
            self.heading,
            SHADOW,
        );

        let mut b = list.brush(self.pos, self.heading, scale);

        // Boots, under the body: one strides forward as the other trails. A
        // seated Bim tucks them in.
        if !self.seated {
            for side in [-1.0f32, 1.0] {
                let step = swing * 8.0 * side * moving;
                b.ellipse(vec2(step, 7.0 * side), vec2(13.5, 9.0), 0.0, BOOT);
                // A wounded leg bleeds onto the boot.
                if self.wounds[2] {
                    b.ellipse(vec2(step - 2.0, 7.0 * side), vec2(8.0, 6.0), 0.0, BLOOD);
                }
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
        b.ellipse(
            Vec2::ZERO,
            vec2(22.0, 30.0) * breath,
            0.0,
            self.uniform.shirt(),
        );
        // The yoke across the shoulders, in the wearer's own colour — the
        // coverall is the ship's or the station's and says nothing about
        // who is in it.
        b.ellipse(
            vec2(-6.5, 0.0),
            vec2(7.0, 24.0) * breath,
            0.0,
            self.look.trim(),
        );

        // A wound on the body: a blotch in the middle of the coverall.
        if self.wounds[1] {
            b.ellipse(vec2(1.0, 0.0), vec2(11.0, 9.0), 0.3, BLOOD);
        }

        // What it has got on itself. Down the front and around the legs, where
        // it would be, and in the same colour as the mess on the deck so the
        // two read as the same substance.
        if self.filth > 0.001 {
            let deep = self.filth.clamp(0.0, 1.0);
            for (i, local) in [
                vec2(-4.0, 5.0),
                vec2(2.0, -6.5),
                vec2(-8.0, -3.0),
                vec2(6.0, 4.0),
            ]
            .into_iter()
            .enumerate()
            {
                // The worse it is, the more of the four show.
                if (i as f32 + 1.0) / 4.0 > deep + 0.25 {
                    continue;
                }
                let size = 7.0 + 4.0 * deep;
                b.ellipse(
                    local,
                    vec2(size, size * 0.85),
                    0.0,
                    GRIME.alpha(0.55 + 0.4 * deep),
                );
            }
        }

        // Arms: swinging while walking, reaching or working otherwise.
        for (side, forward) in [(-1.0f32, pose.left), (1.0f32, pose.right)] {
            let at = vec2(forward - 1.0, 13.5 * side);
            b.ellipse(at, vec2(11.5, 11.5), 0.0, OUTLINE);
            b.ellipse(at, vec2(9.5, 9.5), 0.0, self.uniform.sleeve());
        }

        // Head assembly, pivoting about the neck. Seen from above it is mostly
        // hair, with the face and nose showing at the leading edge.
        let pivot = vec2(2.5 + bob * 0.3, 0.0);
        let at = |local: Vec2| pivot + local.rotate(look);

        b.ellipse(at(Vec2::ZERO), vec2(15.5, 15.5), 0.0, OUTLINE);
        b.ellipse(at(Vec2::ZERO), vec2(13.5, 13.5), 0.0, SKIN);
        // Hair worn long falls back over the shoulders, which from directly
        // above is a second ellipse behind the head. Drawn before the crown so
        // the crown sits on top of it rather than the fall sitting on the face.
        let mane = self.look.mane();
        if mane > 0.0 {
            b.ellipse(
                at(vec2(-7.5 * mane, 0.0)),
                vec2(14.0, 17.5) * mane,
                look,
                self.look.hair(),
            );
        }
        b.ellipse(
            at(vec2(-2.0, 0.0)),
            vec2(11.0, 13.0),
            look,
            self.look.hair(),
        );
        b.ellipse(at(vec2(5.6, 0.0)), vec2(4.0, 3.2), look, NOSE);
        // A wound on the head: a blotch over the crown.
        if self.wounds[0] {
            b.ellipse(at(vec2(-1.0, 2.0)), vec2(7.0, 6.0), look, BLOOD);
        }
        // The visor over all of that, in the suit: a helmet from above is
        // a bigger circle than the head, and the face shows through it.
        if self.uniform == Uniform::Suit {
            b.ellipse(at(Vec2::ZERO), vec2(18.0, 18.0), 0.0, OUTLINE);
            b.ellipse(at(Vec2::ZERO), vec2(16.5, 16.5), 0.0, VISOR.alpha(0.55));
        }

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

    /// The rings on the deck under a living Bim: selected, an enemy, under
    /// orders. Drawn before the body, standing or lying.
    fn draw_rings(&self, list: &mut DrawList) {
        if self.selected {
            // A ring on the ground under the Bim, breathing gently so it stays
            // legible against the floor.
            let pulse = (1.0 + self.select_pulse.sin() * 0.04) * BODY_SCALE;
            list.circle(self.pos, 40.0 * pulse, ACCENT.alpha(0.10));
            list.ring(self.pos, 40.0 * pulse, 2.5, ACCENT.alpha(0.85));
        }

        if self.hostile {
            // An enemy is ringed in the colour its shots are, thin and
            // steady: a warning, not a selection.
            list.ring(self.pos, 46.0 * BODY_SCALE, 2.0, ENEMY.alpha(0.75));
        }

        if self.recruited {
            // A wider ring outside the selection one, broken into four arcs so
            // the two never read as the same thing. Shown whether or not the
            // Bim is selected: being under orders outlasts a click elsewhere.
            let pulse = (1.0 + self.select_pulse.sin() * 0.05) * BODY_SCALE;
            let span = 52.0 * pulse;
            list.ring(self.pos, span, 1.5, COMMAND.alpha(0.35));
            for i in 0..4 {
                let a = i as f32 * (TAU / 4.0) + self.select_pulse * 0.25;
                let at = self.pos + Vec2::from_angle(a) * (span * 0.5);
                list.rect(at, vec2(9.0, 3.0), a + PI * 0.5, 1.5, COMMAND.alpha(0.9));
            }
        }
    }

    /// Face down where it dropped. Drawn cold and flat — no bob, no breath,
    /// no glancing about — because every other state has one of those, and
    /// the absence is what reads as dead.
    fn draw_fallen(&self, list: &mut DrawList) {
        self.draw_flat(
            list,
            BODY_SCALE,
            (GONE_SHIRT, GONE_SLEEVE, GONE_SKIN, GONE_HAIR),
        );
    }

    /// Out cold: the same figure as a fallen one, in its own colours, and
    /// breathing — a slow swell of the whole body, since from above a chest
    /// rising is the outline growing. No Zs: this is not sleep. The breath
    /// is what says alive; the blotches say why it is down.
    fn draw_lying(&self, list: &mut DrawList) {
        let breath = 1.0 + 0.03 * (self.idle * TAU / BREATH_PERIOD).sin();
        self.draw_flat(
            list,
            BODY_SCALE * breath,
            (
                self.uniform.shirt(),
                self.uniform.sleeve(),
                SKIN,
                self.look.hair(),
            ),
        );
    }

    /// The figure lying flat, in the given shirt, sleeve, skin and hair,
    /// with the blood on whichever parts bleed.
    fn draw_flat(&self, list: &mut DrawList, scale: f32, colours: (Color, Color, Color, Color)) {
        let (shirt, sleeve, skin, hair) = colours;
        list.ellipse(
            self.pos + vec2(3.0, 5.0),
            vec2(34.0, 40.0) * BODY_SCALE,
            self.heading,
            SHADOW,
        );
        let mut b = list.brush(self.pos, self.heading, scale);
        b.ellipse(Vec2::ZERO, vec2(26.0, 34.0), 0.0, OUTLINE);
        b.ellipse(Vec2::ZERO, vec2(22.0, 30.0), 0.0, shirt);
        if self.wounds[1] {
            b.ellipse(vec2(-1.0, -2.0), vec2(11.0, 9.0), 0.3, BLOOD);
        }
        for side in [-1.0f32, 1.0] {
            b.ellipse(vec2(-4.0, 14.0 * side), vec2(10.0, 10.0), 0.0, sleeve);
            // The boots trail behind the body, and a wounded leg bleeds
            // onto them.
            if self.wounds[2] {
                b.ellipse(vec2(-13.0, 7.0 * side), vec2(8.0, 6.0), 0.0, BLOOD);
            }
        }
        // Head turned aside, face down.
        b.ellipse(vec2(3.0, 4.0), vec2(15.5, 15.5), 0.0, OUTLINE);
        b.ellipse(vec2(3.0, 4.0), vec2(13.0, 13.0), 0.0, skin);
        b.ellipse(vec2(1.0, 4.0), vec2(11.0, 12.5), 0.4, hair);
        if self.wounds[0] {
            b.ellipse(vec2(2.0, 5.0), vec2(7.0, 6.0), 0.4, BLOOD);
        }
    }

    /// Whatever is in the hands, placed in front of the body.
    fn draw_held(&self, list: &mut DrawList, pose: Pose) {
        let to_world = |local: Vec2| self.pos + (local * BODY_SCALE).rotate(self.heading);

        // The weapon, drawn: held out in the right hand, the barrel along
        // the way the body faces and the emitter lit at the end of it.
        if self.armed && !self.dead {
            let hand = vec2(pose.right + 8.0, 12.5);
            let barrel = to_world(hand + vec2(9.0, 0.0));
            list.rect(barrel, vec2(22.0, 6.0), self.heading, 1.5, GUN_EDGE);
            list.rect(barrel, vec2(20.0, 4.0), self.heading, 1.0, GUN);
            list.rect(
                to_world(hand + vec2(1.0, 2.5)),
                vec2(6.0, 9.0),
                self.heading,
                1.0,
                GUN,
            );
            list.circle(to_world(hand + vec2(19.5, 0.0)), 4.0, GUN_LIT.alpha(0.9));
        }

        match self.main {
            // The tools are drawn from the tool hand below; the main hand
            // never holds one.
            Held::Nothing | Held::Fork | Held::Pick => {}
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
            Held::Chopped { rounds, cubes } => {
                // A handful of what came off the board, cupped in both hands:
                // rounds and cubes in turn, as many as will show.
                let mut left = (rounds, cubes);
                for i in 0..(rounds + cubes).min(6) {
                    let cube = match left {
                        (0, _) => true,
                        (_, 0) => false,
                        _ => i % 2 == 1,
                    };
                    if cube {
                        left.1 -= 1;
                    } else {
                        left.0 -= 1;
                    }
                    let at = to_world(vec2(
                        16.0 + pose.reach * 13.0 + (i % 2) as f32 * 7.0,
                        -9.0 + i as f32 * 3.6,
                    ));
                    if cube {
                        list.rect(at, vec2(8.0, 8.0), self.heading, 2.0, TOFU);
                        list.stroke_rect(at, vec2(8.0, 8.0), self.heading, 2.0, 1.0, TOFU_EDGE);
                    } else {
                        list.circle(at, 9.0, VEG);
                        list.circle(at, 4.0, VEG_DARK);
                    }
                }
            }
            Held::Tofu => {
                let at = to_world(vec2(18.0 + pose.reach * 13.0, -6.0));
                list.rect(at, vec2(28.0, 20.0), self.heading, 3.0, TOFU);
                list.stroke_rect(at, vec2(28.0, 20.0), self.heading, 3.0, 1.5, TOFU_EDGE);
            }
            Held::Plate(fill, dish) => {
                draw_plate(
                    list,
                    to_world(vec2(23.0 + pose.reach * 5.0, 0.0)),
                    fill,
                    1.0,
                    dish,
                );
            }
            Held::Knife => draw_knife(list, to_world(vec2(20.0, -6.0)), self.heading),
            Held::Spoon => draw_spoon(list, to_world(vec2(20.0, -6.0)), self.heading),
            Held::Stew => draw_stew_tub(
                list,
                to_world(vec2(20.0 + pose.reach * 8.0, 0.0)),
                self.heading,
            ),
            // A crate in both arms, square to the body, with a strap
            // across it so it reads as a box and not a plate.
            Held::Crate => {
                let at = to_world(vec2(21.0 + pose.reach * 8.0, 0.0));
                list.rect(at, vec2(26.0, 30.0), self.heading, 2.0, CRATE);
                list.stroke_rect(at, vec2(26.0, 30.0), self.heading, 2.0, 1.5, CRATE_STRAP);
                list.rect(at, vec2(26.0, 5.0), self.heading, 0.0, CRATE_STRAP);
            }
            Held::Broom => {
                // Held out in front and across, the way anyone carries one:
                // the pole running away from the body and the head on the
                // deck at the far end of it, swinging with the pose.
                let across = pose.tool.y * 0.5;
                let grip = to_world(vec2(13.0, across * 0.3));
                let head = to_world(vec2(36.0, across));
                list.line(grip, head, 4.0, BROOM_POLE);
                list.rect(
                    head,
                    vec2(9.0, 30.0),
                    self.heading + pose.tool_rot,
                    2.0,
                    BROOM_HEAD,
                );
                // Bristles, splayed the way the stroke is going.
                list.rect(
                    head + Vec2::from_angle(self.heading) * 5.0,
                    vec2(4.0, 26.0),
                    self.heading + pose.tool_rot,
                    1.5,
                    BROOM_POLE.alpha(0.55),
                );
            }
        }

        let tool_at = to_world(pose.tool);
        let tool_rot = self.heading + pose.tool_rot;
        match self.tool {
            Held::Knife => draw_knife(list, tool_at, tool_rot),
            Held::Spoon => draw_spoon(list, tool_at, tool_rot),
            Held::Pick => draw_pick(list, tool_at, tool_rot),
            Held::Fork => {
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

/// A pick: a handle along `rot` with the head across its end, the point
/// forward.
fn draw_pick(list: &mut DrawList, at: Vec2, rot: f32) {
    let dir = Vec2::from_angle(rot);
    list.rect(at - dir * 4.0, vec2(30.0, 4.0), rot, 2.0, BROOM_POLE);
    list.rect(at + dir * 12.0, vec2(6.0, 22.0), rot, 2.0, STEEL);
    list.rect(at + dir * 16.0, vec2(6.0, 8.0), rot, 1.5, STEEL);
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
