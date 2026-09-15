//! World state: the room, the Bim in it, the job it is doing, and the
//! per-frame draw list handed to the renderer.

use crate::character::{ACCENT, Action, BODY_MARGIN, Character};
use crate::clock::Clock;
use crate::clock::MINUTES_PER_SECOND;
use crate::draw::{Color, DrawList};
use crate::filth::{self, Filth};
use crate::health::Health;
use crate::hydro;
use crate::manager::Manager;
use crate::math::{Rect, Vec2, vec2};
use crate::nav::Maps;
use crate::needs::{Need, Needs, Urge};
use crate::rng::Rng;
use crate::room::{Dish, HIT_NONE, ROOM_H, ROOM_W, Room, Switch, WARN};
use crate::schedule::{IGNORE_ABOVE, Schedule, Slot, WAKE_AT};
use crate::task::{self, Kind, SLEEP_MINUTES, Saved, Task};

const TRAIL: Color = ACCENT;
const MARQUEE_EDGE: Color = ACCENT;
const MARQUEE_FILL: Color = Color::rgba(0.50, 0.82, 0.66, 0.10);

/// Seconds a footprint lingers.
const TRAIL_LIFE: f32 = 2.2;
/// Seconds between footprints.
const TRAIL_INTERVAL: f32 = 0.08;

/// How long the ping at an ordered destination lasts.
const MARKER_LIFE: f32 = 0.7;

/// How long the bathroom door stands open after the Bim has walked through it
/// before sliding shut again. Doors aboard shut themselves; the chains that
/// close one by hand simply get there first.
const DOOR_SHUT_AFTER: f32 = 5.0;
/// How much room the Bim needs either side of the doorway to count as clear
/// of it — its own half-width, near enough.
const DOOR_CLEARANCE: f32 = 16.0;

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
    }
}

/// The room after dark. Laid over the finished frame rather than mixed into
/// every colour, so nothing in `room.rs` has to know what time it is.
const NIGHT: Color = Color::rgb(0.03, 0.05, 0.13);
/// How dark it gets at the middle of the night. Short of black: you still
/// want to see the Bim sleeping.
const NIGHT_DEPTH: f32 = 0.46;

struct Footprint {
    pos: Vec2,
    age: f32,
}

struct Marker {
    pos: Vec2,
    age: f32,
    /// A place the Bim cannot get to. Drawn in the one warm colour the room
    /// keeps for things worth noticing, so a refused order is not silent.
    bad: bool,
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
    character: Character,
    task: Option<Task>,
    /// Chains put down part-way through, waiting to be picked up. The one at
    /// the front goes next, so an interruption pushes onto the front and the
    /// chain that was displaced is the very next thing the Bim does.
    queue: Vec<Saved>,
    trail: Vec<Footprint>,
    trail_timer: f32,
    /// Corners of the selection box while the mouse is down; `None` otherwise.
    drag: Option<(Vec2, Vec2)>,
    markers: Vec<Marker>,

    /// What the Bim wants, what going without has done to it, and whether it
    /// is allowed to act on any of that.
    needs: Needs,
    health: Health,
    /// The day's timetable. Only sleep is on it so far.
    schedule: Schedule,
    /// What the place has been told to keep in stock. The bay works to it.
    manager: Manager,
    /// The state of the deck, tile by tile, and the clocks that turn a mess
    /// into something that happens to the Bim.
    filth: Filth,
    /// Seconds until the Bim next looks for somewhere cleaner to stand.
    flee_wait: f32,
    /// Game minutes left of having dropped off standing up.
    nap_left: f32,
    /// Where the player sent the Bim, held back until a door has been opened
    /// for it. Taken up the moment that errand finishes.
    pending_move: Option<Vec2>,
    autonomous: bool,
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
        // Start somewhere in the open floor, clear of the kitchen units.
        let start = vec2(
            rng.range(ROOM_W * 0.3, ROOM_W * 0.7),
            rng.range(ROOM_H * 0.45, ROOM_H * 0.55),
        );
        let character = Character::new(start, &mut rng);
        let room_area = room.interior;
        let mut game = Game {
            room,
            clock: Clock::new(),
            maps,
            blockers: Vec::new(),
            door_was_shut: false,
            bim_was_inside: false,
            door_shut_in: 0.0,
            rng,
            character,
            task: None,
            queue: Vec::new(),
            trail: Vec::new(),
            trail_timer: 0.0,
            drag: None,
            markers: Vec::new(),
            needs: Needs::new(),
            health: Health::new(),
            schedule: Schedule::new(),
            manager: Manager::new(),
            filth: Filth::new(room_area),
            flee_wait: 0.0,
            nap_left: 0.0,
            pending_move: None,
            autonomous: true,
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

        if self.character.is_dead() {
            // Nothing else moves. The room carries on — a cycle finishes, a
            // hob goes out — but nobody is coming to see to any of it.
            let solids = self.blockers.clone();
            self.character
                .update(dt, self.room.interior, &solids, &mut self.rng);
            self.render();
            return;
        }

        // Whatever the Bim is doing, the needs move with the clock; the one it
        // is actually seeing to right now is the one that goes back up.
        let minutes = dt * MINUTES_PER_SECOND;

        // Dropping off where it stands counts as sleeping for what it does to
        // the Bim — rest comes back at the proper rate — but not for what it
        // clears, because a quarter of an hour is worth a few per cent and the
        // status only lifts above four fifths rested.
        let nodded_off = self.nap_left > 0.0;
        if nodded_off {
            self.nap_left -= minutes;
            if self.nap_left <= 0.0 {
                self.nap_left = 0.0;
                self.character.nod_off(false);
            }
        } else if self.health.drowsiness().nods_off()
            && self.task.as_ref().is_none_or(|t| t.restoring().is_none())
            && self.rng.chance(minutes / NAP_EVERY)
        {
            self.nap_left = NAP_MINUTES;
            self.character.nod_off(true);
        }

        let restoring = if nodded_off {
            Some(Need::Rest)
        } else {
            self.task.as_ref().and_then(|t| t.restoring())
        };
        self.needs
            .update(dt, restoring, self.health.stage().tiring());

        // The bay grows on the clock, against the manager's target. It is the
        // game that drives it rather than the room, because the target is the
        // manager's and the room has never heard of the manager.
        let want = self.manager.stock_target();
        self.room
            .bay
            .update(dt, minutes, self.room.veg, self.room.tofu, want);

        // Cleanliness follows the deck rather than the clock: the tiles the
        // Bim is standing among, and what it has on itself.
        let grinding = self
            .filth
            .grinding(self.character.pos, self.character.filth());
        self.needs.scrub(minutes, grinding);
        self.mind_the_mess(minutes, restoring == Some(Need::Rest));

        // Going without is measured in time, not in how empty the bar is.
        self.health.update(
            minutes,
            self.needs.level(Need::Food),
            self.needs.level(Need::Rest),
            restoring == Some(Need::Rest),
        );
        // Picking its way around a mess is slower than walking through it.
        self.character
            .set_pace(self.health.stage().pace() * self.filth.discomfort().pace());
        if self.health.is_dead() {
            self.die();
            return;
        }

        // Fully rested is fully rested, whatever the timetable or the clock
        // say: the Bim gets up rather than lying there.
        if self.needs.level(Need::Rest) >= WAKE_AT {
            if let Some(task) = &mut self.task {
                task.wake();
            }
        }

        // The timetable only ever adds a sleep, and only at the start of a
        // block. It goes on the *back* of the queue: a standing instruction
        // waits its turn, unlike an interruption, which jumps the front.
        //
        // The block is still tracked while the Bim is under orders — `due` has
        // to see the hour go by to re-arm — but nothing is added, because a
        // recruited Bim takes no instruction but the player's.
        let due = self.schedule.due(self.clock.minutes());
        if due && !self.is_recruited() && self.needs.level(Need::Rest) <= IGNORE_ABOVE {
            self.queue.push(Saved::fresh(Kind::Rest, SLEEP_MINUTES));
        }

        // Out cold on its feet: the errand waits, and so does everything else.
        if self.character.is_napping() {
            self.character
                .update(dt, self.room.interior, &self.blockers, &mut self.rng);
            self.render();
            return;
        }

        let fumble = self.health.drowsiness().fumble();
        if let Some(task) = &mut self.task {
            task.update(
                dt,
                &mut self.character,
                &mut self.room,
                &self.maps,
                &mut self.rng,
                fumble,
            );
            if task.is_done() {
                self.task = None;
            }
        }
        self.take_pending_move();
        self.pump_queue();
        self.consider_errand();
        self.flee_filth(dt);

        self.tick_door_closer(dt);

        if self.room.closed_door().is_some() != self.door_was_shut {
            self.refresh_blockers();
        }
        self.character
            .update(dt, self.room.interior, &self.blockers, &mut self.rng);

        // Footprints track the wander; during a task they would just be clutter.
        self.trail_timer -= dt;
        if self.trail_timer <= 0.0 && self.character.is_walking() && !self.character.is_scripted() {
            self.trail_timer = TRAIL_INTERVAL;
            self.trail.push(Footprint {
                pos: self.character.pos,
                age: 0.0,
            });
        }
        for f in &mut self.trail {
            f.age += dt;
        }
        self.trail.retain(|f| f.age < TRAIL_LIFE);

        for m in &mut self.markers {
            m.age += dt;
        }
        self.markers.retain(|m| m.age < MARKER_LIFE);

        self.render();
    }

    // --- mess -------------------------------------------------------------

    /// Everything a mess does to the Bim, and everything the Bim does about it
    /// short of an errand: hopping about, accidents, and being sick.
    ///
    /// None of it interrupts a chain. These are things that happen *to* the
    /// Bim — a Bim that wets itself on the way to the galley carries on to the
    /// galley — and the pose is only taken up when nothing else owns it.
    fn mind_the_mess(&mut self, minutes: f32, asleep: bool) {
        // Asleep, the needs are frozen — see `needs.rs` — so the accidents that
        // hang off them are frozen too. Without that a Bim that turns in at the
        // middle urge rolls the one-in-ten every hour of a six-hour night and
        // wets the bed about half the time, for a need that is not even moving.
        let urge = if asleep {
            Urge::None
        } else {
            self.needs.urge()
        };
        let mishap = self.filth.update(
            minutes,
            self.needs.level(Need::Restroom),
            self.needs.level(Need::Cleanliness),
            urge == Urge::Extreme,
            urge == Urge::Medium,
            &mut self.rng,
        );

        let at = self.character.pos;
        // Relief is relief, however it comes: the need goes back up, and
        // everything else gets worse. Without that the Bim would foul the
        // deck again on the very next frame, for ever.
        if mishap.fouled {
            let (on_bim, relief) = self.filth.foul(at);
            self.character.soil(on_bim);
            self.needs.refill(Need::Restroom, relief);
        } else if mishap.wet {
            let (on_bim, relief) = self.filth.wet(at);
            self.character.soil(on_bim);
            self.needs.refill(Need::Restroom, relief);
        }
        if mishap.sick {
            self.filth.sick_on(at);
            self.needs.spend(Need::Food, filth::SICK_COSTS_FOOD);
            self.character.antic(Action::Retch, RETCH_TIME);
        }

        // Hopping about on the spot: the one warning the player gets that an
        // accident is coming, and the only outward sign of a need that has no
        // bar-width left to lose.
        if !self.character.in_antic() && self.rng.chance(minutes * urge.fidget_chance()) {
            self.character.antic(Action::Fidget, FIDGET_TIME);
        }
    }

    /// Get away from it.
    ///
    /// Past the first stage of discomfort the Bim will not stand in a mess if
    /// there is anywhere better within a few tiles. It is a walk rather than an
    /// errand, so anything it is actually doing — and anything waiting on the
    /// queue — outranks it: it only moves when it would otherwise be idling.
    fn flee_filth(&mut self, dt: f32) {
        self.flee_wait = (self.flee_wait - dt).max(0.0);
        if !self.filth.discomfort().flees()
            || self.is_recruited()
            || !self.autonomous
            || self.task.is_some()
            || !self.queue.is_empty()
            || !self.character.arrived()
            || self.flee_wait > 0.0
        {
            return;
        }
        self.flee_wait = FLEE_EVERY;
        let Some(to) = self.filth.somewhere_cleaner(self.character.pos, FLEE_LOOK) else {
            return;
        };
        let nav = self.maps.pick(self.room.bath.is_open());
        let route = nav.path(self.character.pos, nav.nearest_free(to));
        if !route.is_empty() {
            self.character.follow_path(route);
        }
    }

    // --- interrupting, and getting back to it ----------------------------

    /// Put the running chain down and queue it to be picked up next.
    ///
    /// It goes on the *front*, so when one interruption interrupts another the
    /// chains come back in the order they were displaced: the most recently
    /// dropped is the first one resumed.
    fn interrupt(&mut self) {
        if let Some(task) = self.task.take() {
            let saved = task.suspend(&mut self.character, &mut self.room);
            self.queue.insert(0, saved);
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
    fn interrupt_for_order(&mut self) {
        let stale = self.pending_move.take().is_some() && self.on_a_door_errand();
        if let Some(task) = self.task.take() {
            let saved = task.suspend(&mut self.character, &mut self.room);
            if !stale {
                self.queue.insert(0, saved);
            }
        }
    }

    /// Whether the Bim is at this moment on its way to work the door panel.
    fn on_a_door_errand(&self) -> bool {
        self.task
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
    fn can_begin(&self, kind: Kind) -> bool {
        let Some(to) = task::first_station(kind, &self.room, self.character.pos) else {
            return true;
        };
        self.can_reach(to)
    }

    /// Shut the bathroom door behind whoever last walked through it.
    ///
    /// Crossing the bulkhead line arms the closer; three seconds later the
    /// door slides shut, wherever the Bim has got to by then. The chains that
    /// shut the door by hand still do — this only catches the door nobody
    /// thought to close, which otherwise stood open for the rest of the game.
    ///
    /// The Bim's own position is a frame old here, which at three seconds does
    /// not matter, and the doorway check is on the same footing as the walk it
    /// just finished.
    fn tick_door_closer(&mut self, dt: f32) {
        let inside = self.room.bath.shell.contains(self.character.pos);
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

        // On its way through: the door waits. A route is planned once and
        // never replanned, so a door that shuts across one leaves the Bim
        // walking into the panels for ever, and the chain waiting on an
        // arrival that cannot happen.
        if self
            .character
            .destination()
            .is_some_and(|to| self.room.bath.shell.contains(to) != inside)
        {
            return;
        }

        // Still in the opening. The panels do not close on the Bim, so the
        // count is held where it is until the Bim is clear of the doorway —
        // which also means the three seconds run from walking through it,
        // not from the moment the bulkhead line was crossed.
        if self
            .room
            .bath
            .door
            .expand(DOOR_CLEARANCE)
            .contains(self.character.pos)
        {
            return;
        }

        self.door_shut_in -= dt;
        if self.door_shut_in <= 0.0 {
            self.door_shut_in = 0.0;
            self.room.bath.set_open(false);
        }
    }

    fn can_reach(&self, to: Vec2) -> bool {
        let nav = self.maps.pick(self.room.bath.is_open());
        !nav.path(self.character.pos, nav.nearest_free(to))
            .is_empty()
    }

    /// Clear the way for a new errand. Returns false when the Bim is already
    /// on that very errand, so asking twice does not restart it.
    fn take_over(&mut self, kind: Kind, minutes: f32) -> bool {
        if let Some(task) = &self.task {
            if task.kind() == kind && (kind != Kind::Rest || task.rest_minutes() == minutes) {
                return false;
            }
        }
        self.interrupt();
        true
    }

    /// Back on its feet with something waiting: pick it up. Waits for an
    /// ordered walk to finish first, so the Bim gets where it was sent before
    /// returning to what it was doing.
    fn pump_queue(&mut self) {
        // Under orders, work that was put down stays put down. The queue is
        // kept, not thrown away, so letting the Bim go picks it all up again.
        if self.is_recruited()
            || self.task.is_some()
            || self.queue.is_empty()
            || !self.character.arrived()
        {
            return;
        }
        if !self.queue_ready() {
            return;
        }
        let saved = self.queue.remove(0);
        self.task = Some(Task::resume(
            saved,
            &mut self.character,
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
    fn queue_ready(&self) -> bool {
        self.queue.first().is_some_and(|saved| {
            if saved.kind() == Kind::Rest && (self.eats_first() || self.goes_first()) {
                return false;
            }
            saved
                .resume_station(&self.room, self.character.pos)
                .is_none_or(|to| self.can_reach(to))
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
    fn eats_first(&self) -> bool {
        // Only while the Bim is free to go and see to it. With autonomy off,
        // or under orders, nothing would ever start that meal — and a night
        // held back for a meal that is never cooked is a Bim that never
        // sleeps at all.
        self.autonomous
            && !self.is_recruited()
            && self.needs.level(Need::Food) < crate::needs::URGENT
            && self
                .need_station(Need::Food)
                .is_some_and(|to| self.can_reach(to))
    }

    /// Whether a trip to the heads comes before a sleep the Bim was about to
    /// start. You go before bed.
    ///
    /// The same shape as [`Game::eats_first`] and for the same reason: a need
    /// past its trigger is seen to first, but only while the Bim is actually
    /// free to see to it — autonomy on, not under orders, and the pan within
    /// reach. A locked door is not a reason to keep a tired Bim up.
    ///
    /// Nothing worse than an early start hangs on this any more: the restroom
    /// need and the accidents that hang off it are both frozen while the Bim
    /// sleeps. What it buys is a Bim that wakes up comfortable rather than one
    /// that gets up at four and runs for the heads.
    fn goes_first(&self) -> bool {
        self.autonomous
            && !self.is_recruited()
            && self.needs.level(Need::Restroom) < crate::needs::URGENT
            && self.can_use_toilet()
    }

    // --- the agenda, for the host's checklist ----------------------------

    /// The chain running now, if any, followed by everything queued behind it.
    pub fn agenda_len(&self) -> u32 {
        self.task.is_some() as u32 + self.queue.len() as u32
    }

    /// Which errand entry `i` is: see the `JOB_` codes. Zero if out of range.
    pub fn agenda_job(&self, i: u32) -> u32 {
        match self.agenda_at(i) {
            Some((kind, minutes, _)) => job_code(kind, minutes),
            None => 0,
        }
    }

    /// How far through entry `i` is, 0 to 1. A queued chain keeps whatever it
    /// had reached when it was put down.
    pub fn agenda_progress(&self, i: u32) -> f32 {
        self.agenda_at(i).map_or(0.0, |(_, _, progress)| progress)
    }

    /// Non-zero for the one entry that is actually running.
    pub fn agenda_active(&self, i: u32) -> u32 {
        (i == 0 && self.task.is_some()) as u32
    }

    fn agenda_at(&self, i: u32) -> Option<(Kind, f32, f32)> {
        let running = self.task.is_some() as u32;
        if i < running {
            let task = self.task.as_ref()?;
            return Some((task.kind(), task.rest_minutes(), task.progress()));
        }
        let saved = self.queue.get((i - running) as usize)?;
        Some((saved.kind(), saved.rest_minutes(), saved.progress()))
    }

    // --- deciding for itself ---------------------------------------------

    /// The end of it. Whatever it was doing is dropped, and so is everything
    /// waiting behind it: there is no one left to do any of it.
    fn die(&mut self) {
        self.task = None;
        self.queue.clear();
        self.character.die();
    }

    pub fn is_alive(&self) -> bool {
        !self.character.is_dead()
    }

    pub fn health(&self) -> f32 {
        self.health.points()
    }

    /// 0 when well fed, then 1, 2, 3 for the three stages of malnutrition.
    pub fn malnutrition(&self) -> u32 {
        self.health.stage().stage()
    }

    /// 0 wide awake, then 1, 2, 3 for sleepy, deprived and wrecked.
    pub fn drowsiness(&self) -> u32 {
        self.health.drowsiness().stage()
    }

    /// Whether the Bim has dropped off on its feet this instant.
    pub fn is_napping(&self) -> bool {
        self.character.is_napping()
    }

    /// Whether it is standing there having lost the thread of what it was on.
    pub fn is_stalled(&self) -> bool {
        self.task.as_ref().is_some_and(|t| t.stalled())
    }

    /// How many times the errand running now has been fumbled.
    pub fn task_fumbles(&self) -> u32 {
        self.task.as_ref().map_or(0, |t| t.fumbles())
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
    fn consider_errand(&mut self) {
        // Queued work that the Bim cannot get to does not count as having
        // something on: it would block every need there is while it waited.
        // Nor does a scheduled sleep the Bim is too hungry — or too desperate
        // for the heads — to take yet: that is what lets the errand below be
        // started in its place.
        if self.is_recruited()
            || !self.autonomous
            || self.task.is_some()
            || self.queue_ready()
            || !self.character.arrived()
        {
            return;
        }
        // Most pressing first, but a need it cannot do anything about — an
        // empty fridge, a locked door, a rest that is nobody's business here —
        // must not block the ones it can. An empty cold store would otherwise
        // pin hunger at zero and the Bim would never go to the heads again.
        for need in self.needs.urgent() {
            let started = match need {
                // Sleep is the timetable's business, not the need's. Running
                // low on rest is no longer a reason in itself to go to bed:
                // the Bim goes when the schedule says, and the level only
                // decides whether that is worth doing.
                Need::Rest => false,
                Need::Food => self.make_food(),
                Need::Restroom => self.use_toilet(),
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

        // Nothing pressing. The bay is next: it only ever asks when it is
        // running — automated and under target, or under a standing order —
        // so this is silent until the player has asked for something.
        if self.tend_bay() {
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
        if self.shut_in() {
            self.send_to_switch(Switch::BathDoor(true));
        }
    }

    /// Start on whichever tray of the bay wants a hand, if any.
    ///
    /// One errand per tray rather than one for the whole bay: an interruption
    /// then costs a tray and not the afternoon, and the Bim walks along the
    /// front of the bay tray by tray the way it would.
    fn tend_bay(&mut self) -> bool {
        let Some(job) = self.room.bay.wants_work(self.room.veg, self.room.tofu) else {
            return false;
        };
        let kind = Kind::Tend(job.spot());
        if !self.can_begin(kind) || !self.take_over(kind, 0.0) {
            return false;
        }
        self.task = Some(Task::tend(
            job.spot(),
            &mut self.character,
            &mut self.room,
            &self.maps,
        ));
        true
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
    fn shut_in(&self) -> bool {
        if self.room.bath.is_open() || self.room.bath.locked {
            return false;
        }
        let queued = self
            .queue
            .first()
            .and_then(|saved| saved.resume_station(&self.room, self.character.pos));
        let wanted = self
            .needs
            .urgent()
            .into_iter()
            .filter_map(|need| self.need_station(need));
        queued
            .into_iter()
            .chain(wanted)
            .any(|to| !self.can_reach(to) && self.can_reach_through_door(to))
    }

    /// Where a need would first send the Bim, for the needs it could actually
    /// see to. An empty cold store is not a door problem, so hunger with
    /// nothing to cook asks for no door to be opened — otherwise the Bim
    /// would work the panel over and over for a meal it can never start.
    fn need_station(&self, need: Need) -> Option<Vec2> {
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
        task::first_station(kind, &self.room, self.character.pos)
    }

    /// Whether `to` is somewhere the Bim could walk if the door were open.
    fn can_reach_through_door(&self, to: Vec2) -> bool {
        let nav = self.maps.pick(true);
        !nav.path(self.character.pos, nav.nearest_free(to))
            .is_empty()
    }

    // --- what the Bim wants ----------------------------------------------

    /// How full need `i` is, 0 to 1, in the order `Need::ALL` lists them.
    pub fn need_level(&self, i: u32) -> f32 {
        Need::from_index(i).map_or(0.0, |need| self.needs.level(need))
    }

    pub fn need_count(&self) -> u32 {
        Need::ALL.len() as u32
    }

    /// The level at which the Bim goes and does something about it.
    pub fn need_threshold(&self) -> f32 {
        crate::needs::URGENT
    }

    /// How badly it needs the heads, 0 to 3. The host names them.
    pub fn urge(&self) -> u32 {
        self.needs.urge().stage()
    }

    /// How far gone it is for want of somewhere clean, 0 to 3.
    pub fn discomfort(&self) -> u32 {
        self.filth.discomfort().stage()
    }

    pub fn bim_filth(&self) -> f32 {
        self.character.filth()
    }

    /// What the tile under a point scores, [`filth::FOULED`] to
    /// [`filth::BASELINE`]. For the probes: the host has `deck_filth` for the
    /// one number it needs.
    #[allow(dead_code)]
    pub fn tile_filth(&self, at: Vec2) -> f32 {
        self.filth.at(at)
    }

    /// How much of the deck has something on it, 0 to 1 — one number for a
    /// state that is really two hundred, so the host can say "the place is a
    /// tip" without reading every tile.
    pub fn deck_filth(&self) -> f32 {
        self.filth.dirty_share()
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
        let now = !self.character.is_recruited();
        self.character.set_recruited(now);
        if now {
            // It is the thing being ordered about, so it is the thing selected.
            self.character.selected = true;
        }
    }

    pub fn is_recruited(&self) -> bool {
        self.character.is_recruited()
    }

    pub fn is_autonomous(&self) -> bool {
        self.autonomous
    }

    // --- player input ---------------------------------------------------

    /// Select by control group. There is one Bim, so group 1 is all of them.
    pub fn select_group(&mut self, group: u32) {
        if group == 1 {
            self.character.selected = true;
        }
    }

    pub fn clear_selection(&mut self) {
        self.character.selected = false;
    }

    pub fn bim_pos(&self) -> Vec2 {
        self.character.pos
    }

    pub fn selected_count(&self) -> u32 {
        self.character.selected as u32
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

        self.character.selected =
            box_.touches_circle(self.character.pos, self.character.pick_radius());
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
        if !self.character.selected || !self.is_alive() {
            return ORDER_IGNORED;
        }
        let want = vec2(x, y);

        // Snap to somewhere the Bim can actually stand, so an order onto the
        // table means "the floor beside the table" rather than nothing at all.
        let nav = self.maps.pick(self.room.bath.is_open());
        let target = nav.nearest_free(want);
        let route = nav.path(self.character.pos, target);
        if !route.is_empty() {
            // The order outranks whatever the Bim is on. That chain goes into
            // the queue and is picked up once it has been where it was sent.
            self.interrupt_for_order();
            self.character.follow_path(route);
            self.mark(target, false);
            return ORDER_MOVING;
        }

        // Nothing as things stand. Would there be with the door open?
        let open = self.maps.pick(true);
        let through = open.nearest_free(want);
        if open.path(self.character.pos, through).is_empty() {
            self.mark(through, true);
            return ORDER_NOWHERE;
        }

        // A Bim part-way through a trip to the heads has locked the door
        // behind itself, and letting go of that errand unlocks it again. So
        // that is not being locked out: it is the Bim's own door, and the
        // order is what opens it. Any other locked door genuinely blocks, and
        // nothing is attempted — not even dropping what it was doing.
        let own_lock = self
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
        if self.on_a_door_errand() {
            self.pending_move = Some(want);
            self.mark(through, false);
            return ORDER_VIA_DOOR;
        }

        self.interrupt_for_order();

        // Letting go may have opened the door by itself, so look again before
        // sending the Bim off to work a panel it no longer needs to touch.
        let nav = self.maps.pick(self.room.bath.is_open());
        let target = nav.nearest_free(want);
        let route = nav.path(self.character.pos, target);
        if !route.is_empty() {
            self.character.follow_path(route);
            self.mark(target, false);
            return ORDER_MOVING;
        }

        // Go and open it, then carry on to where it was sent.
        self.pending_move = Some(want);
        self.task = Some(Task::work_switch(
            Switch::BathDoor(true),
            &mut self.character,
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
    fn take_pending_move(&mut self) {
        if self.task.is_some() || !self.character.arrived() {
            return;
        }
        let Some(want) = self.pending_move.take() else {
            return;
        };
        let nav = self.maps.pick(self.room.bath.is_open());
        let target = nav.nearest_free(want);
        let route = nav.path(self.character.pos, target);
        if route.is_empty() {
            // The door shut again, or was locked while the Bim walked to it.
            self.mark(target, true);
            return;
        }
        self.character.follow_path(route);
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

    /// Game minutes of sleep the Bim still has ahead of it, or zero.
    pub fn rest_left(&self) -> f32 {
        match &self.task {
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
    pub fn is_busy(&self) -> bool {
        self.task.is_some()
    }

    /// Send the Bim to work a switch. Nothing aboard changes without it: the
    /// state only moves when the Bim's hand gets there, and this displaces
    /// whatever it was doing exactly like any other errand.
    fn send_to_switch(&mut self, which: Switch) {
        if !self.can_begin(Kind::Switch(which)) || !self.take_over(Kind::Switch(which), 0.0) {
            return;
        }
        self.task = Some(Task::work_switch(
            which,
            &mut self.character,
            &mut self.room,
            &self.maps,
        ));
    }

    pub fn toggle_fridge(&mut self) {
        self.send_to_switch(Switch::FridgeDoor);
    }

    /// Flipping the hob is a physical act: the Bim walks over and does it,
    /// rather than the switch moving by itself.
    pub fn toggle_stove(&mut self) {
        self.send_to_switch(Switch::Hob);
    }

    /// Kick off the make-a-meal chain. Ignored if one is already running.
    /// Cook whichever the Bim fancies. Both recipes cost two out of the cold
    /// store, so with fewer than that left there is nothing to be done.
    pub fn make_food(&mut self) -> bool {
        // Already on a meal: asking again is not a second dinner. `cook` turns
        // down the *same* dish by itself, but this one picks at random and
        // would otherwise displace a half-chopped stew with a bowl.
        if self
            .task
            .as_ref()
            .is_some_and(|task| matches!(task.kind(), Kind::Meal(_) | Kind::Leftovers))
        {
            return false;
        }

        // There is a pot on the hob with something in it. Cooking a second one
        // on top of that would throw the first away, and the whole point of a
        // pot holding two is that the Bim comes back to it.
        if self.eat_leftovers() {
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
        self.cook(first) || self.cook(second)
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
    pub fn eat_leftovers(&mut self) -> bool {
        if self.room.pot_servings == 0
            || !self.can_begin(Kind::Leftovers)
            || !self.take_over(Kind::Leftovers, 0.0)
        {
            return false;
        }
        self.task = Some(Task::leftovers(
            &mut self.character,
            &mut self.room,
            &self.maps,
        ));
        true
    }

    pub fn cook(&mut self, dish: Dish) -> bool {
        if !self.room.can_cook(dish)
            || !self.can_begin(Kind::Meal(dish))
            || !self.take_over(Kind::Meal(dish), 0.0)
        {
            return false;
        }
        self.task = Some(Task::make_food(
            dish,
            &mut self.character,
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
    pub fn run_dishwasher(&mut self) {
        self.send_to_switch(Switch::Dishwasher);
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
    pub fn toggle_door(&mut self) {
        let want = !self.room.bath.is_open();
        self.send_to_switch(Switch::BathDoor(want));
    }

    /// Locking shuts it too. Unlocking leaves it shut but openable.
    ///
    /// Decided at the click, for the same reason: a Bim asked to unlock while
    /// sitting on the pan used to let go of the errand — which takes its own
    /// lock off — walk to the panel, find the door unlocked, and lock itself
    /// back in.
    pub fn toggle_door_lock(&mut self) {
        let want = !self.room.bath.locked;
        self.send_to_switch(Switch::BathLock(want));
    }

    /// Use the heads, then wash. Refused through a locked door.
    /// Whether the Bim could set off for the heads right now.
    ///
    /// A locked door only stops a Bim on the wrong side of it. One already in
    /// there has no door to get through and starts at the pan.
    pub fn can_use_toilet(&self) -> bool {
        let shut_out = self.room.bath.locked && !self.room.bath.shell.contains(self.character.pos);
        !shut_out && self.can_begin(Kind::Heads)
    }

    pub fn use_toilet(&mut self) -> bool {
        if !self.can_use_toilet() || !self.take_over(Kind::Heads, 0.0) {
            return false;
        }
        self.task = Some(Task::use_toilet(
            &mut self.character,
            &mut self.room,
            &self.maps,
        ));
        true
    }

    /// Send the Bim to bed for `minutes` of game time. Ignored while it is
    /// already busy, like every other errand.
    pub fn rest(&mut self, minutes: f32) -> bool {
        if !self.can_begin(Kind::Rest) || !self.take_over(Kind::Rest, minutes) {
            return false;
        }
        self.task = Some(Task::rest(
            minutes,
            &mut self.character,
            &mut self.room,
            &self.maps,
        ));
        true
    }

    /// What the Bim is busy with, for the host's status line: 0 idle, then
    /// one number per errand. Numbers rather than names, because no strings
    /// cross the boundary.
    pub fn activity(&self) -> u32 {
        match &self.task {
            None => 0,
            Some(task) => job_code(task.kind(), task.rest_minutes()),
        }
    }

    /// Which step of the chain is running, for the host's status line.
    /// Zero means idle.
    pub fn task_step(&self) -> u32 {
        match &self.task {
            None => 0,
            Some(t) => t.step() as u32 + 1,
        }
    }

    // --- rendering --------------------------------------------------------

    fn render(&mut self) {
        self.list.clear();
        self.room.draw(&mut self.list);
        // On the deck, under everything: the Bim walks over its own mess.
        self.filth.draw(&mut self.list);

        // The path fades out behind the Bim, so the shape of the wander is visible.
        for f in &self.trail {
            let t = 1.0 - f.age / TRAIL_LIFE;
            self.list
                .circle(f.pos, 3.0 + 4.0 * t, TRAIL.alpha(0.16 * t * t));
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

        self.character.draw(&mut self.list);
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
