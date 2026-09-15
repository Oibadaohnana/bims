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
