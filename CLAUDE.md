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

Both come out of **one served directory** and differ only in which page is
opened, so they must not share a default port: the second one started would
find the first already there, decide Bims was running, and hand you the wrong
page. `--default-port` in `dev-server.py` is what keeps them apart, and a new
front end needs its own.

`./run game` and `./run builder` are the same two against the **live** `web/`
rather than the store copy — that is the one to use while editing. Anything
after the name (`--no-open`, a port) goes to the server.

Run it in the foreground, in a real terminal. A server launched as a background
job from a non-interactive shell inherits `SIGINT` set to `SIG_IGN`, so Python
never sees the interrupt and Ctrl+C (or `kill -INT`) will not stop it — check
`SigIgn` in `/proc/<pid>/status` if one will not die, and use `kill -TERM`.

`./serve.sh` is `./run game` without the dispatcher, kept because it is in
muscle memory; it takes the same arguments. `./build.sh` alone just compiles
the wasm into `web/`.

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

## It is a workspace now, and the room is one crate of four

Everything is under `crates/`. `game` is the room — the simulation and the wasm
exports, the only cdylib, and what used to be `src/`. Beside it are three
libraries that have to give the same answer in more than one place: `worldgen`
(the galaxy, systems and station blueprints, which a native server will one day
generate identically), `physics` (ship mass, thrust and travel time, wanted by
`worldgen` now and by the builder and flight steps later) and `time` (how long
a day is).

Four things about that are easy to get wrong:

- **Profiles only work at the workspace root.** A `[profile.release]` in
  `crates/game/Cargo.toml` is *silently ignored* — the `opt-level = "z"`,
  `lto` and `panic = "abort"` that keep the wasm small have to stay in the root
  `Cargo.toml`, which is otherwise a bare `[workspace]`.
- **`cargo build` at the root builds every member.** `nix flake check` passes
  `--workspace` on purpose: without it only `bims` is built and `worldgen`
  breaking for wasm would go unnoticed until something imported it.
- **The libraries' tests are `cargo test`, the room's are the probes.** Running
  them wants a target: `.cargo/config.toml` pins `wasm32-unknown-unknown`, so
  it is
  `cargo test --target x86_64-unknown-linux-gnu -p physics -p worldgen`.
  `nix flake check` runs exactly that as `checks.tests`.
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
- `nix-shell -p nodejs --run "node --check web/bims.js web/builder.js"` —
  catches the parse errors that produce a black page.
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

## The stub has two ways in now

`boot()` is the room: wasm, a frame loop, a canvas. `bootPage()` is a page that
is only a page — markup and one script, no wasm to instantiate and no frame to
step, with `advance(ms)` to let timers fire. Both build their DOM with the same
`makeDom`, which is the point: a third copy of the element builder is how the
stub drifts.

`bootPage` deliberately provides **no `navigator` and no `crypto`**. Both are
genuinely missing in real browsers often enough — the clipboard over plain
http, older engines — that the host has to cope, and the harness is where that
gets found out rather than in somebody's browser.

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
