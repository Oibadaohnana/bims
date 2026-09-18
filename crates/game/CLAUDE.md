# The room

Notes on `crates/game` — the Bims' simulation, the behaviour test room and the
room aboard a ship. The root `CLAUDE.md` is how to run and verify anything;
the probe notes are `scratchpad/CLAUDE.md`, and the app that draws the room
and its panels is `crates/app` (`screens/room.rs`, `crew.rs`).

> **Since the port to Bevy (September 2026):** the browser host is gone.
> Every name table this file points at in `web/*.js` is now
> `crates/app/src/names.rs`; the pages are the screens in
> `crates/app/src/screens/`; the `bims_*` and `ship_*` exports are methods on
> `bims::game::Game` and `ship::Session`. The node harnesses
> (`scratchpad/*.mjs`, `smoke.mjs`, `stub.mjs`) are gone with the host — a
> check this file says lives in one of them lives nowhere now unless it says
> so in `crates/app`'s tests or `./check smoke`, and is worth restoring as a
> unit test if the thing it guarded moves again.

## An empty route means "arrived"

`Character::follow_path` with an empty route deliberately does not enter the
marching state, so `arrived()` is true immediately — the alternative is a task
that waits for ever on a walk that cannot happen. Any code that treats arrival
as proof the Bim is somewhere has to handle that case, or a whole chain will run
through in one frame and the first step that sets a position teleports the Bim
across the room. `Task::enter` now marks the chain blocked instead, and
`Game::can_begin` refuses to start one whose first station is unreachable.

## A route is planned once, never replanned

`Character::follow_path` is handed a finished list of waypoints and walks it to
the end. Nothing re-runs the pathfinder when the room changes shape, so anything
that puts the bathroom door across a walk already in progress leaves the Bim
pressed against the panels for ever — still *marching*, never arriving, with the
chain waiting on an `arrived()` that will not come. It converges on the door and
looks for all the world like a stuck animation.

`Game::tick_door_closer` therefore holds the door while
`Character::destination()` is on the far side of the bulkhead from the Bim, as
well as while the Bim is still in the opening. Anything else that shuts a door
by itself needs the same two guards.

## The cold store is two counts

`Room::veg` and `Room::tofu`, not one number. A stew is two vegetables, a bowl
is a block of tofu *and* a vegetable for the salad, so `can_cook(dish)` is the
question to ask and `has_ingredients()` only means "something can be made".
Anything that asks "is there enough" without naming a dish will let a chain
start that cannot finish.

## Adding furniture moves everything

The hydroponic bay added one rect to `Room::solids`, which changed the nav grid,
which changed every route length, which reshuffled the RNG stream — every roll
in `filth.rs` is drawn per frame, so a different number of frames before one
means a different outcome. Two long-run probes failed on that alone and neither
failure was in the code under test. When a day-in-the-life probe starts failing
after a layout change, check the trace before the logic: `mealtrace`/`messtrace`
in the scratchpad print every state change with a clock reading, which is how
the one that mattered — a Bim wetting itself in its sleep — was found.

## A route the body cannot hold to

`nav.rs` marks a cell blocked by its *centre*, so a straight line between two
free cells can pass within half a cell of an inflated obstacle and still test
clear. The Bim walks that line, the collision push-out shoves it back, and
where the push is exactly opposite the walk — a waypoint through the corner of
the table — the two cancel exactly: it marches on the spot for ever, `arrived()`
never comes true, and the chain waiting on it never finishes. It stood there
for ten game hours with a trip to the heads on its agenda, and nothing in the
game says anything is wrong: `is_stalled()` is false, it is not napping, it is
simply not moving.

`line_clear` now samples half a cell either side of the line as well as along
it. If a Bim is ever found frozen mid-errand, check its position against the
inflated furniture before looking anywhere else.

**It is rarer since, and which seeds hit it moves with the RNG — and what
is left is caught by `Game::unstick`, the watchdog described at the end of
this file.**
Sampling forty seeds for a week each: before dirt spreading, seeds 17, 18 and 29
froze a Bim mid-errand for 3152, 2651 and 1136 game minutes; after it, seed 8
for 189 and seed 25 for 49, and seed 101 — the seed `sweep.rs`, `crew.rs` and
`crowding.rs` all happen to pin — for about fifty hours, which is what those
three now fail on. The failure reads as starvation or as a filthy deck, because
that is what happens to a Bim standing still for two days; the cause is a Bim
`is_walking()` at a fixed position with a destination it never reaches.

So **any change that draws from the RNG at all re-rolls which seeds show it**,
and a probe that starts failing at one seed after an unrelated change is very
likely this and not the change. The trace to run is the one that prints
`activity`, `is_walking`, `bim_pos` and `destination_for_probe` for a Bim whose
errand has lasted an hour — a frozen position with `walking true` is the
signature, and nothing else aboard looks like it.

## The `SPOT_` codes are not the `HIT_` codes

There are two "what is at this point" questions and they want opposite
answers. `Room::hit` answers *what would a click act on*: few codes, only the
things with a menu behind them, and every rect expanded a few pixels so a near
miss on a handle still opens the menu. `Room::spot` answers *what is this*, for
the readout at the top left: everything aboard has a name, including the
worktop and the bulkheads, and nothing is expanded — a pixel beside the pan has
to read as deck.

Keep them separate. Folding one into the other loses the slack a click needs or
gives the readout a lie. `SPOT_NAMES` in `crates/app/src/names.rs` is indexed by the code,
so a new fixture needs a name added there in the same commit or the readout
says "Something".

## A need trigger is two things, and off is not zero

`needs::Trigger` is a level *and* a switch. Switching one off stops the errand
and nothing else: the need carries on draining, and everything going without
does to the Bim still bites. A probe that asserts a switched-off need "empties"
is wrong — a Bim past the rest trigger nods off where it stands, and that
claws back enough that the level settles just under the trigger rather than at
nothing. Assert it fell past the trigger and that `drowsiness()` is non-zero.

The drain rates in `needs.rs` are still derived from `URGENT`: each is the drop
from full to a tenth over the time that drop is meant to take. Moving a trigger
is the player's business and deliberately allowed, but it means that need's
slot no longer tiles the day the way the arithmetic says. Do not "fix" the
rates to follow the trigger — the derivation is the documentation.

## Two rules that have to move together

`Game::eats_first` and `Game::goes_first` hold a sleep back. Whatever *starts*
that sleep has to consult both, and whatever the sleep is waiting for has to be
started by something. When the rest trigger was added, `consider_errand` gained
both halves: `Need::Rest` refuses while either is true, and the block at the
bottom that starts the trip now fires for `wants_bed()` as well as for a queued
bed. Add a third reason to go to bed and it needs both halves too, or the two
rules deadlock and the Bim neither goes nor turns in.

## `suspend` must not bank what `Saved` is still carrying

`Task::let_go` takes the crop the Bim is carrying and puts it in the store,
because a chain given up for good must not evaporate a plant. `Task::suspend`
calls the same function — and a suspended chain is *kept*: the `Saved` it just
built still holds `lifted`, and the Bim will walk back to the fridge with it.
So `suspend` passes `None` and only the `blocked` path passes the crop, via
`take()` so nothing can bank it twice. Get this wrong and the bay quietly
doubles its output every time an errand is interrupted.

## The bay's progress bar stops short on a planting, deliberately

`Kind::Tend` is one chain with two endings: a planting finishes at the tray, a
harvest carries on to the cold store. `Kind::steps()` counts the whole thing,
so a planting's agenda row vanishes at about half a bar.

Do not "fix" this by weighing the chain against what the Bim is holding. It is
holding nothing until `WorkTray` has *finished*, so the bar would read 100% all
the way through the tray and then jump back to half when the carry began. A bar
that stops short beats one that goes backwards.

## There are two Bims, and `who` goes everywhere

`Game` holds `bims: Vec<Bim>` and almost every method that used to touch
`self.character` / `self.task` / `self.needs` now takes a `who: usize` first.
`crates/game/src/bim.rs` owns the per-crew state; the room, the clock, the deck's filth,
the timetable and the manager stay on `Game` because they are the ship's.

Three rules that keep it honest:

- **`bim::PLAYER` is the only one the player touches.** Selection, orders,
  recruiting and every fixture menu go through it, and the wasm exports for
  those take no index at all — the boundary itself says only James is steered.
  Anything that *reads* takes a `who` so the host can show both.
- **The timetable and the action thresholds are the ship's, not a Bim's.**
  `Schedule::due` re-arms itself the moment it is asked, so it is asked **once
  per frame** in `Game::update` and the answer is handed to each of the crew.
  Ask it inside the per-Bim loop and only the first one ever gets a night.
  `set_need_trigger` likewise writes to every Bim.
- **A berth and a seat belong to a Bim.** `Task` and `Saved` carry `who` for
  exactly this: `destination` needs it to pick a bed and a chair, and a chain
  put down and resumed has to come back to the same ones.

## Borrowing one Bim and the room at once

`self.bims[who].task = Some(Task::rest(who, m, &mut self.bims[who].character,
&mut self.room, &self.maps))` does not compile, and neither does anything that
holds `&mut self.bims[who]` across a call to another `&mut self` method. Split
the fields locally first:

```rust
let (bims, room, maps) = (&mut self.bims, &mut self.room, &self.maps);
let bim = &mut bims[who];
```

Disjoint field borrows are fine *within one function body*. That is why the
per-Bim half of the frame is written as `&mut self` methods taking `who` and
re-indexing each time, rather than as one long-lived borrow.

## Every fixture is a list, and a chain picks the closest free one

`crates/game/src/galley.rs` is the galley as structs — `Worktop` (board,
drawer, knife, slices), `Hob` (heat, idle clock, the pot, the serving
plate, the bad-food judgement), `Fridge` (a door), `Locker` (a broom) —
and `Room` holds `worktops`, `hobs`, `fridges`, `dishwashers`, `lockers`,
`showers`, `bays`, and `bath` + `more_baths` (`Room::bath_at(i)`, nought
the room's own, which in the classic room has the walls and the door).
`Layout::more` (`room::More`) is every fixture past the first of its kind,
from `aboard::layout_of`; a toilet is paired with the basin nearest it.
What is *shared* stays on the room — the cold store's counts, the plate
count, `dish`: a cold store is a class of storage, not a box.

A chain **picks** (`task::Picks`, one `Option<usize>` a kind) the moment a
step first walks to a kind of fixture — `pick_for` in `Task::enter`, the
closest one nobody else has (`galley::closest_free`), with two extra
asks: leftovers want a hob with something in the pot, a broom wants a
locker with a broom in it — and keeps it for the rest of the errand and
across a suspend, since a half-cooked meal's pot is on its hob.
`destination` reads the picks; the standing steps index by them
(`Task::hob()` etc., nought before a pick). A cooking chain clears the
worktop and the hob's serving spot as it picks them (`reset_worktop`,
`reset_hob`) rather than at the start, since it does not know which yet.

What the *others* hold is `task::Taken`, gathered by `Game::taken_for(who)`
from every other Bim's errand **and queue**, and handed to every `Task`
constructor and `update`. Two places have to ask it, and forgetting the
second is the trap: `can_begin` (`task::can_pick_all` — one of
*everything* the kind walks to, `fixtures_used`, so a Bim does not set out
for a galley with no free hob and drop its slices there) covers errands
that *start*, and `queue_ready` (`Picks::clashes`) covers ones that
**resume**. `Exclusive` in `game.rs` keeps only what is not a fixture list:
a bench, the airlock, a site. Nothing free at all is `blocked`, the way a
shut door is. With one of everything this is exactly the old one-galley
rule — `REFERENCE_CHECKSUM` did not move — and with two it is two cooks:
`a_second_galley_is_cooked_in_at_the_same_time` in the world tests.

The menus act on the fixture under the click: `Game::hit_at` records
`hit_hob`, `hit_fridge`, `hit_dishwasher`, `hit_locker`, `hit_shower`,
`hit_bath` beside `hit_bay` and `hit_door`, `Switch::Hob(i)`,
`FridgeDoor(i)` and `Dishwasher(i)` carry the index, and every accessor
the app reads takes one; `galley_busy_by(who)` is "no galley free at all",
which is what greys **Make food** now, not one Bim cooking.

Stations aboard are per fixture (`Room::station_at(frame, x)`): a Bim
stands below the fixture it is working. The classic room keeps every
galley station on the worktop's line, whatever the fixture's depth,
because the probes are seeded on where the Bim stands.

`reset_worktop` deliberately does not clear the table: the other Bim may
well be sitting there eating what it cooked twenty minutes ago, and
wiping its plate would be the cook reaching across the room.

## Crew separation runs after both have moved

`Game::separate_crew` is its own pass at the end of the frame, not part of
either Bim's update. Run per-Bim it would give the one updated second the last
word and the pair would creep across the deck. It shoves each half the overlap,
and skips anyone seated, in bed or dead — they are where a chain put them —
pushing the other the whole way instead.

Knock-on worth remembering: **a recruited Bim drifts.** It never sets off
anywhere, but the other one walking into it moves it a body's width at a time.
A probe asserting "a recruited Bim stays put" by distance is asserting the
wrong thing; assert `is_walking()` is never true.

## The bed is a bed, and its footprint is still the bunk's

`Berth` in `crates/game/src/room.rs` draws a single bed now — headboard,
footboard, mattress, pillow, duvet — where it used to draw a bunk bed with a
ladder. The **footprint did not change**: the classic room's is still
94×172, the old upper deck plus the strip the lower one stuck out by, and
`station()` and `lie_pos()` are the bunk's numbers expressed off the frame.
That is deliberate. The footprint is what the nav grid is built on, and a
bed one pixel smaller is every route re-lengthened and every seed the probes
pin re-rolled — see "Adding furniture moves everything". Make the bed
smaller and expect `probe`, `crew`, `sweep`, `neglect` and `social` to move.

`social.rs` already fails at HEAD — seeds 1 and 7, "the bar hovers about its
trigger" — before and after the bed changed; it is not the bed.

## A per-Bim clock must not live on a shared object

`Filth` is the *deck*, and there is one deck. It used to also hold
`bursting_for`, `filthy_for` and `since_sick` — which are clocks on a **body**:
how long *this* Bim has been at the extreme urge, how long *it* has stood in
the mess. With one Bim that made no difference. With two it made every
difference: `Filth::update` ran once per crew member per frame on the same
object, so whichever Bim was comfortable zeroed the other's clock on the way
past. The hour was never reached, nobody ever had an accident, and nobody was
ever sick. The needs panel looked fine throughout — the *levels* were right,
and only the stages built on top of them were dead.

They are `filth::Ordeal` now, held by `Bim`. The rule the bug came from is
worth keeping: **when adding a second of something, every mutable clock has to
be asked whose it is.** A level that looks right is not evidence that the
machinery hanging off it does.

`scratchpad/neglect.rs` is the probe: it drives both crew to the far end of
every neglect and checks each reaches stage 3 and actually has the accident.

## Autonomy off does not stop a scheduled night

`set_autonomous(false)` stops a Bim *deciding* to sleep. The timetable still
queues one, because a scheduled night is a standing instruction from the player
rather than the Bim's own idea, and `pump_queue` is not gated on autonomy.

So a probe measuring "what going without sleep looks like" has to wipe the
schedule as well, or it measures a well-rested Bim and concludes the last stage
of drowsiness is unreachable. That cost a wrong diagnosis once already.

## Selecting is not commanding

Any of the crew can be selected; only `bim::PLAYER` takes orders. The two are
deliberately separate questions, and the split is what lets the right-hand side
show whichever Bim you clicked while `order_move` still refuses anybody but
James. Exactly one is selected at a time, because the panels show one at a time.

## The diary keeps no words, and almost no entries

`memory.rs` stores `(day, minutes, code, one number)` and nothing else;
`MEMORY_LINES` in `crates/app/src/names.rs` is where every sentence lives.

**It only records what went wrong.** The day's work is not written down at all
— it used to be, and the page filled with "Went to the heads." three times a
day. So a quiet week leaves the diary *empty*, and `scratchpad/diary.rs`
asserts exactly that: a clean four days must produce nought entries. If you
find yourself adding a `What` for something that goes right, that is the rule
saying no.

An entry with no line in `MEMORY_LINES` is **dropped from the page** rather
than rendered as a placeholder. There used to be a "Something happened."
fallback and a harness check for that string; both are gone. What catches a
missing line now is `smoke.mjs`'s `lines.length === bims_memory_len(0)` — a
code with no words is a row that never appears, so the count comes up short.

Two knock-ons worth knowing, both of which bit when this changed:

- **A Bim had nothing to say.** Conversation topics were drawn from the
  speaker's own diary, so trimming the diary left both crew talking about
  "nothing much" for ever. `Bim::lately` now holds the last few finished
  errands as `job_code`s purely for small talk. `Game::chat_topic` therefore
  returns **two code spaces** — `JOB_` codes from 1, `What` codes from 20 —
  and `CHAT_TOPICS` is indexed by both. They do not overlap; keep it that way.
- **`Task::stowed` and `Task::swept()` existed only for the diary** and were
  dead the moment it stopped recording errands. Both are gone. If a diary
  entry ever wants that detail back, it has to be carried again.

## The crew pass through each other on purpose

Bodies do not collide. Two Bims that meet overlap, and both drop to
`CROWDED_PACE` while they are within `CREW_CLEARANCE`. That is the whole rule —
`Game::crowding` and one multiplier on the pace.

It is deliberate and it is the *only* resolution that cannot deadlock. **A
route is planned once and never replanned**, so a Bim whose line goes through
another has no second plan; anything that stops it reaching that line risks two
of them standing nose to nose for ever with errands on both agendas.

There was a whole apparatus here before — push apart, pick one at random to
stand aside for three seconds, shove that one sideways out of the other's lane,
with a blunt backstop for when the sideways step hit a bulkhead. It worked, but
only after two rounds of fixing what the fix broke, and every part of it existed
to keep bodies from overlapping. Once overlapping is allowed it all has nothing
to do. If a reason to reinstate collision ever appears, reinstate the deadlock
with it and read that history first.

A seated Bim crowds nobody: `crowding` skips anyone `is_seated()`, because a
body tucked into a bunk or a chair is not in the gangway.

## `Filth` lives on `Room` now

The deck used to hang off `Game`. It moved because `task.rs` needs it: the
sweeping chain has to ask where the dirt is, and a chain is handed the room and
nothing else. `Game::render` still draws it separately — `self.room.filth.draw`
— because the layering matters and only `Game` knows the order.

## Sweeping: two corners that will bite again

- **Sweep the tile the Bim set out for, not the one under its boots.** They
  differ whenever the dirt is somewhere a body cannot quite stand, and a broom
  has the reach. Sweeping underfoot leaves those tiles filthy for ever *and*
  loops, because the unswept tile is still the worst on the deck next time.
- **`worst_tile` takes a reachability predicate, and it is not optional.**
  Parts of the deck read as `SPOT_DECK` and have no route to them — the corner
  past the end of the counter, hemmed in by the second bunk. Without the
  predicate a mess there is chosen as the worst tile for ever: fetch broom,
  fail the walk, give up, start again, three tiles left on the deck after four
  simulated days. `Filth` has no idea where a body fits, so the question goes
  to `next_dirty` in `task.rs`, which has the nav grid.

`Task::enter` and `Task::next_step` must ask that question *the same way* —
both go through `next_dirty`. If `next_step` says "more to sweep" and `enter`
then finds nowhere to go, `CarryBroomTo` is a step of no length and the chain
spins between it and `Sweep` for ever.

## Anything held has to survive the chain being given up

`Task::let_go` puts the broom back the same way it banks a carried harvest. A
Clean chain abandoned mid-sweep would otherwise leave the Bim holding the broom
for good — and since the locker door is drawn from whose hands it is in, the
cupboard would stand empty and nobody could ever take a broom that was never
returned. Add a held item and this is the second place to touch.

## A new `What` needs four edits, not one

`memory.rs` for the code, `MEMORY_LINES` in `crates/app/src/names.rs` for the sentence,
`CHAT_TOPICS` beside it for the short form a Bim says out loud, and the range
in `scratchpad/diary.rs` that checks every entry is nameable.

Miss the sentence and the entry is silently **dropped from the page** — no
placeholder any more — which `smoke.mjs` catches only through its row count.
Miss the range and the probe fails on a perfectly good entry, which is what
`Swept = 9` did.

And before adding one at all: the diary keeps only what went wrong. A `What`
for something that went right does not belong in it.

## Adding a job to the work list needs three edits

`work.rs` for the variant, `JOB_NAMES` in `crates/app/src/names.rs` for the word, and the
range in `scratchpad/priority.rs` that checks every code names a job. Same
shape as `memory.rs`'s `What` and for the same reason — no strings cross the
boundary, so the ship knows `Job::Clean` and only the host knows "Cleaning".
Miss the second and the row comes up blank; `scratchpad/work-check.mjs` fails
on exactly that, because a blank row is a row the player cannot use and
nothing else would notice.

The host builds its rows off `bims_work_count()` rather than off the length of
its own name table, so the two disagreeing shows up as a blank row rather than
as a job silently missing from the panel. Keep it that way.

## Health mends every frame, so a sudden nothing is not nothing

`Health::update` runs before `Game` looks at whether the Bim is dead, and a fed
Bim's health climbs. Anything that takes the bar to zero *later* in the frame —
a Bim hurting itself, a Bim giving up — was therefore back above zero by the
time the check came round, and the death simply never happened: the run ended
with a Bim reading nought health and still walking about. `update` now returns
immediately when `points <= 0.0`. Anything new that damages health in one go
depends on that, so do not "tidy" it away.

## Two Bims standing together is a layout problem

Names are painted a body's height above each head. A pair who meet walking
north–south end up one above the other, and the lower one's name lands on the
upper one's body — `./layout talk` shows it immediately and nothing else does.
`Game::chat` therefore always stands them **left and right**, `TALKING_GAP`
apart, and that gap is set by the *names* rather than by the bodies: a body is
`BODY_MARGIN` across the radius, but two labels need a good deal more.

## Aboard, the nav grid is phased to the tiles, and that is what makes a one-tile corridor walkable

`nav.rs` inflates every obstacle by a `BODY_MARGIN` of 23 over a 52-unit
tile, so a one-tile gap leaves a six-unit strip down its middle — narrower
than a cell. Whether any cell centre lands in it used to be luck: the
classic grid (`Nav::new`, 10-unit cells) starts from the walkable area's
edge, and the phase against the tiles drifted two units a tile, so one
corridor walked and the next did not. Aboard the room now builds
`Nav::tiled` — `CELLS_PER_TILE = 5`, origin half a cell in from the
interior's tile-aligned corner — so every tile's middle is a cell's middle
and the strip always holds one. `Room::nav_tile()` says which grid a room
gets (`None` for the classic room, which keeps its grid and its probe
seeds byte for byte; `Some(TILE)` aboard), and `Maps::new` takes it.
`a_one_tile_corridor_can_be_walked` in the world tests pins a straight gap
and an L, and fails on the old grid. That moved `REFERENCE_CHECKSUM`.

What is still not walked is a gap that is only **diagonal** — two solids
corner to corner one tile apart — because nothing of that radius fits
through it; `validate` does not know that, and the station layout's rule
about it stands. The contract at the top of `crates/shipdesign/src/lib.rs`
says the same; keep the two in step.

## A ship's doors are powered, and a lock is the only thing the nav sees

`crates/game/src/door.rs` is a designed `Door` part aboard: a sliding door
that **opens by itself** for any body within `REACH` and shuts `SHUT_AFTER`
after the doorway is clear. The part is **1×2, like the airlock** — two
tiles along the bulkhead, one deep, so one door is the whole of a two-tile
doorway — and which way its leaves slide is its **rotation**, through
`parts::door_slides_along_x` (the long side of the turned footprint: `R0`
in a bulkhead running north–south, `R90` in one running east–west). It
used to be one tile with the direction guessed off the neighbours, which
is why a door standing in nothing was drawn one way and walked another;
now `aboard.rs` and `fittings::door` read the same function, and
`fittings::door` draws in the part's own frame through `hull::Local` so it
turns with the part. A doorway in a layout is therefore **one `put`**:
`playtest_ship` and `station::build_layout` both place the door at the
run's first tile, `R90` along a row and `R0` down a column, and
`PLAYTEST_PARTS` is 594. The bathroom door is worked by hand and is a
solid when shut, with a grid per state in `Maps`; a ship has a door in
every bulkhead and a grid per combination is not a thing, so **an unlocked
door is never a solid** — the pathfinder plans through it open or shut, and
the leaves are open by the time the body arrives. `Room::doors` holds them
(`Layout::doors` from `aboard.rs`, which reads which way the leaves slide
off the rotation), `Game::update` ticks them with everybody's position,
and the room draws them (`Door` is in `drawn_by_room`).

**Locked is the one state routing has to know about**, and it costs a
rebuild of `maps`: `Game::refresh_maps` when `locked_doors().len()` changes,
`refresh_blockers` when `shut_doors().len()` does. `Nav::new` now
rasterises each solid over the cells its inflated box reaches instead of
testing every cell against every solid — the same predicate, so the grids
are identical, and a joined room's rebuild is milliseconds. A lock is
ordered but the leaves **wait for anyone in the opening** (`IN_THE_WAY`),
and nav treats the door as solid from the order, so a route planned after
the click already goes round.

The player's four words are the bathroom door's — `door::Order` Open (hold),
Close (let go), Lock, Unlock — and each is an errand through
`Switch::Door(index, order)` that walks James to the panel
(`Door::station`, a `STAND_OFF` out of the opening on his side). The host
asks `bims_hit_door()` after `Game::hit_at` said `HIT_SHIP_DOOR = 9`; the
readout's `SPOT_SHIP_DOOR = 17` carries its state through
`bims_ship_door_at(x, y)`. `a_locked_door_is_a_wall_and_an_unlocked_one_is_not`
in `crates/world/src/tests.rs` pins the routing half and
`simulation-check.mjs`'s door section the menu half. Two things that bit:

- **The camera follows James.** A pixel a harness found a door at is
  stale once he has walked to its panel — `findDoor` in the section looks
  again, by asking the room what is under each tile, not by reusing pixels.
- **A station's room draws no doors while docked**
  (`Game::set_doors_drawn(false)` in `World::join_rooms`). The joined deck
  has the same doors and draws them, and two pictures of one door would be
  a door in two states; the residents, who walk about in the station's
  room, are put on the joined deck as *visitors* (`Game::set_visitors`,
  from `Aboard::visit` every step) so its doors open for them too. Visitors
  are bodies for the doors and nothing else — not crew, not solid, not
  selectable.

## The bay is six tiles, and which side it is worked from is data

`PartKind::HydroBay` is `(6, 1)` with six use spots along its north side,
one a tray; the room's `Bay` already had `SPOTS = 6` trays, so each tile is
one of them. `Bay::at(frame, side)` lays the trays along the long axis and
stands the Bim on `side` — `Layout::bay_side`, which `aboard.rs` reads off
the part's first use spot — so a bay turned to `R90` or `R270` is six trays
down and worked from the east or the west, rather than "used from the wrong
side" like the rest. The pictures still grow plants upwards in every tray.

**The side is which edge of the frame the spot lies beyond, never which
way it is off the centre.** The first spot is at the *end* of the run, and
measured from the middle of six tiles it reads as off the end — which laid
the trays *across* the bay, six strips one tile long, with the Bim working
them from the west. `the_bay_aboard_is_a_tray_a_tile_worked_from_the_spots_side`
in `crates/world/src/tests.rs` pins a tray a tile for a bay lying and one
standing, through `Bay::station`. Fixing it moved where the Bim stands to
tend, which re-rolled the solo session in `ship-check.mjs` enough that the
crew member was off the helm at the redirect; the harness now takes the
helm again before it, the page's way.

Laying a run of six is where the corner-to-corner rule bites hardest, and
`a_station_s_rooms_can_all_be_walked_from_its_door` found both cases in one
afternoon: a run ending diagonally against the locker pinched the locker's
spot, and a run two tiles from the chamfer's corner piece — one tile
between them — left the whole strip under it as deck nobody could reach.
The station's runs start three tiles in from the west wall and stop three
rows off the south one, and the locker went to the west wall.

`nav_map_of_a_station` beside that test is `#[ignore]`d on purpose: it
prints the nav grid of one station as a digit per tile, and it is the
first thing to run when the walkability test names a tile that looks fine.

## Stew for the store is a third count, a third target, and the cook row's

`Room::stew` is pots of stew on the shelf, beside `veg` and `tofu`, and
`manager::Stock` is the three targets — `Veg`, `Tofu`, `Stew`, codes 0–2,
through `bims_target(which)`/`bims_set_target(which, n)`. The food-units
dial and its 2:1 split are gone: `Game::target`, `Game::target_veg` and
friends no longer exist, and the management tab has three inputs
(`#veg-target`, `#tofu-target`, `#stew-target`, in `#keeps`) on both pages.
**The stew target starts at nought** on purpose: a default above it would
have the crew cook the store down from the first morning and re-roll every
probe that pins where they stand.

Two chains in `task.rs` hang off it, both sharing the meal chain's steps
where they can:

- **`Kind::Batch`** is the cook row's stew errand and the hob menu's "Cook a
  stew for the store" (`Game::make_stew`, not `Game::cook`, which is
  the table stew). One vegetable, then one block of tofu, each through
  the fridge–board–knife loop — `laps(kind)` is what says twice, and
  `TakeVegetable` picks the crop off `chopped` — then the pot, and instead
  of a plate `PackStew` empties the pot into a tub (`Held::Stew`) and
  `CarryStewToStore … StowStew` puts it away. The store's door is
  `OpenStoreForStew`/`ShutStoreOnStew`, **not** `OpenFridge`/`CloseFridge`:
  the chain has already been through those on the way to the board, and a
  step that appears twice in one chain is one `rewind` and `progress_of`
  cannot tell apart.
- **`Kind::Reheat`** is what a hungry Bim does when `room.stew > 0`:
  `make_food` tries leftovers, then the shelf, then the knife. `TakeStew`
  takes the count down as the tub leaves the shelf, `TipStewIntoPot`
  fills the pot most of the way to cooked, and from `TurnStoveOn` on it is
  the meal chain.

A tub in the hands is banked like a harvest — `let_go(.., for_good, ..)`
puts it back on the shelf only when the chain is given up for good, since
a suspended chain keeps it on `Saved.main`, and `take_crew` asks
`Saved::holds_stew()`. **There is no stew row on the work list**: stew for
the store is `Job::Cook`'s, beside a meal — `work_on_offer` offers Cook
while the Bim is hungry *or* `wants_stew()` (short of the target **and**
`can_make_stew()`, so a target with nothing to make it of is not a job that
comes round every frame), and `do_some_work` has the hungry one eat first
and only a Bim that is not cook for the shelf. That means the row being on
offer says nothing about which half wants it: `scratchpad/stew.rs` reads
`wants_stew_for_probe()` for the shelf's half, because a Bim that happens
to be hungry when the probe looks would otherwise read as the shelf
asking. It is the probe: target → shelf asks → stew on the shelf → stops
at the target → a hungry Bim warms one up, measured from the moment the
warming begins, since the Bim may be halfway through a pot *for* the shelf
when hunger bites and finishes that first. Job codes 15 and 16
(`JOB_STEW`, `JOB_REHEAT`) are in `JOB_NAMES` and `ACTIVITY`; the ship's
items panel lists it under `MADE_ABOARD` with its own `data-made="stew"`
icon rule, because it is not a `ResourceId` and the manifest has never
heard of it.

The management tab's three targets are a **Target column** of the `#stock`
table, one input in the row of the thing it is a target for
(`targetInput` in `crates/app/src/crew.rs`; the ids `veg-target`/`tofu-target`/
`stew-target` and the cells `#veg-control`… survive, since the harness
types into them), and the one explanation is a `?` on the column heading.
The `#keeps` row is gone. The table has three rows — vegetables, tofu and
stew, all in the cold store; the pot on the hob has no row, a meal in the
making being the agenda's business. A target cell rings the bay or the
hob while its row rings the cold store, and `pointerenter`/`leave` do not
bubble, so `points(el, spot, back)` takes a third argument: what to ring
when the pointer leaves — the row's spot for a cell inside a row, nothing
otherwise. Without it, leaving the cell for the row put the ring out while
the row was still under the pointer, and `smoke.mjs` says so.

Never pipe `rustc` into `head`: `head` closing the pipe kills the compiler
with SIGPIPE before it writes the binary, and the "no such file" that
follows looks like a compile error that is not there.

## The helm is a job, and the room only knows it through the world

`Job::Helm` — "Controlling the ship", the sixth row — is on offer while
`Game::helm` is `Some` and nobody is *posted* within `HELM_SLACK` of the
seat, by the job or by the player's own walk to Confirm alike. The room has
no idea where the helm is or whether the ship wants anybody at it: the
world says, **every step**, through `Aboard::set_helm` → `Game::set_helm`
in stage 5 — the seat while the ship is anywhere but `Docked` or
`CastingOff` (the crew are being walked home then), `None` otherwise. The
classic room never gets one and never offers the row. Whoever takes the
job is `send_to` the seat — a post, exactly like the button — and recorded
as `helmsman`, so that `set_helm(None)` at the berth lifts *that* post and
no other; a helmsman ordered elsewhere loses the post and the record with
it, and the job comes round for whoever is free. `SPOT_HELM = 18` exists
only so the row can ring the helm's footprint (`Layout::helm`, off the
first `Helm` part); `Room::spot` never returns it, the ship's readout names
the part itself. `under_way_the_helm_is_a_job_and_somebody_takes_it` in
`crates/world/src/tests.rs` pins it — and note it has to `select_group(1)`
before `order_move`, which refuses an unselected James.

## Washing is its own need, and it is not Cleanliness

`Need::Hygiene` — "Washing" on the panel, index 5, with a trigger row of
its own in `TRIGGER_NEEDS` — drains on the waking day like food and comes
round about once a day (`SHOWERS_PER_DAY`, `SHOWER_COST`), and the errand
is `Kind::Shower`: `GoToShower`, then `Shower` for `SHOWER_MINUTES` under
`Exclusive::Shower`, restoring the need and `ch.wash(1.0)` — the whole of
the Bim's own filth off, which the basin never managed. `JOB_SHOWER = 17`.

It is deliberately **not** a clock on `Need::Cleanliness`. That one is the
*deck* — it follows the mess round the Bim — and the stages of being sick
in `filth::Ordeal` hang off it reaching nothing, so a time drain on it
would have a crew with nowhere to wash falling ill on a spotless deck.
The classic room has no shower (`Room::shower` is `None`;
`Layout::shower` comes off the first `Shower` part's use spot aboard), and
there a Bim simply goes on wanting one: `take_shower` refuses, and nothing
else comes of it. `first_station` returns `None` for a chain with nowhere
to go and `can_begin` reads that as "starts on the spot", so **`take_shower`
asks `room.shower.is_some()` itself** — any other errand whose station is
optional needs the same guard.
`a_bim_aboard_takes_a_shower_when_a_day_s_grime_has_caught_up_with_it` in
the world tests empties the need by hand rather than waiting a day for it.

## A meal is judged by the mess round the hob, not by the stains it makes

`Game::judge_the_food` runs the frame the pot finishes cooking
(`Room::update` flags it) or a bowl is filled (`FillBowl` calls
`made_a_bowl`): every tile within `GALLEY_REACH` (two) of the hob at or
below `filth::SPOILS_FOOD` is one roll at `BAD_FOOD_PER_DIRTY_TILE` (a
fifth), one bad roll is `Room::food_bad`, and a Bim `restoring`
`Need::Food` off a bad galley is `poison`ed — `Bim::poisoned_for`, two
days, the restroom drain trebled through `Needs::update`'s `purging` and
the hour of holding on skipped in `Ordeal::update`, so the need reaching
nothing *is* the accident. `What::FoodPoisoning = 31`, `Game::poisoning`
is the hours left for the health panel's `poisoning` line. A bowl is
judged because it is made in the same galley, and a reheated stew is
judged again because it is the hob it is warmed on — the shelf keeps no
record of a bad tub. `scratchpad/poison.rs` is the probe.

**`SPOILS_FOOD` is the wetting line, not `WORTH_SWEEPING`**, and that is
the trap: the cook's own chopping flicks a stain or two onto the deck on
the way (`JOB_MESSES`, `GRIME_COST`), a single stain is already past
"worth sweeping", and judged by that the galley poisoned the cook every
meal on a deck nobody had touched — `diary.rs`'s quiet four days came
back a page of accidents and `crew.rs` lost Kate. A clean galley draws
nothing from the RNG, so a clean run is unchanged; a fouled one re-rolls
every seed from the first meal.

## Aboard, a fixture is drawn inside its tile

The room's pictures were drawn at the classic room's offsets, and on a
ship's one-tile part they ran out of the tile: the pot overhung its
neighbours, the fridge's shelves ran out through its side, the chair was
wider than its tile and the dishwasher was only its door. The rule now is
that everything the room draws for a part is laid out **off the part's
rect**, not at fixed offsets: `Room::hob_scale` fits the burner inside a
one-tile hob and the pot is `POT_SIZE` of that; the fridge's shelves and
stock are placed in fractions of its interior; `CHAIR_SIZE` is 48×42 with
the backrest a unit inside the tile; and `Dishwasher::body` is the whole
tile aboard (`None` in the classic room, where the counter is the body).
`ship-layout.mjs simulation` rendered to PNG is how to check — there is no
assertion that can see a picture spill.

The shower has a menu, like the pan: `HIT_SHOWER = 10` → "Take a shower"
(`Game::can_shower`, `Game::take_shower`, `Game::shower_held_by`), and
`SPOT_SHOWER = 21` names it in the readout and rings it. The simulation
harness has a section for it. Codes 19 and 20 are the bench and the suit
locker, added beside these; `SPOT_NAMES` in `crates/app/src/names.rs` is indexed by code,
so every one of them needs its entry there in order, whoever adds it.

## The hob is one burner, left of centre

`Room::burner` is where the pot stands and the only ring that lights. It
sits at `centre - 30`, where the left of the old pair was, because the
cooking station is measured off `pot_pos` and centring it would move where
the Bim stands and re-roll every probe seed for a picture.

## The chopping board has two sides, and the tofu's is the left

`Room::board_sides` is two `room::Cut`s — what was put down, how much is
still whole, how many pieces it is in — the left one first. `put_on_board`
puts tofu on the left and a vegetable on the right, and the second thing
chopped for one meal takes whichever is free, so a shelf stew (vegetable,
then tofu) and a table stew (two vegetables) both end with both sides full
and everything goes into the pot together: `GatherSlices` takes
`board_pieces()` into `Held::Chopped { rounds, cubes }`, which is drawn as
a handful of both, and `clear_board`. `chop(done, of)` works on the side
the last `put_on_board` named. `Held::Fork` is the fork; it used to be
`Held::Slices` standing in for one. `target/probes/layout board` is the
picture, caught mid-stew with the tofu half cut.

## A berth has an axis

`Berth::lying` is a bunk turned a quarter — a frame wider than tall — and
everything about the bed goes through `Berth::at(across, along)`,
`size(across, along)`, `breadth()` and `length()`: the picture, the pillow,
where the Bim lies (`lie_pos`), which way it faces lying there
(`lie_facing`, which `task.rs` reads instead of the old `FACE_PILLOW`), and
where it stands to get in. For a bed standing up the arithmetic is the
classic room's to the number, so no probe seed moved; a bed lying down is
the same bed on its side rather than a blanket drawn off the end of it.

## A frozen walk is caught, and the two ways it happened

`Game::unstick` is the watchdog: a Bim marching with a destination that
has moved less than `STUCK_STEP` a second for `STUCK_AFTER` is stopped
dead, and nothing else aboard looks like that. It plans a fresh route to
the same destination from where the body is; if there is none, it clears
the walk (`follow_path(Vec::new())`, so `arrived()` comes true and the Bim
can start something else) and `interrupt`s the chain onto the queue,
where `queue_ready` holds it until the way is open again.

The two cases the social probe found, seeds 7 and 1 — the ones the notes
above said "are left":

- **The turn is an arc.** A body turns at `TURN_RATE` while moving at
  `MARCH_SPEED`, a radius of some seventeen units, so a walk that starts
  facing the wrong way leaves the line it was given by up to a body's
  width. Off the line against the inflated face of the table with the
  next waypoint straight through it, the push-out undid every step
  exactly: Kate stood at (315, 320) for days. The replan takes her round.
- **The heads' door shut across a walk into it.** `ShutDoorBehind` on the
  other Bim's chain is by hand, and `tick_door_closer`'s crossing check
  only holds the *closer*. James, sent by the sweep to a tile inside while
  the door stood open, pressed against the panels all night while Kate
  slept — and with the walk never arriving, `consider_errand` refused him
  a bed. Now the chain is put down and he goes to bed; the sweep resumes
  from the queue when the door is open again, re-picking its tile.

Both were the company bar collapsing to nothing in `social.rs`, not
anything about company: the drain runs while the other one is asleep or
walled in, and a Bim that cannot move cannot be talked to. A need bar that
empties on one seed is a frozen Bim until proved otherwise.

## Outside, the body is on a second grid, and a rock is mined straight on

A walk outside (`Kind::Eva`) is a walk now, not a clock: `StepOut`'s
`leave` stands the body a tile beyond the collar (`Character::go_outside`
— standing, no longer seated, with the suit on and `Held::Pick` in the
tool hand, both off again in `come_inside`), and from there `PickRock` →
`WalkToRock` → `Mine` go round until `next_rock` finds nothing, then
`WalkToPort` → `StepIn`. `Kind::steps()` walks that loop **once**
(`(Kind::Eva, Mine) => WalkToPort`), or `rewind` and `progress_of` never
return; `next_step` is what goes round.

The grid it walks is `Maps::outside`, `Nav::outside`: one cell a **tile**
(not five), `OUTSIDE_RADIUS` (100) tiles every way about the body, over
`Room::hull` (every structure tile, from `aboard.rs`) and `Room::rocks`
(what the world handed over in `Game::set_eva`). `Game::refresh_outside`
at the top of every `simulate` rebuilds it when `Room::rocks_version`
moves or the body is `OUTSIDE_RECENTRE` tiles off its middle, centred on
whoever is out or on `Room::outside` when nobody is, and drops it when
there is no outside and nobody in it. Everything that plans or moves a
body asks which grid by `Character::is_outside()`: `Task::enter` and
`unstick` through `Maps::for_body`, and `Game::move_body` — the **one**
call that moves a Bim now, replacing four `character.update` sites —
hands an outside body the grid's span and `outside_blockers` instead of
the deck and the furniture, so nothing shoves it back through the hull.
`crowding` skips a body outside; `order_move` refuses one (the walk
brings it in).

Two things that bit, in one afternoon:

- **`PickRock` is a step of no length, and it is not decoration.**
  `Mine`'s `leave` writes the rock on `Room::mined`; the world takes the
  tile out *after* the room's step and hands the rocks back on the next
  one, and the grid is rebuilt at the top of that step. Without a step
  between, `next_rock` ran on the old grid with the mined tile still
  solid, and the rock behind it — whose stand spot *is* the mined tile —
  read as unreachable, so a dig stopped after every tile. `Mine`'s
  `leave` also takes the rock off the room's own `rock_targets` and
  `rocks`, or the same rock is picked again before the world has spoken.
- **A rock is mined from the tile beside it, four ways, never a corner.**
  `next_rock` tried eight neighbours first. A rock reachable only
  diagonally was mined from its corner, and the pocket it left was a
  tile the grid rightly refuses to squeeze into diagonally between two
  rocks — so the rock behind it was marked for nothing and the walk went
  home. Straight on, the tile mined is always the tile stood in next.
  `Game::rocks_reachable` (from the port, once per rebuild, since a route
  to every mark is a search of the whole grid when there is none) is
  what keeps `Job::Mine` from being offered to marks nobody can reach,
  and what the Actions tab says are "out of reach".

A walk put down out there — hunger, an order — restarts its outside half
from the gangway: `rewind` sends the `outside_half` steps back to
`GoToGangway` and `resume` drops nothing back in, so it goes out again and
picks its rock afresh rather than dropping into `Mine` at a rock it is no
longer beside. `walks_done` is counted at `StepIn`'s **enter**, the same
step the world reads it. `JOB_EVA` is still 19; "Mining outside".

## Plates are counted, and the count is the drawer

`Room::plates` is the clean plates in the chopping board's drawer —
`START_PLATES` (20) in every room, `PLATE_DRAWER` (40) at most; nothing on
a manifest carries plates, so a designed ship starts with the same twenty
as the playtest one. `TakePlateAndSpoon` and `TakeBowl` take one
(`Room::take_plate`, saturating — an empty drawer serves an imaginary
plate rather than starving anybody), the rack gives them back when a
dishwasher cycle finishes (`Dishwasher::update` now *returns* the washed
count and `Room::update` puts it in the drawer), and the two places a
plate used to vanish put it back instead: `let_go(.., for_good, ..)` with
one in hand, and `reset_for_cooking` with one left on the counter. A plate
on the table stays out until `ClearTable`. The open drawer draws a pip per
plate up to what its face has room for; `Game::plates` and
`plate_drawer_capacity` are the readout (the Aboard panel's "Drawer" row,
and the board's spot state).

## A site is worked from the deck or from outside, and the room is relaid under the crew

`Kind::Haul { site, outside }` and `Kind::Build { site, outside }` are
the construction chains — `GoToShelf → TakeMaterials → CarryToSite →
DropMaterials`, and `GoToSite → Construct` — with, for a site beyond the
hull, the walk outside's own steps either side (`GoToSuitLocker` … `StepOut`
before, `WalkToPort` … `PutSuitBack` after). `Kind::fork` is where every
chain's turn-offs live now, shared by `Kind::steps` and `Task::next_step`
so the agenda and the chain cannot disagree; `outside_half(kind, step)`
is the generalised Eva rule for `rewind` and `resume`. The world says
what there is to build (`Game::set_build_orders`, `Room::builds`, one
`Build` a site with its tiles in room units, `haul` and `minutes`) and
who may suit up (`Room::suit_ok`); the room reports on `Room::picked`,
`dropped`, `returned` and `built`, and **moves no materials** — a crate
in the hands (`Held::Crate`) is a picture, the count is the world's.
`JOB_HAUL = 20`, `JOB_BUILD = 21`; `Job::Build` is the ninth row, and
`Job::Haul`'s row now starts an errand of its own.

- **Inside or outside is decided by `task::site_stand`**, one place: the
  nearest tile beside the footprint — four ways, never a corner, like a
  rock — that the grid has a route to, else a tile *of* the footprint
  (plating goes under the Bim's own feet). Asked of the deck's grid from
  the Bim, and failing that of the outside grid from `Room::outside`, in
  `Game::site_reach`; asked again as the walk is entered, from wherever
  the body is on whichever grid it is on. A site the world has dropped
  has nowhere to stand and the walk is `blocked`, the way a shut door
  blocks one. `nearest_shelf` is the other end of a haul, and both are
  checked before the errand is offered, since `first_station` has no
  answer for a chain that picks its spot on entry.
- **`refresh_outside` keeps the outside grid while there are sites**, not
  only while there is a mining site — the decision above needs it.
- **`go_outside` takes `pick: bool`**: a walk to mine takes the pick, a
  trip to a site does not, and `StepIn` counts a walk (`walks_done`) only
  for `Kind::Eva`, or the world says "back from outside with nothing"
  after every build.
- **A load given up for good is handed back** in `let_go`: a `Haul` past
  `TakeMaterials` pushes `Room::returned`, whatever the hands happen to
  show — a walk resumed after an interruption starts with empty hands and
  `CarryToSite`'s `enter` puts the crate back in them, so the picture is
  not the record.
- **`Exclusive::Site(id)`** is one pair of hands per site aboard;
  outside chains take `Exclusive::Airlock` like a walk.
- **`Room::relayout` and `Game::relayout`** lay the room out again under
  the crew for a design that changed: geometry from the new layout,
  state kept — `Filth::resized` carries the dirt across by position, the
  doors match by opening, the beds by frame (index is identity, and new
  ones go on the end), the bay and the heads only if they moved. The
  grids and blockers are rebuilt and `rocks_version` bumped for the
  outside grid; a Craft chain whose bench index now names a different
  bench is abandoned. `from_layout` and `relayout` share `bed_side`,
  `layout_chairs` and `galley_faces` so the two cannot lay the board out
  differently. `Layout::shelves`/`Room::shelves` (every `Shelf`, worked
  from its use spot) is what a haul fetches from.

## Sight is traced on the tile grid, shared, and the fight reads it

`crates/game/src/sight.rs`. Every Bim sees all the way round; what stops
its eyes is what stands in the way, and `Sight` works that out by tracing
a straight line from each crew member to the middle of every tile of the
room's grid (`TILE`, 52) — the grid-line-by-grid-line traversal, so a
diagonal gap between two opaque tiles is stopped — and marking the tile
seen when nothing opaque is crossed before it. The tile the line stops at
is seen too, so a wall is seen from the room it walls. Opaque is
`Layout::opaque` (`aboard.rs`: every no-deck tile in the deck's box and
every Object-layer part whose `PartDef::blocks_sight` says so — the rule
is in `shipdesign::parts`, and it is the movement rule minus the low
furniture) plus everything outside `interior`, plus what is shut **right
now**, added at every trace: `Room::shut_leaves(bodies)` is every powered
door whose leaves are not open — locked or not; `shut_doors()` is the
pathfinder's question, locked *and* shut, and asking that let sight
through every unlocked door — and every airlock (`Layout::airlocks`)
with nobody within a door's `REACH` of it, since the room has no leaves
for an airlock and the ship painter's `airlock_ajar` is a picture; plus
the heads' `closed_door()`. The bodies are the crew and the visitors, as
for the doors themselves. The classic room's opaque is the heads' walls.

- **It is recomputed only when it could have changed** — an eye crossed a
  tile, or the set of shut doors differs — and once a frame, from
  `Game::render`, not once a step. A whole trace of a joined room is about
  a hundred microseconds. A probe that wants `Game::seen_at` without a
  picture calls `Game::observe` itself.
- **The mask is shared** by construction: one trace per eye, OR'd. Every
  Bim in `Game::bims` is an eye, which aboard is the crew and nobody else.
- **The fog is drawn over `Layout::hull`** (every structure tile) and,
  in the classic room, over the whole box; unseen tiles are merged into
  row runs and stacked into rectangles so it is a few dozen shapes. It
  goes over the fixtures and under the night wash, the highlight ring and
  the marquee.
- **`sight::Fog` says whose eyes a room is drawn through.** `Crew` is the
  default: the crew's trace, every body drawn. `All` is a room looked at
  from outside — a station's while the ship is merely alongside — with
  everything fogged and nobody drawn. `None` is a station's room under a
  joined deck: no fog of its own (the joined room's covers both hulls) and
  a body drawn only where the world said it is in view
  (`Game::set_seen`, from `Aboard::seen` every step in `World::visit`).
  `Game::body_seen(who)` is the one question, and
  `Session::resident_on_screen` asks it so a name never floats over a
  body that is not drawn. `sight_is_traced_and_stops_at_walls_and_shut_doors`
  and `docked_the_crew_see_what_is_in_view_and_the_station_s_people_only_there`
  in `crates/world/src/tests.rs` pin all of it.
- **A Bim against a wall peeks round it.** `Sight::eyes_from(p)` is the
  body's own eyes plus, for every opaque 4-neighbour of its tile, the free
  tile either side of it *along* that wall, each restricted to tiles past
  the wall's line (`Eye::admits`: `(tile − body)·wall ≥ 1`). `observe`
  traces every eye of every body; `sees_from(body, target)` answers for
  one body and one target and says *which* eye saw it, and a shot is
  fired from that eye — so a peeking Bim shoots from the peek and not
  through the wall. A tile back from the wall there is no peek and the
  trace stops at the corner's edge. `a_bim_against_a_wall_peeks_round_it`
  in the world tests is the layout the rule was asked for with.
- **Whose a tile is decides its fog.** `sight::Stance` — Friendly,
  Neutral, Hostile — on the room's own tiles (`Game::set_stance`) and on a
  foreign box (`Game::set_foreign`, the station's box on a joined deck;
  both kept on `Game` and put back on the fresh grid at `relayout`). A
  friendly tile unseen is the semi `FOG`; a stranger's is `FOG_BLACK`,
  or `FOG_GREY` within `RING` (3) tiles of a seen one — `near`, dilated
  in two passes after each trace and **sticky**: explored stays grey.
  `draw` is three passes of the run-merging, one a veil; an opaque
  rectangle is grown by `OVERLAP` so the feathered seams between black
  runs do not show as lines. `Game::veil_at` reads it for the probes.
  Under `Fog::All` a stranger's room is black entirely, which is what a
  hostile station alongside looks like; beyond the residents' range the
  ship painter keeps drawing a stranger as its far plate
  (`HULL_UNKNOWN`) so nothing is revealed at fifty tiles.
- **A body on somebody else's deck stays drawn for `SEEN_FOR`** (2 s)
  after the world last said it was in view: `set_seen` re-arms
  `seen_for` per body, `body_seen` reads it, `simulate` counts it down.
- **The fight is the first thing to read sight.** See the next section.

## The fight: targets in, hits out, and the room never decides who is an enemy

`crates/game/src/combat.rs`. Every `Bim` has a `gear: Gear` — three armour
slots (`ArmourKind`, an **empty** enum so the name table pins to one
entry, the empty slot) and a weapon slot, `Gear::issued()` a
`WeaponKind::LaserPistol` — plus `reload` and `hit_flash` clocks.
`WeaponKind::stats()` is the one table: range and speed in **tiles**,
`accuracy` the odds at `ACCURACY_RANGE` (10) tiles, `hit_chance(tiles) =
accuracy ^ (tiles / 10)`, `dps()` = rate × damage. The app names kinds
through `WEAPON_NAMES`/`ARMOUR_NAMES` and prints the stats itself.

- **Combat mode is being recruited with a weapon**, and nothing else:
  `Game::tick_combat` (after the crew have moved, in `simulate`) sets
  `Character::armed` — recruited, alive, armed, not outside, seated,
  napping or out cold — which is drawing only (the pistol in the right
  hand). An armed Bim asks `Combat::aim` for the nearest target in range
  that `Sight::sees_from` says it sees, faces it, and fires when `reload`
  is out — not while walking. The crew's goes nowhere of its own: a
  recruited Bim stands where it was put. **Recruiting is per Bim**:
  `pump_queue`, `consider_errand` and `flee_filth` ask
  `bims[who].character.is_recruited()`, not `Game::is_recruited()` (the
  player's) — they used to ask the player's, and recruiting James froze
  Kate's errands with his.
- **The targets are the world's.** `Combat::targets` is
  `Vec<Option<Vec2>>`, index for index with the other room's people
  (`None` for one down), set every step by `World::visit` through
  `Aboard::hostiles` → `Game::set_hostiles` while the station is hostile
  — on **both** rooms, each told where the other's are — and cleared
  otherwise.
- **A bolt flies in one room only.** The crew and a station's people are
  two rooms, and a bolt that flew in both would be two bolts. So a
  friendly bolt (`hostile: false`) looks for the targets, a hostile one
  for this room's **own bodies** — `Combat::step(dt, sight, bodies)`,
  `bodies[i]` Bim i's position while alive and on the deck, conscious or
  not — and a room whose bodies are hostile (`Game::set_hostile_bodies`)
  fires **no bolts**: its people's shooting is a `combat::Shot { from, at,
  weapon }` on `Combat::shots`, `Game::take_shots()`, which the world maps
  into the crew's room and fires there as a red bolt through
  `Game::enemy_fire(from, at, weapon)` → `Combat::fire(.., true)`. A
  friendly bolt landing pushes a `combat::Hit { who, part, damage }` onto
  `hits` (`Game::take_hits`, for the world to carry to the residents'
  room's `Game::wound`); a hostile one onto `Combat::wounds_taken`, which
  `tick_combat` drains into `Game::wound` at once and keeps a copy of for
  `Game::take_wounds_taken()` — the world logs `CrewHit` off that, it does
  not apply it. The **part** is rolled the instant the bolt reaches the
  body, `Part::hit_by(rng.unit())` off the combat stream.
- **`Game::wound(who, part, damage) -> bool`** is `Health::shot` (the
  damage off that part, a wound opened on it; `true` when a leg went), the
  flash, and `Character::set_wounds` — a `BLOOD` blotch on the head, the
  coverall's middle or the boots while that part bleeds, standing or
  lying. `part_health`, `blood`, `wounds(who, part)`, `bleeding`,
  `legs_lost`, `is_unconscious` are the readouts. `tick_bim` multiplies
  the pace by `Health::pace()` (the legs left, and the blood under half);
  after the death check, `Health::unconscious()` differing from
  `Character::is_unconscious()` is `knock_out(out)` — going out
  `interrupt`s the errand onto the queue first, and the frame stops there
  for that Bim, like a nap, until the blood comes back. Out cold it is
  drawn by `draw_lying`: the fallen figure in the **live** colours with a
  slow breath and no Zs. `Bim::tick_drips` lays a `Drip` on the deck every
  `DRIP_EVERY / wounds` seconds while it bleeds and lives, scattered ±10
  off the body from the room's stream (a bleeding Bim is a fight, and no
  seed-pinned probe has one), living `DRIP_LIFE`; `render` draws them
  under the bodies, fading over their last third.
- **The enemy's tactics.** A room with hostile bodies **and** a `Some`
  target is **at war** (`Game::at_war`, `muster`): every living body is
  `set_recruited(true)` with its errand `interrupt`ed (queue kept) and its
  post dropped; targets cleared is `set_recruited(false)` and the queue
  picks up. At war each armed body runs `plan_stand` every `PLAN_EVERY`
  (1.5 s, its own `plan_wait`): `combat::Tactics::stand(sight, nav, from,
  targets, stats)` scores a tile-spaced lattice of free cells within the
  weapon's reach of every target (`Nav::free_cells_within(centre, radius,
  spacing)`, kept to `can_reach`) plus the spot it stands on — **cover**
  (the body's own eye blind to the target, a peek beside a wall seeing
  it: 2000 + distance) over the **open** (the body's eye seeing it: 1000
  + distance), less 4 a tile of walking, plus 60 for the spot it is on
  so equals do not have it dithering — and marches there when it is more
  than a tile from where it is already going (`follow_path`, no marker: a
  ping through the fog). It shoots the moment `aim` sees a target,
  mid-plan or not, and not while walking, like the crew. `Eye::is_peek`,
  `Eye::admits`, `Sight::tile_of` and `Sight::clear_line` are public for
  it. Enemies know where the crew are — the world tells them — and need
  no line of sight to *know*, only to shoot.
- **The combat RNG is its own stream** (`Rng::new(seed ^ 0xC0BA7)`): a
  fight rolls nothing the rest of the room rolls, so a probe's seed is
  the same whether or not somebody shot.
- **`Character::hostile`** rings a body in red; the world sets it on every
  Bim of a hostile station's room (`Game::set_hostile_bodies`).
- **`HIT_BIM = 11`**: `Game::hit_at` answers it, after noting the
  fixtures and before asking the room, for a click within `pick_radius`
  of a living Bim, and `Game::hit_bim()` says which — the bandage menu.

`combat::tests` pin the tactics on a hand-laid walled room and which
bodies each kind of bolt finds; `game::tests` pin the war, an enemy's shot
wounding, the knock-out and `HIT_BIM` in the classic room.
`a_recruited_bim_shoots_the_enemies_it_can_see_and_they_are_hurt` in the
world tests stands James beside a resident of a station made hostile and
runs until `EnemyDown`; `BIMS_FIGHT=1 bims combat` is the picture
(`World::stage_fight_for_probe`).

## "Can it get there" is two lookups, and a step must never search the grid for a no

`Nav::can_reach(from, to)` compares the region labels of the two cells —
`Nav::label` floods every patch of free cells once when the grid is
built, stepping by `steps_from`, the one neighbour rule the search and
the labelling share — and `Nav::path` refuses a pair in different patches
before searching. Every yes/no reachability question in the room asks
`can_reach`: `Game::can_reach`, `can_reach_through_door`, the seat pair in
`chat`, `nearest_shelf`, `site_stand`, `next_dirty`, `reachable_rocks`
and the spatter's filter. `path` is for a route that will be walked.

Why it is a rule and not a tidy-up: `work_on_offer` runs for every idle
Bim every step, and it asks about every shelf, every site, the bay and
the benches. When those asked `!nav.path(..).is_empty()`, an A* that
finds nothing walks every cell of its patch first — 40 000 cells on a
station's grid — and a station's room cost **900 µs a step**, which at
24x is the whole frame. `haul_on_offer` also looks for a site wanting a
load *before* it looks for a shelf, for the same reason. A docked step is
about 7 µs now; measure a new per-step question with the scratch bench
before adding it, and if it is a route, ask whether it is asked only when
the answer is used.

## Every bay is worked; a second table or a lone basin is drawn as itself, standing still

`Room::bays` is every bay aboard, the layout's own first and
`Layout::more_bays` after it, each with the side it is worked from.
`Kind::Tend { bay, spot }` names the bay; `Game::bay_work` asks every
bay in order and `tend_bay` walks to the first tray wanting a hand;
`update` grows them all against the one manager target. A click is
`Room::bay_at` → `HIT_HYDRO`, with the index kept as `Game::hit_bay`
exactly as `hit_door` is, and every `hydro_*` accessor takes the bay, so
the menu on a bay the crew built has that bay's switch and standing
order. `relayout` keeps a bay's trays by frame, like the beds.
`every_bay_aboard_is_worked_and_has_its_own_menu` in the world tests
pins it. **On a joined deck the station's bays are not the crew's**:
`Aboard::leave_the_station_s` drops every `more_bays` and `extras` entry
standing inside `Aboard::station_box` — at the join and at every
`Aboard::relayout` after it, since a part built relays the whole deck —
because the residents work and draw those in their own room, and a still
painted over a live fridge is two pictures of one door again.
`a_bay_the_crew_build_is_a_bay_like_the_first` lays a bay out as a site,
has the crew build it, and clicks it.

Every worktop, hob, cold store, dishwasher, locker, shower and toilet is
worked now — see the picks section above. What is left for the room to
only *draw* is a second table (every chair is seated already) and a basin
no toilet pairs with. The room used to leave every second fixture to the
ship painter, which has no picture for
those kinds and drew a coloured block: a station's hydroponics was one
bay of trays and three green slabs, and a hob the crew built was a
square. Now `layout_of` lists them as `Layout::extras` (a `room::Still` kind and the
rect), `Room::stills` (`Stills::from_extras`) holds them ready — a `Bath`
for a basin, a rect for a table — and
`Room::draw_stills` draws them after the fixtures, in `draw` and in the
designer's `draw_fixtures` alike. `drawn_by_room` now returns **every**
part of those kinds, so the painter draws none of them.

The pictures are functions of the fixture's index or rect:
`draw_worktop(list, i)`, `draw_hob(list, i)`, `draw_fridge(list, i)`,
`draw_locker(list, i)`, `draw_table_top(list, rect)`, with
`burner_of`/`hob_scale_of`/`knob_of` as functions of the hob's rect. A new picture for a fixture
goes in as a function of its rect first, and the room's own state on top.
