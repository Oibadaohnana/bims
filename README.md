# Bims

A 2D top-down game written in Rust, running in the browser via WebAssembly.

One character — a Bim — pottering about a compartment on a ship. Told to, it
will cook itself a meal from start to finish and clear up after it, take itself
to bed, or go and use the heads; left alone, it decides for itself which of
those to do. A clock runs the whole time, and the deck goes dark at night.

## Running it

```sh
nix run .
```

That builds the game, serves it, and opens it in a browser tab. `Ctrl+C` stops
the server. Pass a port to pin one (`nix run . -- 3000`), or `--no-open` to keep
it out of your browser.

Running it again while it is already up starts neither a second server nor a
second tab. It recognises its own build by the wasm being served and just tells
you where it is. That matters because an old tab keeps its own copy of the game
running for as long as it stays open, so stacking them leaves you staring at two
Bims wondering which is real — pass `--open` if you want another tab anyway.

If the port has an *older* build on it, it says so and stops rather than quietly
showing you stale content. If the port is taken by something unrelated, it moves
to the next free one.

Without flakes, `./serve.sh` does the same thing and takes the same arguments.

The rest:

```sh
nix develop            # a shell with the toolchain
nix build              # the playable site lands in result/share/bims
nix flake check        # builds the wasm and checks formatting
```

`./build.sh` alone compiles the wasm into `web/`. Both scripts drop into
`shell.nix` for the toolchain if `cargo` and `lld` are not already on PATH, so
no global install is needed. `nix develop` uses that same `shell.nix`, so there
is one list of development tools rather than two that drift apart.

Opening `web/index.html` as a `file://` URL will not work: the page fetches
`bims.wasm`, and `fetch` is blocked on file URLs. Serve it over http.

Both entry points use `dev-server.py` rather than `python3 -m http.server`,
because browser caching bites hard here. Files served out of the Nix store
carry an mtime of 1970, and with no `Cache-Control` header the heuristic
freshness rule — about a tenth of a file's apparent age — decides the page is
good for decades and stops asking for it. You rebuild, reload, and are still
looking at a build from hours ago. `dev-server.py` sends `no-store`, and the
page loads `bims.js` and `bims.wasm` under a per-load query string, so a cache
poisoned before any of that was in place cannot outlive it either.

## Controls

| | |
| --- | --- |
| Click the fridge | Menu: what is left, **Make a stew**, **Make a bowl**, the door |
| Click the stove | Menu: turn on/off — the Bim walks over and flips it |
| Click the bunk bed | Menu: **Nap** (30 min) or **Sleep** (6 hours) |
| Click the dishwasher | Menu: what is stowed, and **Run now** |
| Click the hydroponic bay | Menu: the trays, **Automate**, and what to plant |
| Click the toilet | Menu: **Use** — and a wash at the basin after |
| Click the bathroom door | Menu: open/close, lock/unlock |
| Right-click a fixture | The same menu, on the other button |
| `1` | Select the Bim (control group 1) |
| `r` | Recruit the Bim, or let it go — see below |
| Drag a box over it | Select the Bim |
| Click it | Select it — a click is just a box of no size |
| Click empty floor / `Esc` | Deselect |
| Right-click the floor | Send the selection there — opens a door on the way if it must |
| Tray, bottom left | **Schedule** — paint the day; **Management** — autonomy, speed, food to keep |
| Let the Bim decide | Whether it starts errands on its own when idle |
| Speed slider | Run the simulation from 1x up to 24x |
| Rest on an underlined word or a ? | It explains itself, after a third of a second |

Nothing on the page explains itself in prose any more. The explanations are in
tooltips, and a tooltip is always *asked for* rather than stumbled into. Two
rules, and nothing else pops anything up:

- **An underlined word.** Where the thing already has a name — each need, Health,
  "Let the Bim decide", "Speed" — the name carries it, dotted-underlined and
  with the help cursor. Hovering the row, the bar or the panel does nothing.
- **A "?".** Where there is no word of its own to underline, a small question
  mark sits beside the controls: the schedule's is at the end of the brush row,
  past **Sleep** and **Everything**.

Either opens after the pointer has rested on it for 300ms. The delay is the
point: crossing a panel on the way somewhere else sets nothing off. The one
number inside a tooltip that could go stale — the rested-enough threshold in
the schedule's — is filled in from wasm rather than written into the markup.

Either button opens a fixture's menu, and doing so leaves the selection alone;
a *sweep* across one is still a marquee. Right-clicking bare floor is still a
move order — the hit test decides which of the two a right-click meant. Nothing
is greyed out for being busy any more: a new errand takes over and the old one
goes on the agenda (below). Items are only ever greyed out for a reason of their
own — a locked door, a dishwasher with nothing in it.

## Nothing is remote

Every menu item is an errand, including the ones that look like switches. Asking
to open the fridge, work the hob, open or lock the bathroom door, or start the
dishwasher sends the Bim walking to that thing; the state changes at the moment
its hand arrives and not before. There is no way to reach into the room from
outside it.

That falls out of one shared pair of steps — walk to it, put a hand on it —
carrying which switch on the task rather than in the step, so a switch errand
queues, resumes and shows up on the agenda exactly like a meal does. It also
means a toggle reads the state at the moment the Bim touches it rather than when
the player asked, which is the honest reading: ask for "turn on" and then change
your mind twice while it walks, and what you get is what the hob was actually
like when it got there.

Where a fixture has two sides — the bathroom door — the Bim goes to whichever
panel it is nearest, so it can let itself out as readily as in.

## Interrupting, and getting back to it

Send a selected Bim somewhere, or start it on something else, and the new thing
wins immediately. What it was doing is not thrown away: it goes on a queue with
its progress intact, and the Bim picks it up again as soon as it is free — after
walking to wherever it was sent, or after the errand that displaced it.

The queue is a stack. Each interruption goes on the *front*, so a chain always
resumes directly after whatever displaced it: interrupt a meal with a nap and
the nap with a trip to the heads, and the order back out is heads, nap, meal.

Resuming is the part with a trap in it. Standing steps assume the Bim is already
in the right place — `Chop` chops whatever is in front of it — so dropping the
Bim back on the step it was interrupted at would have it chopping thin air in
the middle of the floor. Instead, resuming rewinds to the most recent *walking*
step of that chain and lets the Bim walk back to the bench first, then drops
into the interrupted step with the elapsed time and the count of knife strokes
it had already done. The steps skipped on the way in are exactly the ones whose
work the room is already holding: a chopped vegetable stays chopped, a full pot
stays full.

What the room cannot hold is what was in the Bim's hands, and whether it was
sitting on something — `set_scripted(false)` throws both away. So those are
snapshotted too, and put back after the walk. A Bim interrupted mid-meal walks
back to the table, sits down again with its fork, and carries on eating.

Two cases need the world tidied on the way out, or the Bim would be stranded:
being lifted out of the bunk, and being let out of the heads. The second matters
because the Bim locks the door behind itself, and a locked door is a wall to the
pathfinder — being called out of a locked room unlocks it first, or the order
would have nowhere to go.

### The agenda

The panel over the top-left of the deck is the checklist for all of this. One
row per chain — the one running first, then the queue in the order they come
back — each with a progress bar at the left-hand end, a tick box that is filled
for the chain actually running and empty for one that is waiting, and the name.
A queued row keeps the progress it had when it was put down, so you can see the
meal sitting at 32% while the nap it was interrupted by runs at the top. The
panel hides itself when there is nothing on.

## Make food

The whole chain, about fifty seconds of it, runs off one menu item. Every beat
is animated rather than implied: the Bim walks to the fridge and opens it,
takes out a vegetable and shuts the door, carries it to the board, fetches a
knife from the counter drawer, chops — the vegetable visibly shrinking as
slices pile up beside it — puts the knife down, gathers the slices into the
pot, turns the hob on, waits while it bubbles and the contents turn from raw
green to stew, fetches a plate and spoon from the drawer, spoons three helpings
across onto the plate (the pot keeps the rest), turns the hob off, carries the
plate to the table, sits down, eats it with cutlery until the plate is empty,
sits a moment, gets up, gathers the plate and the cutlery, carries them to the
dishwasher, opens it, stacks them, shuts it — and, if that was the tenth plate,
presses the button — then goes back to wandering.

## Two recipes, and a cold store that runs out

There are two things the Bim can make, and both start the same way — fridge,
board, knife — before parting company:

- **A stew.** Two vegetables, fetched one at a time, chopped one after the
  other, tipped in the pot and cooked. The second trip skips the drawer,
  because the knife is already in hand.
- **A bowl.** One block of tofu chopped into cubes, with the salad that came
  out of the fridge alongside it, tipped into a bowl and eaten cold. No pot, no
  heat, and about two thirds the time of a stew.

The loop and the fork in the chain are the same one mechanism: `Step::next` is
still a straight line, and `Task::next_step` overrides it in the three places
where the recipe matters — round again for the second vegetable, skip the
drawer on that second trip, and turn off towards the bowl instead of the pot.

The cold store is **two counts, not one**: vegetables and blocks of tofu, and
they are not interchangeable. A stew is two vegetables; a bowl is one block of
tofu with one vegetable as the salad. So either can be the thing that runs out,
and a store of nothing but tofu feeds nobody. It starts at twenty and ten —
two to one, the ratio the Bim eats at — and the fridge menu says what is left
of each.

The only thing that puts any back is the hydroponic bay, below. Left to itself
the store still runs down: two meals a day against ten days of greens, and
after that the Bim cannot eat at all, which is what the next part is about.

## Going hungry

Staying hungry costs the Bim, in three stages. What is measured is *time spent
with nothing in it* rather than how empty the bar is — being hungry for an hour
is just being hungry — so the stages are thresholds on a clock that runs while
the stomach is empty and winds back three times as fast once it is eating again.

| stage | after | walks at | tires | health |
| --- | --- | --- | --- | --- |
| Mild malnutrition | 8 h empty | ×0.85 | — | — |
| Malnutrition | 16 h | ×0.70 | ×2 | — |
| Extreme malnutrition | 24 h | ×0.55 | ×3 | 100 → 0 over a day |

Only the last stage costs health; the first two are a warning, and feeding the
Bim at any point walks the whole thing back. Health regrows over two days of
eating properly. At zero the Bim dies: whatever it was doing is dropped, the
queue is emptied, it stops where it stands and is drawn cold and flat on the
deck, and nothing will take an order from it again.

The stages compound on purpose. Tiring three times as fast means a starving Bim
spends more of its day asleep, which is less of it spent doing anything about
being starving. Unlike the needs, that clock keeps running while it sleeps —
you do not stop starving because you are asleep.

Health sits under the needs on the right, with the stage named underneath it,
and the state of its sleep named under that.

## Going without sleep

The same shape as going hungry, and measured the same way: time spent on
nothing, not how low the bar is.

| stage | after | errands take | and |
| --- | --- | --- | --- |
| Sleepy | 4 h on empty | ×1.25 | — |
| Sleep deprived | 10 h | ×1.50 | — |
| Past it | 18 h | ×2.00 | drops off where it stands |

The slowdown is not a multiplier on a timer. A step that finishes has a chance
of having to be done again — the Bim stands there for as long as that step
takes and then starts it over — and the chance is set from the target rather
than guessed at. A step repeated with probability *p* takes `1 / (1 − p)` times
as long on average, so the three stages use *p* = 0.2, 0.333 and 0.5. Measured
over a dozen runs of the same meal, that comes out at ×1.29, ×1.59 and ×1.93.
The fumbling is random; what it costs over a whole errand is not.

The status line says *Lost the thread of it…* while it is stalled, because
otherwise a chain that has silently doubled in length reads as the game having
hung.

At the worst stage the Bim also drops off on its feet, for fifteen minutes at a
time, roughly once every three quarters of an hour upright. It recovers rest at
the proper sleeping rate while it is out — which is worth about four per cent —
and it keeps hold of whatever it was carrying and whatever route it was on, so
it carries straight on when it comes round.

What a nod-off does **not** do is clear the state. That lifts only when the Bim
has properly slept and is back above 80% rested, which four per cent at a time
will never reach. The only way out is a night in bed, which means the timetable:
wipe the strip and a Bim will run itself into the ground and stay there.

## The hob turns itself off

A hob left lit with nothing coming up to heat shuts itself off after fifteen
game minutes. The menu says so while it is counting — *left on — cuts out in
12 min* — because a ring going out on its own with no explanation reads as a
bug rather than as a safety feature.

The word doing the work is *cooking*, and it means something in the pot that has
not finished heating — not merely that the ring is lit. The distinction matters,
and the margin is thinner than it looks. A meal keeps the hob on for about
fifteen game minutes in total, which a naive "fifteen minutes lit" rule would
cut short at the worst possible moment. But the stew is only *coming up to heat*
for the first five of those: the rest is the Bim fetching a plate, spooning out
a helping and reaching for the knob. Counting only the idle time leaves the Bim
turning the hob off with five minutes still on the clock, which is what it does.

## The dishwasher

A unit in the galley counter front, between the drawer and the hob. It stows ten
plates, one per meal, and the tenth one in is what starts a cycle. The cycle
takes two hours on the world clock and runs entirely on its own: the Bim walks
away the moment it has pressed the button and is free for all of it.

Plates are counted in two places, which is the only part of it worth explaining.
`loaded` is the dirty stack waiting to go round; `washing` is what is going
round now. Starting a cycle moves one to the other. Without the split, a plate
cleared away *during* a cycle would be washed and then thrown out with the load
it was never part of; with it, that plate simply waits for the next one. The
pips across the front show both — bright for a plate waiting, dim for one going
round — so a running machine looks full rather than looking empty.

Everything else falls out of that. A cycle that cannot start (nothing in it, or
one already running) is simply refused, and the next Bim to finish a meal tries
again. The rack goes back into the galley stores when the cycle ends, which is
the one bit of hand-waving in here: nobody unloads it.

## The hydroponic bay

Five trays along the bottom-left wall, and the only thing aboard that puts food
*back* into the cold store. Right-click it for its two controls.

**Automate** puts the bay on the manager's target (below). While the store is
under it the bay has work, and the Bim walks over and does it a tray at a time:
lift anything ripe, then plant whatever is missing. Greens come up in **one
day**, soy in **a day and a half** and is pressed into tofu.

What it plants is decided by which of the two the store is furthest behind on,
as a *share* of what was asked for. The share matters: measured in plain
numbers the bigger target would always be the shorter one and the bay would
fill all five trays with greens, so the tofu it is just as far behind on never
went in. Measured proportionally the trays come out at roughly the ratio that
was asked for, which is the point of a food unit having two halves.

**Hibernation** is what happens when both marks are met: nothing grows, nothing
is planted, nothing is lifted, and the trays hold exactly what they hold. The
lights over them go out, which is how it reads across the room. Drop below the
mark again and it picks up where it stopped, part-grown plants and all.

**Plant greens / soy in every tray** is the override: a standing order that
ignores both the target and hibernation, for when you want a bay full of soy
whatever the store says. Click it again to lift the order and go back to the
target.

Both controls are *settings* rather than errands — the same kind of thing as
the timetable or letting the Bim decide. The deciding is the player's; every
bit of the doing is still the Bim's, on foot, one tray at a time.

## The manager

**Management** in the tray holds one number so far: **how much food to keep**.
It is in *food units*, and a unit divides two to one — two thirds vegetables,
one third tofu — so asking for **99** sets the target to **66 vegetables and 33
blocks of tofu**. The field shows the split as you type it, read back out of
wasm rather than worked out twice.

That target is what the bay plants to, and it is the only demand there is for
now. The manager is where the rest will go as they arrive: one row per thing
the place is told to keep up.

A note on the arithmetic, since the two readings of "food unit" differ. A unit
here is one *item* split two to one, so 99 units is 99 things — 33 days of
greens at two a stew, or about seven weeks for one Bim. Read instead as "99
meals' worth", at two vegetables and a block of tofu each, it would be 198 and
99. The first is what the field does, because that is what was asked for.

## The timetable

The tray at the bottom left folds away and has two tabs. **Management** holds
the controls that used to sit in the page header — whether the Bim decides for
itself, the speed, and what to keep in stock. **Schedule** is a strip of
twenty-four hours you paint
with one of two brushes: blue for sleep, grey for everything else. The hour the
clock is on is ringed.

It comes with a night already painted in: **22:00 through to 04:00**, six hours,
the same length as a night actually is. Wipe it with the grey brush if you want
the Bim left entirely to its own devices.

Only sleep is timetabled so far, and **the timetable is the only thing that
sends the Bim to bed**. Running low on rest is not a reason in itself: the level
decides whether a scheduled night is worth taking, not whether to have one. Wipe
the strip entirely and the Bim never sleeps at all, and rest sits at zero —
which costs it nothing, there being no penalty attached to it, but it will not
put itself to bed.

The timetable is still not an order. When the clock walks into a painted block
the Bim adds a sleep to the **back** of its agenda — behind whatever it is doing
and behind anything already queued, unlike an interruption, which jumps the
front. A standing instruction waits its turn.

Two rules keep it from being silly:

- **Already more than 80% rested, and it ignores that block.** Being sent to
  bed nearly rested means a walk to the bunk, a climb and a walk back for a few
  minutes' lie-down. It declines that particular block rather than the
  timetable as a whole.
- **Fully rested, and it gets up** — whatever the hour says. So a scheduled
  early night taken at three quarters rested runs about ninety minutes rather
  than the full six hours, and the Bim is up and about again.
- **Hungry, and it eats first.** A sleep waiting at the front of the agenda
  stands aside while food is past its trigger and there is something to cook:
  turning in starving costs the Bim six hours of losing health and it wakes no
  better off, where the meal costs it three quarters of an hour and puts the
  need away entirely. The sleep is held, not dropped, and goes ahead the moment
  the plate is cleared. It only stands aside while a meal is actually to be
  had, and while the Bim is free to go and have it — with the cold store empty,
  the galley behind a locked door, or autonomy switched off, bedtime goes ahead
  as it is. Waiting on a meal nobody is going to cook would be a Bim that never
  sleeps at all.
- **Needing the heads, and it goes first.** You go before bed. The same rule and
  the same guards: the sleep is held while the restroom need is past its trigger
  and the pan is actually reachable, and a locked door is not a reason to keep a
  tired Bim up. Nothing worse than an early start hangs on it — the need and its
  accidents are both frozen while the Bim sleeps — but a Bim that turns in at
  nothing per cent wakes at nothing per cent and has to run for it, and now it
  simply goes first.

A block fires once however long it is: painting the whole day sends the Bim to
bed once, not twenty-four times. Painting sleep onto the hour it already is
takes effect immediately rather than waiting for the next hour to tick over.

A block fires on the way *in*, which has one edge worth knowing: painting the
whole day makes a single block with no ordinary hour to re-arm it, so it asks
once and never again. Blocks the clock walks into are the ones that work.

The default night is what keeps the Bim's drifting body clock (below) anchored,
and it does it the way daylight does for an animal. Six nights running it went
to bed at 22:01, 22:00, 22:00, 22:05, 22:03, 22:00, arriving at ten o'clock
between 8% and 14% rested every time and getting up around a quarter to four,
fully rested and a little short of the six hours. A 25-hour rhythm pulled onto a
24-hour day is exactly what entrainment looks like.

## Recruiting

Press **r** and the Bim is under direct orders. It stops doing anything of its
own accord: no errand from a need, no sleep from the timetable, nothing picked
back up off the queue, and none of the pottering about it does between jobs. It
stands where it is put until told otherwise. Press **r** again to let it go.

What recruiting does *not* touch is the going-without. The needs drain exactly
as before and every stage of hunger and drowsiness bites exactly as hard — a
recruited Bim left alone for two days still ends up at the worst stage of both,
and walks and fumbles accordingly. Being under orders is not being looked after.

Everything the player asks still works: move orders, fixture menus, all of it.
Whatever it was in the middle of when recruited is left to finish rather than
cancelled, since throwing away a half-cooked meal for tidiness would be worse,
and a right click interrupts it anyway. The queue is kept rather than emptied,
so letting the Bim go picks up where it left off.

It shows in two places, because one of them is easy to miss: a badge in the
header, and a broken amber ring around the Bim itself, in a colour used for
nothing else and drawn whether or not the Bim happens to be selected.

## Needs, and the day they make

Four levels run the Bim's day, shown down the right-hand side of the deck and
always visible: **Rest**, **Food**, **Restroom** and **Cleanliness**. Each sits
at 1 when the Bim is comfortable and falls as the day goes on. One dropping
below 10% is what sends the Bim to bed, to the fridge or to the heads; doing the
thing fills it back up, and a meal fills hunger completely however empty it was.

The restroom need covers both ends of the business deliberately, rather than
being two numbers. They come up together, they are dealt with in one trip, and
splitting them would only mean two bars that always move as one.

Cleanliness is the odd one out and is described under [Mess](#mess): it has no
clock of its own and no errand behind it, and follows the state of the deck the
Bim is standing on.

### Where the rates come from

Nothing here is eyeballed. The day is 1440 game minutes and a night is 360, so
the Bim is awake for 1080. Each need should come round a set number of times in
that, which fixes everything else:

```
slot  = waking minutes / times per day − what the errand itself costs
drain = (1.00 − 0.10) / slot        full, down to the trigger
```

The subtraction is the part that is easy to miss. A cycle is the draining *plus*
the doing, so if the whole slot goes on draining there is no room left for the
errand and the day comes up short — two meals an hour apart in the making are
two hours not spent getting hungry. The costs are measured off the chains rather
than guessed, and measured to the moment the need is *full* rather than to the
end of the chain: a meal carries on for another ten minutes stacking the
dishwasher, and the Bim is getting hungry again through all of it.

| need | per day | slot | errand costs | drain per waking minute |
| --- | --- | --- | --- | --- |
| Rest | 1 | 1140 | 6 | 0.9 / 1134 |
| Food | 2 | 540 | 48 | 0.9 / 492 |
| Restroom | 3 | 360 | 14 | 0.9 / 346 |

Recovery is spread across the act itself, so a bar fills while the Bim is doing
the thing rather than jumping at the end: six hours in bed carries Rest from the
trigger back to full exactly, and a meal or a visit covers the whole range.

### The body clock runs slow

Rest is the one need whose slot is not the ship's waking day. It uses 1140
minutes rather than 1080, which makes the whole sleep cycle **25 hours** — six
hours in bed and nineteen out of it — against a 24-hour day.

That one-hour difference is the point. Drained at exactly the rate that empties
it in a day, bedtime lands on the same hour for ever: the Bim has no rhythm of
its own, only the clock's. An hour slow and its body clock free-runs, so bedtime
walks about an hour later every day and the Bim keeps resettling — roughly what
an animal left in the dark does. It is also what makes the timetable worth
having, since a schedule is now something to pull a drifting Bim back onto
rather than a restatement of what it was going to do anyway.

Only rest drifts. Food and the restroom need still drain per waking minute, so
the Bim still eats twice and visits three times in a waking day whatever hour it
starts.
The one figure that moves is how much of a *calendar* day is spent asleep: 360
minutes in every 1502 is 5.75 hours a day rather than a flat six.

Nothing drains at all while the Bim is asleep. That is deliberate, and it is
what makes the counts come out: if the restroom need ran down overnight the Bim
would wake already past the trigger and three visits a day would drift into four.
Sleeping through the night is the point of sleeping. The accidents that hang
off the restroom need are frozen with it, for the same reason: the one-in-ten
an hour rolled through a six-hour night would wet the bed about half the time,
for a need that is not even moving while the Bim sleeps.

Run it and the Bim eats twice, visits the heads three times, and — because the
default night is painted in — sleeps once, at ten.

### What it does not do

A need arising while the Bim is mid-errand waits for that errand to finish
rather than interrupting it, and a walk you ordered counts as having something
on. So a bar can sit at zero for a while — cross the threshold as a meal starts
and the Bim finishes its dinner before going to the heads.

## Mess

A bar at zero is not the end of anything, it is the start. Two of them have
something waiting past the bottom, and both are reached by a clock rather than
by the level: the level running out starts the clock, and each stage is an hour
further into it. The needs panel shows the bars; the lines under the health bar
name the stage.

### Needing the heads

The restroom need has three urges under it, and they only ever come up when the
Bim cannot go — shut in, under orders, or with autonomy switched off. Left to
itself it sets off at 10% and none of this happens at all.

| below | urge | what it does |
| --- | --- | --- |
| 25% | mild | hops about on the spot now and then |
| 10% | medium | hops about more, and a **one in ten chance an hour** of wetting itself |
| 0% | extreme | holds on for **one hour**, and then does not |

Wetting itself takes the edge off — the need goes back to 45% — and leaves the
Bim and the tile it is standing on in a state. The hour at the extreme urge ends
in the worst of it: the need goes back to *full*, because relief is relief
however it comes, the Bim is covered, and the tile under it goes straight to the
bottom of the scale. The hopping is the only warning the player gets, which is
why it is there.

### The deck, tile by tile

The deck is scored tile by tile on the same 52-pixel grid it is drawn on. A tile
starts at **10** and only ever goes down: an accident takes one straight to
**−100**, and so does being sick on one; wetting one costs 35. **Nothing cleans
up yet** — that is a job for something that does not exist, and until it does a
mess is permanent. Being sick is the one a Bim does over and over, so a Bim at
the worst stage leaves a trail of ruined tiles behind it as it moves away from
each in turn. The Bim carries its own share around separately, 0 to 1, because
a Bim that soils itself takes the mess with it when it walks away. A wash at
the basin gets half of it off; nothing else does anything.

### Cleanliness

The fourth bar has no clock of its own. It follows the average of the tiles
within three of where the Bim is standing — a seven by seven block — and how
filthy the Bim itself is:

```
average of 49 tiles ≤ 0   →  falls, a full bar in an hour at exactly 0
                             and 11× that on a fouled tile (1 + |average|/10)
the Bim itself filthy     →  falls, up to 5× the base rate at fully covered
both clean                →  fills, four hours to full
```

The radius cuts both ways, and it is worth being clear about which. A wider
block reaches further — a mess three tiles off now counts for something — but it
also *dilutes*, because what the bar follows is the average: one ruined tile
among forty-nine drags it less than one among twenty-five, and it takes five
of them nearby before the room alone pulls the average under zero. The Bim's
own filth is the other half of the sum, and that is what makes a single
accident bite immediately whatever the radius is.

So one accident is worth minutes rather than hours: a Bim standing in it is at
nothing within a quarter of an hour, and walking away does not clear it while it
is still covered. What that leads to, an hour a stage:

| stage | what it does |
| --- | --- |
| mildly uncomfortable | walks 10% slower, picking its way around it |
| uncomfortable | moves away from the mess whenever it is otherwise idle |
| extremely uncomfortable | **is sick every half hour**, which takes **50% off the food bar** — 92% becomes 42%, and at or below half full it goes to nothing — and puts more on the deck |

None of it interrupts a chain: these are things that happen *to* the Bim, so one
that wets itself on the way to the galley carries on to the galley. Moving away
from the mess is a walk rather than an errand, and anything the Bim is actually
doing — or has waiting on the queue — outranks it.

**This is a spiral, by design.** A Bim at the worst stage brings up half a meal
every half hour, which is faster than it can cook, so with nothing aboard able
to clean the deck the only way out is the player moving it somewhere clean
before it gets there. That is what the cleaning job this is built for will be
for.

Everything it starts for itself is an ordinary chain, so it can be interrupted
and queued like any other. Uncheck **Let the Bim decide** in the header to drive
it entirely by hand; the levels carry on moving, they just stop giving orders.

## The heads

Called the bathroom, the toilet and the basin everywhere the player can see —
the agenda says *Using the toilet*. "The heads" is the code's own word for the
compartment and stays in the source and in this file; nothing on screen uses it.

The compartment in the bottom-right corner is a room in its own right, walled
off with its own bulkheads and shut by a sliding door. The toilet's menu has one
item on it, and that one item is a whole errand: the Bim walks to the door,
opens it, steps through, shuts it behind itself **and locks it**, crosses to the
pan, sits, waits, gets up, flushes, moves to the basin, washes its hands under a
running tap, comes back to the door, unlocks and opens it, steps out onto the
deck and shuts it again. About half a minute, all of it animated.

The door is the interesting part, because it is the one thing aboard that
changes the shape of the room. Locking it shuts it as well — a locked door
standing open is not locked — and while it is locked the toilet's **Use** item is
greyed out with the reason, rather than the Bim walking into a sealed door. The
Bim locking up behind itself is why the lock is worth having: the state the
player left it in is restored on the way out.

The door has a panel on each side of the bulkhead, and asking for it sends the
Bim to whichever one it is nearest — including from the inside, so a Bim that
has shut itself in can always let itself out again.

These two panels are the only switches aboard that carry what was asked for
rather than reading the state they find. Every other switch is a toggle worked
at the moment the Bim's hand arrives, which is the honest thing for a hob or a
fridge. The door is different because *asking* changes it: the ask drops
whatever the Bim was doing, and dropping a trip to the heads takes off the lock
the Bim set behind itself. A toggle then found the door already unlocked and
locked it straight back — press **Unlock** on a Bim sitting on the pan and it
got up, walked to the panel, and locked itself back in.

It also shuts itself. Five seconds after the Bim has walked through, the panels
slide to — the chains that shut the door by hand simply get there first — so the
door is never left standing open by an errand that had no reason to close it.
The count waits while the Bim is still in the opening, and waits again while the
Bim is walking a route that goes through the doorway: a route is planned once
and never replanned, so a door that shut across one would leave the Bim walking
into the panels for good.

### Starting from inside

The chain is written for a Bim out on the deck, and a Bim already in there would
be sent round to a handle on the wrong side of the bulkhead. So from inside it
starts at the pan instead, and a locked door is no obstacle: a lock only stops a
Bim on the wrong side of one.

Skipping the four steps that let it in also skips the four that let it out —
they exist to undo each other, and a Bim that never opened the door has no
business unlocking it on the way past. So a Bim shut in stays shut in, finishes
at the basin, and the lock it never set stays set.

## Bed time

The bunk bed is against the left wall. Its menu offers a nap of thirty minutes
or a sleep of six hours; both run the same chain — walk over, up the ladder,
under the covers, out cold, a stretch, and back down — and differ only in how
long the Bim stays put. The menu says what time it will be up, and the status
line counts the rest down while it sleeps.

Drawing a bunk bed from directly above is the interesting part, because the top
bunk hides the bottom one almost entirely. So the lower bed is drawn first, set
down and to the right by `BUNK_DROP`, and what you see of it is the band of its
own mattress and bedding along two sides, with the upper bunk's shadow falling
across it. The Bim sleeps on the top bunk — the one you can see — and the
bedding, the safety rails and the corner posts are drawn *after* the character,
which is what actually puts it under the covers rather than on top of them. The
footprint the pathfinder is given covers both bunks, so the half of the lower
one that sticks out is solid too.

## Time

`src/clock.rs` keeps one clock for the whole game: minutes since midnight, and
which day it is. One real second is one game minute at 1x, so a day takes
twenty-four minutes of real time and a six-hour sleep takes six — and because
the clock runs off the same `dt` as everything else, the speed slider carries it
along. At 24x a whole day goes by in one minute.

Nothing about time crosses the wasm boundary as a string: the host reads the
minute count and formats it. The light level comes from the same clock, eased in
at dawn and out at dusk, and is laid over the finished frame as one tinted
rectangle — so nothing in `room.rs` has to know what time it is.

## The look of it

Everything is drawn from the two primitives in `src/draw.rs`, so "futuristic"
here is a matter of palette and of what gets a light on it rather than of any
new drawing machinery. The deck is dark blue-grey, the fittings are composite
panel in three shades, and one cyan running light is picked up by every powered
surface: the counter fascia, the induction coils etched into the hob, the rim of
the table, the underside of the top bunk, the jambs of the bathroom door. Warm
colours are held back for heat and for trouble, which is why a live hob and a
locked door are the only two warm things in the room and both read instantly.

## How it fits together

The simulation is entirely Rust. Each frame the host calls `bims_update`, which
advances the world and rebuilds a flat buffer of shapes in wasm linear memory;
`web/bims.js` reads that buffer and replays it onto a canvas. The only things
crossing the boundary are a handful of numbers and one pointer, which means no
binding generator and no JavaScript in the build — `cargo build` is the whole
pipeline, and the module has zero imports.

| File | What lives there |
| --- | --- |
| `src/lib.rs` | The wasm exports the host calls |
| `src/game.rs` | Ties the room, the Bim and the running task together; input |
| `src/room.rs` | The room: layout, fixture state, and how it is drawn |
| `src/bath.rs` | The heads: its bulkheads, its door, and its fittings |
| `src/dish.rs` | The dishwasher: what is in it, and the cycle it runs |
| `src/needs.rs` | What the Bim wants, and the rates that shape its day |
| `src/health.rs` | Going hungry, the three stages of it, and health |
| `src/filth.rs` | The state of the deck, and what a mess does to the Bim |
| `src/hydro.rs` | The hydroponic bay: five trays, and what goes in them |
| `src/manager.rs` | What the place is told to keep in stock |
| `src/schedule.rs` | The day's timetable, and when it is worth obeying |
| `src/task.rs` | The scripted chains — a meal, a sleep, a trip to the heads |
| `src/clock.rs` | The time of day, and how much light there is |
| `src/character.rs` | The Bim — how it decides where to go, and how it is drawn |
| `src/draw.rs` | The shape buffer and the local frame used for sprites |
| `src/math.rs`, `src/rng.rs` | Vectors, rectangles, angles, and a PCG32 generator |
| `web/bims.js` | Canvas renderer, input, and the fixture menus |

### Getting about

Every walk — a right-click order, and every leg of a scripted job — is planned
on a grid in `src/nav.rs`. Obstacles are inflated by the body radius before the
search, so a route that exists on the grid is one the Bim can physically walk
without clipping a corner. A* returns a staircase of cells, which is then pulled
straight by dropping every waypoint that can be skipped with a clear line of
sight; what is left is the two or three straight legs a person would actually
walk. A destination that is not standable — inside the table, behind the counter
— snaps to the nearest spot that is, so an order there means "the floor beside
it" rather than nothing at all.

The steering and the collision push-out are still there, but only for the
wander and as a backstop. While following a path the Bim trusts the plan.

### Ordering the Bim through a door

A shut door is a wall to the pathfinder, so a route is worked out twice: once
with the door as it stands, and — only if that comes back with nothing — once
with it open. A destination that only the second finds is one the Bim can reach
by letting itself through, so it goes and works the door panel and then carries
straight on. The order waits on that errand rather than joining the queue
behind it: the player asked for it now, not after whatever the door displaced.

There is only ever one of those errands. Clicking again while the Bim is on its
way to the panel replaces the destination waiting on it and nothing else —
before, each click put the door errand on the queue and started another exactly
like it, and a few impatient clicks left the agenda with ten *Bathroom door*
jobs all opening the same door. An order the Bim can reach without the door
cancels the waiting one outright, and the errand that was opening the door goes
with it: the two exist only to serve each other.

Three things can come back instead:

- **Blocked by a locked door.** The second route exists but the door is locked,
  so nothing is attempted at all — the Bim does not walk over and fail, and it
  does not drop what it was doing either. The one exception is a Bim part-way
  through its own trip to the heads, which locked that door behind itself:
  letting go of that errand unlocks it, so calling the Bim out works.
- **Nowhere to go.** No route with every door in the place wide open. There is
  nowhere in the current layout that this can happen, but the case is handled
  rather than assumed away.
- **Ignored**, if nothing is selected or the Bim is dead.

Both refusals are said out loud, because an order that quietly does nothing
reads as a broken click: the status line says which it was, in the warm colour
the room keeps for things worth noticing, and a cross is drawn where the order
landed.

### A chain never starts somewhere it cannot walk to

An empty route reports itself *arrived* the instant it is handed over — the
alternative, waiting on a walk that will never finish, hangs the chain for good.
That is fine for one step and quietly disastrous for a whole chain: every
remaining step finds itself already arrived and runs in the same frame, until
one of them sets a position outright and flings the Bim across the deck. Shut a
Bim in the heads, start it on a meal, and it appeared at the dining table
without having walked a step of the way.

Two things stop it now. A walk with nowhere to go gives the chain up rather than
taking it for an arrival, putting the world back in a state the Bim can be left
in. And no chain is begun at all unless the Bim can reach the place it starts
at, so a Bim behind a shut door does not set off for the galley: the need stays
unmet until the way opens. Queued chains are held back the same way rather than
dropped — they wait on the queue for the door.

Waiting is only right when the Bim can do nothing about it. A shut door is not
that: the panel is on the inside too, so a Bim that finds the galley out of
reach behind one goes and opens it, and the need comes round again with the way
clear. Only a *locked* door is a real wait. Without that, unlocking the door on
a Bim shut in the heads left it exactly where it was — unlocking leaves a door
shut — and it stood there at nothing per cent food until it starved.

Reachability is worked out afresh every frame rather than cached, so opening or
unlocking a door needs no nudge: the moment the way is clear the Bim takes up
whatever it could not reach a moment ago, queued work included. Queued work it
cannot reach does not count as having something on, either, or it would hold up
every need there is while it waited — and a Bim standing over an unreachable
meal would starve beside a queue it was never going to get to.

### Two grids, because of one door

A grid built once is only right while the room never changes shape, and the
bathroom door changes it. Rebuilding the grid when the door moves does not work:
a task opens the door and asks for a route through it *in the same step*, so any
rebuild in the frame loop is a frame late and the route comes back empty — the
Bim would stand at a door it had just opened and give up.

So `nav.rs` builds two grids at startup, one with the door in the way and one
without, and the choice between them is made at the moment a path is asked for.
That also gives the closed door its proper behaviour for free: order the Bim
through one and the route is genuinely empty, so it stays where it is instead of
walking through it. Shut the door on a Bim that is inside and it really is shut
in until someone opens it.

Physics is allowed to be less careful. The push-out that keeps the body out of
the furniture takes the door a frame late, which nobody can see.

### Simulation speed

The speed control multiplies how many fixed 1/60 steps run per frame, never the
size of a step. At 24x a single stretched step would move the Bim four times
its own body in one frame and it would pass straight through the counter;
twenty-four normal steps give exactly the result 1x would, just sooner.
`MAX_STEPS_PER_FRAME` caps the catch-up so a backgrounded tab cannot come back
and lock the page up — it has to stay above the top speed, or the top of the
slider would quietly be slower than it says.

### The wander

A plain per-frame random direction looks like vibration, not walking. Instead
the Bim commits to a leg of a journey at a time: a direction and a speed held
for a second or three, then either a pause or a fresh leg. Course changes are
usually gentle and occasionally a complete change of mind, and the body eases
towards each new heading rather than snapping to it.

Near the walls and the furniture it steers rather than bounces, and the steering
bends the Bim's *intent* as well as its current heading — otherwise it turns
back into the wall the moment it comes clear, and spends its life hugging the
edge. A push-out pass after each move is the backstop that keeps it out of the
table when steering is not enough.

### The room is a fixed size

The room is 860×580 world units whatever the window is; `Game::resize` works
out a scale and offset to centre it in the canvas, and the host applies that
transform when drawing and inverts it for pointer positions. Furniture at
honest proportions is worth more than filling every pixel, and it means the
layout constants in `room.rs` can be trusted.

One consequence worth knowing when editing `room.rs`: the Bim stands
`STAND_OFF` from the counter front, and `STAND_OFF` has to clear `BODY_MARGIN`
or the collision push-out fights the station it is walking to. Everything on
the worktop also has to sit within arm's reach of that line, or the Bim ends up
chopping thin air.

### The draw buffer

Twelve floats per shape: `kind, x, y, w, h, rot, radius, line, r, g, b, a`.
`kind` is 0 for a rectangle and 1 for an ellipse; everything on screen is built
from those two. Rotation is about the shape's own centre, and a non-zero `line`
strokes the outline instead of filling it. `bims_stride()` reports the stride so
the host never has to hardcode it.

If you change the layout, note that the renderer reads the stride at runtime but
still assumes the field *order* in `src/draw.rs`.

One trap worth knowing: `f32::clamp` panics when its bounds are crossed, and
that panic path drags Rust's formatting machinery into the wasm — it cost 19 KB
of the binary before being swapped for the branchless `clamp` in `src/math.rs`.
