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

Run it in the foreground, in a real terminal. A server launched as a background
job from a non-interactive shell inherits `SIGINT` set to `SIG_IGN`, so Python
never sees the interrupt and Ctrl+C (or `kill -INT`) will not stop it — check
`SigIgn` in `/proc/<pid>/status` if one will not die, and use `kill -TERM`.

`./serve.sh` does the same thing without flakes and takes the same arguments
(`--no-open`, a port). `./build.sh` alone just compiles the wasm into `web/`.

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

## Verifying a change

`cargo build` succeeding proves nothing about the browser. The host is plain
JavaScript that nothing in the build ever type-checks or runs, and a mistake
there fails silently: a parse error in `bims.js` means `boot()` never runs and
the page is a black canvas with no message. Two separate breakages shipped that
way. So:

- `nix flake check` — builds the wasm and gates `cargo fmt`. Necessary, not
  sufficient.
- `nix-shell -p nodejs --run "node --check web/bims.js"` — catches the parse
  errors that produce a black page.
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
  `clamp` in `src/math.rs`.
- The renderer reads the stride at runtime via `bims_stride()` but still assumes
  the field *order* in `src/draw.rs`.
- No strings cross the wasm boundary. The host asks for numbers and does its own
  formatting; that is deliberate, so keep it that way.

## New files need `git add`

`nix run .` and `nix flake check` build from the **git tree**, so a new `src/*.rs`
that is only on disk is invisible to them — the build fails with
`failed to resolve mod <name>: /build/source/src/<name>.rs does not exist` even
though `cargo build` is perfectly happy. `git add -N src/<name>.rs` is enough;
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
When they each carried their own copy of that list, adding a new `src/*.rs`
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
