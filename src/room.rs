//! The kitchen: its fixed layout, the state of everything in it that moves,
//! and how all of that is drawn.
//!
//! The room is a fixed size in world units and the view is scaled to fit the
//! canvas around it (see `Game::resize`). That keeps the furniture at honest
//! proportions instead of stretching a table when someone widens the window.

use crate::bath::Bath;
use crate::clock::MINUTES_PER_SECOND;
use crate::dish::Dishwasher;
use crate::draw::{Color, DrawList};
use crate::hydro::Bay;
use crate::math::{PI, Rect, TAU, Vec2, clamp, lerp, vec2};

pub const ROOM_W: f32 = 860.0;
pub const ROOM_H: f32 = 580.0;

const WALL: f32 = 18.0;
/// Depth of the counter run against the top wall.
const COUNTER_D: f32 = 58.0;
/// How far out from the counter front the Bim stands to work. Has to clear
/// `BODY_MARGIN` or the collision push-out fights the station, but any more
/// than this and the worktop is out of arm's reach.
const STAND_OFF: f32 = 26.0;

const TILE: f32 = 52.0;

// --- palette ------------------------------------------------------------
//
// One ship's palette: a dark deck, grey composite panels, brushed steel, and a
// cyan running light that everything powered picks up. Warm colours are kept
// for heat and for anything wrong, so those read at a glance against all the
// blue-grey. Food keeps its own colours — it is the one thing aboard that
// should not look fabricated.

/// Outside the deck plating, and the recesses things retract into.
pub const HULL: Color = Color::rgb(0.05, 0.06, 0.08);
pub const DECK: Color = Color::rgb(0.13, 0.15, 0.18);
pub const DECK_SEAM: Color = Color::rgba(0.55, 0.85, 0.95, 0.055);
/// Composite panelling, in the three shades everything built out of it uses.
pub const PANEL: Color = Color::rgb(0.19, 0.22, 0.26);
pub const PANEL_LIT: Color = Color::rgb(0.28, 0.32, 0.37);
pub const PANEL_EDGE: Color = Color::rgb(0.40, 0.46, 0.53);
/// The running light. `GLOW_DIM` is the same colour at strip brightness.
pub const GLOW: Color = Color::rgb(0.38, 0.86, 0.95);
pub const GLOW_DIM: Color = Color::rgba(0.38, 0.86, 0.95, 0.28);
/// Hot, locked, or otherwise worth noticing.
pub const WARN: Color = Color::rgb(0.98, 0.45, 0.32);

const FLOOR: Color = DECK;
const TILE_LINE: Color = DECK_SEAM;
const WALL_C: Color = HULL;
const WALL_TRIM: Color = Color::rgb(0.22, 0.26, 0.31);

const CAB: Color = PANEL;
const WORKTOP: Color = Color::rgb(0.52, 0.57, 0.63);
const WORKTOP_EDGE: Color = Color::rgb(0.33, 0.38, 0.44);
const DRAWER_FACE: Color = PANEL_LIT;
const HANDLE: Color = Color::rgb(0.66, 0.72, 0.78);

const FRIDGE: Color = Color::rgb(0.56, 0.63, 0.70);
const FRIDGE_DOOR: Color = Color::rgb(0.69, 0.76, 0.82);
const FRIDGE_IN: Color = Color::rgb(0.09, 0.17, 0.21);
const SHELF: Color = Color::rgb(0.40, 0.50, 0.57);

const STOVE_TOP: Color = Color::rgb(0.09, 0.11, 0.13);
const BURNER: Color = Color::rgb(0.16, 0.19, 0.22);
const BURNER_HOT: Color = Color::rgb(1.0, 0.45, 0.16);
const KNOB: Color = Color::rgb(0.48, 0.54, 0.60);
const KNOB_ON: Color = WARN;

const POT: Color = Color::rgb(0.38, 0.43, 0.48);
const POT_RIM: Color = Color::rgb(0.56, 0.63, 0.69);
const BROTH_RAW: Color = Color::rgb(0.45, 0.62, 0.28);
const BROTH_DONE: Color = Color::rgb(0.80, 0.50, 0.18);
const STEAM: Color = Color::rgba(1.0, 1.0, 1.0, 0.16);

const TABLE: Color = PANEL_LIT;
const TABLE_EDGE: Color = PANEL;
const CHAIR: Color = Color::rgb(0.24, 0.28, 0.33);

const BOARD: Color = Color::rgb(0.22, 0.26, 0.31);
const VEG: Color = Color::rgb(0.44, 0.68, 0.24);
const VEG_DARK: Color = Color::rgb(0.30, 0.50, 0.16);
const PLATE: Color = Color::rgb(0.86, 0.89, 0.91);
const PLATE_RIM: Color = Color::rgb(0.64, 0.70, 0.75);
const TOFU: Color = Color::rgb(0.93, 0.91, 0.82);
const TOFU_EDGE: Color = Color::rgb(0.78, 0.76, 0.66);
const SALAD: Color = Color::rgb(0.36, 0.62, 0.30);

const BUNK_FRAME: Color = PANEL;
const BUNK_LOWER: Color = Color::rgb(0.13, 0.15, 0.18);
const BUNK_POST: Color = PANEL_EDGE;
const BUNK_RAIL: Color = Color::rgb(0.46, 0.53, 0.60);
const BUNK_SHADOW: Color = Color::rgba(0.0, 0.0, 0.0, 0.34);
const MATTRESS: Color = Color::rgb(0.72, 0.75, 0.79);
const MATTRESS_LOW: Color = Color::rgb(0.40, 0.44, 0.49);
const PILLOW: Color = Color::rgb(0.88, 0.91, 0.94);
const BLANKET: Color = Color::rgb(0.19, 0.33, 0.47);
const BLANKET_FOLD: Color = Color::rgb(0.31, 0.51, 0.66);
const BLANKET_LOW: Color = Color::rgb(0.15, 0.25, 0.37);

pub const STEEL: Color = Color::rgb(0.78, 0.83, 0.87);
pub const GRIP: Color = Color::rgb(0.12, 0.14, 0.17);

/// What is on a plate. A bowl is tofu and salad, uncooked; a stew has been in
/// the pot. They are drawn from the same shapes in different colours, which is
/// as much difference as a plate seen from above can carry.
#[derive(Clone, Copy, PartialEq)]
pub enum Dish {
    Stew,
    Bowl,
}

/// What the cold store starts with, counted separately because the two are
/// not interchangeable: a stew is two vegetables, a bowl is a block of tofu
/// with a salad beside it. Two to one, which is the ratio the Bim eats at and
/// the ratio the manager's food units divide into.
pub const START_VEG: u32 = 20;
pub const START_TOFU: u32 = 10;
/// How full the shelves are drawn as, at the most.
const SHELF_FULL: f32 = (START_VEG + START_TOFU) as f32;

/// Something the Bim can walk up to and work with its hands.
///
/// Nothing aboard is remote-controlled: asking for any of these starts an
/// errand that walks the Bim over, and the state only changes at the moment
/// its hand arrives. That is why each one is a place as well as an effect.
#[derive(Clone, Copy, PartialEq)]
pub enum Switch {
    Hob,
    FridgeDoor,
    /// Wanted open, and wanted locked. These two carry what was asked for
    /// rather than being read off the door when the hand arrives, because the
    /// act of asking can change the door itself: dropping a trip to the heads
    /// unlocks and opens the door the Bim locked behind itself, so a toggle
    /// would find the lock already off and put it straight back on. A Bim
    /// asked to unlock while sitting on the pan locked itself back in.
    BathDoor(bool),
    BathLock(bool),
    Dishwasher,
}

/// Fixtures a click can land on.
pub const HIT_NONE: u32 = 0;
pub const HIT_FRIDGE: u32 = 1;
pub const HIT_STOVE: u32 = 2;
pub const HIT_BED: u32 = 3;
pub const HIT_TOILET: u32 = 4;
pub const HIT_DOOR: u32 = 5;
pub const HIT_DISHWASHER: u32 = 6;
pub const HIT_HYDRO: u32 = 7;

/// How far the lower bunk sits down and to the right of the upper one. It is
/// the only part of it you can see from above, so the whole thing reading as a
/// bunk bed rather than a bed rests on this.
const BUNK_DROP: Vec2 = vec2(10.0, 22.0);

/// How long the hob will sit lit with nothing coming up to heat before it
/// shuts itself off, in game minutes.
///
/// It has to clear the gap a normal meal leaves: the stew finishes about ten
/// minutes before the Bim has served it and reached for the knob, and cutting
/// out in the middle of that would be a bug rather than a safety feature.
const HOB_TIMEOUT: f32 = 15.0;

/// How fast doors and drawers travel, in fractions of open per second.
const SWING_RATE: f32 = 3.0;

pub struct Room {
    pub interior: Rect,
    pub counter: Rect,
    pub fridge: Rect,
    pub stove: Rect,
    pub board: Rect,
    pub drawer: Rect,
    pub table: Rect,
    pub chair: Vec2,
    pub bed: Rect,
    /// The heads, which owns its own walls, door and fittings.
    pub bath: Bath,
    /// The galley dishwasher, which runs on the clock rather than on the Bim.
    pub dishwasher: Dishwasher,
    /// The hydroponic bay. It lives here because it is furniture — something
    /// to walk round, click on and draw — but the game drives its clock,
    /// because the target it works to is the manager's rather than the
    /// room's.
    pub bay: Bay,

    /// 0 shut, 1 wide open. Animated towards `fridge_target`.
    pub fridge_door: f32,
    fridge_target: f32,
    pub drawer_open: f32,
    drawer_target: f32,

    pub stove_on: bool,
    /// Lags `stove_on` so the burner glows up and fades rather than snapping.
    pub stove_heat: f32,
    /// Game minutes the hob has been lit with nothing cooking on it.
    stove_idle: f32,

    /// How full the pot is, and how far along the cooking is.
    pub pot_contents: f32,
    /// Helpings left in the pot. A stew is cooked once and eaten twice, so
    /// this is what says whether the Bim has to cook at all.
    pub pot_servings: u32,
    pub pot_cooked: f32,

    /// The whole vegetable on the board, and the slices piling up beside it.
    pub board_veg: f32,
    pub board_slices: u32,
    pub knife_on_board: bool,
    /// What is being chopped, which is all that separates tofu from a carrot
    /// as far as the board is concerned.
    pub board_tofu: bool,
    /// What the meal in progress is, for the plate and the bowl.
    pub dish: Dish,
    /// Produce left in the cold store.
    /// The cold store, counted in the two things that go into a meal.
    pub veg: u32,
    pub tofu: u32,

    /// Plates in the world, carrying how full they are.
    pub plate_on_counter: Option<f32>,
    pub plate_on_table: Option<f32>,

    /// 0 with the bed made, 1 with the blanket pulled up over a sleeper.
    blanket: f32,
    blanket_target: f32,

    /// Free-running clock for bubbling, steam and the burner flicker.
    time: f32,
}

impl Room {
    pub fn new() -> Room {
        let interior = Rect::from_corners(vec2(WALL, WALL), vec2(ROOM_W - WALL, ROOM_H - WALL));
        let top = interior.min.y;

        let fridge = Rect::from_min_size(vec2(interior.min.x + 26.0, top), vec2(76.0, 62.0));
        let counter =
            Rect::from_min_size(vec2(interior.min.x + 152.0, top), vec2(532.0, COUNTER_D));
        let stove = Rect::from_min_size(vec2(counter.min.x + 366.0, top), vec2(118.0, COUNTER_D));
        let board = Rect::from_min_size(vec2(counter.min.x + 28.0, top + 18.0), vec2(96.0, 34.0));
        // The drawer is a face on the front of the counter, between board and stove.
        let drawer = Rect::from_min_size(
            vec2(counter.min.x + 176.0, counter.max.y - 20.0),
            vec2(112.0, 18.0),
        );

        let table = Rect::from_center_size(vec2(ROOM_W * 0.5, ROOM_H * 0.68), vec2(184.0, 116.0));
        let chair = vec2(table.center().x, table.min.y - 30.0);

        // Against the left wall and clear of the fridge above it, with the
        // head end towards the top of the room and the ladder at the foot.
        // This is the footprint of both bunks together, so the Bim cannot
        // walk through the half of the lower one that sticks out.
        let bed = Rect::from_min_size(
            vec2(interior.min.x + 20.0, interior.min.y + 182.0),
            vec2(84.0, 150.0) + BUNK_DROP,
        );

        Room {
            interior,
            counter,
            fridge,
            stove,
            board,
            drawer,
            table,
            chair,
            bed,
            bath: Bath::new(interior),
            dishwasher: Dishwasher::new(counter),
            bay: Bay::new(interior),
            fridge_door: 0.0,
            fridge_target: 0.0,
            drawer_open: 0.0,
            drawer_target: 0.0,
            stove_on: false,
            stove_heat: 0.0,
            stove_idle: 0.0,
            pot_contents: 0.0,
            pot_servings: 0,
            pot_cooked: 0.0,
            board_veg: 0.0,
            board_slices: 0,
            knife_on_board: false,
            board_tofu: false,
            dish: Dish::Stew,
            veg: START_VEG,
            tofu: START_TOFU,
            plate_on_counter: None,
            plate_on_table: None,
            blanket: 0.0,
            blanket_target: 0.0,
            time: 0.0,
        }
    }

    // --- geometry the rest of the game asks about ------------------------

    /// Where the Bim stands to work at something along the top wall.
    fn station_at(&self, x: f32) -> Vec2 {
        vec2(x, self.counter.max.y + STAND_OFF)
    }

    pub fn fridge_station(&self) -> Vec2 {
        // Stand to the side the door does not swing into.
        self.station_at(self.fridge.center().x - 6.0)
    }

    pub fn board_station(&self) -> Vec2 {
        self.station_at(self.board.center().x)
    }

    pub fn drawer_station(&self) -> Vec2 {
        self.station_at(self.drawer.center().x)
    }

    pub fn dishwasher_station(&self) -> Vec2 {
        self.station_at(self.dishwasher.face.center().x)
    }

    /// The thing itself: where the Bim's hand has to end up.
    fn switch_target(&self, which: Switch) -> Vec2 {
        match which {
            Switch::Hob => self.knob_pos(),
            Switch::FridgeDoor => self.fridge.center(),
            Switch::BathDoor(_) | Switch::BathLock(_) => self.bath.door.center(),
            Switch::Dishwasher => self.dishwasher.face.center(),
        }
    }

    /// Where it stands to reach that. The bathroom door has a panel on both
    /// sides, so which one depends on where the Bim is when it sets off — a
    /// Bim shut inside would otherwise be sent to a handle it cannot reach.
    pub fn switch_station(&self, which: Switch, from: Vec2) -> Vec2 {
        match which {
            Switch::Hob => self.stove_station(),
            Switch::FridgeDoor => self.fridge_station(),
            Switch::BathDoor(_) | Switch::BathLock(_) => {
                if self.bath.shell.contains(from) {
                    self.bath.inside_station()
                } else {
                    self.bath.outside_station()
                }
            }
            Switch::Dishwasher => self.dishwasher_station(),
        }
    }

    /// Which way it turns to work it: towards the thing itself.
    pub fn switch_facing(&self, which: Switch, from: Vec2) -> f32 {
        (self.switch_target(which) - self.switch_station(which, from)).angle()
    }

    /// Work it. Called the instant the Bim's hand reaches it, so what it does
    /// happens then and not when the player asked. The hob, the fridge and the
    /// dishwasher read the state they find; the bathroom door and its lock are
    /// set to what was asked for instead — see [`Switch`].
    pub fn work_switch(&mut self, which: Switch) {
        match which {
            Switch::Hob => {
                let on = self.stove_on;
                self.set_stove(!on);
            }
            Switch::FridgeDoor => {
                let open = self.fridge_is_open();
                self.set_fridge_open(!open);
            }
            Switch::BathDoor(open) => self.bath.set_open(open),
            Switch::BathLock(locked) => self.bath.set_locked(locked),
            Switch::Dishwasher => self.dishwasher.start(),
        }
    }

    pub fn stove_station(&self) -> Vec2 {
        self.station_at(self.pot_pos().x)
    }

    /// Serving needs the pot on one side and the plate on the other, both
    /// within a spoon's swing.
    pub fn serve_station(&self) -> Vec2 {
        self.station_at((self.serving_pos().x + self.pot_pos().x) * 0.5)
    }

    /// The two hobs. The pot always stands on the left one, which is the one
    /// that lights, so heat and pot can never drift apart.
    fn burners(&self) -> [Vec2; 2] {
        let c = self.stove.center();
        [c + vec2(-30.0, 0.0), c + vec2(30.0, 0.0)]
    }

    pub fn pot_pos(&self) -> Vec2 {
        self.burners()[0]
    }

    /// The control knob, set towards the near-left of the hob so it is within
    /// reach from both the pot and the serving spot.
    fn knob_pos(&self) -> Vec2 {
        vec2(self.stove.min.x + 20.0, self.stove.max.y - 7.0)
    }

    /// Where a plate sits while being served, on the worktop beside the hob.
    pub fn serving_pos(&self) -> Vec2 {
        vec2(self.stove.min.x - 24.0, self.counter.min.y + 34.0)
    }

    /// The upper bunk: the bed you can actually see, and the one the Bim
    /// sleeps in. It sits at the top-left of the footprint, with the lower
    /// bunk showing along the other two sides.
    fn top_bunk(&self) -> Rect {
        Rect::from_min_size(self.bed.min, self.bed.size() - BUNK_DROP)
    }

    /// Where the Bim stands to climb into the bed: beside the ladder, which
    /// hangs on the right-hand rail at the foot end.
    pub fn bed_station(&self) -> Vec2 {
        vec2(self.bed.max.x + 28.0, self.top_bunk().max.y - 34.0)
    }

    /// Where the Bim lies once it is up on the top bunk, head towards the
    /// pillow. Everything else on the bed is placed relative to this, so the
    /// head cannot drift off the pillow when the bed moves.
    pub fn bed_lie_pos(&self) -> Vec2 {
        let top = self.top_bunk();
        vec2(top.center().x, top.min.y + 46.0)
    }

    fn pillow_pos(&self) -> Vec2 {
        self.bed_lie_pos() + vec2(0.0, -12.0)
    }

    pub fn table_plate_pos(&self) -> Vec2 {
        vec2(self.table.center().x, self.table.min.y + 34.0)
    }

    /// Everything fixed that the Bim has to walk around. The bathroom door is
    /// not in here: it comes and goes, and the pathfinder handles it
    /// separately through [`Room::closed_door`].
    pub fn solids(&self) -> [Rect; 8] {
        let [north_left, north_right, west] = self.bath.solids();
        [
            self.counter,
            self.fridge,
            self.table,
            self.bed,
            self.bay.frame,
            north_left,
            north_right,
            west,
        ]
    }

    /// The bathroom door, when it is shut and therefore in the way.
    pub fn closed_door(&self) -> Option<Rect> {
        self.bath.closed_door()
    }

    /// Which fixture, if any, is under a click.
    pub fn hit(&self, p: Vec2) -> u32 {
        // The stove sits within the counter run, so test it first.
        if self.stove.contains(p) {
            HIT_STOVE
        } else if self.fridge.expand(4.0).contains(p) {
            HIT_FRIDGE
        } else if self.dishwasher.face.expand(6.0).contains(p) {
            HIT_DISHWASHER
        } else if self.bed.expand(4.0).contains(p) {
            HIT_BED
        } else if self.bay.frame.expand(6.0).contains(p) {
            HIT_HYDRO
        } else {
            self.bath.hit(p)
        }
    }

    // --- state the player and the task both drive -----------------------

    pub fn set_fridge_open(&mut self, open: bool) {
        self.fridge_target = if open { 1.0 } else { 0.0 };
    }

    pub fn fridge_is_open(&self) -> bool {
        self.fridge_target > 0.5
    }

    pub fn set_drawer_open(&mut self, open: bool) {
        self.drawer_target = if open { 1.0 } else { 0.0 };
    }

    pub fn set_stove(&mut self, on: bool) {
        self.stove_on = on;
        // Lighting it starts the clock again, however it was lit.
        self.stove_idle = 0.0;
    }

    /// Game minutes before the hob gives up and turns itself off, or zero when
    /// it is not counting — off already, or with something actually cooking.
    pub fn stove_idle_left(&self) -> f32 {
        if !self.stove_on || self.is_cooking() {
            return 0.0;
        }
        (HOB_TIMEOUT - self.stove_idle).max(0.0)
    }

    /// Something in the pot that has not finished coming up to heat. A stew
    /// already done is not cooking — it is just sitting on a live ring, which
    /// is the case the timeout is there to catch.
    fn is_cooking(&self) -> bool {
        self.pot_contents > 0.0 && self.pot_cooked < 1.0
    }

    /// Pull the blanket up over a sleeper, or make the bed again.
    pub fn set_bed_occupied(&mut self, occupied: bool) {
        self.blanket_target = if occupied { 1.0 } else { 0.0 };
    }

    /// Clear the worktop so a fresh cook does not inherit the last one's mess.
    pub fn reset_for_cooking(&mut self, dish: Dish) {
        self.board_veg = 0.0;
        self.board_slices = 0;
        self.knife_on_board = false;
        self.board_tofu = dish == Dish::Bowl;
        self.dish = dish;
        self.plate_on_counter = None;
        self.plate_on_table = None;
    }

    /// One trip to the fridge for `dish`, taking what that trip carries: a
    /// vegetable for a stew — twice, once per portion — and for a bowl the
    /// block of tofu and the salad together.
    pub fn take_from_fridge(&mut self, dish: Dish) {
        match dish {
            Dish::Stew => self.veg = self.veg.saturating_sub(1),
            Dish::Bowl => {
                self.tofu = self.tofu.saturating_sub(1);
                self.veg = self.veg.saturating_sub(1);
            }
        }
    }

    /// Whether the store holds what this recipe needs, all of it: a chain that
    /// starts without one of its halves ends with the Bim eating an empty
    /// plate.
    pub fn can_cook(&self, dish: Dish) -> bool {
        match dish {
            Dish::Stew => self.veg >= 2,
            Dish::Bowl => self.tofu >= 1 && self.veg >= 1,
        }
    }

    /// Whether there is enough left for a meal of some sort.
    pub fn has_ingredients(&self) -> bool {
        self.can_cook(Dish::Stew) || self.can_cook(Dish::Bowl)
    }

    /// Put a crop from the bay away.
    pub fn store(&mut self, crop: crate::hydro::Crop) {
        match crop {
            crate::hydro::Crop::Veg => self.veg += 1,
            crate::hydro::Crop::Soy => self.tofu += 1,
        }
    }

    pub fn update(&mut self, dt: f32) {
        self.time += dt;
        self.bath.update(dt);
        self.dishwasher.update(dt);

        let step = SWING_RATE * dt;
        self.fridge_door += clamp(self.fridge_target - self.fridge_door, -step, step);
        self.drawer_open += clamp(self.drawer_target - self.drawer_open, -step, step);
        // Slower than a drawer: a blanket is drawn up, not snapped.
        let pull = dt * 1.2;
        self.blanket += clamp(self.blanket_target - self.blanket, -pull, pull);

        let want = if self.stove_on { 1.0 } else { 0.0 };
        // Heating up is slower than cooling down, as a real hob is.
        let rate = if self.stove_on { 1.6 } else { 0.9 };
        self.stove_heat += clamp(want - self.stove_heat, -rate * dt, rate * dt);

        // Anything in the pot slowly turns from raw greens into a stew.
        if self.pot_contents > 0.0 && self.stove_heat > 0.4 {
            self.pot_cooked = (self.pot_cooked + dt * 0.22).min(1.0);
        }

        // A hob left lit with nothing coming up to heat shuts itself off. The
        // count only runs while nothing is cooking, so a meal in progress
        // keeps resetting it and is never cut short.
        if self.stove_on && !self.is_cooking() {
            self.stove_idle += dt * MINUTES_PER_SECOND;
            if self.stove_idle >= HOB_TIMEOUT {
                self.set_stove(false);
            }
        } else if self.stove_on {
            self.stove_idle = 0.0;
        }
    }

    // --- drawing ---------------------------------------------------------

    pub fn draw(&self, list: &mut DrawList) {
        self.draw_floor(list);
        self.draw_counter(list);
        self.draw_stove(list);
        self.draw_fridge(list);
        self.draw_table(list);
        self.draw_bunk(list);
        self.bay.draw(list);
        self.bath.draw(list);
    }

    /// The parts of the room that belong *above* the Bim: the blanket over a
    /// sleeper, the safety rails along the top bunk, the ladder hooked on the
    /// outside of one, and the corner posts, which stand proud of everything.
    /// Drawn after the character, so climbing into bed actually puts the Bim
    /// under the covers.
    pub fn draw_over(&self, list: &mut DrawList) {
        let top = self.top_bunk();
        // The duvet reaches from just below the pillow to the foot of the
        // mattress whether the bed is made or slept in — pulled up to the chin
        // is where it ends up either way. What changes is the shape of it.
        let head = top.min.y + 52.0;
        let foot = top.max.y - 12.0;
        let mid = vec2(top.center().x, (head + foot) * 0.5);
        list.rect(
            mid,
            vec2(top.width() - 22.0, foot - head),
            0.0,
            6.0,
            BLANKET,
        );
        // A turned-down cuff along the top edge, so it reads as bedding
        // rather than a coloured panel.
        list.rect(
            vec2(top.center().x, head + 5.0),
            vec2(top.width() - 22.0, 11.0),
            0.0,
            5.0,
            BLANKET_FOLD,
        );
        // The shape of whoever is under it, rising and falling as they
        // breathe. On an empty bed this fades away to nothing.
        if self.blanket > 0.01 {
            let breath = 1.0 + 0.03 * (self.time * 1.1).sin();
            list.ellipse(
                vec2(top.center().x, head + 36.0),
                vec2(46.0, 74.0 * breath),
                0.0,
                BLANKET_FOLD.alpha(0.33 * self.blanket),
            );
        }
        // Creases running down towards the foot.
        for side in [-1.0f32, 1.0] {
            list.rect(
                vec2(top.center().x + side * 17.0, mid.y + 8.0),
                vec2(3.5, foot - head - 34.0),
                0.0,
                2.0,
                BUNK_SHADOW.alpha(0.12),
            );
        }

        // Safety rails down both long sides of the upper bunk — the thing
        // that says this is the top of two beds and not just a bed.
        for side in [-1.0f32, 1.0] {
            list.rect(
                vec2(
                    top.center().x + side * (top.width() * 0.5 - 5.0),
                    top.center().y,
                ),
                vec2(11.0, top.height() - 24.0),
                0.0,
                5.0,
                BUNK_RAIL,
            );
        }

        self.draw_ladder(list);

        // Corner posts, carrying on past the upper bunk towards the ceiling.
        // Seen from directly above they are the ends of four uprights.
        for dx in [-1.0f32, 1.0] {
            for dy in [-1.0f32, 1.0] {
                let at = top.center()
                    + vec2(
                        dx * (top.width() * 0.5 - 5.0),
                        dy * (top.height() * 0.5 - 5.0),
                    );
                list.circle(at, 23.0, BUNK_SHADOW.alpha(0.22));
                list.circle(at, 20.0, BUNK_LOWER);
                list.circle(at, 14.0, BUNK_POST);
                list.circle(at, 6.0, BUNK_FRAME);
            }
        }
    }

    /// A bunk bed seen from above. The trouble with drawing one this way up is
    /// that the upper bunk covers the lower one almost completely, so the bed
    /// underneath is set down and to the right by `BUNK_DROP` and what you see
    /// of it is the band of mattress and bedding along two sides.
    fn draw_bunk(&self, list: &mut DrawList) {
        let top = self.top_bunk();
        let low = Rect::from_min_size(top.min + BUNK_DROP, top.size());

        // The bed underneath, made up in its own bedding so the strip that
        // shows is recognisably another bed.
        list.rect(
            low.center() + vec2(3.0, 4.0),
            low.size() + vec2(7.0, 7.0),
            0.0,
            10.0,
            BUNK_SHADOW,
        );
        list.rect(low.center(), low.size(), 0.0, 8.0, BUNK_FRAME);
        list.rect(
            low.center(),
            low.size() - vec2(18.0, 18.0),
            0.0,
            5.0,
            MATTRESS_LOW,
        );
        list.rect(
            vec2(low.center().x, low.min.y + low.height() * 0.62),
            vec2(low.width() - 22.0, low.height() * 0.66),
            0.0,
            6.0,
            BLANKET_LOW,
        );
        // Its own pillow, at the same head end as the one above.
        list.rect(
            vec2(low.center().x, low.min.y + 34.0),
            vec2(low.width() - 34.0, 34.0),
            0.0,
            9.0,
            PILLOW.alpha(0.75),
        );

        // The shadow the upper bunk throws onto it, which is what sells one
        // bed being above the other rather than beside it.
        list.rect(
            top.center() + BUNK_DROP * 0.5,
            top.size() + vec2(6.0, 6.0),
            0.0,
            10.0,
            BUNK_SHADOW,
        );

        // A light under the upper bunk, throwing onto the bed below — the
        // usual way a bunk is lit, and it separates the two decks of it.
        list.stroke_rect(
            low.center(),
            low.size() - vec2(12.0, 12.0),
            0.0,
            6.0,
            2.0,
            GLOW.alpha(0.22),
        );

        // The upper bunk: frame, then mattress.
        list.rect(top.center(), top.size(), 0.0, 8.0, BUNK_FRAME);
        list.rect(
            top.center(),
            top.size() - vec2(20.0, 20.0),
            0.0,
            5.0,
            MATTRESS,
        );

        // Pillow at the head end, placed off the lying position so the Bim's
        // head always lands on it.
        let pillow = self.pillow_pos();
        list.rect(pillow, vec2(top.width() - 34.0, 34.0), 0.0, 9.0, PILLOW);
        list.line(
            pillow + vec2(0.0, -12.0),
            pillow + vec2(0.0, 12.0),
            1.5,
            MATTRESS_LOW.alpha(0.35),
        );
    }

    /// The ladder, hooked over the right-hand rail at the foot end — which is
    /// where the Bim stands when it climbs up.
    fn draw_ladder(&self, list: &mut DrawList) {
        let top = self.top_bunk();
        let x = top.max.x - 2.0;
        let (head, foot) = (top.max.y - 66.0, top.max.y - 6.0);
        for side in [-1.0f32, 1.0] {
            list.rect(
                vec2(x + side * 10.0, (head + foot) * 0.5),
                vec2(5.5, foot - head),
                0.0,
                2.0,
                BUNK_FRAME,
            );
        }
        for i in 0..4 {
            let y = lerp(head + 7.0, foot - 7.0, i as f32 / 3.0);
            list.rect(vec2(x, y), vec2(25.0, 5.0), 0.0, 2.0, BUNK_POST);
        }
    }

    fn draw_floor(&self, list: &mut DrawList) {
        let room = Rect::from_min_size(Vec2::ZERO, vec2(ROOM_W, ROOM_H));
        list.rect(room.center(), room.size(), 0.0, 0.0, WALL_C);
        list.rect(
            self.interior.center(),
            self.interior.size() + vec2(6.0, 6.0),
            0.0,
            0.0,
            WALL_TRIM,
        );
        list.rect(
            self.interior.center(),
            self.interior.size(),
            0.0,
            0.0,
            FLOOR,
        );

        let mut x = self.interior.min.x + TILE;
        while x < self.interior.max.x {
            list.line(
                vec2(x, self.interior.min.y),
                vec2(x, self.interior.max.y),
                1.0,
                TILE_LINE,
            );
            x += TILE;
        }
        let mut y = self.interior.min.y + TILE;
        while y < self.interior.max.y {
            list.line(
                vec2(self.interior.min.x, y),
                vec2(self.interior.max.x, y),
                1.0,
                TILE_LINE,
            );
            y += TILE;
        }

        // A light run at the foot of the hull, all the way round. It is the
        // one thing that lights the deck, so it sets how the room reads.
        let skirt = self.interior.expand(-3.0);
        list.stroke_rect(
            skirt.center(),
            skirt.size(),
            0.0,
            0.0,
            2.5,
            GLOW.alpha(0.16),
        );
        // Brighter at intervals, where the fittings are.
        let stops = 9;
        for i in 0..stops {
            let x = lerp(
                skirt.min.x + 40.0,
                skirt.max.x - 40.0,
                i as f32 / (stops - 1) as f32,
            );
            list.rect(
                vec2(x, skirt.max.y),
                vec2(26.0, 3.0),
                0.0,
                1.5,
                GLOW.alpha(0.45),
            );
            list.rect(
                vec2(x, skirt.min.y),
                vec2(26.0, 3.0),
                0.0,
                1.5,
                GLOW.alpha(0.30),
            );
        }
    }

    fn draw_counter(&self, list: &mut DrawList) {
        let c = self.counter;
        list.rect(c.center(), c.size(), 0.0, 3.0, CAB);
        // A worktop lip along the front edge reads as thickness from above.
        list.rect(
            vec2(c.center().x, c.max.y - 7.0),
            vec2(c.width(), 14.0),
            0.0,
            3.0,
            WORKTOP_EDGE,
        );
        list.rect(
            vec2(c.center().x, c.min.y + (c.height() - 14.0) * 0.5),
            vec2(c.width() - 6.0, c.height() - 14.0),
            0.0,
            2.0,
            WORKTOP,
        );
        // A light strip along the whole fascia, the way every powered surface
        // on the ship is edged. It is what makes the run read as built in
        // rather than a slab of grey.
        list.rect(
            vec2(c.center().x, c.max.y - 2.0),
            vec2(c.width() - 12.0, 3.0),
            0.0,
            1.5,
            GLOW_DIM,
        );
        // Service panels along the front, with a status lamp on each.
        let panels = 6;
        for i in 0..panels {
            let x = lerp(
                c.min.x + 34.0,
                c.max.x - 34.0,
                i as f32 / (panels - 1) as f32,
            );
            list.rect(vec2(x, c.max.y - 9.0), vec2(1.5, 9.0), 0.0, 0.0, PANEL_EDGE);
            list.circle(vec2(x, c.max.y - 11.0), 3.0, GLOW.alpha(0.55));
        }

        // Drawer, sliding out towards the room. The open box is drawn behind
        // the face so you can see into it.
        let slide = self.drawer_open * 20.0;
        let d = self.drawer;
        if self.drawer_open > 0.02 {
            list.rect(
                d.center() + vec2(0.0, slide * 0.5),
                vec2(d.width() - 6.0, d.height() + slide),
                0.0,
                3.0,
                Color::rgb(0.14, 0.12, 0.10),
            );
        }
        list.rect(
            d.center() + vec2(0.0, slide),
            d.size() + vec2(0.0, 4.0),
            0.0,
            3.0,
            DRAWER_FACE,
        );
        list.rect(
            d.center() + vec2(0.0, slide),
            vec2(d.width() * 0.5, 4.0),
            0.0,
            2.0,
            HANDLE,
        );
        list.rect(
            d.center() + vec2(0.0, slide),
            vec2(d.width() * 0.5 - 8.0, 1.5),
            0.0,
            1.0,
            GLOW.alpha(0.7),
        );

        // Cutting board and whatever is on it.
        let b = self.board;
        list.rect(b.center(), b.size(), 0.0, 4.0, BOARD);
        list.stroke_rect(b.center(), b.size(), 0.0, 4.0, 1.5, GLOW.alpha(0.35));
        // Tofu is a block and chops into cubes; a vegetable is a lump and
        // chops into rounds. Same board, same knife, same counter of slices.
        let (whole, bits) = if self.board_tofu {
            (TOFU, TOFU_EDGE)
        } else {
            (VEG, VEG_DARK)
        };
        if self.board_veg > 0.0 {
            let grow = 0.35 + 0.65 * self.board_veg;
            let at = vec2(b.min.x + 26.0, b.center().y);
            if self.board_tofu {
                list.rect(at, vec2(30.0 * grow, 20.0), 0.0, 3.0, whole);
                list.stroke_rect(at, vec2(30.0 * grow, 20.0), 0.0, 3.0, 1.5, bits);
            } else {
                list.ellipse(at, vec2(30.0 * grow, 17.0), 0.0, whole);
                list.ellipse(at + vec2(-13.0 * grow, 0.0), vec2(8.0, 12.0), 0.0, bits);
            }
        }
        for i in 0..self.board_slices {
            // Slices pile up to the right of what is left of it.
            let col = (i % 5) as f32;
            let row = (i / 5) as f32;
            let at = vec2(b.min.x + 48.0 + col * 9.5, b.center().y - 6.0 + row * 11.0);
            if self.board_tofu {
                list.rect(at, vec2(9.0, 9.0), 0.0, 2.0, whole);
                list.stroke_rect(at, vec2(9.0, 9.0), 0.0, 2.0, 1.0, bits);
            } else {
                list.circle(at, 9.0, whole);
                list.circle(at, 4.0, bits);
            }
        }
        if self.knife_on_board {
            draw_knife(list, b.center() + vec2(26.0, 13.0), 0.35);
        }

        self.dishwasher.draw(list);

        if let Some(fill) = self.plate_on_counter {
            draw_plate(list, self.serving_pos(), fill, self.pot_cooked, self.dish);
        }
    }

    fn draw_stove(&self, list: &mut DrawList) {
        let s = self.stove;
        list.rect(s.center(), s.size() - vec2(0.0, 14.0), 0.0, 4.0, STOVE_TOP);

        let hot = self.stove_heat;
        let flicker = 0.85 + 0.15 * (self.time * 9.0).sin();
        for (i, at) in self.burners().iter().enumerate() {
            list.circle(*at, 46.0, BURNER);
            // Induction coils, etched into the glass and lit low even when
            // cold, so a dead hob still looks like equipment.
            for ring in 0..3 {
                list.ring(*at, 18.0 + ring as f32 * 11.0, 1.5, GLOW.alpha(0.16));
            }
            list.ring(*at, 38.0, 2.0, STOVE_TOP);
            list.ring(*at, 42.0, 1.5, GLOW.alpha(0.30));
            // Only the hob under the pot lights.
            if i == 0 && hot > 0.01 {
                list.circle(*at, 40.0, BURNER_HOT.alpha(0.7 * hot * flicker));
                list.ring(*at, 30.0, 3.0, BURNER_HOT.alpha(0.9 * hot));
            }
        }

        // Touch control, so "on" is readable at a glance: a lit pip that
        // slides along its track and goes warm when the hob is live.
        let knob = self.knob_pos();
        list.rect(knob, vec2(34.0, 12.0), 0.0, 6.0, STOVE_TOP);
        list.stroke_rect(knob, vec2(34.0, 12.0), 0.0, 6.0, 1.0, PANEL_EDGE);
        let lit = if self.stove_on { KNOB_ON } else { GLOW };
        let pip = knob + vec2(lerp(-9.0, 9.0, self.stove_heat), 0.0);
        list.circle(pip, 15.0, lit.alpha(0.20));
        list.circle(pip, 8.0, lit);
        list.circle(pip, 3.0, KNOB.alpha(0.5));

        self.draw_pot(list);
    }

    fn draw_pot(&self, list: &mut DrawList) {
        let at = self.pot_pos();
        list.circle(at, 50.0, POT);
        list.ring(at, 46.0, 4.0, POT_RIM);
        // Handles either side.
        list.rect(at + vec2(-30.0, 0.0), vec2(14.0, 7.0), 0.0, 3.0, POT_RIM);
        list.rect(at + vec2(30.0, 0.0), vec2(14.0, 7.0), 0.0, 3.0, POT_RIM);

        if self.pot_contents <= 0.0 {
            list.circle(at, 40.0, Color::rgb(0.10, 0.11, 0.12));
            return;
        }

        let broth = Color::rgb(
            lerp(BROTH_RAW.r, BROTH_DONE.r, self.pot_cooked),
            lerp(BROTH_RAW.g, BROTH_DONE.g, self.pot_cooked),
            lerp(BROTH_RAW.b, BROTH_DONE.b, self.pot_cooked),
        );
        let surface = 40.0 * (0.6 + 0.4 * self.pot_contents);
        list.circle(at, surface, broth);

        if self.stove_heat <= 0.3 {
            return;
        }

        // Bubbles working their way around the surface.
        let n = 5;
        for i in 0..n {
            let phase = self.time * 2.4 + i as f32 * (TAU / n as f32);
            let r = surface * 0.3 + surface * 0.25 * ((phase * 0.7).sin() * 0.5 + 0.5);
            let a = phase.sin() * 0.5 + 0.5;
            let p = at + Vec2::from_angle(phase * 1.7) * r;
            list.circle(p, 4.0 + 3.0 * a, broth.alpha(0.3 + 0.45 * a));
        }
        // Steam. Seen from above it spreads out from the pot rather than
        // rising, which also keeps it from drifting through the wall.
        for i in 0..4 {
            let t = (self.time * 0.5 + i as f32 * 0.25) % 1.0;
            let a = i as f32 * (TAU / 4.0) + self.time * 0.35;
            let p = at + Vec2::from_angle(a) * (t * 30.0);
            list.circle(
                p,
                14.0 + t * 26.0,
                STEAM.alpha(0.16 * (1.0 - t) * self.stove_heat),
            );
        }
    }

    fn draw_fridge(&self, list: &mut DrawList) {
        let f = self.fridge;
        list.rect(f.center(), f.size(), 0.0, 4.0, FRIDGE);
        // A frosted inspection panel with the cold light behind it, and a
        // status bar down the hinge side.
        list.rect(
            f.center() + vec2(4.0, 0.0),
            f.size() - vec2(22.0, 20.0),
            0.0,
            3.0,
            FRIDGE_IN.alpha(0.35),
        );
        list.rect(
            vec2(f.min.x + 7.0, f.center().y),
            vec2(3.5, f.height() - 22.0),
            0.0,
            1.5,
            GLOW.alpha(if self.fridge_is_open() { 0.9 } else { 0.45 }),
        );

        if self.fridge_door > 0.01 {
            // Interior, revealed as the door swings clear.
            list.rect(
                f.center() + vec2(0.0, 3.0),
                f.size() - vec2(10.0, 10.0),
                0.0,
                3.0,
                FRIDGE_IN,
            );
            // Two shelves, stocked with odds and ends.
            for (row, y) in [f.min.y + 22.0, f.min.y + 44.0].iter().enumerate() {
                list.rect(
                    vec2(f.center().x, *y),
                    vec2(f.width() - 16.0, 2.5),
                    0.0,
                    1.0,
                    SHELF,
                );
                // Eight places on the shelves for twenty items, so each one
                // stands for a couple and the shelves visibly empty out.
                let stock = (self.veg + self.tofu) as f32;
                let shown = (stock * 8.0 / SHELF_FULL).ceil().min(8.0) as usize;
                for i in 0..4 {
                    if row * 4 + i >= shown {
                        continue;
                    }
                    let at = vec2(f.min.x + 16.0 + i as f32 * 15.0, *y - 7.0);
                    let c = STOCK[(row * 4 + i) % STOCK.len()];
                    list.ellipse(at, vec2(10.0, 11.0), 0.0, c.alpha(self.fridge_door));
                }
            }
        }

        // The door is hinged at the front-left corner and swings into the room.
        let hinge = vec2(f.min.x, f.max.y);
        let angle = self.fridge_door * (PI * 0.58);
        let dir = Vec2::from_angle(angle);
        let len = f.width();
        list.rect(
            hinge + dir * (len * 0.5),
            vec2(len, 9.0),
            angle,
            3.0,
            FRIDGE_DOOR,
        );
        // Handle at the free end of the door.
        list.circle(hinge + dir * (len - 9.0), 8.0, HANDLE);
    }

    fn draw_table(&self, list: &mut DrawList) {
        // Chair first: the Bim sits on top of it, so it has to be wide enough
        // that a seated Bim does not overhang the sides.
        let ch = self.chair;
        list.rect(ch, vec2(62.0, 52.0), 0.0, 9.0, CHAIR);
        list.stroke_rect(ch, vec2(52.0, 42.0), 0.0, 7.0, 1.5, GLOW.alpha(0.28));
        list.rect(ch - vec2(0.0, 24.0), vec2(64.0, 10.0), 0.0, 4.0, TABLE_EDGE);
        list.rect(
            ch - vec2(0.0, 24.0),
            vec2(40.0, 2.0),
            0.0,
            1.0,
            GLOW.alpha(0.5),
        );

        let t = self.table;
        list.rect(t.center(), t.size(), 0.0, 14.0, TABLE_EDGE);
        list.rect(t.center(), t.size() - vec2(9.0, 9.0), 0.0, 11.0, TABLE);
        // An inlaid light around the top, and a seam across the middle where
        // the two halves of it fold.
        list.stroke_rect(
            t.center(),
            t.size() - vec2(24.0, 24.0),
            0.0,
            8.0,
            1.5,
            GLOW.alpha(0.40),
        );
        list.line(
            vec2(t.center().x, t.min.y + 12.0),
            vec2(t.center().x, t.max.y - 12.0),
            1.0,
            PANEL_EDGE.alpha(0.5),
        );

        if let Some(fill) = self.plate_on_table {
            draw_plate(
                list,
                self.table_plate_pos(),
                fill,
                self.pot_cooked,
                self.dish,
            );
            // Cutlery laid either side of the plate.
            let p = self.table_plate_pos();
            list.rect(p + vec2(-27.0, 0.0), vec2(4.0, 26.0), 0.0, 2.0, STEEL);
            list.rect(p + vec2(27.0, 0.0), vec2(4.0, 26.0), 0.0, 2.0, STEEL);
        }
    }
}

/// Odds and ends on the fridge shelves — greens, a bottle, leftovers.
const STOCK: [Color; 6] = [
    Color::rgb(0.44, 0.68, 0.24),
    Color::rgb(0.86, 0.34, 0.26),
    Color::rgb(0.92, 0.76, 0.32),
    Color::rgb(0.55, 0.42, 0.78),
    Color::rgb(0.30, 0.58, 0.72),
    Color::rgb(0.80, 0.55, 0.25),
];

/// A plate, optionally with a helping of stew on it.
pub fn draw_plate(list: &mut DrawList, at: Vec2, fill: f32, cooked: f32, dish: Dish) {
    list.circle(at, 44.0, PLATE_RIM);
    list.circle(at, 38.0, PLATE);
    if fill <= 0.0 {
        return;
    }
    let spread = 30.0 * fill.clamp(0.0, 1.0).sqrt();
    match dish {
        Dish::Stew => {
            let stew = Color::rgb(
                lerp(BROTH_RAW.r, BROTH_DONE.r, cooked),
                lerp(BROTH_RAW.g, BROTH_DONE.g, cooked),
                lerp(BROTH_RAW.b, BROTH_DONE.b, cooked),
            );
            list.circle(at, spread, stew);
            // A few chunks so the helping reads as food rather than a disc.
            for i in 0..4 {
                let a = i as f32 * (TAU / 4.0) + 0.6;
                let p = at + Vec2::from_angle(a) * (9.0 * fill);
                list.circle(p, 7.0 * fill, VEG.alpha(0.85));
            }
        }
        // A bowl is loose: leaves underneath, cubes of tofu on top, and no
        // broth to pool, so it reads as cold food rather than a helping.
        Dish::Bowl => {
            list.circle(at, spread, SALAD);
            for i in 0..5 {
                let a = i as f32 * (TAU / 5.0) + 0.3;
                let p = at + Vec2::from_angle(a) * (11.0 * fill);
                list.ellipse(p, vec2(15.0, 9.0) * fill, a, SALAD);
            }
            for i in 0..4 {
                let a = i as f32 * (TAU / 4.0) + 1.1;
                let p = at + Vec2::from_angle(a) * (8.0 * fill);
                list.rect(p, vec2(10.0, 10.0) * fill, a, 2.0, TOFU);
            }
        }
    }
}

/// A knife, pointing along `rot`.
pub fn draw_knife(list: &mut DrawList, at: Vec2, rot: f32) {
    let dir = Vec2::from_angle(rot);
    list.rect(at - dir * 9.0, vec2(18.0, 7.0), rot, 3.0, GRIP);
    list.rect(at + dir * 12.0, vec2(26.0, 9.0), rot, 2.0, STEEL);
}

/// A spoon, bowl-end pointing along `rot`.
pub fn draw_spoon(list: &mut DrawList, at: Vec2, rot: f32) {
    let dir = Vec2::from_angle(rot);
    list.rect(at - dir * 8.0, vec2(20.0, 4.0), rot, 2.0, STEEL);
    list.ellipse(at + dir * 9.0, vec2(13.0, 10.0), rot, STEEL);
}
