# Flight

Notes on `crates/flight` — what a design does when pushed, and the closed-form
plan that flies a trip.

> **Since the port to Bevy (September 2026):** the browser host is gone.
> Every name table this file points at in `web/*.js` is now
> `crates/app/src/names.rs`; the pages are the screens in
> `crates/app/src/screens/`; the `bims_*` and `ship_*` exports are methods on
> `bims::game::Game` and `ship::Session`. The node harnesses
> (`scratchpad/*.mjs`, `smoke.mjs`, `stub.mjs`) are gone with the host — a
> check this file says lives in one of them lives nowhere now unless it says
> so in `crates/app`'s tests or `./check smoke`, and is worth restoring as a
> unit test if the thing it guarded moves again.

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
