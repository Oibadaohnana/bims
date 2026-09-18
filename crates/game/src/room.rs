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
use crate::galley::{Fridge, Hob, Locker, Worktop};
use crate::hydro::Bay;
use crate::math::{PI, Rect, TAU, Vec2, clamp, lerp, vec2};
use crate::sight::Sight;

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

pub const TILE: f32 = 52.0;

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

/// Plates, which live in the chopping board's drawer: how many it holds,
/// and how many a room starts with. A plate is an item — one comes out of
/// the drawer for every meal served, goes through the dishwasher and comes
/// back — so the count is what is *clean and put away*; the rest are on
/// the table, in a Bim's hands or in the rack. Twenty is the playtest
/// ship's, and every room's, since nothing on a manifest carries plates.
pub const PLATE_DRAWER: u32 = 40;
pub const START_PLATES: u32 = 20;

/// Bandages the classic room starts with, so `bims room` can try the
/// dressing without a drug lab. Aboard, the count is the hold's and the
/// world sets it — see `Room::bandages`.
pub const BANDAGES_AT_DAWN: u32 = 3;

/// Something the Bim can walk up to and work with its hands.
///
/// Nothing aboard is remote-controlled: asking for any of these starts an
/// errand that walks the Bim over, and the state only changes at the moment
/// its hand arrives. That is why each one is a place as well as an effect.
#[derive(Clone, Copy, PartialEq)]
pub enum Switch {
    /// By index into `Room::hobs`, `Room::fridges` and
    /// `Room::dishwashers`: the one the click landed on.
    Hob(usize),
    FridgeDoor(usize),
    /// Wanted open, and wanted locked. These two carry what was asked for
    /// rather than being read off the door when the hand arrives, because the
    /// act of asking can change the door itself: dropping a trip to the heads
    /// unlocks and opens the door the Bim locked behind itself, so a toggle
    /// would find the lock already off and put it straight back on. A Bim
    /// asked to unlock while sitting on the pan locked itself back in.
    BathDoor(bool),
    BathLock(bool),
    Dishwasher(usize),
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
/// One of the crew — a body, not a fixture: `Game::hit_at` answers it
/// before asking the room, and `Game::hit_bim` says which. The menu on it
/// is the bandages, for the player's Bim to dress whoever was clicked.
pub const HIT_BIM: u32 = 11;

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
    /// Whether the bed lies along `x` — a bunk aboard turned a quarter —
    /// rather than along `y` like the classic room's. The head is at the
    /// low end either way, and everything about the bed is worked out in
    /// its own frame, `across` and `along`, so a bed lying down is the same
    /// bed on its side and not a different picture.
    lying: bool,
    /// 0 with the bed made, 1 with the blanket pulled up over a sleeper.
    blanket: f32,
    blanket_target: f32,
}

impl Berth {
    fn new(frame: Rect, side: f32) -> Berth {
        Berth {
            frame,
            side,
            lying: frame.width() > frame.height(),
            blanket: 0.0,
            blanket_target: 0.0,
        }
    }

    /// The bed's own frame to the room's: `across` from the bed's centre
    /// line, `along` from its head end. A bed standing up is the classic
    /// arithmetic — `x` off the centre, `y` off the top.
    fn at(&self, across: f32, along: f32) -> Vec2 {
        if self.lying {
            vec2(self.frame.min.x + along, self.frame.center().y + across)
        } else {
            vec2(self.frame.center().x + across, self.frame.min.y + along)
        }
    }

    /// A size given across and along, as the room's width and height.
    fn size(&self, across: f32, along: f32) -> Vec2 {
        if self.lying {
            vec2(along, across)
        } else {
            vec2(across, along)
        }
    }

    /// How wide and how long the bed is, in its own frame.
    fn breadth(&self) -> f32 {
        if self.lying {
            self.frame.height()
        } else {
            self.frame.width()
        }
    }

    fn length(&self) -> f32 {
        if self.lying {
            self.frame.width()
        } else {
            self.frame.height()
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
        self.at(
            self.side * (self.breadth() / 2.0 + 28.0),
            self.length() - 56.0,
        )
    }

    /// Which way the Bim turns to get in — towards the bed, so it climbs in
    /// facing it rather than backwards.
    fn facing(&self) -> f32 {
        match (self.lying, self.side > 0.0) {
            (false, true) => PI,
            (false, false) => 0.0,
            (true, true) => -PI * 0.5,
            (true, false) => PI * 0.5,
        }
    }

    /// Which way the sleeper faces: towards the head of the bed.
    fn lie_facing(&self) -> f32 {
        if self.lying { PI } else { -PI * 0.5 }
    }

    /// Where the Bim lies, head towards the pillow. Everything else on the
    /// bed is placed relative to this, so the head cannot drift off the
    /// pillow when the bed moves.
    fn lie_pos(&self) -> Vec2 {
        self.at(0.0, 46.0)
    }

    fn pillow_pos(&self) -> Vec2 {
        self.at(0.0, 34.0)
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
    /// Everything a line of sight stops at that is not a door: the walls,
    /// the tall parts, and every tile inside the deck's box that has no
    /// deck — `PartDef::blocks_sight` is the rule. The doors are their own
    /// list, since a door is in the way only while it is shut.
    pub opaque: Vec<Rect>,
    /// Every tile of the hull, frame and all: what a body outside walks
    /// round, and what the fog of what the crew cannot see is drawn over.
    /// Empty in the classic room, which has no outside and whose fog
    /// covers its whole box.
    pub hull: Vec<Rect>,
    /// The shelves, each its footprint and where the Bim stands at it: where
    /// a load of materials for a construction site is fetched from. Solids
    /// in `others` as well — the room has no picture for a shelf; the ship
    /// draws it. Empty in the classic room.
    pub shelves: Vec<(Rect, Vec2)>,
    /// The powered doors, each its opening and whether its leaves slide
    /// along `x`. None in the classic room, whose one door is the heads'.
    pub doors: Vec<(Rect, bool)>,
    /// The airlocks, each its footprint. Walked onto, never a solid, and
    /// the ship draws them; they are here for sight alone, which they
    /// stop like a door with nobody at it. Empty in the classic room.
    pub airlocks: Vec<Rect>,
    /// Every fixture beyond the first of its kind, all of them worked: a
    /// hob the crew built is a hob somebody cooks on. See [`More`].
    pub more: More,
    /// What the room only draws: a second table, a basin with no toilet of
    /// its own. Solids in `others` still. See [`Still`].
    pub extras: Vec<(Still, Rect)>,
    /// What is in the cold store to begin with.
    pub veg: u32,
    pub tofu: u32,
}

/// Which side a Bim gets into a bunk from: whichever faces the middle of
/// the room.
fn bed_side(frame: Rect, interior: Rect) -> f32 {
    if frame.center().x < interior.center().x {
        1.0
    } else {
        -1.0
    }
}

/// A layout's seats, with a stand-in below the table for a layout that
/// brought none, so a seat index is never out of range.
fn layout_chairs(layout: &Layout) -> Vec<Vec2> {
    let mut chairs = layout.chairs.clone();
    if chairs.is_empty() {
        chairs.push(vec2(layout.table.center().x, layout.table.max.y + 30.0));
    }
    chairs
}

/// The galley's three faces, off the worktop and the dishwasher's tile:
/// the board on the worktop and the drawer a face on its front, as in the
/// classic room, cut down to what the worktop has room for; and the
/// dishwasher's door, a face on the front of its tile — the tile itself is
/// the appliance, or the dishwasher reads as half a tile.
fn galley_faces(counter: Rect, dishwasher: Rect) -> (Rect, Rect, Rect) {
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
    let dish_face = Rect::from_min_size(
        vec2(dishwasher.min.x + 4.0, dishwasher.max.y - 20.0),
        vec2((dishwasher.width() - 8.0).max(20.0), 18.0),
    );
    (board, drawer, dish_face)
}

/// One side of the chopping board: what was put down on it, how much of it
/// is still whole, and how many pieces it has been cut into. Tofu is a
/// block and chops into cubes; a vegetable is a lump and chops into rounds.
#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub struct Cut {
    pub tofu: bool,
    pub whole: f32,
    pub pieces: u32,
}

impl Cut {
    pub fn is_empty(&self) -> bool {
        self.whole <= 0.0 && self.pieces == 0
    }
}

/// Every fixture beyond the first of its kind, in id order, each its
/// footprint — and for a shower or a bay the spot it is worked from, for a
/// toilet the basin nearest it. All of them are worked: a chain picks the
/// closest free one of each kind it needs (`task::Picks`). The first of
/// each is the layout's own field, since a layout always has one, if only
/// a stand-in.
#[derive(Clone, Default, Debug)]
pub struct More {
    pub worktops: Vec<Rect>,
    pub hobs: Vec<Rect>,
    pub fridges: Vec<Rect>,
    pub dishwashers: Vec<Rect>,
    pub lockers: Vec<Rect>,
    pub showers: Vec<(Rect, Vec2)>,
    pub bays: Vec<(Rect, Vec2)>,
    /// A heads apiece: the toilet, and the basin nearest it. A basin may
    /// be two heads'.
    pub heads: Vec<(Rect, Rect)>,
}

/// A kind of fixture the room only draws, when it is not one the room
/// works: a second table — every chair is seated already, and a table is
/// what stands between them — and a basin that is no toilet's. Solids the
/// room draws as themselves, standing still. See `Layout::extras` and
/// [`Stills`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Still {
    Table,
    Basin,
}

/// The extras, ready to draw. A basin is drawn by a `Bath` given the basin
/// for both fittings and asked for the one; a table is a rect.
#[derive(Default)]
pub struct Stills {
    pub tables: Vec<Rect>,
    pub basins: Vec<Bath>,
}

impl Stills {
    fn from_extras(extras: &[(Still, Rect)]) -> Stills {
        let mut stills = Stills::default();
        for &(kind, frame) in extras {
            match kind {
                Still::Table => stills.tables.push(frame),
                Still::Basin => stills.basins.push(Bath::aboard(frame, frame)),
            }
        }
        stills
    }
}

/// Which of the room's fixtures a picture of it is to have.
///
/// A room laid out from a design that is not finished — the designer's,
/// while the ship is being built — has a layout that puts every fixture
/// it has not got on the worktop, and the worktop on the first deck tile
/// (see `aboard::layout_of`). Drawn whole, that is a heap of galley on one
/// tile; so the designer says which are really there and gets a picture of
/// those. The game's room is always whole and draws everything.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct Fixtures {
    pub counter: bool,
    pub stove: bool,
    pub fridge: bool,
    pub dishwasher: bool,
    pub table: bool,
    pub chairs: bool,
    pub locker: bool,
    pub beds: bool,
    pub bay: bool,
    pub toilet: bool,
    pub basin: bool,
    pub doors: bool,
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
    /// The airlocks' footprints, for sight: a passage with nobody at it is
    /// a wall to the eyes, exactly as a powered door with its leaves shut.
    /// The ship draws them and the pathfinder walks through them.
    pub airlocks: Vec<Rect>,
    /// The fixtures beyond the ones the room works, drawn as themselves.
    pub stills: Stills,
    /// Whether this room draws its doors. A station's room kept open for
    /// its pictures while the ship is docked does not: the joined room has
    /// the same doors, with the people walking through them, and two
    /// pictures of one door in two states is a door in two states.
    pub doors_drawn: bool,
    pub interior: Rect,
    /// The galley, a list of each fixture: every worktop, hob and cold
    /// store aboard, the layout's own first. A chain picks the closest one
    /// nobody else has when it first needs one — see `task::Picks` — so a
    /// second hob is a hob somebody cooks on. Never empty: a layout without
    /// one gets a stand-in on the worktop, like a bed.
    pub worktops: Vec<Worktop>,
    pub hobs: Vec<Hob>,
    pub fridges: Vec<Fridge>,
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
    /// Every other toilet aboard, each a bare heads of its own with the
    /// nearest basin, in id order. See `Room::bath_at`.
    pub more_baths: Vec<Bath>,
    /// What the crew can see of the room, shared. Its walls are the
    /// layout's; the shut doors are added at every trace. See `crate::sight`.
    pub sight: Sight,
    /// The galley dishwashers, which run on the clock rather than on the
    /// Bim. Never empty, like the worktops.
    pub dishwashers: Vec<Dishwasher>,
    /// The state of the deck, tile by tile: what has been spilt on it and
    /// where. It lives here because the deck *is* the room — and because the
    /// chains in `task.rs` have to be able to ask where the dirt is, and a
    /// task is handed the room and nothing else.
    /// The broom lockers: in the classic room a shallow door set into the
    /// port bulkhead, between the foot of the first bunk and the hydroponic
    /// bay. Not a solid — it is *in* the wall, and a body cannot get within
    /// its depth of the bulkhead anyway, so the nav grid is untouched by it.
    /// A broom in each; never empty.
    pub lockers: Vec<Locker>,
    /// The showers, each its footprint and the spot in front of it. See
    /// `Layout::shower`. Empty in the classic room.
    pub showers: Vec<(Rect, Vec2)>,
    /// The helm's footprint, if there is one, for ringing. See `Layout::helm`.
    pub helm: Option<Rect>,
    /// The workstations. See [`Bench`] and `Layout::benches`.
    pub benches: Vec<Bench>,
    /// The suit locker, the deck inside the port and the spot outside it.
    /// See `Layout::suit_locker`, `Layout::gangway`, `Layout::outside`.
    pub suit_locker: Option<(Rect, Vec2)>,
    pub gangway: Option<Vec2>,
    pub outside: Option<Vec2>,
    /// Every tile of the hull, for the outside grid. See `Layout::hull`.
    pub hull: Vec<Rect>,
    /// The shelves, footprint and stand spot each. See `Layout::shelves`.
    pub shelves: Vec<(Rect, Vec2)>,
    /// The construction sites the world wants worked, this step: what each
    /// still wants carried to it, or that it is to be built. Set by
    /// `Game::set_build_orders`; empty in the classic room and while the
    /// ship is under way. See `crate::game::Build`.
    pub builds: Vec<crate::game::Build>,
    /// Who may put a suit on and go out to a site outside the hull, by crew
    /// index — the world's say, like `eva_allowed`, and set with the sites.
    pub suit_ok: Vec<bool>,
    /// What the construction chains did since the world last asked, each
    /// drained by the world every step — `Game::take_picked` and friends.
    /// The room moves no materials: it says a load of `units` of resource
    /// `resource` was taken off a shelf for `site`, that the load reached
    /// the site, that a load was given up short of it, or that the site was
    /// built, and the world moves the count. `(site, resource, units)`,
    /// `site`, `site` and `site` respectively.
    pub picked: Vec<(u32, u32, u32)>,
    pub dropped: Vec<u32>,
    pub returned: Vec<u32>,
    pub built: Vec<u32>,
    /// The outside, as the world says it is this step: every rock tile as
    /// a solid, the marked ones as where a walk goes, and a version that
    /// moves when the rocks do. Set by `Game::set_eva`; empty in the
    /// classic room and away from a site.
    pub rocks: Vec<Rect>,
    pub rock_targets: Vec<Vec2>,
    pub rocks_version: u64,
    /// Who may be out there, by crew index: a Bim past the dose limit is
    /// sent home from the next rock. Set with the rocks.
    pub eva_allowed: Vec<bool>,
    /// How long one rock takes to mine, in game minutes. The world's
    /// number, carried because the room has no table of its own.
    pub tile_minutes: f32,
    /// The middles of the rocks mined since the world last asked. The
    /// world drains it with `Game::take_mined` and moves what each yields
    /// onto the shelf; the room never touches a resource itself.
    pub mined: Vec<Vec2>,
    /// Walks outside finished since the world last asked. The world drains
    /// it with `Game::take_walks` and says what came back.
    pub walks_done: u32,
    /// Recipes finished at a bench since the world last asked — indices into
    /// `shipdesign::recipes::RECIPES`. The world drains it every step with
    /// `Game::take_crafted` and moves the cargo; the room keeps no stock of
    /// ore or metal and never will.
    pub crafted: Vec<u32>,
    pub filth: Filth,
    /// The hydroponic bays, every one aboard, the first the layout's own.
    /// They live here because they are furniture — something to walk
    /// round, click on and draw — but the game drives their clock, because
    /// the target they work to is the manager's rather than the room's.
    /// Never empty: a layout without one gets a stand-in, like a bed.
    pub bays: Vec<Bay>,

    /// What the meal in progress is, for the plate and the bowl.
    pub dish: Dish,
    /// The cold store, counted in the two things that go into a meal and
    /// the one thing that comes out of the galley ready: pots of stew,
    /// cooked ahead to the manager's target and warmed up when somebody is
    /// hungry. And fibre, the one crop nobody eats — what the drug lab
    /// makes a bandage of — in the cold store beside them, since it is a
    /// crop, and counted apart.
    pub veg: u32,
    pub tofu: u32,
    pub stew: u32,
    pub fibre: u32,
    /// Fibre put away since the world last asked. The world drains it every
    /// step (`Game::take_harvested_fibre`) and moves it into the hold, the
    /// way it takes a finished recipe: the room's own count above is the
    /// bay's picture of the shelf, the hold is the record.
    pub harvested_fibre: u32,

    /// Bandages to hand. Aboard, the world sets it every step off the hold
    /// (`Game::set_bandages`) and takes back what was used
    /// (`Game::take_bandages_used`); the classic room starts with a few so
    /// the chain can be tried there.
    pub bandages: u32,
    pub bandages_used: u32,
    /// Every dressing finished since the game last looked — `(patient,
    /// part code)`, pushed by the bandage chain as its hands come off the
    /// patient. The game drains it after everybody has moved and does the
    /// dressing: the room has the bandages but the patient's body is a
    /// `Bim`, which the room never holds.
    pub dressed: Vec<(usize, u32)>,
    /// Where every one of the crew stands this step, by index — `None` for
    /// one dead or outside. The one thing about the crew the room is told,
    /// set by the game at the top of every step, so that a chain walking
    /// to a *crewmate* — the bandage — can pick its spot as the walk is
    /// entered, the way a sweep picks its tile. Nothing else reads it.
    pub crew: Vec<Option<Vec2>>,

    /// Clean plates in the drawer. See [`PLATE_DRAWER`].
    pub plates: u32,

    /// Plates on the table, carrying how full they are; the one on a
    /// worktop is that worktop's.
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
        let bounds = Rect::from_min_size(Vec2::ZERO, vec2(ROOM_W, ROOM_H));
        let bath = Bath::new(interior);

        Room {
            bounds,
            shell: true,
            others: Vec::new(),
            doors: Vec::new(),
            doors_drawn: true,
            airlocks: Vec::new(),
            stills: Stills::default(),
            interior,
            worktops: vec![Worktop::new(counter, board, drawer)],
            hobs: vec![Hob::new(stove)],
            fridges: vec![Fridge::new(fridge)],
            table,
            chairs: chairs.to_vec(),
            beds: beds.into(),
            lockers: vec![Locker::new(locker)],
            showers: Vec::new(),
            benches: Vec::new(),
            suit_locker: None,
            gangway: None,
            outside: None,
            hull: Vec::new(),
            shelves: Vec::new(),
            builds: Vec::new(),
            suit_ok: Vec::new(),
            picked: Vec::new(),
            dropped: Vec::new(),
            returned: Vec::new(),
            built: Vec::new(),
            rocks: Vec::new(),
            rock_targets: Vec::new(),
            rocks_version: 0,
            eva_allowed: Vec::new(),
            tile_minutes: 0.0,
            mined: Vec::new(),
            walks_done: 0,
            crafted: Vec::new(),
            helm: None,
            filth: Filth::new(interior),
            // The heads' walls are the one thing inside the classic room a
            // Bim cannot see past; the shell round it is outside `interior`.
            sight: Sight::new(bounds, interior, TILE, &bath.solids(), &[]),
            bath,
            more_baths: Vec::new(),
            dishwashers: vec![Dishwasher::new(counter)],
            bays: vec![Bay::new(interior)],
            dish: Dish::Stew,
            veg: START_VEG,
            tofu: START_TOFU,
            stew: 0,
            fibre: 0,
            harvested_fibre: 0,
            bandages: BANDAGES_AT_DAWN,
            bandages_used: 0,
            dressed: Vec::new(),
            crew: Vec::new(),
            plates: START_PLATES,
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
        let mut beds: Vec<Berth> = layout
            .beds
            .iter()
            .map(|&frame| Berth::new(frame, bed_side(frame, interior)))
            .collect();
        if beds.is_empty() {
            beds.push(Berth::new(counter, bed_side(counter, interior)));
        }
        let chairs = layout_chairs(&layout);
        let seats_free = vec![None; chairs.len()];
        let (board, drawer, dish_face) = galley_faces(counter, layout.dishwasher);
        let mut dishwasher = Dishwasher::at(dish_face);
        dishwasher.body = Some(layout.dishwasher);
        let sight = Sight::new(layout.bounds, interior, TILE, &layout.opaque, &layout.hull);

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
            airlocks: layout.airlocks,
            stills: Stills::from_extras(&layout.extras),
            interior,
            worktops: core::iter::once(Worktop::new(counter, board, drawer))
                .chain(layout.more.worktops.iter().map(|&c| {
                    let (board, drawer, _) = galley_faces(c, c);
                    Worktop::new(c, board, drawer)
                }))
                .collect(),
            hobs: core::iter::once(layout.stove)
                .chain(layout.more.hobs.iter().copied())
                .map(Hob::new)
                .collect(),
            fridges: core::iter::once(layout.fridge)
                .chain(layout.more.fridges.iter().copied())
                .map(Fridge::new)
                .collect(),
            table: layout.table,
            chairs,
            beds,
            lockers: core::iter::once(layout.locker)
                .chain(layout.more.lockers.iter().copied())
                .map(Locker::new)
                .collect(),
            showers: layout
                .shower
                .into_iter()
                .chain(layout.more.showers.iter().copied())
                .collect(),
            helm: layout.helm,
            benches: layout.benches,
            suit_locker: layout.suit_locker,
            gangway: layout.gangway,
            outside: layout.outside,
            hull: layout.hull,
            shelves: layout.shelves,
            builds: Vec::new(),
            suit_ok: Vec::new(),
            picked: Vec::new(),
            dropped: Vec::new(),
            returned: Vec::new(),
            built: Vec::new(),
            rocks: Vec::new(),
            rock_targets: Vec::new(),
            rocks_version: 0,
            eva_allowed: Vec::new(),
            tile_minutes: 0.0,
            mined: Vec::new(),
            walks_done: 0,
            crafted: Vec::new(),
            filth: Filth::new(interior),
            sight,
            bath: Bath::aboard(layout.toilet, layout.sink),
            more_baths: layout
                .more
                .heads
                .iter()
                .map(|&(toilet, sink)| Bath::aboard(toilet, sink))
                .collect(),
            dishwashers: core::iter::once(dishwasher)
                .chain(layout.more.dishwashers.iter().map(|&body| {
                    let (_, _, face) = galley_faces(body, body);
                    let mut washer = Dishwasher::at(face);
                    washer.body = Some(body);
                    washer
                }))
                .collect(),
            bays: core::iter::once((layout.bay, layout.bay_side))
                .chain(layout.more.bays.iter().copied())
                .map(|(frame, side)| Bay::at(frame, side))
                .collect(),
            dish: Dish::Stew,
            veg: layout.veg,
            tofu: layout.tofu,
            stew: 0,
            fibre: 0,
            harvested_fibre: 0,
            // None until the world says: aboard, the bandages are the
            // hold's, and a design carries them on its manifest.
            bandages: 0,
            bandages_used: 0,
            dressed: Vec::new(),
            crew: Vec::new(),
            plates: START_PLATES,
            plate_on_table: seats_free,
            time: 0.0,
        }
    }

    /// The same room laid out again from a design that has changed under
    /// it — a part built — with everything that is **state** kept. The
    /// crew are not in here, but the dirt on the deck is, and so are the
    /// crops in the bay, the doors' locks and leaves, the beds' blankets,
    /// the plates on the table and what is in the cold store; a room built
    /// afresh would lose the lot for the sake of one wall. Geometry is taken
    /// from the new layout; anything whose place did not move keeps what it
    /// was doing, and anything new goes on the end, so a berth or a seat
    /// index the crew already hold still names the same bed and the same
    /// chair. The nav grids and the blockers are the game's to rebuild —
    /// `Game::relayout` — since only it knows about the locked doors.
    pub fn relayout(&mut self, layout: Layout) {
        let interior = layout.interior;
        let counter = layout.counter;
        self.bounds = layout.bounds;
        // The deck: the dirt on it comes across by position, so a deck that
        // grew a tile keeps every stain where it was.
        if interior != self.interior {
            self.filth = self.filth.resized(interior);
        }
        self.interior = interior;
        let chairs = layout_chairs(&layout);
        self.others = layout.others;
        // The walls moved: the mask is traced again from the new ones the
        // first time anybody looks.
        self.sight = Sight::new(layout.bounds, interior, TILE, &layout.opaque, &layout.hull);
        // The doors, by opening: one that was there keeps its state, one
        // that is new starts shut.
        let mut doors: Vec<Door> = Vec::with_capacity(layout.doors.len());
        let mut old_doors = core::mem::take(&mut self.doors);
        for (rect, along_x) in layout.doors.iter().copied() {
            match old_doors.iter().position(|d| d.rect == rect) {
                Some(i) => doors.push(old_doors.swap_remove(i)),
                None => doors.push(Door::new(rect, along_x)),
            }
        }
        self.doors = doors;
        self.airlocks = layout.airlocks;
        self.stills = Stills::from_extras(&layout.extras);
        // Every galley fixture, by frame: one that was there keeps what it
        // was doing — the slices on its board, the stew in its pot, its
        // door half open — and one that is new starts fresh. The first of
        // each is the layout's own, which may have moved onto a new part;
        // by frame it is then a new one, which is what it is.
        let (board, drawer, dish_face) = galley_faces(counter, layout.dishwasher);
        let mut old = core::mem::take(&mut self.worktops);
        self.worktops = core::iter::once((counter, board, drawer))
            .chain(layout.more.worktops.iter().map(|&c| {
                let (b, d, _) = galley_faces(c, c);
                (c, b, d)
            }))
            .map(|(c, b, d)| match old.iter().position(|w| w.frame == c) {
                Some(i) => old.swap_remove(i),
                None => Worktop::new(c, b, d),
            })
            .collect();
        let mut old = core::mem::take(&mut self.hobs);
        self.hobs = core::iter::once(layout.stove)
            .chain(layout.more.hobs.iter().copied())
            .map(|s| match old.iter().position(|h| h.frame == s) {
                Some(i) => old.swap_remove(i),
                None => Hob::new(s),
            })
            .collect();
        let mut old = core::mem::take(&mut self.fridges);
        self.fridges = core::iter::once(layout.fridge)
            .chain(layout.more.fridges.iter().copied())
            .map(|f| match old.iter().position(|x| x.frame == f) {
                Some(i) => old.swap_remove(i),
                None => Fridge::new(f),
            })
            .collect();
        let mut old = core::mem::take(&mut self.dishwashers);
        self.dishwashers = core::iter::once((layout.dishwasher, dish_face))
            .chain(layout.more.dishwashers.iter().map(|&body| {
                let (_, _, face) = galley_faces(body, body);
                (body, face)
            }))
            .map(
                |(body, face)| match old.iter().position(|d| d.body == Some(body)) {
                    Some(i) => old.swap_remove(i),
                    None => {
                        let mut washer = Dishwasher::at(face);
                        washer.body = Some(body);
                        washer
                    }
                },
            )
            .collect();
        let mut old = core::mem::take(&mut self.lockers);
        self.lockers = core::iter::once(layout.locker)
            .chain(layout.more.lockers.iter().copied())
            .map(|l| match old.iter().position(|x| x.frame == l) {
                Some(i) => old.swap_remove(i),
                None => Locker::new(l),
            })
            .collect();
        self.showers = layout
            .shower
            .into_iter()
            .chain(layout.more.showers.iter().copied())
            .collect();
        self.table = layout.table;
        // Seats and beds keep their order — index is identity — and grow.
        self.chairs = chairs;
        self.plate_on_table.resize(self.chairs.len(), None);
        let mut beds: Vec<Berth> = Vec::with_capacity(layout.beds.len());
        let mut old_beds = core::mem::take(&mut self.beds);
        for frame in &layout.beds {
            match old_beds.iter().position(|b| b.frame == *frame) {
                Some(i) => beds.push(old_beds.remove(i)),
                None => beds.push(Berth::new(*frame, bed_side(*frame, interior))),
            }
        }
        if beds.is_empty() {
            beds.push(Berth::new(counter, bed_side(counter, interior)));
        }
        self.beds = beds;
        self.helm = layout.helm;
        self.benches = layout.benches;
        self.suit_locker = layout.suit_locker;
        self.gangway = layout.gangway;
        self.outside = layout.outside;
        self.hull = layout.hull;
        self.shelves = layout.shelves;
        // A bay keeps its trays unless it moved: a bay somewhere else is a
        // different bay, with nothing planted in it yet. By frame, like the
        // beds, so a bay built while another is growing leaves that one be.
        let mut old_bays = core::mem::take(&mut self.bays);
        self.bays = core::iter::once((layout.bay, layout.bay_side))
            .chain(layout.more.bays.iter().copied())
            .map(
                |(frame, side)| match old_bays.iter().position(|b| b.frame == frame) {
                    Some(i) => old_bays.swap_remove(i),
                    None => Bay::at(frame, side),
                },
            )
            .collect();
        // The heads aboard are a pan and a basin with nothing to remember
        // but a tap running and a flush; laid out again if either moved.
        if self.bath.toilet != layout.toilet || self.bath.sink != layout.sink {
            self.bath = Bath::aboard(layout.toilet, layout.sink);
        }
        // And the other heads, by their pan.
        let mut old = core::mem::take(&mut self.more_baths);
        self.more_baths = layout
            .more
            .heads
            .iter()
            .map(|&(toilet, sink)| {
                match old
                    .iter()
                    .position(|b| b.toilet == toilet && b.sink == sink)
                {
                    Some(i) => old.swap_remove(i),
                    None => Bath::aboard(toilet, sink),
                }
            })
            .collect();
    }

    // --- geometry the rest of the game asks about ------------------------

    /// Where the Bim stands to work at something: below it, on the deck
    /// side. In the classic room every galley fixture is along the top
    /// wall and the station is on the worktop's line whatever the fixture's
    /// depth — the fridge is four units deeper than the run, and the probes
    /// are seeded on where the Bim stands; aboard, a fixture stands where
    /// the design put it and the Bim stands below *it*.
    fn station_at(&self, frame: Rect, x: f32) -> Vec2 {
        let line = if self.shell {
            self.worktops[0].frame.max.y
        } else {
            frame.max.y
        };
        vec2(x, line + STAND_OFF)
    }

    /// A fixture by index, clamped: a pick is never out of range, only
    /// early, and every list has at least one.
    fn worktop(&self, i: usize) -> &Worktop {
        &self.worktops[i.min(self.worktops.len() - 1)]
    }

    fn worktop_mut(&mut self, i: usize) -> &mut Worktop {
        let last = self.worktops.len() - 1;
        &mut self.worktops[i.min(last)]
    }

    pub fn hob(&self, i: usize) -> &Hob {
        &self.hobs[i.min(self.hobs.len() - 1)]
    }

    pub fn hob_mut(&mut self, i: usize) -> &mut Hob {
        let last = self.hobs.len() - 1;
        &mut self.hobs[i.min(last)]
    }

    fn fridge(&self, i: usize) -> &Fridge {
        &self.fridges[i.min(self.fridges.len() - 1)]
    }

    /// How many heads there are: the room's own and every other toilet.
    pub fn baths(&self) -> usize {
        1 + self.more_baths.len()
    }

    /// The heads by index: nought is `bath`, the room's own, which in the
    /// classic room has the walls and the door; the rest are bare.
    pub fn bath_at(&self, i: usize) -> &Bath {
        match i {
            0 => &self.bath,
            i => &self.more_baths[(i - 1).min(self.more_baths.len() - 1)],
        }
    }

    pub fn bath_at_mut(&mut self, i: usize) -> &mut Bath {
        match i {
            0 => &mut self.bath,
            i => {
                let last = self.more_baths.len() - 1;
                &mut self.more_baths[(i - 1).min(last)]
            }
        }
    }

    pub fn fridge_station(&self, i: usize) -> Vec2 {
        // Stand to the side the door does not swing into.
        let f = self.fridge(i).frame;
        self.station_at(f, f.center().x - 6.0)
    }

    pub fn board_station(&self, i: usize) -> Vec2 {
        let w = self.worktop(i);
        self.station_at(w.frame, w.board.center().x)
    }

    pub fn drawer_station(&self, i: usize) -> Vec2 {
        let w = self.worktop(i);
        self.station_at(w.frame, w.drawer.center().x)
    }

    pub fn dishwasher_station(&self, i: usize) -> Vec2 {
        let d = &self.dishwashers[i.min(self.dishwashers.len() - 1)];
        self.station_at(d.body.unwrap_or(d.face), d.face.center().x)
    }

    /// Where the Bim stands to open the broom locker: on the deck side of it,
    /// since the locker itself is set into the port bulkhead.
    pub fn locker_station(&self, i: usize) -> Vec2 {
        let l = self.lockers[i.min(self.lockers.len() - 1)].frame;
        vec2(l.max.x + STAND_OFF, l.center().y)
    }

    /// Every hob whose pot has finished cooking since this was last asked.
    pub fn take_judgements(&mut self) -> Vec<usize> {
        self.hobs
            .iter_mut()
            .enumerate()
            .filter_map(|(i, hob)| core::mem::take(&mut hob.judge_food).then_some(i))
            .collect()
    }

    /// A bowl has been made off a board: judged like a pot, by the dirt
    /// round the hob the bowl's chain has — the galley the Bim is
    /// standing in.
    pub fn made_a_bowl(&mut self, hob: usize) {
        self.hob_mut(hob).judge_food = true;
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
    pub fn shower_station(&self, i: usize) -> Option<Vec2> {
        self.showers
            .get(i)
            .or(self.showers.first())
            .map(|&(_, at)| at)
    }

    /// Which way the Bim faces under the shower: into it.
    pub fn shower_facing(&self, i: usize) -> f32 {
        match self.showers.get(i).or(self.showers.first()).copied() {
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
            Switch::Hob(i) => knob_of(self.hob(i).frame),
            Switch::FridgeDoor(i) => self.fridge(i).frame.center(),
            Switch::BathDoor(_) | Switch::BathLock(_) => self.bath.door.center(),
            Switch::Dishwasher(i) => self.dishwashers[i.min(self.dishwashers.len() - 1)]
                .face
                .center(),
            Switch::Door(i, _) => self.doors[i.min(self.doors.len() - 1)].rect.center(),
        }
    }

    /// Where it stands to reach that. The bathroom door has a panel on both
    /// sides, so which one depends on where the Bim is when it sets off — a
    /// Bim shut inside would otherwise be sent to a handle it cannot reach.
    pub fn switch_station(&self, which: Switch, from: Vec2) -> Vec2 {
        match which {
            Switch::Hob(i) => self.stove_station(i),
            Switch::FridgeDoor(i) => self.fridge_station(i),
            Switch::BathDoor(_) | Switch::BathLock(_) => {
                if self.bath.shell.contains(from) {
                    self.bath.inside_station()
                } else {
                    self.bath.outside_station()
                }
            }
            Switch::Dishwasher(i) => self.dishwasher_station(i),
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
            Switch::Hob(i) => {
                let on = self.hob(i).on;
                self.set_stove(i, !on);
            }
            Switch::FridgeDoor(i) => {
                let open = self.fridge_is_open(i);
                self.set_fridge_open(i, !open);
            }
            Switch::BathDoor(open) => self.bath.set_open(open),
            Switch::BathLock(locked) => self.bath.set_locked(locked),
            Switch::Dishwasher(i) => {
                let last = self.dishwashers.len() - 1;
                self.dishwashers[i.min(last)].start()
            }
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

    /// What an eye stops at that a step may not: every powered door whose
    /// leaves are shut, locked or not, and every airlock with nobody within
    /// a door's `REACH` of it — the room's own rule for when a door opens,
    /// asked of the airlock too, since the room has no leaves for it.
    pub fn shut_leaves(&self, bodies: &[Vec2]) -> Vec<Rect> {
        let mut shut: Vec<Rect> = self
            .doors
            .iter()
            .filter(|d| !d.is_open())
            .map(|d| d.rect)
            .collect();
        for &lock in &self.airlocks {
            let near = bodies
                .iter()
                .any(|&p| lock.expand(crate::door::REACH).contains(p));
            if !near {
                shut.push(lock);
            }
        }
        shut
    }

    /// The powered doors a body walks into right now: locked and shut.
    pub fn shut_doors(&self) -> Vec<Rect> {
        self.doors
            .iter()
            .filter(|d| d.blocks())
            .map(|d| d.rect)
            .collect()
    }

    /// Which bay a point is on, with a little slack, as a click wants;
    /// `None` off all of them.
    pub fn bay_at(&self, p: Vec2) -> Option<usize> {
        self.bays
            .iter()
            .position(|b| b.frame.expand(6.0).contains(p))
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

    pub fn stove_station(&self, i: usize) -> Vec2 {
        self.station_at(self.hob(i).frame, self.pot_pos(i).x)
    }

    /// Serving needs the pot on one side and the plate on the other, both
    /// within a spoon's swing.
    pub fn serve_station(&self, i: usize) -> Vec2 {
        self.station_at(
            self.hob(i).frame,
            (self.serving_pos(i).x + self.pot_pos(i).x) * 0.5,
        )
    }

    /// The one hob, where the pot stands and the only thing that lights, so
    /// heat and pot can never drift apart. In the middle of the stove — a
    /// ship's hob is one tile — except on the classic room's double-width
    /// run, where it stays where the left of the old pair was: the cooking
    /// station is measured off it, and centring it there would move where
    /// the Bim stands and re-roll every probe seed.
    pub fn pot_pos(&self, i: usize) -> Vec2 {
        burner_of(self.hob(i).frame)
    }

    /// Where a plate sits while being served, on the worktop beside the hob.
    pub fn serving_pos(&self, i: usize) -> Vec2 {
        self.hob(i).serving_pos()
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

    /// Which way the sleeper faces: towards the head of the bed, whichever
    /// way the bed lies.
    pub fn bed_lie_facing(&self, who: usize) -> f32 {
        self.berth(who).lie_facing()
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
        let mut all = vec![
            self.worktops[0].frame,
            self.hobs[0].frame,
            self.fridges[0].frame,
            self.table,
        ];
        all.extend(self.beds.iter().map(|b| b.frame));
        all.extend([self.bays[0].frame, north_left, north_right, west]);
        all.extend(self.bays.iter().skip(1).map(|b| b.frame));
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
        if self.hob_at(p).is_some() {
            HIT_STOVE
        } else if self.fridge_at(p).is_some() {
            HIT_FRIDGE
        } else if self.dishwasher_at(p).is_some() {
            HIT_DISHWASHER
        } else if self.beds.iter().any(|b| b.frame.expand(4.0).contains(p)) {
            HIT_BED
        } else if self.bay_at(p).is_some() {
            HIT_HYDRO
        } else if self.locker_at(p).is_some() {
            HIT_LOCKER
        } else if self.door_at(p).is_some() {
            HIT_SHIP_DOOR
        } else if self.shower_at(p).is_some() {
            HIT_SHOWER
        } else {
            // The heads: the room's own first, then every other, so a click
            // on any pan is a pan.
            match self.heads_at(p) {
                Some(i) => self.bath_at(i).hit(p),
                None => self.bath.hit(p),
            }
        }
    }

    // Which of a kind a click landed on, with a click's slack; `None` off
    // all of them. What `hit` is made of, and what the game remembers as
    // the `hit_*` index for the menu that opens.
    pub fn hob_at(&self, p: Vec2) -> Option<usize> {
        self.hobs.iter().position(|h| h.frame.contains(p))
    }

    pub fn fridge_at(&self, p: Vec2) -> Option<usize> {
        self.fridges
            .iter()
            .position(|f| f.frame.expand(4.0).contains(p))
    }

    pub fn dishwasher_at(&self, p: Vec2) -> Option<usize> {
        self.dishwashers
            .iter()
            .position(|d| d.face.expand(6.0).contains(p))
    }

    pub fn locker_at(&self, p: Vec2) -> Option<usize> {
        self.lockers
            .iter()
            .position(|l| l.frame.expand(8.0).contains(p))
    }

    pub fn shower_at(&self, p: Vec2) -> Option<usize> {
        self.showers
            .iter()
            .position(|(frame, _)| frame.expand(6.0).contains(p))
    }

    /// Which heads a click on a pan is in: by the pan, with a click's slack.
    pub fn heads_at(&self, p: Vec2) -> Option<usize> {
        (0..self.baths()).find(|&i| self.bath_at(i).toilet.expand(4.0).contains(p))
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
        if self.hobs.iter().any(|h| h.frame.contains(p)) {
            SPOT_HOB
        } else if self.worktops.iter().any(|w| w.board.contains(p)) {
            SPOT_BOARD
        } else if self.fridges.iter().any(|f| f.frame.contains(p)) {
            SPOT_FRIDGE
        } else if self.dishwashers.iter().any(|d| d.face.contains(p)) {
            SPOT_DISHWASHER
        } else if self.worktops.iter().any(|w| w.frame.contains(p)) {
            SPOT_WORKTOP
        } else if self.bays.iter().any(|b| b.frame.contains(p)) {
            SPOT_BAY
        } else if self.lockers.iter().any(|l| l.frame.contains(p)) {
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
        } else if self.showers.iter().any(|(frame, _)| frame.contains(p)) {
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
            SPOT_WORKTOP => self.worktops[0].frame,
            SPOT_BOARD => self.worktops[0].board,
            SPOT_FRIDGE => self.fridges[0].frame,
            SPOT_HOB => self.hobs[0].frame,
            SPOT_DISHWASHER => self.dishwashers[0].face,
            SPOT_TABLE => self.table,
            // Two of each, and one rect to ring. The first is the answer: a
            // readout that names "Bunk" is naming the kind of thing, and
            // ringing both would read as one enormous fixture.
            SPOT_CHAIR => Rect::from_center_size(self.chairs[0], CHAIR_SIZE),
            SPOT_BUNK => self.beds[0].frame,
            SPOT_BAY => self.bays[0].frame,
            SPOT_LOCKER => self.lockers[0].frame,
            SPOT_TOILET => self.bath.toilet,
            SPOT_BASIN => self.bath.sink,
            SPOT_DOOR => self.bath.door,
            SPOT_HELM => self.helm?,
            SPOT_BENCH => self.benches.first()?.frame,
            SPOT_SHOWER => self.showers.first().copied()?.0,
            SPOT_SUIT_LOCKER => self.suit_locker?.0,
            _ => return None,
        })
    }

    // --- state the player and the task both drive -----------------------

    pub fn set_fridge_open(&mut self, i: usize, open: bool) {
        let last = self.fridges.len() - 1;
        self.fridges[i.min(last)].target = if open { 1.0 } else { 0.0 };
    }

    pub fn fridge_is_open(&self, i: usize) -> bool {
        self.fridge(i).target > 0.5
    }

    pub fn set_drawer_open(&mut self, i: usize, open: bool) {
        self.worktop_mut(i).drawer_target = if open { 1.0 } else { 0.0 };
    }

    pub fn set_stove(&mut self, i: usize, on: bool) {
        let hob = self.hob_mut(i);
        hob.on = on;
        // Lighting it starts the clock again, however it was lit.
        hob.idle = 0.0;
    }

    /// Game minutes before the hob gives up and turns itself off, or zero when
    /// it is not counting — off already, or with something actually cooking.
    pub fn stove_idle_left(&self, i: usize) -> f32 {
        let hob = self.hob(i);
        if !hob.on || Self::is_cooking(hob) {
            return 0.0;
        }
        (HOB_TIMEOUT - hob.idle).max(0.0)
    }

    /// Something in the pot that has not finished coming up to heat. A stew
    /// already done is not cooking — it is just sitting on a live ring, which
    /// is the case the timeout is there to catch.
    fn is_cooking(hob: &Hob) -> bool {
        hob.pot_contents > 0.0 && hob.pot_cooked < 1.0
    }

    /// Pull one bed's blanket up over a sleeper, or make it again.
    pub fn set_bed_occupied(&mut self, who: usize, occupied: bool) {
        let bed = who.min(self.beds.len() - 1);
        self.beds[bed].blanket_target = if occupied { 1.0 } else { 0.0 };
    }

    /// Put something on the board to chop: the tofu on the left, a vegetable
    /// on the right, or whichever side is free when that one is taken.
    pub fn put_on_board(&mut self, i: usize, tofu: bool) {
        let top = self.worktop_mut(i);
        let mut side = if tofu { 0 } else { 1 };
        if !top.board_sides[side].is_empty() {
            side = 1 - side;
        }
        top.board_sides[side] = Cut {
            tofu,
            whole: 1.0,
            pieces: 0,
        };
        top.board_cutting = side;
    }

    /// One knife stroke on what is under it: `done` of `of` strokes are
    /// through, so that much less is whole and a piece more is cut.
    pub fn chop(&mut self, i: usize, done: u32, of: u32) {
        let top = self.worktop_mut(i);
        let cut = &mut top.board_sides[top.board_cutting];
        cut.whole = (1.0 - done as f32 / of.max(1) as f32).max(0.0);
        cut.pieces += 1;
    }

    /// What is cut on the board, to be gathered up: rounds of vegetable and
    /// cubes of tofu.
    pub fn board_pieces(&self, i: usize) -> (u32, u32) {
        self.worktop(i)
            .board_sides
            .iter()
            .fold((0, 0), |(rounds, cubes), cut| {
                if cut.tofu {
                    (rounds, cubes + cut.pieces)
                } else {
                    (rounds + cut.pieces, cubes)
                }
            })
    }

    pub fn clear_board(&mut self, i: usize) {
        let top = self.worktop_mut(i);
        top.board_sides = [Cut::default(); 2];
        top.board_cutting = 0;
    }

    /// Clear a worktop so a fresh cook does not inherit the last one's mess:
    /// done the moment a cooking chain picks it. The table is *not* cleared
    /// — the other Bim may well be sitting at it eating what it cooked
    /// twenty minutes ago, and wiping the plate out from under it would be
    /// the cook reaching across the room.
    pub fn reset_worktop(&mut self, i: usize) {
        self.clear_board(i);
        self.worktop_mut(i).knife_on_board = false;
    }

    /// Clear a hob's serving spot as a cooking chain picks the hob: a plate
    /// left there by a serve that never finished goes back in the drawer
    /// rather than nowhere, since it is counted.
    pub fn reset_hob(&mut self, i: usize) {
        if self.hob_mut(i).plate_on_counter.take().is_some() {
            self.return_plate();
        }
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
            crate::hydro::Crop::Fibre => self.fibre = self.fibre.saturating_sub(1),
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

    /// A plate out of the drawer, for a meal. Saturating: a drawer that has
    /// run empty — twenty plates, ten in the rack, two crew, so in practice
    /// never — serves an imaginary one rather than starving anybody, and the
    /// dishwasher brings the count back on its own.
    pub fn take_plate(&mut self) {
        self.plates = self.plates.saturating_sub(1);
    }

    /// A plate back in the drawer without going through the rack: what a
    /// chain given up for good does with the one in its hands.
    pub fn return_plate(&mut self) {
        self.plates = (self.plates + 1).min(PLATE_DRAWER);
    }

    /// Whether there is enough left for a meal of some sort.
    pub fn has_ingredients(&self) -> bool {
        self.can_cook(Dish::Stew) || self.can_cook(Dish::Bowl)
    }

    /// Put a crop from the bay away. Fibre is counted twice over — on the
    /// shelf, and on the tally the world drains into the hold — because the
    /// hold is where a bandage is made from it; a harvest given up short of
    /// the store (`Task::let_go`) comes through here too, so it counts.
    pub fn store(&mut self, crop: crate::hydro::Crop) {
        match crop {
            crate::hydro::Crop::Veg => self.veg += 1,
            crate::hydro::Crop::Soy => self.tofu += 1,
            crate::hydro::Crop::Fibre => {
                self.fibre += 1;
                self.harvested_fibre += 1;
            }
        }
    }

    pub fn update(&mut self, dt: f32) {
        self.time += dt;
        self.bath.update(dt);
        // A finished cycle's rack goes back in the drawer, which is where
        // the plates are counted; over the drawer's capacity they are
        // simply stacked, and nothing is lost.
        for bath in &mut self.more_baths {
            bath.update(dt);
        }
        for washer in &mut self.dishwashers {
            let washed = washer.update(dt);
            self.plates = (self.plates + washed).min(PLATE_DRAWER);
        }

        let step = SWING_RATE * dt;
        for fridge in &mut self.fridges {
            fridge.door += clamp(fridge.target - fridge.door, -step, step);
        }
        for top in &mut self.worktops {
            top.drawer_open += clamp(top.drawer_target - top.drawer_open, -step, step);
        }
        // Slower than a drawer: a blanket is drawn up, not snapped.
        let pull = dt * 1.2;
        for bed in &mut self.beds {
            bed.blanket += clamp(bed.blanket_target - bed.blanket, -pull, pull);
        }

        for hob in &mut self.hobs {
            let want = if hob.on { 1.0 } else { 0.0 };
            // Heating up is slower than cooling down, as a real hob is.
            let rate = if hob.on { 1.6 } else { 0.9 };
            hob.heat += clamp(want - hob.heat, -rate * dt, rate * dt);

            // Anything in the pot slowly turns from raw greens into a stew.
            if hob.pot_contents > 0.0 && hob.heat > 0.4 {
                let was = hob.pot_cooked;
                hob.pot_cooked = (hob.pot_cooked + dt * 0.22).min(1.0);
                if was < 1.0 && hob.pot_cooked >= 1.0 {
                    hob.judge_food = true;
                }
            }

            // A hob left lit with nothing coming up to heat shuts itself off.
            // The count only runs while nothing is cooking, so a meal in
            // progress keeps resetting it and is never cut short.
            if hob.on && !Self::is_cooking(hob) {
                hob.idle += dt * MINUTES_PER_SECOND;
                if hob.idle >= HOB_TIMEOUT {
                    hob.on = false;
                    hob.idle = 0.0;
                }
            } else if hob.on {
                hob.idle = 0.0;
            }
        }
    }

    // --- drawing ---------------------------------------------------------

    pub fn draw(&self, list: &mut DrawList) {
        if self.shell {
            self.draw_floor(list);
        }
        self.draw_galley(list);
        for i in 0..self.hobs.len() {
            self.draw_stove(list, i);
        }
        for i in 0..self.fridges.len() {
            self.draw_fridge(list, i);
        }
        self.draw_table(list);
        for i in 0..self.lockers.len() {
            self.draw_locker(list, i);
        }
        for bed in &self.beds {
            self.draw_bed(list, bed);
        }
        for bay in &self.bays {
            bay.draw(list);
        }
        self.bath.draw(list);
        for bath in &self.more_baths {
            bath.draw(list);
        }
        self.draw_stills(list);
        if self.doors_drawn {
            for door in &self.doors {
                door.draw(list);
            }
        }
    }

    /// The fixtures the room does not work, each as itself and standing
    /// still. See [`Stills`].
    fn draw_stills(&self, list: &mut DrawList) {
        let s = &self.stills;
        for &t in &s.tables {
            Self::draw_table_top(list, t);
        }
        for bath in &s.basins {
            bath.draw_fittings(list, false, true);
        }
    }

    /// The fixtures alone, and only the ones asked for: the picture the
    /// designer puts on a ship it is building. See [`Fixtures`]. A bed's
    /// bedding goes on with it, there being nobody in it.
    pub fn draw_fixtures(&self, list: &mut DrawList, has: Fixtures) {
        if has.counter {
            for i in 0..self.worktops.len() {
                self.draw_worktop(list, i);
            }
        }
        if has.dishwasher {
            for washer in &self.dishwashers {
                washer.draw(list);
            }
        }
        if has.stove {
            for i in 0..self.hobs.len() {
                self.draw_stove(list, i);
            }
        }
        if has.fridge {
            for i in 0..self.fridges.len() {
                self.draw_fridge(list, i);
            }
        }
        if has.table || has.chairs {
            self.draw_seating(list, has.table, has.chairs);
        }
        if has.locker {
            for i in 0..self.lockers.len() {
                self.draw_locker(list, i);
            }
        }
        if has.beds {
            for bed in &self.beds {
                self.draw_bed(list, bed);
                self.draw_bedding_over(list, bed);
            }
        }
        if has.bay {
            for bay in &self.bays {
                bay.draw(list);
            }
        }
        if has.toilet || has.basin {
            self.bath.draw_fittings(list, has.toilet, has.basin);
            for bath in &self.more_baths {
                bath.draw_fittings(list, has.toilet, has.basin);
            }
        }
        if has.doors {
            for door in &self.doors {
                door.draw(list);
            }
        }
        // And every fixture beyond the first of its kind, which is there
        // by definition: a layout only lists what the design has twice.
        self.draw_stills(list);
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
        // Everything on the bed is laid out in the bed's own frame — across
        // its centre line and along it from the head — so a bed lying
        // along `x` is the same picture on its side. See `Berth::at`.
        let m_across = bed.breadth() - 16.0;
        let m_along = bed.length() - 16.0;
        // The duvet reaches from just below the pillow to the foot of the
        // mattress whether the bed is made or slept in — pulled up to the chin
        // is where it ends up either way. What changes is the shape of it.
        let head = 52.0;
        let foot = bed.length() - 12.0;
        let mid = (head + foot) * 0.5;
        let width = m_across - 6.0;
        list.rect(
            bed.at(0.0, mid),
            bed.size(width, foot - head),
            0.0,
            6.0,
            BLANKET,
        );
        // A turned-down cuff along the top edge, so it reads as bedding
        // rather than a coloured panel.
        list.rect(
            bed.at(0.0, head + 5.0),
            bed.size(width, 11.0),
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
                bed.at(0.0, head + 0.33 * (foot - head)),
                bed.size(0.6 * m_across, 0.47 * m_along * breath),
                0.0,
                BLANKET_FOLD.alpha(0.33 * bed.blanket),
            );
        }
        // Creases running down towards the foot.
        for side in [-1.0f32, 1.0] {
            list.rect(
                bed.at(side * 0.22 * m_across, mid + 8.0),
                bed.size(3.5, 0.68 * (foot - head)),
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
        let (b, l) = (bed.breadth(), bed.length());

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
        for (along, depth) in [(5.0, 14.0), (l - 4.0, 9.0)] {
            list.rect(
                bed.at(0.0, along),
                bed.size(b + 6.0, depth),
                0.0,
                3.0,
                BED_BOARD,
            );
            let lit = if along < l / 2.0 {
                depth * 0.5 - 1.5
            } else {
                1.5 - depth * 0.5
            };
            list.rect(
                bed.at(0.0, along + lit),
                bed.size(b + 6.0, 2.5),
                0.0,
                1.0,
                BED_BOARD_EDGE,
            );
        }

        // Pillow at the head end, placed off the lying position so the Bim's
        // head always lands on it. Its height gives way on a narrow bed
        // before its width does.
        let pillow = bed.pillow_pos();
        let pillow_size = (b - 16.0 - 14.0, 32.0f32.min(0.22 * (l - 16.0)));
        list.rect(
            pillow,
            bed.size(pillow_size.0, pillow_size.1),
            0.0,
            9.0,
            PILLOW,
        );
        list.line(
            bed.at(0.0, 34.0 - 0.4 * pillow_size.1),
            bed.at(0.0, 34.0 + 0.4 * pillow_size.1),
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

    /// Every worktop, every dishwasher, and the plate being served beside
    /// each hob.
    fn draw_galley(&self, list: &mut DrawList) {
        for i in 0..self.worktops.len() {
            self.draw_worktop(list, i);
        }
        for washer in &self.dishwashers {
            washer.draw(list);
        }
        for (i, hob) in self.hobs.iter().enumerate() {
            if let Some(fill) = hob.plate_on_counter {
                draw_plate(list, self.serving_pos(i), fill, hob.pot_cooked, self.dish);
            }
        }
    }

    /// A run of worktop with its drawer and its board on it.
    fn draw_worktop(&self, list: &mut DrawList, i: usize) {
        Self::draw_worktop_run(list, self.worktops[i].frame);
        self.draw_worktop_top(list, i);
    }

    /// A run of worktop alone: the cabinet, the lip, the top and the lit
    /// fascia. What every worktop aboard is drawn as; the room's own gets
    /// the drawer and the board on top. A function of the rect and nothing
    /// else, so a second worktop is drawn the same way.
    fn draw_worktop_run(list: &mut DrawList, c: Rect) {
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
    }

    /// What stands on the room's own worktop: the drawer and the board.
    fn draw_worktop_top(&self, list: &mut DrawList, i: usize) {
        let top = &self.worktops[i];
        // Drawer, sliding out towards the room. The open box is drawn behind
        // the face so you can see into it.
        let slide = top.drawer_open * 20.0;
        let d = top.drawer;
        if top.drawer_open > 0.02 {
            list.rect(
                d.center() + vec2(0.0, slide * 0.5),
                vec2(d.width() - 6.0, d.height() + slide),
                0.0,
                3.0,
                Color::rgb(0.14, 0.12, 0.10),
            );
            // The plates in it, on edge in a row: a pip each up to what the
            // face has room for, so a drawer running low looks it.
            let across = ((d.width() - 12.0) / 6.0).floor().max(1.0) as u32;
            let shown = self.plates.min(across);
            for i in 0..shown {
                let x = d.min.x + 6.0 + (i as f32 + 0.5) * 6.0;
                list.rect(
                    vec2(x, d.center().y + slide * 0.5),
                    vec2(2.0, (d.height() + slide) * 0.6),
                    0.0,
                    1.0,
                    PLATE,
                );
            }
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
        let b = top.board;
        list.rect(b.center(), b.size(), 0.0, 4.0, BOARD);
        list.stroke_rect(b.center(), b.size(), 0.0, 4.0, 1.5, GLOW.alpha(0.35));
        // Each side of the board holds one thing: what is left of it at the
        // outer end, and its pieces piling up towards the middle in two
        // columns of three. Tofu is a block and chops into cubes; a
        // vegetable is a lump and chops into rounds. Laid out in fractions
        // of the board, which is a tile less its margins aboard and wider in
        // the classic room.
        let w = b.width();
        for (side, cut) in top.board_sides.iter().enumerate() {
            if cut.is_empty() {
                continue;
            }
            let (whole, bits) = if cut.tofu {
                (TOFU, TOFU_EDGE)
            } else {
                (VEG, VEG_DARK)
            };
            // The left side runs left to right, the right side the other way.
            let dir = if side == 0 { 1.0 } else { -1.0 };
            let end = if side == 0 { b.min.x } else { b.max.x };
            if cut.whole > 0.0 {
                let grow = 0.35 + 0.65 * cut.whole;
                let at = vec2(end + dir * w * 0.14, b.center().y);
                if cut.tofu {
                    list.rect(at, vec2(22.0 * grow, 18.0), 0.0, 3.0, whole);
                    list.stroke_rect(at, vec2(22.0 * grow, 18.0), 0.0, 3.0, 1.5, bits);
                } else {
                    list.ellipse(at, vec2(24.0 * grow, 15.0), 0.0, whole);
                    list.ellipse(
                        at - vec2(dir * 10.0 * grow, 0.0),
                        vec2(7.0, 11.0),
                        0.0,
                        bits,
                    );
                }
            }
            for i in 0..cut.pieces {
                let col = (i / 3) as f32;
                let row = (i % 3) as f32;
                let at = vec2(
                    end + dir * (w * 0.3 + col * 9.0),
                    b.center().y - 9.0 + row * 9.0,
                );
                if cut.tofu {
                    list.rect(at, vec2(8.0, 8.0), 0.0, 2.0, whole);
                    list.stroke_rect(at, vec2(8.0, 8.0), 0.0, 2.0, 1.0, bits);
                } else {
                    list.circle(at, 8.0, whole);
                    list.circle(at, 3.5, bits);
                }
            }
        }
        if top.knife_on_board {
            draw_knife(list, b.center() + vec2(0.0, 12.0), 0.35);
        }
    }

    fn draw_stove(&self, list: &mut DrawList, i: usize) {
        self.draw_hob(list, i);
        self.draw_pot(list, i);
    }

    /// A hob: the glass, the burner and its coils, the control, with its
    /// heat and its switch.
    fn draw_hob(&self, list: &mut DrawList, i: usize) {
        let hob = &self.hobs[i];
        let s = hob.frame;
        list.rect(s.center(), s.size() - vec2(0.0, 14.0), 0.0, 4.0, STOVE_TOP);

        let hot = hob.heat;
        let flicker = 0.85 + 0.15 * (self.time * 9.0).sin();
        let at = burner_of(s);
        let k = hob_scale_of(s);
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
        let knob = knob_of(s);
        list.rect(knob, vec2(34.0, 12.0), 0.0, 6.0, STOVE_TOP);
        list.stroke_rect(knob, vec2(34.0, 12.0), 0.0, 6.0, 1.0, PANEL_EDGE);
        let lit = if hob.on { KNOB_ON } else { GLOW };
        let pip = knob + vec2(lerp(-9.0, 9.0, hot), 0.0);
        list.circle(pip, 15.0, lit.alpha(0.20));
        list.circle(pip, 8.0, lit);
        list.circle(pip, 3.0, KNOB.alpha(0.5));
    }

    fn draw_pot(&self, list: &mut DrawList, i: usize) {
        let hob = &self.hobs[i];
        let at = self.pot_pos(i);
        // A shade smaller than the hob it stands on, and scaled with it.
        let k = POT_SIZE * hob_scale_of(hob.frame);
        list.circle(at, 50.0 * k, POT);
        list.ring(at, 46.0 * k, 4.0 * k, POT_RIM);
        // Handles either side.
        let handle = vec2(14.0 * k, 7.0 * k);
        list.rect(at + vec2(-30.0 * k, 0.0), handle, 0.0, 3.0 * k, POT_RIM);
        list.rect(at + vec2(30.0 * k, 0.0), handle, 0.0, 3.0 * k, POT_RIM);

        if hob.pot_contents <= 0.0 {
            list.circle(at, 40.0 * k, Color::rgb(0.10, 0.11, 0.12));
            return;
        }

        let broth = Color::rgb(
            lerp(BROTH_RAW.r, BROTH_DONE.r, hob.pot_cooked),
            lerp(BROTH_RAW.g, BROTH_DONE.g, hob.pot_cooked),
            lerp(BROTH_RAW.b, BROTH_DONE.b, hob.pot_cooked),
        );
        let surface = 40.0 * k * (0.6 + 0.4 * hob.pot_contents);
        list.circle(at, surface, broth);

        if hob.heat <= 0.3 {
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
                STEAM.alpha(0.16 * (1.0 - t) * hob.heat),
            );
        }
    }

    /// A cold store: the cabinet, its panel and its door, swinging.
    fn draw_fridge(&self, list: &mut DrawList, i: usize) {
        let f = self.fridges[i].frame;
        let door = self.fridges[i].door;
        let open = self.fridge_is_open(i);
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
            GLOW.alpha(if open { 0.9 } else { 0.45 }),
        );

        if door > 0.01 {
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
                let stock = (self.veg + self.tofu + self.stew + self.fibre) as f32;
                let shown = (stock * 8.0 / SHELF_FULL).ceil().min(8.0) as usize;
                for i in 0..PER_SHELF {
                    if row * PER_SHELF + i >= shown {
                        continue;
                    }
                    let x = inner.min.x + 3.0 + step * (i as f32 + 0.5);
                    let at = vec2(x, y - r * 1.1 + 1.0);
                    let c = STOCK[(row * PER_SHELF + i) % STOCK.len()];
                    list.ellipse(at, vec2(r, r * 1.1), 0.0, c.alpha(door));
                }
            }
        }

        // The door is hinged at the front-left corner and swings into the room.
        let hinge = vec2(f.min.x, f.max.y);
        let angle = door * (PI * 0.58);
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
    /// A broom locker, with its broom in or out.
    fn draw_locker(&self, list: &mut DrawList, i: usize) {
        let l = self.lockers[i].frame;
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
        if self.lockers[i].broom_out {
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
        self.draw_seating(list, true, true);
    }

    /// The table and its chairs, either without the other for a room that
    /// has only one of them yet.
    fn draw_seating(&self, list: &mut DrawList, table: bool, chairs: bool) {
        // Chairs first: a Bim sits on top of one, so each has to be wide
        // enough that a seated Bim does not overhang the sides. The backrest
        // goes on the side away from the table, which is opposite ways round
        // for the two of them.
        for &ch in &self.chairs {
            if !chairs {
                break;
            }
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

        if !table {
            return;
        }
        Self::draw_table_top(list, self.table);
        self.draw_place_settings(list);
    }

    /// A table top: what every table aboard is drawn as. A function of the
    /// rect alone, so a second table is drawn the same way.
    fn draw_table_top(list: &mut DrawList, t: Rect) {
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
    }

    /// The plates on the room's own table, at every seat that has one.
    fn draw_place_settings(&self, list: &mut DrawList) {
        for seat in 0..self.chairs.len() {
            let Some(fill) = self.plate_on_table[seat] else {
                continue;
            };
            let p = self.table_plate_pos(seat);
            draw_plate(list, p, fill, self.hobs[0].pot_cooked, self.dish);
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

// --- the hob, as a function of its rect ----------------------------------------
// Where the burner, the knob and the pot are on a hob of this size: the
// room's own hob asks with its own rect, and a second hob aboard — built by
// the crew, or a station's — is drawn from its own the same way.

/// The burner's middle: centred, since a ship's hob is one tile — except
/// on the classic room's double-width run, where it stays where the left of
/// the old pair was: the cooking station is measured off it, and centring
/// it there would move where the Bim stands and re-roll every probe seed.
fn burner_of(stove: Rect) -> Vec2 {
    let c = stove.center();
    if stove.width() > 1.5 * stove.height() {
        c + vec2(-30.0, 0.0)
    } else {
        c
    }
}

/// How big the hob and the pot are drawn: 1 on the classic room's
/// double-width run, whose burner always overhung the counter, and on a
/// one-tile hob whatever fits the burner inside the tile, so the pot reads
/// as standing on it rather than over its neighbours. Drawing only.
fn hob_scale_of(stove: Rect) -> f32 {
    if stove.width() > 1.5 * stove.height() {
        1.0
    } else {
        (stove.width().min(stove.height()) / (2.0 * BURNER_R)).min(1.0)
    }
}

/// The control knob, set towards the near-left of the hob so it is within
/// reach from both the pot and the serving spot.
fn knob_of(stove: Rect) -> Vec2 {
    vec2(stove.min.x + 20.0, stove.max.y - 7.0)
}
