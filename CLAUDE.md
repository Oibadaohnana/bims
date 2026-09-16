# Working on Bims

A 2D top-down game: all the simulation is Rust compiled to
`wasm32-unknown-unknown`, with `web/bims.js` as a thin host that replays a shape
buffer onto a canvas. `README.md` explains how the game itself fits together;
this file is about working on it.

## Running it

`nix run .` is the one command. It **builds, serves, and opens a browser tab**,
all three, without being asked — and **Ctrl+C in that terminal stops the
server**. Neither half is optional: do not add a step that makes someone open a
tab themselves, and do not leave the server running after Ctrl+C.

There is now more than one thing to run, and each is a name rather than a flag:

| | | |
| --- | --- | --- |
| `nix run .` / `nix run .#game` | the room — the simulation on a canvas | `:8080/index.html` |
| `nix run .#builder` | the start menu, the setup screen and the lobby | `:8081/builder.html` |
| `nix run .#ship` | the design phase the lobby starts — a ship on a tile grid | `:8082/ship.html` |

All three come out of **one served directory** and differ only in which page is
opened, so they must not share a default port: the second one started would
find the first already there, decide Bims was running, and hand you the wrong
page. `--default-port` in `dev-server.py` is what keeps them apart, and a new
front end needs its own.

`--wasm` is the other half of that, and it is newer. The server tells one
build from another by **hashing the module it serves**, and there is more than
one module now. A front end pointed at somebody else's wasm looks unchanged
after a rebuild that changed every line it serves — which is exactly the stale
server this check exists to catch. Each front end passes its own; the builder
has none of its own yet and rides along with the room's.

`./run game`, `./run builder` and `./run ship` are the same three against the
**live** `web/` rather than the store copy — that is the one to use while
editing. Anything after the name (`--no-open`, a port) goes to the server.

Run it in the foreground, in a real terminal. A server launched as a background
job from a non-interactive shell inherits `SIGINT` set to `SIG_IGN`, so Python
never sees the interrupt and Ctrl+C (or `kill -INT`) will not stop it — check
`SigIgn` in `/proc/<pid>/status` if one will not die, and use `kill -TERM`.

`./serve.sh` is `./run game` without the dispatcher, kept because it is in
muscle memory; it takes the same arguments. `./build.sh` alone just compiles
the wasm into `web/` — **both modules**. `cargo build` at the root builds every
member, so one command does it, but each cdylib has to be *copied*, and a front
end whose wasm was never copied is a page that fetches a 404 and shows nothing.
`flake.nix` copies them one by one for the same reason.

### `nix run .` serves a frozen copy

It serves `/nix/store/…-bims-0.1.0/share/bims`, **not** `web/`. A server that is
already up keeps handing out the build it started with, so editing `web/bims.js`
or rebuilding the wasm changes nothing in the browser until it is restarted. If
a fix appears not to have worked, check this first:

```sh
curl -s http://localhost:8080/bims.js | cmp - web/bims.js
```

The page loads `bims.js` and `bims.wasm` under a per-load query string and the
server sends `no-store`, so a stale *browser* cache is not usually the culprit —
a stale *server* is.

## It is a workspace now, and the room is one crate of eight

Everything is under `crates/`. There are **two cdylibs**: `game` is the room —
the simulation and its wasm exports, what used to be `src/` — and `ship` is the
design phase, the camera and pointer and draw buffer behind `web/ship.html`.
Beside them are six libraries that have to give the same answer in more than
one place: `worldgen` (the galaxy, systems and station blueprints, which a
native server will one day generate identically), `physics` (ship mass, thrust
and travel time, wanted by `worldgen` now and by the designer and the flight
step later), `shipdesign` (what a ship is made of and the rules for putting one
together, wanted by the designer now and by the play phase later), `economy`
(money: whole euros, the crew's shared pool, and sums that must not wrap),
`health` (one body's health points, what is wrong with it and what that costs
— radiation dose, sickness and cancer, wanted by the play phase) and `time`
(how long a day is).

Five things about that are easy to get wrong:

- **Profiles only work at the workspace root.** A `[profile.release]` in
  `crates/game/Cargo.toml` is *silently ignored* — the `opt-level = "z"`,
  `lto` and `panic = "abort"` that keep the wasm small have to stay in the root
  `Cargo.toml`, which is otherwise a bare `[workspace]`.
- **`cargo build` at the root builds every member.** `nix flake check` passes
  `--workspace` on purpose: without it only `bims` is built and `worldgen`
  breaking for wasm would go unnoticed until something imported it.
- **The libraries' tests are `cargo test`, the cdylibs' are the probes and the
  harnesses.** Running them wants a target: `.cargo/config.toml` pins
  `wasm32-unknown-unknown`, so it is `cargo test --target
  x86_64-unknown-linux-gnu -p physics -p worldgen -p shipdesign -p economy
  -p health`. `nix flake check` runs exactly that as `checks.tests`.
- **The rules go in `shipdesign`, never in `ship`.** `ship` may ask whether a
  part can be placed; it may not decide. A native server has to give the same
  answer, and `design_hash` — which is what an Accept is recorded against — has
  to come out **identical on native and on wasm32**. That is why nothing in
  `shipdesign`'s data or its hash is a `usize` or a float and why no `HashMap`
  is iterated in it. Both ends of that are pinned: the native half is
  `crates/shipdesign/src/tests.rs`, the wasm half is `ship_self_check`, and
  both compare against the same written-down constants in
  `shipdesign::fixture`. A target that drifted fails exactly one of the two.
- **`crate::time`, never `time::`.** `clock.rs` reaches the `time` crate
  through the crate root, because the probes link nothing and stand a plain
  `mod time;` over the same file — see `scratchpad/modules.rs`. Spelled
  `time::` it would only resolve through the extern prelude and every probe
  would need a `--extern` and a build step behind it.

## Verifying a change

`cargo build` succeeding proves nothing about the browser. The host is plain
JavaScript that nothing in the build ever type-checks or runs, and a mistake
there fails silently: a parse error in `bims.js` means `boot()` never runs and
the page is a black canvas with no message. Two separate breakages shipped that
way. So:

- `nix flake check` — builds the wasm and gates `cargo fmt`. Necessary, not
  sufficient.
- `nix-shell -p nodejs --run "node --check web/bims.js web/builder.js web/ship.js"`
  — catches the parse errors that produce a black page.
- Actually execute the host. Node with a stub DOM (`document.getElementById`,
  a no-op 2D context, captured event listeners) can load `web/bims.js` through
  `vm.runInContext`, boot it against the real `web/bims.wasm`, dispatch
  `pointerdown`/`contextmenu` at a fixture and click the menu item that comes
  back. That is the only way to exercise `openMenu`, which nothing else covers.
- Check the boundary both ways: every `wasm.bims_*` the host calls must exist as
  an export, and exports nothing calls are worth a look — one side may have been
  renamed without the other.

For the simulation itself, the modules compile natively. A scratch `main.rs`
that pulls them in by `#[path]` and declares them at the crate root (so
`crate::room` still resolves) can drive `Game` directly — run a task to
completion, assert the Bim ends up where it should — and can dump the real draw
buffer as SVG to look at the room without a browser.

## Editing gotchas

- Beware `sed`/`perl` substitutions anchored on leading whitespace: a pattern
  indented four spaces also matches inside a six-space-indented line, which is
  how a rename inside the frame loop silently corrupted `openMenu`. Anchor on
  something unique, and re-read the result.
- `f32::clamp` panics when its bounds cross, and that panic path drags Rust's
  formatting machinery into the wasm — 19 KB of binary. Use the branchless
  `clamp` in `crates/game/src/math.rs`.
- The renderer reads the stride at runtime via `bims_stride()` but still assumes
  the field *order* in `crates/game/src/draw.rs`.
- No strings cross the wasm boundary. The host asks for numbers and does its own
  formatting; that is deliberate, so keep it that way.

## New files need `git add`

`nix run .` and `nix flake check` build from the **git tree**, so a new `crates/game/src/*.rs`
that is only on disk is invisible to them — the build fails with
`failed to resolve mod <name>: /build/source/crates/game/src/<name>.rs does not exist` even
though `cargo build` is perfectly happy. `git add -N crates/game/src/<name>.rs` is enough;
it does not commit anything.

A whole new crate is the same trap with a louder failure: `members = ["crates/*"]`
matches a directory the git tree does not have, and the build stops on a
missing `Cargo.toml`. `git add -N crates/<name>` before the first
`nix flake check`.

## The stub DOM has to keep up with the host

The node harnesses fake just enough DOM for `web/bims.js` to run. Every DOM
method the host starts using has to be added to the stub or the harness dies
with `x is not a function` — which looks like an app failure and is not. There
is more than one harness script and they each carry their own copy of the stub;
if you touch one, check the others. A single shared stub module would be better
than the copies.

## Probes must share one module list

The native probes in the scratchpad declare the crate's modules by `#[path]`.
When they each carried their own copy of that list, adding a new `crates/game/src/*.rs`
meant every probe but the newest failed to *compile* — and `rustc` failing
leaves the previous binary in place, so running it printed a confident pass
from stale code. Three separate rounds of that happened. They now
`include!("modules.rs")` from one shared file; keep it that way, and if you
ever see a probe pass that you expected to fail, check it actually rebuilt.

## `boot()` in web/bims.js is one big scope

Every helper in `web/bims.js` lives in the same function scope, and JavaScript
lets two `function foo()` declarations coexist there: the later one silently
wins and the earlier one is unreachable. That is how a scheduler handler ended
up calling the canvas renderer — no error, no warning, the click simply did
nothing. Before adding a function to `boot()`, check the name is free:

```sh
grep -o "^  function [a-zA-Z]*" web/bims.js | sed 's/.*function //' | sort | uniq -d
```

## The harnesses share one stub

`scratchpad/stub.mjs` is the only stub DOM; the harnesses import `boot` and
`settle` from it. Before that they each carried a copy, and any DOM method the
host newly used broke whichever copies had not been updated — which reads like
an application failure and is not one.

## An empty route means "arrived"

`Character::follow_path` with an empty route deliberately does not enter the
marching state, so `arrived()` is true immediately — the alternative is a task
that waits for ever on a walk that cannot happen. Any code that treats arrival
as proof the Bim is somewhere has to handle that case, or a whole chain will run
through in one frame and the first step that sets a position teleports the Bim
across the room. `Task::enter` now marks the chain blocked instead, and
`Game::can_begin` refuses to start one whose first station is unreachable.

## Never copy a UI string into a harness

A harness that compares against a hardcoded copy of a status-line string breaks
silently the moment the wording changes: the comparison simply never matches,
so a wait loop spins instead of failing, and the Bim has time to starve to
death inside it. The failure then looks like a deadlock in the game. Read the
wording off the page at a known-good moment instead:

```js
const IDLE = byId.get("status").textContent;   // captured while idle
```

Better still, wait on the thing itself — an empty agenda panel means nothing is
running and nothing is queued, whatever the status line happens to say.

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

## Never use `|` as a perl `s|…|…|` delimiter here

Two files were corrupted by `perl -0pi -e 's|…|…|'` where the pattern or the
replacement contained a `|` — a markdown table row, a JS `||`. The delimiter
closes at the first one, and what is left is pasted somewhere unrelated: a
README got a table row prepended to its `# Bims` heading, a harness got a stray
`if` before its own first comment. Interpolation is the other half of the trap:
`${…}` inside a double-quoted perl replacement is *perl* interpolation, so a
JS template literal comes out with its placeholders emptied.

Use the Edit tool for anything containing `|`, `$` or backticks, and re-read
whatever you touched.

## The stub has a frame clock, and now a timer queue

`web/bims.js` uses `setTimeout` for the tooltip delay. `scratchpad/stub.mjs`
provides `setTimeout`/`clearTimeout` off its own frame clock rather than Node's:
a timer fires from inside `step()` once enough frames have gone by, so a
harness stays synchronous and "300ms" is 19 steps rather than a real wait. Any
new timing the host introduces has to be reachable the same way, or the harness
will sit there while the page works fine.

Two other things the stub has to mirror rather than infer: an element that
carries `hidden` in `index.html` must start hidden in the stub, and
`getBoundingClientRect` has to return a whole rect — `bottom` and `right`
included — or positioning code silently computes `NaN` and the harness cannot
tell you.

## Tooltips are asked for, never stumbled into

Every tooltip hangs off an affordance: a word underlined with class `asks`, or
a `?` button (`questionMark()` in `web/bims.js`). Never a row, a bar or a
panel — a player crossing the needs panel on the way to the deck should get
nothing. `explain()` is the mechanism; `explainWord()` and `questionMark()` are
the only two ways it is meant to be reached, and `scratchpad/tip-check.mjs`
asserts both halves: the affordances talk, and the containers around them stay
silent.

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

## `hidden` loses to any `display` rule

The tray's panels are switched with the `hidden` attribute, and the UA rule
behind it is only `display: none` at the lowest specificity. A rule like
`#tray [data-panel="management"] { display: flex }` outranks it, and the panel
then shows under *every* tab however correctly the attribute is set — which is
how the management row ended up sitting under the schedule strip. Any panel
that gets a `display` needs `:not([hidden])` on the selector.

No harness can see this: the stub DOM has no layout engine, and the attribute
it *can* see is set correctly all along. `scratchpad/tray-check.mjs` therefore
reads `index.html` and fails any `[data-panel]` rule that sets `display`
without the guard — a text check, because the rendering one is not available.

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

**It is not fixed, only rarer, and which seeds hit it moves with the RNG.**
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

## `grep` here is `ugrep`, and it silently skips `web/`

The shell's `grep` is a function wrapping `ugrep --ignore-files`, and for
whatever reason it returns *nothing* for `web/bims.js` and `web/index.html` —
no match, no error, exit 1. It is not that the pattern is wrong; the file is
never read. A boundary check built on it ("every `wasm.bims_*` the host calls
must exist as an export") comes back clean because it found no calls at all.

Use `command grep` for anything under `web/`, and sanity-check the count:

```sh
command grep -c "wasm\." web/bims.js    # should be in the hundreds
```

## Check for a stray NUL after editing `web/bims.js`

An edit put a `\0` where a space belonged, inside a template literal —
`` `${thing}\0${onIt}` `` — and nothing caught it. `node --check` parses it
fine, the page would have run, and GNU grep quietly switches to binary mode and
stops printing matches, which is what made the boundary check above look like
it had passed. One byte, invisible in every editor.

```sh
[ "$(wc -c < web/bims.js)" = "$(tr -d '\000' < web/bims.js | wc -c)" ] || echo "NUL in bims.js"
```

Worth running after any batch of edits to that file.

## The `SPOT_` codes are not the `HIT_` codes

There are two "what is at this point" questions and they want opposite
answers. `Room::hit` answers *what would a click act on*: few codes, only the
things with a menu behind them, and every rect expanded a few pixels so a near
miss on a handle still opens the menu. `Room::spot` answers *what is this*, for
the readout at the top left: everything aboard has a name, including the
worktop and the bulkheads, and nothing is expanded — a pixel beside the pan has
to read as deck.

Keep them separate. Folding one into the other loses the slack a click needs or
gives the readout a lie. `SPOT_NAMES` in `web/bims.js` is indexed by the code,
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

## A highlight is not a tooltip

Resting on a management row rings the fixture it names on the deck. That looks
like it breaks "tooltips are asked for, never stumbled into", and it does not:
nothing pops up, nothing is said, and the ring is gone the moment the pointer
moves. It is `points(el, spot)` in `web/bims.js`, not `explain()`, and it hangs
off a whole row on purpose — every row is about exactly one place.

Keep the two mechanisms apart. If a highlight ever starts carrying words it has
become a tooltip and needs an affordance.

Three ways a highlight gets left burning with no `pointerleave` behind it: the
tray folding away, a tab changing under the pointer, and the window losing
focus. All three clear it explicitly. Anything else that hides a panel has to
do the same, or a fixture stays ringed and reads as a state the game is in.

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

## Do not Proxy a wasm exports object

`scratchpad/stub.mjs` hands the host a recording wrapper round the wasm
exports, so a harness can check things the host only ever *tells* wasm — "ring
this fixture" — without adding a getter the game itself would never want.

It has to be a plain copied object. An exports object holds its entries as
read-only, non-configurable data properties, and a `Proxy` whose `get` trap
returns anything but the identical function throws `TypeError` on the first
read. `boot()` then dies before the first frame and the harness reports "boot
never asked for a frame", which looks like an application failure.

## A long-run probe's *final* reading is an accident

A Bim past the rest trigger with that trigger switched off nods off where it
stands, and a nod-off claws back about as much rest as the wait for it cost. So
the level does not settle — it oscillates, and where it lands on the last frame
depends on how the day fell out. Adding one walk to the tend chain moved it
from 0.08 to 0.33 with nothing else changed.

Assert the low-water mark, not the last sample. Same family as the layout note
above: when a long-run probe starts failing after an unrelated change, ask
first whether the thing you asserted was ever stable.

## Never wait on `boot()` by counting ticks

`WebAssembly.instantiate` resolves off-thread, so spinning a fixed number of
`setImmediate` turns waiting for the host's first `requestAnimationFrame` is a
race — and it loses maybe one run in ten, more when the machine is busy. It
then reports "boot() never asked for a frame", which is indistinguishable from
the page genuinely failing to start.

`scratchpad/stub.mjs` waits on a real clock with a 15-second deadline instead.
If a harness ever reports a boot failure, run it again twice before believing
it.

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

## One galley, one pan

`Exclusive` in `game.rs` names the parts of the ship only one Bim can be using.
An errand that needs one is simply not begun while the other is on one that
needs the same — no queue, nobody waits in line, `consider_errand` moves on.

Two places have to check it, and forgetting the second is the trap: `can_begin`
covers errands that *start*, and `queue_ready` covers ones that **resume**.
`pump_queue` does not go through `can_begin`, so without the second check a
half-cooked meal picks itself back up in a galley the other Bim is standing in,
and two chains fight over the fridge door.

`reset_for_cooking` deliberately no longer clears the table: only one Bim cooks
at a time, but the other may well be sitting there eating what it cooked twenty
minutes ago, and wiping its plate would be the cook reaching across the room.

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

## The names are the host's, and so is drawing them

No strings cross the wasm boundary, which includes "James". The simulation has
crew member 0 and crew member 1; `CREW_NAMES` in `web/bims.js` is the only
place the names exist. They are painted onto the canvas by `paintNames()` after
the shape buffer has been replayed — the draw buffer holds rectangles and
ellipses and nothing else, so text cannot come through it.

`scratchpad/stub.mjs` therefore records `fillText` calls, and the harness
asserts the names are drawn, in the right place, in the right colours. That is
the only part of the rendering a stub can see at all.

## Looking at the room without a browser

`scratchpad/layout.rs` dumps one frame of the real draw buffer as SVG and takes
an argument — `start`, `bed`, `table` — for which moment to catch. `rsvg-convert`
turns it into a PNG. After any layout change this is worth thirty seconds: a
bunk half inside the heads, a chair inside the table, a station on top of the
furniture are all obvious here and invisible in every assertion. It is how the
second bunk's placement was settled.

`scratchpad/ship-layout.mjs` is the same thing for the designer, in node
because that page's draw buffer is only reachable through the host:
`node scratchpad/ship-layout.mjs ship > /tmp/ship.svg`, with `empty`, `ghost`
and `spots` for the other three moments worth catching. It is how the ring
round every deck tile the pointer crossed was found — a light flashing on and
off across the whole grid, which no assertion was ever going to mention.

## A probe that stages a meeting has to ask where a body fits

`spot_at(..) == SPOT_DECK` means "this is floor". It does **not** mean "a body
can stand here": the nav grid inflates every obstacle by `BODY_MARGIN`, so the
strip of deck along the counter front reads as floor and has no route through
it. A probe that picks a corridor that way gets `nav.path` returning empty,
`follow_path` leaves the Bim wandering, and the whole run measures a wander
while claiming to measure a walk.

`Game::put_for_probe` snaps through `nav.nearest_free` and `send_for_probe`
returns false when there was no route. Check it. And recruit both Bims first —
that takes away the wander but not the walk, so what is left is only the thing
under test.

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

## "Blocked" and "asleep" look the same from outside

A Bim lying in its bunk is standing *inside* a solid, so `nav.path` from its
position returns empty and `can_reach` — and therefore `can_use_toilet` — is
false. A probe counting "desperate and unable to get to the pan" counts sleeping
Bims and reports a contention problem that does not exist. Measured properly —
awake, desperate, and refused — it is zero for both of them in every seed.

I chased a phantom fairness bug on exactly this. Check `rest_left() > 0` before
concluding anything about reachability.

## Selecting is not commanding

Any of the crew can be selected; only `bim::PLAYER` takes orders. The two are
deliberately separate questions, and the split is what lets the right-hand side
show whichever Bim you clicked while `order_move` still refuses anybody but
James. Exactly one is selected at a time, because the panels show one at a time.

## The diary keeps no words, and almost no entries

`memory.rs` stores `(day, minutes, code, one number)` and nothing else;
`MEMORY_LINES` in `web/bims.js` is where every sentence lives.

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
  errands as `job_code`s purely for small talk. `bims_chat_topic` therefore
  returns **two code spaces** — `JOB_` codes from 1, `What` codes from 20 —
  and `CHAT_TOPICS` is indexed by both. They do not overlap; keep it that way.
- **`Task::stowed` and `Task::swept()` existed only for the diary** and were
  dead the moment it stopped recording errands. Both are gone. If a diary
  entry ever wants that detail back, it has to be carried again.

## The stub's attribute selector wanted quotes

`querySelectorAll("[data-sheet=about]")` matched nothing in `scratchpad/stub.mjs`
while `[data-sheet="about"]` worked — the regex required the quotes that CSS
makes optional. A selector that silently matches nothing reads as the element
not existing, which is a long way from the truth. Fixed; worth remembering as
the shape of the next stub gap.

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

## Measuring a slowdown needs the other Bim to actually stay put

`stage_meeting` in `scratchpad/crowding.rs` hands *both* crew a route across the
room. Standing one of them somewhere else afterwards with `put_for_probe` does
**not** take that route away — it marches off and gets met either way, so the
"clear run" and the "walk through somebody" runs were the same scenario and came
out identical to the frame. Send it to where it stands as well, and the
comparison means something: 6.40s clear, 6.73s through the other.

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

`memory.rs` for the code, `MEMORY_LINES` in `web/bims.js` for the sentence,
`CHAT_TOPICS` beside it for the short form a Bim says out loud, and the range
in `scratchpad/diary.rs` that checks every entry is nameable.

Miss the sentence and the entry is silently **dropped from the page** — no
placeholder any more — which `smoke.mjs` catches only through its row count.
Miss the range and the probe fails on a perfectly good entry, which is what
`Swept = 9` did.

And before adding one at all: the diary keeps only what went wrong. A `What`
for something that went right does not belong in it.

## The builder is a second page, not a second mode

`web/builder.html` + `web/builder.js` are the menus in front of the room;
`web/index.html` + `web/bims.js` are the room. They share a served directory
and, one day, the wasm — nothing else. Do not reach from one into the other:
the builder deals in settings and has no `Game`, and the room deals in a
`Game` and has no menus.

Three rules the page already follows and that are cheap to break:

- **Screens and tabs are switched with `hidden`**, so every rule that gives one
  a `display` carries `:not([hidden])`. `scratchpad/tray-check.mjs` now reads
  both pages and fails any `[data-screen]`, `[data-tab]` or `[data-panel]` rule
  that forgets — and fails if it finds *nothing* to check, because a regex that
  quietly matches nothing reads exactly like a pass.
- **Everything to do with the network goes through `net`.** It is a local
  stand-in with a transport's shape, and the screens are written as if the
  other end were listening — the slot list is drawn from what `net` reports,
  the host's changes already call `net.push`. Wiring a socket up should be a
  change to that object and nothing else. Anything that talks to a server from
  inside a click handler has broken the seam.
- **Settings are numbers.** A factor and a tile count, not `"double"` and
  `"large"`. The ids exist for the buttons; what the game will be started with
  is what crosses into wasm, and no strings do.

`scratchpad/builder-check.mjs` walks the whole path — menu, setup, back,
lobby, join refusal, start — through `bootPage` in the stub. It is the only
thing that executes that file at all, so run it after touching it.

## The stub has three ways in now, and one of everything behind them

`boot()` is the room. `bootPage()` is a page that is only a page — markup and
one script, no wasm to instantiate and no frame to step, with `advance(ms)` to
let timers fire; it is synchronous, and the builder harness depends on that.
`bootWasmPage()` is a page with a wasm of its own, which is the ship designer,
and `boot()` is now that called with the room's three files.

Behind all three there is **one** `makeDom`, **one** `makeClock` and **one**
`recordingWasm`. That is the whole point of the file: a second copy of any of
them is how a stub drifts from the host it stands in for. `startPage` is where
they come together, and it hands back a `ready` that is `null` when there is
nothing to instantiate — which is what lets `bootPage` stay synchronous
without a second code path.

The clock has two ways to move: `step(n)` walks whole frames, for a page with
a render loop, and `advance(ms)` jumps to each timer in turn, for a page that
is only a page. They share the queue, so a page with both — the designer has a
frame loop *and* timed remarks — can use whichever fits.

`navigator` and `crypto` are deliberately **absent**. Both are genuinely
missing in real browsers often enough — the clipboard over plain http, older
engines — that a host has to cope, and the harness is where that gets found
out rather than in somebody's browser. `location` **is** provided, because a
real browser always has one; its `assign` records where it was sent rather than
going there, which is how `builder-check` sees that Start hands the designer
the right numbers. The pages still guard it, because the node stub is the one
place it is missing.

## Staging a walk over a particular tile

`scratchpad/spread.rs` had to walk a Bim through fouled deck to see the dirt
move, and two things made that harder than it looks. Both are worth knowing
before writing any probe that turns on *where* a Bim goes.

- **Ordering a Bim to `y` does not put it on that row.** Route smoothing, the
  push-out and the other Bim all nudge it, and it settles a whole tile from
  where it was sent. A band of mess one tile deep across the lane was missed
  entirely — the walk crossed nothing but clean deck for twenty laps and the
  probe reported the feature dead. Foul three or four rows, or read the row
  back off `bim_pos` after it has settled.
- **`is_walking()` is false while it is still turning to face the route.** A
  lap loop shaped `if !is_walking { send_somewhere_else() }` hands the Bim a
  fresh route every frame: it never moves, and the lap counter climbs happily
  the whole time. Turn round on arrival, measured as a distance from the
  target, with a frame budget as the backstop.

Neither failure says anything. The probe runs, the assertions are checked, and
the answer is simply wrong.

## Adding a job to the work list needs three edits

`work.rs` for the variant, `JOB_NAMES` in `web/bims.js` for the word, and the
range in `scratchpad/priority.rs` that checks every code names a job. Same
shape as `memory.rs`'s `What` and for the same reason — no strings cross the
boundary, so the ship knows `Job::Clean` and only the host knows "Cleaning".
Miss the second and the row comes up blank; `scratchpad/work-check.mjs` fails
on exactly that, because a blank row is a row the player cannot use and
nothing else would notice.

The host builds its rows off `bims_work_count()` rather than off the length of
its own name table, so the two disagreeing shows up as a blank row rather than
as a job silently missing from the panel. Keep it that way.

## A priority is not measurable by watching who does what

The obvious probe for "cleaning before cooking" is to stage both, run it, and
see which activity starts first. It measures nothing. There are **two** crew,
and the broom and the galley are separate, so whichever one cannot have the job
at the top of the list simply takes the one below it — both jobs begin on the
same frame under every arrangement of the numbers. The first version of
`scratchpad/priority.rs` asserted on that and passed for the wrong reason.

`Game::work_on_offer` is split out from `do_some_work` for this: it is the
*choice*, with no galley or broom in the way, and `work_on_offer_for_probe`
reads it. Whether a chosen job is then actually begun depends on things that
have nothing to do with the list. Assert on the choice; assert separately that
the work still all gets done.

Two things that cannot be staged at frame 0, and cost a round each to find out:

- **The bay's trays start sown**, so `wants_work` offers nothing until one
  ripens or one empties — several game hours in. A planting cannot be staged;
  run forward to the moment the bay speaks up and read it there.
- **The cold store starts at the manager's target**, so the bay is not running
  at the start either, whatever the trays hold.

Hunger is the one that *can* be staged: `spend_for_probe(who, 1, 1.0)` empties
the food need and the cook job is on offer on the next frame.

## `boot()` is one big scope for `const` too

The note above is about two `function foo()` declarations coexisting. The same
scope has the same trap for `const`, and it is quieter: a `const JOB_NAMES`
declared at the top of the file and a second one declared inside `boot()` do
not clash — the inner one simply *shadows* the outer for the whole function,
including code written long before it. That is how the agenda's errand labels
came to be read out of the work-priority table: `JOB_NAMES[1]` stopped being
"Making food" and became "Planting", with no error and nothing in any harness
looking at that string.

It does not even throw. The reference is inside a function called from the
frame loop, which runs after `boot()` has finished, so the temporal dead zone
never bites. Before adding a `const` to `boot()`, check the name is free at
the top level as well:

```sh
comm -12 <(command grep -o "^const [A-Z_]*" web/bims.js | sed 's/const //' | sort) \
         <(command grep -o "^  const [A-Z_]*" web/bims.js | sed 's/.*const //' | sort)
```

Anything that prints is shadowed. It should print nothing.

`web/ship.js` has the same one-big-scope `boot()` and wants both checks run
against it too.

## Health mends every frame, so a sudden nothing is not nothing

`Health::update` runs before `Game` looks at whether the Bim is dead, and a fed
Bim's health climbs. Anything that takes the bar to zero *later* in the frame —
a Bim hurting itself, a Bim giving up — was therefore back above zero by the
time the check came round, and the death simply never happened: the run ended
with a Bim reading nought health and still walking about. `update` now returns
immediately when `points <= 0.0`. Anything new that damages health in one go
depends on that, so do not "tidy" it away.

## The solitude clock has to be pinned, not waited for

`social.rs` runs to ten game days, and the interesting part is all at the far
end. `scratchpad/social.rs` pins it with `leave_alone_for_probe` **every
frame** — once is not enough, because the other Bim comes over for a word and
`Solitude::talked()` puts it straight back to nothing.

Pinning rather than switching autonomy off is the point: a Bim with autonomy
off does not eat either, and a run that starves its subject is measuring
starvation. The same reasoning applies to anything else on a multi-day clock.

## Two Bims standing together is a layout problem

Names are painted a body's height above each head. A pair who meet walking
north–south end up one above the other, and the lower one's name lands on the
upper one's body — `./layout talk` shows it immediately and nothing else does.
`Game::chat` therefore always stands them **left and right**, `TALKING_GAP`
apart, and that gap is set by the *names* rather than by the bodies: a body is
`BODY_MARGIN` across the radius, but two labels need a good deal more.

## A new part needs five edits, and the compiler catches two

`PartKind` for the variant, `PARTS` in `crates/shipdesign/src/parts.rs` for
the row, `PART_COLORS` in `crates/ship/src/paint.rs` for the colour,
`PART_NAMES` in `web/ship.js` for the word, and `PART_GROUPS` beside it for
which heading it lives under. Same shape as `memory.rs`'s `What` and
`work.rs`'s `Job`, and for the same reason: no strings cross the boundary, so
the ship knows `PartKind::Hob` and only the host knows "Hob".

The row itself is five decisions, and three of them are easy to get wrong by
leaving them at the default:

- **`layer` and `requires`.** What the part *is* and what has to be there
  already. `defs_are_sound` refuses a part that needs its own layer and one
  that needs nothing unless it is the frame; nothing else can catch a hull
  part that quietly asks for deck.
- **`recipe`.** What it is made of, which is also **what it weighs** — there
  is no mass column. See the next section.
- **`shields`.** Whether it keeps the radiation out. Defaulting to `false` is
  safe; defaulting to `true` on something that is not hull puts a hole in
  every exposure check that will never be noticed.
- **`capacity`.** A class of storage and how much, or `None`. A container
  with no capacity holds nothing and refuses every purchase.

The first three are fixed-size arrays, so leaving one out is a compile error.
The last two are not, and both fail quietly — which is what
`scratchpad/ship-check.mjs` is for. It counts palette buttons against
`ship_part_count()` and fails a name that is missing or still reads `Part 7`,
and the page builds its rows off that count rather than off `PART_NAMES`, so a
part left out of `PART_GROUPS` turns up under **Anything else** instead of
disappearing. Same arrangement as the room's work panel.

An `IssueCode` is the same trap with worse consequences: an issue whose code
has no line in `ISSUE_LINES` is **dropped from the page**, exactly as a diary
entry with no `MEMORY_LINES` is. What catches it is the row count against
`ship_issue_count()`, and nothing else would.

## A part weighs its recipe, and there is no mass column

`PartDef::recipe` is what a part is made of — metal and components, never ore
and never food — and `parts::part_mass` adds it up. **That is the only place a
mass comes from.** A separate `mass` field existed and is gone, because two
numbers that are meant to agree are two numbers that will not: a part heavier
than what went into it is mass appearing out of nothing every time one is
built.

What that buys is in `crates/shipdesign/src/materials.rs`, and it is a
contract rather than a feature. Building moves the recipe out of the hold and
into the part; deconstructing moves **all** of it back. Total ship mass is
unchanged either way, and changes only through trading while docked, fuel
burnt, food eaten or grown, and crew joining or leaving. `build_from_cargo`
and `deconstruct_to_cargo` are that rule written down and tested against every
part in the table; **nothing calls them** — the construction step will, with a
Bim and a site in the middle.

Three things that go with it:

- **Money only works at a station.** The design phase is instant and paid in
  euros because it is docked at the spawn station. Away from one, `price` means
  nothing and a part comes out of the hold or is not built. `price` and
  `recipe` are deliberately **unrelated** — nothing derives one from the other.
- **The centre of mass is not conserved, only the total.** Parts sit at their
  tile centres and cargo and crew at the centre of mass, so welding the hold
  into an engine at the stern moves it. Nothing reads that yet; `mass.rs` is
  one figure.
- **Deconstruction can be refused for want of a shelf.** The materials have to
  go somewhere, and a ship whose only shelf is the part coming off has nowhere
  — `NoRoomAboard`, measured against the design *after* the removal. Losing
  them quietly would be the one thing the contract forbids.

## Four layers, and the stack is a chain of `requires`

A tile holds at most one part per `Layer`: `Structure` under everything,
`Floor` on it, `Object` standing on that, and `Utility` running through the
tile alongside whatever is standing in it. `PartDef::requires` is the whole of
the rule — `Some(Layer::Structure)` for the frame's own plating and for the
hull parts that stand straight on it, `Some(Layer::Floor)` for everything
else, `None` for `Structure` itself.

Three consequences that are not obvious:

- **Removal is the same rule read backwards.** `remove` asks every layer of
  every tile whether the part there `requires` this one's layer. That is why
  there is no list of "what holds up what" anywhere: there is one column of
  the table and both directions read it.
- **A right-drag has to come off top-down** — utility and object, then floor,
  then structure. `Editor::drag_parts` does that ordering, not the host. Any
  other order refuses everything under the first thing it meets.
- **Connectivity is asked of the structure layer alone.** A ship is its
  frame; everything else stands on it. Walking every occupied tile instead
  would let a wall touching nothing but another wall count as holding the
  ship together.

## Radiation is a warning that outranks the errors

`validate::exposure` floods in from a **one-tile ring outside the build area**,
4-neighbour, through every tile whose object-layer part does not shield. A
reached tile holding any part is exposed.

- **The ring is outside the area**, or a ship built flush to the edge would be
  sealed by a wall that does not exist.
- **Never 8-neighbour.** Two hull parts meeting at a corner seal it; eight-way
  would leak through every diagonal join and no hand-drawn hull would pass.
- **A shielding part is never exposed itself**, because the fill cannot enter
  it. That falls out of the rule rather than being a special case.
- **A door does not shield, and neither does a plain wall.** `OutsideWall`,
  `Airlock`, `SensorArray` and `Engine` do, and that list is in
  `shielding_and_storage_are_where_they_are_meant_to_be`.

It is `IssueCode::RadiationExposure = 24`, it is **first in the issue list**,
and the host styles it `grave` — louder than an error, which is a deliberate
exception to "an error blocks Accept and a warning does not". It does not
block; it shouts, because it is the only fault on the list that kills people
and a player who never opens the checks panel is exactly the one about to
accept it. The deck carries the tint whether or not anybody is pointing at
the row, which is why `paint::faults` skips outlining that one issue — a wash
and a grid of markers saying the same thing makes neither legible.

`scratchpad/ship-layout.mjs exposure` is the only way to look at it.

## Cargo is part of the design, not something beside it

`ShipDesign::cargo` is units per `ResourceId`, bought and sold through `apply`
like any other edit. Four things hang off that and all four are load-bearing:

- **It is in `design_hash`**, after the parts. Two players accepting are
  accepting the same ship *and* the same manifest, so a Buy clears every
  Accept exactly as a wall does.
- **It is in `Budget::spent`.** Parts and goods come out of one pool, which is
  the whole of the decision the design phase asks anybody to make.
- **It is in `ship_mass`.** A ship with full tanks is heavier and the
  acceleration on the handoff screen says so before Accept, not after.
- **It is bounded by the ship, never by the station.** Supply is unlimited;
  what refuses a purchase is the pool or the hold. `economy::storage` says
  which class a resource goes in and `PartDef::capacity` says what provides
  it — a shelf and a second shelf are two hundred units of one class, not two
  holds.

A storage part with something in it cannot be removed. Sell first; the error
is `StorageInUse` and the sentence says so.

## The designer's rules are in `shipdesign`, and `apply` is the only door

`crates/shipdesign` renders nothing and exports nothing to wasm. `crates/ship`
draws and takes input. The split is not tidiness: a native server has to be
able to say whether a ship is legal without a canvas, and two implementations
of that would be two different games.

Three rules the crate is built around:

- **`apply(&design, &budget, edit) -> Result<ShipDesign, EditError>` is the
  only way a design ever changes.** It hands back a new design, so a refused
  edit cannot leave a half-changed one behind. Nothing in the UI mutates
  `parts` directly, and neither should anything else.
- **Remaining money is derived, never decremented.** `Budget` holds the pool
  and nothing else; what is left is worked out from the design every time it
  is asked. A counter kept alongside would drift from the ship the first time
  an edit was refused or replayed.
- **The occupancy grid is rebuilt from `parts` on demand.** A cached one is a
  second source of truth about what is where.

## Money is one pool, in whole euros, and every sum is checked

Nobody starts with stores. Each Bim brings money, all of it goes into one
pool, and every part has a price in euros paid out of that pool —
`crates/economy` is the arithmetic, `PartDef::price` is the table, and
`shipdesign::Budget` is what spends it.

Four things that are load-bearing rather than tidy:

- **`Money` is a `u64` of whole euros and never a float.** Two players have
  to end up with the same pool down to the last euro, and a rounding rule is
  something two machines can disagree about. The euro sign and the digit
  grouping exist **only in the host** — `euros()` in `web/builder.js` and in
  `web/ship.js`, one copy each, and nothing else makes either.
- **Overflow is an error, never a wrap and never a saturation.** A wrap hands
  somebody a fortune; a saturation quietly makes two different lobbies agree.
  `economy::starting_pool` refuses both, and a crew of nobody as well.
- **The solo bonus is a lump, not a multiplier.** A ship for one costs what a
  ship for four does — the hull, the galley and the heads are the same — so a
  lone player gets `SOLO_BONUS` on top of their own purse. Scaling it would
  miss the point, which is that the *fixed* part of a ship does not scale.
- **Money crosses the wasm boundary in two `u32` halves**, `_hi` and `_lo`,
  the same way `design_hash` does. That goes for `ship_init` as well: what
  goes *in* is what one Bim brings, and wasm does the multiplying, so the pool
  is worked out in exactly one place for the browser and for a native server
  both.

## An Accept is for a hash, not for "the design"

`design_hash` sorts the parts by `(y, x, kind)` and leaves the **ids out**, so
the same layout built in two orders gives the same number. That is what makes
an Accept comparable between players. Any successful edit by anybody clears
every Accept, which is the whole protocol — there is no way to be holding one
for a ship that is no longer on screen.

Three things this depends on and that are easy to undo:

- **Part ids only ever climb**, and a removed one is never reissued. An Edit
  in flight that names it is then refused rather than landing on whatever took
  its place. Stage 5 also wants that order: Bim *i* spawns at bunk *i*, bunks
  in id order.
- **The hash is written out by hand** — FNV-1a over little-endian `u32`s. Not
  a `Hash` derive and not `DefaultHasher`: those are explicitly allowed to
  differ between builds, and this number crosses between machines.
- **The build area is in the hash.** The same parts in a bigger square are a
  different ship, and an Accept must not carry across a resize.

## "Is it finished" is one export, not three

`ship_phase()` and nothing else. "Is it finished", "may I still edit" and
"which phase is it" are the same question, and three exports answering it are
three things that can disagree — there were three for about an hour, and the
boundary check in `scratchpad/ship-check.mjs` is what said so. `PHASE_DESIGN`
in `web/ship.js` is the host's half of the pair.

That check is worth keeping in mind generally: it reads `web/ship.js` with
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
- **Objects come off before deck.** The other way round, every floor tile with
  something standing on it is refused as `FloorUnderObject` and a right-drag
  over the galley leaves the deck behind and looks half broken. That ordering
  is in `Editor::drag_parts`, not in the host.

A failing Edit inside a drag is **skipped and counted, never fatal**: a
rectangle of deck over a half-floored room is meant to fill the gaps.

## The play phase cannot assume the room's navigation

`validate` passes a design whose use spots are all reachable over floor tiles
whose object layer is empty or non-blocking — one-tile corridors and doorways
included. The room's `nav.rs` **cannot be assumed to walk that**: it is a
10-unit cell grid inflating every obstacle by a `BODY_MARGIN` of 23, over a
tile that is 52 units. A one-tile gap between two walls leaves 6 units of
clearance, and the centre-sampled line test already has a known failure mode at
about that width — see "A route the body cannot hold to" above.

So stage 5 needs tile-based navigation, or has to prove the existing one walks
every design this crate accepts. Accepting a ship the crew cannot cross reads
as a Bim frozen mid-errand, which is the hardest failure aboard to diagnose.
The contract is written out at the top of `crates/shipdesign/src/lib.rs`; keep
the two in step.

## A drag reports its *first* refusal, not its last

A removing drag goes from the top of the stack down, so the first thing to
refuse is the thing the player was pointing at — and everything underneath it
then refuses too, because it is holding that up. Reporting the last one
answers a question nobody asked: "take what is standing on it off first" about
the frame, when what actually said no was the shelf with a hundred units of
ore in it.

## The required-fixture list is a mirror of the chains

`REQUIRED` in `crates/shipdesign/src/validate.rs` is table, cold store,
worktop, hob, dishwasher, toilet, basin, with bunks and chairs counted against
the crew. That is **exactly what the room's chains walk to today** and nothing
else. It is not a design decision about what a ship should have.

If stage 5 changes what a chain walks to, this list changes with it. A ship
validated against a stale list is a ship whose crew starve standing in front of
the fixture nobody required.

## `crates/health` is not `crates/game/src/health.rs`

Two files with the same name and nothing else in common. The one in `game` is
the behaviour test room's hunger-and-sleep bar; the crate is one body's
health for the ship game — conditions with stages, mending and death, with
radiation as the first source. **Nothing imports the other**, and the room's
hunger, sleep and filth have deliberately *not* been ported: they are meant
to arrive as further `Condition` variants, and translating the room's clocks
is its own step.

The one thing taken across is the lesson written in the room's file: health
that mends every frame will undo a killing blow before anything checks
whether the body is dead. In the crate that cannot happen twice over —
everything that does damage also suppresses mending, and `update` returns the
instant health reaches nothing rather than carrying on through the rest of
the interval.

## One update of a day, or 1440 of a minute, and no difference

`health::update` is called every frame by the browser, with hours by a
fast-forward, and with a day by a native server catching up on a
disconnection. All three have to agree or the same ship gives two answers
about who survived, so the interval is **cut at every boundary it crosses** —
a dose threshold, a dose reaching nothing, cancer beginning or advancing,
health reaching full or nothing — and each piece is applied in closed form at
a rate that does not change inside it. Nothing integrates numerically and
nothing has a fixed internal tick; either would make the answer depend on how
the caller happened to chop the time up. `a_step_of_any_length_gives_the_same_answer`
is what holds it, and it compares the events as well as the state.

Two corners in that loop that will bite whoever adds hunger:

- **A band is closed at the bottom, but a falling dose on the line is already
  out of it.** A dose of exactly `CRITICAL` reads as critical — and the next
  instant of a dose coming down is elevated. Charging the segment at the
  critical rate would take health off for time the body was never critical
  for, and worse, the segment would be of no length at all and the loop would
  not advance. `RadiationStage::below` is that distinction and it is the only
  place it exists.
- **Every segment edge must be strictly in the future.** A body already at
  full health "mends" at a boundary it is standing on, which is why
  `rate_of` returns nothing for it rather than the mending rate. Any new
  condition with a rate needs the same question asked of it.

## A health event is the only thing that says a line was crossed

`update` hands back one `HealthEvent` per transition, in order. A caller that
wants to say "James is ill" watches for the event; a caller that wants to
draw a bar reads the state. Polling the state for a change instead misses a
crossing that happened and reversed inside one update, which is exactly what
a long step does — and a step long enough to take a body from a clean bill to
radiation sickness emits all three events on the way rather than only naming
where it ended up.

`ISSUE_LINES` and `MEMORY_LINES` are the shape the host's half of this will
take: a code with no sentence is a row that never appears, so whatever draws
these will need a count check the way `smoke.mjs` has one.
