# Bims

A 2D top-down game written in Rust, running in the browser via WebAssembly.

One character — a Bim — pottering about a compartment on a ship. Left alone it
wanders; told to, it will cook itself a meal from start to finish, take itself
to bed, or go and use the heads. A clock runs the whole time, and the deck goes
dark at night.

## Running it

```sh
nix run .
```

That builds the game, serves it, and opens it in a browser tab. `Ctrl+C` stops
the server. Pass a port to pin one (`nix run . -- 3000`), or `--no-open` to keep
it out of your browser.

Running it again while it is already up starts neither a second server nor a
second tab. It recognises its own build by the wasm being served and just tells
you where it is. That matters because an old tab keeps its own copy of the game
running for as long as it stays open, so stacking them leaves you staring at two
Bims wondering which is real — pass `--open` if you want another tab anyway.

If the port has an *older* build on it, it says so and stops rather than quietly
showing you stale content. If the port is taken by something unrelated, it moves
to the next free one.

Without flakes, `./serve.sh` does the same thing and takes the same arguments.

The rest:

```sh
nix develop            # a shell with the toolchain
nix build              # the playable site lands in result/share/bims
nix flake check        # builds the wasm and checks formatting
```

`./build.sh` alone compiles the wasm into `web/`. Both scripts drop into
`shell.nix` for the toolchain if `cargo` and `lld` are not already on PATH, so
no global install is needed. `nix develop` uses that same `shell.nix`, so there
is one list of development tools rather than two that drift apart.

Opening `web/index.html` as a `file://` URL will not work: the page fetches
`bims.wasm`, and `fetch` is blocked on file URLs. Serve it over http.

Both entry points use `dev-server.py` rather than `python3 -m http.server`,
because browser caching bites hard here. Files served out of the Nix store
carry an mtime of 1970, and with no `Cache-Control` header the heuristic
freshness rule — about a tenth of a file's apparent age — decides the page is
good for decades and stops asking for it. You rebuild, reload, and are still
looking at a build from hours ago. `dev-server.py` sends `no-store`, and the
page loads `bims.js` and `bims.wasm` under a per-load query string, so a cache
poisoned before any of that was in place cannot outlive it either.

## Controls

| | |
| --- | --- |
| Click the fridge | Menu: **Make food**, open/close the door |
| Click the stove | Menu: turn on/off — the Bim walks over and flips it |
| Click the bunk bed | Menu: **Nap** (30 min) or **Sleep** (6 hours) |
| Click the toilet | Menu: **Use** — and a wash at the basin after |
| Click the bathroom door | Menu: open/close, lock/unlock |
| Right-click a fixture | The same menu, on the other button |
| `1` | Select the Bim (control group 1) |
| Drag a box over it | Select the Bim |
| Click it | Select it — a click is just a box of no size |
| Click empty floor / `Esc` | Deselect |
| Right-click the floor | Send the selection there, routed around the furniture |
| Speed slider | Run the simulation from 1x up to 12x |

Either button opens a fixture's menu, and doing so leaves the selection alone;
a *sweep* across one is still a marquee. Right-clicking bare floor is still a
move order — the hit test decides which of the two a right-click meant. While a
task is running the Bim ignores move orders and the menus are greyed out.

## Make food

The whole chain, about fifty seconds of it, runs off one menu item. Every beat
is animated rather than implied: the Bim walks to the fridge and opens it,
takes out a vegetable and shuts the door, carries it to the board, fetches a
knife from the counter drawer, chops — the vegetable visibly shrinking as
slices pile up beside it — puts the knife down, gathers the slices into the
pot, turns the hob on, waits while it bubbles and the contents turn from raw
green to stew, fetches a plate and spoon from the drawer, spoons three helpings
across onto the plate (the pot keeps the rest), turns the hob off, carries the
plate to the table, sits down, eats it with cutlery until the plate is empty,
sits a moment, then gets up and goes back to wandering.

## The heads

The compartment in the bottom-right corner is a room in its own right, walled
off with its own bulkheads and shut by a powered door. The toilet's menu has one
item on it, and that one item is a whole errand: the Bim walks to the door,
opens it, steps through, shuts it behind itself **and locks it**, crosses to the
pan, sits, waits, gets up, flushes, moves to the basin, washes its hands under a
running tap, comes back to the door, unlocks and opens it, steps out onto the
deck and shuts it again. About half a minute, all of it animated.

The door is the interesting part, because it is the one thing aboard that
changes the shape of the room. Locking it shuts it as well — a locked door
standing open is not locked — and while it is locked the toilet's **Use** item is
greyed out with the reason, rather than the Bim walking into a sealed door. The
Bim locking up behind itself is why the lock is worth having: the state the
player left it in is restored on the way out.

Being powered, the door answers the player from anywhere, the way the fridge
does. The Bim still walks to the panel when its own errand needs it.

## Bed time

The bunk bed is against the left wall. Its menu offers a nap of thirty minutes
or a sleep of six hours; both run the same chain — walk over, up the ladder,
under the covers, out cold, a stretch, and back down — and differ only in how
long the Bim stays put. The menu says what time it will be up, and the status
line counts the rest down while it sleeps.

Drawing a bunk bed from directly above is the interesting part, because the top
bunk hides the bottom one almost entirely. So the lower bed is drawn first, set
down and to the right by `BUNK_DROP`, and what you see of it is the band of its
own mattress and bedding along two sides, with the upper bunk's shadow falling
across it. The Bim sleeps on the top bunk — the one you can see — and the
bedding, the safety rails and the corner posts are drawn *after* the character,
which is what actually puts it under the covers rather than on top of them. The
footprint the pathfinder is given covers both bunks, so the half of the lower
one that sticks out is solid too.

## Time

`src/clock.rs` keeps one clock for the whole game: minutes since midnight, and
which day it is. One real second is one game minute at 1x, so a day takes
twenty-four minutes of real time and a six-hour sleep takes six — and because
the clock runs off the same `dt` as everything else, the speed slider carries it
along. At 12x a whole day goes by in two minutes.

Nothing about time crosses the wasm boundary as a string: the host reads the
minute count and formats it. The light level comes from the same clock, eased in
at dawn and out at dusk, and is laid over the finished frame as one tinted
rectangle — so nothing in `room.rs` has to know what time it is.

## The look of it

Everything is drawn from the two primitives in `src/draw.rs`, so "futuristic"
here is a matter of palette and of what gets a light on it rather than of any
new drawing machinery. The deck is dark blue-grey, the fittings are composite
panel in three shades, and one cyan running light is picked up by every powered
surface: the counter fascia, the induction coils etched into the hob, the rim of
the table, the underside of the top bunk, the jambs of the bathroom door. Warm
colours are held back for heat and for trouble, which is why a live hob and a
locked door are the only two warm things in the room and both read instantly.

## How it fits together

The simulation is entirely Rust. Each frame the host calls `bims_update`, which
advances the world and rebuilds a flat buffer of shapes in wasm linear memory;
`web/bims.js` reads that buffer and replays it onto a canvas. The only things
crossing the boundary are a handful of numbers and one pointer, which means no
binding generator and no JavaScript in the build — `cargo build` is the whole
pipeline, and the module has zero imports.

| File | What lives there |
| --- | --- |
| `src/lib.rs` | The wasm exports the host calls |
| `src/game.rs` | Ties the room, the Bim and the running task together; input |
| `src/room.rs` | The room: layout, fixture state, and how it is drawn |
| `src/bath.rs` | The heads: its bulkheads, its door, and its fittings |
| `src/task.rs` | The scripted chains — a meal, a sleep, a trip to the heads |
| `src/clock.rs` | The time of day, and how much light there is |
| `src/character.rs` | The Bim — how it decides where to go, and how it is drawn |
| `src/draw.rs` | The shape buffer and the local frame used for sprites |
| `src/math.rs`, `src/rng.rs` | Vectors, rectangles, angles, and a PCG32 generator |
| `web/bims.js` | Canvas renderer, input, and the fixture menus |

### Getting about

Every walk — a right-click order, and every leg of a scripted job — is planned
on a grid in `src/nav.rs`. Obstacles are inflated by the body radius before the
search, so a route that exists on the grid is one the Bim can physically walk
without clipping a corner. A* returns a staircase of cells, which is then pulled
straight by dropping every waypoint that can be skipped with a clear line of
sight; what is left is the two or three straight legs a person would actually
walk. A destination that is not standable — inside the table, behind the counter
— snaps to the nearest spot that is, so an order there means "the floor beside
it" rather than nothing at all.

The steering and the collision push-out are still there, but only for the
wander and as a backstop. While following a path the Bim trusts the plan.

### Two grids, because of one door

A grid built once is only right while the room never changes shape, and the
bathroom door changes it. Rebuilding the grid when the door moves does not work:
a task opens the door and asks for a route through it *in the same step*, so any
rebuild in the frame loop is a frame late and the route comes back empty — the
Bim would stand at a door it had just opened and give up.

So `nav.rs` builds two grids at startup, one with the door in the way and one
without, and the choice between them is made at the moment a path is asked for.
That also gives the closed door its proper behaviour for free: order the Bim
through one and the route is genuinely empty, so it stays where it is instead of
walking through it. Shut the door on a Bim that is inside and it really is shut
in until someone opens it.

Physics is allowed to be less careful. The push-out that keeps the body out of
the furniture takes the door a frame late, which nobody can see.

### Simulation speed

The speed control multiplies how many fixed 1/60 steps run per frame, never the
size of a step. At 12x a single stretched step would move the Bim further than
its own body in one frame and it would pass straight through the counter; twelve
normal steps give exactly the result 1x would, just sooner. `MAX_STEPS_PER_FRAME`
caps the catch-up so a backgrounded tab cannot come back and lock the page up.

### The wander

A plain per-frame random direction looks like vibration, not walking. Instead
the Bim commits to a leg of a journey at a time: a direction and a speed held
for a second or three, then either a pause or a fresh leg. Course changes are
usually gentle and occasionally a complete change of mind, and the body eases
towards each new heading rather than snapping to it.

Near the walls and the furniture it steers rather than bounces, and the steering
bends the Bim's *intent* as well as its current heading — otherwise it turns
back into the wall the moment it comes clear, and spends its life hugging the
edge. A push-out pass after each move is the backstop that keeps it out of the
table when steering is not enough.

### The room is a fixed size

The room is 860×580 world units whatever the window is; `Game::resize` works
out a scale and offset to centre it in the canvas, and the host applies that
transform when drawing and inverts it for pointer positions. Furniture at
honest proportions is worth more than filling every pixel, and it means the
layout constants in `room.rs` can be trusted.

One consequence worth knowing when editing `room.rs`: the Bim stands
`STAND_OFF` from the counter front, and `STAND_OFF` has to clear `BODY_MARGIN`
or the collision push-out fights the station it is walking to. Everything on
the worktop also has to sit within arm's reach of that line, or the Bim ends up
chopping thin air.

### The draw buffer

Twelve floats per shape: `kind, x, y, w, h, rot, radius, line, r, g, b, a`.
`kind` is 0 for a rectangle and 1 for an ellipse; everything on screen is built
from those two. Rotation is about the shape's own centre, and a non-zero `line`
strokes the outline instead of filling it. `bims_stride()` reports the stride so
the host never has to hardcode it.

If you change the layout, note that the renderer reads the stride at runtime but
still assumes the field *order* in `src/draw.rs`.

One trap worth knowing: `f32::clamp` panics when its bounds are crossed, and
that panic path drags Rust's formatting machinery into the wasm — it cost 19 KB
of the binary before being swapped for the branchless `clamp` in `src/math.rs`.
