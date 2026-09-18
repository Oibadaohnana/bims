# The host

Notes on `web/` — the pages and the plain JavaScript that replays the wasm's
draw buffer. Nothing here is type-checked or run by the build; see "Verifying
a change" in the root `CLAUDE.md`, and `scratchpad/CLAUDE.md` for the
harnesses that execute it.

## `boot()` in web/bims.js is one big scope

Every helper in `web/bims.js` lives in the same function scope, and JavaScript
lets two `function foo()` declarations coexist there: the later one silently
wins and the earlier one is unreachable. That is how a scheduler handler ended
up calling the canvas renderer — no error, no warning, the click simply did
nothing. Before adding a function to `boot()`, check the name is free:

```sh
grep -o "^  function [a-zA-Z]*" web/bims.js | sed 's/.*function //' | sort | uniq -d
```

## Tooltips are asked for, never stumbled into

Every tooltip hangs off an affordance: a word underlined with class `asks`, or
a `?` button (`questionMark()` in `web/bims.js`). Never a row, a bar or a
panel — a player crossing the needs panel on the way to the deck should get
nothing. `explain()` is the mechanism; `explainWord()` and `questionMark()` are
the only two ways it is meant to be reached, and `scratchpad/tip-check.mjs`
asserts both halves: the affordances talk, and the containers around them stay
silent.

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

## The names are the host's, and so is drawing them

No strings cross the wasm boundary, which includes "James". The simulation has
crew member 0 and crew member 1; `CREW_NAMES` in `web/bims.js` is the only
place the names exist. They are painted onto the canvas by `paintNames()` after
the shape buffer has been replayed — the draw buffer holds rectangles and
ellipses and nothing else, so text cannot come through it.

`scratchpad/stub.mjs` therefore records `fillText` calls, and the harness
asserts the names are drawn, in the right place, in the right colours. That is
the only part of the rendering a stub can see at all.

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

## The ship page's left column is in a fixed order

`#left-stack` holds, top down: `#items`, `#game-readout`, `#game-hover`,
`#agendas`. The agendas are the only thing there whose height moves, so
they go last; `#game-hover` keeps its height (`visibility: hidden`, not
`display: none`) when there is nothing under the pointer, or the agendas
would jump every time the pointer crossed a part. Anything new on the
left goes into the stack above the agendas, not at an absolute position.
