# Health

Notes on `crates/health` — one body's health for the ship game.

> **Since the port to Bevy (September 2026):** the browser host is gone.
> Every name table this file points at in `web/*.js` is now
> `crates/app/src/names.rs`; the pages are the screens in
> `crates/app/src/screens/`; the `bims_*` and `ship_*` exports are methods on
> `bims::game::Game` and `ship::Session`. The node harnesses
> (`scratchpad/*.mjs`, `smoke.mjs`, `stub.mjs`) are gone with the host — a
> check this file says lives in one of them lives nowhere now unless it says
> so in `crates/app`'s tests or `./check smoke`, and is worth restoring as a
> unit test if the thing it guarded moves again.

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
