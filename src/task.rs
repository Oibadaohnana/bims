//! Scripted jobs the Bim carries out, one step at a time.
//!
//! A task is a straight line of [`Step`]s. Each step either sends the Bim
//! somewhere and waits for it to arrive, or holds it in place for a while
//! playing an animation. Anything the step changes about the world — a door
//! opening, a vegetable moving from hand to board — happens on the way in or
//! on the way out, so the state and what you can see never disagree.

use crate::character::{Action, BITE_PERIOD, CHOP_PERIOD, Character, Held, SCOOP_PERIOD};
use crate::clock::{self, HOUR, MINUTES_PER_SECOND};
use crate::math::{PI, Vec2};
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

/// Facing for the fixtures along the top wall, and for the table.
const FACE_WALL: f32 = -PI * 0.5;
const FACE_TABLE: f32 = PI * 0.5;
/// The bed is against the left wall, so the Bim faces the ladder from the
/// room side, and lies with its head towards the pillow at the top.
const FACE_BED: f32 = PI;
const FACE_PILLOW: f32 = -PI * 0.5;
/// In the heads: the door is in the north bulkhead and the pan is against the
/// hull on the east side, so the Bim turns its back on each in turn.
const FACE_DOOR: f32 = -PI * 0.5;
const FACE_INWARD: f32 = PI * 0.5;
const FACE_OFF_PAN: f32 = PI;
/// The bay is against the bottom wall, so the Bim turns its back on the room.
const FACE_TRAY: f32 = PI * 0.5;

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
            StartDishwasher | FlipSwitch | WorkTray | ClimbOutOfBed | ShutDoorBehind | Done => Done,
        }
    }

    /// How long a stationary step lasts. Walking steps run until the Bim
    /// arrives, so their length here is ignored, and a doze runs for as long
    /// as the task was told to sleep — see `Task::duration`.
    fn duration(self) -> f32 {
        use Step::*;
        match self {
            OpenFridge | CloseFridge => 0.7,
            TakeVegetable | TakeKnife | TakePlateAndSpoon => 0.8,
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
                | GoToSwitch
                | GoToTray
                | GoToBed
                | GoToDoor
                | StepInside
                | GoToToilet
                | GoToSink
                | BackToDoor
                | StepOutside
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
    /// Walking over to something and working it by hand. Every switch aboard
    /// goes through this: none of them answer from across the room.
    Switch(Switch),
    Rest,
    Heads,
    /// Helping itself to what is left in the pot: a plate, the rest of the
    /// stew, and the same sit-down and clearing-up as a fresh meal. No
    /// fridge, no knife, no hob.
    Leftovers,
    /// One tray of the hydroponic bay: whatever that tray wants when the Bim
    /// gets to it. One errand per tray, so an interruption costs a tray and
    /// not the whole bay.
    Tend(usize),
}

impl Kind {
    /// Where this chain begins. Needed to walk a chain from the top, which is
    /// how a half-finished one works out where to pick itself up.
    fn first_step(self) -> Step {
        match self {
            Kind::Meal(_) => Step::GoToFridge,
            Kind::Switch(_) => Step::GoToSwitch,
            Kind::Rest => Step::GoToBed,
            Kind::Heads => Step::GoToDoor,
            Kind::Leftovers => Step::GoToDrawerForPlate,
            Kind::Tend(_) => Step::GoToTray,
        }
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
                (Kind::Meal(Dish::Bowl), Step::PutKnifeDown) => Step::GoToDrawerForBowl,
                // Nothing was lit, so there is no hob to turn off.
                (Kind::Leftovers, Step::Serve) => Step::PickUpPlate,
                _ => here.next(),
            };
            at = if next == Step::Done { None } else { Some(next) };
            Some(here)
        })
    }
}

/// A chain put down part-way through, and everything needed to pick it up.
///
/// The room remembers most of it by itself — a chopped vegetable stays
/// chopped, a pot stays full — so what has to be carried here is the handful
/// of things that live on the Bim and are thrown away when a task lets go of
/// it: which step it had reached and how far into it, and what it was holding
/// or sitting on.
pub struct Saved {
    kind: Kind,
    step: Step,
    elapsed: f32,
    done_count: u32,
    rest_minutes: f32,
    chopped: u32,
    started_inside: bool,
    main: Held,
    tool: Held,
    /// Where it was sitting or lying, and which way it faced, if it was.
    seat: Option<(Vec2, f32)>,
}

impl Saved {
    /// An errand that has not started yet, dressed as one that was put down at
    /// its first step. It is the same thing to everything downstream — the
    /// queue, the agenda, the resume — which is what lets work be *added* to
    /// the queue rather than only ever put back into it.
    pub fn fresh(kind: Kind, rest_minutes: f32) -> Saved {
        Saved {
            kind,
            step: kind.first_step(),
            elapsed: 0.0,
            done_count: 0,
            rest_minutes,
            chopped: 0,
            started_inside: false,
            main: Held::Nothing,
            tool: Held::Nothing,
            seat: None,
        }
    }

    pub fn kind(&self) -> Kind {
        self.kind
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
        destination(self.kind, rewind(self.kind, self.step), room, from)
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
fn destination(kind: Kind, step: Step, room: &Room, from: Vec2) -> Option<Vec2> {
    use Step::*;
    match step {
        GoToFridge => Some(room.fridge_station()),
        CarryToBoard | BackToBoard | BackToBoardWithBowl => Some(room.board_station()),
        GoToDrawerForKnife | GoToDrawerForPlate | GoToDrawerForBowl => Some(room.drawer_station()),
        CarryToPot => Some(room.stove_station()),
        // Serving needs pot and plate either side, so it has its own spot.
        BackToStove => Some(room.serve_station()),
        // The chair sits clear of the table footprint, so the Bim can actually
        // stand on it before sitting down.
        CarryToTable => Some(room.chair),
        GoToSwitch => match kind {
            Kind::Switch(which) => Some(room.switch_station(which, from)),
            _ => None,
        },
        GoToTray => match kind {
            Kind::Tend(spot) => Some(room.bay.station(spot)),
            _ => None,
        },
        CarryToDishwasher => Some(room.dishwasher_station()),
        GoToBed => Some(room.bed_station()),
        GoToDoor | StepOutside => Some(room.bath.outside_station()),
        StepInside | BackToDoor => Some(room.bath.inside_station()),
        GoToToilet => Some(room.bath.toilet_station()),
        GoToSink => Some(room.bath.sink_station()),
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
pub fn first_station(kind: Kind, room: &Room, from: Vec2) -> Option<Vec2> {
    destination(kind, opening_step(kind, room, from), room, from)
}

/// The step to start at when picking `target` up again.
///
/// Standing steps assume the Bim is already in the right place — `Chop` chops
/// whatever is in front of it — so resuming rewinds to the most recent walking
/// step and lets the Bim walk back to the bench first. The steps skipped on
/// the way are the ones whose work the room is already holding.
fn rewind(kind: Kind, target: Step) -> Step {
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
    } else if step == Step::Doze {
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
    let laps = match kind {
        Kind::Meal(dish) => portions(dish),
        _ => 1,
    };

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
}

impl Task {
    /// `rest_minutes` only means anything to a rest; every other errand
    /// passes zero.
    fn starting_at(
        kind: Kind,
        step: Step,
        rest_minutes: f32,
        ch: &mut Character,
        room: &mut Room,
        maps: &Maps,
    ) -> Task {
        ch.set_scripted(true);
        let mut task = Task {
            kind,
            step,
            elapsed: 0.0,
            done_count: 0,
            rest_minutes,
            chopped: 0,
            started_inside: false,
            blocked: false,
            stall: 0.0,
            fumbles: 0,
            resume: None,
        };
        task.enter(ch, room, maps);
        task
    }

    /// Start the make-a-meal chain.
    pub fn make_food(dish: Dish, ch: &mut Character, room: &mut Room, maps: &Maps) -> Task {
        room.reset_for_cooking(dish);
        Task::starting_at(Kind::Meal(dish), Step::GoToFridge, 0.0, ch, room, maps)
    }

    /// Walk over to a switch and work it. Everything the Bim can operate goes
    /// through here, so nothing in the room can be changed without the Bim
    /// being there to change it.
    pub fn work_switch(which: Switch, ch: &mut Character, room: &mut Room, maps: &Maps) -> Task {
        Task::starting_at(Kind::Switch(which), Step::GoToSwitch, 0.0, ch, room, maps)
    }

    /// Send the Bim to the heads. Refused from the outside if the door is
    /// locked — the host greys the menu item out for the same reason.
    /// Help itself to what is left in the pot: a plate, the rest of the stew,
    /// and the same sit-down and clearing-up as a meal it cooked.
    pub fn leftovers(ch: &mut Character, room: &mut Room, maps: &Maps) -> Task {
        Task::starting_at(
            Kind::Leftovers,
            Step::GoToDrawerForPlate,
            0.0,
            ch,
            room,
            maps,
        )
    }

    /// Walk to one tray of the bay and do whatever it wants doing.
    pub fn tend(spot: usize, ch: &mut Character, room: &mut Room, maps: &Maps) -> Task {
        Task::starting_at(Kind::Tend(spot), Step::GoToTray, 0.0, ch, room, maps)
    }

    pub fn use_toilet(ch: &mut Character, room: &mut Room, maps: &Maps) -> Task {
        let from = opening_step(Kind::Heads, room, ch.pos);
        let inside = from != Kind::Heads.first_step();
        let mut task = Task::starting_at(Kind::Heads, from, 0.0, ch, room, maps);
        task.started_inside = inside;
        task
    }

    /// Send the Bim to bed for `minutes` of game time: up the ladder, under
    /// the covers, and back out again when the clock says so.
    pub fn rest(minutes: f32, ch: &mut Character, room: &mut Room, maps: &Maps) -> Task {
        Task::starting_at(Kind::Rest, Step::GoToBed, minutes, ch, room, maps)
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
    fn next_step(&self, room: &Room) -> Step {
        use Step::*;
        if self.step == ShutDishwasher && !room.dishwasher.is_full() {
            return Done;
        }
        if let Kind::Meal(dish) = self.kind {
            match self.step {
                // Round again for the second vegetable of a stew.
                Chop if self.chopped < portions(dish) => return GoToFridge,
                // On that second trip the knife is already in hand, so the
                // Bim goes straight back to chopping.
                PutVegetableDown if self.chopped >= 1 => return Chop,
                // A bowl needs no pot and no heat: fetch it and fill it.
                PutKnifeDown if dish == Dish::Bowl => return GoToDrawerForBowl,
                _ => {}
            }
        }
        // A trip that began inside ends at the basin: the steps that let the
        // Bim out are the undoing of the ones that let it in, and it did not
        // use those either.
        if self.kind == Kind::Heads && self.started_inside && self.step == WashHands {
            return Done;
        }
        // Helping itself to the pot lit nothing, so it turns nothing off.
        if self.kind == Kind::Leftovers && self.step == Serve {
            return PickUpPlate;
        }
        self.step.next()
    }

    /// How long the step running now lasts. Everything but a doze is a fixed
    /// length; a doze runs for as long as the Bim was told to sleep.
    fn duration(&self) -> f32 {
        match self.step {
            Step::Doze => clock::seconds(self.rest_minutes),
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
            _ => None,
        }
    }

    /// How far through the chain the Bim is, 0 to 1.
    pub fn progress(&self) -> f32 {
        let at = self.resume.as_ref().map_or(self.step, |s| s.step);
        let elapsed = self.resume.as_ref().map_or(self.elapsed, |s| s.elapsed);
        progress_of(self.kind, at, elapsed, self.rest_minutes, self.chopped)
    }

    /// Put this chain down and hand back everything needed to resume it.
    /// Consumes the task: a suspended chain lives in the queue, not here.
    pub fn suspend(self, ch: &mut Character, room: &mut Room) -> Saved {
        // A chain already walking back to where it left off is saved at the
        // step it was heading for, not at the walk.
        let saved = match self.resume {
            Some(saved) => saved,
            None => Saved {
                kind: self.kind,
                step: self.step,
                elapsed: self.elapsed,
                done_count: self.done_count,
                rest_minutes: self.rest_minutes,
                chopped: self.chopped,
                started_inside: self.started_inside,
                main: ch.main_held(),
                tool: ch.tool_held(),
                seat: ch.seat(),
            },
        };
        Task::let_go(self.kind, self.step, ch, room);
        ch.set_scripted(false);
        saved
    }

    /// Leave the world in a state the Bim can walk away from. Mostly this is
    /// getting it off whatever it was sitting on, and never leaving it shut in
    /// behind a door it locked itself.
    fn let_go(kind: Kind, step: Step, ch: &mut Character, room: &mut Room) {
        use Step::*;
        match step {
            Doze | WakeUp => {
                room.set_bed_occupied(false);
                ch.stand_at(room.bed_station());
            }
            SitOnToilet | UseToilet | RiseFromToilet => {
                ch.stand_at(room.bath.toilet_station());
            }
            _ => {}
        }
        if kind == Kind::Heads {
            room.bath.set_locked(false);
            room.bath.set_open(true);
        }
    }

    /// Pick a chain back up. The Bim walks to the last place the chain had it
    /// standing, and only then drops back into the step it was on.
    pub fn resume(saved: Saved, ch: &mut Character, room: &mut Room, maps: &Maps) -> Task {
        let back_at = rewind(saved.kind, saved.step);
        ch.set_scripted(true);
        let mut task = Task {
            kind: saved.kind,
            step: back_at,
            elapsed: 0.0,
            done_count: 0,
            rest_minutes: saved.rest_minutes,
            chopped: saved.chopped,
            started_inside: saved.started_inside,
            blocked: false,
            stall: 0.0,
            fumbles: 0,
            // Interrupted on the walk itself: there is nothing to drop into
            // afterwards, it simply walks it again.
            resume: if back_at == saved.step {
                None
            } else {
                Some(saved)
            },
        };
        task.enter(ch, room, maps);
        task
    }

    pub fn step(&self) -> Step {
        self.step
    }

    /// Set the Bim and the room up for whichever step we just moved into.
    fn enter(&mut self, ch: &mut Character, room: &mut Room, maps: &Maps) {
        use Step::*;
        self.elapsed = 0.0;
        self.done_count = 0;

        // Walking steps: point the Bim at the right spot and let it march.
        if let Some(to) = destination(self.kind, self.step, room, ch.pos) {
            // Routed around the furniture, same as a player order. The grid is
            // chosen here rather than by the caller because the step before
            // this one may have just opened the door.
            let nav = maps.pick(room.bath.is_open());
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
            | ShutDishwasher | StartDishwasher | TakeBowl | FillBowl => {
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
                ch.sit(room.bath.toilet_seat(), FACE_OFF_PAN);
                ch.set_action(Action::Reach);
            }
            // Sitting there. No animation is the point of it.
            UseToilet => ch.set_action(Action::None),
            RiseFromToilet => {
                ch.stand_at(room.bath.toilet_station());
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
                ch.face(FACE_BED);
                ch.set_action(Action::Reach);
            }
            Doze => {
                ch.lie(room.bed_lie_pos(), FACE_PILLOW);
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
                ch.sit(room.chair, FACE_TABLE);
                ch.set_action(Action::Reach);
            }
            Eat => ch.set_action(Action::Eat),
            // Gathering up after the meal, still at the table.
            ClearTable => {
                ch.face(FACE_TABLE);
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
            OpenFridge => room.set_fridge_open(true),
            CloseFridge => room.set_fridge_open(false),
            GoToDrawerForKnife | GoToDrawerForPlate => {}
            TakeKnife | TakePlateAndSpoon | TakeBowl => room.set_drawer_open(true),
            Doze => room.set_bed_occupied(true),
            OpenDishwasher => room.dishwasher.set_open(true),
            ShutDishwasher => room.dishwasher.set_open(false),
            // The door slides as the Bim touches the panel, so the walk that
            // follows already has somewhere to go.
            OpenDoor | UnlockDoor => {
                room.bath.set_locked(false);
                room.bath.set_open(true);
            }
            // Shut behind itself, and locked: the point of a door.
            ShutDoor => {
                room.bath.set_open(false);
                room.bath.set_locked(true);
            }
            ShutDoorBehind => room.bath.set_open(false),
            WashHands => {
                room.bath.run_tap(WASH_TIME);
                // A basin is a basin: it gets the worst of a mess off a Bim,
                // and is the only thing aboard that does so far.
                ch.wash(WASH_TAKES_OFF);
            }
            SitDown => {
                // The plate goes on the table as the Bim sits down to it.
                if let Held::Plate(fill, _) = ch.main_held() {
                    room.plate_on_table = Some(fill);
                }
                ch.hold_main(Held::Nothing);
                ch.hold_tool(Held::Slices); // stands in for a fork
            }
            _ => {}
        }
    }

    /// Everything that changes as a step finishes.
    fn leave(&mut self, ch: &mut Character, room: &mut Room) {
        use Step::*;
        match self.step {
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
                ch.hold_main(Held::Nothing);
                room.board_veg = 1.0;
            }
            TakeKnife => {
                ch.hold_tool(Held::Knife);
                room.set_drawer_open(false);
            }
            PutKnifeDown => {
                ch.hold_tool(Held::Nothing);
                room.knife_on_board = true;
            }
            GatherSlices => {
                ch.hold_main(Held::Slices);
                room.board_slices = 0;
            }
            TipIntoPot => {
                ch.hold_main(Held::Nothing);
                room.pot_contents = 1.0;
                room.pot_cooked = 0.0;
                room.pot_servings = SERVINGS_PER_POT;
            }
            TurnStoveOn => room.set_stove(true),
            TurnStoveOff => room.set_stove(false),
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
            WorkTray => {
                if let Some(job) = room.bay.wants_work(room.veg, room.tofu) {
                    if let Some(crop) = room.bay.work(job) {
                        room.store(crop);
                    }
                }
            }
            TakePlateAndSpoon => {
                ch.hold_main(Held::Plate(0.0, room.dish));
                ch.hold_tool(Held::Spoon);
                room.set_drawer_open(false);
            }
            TakeBowl => {
                ch.hold_main(Held::Plate(0.0, Dish::Bowl));
                room.set_drawer_open(false);
            }
            // Chopped tofu and the salad go in together, and that is the meal.
            FillBowl => {
                ch.hold_main(Held::Plate(1.0, Dish::Bowl));
                room.board_slices = 0;
                room.board_veg = 0.0;
            }
            Chop => self.chopped += 1,
            SetPlateDown => {
                ch.hold_main(Held::Nothing);
                room.plate_on_counter = Some(0.0);
            }
            // The helping comes off the pot here rather than spoonful by
            // spoonful, so an interrupted serve costs the pot nothing: what
            // the scoops move is the picture, and this is the count.
            Serve => {
                room.pot_servings = room.pot_servings.saturating_sub(1);
                room.pot_contents = room.pot_servings as f32 / SERVINGS_PER_POT as f32;
            }
            PickUpPlate => {
                let fill = room.plate_on_counter.take().unwrap_or(0.0);
                ch.hold_main(Held::Plate(fill, room.dish));
                ch.hold_tool(Held::Spoon);
            }
            // The plate and the cutlery come up off the table together; the
            // fork is already in hand from eating with it.
            ClearTable => {
                room.plate_on_table = None;
                ch.hold_main(Held::Plate(0.0, room.dish));
            }
            StackDishes => {
                ch.hold_main(Held::Nothing);
                ch.hold_tool(Held::Nothing);
                room.dishwasher.stack();
            }
            StartDishwasher => room.dishwasher.start(),
            FlushToilet => room.bath.flush(),
            // Back down the ladder: the Bim was lying in the middle of the
            // bed, and the floor beside it is the only place it can stand.
            WakeUp => {
                ch.stand_at(room.bed_station());
                room.set_bed_occupied(false);
            }
            _ => {}
        }
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
                Chop => {
                    room.board_veg = (1.0 - self.done_count as f32 / CHOPS as f32).max(0.0);
                    room.board_slices += 1;
                }
                Serve => {
                    room.plate_on_counter = Some(self.done_count as f32 / SCOOPS as f32);
                    // One helping out of however many are left, spread over the
                    // spoonfuls. Worked out from the count rather than taken
                    // off what is there, so a serve that was interrupted and
                    // started again does not drain the pot twice.
                    let left = room.pot_servings as f32 - self.done_count as f32 / SCOOPS as f32;
                    room.pot_contents = (left / SERVINGS_PER_POT as f32).max(0.0);
                }
                Eat => {
                    let left = 1.0 - self.done_count as f32 / BITES as f32;
                    room.plate_on_table = Some(left.max(0.0));
                }
                _ => {}
            }
        }
    }

    /// `fumble` is the chance a finished step has to be done over again —
    /// zero for a Bim that has slept, rising as it goes without.
    pub fn update(
        &mut self,
        dt: f32,
        ch: &mut Character,
        room: &mut Room,
        maps: &Maps,
        rng: &mut Rng,
        fumble: f32,
    ) {
        if self.step == Step::Done {
            return;
        }

        // Nowhere to walk: give the errand up, and put the world back in a
        // state the Bim can be left in.
        if self.blocked {
            Task::let_go(self.kind, self.step, ch, room);
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
                self.enter(ch, room, maps);
                self.elapsed = saved.elapsed;
                self.done_count = saved.done_count;
                return;
            }
            self.leave(ch, room);
            self.step = self.next_step(room);
            if self.step == Step::Done {
                ch.set_scripted(false);
                return;
            }
            self.enter(ch, room, maps);
        }
    }
}
