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

There are four things to run, and each is a name rather than a flag:

| command | `./run` | opens | port |
| --- | --- | --- | --- |
| `nix run .` / `nix run .#game` | `./run game` | the whole game in order — menu, setup or lobby, world and station, ship design, then the world docked where you said | `:8080/builder.html` |
| `nix run .#simulation` | `./run simulation` | straight into the world on the playtest ship | `:8083/ship.html?mode=1` |
| `nix run .#room` | `./run room` | the behaviour test room — Bims on a deck | `:8084/index.html` |
| `nix run .#test` | `./run test` | the simulation somewhere else each time — docked at a random station somebody lives on, in a random galaxy | `:8085/ship.html?mode=1&random=1` |

**8080 used to serve the room**, and `builder`, `ship` and `serve` used to be
names; all four are gone rather than aliased. A server from an older build
still sitting on a port is refused, not reused — `nix run .` on a machine
where the old room server is still up says "a different build is on 8080",
and the answer is Ctrl+C in that terminal.

All four come out of **one served directory** and differ only in which page is
opened, so they must not share a default port: the second one started would
find the first already there, decide Bims was running, and hand you the wrong
page. `--default-port` in `dev-server.py` is what keeps them apart, and a new
front end needs its own.

`--wasm` is the other half of that. The server tells one build from another
by **hashing the modules it serves, together** — `--wasm` repeats, and a
front end passes *every* module its pages load: the game is two pages and so
two modules, `lobby.wasm` and `ship.wasm`; the simulation `ship.wasm`; the
room `bims.wasm`. A front end identified by only some of what it serves looks
unchanged after a rebuild that changed the rest — which is exactly the stale
server this check exists to catch, and the game with only `lobby.wasm` named
would have been exactly that after every change to the designer.

The `./run` forms are the same four against the **live** `web/` rather than
the store copy — that is the one to use while editing. Anything after the
name (`--no-open`, a port) goes to the server; an old name prints the usage.

Run it in the foreground, in a real terminal. A server launched as a background
job from a non-interactive shell inherits `SIGINT` set to `SIG_IGN`, so Python
never sees the interrupt and Ctrl+C (or `kill -INT`) will not stop it — check
`SigIgn` in `/proc/<pid>/status` if one will not die, and use `kill -TERM`.

`./serve.sh` is `./run game`, kept because it is in muscle memory; it takes
the same arguments, and it no longer serves the room. `./build.sh` alone just
compiles the wasm into `web/` — **all three modules**. `cargo build` at the root builds every
member, so one command does it, but each cdylib has to be *copied*, and a front
end whose wasm was never copied is a page that fetches a 404 and shows nothing.
`flake.nix` copies them one by one for the same reason.

### `nix run .` serves a frozen copy

It serves `/nix/store/…-bims-0.1.0/share/bims`, **not** `web/`. A server that is
already up keeps handing out the build it started with, so editing `web/bims.js`
or rebuilding the wasm changes nothing in the browser until it is restarted. If
a fix appears not to have worked, check this first:

```sh
curl -s http://localhost:8080/ship.js | cmp - web/ship.js
```

The page loads `bims.js` and `bims.wasm` under a per-load query string and the
server sends `no-store`, so a stale *browser* cache is not usually the culprit —
a stale *server* is.

## It is a workspace now, and the room is one crate of eleven

Everything is under `crates/`. There are **three cdylibs**: `game` is the room
— the Bims' simulation and its wasm exports, what used to be `src/`, and
since the room came aboard also a library that `world` and `ship` import —
`ship` is
`web/ship.html`, which is now **two things**: the design phase, and the game
the last Accept starts — and `lobby` is the World tab of `web/builder.html`:
a galaxy, every system in it, a camera and a pick, and nothing that decides
anything. Beside them are eight libraries that have to give the
same answer in more than one place: `worldgen` (the galaxy, systems and station
blueprints, which a native server will one day generate identically), `physics`
(ship mass, thrust and travel time), `shipdesign` (what a ship is made of and
the rules for putting one together), `flight` (what a design does when you push
it, and the closed-form plan that flies a trip), `world` (one star system, the
ship in it, and the one clock they both run on), `economy` (money: whole euros,
the crew's shared pool, and sums that must not wrap), `health` (one body's
health points, what is wrong with it and what that costs — radiation dose,
sickness and cancer, wanted by the play phase) and `time` (how long a day is).

Five things about that are easy to get wrong:

- **Profiles only work at the workspace root.** A `[profile.release]` in
  `crates/game/Cargo.toml` is *silently ignored* — the `opt-level = "z"`,
  `lto` and `panic = "abort"` that keep the wasm small have to stay in the root
  `Cargo.toml`, which is otherwise a bare `[workspace]`.
- **`cargo build` at the root builds every member.** `nix flake check` passes
  `--workspace` on purpose: without it only `bims` is built and `worldgen`
  breaking for wasm would go unnoticed until something imported it.
- **The libraries' tests are `cargo test`, the room's are the probes and the
  harnesses.** Running them wants a target: `.cargo/config.toml` pins
  `wasm32-unknown-unknown`, so it is `cargo test --target
  x86_64-unknown-linux-gnu -p physics -p worldgen -p shipdesign -p economy
  -p health -p flight -p world -p ship -p lobby`. `nix flake check` runs
  exactly that as `checks.tests`.

  **`ship` and `lobby` are on that list and are cdylibs**, which is the one
  exception, made twice. Each is also an `rlib`, and what its `tests.rs`
  covers is arithmetic — a design tile to a place on screen and back, a star
  to a pixel and the pixel to the nearest star — pure, and wrong in a way no
  harness can see. `lobby`'s also pins the one promise its cache makes: that
  "has a station" is what generating the system says. Everything else in
  those crates is still `scratchpad/ship-check.mjs` and
  `scratchpad/builder-check.mjs` against the real pages. `game` is not on
  the list and should not be.
- **The rules go in `shipdesign` and `world`, never in `ship`.** `ship` may ask
  whether a part can be placed or a trip can be flown; it may not decide. A
  native server has to give the same answer, and two numbers — `design_hash`,
  which is what an Accept is recorded against, and `world_checksum`, which is
  what a client will one day be verified against — have to come out
  **identical on native and on wasm32**. That is why nothing in `shipdesign`'s
  data or its hash is a `usize` or a float and why no `HashMap` is iterated in
  it. Both ends of both are pinned: the native halves are
  `crates/shipdesign/src/tests.rs` and `crates/world/src/tests.rs`, the wasm
  half of each is a bit in `ship_self_check`, and all of them compare against
  written-down constants in `shipdesign::fixture` and `world::fixture`. A
  target that drifted fails exactly one of each pair.

  `world_checksum` **rounds its floats onto a grid on the way in**, and that
  is not sloppiness — it is the only way it can work. Everything in `flight`
  that is not arithmetic is `sin`, `cos`, `atan2` and `sqrt`, and the first
  three come out of the platform's libm natively and Rust's own on wasm32;
  those two may differ in the last bit. A checksum that noticed a last-bit
  difference would fire constantly and say nothing. Positions go in at a
  thousandth of a unit and angles at a millionth, which is orders of magnitude
  finer than any real divergence and orders coarser than a rounding one.
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

**`./check` runs all of it.** The quick tier — `./check` — is the git-tree
check, the JavaScript traps below, `cargo fmt`, the wasm build, every
`scratchpad/*-check.mjs` and the libraries' `cargo test`, in about two
minutes; `./check full` adds the native probes (compiled fresh into
`target/probes/`, never the stale binaries in `scratchpad/`) and `nix flake
check`. `./check <step>...` runs a subset, `./check --list` explains each.
Full output is under `target/check/`; only the failing lines are printed.
"Done" means `./check` is green, and a step that was red before the change
should be said so out loud rather than folded into the report. It re-execs
itself under `nix-shell shell.nix` if the toolchain is not on PATH, and
a global Claude Code hook (`~/.claude/hooks/nix-toolchain-env.sh`) puts it there for every session
(`.envrc` does the same for a human with direnv). What each step does is
below.

- `nix flake check` — builds the wasm and gates `cargo fmt`. Necessary, not
  sufficient.
- `nix-shell -p nodejs --run "node --check web/bims.js web/builder.js web/ship.js web/crew.js"`
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

`crates/game/src/aboard.rs` is deliberately **not** on that list. It is the
one module of the room that names a `ShipDesign`, and the probes link no
other crate — which is why the rest of the room takes a `room::Layout` of
plain rects and `aboard.rs` is the only place that builds one from a
design. A second use of `shipdesign` anywhere else in `crates/game` breaks
every probe at once.

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

## The ship's game view has the room's readout

`paintGameHover` in `web/ship.js` names what the pointer is over in the ship
view, in `#game-readout`: the part under it off the design
(`ship_game_hovered_part`, then `ship_part_kind`, which reads the **live**
ship in either phase) and, where the room aboard has a name for the spot,
the room's word with its state — "Hob · lit", and whatever is lying on the
deck there. `spotReadout(x, y)` in `web/crew.js` is that second half and it
is the same function the room's own `#hover` box uses, so the two pages say
the same thing about the same deck. `PLAIN_SPOTS` beside `DECK_SPOTS` is
which `SPOT_` codes are only a word for *where* — outside, deck, bulkhead —
and on the ship those give way to the part's name, since every part the
room does not draw reads as one of them.

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
and nothing else — the builder's wasm is `lobby.wasm`, not the room's, and
only its World tab loads it. Do not reach from one into the other: the
builder deals in settings and has no `Game`, and the room deals in a `Game`
and has no menus.

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

`boot()` is the room. `bootPage()` is a page booted as if it were only a page
— markup and one script, no wasm to instantiate and no frame to step, with
`advance(ms)` to let timers fire; it is synchronous. The builder used to be
exactly that, and `builder-check.mjs` still boots it that way once at the
end, to see the World tab say the module is missing rather than sit there
loading. `bootWasmPage()` is a page with a wasm of its own — the designer
with `ship.wasm`, and now the builder with `lobby.wasm` — and `boot()` is
that called with the room's three files.

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

## A new part needs five edits, and the compiler catches three

`PartKind` for the variant, `PARTS` in `crates/shipdesign/src/parts.rs` for
the row, `PART_COLORS` in `crates/ship/src/paint.rs` for the colour,
`PART_NAMES` in `web/ship.js` for the word, and `PART_GROUPS` beside it for
which heading it lives under. A part with a recipe made at it is a sixth:
a row in `recipes::RECIPES`, and `aboard.rs` makes it a bench off that. The three arrays are fixed-length, so those
three are compile errors; the two tables in the host are not, and
`scratchpad/ship-check.mjs` is what catches them. Same shape as `memory.rs`'s `What` and
`work.rs`'s `Job`, and for the same reason: no strings cross the boundary, so
the ship knows `PartKind::Hob` and only the host knows "Hob".

The row itself is seven decisions, and five of them are easy to get wrong by
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
- **`thrust` and `torque_thrust`.** What it pushes with and what it turns
  with, and `defs_are_sound` insists they are **exclusive**: thrust on
  `Engine` and nowhere else, turning force on `Thruster` and nowhere else. A
  part that did both would make "which engines are burning" — and therefore
  the fuel bill — a different question for every design.
- **`power` and `charge`.** Whether it draws, and how much. Leaving a
  workstation at nought is a machine that runs in a brownout and never
  needs the conduit under it; see "Power is a column" below.

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

The game half has four more tables with the same rule: `PLAN_ERRORS`,
`REFUSALS`, `PHASE_NAMES` and `EVENT_LINES`, plus `BODY_KIND_NAMES` and
`STATION_KIND_NAMES` for what is on the map. A `WorldEvent` whose code has no
line in `EVENT_LINES` is dropped from the log the same way.

## Power is a column, a flood and one number

`PartDef::power` is signed — `REACTOR_OUTPUT` on the reactor, negative on
what draws, nought elsewhere — and `PartDef::charge` is `BATTERY_CHARGE` on
the battery and nothing else. `supplies()`, `draws()` and `stores()` are
the questions; `defs_are_sound` keeps each on exactly its kind the way it
keeps thrust on engines, and insists everything `parts::essential` names
(life support, doors) draws. A new consumer is one row: set `power`.

`shipdesign::power::networks` is the rule. A part is on a network when a
tile of its footprint carries conduit on the **utility** layer — no
adjacency, under it or not at all — and networks are conduit runs joined
four-neighbour, plus one more join: two conduit tiles under one
*electrical* part are one network (the reactor is the wire between them; a
table is not). Only a network with a reactor is `live()`. `validate` warns
`Unpowered = 33` (every dark consumer in one issue, tiles = footprints) and
`PowerShort = 34` (one per live run drawing more than it makes, tiles = the
run); neither is an error, for the flight warnings' reason.

`World::run_power` is stage 6: `charge += (supply − draw) · STEP_MINUTES`,
clamped to the wired batteries' storage, closed form. `Power::brownout()`
is `draw > supply && charge == 0`, and `World::powered(kind)` is what a
chain will ask: some part of that kind wired, and not browned out unless
the kind is essential. **Nothing aboard reads it yet** — the smelter will.
`Ship::charge` opens full, is clamped in `on_ship_changed` (a battery taken
off takes its charge), and is in `world_checksum`.

Both fixtures are wired — `REFERENCE_CONDUIT`, `PLAYTEST_BRANCHES` off the
spine in column 8 — which moved `REFERENCE_HASH`, `REFERENCE_PARTS`,
`PLAYTEST_HASH`, `PLAYTEST_PARTS` and `REFERENCE_CHECKSUM`;
`the_fixtures_are_wired` pins that neither warns and the playtest ship
draws 57 of 100. `ship-check.mjs`'s `buildShip` lays the same run by
dragging conduit, which is an area tool like deck. **The station layout is
not wired**: nothing reads a station's power and it is only checked for
errors, so its reactors and batteries are still furniture.

## A new resource needs six edits, and the compiler catches three

`ResourceId` in `crates/physics/src/data.rs` — appended, `ALL` and
`RESOURCES` grown with it — then `trade_price` and `storage` in `economy`
(both `match`es, so a missing arm is a compile error), `CARGO_SLOTS` in
`crates/shipdesign/src/design.rs` (an array length; `cargo_is_the_right_length`
pins it, and **every design hash moves** because the cargo is hashed at
fixed length — re-pin `REFERENCE_HASH`, `PLAYTEST_HASH` and
`REFERENCE_CHECKSUM`), `RESOURCE_NAMES` in `web/ship.js`, and an icon rule
in `web/ship.html` (`simulation-check.mjs` fails on a missing one). Then
decide **who sells it**: `StationKind::sells` in `crates/worldgen/src/data.rs`
is the only place that is written down, and its test enumerates every
pair. Galvum is the outposts' alone, an emitter is nobody's, a derelict
sells nothing. `Refusal::NotSoldHere = 9` is the world's answer and
`EditError::NotSoldHere = 17` the design phase's — the editor asks
`Editor::market`, the spawn station's kind, before it asks `apply`, since
`shipdesign` knows no stations. `ship_sold_here` is the one export both
phases read.

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
`Command::SetCraftTarget` (`net.keep` → `ship_cmd_keep`) and clamped to
the class's capacity. Nought at the start, for the same reason as the
stew target. The items panel's `.keep` box is the control and
`recipeLines` is what reads the `ship_recipe_*` exports — all seven of
them, or the boundary check names the unused one. `JOB_CRAFT = 18`,
`SPOT_BENCH = 19` (ringing only; the readout names the part), and
`WORK_NAMES`/`WORK_SPOTS` in `web/crew.js` grew a row.

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

## The outside is a clock, not a place

A walk outside — `Kind::Eva`, eight steps from `GoToSuitLocker` to
`PutSuitBack` — never gives the outside a nav grid. `StepOut`'s `leave`
calls `Character::go_outside(room.outside, facing)`, which is **sitting**
under another name: `seated = true` at a spot a tile beyond the port's
collar (`Layout::outside`, from `dock::port` in `aboard.rs`), so nothing
shoves the body back through the hull and nothing walking the deck is
slowed by it, with `Uniform::Suit` over whatever was worn. `StepIn`'s
`enter` calls `come_inside(room.gangway)`. `Outside`'s length rides in
`rest_minutes` like a doze's. **Any path that gives a chain up has to bring
the body in**: `let_go` does, before the step match, for both `suspend` and
`abandon`, and a resumed walk goes out again from the gangway with the
minutes it had.

The world says when: `World::eva_offer` — `Holding`, `Frame::Local` of a
belt, a port, a suit in the hold, shelf room, and per-crew `dose <
EVA_DOSE_LIMIT` — becomes `Game::set_eva(Option<Eva>)` every step;
`Job::Mine` is offered while it says yes and `Exclusive::Airlock` is free.
The suit is **counted, never taken out of the hold** — that exclusive is
what stops two Bims wearing one suit. `Outside`'s `leave` counts
`Room::walks_done`; the world drains it and `finish_walk` reads
`worldgen::belt_yield(seed, star, body, kind)` — a stream of its own,
`Purpose::BeltYield`, so no galaxy checksum moved — onto the shelf, as much
as fits, and emits `Mined { ore, galvum }` (value packed `ore + 100·galvum`).

Stage 8 is live now: `World::health` is a `health::HealthState` per crew,
dosed at `SUIT_INTENSITY` while `Game::is_outside(who)` and sheltered
otherwise, its events forwarded as `WorldEvent::Health` (codes 17–26 =
16 + `HealthEvent::code`). It is in the checksum. **The crate's body and the
room's are two bodies**: nothing yet tells the room a Bim the crate says has
died, and only the suit doses anybody — the exposure map is still unwired.
The dose limit is what keeps the two from disagreeing about death.

The three `leave` arms for `StepOut`, `Outside` and `Work` are in
`Task::leave`, not in the "things that happen the moment a step begins"
block of `enter` — both blocks have a `Shower => ch.wash(1.0)` arm to anchor
on, and the first time round they landed in `enter`, which counted a walk the
moment it began.

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
- **A removing drag peels one layer.** `Editor::drag_parts` hands back the
  top part of each tile in the drag and nothing under it — object, then
  utility, then floor, then structure — so a right-click on a hob takes the
  hob and leaves the deck, and the next click takes the deck. Across the
  tiles of one drag the parts are ordered top down, or a deck would be
  refused because the neighbouring tile's object was still standing on it.
  Clearing a plated tile to nothing is therefore two clicks, and a harness
  that expects one to do it is wrong.
- **Deck plating lays its own frame.** `Edit::Plate` is structure-if-missing
  then floor, one edit, both prices; the plating tool sends it (`placing`
  in `crates/ship/src/editor.rs`), and the ghost asks the same edit. There
  is no frame button in the palette — `NOT_A_TOOL` in `web/ship.js`, and
  `ship-check.mjs` counts one fewer button than parts. Bare frame is still
  a part: plate and peel the deck, which is how the fixtures and the ghost
  view get it.
- **Connectivity is asked of the structure layer alone.** A ship is its
  frame; everything else stands on it. Walking every occupied tile instead
  would let a wall touching nothing but another wall count as holding the
  ship together.

## An engine fires into space, and is worked on from any side

Two rules on the main engines, both in `validate.rs`, both with the
painter behind them so they are seen before the checks panel says so:

- **`IssueCode::ExhaustBlocked = 32` is an error.** `exhaust_tiles(part)`
  is the row of tiles straight behind the bell — aft is grid-down at `R0`,
  the way the engine does *not* push, and turns with it — and every one of
  them has to hold no frame (`exhaust_blocked`). An engine inside the hull
  with deck behind it is firing into a room; the fix is to stand it in the
  skin, its last row on the stern ring (decked, since an engine stands on
  deck; sealed, since an engine shields), the way the fixtures now do —
  `reference` has `REFERENCE_ENGINE_STERN`, the harness ships peel two
  stern tiles and put the engine at `(7, 16)`. That moved `REFERENCE_HASH`,
  `REFERENCE_CHECKSUM` and the harness builds.
- **`parts::any_side_will_do(kind)` — every engine — is used from the ring
  round its footprint**, corners left out, and the validator wants **one**
  ring tile to be standable rather than all of them (`use_spots` and
  `reachability` both branch on it). Any other part still stands where its
  table row says; the rotation test that pinned the engine's single spot by
  hand pins the bay's instead.

`paint::exhausts` washes the exhaust tiles of every placed engine in the
flame's colour with a tongue pointing aft, and in the warning colour with
a cross on every tile of the ship it would cook; the ghost gets the same
whether or not it is placeable, and marks only the ring tiles a body could
stand on. `ship-layout.mjs given | spots | ghost` are the pictures.

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

## The design phase, then the game, and one clock in it

`web/ship.html` is two screens and one wasm. The last Accept settles the ship
**and** opens the world — one event, in `ship_accept`, because two exports for
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
7. **construction** — empty;
8. **health and radiation** — empty.

The last two are extension points, not oversights. Whatever goes in them
goes in *there*, on *that* clock. A second clock or a second loop is two
simulations that will disagree, and the failure reads as a ship in two places.

`web/ship.js` turns real time into steps with an accumulator — `dt *
ship_steps_per_second() * multiplier` — and never into bigger steps. Same rule
as the room, same reason: a 24x step would move the ship several times its own
length and skip straight past its own braking phase. `MAX_STEPS_PER_FRAME` has
to stay at or above `TOP_SPEED * 60 / 30` or the top of the range quietly stops
being reachable.

## A plan is read, never integrated

`flight::plan_trip` works a trip out **once** and `state_at(plan, minutes)`
evaluates it. Nothing accumulates: calling it with 0, then 3, then 3.5 gives
the same answers as calling it with 3.5. That is the only reason a browser at
24x, a browser at 1x and a server catching up on an hour of somebody's
disconnection can agree about where the ship is.

So: **do not add anything to a plan that has to be stepped.** A plan is a list
of segments with fixed durations, and inside a segment nothing changes rate.
If something new needs a rate change, it is a new segment.

Two things fall out of that and are easy to undo:

- **A trip keeps the `Dynamics` it was planned with.** Welding a wall on
  halfway does not move an arrival that has already been promised; it changes
  what the *next* plan will be like. `World::on_ship_changed` recomputes the
  live dynamics and deliberately leaves the active plan alone.
- **Fuel is reserved at Confirm, burnt over the engine phases, and taken out
  of the hold at plan end.** Not continuously — a continuously lightening ship
  is a plan whose arithmetic was wrong from the moment it was quoted.

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
— which is what makes the stamp `web/ship.js` puts on it mean anything.
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

## What a trip needs is five warnings, not an error

`validate` now warns about a missing thruster, airlock, sensor array, fuel and
forward engine. All five are warnings, like the engine warnings before them: a
ship that cannot fly is still a ship you can live on, and refusing to let a
player accept one would be the design phase having an opinion about how to
play. `NoEngineOnAxis` **became** `NoForwardEngine` and kept its code — the
codes cross the wasm boundary — because the autopilot flies the start–arrival
line and a sideways engine is dead weight.

`shipdesign::fixture` therefore has **two** ships. `reference` is one you can
live on and deliberately cannot fly, which is what those warnings are tested
against; `flyer` is `reference` plus four thrusters, an airlock, an array, a
tank and a full load of fuel, and it is what `flight` and `world` measure
their scenarios against. Only `reference`'s hash is pinned, because only that
one is about two targets agreeing.

## Two placeholder numbers are pinned to scenarios, not to taste

`FUEL_PER_THRUST_MINUTE` in `flight::data` and `torque_thrust` on the thruster
in `shipdesign::parts` are both chosen against `flyer` and a stated outcome:
one full tank crosses the world generator's longest reference hop, and four
thrusters turn the ship through half a circle inside two game hours. The fuel
constant is written as the small engine's bill over its thrust — `0.0015 /
500.0` — so that engine burns what it always did and the heavy one five times
that; see the next section. The tests
that pin them are `one_full_tank_crosses_the_longest_reference_hop` and
`four_thrusters_flip_the_reference_inside_two_hours`, and
`what_the_fixture_actually_flies_like` beside them prints the numbers for
whoever has to move one next. Changing either without rerunning those is how
a flip becomes a worse deal than a backward engine in every case and the
choice between them stops being a choice.

## Two engines, and "is it an engine" is asked of the table

`PartKind::Engine` and `PartKind::HeavyEngine` are both main engines: five
times the push for three and a half times the weight, so the heavy one is the
better engine per tonne and the worse one to carry, and a 3×4 for €75 000
against a 2×3 for €20 000. **Nothing lists the two kinds.** `PartDef::pushes()`
(`thrust > 0.0`) is the question, and `turns()` is its partner for the
thruster; the validator, `mass::engines`, `flight::dynamics`,
`World::can_modify_part` and the painter's `exhaust` and `part` all ask it,
so a third size is one row in the table. `defs_are_sound` is the one place
the kinds are named, because it is the thing checking the column.

**Fuel goes as thrust, not as a count of engines.** `Dynamics::fuel_per_minute`
takes a segment's `accel` and multiplies by the mass — that is the thrust
burning — rather than the `engines` count, which is still on every `Segment`
and `Effort` because it is what the painter lights. A flat rate per engine
would have made the heavy engine the only engine worth having.
`the_heavy_engine_is_faster_and_dearer_over_the_same_hop` in
`crates/flight/src/tests.rs` pins the trade: swapped into the flyer it crosses
the longest hop sooner and burns more doing it, and its minute costs exactly
five times the small one's whatever the ship weighs.

The `yard` in `crates/shipdesign/src/tests.rs` stocks **exactly** the heavy
engine's recipe, because it is the heaviest recipe there is and the
"one unit short" case builds one and is then refused a wall. A part with a
bigger recipe than 150 metal and 100 components wants that fixture moved with
it, and a fourth shelf.

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

## The mated airlocks are a door, and the door is a picture clock

`Game::airlock_ajar` in `crates/ship/src/game.rs` is how far the two mated
doors stand open, 0 to 1. `tick_airlock` (called from `ship_render`, beside
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

## Docked, the ship and the station are one room

`crates/world/src/docking.rs` lays the ship's design and the station's
down again as **one** `ShipDesign` — the ship first, at its own coordinates
plus a whole-tile shift; the station turned into the ship's frame by the
quarter turns the berth put between them; and the one tile between the two
hulls, where the collars meet, decked — and `World::dock_at` opens the
room on that (`Aboard::joined`). One deck, one nav grid, so a right-click
on the station's deck walks James through the airlocks and the residents
wander aboard. `set_off` takes it apart again (`Aboard::unjoined`): the
crew back into a room of the ship alone, the residents let go, the
station's room reopening fresh next step.

What that rests on, and what will bite:

- **`Aboard` has an `offset` and a `crew` count.** `offset` is where the
  ship's origin sits in the room's grid (nought alone); `position(who)` is
  in the **ship's** frame whatever the room, so the checksum and the crew
  names do not care. The painter adds the offset to the room's centre
  (`room_centre` in `paint_ship`) and `ship_room_x`/`_y` add it to the
  pointer — those two are the only places it is added. `crew` says which
  of the room's Bims are the ship's: below it crew, from it residents.
  `ship_crew_count` is `crew`; `ship_resident_count` is the rest while
  joined, else the station's own room.
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
- **One galley.** The joined room maps the first of each fixture kind by
  id — the ship's — so the station's galley, heads, bay and locker are
  furniture to walk round while docked, and the residents cook and wash
  aboard. Their own room, kept open with nobody in it, is what draws the
  station's fixtures; the joined room draws the ship's and everybody's
  bunks, chairs and bodies, over it. The cold store is restocked from the
  ship's cargo at every join and unjoin, like at world start.
- **The crew panels rebuild when the room's crew changes.**
  `crewHost().rebuildCrew(n)` in `web/crew.js` tears down and rebuilds the
  sheets and agendas only; `keepCrewPanels` in `ship.js` calls it from
  `paintGame` when `bims_crew()` moves, and passes `name` so residents are
  named as on the canvas. The stub's `remove()` exists for this.
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
(the View buttons and `N` in `web/ship.js`, `ship_set_head_up` at the
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
`crew_on_screen(PLAYER)` every frame from `ship_render`, so the view
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
how.** The Follow / Free camera buttons and `F` (`ship_set_follow`, beside
`ship_set_head_up`) let the ship view go: `follow_player` then sets
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
  clock counted in `ship_render` and read by nothing that decides anything
  — never the RNG, which is the simulation's, and never `world.steps`,
  which stops at a pause. Hash it; do not draw from the stream.
- **The starfield streams on the world's clock.** `Starfield::advance`
  (`Game::stream_sky`, from `ship_render`) moves each layer on by the
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
- **Objects come off before deck, and only the top of each tile comes off
  at all.** The other way round, every floor tile with something standing
  on it is refused as `FloorUnderObject` and a right-drag over the galley
  leaves the deck behind and looks half broken. That ordering — and the
  one-layer peel — is in `Editor::drag_parts`, not in the host.

A failing Edit inside a drag is **skipped and counted, never fatal**: a
rectangle of deck over a half-floored room is meant to fill the gaps.

## The world is bounded by the ship, and money by the dock

Two rules carried straight over from the design phase into the game, and both
are easy to lose:

- **Money only works while docked.** `Buy` and `Sell` are refused anywhere
  else, with `Refusal::NotDocked` — and *holding station beside* a station is
  not docked either, which wants an airlock. The trade panel is hidden rather
  than disabled, because a panel full of dead buttons is a panel nobody can
  tell is dead on purpose.
- **Reserved fuel is not the crew's to sell.** It has been promised to a trip
  already under way and there is nowhere out there to buy more.
  `World::can_modify_part` says the same thing about the tank it is sitting
  in, and about engines and thrusters while a trip is in the air: those three
  would change a trip that has already been quoted.

## The room's navigation is aboard, and it cannot walk everything the designer admits

`validate` passes a design whose use spots are all reachable over floor tiles
whose object layer is empty or non-blocking — one-tile corridors and doorways
included. The room's `nav.rs` is what walks the ship now, and it **does not
walk that**: it is a 10-unit cell grid inflating every obstacle by a
`BODY_MARGIN` of 23, over a tile that is 52 units. A one-tile gap between
two walls leaves 6 units of clearance, and the centre-sampled line test has a
known failure mode at about that width — see "A route the body cannot hold
to" above. The playtest ship is open-plan, which is why it works.

So a designed ship with one-tile corridors is a ship whose crew freeze
mid-errand — the hardest failure aboard to diagnose — and the fix is either
tile-based navigation or a designer that refuses what the crew cannot walk.
Neither is done. The contract is written out at the top of
`crates/shipdesign/src/lib.rs`; keep the two in step.

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
the crew. That is **exactly what the room's chains walk to** and nothing else
— and it is now also exactly what `crates/game/src/aboard.rs` maps onto the
room's fixtures, plus the locker and the bay. It is not a design decision
about what a ship should have.

If a chain changes what it walks to, this list and `aboard.rs` change with
it. A ship validated against a stale list is a ship whose crew starve standing
in front of the fixture nobody required; `aboard.rs` puts a fixture that is
missing on the worktop rather than panicking, which is a room that stands up
and a crew that cannot use it.

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

## "Has a station" is answered by generating the system, never by the designations

`Galaxy::designation_for` knows about the five stars *promised* a station of
each kind and nothing else. A quarter of the rest roll one on their own, and
a promised one can still lose it — a station with nowhere in its own system
to fly to is pruned. So `crates/lobby` builds **every** system once per
galaxy (`Galaxy::every_system`) and keeps a `has_station` bit per star; the
inspected system is then generated *afresh* from `Galaxy::system`, so the
harness's comparison of "reported with a station" against "what inspecting
it lists" is two paths and not one path against itself.

`galaxy_checksum` covers the systems as well as the stars for the same
reason: two players whose stars all matched and whose stations did not
would have spawn pickers on different stations. `worldgen::fixture` pins it
per galaxy type; `crates/worldgen/src/tests.rs` is the native end,
`lobby_galaxy_checksum_hi`/`_lo` the wasm end, and `builder-check.mjs`
parses the constants **out of `fixture.rs`** rather than carrying a copy.

## A `u32` with its top bit set comes back from wasm negative

Every wasm `u32` arrives in JavaScript as an `i32`. `lobby_none()` is
`u32::MAX` and reads as `-1`; the checksum's high half is negative half the
time. The host copes by never writing the number down — everything is
compared against `wasm.lobby_none()`, whatever it reads as — and the
harness's `whole(hi, lo)` puts `>>> 0` on both halves before `BigInt`. A
check comparing against `4294967295` would never match.

## The World panel is one element, moved between the two tools

`makeWorld()` builds the World tab **once** and `makeTool`'s `sync()` moves
it into whichever tool is on screen (`hiddenAbove` decides, and it stops at
`document.body` — `document.hidden` is whether the *tab* is in the
background). There is one galaxy in wasm, one camera over it and one canvas;
two copies of the panel would be two views of one camera that could not both
be right. Knock-on: `scratchpad/stub.mjs`'s `appendChild` now detaches a
child from its old parent, as the real DOM does — before it did not, and a
moved panel would have answered a `querySelector` off whichever copy came
first.

The two canvases share **one** draw buffer: `lobby_render` fills it with the
galaxy and `lobby_render_system` with the diagram, and the host replays each
straight after the call. After a frame the buffer holds the diagram, so a
harness that wants the galaxy's shapes calls `lobby_render()` itself.

## `window.bimsDeliver` is the transport's inbound door

`net.deliver(event, payload)` is what a socket will call when a message
arrives, and it is the one thing the page puts on `window` — for a transport
script, and for `builder-check.mjs`, which delivers a `room` with
`host: false` to become a guest. That is the only way to be one: `join`
still refuses out loud. A delivered room opens the lobby screen; delivered
`settings` are applied without being pushed back; a `spawnStar: null`
means "cleared".

Anything new the World tab lets a host do needs its guest half: the control
disabled *and* the handler refusing when it is, the same rule as the option
buttons — and a read-only seed field puts back whatever got typed into it,
because a harness types straight into the value.

## Looking at the World tab without a browser

`node scratchpad/lobby-layout.mjs galaxy | zoomed | system > /tmp/lobby.svg`
is `ship-layout.mjs` for the builder. The diagram's labels are the host's
`fillText`, recorded by the stub, and they are only drawn on a frame that
repaints the panel — so the script clicks the star again before stepping.
It is how a relay was checked to be sitting in deep space rather than
mis-drawn beside its planet.

## `?random=1` is the simulation somewhere else

`nix run .#test` is `ship.html?mode=1&random=1`. The page rolls a seed and
a pick with `Math.random` — nothing about them has to agree with anybody —
and `ship_pick_dock(seed, galaxy, roll)` turns the pick into a station
somebody lives on, the `roll`-th across the whole galaxy
(`world::spawn_anywhere`, which generates every system, as the lobby does).
`ship_picked_station` is the other half of the same answer. A `seedHi`/
`seedLo`/`roll` on the query pins it, which is how `simulation-check.mjs`
looks at it: the same roll is the same place, a different roll another,
and roll 0 is the simulation's own dock. Never a derelict.

## `ship.html` has three ways in, and no spawn of its own

`web/ship.html` reads `mode`, `star` and `station` off its query with
everything else. `mode=1` is the simulation: `ship_simulate` settles
`shipdesign::playtest_ship()` and opens the world at once — the default seed,
a two-arm spiral, `world::spawn`'s dock and `SIMULATION_MONEY`, each
overridden by the query when it says. Anything else is the game, and the
game **must be told where to start**: `ship_init` takes the star and the
station, `ship_spawn_ok` is asked once at boot, and a page with no spawn or
a wrong one shows the `lost` screen with the link back to `builder.html` —
before a design phase, never after an hour of laying one out, and never a
different dock. `World::start` takes the pair for the same reason and
returns `StartError::NoSuchStation` rather than choosing.

Three things that follow, and bit on the way:

- **Every harness that wants a design phase has to bring a spawn.** There is
  no lobby in front of it, so `scratchpad/spawn.mjs` boots a bare page once
  and reads `ship_simulation_star`/`_station` — the simulation's dock,
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

The names are still the host's: `CREW_NAMES` in `web/ship.js`, painted by
`paintCrewNames` off `ship_crew_x`/`_y`, which are camera units about the
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

## The designer opens on the playtest ship, as a gift

`ship_init` takes a `preset`: `PRESET_PLAYTEST` (the default, and what a
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

## The room's panels are one script, and the ship page has them too

`web/crew.js` and `web/crew.css` are the crew's panels — the selected Bim's
needs, health and diary, the agendas, the tray with the timetable, the work
list and the management row, the fixture menus, and the tooltips everything
hangs off — shared by the room (`index.html` + `bims.js`) and the ship
(`ship.html` + `ship.js`). Both pages load `crew.js` **before** their own
script (`async = false`, or two dynamically added scripts race) and call
`crewHost({ wasm, player, crewCount })`; `scratchpad/stub.mjs` runs it
first whenever the markup names it. The name tables the room's codes index
— `CREW_NAMES`, `JOB_NAMES`, `MEMORY_LINES`, `CHAT_TOPICS`, `SPOT_NAMES`,
the `HIT_` codes — live in it and nowhere else, so the collision checks
above want a third comparison: a top-level name in `crew.js` must not be
declared again in either page, at the top or inside `boot()`.

What makes that possible is that **`ship.wasm` already exports every
`bims_*`**: a `#[no_mangle]` in an rlib is exported from every cdylib that
links it. They were dead — acting on the room crate's own static, which the
ship page never initialises — until `bims::host_aboard` gave them a
provider, which `ship_init` and `ship_simulate` point at
`world.aboard.room`. Two rules that follow:

- **The ship page must never call `bims_init`, `bims_update`,
  `bims_resize` or `bims_view_*`.** The world steps the room and the ship's
  camera draws it; a second clock is two simulations. `ship-check.mjs`
  fails on the first two and the view three.
- **Every `bims_*` coordinate is a room coordinate**, which aboard is a
  design world unit. `ship_room_x`/`_y` are the canvas read back through
  the ship's camera and heading — `Game::design_point_at`, the same
  arithmetic `tile_at` floors — and `web/ship.js` converts *before* every
  `bims_drag_*`, `bims_hit_at` and `bims_order_move`. The marquee is
  therefore drawn on the deck and turns with the ship; that is the box the
  room tests the crew against, not the one on the glass.

`bims_crew()` goes through the same accessor: it used to read the room's
own static so it could be asked before `bims_init`, and aboard that
reported the classic room's two for a ship with one Bim, which was a trap
in `bims_is_selected(1)` on the first paint. Anything else in
`crates/game/src/lib.rs` that reaches `GAME` directly has the same bug
waiting.

The ship's design screen used to have a `<div id="work">` for its three
columns; it is `#columns` now, because `#work` is the tray's table in
`crew.css`. Two pages sharing a stylesheet share an id space — **and a class
space**. `crew.css` styles `.what` bare (the 17-pixel `?` button), and the
ship page's readouts used `class="what"` for their labels: "Docked" and
the hover text were being squeezed into a box the size of a letter and
clipped. They are `.named` now, the palette swatch is `.tint` (crew's
`.swatch` is the timetable legend) and the settings sheet is
`.settings-sheet`. `tray-check.mjs` reads every bare class rule out of
`crew.css` and fails a page whose own `<style>` has a rule for the same
class — that is the signature: two stylesheets over one element. Markup
that carries crew's own component (the legend's `<i class="swatch">`) is
fine and is not what it looks for.

The ship page's `r` is two keys: the ghost's rotation in the design phase,
recruit once `deck` exists. Escape shuts a fixture's menu if one is open;
otherwise it stops aiming and opens the **settings sheet** (`#settings` in
`web/ship.html`), and Escape or its button closes that. It no longer
clears the selection — clicking empty deck does. The sheet is where the
keys are explained, and the hint line under the readout is gone.
`simulation-check.mjs` reads the sheet off the file (the stub keeps no
markup text) and fails if any key `ship.js` handles — every `key === "x"`
and the `"wasd"` set — has no `<kbd>` on it, so a new key needs a row.

The hover readout is its own box, `#game-hover`, under `#game-readout`,
and it hides itself (`:has(.what:empty)`) rather than sitting empty.

## The ship is flown from the helm, and a post is not an order

`World::can_command(slot)` is true only while that player's crew member is
within `HELM_REACH` (a tile) of the first helm's use spot — `helm_spot()`,
`at_the_helm(slot)` — and Confirm, Brake and Abort ask it; speed and
trading do not. **Brake and Abort are one command** (`Command::Abort`,
`net.stop()`) behind two buttons that are never both live: Brake for a
ship `Travelling` and not already stopping (`ship_trip_aborting`), Abort
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
  the second trip for exactly that reason. The page's own way is
  `ship_order_helm()` (the **Take the helm** button) and `ship_at_helm(slot)`;
  `aimAt` in `web/ship.js` refuses to aim from anywhere else.
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
  and trade, `ship_docked_at` and the station panel ask `Docked` alone.
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

## A corner piece is a whole tile, and only the picture is a triangle

`PartKind::DiagonalWall = 29` and `DiagonalOutsideWall = 30` are the wall and
the outside wall cut across their tile at forty-five degrees. **Every rule
treats them as the straight kind**: one object a tile, `blocks_movement`,
`requires: Some(Layer::Structure)`, and the hull one `shields` — the
exposure fill is four-neighbour, so a staircase of them touching corner to
corner is as tight as a straight run, and nothing in `validate` or the room's
nav knows the other half of the tile is empty. Do not "improve" that by
letting the fill or a body through the open half; the fill would then leak
through every chamfer and the nav would plan through a wall.

Which half is solid is the part's `Rotation`, through one function:
`parts::solid_corner` — `R0` is the **south-west** corner, then clockwise
with `R`. The draw format grew its one new shape for this,
`KIND_TRIANGLE = 2`: the bottom-left half of the box at `rot` nought, which
is `R0`'s corner, so the triangle's turn *is* the part's turn. Both
`draw.rs` files, both replay loops (`ship.js`, `bims.js`) and
`ship-layout.mjs`'s SVG dumper know the kind; `hull::corner` is the one
place the turn, the outward normal and the hypotenuse's angle are worked
out, and every painter reads it — the designer's frame and object and ghost,
the hull's diagonal plate and the shadow along the hypotenuse, `fittings`'
diagonal bulkhead, and the frame triangle under a corner piece in both
views. A second copy of that arithmetic is a chamfer filled on one side by
one painter and bevelled on the other by another.

Two things that follow:

- **A chamfer's end tiles replace straight hull.** A cut of `c` from a
  corner at `(x0, y0)` is void where `(x - x0) + (y - y0) < c`, corner pieces
  where it equals `c`, and the ship beyond — so the run's two ends sit *on*
  the skin rows, in place of the plating there. `ship-check.mjs`'s chamfer
  section peels those two before laying the run, and the frame out of the
  void corner as well, or the tiles left behind are exposed and the count
  never reaches nought. `playtest_outline` in `fixture.rs` and `outline` in
  `station.rs` are that rule written down.
- **A diagonal drag is a staircase**, `editor::diagonal_line`: one tile a
  step along the shorter of the two distances, every tile at the ghost's
  turn. A removing drag is still a rectangle — it would take the deck beside
  the run too — so the harness peels a run one tile at a time.

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

## The playtest ship is three compartments, and its numbers are pinned

`playtest_ship()` is a chamfered bow with the bridge in it, the main deck,
and engineering aft of a second bulkhead, with a two-tile doorway in each —
the ASCII on `playtest_ship()`'s doc comment is the quickest way to see it,
and it is hand-copied from a dump, so redraw it when the ship moves. The airlock is in the **starboard** skin at
`(17, 11)`, so it docks at heading nought and the station is to starboard,
which `simulation-check.mjs` relies on; the engine is at `(9, 16)` with the
two stern ring tiles decked so its bell *is* the stern and nothing of the
ship is aft of it. Moving anything moves `PLAYTEST_HASH` and
`PLAYTEST_PARTS` in `fixture.rs`, the hob's tile in `ship-check.mjs` and
`ship-layout.mjs given` (`(6, 7)` on the twenty grid, `(16, 17)` on the
lobby's forty), and possibly `world::fixture::REFERENCE_CHECKSUM`.

`ship-layout.mjs simulation` and `... station` are the pictures — `mode=1`,
no ship built by hand, the playtest ship docked at its spawn an hour in.
They are the only moments that show the fittings and the station's rooms at
all, since every other game moment builds the flyer.

## Fittings are the pictures the room has none of

`crates/ship/src/fittings.rs` draws what `hull` and the room between them do
not: the plain wall and the diagonal wall, the door (its leaves parted along
the part's long side, in its own frame), the conduit, the helm, the
shelf and the shower, in each part's own frame through `hull::Local` so a
turned part is drawn turned. `world_paint::hull_tiles` asks `hull::part`
first and `fittings::part` second and draws a block for whatever both
refuse — the reactor, the tank, life support, the battery are still blocks.
The design phase deliberately keeps its blocks (`paint.rs`'s module note);
these are for the game's scale only.

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
asks `bims_hit_door()` after `bims_hit_at` said `HIT_SHIP_DOOR = 9`; the
readout's `SPOT_SHIP_DOOR = 17` carries its state through
`bims_ship_door_at(x, y)`. `a_locked_door_is_a_wall_and_an_unlocked_one_is_not`
in `crates/world/src/tests.rs` pins the routing half and
`simulation-check.mjs`'s door section the menu half. Two things that bit:

- **The camera follows James.** A pixel a harness found a door at is
  stale once he has walked to its panel — `findDoor` in the section looks
  again, by asking the room what is under each tile, not by reusing pixels.
- **A station's room kept open while docked draws no doors**
  (`Game::set_doors_drawn(false)` in `World::dock_at`). The joined room has
  the same doors with the people going through them, and two pictures of
  one door would be a door in two states.

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

## The items panel reads two sources, and its icons are a third table

`#items` on the ship page — left, under the agendas — is a row a resource
with an icon and a count, grouped by `ship_storage_of`. `buildItems` in
`web/ship.js` builds it off `ship_resource_count()`, and the icons are CSS
rules in `web/ship.html`, `#items .icon[data-resource="n"]`, one a
`ResourceId` — a new resource wants one there as well as a `RESOURCE_NAMES`
entry, and `simulation-check.mjs` reads the file and fails on a missing one
(the row still appears, with a bare slot).

The counts are not all `ship_cargo`. The cold store aboard is the room's
(`bims_store_veg`/`_tofu`): stocked off the manifest when the world opens
and at every dock, and what is eaten and grown in between never goes back
on `ship_cargo`. `ROOM_HELD` in `ship.js` is which resources read the room,
because that is what the crew can eat; the station panel beside it reads
the manifest, so the two can disagree about vegetables while docked. That
is the manifest gap in `crates/world/src/crew.rs`'s module note showing,
not the panel. The `?` on the panel's corner is wrapped in `.explains`
rather than positioned by `.what` — that class is crew.css's, and a rule
for it here fails `tray-check.mjs`.

## Stew for the store is a third count, a third target, and the cook row's

`Room::stew` is pots of stew on the shelf, beside `veg` and `tofu`, and
`manager::Stock` is the three targets — `Veg`, `Tofu`, `Stew`, codes 0–2,
through `bims_target(which)`/`bims_set_target(which, n)`. The food-units
dial and its 2:1 split are gone: `bims_food_target`, `bims_target_veg` and
friends no longer exist, and the management tab has three inputs
(`#veg-target`, `#tofu-target`, `#stew-target`, in `#keeps`) on both pages.
**The stew target starts at nought** on purpose: a default above it would
have the crew cook the store down from the first morning and re-roll every
probe that pins where they stand.

Two chains in `task.rs` hang off it, both sharing the meal chain's steps
where they can:

- **`Kind::Batch`** is the cook row's stew errand and the hob menu's "Cook a
  stew for the store" (`bims_stock_stew`, not `bims_make_stew`, which is
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
(`targetInput` in `web/crew.js`; the ids `veg-target`/`tofu-target`/
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
seat, by the job or by the player's own Take the helm alike. The room has
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
nothing *is* the accident. `What::FoodPoisoning = 31`, `bims_poisoning`
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
(`bims_can_shower`, `bims_take_shower`, `bims_shower_held_by`), and
`SPOT_SHOWER = 21` names it in the readout and rings it. The simulation
harness has a section for it. Codes 19 and 20 are the bench and the suit
locker, added beside these; `SPOT_NAMES` in `crew.js` is indexed by code,
so every one of them needs its entry there in order, whoever adds it.

## The ship page's left column is in a fixed order

`#left-stack` holds, top down: `#items`, `#game-readout`, `#game-hover`,
`#agendas`. The agendas are the only thing there whose height moves, so
they go last; `#game-hover` keeps its height (`visibility: hidden`, not
`display: none`) when there is nothing under the pointer, or the agendas
would jump every time the pointer crossed a part. Anything new on the
left goes into the stack above the agendas, not at an absolute position.

## The hob is one burner, left of centre

`Room::burner` is where the pot stands and the only ring that lights. It
sits at `centre - 30`, where the left of the old pair was, because the
cooking station is measured off `pot_pos` and centring it would move where
the Bim stands and re-roll every probe seed for a picture.
