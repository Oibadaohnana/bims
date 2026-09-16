//! World state: the room, the Bim in it, the job it is doing, and the
//! per-frame draw list handed to the renderer.

use crate::bim::{Bim, CREW, PLAYER, TRAIL_LIFE};
use crate::character::{ACCENT, Action, BODY_MARGIN, Held};
use crate::clock::MINUTES_PER_SECOND;
use crate::clock::{self, Clock};
use crate::draw::{Color, DrawList};
use crate::filth;
use crate::health::Malnutrition;
use crate::hydro;
use crate::manager::Manager;
use crate::math::{Rect, Vec2, vec2};
use crate::memory::What;
use crate::nav::Maps;
use crate::needs::{Need, Urge};
use crate::rng::Rng;
use crate::room::{self, Dish, GLOW, HIT_NONE, ROOM_H, ROOM_W, Room, Switch, WARN};
use crate::schedule::{IGNORE_ABOVE, Schedule, Slot, WAKE_AT};
use crate::social;
use crate::task::{self, Kind, SLEEP_MINUTES, Saved, Task};
use crate::work::{self, Job, Priorities};

const TRAIL: Color = ACCENT;
const MARQUEE_EDGE: Color = ACCENT;
const MARQUEE_FILL: Color = Color::rgba(0.50, 0.82, 0.66, 0.10);

/// How long the ping at an ordered destination lasts.
const MARKER_LIFE: f32 = 0.7;

/// How far outside a fixture its highlight ring sits. Enough to read as a ring
/// round the thing rather than an outline drawn on it.
const HIGHLIGHT_MARGIN: f32 = 10.0;

/// How close one Bim has to be to another to count as having seen what
/// happened to it. About a third of the room: across the galley, not through
/// the bulkhead into the heads.
const WITHIN_SIGHT: f32 = 260.0;

/// How long the bathroom door stands open after the Bim has walked through it
/// before sliding shut again. Doors aboard shut themselves; the chains that
/// close one by hand simply get there first.
const DOOR_SHUT_AFTER: f32 = 5.0;
/// How much room the Bim needs either side of the doorway to count as clear
/// of it — its own half-width, near enough.
const DOOR_CLEARANCE: f32 = 16.0;

/// How close two Bims have to be to be squeezing past one another. A little
/// under two body margins: shoulder to shoulder in the gangway beside the
/// table, not merely in the same half of the room.
const CREW_CLEARANCE: f32 = 2.0 * BODY_MARGIN - 8.0;

/// How long one of them holds the floor before the other gets a word in, in
/// game minutes. Read off the ship's clock rather than off either chain, so
/// the two always agree about whose turn it is and exactly one bubble is up.
const TAKES_A_TURN: f32 = 2.0;

/// How far apart two Bims stand to talk, measured centre to centre.
///
/// A body is `BODY_MARGIN` across the radius, so anything under about fifty
/// puts them inside one another. This leaves a clear gap between them and,
/// just as much to the point, keeps the two *names* apart: the host paints
/// those a body's height above each head, and a pair standing closer than
/// this have their labels written across each other.
const TALKING_GAP: f32 = 76.0;

/// How far back through a diary a Bim will reach for something to say. A few
/// days of entries, so the conversation is about the week rather than about
/// last month.
const RECENT_ENOUGH: usize = 40;

/// What squeezing past costs, as a fraction of the usual pace.
///
/// Bodies do not block one another — they pass through, which is the one thing
/// that cannot deadlock. A route is planned once and never replanned here, so
/// anything that stops a Bim getting where its line goes risks two of them
/// standing nose to nose for ever with errands on both agendas; letting them
/// overlap gives that failure nowhere to happen. The cost is speed instead:
/// close quarters are slow, and two Bims edging round each other in a gangway
/// take noticeably longer than one.
const CROWDED_PACE: f32 = 0.70;

/// How long the Bim hops about for, and how long a bout of sickness takes.
const FIDGET_TIME: f32 = 1.4;
const RETCH_TIME: f32 = 2.2;

/// How often a Bim that wants to be somewhere cleaner looks for somewhere to
/// go, and how far it looks, in tiles. Rechecking every frame would have it
/// replanning on the spot; this is often enough to look decisive.
const FLEE_EVERY: f32 = 2.0;
const FLEE_LOOK: i32 = 4;

/// How long a nod-off lasts, and how often one comes over a Bim that is past
/// the point of staying upright — about one in every three quarters of an hour
/// on its feet, so it spends a fifth of that time out cold.
const NAP_MINUTES: f32 = 15.0;
const NAP_EVERY: f32 = 45.0;

/// What the Bim is doing, as plain codes the host turns into words. A nap and
/// a full sleep are the same chain but read differently, so they count as two.
pub const JOB_MEAL: u32 = 1;
pub const JOB_STOVE: u32 = 2;
pub const JOB_NAP: u32 = 3;
pub const JOB_SLEEP: u32 = 4;
pub const JOB_HEADS: u32 = 5;
pub const JOB_FRIDGE: u32 = 6;
pub const JOB_BATH_DOOR: u32 = 7;
pub const JOB_BATH_LOCK: u32 = 8;
pub const JOB_DISHWASHER: u32 = 9;
pub const JOB_BOWL: u32 = 10;
pub const JOB_TEND: u32 = 11;
pub const JOB_LEFTOVERS: u32 = 12;
pub const JOB_CLEAN: u32 = 13;
pub const JOB_CHAT: u32 = 14;

fn job_code(kind: Kind, rest_minutes: f32) -> u32 {
    match kind {
        Kind::Meal(Dish::Stew) => JOB_MEAL,
        Kind::Meal(Dish::Bowl) => JOB_BOWL,
        Kind::Rest if rest_minutes >= SLEEP_MINUTES => JOB_SLEEP,
        Kind::Rest => JOB_NAP,
        Kind::Heads => JOB_HEADS,
        Kind::Switch(Switch::Hob) => JOB_STOVE,
        Kind::Switch(Switch::FridgeDoor) => JOB_FRIDGE,
        Kind::Switch(Switch::BathDoor(_)) => JOB_BATH_DOOR,
        Kind::Switch(Switch::BathLock(_)) => JOB_BATH_LOCK,
        Kind::Switch(Switch::Dishwasher) => JOB_DISHWASHER,
        Kind::Tend(_) => JOB_TEND,
        Kind::Leftovers => JOB_LEFTOVERS,
        Kind::Clean => JOB_CLEAN,
        Kind::Chat => JOB_CHAT,
    }
}

/// The room after dark. Laid over the finished frame rather than mixed into
/// every colour, so nothing in `room.rs` has to know what time it is.
const NIGHT: Color = Color::rgb(0.03, 0.05, 0.13);
/// How dark it gets at the middle of the night. Short of black: you still
/// want to see the Bim sleeping.
const NIGHT_DEPTH: f32 = 0.46;

struct Marker {
    pos: Vec2,
    age: f32,
    /// A place the Bim cannot get to. Drawn in the one warm colour the room
    /// keeps for things worth noticing, so a refused order is not silent.
    bad: bool,
}

/// A part of the ship only one Bim can be using at a time.
///
/// Most of the room is shared happily — two of them can walk past each other,
/// sit at the table, sleep — but some of it is one set of hands' worth. There
/// is one pot, one board, one knife and one fridge door; there is one pan.
/// Two chains running through the same fixtures would each set state the other
/// reads, and the visible half of that is a fridge door flapping while two
/// meals fight over it.
///
/// So an errand that needs one of these does not start while the other Bim is
/// on an errand that needs the same. It is not a queue and nobody waits in
/// line: the errand simply is not begun, and `consider_errand` moves on to
/// whatever else that Bim could be doing. It will come round again.
#[derive(Clone, Copy, PartialEq)]
enum Exclusive {
    /// The galley end: the cold store, the board, the pot, the hob, the
    /// dishwasher. Tending the bay is in here too — that chain ends by putting
    /// the harvest in the fridge, and the fridge is the galley's.
    Galley,
    /// The heads: one pan, one basin, one door.
    Heads,
    /// The broom. There is one of it, so one Bim sweeps at a time — the other
    /// would otherwise set off for the same tile with the same broom.
    Broom,
}

/// What a chain needs to itself, or `None` when it treads on nothing.
fn exclusive(kind: Kind) -> Option<Exclusive> {
    match kind {
        Kind::Meal(_) | Kind::Leftovers | Kind::Tend(_) => Some(Exclusive::Galley),
        Kind::Switch(Switch::Hob | Switch::FridgeDoor | Switch::Dishwasher) => {
            Some(Exclusive::Galley)
        }
        Kind::Heads | Kind::Switch(Switch::BathDoor(_) | Switch::BathLock(_)) => {
            Some(Exclusive::Heads)
        }
        Kind::Clean => Some(Exclusive::Broom),
        // A berth apiece, so going to bed treads on nobody. And a
        // conversation is the one errand that *wants* the other Bim doing the
        // same thing at the same time, so it can hardly reserve anything.
        Kind::Rest | Kind::Chat => None,
    }
}

/// What became of a right-click on the floor. The host turns these into words.
pub const ORDER_IGNORED: u32 = 0;
pub const ORDER_MOVING: u32 = 1;
/// Reachable, but the door has to be opened on the way, which the Bim is now
/// going to do before carrying on.
pub const ORDER_VIA_DOOR: u32 = 2;
/// Reachable only through a door that is locked.
pub const ORDER_LOCKED: u32 = 3;
/// No route there at all, with every door in the place wide open.
pub const ORDER_NOWHERE: u32 = 4;

pub struct Game {
    room: Room,
    clock: Clock,
    /// Built once, one grid per state of the bathroom door.
    maps: Maps,
    /// Everything the body is pushed out of, the shut door included. Rebuilt
    /// only when the door changes, which physics can afford to be a frame
    /// behind on — routing, which cannot, goes through `maps` instead.
    blockers: Vec<Rect>,
    door_was_shut: bool,
    /// Which side of the bathroom bulkhead the Bim was on last frame, and how
    /// long the door has left to stand open after it crossed.
    bim_was_inside: bool,
    door_shut_in: f32,
    rng: Rng,
    /// The crew. Two of them, and the index into this is a Bim's whole
    /// identity — it picks the berth, the seat at the table, the coverall, and
    /// the name the host prints over its head.
    bims: Vec<Bim>,
    /// Which of the crew is ticked first this frame. Rotates every frame so
    /// first refusal on the galley and the heads goes round rather than
    /// always falling to the same Bim.
    first_tick: usize,
    /// Corners of the selection box while the mouse is down; `None` otherwise.
    drag: Option<(Vec2, Vec2)>,
    markers: Vec<Marker>,

    /// The day's timetable. Only sleep is on it so far, and there is one of it
    /// for the whole ship: the crew keep the same hours.
    schedule: Schedule,
    /// What the place has been told to keep in stock. The bay works to it.
    manager: Manager,
    /// The order the player wants the work done in. One list for the ship,
    /// like the timetable: both crew work to it. See `work.rs`.
    priorities: Priorities,
    autonomous: bool,
    /// A `SPOT_` code the host has asked to have ringed on the deck, or
    /// `SPOT_NOTHING`. This is how a panel that names a place — "Cold store",
    /// "Pot on the hob" — points at the actual thing rather than leaving the
    /// player to hunt for it. Set and cleared by the pointer; nothing in the
    /// simulation reads it.
    highlight: u32,
    list: DrawList,

    /// Maps the fixed-size room onto whatever canvas the host has. The host
    /// applies this before replaying the draw list, and inverts it to turn
    /// pointer positions back into room coordinates.
    view_scale: f32,
    view_offset: Vec2,
}

impl Game {
    pub fn new(seed: u64, width: f32, height: f32) -> Game {
        let room = Room::new();
        let maps = Maps::new(room.interior, &room.solids(), room.bath.door, BODY_MARGIN);
        let mut rng = Rng::new(seed);
        // Each starts somewhere in the open floor, clear of the kitchen units
        // and a body's width apart, so the first frame does not begin with the
        // two of them shoving each other out of one spot.
        let bims = (0..CREW)
            .map(|who| {
                let start = vec2(
                    rng.range(ROOM_W * 0.30, ROOM_W * 0.70),
                    rng.range(ROOM_H * 0.42 + who as f32 * 0.12, ROOM_H * 0.52),
                );
                Bim::new(who, start, &mut rng)
            })
            .collect();
        let mut game = Game {
            room,
            clock: Clock::new(),
            maps,
            blockers: Vec::new(),
            door_was_shut: false,
            bim_was_inside: false,
            door_shut_in: 0.0,
            rng,
            bims,
            first_tick: 0,
            drag: None,
            markers: Vec::new(),
            schedule: Schedule::new(),
            manager: Manager::new(),
            priorities: Priorities::new(),
            autonomous: true,
            highlight: room::SPOT_NOTHING,
            list: DrawList::new(),
            view_scale: 1.0,
            view_offset: Vec2::ZERO,
        };
        game.refresh_blockers();
        game.resize(width, height);
        game
    }

    /// The fixed furniture plus the door, if the door is currently something
    /// to walk into.
    fn refresh_blockers(&mut self) {
        self.door_was_shut = self.room.closed_door().is_some();
        self.blockers = self.room.solids().to_vec();
        if let Some(door) = self.room.closed_door() {
            self.blockers.push(door);
        }
    }

    /// Fit the room into the canvas, centred, without distorting it.
    pub fn resize(&mut self, width: f32, height: f32) {
        let w = width.max(1.0);
        let h = height.max(1.0);
        self.view_scale = (w / ROOM_W).min(h / ROOM_H);
        self.view_offset = vec2(
            (w - ROOM_W * self.view_scale) * 0.5,
            (h - ROOM_H * self.view_scale) * 0.5,
        );
    }

    pub fn view_scale(&self) -> f32 {
        self.view_scale
    }

    pub fn view_offset(&self) -> Vec2 {
        self.view_offset
    }

    pub fn update(&mut self, dt: f32) {
        self.clock.advance(dt);
        self.room.update(dt);

        // The bay grows on the clock, against the manager's target. It is the
        // game that drives it rather than the room, because the target is the
        // manager's and the room has never heard of the manager.
        let want = self.manager.stock_target();
        let minutes = dt * MINUTES_PER_SECOND;
        self.room
            .bay
            .update(dt, minutes, self.room.veg, self.room.tofu, want);

        // The timetable is the ship's, not a Bim's, and `due` re-arms itself
        // the  it is asked — so it is asked once a frame here and the
        // answer handed to each of the crew, rather than the first one to look
        // taking the night for itself.
        let bedtime = self.schedule.due(self.clock.minutes());

        // Each in turn, and each entirely on its own account. Nothing below
        // knows or cares which one it is working on except where the room is
        // shared, and that is arbitrated by `galley_taken` and `heads_taken`.
        // Turn and turn about, rather than always in crew order.
        //
        // Whoever is ticked first gets first refusal on everything shared: it
        // looks at the heads, finds them free and starts, and the other then
        // finds them taken. In a fixed order that is the same Bim winning
        // every tie for the whole game. Nothing has been caught going wrong
        // because of it — measured over twenty simulated weeks, neither of
        // them was ever left desperate *and* barred from the pan — but a tie
        // that always breaks the same way is a bias whether or not it has bit
        // yet, and rotating the starting point costs nothing.
        let crew = self.bims.len();
        for step in 0..crew {
            self.tick_bim((self.first_tick + step) % crew, dt, minutes, bedtime);
        }
        self.first_tick = (self.first_tick + 1) % crew;

        // The locker door shows the broom when nobody has it. Derived rather
        // than set, so an errand given up mid-sweep cannot leave the cupboard
        // claiming to hold a broom that is in somebody's hands.
        self.room.broom_out = self
            .bims
            .iter()
            .any(|b| b.character.main_held() == Held::Broom);

        self.tick_door_closer(dt);
        if self.room.closed_door().is_some() != self.door_was_shut {
            self.refresh_blockers();
        }

        for m in &mut self.markers {
            m.age += dt;
        }
        self.markers.retain(|m| m.age < MARKER_LIFE);

        self.render();
    }

    /// One crew member's half of the frame.
    fn tick_bim(&mut self, who: usize, dt: f32, minutes: f32, bedtime: bool) {
        if self.bims[who].character.is_dead() {
            // Nothing else moves for this one. The room carries on — a cycle
            // finishes, a hob goes out — and so does the other Bim.
            let solids = self.blockers.clone();
            let (bims, room, rng) = (&mut self.bims, &self.room, &mut self.rng);
            bims[who].character.update(dt, room.interior, &solids, rng);
            return;
        }

        // Dropping off where it stands counts as sleeping for what it does to
        // the Bim — rest comes back at the proper rate — but not for what it
        // clears, because a quarter of an hour is worth a few per cent and the
        // status only lifts above four fifths rested.
        let nodded_off = self.bims[who].nap_left > 0.0;
        if nodded_off {
            self.bims[who].nap_left -= minutes;
            if self.bims[who].nap_left <= 0.0 {
                self.bims[who].nap_left = 0.0;
                self.bims[who].character.nod_off(false);
            }
        } else if self.bims[who].health.drowsiness().nods_off()
            && self.bims[who]
                .task
                .as_ref()
                .is_none_or(|t| t.restoring().is_none())
            && self.rng.chance(minutes / NAP_EVERY)
        {
            self.bims[who].nap_left = NAP_MINUTES;
            self.bims[who].character.nod_off(true);
            self.remember(who, What::NoddedOff, 0);
        }

        let restoring = if nodded_off {
            Some(Need::Rest)
        } else {
            self.bims[who].task.as_ref().and_then(|t| t.restoring())
        };
        let tiring = self.bims[who].health.stage().tiring();
        self.bims[who].needs.update(dt, restoring, tiring);

        // Cleanliness follows the deck rather than the clock: the tiles the
        // Bim is standing among, and what it has on itself.
        let grinding = self.room.filth.grinding(
            self.bims[who].character.pos,
            self.bims[who].character.filth(),
        );
        self.bims[who].needs.scrub(minutes, grinding);
        self.mind_the_mess(who, minutes, restoring == Some(Need::Rest));

        // Going without is measured in time, not in how empty the bar is.
        let (food, rest) = (
            self.bims[who].needs.level(Need::Food),
            self.bims[who].needs.level(Need::Rest),
        );
        self.bims[who]
            .health
            .update(minutes, food, rest, restoring == Some(Need::Rest));
        // Going a stage further into hunger or sleeplessness is worth
        // remembering — once, as it happens, rather than for every frame it
        // goes on lasting. Only downwards: coming back out of it is a relief
        // rather than an event, and the bars say so anyway.
        let hunger = self.bims[who].health.stage().stage();
        if hunger > self.bims[who].worst_hunger {
            self.bims[who].worst_hunger = hunger;
            self.remember(who, What::Hungrier, hunger);
        }
        let weary = self.bims[who].health.drowsiness().stage();
        if weary > self.bims[who].worst_weariness {
            self.bims[who].worst_weariness = weary;
            self.remember(who, What::Wearier, weary);
        }
        // A proper night, or a proper meal, sets the mark back so the next
        // slide into it is noticed afresh.
        if hunger == 0 {
            self.bims[who].worst_hunger = 0;
        }
        if weary == 0 {
            self.bims[who].worst_weariness = 0;
        }
        // Picking its way around a mess is slower than walking through it, and
        // so is squeezing past the other Bim. All three multiply: a starving
        // Bim edging round its shipmate through a fouled galley is slow for
        // three separate reasons and should read as slow.
        let pace = self.bims[who].health.stage().pace()
            * self.bims[who].ordeal.discomfort().pace()
            * self.bims[who].solitude.stage().works_at()
            * self.crowding(who);
        self.bims[who].character.set_pace(pace);
        if self.bims[who].health.is_dead() {
            self.die(who);
            return;
        }

        // Fully rested is fully rested, whatever the timetable or the clock
        // say: the Bim gets up rather than lying there.
        if self.bims[who].needs.level(Need::Rest) >= WAKE_AT {
            if let Some(task) = &mut self.bims[who].task {
                task.wake();
            }
        }

        // The timetable only ever adds a sleep, and only at the start of a
        // block. It goes on the *back* of the queue: a standing instruction
        // waits its turn, unlike an interruption, which jumps the front.
        //
        // The block is still tracked while the Bim is under orders — `due` is
        // asked once a frame at the top of `update` and has to see the hour go
        // by to re-arm — but nothing is added, because a recruited Bim takes
        // no instruction but the player's.
        if bedtime
            && !self.bims[who].character.is_recruited()
            && self.bims[who].needs.level(Need::Rest) <= IGNORE_ABOVE
        {
            self.bims[who]
                .queue
                .push(Saved::fresh(who, Kind::Rest, SLEEP_MINUTES));
        }

        // Out cold on its feet: the errand waits, and so does everything else.
        if self.bims[who].character.is_napping() {
            let (bims, room, blockers, rng) =
                (&mut self.bims, &self.room, &self.blockers, &mut self.rng);
            bims[who].character.update(dt, room.interior, blockers, rng);
            return;
        }

        // Alone too long: the clock, and whatever it has just cost.
        self.bear_the_solitude(who, minutes);
        // Sitting on the deck having given up for a bit. The same shape as a
        // nod-off: the frame stops here for this Bim, and whatever it was in
        // the middle of is still there when it gets up.
        if self.bims[who].sad_left > 0.0 {
            self.bims[who].sad_left -= minutes;
            if self.bims[who].sad_left <= 0.0 {
                self.bims[who].sad_left = 0.0;
                self.bims[who].character.stand();
            }
            let (bims, room, blockers, rng) =
                (&mut self.bims, &self.room, &self.blockers, &mut self.rng);
            bims[who].character.update(dt, room.interior, blockers, rng);
            return;
        }

        let fumble = self.bims[who].health.drowsiness().fumble();
        // A Bim with nobody to talk to drags: a tenth longer over everything
        // it does. `Task` applies it only to the working steps — sleeping and
        // eating are not work and are not slowed — and the walking half of the
        // same tenth is in the pace above.
        let effort = self.bims[who].solitude.stage().works_at();
        {
            let (bims, room, maps, rng) =
                (&mut self.bims, &mut self.room, &self.maps, &mut self.rng);
            let bim = &mut bims[who];
            if let Some(task) = &mut bim.task {
                task.update(dt, &mut bim.character, room, maps, rng, fumble, effort);
                if task.is_done() {
                    bim.task = None;
                }
            }
        }
        self.take_pending_move(who);
        self.pump_queue(who);
        self.consider_errand(who);
        self.flee_filth(who, dt);

        {
            let (bims, room, blockers, rng) = (
                &mut self.bims,
                &mut self.room,
                &self.blockers,
                &mut self.rng,
            );
            // Dirt travels on boots. Where the body was, and where this frame
            // of walking has put it — taken either side of the one call that
            // moves it, so that a chain setting a Bim down somewhere (into a
            // bunk, onto a chair) is not read as a stride across the deck.
            let was = bims[who].character.pos;
            bims[who].character.update(dt, room.interior, blockers, rng);
            room.filth.track(was, bims[who].character.pos, rng);
        }
        self.bims[who].tick_trail(dt);
    }

    /// How much `who` is slowed by having somebody in the same space.
    ///
    /// The crew do not collide. Two of them walking into each other pass
    /// straight through and both slow down, which is the one resolution that
    /// cannot leave anybody stuck — and being stuck is the real hazard here,
    /// because a route is planned once and never replanned, so a Bim whose
    /// line goes through another has no second plan to fall back on.
    ///
    /// Only bodies on their feet count. One sitting at the table or asleep in
    /// its bunk is tucked into the furniture rather than in the gangway, and
    /// slowing somebody walking past the foot of a bed would be wrong.
    fn crowding(&self, who: usize) -> f32 {
        let at = self.bims[who].character.pos;
        let close = self.bims.iter().enumerate().any(|(i, other)| {
            i != who
                && other.is_alive()
                && !other.character.is_seated()
                && (other.character.pos - at).len() < CREW_CLEARANCE
        });
        if close { CROWDED_PACE } else { 1.0 }
    }

    // --- what a Bim remembers ---------------------------------------------

    /// Write one line of the diary.
    fn remember(&mut self, who: usize, what: What, detail: u32) {
        let (day, at) = (self.clock.day(), self.clock.minutes());
        self.bims[who].memory.note(day, at, what, detail);
    }

    /// Something happened to `who` that the rest of the crew could see.
    ///
    /// Only what is actually in front of them, and only if they are awake to
    /// see it: a Bim asleep in its bunk on the far side of the compartment did
    /// not witness anything, and a diary that says it did is a diary nobody
    /// can trust.
    fn witnessed(&mut self, who: usize, what: What) {
        let at = self.bims[who].character.pos;
        for other in 0..self.bims.len() {
            if other == who {
                continue;
            }
            let them = &self.bims[other];
            let close = (them.character.pos - at).len() <= WITHIN_SIGHT;
            if !close || !them.is_alive() || them.character.is_napping() {
                continue;
            }
            // In bed with the covers up is not watching the room either.
            if them.task.as_ref().is_some_and(|t| t.rest_left() > 0.0) {
                continue;
            }
            self.remember(other, what, who as u32);
        }
    }

    // --- mess -------------------------------------------------------------

    /// Everything a mess does to the Bim, and everything the Bim does about it
    /// short of an errand: hopping about, accidents, and being sick.
    ///
    /// None of it interrupts a chain. These are things that happen *to* the
    /// Bim — a Bim that wets itself on the way to the galley carries on to the
    /// galley — and the pose is only taken up when nothing else owns it.
    fn mind_the_mess(&mut self, who: usize, minutes: f32, asleep: bool) {
        // Asleep, the needs are frozen — see `needs.rs` — so the accidents that
        // hang off them are frozen too. Without that a Bim that turns in at the
        // middle urge rolls the one-in-ten every hour of a six-hour night and
        // wets the bed about half the time, for a need that is not even moving.
        let urge = if asleep {
            Urge::None
        } else {
            self.bims[who].needs.urge()
        };
        let (restroom, cleanliness) = (
            self.bims[who].needs.level(Need::Restroom),
            self.bims[who].needs.level(Need::Cleanliness),
        );
        let (bims, rng) = (&mut self.bims, &mut self.rng);
        let mishap = bims[who].ordeal.update(
            minutes,
            restroom,
            cleanliness,
            urge == Urge::Extreme,
            urge == Urge::Medium,
            rng,
        );

        let at = self.bims[who].character.pos;
        // Relief is relief, however it comes: the need goes back up, and
        // everything else gets worse. Without that the Bim would foul the
        // deck again on the very next frame, for ever.
        if mishap.fouled {
            let (on_bim, relief) = self.room.filth.foul(at);
            self.bims[who].character.soil(on_bim);
            self.bims[who].needs.refill(Need::Restroom, relief);
            self.remember(who, What::Accident, 1);
            self.witnessed(who, What::SawAccident);
        } else if mishap.wet {
            let (on_bim, relief) = self.room.filth.wet(at);
            self.bims[who].character.soil(on_bim);
            self.bims[who].needs.refill(Need::Restroom, relief);
            self.remember(who, What::Accident, 0);
            self.witnessed(who, What::SawAccident);
        }
        if mishap.sick {
            self.room.filth.sick_on(at);
            self.bims[who]
                .needs
                .spend(Need::Food, filth::SICK_COSTS_FOOD);
            self.bims[who].character.antic(Action::Retch, RETCH_TIME);
            self.remember(who, What::WasSick, 0);
            self.witnessed(who, What::SawSickness);
        }

        // Hopping about on the spot: the one warning the player gets that an
        // accident is coming, and the only outward sign of a need that has no
        // bar-width left to lose.
        if !self.bims[who].character.in_antic() && self.rng.chance(minutes * urge.fidget_chance()) {
            self.bims[who].character.antic(Action::Fidget, FIDGET_TIME);
        }
    }

    /// Get away from it.
    ///
    /// Past the first stage of discomfort the Bim will not stand in a mess if
    /// there is anywhere better within a few tiles. It is a walk rather than an
    /// errand, so anything it is actually doing — and anything waiting on the
    /// queue — outranks it: it only moves when it would otherwise be idling.
    fn flee_filth(&mut self, who: usize, dt: f32) {
        self.bims[who].flee_wait = (self.bims[who].flee_wait - dt).max(0.0);
        if !self.bims[who].ordeal.discomfort().flees()
            || self.is_recruited()
            || !self.autonomous
            || self.bims[who].task.is_some()
            || !self.bims[who].queue.is_empty()
            || !self.bims[who].character.arrived()
            || self.bims[who].flee_wait > 0.0
        {
            return;
        }
        self.bims[who].flee_wait = FLEE_EVERY;
        let Some(to) = self
            .room
            .filth
            .somewhere_cleaner(self.bims[who].character.pos, FLEE_LOOK)
        else {
            return;
        };
        let nav = self.maps.pick(self.room.bath.is_open());
        let route = nav.path(self.bims[who].character.pos, nav.nearest_free(to));
        if !route.is_empty() {
            self.bims[who].character.follow_path(route);
        }
    }

    // --- interrupting, and getting back to it ----------------------------

    /// Put the running chain down and queue it to be picked up next.
    ///
    /// It goes on the *front*, so when one interruption interrupts another the
    /// chains come back in the order they were displaced: the most recently
    /// dropped is the first one resumed.
    fn interrupt(&mut self, who: usize) {
        if let Some(task) = self.bims[who].task.take() {
            // A conversation is dropped rather than put down. Half of one is
            // worth nothing, the other half of it has walked off by now, and
            // a chain that walks back to a spot on the deck to talk to nobody
            // is worse than no chain at all.
            let keep = task.kind() != Kind::Chat;
            self.bims[who].chat_topic = 0;
            let saved = task.suspend(&mut self.bims[who].character, &mut self.room);
            if keep {
                self.bims[who].queue.insert(0, saved);
            }
        }
    }

    /// The same, for a new order from the player, which also cancels any move
    /// that was waiting on a door.
    ///
    /// The errand opening that door goes with it: the two exist only to serve
    /// each other, so keeping the door errand would have the Bim walk back and
    /// work a panel for a destination nobody is going to any more. It is put
    /// down properly rather than dropped on the floor, so the hands and the
    /// scripting come back the way a suspend leaves them.
    fn interrupt_for_order(&mut self, who: usize) {
        let stale = self.bims[who].pending_move.take().is_some() && self.on_a_door_errand(who);
        if let Some(task) = self.bims[who].task.take() {
            let saved = task.suspend(&mut self.bims[who].character, &mut self.room);
            if !stale {
                self.bims[who].queue.insert(0, saved);
            }
        }
    }

    /// Whether the Bim is at this moment on its way to work the door panel.
    fn on_a_door_errand(&self, who: usize) -> bool {
        self.bims[who]
            .task
            .as_ref()
            .is_some_and(|task| matches!(task.kind(), Kind::Switch(Switch::BathDoor(_))))
    }

    /// Whether the Bim could actually walk to where a chain would start it.
    ///
    /// A chain that cannot be begun is not begun. A walk with nowhere to go
    /// reports itself arrived the instant it starts, and a chain that takes
    /// that for arrival runs through every remaining step in a single frame —
    /// which is how a Bim shut in the heads used to end up at the dining
    /// table without having walked there.
    fn can_begin(&self, who: usize, kind: Kind) -> bool {
        // Somebody else's hands are already in it. See [`Exclusive`].
        if exclusive(kind).is_some_and(|want| self.taken_by_other(who, want)) {
            return false;
        }
        let Some(to) = task::first_station(who, kind, &self.room, self.bims[who].character.pos)
        else {
            return true;
        };
        self.can_reach(who, to)
    }

    /// Whether any *other* Bim is on an errand that has the run of `want`.
    ///
    /// Only what is actually running counts. A chain on the queue is put down:
    /// its hands are off the pot, and holding the galley for it would have one
    /// Bim's interrupted meal keep the other out of the galley all night.
    fn taken_by_other(&self, who: usize, want: Exclusive) -> bool {
        self.bims.iter().enumerate().any(|(i, bim)| {
            i != who
                && bim
                    .task
                    .as_ref()
                    .is_some_and(|task| exclusive(task.kind()) == Some(want))
        })
    }

    /// Which Bim has the galley, counting from 1, or 0 for nobody. The host
    /// says whose it is when a menu item will not start.
    pub fn galley_held_by(&self) -> u32 {
        self.held_by(Exclusive::Galley)
    }

    pub fn broom_held_by(&self) -> u32 {
        self.held_by(Exclusive::Broom)
    }

    pub fn heads_held_by(&self) -> u32 {
        self.held_by(Exclusive::Heads)
    }

    fn held_by(&self, want: Exclusive) -> u32 {
        self.bims
            .iter()
            .position(|bim| {
                bim.task
                    .as_ref()
                    .is_some_and(|task| exclusive(task.kind()) == Some(want))
            })
            .map_or(0, |i| i as u32 + 1)
    }

    /// Shut the bathroom door behind whoever last walked through it.
    ///
    /// Crossing the bulkhead line arms the closer; three seconds later the
    /// door slides shut, wherever the Bim has got to by then. The chains that
    /// shut the door by hand still do — this only catches the door nobody
    /// thought to close, which otherwise stood open for the rest of the game.
    ///
    /// A Bim's own position is a frame old here, which at three seconds does
    /// not matter, and the doorway check is on the same footing as the walk it
    /// just finished.
    ///
    /// With two aboard every guard below has to hold for *either* of them. The
    /// door does not know whose trip it is: one crew member walking towards it
    /// is reason enough to hold it, and only when nobody is inside, nobody is
    /// heading through and nobody is standing in the opening does it shut.
    fn tick_door_closer(&mut self, dt: f32) {
        let inside = self
            .bims
            .iter()
            .any(|b| self.room.bath.shell.contains(b.character.pos));
        if inside != self.bim_was_inside {
            self.bim_was_inside = inside;
            self.door_shut_in = DOOR_SHUT_AFTER;
        }

        // Nothing to close, or nothing armed: a door shut by hand in the
        // meantime disarms the closer rather than leaving it to fire later.
        if self.door_shut_in <= 0.0 || !self.room.bath.is_open() {
            self.door_shut_in = 0.0;
            return;
        }

        // Somebody is on their way through: the door waits. A route is planned
        // once and never replanned, so a door that shuts across one leaves the
        // Bim walking into the panels for ever, and the chain waiting on an
        // arrival that cannot happen.
        //
        // Asked of each Bim against *its own* side of the bulkhead, not the
        // `inside` above: that one is true if either of them is in there, and
        // a Bim out on the deck heading for the heads while the other is
        // already inside would otherwise not count as crossing.
        let crossing = self.bims.iter().any(|b| {
            let here = self.room.bath.shell.contains(b.character.pos);
            b.character
                .destination()
                .is_some_and(|to| self.room.bath.shell.contains(to) != here)
        });
        if crossing {
            return;
        }

        // Still in the opening. The panels do not close on the Bim, so the
        // count is held where it is until everyone is clear of the doorway —
        // which also means the three seconds run from walking through it,
        // not from the moment the bulkhead line was crossed.
        let doorway = self.room.bath.door.expand(DOOR_CLEARANCE);
        if self.bims.iter().any(|b| doorway.contains(b.character.pos)) {
            return;
        }

        self.door_shut_in -= dt;
        if self.door_shut_in <= 0.0 {
            self.door_shut_in = 0.0;
            self.room.bath.set_open(false);
        }
    }

    fn can_reach(&self, who: usize, to: Vec2) -> bool {
        let nav = self.maps.pick(self.room.bath.is_open());
        !nav.path(self.bims[who].character.pos, nav.nearest_free(to))
            .is_empty()
    }

    /// Clear the way for a new errand. Returns false when the Bim is already
    /// on that very errand, so asking twice does not restart it.
    fn take_over(&mut self, who: usize, kind: Kind, minutes: f32) -> bool {
        if let Some(task) = &self.bims[who].task {
            if task.kind() == kind && (kind != Kind::Rest || task.rest_minutes() == minutes) {
                return false;
            }
        }
        self.interrupt(who);
        true
    }

    /// Back on its feet with something waiting: pick it up. Waits for an
    /// ordered walk to finish first, so the Bim gets where it was sent before
    /// returning to what it was doing.
    fn pump_queue(&mut self, who: usize) {
        // Under orders, work that was put down stays put down. The queue is
        // kept, not thrown away, so letting the Bim go picks it all up again.
        if self.is_recruited()
            || self.bims[who].task.is_some()
            || self.bims[who].queue.is_empty()
            || !self.bims[who].character.arrived()
        {
            return;
        }
        if !self.queue_ready(who) {
            return;
        }
        let saved = self.bims[who].queue.remove(0);
        self.bims[who].task = Some(Task::resume(
            saved,
            &mut self.bims[who].character,
            &mut self.room,
            &self.maps,
        ));
    }

    /// Whether the chain at the front of the queue is one the Bim should be
    /// getting on with right now.
    ///
    /// Queued work whose way is shut waits for the door rather than being
    /// thrown away — but it must not hold up everything else while it waits.
    /// A Bim with an unreachable meal on the queue still has to be free to go
    /// and do something it *can* reach, or it stands there until it starves.
    fn queue_ready(&self, who: usize) -> bool {
        self.bims[who].queue.first().is_some_and(|saved| {
            if saved.kind() == Kind::Rest && (self.eats_first(who) || self.goes_first(who)) {
                return false;
            }
            // Picking a chain back up is beginning one as far as the galley is
            // concerned. `pump_queue` does not go through `can_begin` — it
            // resumes rather than starts — so the check has to be here as
            // well, or a half-cooked meal resumes into a galley the other Bim
            // is standing in.
            if exclusive(saved.kind()).is_some_and(|want| self.taken_by_other(who, want)) {
                return false;
            }
            saved
                .resume_station(&self.room, self.bims[who].character.pos)
                .is_none_or(|to| self.can_reach(who, to))
        })
    }

    /// Whether a meal comes before a sleep the Bim was about to start.
    ///
    /// Food outranks rest when both are going begging. A Bim that turns in
    /// starving is a Bim that sleeps six hours on an empty stomach and wakes
    /// no better off, having lost health all night, while a meal costs it
    /// three quarters of an hour and puts the need away entirely.
    ///
    /// Only while there is a meal to be had, though: with the cold store empty
    /// or the galley out of reach, holding bedtime off would leave the Bim
    /// standing about all night instead, hungry *and* tired.
    fn eats_first(&self, who: usize) -> bool {
        // Only while the Bim is free to go and see to it. With autonomy off,
        // or under orders, nothing would ever start that meal — and a night
        // held back for a meal that is never cooked is a Bim that never
        // sleeps at all.
        self.autonomous
            && !self.bims[who].character.is_recruited()
            && self.bims[who]
                .needs
                .trigger(Need::Food)
                .caught(self.bims[who].needs.level(Need::Food))
            && self
                .need_station(who, Need::Food)
                .is_some_and(|to| self.can_reach(who, to))
    }

    /// Whether a trip to the heads comes before a sleep the Bim was about to
    /// start. You go before bed.
    ///
    /// Unlike [`Game::eats_first`] this does not wait for the trigger. The
    /// restroom need is the one thing that keeps draining through the night,
    /// and a night is six hours: turn in at four fifths and the Bim wakes at
    /// nothing, which is how three visits a day become four. Anything under
    /// [`needs::BEFORE_BED`] is worth emptying out first.
    ///
    /// The usual guards: only while the Bim is free to see to it — autonomy
    /// on, not under orders — and only while the pan is actually reachable. A
    /// locked door is not a reason to keep a tired Bim up.
    fn goes_first(&self, who: usize) -> bool {
        self.autonomous
            && !self.bims[who].character.is_recruited()
            && self.bims[who].needs.level(Need::Restroom) < crate::needs::BEFORE_BED
            && self.can_use_toilet(who)
    }

    /// Whether the rest trigger is asking for a bed — switched on, and the
    /// level past it. Not whether the Bim can have one.
    fn wants_bed(&self, who: usize) -> bool {
        self.bims[who]
            .needs
            .trigger(Need::Rest)
            .caught(self.bims[who].needs.level(Need::Rest))
    }

    /// Whether the thing waiting at the front of the queue is a lie-down.
    fn bed_is_queued(&self, who: usize) -> bool {
        self.bims[who]
            .queue
            .first()
            .is_some_and(|saved| saved.kind() == Kind::Rest)
    }

    // --- the agenda, for the host's checklist ----------------------------

    /// The chain running now, if any, followed by everything queued behind it.
    pub fn agenda_len(&self, who: usize) -> u32 {
        self.bims[who].task.is_some() as u32 + self.bims[who].queue.len() as u32
    }

    /// Which errand entry `i` is: see the `JOB_` codes. Zero if out of range.
    pub fn agenda_job(&self, who: usize, i: u32) -> u32 {
        match self.agenda_at(who, i) {
            Some((kind, minutes, _)) => job_code(kind, minutes),
            None => 0,
        }
    }

    /// How far through entry `i` is, 0 to 1. A queued chain keeps whatever it
    /// had reached when it was put down.
    pub fn agenda_progress(&self, who: usize, i: u32) -> f32 {
        self.agenda_at(who, i)
            .map_or(0.0, |(_, _, progress)| progress)
    }

    /// Non-zero for the one entry that is actually running.
    pub fn agenda_active(&self, who: usize, i: u32) -> u32 {
        (i == 0 && self.bims[who].task.is_some()) as u32
    }

    fn agenda_at(&self, who: usize, i: u32) -> Option<(Kind, f32, f32)> {
        let running = self.bims[who].task.is_some() as u32;
        if i < running {
            let task = self.bims[who].task.as_ref()?;
            return Some((task.kind(), task.rest_minutes(), task.progress()));
        }
        let saved = self.bims[who].queue.get((i - running) as usize)?;
        Some((saved.kind(), saved.rest_minutes(), saved.progress()))
    }

    // --- deciding for itself ---------------------------------------------

    /// The end of it. Whatever it was doing is dropped, and so is everything
    /// waiting behind it: there is no one left to do any of it.
    fn die(&mut self, who: usize) {
        // The others are told. This is the one thing that can happen aboard
        // that nobody could fail to notice, so it goes in every surviving
        // diary regardless of where they were standing — unlike `witnessed`,
        // which asks whether they could see it.
        for other in 0..self.bims.len() {
            if other != who && self.bims[other].is_alive() {
                self.remember(other, What::CrewDied, who as u32);
            }
        }
        self.bims[who].task = None;
        self.bims[who].queue.clear();
        self.bims[who].character.die();
    }

    pub fn is_alive(&self, who: usize) -> bool {
        self.bims[who].is_alive()
    }

    pub fn health(&self, who: usize) -> f32 {
        self.bims[who].health.points()
    }

    /// 0 when well fed, then 1, 2, 3 for the three stages of malnutrition.
    pub fn malnutrition(&self, who: usize) -> u32 {
        self.bims[who].health.stage().stage()
    }

    /// 0 wide awake, then 1, 2, 3 for sleepy, deprived and wrecked.
    pub fn drowsiness(&self, who: usize) -> u32 {
        self.bims[who].health.drowsiness().stage()
    }

    /// Whether it is actually going somewhere. For the probes: a Bim that is
    /// merely being shoved aside by the other one is not walking.
    #[allow(dead_code)]
    pub fn is_walking(&self, who: usize) -> bool {
        self.bims[who].character.is_walking()
    }

    /// Stand a Bim somewhere, for the probes.
    ///
    /// Nothing in the game does this — a Bim is where it walked to — but a
    /// probe that wants two of them to meet head-on has to be able to set the
    /// meeting up, and waiting for one to happen by chance is not a test.
    /// Snapped to somewhere a body can actually stand, and the spot returned.
    /// "On the deck" is not the same question as "a body fits here": the nav
    /// grid inflates every obstacle by a body's margin, so the strip of deck
    /// along the counter front reads as floor and is not walkable.
    #[allow(dead_code)]
    pub fn put_for_probe(&mut self, who: usize, at: Vec2) -> Vec2 {
        let nav = self.maps.pick(self.room.bath.is_open());
        let spot = nav.nearest_free(at);
        self.bims[who].character.stand_at(spot);
        spot
    }

    /// Send a Bim somewhere by the same route a player order would take, but
    /// without needing it selected or it being the player's. For the probes.
    /// False when there was no route, which leaves the Bim wandering — a probe
    /// that does not check gets a wander and calls it a walk.
    #[allow(dead_code)]
    pub fn send_for_probe(&mut self, who: usize, to: Vec2) -> bool {
        let nav = self.maps.pick(self.room.bath.is_open());
        let route = nav.path(self.bims[who].character.pos, nav.nearest_free(to));
        let got = !route.is_empty();
        self.bims[who].character.follow_path(route);
        got
    }

    /// Stop a Bim wandering off, so a probe can watch one walk and nothing
    /// else. The same switch `r` throws, but for any of the crew.
    #[allow(dead_code)]
    pub fn recruit_for_probe(&mut self, who: usize, on: bool) {
        self.bims[who].character.set_recruited(on);
    }

    /// How much this Bim is slowed by the other being in the same space, for
    /// the probes: 1 clear, less than 1 squeezing past.
    #[allow(dead_code)]
    pub fn crowding_for_probe(&self, who: usize) -> f32 {
        self.crowding(who)
    }

    /// Whether this Bim has the broom in its hands. For the probes.
    #[allow(dead_code)]
    pub fn holds_broom_for_probe(&self, who: usize) -> bool {
        self.bims[who].character.main_held() == Held::Broom
    }

    /// Take an exact amount off one tile, for the probes. `foul_for_probe`
    /// only stages the worst there is, and the interesting question about
    /// what a mess *looks* like is the faint end of the range.
    #[allow(dead_code)]
    pub fn soil_for_probe(&mut self, at: Vec2, cost: f32) {
        self.room.filth.soil(at, cost, filth::Mess::Grime);
    }

    /// Foul one tile of the deck outright, for the probes: staging a mess is
    /// the only way to test the thing that clears one.
    #[allow(dead_code)]
    pub fn foul_for_probe(&mut self, at: Vec2) {
        self.room.filth.sick_on(at);
    }

    /// Run a need down by hand, for the probes.
    #[allow(dead_code)]
    pub fn spend_for_probe(&mut self, who: usize, need: u32, amount: f32) {
        if let Some(need) = Need::from_index(need) {
            self.bims[who].needs.spend(need, amount);
        }
    }

    /// Whether a Bim could walk to a spot. For the probes.
    #[allow(dead_code)]
    pub fn can_reach_for_probe(&self, who: usize, to: Vec2) -> bool {
        self.can_reach(who, to)
    }

    /// Whether a Bim is sitting or lying. For the probes.
    #[allow(dead_code)]
    pub fn is_seated_for_probe(&self, who: usize) -> bool {
        self.bims[who].character.is_seated()
    }

    /// Where the route a Bim is on ends, or `None` when it is not on one. For
    /// the probes: it is how "standing aside is not replanning" is checked.
    #[allow(dead_code)]
    pub fn destination_for_probe(&self, who: usize) -> Option<Vec2> {
        self.bims[who].character.destination()
    }

    /// Whether the Bim has dropped off on its feet this instant.
    pub fn is_napping(&self, who: usize) -> bool {
        self.bims[who].character.is_napping()
    }

    /// Whether it is standing there having lost the thread of what it was on.
    pub fn is_stalled(&self, who: usize) -> bool {
        self.bims[who].task.as_ref().is_some_and(|t| t.stalled())
    }

    /// How many times the errand running now has been fumbled.
    pub fn task_fumbles(&self, who: usize) -> u32 {
        self.bims[who].task.as_ref().map_or(0, |t| t.fumbles())
    }

    pub fn store_veg(&self) -> u32 {
        self.room.veg
    }

    pub fn store_tofu(&self) -> u32 {
        self.room.tofu
    }

    // --- the timetable ----------------------------------------------------

    /// What hour `hour` is set aside for: 0 anything, 1 sleep.
    pub fn schedule_slot(&self, hour: u32) -> u32 {
        match self.schedule.slot(hour) {
            Slot::Anything => 0,
            Slot::Sleep => 1,
        }
    }

    pub fn set_schedule_slot(&mut self, hour: u32, slot: u32) {
        self.schedule.set(
            hour,
            if slot == 1 {
                Slot::Sleep
            } else {
                Slot::Anything
            },
        );
    }

    /// Rested above this, a scheduled sleep is passed over.
    pub fn schedule_ignore_above(&self) -> f32 {
        IGNORE_ABOVE
    }

    /// A need fallen past its trigger sends the Bim to see to it.
    ///
    /// Only when it is otherwise free: a need arising mid-errand waits for
    /// that errand to finish rather than interrupting it, and a walk the
    /// player ordered counts as having something on, so the Bim gets where it
    /// was sent before going off on its own account. Nothing here fires while
    /// the Bim is asleep, because nothing drains while it is asleep.
    fn consider_errand(&mut self, who: usize) {
        // Queued work that the Bim cannot get to does not count as having
        // something on: it would block every need there is while it waited.
        // Nor does a scheduled sleep the Bim is too hungry — or too desperate
        // for the heads — to take yet: that is what lets the errand below be
        // started in its place.
        if self.is_recruited()
            || !self.autonomous
            || self.bims[who].task.is_some()
            || self.queue_ready(who)
            || !self.bims[who].character.arrived()
        {
            return;
        }
        // Most pressing first, but a need it cannot do anything about — an
        // empty fridge, a locked door, a rest that is nobody's business here —
        // must not block the ones it can. An empty cold store would otherwise
        // pin hunger at zero and the Bim would never go to the heads again.
        //
        // These are the needs that are *not* work: there is no row for sleep
        // and none for the heads, because neither is a thing the player gets
        // to put off. Hunger is the odd one — being hungry is a need and
        // cooking is a job — so it is handed to `do_some_work` below and
        // ordered against the rest of the work there.
        for need in self.bims[who].needs.urgent() {
            let started = match need {
                // The timetable is still what sends the Bim to bed on an
                // ordinary day. The rest trigger is the floor under it: a Bim
                // this far gone has either had its night painted out of the
                // schedule or has been kept from taking it, and standing there
                // getting no sleep at all is not an answer. It defers to a
                // meal and to the heads exactly as a scheduled night does —
                // see `eats_first` and `goes_first`.
                Need::Rest => {
                    !self.eats_first(who) && !self.goes_first(who) && self.rest(who, SLEEP_MINUTES)
                }
                // The one thing the priority list is not allowed to do is
                // starve somebody. Once going without has actually begun to
                // tell, a meal jumps the queue whatever the cook row says —
                // the list is a statement about what to do next, not about
                // whether to eat at all.
                Need::Food => self.going_without(who) && self.make_food(who),
                Need::Restroom => self.use_toilet(who),
                // Going and having a word. Not work — there is no row for it
                // in the priority list and no number to put it off with —
                // because the alternative is a player who can set a Bim to
                // die of loneliness by accident.
                Need::Company => self.chat(who),
                // Nothing aboard cleans anything yet. Being filthy is
                // something that happens to the Bim rather than something it
                // can go and see to, so it starts no errand — what it does
                // instead is in `mind_the_mess` and `flee_filth`.
                Need::Cleanliness => false,
            };
            if started {
                return;
            }
        }

        // Bedtime is waiting and the Bim would rather go first. This is the
        // other half of the rule in `goes_first`: that one holds the sleep
        // back, and without this nothing would ever start the trip it is being
        // held for — the need is nowhere near its trigger, so the loop above
        // passed straight over it and the Bim would stand there all night.
        //
        // A bed the rest trigger is asking for counts the same as a queued
        // one. It has to: the loop above has just refused to start that sleep
        // for this very reason, so without this the two rules would deadlock
        // and the Bim would neither go nor turn in.
        if (self.bed_is_queued(who) || self.wants_bed(who))
            && self.goes_first(who)
            && self.use_toilet(who)
        {
            return;
        }

        // Nothing pressing on the body. What is left is work, and the player
        // says what order that gets done in.
        if self.do_some_work(who) {
            return;
        }

        // Nothing could be started — and a shut door may be the whole of the
        // reason. A Bim in the heads is on the wrong side of it for the
        // galley, so every meal reads as unreachable and hunger sits at
        // nothing while the Bim stands there: exactly what happens when a
        // player locks it in and then unlocks the door again, since unlocking
        // leaves the door shut. The panel is on this side of it, so letting
        // itself out is the errand; the need comes round again with the way
        // clear as soon as the door is open.
        if self.shut_in(who) {
            self.send_to_switch(who, Switch::BathDoor(true));
        }
    }

    /// Whatever work is going, in the order the player asked for.
    ///
    /// Everything here is discretionary: a tray is only asking if the bay is
    /// running, the deck only wants sweeping if something is on it, and
    /// cooking is only offered to a Bim that is actually hungry. Whichever of
    /// those is going is collected, sorted, and tried in turn — the first one
    /// that can actually be started wins, so a galley the other Bim is
    /// standing in does not stop this one getting on with the bay.
    ///
    /// The sort is **stable**, and that is what makes an untouched list behave
    /// exactly as the fixed order did before there was a list: all equal, the
    /// jobs come out in the order they were offered, which is the order they
    /// used to be written in.
    fn do_some_work(&mut self, who: usize) -> bool {
        for job in self.work_on_offer(who) {
            let started = match job {
                Job::Cook => self.make_food(who),
                Job::Plant | Job::Cut => self.tend_bay(who),
                Job::Clean => self.sweep_up(who),
                // Never offered on its own: see `waits_on`.
                Job::Haul => false,
            };
            if started {
                return true;
            }
        }
        false
    }

    /// What work there is for `who` right now, most important first.
    ///
    /// Kept apart from starting any of it because this is the half the player
    /// can actually see the effect of, and the half worth checking: whether a
    /// job is *begun* also depends on the galley being free and the broom
    /// being in its locker, and those have nothing to do with the list.
    fn work_on_offer(&self, who: usize) -> Vec<Job> {
        let mut offered: Vec<Job> = Vec::new();
        // Hunger, if the need is past its trigger. Asked of `urgent` rather
        // than of the level so that switching the food trigger off switches
        // the cooking off with it, exactly as it did when this lived in the
        // loop above.
        if self.bims[who].needs.urgent().contains(&Need::Food) {
            offered.push(Job::Cook);
        }
        // What the bay wants decides which of the two bay rows applies. Asked
        // now rather than assumed: a tray with something ripe in it is a
        // cutting and an empty one is a planting, and the player may well
        // have set those a long way apart.
        if let Some(job) = self.room.bay.wants_work(self.room.veg, self.room.tofu) {
            offered.push(match job {
                hydro::Job::Harvest(_) => Job::Cut,
                hydro::Job::Plant(_, _) => Job::Plant,
            });
        }
        if self.room.filth.dirty_tiles() > 0 {
            offered.push(Job::Clean);
        }
        offered.sort_by_key(|&job| self.waits_on(job));
        offered
    }

    /// The number a job actually waits on.
    ///
    /// Only cutting is not simply its own row. A crop lifted out of a tray is
    /// in the Bim's hands until it has been carried to the cold store — that
    /// is one chain, not two — so a cutting is a cut *and* a haul, and it
    /// waits on whichever of the two the player has set later. Set hauling to
    /// the bottom and the bay stops being emptied, which is the truthful
    /// answer: there is nobody to carry it.
    fn waits_on(&self, job: Job) -> u32 {
        match job {
            Job::Cut => self
                .priorities
                .of(Job::Cut)
                .max(self.priorities.of(Job::Haul)),
            other => self.priorities.of(other),
        }
    }

    /// Whether going without food has begun to do this Bim damage, as against
    /// merely being hungry. The one thing no priority overrides.
    fn going_without(&self, who: usize) -> bool {
        self.bims[who].health.stage() >= Malnutrition::Mild
    }

    /// Run this Bim's solitude clock and apply whatever came of it.
    ///
    /// All of it is `social.rs`'s arithmetic; what lives here is everything
    /// that needs to know about the world — where the Bim is standing, what it
    /// is in the middle of, and that the diary exists.
    fn bear_the_solitude(&mut self, who: usize, minutes: f32) {
        // A Bim already sitting at the table, in its bunk or on the pan
        // cannot sink to the deck: it is where a chain put it, and putting it
        // somewhere else would drop it through the furniture.
        let can_sit_down = !self.bims[who].character.is_seated();
        let fallout = {
            let (bims, rng) = (&mut self.bims, &mut self.rng);
            bims[who].solitude.update(minutes, can_sit_down, rng)
        };

        if fallout.brooded {
            let days = (self.bims[who].solitude.alone_for() / clock::DAY) as u32;
            self.remember(who, What::FeltLow, days);
        }
        if fallout.broke_down {
            self.bims[who].sad_left = social::SITS_FOR;
            let (at, facing) = (
                self.bims[who].character.pos,
                self.bims[who].character.heading,
            );
            self.bims[who].character.sit(at, facing);
            self.bims[who].character.set_action(Action::None);
            self.remember(who, What::BrokeDown, 0);
        }
        if fallout.hurt_itself {
            self.bims[who].health.hurt(social::SELF_HARM);
            self.remember(who, What::HurtSelf, social::SELF_HARM as u32);
        }
        if fallout.gave_up {
            self.bims[who].health.give_up();
        }
    }

    /// Go and have a word with the other one.
    ///
    /// The only errand aboard that takes two Bims, and the only one started
    /// for somebody else as well as for oneself: both crew are handed a chain
    /// in the same frame, each walking to **its own** spot either side of a
    /// meeting point worked out here.
    ///
    /// That is not a flourish. A route is planned once and never replanned, so
    /// a Bim sent to *where the other one is* would be walking at a target
    /// that is itself walking, converge on empty deck, and stand there with
    /// `arrived()` never coming true. Both walking to fixed points is the only
    /// shape of this that terminates.
    fn chat(&mut self, who: usize) -> bool {
        let Some(other) = self.free_to_talk(who) else {
            return false;
        };
        let (here, there) = (self.bims[who].character.pos, self.bims[other].character.pos);
        let nav = self.maps.pick(self.room.bath.is_open());
        // Side by side, always, rather than either side of the line they
        // happened to approach along. Two Bims who met walking north-south
        // would stand one above the other, and the lower one's name — which
        // the host paints a body's height above its head — lands squarely on
        // the upper one. Left and right, and nothing written over anything.
        // Whoever is further left stays on the left, so they do not cross.
        let middle = (here + there) * 0.5;
        let step = vec2(TALKING_GAP * 0.5, 0.0);
        let (mine, theirs) = if here.x <= there.x {
            (middle - step, middle + step)
        } else {
            (middle + step, middle - step)
        };
        let (mine, theirs) = (nav.nearest_free(mine), nav.nearest_free(theirs));
        // Both have to be able to get there, or one of them stands about while
        // the other talks to the bulkhead.
        if nav.path(here, mine).is_empty() || nav.path(there, theirs).is_empty() {
            return false;
        }

        if !self.take_over(who, Kind::Chat, 0.0) {
            return false;
        }
        self.interrupt(other);
        // What each of them has to say is picked now, from its own diary, and
        // written down now: by the time the chain ends the topic is gone, and
        // a chat cut short should still be a chat that happened.
        for (bim, at, toward) in [(who, mine, theirs), (other, theirs, mine)] {
            let topic = self.something_to_say(bim);
            self.bims[bim].chat_topic = topic;
            let facing = (toward - at).angle();
            let (bims, room, maps) = (&mut self.bims, &mut self.room, &self.maps);
            bims[bim].task = Some(Task::chat(
                bim,
                at,
                facing,
                &mut bims[bim].character,
                room,
                maps,
            ));
            self.bims[bim].solitude.talked();
        }
        true
    }

    /// The other one, if it is in a state to be talked to.
    ///
    /// Dead, asleep, shut in the heads or sitting on the deck in despair all
    /// mean no. Work does not: a Bim sweeping or at the bay is interrupted,
    /// because the alternative is two Bims who are never both free at the same
    /// moment and therefore never speak — and with the deck always finding
    /// something to be swept, that is not a hypothetical.
    fn free_to_talk(&self, who: usize) -> Option<usize> {
        (0..CREW).find(|&other| {
            other != who
                && self.bims[other].is_alive()
                && self.bims[other].sad_left <= 0.0
                && !self.bims[other].character.is_napping()
                && !self
                    .room
                    .bath
                    .shell
                    .contains(self.bims[other].character.pos)
                && self.bims[other]
                    .task
                    .as_ref()
                    .is_none_or(|t| matches!(t.kind(), Kind::Clean | Kind::Tend(_) | Kind::Chat))
        })
    }

    /// Something for `who` to talk about: one of the things it has been doing,
    /// picked at random out of its recent diary. 0 when it has nothing to
    /// report, which the host renders as talking about nothing much.
    fn something_to_say(&mut self, who: usize) -> u32 {
        let held = self.bims[who].memory.len();
        if held == 0 {
            return 0;
        }
        // Out of the last stretch of it rather than the whole book: what they
        // talk about should be the day they have had, not something from the
        // week before last.
        let recent = held.min(RECENT_ENOUGH);
        let pick = held - recent + self.rng.below(recent as u32) as usize;
        self.bims[who]
            .memory
            .at(pick)
            .map_or(0, |moment| moment.what.code())
    }

    /// Start on whichever tray of the bay wants a hand, if any.
    ///
    /// One errand per tray rather than one for the whole bay: an interruption
    /// then costs a tray and not the afternoon, and the Bim walks along the
    /// front of the bay tray by tray the way it would.
    fn tend_bay(&mut self, who: usize) -> bool {
        let Some(job) = self.room.bay.wants_work(self.room.veg, self.room.tofu) else {
            return false;
        };
        let kind = Kind::Tend(job.spot());
        if !self.can_begin(who, kind) || !self.take_over(who, kind, 0.0) {
            return false;
        }
        self.bims[who].task = Some(Task::tend(
            who,
            job.spot(),
            &mut self.bims[who].character,
            &mut self.room,
            &self.maps,
        ));
        true
    }

    /// Get the broom out, if the deck wants it.
    ///
    /// The last thing a Bim considers and the first thing it drops. Sweeping
    /// is what it does when there is genuinely nothing else — no need past its
    /// threshold, no night due, no tray asking — so it sits at the bottom of
    /// `consider_errand`, below the bay and above the wander. Anything that
    /// comes up interrupts it exactly like any other errand, and the half-swept
    /// deck waits on the queue.
    pub fn sweep_up(&mut self, who: usize) -> bool {
        if self.room.filth.dirty_tiles() == 0 {
            return false;
        }
        if !self.can_begin(who, Kind::Clean) || !self.take_over(who, Kind::Clean, 0.0) {
            return false;
        }
        let (bims, room, maps) = (&mut self.bims, &mut self.room, &self.maps);
        bims[who].task = Some(Task::clean(who, &mut bims[who].character, room, maps));
        true
    }

    /// How many tiles are dirty enough to be worth the broom. The host greys
    /// the menu item out with it, and says how much there is to do.
    pub fn dirty_tiles(&self) -> u32 {
        self.room.filth.dirty_tiles()
    }

    // --- the bay and the manager ------------------------------------------

    pub fn hydro_spots(&self) -> u32 {
        hydro::SPOTS as u32
    }

    /// What is in tray `i`: 0 empty, 1 greens, 2 soy.
    pub fn hydro_crop(&self, i: u32) -> u32 {
        self.room.bay.crop_at(i as usize)
    }

    /// How far along tray `i` is, 0 to 1.
    pub fn hydro_growth(&self, i: u32) -> f32 {
        self.room.bay.growth_at(i as usize)
    }

    pub fn hydro_ripe(&self) -> u32 {
        self.room.bay.ripe_count()
    }

    pub fn hydro_automated(&self) -> bool {
        self.room.bay.automated()
    }

    /// Follow the manager's target, or stop. A setting rather than an errand:
    /// the Bim does the planting, but deciding *whether* to is the player's,
    /// the same as the timetable or letting the Bim decide for itself.
    pub fn set_hydro_automated(&mut self, on: bool) {
        self.room.bay.set_automated(on);
    }

    /// The standing order: 0 none, 1 greens everywhere, 2 soy everywhere.
    pub fn hydro_forced(&self) -> u32 {
        self.room.bay.forced().map_or(0, |c| c.code())
    }

    pub fn set_hydro_forced(&mut self, code: u32) {
        self.room.bay.force(hydro::Crop::from_code(code));
    }

    pub fn hydro_hibernating(&self) -> bool {
        self.room.bay.hibernating()
    }

    pub fn food_target(&self) -> u32 {
        self.manager.food_units()
    }

    pub fn set_food_target(&mut self, units: u32) {
        self.manager.set_food_units(units);
    }

    pub fn target_veg(&self) -> u32 {
        self.manager.veg()
    }

    pub fn target_tofu(&self) -> u32 {
        self.manager.tofu()
    }

    /// Whether a shut — but not locked — door is the only thing between the
    /// Bim and something it is waiting to get on with.
    ///
    /// Asked of whatever is at the front of the queue as well as of the needs,
    /// because queued work whose way is shut waits rather than being thrown
    /// away, and would wait for ever.
    fn shut_in(&self, who: usize) -> bool {
        if self.room.bath.is_open() || self.room.bath.locked {
            return false;
        }
        let queued = self.bims[who]
            .queue
            .first()
            .and_then(|saved| saved.resume_station(&self.room, self.bims[who].character.pos));
        let wanted = self.bims[who]
            .needs
            .urgent()
            .into_iter()
            .filter_map(|need| self.need_station(who, need));
        queued
            .into_iter()
            .chain(wanted)
            .any(|to| !self.can_reach(who, to) && self.can_reach_through_door(who, to))
    }

    /// Where a need would first send the Bim, for the needs it could actually
    /// see to. An empty cold store is not a door problem, so hunger with
    /// nothing to cook asks for no door to be opened — otherwise the Bim
    /// would work the panel over and over for a meal it can never start.
    fn need_station(&self, who: usize, need: Need) -> Option<Vec2> {
        let kind = match need {
            // A pot with something in it is the first place it would go, and
            // it needs nothing out of the store to do it.
            Need::Food if self.room.pot_servings > 0 => Kind::Leftovers,
            // Either recipe starts at the fridge, so which one it would pick
            // makes no difference to where it has to be able to walk.
            Need::Food if self.room.has_ingredients() => Kind::Meal(Dish::Stew),
            // The heads are behind the door, and that chain opens it itself;
            // rest is the timetable's business and never starts a need errand.
            _ => return None,
        };
        task::first_station(who, kind, &self.room, self.bims[who].character.pos)
    }

    /// Whether `to` is somewhere the Bim could walk if the door were open.
    fn can_reach_through_door(&self, who: usize, to: Vec2) -> bool {
        let nav = self.maps.pick(true);
        !nav.path(self.bims[who].character.pos, nav.nearest_free(to))
            .is_empty()
    }

    // --- company, and going without it --------------------------------------

    /// 0 for a Bim with company, then 1, 2, 3 for the three stages of going
    /// without it. The host names them.
    pub fn loneliness(&self, who: usize) -> u32 {
        self.bims[who].solitude.stage().stage()
    }

    /// Whole days since it last spoke to anybody. What the stages are built
    /// on, so the readout and the behaviour cannot drift apart.
    pub fn days_alone(&self, who: usize) -> f32 {
        self.bims[who].solitude.alone_for() / clock::DAY
    }

    /// What `who` is saying this instant, as a `memory::What` code, or 0 for
    /// nothing at all.
    ///
    /// **Only one of them talks at a time.** Both hold a topic for the length
    /// of the conversation, and whose turn it is comes off the ship's clock
    /// rather than off either chain's own elapsed time — they arrive a moment
    /// apart, and two bubbles over two Bims a body's width apart would sit on
    /// top of each other.
    pub fn chat_topic(&self, who: usize) -> u32 {
        let talking = self.bims[who]
            .task
            .as_ref()
            .is_some_and(|t| t.step() == task::Step::Talk);
        if !talking {
            return 0;
        }
        let turn = (self.clock.minutes() / TAKES_A_TURN) as u32 as usize;
        if turn % CREW == who {
            self.bims[who].chat_topic
        } else {
            0
        }
    }

    /// Wind a Bim's solitude clock forward by hand, for the probes: watching
    /// one reach the far end of this in real time is ten game days of frames,
    /// and what is worth checking is what happens once it is there.
    #[allow(dead_code)]
    pub fn leave_alone_for_probe(&mut self, who: usize, days: f32) {
        self.bims[who]
            .solitude
            .set_alone_for(days * crate::clock::DAY);
    }

    /// Whether `who` is sitting on the deck having given up for a while.
    #[allow(dead_code)]
    pub fn broken_down_for_probe(&self, who: usize) -> bool {
        self.bims[who].sad_left > 0.0
    }

    // --- the order the work gets done in ------------------------------------
    //
    // One list for the ship, so none of these takes a `who`. The host holds
    // the names; everything that crosses here is a code and a number.

    pub fn work_count(&self) -> u32 {
        Job::ALL.len() as u32
    }

    /// The range the boxes cycle through. Read rather than repeated, so the
    /// host's colour scale cannot come to disagree with what a click does.
    pub fn work_highest(&self) -> u32 {
        work::HIGHEST
    }

    pub fn work_lowest(&self) -> u32 {
        work::LOWEST
    }

    pub fn work_priority(&self, job: u32) -> u32 {
        Job::from_code(job).map_or(work::DEFAULT, |job| self.priorities.of(job))
    }

    /// Set one outright. Nothing in the host does — a box is clicked, not
    /// typed into — so this is the probes' way in.
    #[allow(dead_code)]
    pub fn set_work_priority(&mut self, job: u32, level: u32) {
        if let Some(job) = Job::from_code(job) {
            self.priorities.set(job, level);
        }
    }

    /// The jobs on offer to `who` right now, most important first, as codes.
    /// For the probes: what the list actually decides, without the galley
    /// being busy or the broom being out confusing the reading.
    #[allow(dead_code)]
    pub fn work_on_offer_for_probe(&self, who: usize) -> Vec<u32> {
        self.work_on_offer(who).iter().map(|j| j.code()).collect()
    }

    /// What a job actually waits on once the chains it is part of are taken
    /// into account, for the probes. Only cutting differs from its own row —
    /// see [`Game::waits_on`] — and staging a ripe tray to watch it happen
    /// takes most of a day, so the rule is checked here directly.
    #[allow(dead_code)]
    pub fn work_waits_on_for_probe(&self, job: u32) -> u32 {
        Job::from_code(job).map_or(work::DEFAULT, |job| self.waits_on(job))
    }

    /// One click on a box: a step less important, and round to the top from
    /// the bottom. The cycling is the simulation's so the range has one home.
    /// Gives back what it now reads, so the host paints what actually landed
    /// rather than what it expected.
    pub fn cycle_work_priority(&mut self, job: u32) -> u32 {
        match Job::from_code(job) {
            Some(job) => self.priorities.cycle(job),
            None => work::DEFAULT,
        }
    }

    // --- what the Bim wants ----------------------------------------------

    /// How full need `i` is, 0 to 1, in the order `Need::ALL` lists them.
    pub fn need_level(&self, who: usize, i: u32) -> f32 {
        Need::from_index(i).map_or(0.0, |need| self.bims[who].needs.level(need))
    }

    pub fn need_count(&self) -> u32 {
        Need::ALL.len() as u32
    }

    /// The level at which need `i` sends the Bim to see to it, and whether it
    /// does so at all. The host reads both rather than repeating them, so the
    /// bar cannot disagree with the behaviour it is meant to be predicting.
    /// The action thresholds are the ship's, not a Bim's: one setting, applied
    /// to the whole crew, the same way the timetable is. So they are written
    /// to every Bim and read back off whichever one — they cannot differ.
    pub fn need_trigger(&self, i: u32) -> f32 {
        Need::from_index(i).map_or(0.0, |need| self.bims[PLAYER].needs.trigger(need).at)
    }

    pub fn need_trigger_on(&self, i: u32) -> bool {
        Need::from_index(i).is_some_and(|need| self.bims[PLAYER].needs.trigger(need).on)
    }

    pub fn set_need_trigger(&mut self, i: u32, at: f32) {
        let Some(need) = Need::from_index(i) else {
            return;
        };
        for bim in &mut self.bims {
            bim.needs.set_trigger_at(need, at);
        }
    }

    pub fn set_need_trigger_on(&mut self, i: u32, on: bool) {
        let Some(need) = Need::from_index(i) else {
            return;
        };
        for bim in &mut self.bims {
            bim.needs.set_trigger_on(need, on);
        }
    }

    /// How badly it needs the heads, 0 to 3. The host names them.
    pub fn urge(&self, who: usize) -> u32 {
        self.bims[who].needs.urge().stage()
    }

    /// How far gone it is for want of somewhere clean, 0 to 3.
    ///
    /// Per Bim, not per deck. The mess is shared; how long *this* one has been
    /// standing in it is not, and a Bim that has just walked in from the heads
    /// is not as far gone as the one that has been beside it for two hours.
    pub fn discomfort(&self, who: usize) -> u32 {
        self.bims[who].ordeal.discomfort().stage()
    }

    pub fn bim_filth(&self, who: usize) -> f32 {
        self.bims[who].character.filth()
    }

    /// What the tile under a point scores, [`filth::FOULED`] to
    /// [`filth::BASELINE`]. For the probes: the host has `deck_filth` for the
    /// one number it needs.
    #[allow(dead_code)]
    pub fn tile_filth(&self, at: Vec2) -> f32 {
        self.room.filth.at(at)
    }

    /// How much of the deck has something on it, 0 to 1 — one number for a
    /// state that is really two hundred, so the host can say "the place is a
    /// tip" without reading every tile.
    pub fn deck_filth(&self) -> f32 {
        self.room.filth.dirty_share()
    }

    // --- naming what the pointer is over ----------------------------------
    //
    // Three numbers rather than a string, the same as everything else across
    // the boundary: what the thing is, what is on it, and how much. The host
    // does the wording.

    /// What is at a point, in the `SPOT_` codes from `room.rs`.
    pub fn spot_at(&self, x: f32, y: f32) -> u32 {
        self.room.spot(vec2(x, y))
    }

    /// What kind of mess is on the tile under a point: 0 none, then grime,
    /// wetting, soiling, sick. The deck grid runs under the furniture as well
    /// as the open floor, so the host only asks this where it makes sense to.
    pub fn spot_mess(&self, x: f32, y: f32) -> u32 {
        self.room.filth.kind_at(vec2(x, y)).code()
    }

    /// How far down that tile has been taken, 0 clean to 1 fouled.
    pub fn spot_mess_depth(&self, x: f32, y: f32) -> f32 {
        self.room.filth.depth_at(vec2(x, y))
    }

    /// Ring a fixture on the deck, by its `SPOT_` code, or `SPOT_NOTHING` to
    /// ring nothing. A code with no single place behind it — deck, bulkhead —
    /// rings nothing too, rather than the whole room.
    pub fn set_highlight(&mut self, spot: u32) {
        self.highlight = spot;
    }

    pub fn set_autonomous(&mut self, on: bool) {
        self.autonomous = on;
    }

    // --- under direct orders ----------------------------------------------

    /// Recruit the Bim, or let it go again.
    ///
    /// Recruited, it does nothing of its own accord: no errand from a need, no
    /// sleep from the timetable, nothing picked back up off the queue, and no
    /// pottering about between jobs. What it will still do is everything the
    /// player asks of it — and everything going without does to it, since the
    /// needs carry on draining and every stage of hunger and drowsiness bites
    /// exactly as before.
    ///
    /// Whatever it is in the middle of is left to finish. Cancelling would
    /// throw away a half-cooked meal for the sake of tidiness, and a right
    /// click interrupts it anyway.
    pub fn toggle_recruited(&mut self) {
        let now = !self.bims[PLAYER].character.is_recruited();
        self.bims[PLAYER].character.set_recruited(now);
        if now {
            // It is the thing being ordered about, so it is the thing selected.
            self.bims[PLAYER].character.selected = true;
        }
    }

    pub fn is_recruited(&self) -> bool {
        self.bims[PLAYER].character.is_recruited()
    }

    pub fn is_autonomous(&self) -> bool {
        self.autonomous
    }

    // --- player input ---------------------------------------------------

    /// Select by control group. There is one Bim, so group 1 is all of them.
    pub fn select_group(&mut self, group: u32) {
        if group == 1 {
            self.select_only(Some(PLAYER));
        }
    }

    pub fn clear_selection(&mut self) {
        self.select_only(None);
    }

    /// Exactly one of the crew is selected, or none.
    ///
    /// Selecting is *looking at*, not taking charge of: any of them can be
    /// picked, and picking one is what puts its panels on screen. Whether it
    /// takes orders is a separate question, and the answer is always the same
    /// one — see `order_move`, which asks after [`PLAYER`] and nobody else.
    ///
    /// One at a time because the panels show one at a time. A marquee over
    /// both has to mean something, and "whichever you can actually order
    /// about" is the least surprising thing it can mean.
    fn select_only(&mut self, who: Option<usize>) {
        for (i, bim) in self.bims.iter_mut().enumerate() {
            bim.character.selected = who == Some(i);
        }
    }

    /// Which of them is selected, or `None`.
    pub fn selected(&self) -> Option<usize> {
        self.bims.iter().position(|b| b.character.selected)
    }

    pub fn is_selected(&self, who: usize) -> bool {
        self.bims[who].character.selected
    }

    pub fn bim_pos(&self, who: usize) -> Vec2 {
        self.bims[who].character.pos
    }

    pub fn selected_count(&self) -> u32 {
        self.selected().is_some() as u32
    }

    pub fn drag_begin(&mut self, x: f32, y: f32) {
        let p = vec2(x, y);
        self.drag = Some((p, p));
    }

    pub fn drag_update(&mut self, x: f32, y: f32) {
        if let Some((_, end)) = &mut self.drag {
            *end = vec2(x, y);
        }
    }

    /// Finish a marquee. A plain click is just a box of zero size, so the same
    /// hit test covers picking and dragging; missing entirely deselects.
    ///
    /// Returns the fixture under a click, if any, so the host can open a menu
    /// on it. Clicking a fixture leaves the selection alone — opening the
    /// fridge should not deselect the Bim you were about to give a job to.
    pub fn drag_end(&mut self, x: f32, y: f32) -> u32 {
        self.drag_update(x, y);
        let Some((start, end)) = self.drag.take() else {
            return HIT_NONE;
        };
        let box_ = Rect::from_corners(start, end);

        // Only a click, not a sweep, counts as poking at the furniture.
        if box_.width() < 4.0 && box_.height() < 4.0 {
            let hit = self.room.hit(box_.center());
            if hit != HIT_NONE {
                return hit;
            }
        }

        // Whoever the box touched. The player's own comes first when it
        // caught both: a sweep across the compartment most likely meant the
        // one that can be told to do something.
        let touched: Vec<usize> = (0..self.bims.len())
            .filter(|&i| {
                box_.touches_circle(
                    self.bims[i].character.pos,
                    self.bims[i].character.pick_radius(),
                )
            })
            .collect();
        let pick = touched
            .iter()
            .copied()
            .find(|&i| i == PLAYER)
            .or_else(|| touched.first().copied());
        self.select_only(pick);
        HIT_NONE
    }

    pub fn drag_cancel(&mut self) {
        self.drag = None;
    }

    /// Right-click on the floor: send whatever is selected to that spot. A task
    /// in progress outranks the player, so orders are ignored while cooking.
    /// Send the selection to a spot, opening a door on the way if that is what
    /// it takes. Says what became of the order; see the `ORDER_` codes.
    ///
    /// A shut door is a wall to the pathfinder, so the route is worked out
    /// twice: once with the door as it stands, and — if that comes back with
    /// nothing — once with it open. A destination only the second finds is one
    /// the Bim can reach by letting itself through, which it goes and does.
    pub fn order_move(&mut self, x: f32, y: f32) -> u32 {
        if !self.bims[PLAYER].character.selected || !self.is_alive(PLAYER) {
            return ORDER_IGNORED;
        }
        let want = vec2(x, y);

        // Snap to somewhere the Bim can actually stand, so an order onto the
        // table means "the floor beside the table" rather than nothing at all.
        let nav = self.maps.pick(self.room.bath.is_open());
        let target = nav.nearest_free(want);
        let route = nav.path(self.bims[PLAYER].character.pos, target);
        if !route.is_empty() {
            // The order outranks whatever the Bim is on. That chain goes into
            // the queue and is picked up once it has been where it was sent.
            self.interrupt_for_order(PLAYER);
            self.bims[PLAYER].character.follow_path(route);
            self.mark(target, false);
            return ORDER_MOVING;
        }

        // Nothing as things stand. Would there be with the door open?
        let open = self.maps.pick(true);
        let through = open.nearest_free(want);
        if open
            .path(self.bims[PLAYER].character.pos, through)
            .is_empty()
        {
            self.mark(through, true);
            return ORDER_NOWHERE;
        }

        // A Bim part-way through a trip to the heads has locked the door
        // behind itself, and letting go of that errand unlocks it again. So
        // that is not being locked out: it is the Bim's own door, and the
        // order is what opens it. Any other locked door genuinely blocks, and
        // nothing is attempted — not even dropping what it was doing.
        let own_lock = self.bims[PLAYER]
            .task
            .as_ref()
            .is_some_and(|task| task.kind() == Kind::Heads);
        if self.room.bath.locked && !own_lock {
            self.mark(through, true);
            return ORDER_LOCKED;
        }

        // Already on its way to the panel: the new order simply replaces the
        // one that was waiting on the door, and the Bim carries on to it.
        // Interrupting here would put that errand on the queue and start a
        // second one exactly like it — click a few times and the agenda fills
        // with door errands that all open the same door.
        if self.on_a_door_errand(PLAYER) {
            self.bims[PLAYER].pending_move = Some(want);
            self.mark(through, false);
            return ORDER_VIA_DOOR;
        }

        self.interrupt_for_order(PLAYER);

        // Letting go may have opened the door by itself, so look again before
        // sending the Bim off to work a panel it no longer needs to touch.
        let nav = self.maps.pick(self.room.bath.is_open());
        let target = nav.nearest_free(want);
        let route = nav.path(self.bims[PLAYER].character.pos, target);
        if !route.is_empty() {
            self.bims[PLAYER].character.follow_path(route);
            self.mark(target, false);
            return ORDER_MOVING;
        }

        // Go and open it, then carry on to where it was sent.
        self.bims[PLAYER].pending_move = Some(want);
        self.bims[PLAYER].task = Some(Task::work_switch(
            PLAYER,
            Switch::BathDoor(true),
            &mut self.bims[PLAYER].character,
            &mut self.room,
            &self.maps,
        ));
        self.mark(through, false);
        ORDER_VIA_DOOR
    }

    fn mark(&mut self, pos: Vec2, bad: bool) {
        self.markers.push(Marker { pos, age: 0.0, bad });
    }

    /// Once the door is open, take up the order that was waiting on it. This
    /// goes before the queue: the player asked for it now, not after whatever
    /// the door errand displaced.
    fn take_pending_move(&mut self, who: usize) {
        if self.bims[who].task.is_some() || !self.bims[who].character.arrived() {
            return;
        }
        let Some(want) = self.bims[who].pending_move.take() else {
            return;
        };
        let nav = self.maps.pick(self.room.bath.is_open());
        let target = nav.nearest_free(want);
        let route = nav.path(self.bims[who].character.pos, target);
        if route.is_empty() {
            // The door shut again, or was locked while the Bim walked to it.
            self.mark(target, true);
            return;
        }
        self.bims[who].character.follow_path(route);
        self.mark(target, false);
    }

    // --- the clock --------------------------------------------------------

    /// Minutes since midnight. The host formats the reading; no strings cross
    /// the boundary.
    pub fn clock_minutes(&self) -> f32 {
        self.clock.minutes()
    }

    pub fn clock_day(&self) -> u32 {
        self.clock.day()
    }

    // --- who they are -------------------------------------------------------
    //
    // The ship's calendar, and a birthday apiece. The host formats both; all
    // that crosses is numbers, so "12 April 2367" is assembled from a year, a
    // month and a date on the other side.

    pub fn clock_year(&self) -> u32 {
        self.clock.year()
    }

    pub fn clock_month(&self) -> u32 {
        crate::clock::month_and_date(self.clock.day_of_year()).0
    }

    pub fn clock_date(&self) -> u32 {
        crate::clock::month_and_date(self.clock.day_of_year()).1
    }

    pub fn born_year(&self, who: usize) -> u32 {
        self.bims[who].born_year
    }

    pub fn born_month(&self, who: usize) -> u32 {
        crate::clock::month_and_date(self.bims[who].born_day).0
    }

    pub fn born_date(&self, who: usize) -> u32 {
        crate::clock::month_and_date(self.bims[who].born_day).1
    }

    /// How old it is today, in whole years.
    pub fn age(&self, who: usize) -> u32 {
        self.bims[who].age(self.clock.year(), self.clock.day_of_year())
    }

    // --- what they remember -------------------------------------------------

    pub fn memory_len(&self, who: usize) -> u32 {
        self.bims[who].memory.len() as u32
    }

    /// One line of the diary, oldest first. Four numbers and no words: what
    /// day, what time, what happened, and the one detail that goes with it.
    pub fn memory_day(&self, who: usize, i: u32) -> u32 {
        self.bims[who]
            .memory
            .at(i as usize)
            .map_or(0, |moment| moment.day)
    }

    pub fn memory_at(&self, who: usize, i: u32) -> f32 {
        self.bims[who]
            .memory
            .at(i as usize)
            .map_or(0.0, |moment| moment.at)
    }

    pub fn memory_what(&self, who: usize, i: u32) -> u32 {
        self.bims[who]
            .memory
            .at(i as usize)
            .map_or(0, |moment| moment.what.code())
    }

    pub fn memory_detail(&self, who: usize, i: u32) -> u32 {
        self.bims[who]
            .memory
            .at(i as usize)
            .map_or(0, |moment| moment.detail)
    }

    /// Game minutes of sleep the Bim still has ahead of it, or zero.
    pub fn rest_left(&self, who: usize) -> f32 {
        match &self.bims[who].task {
            None => 0.0,
            Some(t) => t.rest_left(),
        }
    }

    // --- fixtures ---------------------------------------------------------

    /// Which fixture is at a point, without disturbing the selection. The
    /// host asks this on a right-click, so poking at the furniture and
    /// ordering the Bim about can share one button.
    pub fn hit_at(&self, x: f32, y: f32) -> u32 {
        self.room.hit(vec2(x, y))
    }

    pub fn fridge_is_open(&self) -> bool {
        self.room.fridge_is_open()
    }

    pub fn stove_is_on(&self) -> bool {
        self.room.stove_on
    }

    /// Game minutes before the hob turns itself off, or zero when it is not
    /// counting down at all.
    pub fn stove_idle_left(&self) -> f32 {
        self.room.stove_idle_left()
    }

    /// True while a task has the Bim, so the host can grey out "Make food".
    pub fn is_busy(&self, who: usize) -> bool {
        self.bims[who].task.is_some()
    }

    /// Send the Bim to work a switch. Nothing aboard changes without it: the
    /// state only moves when the Bim's hand gets there, and this displaces
    /// whatever it was doing exactly like any other errand.
    fn send_to_switch(&mut self, who: usize, which: Switch) {
        if !self.can_begin(who, Kind::Switch(which))
            || !self.take_over(who, Kind::Switch(which), 0.0)
        {
            return;
        }
        self.bims[who].task = Some(Task::work_switch(
            who,
            which,
            &mut self.bims[who].character,
            &mut self.room,
            &self.maps,
        ));
    }

    pub fn toggle_fridge(&mut self, who: usize) {
        self.send_to_switch(who, Switch::FridgeDoor);
    }

    /// Flipping the hob is a physical act: the Bim walks over and does it,
    /// rather than the switch moving by itself.
    pub fn toggle_stove(&mut self, who: usize) {
        self.send_to_switch(who, Switch::Hob);
    }

    /// Kick off the make-a-meal chain. Ignored if one is already running.
    /// Cook whichever the Bim fancies. Both recipes cost two out of the cold
    /// store, so with fewer than that left there is nothing to be done.
    pub fn make_food(&mut self, who: usize) -> bool {
        // Already on a meal: asking again is not a second dinner. `cook` turns
        // down the *same* dish by itself, but this one picks at random and
        // would otherwise displace a half-chopped stew with a bowl.
        if self.bims[who]
            .task
            .as_ref()
            .is_some_and(|task| matches!(task.kind(), Kind::Meal(_) | Kind::Leftovers))
        {
            return false;
        }

        // There is a pot on the hob with something in it. Cooking a second one
        // on top of that would throw the first away, and the whole point of a
        // pot holding two is that the Bim comes back to it.
        if self.eat_leftovers(who) {
            return true;
        }
        // Half and half, which over two meals a day comes out at about one
        // bowl a day — the tofu the Bim wants daily. If the store cannot run
        // to what it fancies it has the other, which is what keeps a Bim with
        // plenty of greens and no soy eating at all.
        let (first, second) = if self.rng.chance(0.5) {
            (Dish::Stew, Dish::Bowl)
        } else {
            (Dish::Bowl, Dish::Stew)
        };
        self.cook(who, first) || self.cook(who, second)
    }

    /// Helpings left in the pot, 0 when there is nothing to come back to.
    pub fn pot_servings(&self) -> u32 {
        self.room.pot_servings
    }

    /// How many a fresh pot holds, so the host can say so without repeating it.
    pub fn pot_capacity(&self) -> u32 {
        task::SERVINGS_PER_POT
    }

    /// Go and have what is left in the pot: a plate, the rest of the stew, and
    /// the same sit-down and clearing-up as a meal that was cooked.
    pub fn eat_leftovers(&mut self, who: usize) -> bool {
        if self.room.pot_servings == 0
            || !self.can_begin(who, Kind::Leftovers)
            || !self.take_over(who, Kind::Leftovers, 0.0)
        {
            return false;
        }
        self.bims[who].task = Some(Task::leftovers(
            who,
            &mut self.bims[who].character,
            &mut self.room,
            &self.maps,
        ));
        true
    }

    pub fn cook(&mut self, who: usize, dish: Dish) -> bool {
        if !self.room.can_cook(dish)
            || !self.can_begin(who, Kind::Meal(dish))
            || !self.take_over(who, Kind::Meal(dish), 0.0)
        {
            return false;
        }
        self.bims[who].task = Some(Task::make_food(
            who,
            dish,
            &mut self.bims[who].character,
            &mut self.room,
            &self.maps,
        ));
        true
    }

    pub fn dishwasher_loaded(&self) -> u32 {
        self.room.dishwasher.loaded
    }

    pub fn dishwasher_capacity(&self) -> u32 {
        crate::dish::CAPACITY
    }

    /// Game minutes left of the cycle, or zero when it is not running.
    pub fn dishwasher_cycle_left(&self) -> f32 {
        self.room.dishwasher.cycle_left
    }

    /// Start a cycle early, without waiting for the rack to fill. The Bim
    /// walks over and presses the button, the same as it does when its own
    /// clearing-up fills the rack.
    pub fn run_dishwasher(&mut self, who: usize) {
        self.send_to_switch(who, Switch::Dishwasher);
    }

    pub fn door_is_open(&self) -> bool {
        self.room.bath.is_open()
    }

    pub fn door_is_locked(&self) -> bool {
        self.room.bath.locked
    }

    /// The Bim walks to the door panel and works it. There is one on each side
    /// of the bulkhead, so it uses whichever it is nearest.
    ///
    /// Which way it works it is decided here, from the door the player is
    /// looking at, and carried along. Reading the door again when the hand
    /// arrives would undo the very thing that was asked for: setting off
    /// drops whatever the Bim was on, and dropping a trip to the heads opens
    /// the door it had shut behind itself.
    pub fn toggle_door(&mut self, who: usize) {
        let want = !self.room.bath.is_open();
        self.send_to_switch(who, Switch::BathDoor(want));
    }

    /// Locking shuts it too. Unlocking leaves it shut but openable.
    ///
    /// Decided at the click, for the same reason: a Bim asked to unlock while
    /// sitting on the pan used to let go of the errand — which takes its own
    /// lock off — walk to the panel, find the door unlocked, and lock itself
    /// back in.
    pub fn toggle_door_lock(&mut self, who: usize) {
        let want = !self.room.bath.locked;
        self.send_to_switch(who, Switch::BathLock(want));
    }

    /// Use the heads, then wash. Refused through a locked door.
    /// Whether the Bim could set off for the heads right now.
    ///
    /// A locked door only stops a Bim on the wrong side of it. One already in
    /// there has no door to get through and starts at the pan.
    pub fn can_use_toilet(&self, who: usize) -> bool {
        let shut_out =
            self.room.bath.locked && !self.room.bath.shell.contains(self.bims[who].character.pos);
        !shut_out && self.can_begin(who, Kind::Heads)
    }

    pub fn use_toilet(&mut self, who: usize) -> bool {
        if !self.can_use_toilet(who) || !self.take_over(who, Kind::Heads, 0.0) {
            return false;
        }
        self.bims[who].task = Some(Task::use_toilet(
            who,
            &mut self.bims[who].character,
            &mut self.room,
            &self.maps,
        ));
        true
    }

    /// Send the Bim to bed for `minutes` of game time. Ignored while it is
    /// already busy, like every other errand.
    pub fn rest(&mut self, who: usize, minutes: f32) -> bool {
        if !self.can_begin(who, Kind::Rest) || !self.take_over(who, Kind::Rest, minutes) {
            return false;
        }
        self.bims[who].task = Some(Task::rest(
            who,
            minutes,
            &mut self.bims[who].character,
            &mut self.room,
            &self.maps,
        ));
        true
    }

    /// What the Bim is busy with, for the host's status line: 0 idle, then
    /// one number per errand. Numbers rather than names, because no strings
    /// cross the boundary.
    pub fn activity(&self, who: usize) -> u32 {
        match &self.bims[who].task {
            None => 0,
            Some(task) => job_code(task.kind(), task.rest_minutes()),
        }
    }

    /// Which step of the chain is running, for the host's status line.
    /// Zero means idle.
    pub fn task_step(&self, who: usize) -> u32 {
        match &self.bims[who].task {
            None => 0,
            Some(t) => t.step() as u32 + 1,
        }
    }

    // --- rendering --------------------------------------------------------

    fn render(&mut self) {
        self.list.clear();
        self.room.draw(&mut self.list);
        // On the deck, under everything: the Bim walks over its own mess.
        self.room.filth.draw(&mut self.list);

        // The path fades out behind each Bim, so the shape of a wander is
        // visible. Both trails are the same colour: they are the shape of the
        // room being used, not a label saying who went where — the two are
        // told apart by the bodies at the head of them.
        for bim in &self.bims {
            for f in &bim.trail {
                let t = 1.0 - f.age / TRAIL_LIFE;
                self.list
                    .circle(f.pos, 3.0 + 4.0 * t, TRAIL.alpha(0.16 * t * t));
            }
        }

        // Destination pings: a ring that expands and fades where the order landed.
        for m in &self.markers {
            let t = m.age / MARKER_LIFE;
            let fade = 1.0 - t;
            let colour = if m.bad { WARN } else { ACCENT };
            self.list.circle(m.pos, 5.0, colour.alpha(0.55 * fade));
            self.list
                .ring(m.pos, 10.0 + 26.0 * t, 2.0, colour.alpha(0.7 * fade * fade));
            // A cross through a place it cannot get to, so a refusal reads as
            // a refusal and not as a ping that happens to be a different hue.
            if m.bad {
                for turn in [0.7, -0.7] {
                    self.list
                        .rect(m.pos, vec2(26.0, 3.0), turn, 1.5, WARN.alpha(0.85 * fade));
                }
            }
        }

        // Bodies in crew order, so who is drawn on top of whom does not
        // change as they walk past each other.
        for bim in &self.bims {
            bim.character.draw(&mut self.list);
        }
        // Bedding and bunk rails go over the Bim, so getting into bed puts it
        // under the covers rather than on top of them.
        self.room.draw_over(&mut self.list);

        // Night falls over the whole room at once.
        let dark = (1.0 - self.clock.daylight()) * NIGHT_DEPTH;
        if dark > 0.002 {
            let room = Rect::from_min_size(Vec2::ZERO, vec2(ROOM_W, ROOM_H));
            self.list
                .rect(room.center(), room.size(), 0.0, 0.0, NIGHT.alpha(dark));
        }

        // Whatever a panel is pointing at, ringed. Over the night wash rather
        // than under it: a highlight that dims at three in the morning is no
        // highlight. In the ship's own cyan, so it reads as neither a
        // selection (which is the Bim's green) nor trouble (which is warm).
        if let Some(area) = self.room.spot_rect(self.highlight) {
            let size = area.size() + vec2(HIGHLIGHT_MARGIN, HIGHLIGHT_MARGIN);
            self.list
                .rect(area.center(), size, 0.0, 6.0, GLOW.alpha(0.16));
            self.list
                .stroke_rect(area.center(), size, 0.0, 6.0, 2.0, GLOW.alpha(0.95));
        }

        // The marquee sits on top of everything, like the cursor it belongs to.
        if let Some((start, end)) = self.drag {
            let box_ = Rect::from_corners(start, end);
            self.list
                .rect(box_.center(), box_.size(), 0.0, 0.0, MARQUEE_FILL);
            self.list.stroke_rect(
                box_.center(),
                box_.size(),
                0.0,
                0.0,
                1.5,
                MARQUEE_EDGE.alpha(0.8),
            );
        }
    }

    pub fn draw_ptr(&self) -> *const f32 {
        self.list.as_ptr()
    }

    pub fn draw_len(&self) -> usize {
        self.list.len()
    }
}
