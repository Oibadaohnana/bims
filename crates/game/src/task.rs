//! Scripted jobs the Bim carries out, one step at a time.
//!
//! A task is a straight line of [`Step`]s. Each step either sends the Bim
//! somewhere and waits for it to arrive, or holds it in place for a while
//! playing an animation. Anything the step changes about the world — a door
//! opening, a vegetable moving from hand to board — happens on the way in or
//! on the way out, so the state and what you can see never disagree.

use crate::character::{Action, BITE_PERIOD, CHOP_PERIOD, Character, Held, SCOOP_PERIOD};
use crate::clock::{self, HOUR, MINUTES_PER_SECOND};
use crate::filth;
use crate::galley::closest_free;
use crate::hydro::Crop;
use crate::math::{PI, Vec2, vec2};
use crate::nav::Maps;
use crate::needs::Need;
use crate::rng::Rng;
use crate::room::{Dish, Room, Switch};

/// How many things go under the knife for each recipe, and how much of the
/// cold store the whole meal uses up. A stew is two vegetables chopped one
/// after the other; a bowl is one block of tofu, with the salad going in
/// straight from the fridge alongside it.
fn portions(dish: Dish) -> u32 {
    match dish {
        Dish::Stew => 2,
        Dish::Bowl => 1,
    }
}

/// How many times a chain goes round the fridge-board-knife loop before it
/// carries on: once per portion of a meal, and twice for the shelf stew —
/// a vegetable and then a block of tofu.
fn laps(kind: Kind) -> u32 {
    match kind {
        Kind::Meal(dish) => portions(dish),
        Kind::Batch => 2,
        _ => 1,
    }
}

/// How many knife strokes it takes to get through one vegetable.
const CHOPS: u32 = 5;
/// How many spoonfuls go from the pot onto the plate.
const SCOOPS: u32 = 3;
/// How many mouthfuls it takes to clear the plate.
const BITES: u32 = 6;
/// How long the pot sits bubbling before it is ready to serve.
const COOK_TIME: f32 = 6.0;
/// How many helpings a pot of stew holds. Cooked once, eaten twice: the Bim
/// makes a pot, has a plate of it, and comes back to the rest of it when it is
/// hungry again. Nothing else aboard keeps, so a bowl is still one sitting.
pub const SERVINGS_PER_POT: u32 = 2;
/// The shortest a fumble ever costs. A walk that was over before it started
/// would otherwise stall for nothing at all and the fumble would be invisible.
/// Kept well under the shortest real step, so it does not skew the arithmetic.
const MIN_STALL: f32 = 0.3;

/// How long a pair of hands in a tray takes: a plant in or a plant out.
const TRAY_TIME: f32 = 2.4;

/// How long two of them stand there talking. Six seconds is six game minutes,
/// which is exactly what `needs::TALKING` fills the company bar over — the two
/// numbers are the same length of time said twice, so if one moves the other
/// has to move with it.
const CHAT_TIME: f32 = 6.0;

/// How long one tile takes to sweep. Long enough that clearing a fouled deck
/// is visibly an afternoon's work rather than a lap of the room.
const SWEEP_TIME: f32 = 3.2;
/// How many tiles a Bim does before putting the broom away. It goes back for
/// more if the deck still wants it and nothing else has come up — this is
/// about letting hunger and the heads get a look in, not about giving up.
pub const TILES_PER_SWEEP: u32 = 5;

/// How long the Bim sits there, and how long it spends at the basin after.
const TOILET_TIME: f32 = 5.0;
const WASH_TIME: f32 = 4.2;
/// How much of a mess a turn at the basin gets off the Bim. Hands and face,
/// not a change of clothes, so an accident is not simply washed away.
const WASH_TAKES_OFF: f32 = 0.5;

/// The two lengths of lie-down on offer, in game minutes. The host reads both
/// of these so its menu cannot disagree with what the Bim actually does.
pub const NAP_MINUTES: f32 = 30.0;
pub const SLEEP_MINUTES: f32 = 6.0 * HOUR;

/// How long dressing a wound takes, in game minutes, hands on the patient.
/// Carried on the task in `rest_minutes`, the way a craft's length is, so
/// the chain has one clock for "for as long as it was told".
pub const BANDAGE_MINUTES: f32 = 10.0;

/// Facing for the fixtures along the top wall.
const FACE_WALL: f32 = -PI * 0.5;
/// In the heads: the door is in the north bulkhead and the pan is against the
/// hull on the east side, so the Bim turns its back on each in turn.
const FACE_DOOR: f32 = -PI * 0.5;
const FACE_INWARD: f32 = PI * 0.5;
const FACE_OFF_PAN: f32 = PI;
/// The bay is against the bottom wall, so the Bim turns its back on the room.
const FACE_TRAY: f32 = PI * 0.5;
/// The broom locker is in the port bulkhead, so the Bim faces it across the
/// room rather than along it.
const FACE_LOCKER: f32 = PI;

#[derive(Clone, Copy, PartialEq)]
pub enum Step {
    GoToFridge,
    OpenFridge,
    TakeVegetable,
    CloseFridge,
    CarryToBoard,
    PutVegetableDown,
    GoToDrawerForKnife,
    TakeKnife,
    BackToBoard,
    Chop,
    PutKnifeDown,
    GatherSlices,
    CarryToPot,
    TipIntoPot,
    TurnStoveOn,
    Cook,
    GoToDrawerForPlate,
    TakePlateAndSpoon,
    BackToStove,
    SetPlateDown,
    Serve,
    TurnStoveOff,
    PickUpPlate,
    CarryToTable,
    SitDown,
    Eat,
    Rest,
    StandUp,
    // A bowl instead of a pot: fetch one, bring it back to the board, and
    // tip the chopped tofu and the salad into it. No heat involved.
    GoToDrawerForBowl,
    TakeBowl,
    BackToBoardWithBowl,
    FillBowl,

    // A stew for the store, and a stew out of it. Making one shares the
    // meal chain as far as the hob — fridge, board, knife, twice round, pot,
    // heat — and then, instead of a plate, the pot goes into a tub and the
    // tub into the cold store. The store's door has its own pair of steps
    // here rather than borrowing `OpenFridge`/`CloseFridge`, because that
    // chain has already been through those once on the way to the board,
    // and a step that appears twice in one chain is a step `rewind` and the
    // progress bar cannot tell apart.
    PackStew,
    CarryStewToStore,
    OpenStoreForStew,
    StowStew,
    ShutStoreOnStew,
    // Warming one up is the other way about: the tub out of the store and
    // into the pot, and from the hob on it is the meal chain's own serving
    // and sitting down.
    TakeStew,
    CarryStewToPot,
    TipStewIntoPot,

    // Clearing up afterwards. The last of these is skipped unless the
    // machine came up full, which is the one branch in any of the chains.
    ClearTable,
    CarryToDishwasher,
    OpenDishwasher,
    StackDishes,
    ShutDishwasher,
    StartDishwasher,

    // Working any switch is the same little errand: walk to it, and put a
    // hand on it. Which switch is on the task, not on the step.
    GoToSwitch,
    FlipSwitch,

    // Tending the hydroponic bay: over to the tray that wants doing, and a
    // pair of hands in it. Which tray, and whether it is a planting or a
    // lifting, is decided when the hands arrive rather than when the errand
    // started — the store moves while the Bim walks.
    GoToTray,
    WorkTray,
    // And, if the tray had something ripe in it, the walk back up the room
    // with it. Nothing aboard is remote, and that has to include the harvest:
    // a plant lifted at the bay is in the Bim's hands, not in the store, until
    // the Bim has carried it there and put it away. The fridge steps either
    // side of this are the meal chain's own — the cold store has one door and
    // one way of opening it.
    CarryCropToStore,
    StowCrop,

    // Sweeping up. The broom comes out of its locker in the port bulkhead,
    // goes to whichever tile most wants it, and goes back when the deck is
    // clean or the Bim has had enough of it. `CarryBroomTo` and `Sweep` loop
    // between them, a tile at a time — see `Task::next_step`.
    GoToLocker,
    TakeBroom,
    CarryBroomTo,
    Sweep,
    BackToLocker,
    PutBroomBack,

    // Going to bed is one errand with a dial on it: a nap and a night's sleep
    // are the same walk, ladder and pillow, and differ only in how long the
    // Bim stays put.
    // A trip to the heads: through the door, shut it, sit, flush, wash, and
    // back out again. The door is shut and locked behind the Bim on the way
    // in and opened again on the way out, so the lock is never left on.
    GoToDoor,
    OpenDoor,
    StepInside,
    ShutDoor,
    GoToToilet,
    SitOnToilet,
    UseToilet,
    RiseFromToilet,
    FlushToilet,
    GoToSink,
    WashHands,
    BackToDoor,
    UnlockDoor,
    StepOutside,
    ShutDoorBehind,

    GoToBed,
    ClimbIntoBed,
    Doze,
    WakeUp,
    ClimbOutOfBed,

    // Having a word with the other one. Two steps: over to the spot they are
    // to meet at, and then standing there talking. Both crew are given one of
    // these at the same moment, each walking to its own side of the meeting
    // point, so neither is chasing a target that is itself walking — see the
    // note in `Game::chat`.
    GoToMeet,
    Talk,

    // A shower: over to it and standing under it. Nothing to take, open or
    // put back, so the chain is the walk and the wash.
    GoToShower,
    Shower,

    // Making something: over to the bench the recipe wants, and hands on it
    // for the recipe's length. What is made, out of what, is the world's —
    // the room only says, on the way out of `Work`, that a recipe was
    // finished, and `Game::take_crafted` hands that on. See
    // `shipdesign::recipes`.
    GoToBench,
    Work,

    // A walk outside: a suit out of its locker, over to the deck inside the
    // port, out through the airlock, and then — on the outside's own grid,
    // see `nav::Nav::outside` — to a marked rock, a pick swung at it until
    // it is gone, the next rock, and so on until there is none it can get
    // to; then back to the port, in, and the suit hung up again. `PickRock`
    // is a step of no length between one rock and the next: the world
    // takes the mined tile out and hands the room the rocks afresh, and
    // the grid is rebuilt, before the next rock is chosen — so a tile just
    // mined is a tile the Bim can now stand in to reach the one behind it.
    // What a rock yields is the world's to add: the room puts the mined
    // rock's middle on `Room::mined` and counts finished walks in
    // `Room::walks_done`. See `Kind::Eva`.
    GoToSuitLocker,
    TakeSuit,
    GoToGangway,
    StepOut,
    PickRock,
    WalkToRock,
    Mine,
    WalkToPort,
    StepIn,
    BackToSuitLocker,
    PutSuitBack,

    // Building. A load off a shelf, carried to a construction site and put
    // down there — one trip an errand, `Kind::Haul` — and, once everything
    // is there, standing beside the site and putting it together for as
    // long as the world says, `Kind::Build`. A site outside the hull is the
    // same two errands with the suit and the airlock either side of the
    // walk to it, on the outside's grid: the walk out borrows the walk
    // outside's steps above, from `GoToSuitLocker` to `StepOut` and from
    // `WalkToPort` to `PutSuitBack`. The room moves no materials and
    // changes no ship: `TakeMaterials` says a load was taken, `DropMaterials`
    // that it arrived and `Construct` that the site is built, on
    // `Room::picked`, `Room::dropped` and `Room::built`, and the world
    // moves the count and puts the part down.
    GoToShelf,
    TakeMaterials,
    CarryToSite,
    DropMaterials,
    GoToSite,
    Construct,

    // Dressing a wound: over to the patient — a crewmate, or the spot the
    // Bim already stands on for its own — and hands on the part for
    // `BANDAGE_MINUTES`, riding in `rest_minutes` like a craft's. The room
    // has the bandages but not the bodies, so `Dress` only says, on the way
    // out, that a part was dressed (`Room::dressed`), and `Game` does the
    // dressing: the patient's health is a `Bim`'s. See `Kind::Bandage`.
    GoToPatient,
    Dress,

    Done,
}

impl Step {
    fn next(self) -> Step {
        use Step::*;
        match self {
            GoToFridge => OpenFridge,
            OpenFridge => TakeVegetable,
            TakeVegetable => CloseFridge,
            CloseFridge => CarryToBoard,
            CarryToBoard => PutVegetableDown,
            PutVegetableDown => GoToDrawerForKnife,
            GoToDrawerForKnife => TakeKnife,
            TakeKnife => BackToBoard,
            BackToBoard => Chop,
            Chop => PutKnifeDown,
            PutKnifeDown => GatherSlices,
            GoToDrawerForBowl => TakeBowl,
            TakeBowl => BackToBoardWithBowl,
            BackToBoardWithBowl => FillBowl,
            FillBowl => CarryToTable,
            PackStew => CarryStewToStore,
            CarryStewToStore => OpenStoreForStew,
            OpenStoreForStew => StowStew,
            StowStew => ShutStoreOnStew,
            TakeStew => CloseFridge,
            CarryStewToPot => TipStewIntoPot,
            TipStewIntoPot => TurnStoveOn,
            GatherSlices => CarryToPot,
            CarryToPot => TipIntoPot,
            TipIntoPot => TurnStoveOn,
            TurnStoveOn => Cook,
            Cook => GoToDrawerForPlate,
            GoToDrawerForPlate => TakePlateAndSpoon,
            TakePlateAndSpoon => BackToStove,
            BackToStove => SetPlateDown,
            SetPlateDown => Serve,
            Serve => TurnStoveOff,
            TurnStoveOff => PickUpPlate,
            PickUpPlate => CarryToTable,
            CarryToTable => SitDown,
            SitDown => Eat,
            Eat => Rest,
            Rest => StandUp,
            StandUp => ClearTable,
            ClearTable => CarryToDishwasher,
            CarryToDishwasher => OpenDishwasher,
            OpenDishwasher => StackDishes,
            StackDishes => ShutDishwasher,
            ShutDishwasher => StartDishwasher,
            GoToSwitch => FlipSwitch,
            GoToTray => WorkTray,
            GoToLocker => TakeBroom,
            TakeBroom => CarryBroomTo,
            CarryBroomTo => Sweep,
            Sweep => BackToLocker,
            BackToLocker => PutBroomBack,
            WorkTray => CarryCropToStore,
            CarryCropToStore => OpenFridge,
            StowCrop => CloseFridge,
            GoToDoor => OpenDoor,
            OpenDoor => StepInside,
            StepInside => ShutDoor,
            ShutDoor => GoToToilet,
            GoToToilet => SitOnToilet,
            SitOnToilet => UseToilet,
            UseToilet => RiseFromToilet,
            RiseFromToilet => FlushToilet,
            FlushToilet => GoToSink,
            GoToSink => WashHands,
            WashHands => BackToDoor,
            BackToDoor => UnlockDoor,
            UnlockDoor => StepOutside,
            StepOutside => ShutDoorBehind,
            GoToBed => ClimbIntoBed,
            ClimbIntoBed => Doze,
            Doze => WakeUp,
            WakeUp => ClimbOutOfBed,
            GoToMeet => Talk,
            GoToShower => Shower,
            GoToBench => Work,
            GoToSuitLocker => TakeSuit,
            TakeSuit => GoToGangway,
            GoToGangway => StepOut,
            StepOut => PickRock,
            PickRock => WalkToRock,
            WalkToRock => Mine,
            // Round again for the next rock; `next_step` is what sends the
            // Bim home instead when there is none.
            Mine => PickRock,
            WalkToPort => StepIn,
            StepIn => BackToSuitLocker,
            BackToSuitLocker => PutSuitBack,
            // The two building errands as they run inside the hull; the
            // outside variants turn off through the airlock in
            // `Kind::steps` and `Task::next_step`.
            GoToShelf => TakeMaterials,
            TakeMaterials => CarryToSite,
            CarryToSite => DropMaterials,
            GoToSite => Construct,
            GoToPatient => Dress,
            StartDishwasher | FlipSwitch | ClimbOutOfBed | ShutDoorBehind | PutBroomBack | Talk
            | ShutStoreOnStew | Shower | Work | PutSuitBack | DropMaterials | Construct | Dress
            | Done => Done,
        }
    }

    /// How long a stationary step lasts. Walking steps run until the Bim
    /// arrives, so their length here is ignored, and a doze runs for as long
    /// as the task was told to sleep — see `Task::duration`.
    fn duration(self) -> f32 {
        use Step::*;
        match self {
            OpenFridge | CloseFridge | OpenStoreForStew | ShutStoreOnStew => 0.7,
            TakeVegetable | TakeKnife | TakePlateAndSpoon | TakeStew => 0.8,
            // Ladling a pot into a tub, and a tub into a pot.
            PackStew => 1.2,
            StowStew => 0.7,
            TipStewIntoPot => 0.9,
            PutVegetableDown | PutKnifeDown | SetPlateDown | PickUpPlate => 0.5,
            GatherSlices => 0.6,
            TakeBowl => 0.8,
            FillBowl => 1.2,
            TipIntoPot => 0.9,
            TurnStoveOn | TurnStoveOff => 0.5,
            Cook => COOK_TIME,
            Chop => CHOP_PERIOD * CHOPS as f32,
            Serve => SCOOP_PERIOD * SCOOPS as f32,
            SitDown | StandUp => 0.6,
            ClearTable => 0.7,
            OpenDishwasher | ShutDishwasher => 0.6,
            StackDishes => 1.1,
            StartDishwasher => 0.6,
            FlipSwitch => 0.5,
            WorkTray => TRAY_TIME,
            TakeBroom | PutBroomBack => 0.7,
            Sweep => SWEEP_TIME,
            Talk => CHAT_TIME,
            Shower => clock::seconds(crate::needs::SHOWER_MINUTES),
            TakeSuit | PutSuitBack => 1.5,
            StepOut | StepIn => 1.0,
            TakeMaterials | DropMaterials => 1.2,
            StowCrop => 0.7,
            OpenDoor | ShutDoor | UnlockDoor | ShutDoorBehind => 0.7,
            SitOnToilet | RiseFromToilet => 0.7,
            UseToilet => TOILET_TIME,
            FlushToilet => 0.8,
            WashHands => WASH_TIME,
            ClimbIntoBed => 1.1,
            WakeUp => 1.4,
            ClimbOutOfBed => 1.0,
            Eat => BITE_PERIOD * BITES as f32,
            Rest => 1.6,
            _ => 0.0,
        }
    }

    fn is_walk(self) -> bool {
        use Step::*;
        matches!(
            self,
            GoToFridge
                | CarryToBoard
                | GoToDrawerForKnife
                | BackToBoard
                | CarryToPot
                | GoToDrawerForPlate
                | BackToStove
                | CarryToTable
                | CarryToDishwasher
                | GoToDrawerForBowl
                | BackToBoardWithBowl
                | CarryStewToStore
                | CarryStewToPot
                | GoToSwitch
                | GoToTray
                | CarryCropToStore
                | GoToLocker
                | CarryBroomTo
                | BackToLocker
                | GoToBed
                | GoToDoor
                | StepInside
                | GoToToilet
                | GoToSink
                | BackToDoor
                | StepOutside
                | GoToMeet
                | GoToShower
                | GoToBench
                | GoToSuitLocker
                | GoToGangway
                | WalkToRock
                | WalkToPort
                | BackToSuitLocker
                | GoToShelf
                | CarryToSite
                | GoToSite
                | GoToPatient
        )
    }

    /// Whether finishing this step makes a mess of the deck around it.
    ///
    /// The hands-in-it steps and nothing else: a knife going through a
    /// vegetable, a pot being tipped and served, a pair of hands in a
    /// hydroponic tray, a crop going into the cold store. Walking between
    /// them is not dirty work — otherwise the length of the errand would set
    /// how filthy it is, and carrying a plate across the room would foul more
    /// deck than chopping the meal did.
    ///
    /// Clearing up afterwards is deliberately not on the list. The dishwasher
    /// is where the mess *goes*.
    fn is_dirty_work(self) -> bool {
        use Step::*;
        matches!(
            self,
            Chop | GatherSlices
                | TipIntoPot
                | Serve
                | FillBowl
                | WorkTray
                | StowCrop
                | PackStew
                | TipStewIntoPot
        )
    }
}

/// Which errand is running. The host labels its status line off this, so a
/// trip to the heads does not announce itself as cooking.
#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    /// A meal, of one recipe or the other. Both start the same way — fridge,
    /// board, knife — and part company once the chopping is done.
    Meal(Dish),
    /// A stew for the cold store rather than the table: a vegetable and a
    /// block of tofu, chopped one after the other, cooked, and put away in
    /// a tub. What the manager's stew target is met with.
    Batch,
    /// A stew out of the cold store, warmed up and eaten. What a hungry Bim
    /// does when there is one on the shelf, instead of cooking from raw.
    Reheat,
    /// Walking over to something and working it by hand. Every switch aboard
    /// goes through this: none of them answer from across the room.
    Switch(Switch),
    Rest,
    Heads,
    /// Helping itself to what is left in the pot: a plate, the rest of the
    /// stew, and the same sit-down and clearing-up as a fresh meal. No
    /// fridge, no knife, no hob.
    Leftovers,
    /// Sweeping the deck: the broom out of its locker, a few tiles put right,
    /// and the broom away again. The only errand that undoes a mess rather
    /// than making or avoiding one.
    Clean,
    /// One tray of one hydroponic bay: whatever that tray wants when the
    /// Bim gets to it. One errand per tray, so an interruption costs a
    /// tray and not the whole bay.
    Tend {
        bay: usize,
        spot: usize,
    },
    /// Going over and having a word with the other one. The only errand that
    /// takes two Bims, and the only one that is **dropped rather than queued**
    /// when it is interrupted — half a conversation is worth nothing, and the
    /// other half will have walked off. See `Game::interrupt`.
    Chat,
    /// A shower, for a Bim a day's grime has caught up with. Only aboard a
    /// room that has one — see `Room::shower`.
    Shower,
    /// Making one recipe at one bench: `recipe` indexes
    /// `shipdesign::recipes::RECIPES` and `bench` the room's `benches`. How
    /// long it takes rides in `rest_minutes`, the way a doze's length does,
    /// because the room has no recipe table to read it off.
    Craft {
        recipe: u32,
        bench: usize,
    },
    /// A walk outside to mine the marked rocks, in a suit. How long one
    /// rock takes rides in `rest_minutes`. Only aboard a room with a suit
    /// locker and a port — see `Room::suit_locker` and `Room::gangway` —
    /// and only when the world says the ship is at a site with rocks
    /// marked and a suit aboard; see `Game::set_eva`.
    Eva,
    /// One load of materials from a shelf to construction site `site`, put
    /// down there. `outside` is whether the site is beyond the hull, in
    /// which case the walk goes out through the airlock in a suit and back
    /// in again after — decided when the errand is begun, off whether any
    /// tile beside the site can be stood on from the deck. Which materials
    /// and how many is the world's: the room says a load was taken and a
    /// load arrived, on `Room::picked` and `Room::dropped`.
    Haul {
        site: u32,
        outside: bool,
    },
    /// Putting construction site `site` together, standing beside it, for
    /// the minutes riding in `rest_minutes` the way a craft's do. `outside`
    /// as for a haul. The room says it is done on `Room::built`; the world
    /// puts the part down.
    Build {
        site: u32,
        outside: bool,
    },
    /// Dressing every wound on one part of `patient` — a crewmate, or the
    /// Bim itself — with a bandage out of the room's store. `part` is a
    /// `health::Part` code, carried as a number because the room never
    /// reads it: `Dress` hands the pair back on `Room::dressed` and the game
    /// does the dressing. The walk goes to where the patient stands as the
    /// walk is entered — `Room::crew` — and the game checks the two are
    /// still together when the hands come off. How long it takes rides in
    /// `rest_minutes`, [`BANDAGE_MINUTES`].
    Bandage {
        patient: usize,
        part: u32,
    },
}

impl Kind {
    /// Where this chain begins. Needed to walk a chain from the top, which is
    /// how a half-finished one works out where to pick itself up.
    fn first_step(self) -> Step {
        match self {
            Kind::Meal(_) | Kind::Batch | Kind::Reheat => Step::GoToFridge,
            Kind::Switch(_) => Step::GoToSwitch,
            Kind::Rest => Step::GoToBed,
            Kind::Heads => Step::GoToDoor,
            Kind::Leftovers => Step::GoToDrawerForPlate,
            Kind::Tend { .. } => Step::GoToTray,
            Kind::Clean => Step::GoToLocker,
            Kind::Chat => Step::GoToMeet,
            Kind::Shower => Step::GoToShower,
            Kind::Craft { .. } => Step::GoToBench,
            Kind::Eva => Step::GoToSuitLocker,
            Kind::Haul { .. } => Step::GoToShelf,
            Kind::Build { outside: true, .. } => Step::GoToSuitLocker,
            Kind::Build { outside: false, .. } => Step::GoToSite,
            Kind::Bandage { .. } => Step::GoToPatient,
        }
    }

    /// The site a building errand is about, or `None` for every other
    /// errand.
    pub fn site(self) -> Option<u32> {
        match self {
            Kind::Haul { site, .. } | Kind::Build { site, .. } => Some(site),
            _ => None,
        }
    }

    /// Whether this errand goes out through the airlock.
    fn goes_outside(self) -> bool {
        matches!(
            self,
            Kind::Eva | Kind::Haul { outside: true, .. } | Kind::Build { outside: true, .. }
        )
    }

    /// The step after `here` where this chain turns off the straight line
    /// `Step::next` draws — the forks that are the same for `steps` and for
    /// `Task::next_step`, so the two cannot disagree about the shape of a
    /// chain. `None` where the line is followed.
    fn fork(self, here: Step) -> Option<Step> {
        Some(match (self, here) {
            // A bowl turns off towards the drawer where a stew carries on
            // to the pot.
            (Kind::Meal(Dish::Bowl), Step::PutKnifeDown) => Step::GoToDrawerForBowl,
            // Off the hob and into a tub, rather than onto a plate.
            (Kind::Batch, Step::Cook) => Step::TurnStoveOff,
            (Kind::Batch, Step::TurnStoveOff) => Step::PackStew,
            // A tub out of the store, and from the pot on the meal chain
            // takes over.
            (Kind::Reheat, Step::OpenFridge) => Step::TakeStew,
            (Kind::Reheat, Step::CloseFridge) => Step::CarryStewToPot,
            // Nothing was lit, so there is no hob to turn off.
            (Kind::Leftovers, Step::Serve) => Step::PickUpPlate,
            // The bay borrows the meal chain's fridge steps and leaves by
            // its own door: open, put the harvest in, shut, done. Without
            // these two it would carry on into the meal chain and start
            // chopping.
            (Kind::Tend { .. }, Step::OpenFridge) => Step::StowCrop,
            (Kind::Tend { .. }, Step::CloseFridge) => Step::Done,
            // A load for a site outside goes out through the airlock with
            // the Bim, and the Bim comes back in after putting it down; a
            // build outside is the same walk round the work.
            (Kind::Haul { outside: true, .. }, Step::TakeMaterials) => Step::GoToSuitLocker,
            (Kind::Haul { outside: true, .. }, Step::StepOut) => Step::CarryToSite,
            (Kind::Haul { outside: true, .. }, Step::DropMaterials) => Step::WalkToPort,
            (Kind::Build { outside: true, .. }, Step::StepOut) => Step::GoToSite,
            (Kind::Build { outside: true, .. }, Step::Construct) => Step::WalkToPort,
            _ => return None,
        })
    }

    /// Every step of the chain, in order, with the chopping counted once.
    ///
    /// A stew goes round the chopping twice, but the second pass is the same
    /// steps again, so it is added back by weight in `progress_of` rather than
    /// walked here. What this does have to get right is the fork: a bowl turns
    /// off towards the drawer where a stew carries on to the pot.
    fn steps(self) -> impl Iterator<Item = Step> {
        let mut at = Some(self.first_step());
        core::iter::from_fn(move || {
            let here = at?;
            let next = match (self, here) {
                (Kind::Meal(_), Step::Chop) => Step::PutKnifeDown,
                // One rock, counted once: the chain goes round for every rock
                // it can reach, and how many that is nothing here can know.
                (Kind::Eva, Step::Mine) => Step::WalkToPort,
                _ => self.fork(here).unwrap_or_else(|| here.next()),
            };
            at = if next == Step::Done { None } else { Some(next) };
            Some(here)
        })
    }
}

/// A chain put down part-way through, and everything needed to pick it up.
///
/// Which of the room's fixtures a chain is using, one index a kind, chosen
/// as the chain first walks to one and kept for the rest of the errand.
///
/// This is how a second hob is a hob somebody cooks on: a step that goes
/// to a fixture asks for the closest one nobody else has (`closest_free`),
/// and every later step of the same errand goes back to that one — the
/// slices are on *that* board, the pot is on *that* hob. What another Bim
/// holds is [`Taken`], gathered by the game from every other Bim's errand
/// and queue. A pick is kept across a suspend, since a half-cooked meal's
/// pot is still on its hob. `None` is not yet chosen; a room always has at
/// least one of each, so `unwrap_or(0)` is never wrong, only early.
#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub struct Picks {
    pub worktop: Option<usize>,
    pub hob: Option<usize>,
    pub fridge: Option<usize>,
    pub dishwasher: Option<usize>,
    pub bath: Option<usize>,
    pub shower: Option<usize>,
    pub locker: Option<usize>,
}

impl Picks {
    /// Whether any fixture this chain holds is one of `other`'s.
    pub fn clashes(&self, other: &Taken) -> bool {
        let held =
            |mine: Option<usize>, theirs: &[usize]| mine.is_some_and(|i| theirs.contains(&i));
        held(self.worktop, &other.worktops)
            || held(self.hob, &other.hobs)
            || held(self.fridge, &other.fridges)
            || held(self.dishwasher, &other.dishwashers)
            || held(self.bath, &other.baths)
            || held(self.shower, &other.showers)
            || held(self.locker, &other.lockers)
    }
}

/// Every fixture the *other* Bims are using — their errands' picks and
/// their queued chains' — so a pick can go to the next one along.
#[derive(Clone, Default, PartialEq, Debug)]
pub struct Taken {
    pub worktops: Vec<usize>,
    pub hobs: Vec<usize>,
    pub fridges: Vec<usize>,
    pub dishwashers: Vec<usize>,
    pub baths: Vec<usize>,
    pub showers: Vec<usize>,
    pub lockers: Vec<usize>,
}

impl Taken {
    pub fn add(&mut self, picks: &Picks) {
        let put = |list: &mut Vec<usize>, pick: Option<usize>| {
            if let Some(i) = pick
                && !list.contains(&i)
            {
                list.push(i);
            }
        };
        put(&mut self.worktops, picks.worktop);
        put(&mut self.hobs, picks.hob);
        put(&mut self.fridges, picks.fridge);
        put(&mut self.dishwashers, picks.dishwasher);
        put(&mut self.baths, picks.bath);
        put(&mut self.showers, picks.shower);
        put(&mut self.lockers, picks.locker);
    }
}

/// Which kind of fixture a step walks to, if any.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Fixture {
    Worktop,
    Hob,
    Fridge,
    Dishwasher,
    Bath,
    Shower,
    Locker,
}

fn fixture_of(step: Step) -> Option<Fixture> {
    use Step::*;
    Some(match step {
        GoToFridge | CarryCropToStore | CarryStewToStore => Fixture::Fridge,
        CarryToBoard | BackToBoard | BackToBoardWithBowl | GoToDrawerForKnife
        | GoToDrawerForPlate | GoToDrawerForBowl => Fixture::Worktop,
        CarryToPot | CarryStewToPot | BackToStove => Fixture::Hob,
        CarryToDishwasher => Fixture::Dishwasher,
        GoToLocker | BackToLocker => Fixture::Locker,
        GoToDoor | StepOutside | StepInside | BackToDoor | GoToToilet | GoToSink => Fixture::Bath,
        GoToShower => Fixture::Shower,
        _ => return None,
    })
}

/// The fixture a step wants, picked if it has not been: the closest one
/// nobody else has, from `from`. Some picks ask more of a fixture than
/// being free — leftovers want a hob with something in the pot, a broom
/// wants a locker with a broom in it. `false` when there is none to be
/// had, which is a chain with nowhere to go.
fn pick_for(
    picks: &mut Picks,
    kind: Kind,
    step: Step,
    room: &Room,
    from: Vec2,
    taken: &Taken,
) -> bool {
    let Some(fixture) = fixture_of(step) else {
        return true;
    };
    match fixture {
        Fixture::Fridge => {
            if picks.fridge.is_none() {
                picks.fridge = closest_free(room.fridges.iter().map(|f| f.frame), from, |i| {
                    taken.fridges.contains(&i)
                });
            }
            picks.fridge.is_some()
        }
        Fixture::Worktop => {
            if picks.worktop.is_none() {
                picks.worktop = closest_free(room.worktops.iter().map(|w| w.frame), from, |i| {
                    taken.worktops.contains(&i)
                });
            }
            picks.worktop.is_some()
        }
        Fixture::Hob => {
            if picks.hob.is_none() {
                let wants_leftovers = kind == Kind::Leftovers;
                picks.hob = closest_free(room.hobs.iter().map(|h| h.frame), from, |i| {
                    taken.hobs.contains(&i) || (wants_leftovers && room.hobs[i].pot_servings == 0)
                });
            }
            picks.hob.is_some()
        }
        Fixture::Dishwasher => {
            if picks.dishwasher.is_none() {
                picks.dishwasher =
                    closest_free(room.dishwashers.iter().map(|d| d.face), from, |i| {
                        taken.dishwashers.contains(&i)
                    });
            }
            picks.dishwasher.is_some()
        }
        Fixture::Locker => {
            if picks.locker.is_none() {
                picks.locker = closest_free(room.lockers.iter().map(|l| l.frame), from, |i| {
                    taken.lockers.contains(&i) || room.lockers[i].broom_out
                });
            }
            picks.locker.is_some()
        }
        Fixture::Bath => {
            if picks.bath.is_none() {
                picks.bath = closest_free(
                    (0..room.baths()).map(|i| room.bath_at(i).toilet),
                    from,
                    |i| taken.baths.contains(&i),
                );
            }
            picks.bath.is_some()
        }
        Fixture::Shower => {
            if picks.shower.is_none() {
                picks.shower = closest_free(room.showers.iter().map(|(f, _)| *f), from, |i| {
                    taken.showers.contains(&i)
                });
            }
            picks.shower.is_some()
        }
    }
}

/// Every kind of fixture an errand of `kind` walks to, in the order it
/// walks to them. What `can_pick_all` checks before the errand starts.
fn fixtures_used(kind: Kind) -> &'static [Fixture] {
    match kind {
        Kind::Meal(_) | Kind::Batch => &[Fixture::Fridge, Fixture::Worktop, Fixture::Hob],
        Kind::Reheat => &[Fixture::Fridge, Fixture::Hob, Fixture::Worktop],
        Kind::Leftovers => &[Fixture::Hob, Fixture::Worktop],
        Kind::Tend { .. } => &[Fixture::Fridge],
        Kind::Heads => &[Fixture::Bath],
        Kind::Shower => &[Fixture::Shower],
        Kind::Clean => &[Fixture::Locker],
        _ => &[],
    }
}

/// Whether an errand of `kind` could pick one of everything it walks to,
/// from `from`, with what the others hold: what `Game::can_begin` asks
/// before starting one, so a Bim does not set out for a galley with no
/// free hob in it and drop its slices when it gets there. Each is picked
/// as if from where the Bim stands now; the chain picks again, properly,
/// step by step.
pub fn can_pick_all(kind: Kind, room: &Room, from: Vec2, taken: &Taken) -> bool {
    let mut picks = Picks::default();
    fixtures_used(kind).iter().all(|&fixture| {
        let step = match fixture {
            Fixture::Fridge => Step::GoToFridge,
            Fixture::Worktop => Step::CarryToBoard,
            Fixture::Hob => Step::CarryToPot,
            Fixture::Dishwasher => Step::CarryToDishwasher,
            Fixture::Bath => Step::GoToToilet,
            Fixture::Shower => Step::GoToShower,
            Fixture::Locker => Step::GoToLocker,
        };
        pick_for(&mut picks, kind, step, room, from, taken)
    })
}

/// The room remembers most of it by itself — a chopped vegetable stays
/// chopped, a pot stays full — so what has to be carried here is the handful
/// of things that live on the Bim and are thrown away when a task lets go of
/// it: which step it had reached and how far into it, and what it was holding
/// or sitting on.
pub struct Saved {
    /// Which Bim this chain belongs to. A chain is never handed over — it goes
    /// back on the queue of the Bim that put it down — but the bed and the
    /// chair it walks to are that Bim's own, so the index has to survive being
    /// put down as much as the progress does.
    who: usize,
    kind: Kind,
    step: Step,
    elapsed: f32,
    done_count: u32,
    rest_minutes: f32,
    chopped: u32,
    /// The tile it was on its way to sweep, and how many it has done. Kept
    /// across a suspend so a chain picked back up finishes the job rather
    /// than starting the count again.
    target: Option<Vec2>,
    swept: u32,
    /// What the Bim was carrying up from the bay. Held here as well as in the
    /// hands because the hands only know it is a vegetable; the store wants
    /// the crop. Lose this and an interrupted harvest is a harvest thrown
    /// away.
    /// Public for `Game::take_crew`, which banks a suspended chain's crop
    /// when the Bim leaves the room for good.
    pub lifted: Option<Crop>,
    started_inside: bool,
    main: Held,
    tool: Held,
    /// Where it was sitting or lying, and which way it faced, if it was.
    seat: Option<(Vec2, f32)>,
    /// The fixtures it was using, kept: its pot is still on its hob.
    picks: Picks,
}

impl Saved {
    /// An errand that has not started yet, dressed as one that was put down at
    /// its first step. It is the same thing to everything downstream — the
    /// queue, the agenda, the resume — which is what lets work be *added* to
    /// the queue rather than only ever put back into it.
    pub fn fresh(who: usize, kind: Kind, rest_minutes: f32) -> Saved {
        Saved {
            who,
            kind,
            step: kind.first_step(),
            elapsed: 0.0,
            done_count: 0,
            rest_minutes,
            chopped: 0,
            target: None,
            swept: 0,
            lifted: None,
            started_inside: false,
            main: Held::Nothing,
            tool: Held::Nothing,
            seat: None,
            picks: Picks::default(),
        }
    }

    pub fn picks(&self) -> Picks {
        self.picks
    }

    pub fn kind(&self) -> Kind {
        self.kind
    }

    /// Whether a tub of stew was in the hands when the chain was put down.
    /// `Game::take_crew` banks it when the Bim leaves the room for good, the
    /// same as `lifted`.
    pub fn holds_stew(&self) -> bool {
        self.main == Held::Stew
    }

    /// How far through the chain this was, for the host's readout.
    pub fn progress(&self) -> f32 {
        progress_of(
            self.kind,
            self.step,
            self.elapsed,
            self.rest_minutes,
            self.chopped,
        )
    }

    pub fn rest_minutes(&self) -> f32 {
        self.rest_minutes
    }

    /// Where picking this chain up again would first walk the Bim to.
    pub fn resume_station(&self, room: &Room, from: Vec2) -> Option<Vec2> {
        destination(
            self.who,
            self.kind,
            rewind(self.kind, self.step),
            room,
            from,
            self.target,
            self.picks,
        )
    }

    /// Put back what `set_scripted(false)` threw away.
    fn restore(&self, ch: &mut Character) {
        ch.hold_main(self.main);
        ch.hold_tool(self.tool);
        if let Some((at, facing)) = self.seat {
            ch.sit(at, facing);
        }
    }
}

/// Where a walking step is headed, or `None` for a step that stands still.
///
/// Out here rather than inside `enter` so that the question "could the Bim
/// actually get there?" can be asked before a chain is started, with the same
/// answer the chain itself would get.
/// `who` is which Bim the chain belongs to. Most of the room is shared and
/// does not care, but a bed and a chair are not: each Bim has its own, and a
/// chain that walked to "the" bed would put two Bims in one bunk.
fn destination(
    who: usize,
    kind: Kind,
    step: Step,
    room: &Room,
    from: Vec2,
    target: Option<Vec2>,
    picks: Picks,
) -> Option<Vec2> {
    use Step::*;
    // The fixtures this chain picked, or the first of each for a step asked
    // about before any pick — a resume station, an opening walk.
    let (w, h, f) = (
        picks.worktop.unwrap_or(0),
        picks.hob.unwrap_or(0),
        picks.fridge.unwrap_or(0),
    );
    let (d, b, s, l) = (
        picks.dishwasher.unwrap_or(0),
        picks.bath.unwrap_or(0),
        picks.shower.unwrap_or(0),
        picks.locker.unwrap_or(0),
    );
    match step {
        GoToFridge | CarryCropToStore | CarryStewToStore => Some(room.fridge_station(f)),
        CarryToBoard | BackToBoard | BackToBoardWithBowl => Some(room.board_station(w)),
        GoToDrawerForKnife | GoToDrawerForPlate | GoToDrawerForBowl => Some(room.drawer_station(w)),
        CarryToPot | CarryStewToPot => Some(room.stove_station(h)),
        // Serving needs pot and plate either side, so it has its own spot.
        BackToStove => Some(room.serve_station(h)),
        // The chair sits clear of the table footprint, so the Bim can actually
        // stand on it before sitting down — and it is this Bim's chair, not
        // the other one's.
        CarryToTable => Some(room.chair_at(who)),
        GoToSwitch => match kind {
            Kind::Switch(which) => Some(room.switch_station(which, from)),
            _ => None,
        },
        GoToTray => match kind {
            Kind::Tend { bay, spot } => room.bays.get(bay).map(|b| b.station(spot)),
            _ => None,
        },
        CarryToDishwasher => Some(room.dishwasher_station(d)),
        GoToLocker | BackToLocker => Some(room.locker_station(l)),
        // Wherever the broom is wanted next. Picked as the step is entered and
        // carried on the task: a tile is chosen off the deck, and this is
        // handed the room without being told where to look.
        CarryBroomTo => target,
        // The spot this Bim is to stand on for the conversation. Worked out
        // by `Game`, which is the only thing that knows where the other one
        // is, and fixed when the errand starts: walking to a Bim that is
        // itself walking has no end, because a route is planned once here and
        // never replanned.
        GoToMeet => target,
        GoToBed => Some(room.bed_station(who)),
        GoToDoor | StepOutside => Some(room.bath_at(b).outside_station()),
        StepInside | BackToDoor => Some(room.bath_at(b).inside_station()),
        GoToToilet => Some(room.bath_at(b).toilet_station()),
        GoToSink => Some(room.bath_at(b).sink_station()),
        // A room without a shower has nowhere to send the Bim, and the errand
        // is not begun — `Game::take_shower` asks first.
        GoToShower => room.shower_station(s),
        GoToBench => match kind {
            Kind::Craft { bench, .. } => room.benches.get(bench).map(|b| b.at),
            _ => None,
        },
        // A room without a suit locker or a port has nowhere to send the
        // Bim, and the errand is not begun — `Game::can_go_outside` asks.
        GoToSuitLocker | BackToSuitLocker => room.suit_locker_station(),
        GoToGangway => room.gangway,
        // Beside the rock chosen as the step was entered, the way a tile to
        // sweep is; and back to the spot outside the port.
        WalkToRock => target,
        WalkToPort => room.outside,
        // The nearest shelf, and a tile beside the site, both chosen as the
        // step is entered — from wherever the Bim is standing, on whichever
        // grid it is standing on.
        GoToShelf | CarryToSite | GoToSite => target,
        // Beside the patient, chosen as the walk is entered from where the
        // patient stands then.
        GoToPatient => target,
        _ => None,
    }
}

/// Where a chain begins, given where the Bim happens to be standing.
///
/// Only the heads has two answers. Its chain is written for a Bim out on the
/// deck — walk to the door, let itself in, shut it behind — and a Bim already
/// in there would be sent round to a handle on the wrong side of the bulkhead.
/// From inside it starts at the pan instead.
pub fn opening_step(kind: Kind, room: &Room, from: Vec2) -> Step {
    match kind {
        Kind::Heads if room.bath.shell.contains(from) => Step::GoToToilet,
        _ => kind.first_step(),
    }
}

/// Where a chain would first have to walk to, were it started now. `None` when
/// it starts on the spot and so cannot be blocked at the outset.
/// The next tile worth the broom that the Bim can actually walk to.
///
/// One place, called both when the walk is set up and when the chain asks
/// itself whether there is more to do. If the two disagreed the chain would
/// loop: "yes, more to sweep" followed by "but nowhere to go" is a step of no
/// length that comes straight back round.
fn next_dirty(room: &Room, maps: &Maps, from: Vec2) -> Option<Vec2> {
    let nav = maps.pick(room.bath.is_open());
    room.filth.worst_tile(from, |at| nav.can_reach(from, at))
}

pub fn first_station(
    who: usize,
    kind: Kind,
    room: &Room,
    from: Vec2,
    taken: &Taken,
) -> Option<Vec2> {
    let step = opening_step(kind, room, from);
    let mut picks = Picks::default();
    pick_for(&mut picks, kind, step, room, from, taken);
    destination(who, kind, step, room, from, None, picks)
}

/// The steps of an errand outside that happen beyond the door: from
/// stepping out to stepping in, for a walk to mine and for a load or a
/// build at a site outside the hull alike. Nothing for an errand that
/// stays aboard.
fn outside_half(kind: Kind, step: Step) -> bool {
    if !kind.goes_outside() {
        return false;
    }
    matches!(
        step,
        Step::StepOut
            | Step::PickRock
            | Step::WalkToRock
            | Step::Mine
            | Step::CarryToSite
            | Step::DropMaterials
            | Step::GoToSite
            | Step::Construct
            | Step::WalkToPort
    )
}

/// The construction site an errand is about, as the world described it
/// this step, or `None` when the world no longer wants it — cancelled, or
/// built by somebody else.
fn site_of(room: &Room, site: u32) -> Option<&crate::game::Build> {
    room.builds.iter().find(|b| b.site == site)
}

/// The middle of a site: of the box round its tiles.
fn site_middle(build: &crate::game::Build) -> Vec2 {
    let mut lo = vec2(f32::MAX, f32::MAX);
    let mut hi = vec2(f32::MIN, f32::MIN);
    for tile in &build.tiles {
        lo = vec2(lo.x.min(tile.min.x), lo.y.min(tile.min.y));
        hi = vec2(hi.x.max(tile.max.x), hi.y.max(tile.max.y));
    }
    (lo + hi) * 0.5
}

/// Where to stand to work site `site` from `from`, on the grid a body
/// `outside` or not is on: the nearest tile beside its footprint — four
/// ways, never a corner, like a rock — that the grid has a route to, and
/// failing that the nearest tile *of* the footprint, since deck plating
/// and conduit go under the Bim's own feet. `None` when the site is not
/// there, or nothing beside it can be stood on from here — which, asked of
/// the deck's grid, is what says a site is outside the hull.
///
/// One place, asked when the errand is begun and again as the walk to the
/// site is entered, for the reason `next_dirty` gives: the two have to
/// agree or the chain loops.
pub fn site_stand(room: &Room, maps: &Maps, site: u32, from: Vec2, outside: bool) -> Option<Vec2> {
    let build = site_of(room, site)?;
    let nav = if outside {
        maps.outside()?
    } else {
        maps.pick(room.bath.is_open())
    };
    let t = crate::filth::TILE;
    let mut ring: Vec<Vec2> = Vec::new();
    let mut own: Vec<Vec2> = Vec::new();
    for tile in &build.tiles {
        own.push(tile.center());
        for (dx, dy) in [(0.0, -1.0), (-1.0, 0.0), (1.0, 0.0), (0.0, 1.0)] {
            let p = tile.center() + vec2(dx * t, dy * t);
            if !build.tiles.iter().any(|r| r.contains(p)) && !ring.contains(&p) {
                ring.push(p);
            }
        }
    }
    let pick = |mut spots: Vec<Vec2>| {
        spots.sort_by(|a, b| {
            (*a - from)
                .len()
                .partial_cmp(&(*b - from).len())
                .unwrap_or(core::cmp::Ordering::Equal)
        });
        spots
            .into_iter()
            .find(|&p| nav.interior().contains(p) && nav.is_free(p) && nav.can_reach(from, p))
    };
    pick(ring).or_else(|| pick(own))
}

/// Where to stand to dress `patient`, for `who` standing at `from`: the
/// spot it is already on for its own wounds, else the nearest free cell to
/// a point a tile from the patient towards the helper — beside the body,
/// not on it, so two of them do not stand in one spot for ten minutes.
/// `None` for a patient the room has no position for (dead, outside) or
/// no route to.
///
/// Asked as the walk is entered, from where the patient stands *then*
/// (`Room::crew`), so a chain picked up off the queue walks to where the
/// patient has got to rather than where it was.
fn patient_stand(room: &Room, maps: &Maps, who: usize, patient: usize, from: Vec2) -> Option<Vec2> {
    if patient == who {
        return Some(from);
    }
    let at = room.crew.get(patient).copied().flatten()?;
    let nav = maps.pick(room.bath.is_open());
    let toward = from - at;
    let step = if toward.len() > 1e-3 {
        toward * (crate::filth::TILE / toward.len())
    } else {
        vec2(crate::filth::TILE, 0.0)
    };
    let stand = nav.nearest_free(at + step);
    nav.can_reach(from, stand).then_some(stand)
}

/// The nearest shelf the Bim can get to from `from`, and where it stands
/// at it; `None` in a room with no shelf it can reach.
pub fn nearest_shelf(room: &Room, maps: &Maps, from: Vec2) -> Option<Vec2> {
    let nav = maps.pick(room.bath.is_open());
    let mut shelves: Vec<Vec2> = room.shelves.iter().map(|&(_, at)| at).collect();
    shelves.sort_by(|a, b| {
        (*a - from)
            .len()
            .partial_cmp(&(*b - from).len())
            .unwrap_or(core::cmp::Ordering::Equal)
    });
    shelves.into_iter().find(|&at| nav.can_reach(from, at))
}

/// The next marked rock the Bim can get to from `from`, and where to stand
/// to mine it: the nearest marked rock with a tile beside it — four ways,
/// never a diagonal — that the outside grid has a route to. `None` with no
/// outside grid, no rocks marked, or none of them reachable.
///
/// Four ways and not eight because of what a dig is: the tile mined is the
/// tile the Bim stands in to reach the one behind it, and a body cannot
/// squeeze diagonally between two rocks into a tile that was mined from
/// its corner — the grid refuses that, rightly — so a rock mined from a
/// corner would be a pocket nothing can get into, and the rock behind it
/// marked for nothing.
///
/// One place, asked both when the walk is set up and when the chain asks
/// itself whether there is another, for the same reason as `next_dirty`:
/// the two have to agree or the chain loops.
pub fn next_rock(room: &Room, maps: &Maps, from: Vec2) -> Option<(Vec2, Vec2)> {
    reachable_rocks(room, maps, from).into_iter().next()
}

/// Every marked rock the Bim can get to from `from`, nearest first, each
/// with the tile to stand on. What `next_rock` picks the first of, and
/// what the room counts to say how many marks are out of reach.
pub fn reachable_rocks(room: &Room, maps: &Maps, from: Vec2) -> Vec<(Vec2, Vec2)> {
    let mut found = Vec::new();
    let Some(nav) = maps.outside() else {
        return found;
    };
    let t = crate::filth::TILE;
    let mut rocks: Vec<Vec2> = room.rock_targets.clone();
    rocks.sort_by(|a, b| {
        (*a - from)
            .len()
            .partial_cmp(&(*b - from).len())
            .unwrap_or(core::cmp::Ordering::Equal)
    });
    for rock in rocks {
        let mut beside: Vec<Vec2> = [(0.0, -1.0), (-1.0, 0.0), (1.0, 0.0), (0.0, 1.0)]
            .into_iter()
            .map(|(dx, dy)| rock + vec2(dx * t, dy * t))
            .collect();
        beside.sort_by(|a, b| {
            (*a - from)
                .len()
                .partial_cmp(&(*b - from).len())
                .unwrap_or(core::cmp::Ordering::Equal)
        });
        for stand in beside {
            if nav.is_free(stand) && nav.can_reach(from, stand) {
                found.push((rock, stand));
                break;
            }
        }
    }
    found
}

/// The step to start at when picking `target` up again.
///
/// Standing steps assume the Bim is already in the right place — `Chop` chops
/// whatever is in front of it — so resuming rewinds to the most recent walking
/// step and lets the Bim walk back to the bench first. The steps skipped on
/// the way are the ones whose work the room is already holding.
fn rewind(kind: Kind, target: Step) -> Step {
    // An errand outside put down out there — or at the door — starts its
    // outside half again from the gangway: the body was brought in when
    // the chain was put down, and the rock or the spot beside the site is
    // chosen afresh once it is out again. See `Task::resume`.
    if outside_half(kind, target) {
        return Step::GoToGangway;
    }
    let mut back_at = kind.first_step();
    for step in kind.steps() {
        if step.is_walk() {
            back_at = step;
        }
        if step == target {
            break;
        }
    }
    back_at
}

/// A rough length for a step, for the progress readout only. Walking steps
/// have no fixed length — it depends where the Bim happens to be standing —
/// so they get a nominal figure. Being a second out only nudges a bar.
const NOMINAL_WALK: f32 = 3.0;

fn weight(step: Step, rest_minutes: f32) -> f32 {
    if step.is_walk() {
        NOMINAL_WALK
    } else if matches!(
        step,
        Step::Doze | Step::Work | Step::Mine | Step::Construct | Step::Dress
    ) {
        clock::seconds(rest_minutes)
    } else {
        step.duration().max(0.05)
    }
}

/// How far through a chain a given step is, 0 to 1, weighted by how long each
/// step takes rather than by how many there are — otherwise a six-hour sleep
/// would read as one fifth done the moment the Bim lay down.
///
/// `laps_done` is how many vegetables are already chopped. Without it the bar
/// would run backwards when a stew goes round for its second one, because the
/// Bim really is back at the fridge where it started.
fn progress_of(kind: Kind, step: Step, elapsed: f32, rest_minutes: f32, laps_done: u32) -> f32 {
    let laps = laps(kind);

    // Everything up to and including the chopping is the part that repeats;
    // everything after it happens once.
    let (mut round, mut tail) = (0.0, 0.0);
    let (mut in_round, mut in_tail) = (None, None);
    let mut repeating = true;
    for s in kind.steps() {
        let w = weight(s, rest_minutes);
        if repeating {
            if s == step && in_round.is_none() {
                in_round = Some(round);
            }
            round += w;
            repeating = s != Step::Chop;
        } else {
            if s == step && in_tail.is_none() {
                in_tail = Some(tail);
            }
            tail += w;
        }
    }

    let total = round * laps as f32 + tail;
    if total <= 0.0 {
        return 1.0;
    }
    let here = weight(step, rest_minutes);
    let within = if here > 0.0 {
        (elapsed / here).clamp(0.0, 1.0)
    } else {
        0.0
    };
    // Rounds already finished come first, then how far into this one it is.
    let before = match (in_round, in_tail) {
        (Some(w), _) => round * laps_done.min(laps.saturating_sub(1)) as f32 + w,
        (None, Some(w)) => round * laps as f32 + w,
        (None, None) => return 1.0,
    };
    ((before + within * here) / total).clamp(0.0, 1.0)
}

pub struct Task {
    /// Which Bim is doing this. Everything shared in the room is reached
    /// without it; a bed, a chair and a place at the table are not.
    who: usize,
    kind: Kind,
    step: Step,
    /// Time spent in the current step.
    elapsed: f32,
    /// Discrete progress within a repeating step: knife strokes, spoonfuls, bites.
    done_count: u32,
    /// How long the Bim was told to stay in bed, in game minutes. Zero for
    /// every errand that is not a rest.
    rest_minutes: f32,
    /// How many vegetables have been through the knife so far.
    chopped: u32,
    /// Which way to turn once the walking is done. Only a chat uses it: the
    /// two of them face each other, and which way that is depends on where the
    /// other one ended up, which nothing inside a chain can see. Zero and
    /// ignored for everything else, all of which turns to face a fixture whose
    /// heading the room knows.
    ///
    /// Deliberately **not** on `Saved`: a chat is dropped rather than put down
    /// when it is interrupted, so there is no such thing as resuming one.
    face: f32,
    /// Which tile the broom is being taken to, and how many have been done
    /// this time out. The tile is chosen as `CarryBroomTo` is entered and kept
    /// until it is swept, so the Bim sweeps the tile it set out for rather
    /// than whichever one it happened to stop on — the two differ whenever the
    /// dirt is somewhere a body cannot quite stand.
    target: Option<Vec2>,
    swept: u32,
    /// The rock the Bim is walking to or swinging at, by its middle, while
    /// `target` is the tile beside it that it stands on. Chosen as
    /// `WalkToRock` is entered. Not on `Saved`: a walk put down out there
    /// chooses its rock afresh when it goes out again.
    rock: Option<Vec2>,
    /// What the Bim lifted out of a tray and has not put away yet.
    ///
    /// The hands carry it and this remembers what it is, because `Held` knows
    /// a vegetable from a block of tofu but the store wants a `Crop`. It is
    /// also what decides the shape of the rest of the chain: a planting leaves
    /// this empty and the errand ends at the bay, a lifting sends the Bim up
    /// the room with it.
    lifted: Option<Crop>,
    /// Set when a trip to the heads began with the Bim already in there. The
    /// four steps that let it in are skipped, and so are the four that let it
    /// out again — they exist to undo each other, and a Bim that never opened
    /// the door has no business unlocking it on the way past.
    started_inside: bool,
    /// Set when a walk has nowhere to go. The chain is given up on the next
    /// tick rather than pretending it arrived.
    blocked: bool,
    /// Seconds left of standing there having lost the thread. Nothing about
    /// the errand moves on until it runs out.
    stall: f32,
    /// How many times this errand has been fumbled, for the readout.
    fumbles: u32,
    /// Set while walking back to a chain that was put down: the step to drop
    /// into, with its progress, once the Bim is in position again.
    resume: Option<Saved>,
    /// The fixtures this chain is using. See [`Picks`].
    picks: Picks,
}

impl Task {
    /// `rest_minutes` only means anything to a rest; every other errand
    /// passes zero.
    fn starting_at(
        who: usize,
        kind: Kind,
        step: Step,
        rest_minutes: f32,
        ch: &mut Character,
        room: &mut Room,
        maps: &Maps,
        taken: &Taken,
    ) -> Task {
        Task::starting_toward(
            who,
            kind,
            step,
            rest_minutes,
            None,
            0.0,
            ch,
            room,
            maps,
            taken,
        )
    }

    /// The same, for a chain that is walked to a spot rather than to a
    /// fixture: the target has to be on the task *before* `enter` runs, since
    /// that is what looks it up. Sweeping picks its own tile inside `enter`;
    /// a chat is handed its spot by `Game`, which is the only thing that knows
    /// where the other Bim is standing.
    #[allow(clippy::too_many_arguments)]
    fn starting_toward(
        who: usize,
        kind: Kind,
        step: Step,
        rest_minutes: f32,
        target: Option<Vec2>,
        face: f32,
        ch: &mut Character,
        room: &mut Room,
        maps: &Maps,
        taken: &Taken,
    ) -> Task {
        ch.set_scripted(true);
        let mut task = Task {
            who,
            kind,
            step,
            elapsed: 0.0,
            done_count: 0,
            rest_minutes,
            chopped: 0,
            face,
            target,
            swept: 0,
            rock: None,
            lifted: None,
            started_inside: false,
            blocked: false,
            stall: 0.0,
            fumbles: 0,
            resume: None,
            picks: Picks::default(),
        };
        task.enter(ch, room, maps, taken);
        task
    }

    /// Start the make-a-meal chain.
    pub fn make_food(
        who: usize,
        dish: Dish,
        ch: &mut Character,
        room: &mut Room,
        maps: &Maps,
        taken: &Taken,
    ) -> Task {
        room.dish = dish;
        Task::starting_at(
            who,
            Kind::Meal(dish),
            Step::GoToFridge,
            0.0,
            ch,
            room,
            maps,
            taken,
        )
    }

    /// Cook a stew for the cold store: a vegetable and a block of tofu,
    /// chopped, through the pot, and put away in a tub.
    pub fn batch(
        who: usize,
        ch: &mut Character,
        room: &mut Room,
        maps: &Maps,
        taken: &Taken,
    ) -> Task {
        room.dish = Dish::Stew;
        Task::starting_at(
            who,
            Kind::Batch,
            Step::GoToFridge,
            0.0,
            ch,
            room,
            maps,
            taken,
        )
    }

    /// Warm a stew from the cold store through and eat it. The same sit-down
    /// and clearing-up as a meal cooked from raw, and no knife.
    pub fn reheat(
        who: usize,
        ch: &mut Character,
        room: &mut Room,
        maps: &Maps,
        taken: &Taken,
    ) -> Task {
        room.dish = Dish::Stew;
        Task::starting_at(
            who,
            Kind::Reheat,
            Step::GoToFridge,
            0.0,
            ch,
            room,
            maps,
            taken,
        )
    }

    /// Walk over to a switch and work it. Everything the Bim can operate goes
    /// through here, so nothing in the room can be changed without the Bim
    /// being there to change it.
    pub fn work_switch(
        who: usize,
        which: Switch,
        ch: &mut Character,
        room: &mut Room,
        maps: &Maps,
        taken: &Taken,
    ) -> Task {
        Task::starting_at(
            who,
            Kind::Switch(which),
            Step::GoToSwitch,
            0.0,
            ch,
            room,
            maps,
            taken,
        )
    }

    /// Send the Bim to the heads. Refused from the outside if the door is
    /// locked — the host greys the menu item out for the same reason.
    /// Help itself to what is left in the pot: a plate, the rest of the stew,
    /// and the same sit-down and clearing-up as a meal it cooked.
    pub fn leftovers(
        who: usize,
        ch: &mut Character,
        room: &mut Room,
        maps: &Maps,
        taken: &Taken,
    ) -> Task {
        Task::starting_at(
            who,
            Kind::Leftovers,
            Step::GoToDrawerForPlate,
            0.0,
            ch,
            room,
            maps,
            taken,
        )
    }

    /// Fetch the broom and put a few tiles of the deck right.
    pub fn clean(
        who: usize,
        ch: &mut Character,
        room: &mut Room,
        maps: &Maps,
        taken: &Taken,
    ) -> Task {
        Task::starting_at(
            who,
            Kind::Clean,
            Step::GoToLocker,
            0.0,
            ch,
            room,
            maps,
            taken,
        )
    }

    /// Walk to one tray of one bay and do whatever it wants doing.
    pub fn tend(
        who: usize,
        bay: usize,
        spot: usize,
        ch: &mut Character,
        room: &mut Room,
        maps: &Maps,
        taken: &Taken,
    ) -> Task {
        Task::starting_at(
            who,
            Kind::Tend { bay, spot },
            Step::GoToTray,
            0.0,
            ch,
            room,
            maps,
            taken,
        )
    }

    /// Go and stand at `meet`, then turn to `face` and talk.
    ///
    /// Both crew are handed one of these in the same frame, each with its own
    /// spot and its own heading, so neither is walking toward something that
    /// is itself walking. That matters more here than anywhere: a route is
    /// planned once and never replanned, so a Bim sent to where the other one
    /// *was* would converge on empty deck and stand there.
    pub fn chat(
        who: usize,
        meet: Vec2,
        face: f32,
        ch: &mut Character,
        room: &mut Room,
        maps: &Maps,
        taken: &Taken,
    ) -> Task {
        Task::starting_toward(
            who,
            Kind::Chat,
            Step::GoToMeet,
            0.0,
            Some(meet),
            face,
            ch,
            room,
            maps,
            taken,
        )
    }

    pub fn use_toilet(
        who: usize,
        ch: &mut Character,
        room: &mut Room,
        maps: &Maps,
        taken: &Taken,
    ) -> Task {
        let from = opening_step(Kind::Heads, room, ch.pos);
        let inside = from != Kind::Heads.first_step();
        let mut task = Task::starting_at(who, Kind::Heads, from, 0.0, ch, room, maps, taken);
        task.started_inside = inside;
        task
    }

    pub fn shower(
        who: usize,
        ch: &mut Character,
        room: &mut Room,
        maps: &Maps,
        taken: &Taken,
    ) -> Task {
        Task::starting_at(
            who,
            Kind::Shower,
            Step::GoToShower,
            0.0,
            ch,
            room,
            maps,
            taken,
        )
    }

    /// Out for a walk: the suit, the airlock, the marked rocks at `minutes`
    /// each, and back.
    pub fn eva(
        who: usize,
        minutes: f32,
        ch: &mut Character,
        room: &mut Room,
        maps: &Maps,
        taken: &Taken,
    ) -> Task {
        Task::starting_at(
            who,
            Kind::Eva,
            Step::GoToSuitLocker,
            minutes,
            ch,
            room,
            maps,
            taken,
        )
    }

    /// One load of materials from a shelf to site `site`, out through the
    /// airlock and back if the site is `outside` the hull.
    pub fn haul(
        who: usize,
        site: u32,
        outside: bool,
        ch: &mut Character,
        room: &mut Room,
        maps: &Maps,
        taken: &Taken,
    ) -> Task {
        let kind = Kind::Haul { site, outside };
        Task::starting_at(who, kind, kind.first_step(), 0.0, ch, room, maps, taken)
    }

    /// Off to put site `site` together, for `minutes` beside it, out
    /// through the airlock and back if it is `outside` the hull.
    pub fn build(
        who: usize,
        site: u32,
        outside: bool,
        minutes: f32,
        ch: &mut Character,
        room: &mut Room,
        maps: &Maps,
        taken: &Taken,
    ) -> Task {
        let kind = Kind::Build { site, outside };
        Task::starting_at(who, kind, kind.first_step(), minutes, ch, room, maps, taken)
    }

    /// Off to dress `part` of `patient` — over to wherever it stands, and
    /// [`BANDAGE_MINUTES`] with hands on it. The patient may be the Bim
    /// itself, in which case the walk is to the spot it is on.
    pub fn bandage(
        who: usize,
        patient: usize,
        part: u32,
        ch: &mut Character,
        room: &mut Room,
        maps: &Maps,
        taken: &Taken,
    ) -> Task {
        Task::starting_at(
            who,
            Kind::Bandage { patient, part },
            Step::GoToPatient,
            BANDAGE_MINUTES,
            ch,
            room,
            maps,
            taken,
        )
    }

    /// Off to make `recipe` at `bench`, for `minutes` at it.
    pub fn craft(
        who: usize,
        recipe: u32,
        bench: usize,
        minutes: f32,
        ch: &mut Character,
        room: &mut Room,
        maps: &Maps,
        taken: &Taken,
    ) -> Task {
        Task::starting_at(
            who,
            Kind::Craft { recipe, bench },
            Step::GoToBench,
            minutes,
            ch,
            room,
            maps,
            taken,
        )
    }

    /// Send the Bim to bed for `minutes` of game time: up the ladder, under
    /// the covers, and back out again when the clock says so.
    pub fn rest(
        who: usize,
        minutes: f32,
        ch: &mut Character,
        room: &mut Room,
        maps: &Maps,
        taken: &Taken,
    ) -> Task {
        Task::starting_at(
            who,
            Kind::Rest,
            Step::GoToBed,
            minutes,
            ch,
            room,
            maps,
            taken,
        )
    }

    /// Game minutes until the Bim is back on its feet, for the host's
    /// readout, and zero for anything that is not a rest. Waking up and
    /// coming down the ladder count: the readout should not run out while the
    /// Bim is still in bed. The walk there cannot be known in advance, so
    /// during it this reads a little short and then corrects itself.
    pub fn rest_left(&self) -> f32 {
        use Step::*;
        let remaining = |step: Step, elapsed: f32| (step.duration() - elapsed).max(0.0);
        let tail = WakeUp.duration() + ClimbOutOfBed.duration();
        let seconds = match self.step {
            GoToBed | ClimbIntoBed => clock::seconds(self.rest_minutes) + tail,
            Doze => (self.duration() - self.elapsed).max(0.0) + tail,
            WakeUp => remaining(WakeUp, self.elapsed) + ClimbOutOfBed.duration(),
            ClimbOutOfBed => remaining(ClimbOutOfBed, self.elapsed),
            _ => return 0.0,
        };
        seconds * MINUTES_PER_SECOND
    }

    /// The step after this one. Every chain is a straight line except for
    /// the button on the dishwasher, which is only worth pressing when the
    /// machine came up full — so that one step asks the room first.
    fn next_step(&self, room: &Room, maps: &Maps, from: Vec2) -> Step {
        use Step::*;
        if self.step == ShutDishwasher && !room.dishwashers[self.dishwasher()].is_full() {
            return Done;
        }
        if laps(self.kind) > 1 {
            match self.step {
                // Round again for the second vegetable of a stew — or the
                // block of tofu that goes into a shelf stew after the greens.
                Chop if self.chopped < laps(self.kind) => return GoToFridge,
                // On that second trip the knife is already in hand, so the
                // Bim goes straight back to chopping.
                PutVegetableDown if self.chopped >= 1 => return Chop,
                _ => {}
            }
        }
        // A trip that began inside ends at the basin: the steps that let the
        // Bim out are the undoing of the ones that let it in, and it did not
        // use those either.
        if self.kind == Kind::Heads && self.started_inside && self.step == WashHands {
            return Done;
        }
        // Sweeping goes round: one tile, then the next worst, until the deck
        // is clean or the Bim has done its share and puts the broom away.
        // Asked of the deck as it stands, so a mess made while it was sweeping
        // is one it turns round and deals with.
        if self.kind == Kind::Clean {
            // From where the Bim is standing, and asked the same way `enter`
            // asks it: the two have to agree or the chain loops.
            let more = self.swept < TILES_PER_SWEEP && next_dirty(room, maps, from).is_some();
            match self.step {
                TakeBroom | Sweep if more => return CarryBroomTo,
                TakeBroom | Sweep => return BackToLocker,
                _ => {}
            }
        }
        // Out there, the chain goes round: after each rock — and straight
        // after stepping out — a moment to count the rocks again, and then
        // the nearest marked one it can get to, or home when there is none
        // or the world says this Bim is to come in.
        if self.kind == Kind::Eva {
            match self.step {
                StepOut | Mine => return PickRock,
                PickRock => {
                    let may_stay = room.eva_allowed.get(self.who).copied().unwrap_or(false);
                    return if may_stay && next_rock(room, maps, from).is_some() {
                        WalkToRock
                    } else {
                        WalkToPort
                    };
                }
                _ => {}
            }
        }
        // A planting leaves nothing in the Bim's hands, so there is nothing
        // to carry anywhere and the errand is over at the tray. Asked of
        // what was actually lifted rather than of what the bay wanted when
        // the Bim set off: a tray seen to in the meantime leaves the hands
        // empty either way.
        //
        // `Kind::steps` still counts the carrying half, so a planting reads
        // about half done on the agenda when its row vanishes. That is
        // deliberate rather than overlooked: the alternative is to weigh
        // the chain by what the Bim turns out to be holding, and since it
        // is not holding anything until `WorkTray` has finished, the bar
        // would run *backwards* the moment a harvest came up. A bar that
        // stops short beats one that goes back.
        if let Kind::Tend { .. } = self.kind
            && self.step == WorkTray
            && self.lifted.is_none()
        {
            return Done;
        }
        // Everything else that turns off the straight line is the same
        // fork `Kind::steps` takes, so the agenda and the chain agree.
        self.kind
            .fork(self.step)
            .unwrap_or_else(|| self.step.next())
    }

    /// How long the step running now lasts. Everything but a doze is a fixed
    /// length; a doze runs for as long as the Bim was told to sleep.
    fn duration(&self) -> f32 {
        match self.step {
            Step::Doze | Step::Work | Step::Mine | Step::Construct | Step::Dress => {
                clock::seconds(self.rest_minutes)
            }
            step => step.duration(),
        }
    }

    pub fn is_done(&self) -> bool {
        self.step == Step::Done
    }

    pub fn kind(&self) -> Kind {
        self.kind
    }

    pub fn rest_minutes(&self) -> f32 {
        self.rest_minutes
    }

    /// How many times the Bim has lost the thread on this errand.
    pub fn fumbles(&self) -> u32 {
        self.fumbles
    }

    /// True while it is standing there having lost the thread.
    pub fn stalled(&self) -> bool {
        self.stall > 0.0
    }

    /// Cut a doze short. Sleeping on past being rested is not something a Bim
    /// does, and a scheduled early night is exactly the case that produces it.
    /// Anything that is not a doze is left alone.
    pub fn wake(&mut self) {
        if self.step == Step::Doze {
            self.elapsed = self.elapsed.max(self.duration());
        }
    }

    /// Which need the step running right now is actually seeing to, if any.
    /// It is the step and not the chain: walking to the bed is not sleeping,
    /// and carrying a plate to the table is not eating.
    pub fn restoring(&self) -> Option<Need> {
        match self.step {
            Step::Doze => Some(Need::Rest),
            Step::Eat => Some(Need::Food),
            Step::UseToilet => Some(Need::Restroom),
            Step::Talk => Some(Need::Company),
            Step::Shower => Some(Need::Hygiene),
            _ => None,
        }
    }

    /// How far through the chain the Bim is, 0 to 1.
    pub fn progress(&self) -> f32 {
        let at = self.resume.as_ref().map_or(self.step, |s| s.step);
        let elapsed = self.resume.as_ref().map_or(self.elapsed, |s| s.elapsed);
        progress_of(self.kind, at, elapsed, self.rest_minutes, self.chopped)
    }

    /// Give this chain up for good, the way a blocked one is: whatever is in
    /// the Bim's hands goes back where it came from and the Bim is stood up.
    /// For a Bim leaving the room altogether.
    pub fn abandon(mut self, ch: &mut Character, room: &mut Room) {
        Task::let_go(
            self.who,
            self.kind,
            self.step,
            self.lifted.take(),
            true,
            self.picks,
            ch,
            room,
        );
        ch.set_scripted(false);
    }

    /// Put this chain down and hand back everything needed to resume it.
    /// Consumes the task: a suspended chain lives in the queue, not here.
    pub fn suspend(self, ch: &mut Character, room: &mut Room) -> Saved {
        // A chain already walking back to where it left off is saved at the
        // step it was heading for, not at the walk.
        let saved = match self.resume {
            Some(saved) => saved,
            None => Saved {
                who: self.who,
                kind: self.kind,
                step: self.step,
                elapsed: self.elapsed,
                done_count: self.done_count,
                rest_minutes: self.rest_minutes,
                chopped: self.chopped,
                target: self.target,
                swept: self.swept,
                lifted: self.lifted,
                started_inside: self.started_inside,
                main: ch.main_held(),
                tool: ch.tool_held(),
                seat: ch.seat(),
                picks: self.picks,
            },
        };
        // `None`, deliberately: a suspended chain is kept, not given up, and
        // `saved` above is still carrying the crop. Banking it here as well
        // would put one plant in the store twice — once now and once when the
        // Bim gets back to the fridge with it. `false` for the same reason:
        // a tub of stew in the hands is on `saved.main` and comes back with
        // the chain.
        Task::let_go(
            self.who, self.kind, self.step, None, false, self.picks, ch, room,
        );
        ch.set_scripted(false);
        saved
    }

    /// Leave the world in a state the Bim can walk away from. Mostly this is
    /// getting it off whatever it was sitting on, and never leaving it shut in
    /// behind a door it locked itself.
    fn let_go(
        who: usize,
        kind: Kind,
        step: Step,
        lifted: Option<Crop>,
        for_good: bool,
        picks: Picks,
        ch: &mut Character,
        room: &mut Room,
    ) {
        let bath = picks.bath.unwrap_or(0);
        use Step::*;
        // A tub of stew, the same as a harvest: a chain given up for good
        // with one in hand puts it back on the shelf rather than nowhere.
        // Only for good — a suspended chain keeps it on `Saved` and would
        // otherwise have it twice.
        if for_good && ch.main_held() == Held::Stew {
            room.stew += 1;
            ch.hold_main(Held::Nothing);
        }
        // And a plate, the same way: it is counted, and one that vanished
        // with the chain would be a drawer that quietly empties.
        if for_good && matches!(ch.main_held(), Held::Plate(..)) {
            room.return_plate();
            ch.hold_main(Held::Nothing);
        }
        // A harvest in the hands of a chain that is being given up for good.
        // It goes in the store rather than nowhere: the produce is real, the
        // Bim grew it, and a plant that evaporates because a door shut across
        // a walk is a bay whose output quietly depends on the traffic. This is
        // the one place anything aboard moves without a hand on it, and it is
        // the lesser of the two wrongs.
        if let Some(crop) = lifted {
            room.store(crop);
            ch.hold_main(Held::Nothing);
        }
        // The broom, likewise. A chain given up with it still in hand would
        // leave the Bim holding it for ever — the locker door is drawn from
        // whose hands it is in, so the cupboard would stand empty and nobody
        // could take a broom that was never put back. It goes back in the
        // cupboard from wherever the Bim was standing, which is the same
        // lesser-of-two-wrongs the harvest above takes.
        if kind == Kind::Clean && ch.main_held() == Held::Broom {
            ch.hold_main(Held::Nothing);
        }
        // A load for a site given up for good, once it is off the shelf,
        // goes back on the shelf: the room says so and the world moves the
        // count, the same way it took it off. Not a suspended one — the
        // chain is kept and walks on with it — and not one that has not
        // been taken yet, which is every step up to and including the
        // reach into the shelf, since `TakeMaterials` says so as it ends.
        if for_good
            && let Kind::Haul { site, .. } = kind
            && !matches!(step, GoToShelf | TakeMaterials)
        {
            room.returned.push(site);
            ch.hold_main(Held::Nothing);
        }
        // A walk outside given up brings the body back in through the door,
        // whatever it was doing out there: a suspended one resumes from the
        // gangway and goes out again, and an abandoned one is simply back.
        if ch.is_outside()
            && let Some(gangway) = room.gangway
        {
            ch.come_inside(gangway);
        }
        match step {
            Doze | WakeUp => {
                room.set_bed_occupied(who, false);
                ch.stand_at(room.bed_station(who));
            }
            SitOnToilet | UseToilet | RiseFromToilet => {
                ch.stand_at(room.bath_at(bath).toilet_station());
            }
            _ => {}
        }
        if kind == Kind::Heads {
            room.bath_at_mut(bath).set_locked(false);
            room.bath_at_mut(bath).set_open(true);
        }
    }

    /// Pick a chain back up. The Bim walks to the last place the chain had it
    /// standing, and only then drops back into the step it was on.
    pub fn resume(
        saved: Saved,
        ch: &mut Character,
        room: &mut Room,
        maps: &Maps,
        taken: &Taken,
    ) -> Task {
        let back_at = rewind(saved.kind, saved.step);
        ch.set_scripted(true);
        let mut task = Task {
            who: saved.who,
            kind: saved.kind,
            step: back_at,
            elapsed: 0.0,
            done_count: 0,
            rest_minutes: saved.rest_minutes,
            chopped: saved.chopped,
            // A chat is never put down, so there is never one to resume and
            // no heading to bring back with it.
            face: 0.0,
            target: saved.target,
            swept: saved.swept,
            rock: None,
            lifted: saved.lifted,
            started_inside: saved.started_inside,
            blocked: false,
            stall: 0.0,
            fumbles: 0,
            // Interrupted on the walk itself: there is nothing to drop into
            // afterwards, it simply walks it again. A walk outside put down
            // beyond the door is the same: it goes out again and picks its
            // rock afresh, rather than dropping into a step at a rock it is
            // no longer beside.
            picks: saved.picks,
            resume: if back_at == saved.step || outside_half(saved.kind, saved.step) {
                None
            } else {
                Some(saved)
            },
        };
        task.enter(ch, room, maps, taken);
        task
    }

    pub fn step(&self) -> Step {
        self.step
    }

    pub fn picks(&self) -> Picks {
        self.picks
    }

    // The fixture each step works, by index: the pick, or the first before
    // one is made.
    fn worktop(&self) -> usize {
        self.picks.worktop.unwrap_or(0)
    }

    fn hob(&self) -> usize {
        self.picks.hob.unwrap_or(0)
    }

    fn fridge(&self) -> usize {
        self.picks.fridge.unwrap_or(0)
    }

    fn dishwasher(&self) -> usize {
        self.picks.dishwasher.unwrap_or(0)
    }

    fn bath(&self) -> usize {
        self.picks.bath.unwrap_or(0)
    }

    fn shower_pick(&self) -> usize {
        self.picks.shower.unwrap_or(0)
    }

    /// Set the Bim and the room up for whichever step we just moved into.
    fn enter(&mut self, ch: &mut Character, room: &mut Room, maps: &Maps, taken: &Taken) {
        use Step::*;
        self.elapsed = 0.0;
        self.done_count = 0;

        // The fixture this step walks to, picked now if it has not been:
        // the closest one nobody else has. None free is nowhere to go, and
        // the chain gives up the way it does at a shut door. A worktop
        // picked for cooking is cleared of whatever the last meal left.
        let (had_worktop, had_hob) = (self.picks.worktop, self.picks.hob);
        if !pick_for(&mut self.picks, self.kind, self.step, room, ch.pos, taken) {
            self.blocked = true;
            return;
        }
        let cooking = matches!(self.kind, Kind::Meal(_) | Kind::Batch | Kind::Reheat);
        if cooking
            && had_worktop.is_none()
            && let Some(w) = self.picks.worktop
        {
            room.reset_worktop(w);
        }
        if cooking
            && had_hob.is_none()
            && let Some(h) = self.picks.hob
        {
            room.reset_hob(h);
        }

        // Walking steps: point the Bim at the right spot and let it march.
        // Which tile the broom is going to is chosen now, on the way into the
        // walk, so it is the worst one as of this moment rather than as of
        // whenever the errand started.
        if self.step == Step::CarryBroomTo {
            self.target = next_dirty(room, maps, ch.pos);
        }
        // Which rock, likewise: the nearest marked one it can get to now,
        // and the tile beside it to stand on.
        if self.step == Step::WalkToRock {
            let next = next_rock(room, maps, ch.pos);
            self.rock = next.map(|(rock, _)| rock);
            self.target = next.map(|(_, stand)| stand);
        }
        // The shelf and the spot beside the site, likewise: the nearest of
        // each from here, on the grid the body is on. A site the world no
        // longer wants — cancelled, built by the other one — has nowhere to
        // walk to, and the errand is given up the way a shut door gives one
        // up. The load is in hand on the walk to the site whatever the
        // hands were doing before it: a walk picked up again starts with
        // them empty.
        match self.step {
            Step::GoToShelf => {
                self.target = nearest_shelf(room, maps, ch.pos);
                if self.target.is_none() {
                    self.blocked = true;
                    return;
                }
            }
            Step::CarryToSite | Step::GoToSite => {
                if self.step == Step::CarryToSite {
                    ch.hold_main(Held::Crate);
                }
                self.target = self
                    .kind
                    .site()
                    .and_then(|site| site_stand(room, maps, site, ch.pos, ch.is_outside()));
                if self.target.is_none() {
                    self.blocked = true;
                    return;
                }
            }
            // Beside the patient, wherever it stands now. Nowhere — the
            // patient dead since the order, or outside — and the errand is
            // given up like any other walk with nowhere to go.
            Step::GoToPatient => {
                let Kind::Bandage { patient, .. } = self.kind else {
                    unreachable!("GoToPatient is a Bandage's step")
                };
                self.target = patient_stand(room, maps, self.who, patient, ch.pos);
                if self.target.is_none() {
                    self.blocked = true;
                    return;
                }
            }
            _ => {}
        }

        if let Some(to) = destination(
            self.who,
            self.kind,
            self.step,
            room,
            ch.pos,
            self.target,
            self.picks,
        ) {
            // Routed around the furniture, same as a player order. The grid is
            // chosen here rather than by the caller because the step before
            // this one may have just opened the door — or stepped out of the
            // airlock, which puts the body on the outside's grid.
            let nav = maps.for_body(ch.is_outside(), room.bath.is_open());
            let route = nav.path(ch.pos, nav.nearest_free(to));
            if route.is_empty() {
                // Nowhere to walk — a door has been shut across the way. The
                // chain gives up here rather than carrying on: an empty route
                // leaves `arrived` true straight away, and a chain that takes
                // that for arrival runs itself through every remaining step in
                // one frame and flings the Bim across the deck the moment one
                // of them sets a position.
                self.blocked = true;
                return;
            }
            ch.follow_path(route);
            ch.set_action(Action::None);
            return;
        }

        // Standing steps: face the work and start the right animation.
        match self.step {
            OpenFridge | CloseFridge | TakeVegetable | PutVegetableDown | TakeKnife
            | PutKnifeDown | GatherSlices | TipIntoPot | TurnStoveOn | TurnStoveOff
            | TakePlateAndSpoon | SetPlateDown | PickUpPlate | OpenDishwasher | StackDishes
            | ShutDishwasher | StartDishwasher | TakeBowl | FillBowl | StowCrop | PackStew
            | StowStew | OpenStoreForStew | ShutStoreOnStew | TakeStew | TipStewIntoPot => {
                ch.face(FACE_WALL);
                ch.set_action(Action::Reach);
            }
            // A switch can be on any wall, so the Bim turns to whichever one
            // it walked up to rather than to a fixed heading.
            FlipSwitch => {
                if let Kind::Switch(which) = self.kind {
                    ch.face(room.switch_facing(which, ch.pos));
                }
                ch.set_action(Action::Reach);
            }
            // Turned to the trays, which are against the bottom wall.
            WorkTray => {
                ch.face(FACE_TRAY);
                ch.set_action(Action::Reach);
            }
            // At the broom locker, facing the port bulkhead.
            TakeBroom => {
                ch.face(FACE_LOCKER);
                ch.set_action(Action::Reach);
            }
            PutBroomBack => {
                ch.face(FACE_LOCKER);
                ch.set_action(Action::Reach);
            }
            // Working the broom across the deck. No facing is forced: it
            // sweeps whichever way it arrived, which is the way the dirt is.
            Sweep => ch.set_action(Action::Sweep),
            // Turned to the other one. The heading came in with the errand,
            // because where the other one is standing is not something a
            // chain can look up.
            Talk => {
                ch.face(self.face);
                ch.set_action(Action::Talk);
            }
            Shower => {
                ch.face(room.shower_facing(self.shower_pick()));
                ch.set_action(Action::Wash);
            }
            // At the locker, facing it; at the port, facing out; outside,
            // hands busy; and back in, facing the deck.
            TakeSuit | PutSuitBack => {
                ch.face(room.suit_locker_facing());
                ch.set_action(Action::Reach);
            }
            StepOut => {
                ch.face(room.port_facing());
                ch.set_action(Action::Reach);
            }
            // Between rocks: standing there a moment while the rocks are
            // counted again. At a rock: turned to it, the pick going.
            PickRock => ch.set_action(Action::None),
            Mine => {
                if let Some(rock) = self.rock {
                    let d = rock - ch.pos;
                    ch.face(d.y.atan2(d.x));
                }
                ch.set_action(Action::Chop);
            }
            StepIn => {
                if let Some(gangway) = room.gangway {
                    ch.come_inside(gangway);
                }
                ch.face(room.port_facing() + PI);
                ch.set_action(Action::Reach);
            }
            // Hands on the bench, turned into it.
            Work => {
                if let Kind::Craft { bench, .. } = self.kind
                    && let Some(bench) = room.benches.get(bench)
                {
                    ch.face(bench.facing());
                }
                ch.set_action(Action::Reach);
            }
            // At the shelf, reaching in; at the site, turned to it — the
            // load set down, or the arms going at it for the build.
            TakeMaterials => {
                if let Some((frame, _)) = self
                    .target
                    .and_then(|at| room.shelves.iter().find(|(_, spot)| *spot == at))
                {
                    let d = frame.center() - ch.pos;
                    ch.face(d.y.atan2(d.x));
                }
                ch.set_action(Action::Reach);
            }
            DropMaterials | Construct => {
                if let Some(build) = self.kind.site().and_then(|site| site_of(room, site)) {
                    let d = site_middle(build) - ch.pos;
                    ch.face(d.y.atan2(d.x));
                }
                ch.set_action(if self.step == Construct {
                    Action::Chop
                } else {
                    Action::Reach
                });
            }
            // Turned to the patient, hands on it. Its own wounds it dresses
            // facing whichever way it arrived.
            Dress => {
                if let Kind::Bandage { patient, .. } = self.kind
                    && patient != self.who
                    && let Some(Some(at)) = room.crew.get(patient)
                {
                    let d = *at - ch.pos;
                    if d.len() > 1e-3 {
                        ch.face(d.y.atan2(d.x));
                    }
                }
                ch.set_action(Action::Reach);
            }
            // Working the door panel. From the deck the Bim faces the
            // bulkhead; from inside it turns round and faces it the other way.
            OpenDoor | ShutDoorBehind => {
                ch.face(FACE_DOOR);
                ch.set_action(Action::Reach);
            }
            ShutDoor | UnlockDoor => {
                ch.face(FACE_INWARD);
                ch.set_action(Action::Reach);
            }
            SitOnToilet => {
                ch.sit(room.bath_at(self.bath()).toilet_seat(), FACE_OFF_PAN);
                ch.set_action(Action::Reach);
            }
            // Sitting there. No animation is the point of it.
            UseToilet => ch.set_action(Action::None),
            RiseFromToilet => {
                ch.stand_at(room.bath_at(self.bath()).toilet_station());
                ch.set_action(Action::None);
            }
            FlushToilet => {
                ch.face(FACE_OFF_PAN + PI);
                ch.set_action(Action::Reach);
            }
            WashHands => {
                ch.face(FACE_DOOR);
                ch.set_action(Action::Wash);
            }
            // Up and down the ladder, facing the bed from the room side.
            ClimbIntoBed | ClimbOutOfBed => {
                ch.face(room.bed_facing(self.who));
                ch.set_action(Action::Reach);
            }
            Doze => {
                ch.lie(room.bed_lie_pos(self.who), room.bed_lie_facing(self.who));
                ch.set_action(Action::Sleep);
            }
            // A stretch, still lying down, before getting up.
            WakeUp => ch.set_action(Action::Reach),
            Chop => {
                ch.face(FACE_WALL);
                ch.set_action(Action::Chop);
            }
            BackToBoardWithBowl => {}
            Serve => {
                ch.face(FACE_WALL);
                ch.set_action(Action::Serve);
            }
            SitDown => {
                ch.sit(room.chair_at(self.who), room.chair_facing(self.who));
                ch.set_action(Action::Reach);
            }
            Eat => ch.set_action(Action::Eat),
            // Gathering up after the meal, still at the table.
            ClearTable => {
                ch.face(room.chair_facing(self.who));
                ch.set_action(Action::Reach);
            }
            Rest => ch.set_action(Action::None),
            StandUp => {
                ch.stand();
                ch.set_action(Action::None);
            }
            _ => ch.set_action(Action::None),
        }

        // Things that happen the moment a step begins.
        match self.step {
            OpenFridge | OpenStoreForStew => room.set_fridge_open(self.fridge(), true),
            CloseFridge | ShutStoreOnStew => room.set_fridge_open(self.fridge(), false),
            GoToDrawerForKnife | GoToDrawerForPlate => {}
            TakeKnife | TakePlateAndSpoon | TakeBowl => room.set_drawer_open(self.worktop(), true),
            Doze => room.set_bed_occupied(self.who, true),
            OpenDishwasher => room.dishwashers[self.dishwasher()].set_open(true),
            ShutDishwasher => room.dishwashers[self.dishwasher()].set_open(false),
            // The door slides as the Bim touches the panel, so the walk that
            // follows already has somewhere to go.
            OpenDoor | UnlockDoor => {
                room.bath_at_mut(self.bath()).set_locked(false);
                room.bath_at_mut(self.bath()).set_open(true);
            }
            // Shut behind itself, and locked: the point of a door.
            ShutDoor => {
                room.bath_at_mut(self.bath()).set_open(false);
                room.bath_at_mut(self.bath()).set_locked(true);
            }
            ShutDoorBehind => room.bath_at_mut(self.bath()).set_open(false),
            WashHands => {
                room.bath_at_mut(self.bath()).run_tap(WASH_TIME);
                // A basin is a basin: it gets the worst of a mess off a Bim.
                ch.wash(WASH_TAKES_OFF);
            }
            // A shower takes the lot off, whatever it was.
            Shower => ch.wash(1.0),
            // Back through the door: a walk to mine finished, and the room
            // counts it the moment the body is in, which is the same step
            // the world reads the count. A trip out to a site is not a walk
            // and brings nothing back.
            StepIn if self.kind == Kind::Eva => room.walks_done += 1,
            SitDown => {
                // The plate goes on the table as the Bim sits down to it.
                if let Held::Plate(fill, _) = ch.main_held() {
                    room.set_plate_at(self.who, Some(fill));
                }
                ch.hold_main(Held::Nothing);
                ch.hold_tool(Held::Fork);
            }
            _ => {}
        }
    }

    /// Everything that changes as a step finishes.
    fn leave(&mut self, ch: &mut Character, room: &mut Room) {
        use Step::*;
        match self.step {
            // Out through the door: held beyond the hull, in the suit, until
            // `StepIn` brings the body back. With the pick for a rock; with
            // nothing, or the load, for a site.
            StepOut => {
                if let Some(outside) = room.outside {
                    ch.go_outside(outside, room.port_facing(), self.kind == Kind::Eva);
                }
            }
            // A load off the shelf: the room says so, and the world decides
            // how much that is and takes it off the count. What the hands
            // hold is a crate, whatever is in it.
            TakeMaterials => {
                if let Kind::Haul { site, .. } = self.kind
                    && let Some(build) = site_of(room, site)
                    && let Some((resource, units)) = build.haul
                {
                    room.picked.push((site, resource, units));
                }
                ch.hold_main(Held::Crate);
            }
            // Put down at the site: the world moves the load onto it. The
            // room's own copy of the order is a step behind the world, so
            // the site is marked wanting nothing here as well, or the same
            // load is offered again before the world has spoken.
            DropMaterials => {
                ch.hold_main(Held::Nothing);
                if let Some(site) = self.kind.site() {
                    room.dropped.push(site);
                    if let Some(build) = room.builds.iter_mut().find(|b| b.site == site) {
                        build.haul = None;
                    }
                }
            }
            // Built: the world puts the part down. Off the room's list at
            // once, for the same reason.
            Construct => {
                if let Some(site) = self.kind.site() {
                    room.built.push(site);
                    room.builds.retain(|b| b.site != site);
                }
            }
            // A rock gone: the room writes down which and the world moves
            // what it yields; the room never touches a resource itself. It
            // comes off the room's own copy of the rocks and the marks too,
            // so the next rock chosen is not this one again.
            Mine => {
                if let Some(rock) = self.rock.take() {
                    room.mined.push(rock);
                    room.rock_targets.retain(|&r| r != rock);
                    room.rocks.retain(|r| !r.contains(rock));
                }
            }
            // Finished: the room writes it down and the world moves the
            // cargo. The room never touches a resource itself.
            Work => {
                if let Kind::Craft { recipe, .. } = self.kind {
                    room.crafted.push(recipe);
                }
            }
            // Hands off the patient: the room says which part of whom was
            // dressed, and the game — which has the body and the count of
            // bandages — does the dressing, if the two are still together.
            Dress => {
                if let Kind::Bandage { patient, part } = self.kind {
                    room.dressed.push((patient, part));
                }
            }
            // A shelf stew takes its two things one trip each — the
            // vegetable first, the block of tofu on the way round again —
            // where a meal takes what its recipe says.
            TakeVegetable if self.kind == Kind::Batch => {
                let crop = if self.chopped == 0 {
                    Crop::Veg
                } else {
                    Crop::Soy
                };
                ch.hold_main(match crop {
                    Crop::Soy => Held::Tofu,
                    _ => Held::Vegetable,
                });
                room.take(crop);
            }
            TakeVegetable => {
                let dish = match self.kind {
                    Kind::Meal(dish) => dish,
                    _ => Dish::Stew,
                };
                ch.hold_main(if dish == Dish::Bowl {
                    Held::Tofu
                } else {
                    Held::Vegetable
                });
                room.take_from_fridge(dish);
            }
            PutVegetableDown => {
                // The board draws whatever was put on it, on its own side: a
                // block chops into cubes and a vegetable into rounds.
                room.put_on_board(self.worktop(), ch.main_held() == Held::Tofu);
                ch.hold_main(Held::Nothing);
            }
            // Out of the store and into the hands; the tub is the Bim's now
            // and the count says so, the same way a crop is off the tray the
            // moment it is lifted.
            TakeStew => {
                ch.hold_main(Held::Stew);
                room.stew = room.stew.saturating_sub(1);
            }
            // Into the pot to warm through. It was cooked once already, so
            // it starts most of the way to done rather than raw.
            TipStewIntoPot => {
                ch.hold_main(Held::Nothing);
                room.hobs[self.hob()].pot_contents = 1.0;
                room.hobs[self.hob()].pot_cooked = 0.6;
                room.hobs[self.hob()].food_bad = false;
                room.hobs[self.hob()].pot_servings = SERVINGS_PER_POT;
            }
            // The whole pot into a tub. Nothing is left on the hob to come
            // back to: the pot is empty and the tub is what holds it now.
            PackStew => {
                ch.hold_main(Held::Stew);
                room.hobs[self.hob()].pot_contents = 0.0;
                room.hobs[self.hob()].pot_cooked = 0.0;
                room.hobs[self.hob()].pot_servings = 0;
            }
            StowStew => {
                ch.hold_main(Held::Nothing);
                room.stew += 1;
            }
            TakeKnife => {
                ch.hold_tool(Held::Knife);
                room.set_drawer_open(self.worktop(), false);
            }
            PutKnifeDown => {
                ch.hold_tool(Held::Nothing);
                room.worktops[self.worktop()].knife_on_board = true;
            }
            GatherSlices => {
                // Everything on the board, both sides of it, in two hands.
                let (rounds, cubes) = room.board_pieces(self.worktop());
                ch.hold_main(Held::Chopped { rounds, cubes });
                room.clear_board(self.worktop());
            }
            TipIntoPot => {
                ch.hold_main(Held::Nothing);
                room.hobs[self.hob()].pot_contents = 1.0;
                room.hobs[self.hob()].pot_cooked = 0.0;
                room.hobs[self.hob()].food_bad = false;
                room.hobs[self.hob()].pot_servings = SERVINGS_PER_POT;
            }
            TurnStoveOn => room.set_stove(self.hob(), true),
            TurnStoveOff => room.set_stove(self.hob(), false),
            // The player asked for a toggle, so read the state at the moment
            // the Bim's hand actually reaches it.
            FlipSwitch => {
                if let Kind::Switch(which) = self.kind {
                    room.work_switch(which);
                }
            }
            // Same again for the bay: what the tray wants is asked now, with
            // the Bim's hands in it, rather than when it set off. A tray that
            // has been seen to in the meantime simply leaves nothing to do.
            // The tray gives up its plant into the Bim's hands, and no
            // further: the store is the other end of the room and nothing
            // aboard travels by itself. What is lifted here is carried,
            // and only `StowCrop` puts it away.
            WorkTray => {
                let Kind::Tend { bay, .. } = self.kind else {
                    unreachable!("WorkTray is a Tend's step")
                };
                let (veg, tofu, fibre) = (room.veg, room.tofu, room.fibre);
                if let Some(bay) = room.bays.get_mut(bay)
                    && let Some(job) = bay.wants_work(veg, tofu, fibre)
                {
                    if let Some(crop) = bay.work(job) {
                        self.lifted = Some(crop);
                        // A sheaf of fibre in the hands is drawn as greens:
                        // the hands know two shapes, and `lifted` is what
                        // the store goes by.
                        ch.hold_main(match crop {
                            Crop::Veg | Crop::Fibre => Held::Vegetable,
                            Crop::Soy => Held::Tofu,
                        });
                    }
                }
            }
            TakeBroom => ch.hold_main(Held::Broom),
            PutBroomBack => ch.hold_main(Held::Nothing),
            // The tile the Bim set out for, not the one under its boots. The
            // two differ whenever the dirt is somewhere a body cannot quite
            // stand — under the lip of the counter, in the corner by the
            // heads — and a broom has the reach for that. Sweeping underfoot
            // instead would leave those tiles dirty for ever *and* send the
            // Bim back to the same unreachable one every time, because it
            // would still be the worst on the deck.
            Sweep => {
                if let Some(tile) = self.target.take() {
                    room.filth.sweep(tile);
                    self.swept += 1;
                }
            }
            // Into the cold store, at last. `take` rather than a read: the
            // crop is off the Bim now, and a chain that somehow came back
            // through here must not bank it twice.
            StowCrop => {
                ch.hold_main(Held::Nothing);
                if let Some(crop) = self.lifted.take() {
                    room.store(crop);
                }
            }
            // A plate out of the drawer, and one fewer in it.
            TakePlateAndSpoon => {
                room.take_plate();
                ch.hold_main(Held::Plate(0.0, room.dish));
                ch.hold_tool(Held::Spoon);
                room.set_drawer_open(self.worktop(), false);
            }
            TakeBowl => {
                room.take_plate();
                ch.hold_main(Held::Plate(0.0, Dish::Bowl));
                room.set_drawer_open(self.worktop(), false);
            }
            // Chopped tofu and the salad go in together, and that is the meal.
            FillBowl => {
                ch.hold_main(Held::Plate(1.0, Dish::Bowl));
                room.clear_board(self.worktop());
                // Nothing cooked, but made in the same galley: judged the
                // same way as a pot.
                room.made_a_bowl(self.hob());
            }
            Chop => self.chopped += 1,
            SetPlateDown => {
                ch.hold_main(Held::Nothing);
                room.hobs[self.hob()].plate_on_counter = Some(0.0);
            }
            // The helping comes off the pot here rather than spoonful by
            // spoonful, so an interrupted serve costs the pot nothing: what
            // the scoops move is the picture, and this is the count.
            Serve => {
                room.hobs[self.hob()].pot_servings =
                    room.hobs[self.hob()].pot_servings.saturating_sub(1);
                room.hobs[self.hob()].pot_contents =
                    room.hobs[self.hob()].pot_servings as f32 / SERVINGS_PER_POT as f32;
            }
            PickUpPlate => {
                let fill = room.hobs[self.hob()].plate_on_counter.take().unwrap_or(0.0);
                ch.hold_main(Held::Plate(fill, room.dish));
                ch.hold_tool(Held::Spoon);
            }
            // The plate and the cutlery come up off the table together; the
            // fork is already in hand from eating with it.
            ClearTable => {
                room.set_plate_at(self.who, None);
                ch.hold_main(Held::Plate(0.0, room.dish));
            }
            StackDishes => {
                ch.hold_main(Held::Nothing);
                ch.hold_tool(Held::Nothing);
                room.dishwashers[self.dishwasher()].stack();
            }
            StartDishwasher => room.dishwashers[self.dishwasher()].start(),
            FlushToilet => room.bath_at_mut(self.bath()).flush(),
            // Back down the ladder: the Bim was lying in the middle of the
            // bed, and the floor beside it is the only place it can stand.
            WakeUp => {
                ch.stand_at(room.bed_station(self.who));
                room.set_bed_occupied(self.who, false);
            }
            _ => {}
        }
    }

    /// A dirty job leaves something on the deck around it.
    ///
    /// Called with the step that has just *finished*, so the roll happens
    /// once per knife-load or per pair of hands in a tray rather than once a
    /// frame. The rest of the time this costs nothing and — as importantly —
    /// draws nothing from `rng`: every roll aboard comes off one stream, so a
    /// die thrown on a frame that used to throw none reshuffles every later
    /// outcome in the run.
    ///
    /// Where a stain is allowed to land is asked the same way the sweeping
    /// chain asks which tile to go to next, through the nav grid. The two
    /// have to agree: a mess made somewhere `next_dirty` will not send the
    /// Bim is a mess the deck keeps for ever.
    fn dirty_work(&self, at: Vec2, room: &mut Room, maps: &Maps, rng: &mut Rng) {
        if !self.step.is_dirty_work() || !rng.chance(filth::JOB_MESSES) {
            return;
        }
        let nav = maps.pick(room.bath.is_open());
        room.filth.spatter(at, rng, |tile| nav.can_reach(at, tile));
    }

    /// Repeating steps do their work in discrete beats, so that each knife
    /// stroke removes a slice's worth and each mouthful clears some plate.
    fn tick_repeating(&mut self, room: &mut Room) {
        use Step::*;
        let (period, total) = match self.step {
            Chop => (CHOP_PERIOD, CHOPS),
            Serve => (SCOOP_PERIOD, SCOOPS),
            Eat => (BITE_PERIOD, BITES),
            _ => return,
        };

        let want = ((self.elapsed / period) as u32).min(total);
        while self.done_count < want {
            self.done_count += 1;
            match self.step {
                Chop => room.chop(self.worktop(), self.done_count, CHOPS),
                Serve => {
                    room.hobs[self.hob()].plate_on_counter =
                        Some(self.done_count as f32 / SCOOPS as f32);
                    // One helping out of however many are left, spread over the
                    // spoonfuls. Worked out from the count rather than taken
                    // off what is there, so a serve that was interrupted and
                    // started again does not drain the pot twice.
                    let left = room.hobs[self.hob()].pot_servings as f32
                        - self.done_count as f32 / SCOOPS as f32;
                    room.hobs[self.hob()].pot_contents = (left / SERVINGS_PER_POT as f32).max(0.0);
                }
                Eat => {
                    let left = 1.0 - self.done_count as f32 / BITES as f32;
                    room.set_plate_at(self.who, Some(left.max(0.0)));
                }
                _ => {}
            }
        }
    }

    /// `fumble` is the chance a finished step has to be done over again —
    /// zero for a Bim that has slept, rising as it goes without.
    ///
    /// `effort` is how fast it is getting on with things, 1 for a Bim in good
    /// order and less for one that is not. It slows the **work** and nothing
    /// else: a step that is putting a need right — a doze, a meal, a sit on
    /// the pan — runs at its own length whatever state the Bim is in, because
    /// those are measured against the need they fill and stretching them would
    /// quietly change how much a night's sleep is worth.
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        dt: f32,
        ch: &mut Character,
        room: &mut Room,
        maps: &Maps,
        rng: &mut Rng,
        fumble: f32,
        effort: f32,
        taken: &Taken,
    ) {
        if self.step == Step::Done {
            return;
        }
        let dt = if self.restoring().is_some() {
            dt
        } else {
            dt * effort
        };

        // Nowhere to walk: give the errand up, and put the world back in a
        // state the Bim can be left in.
        if self.blocked {
            // This chain is over for good, so anything in the Bim's hands is
            // handed over with it — `take`, so nothing can bank it twice.
            Task::let_go(
                self.who,
                self.kind,
                self.step,
                self.lifted.take(),
                true,
                self.picks,
                ch,
                room,
            );
            ch.set_scripted(false);
            self.step = Step::Done;
            return;
        }

        // Lost the thread. Nothing moves on, and `elapsed` is left where it
        // was, so when it comes back to itself the step finishes again — and
        // can be fumbled again, which is what makes the cost compound the way
        // the arithmetic in `Drowsiness::fumble` expects.
        if self.stall > 0.0 {
            self.stall -= dt;
            return;
        }

        self.elapsed += dt;

        let finished = if self.step.is_walk() {
            ch.arrived()
        } else {
            self.tick_repeating(room);
            // Standing steps also wait for the turn-on-the-spot to settle, so
            // the Bim is never seen reaching into a fridge sideways.
            self.elapsed >= self.duration() && ch.facing_settled()
        };

        if finished {
            // Too far gone to hold on to what it was doing: stand there for as
            // long as that step took, then take it from the top.
            if fumble > 0.0 && rng.chance(fumble) {
                // Doing the step again costs what the step costs. For a
                // standing step that is its own length — not the time just
                // elapsed, which also holds however long the Bim spent turning
                // to face the job, and which it will not spend a second time.
                let again = if self.step.is_walk() {
                    self.elapsed
                } else {
                    self.duration()
                };
                self.stall = again.max(MIN_STALL);
                self.fumbles += 1;
                ch.set_action(Action::None);
                return;
            }

            // Back in position after picking a chain up again: carry on from
            // the step it was put down on, with the progress it had.
            if let Some(saved) = self.resume.take() {
                saved.restore(ch);
                self.step = saved.step;
                self.enter(ch, room, maps, taken);
                self.elapsed = saved.elapsed;
                self.done_count = saved.done_count;
                return;
            }
            self.leave(ch, room);
            // Before `step` moves on, because what was just finished is what
            // made the mess.
            self.dirty_work(ch.pos, room, maps, rng);
            self.step = self.next_step(room, maps, ch.pos);
            if self.step == Step::Done {
                ch.set_scripted(false);
                return;
            }
            self.enter(ch, room, maps, taken);
        }
    }
}
