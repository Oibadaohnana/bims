# Working on Bims

A 2D top-down game: all the simulation is Rust, and so is the window —
`crates/app` is a Bevy binary that draws every crate's shape buffer through
egui and lays the panels out round it. `README.md` explains how the game
itself fits together; this file is about working on it.

## Running it

`nix run .` is the one command: it **builds and opens the window**. There are
six things to run, and each is a name rather than a flag:

| command | `cargo run` | opens |
| --- | --- | --- |
| `nix run .` / `nix run .#game` | `cargo run -- game` | the whole game in order — menu, setup or lobby, world and station, ship design, then the world docked where you said |
| `nix run .#simulation` | `cargo run -- simulation` | straight into the world on the playtest ship |
| `nix run .#design` | `cargo run -- design` | straight into the yard, the playtest ship given, docked where the simulation docks — how a change to the designer is looked at |
| `nix run .#room` | `cargo run -- room` | the behaviour test room — Bims on a deck |
| `nix run .#test` | `cargo run -- test` | the simulation somewhere else each time — docked at a random station somebody lives on, in a random galaxy |
| `nix run .#combat` | `cargo run -- combat` | the simulation docked at a **hostile** station: the people living there are enemies, and a recruited crew member draws its laser and shoots at any it can see. `--combat` is taken too |

`cargo run` (with `-p app`, or bare — `default-members` makes the app the
default) builds from the working tree, which is the one to use while editing,
and `cargo run --release -- test` is the release build docked somewhere new;
`nix run` builds from the **git tree**. `bims --self-check` prints whether
the build agrees with the pinned constants and exits non-zero if not.

**The window wants the shell.** winit and wgpu open the window system's
libraries and the Vulkan loader at run time, and `shell.nix` puts them on
`LD_LIBRARY_PATH`. Build anywhere; run inside `nix-shell shell.nix` (or
`nix develop`), or through `./check`, which re-enters the shell itself. A
`cargo run` that opens nothing, or dies looking for `libvulkan`, is this.

**A run with nobody at the keyboard** is `crates/app/src/dev.rs`:
`BIMS_SMOKE_FRAMES=n` runs `n` frames in a hidden window and exits;
`BIMS_SCREENSHOT=file.png` saves the frame thirty before the end;
`BIMS_POINTER="40:move:600,250;60:click:600,250;90:right:300,400"` and
`BIMS_KEYS="60:Escape,90:M"` drive the pointer and the keys at those frames,
in logical points from the window's top left. `wheel` and `wheelup` at
a point are a notch of the wheel, which zooms. `BIMS_AT_BELT=1` opens the
simulation holding at a belt with its mining site laid out, for looking at
the outside without flying there; `BIMS_FIGHT=1` opens `combat` with the
crew member recruited inside the station's door and one of its people a
few tiles down the corridor, for looking at a fight without walking the
station for one. Move at least a frame before
clicking — egui hit-tests a click against the widgets laid out on the
previous frame. That is how a change to a screen is *looked at* from a
terminal: run it, read the PNG. Two things about it: on Wayland the
compositor decides a hidden window's size and it can be small, so read the
picture for what is drawn rather than where; and a scripted click goes out
as a `WindowEvent` as well as a typed message, because bevy_egui reads the
former and Bevy's own input the latter.

## It is a workspace, and the app is one crate of eleven

Everything is under `crates/`. `app` is the **one binary**: Bevy, egui, the
four screens, the pointer, and every word on the screen. The rest are
libraries. `game` is the room — the Bims' simulation, which `world` runs
aboard the designed ship and the app runs on its own as the test room.
`ship` is the design phase and the game it starts: `Session` (the editor,
then the game), the cameras and the painters. `lobby` is the World tab's
galaxy, a camera and a pick. Beside them are eight libraries that have to
give the same answer in more than one place: `worldgen` (the galaxy, systems
and station blueprints, which a native server will one day generate
identically), `physics` (ship mass, thrust and travel time), `shipdesign`
(what a ship is made of and the rules for putting one together), `flight`
(what a design does when you push it, and the closed-form plan that flies a
trip), `world` (one star system, the ship in it, and the one clock they both
run on), `economy` (money: whole euros, the crew's shared pool, and sums that
must not wrap), `health` (one body's health points, what is wrong with it
and what that costs) and `time` (how long a day is).

Things about that which are easy to get wrong:

- **Profiles only work at the workspace root.** A `[profile.*]` in a member's
  `Cargo.toml` is *silently ignored*. The root has `opt-level = 1` for our
  own crates in dev and `3` for every dependency, because a debug build of
  Bevy is unplayable; keep it there.
- **A bare `cargo test` is the app alone.** `default-members` makes a bare
  cargo command mean `app`, so `cargo test` runs one crate of eleven. Use
  `cargo test --workspace`, which is what `./check` runs. `game` has no unit
  tests; its tests are the native probes in `scratchpad/`.
- **The rules go in `shipdesign` and `world`, never in `ship` or `app`.**
  `ship` may ask whether a part can be placed or a trip can be flown; it may
  not decide, and the app may not either. A native server has to give the
  same answer, and two numbers — `design_hash`, which is what an Accept is
  recorded against, and `world_checksum`, which is what a client will one day
  be verified against — have to come out **identical everywhere**. That is
  why nothing in `shipdesign`'s data or its hash is a `usize` or a float and
  why no `HashMap` is iterated in it. Both are pinned against written-down
  constants in `shipdesign::fixture` and `world::fixture`, by
  `crates/shipdesign/src/tests.rs` and `crates/world/src/tests.rs` one at a
  time and by `ship::session::self_check` (`bims --self-check`) all at once.

  `world_checksum` **rounds its floats onto a grid on the way in**, and that
  is not sloppiness: everything in `flight` that is not arithmetic is `sin`,
  `cos`, `atan2` and `sqrt`, which two platforms' libms may disagree about in
  the last bit. Positions go in at a thousandth of a unit and angles at a
  millionth, which is orders of magnitude finer than any real divergence and
  orders coarser than a rounding one.
- **No strings come out of the rules crates.** The room hands over crew
  member 0 and spot code 6; `ship` hands over a part kind and an issue
  code; `world` hands over a `WorldEvent`. Every word is
  `crates/app/src/names.rs`, and `format.rs` is the only place the euro sign,
  the digit grouping and the clock's colon exist. That is deliberate — a
  server has no words to say — so keep it that way: a new part, event or job
  is a variant there and a name here, and `names.rs`'s tests pin the tables'
  lengths against the enums.
- **`crate::time`, never `time::`.** `crates/game/src/clock.rs` reaches the
  `time` crate through the crate root, because the probes link nothing and
  stand a plain `mod time;` over the same file — see `scratchpad/modules.rs`.
  Spelled `time::` it would only resolve through the extern prelude and every
  probe would need a `--extern` and a build step behind it.

## How the app is put together

- **One frame is one system per screen**, in `EguiPrimaryContextPass`:
  step the simulation, lay the panels out, read the pointer, paint the
  shapes, put the words on top. `Screen` in `main.rs` is the state machine
  and each screen's plugin runs only in its state.
- **The shape buffer is drawn by egui**, not by a Bevy mesh:
  `shapes.rs` tessellates the twelve floats a shape into an `egui::Mesh`,
  and `canvas::paint_shapes` adds it to a painter clipped to the canvas —
  the background layer for a canvas between the panels, the panel's own
  painter for one inside it (the lobby's galaxy and diagram). Bevy draws the
  clear colour and nothing else. This matters because egui panels paint an
  opaque fill: a Bevy mesh under a `CentralPanel` is a mesh nobody sees,
  which is how the galaxy preview was blank for a build.
- **Anti-aliasing is feathering, in `shapes.rs`, and nothing else.**
  bevy_egui paints into the window's *unsampled* target, so a camera's
  `Msaa` never reaches the mesh; every edge is instead ramped a pixel wide
  from its colour to transparent the way epaint draws its own shapes, and a
  stroke thinner than a pixel is drawn a pixel wide and fainter. The pixel
  is `painter.pixels_per_point()`, so it stays one device pixel under any
  UI scale. A new kind of shape has to go through `fill` or `stroke` there,
  or it comes out jagged beside everything else.
- **The UI scale is egui's zoom factor** (`theme::ui_scale_row`, on the Esc
  sheet): it scales the type, the panels and the canvas alike, and
  bevy_egui divides the pointer by it, so nothing in the screens has to
  know. A `BIMS_POINTER` script is in points *before* the zoom, so drive a
  screen at 100% or the clicks land elsewhere.
- **The pointer is egui's.** `canvas::Pointer::read` takes the pointer out
  of the egui context, and `Pointer::on(rect)` answers `None` whenever egui
  wants it — over a panel, a window, a menu — so a click on a panel is the
  panel's and a click beside it is the canvas's by the same rule egui uses.
  Do not read Bevy's `ButtonInput<MouseButton>` for the canvas: the two
  would disagree about a click on a floating panel.
- **egui's context lock is not reentrant.** `ctx.input(|i| ...)` holds it;
  calling `ctx.is_pointer_over_egui()` or anything else on `ctx` *inside*
  that closure deadlocks, and epaint's debug build panics after ten seconds
  saying so. Ask everything you need of `ctx` before the closure.
- **The first canvas size is a fit, not a resize.** A session is made
  before the panels have been laid out, so its cameras start at the window's
  size; the first frame that knows the real canvas calls `Session::fit`,
  which starts the views again from that size (the whole build area, the
  whole hull), and every later change is a plain `resize`. Without that the
  designer opens half hidden under its own checks panel.
- **The crew's panels are one module**, `crew.rs`, shared by the room and
  the game, and the screen hands the room its coordinates: on the room's
  screen a canvas point is the room's through `Game::view_scale/offset`; on
  the ship's it is `Session::room_point`, the pointer read back through the
  ship's camera and heading. Every `Game` method that takes a point takes a
  room point.
- **A highlight is not a tooltip.** Resting on a row that names a fixture
  rings it on the deck (`CrewPanels::points`); nothing pops up. The ring is
  worked out afresh every frame from what is hovered, so a panel folding
  away under the pointer cannot leave one lit. Tooltips hang only off an
  underlined word (`theme::asks`) or a `?` (`theme::question_mark`), never
  a row or a bar.
- **A fixture menu opened by a press must not be shut by it.** `Menu::fresh`
  is that guard: the click-away check skips the frame the menu opened on.
- **The default font has no arrows.** `▾`, `←`, `‖` come out as boxes;
  the panels say `Hide`, `< Back`, `||` instead. Check a new glyph on
  screen before trusting it.

## Verifying a change

`cargo build` succeeding proves nothing about the window: a panel laid out
over the canvas, a mesh under a panel, a click that lands on the wrong
layer — none of that is a type error.

**`./check` runs all of it.** The quick tier — `./check` — is the git-tree
check, `cargo fmt`, the workspace build, `cargo test --workspace`, clippy on
the app, and a smoke run of each of the six windows (sixty frames, hidden,
with a screenshot of the room, the simulation, the yard and the combat
dock left in `target/check/`); `./check full` adds the native probes (compiled fresh into `target/probes/`,
never the stale binaries in `scratchpad/`) and `nix flake check`.
`./check <step>...` runs a subset, `./check --list` explains each. Full
output is under `target/check/`; only the failing lines are printed. "Done"
means `./check` is green, and a step that was red before the change should
be said so out loud rather than folded into the report. It re-execs itself
under `nix-shell shell.nix` if the toolchain is not on PATH, and a global
Claude Code hook (`~/.claude/hooks/nix-toolchain-env.sh`) puts it there for
every session (`.envrc` does the same for a human with direnv).

Clippy is run on `app` only, with warnings as errors: the rules crates
predate clippy here and carry style lints that are not bugs. New code in the
app is written under it.

For the simulation itself, the modules compile natively. A scratch `main.rs`
that pulls them in by `#[path]` and declares them at the crate root (so
`crate::room` still resolves) can drive `Game` directly — run a task to
completion, assert the Bim ends up where it should — and can dump the real
draw buffer as SVG to look at the room without a window
(`scratchpad/layout.rs`).

## Editing gotchas

- Beware `sed`/`perl` substitutions anchored on leading whitespace: a pattern
  indented four spaces also matches inside a six-space-indented line. Anchor
  on something unique, and re-read the result.
- `f32::clamp` panics when its bounds cross. Use the branchless `clamp` in
  `crates/game/src/math.rs` in the room: a panic in a step is a stall with no
  message.
- The replay in `crates/app/src/shapes.rs` assumes the field *order* in
  `crates/game/src/draw.rs`, and its test pins `STRIDE` against all three
  painters'.

## New files need `git add`

`nix run .` and `nix flake check` build from the **git tree**, so a new
`crates/*/src/*.rs` that is only on disk is invisible to them — the build
fails with `failed to resolve mod <name>` even though `cargo build` is
perfectly happy. `git add -N <file>` is enough; it does not commit anything.
A whole new crate is the same trap with a louder failure: `members =
["crates/*"]` matches a directory the git tree does not have. `./check
tracked` is the step that catches both.

## Where the rest is

The notes are split by directory, and Claude Code loads each one **on
demand** — the moment a file under that directory is read or edited. Work
that crosses a seam wants the other side's notes read as well: a chain in
the room, say, is also a step of the world's clock and a menu in the app.

| file | what it holds |
| --- | --- |
| `crates/game/CLAUDE.md` | the room: routes, needs, errands, the two Bims, the diary, doors, the bay, stew, the helm as a job |
| `crates/shipdesign/CLAUDE.md` | the rules: parts, layers, power, cargo, money, the hash, the validator, the playtest ship |
| `crates/world/CLAUDE.md` | the world: the clock and its stages, stations, docking, the crew aboard, the helm, crafting, EVA |
| `crates/ship/CLAUDE.md` | the designer and the game view: the camera, the painters, drags, `Session` |
| `crates/flight/CLAUDE.md` | a plan is read, never integrated; the two pinned placeholder numbers |
| `crates/health/CLAUDE.md` | one body's health, and why a step of any length gives the same answer |
| `crates/lobby/CLAUDE.md` | "has a station" is answered by generating the system |
| `scratchpad/CLAUDE.md` | the native probes, and how to look at a room without a window |

## Never use `|` as a perl `s|…|…|` delimiter here

Two files were corrupted by `perl -0pi -e 's|…|…|'` where the pattern or the
replacement contained a `|` — a markdown table row, a `||`. The delimiter
closes at the first one, and what is left is pasted somewhere unrelated.
Interpolation is the other half of the trap: `${…}` inside a double-quoted
perl replacement is *perl* interpolation.

Use the Edit tool for anything containing `|`, `$` or backticks, and re-read
whatever you touched.

## `grep` here is `ugrep`

The shell's `grep` is a function wrapping `ugrep --ignore-files`, and it
has skipped whole files before for no reason it would say. If a search
comes back empty for something you can see, use `command grep`.
