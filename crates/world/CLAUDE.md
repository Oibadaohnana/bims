# The world

Notes on `crates/world` — one star system, the ship in it, the stations, the
crew aboard and the one clock. The room it steps is `crates/game/CLAUDE.md`;
the painter and the page, `crates/ship/CLAUDE.md`.

> **Since the port to Bevy (September 2026):** the browser host is gone.
> Every name table this file points at in `web/*.js` is now
> `crates/app/src/names.rs`; the pages are the screens in
> `crates/app/src/screens/`; the `bims_*` and `ship_*` exports are methods on
> `bims::game::Game` and `ship::Session`. The node harnesses
> (`scratchpad/*.mjs`, `smoke.mjs`, `stub.mjs`) are gone with the host — a
> check this file says lives in one of them lives nowhere now unless it says
> so in `crates/app`'s tests or `./check smoke`, and is worth restoring as a
> unit test if the thing it guarded moves again.

## A recipe is a row, a bench is a solid, and the hold is the world's

`shipdesign::recipes::RECIPES` is the table: a station `PartKind`, inputs,
one output, minutes, and `vents` — the smelter may lose mass and nothing
may gain it; `every_recipe_holds_together` pins the arithmetic against
the resource table, which is why `Emitter` weighs 16 and `Components` 2.
`recipes::at(kind)` is how anything asks "is this a bench"; `aboard.rs`
uses it to build `Layout::benches` (a `room::Bench`: the part's code, its
frame, its use spot) — the **only** `shipdesign` use, still in `aboard.rs`.

The chain is `Kind::Craft { recipe, bench }`, two steps, `GoToBench` and
`Work`, with the length riding in `rest_minutes` the way a doze's does,
because the room has no recipe table. **The room moves no cargo.** `Work`'s
`leave` pushes the recipe onto `Room::crafted`; `World::step` drains it
with `Game::take_crafted` after the room steps and moves the hold —
`finish_craft`, which re-checks `can_make` and emits `Crafted` or
`CraftLost`. Going in, the world hands the room `Vec<Order>` every step
(`Game::set_craft_orders`, from `World::craft_orders`: target unmet, inputs
aboard, room for the output net of the inputs, `powered(station)`, one
order per bench of that kind). `Job::Craft` is offered while any order's
bench is free — `Exclusive::Bench(index)` — and `craft_on_offer` takes the
first, so recipe order is preference order.

Targets are `World::craft_targets`, in `world_checksum`, set by
`Command::SetCraftTarget` (`net.keep` → `Command::SetCraftTarget`) and clamped to
the class's capacity. Nought at the start, for the same reason as the
stew target. The items panel's `.keep` box is the control and
`recipeLines` is what reads the `ship_recipe_*` exports — all seven of
them, or the boundary check names the unused one. `JOB_CRAFT = 18`,
`SPOT_BENCH = 19` (ringing only; the readout names the part), and
`WORK_NAMES`/`WORK_SPOTS` in `crates/app/src/crew.rs` grew a row.

`PartKind::Armoury = 34` is the third bench, and the one that is also a
container (`Storage::Locker`, four); `Handgun = 9`, `Vest = 10` and
`Medkit = 11` are what it makes, all in the locker class. **A held item
is a resource in the locker class** — a count, no per-item state — until
something needs a charge or wear; the suit was the first and these are the
next three. `a_target_for_a_handgun_runs_the_whole_chain_from_the_hold` is
the user's original example run end to end.

The playtest ship has a smelter and a workbench aft, both `R180` so they
are worked from the row forward of them (the row aft is the stern), a
second shelf, and 40 ore; `REACTOR_OUTPUT` went to 120 so the ship as it
comes is not short. `a_target_for_metal_has_a_bim_smelt_ore_at_the_bench`
runs the whole seam natively and `simulation-check.mjs`'s keep section
from a click.

## The outside is a place, and the ship's tile grid is its grid

A walk outside — `Kind::Eva`, from `GoToSuitLocker` to `PutSuitBack` —
used to hold the body at a spot beyond the collar for an hour and a half
and read `worldgen::belt_yield`. Now the belt is a **mining site**,
`crates/world/src/mining.rs`: hold station at a belt and `settle_site`
(stage 4's last word in `World::step`) lays `MiningSite::generate` out
**in the ship's design frame** — every rock a `RockTile { x, y, kind }`
on the same integer tile grid the hull is on, negative and past the build
area both — from `Purpose::MiningSite` and the belt's id, clear of
`hull_box()` by `CLEARANCE`. Once per belt: `World::sites` keeps every
site the ship has held at, mined tiles and all, in belt order, and it is
in `world_checksum` whole. That frame is the whole trick: a suited Bim
walks the outside in room units like the deck, the painter turns the
rocks with the hull (`world_paint::rocks`, and `local_node` skips the
belt's icon-rocks while a site is laid out), and a click on a rock is
`Game::tile_at` like a click on the deck.

Skin and core: `depths` floods each blob from the outside in and a tile
`CORE_DEPTH` (3) or deeper is the ore — `Rock::Iron` on most, `Galvum` on
`GALVUM_SHARE` (a tenth: the whole tenths for certain, the remainder as a
chance, so a site of twelve has one galvum asteroid for certain). What a
tile yields is `mining::yield_of`: two `ResourceId::Rock` (the new
resource, id 12, €2, shelf, sold nowhere — that moved `CARGO_SLOTS` to
13 and re-pinned `PLAYTEST_HASH`, both `REFERENCE_HASH`es and
`REFERENCE_CHECKSUM`), two ore, or one galvum.

Nothing is mined that is not **marked**: `Command::MarkRock { x, y }`
toggles, `Command::ClearMarks` clears, `set_off` clears through
`leave_site` (by the *frame*, not the state — the ship is `Travelling`
by then) and recalls whoever is out (`Game::recall_outside`, which
abandons the walk for good and drops a queued one). The room is handed
the site every step as `bims::game::Eva { allowed, targets, rocks,
version, tile_minutes }` — the marked tiles' middles, every rock as a
`Rect`, and `site_version`, which moves on every mark and every mined
tile so the room rebuilds its outside grid then and only then. `Mine`'s
`leave` puts the rock's middle on `Room::mined`; the step drains it with
`Game::take_mined` **before** `take_walks`, `finish_tile` takes the tile
out, adds what fits to the shelf and tallies it, and `finish_walk` says
the tally as one `Mined { rock, ore, galvum }` when the Bim is in (the
walk is counted at `StepIn`'s *enter*, so the same step sees it).

`hold_at_belt_for_probe` is how a test — and `BIMS_AT_BELT=1` in the
app — gets there without flying: undock, put at the belt, settle the
frame, lay the site. `marked_rocks_are_mined_on_foot_and_what_they_yield_lands_on_the_shelf`
marks a straight dig from a core tile out to the skin and runs it;
`a_mining_site_is_laid_out_about_the_ship_at_a_belt` pins the clearance,
the depth rule, determinism and the galvum share over two hundred belts.

The room's half — the outside grid, `next_rock`, why a rock is mined
straight on and never from a corner — is in `crates/game/CLAUDE.md`.
Stage 8 is as it was: `World::health` doses a body at `SUIT_INTENSITY`
while `Game::is_outside(who)`, `EVA_DOSE_LIMIT` keeps a dosed Bim in and
sends one out there home from the next rock (`eva_allowed` on the room).

## The design phase, then the game, and one clock in it

`web/ship.html` is two screens and one wasm. The last Accept settles the ship
**and** opens the world — one event, in `Session::accept`, because two exports for
it would be two things that could disagree about which ship got handed over.
`ship_phase()` is still the only way to ask which half you are in.

After that there is exactly one loop: `World::step`, which advances
`STEP_MINUTES` and nothing else. Its order is the contract and it is written
out in the function:

1. the commands stamped for this step, in arrival order;
2. the clock;
3. flight;
4. discovery and the local frame;
5. **crew** — the room aboard, stepped;
6. **power** — the reactors against the wired consumers, into the batteries;
7. **construction** — what the crew did at the sites, moved through the hold;
8. **health and radiation** — each body dosed or sheltered.

The last two were extension points, and both are filled now; whatever
joins them goes in *there*, on *that* clock. A second clock or a second loop is two
simulations that will disagree, and the failure reads as a ship in two places.

`crates/app/src/screens/game.rs` turns real time into steps with an accumulator — `dt *
ship_steps_per_second() * multiplier` — and never into bigger steps. Same rule
as the room, same reason: a 24x step would move the ship several times its own
length and skip straight past its own braking phase. `MAX_STEPS_PER_FRAME` has
to stay at or above `TOP_SPEED * 60 / 30` or the top of the range quietly stops
being reachable.

## The anchor is stored and the position is derived

`Ship::anchor` is where design tile (0, 0) sits in the system;
`Ship::position()` is the **centre of mass**, worked out from the anchor and
the heading through `flight::angle::rotate_design`.

That way round on purpose. `on_ship_changed` has to promise that welding a
shelf to the stern does not move the *hull*: if the position were stored and
the anchor derived, every wall anybody built would shove the whole ship
sideways through space. Docking is the one place the ship is *put* somewhere
rather than flown there, and it is `set_position` doing it.

`rotate_design` is **its own inverse** — it is a rotation composed with the
flip between the grid's y-down and the system's y-up, and a rotation composed
with a reflection is a reflection. `unrotate_design` exists so call sites read
the way they mean and calls straight through. Do not write a second one.

## A speed request is the one command that does not wait for a step

Everything a player asks for is queued and applied at the top of the next step
— which is what makes the stamp `crates/app/src/screens/game.rs` puts on it mean anything.
**Except the speed.** At a pause no steps are taken at all, so a queued speed
change would never be applied and the pause could never be lifted.
`World::request_speed` is the door, `Command::SetSpeed` goes through the same
door, and it is safe to be the exception because it changes nothing a step
would have changed — only how fast the caller is expected to turn the crank.

## A redirect is a stop and then a trip, and it needs no mechanism of its own

A Confirm while the ship is under way aborts the current plan to rest, and the
moment that abort *ends* the pending target is picked up and a fresh plan is
made from where the ship stopped. Two plans, one after the other, and the ship
is genuinely at rest in between. Only the **latest** confirmed target is kept.

The consequence that surprised a test: a redirect one step after departure has
no speed and no spin to take out, so the stop is of no length and the second
trip is already under way in the same step. That is the redirect working, not
a shortcut round it.

## A station is a place, and the ship docks beside it

`crates/world/src/station.rs` turns every `StationBlueprint` of the system
into a `ShipDesign` through `apply` — the same parts, the same rules — sized
by kind (26 to 40 tiles) and dressed by `map_seed`, with the port in the
west skin and the array in the north. `World::stations` holds them from
`World::start`; `Station::all_of` is the only place they are built, and
`station::layout` **caches by (kind, seed)** because a layout is two
thousand `apply`s and the world test suite went from two seconds to a
minute before it did.

Five things that hang off that:

- **Docking is airlock to airlock, outside the hull.** `shipdesign::dock::port`
  is a design's first airlock and the side of it with nothing beyond, and
  `Station::berth` turns the ship so its port faces the station's and puts
  the two outer faces on one point. `World::dock_at` is the one place the
  ship is set down (with `World::start`); `target_position` of a station is
  the *berth*, so a trip ends outside the station rather than at its
  middle. `the_ship_docks_airlock_to_airlock_outside_the_station` checks
  every ship tile is clear of the station's frame.
- **An airlock in the deck is a door to nowhere.** `Dynamics::has_airlock`
  is now "has a port", and `IssueCode::AirlockSealedIn = 31` warns about an
  airlock with hull on every side. The flyer fixture's airlock moved from
  `(16, 8)` on the deck into the starboard skin at `(18, 11)` for exactly
  this, and so did the harness builds in `flyer.mjs` and `ship-check.mjs`.
  A ship without a port gets a berth anyway — held off the door by its own
  size, heading north — because a world opens docked whether or not the
  ship can go aboard.
- **A station's room opens within fifty tiles and closes beyond.**
  `World::residents` is one `crew::Residents` — the room again, `Aboard::new`
  on the station's design, seeded by `map_seed`, its clock wound on to the
  world's with `Game::wind_clock` — for the nearest station within
  `RESIDENTS_RANGE` of its *hull* (`Station::clearance`), kept out to
  `LOCAL_HYSTERESIS` further, dropped past that. **Every** station gets one,
  a derelict's with nobody in it (`residents_of` is 0), because the room is
  what has the pictures of the fixtures — without it a station is coloured
  blocks. `Game::with_layout` therefore allows an **empty** crew now; the
  ship's room never is (players are `max(1)`), and `Game::simulate` guards
  its tie-break remainder. Dropped means *forgotten*: come back and they
  start at their bunks. It is in `world_checksum` after the crew.
- **The spawn is the first station somebody lives on**, not the first
  station: `World::spawn` skips derelicts, because a crew that opens docked
  at a wreck sees nobody and blocks. That moved the simulation's dock and
  `REFERENCE_CHECKSUM`. `the_local_frame_has_a_hysteresis_and_uses_it`
  drifts *away from the nearest other node* rather than along `+x` for the
  same reason — in the new spawn system `+x` walked into the parent body's
  frame.
- **The airlock has a collar.** `dock::PROTRUSION` is half a tile, the
  `Port::face` is the end of the collar, and `hull::airlock` draws it that
  long out of the open side — so two docked airlocks meet collar to collar,
  hulls a tile apart, with the doors parted and the deck showing through.
  The picture and the berth read the same constant; move one and you move
  both. Airlocks stay 1×2 — two tiles along the skin, one deep.
- **Three pictures by distance**, all in `world_paint::stations`: out to
  `STATION_VISIBLE` a plate with the icon on it, inside `LOCAL_RADIUS_STATION`
  the hull tile by tile with parts as their colours, and while the room is
  open the room's own pictures with the residents walking between them. A
  station is never turned — heading nought — so its picture goes through
  `camera_turn` alone, via `DrawList::append_turned_at`. `local_node` draws
  bodies only now; the ring a docked ship used to sit inside is gone. The
  mated airlocks are drawn **open** (`hull::part`'s `mated`) and they *are*
  a way through — see the next section.

`ship-layout.mjs docked | residents | approach | crossing` are the pictures.

## Docked, the ship and the station are one room

`crates/world/src/docking.rs` lays the ship's design and the station's
down again as **one** `ShipDesign` — the ship first, at its own coordinates
plus a whole-tile shift; the station turned into the ship's frame by the
quarter turns the berth put between them; and the one tile between the two
hulls, where the collars meet, decked — and `World::dock_at` opens the
room on that (`Aboard::joined`). One deck, one nav grid, so a right-click
on the station's deck walks James through the airlocks. **The station's
people are not in it.** They keep their own room — `World::residents`,
opened on the station's design with the station's galley, heads, bunks,
bay and benches for fixtures — and go on living there while the ship is
tied up: cooking at their own hob, eating at their own table, sleeping in
their own bunks, on their own timetable and under their own manager
(`Residents::open` sets their goals, `data::RESIDENT_*_EACH` a head, and
stocks their larder to them). They never come aboard, and the crew's
Management tab reaches nobody ashore — it is the crew's, and one day
another player's crew on the same ship. `set_off` takes the deck apart
again (`Aboard::unjoined`): the crew back into a room of the ship alone;
the residents' room goes on as it was until the ship is out of range.
`docked_the_station_s_people_keep_to_the_station_and_their_own_agenda`
pins a day of it.

What that rests on, and what will bite:

- **`Aboard` has an `offset`, a `crew` count and a `station_frame`.**
  `offset` is where the ship's origin sits in the room's grid (nought
  alone); `position(who)` is in the **ship's** frame whatever the room, so
  the checksum and the crew names do not care. The painter adds the offset
  to the room's centre (`room_centre` in `paint_ship`) and
  `Session::room_point`/`_y` add it to the pointer — those two are the only
  places it is added. `crew` is how many of the room's Bims are the ship's
  — all of them now, kept as a count for a second crew one day.
  `station_frame` is where a station point lands on the joined deck, for
  `Aboard::visit`: every step the residents' positions are put on the deck
  as *visitors* (`Game::set_visitors`), bodies the joined room's doors open
  for and nothing else reads, since the joined room draws every door —
  the station's own room draws none while docked — and a resident walking
  through a shut-looking door would be a door lying.
- **The room's berths and seats are `Vec`s now**, as many as the layout has
  bunks and chairs, ship's first, so everybody has their own bed;
  `BERTHS`/`SEATS` are the classic room's two. `Game::with_layout` caps the
  crew at the beds there are, and a room may have **none** (a derelict's).
  `plate_on_table` goes through `plate_at`/`set_plate_at`, clamped, and
  `solids()` lists the beds where the classic room always did — between
  the table and the bay — because the push-out walks solids in order and
  a different order re-rolls every probe seed.
- **Moving a Bim between rooms drops its errand.** `Game::take_crew`
  abandons every chain (`Task::abandon`: what was carried goes back in the
  store, whoever was in bed is stood up) and `Game::adopt` stands each
  one where they were, snapped to the nearest free nav cell — a spot
  beside a bunk sits on the edge of the bunk's inflated footprint and
  reads free or not by how the grid happened to fall — or at their bunk
  if that is more than a body's margin away, which is what a crew member
  left on the station at departure gets.
- **One galley, the ship's.** The joined room maps the first of each
  fixture kind by id — the ship's — and `Aboard::leave_the_station_s`
  drops every further fixture standing in the station's box from
  `Layout::more` and `extras`, so the station's galley, heads, bays and
  lockers are furniture to walk round on the joined deck and never a
  chain's pick. The
  residents' own room, with them in it, is what draws the station's
  fixtures and the residents; the joined room draws the ship's fixtures,
  the crew and every door, over it. The cold store is restocked from the
  ship's cargo at every join and unjoin, like at world start.
- **The crew panels rebuild when the room's crew changes**
  (`CrewPanels::rebuild_crew` in `crates/app/src/crew.rs`, from the game
  screen when the count moves). With the residents in their own room that
  is a second crew one day, not a docking; the residents are named over
  their heads on the canvas (`resident_name`) and have no panels.
- **Joining costs ~3000 `apply`s** — a second in a debug test, tens of
  milliseconds in wasm — once per dock. Not cached: the ship's design
  changes.
- **A ship docked side on has the station turned**, so the station's
  fixtures are used from the wrong side (see "Every fixture is used from
  the south"). The playtest ship's airlock is to starboard and every
  station's port is in its west skin, so that ship docks at heading 0.
  `a_station_is_turned_into_the_frame_of_a_ship_docked_side_on` covers the
  arithmetic with a bow airlock; the `debug_assert` in `docking::join`
  checks every turned part's tiles against `covered`.

`docked_the_ship_and_the_station_are_one_room_and_the_crew_can_cross` walks
James over and back; `simulation-check.mjs` does it from a right-click.

## A station has a stance, and the fight crosses between two rooms

`World::stance(id)` is Friendly for `home` — the spawn station — Hostile
for any id on `World::hostile` (sorted; `set_hostile` is the only way on
or off it, and `combat` is the only caller so far, through
`Session::make_dock_hostile`), Neutral for the rest. **Neither is in
`world_checksum` yet**; put them in when a server can set them, and
re-pin. `apply_stances` — at every `join_rooms`, every `settle_residents`
open and every `set_hostile` — tells the rooms: the residents' room its
own stance (`Game::set_stance`, its fog under `Fog::All`) and whether its
people are ringed as enemies (`set_hostile_bodies`), and the joined deck
the station's box and stance (`Game::set_foreign`, the black-and-grey fog
over the station's half). The painter asks it too: a stranger's station
is its far plate (`HULL_UNKNOWN`) until its room is open, so approaching
one reveals nothing at fifty tiles.

`World::visit` is where the fight crosses. While the station is hostile
the residents' positions go to the joined room as **targets**
(`Aboard::hostiles`, `None` for one that is down) beside the visitors the
doors read, and the hits the joined room's bolts landed (`take_hits`) are
delivered to the residents' room one by one (`Game::wound`); the step a
resident's health reaches nought is `WorldEvent::EnemyDown`. At a
friendly or neutral station the target list is empty and nobody shoots.
The residents never come aboard and their room is a step behind on
`seen`, as before; `SEEN_FOR` in the room is what keeps one drawn for
two seconds after the crew lost sight of it.
`a_stranger_s_deck_is_black_beyond_a_grey_ring_and_the_crew_s_own_is_dim`
and `a_recruited_bim_shoots_the_enemies_it_can_see_and_they_are_hurt`
pin both halves; `stage_fight_for_probe` (`BIMS_FIGHT=1` in the app)
stands the two a few tiles apart inside the station's door.

## The world is bounded by the ship, and money by the dock

Two rules carried straight over from the design phase into the game, and both
are easy to lose:

- **Money only works while docked.** `Buy` and `Sell` are refused anywhere
  else, with `Refusal::NotDocked` — and *holding station beside* a station is
  not docked either, which wants an airlock. The trade window — and the
  Station button on the tray that opens it — is hidden rather than
  disabled, because a panel full of dead buttons is a panel nobody can
  tell is dead on purpose.
- **What is on the shelf is the station's, not its kind's.**
  `worldgen::Stock` is a bit a resource on the `StationBlueprint`, carried
  onto `world::Station::stock` and asked by `buy` (`Refusal::NotSoldHere`),
  `Session::sold_here` and the editor's `market`. `StationKind::sells` is
  still the ceiling; under it `Stock::roll` puts the `STAPLES` on every
  shelf and rolls the rest at `STOCKED_CHANCE` off the station's own branch
  of the contents stream. A test that buys something at the spawn buys a
  staple, or reads the shelf first the way
  `a_station_only_sells_what_its_kind_sells` reads the kind.
- **Reserved fuel is not the crew's to sell.** It has been promised to a trip
  already under way and there is nowhere out there to buy more.
  `World::can_modify_part` says the same thing about the tank it is sitting
  in, and about engines and thrusters while a trip is in the air: those three
  would change a trip that has already been quoted.

## The room is aboard the ship, and it is the same room

`crates/game` is a library as well as the room's cdylib, and `world` runs
it: `crates/world/src/crew.rs` holds a `bims::game::Game` laid out from the
accepted design by `bims::aboard` and steps it in stage 5 of `World::step`
— `Game::simulate` once per world step, at one sixtieth of a real second,
which is the room's own frame at 1x. The Bims aboard are the room's Bims:
needs, errands, the galley, the heads, the bay, the diary, all of it, with
the cold store stocked from the cargo when the world opens. Nothing was
copied; `world` imports `bims`, and `ship` imports it too, for the
painter.

Five things that hang off that and will bite:

- **`Game::render` is split from `Game::simulate`.** The room's own page
  calls `update`, which is both; the world calls `simulate` per step and
  the ship painter calls `aboard.render()` once per frame. At 24x that is
  one picture a frame rather than twenty-four.
- **The room draws its fixtures and the ship painter draws the rest.**
  `bims::aboard::drawn_by_room` names the parts the room has pictures for —
  the galley, the heads, the table and seats, the bunks, the bay, the locker
  — and `world_paint` skips those tiles and re-emits the room's whole draw
  buffer turned with the ship (`room_aboard`). The room's night wash and
  its deck plate are off aboard (`Room::shell`); the ship owns the sky.
- **Every fixture is used from the south.** The room's stations stand the
  Bim *below* the counter, the pan and the basin, and above the bay, as the
  classic layout had them; a part turned to face another way is used from
  the wrong side. `aboard.rs` says so at the top. Fixing it is the room's
  stations learning a direction each.
- **At most as many of a crew as the layout has bunks are simulated.** A
  Bim's index is its berth and its seat; the room has a berth per bunk and
  a seat per chair of the design, and the classic room its `BERTHS` and
  `SEATS` of two. A layout with none of one gets a single stand-in.
- **The RNG order in `Game::new` is pinned by every probe.** The classic
  room draws each Bim's start position *between* the Bims, from the one
  stream; `with_room` takes a closure for exactly that reason. Drawing them
  all first reshuffled every seed and failed `probe.rs` on
  "somewhere that counts as deck" — an hour's diagnosis for a two-line
  reorder. Any change to `Game::new` wants `probe`, `diary`, `sweep`,
  `crew` and `neglect` run against it.

The heads aboard have no compartment — `Bath::aboard` is a pan and a basin
with the walls and the door zero-sized off the map, `closed_door` never
anything, and both used from the deck side. The room's nav grid takes every
other blocking part as a solid (`Layout::others`), and every tile inside
the deck's bounding box that is not deck, so an L-shaped ship does not get
a room that thinks the missing corner is floor. `the_crew_live_aboard` in
`crates/world/src/tests.rs` runs the playtest ship six game hours and
asserts the Bim went somewhere and never left the deck.

The names are still the host's: `CREW_NAMES` in `crates/app/src/screens/game.rs`, painted by
`paintCrewNames` off `Session::crew_on_screen`/`_y`, which are camera units about the
ship. The crew's positions and the room's clock are in `world_checksum`, so
the room coming aboard moved `REFERENCE_CHECKSUM`; anything that moves a
Bim moves it again, and that is the checksum working.

## The starting system is charted, and the pictures are one drawing at two scales

`World::start` puts **every** node of the spawn system in `discovered`: the
crew picked the dock off the lobby's chart of that very system, and a map
that then hid what they had just looked at had nothing on it to fly to —
which read as "I cannot click on stations". Discovery (`discover_along`)
is untouched and is for what the chart does not show; a probe of it has to
`uncharted_for_probe()` first or there is nothing left to find. Two tests
already do.

`paint_body` and `paint_station` in `crates/ship/src/world_paint.rs` are
the pictures, by `BodyKind` and `StationKind`, drawn from ellipses and
rectangles about a centre and a diameter, and used twice: at icon size on
the map (pixels over the map scale) and, for a body, hull-sized alongside,
drawn **under** the hull as the ground. A station alongside is no longer
its icon — it is a hull of its own, drawn by `world_paint::stations`; see
"A station is a place". Nothing in them
may paint `VOID` to cut a shape — the derelict's broken ring is short
straight pieces, because a void bite painted over the deck was the first
thing that went wrong. The map also rings whatever the helm is aimed at,
off `Game::aimed`, which is set with the preview and read by nothing else.

`STATION_SHARE` went from a quarter to three fifths and a system rolls for
a second and a third station (`MORE_STATIONS`); the rolls are drawn whether
or not they take so the stream stays in step. That is a re-pin of the four
`worldgen::fixture` checksums and of `world::fixture::REFERENCE_CHECKSUM`
and not a `GENERATOR_VERSION` bump — the shares are deliberately off the
bump list, since no layout changes shape.

## The ship is flown from the helm, and a post is not an order

`World::can_command(slot)` is true only while that player's crew member is
within `HELM_REACH` (a tile) of the first helm's use spot — `helm_spot()`,
`at_the_helm(slot)` — and Confirm, Brake and Abort ask it; speed and
trading do not. **Brake and Abort are one command** (`Command::Abort`,
`net.stop()`) behind two buttons that are never both live: Brake for a
ship `Travelling` and not already stopping (`Plan::aborting`), Abort
for one `CastingOff` or `Undocking`. **Change target** is host-only — it
clears the aim and opens the map, which is what a click on the map did
already — and crosses no seam. Slot *i* is Bim *i*, the same pairing as the bunks. Two consequences:

- **Every test and harness that confirms a trip has to put a Bim there
  first.** `World::man_the_helm_for_probe(slot)` /
  `ship_man_helm_for_probe(slot)` stand one at the seat without the walk;
  `set_off` in `crates/world/src/tests.rs` does that and runs the departure
  through. And it has to be done **again before a later Confirm**: the Bim
  goes off about its errands — at 24x a short trip is an afternoon — and
  `ship-check.mjs` and `simulation-check.mjs` both re-man the helm before
  the second trip for exactly that reason. The screen's own way is the
  walk: a Confirm, Brake or Abort on the strip at the top of
  `crates/app/src/screens/game.rs` is a `HelmOrder` held in `pending`
  while `World::order_to_helm(slot)` walks the Bim there, sent through the
  seam the frame `at_the_helm(slot)` says so, and followed a step later by
  `World::stand_down(slot)` (`Game::stand_down`: the post off, nothing
  else), so the Bim goes back to its errands and the seat is the job's.
  There is no Take the helm button any more, and aiming on the map wants
  nobody anywhere — it is only a preview.
- **A post is a standing order, not a chain.** `Character::post` is where a
  Bim has been told to stand; `Game::send_to` sets it (interrupting the
  errand and walking there), `Game::walk_to` is the same walk *without* the
  post, `return_to_post` walks back once whatever took the Bim away is
  done, and `order_move` clears it. It holds the Bim still exactly as
  `recruited` does. `return_to_post` only re-routes when the last route has
  been walked to its end — "hands the Bim a fresh route every frame: it
  never moves" is the trap it is written round — and `POST_SLACK` is how far
  off the post counts as on it. `adopt` shifts a post and **the route in
  progress** with the body: before it shifted the route, a Bim carried
  between rooms mid-walk marched off to where its old waypoint used to be.

## Leaving a station is three states, and arriving is one

`ShipState` has `CastingOff`, `Undocking` and `Docking` beside the three it
had, codes 3, 4 and 5; `world::data` has `UNDOCK_MINUTES`, `DOCK_MINUTES`
and `CASTING_OFF_LIMIT`. A Confirm at a berth is refused straight away if
`plan_from_here` fails, and otherwise begins `CastingOff` with the target in
`Ship::pending`: every step `Aboard::send_everybody_home` posts the
station's people ashore (`ashore`, the corridor inside the station's port)
and walks the crew back (`gangway`, the deck inside the ship's), and
`everybody_home` is asked against the **ship's** design, not the joined one.
Then `unjoin_rooms`, and `Undocking` pushes the ship straight out along
`way_out` — the station's `face()` — by `undock_distance()` (its own span),
eased, heading untouched; `set_off` plans the trip from where that ends. A
trip to a station is aimed at `hold_point`, the same spot the push-off ends
at, and `finish` hands a docking plan to `Docking`: half of `DOCK_MINUTES`
sliding and turning onto the berth's heading to the hold point, half
straight in, then `dock_at`. Two things that bit:

- **"Docked" is two questions now.** The painter and the airlock ask
  `ShipState::alongside()` — docked *or* casting off, the rooms joined —
  and trade, `Session::docked_at` and the station panel ask `Docked` alone.
  `settle_residents` skips both. Anything new that matches `Docked` has to
  pick one.
- **A Confirm at a berth is not `Travelling` on the next step, and a trip to
  a station is not `Docked` the step the plan ends.** `until_stopped` in the
  world tests runs on through `Docking`; the harnesses wait on
  `ship_world_state() === 2` before reading a plan and on `!== 5` before
  reading a dock. A test that reads either the step after is reading the
  wrong state, not finding a bug. `REFERENCE_CHECKSUM` moved with all of
  this and with the second player standing at the helm in `reference_run`.
- **Nobody is aboard at the start**, so a probe of the walk ashore has to
  bring a resident onto the ship first — `bring_a_resident_aboard` in the
  world tests — or the ship casts off in the same step and the probe
  measures nothing.

`ship-layout.mjs undocking | docking` are the pictures. The map keeps its
zoom between visits (`Game::set_mode` no longer refits), and the crew and
the station's people are told apart by `character::Uniform` — the coverall
is the room's, the yoke and the hair are the person's; `Residents::open` is
the one place the station's is put on.

## The station has rooms, and the layout is a walkability contract

`station::build_layout` is a chamfered square with two three-tile corridors
crossing in the middle — the west one runs in from the port — and four rooms
off them: galley and mess north-west, quarters with the heads north-east,
hydroponics south-west, engineering south-east. Every room has a **two-tile
doorway** onto each corridor and every fixture stands with **two clear tiles
in front of it**, because the room's nav inflates every solid by
`BODY_MARGIN` (23) on a 52-unit tile and a one-tile gap leaves six units,
which it will not walk. That rule is not only about corridors: **two solids
one tile apart corner to corner leave a diagonal gap it will not squeeze
through either**. The reactor at `(x0 + 3, y0)` and the tank at
`(x0, y0 + 3)` did exactly that and cut the whole of engineering off — every
tile in it read as free deck, `validate` was happy, and `send_for_probe`
returned false for all of it. The tank is at `y0 + 4` for that reason.

`a_station_s_rooms_can_all_be_walked_from_its_door` in
`crates/world/src/tests.rs` is the contract: it builds the room's own `Nav`
from the layout and asks it for a route from the deck inside the port to
every use spot and every open deck tile, for every kind at three seeds. Run
it after moving anything in the layout; `validate` will not tell you.

Knock-ons: the layout is one plan sized by kind, and the seed decides only
how many bays, shelves, tables and batteries — two seeds are two stations
without being two buildings. Only the **first** bay, cold store and so on
by id is the room's fixture; the rest are furniture the painter draws as
blocks, which is why a second bay is a green square. And the deck just
inside the port is corridor and stays open: `simulation-check.mjs` finds the
station's deck by scanning to starboard from James.

## Construction is stage 7, and the materials never leave the hold until the part goes down

`crates/world/src/build.rs` is a `BuildSite` — kind, origin, rotation,
the same three numbers a design-phase placement is — and what has been
carried to it (`delivered`) or is in somebody's arms on the way
(`carrying`). `Command::PlaceSite`/`CancelSite` are the seam; `builds`
and `next_site` are in `world_checksum`; `WorldEvent` codes 27–30 are
`SitePlaced`, `SiteCancelled`, `Built`, `BuildLost`; `Refusal` 10–13 are
`UnderWay`, `WontFit`, `NoSuchSite`, `UnderConstruction`. Every step the
world hands the room one `bims::game::Build` **per site**
(`build_orders`): its tiles in room units (the offset added, like the
helm), `haul: Some((resource code, units))` for the first material short
of the recipe that the hold has any free of — a `HAUL_LOAD` at most — and
`minutes > 0` when everything is there and the part would go down now.
Stage 7 drains `take_picked`/`take_dropped`/`take_returned`/`take_built`
and moves the count. Things that bit, or would:

- **A "delivered" load is a reservation, not a move.** `World::free(id)`
  is what is aboard less every site's claim, and `sell`, `can_make` and
  the haul offers all ask it; `finish_build` then calls
  `build_from_cargo` — which takes `Edit::Plate` now, for plating that
  lays its own frame — and the whole recipe leaves the hold in one go.
  So mass is conserved at every step, a cancelled site frees everything,
  and a room taken apart at dock or undock (`drop_loads`) drops what was
  in the arms back onto the count without the room saying anything.
- **Every site is on the room's list, wanting nothing or not.** The
  first cut only listed sites with something to do, and a Bim carrying a
  load found its site gone from the list at `CarryToSite` — `site_stand`
  had nothing to stand beside — and gave the load up, every trip.
- **The room's copy is a step behind the world.** `Construct`'s `leave`
  takes the site off `Room::builds` and `DropMaterials`'s clears its
  `haul`, or the room re-offers the very site it just finished within the
  same step — `consider_errand` runs after the chain ends — and the
  world's `Built` arrives with a fresh chain already on the way to
  nothing. `building_under_way` skips a chain that `is_done()` for the
  same reason.
- **`can_place_site` is `apply` on `design_with_sites()`** — every
  pending site laid on first, in order — and then `validate`: a site is
  refused (`SiteRefusal::Fault(code)`) when the ship *would then* raise
  an error it does not raise now. A wall on the hob's use spot is the
  case; the app says the issue line. The room works the sites in order,
  so a wall on plating that is itself a site waits for the deck
  (`minutes` stays 0 until `apply` on the real design goes).
- **The ship and the building keep off each other.** Sites are placed
  and worked only while `at_rest()` (`Docked | Holding`); `confirm`
  refuses `UnderConstruction` while any site `begun()` or the room says
  `building_under_way()`. A bare blueprint holds nothing. Note that with
  one crew already aboard a Confirm at a berth is `Undocking` in the same
  step — `the_ship_does_not_move_while_built_on…` learnt that — so "under
  way" is checked there rather than at `CastingOff`.
- **A part built relays the room under the crew** — `relayout_room` →
  `Aboard::relayout` → `Game::relayout` → `Room::relayout` — keeping
  every errand, the dirt, the crops and the doors' locks; docking is the
  only thing that still takes the room apart. Joined, the joined design
  is recomputed through `docking::join` and the offset does not move (a
  join's shift is the station's corners against the ship's *build area*).
- `BUILD_MINUTES_BASE`/`_PER_UNIT` and `HAUL_LOAD` are in `data.rs`.
  Adding sites to the checksum moved `REFERENCE_CHECKSUM`.

The room's half — the two chains, the stand spot, the suit — is in
`crates/game/CLAUDE.md`; the blueprint and the Build tab in
`crates/ship/CLAUDE.md`. `a_site_on_the_deck_is_hauled_to_and_built_by_the_crew`,
`a_site_beyond_the_hull_is_built_in_a_suit`,
`the_ship_does_not_move_while_built_on_and_is_not_built_on_while_moving`
and `a_site_is_refused_where_the_designer_would_have_refused_it` are the
tests.

## Two things worked out once, not once a step or once a frame

- **The power budget** is `World::power_budget`, set at `start` and in
  `on_ship_changed`, which is the only path a part joins or leaves the
  design by (`shipdesign::apply`, at the one `self.ship.design = next`).
  `run_power` and `power()` read it. It used to be
  `shipdesign::power_budget(&design)` every step — a union-find over
  every tile of the grid — and was two thirds of a docked step.
- **`ShipDesign::part(id)` is a binary search**, because `parts` is in
  ascending id order (`parts_are_in_id_order` in the shipdesign tests).
  The painters ask it per tile per hull per frame (`hull::diagonal_at`,
  `shadow`, `hull_tiles`), and the linear scan it was made a station's
  picture 2.4 ms a frame. What the ship painter still spends per frame
  is drawing the station's hull from scratch — a few hundred µs — and
  that picture never changes; it is the next thing to cache if a frame
  is short again.
