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
use crate::room::Room;

/// How many knife strokes it takes to get through one vegetable.
const CHOPS: u32 = 8;
/// How many spoonfuls go from the pot onto the plate.
const SCOOPS: u32 = 3;
/// How many mouthfuls it takes to clear the plate.
const BITES: u32 = 6;
/// How long the pot sits bubbling before it is ready to serve.
const COOK_TIME: f32 = 6.0;
/// How much of the pot one serving takes, so there is always some left over.
const SERVING: f32 = 0.17;
/// How long the Bim sits there, and how long it spends at the basin after.
const TOILET_TIME: f32 = 5.0;
const WASH_TIME: f32 = 4.2;

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

    // Flipping the hob is its own little errand: the Bim has to be standing
    // at the cooker to touch it.
    GoToSwitch,
    FlipSwitch,

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
            GoToSwitch => FlipSwitch,
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
            StandUp | FlipSwitch | ClimbOutOfBed | ShutDoorBehind | Done => Done,
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
            TipIntoPot => 0.9,
            TurnStoveOn | TurnStoveOff => 0.5,
            Cook => COOK_TIME,
            Chop => CHOP_PERIOD * CHOPS as f32,
            Serve => SCOOP_PERIOD * SCOOPS as f32,
            SitDown | StandUp => 0.6,
            FlipSwitch => 0.5,
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
                | GoToSwitch
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
    Meal,
    Stove,
    Rest,
    Heads,
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
        };
        task.enter(ch, room, maps);
        task
    }

    /// Start the make-a-meal chain.
    pub fn make_food(ch: &mut Character, room: &mut Room, maps: &Maps) -> Task {
        room.reset_for_cooking();
        Task::starting_at(Kind::Meal, Step::GoToFridge, 0.0, ch, room, maps)
    }

    /// Walk over to the cooker and flip the hob. Switching it is a physical
    /// act, so it takes the Bim as long as the walk does.
    pub fn stove_switch(ch: &mut Character, room: &mut Room, maps: &Maps) -> Task {
        Task::starting_at(Kind::Stove, Step::GoToSwitch, 0.0, ch, room, maps)
    }

    /// Send the Bim to the heads. Refused from the outside if the door is
    /// locked — the host greys the menu item out for the same reason.
    pub fn use_toilet(ch: &mut Character, room: &mut Room, maps: &Maps) -> Task {
        Task::starting_at(Kind::Heads, Step::GoToDoor, 0.0, ch, room, maps)
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

    pub fn step(&self) -> Step {
        self.step
    }

    /// Set the Bim and the room up for whichever step we just moved into.
    fn enter(&mut self, ch: &mut Character, room: &mut Room, maps: &Maps) {
        use Step::*;
        self.elapsed = 0.0;
        self.done_count = 0;

        // Walking steps: point the Bim at the right spot and let it march.
        let destination: Option<Vec2> = match self.step {
            GoToFridge => Some(room.fridge_station()),
            CarryToBoard | BackToBoard => Some(room.board_station()),
            GoToDrawerForKnife | GoToDrawerForPlate => Some(room.drawer_station()),
            CarryToPot => Some(room.stove_station()),
            // Serving needs pot and plate either side, so it has its own spot.
            BackToStove => Some(room.serve_station()),
            // The chair sits clear of the table footprint, so the Bim can
            // actually stand on it before sitting down.
            CarryToTable => Some(room.chair),
            GoToSwitch => Some(room.stove_station()),
            GoToBed => Some(room.bed_station()),
            GoToDoor | StepOutside => Some(room.bath.outside_station()),
            StepInside | BackToDoor => Some(room.bath.inside_station()),
            GoToToilet => Some(room.bath.toilet_station()),
            GoToSink => Some(room.bath.sink_station()),
            _ => None,
        };
        if let Some(to) = destination {
            // Routed around the furniture, same as a player order. The grid is
            // chosen here rather than by the caller because the step before
            // this one may have just opened the door.
            let nav = maps.pick(room.bath.is_open());
            ch.follow_path(nav.path(ch.pos, nav.nearest_free(to)));
            ch.set_action(Action::None);
            return;
        }

        // Standing steps: face the work and start the right animation.
        match self.step {
            OpenFridge | CloseFridge | TakeVegetable | PutVegetableDown | TakeKnife
            | PutKnifeDown | GatherSlices | TipIntoPot | TurnStoveOn | TurnStoveOff
            | TakePlateAndSpoon | SetPlateDown | PickUpPlate | FlipSwitch => {
                ch.face(FACE_WALL);
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
            Serve => {
                ch.face(FACE_WALL);
                ch.set_action(Action::Serve);
            }
            SitDown => {
                ch.sit(room.chair, FACE_TABLE);
                ch.set_action(Action::Reach);
            }
            Eat => ch.set_action(Action::Eat),
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
            TakeKnife | TakePlateAndSpoon => room.set_drawer_open(true),
            Doze => room.set_bed_occupied(true),
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
            WashHands => room.bath.run_tap(WASH_TIME),
            SitDown => {
                // The plate goes on the table as the Bim sits down to it.
                if let Held::Plate(fill) = ch.main_held() {
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
            TakeVegetable => ch.hold_main(Held::Vegetable),
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
                room.pot_contents = (room.pot_contents + 1.0).min(1.0);
                room.pot_cooked = 0.0;
            }
            TurnStoveOn => room.set_stove(true),
            TurnStoveOff => room.set_stove(false),
            // The player asked for a toggle, so read the state at the moment
            // the Bim's hand actually reaches the knob.
            FlipSwitch => {
                let on = room.stove_on;
                room.set_stove(!on);
            }
            TakePlateAndSpoon => {
                ch.hold_main(Held::Plate(0.0));
                ch.hold_tool(Held::Spoon);
                room.set_drawer_open(false);
            }
            SetPlateDown => {
                ch.hold_main(Held::Nothing);
                room.plate_on_counter = Some(0.0);
            }
            PickUpPlate => {
                let fill = room.plate_on_counter.take().unwrap_or(0.0);
                ch.hold_main(Held::Plate(fill));
                ch.hold_tool(Held::Spoon);
            }
            StandUp => {
                ch.hold_tool(Held::Nothing);
                ch.set_scripted(false);
            }
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
                    // The pot keeps most of its contents: this is one helping.
                    room.pot_contents = (room.pot_contents - SERVING).max(0.25);
                }
                Eat => {
                    let left = 1.0 - self.done_count as f32 / BITES as f32;
                    room.plate_on_table = Some(left.max(0.0));
                }
                _ => {}
            }
        }
    }

    pub fn update(&mut self, dt: f32, ch: &mut Character, room: &mut Room, maps: &Maps) {
        if self.step == Step::Done {
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
            self.leave(ch, room);
            self.step = self.step.next();
            if self.step == Step::Done {
                ch.set_scripted(false);
                return;
            }
            self.enter(ch, room, maps);
        }
    }
}
