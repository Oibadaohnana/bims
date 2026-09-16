# Bims

A 2D top-down game written in Rust, running in the browser via WebAssembly.

Two characters — Bims — pottering about a compartment on a ship. Either will
cook a meal from start to finish and clear up after it, take itself to bed, or
go and use the heads, entirely on its own account. A clock runs the whole time,
and the deck goes dark at night.

**You steer one of them.** James takes orders; Kate does not. See
[The crew](#the-crew).

Beside it there is a ship to design and a system to fly it round. The lobby
starts a [design phase](#the-ship-designer); accepting the design starts
[the game](#the-game) — one star system, the ship docked at a station in it,
and one clock everything runs on. The crew are not aboard that one yet.

## Running it

```sh
nix run .
```

That builds the game, serves it, and opens it in a browser tab. `Ctrl+C` stops
the server. Pass a port to pin one (`nix run . -- 3000`), or `--no-open` to keep
it out of your browser.

There are three things to run, and more will follow:

```sh
nix run .#game        # the room: the simulation, on a canvas — port 8080
nix run .#builder     # the start menu, game setup and the lobby — port 8081
nix run .#ship        # the ship design phase, and the game it starts — port 8082
```

`nix run .` is `nix run .#game`. All three serve the same directory and differ
only in which page they open, so they have separate default ports and can be up
at the same time.

While editing, `./run game`, `./run builder` and `./run ship` do the same
against the live `web/` directory rather than the frozen copy in the Nix store
— see [The builder](#the-builder), [The ship designer](#the-ship-designer) and
[The game](#the-game).

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

`./build.sh` alone compiles the wasm modules into `web/` — there are two of
them now, `bims.wasm` for the room and `ship.wasm` for the designer. Both
scripts drop into
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

## The builder

`nix run .#builder` opens what comes *before* the room: a start menu, a game
setup screen, and a lobby. It is `web/builder.html` and `web/builder.js`, a
page of its own — `web/bims.js` is the room and knows nothing about menus, and
the builder touches no wasm at all yet.

**Play** goes straight to game setup. **Create lobby** opens a room other
people will one day be able to walk into, and a code field beside the two of
them is the other half of that: type a code, press Join, and it refuses out
loud. That refusal is the honest state of things — there is no transport yet.

The lobby is the settings screen with company. Down the left, four slots: you
in the first one as host, the rest open and waiting. Across the top, the room
code, set like something you read out to somebody. On the right, the same
tabbed tool the setup screen uses:

- **Game setup** — the money each Bim brings at €50 000, €100 000 or
  €200 000, and the ship you start with at 30 × 30, 40 × 40 or 60 × 60 tiles.
  €100 000 and 40 × 40 unless you say otherwise. Nobody starts with stores:
  everybody's money goes into **one pool** and the designer spends that.
- **World** — where the ship is and what it can reach. Nothing behind it yet;
  the tab says so rather than showing an empty box.

One settings object sits behind both copies of the tool, so what you pick on
the setup screen is what the lobby shows and the other way about.

**Start** hands the game to the ship designer, which is a page of its own —
see below. What crosses is four numbers in a query string and nothing else:
the money each Bim brings, the build area in tiles, how many players there
are, and which slot you are. `"large"` and `"full"` exist for the buttons;
what a game is *started with* is what will cross into wasm, and no strings do
— which goes for the euro sign as well, so what is written down is a bare
count of euros.
`scratchpad/builder-check.mjs` asserts that, query parameter by query
parameter.

### The multiplayer seam

`net` in `web/builder.js` is the whole of it: `create`, `join`, `push`,
`leave`, and events the screens listen to. It is a local stand-in with the
shape a transport will have, and **nothing in the screens reaches past it** —
the slot list is drawn from what `net` says the players are, not from what the
lobby knows about itself. Giving it a socket is meant to be a change to that
object and to nothing else.

Two things it already does properly, because they are easy to get wrong later:
the settings tool knows how to be read-only, for a guest in somebody else's
lobby, and the settings themselves are plain numbers — a sum of money and a
tile count. Nothing but numbers can cross into the simulation anyway.

## The ship designer

`nix run .#ship` opens what the lobby's **Start** goes to: the whole crew
laying out **one ship** together, on a tile grid, before anybody is aboard.
It is `web/ship.html` and `web/ship.js` with a wasm of its own, `ship.wasm`,
out of `crates/ship`.

**No Bims exist during this phase.** Placing a part and taking it off again
are both instant and free: nothing has been welded yet, and the money is only
being promised. That stops the moment everybody accepts — after that, every
change is a Bim's work.

A tile is 52 world units and holds at most one part per **layer**, of which
there are four:

- **Structure** is the frame the ship is built on. It needs nothing under it,
  and it is the first thing anybody lays: nothing else goes down without it.
- **Floor** is the deck plating you walk on. It needs structure.
- **Object** is the one thing standing in the tile — a wall, a bunk, an
  engine. Most need deck; the hull parts stand straight on the frame, which
  is what lets a ship be skinned before it is floored.
- **Utility** runs *through* a tile without filling it: power conduit, which a
  body walks over and a hob can stand on.

A wall is on the object layer like everything else, because a wall and a bunk
in one tile is equally nonsense. What each part needs under it is one column
of the part table, and removal reads the same column backwards: nothing comes
out from under something that is standing on it.

### Laying one out

The palette down the left is grouped the way a ship is thought about — hull,
systems, crew, galley, heads, storage, bay — rather than the way the enum is
numbered. The rows are built from what the wasm says exists, so a part added
and forgotten in the grouping turns up under **Anything else** instead of
quietly not existing.

- **Click** to place. **Drag a rectangle** for the things you fill an area
  with — structure, deck plating, conduit — **drag a line** for both kinds of
  wall, **right-drag a rectangle** to clear. A clearing drag comes off top
  down: what is standing in the tile and what runs through it, then the deck,
  then the frame. Any other order and every tile with something on it would be
  refused and the drag would look half broken.
- **R** turns the ghost a quarter clockwise. The footprint and the use spots
  turn with it, and the palette shows the turned size.
- **Middle-drag** or **WASD** pans; the **wheel** zooms. The view is clamped
  to the build area and a margin, and starts showing all of it.

A drag is applied as a run of single edits, and **a failing one is skipped and
counted rather than fatal**: a rectangle of deck over a half-floored room is
meant to fill the gaps and pass over the rest.

The ghost is green where the tool would go down and red where it would not,
and the readout by the pointer says the same thing in the same colour before
the click rather than after it. Resting on a placed part rings it and shows
its **use spots** — the tiles a Bim will stand in to use it. Those are only
ever shown for the part under the pointer; drawn permanently they would fill
the deck with markers.

### What it costs, and what it checks

Across the top is what is left of the crew's money, beside what there was to
start with. There is **one pool**: every Bim's purse goes into it, a lone
player gets a fixed bonus on top because a ship for one costs what a ship for
four does, and every part anybody places comes out of the same figure. Each
part has a price in euros. A removal hands back the **whole** price, and what
is left over at the end is money rather than cargo — it is not aboard, and it
does not count towards what the ship weighs.

What is left is always **worked out from the design**, never decremented as
parts go down: a refused edit, a removal and a replayed run of edits cannot
drift apart from what is actually on the ship. The sum itself —
`money_per_bim × players`, plus the solo bonus — is `crates/economy`, so the
browser and the native server that will one day be authoritative arrive at the
pool the same way, in whole euros, with overflow an error rather than a wrap.

### Buying what the ship will live on

Under **Station** on the right is what there is to buy: ore, metal, fuel,
components, vegetables and tofu, at a price a unit. Buttons move one, ten or a
hundred, and selling hands back the whole price — nothing has left the dock,
so there is nothing to lose on the deal.

Two things bound a purchase, and the station is neither of them. Supply is
unlimited and every station charges the same; what refuses an order is **the
pool** — goods come out of the same money the hull does, so a player who
spends everything on plating has nothing to load it with — or **the ship**.
Goods are stowed: food in a cold store, fuel in a tank, everything else on a
shelf. The readout under the rows is how full each class is, and a ship with
no tank cannot take fuel at all however much money there is.

What is bought is aboard from the moment it is bought. It is in the design
hash, so a purchase clears everybody's Accept the way a wall does; it is in
the ship's mass, so the acceleration on the handoff screen already accounts
for it; and a shelf with something on it cannot be taken off until it is sold.

### Everything is made of something, and it weighs what it is made of

Every part has a **recipe** — so many units of metal, so many of components,
and never ore, fuel or food, which are mined, burnt and eaten rather than
built with. A part's mass is that recipe added up and there is no other
number: a wall is two metal, and two metal is what a wall weighs.

That is what makes construction a **move** rather than a purchase. Take two
metal out of the hold, put a wall on the frame, and the ship weighs exactly
what it weighed a moment ago — the materials have changed where they are and
nothing else. Take the wall off again and all two units come back; there is no
wastage, no scrap and no scrapping penalty. A ship's mass changes only by
trading at a station, by burning fuel, by food being eaten or grown, and by
crew coming aboard or leaving.

None of that is visible yet, because in the design phase there is a station
outside and everything is bought with money. **Money only works docked** —
that is where euros and materials swap for each other — and the design phase
happens docked at the spawn station, which is the whole reason a part can go
down instantly. Out between stations there is nobody to buy from, and what
gets built comes out of the hold or does not get built. A part's price in
euros and its recipe are deliberately unrelated: they are two different
transactions that happen to end in the same wall.

The rule is written down and tested now, against every part in the table, in
`crates/shipdesign/src/materials.rs`. Nothing calls it — the construction step
will, with a Bim doing the work.

### What it checks

Down the right is what is wrong with it. An **error** blocks Accept; a
**warning** is the design saying what it will be like to live with. Resting on
a row rings the tiles it names — a highlight, not a tooltip: nothing is said,
and it goes the moment the pointer moves.

The errors are:

- the ship is in more than one piece;
- fewer bunks or chairs than there are players;
- no table, cold store, worktop, hob, dishwasher, toilet or basin;
- somewhere a Bim has to stand is off the ship, has no deck, or is blocked;
- parts nobody could walk between — over deck, through doors, which count as a
  way through.

The warnings are no engine, no engine on some axis, no hydroponic bay, no
broom locker, no helm, nothing to eat aboard — and **radiation**.

**Engines are never an error**: a ship that cannot fly is still a ship you can
live on, and refusing to let a player accept one would be the design phase
having an opinion about how to play.

### Radiation, which is a warning and is louder than the errors

Hull keeps it out. The **outside wall**, the **airlock**, the **sensor array**
and the **engine** block shield; a plain internal wall does not, and neither
does a door — so a ship skinned in ordinary walls is a ship whose crew are
being cooked.

It is worked out by flooding in from outside the build area, four ways only,
through everything that does not shield. Every tile the flood reaches and
finds a part in is **exposed**, and the deck is tinted over every one of them
— not when you rest on the row, but always, because a player who has not
looked at the checks panel is exactly the one about to accept a ship with a
hole in it. The row sits first in the list and is styled louder than any error.

It does not block Accept. That is deliberate: it is a decision about how to
play, and the design phase does not take those. It only makes sure nobody
takes it by accident. Two hull parts meeting at a corner seal that corner —
the flood is four-way, so a hull drawn as a staircase does not leak at every
step of it.

That required list is a **mirror of what the room's chains walk to today** — a
meal is a cold store, a worktop, a hob, a table with a chair and a dishwasher;
a night is a bunk; a trip to the heads is a toilet and then a basin. It is not
a design. If the chains change, the list changes with them, and
`crates/shipdesign/src/validate.rs` says so at the top.

### Accepting

Each player has an Accept, disabled while anything is an error. An Accept is
recorded **against a hash** of the design — the parts sorted by position and
kind, with the ids left out, so the same layout gives the same number whatever
order it was built in. Any successful edit by anybody changes that hash and
clears every Accept, so there is no way to be holding one for a ship that is
no longer on screen.

When everybody's Accept matches the current hash the phase ends: editing locks,
and **the game starts**. Solo, one Accept settles it.

### The two crates behind it

- **`crates/shipdesign`** is the rules and nothing else: the part table, one
  `apply` that is the only way a design ever changes, `validate`, and
  `design_hash`. It renders nothing, exports nothing to wasm, and compiles
  natively as well as for wasm32 — the native server that will one day be
  authoritative has to agree with the browser about what a legal ship is, and
  `design_hash` has to come out **identical on both**. That is why nothing in
  its data or its hash is a `usize` or a float. Its unit tests are plain
  `cargo test`; the wasm half of the hash check is `ship_self_check`, read by
  `scratchpad/ship-check.mjs`.
- **`crates/ship`** is the browser's half: the camera, the pointer, the ghost,
  the draw buffer. It decides nothing about what may be placed — it asks.
- **`crates/economy`** is the money: whole euros, the shared pool, what a
  station charges for a unit of anything, and which class of hold it goes in.
  Every sum in it is checked — overflow is an error, never a wrap.

Its multiplayer seam is the same idea as the builder's. `net` in
`web/ship.js` has a transport's shape, every Edit and every Accept goes
through it carrying the design hash it was made against, and the host end
applies messages in arrival order and reports a refusal back to whoever sent
it. No click handler touches the wasm's editing exports directly.

## The game

The design phase is the only phase before it. The moment the last Accept lands,
the ship is real: it is **docked at the spawn station** with whatever was left
of the pool in the crew's hands, and from then on there is one world, one clock
and one loop.

That is worth saying plainly because it is the decision everything else hangs
off. The star system, the ship in it, and everything aboard it all advance
together in `World::step`, which moves the world by a sixtieth of a game minute
and nothing else. Crew, construction and health are not written yet; when they
are, they go **inside that step**, at the places already marked for them. A
second clock would be two simulations that disagree, and the failure would read
as a ship in two places.

### Flying it

Any player can take the helm. Open the map with **M**, click a planet, a
station or a bare point in space, and the panel quotes the trip before anybody
commits to it: how long, how much fuel, and whether it ends docked or holding
alongside. Confirm sends the ship.

A trip is one straight line and four phases:

1. **Align** — turn to face the arrival point, thrusters flat out for half the
   turn and flat out the other way for the rest, so it starts and ends still.
2. **Burn** — the engines, all the way to the changeover.
3. **Brake** — whichever is quicker: flip end over end and burn on the same
   engines, or push on the backward engines without turning at all. A ship with
   no backward engine always flips; a ship with strong ones never does.
4. **Arrive** — docked if it was aimed at a station and has an airlock,
   holding beside it otherwise.

There is no speed limit and no coasting in the middle. It is flat out to the
changeover and braking from there, which is the same shape the world generator
laid every system out against.

**Abort** brings it to rest along the line it is already on — it never
reverses — and holds there. A Confirm while it is under way is a redirect,
which is the same thing followed by a fresh departure: it stops first, and the
moment it has stopped it sets off again on its own. Only the latest confirmed
target is kept, and the route line on the map is drawn in the colour of
whoever set it.

### Fuel, and what it is spoken for

Confirming a trip **reserves** the fuel it will take. Reserved fuel cannot be
sold and no second trip can be planned against it, so a crew cannot promise the
same hundred units to two places. It is burnt over the engine phases — an align
and a flip cost nothing, because thrusters burn nothing — and it comes out of
the hold when the plan ends. An abort is charged for what was actually burnt:
the part of the trip that happened, plus the stopping, and the rest of the
reservation is handed back.

### Seeing where you are

The ship can see `VISION_RANGE` with the crew's own eyes, which out here is
almost nothing, and a great deal further with a **sensor array**. Anything that
comes within range of the stretch the ship travelled — the stretch, not the
endpoints, because at 24x a step is a long way — is discovered, shared by the
whole crew and never forgotten. Undiscovered things are not drawn on the map at
all, and there is no way to plot a trip to one.

### Speed, and who decides

Pause, 1×, 3×, 10× and 24×. **Every player has a request and the slowest one
wins**; a pause by anybody is a pause. That is not a compromise, it is the
point: the player who needs it slow is the player something is going wrong for,
and nobody is ever carried past something they wanted to look at. The panel
shows what everybody asked for as well as what is actually happening, so being
held at 1× is never a mystery.

### Trading

**Money only works while docked**, because a station is where there is somebody
to buy from. Holding station beside one is not docked — that wants an airlock —
and out between them the pool buys nothing at all. Supply is unlimited and every
station charges the same; what bounds a purchase is the money and the hold.

### The two views

**Ship** is the live ship at tile scale, drawn turned to its heading, with the
starfield sliding the other way behind it. **System map** is the star, what the
crew have found, the ring the scanner reaches to, the route, and a marker
pointing where the ship is pointing.

The camera is **north-up in both, always**. It is the ship that turns on
screen. A camera that followed the heading would make a flip legible and every
other moment unreadable — you could not tell which way you were going, because
"which way" would always look the same.

### The two crates behind that

- **`crates/flight`** is what a design does when you push it — mass, centre of
  mass, inertia, acceleration, how fast it turns — and the trip that takes it
  somewhere. A plan is worked out **once** and then read at a time: nothing
  integrates, so a browser at 24×, a browser at 1× and a server catching up on
  an hour of somebody's disconnection all put the ship in the same place.
- **`crates/world`** is the star system, the ship in it, the clock, and the
  order things happen in. It renders nothing and exports nothing to wasm.

What the steps after this one are promised is written down at the top of
`crates/world/src/lib.rs`: Bims live in ship-design tile coordinates and the
ship's rotation does not reach them, every ship change goes through
`on_ship_changed`, construction spends what is aboard and asks
`can_modify_part` first, money is only for docks, and the design's exposure map
is the radiation input.

Which world you land in is a seed and a galaxy shape, and for now they come off
`ship.html`'s query string with a fixed default behind them. The lobby's World
tab will pick them instead; nothing else about them changes.

## The crew

Two Bims live aboard: **James** and **Kate**. Their names are written over
their heads on the deck, each has an agenda down the left, and the one you have
selected has its bars and its crew sheet down the right.

They are not two kinds of thing. Both run the same needs on the same clock,
both take themselves to bed and to the galley and to the heads for the same
reasons, and nothing in the simulation distinguishes them except an index. The
one asymmetry is the player: **every order goes to James**. Selection, the
right-click move order, recruiting, and every item on every fixture menu act on
him and only him. Kate takes no instruction from anybody and lives her whole
day on her own account — which is the point of her being there.

A Bim's name, coverall and hair are the host's business and the drawing's; the
simulation knows crew member 0 and crew member 1. No strings cross the wasm
boundary, so the names are written on the canvas by `web/bims.js` after the
shape buffer has been replayed, not carried across in it.

### Picking one, and the crew sheet

**Clicking a Bim selects it, and selecting is what puts its panels on the
right-hand side**: the four bars, health, and a character sheet under them.
One at a time, and nothing at all when nothing is picked — the right-hand side
answers *who am I looking at*, not *what is everybody up to*. Kate's bars are
hers until you click on her, and her panels carry her own colour so a glance
says whose sheet is open without reading the name.

**Selecting is looking at, not taking charge of.** Any of them can be picked;
only James takes orders. Click Kate, right-click the floor, and nothing
happens — which is the same answer as before, arrived at more visibly.

The sheet is tabbed the way the tray at the bottom left is, because it is the
same kind of thing: pages of detail you open when you want them rather than a
readout to be watched.

- **About** — name, age, and the date they were born. Everyone aboard was born
  between **2350** and **2380**, rolled at the start of the game and fixed
  thereafter; the game opens on the first of January **2400**, so the crew are
  somewhere between twenty and fifty. The ship keeps a 365-day calendar with
  twelve months of the usual lengths and **no leap years** — a leap day buys
  nothing here and costs a special case in every piece of date arithmetic,
  including working out whether this year's birthday has been had yet.
- **Memory** — the Bim's own account of its days.

### What a Bim remembers

**Only what went wrong.** Newest day at the top, in the order it happened
within a day, and a crew who spend the week eating, sleeping and getting on
with their work leave the page **blank** — it says *"Nothing has gone wrong."*
and that is the good outcome rather than a panel that failed to load.

It used to keep the day's work too — *"Woke up after 6 hours"*, *"Went to the
heads"*, *"Swept 5 patches of the deck"* — three or four times a day each,
which meant scrolling past a wall of chores to find the one line that mattered.
Now the chores are not written down at all.

What is left: an accident, being sick, dropping off standing up, each stage
further into hunger or sleeplessness as it arrives — once, as it happens, not
once a frame for as long as it lasts — the low moments of a Bim nobody has
spoken to, sitting down on the deck, hurting itself. Coming back out of one is
not an entry; the bars say so.

And **what it saw**. When something happens to one of the crew, any of the
others near enough to see it and awake to notice remembers that too: *"Saw Kate
have an accident."* A Bim asleep in its bunk on the far side of the compartment
witnessed nothing, and a diary that claims otherwise is one nobody can trust.

**A death is the exception to that.** It goes into every surviving diary
wherever they happened to be standing, because it is the one thing aboard
nobody could fail to notice — and it is the thing the whole "only what matters"
rule exists to make findable.

An entry with no words for it is **dropped from the page** rather than padded
with a placeholder. A line that says "Something happened." reads as the Bim
having had a mysterious experience when in truth the table is simply short an
entry; a missing row is at least honest, and it is what `smoke.mjs` counts.

No strings cross the wasm boundary, here as everywhere. An entry is a day, a
time, a code and one number; `MEMORY_LINES` in `web/bims.js` is where the
sentences live. A Bim that remembers being sick remembers `(day 4, 18:22,
WasSick, 0)` and "on the deck" is the host's wording of nothing at all. The
book is bounded — a few hundred entries, oldest falling off the front — so a
game left running does not grow without end. Memory is finite; so is this.

### What is one each, and what is shared

Each has **a berth of its own** and **a seat of its own**. There are two bunks —
one against the left wall where the only bunk aboard always stood, one in the
top-right corner, mirrored so its ladder faces the room — and two chairs, one
each side of the table, with a place laid in front of each. A Bim goes to its
own bed and sits in its own chair; neither is ever contested.

Everything else is shared, and two of the shared things are one pair of hands'
worth:

- **The galley.** One cold store, one board, one knife, one pot, one hob, one
  dishwasher. While one Bim is on a galley errand the other's simply does not
  start — the menu item says *"Kate is in the galley"* and is greyed out, and
  Kate's own hunger waits and tries again. Tending the hydroponic bay counts as
  a galley errand, because that chain ends by putting the harvest in the
  fridge.
- **The heads.** One pan, one basin, one door.

Nobody queues for either: the errand is not begun, `consider_errand` moves on
to whatever else that Bim could be doing, and it comes round again a moment
later. That is the same shape as every other "can't do that yet" in the game —
an empty cold store, a locked door — rather than a new mechanism.

The **deck** is shared too, which means a mess one of them makes is a mess the
other has to stand in. How far gone each is for standing in it is still its
own: the mess is the room's, the two hours spent beside it are the Bim's.

### They walk through each other

**Bodies do not collide.** Two Bims that meet pass straight through one
another, and both slow to **70% of their pace** for as long as they are within
a body's width. Close quarters are slow; that is the whole of the rule.

It is the one resolution that cannot leave anybody stuck, and being stuck is
the real hazard here. **A route is planned once and never replanned**, so a Bim
whose line goes through the other has no second plan to fall back on. Anything
that stops it getting where that line goes — pushing it aside, holding it up —
risks two of them standing nose to nose for ever with errands on both agendas.
Letting them overlap gives that failure nowhere to happen.

An earlier version did try to be clever about it: push them apart, pick one at
random to stand aside for three seconds, shove that one sideways out of the
other's lane. It worked, eventually, after two rounds of fixing what the fix
broke — but every part of it existed to stop bodies overlapping, and once
overlapping is allowed the whole apparatus has nothing left to do.

A Bim sitting at the table or asleep in its bunk slows nobody: it is tucked
into the furniture rather than standing in the gangway, and walking past the
foot of a bed should cost nothing.

## Controls

| | |
| --- | --- |
| Click the fridge | Menu: what is left, **Make a stew**, **Make a bowl**, the door |
| Click the stove | Menu: turn on/off — James walks over and flips it |
| Click either bunk | Menu: **Nap** (30 min) or **Sleep** (6 hours) — James goes to his own |
| Click the dishwasher | Menu: what is stowed, and **Run now** |
| Click the hydroponic bay | Menu: the trays, **Automate**, and what to plant |
| Click the broom locker | Menu: **Sweep up** — and they get round to it themselves |
| Click the toilet | Menu: **Use** — and a wash at the basin after |
| Click the bathroom door | Menu: open/close, lock/unlock |
| Right-click a fixture | The same menu, on the other button |
| `1` | Select James (control group 1) — the one you steer |
| `r` | Recruit James, or let him go — see below |
| Drag a box over one | Select it — either of them. Selecting shows its crew sheet |
| Click one | Select it — a click is just a box of no size. Only James takes orders |
| Click empty floor / `Esc` | Deselect — the right-hand panels go with it |
| Right-click the floor | Send the selection there — opens a door on the way if it must |
| Tray, bottom left | **Schedule** — paint the day and set the thresholds; **Management** — autonomy, food to keep, what is aboard |
| Action thresholds, under the strip | Rest and Food: how low each may get before the Bim acts, and a tick box to stop it acting at all |
| Point at anything | Top left says what it is, and what is lying on it |
| Point at a management row | The place it names is ringed on the deck |
| Let the Bim decide | Whether the crew start errands on their own when idle |
| Speed slider, top right | Run the simulation from 1x up to 24x |
| Rest on an underlined word or a ? | It explains itself, after a third of a second |

Nothing on the page explains itself in prose any more. The explanations are in
tooltips, and a tooltip is always *asked for* rather than stumbled into. Two
rules, and nothing else pops anything up:

- **An underlined word.** Where the thing already has a name — each need, Health,
  "Let the Bim decide", "Speed" — the name carries it, dotted-underlined and
  with the help cursor. Hovering the row, the bar or the panel does nothing.
- **A "?".** Where there is no word of its own to underline, a small question
  mark sits beside the controls: the schedule's is at the end of the brush row,
  past **Sleep** and **Everything**, and **Action threshold** has one of its
  own — it names a block of two rows rather than a single control, so there is
  no one word the explanation belongs on.

Either opens after the pointer has rested on it for 300ms. The delay is the
point: crossing a panel on the way somewhere else sets nothing off. The ring a
management row draws round its fixture is not one of these and needs no
affordance — nothing pops up, nothing is said, and it is gone the instant the
pointer moves on. The one number inside a tooltip that could go stale — the
rested-enough level in the schedule's — is filled in from wasm rather than
written into the markup.

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

Produce is held to the same rule. A plant lifted from the hydroponic bay is in
the Bim's hands until it has carried it up the room and put it in the cold
store — see [The harvest is carried](#the-harvest-is-carried).

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
  because the knife is already in hand. **A pot holds two helpings**: the Bim
  has a plate of it now and the rest of it later.
- **A bowl.** One block of tofu chopped into cubes, with the salad that came
  out of the fridge alongside it, tipped into a bowl and eaten cold. No pot, no
  heat, and about two thirds the time of a stew.

The loop and the fork in the chain are the same one mechanism: `Step::next` is
still a straight line, and `Task::next_step` overrides it in the three places
where the recipe matters — round again for the second vegetable, skip the
drawer on that second trip, and turn off towards the bowl instead of the pot.

### The pot keeps

A stew is cooked once and eaten twice. Tipping it in fills the pot with two
helpings; serving a plate takes one. When the Bim is hungry again and there is
still something in the pot it goes back to it — a plate out of the drawer, the
rest of the stew, and the same sit-down and clearing-up — which is half the
time of a fresh meal and costs the store nothing. After the second plate the
pot is empty and the next meal is cooked from scratch.

It is the same chain as a meal, started part-way along: at the drawer rather
than the fridge, and skipping the hob on the way past, since nothing was lit.
The hob menu offers it by hand as **Eat from the pot**, and a Bim deciding for
itself always prefers it — cooking a second pot on top of the first would throw
the first away.

The helping comes off the pot when the serving *finishes* rather than spoonful
by spoonful, so a serve that was interrupted and started again costs the pot
nothing. What the spoonfuls move is the picture.

### The cold store

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

## Sweeping up

The broom lives in a locker set into the port bulkhead, between the foot of the
first bunk and the hydroponic bay. It is the **only thing aboard that undoes a
mess**: everything else either makes one or gets out of its way.

A Bim with nothing else on fetches it and sweeps. That is the whole trigger —
sweeping sits at the very bottom of `consider_errand`, below every need, below
a scheduled night and below the bay, so it is what a Bim does with time it has
nothing better to spend. Anything arriving interrupts it exactly like any other
errand, and the half-swept deck goes on the queue to be picked up after. You can
also ask for it: the locker has a **Sweep up** item, greyed out when the deck is
already clean.

The chain is fetch, sweep, fetch again: **broom out → walk to the worst tile →
sweep it → walk to the next → …** up to five tiles, then the broom goes back.
Five rather than "until it is done" so that hunger and the heads get a look in
between armfuls; if the deck still wants it and nothing else has come up, the
Bim goes straight back for the broom.

Which tile is next is worst-first with distance counting against it, so the Bim
works outwards from where it is standing rather than crossing the compartment
for the single filthiest tile every time. A tile comes all the way clean in one
go — it is swept or it is not — and the **time** is in the chain rather than in
chipping away at the score.

Two things worth knowing about the corners:

- **It sweeps the tile it set out for, not the one under its boots.** They
  differ whenever the dirt is somewhere a body cannot quite stand, and a broom
  has the reach for that. Sweeping underfoot instead would leave those tiles
  filthy for ever *and* send the Bim back to the same one every time, because
  it would still be the worst on the deck.
- **A tile nobody can get to is never chosen.** Some of the deck is deck and
  still unreachable — the corner past the end of the counter, hemmed in by the
  bunk. A mess there would otherwise be picked as the worst tile for ever, with
  the Bim fetching the broom, failing the walk, giving up and starting again.

There is one broom, so one Bim sweeps at a time; the other's errand simply does
not start, the same way the galley and the heads work. And the locker door is
drawn from *whose hands the broom is in* rather than from a flag of its own, so
a chain given up mid-sweep cannot leave the cupboard claiming to hold a broom
that is somewhere else.

## The hydroponic bay

Six trays along the bottom-left wall, and the only thing aboard that puts food
*back* into the cold store. Right-click it for its two controls.

**Automate** is on out of the box and puts the bay on the manager's target
(below) — a bay that has to be switched on is a bay that is off whenever the
player has not noticed it, and at dawn the store already holds what the target
asks for, so it sits quietly until the first meal dips below the mark. While
the store is under it the bay has work, and the Bim walks over and does it a tray at a time:
lift anything ripe, then plant whatever is missing. Greens come up in **one
day**, soy in **a day and a half** and is pressed into tofu.

Six trays is one bay per Bim, near enough. At the two-to-one ratio that is four
trays of greens and two of soy, which comes out at about four vegetables and one
and a third blocks of tofu a day. A Bim eats two meals a day and a pot covers
both of them, so it gets through two or three vegetables and a block of tofu —
the bay keeps up, with a little to spare for the days it cooks twice.

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

### The harvest is carried

A plant lifted out of a tray is **in the Bim's hands**, not in the store. The
bay is at the bottom-left wall and the cold store is at the top of the room, so
the tend errand carries on past the tray: up the room with the vegetable,
fridge open, plant in, fridge shut. Only then does the count in the management
tab go up.

That is [Nothing is remote](#nothing-is-remote) applied to the one thing that
was still cheating. The trays used to empty and the store used to fill in the
same instant, with the Bim standing twenty feet away — which is the exact
pattern the rest of the game exists to avoid, and it showed: a bay running flat
out looked like it was posting produce through a wall.

There is a real consequence, not just a nicer animation. The walk is most of
the errand now, so a bay working hard costs the Bim a noticeable part of its
day, and a harvest interrupted half way is a Bim standing about holding a
carrot until it gets back to it. The crop rides along in the saved chain, so
being pulled off it loses nothing; the only case where produce is banked
without a hand on it is a chain given up for good because a door shut across
the walk, and putting it in the store then is the lesser of the two wrongs.

A **planting** has nothing to carry and ends at the tray, which is why a
planting's row on the agenda vanishes at about half a bar. That is deliberate:
weighting the chain by what the Bim turns out to be holding would make the bar
run *backwards* the moment a harvest came up, because nothing is in its hands
until the tray is already worked.

## The manager

**Management** in the tray holds one number so far: **how much food to keep**.
It is in *food units*, and a unit divides two to one — two thirds vegetables,
one third tofu — so asking for **99** sets the target to **66 vegetables and 33
blocks of tofu**. The field shows the split as you type it, read back out of
wasm rather than worked out twice.

That target is what the bay plants to, and it is the only demand there is for
now. The manager is where the rest will go as they arrive: one row per thing
the place is told to keep up.

Under it is **what is aboard**: a table of one row per thing, with what it is,
how many there are, and where they are — vegetables and tofu in the cold store,
and whatever is left in the pot on the hob. It is built from a list rather than
written into the markup, so a new thing to keep track of is a new line rather
than a new table.

**Resting on a row rings the place on the deck.** "Cold store" and "Pot on the
hob" are words, and the room is full of grey rectangles standing against the
same wall; pointing at the row draws a cyan ring round the actual fixture, so
the two do not have to be matched up by eye. The food target does the same for
the hydroponic bay, which is what works to it.

It is not a tooltip and does not go through the tooltip machinery: nothing pops
up, nothing is said, and a pointer crossing the panel on its way somewhere else
lights a fixture for a moment and leaves nothing behind. That is why it can
hang off a whole row rather than needing a word of its own to underline — every
row is about exactly one place. The ring is drawn *over* the night wash, since
a highlight that dims at three in the morning is no highlight, and in the
ship's own cyan rather than the green that means "selected" or the warm colours
that mean trouble.

The speed slider used to sit here too. It is in the header now, beside the
clock it is speeding up.

A note on the arithmetic, since the two readings of "food unit" differ. A unit
here is one *item* split two to one, so 99 units is 99 things — 33 days of
greens at two a stew, or about seven weeks for one Bim. Read instead as "99
meals' worth", at two vegetables and a block of tofu each, it would be 198 and
99. The first is what the field does, because that is what was asked for.

## Work priorities

The **Work** tab is the third in the tray, and it is the answer to "what should
they do first". One row per job, a number from **1 to 5** in a box beside each,
and **1 is done first**. Click a box and it steps one less important, from 5
back round to 1. Everything starts at **3**, all equal, so out of the box this
changes nothing at all — the ship behaves exactly as it did before the panel
existed, which is deliberate: a default that is already an opinion is a default
you have to undo before you can use anything.

| | |
| --- | --- |
| **Cleaning** | sweeping the deck |
| **Planting** | sowing an empty tray in the bay |
| **Plant cutting** | lifting a ripe one out of it |
| **Hauling** | carrying what was lifted to the cold store |
| **Cook** | making a meal |

The colour of the box says what the number means without anybody having to
remember which end is which: warm at the top of the list, cold at the bottom,
five steps across the palette. At the top of the panel are three ways to
reorder the rows — **Priority 1→5**, **Priority 5→1** and **Name A–Z**.

Sorting is something you *press*, not a rule that stays on. If the list
re-sorted itself live, the row you just clicked would jump out from under the
pointer — and at the wrap from 5 back to 1 it would jump the whole length of
the list. So the button reorders the rows there and then, and the mark showing
which order was applied is cleared the moment a box is clicked, because the
list may no longer be in it.

Resting on a row rings the place the job happens, the same as a management row:
the locker for cleaning, the bay for both bay jobs, the cold store for hauling,
the hob for cooking. Nothing pops up and nothing is said — see
[A highlight is not a tooltip](#what-the-pointer-is-over).

### What it actually changes

Only what a Bim takes on **of its own accord**. A Bim with nothing pressing
looks at whatever work is going — a tray asking, a deck wanting the broom, its
own hunger — and does the one nearest the top of the list. Whether it then gets
on with it is a separate question: there is one galley and one broom, so the
other Bim may have the thing it needs, in which case it moves down the list
rather than waiting in line.

Two things the list deliberately does **not** touch:

- **Sleep and the heads are not work.** There is no row for either and no
  number to set. A timetable and an action threshold are how those are steered.
- **Nobody starves for it.** Cooking at the bottom with a deck that never comes
  clean is exactly the arrangement a player will try, and a Bim is allowed to
  put the meal off — but once going without has actually begun to tell on it,
  the meal jumps the queue whatever the cook row says. The list is a statement
  about what to do next, not about whether to eat at all.

**Hauling has no errand of its own yet**, because nothing aboard is fetched or
moved except a harvest, and that is carried in the same chain that lifted it.
So it is the back half of a cutting, and a cutting waits on whichever of
**Plant cutting** and **Hauling** is set later. Put hauling at the bottom and
the bay stops being emptied, which is the truthful answer: there is nobody to
carry it. When something else worth hauling arrives, that is the row it goes
under.

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

Only sleep is timetabled so far, and **the timetable is what sends the Bim to
bed on an ordinary night**. Running low on rest is not a reason in itself for a
scheduled night: the level decides whether that block is worth taking, not
whether to have one. Underneath the timetable there is a floor — the **Rest
threshold**, below — which catches the Bim when the timetable has not. Wipe the
strip entirely and the threshold is all that is left: the Bim goes to bed when
rest runs past it, whatever the hour. Untick that too and it never sleeps at
all.

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
  stands aside while food is past its threshold and there is something to cook:
  turning in starving costs the Bim six hours of losing health and it wakes no
  better off, where the meal costs it three quarters of an hour and puts the
  need away entirely. The sleep is held, not dropped, and goes ahead the moment
  the plate is cleared. It only stands aside while a meal is actually to be
  had, and while the Bim is free to go and have it — with the cold store empty,
  the galley behind a locked door, or autonomy switched off, bedtime goes ahead
  as it is. Waiting on a meal nobody is going to cook would be a Bim that never
  sleeps at all.
- **Under 80% on the restroom need, and it goes first.** You go before bed.
  Unlike the meal, this one does not wait for the 10% threshold: the restroom need
  is the only one that keeps draining through the night, and a night is six
  hours — turn in at four fifths and the Bim wakes at nothing. So anything under
  four fifths is worth emptying out first, and the sleep waits the twenty
  minutes it takes. The same guards as the meal: only while the Bim is free to
  go, and only while the pan is reachable — a locked door is not a reason to
  keep a tired Bim up. This is half a rule on its own: holding the sleep back
  does nothing unless something *starts* the trip, and the need is nowhere near
  the trigger the errand loop watches, so `consider_errand` starts it by name.

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

## Action thresholds

Under the hour strip, in the same tab, sit the two levels that say **how low a
need may get before the Bim does something about it**: one for Rest, one for
Food. Each is a tick box and a slider, and each starts ticked at 10%.

(In the code these are `needs::Trigger` — a level and a switch. "Threshold" is
the player's word for the setting; "trigger" is what the mechanism does.)

They are the other half of the timetable's question. The strip says when the Bim
*may* sleep; the Rest threshold says when it should go anyway. A Bim whose night
has been wiped off the strip, or who has been kept out of bed by a long errand,
falls past the threshold and turns in on its own account — which is why wiping
the timetable no longer means a Bim that never sleeps.

A threshold fires the same errand a need has always fired, so everything that
already governed those errands still governs these:

- **A meal comes first.** The Rest threshold stands aside for hunger exactly as a
  scheduled night does, and for the same reason.
- **So does a trip to the heads.** You go before bed, and the Rest threshold waits
  the twenty minutes it takes. The two rules have to be paired: the threshold
  refusing to start the sleep is what makes something else start the trip.
- **Autonomy and orders still outrank it.** With *Let the Bim decide* off, or
  the Bim recruited, no threshold starts anything.

**Unticking one is not the same as setting it to nothing.** The need carries on
draining and everything going without does to the Bim still bites — a Bim with
the Rest threshold off still gets sleepy, sleep-deprived, and finally drops off on
its feet. All that stops is the Bim going and doing something about it on its
own account. Three days of that leaves it hovering just under the level it would
have acted on, clawing back a few minutes at a time from nodding off where it
stands.

Restroom and Cleanliness have no threshold on the page. The first is not something
a player should be able to talk the Bim out of; the second has no errand behind
it to start.

One thing the numbers assume, and worth knowing before you move one: the drain
rates are derived from a tenth. Each need is written as the drop from full to
10%, divided by how long that drop is supposed to take, so two meals and three
visits a day come out of the arithmetic rather than being tuned by eye. Tell the
Bim to eat at half full and it eats more often than twice a day — which is the
point of being able to say so, but it is no longer the day the rates were built
for.

## What the pointer is over

Top left, above the agenda, one line says **what is under the pointer**. Deck
plating, a bulkhead, the worktop, the chopping board, the cold store, the hob,
the dishwasher, the table, the chair, the bunk, the hydroponic bay, the toilet,
the basin, the bathroom door — and *outside the hull* if the pointer is off the
ship altogether, which it can be, because the room is a fixed size letterboxed
into whatever window you have.

Where there is something worth saying about the state of it, it says that too:
the hob **lit**, the cold store **open**, the door **locked** or **shut**, the
dishwasher **running**, the bay with **two ready to lift**.

And on deck, a second line says what is lying there: **Wet**, **Soiled** or
**Vomit**, with how far down that tile has been taken. The simulation itself has
never distinguished one stain from another — a tile is one number, and the
average around the Bim is all anything reads — so the kind is remembered
alongside the score purely for this readout. It keeps the worst of what has
happened rather than the latest: being sick on a tile already wet reads as sick.

The readout is a different question from a click, and answers accordingly. A
click asks *what would this act on*, which is why only the few things with a
menu behind them are hit-testable and why each of those is given a few pixels of
slack. The readout asks *what is this*, so everything aboard has a name and
nothing is expanded: a pixel beside the pan is deck, not toilet.

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

Four levels run a Bim's day, shown down the right-hand side of the deck for
whichever of the crew is selected: **Rest**, **Food**, **Restroom** and
**Cleanliness**. Kate's are worth going and looking at precisely because you
cannot order her about — her bars are the only warning you get. Each sits
at 1 when the Bim is comfortable and falls as the day goes on. One dropping past
its **threshold** — a tenth, until you move it — is what sends the Bim to bed,
to the fridge or to the heads; doing the thing fills it back up, and a meal fills
hunger completely however empty it was. Rest and Food have theirs on the page,
under the timetable; see [Action thresholds](#action-thresholds).

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

| need | per day | slot | errand costs | drain per minute |
| --- | --- | --- | --- | --- |
| Rest | 1 | 1140 | 6 | 0.9 / 1134 |
| Food | 2 | 540 | 48 | 0.9 / 492 |
| Restroom | 3 | 480 | 14 | 0.9 / 466 |

The first two drain per *waking* minute and the last one per minute of the day,
because the restroom need is the one that keeps going while the Bim sleeps —
hence a slot of 1440/3 rather than 1080/3. See below.

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

Hunger and rest stop while the Bim is asleep — six hours in bed cost it nothing
to put right afterwards. **The restroom need is the exception: it runs all
night.** A body does not stop making water because its owner is unconscious.

Which is why its slot is the whole 1440 rather than the waking 1080. Derive it
off the waking day and the night quietly adds a fourth visit; derive it off the
calendar day and three a day stays three, with one of the three falling not long
after the Bim gets up. The other half of keeping that honest is the rule that
sends the Bim to the heads before bed, so the night starts from full rather than
from wherever the evening left it.

What *is* still frozen is the accidents that hang off the need — the one-in-ten
an hour would otherwise roll six times a night and wet the bed about half the
time. The level falls; nothing happens because of it until the Bim is up.

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
starts at **10** and goes down as things happen on it: an accident takes one
straight to **−100**, and so does being sick on one; wetting one costs 35.
**A broom puts it back** — see [Sweeping up](#sweeping-up) — and it is the only
thing that does. Being sick is the one a Bim does over and over, so a Bim at
the worst stage leaves a trail of ruined tiles behind it as it moves away from
each in turn, and somebody has to go round after it. The Bim carries its own
share around separately, 0 to 1, because a Bim that soils itself takes the mess
with it when it walks away; no broom reaches that. A wash at the basin gets half
of it off, and nothing else does anything.

The score is the whole of what the simulation reads — the average around the
Bim, how fast that grinds it down, which way it walks to get clear. Alongside it
each tile also remembers **what** was spilt on it, which nothing in the
simulation looks at: it is there so that pointing at a tile gets an answer a
player can use. "−65" is no answer to *what is that*.

### Dirty work, and dirt that walks

Not every mess is an accident. **The dirty jobs mark the deck around them**: a
knife going through a vegetable, a pot tipped and served, a pair of hands in a
hydroponic tray, a crop going into the cold store. Each of those steps has
about a **one in three** chance of flicking something onto one of the nine
tiles around the Bim, worth 14 off that tile — a stain, not a ruined tile, but
well past the threshold where the broom is worth getting out. So the galley and
the bay go grubby on their own, and the crew have standing work even on a week
where nobody has a bad day.

It goes on a tile *near* the Bim rather than always underfoot, so a week of
cooking spreads a patch across the galley instead of wearing one hole in the
deck in front of the stove. Nothing is ever flicked somewhere a body cannot
walk to, because that is a stain the broom would never reach and the deck would
keep for good — the same question `worst_tile` asks, asked here too.

And **dirt travels on boots**. Each time a Bim steps from one tile to the next,
there is a **25% chance** it carries **25% of what was on the tile behind** onto
the tile ahead. Both numbers are about the tile it is leaving, not a fixed
amount: a boot out of a ruined tile leaves a real smear, a boot out of a faint
one leaves almost nothing. The mess it left behind is now a source in its own
turn, so a Bim pacing the same route lays a trail that thins as it lengthens — a
quarter, then a sixteenth — and falls under the sweeping threshold after three
or four steps. That is what stops one accident eventually reaching every tile
aboard.

The dirt **moves rather than multiplies**: the tile behind loses exactly what
the tile ahead gains. Copying it would let a Bim walking back and forth across
the galley make filth out of nothing, and the deck would lose to a pair of feet.
Nothing is drawn from the dice at all unless the step crossed a tile boundary
*and* the tile behind had something on it — a Bim crossing clean deck costs
nothing, which matters because every roll aboard comes off one stream.

The smear carries the *name* of what it came off, too: a boot out of a tile
somebody was sick on leaves a fainter patch of the same thing, not a new kind of
mess. Only what a dirty job leaves has its own name — **grime** — and it is the
least bad of them, so grime tracked across a ruined tile never talks the readout
back down.

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

## Socializing

The fifth bar, and the only one that wants **another Bim** rather than a
fixture. Past its trigger the Bim goes and finds the other one — which happens
**about four times in a waking day**, roughly thirty conversations a week.

Two numbers make it that talkative, and both are unlike every other need here:

- **The trigger is half a bar**, not the tenth everything else uses. Company is
  filled by the *other* Bim being free at the same moment, so waiting until the
  bar is nearly empty would mean waiting until the one thing that fixes it is
  least likely to be available. Asking early is how two people sharing one
  compartment actually behave.
- **A conversation only puts three tenths of the bar back.** It is a word on
  the deck, not a meal. So the next one is never far off, and the bar hovers
  around its trigger instead of swinging the whole way down and back — a Bim
  aboard a working ship should never see this one anywhere near empty.

The drain is written against those two rather than against `URGENT`, which is
the one place in `needs.rs` that departs from the house rule: the span it
travels between one chat and the next is what a chat restores, over the time
that is meant to take.

### Talking

Both crew are handed the errand in the same frame, each walking to **its own
spot** either side of a meeting point worked out from where the two of them are
standing. That is not a flourish. A route here is planned once and never
replanned, so a Bim sent to *where the other one is* would be walking at a
target that is itself walking: it would converge on empty deck and stand there
for ever with `arrived()` never coming true. Two fixed points is the only shape
of this that terminates.

They stand **side by side** rather than either side of the line they happened to
approach along. A pair who met walking north–south would end up one above the
other, and the host paints each name a body's height above its head — so the
lower one's label lands squarely on the upper one. Left and right, and nothing
is written over anything.

A **speech bubble** goes up over whichever of them has the floor. They take
turns, and whose turn it is comes off the ship's clock rather than off either
one's own errand: they arrive a moment apart, and two bubbles over two Bims
standing together would sit on top of each other. What they talk about is
picked out of the speaker's own recent diary — a harvest, a bad night, being
sick on the deck — so a conversation is about the week they have actually had.
The code crosses the boundary; the words are entirely the host's, in
`CHAT_TOPICS`.

A conversation is the one errand that is **dropped rather than put down** when
something interrupts it. Half of one is worth nothing and the other half will
have walked off by the time it is picked up again.

The other Bim gets interrupted to have it, if it is merely working — sweeping,
or at the bay. Not if it is asleep, in the heads, or sitting on the deck. The
alternative is two Bims who are never both free at the same moment and so never
speak, and with the deck always finding something to be swept that is not a
hypothetical.

### Going without

The bar is the comfortable end of it. What matters is a second clock, in days,
that only a conversation resets — the same shape as malnutrition and as
standing in the mess, where reaching nothing is the start of it and not the end.

| alone for | stage | what it does |
| --- | --- | --- |
| 3 days | **desocialized** | writes low entries in its diary every four hours, and everything it does takes **a tenth longer** |
| 5 days | **badly desocialized** | that, and **sits down on the deck for ten minutes**, roughly every five hours, wherever it happens to be |
| 7 days | **isolated** | that, and **hurts itself every four hours** — ten points of health a time |
| 10 days | — | **3% an hour of giving up altogether**, and three points more for every further day |

The stages do not replace each other: an isolated Bim is still brooding and
still sitting down, because each one is the one before it and worse.

The self-harm rate is set against the mending, not picked by eye. A well-fed Bim
recovers half its health a day, so six bouts of ten is ten points a day of *net*
damage — the three days between the isolated stage and the despair that follows
it leave it worn down and alive, which is the shape the escalation wants. Make
it much faster and nothing ever reaches the tenth day to give up on it.

Two things that are easy to get wrong here and were:

- **Health mends every frame, so a bar taken to nothing by anything sudden is
  back above nothing before the game has looked at it.** Hunger works on health
  over hours and reaching zero from it is checked in the same frame it happens;
  a Bim hurting itself, or giving up, sets the bar to zero in a *later* part of
  the frame and the recovery on the next one undid it. The result was a Bim
  whose health read nought and who was still walking about. `Health::update`
  now returns at once from zero: nothing comes back from nothing.
- **The clock belongs to a body.** It is per Bim and never on anything shared —
  the same lesson `filth::Ordeal` is there to record. One Bim's solitude must
  never be reachable from the other's frame.

None of this can be switched off in the work list, and there is deliberately no
row for it: the list is for jobs, and a player who could put *company* at the
bottom could set a Bim to die of loneliness without meaning to.

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

There are two bunks, one per crew member: one against the left wall, one in the
top-right corner. A Bim only ever goes to its own. Either bunk's menu offers a
nap of thirty minutes or a sleep of six hours; both run the same chain — walk
over, up the ladder, under the covers, out cold, a stretch, and back down — and
differ only in how long the Bim stays put. The menu says what time it will be
up, and the status line counts the rest down while James sleeps.

The second bunk is the first one mirrored. A `Berth` carries which side of it
the deck is on, and the ladder, the spot the Bim stands on to climb it and the
way it turns to do so are all read off that rather than written into the
drawing twice. Everything else — pillow at the top of the room, head towards
it — is the same in both, so a sleeper lies the same way up whichever bunk it
is in.

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

`crates/game/src/clock.rs` keeps one clock for the whole game: minutes since midnight, and
which day it is. One real second is one game minute at 1x, so a day takes
twenty-four minutes of real time and a six-hour sleep takes six — and because
the clock runs off the same `dt` as everything else, the speed slider carries it
along. At 24x a whole day goes by in one minute.

Nothing about time crosses the wasm boundary as a string: the host reads the
minute count and formats it. The light level comes from the same clock, eased in
at dawn and out at dusk, and is laid over the finished frame as one tinted
rectangle — so nothing in `room.rs` has to know what time it is.

## The look of it

Everything is drawn from the two primitives in `crates/game/src/draw.rs`, so "futuristic"
here is a matter of palette and of what gets a light on it rather than of any
new drawing machinery. The deck is dark blue-grey, the fittings are composite
panel in three shades, and one cyan running light is picked up by every powered
surface: the counter fascia, the induction coils etched into the hob, the rim of
the table, the underside of the top bunk, the jambs of the bathroom door. Warm
colours are held back for heat and for trouble, which is why a live hob and a
locked door are the only two warm things in the room and both read instantly.

The two crew are told apart by their clothes and their hair, not by where they
happen to be standing: James is in the blue coverall with cropped hair, Kate in
the mauve one with hers worn long, which from directly above is a second
ellipse behind the head. Their names are written over them — James's in the
green that everything steerable uses, Kate's in plain ink, so which one takes
orders reads without being explained.

The page around it is arranged the same way: nothing is framed. The deck runs to
the edge of the window, the canvas carries no border of its own, and every panel
sits flush in a corner of it — the readout and the agendas top left, the
selected Bim's bars and crew sheet top right, the tray in the bottom-left
corner. A rounded box drawn
round the whole interface reads as a window frame inside a window, which is one
frame too many; only the header, with the name and the clock, sits above the
deck rather than on it.

The tray is the exception to everything being small. It is set a size larger
than the rest — wider, bigger type, taller hour cells — because the timetable
and the action thresholds under it are where the day is actually decided, and they are
read and edited rather than glanced at.

## How it fits together

The simulation is entirely Rust. Each frame the host calls `bims_update`, which
advances the world and rebuilds a flat buffer of shapes in wasm linear memory;
`web/bims.js` reads that buffer and replays it onto a canvas. The only things
crossing the boundary are a handful of numbers and one pointer, which means no
binding generator and no JavaScript in the build — `cargo build` is the whole
pipeline, and the module has zero imports.

### Ten crates

The repository is a cargo workspace and everything is under `crates/`. The
split is not tidiness — each line of it is something that has to give the same
answer in two places at once:

| Crate | What it is | Who else needs it |
| --- | --- | --- |
| `game` | The room: the simulation, and its wasm exports. A cdylib | — |
| `ship` | `web/ship.html`: the design phase, and the game it starts. Camera, pointer, draw buffer. The other cdylib | — |
| `world` | One star system, the ship in it, and the one clock they both run on | `ship`; the native server, which has to run the identical loop |
| `flight` | What a design does when you push it, and the closed-form plan that flies a trip | `world`; the native server, which has to put the ship in the same place |
| `shipdesign` | What a ship is made of and the rules for putting one together | `ship`, `flight` and `world`; the native server, which has to admit the same ships |
| `worldgen` | The galaxy, what is in each system, and station blueprints | The native server that will one day be authoritative, which has to generate the identical world from the same seed |
| `physics` | Ship mass, engine thrust, travel time. Pure arithmetic | `worldgen`, to check its layouts; `shipdesign` and `flight`, to weigh and fly a real ship |
| `economy` | Money: whole euros, the shared pool, what a station charges and which hold it goes in | `shipdesign`, `world` and `ship`; anything that ever charges for anything later |
| `health` | One body's health: conditions with stages, mending, death. Radiation dose, sickness and cancer are the first two | The crew step, which will hold one per Bim; the native server, which has to agree about who survived |
| `time` | How long a minute, an hour and a day are | All of them — a day that is two lengths is two games |

Everything builds for `wasm32` and for the host both, and `nix flake check`
builds them both ways and runs the libraries' unit tests natively. `game` is
wasm-shaped and is checked by the probes and harnesses in `scratchpad/`
instead; so is most of `ship`, bar the arithmetic that turns a design tile into
a place on screen and back, which is pure and is unit tested.

### Inside the room

Relative to `crates/game/src/`:

| File | What lives there |
| --- | --- |
| `lib.rs` | The wasm exports the host calls |
| `game.rs` | Ties the room, the Bim and the running task together; input |
| `room.rs` | The room: layout, fixture state, and how it is drawn |
| `bath.rs` | The heads: its bulkheads, its door, and its fittings |
| `dish.rs` | The dishwasher: what is in it, and the cycle it runs |
| `needs.rs` | What the Bim wants, and the rates that shape its day |
| `health.rs` | Going hungry, the three stages of it, and health |
| `filth.rs` | The state of the deck, and what a mess does to the Bim |
| `hydro.rs` | The hydroponic bay: five trays, and what goes in them |
| `manager.rs` | What the place is told to keep in stock |
| `schedule.rs` | The day's timetable, and when it is worth obeying |
| `task.rs` | The scripted chains — a meal, a sleep, a trip to the heads |
| `clock.rs` | The time of day, and how much light there is — restated from the `time` crate as `f32`, not defined again |
| `character.rs` | The Bim — how it decides where to go, and how it is drawn |
| `draw.rs` | The shape buffer and the local frame used for sprites |
| `math.rs`, `rng.rs` | Vectors, rectangles, angles, and a PCG32 generator |
| `web/bims.js` | Canvas renderer, input, and the fixture menus |

### Getting about

Every walk — a right-click order, and every leg of a scripted job — is planned
on a grid in `crates/game/src/nav.rs`. Obstacles are inflated by the body radius before the
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
still assumes the field *order* in `crates/game/src/draw.rs`.

One trap worth knowing: `f32::clamp` panics when its bounds are crossed, and
that panic path drags Rust's formatting machinery into the wasm — it cost 19 KB
of the binary before being swapped for the branchless `clamp` in `crates/game/src/math.rs`.
