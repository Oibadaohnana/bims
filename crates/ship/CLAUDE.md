# The ship's crate

Notes on `crates/ship` — the designer, the game view, the camera, the
painters, and `Session`, which is the design phase and the game it turns
into. The rules it asks are in `crates/shipdesign/CLAUDE.md`, the world it
draws in `crates/world/CLAUDE.md`, and the screens over it are
`crates/app/src/screens/designer.rs` and `game.rs`.

> **Since the port to Bevy (September 2026):** the browser host is gone.
> Every name table this file points at in `web/*.js` is now
> `crates/app/src/names.rs`; the pages are the screens in
> `crates/app/src/screens/`; the `bims_*` and `ship_*` exports are methods on
> `bims::game::Game` and `ship::Session`. The node harnesses
> (`scratchpad/*.mjs`, `smoke.mjs`, `stub.mjs`) are gone with the host — a
> check this file says lives in one of them lives nowhere now unless it says
> so in `crates/app`'s tests or `./check smoke`, and is worth restoring as a
> unit test if the thing it guarded moves again.

## The mated airlocks are a door, and the door is a picture clock

`Game::airlock_ajar` in `crates/ship/src/game.rs` is how far the two mated
doors stand open, 0 to 1. `tick_airlock` (called from `Session::render`, beside
`frame`) eases it towards open while anybody in the joined room is within
`AIRLOCK_HAIL` — a tile and a half — of the ship's port face, and towards
shut otherwise; `hull::airlock` slides the two halves apart by it, on the
ship's door and the station's alike, since they are one passage. It is a
**picture** clock like `frame`: nothing that decides anything reads it, and
the passage is walkable whatever the door looks like — a Bim ordered
through a shut-looking door walks through it and the door opens as it
arrives. `the_airlock_opens_for_whoever_comes_to_it_and_shuts_behind_them`
in `crates/ship/src/tests.rs` pins the easing and the shutting.

If a Bim ever "cannot go through an airlock" in the browser, check the
served build first: `nix run .` serves the store copy it started with, and
the crossing was verified natively (`docked_the_ship_and_the_station_are_one_room_and_the_crew_can_cross`)
and from a right-click in `simulation-check.mjs`.

## The camera never rotates; the ship does

North is up in every view, always. A camera that followed the heading would
make a flip legible and every other moment unreadable — you could not tell
which way you were going, because "which way" would always look the same.

So the design is drawn turned by the heading (each tile emitted with `rot` set,
which the host's `ctx.rotate` applies), and the starfield, anything drawn
because it is *out there*, and the whole map are not. The pointer goes back the
same way: `Game::tile_at` is the painter's arithmetic read backwards, and
`a_screen_point_maps_back_to_the_tile_it_is_over` in `crates/ship/src/tests.rs`
checks it at four headings. A wrong sign there is a ship you cannot click on
once it has turned, and nothing else would say so.

`node scratchpad/ship-layout.mjs game` and `... map` are the two views as SVG.
After anything in `world_paint.rs` they are worth thirty seconds: a hull drawn
mirrored, or a starfield turning with the ship, is obvious there and invisible
in every assertion.

**Head up is the player's exception, and it is one number.** `Game::head_up`
(the View buttons and `N` in `crates/app/src/screens/game.rs`, `Game::head_up` at the
boundary) holds the ship square to the window in both views and turns the sky,
the station alongside and the map round it instead. It is done without a
second camera: `Game::camera_turn()` is `-heading` when it is on and nothing
otherwise, `Game::ship_turn()` is `heading + camera_turn()`, and the rule is
that **everything drawing or reading back the ship goes through `ship_turn`
and everything drawing or reading back the world goes through `camera_turn`**
— the tiles, the room aboard, the hover ring and `crew_on_screen` on one side;
the starfield, `local_node`, the map and `DrawList::turn_from` on the other;
`tile_at`, `point_at` and `pick` each on the side of what they read. A new
thing drawn in the game view has to pick a side, or it sits still while
everything round it turns. Two knock-ons: the starfield tiles a square round
the ship rather than the window when head up, or the corners go bare after
the turn; and a view setting is not a `Command` — it is this browser's own
and crosses no seam. `ship-layout.mjs headup` and `... mapup` are the pictures.

## The ship view is about the crew member you steer

`Camera::focus` is the point, in the camera's units about the ship's
centre of mass, that sits in the middle of the window at a pan of nought
and that the pan is clamped about. `Game::follow_player` sets it to
`crew_on_screen(PLAYER)` every frame from `Session::render`, so the view
follows James off the ship and through the airlock, and cannot be dragged
until he is off the edge — it used to clamp about the *ship*, which is why
he could not be zoomed in on across the station. Everything is still
**drawn** about the ship — the painter, `stations`, the pointer — and only
`offset_x`/`offset_y` know about the focus, so nothing else had to change;
`Camera::zoom` is written in terms of `to_view` for the same reason. The
map's focus stays nought, the ship.
`the_camera_follows_the_crew_member_the_player_steers` pins it, including
a zoom about a corner and a pan to the limit.

**`Game::follow` is the player's exception, and `Camera::set_loose` is
how.** The game *opens* free — the whole ship in the middle, dragged
anywhere — and Follow or `F` tethers it; `C` recentres on the steered crew
member either way (`Game::centre_on_player`). The Follow / Free camera buttons and `F` (`Game::set_follow`, beside
`Game::head_up`) let the ship view go: `follow_player` then sets
nothing, and the camera is *loose* — a pan moves the **focus** rather than
the clamped pan, and nothing clamps it, so the view goes wherever it is
dragged. Letting go folds the pan into the focus first (`absorb_pan`) so
nothing on screen moves for the flip of a switch; tethering again zeroes
nothing, the next `follow_player` sets the focus and the view snaps back.
One camera and one transform either way — do not add a second camera for
the free one. The same test pins both halves and `ship-check.mjs` has a
section on the buttons and the key.

## The ship is drawn in its own frame, and the exhaust is read off the plan

`paint_ship` builds the whole ship — rim, exhaust, tiles, the hull's
pictures, the lights, the hover ring — into a ship-space `DrawList` in
design units and turns it once with `DrawList::append_turned`; the room's
buffer goes through the same call after it. A picture made of many shapes
only has to be right the once, and `crates/ship/src/hull.rs`, where the
pictures of the plating, the engines, the thrusters, the airlock and the
array live, has never heard of a heading. Anything new drawn *on* the ship
goes into that list; anything drawn because it is out there does not.

**What fires is `hull::Firing`, and it comes from `flight::effort_at`** —
the plan read a second way, beside `state_at`, and pinned against it by
`the_effort_is_the_derivative_of_the_state`. Nothing in the picture keeps
its own idea of whether the engines are on, because a flame that lagged the
ship at 24x or after a catch-up would say the ship was in two places. Four
rules that fall out of it:

- **A forward engine burns through the burn *and* through a flip brake**;
  a backward one through a brake without a flip; a sideways one never — the
  autopilot does not fly it. `Firing::of` is the arithmetic (the nose
  against the plan's line, times the sign of the acceleration), and
  `the_exhaust_follows_the_plan` in `crates/ship/src/tests.rs` drives a
  real trip through every phase and checks it.
- **A thruster's nozzle is every side of it that faces open space, and the
  one that fires is worked out from where the thruster is** — exhaust
  pushes the ship the other way, that push turns it about the centre of
  mass, and the nozzle whose turn matches the plan's is lit. The dynamics
  never look at placement; the picture does, because a corner thruster
  puffing into the hull is a picture of a broken ship.
- **The exhaust is drawn under the hull, and a plume starts where it
  clears the skin.** (An engine can no longer be inside the hull — see the
  exhaust rule — but the walk aft costs nothing and keeps the picture right
  for a design that predates it.) The playtest ship's engine sits inside the hull, and a
  flame from its bell was a dim smudge at the stern with the bright end
  under the deck; `plume` walks the tiles aft until one holds nothing and
  begins there. An engine flush with the stern is unchanged.
- **The flicker and the running lights run off `Game::frame`**, a picture
  clock counted in `Session::render` and read by nothing that decides anything
  — never the RNG, which is the simulation's, and never `world.steps`,
  which stops at a pause. Hash it; do not draw from the stream.
- **The starfield streams on the world's clock.** `Starfield::advance`
  (`Game::stream_sky`, from `Session::render`) moves each layer on by the
  log-mapped speed times the minutes the clock moved since the last frame,
  wrapped to the tile — so a steady speed is a steady stream, a pause holds
  the sky, and 24x is twenty-four times the stream. It used to be a
  displacement off the speed, which at any constant speed is a still
  picture. `the_sky_streams_on_the_world_s_clock_and_only_under_way` pins it.

`ship-layout.mjs turn` and `... burn` are the pictures, both head up so a
puff into the hull or a flame over the deck is obvious. The map marker is
`hull::marker` — three rectangles, and `FIN_LEAN` is pinned by
`the_map_is_north_up_whatever_the_ship_is_doing`, which knows the marker is
the only thing on the map that turns.

## "Is it finished" is one export, not three

`ship_phase()` and nothing else. "Is it finished", "may I still edit" and
"which phase is it" are the same question, and three exports answering it are
three things that can disagree — there were three for about an hour, and the
boundary check in `scratchpad/ship-check.mjs` is what said so. `PHASE_DESIGN`
in `crates/app/src/screens/game.rs` is the host's half of the pair.

That check is worth keeping in mind generally: it reads `crates/app/src/screens/game.rs` with
`readFileSync`, collects every `wasm.ship_*` it calls, and compares both ways
against the real exports. An export nothing calls fails it unless it is named
in `FOR_THE_HARNESS`. It is deliberately **not** built on the shell's `grep`,
which here is `ugrep --ignore-files` and returns nothing at all for files under
`web/` — a boundary check built on that comes back clean because it never read
the file.

## A drag is geometry; the edits go out one at a time

`ship_drag_*` works out which tiles a drag covers and, for a clearing drag,
which parts it would take off. The host reads that list and sends **each tile
as its own Edit through `net`**. There is deliberately no bulk operation, so a
transport has nothing extra to learn later.

Two things in that order matter:

- **Read the whole list before applying any of it.** The parts a clearing drag
  names are looked up in the design it was drawn over; applying as you go has
  the list shifting under itself.
- **Objects come off before deck, and only the top of each tile comes off
  at all.** The other way round, every floor tile with something standing
  on it is refused as `FloorUnderObject` and a right-drag over the galley
  leaves the deck behind and looks half broken. That ordering — and the
  one-layer peel — is in `Editor::drag_parts`, not in the host.

A failing Edit inside a drag is **skipped and counted, never fatal**: a
rectangle of deck over a half-floored room is meant to fill the gaps.

## A drag reports its *first* refusal, not its last

A removing drag goes from the top of the stack down, so the first thing to
refuse is the thing the player was pointing at — and everything underneath it
then refuses too, because it is holding that up. Reporting the last one
answers a question nobody asked: "take what is standing on it off first" about
the frame, when what actually said no was the shelf with a hundred units of
ore in it.

## `?random=1` is the simulation somewhere else

`nix run .#test` is `ship.html?mode=1&random=1`. The page rolls a seed and
a pick with `Math.random` — nothing about them has to agree with anybody —
and `ship_pick_dock(seed, galaxy, roll)` turns the pick into a station
somebody lives on, the `roll`-th across the whole galaxy
(`world::spawn_anywhere`, which generates every system, as the lobby does).
`session::pick_dock` is the other half of the same answer. A `seedHi`/
`seedLo`/`roll` on the query pins it, which is how `simulation-check.mjs`
looks at it: the same roll is the same place, a different roll another,
and roll 0 is the simulation's own dock. Never a derelict.

## `ship.html` has three ways in, and no spawn of its own

`web/ship.html` reads `mode`, `star` and `station` off its query with
everything else. `mode=1` is the simulation: `Session::simulate` settles
`shipdesign::playtest_ship()` and opens the world at once — the default seed,
a two-arm spiral, `world::spawn`'s dock and `SIMULATION_MONEY`, each
overridden by the query when it says. Anything else is the game, and the
game **must be told where to start**: `Session::design` takes the star and the
station, `Session::spawn_ok` is asked once at boot, and a page with no spawn or
a wrong one shows the `lost` screen with the link back to `builder.html` —
before a design phase, never after an hour of laying one out, and never a
different dock. `World::start` takes the pair for the same reason and
returns `StartError::NoSuchStation` rather than choosing.

Three things that follow, and bit on the way:

- **Every harness that wants a design phase has to bring a spawn.** There is
  no lobby in front of it, so `scratchpad/spawn.mjs` boots a bare page once
  and reads `Session::simulation_spawn`/`_station` — the simulation's dock,
  exported for exactly this — and `ship-check.mjs`'s `session()` appends it
  unless the query names its own. A session opened with `spawn: false` gets
  the error screen, and is the check that it exists.
- **`world::spawn` is the simulation's and the fixtures', and nothing
  else's.** `world::fixture::simulation_world` is how every fixture world
  starts, so the reference checksum did not move when `World::start` stopped
  choosing.
- **"Nearest discovered node" at the spawn is the dock's own parent body**,
  which the planner rightly calls `AlreadyThere`; nothing else is in sight.
  `the_playtest_ship_can_fly_somewhere_from_the_simulation_spawn` therefore
  reveals the next node out through `discover_for_probe` and plans to that.

`scratchpad/flow-check.mjs` walks the seam the two page harnesses cannot:
lobby → station → Start → the designer opened with that query → build →
Accept → docked at the chosen star and station. `simulation-check.mjs` is the
other command. `flyer.mjs` is the flyable build both it and `ship-layout.mjs`
use; `ship-check.mjs` keeps its own because the checks between the parts are
the point there.

## The designer opens on the playtest ship, as a gift

`Session::design` takes a `preset`: `PRESET_PLAYTEST` (the default, and what a
page with no `preset=` on its query gets) lays `playtest_ship_on(area)` in
the middle of the build area; `PRESET_EMPTY` is a bare grid. The ship is
**given**: `Budget::with_gift` records its price as `given`, so `remaining`
starts at the whole pool and the readout says the crew have spent nothing.
Taking a given part off refunds its price like any removal — a gift is a
gift, and "for now" it is fine that a player can sell the ship they were
handed. A build area under twenty tiles gets an empty grid rather than half
a ship.

Two knock-ons for harnesses: `ship-check.mjs`'s `session()` appends
`preset=0` unless the query names one, because everything in it builds its
own; and `flow-check.mjs` accepts the preset as it stands, because that is
now the shortest path a player has to the world. `ship-layout.mjs given` is
the picture.

## Fittings are the pictures the room has none of

`crates/ship/src/fittings.rs` draws what `hull` and the room between them do
not: the plain wall and the diagonal wall, the door (its leaves parted along
the part's long side, in its own frame), the conduit, the helm, the
shelf and the shower, in each part's own frame through `hull::Local` so a
turned part is drawn turned. `world_paint::hull_tiles` asks `hull::part`
first and `fittings::part` second and draws a block for whatever both
refuse. Every part has a picture now — the reactor, the tank, life support,
the battery, and the workshop: the smelter, the workbench, the suit locker
and the armoury — so a block on the deck is a new part somebody forgot to
draw. **The design phase draws the same pictures** (`paint::objects` asks
the same two, in the same order), and the room's own fixtures on top of
them through `paint::fixtures`: the room is laid out from the design as it
stands with `bims::aboard::layout_of` and asked, through
`Room::draw_fixtures` and a `room::Fixtures` of which parts the design has,
for the pictures of those and no others — a layout puts every fixture it
has not got on the worktop, and drawn whole it would be a heap of galley on
one tile. The picture is cached on the `Editor` (`Editor::fixtures`) and
redone in `refresh` with the issues, since laying the room out is a walk of
the whole design. `bims design` is how to look at it.

## The rocks are part of the ship's picture, and the pick is the pointer's

`world_paint::rocks` draws the mining site's tiles into the **ship-space**
list, before `hull::shadow`, so they turn with the hull: they were laid
out on the ship's tile grid (`world::mining`) and that is the whole reason
a click on one is `Game::tile_at`. `ROCK_COLORS` is indexed by
`Rock::code` — stone, iron ore silver, galvum purple — a marked tile is
tinted and ringed in `MARK`, and the hover ring rings a rock as well as a
deck tile while `Game::marking` (the Actions tab's Mine tool, this
window's own) is on. `local_node` draws nothing for a belt the ship has a
site at: the handful of icon-rocks under the hull would sit among the
real ones. `BIMS_AT_BELT=1 bims simulation` is the picture, and
`BIMS_POINTER`'s `wheel` verb zooms it out far enough to see the field.

## The conduit is drawn linked, and only in the electricity view

`fittings::conduit` is **not** reached through `fittings::part`: it takes
`links` — which of the four neighbours is also conduit, from
`fittings::conduit_links`, the same four-neighbour rule `shipdesign::power`
joins a network by — and draws a pad with an arm towards each linked side,
so a run reads as a line, a bend as a corner and a lone tile as a stub, the
way RimWorld draws its. A part standing over the run is joined *through*
the tile and does not get an arm. The design phase draws it always
(`paint::conduit`), since laying it is what the designer is for; the game
draws it only under `Game::overlay == Overlay::Electricity`, the tray's
View tab (`crew.rs`, `Tab::View`, through the `Actions` bundle), and
`world_paint::hull_tiles` skips the utility layer altogether.
`world_paint::electricity` is the overlay: every part that supplies, draws
or stores power washed and rung — `LIVE` on a network with a reactor,
`DEAD` otherwise, asked of `shipdesign::networks` once — with the conduit
on top, appended **after** the room's picture so a powered galley fixture
is rung over its own picture. `Overlay` is a view setting like `head_up`:
this window's own, read by the painter and by nothing that decides
anything.

## The blueprint is the part's own picture, and its answer is asked once a tile

`Game::placing` is the Build tab's tool — a kind and a rotation, this
window's own like `marking` — and `world_paint::blueprint` draws it over
the tile under the pointer as the part's own picture faded
(`faded_part`: `hull::part`, then `fittings::part`, then the deck's tile
for plating or the colour block, through `DrawList::append_faded`), rung
in `LIVE` or `DEAD` by `Game::ghost_ok`, with the use spots marked as the
designer marks them. `world_paint::sites` draws every `BuildSite` the
same way in `BLUEPRINT` blue with a bar along its foot for what has
arrived. Both go into the ship-space list after `hull_tiles`, so they turn
with the hull.

**The answer is `World::can_place_site`, and it validates the whole
ship**, so `Game::ghost_check` asks once per `(kind, tile, rotation)` and
keeps it (`ghost_check`), `Session::render` asks before painting, and
`Game::step` forgets it only when the design hash, the site count or the
ship's state moved — not every step, or 24x would validate the ship
twenty-four times a frame under a still pointer. `ghost_answer` is the
read-only view for the readout, a frame behind at most. `set_placing`
forgets it with the tool; `rotate_placing` is `R`. `site_at(tile)` is
what the readout names a blueprint by.
`the_blueprint_asks_the_world_once_a_tile_and_finds_a_site_under_the_pointer`
pins it. The app side — `Tab::Build`, `Tool::Build(kind)`, `BUILD_GROUPS`
and the search — is `crates/app/src/crew.rs` and `names.rs`; `R` there
turns the blueprint when one is in hand and recruits otherwise.
