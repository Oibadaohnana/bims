//! World state: the room, the Bim in it, the job it is doing, and the
//! per-frame draw list handed to the renderer.

use crate::character::{ACCENT, BODY_MARGIN, Character};
use crate::clock::Clock;
use crate::draw::{Color, DrawList};
use crate::math::{Rect, Vec2, vec2};
use crate::nav::Maps;
use crate::rng::Rng;
use crate::room::{HIT_NONE, ROOM_H, ROOM_W, Room};
use crate::task::{Kind, Task};

const TRAIL: Color = ACCENT;
const MARQUEE_EDGE: Color = ACCENT;
const MARQUEE_FILL: Color = Color::rgba(0.50, 0.82, 0.66, 0.10);

/// Seconds a footprint lingers.
const TRAIL_LIFE: f32 = 2.2;
/// Seconds between footprints.
const TRAIL_INTERVAL: f32 = 0.08;

/// How long the ping at an ordered destination lasts.
const MARKER_LIFE: f32 = 0.7;

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
}

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
    rng: Rng,
    character: Character,
    task: Option<Task>,
    trail: Vec<Footprint>,
    trail_timer: f32,
    /// Corners of the selection box while the mouse is down; `None` otherwise.
    drag: Option<(Vec2, Vec2)>,
    markers: Vec<Marker>,
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
        let mut game = Game {
            room,
            clock: Clock::new(),
            maps,
            blockers: Vec::new(),
            door_was_shut: false,
            rng,
            character,
            task: None,
            trail: Vec::new(),
            trail_timer: 0.0,
            drag: None,
            markers: Vec::new(),
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

        if let Some(task) = &mut self.task {
            task.update(dt, &mut self.character, &mut self.room, &self.maps);
            if task.is_done() {
                self.task = None;
            }
        }

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
    pub fn order_move(&mut self, x: f32, y: f32) {
        if !self.character.selected || self.task.is_some() {
            return;
        }
        // Snap to somewhere the Bim can actually stand, so an order onto the
        // table means "the floor beside the table" rather than nothing at all.
        // A shut door is a wall here, so an order through one goes nowhere.
        let nav = self.maps.pick(self.room.bath.is_open());
        let target = nav.nearest_free(vec2(x, y));
        let route = nav.path(self.character.pos, target);
        if route.is_empty() {
            return;
        }
        self.character.follow_path(route);
        self.markers.push(Marker {
            pos: target,
            age: 0.0,
        });
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

    /// True while a task has the Bim, so the host can grey out "Make food".
    pub fn is_busy(&self) -> bool {
        self.task.is_some()
    }

    pub fn toggle_fridge(&mut self) {
        if self.is_busy() {
            return;
        }
        let open = self.room.fridge_is_open();
        self.room.set_fridge_open(!open);
    }

    /// Flipping the hob is a physical act: the Bim walks over and does it,
    /// rather than the switch moving by itself.
    pub fn toggle_stove(&mut self) {
        if self.task.is_some() {
            return;
        }
        self.task = Some(Task::stove_switch(
            &mut self.character,
            &mut self.room,
            &self.maps,
        ));
    }

    /// Kick off the make-a-meal chain. Ignored if one is already running.
    pub fn make_food(&mut self) {
        if self.task.is_some() {
            return;
        }
        self.task = Some(Task::make_food(
            &mut self.character,
            &mut self.room,
            &self.maps,
        ));
    }

    pub fn door_is_open(&self) -> bool {
        self.room.bath.is_open()
    }

    pub fn door_is_locked(&self) -> bool {
        self.room.bath.locked
    }

    /// The door is powered, so the player works it from anywhere; the Bim only
    /// walks to the panel when a task of its own needs it.
    pub fn toggle_door(&mut self) {
        if self.is_busy() {
            return;
        }
        let open = self.room.bath.is_open();
        self.room.bath.set_open(!open);
    }

    /// Locking shuts it too. Unlocking leaves it shut but openable.
    pub fn toggle_door_lock(&mut self) {
        if self.is_busy() {
            return;
        }
        let locked = self.room.bath.locked;
        self.room.bath.set_locked(!locked);
    }

    /// Use the heads, then wash. Refused through a locked door.
    pub fn use_toilet(&mut self) {
        if self.task.is_some() || self.room.bath.locked {
            return;
        }
        self.task = Some(Task::use_toilet(
            &mut self.character,
            &mut self.room,
            &self.maps,
        ));
    }

    /// Send the Bim to bed for `minutes` of game time. Ignored while it is
    /// already busy, like every other errand.
    pub fn rest(&mut self, minutes: f32) {
        if self.task.is_some() {
            return;
        }
        self.task = Some(Task::rest(
            minutes,
            &mut self.character,
            &mut self.room,
            &self.maps,
        ));
    }

    /// What the Bim is busy with, for the host's status line: 0 idle, then
    /// one number per errand. Numbers rather than names, because no strings
    /// cross the boundary.
    pub fn activity(&self) -> u32 {
        match self.task.as_ref().map(|t| t.kind()) {
            None => 0,
            Some(Kind::Meal) => 1,
            Some(Kind::Stove) => 2,
            Some(Kind::Rest) => 3,
            Some(Kind::Heads) => 4,
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
            self.list.circle(m.pos, 5.0, ACCENT.alpha(0.55 * fade));
            self.list
                .ring(m.pos, 10.0 + 26.0 * t, 2.0, ACCENT.alpha(0.7 * fade * fade));
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
