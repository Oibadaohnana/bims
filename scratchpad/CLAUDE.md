# The probes

Notes on `scratchpad/` — the native probes and the layout dump. What they
test is in the crates' own `CLAUDE.md` files. The node harnesses that used
to live beside them (`*.mjs`, `stub.mjs`) went with the browser host in the
port to Bevy; what they checked about the screens is `./check smoke` and
`crates/app`'s tests now, and a screen is looked at by running it with
`BIMS_SMOKE_FRAMES` and `BIMS_SCREENSHOT` — see the root `CLAUDE.md`.

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

## A long-run probe's *final* reading is an accident

A Bim past the rest trigger with that trigger switched off nods off where it
stands, and a nod-off claws back about as much rest as the wait for it cost. So
the level does not settle — it oscillates, and where it lands on the last frame
depends on how the day fell out. Adding one walk to the tend chain moved it
from 0.08 to 0.33 with nothing else changed.

Assert the low-water mark, not the last sample. Same family as the layout note
above: when a long-run probe starts failing after an unrelated change, ask
first whether the thing you asserted was ever stable.

## Looking at the room without a window

`scratchpad/layout.rs` dumps one frame of the real draw buffer as SVG and takes
an argument — `start`, `bed`, `table`, `board` — for which moment to catch. `rsvg-convert`
turns it into a PNG. After any layout change this is worth thirty seconds: a
bunk half inside the heads, a chair inside the table, a station on top of the
furniture are all obvious here and invisible in every assertion. It is how the
second bunk's placement was settled.

The designer, the game view and the lobby's World tab are looked at by
running the app itself: `BIMS_SMOKE_FRAMES=200 BIMS_SCREENSHOT=/tmp/x.png
bims simulation`, with `BIMS_POINTER` and `BIMS_KEYS` to get a screen into
the state worth seeing — the root `CLAUDE.md` has the shape of both. That is
how a hull drawn mirrored or a starfield turning with the ship is caught.
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

## "Blocked" and "asleep" look the same from outside

A Bim lying in its bunk is standing *inside* a solid, so `nav.path` from its
position returns empty and `can_reach` — and therefore `can_use_toilet` — is
false. A probe counting "desperate and unable to get to the pan" counts sleeping
Bims and reports a contention problem that does not exist. Measured properly —
awake, desperate, and refused — it is zero for both of them in every seed.

I chased a phantom fairness bug on exactly this. Check `rest_left() > 0` before
concluding anything about reachability.

## Measuring a slowdown needs the other Bim to actually stay put

`stage_meeting` in `scratchpad/crowding.rs` hands *both* crew a route across the
room. Standing one of them somewhere else afterwards with `put_for_probe` does
**not** take that route away — it marches off and gets met either way, so the
"clear run" and the "walk through somebody" runs were the same scenario and came
out identical to the frame. Send it to where it stands as well, and the
comparison means something: 6.40s clear, 6.73s through the other.

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

## The solitude clock has to be pinned, not waited for

`social.rs` runs to ten game days, and the interesting part is all at the far
end. `scratchpad/social.rs` pins it with `leave_alone_for_probe` **every
frame** — once is not enough, because the other Bim comes over for a word and
`Solitude::talked()` puts it straight back to nothing.

Pinning rather than switching autonomy off is the point: a Bim with autonomy
off does not eat either, and a run that starves its subject is measuring
starvation. The same reasoning applies to anything else on a multi-day clock.

