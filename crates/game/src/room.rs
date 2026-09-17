//! The kitchen: its fixed layout, the state of everything in it that moves,
//! and how all of that is drawn.
//!
//! The room is a fixed size in world units and the view is scaled to fit the
//! canvas around it (see `Game::resize`). That keeps the furniture at honest
//! proportions instead of stretching a table when someone widens the window.

use crate::bath::Bath;
use crate::clock::MINUTES_PER_SECOND;
use crate::dish::Dishwasher;
use crate::door::{Door, Order};
use crate::draw::{Color, DrawList};
use crate::filth::Filth;
use crate::hydro::Bay;
use crate::math::{PI, Rect, TAU, Vec2, clamp, lerp, vec2};

pub const ROOM_W: f32 = 860.0;
pub const ROOM_H: f32 = 580.0;

const WALL: f32 = 18.0;
/// Depth of the counter run against the top wall.
const COUNTER_D: f32 = 58.0;
/// The pot against the hob it stands on: a shade smaller than the burner's
/// full width, so the ring shows round it.
const POT_SIZE: f32 = 0.85;
/// The burner's radius at full size, which a one-tile hob is scaled to fit.
const BURNER_R: f32 = 46.0;
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

const BED_FRAME: Color = PANEL;
/// The headboard and the footboard, standing proud of the frame.
const BED_BOARD: Color = PANEL_EDGE;
const BED_BOARD_EDGE: Color = Color::rgb(0.46, 0.53, 0.60);
const BED_SHADOW: Color = Color::rgba(0.0, 0.0, 0.0, 0.34);
const MATTRESS: Color = Color::rgb(0.72, 0.75, 0.79);
const MATTRESS_SEAM: Color = Color::rgb(0.40, 0.44, 0.49);
const PILLOW: Color = Color::rgb(0.88, 0.91, 0.94);
const BLANKET: Color = Color::rgb(0.19, 0.33, 0.47);
const BLANKET_FOLD: Color = Color::rgb(0.31, 0.51, 0.66);

/// The broom: a composite pole and a head of stiff grey bristle. Warmer than
/// anything else aboard except the food, because it is the one tool with a
/// wooden ancestor.
pub const BROOM_POLE: Color = Color::rgb(0.55, 0.44, 0.31);
pub const BROOM_HEAD: Color = Color::rgb(0.72, 0.68, 0.58);

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
    /// One of a ship's powered doors, by index into [`Room::doors`], and
    /// what to do to it. Carries the order for the reason the bathroom
    /// door's do.
    Door(usize, Order),
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
pub const HIT_LOCKER: u32 = 8;
/// One of a ship's powered doors. Which one is [`Room::door_at`]'s answer,
/// which the game records for the host to ask after the click.
pub const HIT_SHIP_DOOR: u32 = 9;
/// The shower, where the room has one.
pub const HIT_SHOWER: u32 = 10;

/// What is at a point, for the readout that names whatever the pointer is
/// over.
///
/// A separate set from the `HIT_` codes above and deliberately so. Those
/// answer "what would a click here act on", which is why they are few and why
/// each rect is expanded a little — a near miss on a handle should still open
/// the menu. These answer "what is this", which wants the opposite: everything
/// aboard has a name, including the things no menu hangs off, and pointing a
/// few pixels beside the pan should say deck rather than toilet.
pub const SPOT_NOTHING: u32 = 0;
pub const SPOT_DECK: u32 = 1;
pub const SPOT_BULKHEAD: u32 = 2;
pub const SPOT_WORKTOP: u32 = 3;
pub const SPOT_BOARD: u32 = 4;
pub const SPOT_FRIDGE: u32 = 5;
pub const SPOT_HOB: u32 = 6;
pub const SPOT_DISHWASHER: u32 = 7;
pub const SPOT_TABLE: u32 = 8;
pub const SPOT_CHAIR: u32 = 9;
pub const SPOT_BUNK: u32 = 10;
pub const SPOT_BAY: u32 = 11;
pub const SPOT_TOILET: u32 = 12;
pub const SPOT_BASIN: u32 = 13;
pub const SPOT_DOOR: u32 = 14;
pub const SPOT_HEADS_DECK: u32 = 15;
pub const SPOT_LOCKER: u32 = 16;
/// A ship's powered door, as against the bathroom's.
pub const SPOT_SHIP_DOOR: u32 = 17;
/// The helm. Only ever *pointed at* — the work list's helm row rings it —
/// never read back off the deck: the ship's readout names the part itself.
pub const SPOT_HELM: u32 = 18;
/// A workstation, the same way: the craft row rings the first bench, and
/// the readout names the part.
pub const SPOT_BENCH: u32 = 19;
/// The shower, for the readout and for ringing.
pub const SPOT_SHOWER: u32 = 21;
/// The suit locker, the same way again: the mining row rings it.
pub const SPOT_SUIT_LOCKER: u32 = 20;

/// The chair, which is drawn from a centre and a size rather than kept as a
/// rect. Written down once here so the readout and `draw_table` agree.
/// A chair fits inside one tile, backrest included: 52 units square aboard,
/// and a seated body is `2 * BODY_MARGIN` — 46 — across, so this is as wide
/// as it can be without overhanging its own tile.
const CHAIR_SIZE: Vec2 = vec2(48.0, 42.0);

/// How many of each there are. One apiece: a Bim's bed and a Bim's chair are
/// its own, and nothing hands one over.
pub const BERTHS: usize = 2;
pub const SEATS: usize = 2;

/// One bed, and which side of it the deck is on.
///
/// There are two of them and they stand against opposite walls, so everything
/// about a bed that has a handedness — where the Bim stands to get in, which
/// way it turns to face the thing — is read off `side` rather than written
/// into the drawing twice. `side` is +1 when the room is to the *right* of
/// the bed (so the bed is against the left wall) and -1 when it is to the
/// left.
///
/// It used to be a bunk bed, drawn as an upper deck with the lower one
/// showing along two sides. It is a single bed now, filling its footprint,
/// and the footprint is the one the bunk had — so the crew stand and lie
/// where they always did, and no probe's route moved.
///
/// A berth belongs to exactly one Bim. Nothing shares one: two Bims and two
/// beds, and the pairing never moves.
pub struct Berth {
    pub frame: Rect,
    side: f32,
    /// 0 with the bed made, 1 with the blanket pulled up over a sleeper.
    blanket: f32,
    blanket_target: f32,
}

impl Berth {
    fn new(frame: Rect, side: f32) -> Berth {
        Berth {
            frame,
            side,
            blanket: 0.0,
            blanket_target: 0.0,
        }
    }

    /// The mattress: the frame less its rail all round.
    fn mattress(&self) -> Rect {
        Rect::from_min_size(
            self.frame.min + vec2(8.0, 8.0),
            self.frame.size() - vec2(16.0, 16.0),
        )
    }

    /// Where the Bim stands to get in: beside the bed, on whichever side
    /// faces the room, towards the foot. The numbers are the bunk's — the
    /// ladder hung at the foot — kept so nobody's walk changed.
    fn station(&self) -> Vec2 {
        let edge = if self.side > 0.0 {
            self.frame.max.x
        } else {
            self.frame.min.x
        };
        vec2(edge + self.side * 28.0, self.frame.max.y - 56.0)
    }

    /// Which way the Bim turns to get in — towards the bed, so it climbs in
    /// facing it rather than backwards.
    fn facing(&self) -> f32 {
        if self.side > 0.0 { PI } else { 0.0 }
    }

    /// Where the Bim lies, head towards the pillow. Everything else on the
    /// bed is placed relative to this, so the head cannot drift off the
    /// pillow when the bed moves.
    fn lie_pos(&self) -> Vec2 {
        vec2(self.frame.center().x, self.frame.min.y + 46.0)
    }

    fn pillow_pos(&self) -> Vec2 {
        self.lie_pos() + vec2(0.0, -12.0)
    }
}

/// How long the hob will sit lit with nothing coming up to heat before it
/// shuts itself off, in game minutes.
///
/// It has to clear the gap a normal meal leaves: the stew finishes about ten
/// minutes before the Bim has served it and reached for the knob, and cutting
/// out in the middle of that would be a bug rather than a safety feature.
const HOB_TIMEOUT: f32 = 15.0;

/// How fast doors and drawers travel, in fractions of open per second.
const SWING_RATE: f32 = 3.0;

/// A workstation a Bim can be stood at: the smelter, the workbench. What
/// the craft chain walks to — `task::Kind::Craft` — and nothing else about
/// it is the room's: what is made there, out of what, and whether it has
/// the power to run are the world's, which hands the room a list of
/// `game::Order`s every step. `kind` is the part's code, so the world can
/// say which bench a recipe wants without the room knowing a `PartKind`.
#[derive(Clone, Copy)]
pub struct Bench {
    pub kind: u32,
    /// Its footprint, for ringing.
    pub frame: Rect,
    /// Where the Bim stands to work it, off the part's use spot.
    pub at: Vec2,
}

impl Bench {
    /// Which way the Bim faces at it: into it.
    pub fn facing(&self) -> f32 {
        let d = self.frame.center() - self.at;
        d.y.atan2(d.x)
    }
}

/// Where everything in a room is, for a room that is laid out from somewhere
/// else — a ship design, through `crate::aboard` — rather than by hand in
/// [`Room::new`]. Plain rects and points in room units, and nothing that
/// knows what a ship is: the probes stand this file up with no other crate
/// behind it, and a layout type that named one would take that away.
///
/// The fixtures are what the room's errands walk to, so every one of them
/// has to be somewhere. A design the designer accepted has all of them —
/// `REQUIRED` in `shipdesign::validate` is this list — so a missing one is a
/// caller's mistake, and the room puts the fixture on the worktop rather
/// than panicking about it.
pub struct Layout {
    /// The whole build area.
    pub bounds: Rect,
    /// The deck: the bounding box of every floor tile.
    pub interior: Rect,
    pub counter: Rect,
    pub fridge: Rect,
    pub stove: Rect,
    pub dishwasher: Rect,
    pub table: Rect,
    /// Seat centres, in the order the crew take them.
    pub chairs: Vec<Vec2>,
    /// Bunk footprints, in the order the crew take them.
    pub beds: Vec<Rect>,
    pub locker: Rect,
    pub bay: Rect,
    /// Which side of the bay the Bim stands on to work it, as a unit step
    /// out of the frame: north in the classic room, wherever the design's
    /// use spots are aboard.
    pub bay_side: Vec2,
    pub toilet: Rect,
    pub sink: Rect,
    /// The shower, if the layout has one: its footprint and where the Bim
    /// stands to use it. None in the classic room, which has no shower and
    /// a crew that go on wanting one. The footprint stays in `others` — the
    /// room draws nothing for it; the ship painter does — so this is only
    /// where to walk to.
    pub shower: Option<(Rect, Vec2)>,
    /// The helm's footprint, if the layout has one: for ringing it when the
    /// work list's helm row is pointed at. Nothing walks to it by this — the
    /// seat is the world's to say, through `Game::set_helm`.
    pub helm: Option<Rect>,
    /// The workstations, in the design's id order. None in the classic
    /// room. Each stays a solid in `others` — the ship draws it.
    pub benches: Vec<Bench>,
    /// The suit locker, if the layout has one: its footprint and where the
    /// Bim stands at it. A solid in `others`, like the shower.
    pub suit_locker: Option<(Rect, Vec2)>,
    /// The deck just inside the port, and the spot just outside it beyond
    /// the hull, if the design has a port. Where a walk outside goes out
    /// from and is held at. None in the classic room, which has no
    /// airlock.
    pub gangway: Option<Vec2>,
    pub outside: Option<Vec2>,
    /// Everything else a body cannot walk through.
    pub others: Vec<Rect>,
    /// The powered doors, each its opening and whether its leaves slide
    /// along `x`. None in the classic room, whose one door is the heads'.
    pub doors: Vec<(Rect, bool)>,
    /// What is in the cold store to begin with.
    pub veg: u32,
    pub tofu: u32,
}

pub struct Room {
    /// The whole of the room, bulkheads included: what the view fits and
    /// what `spot` calls bulkhead rather than nothing. The classic room's
    /// is `ROOM_W` by `ROOM_H`; a ship's is its build area.
    pub bounds: Rect,
    /// Whether the room draws its own deck plate and walls. The classic room
    /// does; a room laid out from a ship design does not — the ship painter
    /// draws every tile of the hull itself, so this one draws only what
    /// stands on it.
    pub shell: bool,
    /// Blocking parts the room has no opinion about — an engine, a helm, a
    /// tank, an internal wall — which a body still has to walk round. Empty
    /// in the classic room, where everything solid is one of the fixtures.
    pub others: Vec<Rect>,
    /// The powered doors, in the order the layout listed them. Every one is
    /// a way through unless it is locked — see `crate::door`.
    pub doors: Vec<Door>,
    /// Whether this room draws its doors. A station's room kept open for
    /// its pictures while the ship is docked does not: the joined room has
    /// the same doors, with the people walking through them, and two
    /// pictures of one door in two states is a door in two states.
    pub doors_drawn: bool,
    pub interior: Rect,
    pub counter: Rect,
    pub fridge: Rect,
    pub stove: Rect,
    pub board: Rect,
    pub drawer: Rect,
    pub table: Rect,
    /// One seat per Bim. A seat belongs to a Bim the same way a berth does,
    /// so two of them can sit down to eat without one taking the other's
    /// chair out from under it. The classic room has [`SEATS`]; a room laid
    /// out from a design has as many as the design has chairs, and a Bim
    /// past the last one shares it.
    pub chairs: Vec<Vec2>,
    /// The bunks, one per Bim. See [`Berth`]. [`BERTHS`] in the classic
    /// room; a layout brings its own number, and a docked ship's room has
    /// the station's as well as its own.
    pub beds: Vec<Berth>,
    /// The heads, which owns its own walls, door and fittings.
    pub bath: Bath,
    /// The galley dishwasher, which runs on the clock rather than on the Bim.
    pub dishwasher: Dishwasher,
    /// The state of the deck, tile by tile: what has been spilt on it and
    /// where. It lives here because the deck *is* the room — and because the
    /// chains in `task.rs` have to be able to ask where the dirt is, and a
    /// task is handed the room and nothing else.
    /// The broom locker: a shallow door set into the port bulkhead, between
    /// the foot of the first bunk and the hydroponic bay. Not a solid — it is
    /// *in* the wall, and a body cannot get within its depth of the bulkhead
    /// anyway, so the nav grid is untouched by it.
    pub locker: Rect,
    /// Whether the broom is out of it. Drawn, and nothing else: a broom in a
    /// Bim's hands is not in the cupboard.
    pub broom_out: bool,
    /// The shower, if there is one: its footprint and the spot in front of
    /// it. See `Layout::shower`.
    pub shower: Option<(Rect, Vec2)>,
    /// The helm's footprint, if there is one, for ringing. See `Layout::helm`.
    pub helm: Option<Rect>,
    /// The workstations. See [`Bench`] and `Layout::benches`.
    pub benches: Vec<Bench>,
    /// The suit locker, the deck inside the port and the spot outside it.
    /// See `Layout::suit_locker`, `Layout::gangway`, `Layout::outside`.
    pub suit_locker: Option<(Rect, Vec2)>,
    pub gangway: Option<Vec2>,
    pub outside: Option<Vec2>,
    /// Walks outside finished since the world last asked. The world drains
    /// it with `Game::take_walks` and adds what the belt yields.
    pub walks_done: u32,
    /// Recipes finished at a bench since the world last asked — indices into
    /// `shipdesign::recipes::RECIPES`. The world drains it every step with
    /// `Game::take_crafted` and moves the cargo; the room keeps no stock of
    /// ore or metal and never will.
    pub crafted: Vec<u32>,
    pub filth: Filth,
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
    /// Whether what was last made in the galley — the pot, or a bowl off
    /// the board — was made in a dirty one and will make whoever eats it
    /// ill. Rolled by the game the moment the pot finishes cooking or a bowl
    /// is filled — see `Game::judge_the_food` — and cleared when the pot is
    /// filled afresh.
    pub food_bad: bool,
    /// Set the frame the pot finishes cooking or a bowl is filled, for the
    /// game to take and roll on.
    judge_food: bool,

    /// The whole vegetable on the board, and the slices piling up beside it.
    pub board_veg: f32,
    pub board_slices: u32,
    pub knife_on_board: bool,
    /// What is being chopped, which is all that separates tofu from a carrot
    /// as far as the board is concerned.
    pub board_tofu: bool,
    /// What the meal in progress is, for the plate and the bowl.
    pub dish: Dish,
    /// The cold store, counted in the two things that go into a meal and
    /// the one thing that comes out of the galley ready: pots of stew,
    /// cooked ahead to the manager's target and warmed up when somebody is
    /// hungry.
    pub veg: u32,
    pub tofu: u32,
    pub stew: u32,

    /// Plates in the world, carrying how full they are.
    pub plate_on_counter: Option<f32>,
    /// One per seat: two Bims can be at the table at once, and a shared slot
    /// would have the second one's plate land on top of the first one's.
    pub plate_on_table: Vec<Option<f32>>,

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
        // The broom locker, set into the port bulkhead in the one stretch of
        // it with nothing against it: below the foot of the first bunk and
        // above the hydroponic bay. Shallow, because it is a door in a wall
        // rather than a cupboard standing in the room — which is also why it
        // is not a solid. A body cannot get within its depth of the bulkhead
        // in any case, so the pathfinder never needs to know about it.
        let locker = Rect::from_min_size(vec2(interior.min.x, 400.0), vec2(20.0, 72.0));

        // One chair each side of the table, facing in. Both sit clear of the
        // table footprint so a Bim can stand on the spot before sitting down.
        let chairs = [
            vec2(table.center().x, table.min.y - 30.0),
            vec2(table.center().x, table.max.y + 30.0),
        ];

        // Two beds against opposite walls. The footprint is the one the bunk
        // bed had — its upper deck and the lower one sticking out beside it —
        // because the nav grid, and so every route and every seed a probe
        // pins, is built on it.
        //
        // The first is against the left wall, clear of the fridge above it,
        // head end towards the top of the room — where the only bed aboard
        // always stood. The second is in the top-right corner, the one other
        // stretch of wall with nothing on it: the counter run ends short of
        // it and the heads start well below.
        let bunk = vec2(94.0, 172.0);
        let beds = [
            Berth::new(
                Rect::from_min_size(vec2(interior.min.x + 20.0, interior.min.y + 182.0), bunk),
                1.0,
            ),
            Berth::new(
                Rect::from_min_size(
                    vec2(interior.max.x - 20.0 - bunk.x, interior.min.y + 96.0),
                    bunk,
                ),
                -1.0,
            ),
        ];

        Room {
            bounds: Rect::from_min_size(Vec2::ZERO, vec2(ROOM_W, ROOM_H)),
            shell: true,
            others: Vec::new(),
            doors: Vec::new(),
            doors_drawn: true,
            interior,
            counter,
            fridge,
            stove,
            board,
            drawer,
            table,
            chairs: chairs.to_vec(),
            beds: beds.into(),
            locker,
            broom_out: false,
            shower: None,
            benches: Vec::new(),
            suit_locker: None,
            gangway: None,
            outside: None,
            walks_done: 0,
            crafted: Vec::new(),
            helm: None,
            filth: Filth::new(interior),
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
            food_bad: false,
            judge_food: false,
            board_veg: 0.0,
            board_slices: 0,
            knife_on_board: false,
            board_tofu: false,
            dish: Dish::Stew,
            veg: START_VEG,
            tofu: START_TOFU,
            stew: 0,
            plate_on_counter: None,
            plate_on_table: vec![None; SEATS],
            time: 0.0,
        }
    }

    /// A room laid out from somewhere else — a ship design, through
    /// `crate::aboard` — rather than by hand. Every fixture is where the
    /// layout says; the room draws none of its own shell.
    ///
    /// The room has as many beds and chairs as the layout brings — every
    /// bunk and every chair of a design, in id order, which is what makes a
    /// docked ship and the station it is docked to one room with everybody's
    /// berth in it. A layout with none of one gets a single stand-in, so an
    /// index is never out of range; the crew is cut to the beds there are
    /// (`Game::with_layout`) rather than handed a bed that does not exist.
    pub fn from_layout(layout: Layout) -> Room {
        let interior = layout.interior;
        let counter = layout.counter;
        let side = |frame: Rect| {
            // The Bim gets in from whichever side faces the middle of the room.
            if frame.center().x < interior.center().x {
                1.0
            } else {
                -1.0
            }
        };
        let mut beds: Vec<Berth> = layout
            .beds
            .iter()
            .map(|&frame| Berth::new(frame, side(frame)))
            .collect();
        if beds.is_empty() {
            beds.push(Berth::new(counter, side(counter)));
        }
        let mut chairs = layout.chairs.clone();
        if chairs.is_empty() {
            chairs.push(vec2(layout.table.center().x, layout.table.max.y + 30.0));
        }
        let seats_free = vec![None; chairs.len()];
        // The board sits on the worktop and the drawer is a face on its front,
        // as in the classic room, cut down to what the worktop has room for.
        let board = Rect::from_min_size(
            vec2(counter.min.x + 8.0, counter.min.y + 12.0),
            vec2(
                (counter.width() - 16.0).min(96.0).max(20.0),
                30.0f32.min(counter.height() - 20.0).max(10.0),
            ),
        );
        let drawer = Rect::from_min_size(
            vec2(counter.min.x + 8.0, counter.max.y - 18.0),
            vec2((counter.width() - 16.0).min(112.0).max(20.0), 16.0),
        );
        // The door is a face on the front of the tile, and the tile itself
        // is the appliance — drawn, or the dishwasher reads as half a tile.
        let dish_face = Rect::from_min_size(
            vec2(
                layout.dishwasher.min.x + 4.0,
                layout.dishwasher.max.y - 20.0,
            ),
            vec2((layout.dishwasher.width() - 8.0).max(20.0), 18.0),
        );
        let mut dishwasher = Dishwasher::at(dish_face);
        dishwasher.body = Some(layout.dishwasher);

        Room {
            bounds: layout.bounds,
            shell: false,
            others: layout.others,
            doors: layout
                .doors
                .iter()
                .map(|&(rect, along_x)| Door::new(rect, along_x))
                .collect(),
            doors_drawn: true,
            interior,
            counter,
            fridge: layout.fridge,
            stove: layout.stove,
            board,
            drawer,
            table: layout.table,
            chairs,
            beds,
            locker: layout.locker,
            broom_out: false,
            shower: layout.shower,
            helm: layout.helm,
            benches: layout.benches,
            suit_locker: layout.suit_locker,
            gangway: layout.gangway,
            outside: layout.outside,
            walks_done: 0,
            crafted: Vec::new(),
            filth: Filth::new(interior),
            bath: Bath::aboard(layout.toilet, layout.sink),
            dishwasher,
            bay: Bay::at(layout.bay, layout.bay_side),
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
            food_bad: false,
            judge_food: false,
            board_veg: 0.0,
            board_slices: 0,
            knife_on_board: false,
            board_tofu: false,
            dish: Dish::Stew,
            veg: layout.veg,
            tofu: layout.tofu,
            stew: 0,
            plate_on_counter: None,
            plate_on_table: seats_free,
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

    /// Where the Bim stands to open the broom locker: on the deck side of it,
    /// since the locker itself is set into the port bulkhead.
    pub fn locker_station(&self) -> Vec2 {
        vec2(self.locker.max.x + STAND_OFF, self.locker.center().y)
    }

    /// Whether something was made in the galley since this was last asked.
    pub fn take_judgement(&mut self) -> bool {
        core::mem::take(&mut self.judge_food)
    }

    /// A bowl has been made off the board: judge it.
    pub fn made_a_bowl(&mut self) {
        self.judge_food = true;
    }

    /// Where the Bim stands at the suit locker, or nowhere: a room without
    /// one.
    pub fn suit_locker_station(&self) -> Option<Vec2> {
        self.suit_locker.map(|(_, at)| at)
    }

    /// Which way the Bim faces at the suit locker: into it.
    pub fn suit_locker_facing(&self) -> f32 {
        match self.suit_locker {
            Some((frame, at)) => {
                let d = frame.center() - at;
                d.y.atan2(d.x)
            }
            None => 0.0,
        }
    }

    /// Which way out of the port is: from the gangway towards the spot
    /// outside.
    pub fn port_facing(&self) -> f32 {
        match (self.gangway, self.outside) {
            (Some(inside), Some(out)) => {
                let d = out - inside;
                d.y.atan2(d.x)
            }
            _ => 0.0,
        }
    }

    /// The tile the room's nav grid is phased to, where the room is laid
    /// out on tiles: a ship's is, the classic room is not. See
    /// `nav::Nav::tiled` for why that matters to a one-tile corridor.
    pub fn nav_tile(&self) -> Option<f32> {
        if self.shell {
            None
        } else {
            Some(crate::filth::TILE)
        }
    }

    /// Where the Bim stands to shower, or nowhere: a room without one.
    pub fn shower_station(&self) -> Option<Vec2> {
        self.shower.map(|(_, at)| at)
    }

    /// Which way the Bim faces under the shower: into it.
    pub fn shower_facing(&self) -> f32 {
        match self.shower {
            Some((frame, at)) => {
                let d = frame.center() - at;
                d.y.atan2(d.x)
            }
            None => 0.0,
        }
    }

    /// The thing itself: where the Bim's hand has to end up.
    fn switch_target(&self, which: Switch) -> Vec2 {
        match which {
            Switch::Hob => self.knob_pos(),
            Switch::FridgeDoor => self.fridge.center(),
            Switch::BathDoor(_) | Switch::BathLock(_) => self.bath.door.center(),
            Switch::Dishwasher => self.dishwasher.face.center(),
            Switch::Door(i, _) => self.doors[i.min(self.doors.len() - 1)].rect.center(),
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
            // A panel each side, like the bathroom door's.
            Switch::Door(i, _) => self.doors[i.min(self.doors.len() - 1)].station(from, STAND_OFF),
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
            Switch::Door(i, order) => {
                if let Some(door) = self.doors.get_mut(i) {
                    door.order(order);
                }
            }
        }
    }

    /// The powered doors a route may not be planned through: the locked
    /// ones. What the navigation grids are rebuilt with when it changes.
    pub fn locked_doors(&self) -> Vec<Rect> {
        self.doors
            .iter()
            .filter(|d| !d.passable())
            .map(|d| d.rect)
            .collect()
    }

    /// The powered doors a body walks into right now: locked and shut.
    pub fn shut_doors(&self) -> Vec<Rect> {
        self.doors
            .iter()
            .filter(|d| d.blocks())
            .map(|d| d.rect)
            .collect()
    }

    /// Which powered door a point is in, with a little slack, as a click
    /// wants; `None` off all of them.
    pub fn door_at(&self, p: Vec2) -> Option<usize> {
        self.doors
            .iter()
            .position(|d| d.rect.expand(4.0).contains(p))
    }

    /// One frame of the powered doors, for the bodies standing where
    /// `bodies` says.
    pub fn update_doors(&mut self, dt: f32, bodies: &[Vec2]) {
        for door in &mut self.doors {
            door.update(dt, bodies);
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

    /// The one hob, where the pot stands and the only thing that lights, so
    /// heat and pot can never drift apart. In the middle of the stove — a
    /// ship's hob is one tile — except on the classic room's double-width
    /// run, where it stays where the left of the old pair was: the cooking
    /// station is measured off it, and centring it there would move where
    /// the Bim stands and re-roll every probe seed.
    fn burner(&self) -> Vec2 {
        let c = self.stove.center();
        if self.stove.width() > 1.5 * self.stove.height() {
            c + vec2(-30.0, 0.0)
        } else {
            c
        }
    }

    /// How big the hob and the pot are drawn: 1 on the classic room's
    /// double-width run, whose burner always overhung the counter, and on
    /// a one-tile hob whatever fits the burner inside the tile, so the pot
    /// reads as standing on it rather than over its neighbours. Drawing
    /// only.
    fn hob_scale(&self) -> f32 {
        if self.stove.width() > 1.5 * self.stove.height() {
            1.0
        } else {
            (self.stove.width().min(self.stove.height()) / (2.0 * BURNER_R)).min(1.0)
        }
    }

    pub fn pot_pos(&self) -> Vec2 {
        self.burner()
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

    /// Berth `who`, clamped, so an index that has wandered cannot panic in the
    /// middle of a frame.
    fn berth(&self, who: usize) -> &Berth {
        &self.beds[who.min(self.beds.len() - 1)]
    }

    /// Seat `seat`, clamped the same way.
    fn seat(&self, seat: usize) -> usize {
        seat.min(self.chairs.len() - 1)
    }

    /// What is on the table in front of seat `seat`: how full the plate is,
    /// or nothing.
    pub fn plate_at(&self, seat: usize) -> Option<f32> {
        self.plate_on_table[self.seat(seat)]
    }

    pub fn set_plate_at(&mut self, seat: usize, plate: Option<f32>) {
        let seat = self.seat(seat);
        self.plate_on_table[seat] = plate;
    }

    /// Where Bim `who` stands to climb into its own bed.
    pub fn bed_station(&self, who: usize) -> Vec2 {
        self.berth(who).station()
    }

    /// Which way it turns to do it.
    pub fn bed_facing(&self, who: usize) -> f32 {
        self.berth(who).facing()
    }

    /// Where it lies once it is up there.
    pub fn bed_lie_pos(&self, who: usize) -> Vec2 {
        self.berth(who).lie_pos()
    }

    /// Bim `who`'s seat at the table, and which way it faces once it is in it
    /// — towards the table, which is the opposite way round for the two.
    pub fn chair_at(&self, seat: usize) -> Vec2 {
        self.chairs[self.seat(seat)]
    }

    pub fn chair_facing(&self, seat: usize) -> f32 {
        // The near chair sits above the table and faces down it; the far one
        // sits below and faces back up.
        if self.chair_at(seat).y < self.table.center().y {
            PI * 0.5
        } else {
            -PI * 0.5
        }
    }

    /// Where that seat's plate goes: in front of it, a little in from the
    /// table edge, so two diners are not reaching into the same dish.
    pub fn table_plate_pos(&self, seat: usize) -> Vec2 {
        let near = self.chair_at(seat).y < self.table.center().y;
        let y = if near {
            self.table.min.y + 34.0
        } else {
            self.table.max.y - 34.0
        };
        vec2(self.table.center().x, y)
    }

    /// Everything fixed that the Bim has to walk around. The bathroom door is
    /// not in here: it comes and goes, and the pathfinder handles it
    /// separately through [`Room::closed_door`].
    pub fn solids(&self) -> Vec<Rect> {
        let [north_left, north_right, west] = self.bath.solids();
        // In the order the classic room always listed them — the beds between
        // the table and the bay — because the push-out walks them in order
        // and a different order is a different seed for every probe.
        let mut all = vec![self.counter, self.stove, self.fridge, self.table];
        all.extend(self.beds.iter().map(|b| b.frame));
        all.extend([self.bay.frame, north_left, north_right, west]);
        all.extend(self.others.iter().copied());
        all
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
        } else if self.beds.iter().any(|b| b.frame.expand(4.0).contains(p)) {
            HIT_BED
        } else if self.bay.frame.expand(6.0).contains(p) {
            HIT_HYDRO
        } else if self.locker.expand(8.0).contains(p) {
            HIT_LOCKER
        } else if self.door_at(p).is_some() {
            HIT_SHIP_DOOR
        } else if self
            .shower
            .is_some_and(|(frame, _)| frame.expand(6.0).contains(p))
        {
            HIT_SHOWER
        } else {
            self.bath.hit(p)
        }
    }

    /// What is at a point, in the `SPOT_` codes. Never `None`: everywhere the
    /// pointer can be is *something*, even if that something is the hull.
    ///
    /// Order matters the same way it does in [`Room::hit`] — the hob and the
    /// board both sit within the counter run, so they are tested before it —
    /// and the heads are taken whole, because their bulkheads belong to them
    /// rather than to the room around them.
    pub fn spot(&self, p: Vec2) -> u32 {
        if self.bath.shell.contains(p) {
            return self.bath.spot(p);
        }
        if self.stove.contains(p) {
            SPOT_HOB
        } else if self.board.contains(p) {
            SPOT_BOARD
        } else if self.fridge.contains(p) {
            SPOT_FRIDGE
        } else if self.dishwasher.face.contains(p) {
            SPOT_DISHWASHER
        } else if self.counter.contains(p) {
            SPOT_WORKTOP
        } else if self.bay.frame.contains(p) {
            SPOT_BAY
        } else if self.locker.contains(p) {
            SPOT_LOCKER
        } else if self.beds.iter().any(|b| b.frame.contains(p)) {
            SPOT_BUNK
        } else if self
            .chairs
            .iter()
            .any(|&c| Rect::from_center_size(c, CHAIR_SIZE).contains(p))
        {
            SPOT_CHAIR
        } else if self.table.contains(p) {
            SPOT_TABLE
        } else if self.doors.iter().any(|d| d.rect.contains(p)) {
            SPOT_SHIP_DOOR
        } else if self.shower.is_some_and(|(frame, _)| frame.contains(p)) {
            SPOT_SHOWER
        } else if self.interior.contains(p) {
            SPOT_DECK
        } else if self.bounds.contains(p) {
            SPOT_BULKHEAD
        } else {
            SPOT_NOTHING
        }
    }

    /// Where a `SPOT_` code *is*, for ringing it on the deck.
    ///
    /// The inverse of [`Room::spot`], and only defined for the things that are
    /// one object in one place. Deck plating and bulkheads have no single
    /// rect — they are everywhere the furniture is not — so they have no
    /// answer here, and a panel that points at one gets nothing rather than a
    /// ring round the whole room.
    pub fn spot_rect(&self, spot: u32) -> Option<Rect> {
        Some(match spot {
            SPOT_WORKTOP => self.counter,
            SPOT_BOARD => self.board,
            SPOT_FRIDGE => self.fridge,
            SPOT_HOB => self.stove,
            SPOT_DISHWASHER => self.dishwasher.face,
            SPOT_TABLE => self.table,
            // Two of each, and one rect to ring. The first is the answer: a
            // readout that names "Bunk" is naming the kind of thing, and
            // ringing both would read as one enormous fixture.
            SPOT_CHAIR => Rect::from_center_size(self.chairs[0], CHAIR_SIZE),
            SPOT_BUNK => self.beds[0].frame,
            SPOT_BAY => self.bay.frame,
            SPOT_LOCKER => self.locker,
            SPOT_TOILET => self.bath.toilet,
            SPOT_BASIN => self.bath.sink,
            SPOT_DOOR => self.bath.door,
            SPOT_HELM => self.helm?,
            SPOT_BENCH => self.benches.first()?.frame,
            SPOT_SHOWER => self.shower?.0,
            SPOT_SUIT_LOCKER => self.suit_locker?.0,
            _ => return None,
        })
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

    /// Pull one bed's blanket up over a sleeper, or make it again.
    pub fn set_bed_occupied(&mut self, who: usize, occupied: bool) {
        let bed = who.min(self.beds.len() - 1);
        self.beds[bed].blanket_target = if occupied { 1.0 } else { 0.0 };
    }

    /// Clear the worktop so a fresh cook does not inherit the last one's mess.
    ///
    /// The table is *not* cleared here. Only one Bim is ever in the galley at
    /// a time — see `Game::galley_taken` — but the other may well be sitting
    /// at the table eating what it cooked twenty minutes ago, and wiping the
    /// plate out from under it would be the cook reaching across the room.
    pub fn reset_for_cooking(&mut self, dish: Dish) {
        self.board_veg = 0.0;
        self.board_slices = 0;
        self.knife_on_board = false;
        self.board_tofu = dish == Dish::Bowl;
        self.dish = dish;
        self.plate_on_counter = None;
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

    /// One thing out of the cold store, for a chain that takes them one at a
    /// time rather than by the recipe: the stew for the store is a vegetable
    /// on the first trip and a block of tofu on the second.
    pub fn take(&mut self, crop: crate::hydro::Crop) {
        match crop {
            crate::hydro::Crop::Veg => self.veg = self.veg.saturating_sub(1),
            crate::hydro::Crop::Soy => self.tofu = self.tofu.saturating_sub(1),
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

    /// Whether there is a vegetable and a block of tofu to make a stew for
    /// the store out of. One of each, which is deliberately not the table
    /// stew's two vegetables: the shelf stew is the one that uses the soy.
    pub fn can_make_stew(&self) -> bool {
        self.veg >= 1 && self.tofu >= 1
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
        for bed in &mut self.beds {
            bed.blanket += clamp(bed.blanket_target - bed.blanket, -pull, pull);
        }

        let want = if self.stove_on { 1.0 } else { 0.0 };
        // Heating up is slower than cooling down, as a real hob is.
        let rate = if self.stove_on { 1.6 } else { 0.9 };
        self.stove_heat += clamp(want - self.stove_heat, -rate * dt, rate * dt);

        // Anything in the pot slowly turns from raw greens into a stew.
        if self.pot_contents > 0.0 && self.stove_heat > 0.4 {
            let was = self.pot_cooked;
            self.pot_cooked = (self.pot_cooked + dt * 0.22).min(1.0);
            if was < 1.0 && self.pot_cooked >= 1.0 {
                self.judge_food = true;
            }
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
        if self.shell {
            self.draw_floor(list);
        }
        self.draw_counter(list);
        self.draw_stove(list);
        self.draw_fridge(list);
        self.draw_table(list);
        self.draw_locker(list);
        for bed in &self.beds {
            self.draw_bed(list, bed);
        }
        self.bay.draw(list);
        self.bath.draw(list);
        if self.doors_drawn {
            for door in &self.doors {
                door.draw(list);
            }
        }
    }

    /// The parts of the room that belong *above* the Bim: the bedding, and
    /// the shape of whoever is under it. Drawn after the character, so
    /// getting into bed actually puts the Bim under the covers.
    pub fn draw_over(&self, list: &mut DrawList) {
        for bed in &self.beds {
            self.draw_bedding_over(list, bed);
        }
    }

    fn draw_bedding_over(&self, list: &mut DrawList, bed: &Berth) {
        let m = bed.mattress();
        // The duvet reaches from just below the pillow to the foot of the
        // mattress whether the bed is made or slept in — pulled up to the chin
        // is where it ends up either way. What changes is the shape of it.
        let head = bed.frame.min.y + 52.0;
        let foot = m.max.y - 4.0;
        let mid = vec2(m.center().x, (head + foot) * 0.5);
        let width = m.width() - 6.0;
        list.rect(mid, vec2(width, foot - head), 0.0, 6.0, BLANKET);
        // A turned-down cuff along the top edge, so it reads as bedding
        // rather than a coloured panel.
        list.rect(
            vec2(m.center().x, head + 5.0),
            vec2(width, 11.0),
            0.0,
            5.0,
            BLANKET_FOLD,
        );
        // The shape of whoever is under it, rising and falling as they
        // breathe. On an empty bed this fades away to nothing. Sized off the
        // mattress, because aboard a ship the bed is a tile wide and the
        // classic room's is nearly twice that.
        if bed.blanket > 0.01 {
            let breath = 1.0 + 0.03 * (self.time * 1.1).sin();
            list.ellipse(
                vec2(m.center().x, head + 0.33 * (foot - head)),
                vec2(0.6 * m.width(), 0.47 * m.height() * breath),
                0.0,
                BLANKET_FOLD.alpha(0.33 * bed.blanket),
            );
        }
        // Creases running down towards the foot.
        for side in [-1.0f32, 1.0] {
            list.rect(
                vec2(m.center().x + side * 0.22 * m.width(), mid.y + 8.0),
                vec2(3.5, 0.68 * (foot - head)),
                0.0,
                2.0,
                BED_SHADOW.alpha(0.12),
            );
        }
    }

    /// A bed seen from above: a frame with a headboard standing proud at the
    /// head end and a footboard at the other, a mattress in it, and a pillow.
    /// The bedding goes on in [`Room::draw_over`], after the sleeper.
    fn draw_bed(&self, list: &mut DrawList, bed: &Berth) {
        let f = bed.frame;
        let m = bed.mattress();

        // The shadow it throws on the deck.
        list.rect(
            f.center() + vec2(3.0, 4.0),
            f.size() + vec2(7.0, 7.0),
            0.0,
            10.0,
            BED_SHADOW,
        );
        // Frame, then mattress, with a seam round the mattress where it meets
        // the rail.
        list.rect(f.center(), f.size(), 0.0, 8.0, BED_FRAME);
        list.rect(m.center(), m.size(), 0.0, 5.0, MATTRESS);
        list.stroke_rect(
            m.center(),
            m.size(),
            0.0,
            5.0,
            1.5,
            MATTRESS_SEAM.alpha(0.5),
        );

        // Headboard and footboard: a board across each end, a little wider
        // than the frame, the head one the taller. Seen from above they are
        // the tops of two boards, lit along the edge that faces the room.
        for (y, depth) in [(f.min.y + 5.0, 14.0), (f.max.y - 4.0, 9.0)] {
            let at = vec2(f.center().x, y);
            list.rect(at, vec2(f.width() + 6.0, depth), 0.0, 3.0, BED_BOARD);
            list.rect(
                at + vec2(
                    0.0,
                    if y < f.center().y {
                        depth * 0.5 - 1.5
                    } else {
                        1.5 - depth * 0.5
                    },
                ),
                vec2(f.width() + 6.0, 2.5),
                0.0,
                1.0,
                BED_BOARD_EDGE,
            );
        }

        // Pillow at the head end, placed off the lying position so the Bim's
        // head always lands on it. Its height gives way on a narrow bed
        // before its width does.
        let pillow = bed.pillow_pos();
        let pillow_size = vec2(m.width() - 14.0, 32.0f32.min(0.22 * m.height()));
        list.rect(pillow, pillow_size, 0.0, 9.0, PILLOW);
        list.line(
            pillow + vec2(0.0, -0.4 * pillow_size.y),
            pillow + vec2(0.0, 0.4 * pillow_size.y),
            1.5,
            MATTRESS_SEAM.alpha(0.35),
        );
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
        let at = self.burner();
        let k = self.hob_scale();
        list.circle(at, BURNER_R * k, BURNER);
        // Induction coils, etched into the glass and lit low even when
        // cold, so a dead hob still looks like equipment.
        for ring in 0..3 {
            list.ring(at, (18.0 + ring as f32 * 11.0) * k, 1.5, GLOW.alpha(0.16));
        }
        list.ring(at, 38.0 * k, 2.0, STOVE_TOP);
        list.ring(at, 42.0 * k, 1.5, GLOW.alpha(0.30));
        if hot > 0.01 {
            list.circle(at, 40.0 * k, BURNER_HOT.alpha(0.7 * hot * flicker));
            list.ring(at, 30.0 * k, 3.0, BURNER_HOT.alpha(0.9 * hot));
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
        // A shade smaller than the hob it stands on, and scaled with it.
        let k = POT_SIZE * self.hob_scale();
        list.circle(at, 50.0 * k, POT);
        list.ring(at, 46.0 * k, 4.0 * k, POT_RIM);
        // Handles either side.
        let handle = vec2(14.0 * k, 7.0 * k);
        list.rect(at + vec2(-30.0 * k, 0.0), handle, 0.0, 3.0 * k, POT_RIM);
        list.rect(at + vec2(30.0 * k, 0.0), handle, 0.0, 3.0 * k, POT_RIM);

        if self.pot_contents <= 0.0 {
            list.circle(at, 40.0 * k, Color::rgb(0.10, 0.11, 0.12));
            return;
        }

        let broth = Color::rgb(
            lerp(BROTH_RAW.r, BROTH_DONE.r, self.pot_cooked),
            lerp(BROTH_RAW.g, BROTH_DONE.g, self.pot_cooked),
            lerp(BROTH_RAW.b, BROTH_DONE.b, self.pot_cooked),
        );
        let surface = 40.0 * k * (0.6 + 0.4 * self.pot_contents);
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
            list.circle(p, (4.0 + 3.0 * a) * k, broth.alpha(0.3 + 0.45 * a));
        }
        // Steam. Seen from above it spreads out from the pot rather than
        // rising, which also keeps it from drifting through the wall.
        for i in 0..4 {
            let t = (self.time * 0.5 + i as f32 * 0.25) % 1.0;
            let a = i as f32 * (TAU / 4.0) + self.time * 0.35;
            let p = at + Vec2::from_angle(a) * (t * 30.0 * k);
            list.circle(
                p,
                (14.0 + t * 26.0) * k,
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
            // Interior, revealed as the door swings clear. Everything in it
            // is laid out off this rect rather than at fixed offsets, so a
            // one-tile cold store aboard keeps its stock inside itself — at
            // the classic room's offsets, a 52-unit fridge had its shelves
            // running out through its side.
            let inner =
                Rect::from_center_size(f.center() + vec2(0.0, 3.0), f.size() - vec2(10.0, 10.0));
            list.rect(inner.center(), inner.size(), 0.0, 3.0, FRIDGE_IN);
            // Two shelves, stocked with odds and ends: four places a shelf,
            // spaced across it, and each item sized to its place.
            const PER_SHELF: usize = 4;
            let step = (inner.width() - 6.0) / PER_SHELF as f32;
            let r = (step * 0.42).min(10.0);
            for row in 0..2 {
                let y = inner.min.y + inner.height() * (0.36 + 0.36 * row as f32);
                list.rect(
                    vec2(inner.center().x, y),
                    vec2(inner.width() - 6.0, 2.5),
                    0.0,
                    1.0,
                    SHELF,
                );
                // Eight places on the shelves for twenty items, so each one
                // stands for a couple and the shelves visibly empty out.
                let stock = (self.veg + self.tofu + self.stew) as f32;
                let shown = (stock * 8.0 / SHELF_FULL).ceil().min(8.0) as usize;
                for i in 0..PER_SHELF {
                    if row * PER_SHELF + i >= shown {
                        continue;
                    }
                    let x = inner.min.x + 3.0 + step * (i as f32 + 0.5);
                    let at = vec2(x, y - r * 1.1 + 1.0);
                    let c = STOCK[(row * PER_SHELF + i) % STOCK.len()];
                    list.ellipse(at, vec2(r, r * 1.1), 0.0, c.alpha(self.fridge_door));
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

    /// The broom locker: a shallow door in the port bulkhead, with the head of
    /// the broom showing through the vent when it is stowed and an empty
    /// recess when it is out. Whether the Bim has it is the whole of what the
    /// door says, which is as much as a cupboard seen from above can carry.
    fn draw_locker(&self, list: &mut DrawList) {
        let l = self.locker;
        list.rect(l.center(), l.size(), 0.0, 3.0, PANEL);
        list.stroke_rect(l.center(), l.size(), 0.0, 3.0, 1.5, PANEL_EDGE);
        // A louvred vent down the door, which is what makes it a cupboard
        // rather than a panel.
        for i in 0..4 {
            let y = lerp(l.min.y + 14.0, l.max.y - 14.0, i as f32 / 3.0);
            list.rect(
                vec2(l.center().x, y),
                vec2(l.width() - 9.0, 2.0),
                0.0,
                1.0,
                PANEL_EDGE.alpha(0.55),
            );
        }
        // The broom behind it, or the dark of an empty cupboard.
        if self.broom_out {
            list.rect(
                l.center(),
                vec2(l.width() - 11.0, l.height() - 26.0),
                0.0,
                2.0,
                HULL,
            );
        } else {
            list.rect(
                vec2(l.center().x, l.center().y),
                vec2(3.5, l.height() - 24.0),
                0.0,
                1.5,
                BROOM_POLE,
            );
            list.rect(
                vec2(l.center().x, l.max.y - 17.0),
                vec2(l.width() - 8.0, 12.0),
                0.0,
                2.0,
                BROOM_HEAD,
            );
        }
        // The handle, on the deck side.
        list.rect(
            vec2(l.max.x - 3.0, l.center().y),
            vec2(2.5, 18.0),
            0.0,
            1.0,
            HANDLE,
        );
    }

    fn draw_table(&self, list: &mut DrawList) {
        // Chairs first: a Bim sits on top of one, so each has to be wide
        // enough that a seated Bim does not overhang the sides. The backrest
        // goes on the side away from the table, which is opposite ways round
        // for the two of them.
        for &ch in &self.chairs {
            let back = if ch.y < self.table.center().y {
                -1.0
            } else {
                1.0
            };
            // The backrest stays inside the tile too: its far edge is 25
            // out from the centre, a unit short of the tile's 26.
            let spine = ch + vec2(0.0, back * 21.0);
            list.rect(ch, CHAIR_SIZE, 0.0, 8.0, CHAIR);
            list.stroke_rect(ch, vec2(40.0, 34.0), 0.0, 6.0, 1.5, GLOW.alpha(0.28));
            list.rect(spine, vec2(50.0, 8.0), 0.0, 4.0, TABLE_EDGE);
            list.rect(spine, vec2(32.0, 2.0), 0.0, 1.0, GLOW.alpha(0.5));
        }

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

        for seat in 0..self.chairs.len() {
            let Some(fill) = self.plate_on_table[seat] else {
                continue;
            };
            let p = self.table_plate_pos(seat);
            draw_plate(list, p, fill, self.pot_cooked, self.dish);
            // Cutlery laid either side of the plate.
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

/// A pot of stew to go on the shelf: a lidded tub, seen from above, with a
/// ring of what is in it showing round the lid. Carried by the Bim between
/// the hob and the cold store, and back again when it is warmed up.
pub fn draw_stew_tub(list: &mut DrawList, at: Vec2, rot: f32) {
    list.circle(at, 15.0, POT_RIM);
    list.circle(at, 12.5, BROTH_DONE);
    list.circle(at, 9.0, POT);
    // The lid's handle, across the top.
    list.rect(at, vec2(10.0, 3.0), rot, 1.5, POT_RIM);
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
