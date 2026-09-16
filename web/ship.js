// The ship design phase: a palette, a tile grid, and everything that decides
// whether the ship is finished.
//
// This is the page `nix run .#ship` serves. It is a third front end, not a
// mode of either of the other two: `web/bims.js` is the room and knows
// nothing about tiles, `web/builder.js` is the menus in front of it and has
// no wasm at all. This one has its own — `ship.wasm`, out of `crates/ship` —
// and no `Game` anywhere in it.
//
// Two rules the page is built around and that are cheap to break:
//
//   * **No strings cross the wasm boundary.** The parts, the prices, the
//     reasons an edit was refused and the faults in a design are all numbers.
//     The three tables below are where every word on this page lives, and
//     `euros()` is where the euro sign and the digit grouping live — money
//     crosses as a plain count of euros, in the two `u32` halves the boundary
//     carries a 64-bit number in.
//   * **Everything that changes the ship goes through `net`.** It is a local
//     stand-in with a transport's shape, exactly as in web/builder.js: no
//     click handler calls `wasm.ship_place` directly, every Edit carries the
//     design hash it was made against, and the day this grows a socket the
//     change is to that object and to nothing else.

/** What each part is called. Indexed by the `PartKind` discriminant in
 * `crates/shipdesign/src/parts.rs`, which is written out there and never
 * renumbered. Adding a part is a variant, a name here, and the range check in
 * the tests. */
const PART_NAMES = [
  "Deck plating",
  "Wall",
  "Door",
  "Engine",
  "Bunk",
  "Cold store",
  "Worktop",
  "Hob",
  "Dishwasher",
  "Table",
  "Chair",
  "Toilet",
  "Basin",
  "Hydroponic bay",
  "Broom locker",
  "Structure",
  "Outside wall",
  "Helm",
  "Reactor",
  "Power conduit",
  "Battery",
  "Fuel tank",
  "Life support",
  "Airlock",
  "Sensor array",
  "Shelf",
  "Shower",
  "Thruster",
];

/** The palette, grouped the way a ship is thought about rather than the way
 * the enum is numbered.
 *
 * The rows are built off `ship_part_count()` rather than off this list, so a
 * kind added to the enum and forgotten here still gets a button — under
 * "Anything else", where it is obvious — instead of quietly not existing.
 * Same arrangement as the room's work panel, and for the same reason. */
const PART_GROUPS = [
  // Hull first, in the order a ship is actually built: frame, deck, skin,
  // then the ways through it.
  { name: "Hull", kinds: [15, 0, 1, 16, 2, 23] },
  { name: "Systems", kinds: [3, 27, 17, 18, 19, 20, 21, 22, 24] },
  { name: "Crew", kinds: [4, 9, 10, 26] },
  { name: "Galley", kinds: [5, 6, 7, 8] },
  { name: "Heads", kinds: [11, 12] },
  { name: "Storage", kinds: [25] },
  { name: "Bay", kinds: [13, 14] },
];

/** What a station sells, indexed by `physics::ResourceId`. The first four are
 * materials and the last two are food; what makes one food rather than metal
 * is which hold it goes in, and that is `economy::storage`. */
const RESOURCE_NAMES = [
  "Ore",
  "Metal",
  "Fuel",
  "Components",
  "Vegetables",
  "Tofu",
];

/** Where goods are stowed, indexed by `economy::Storage`. */
const STORAGE_NAMES = ["Shelves", "Fuel tanks", "Cold stores"];

/** How many units a buy or sell button moves. Three sizes, because a hundred
 * units of ore one at a time is not a decision anybody is making. */
const TRADE_STEPS = [1, 10, 100];

/** Why an edit was refused. Indexed by `EditError`; 0 never appears here
 * because 0 is "it took". */
const EDIT_LINES = {
  1: "That falls outside the build area.",
  2: "Something is already standing there.",
  3: "There is no deck under it.",
  4: "There is deck there already.",
  5: "There is not the money left for that.",
  6: "That part is not there any more.",
  7: "Take what is standing on it off first.",
  8: "The page asked for something that is not a part.",
  9: "The design is settled — nothing can be moved now.",
  10: "There is no structure under it. The frame goes down first.",
  11: "There is already something in that tile on that layer.",
  12: "There is not the money left for those goods.",
  13: "Nowhere aboard to put them — the ship needs more storage.",
  14: "There is not that much aboard to sell.",
  15: "Sell what is in it first.",
  16: "There are not the materials aboard to build that.",
};

/** What is wrong with the design. Indexed by `IssueCode` in
 * `crates/shipdesign/src/validate.rs`. An issue with no line here is dropped
 * from the list rather than shown as a placeholder — the same rule the
 * diary follows in web/bims.js — and the count check in
 * scratchpad/ship-check.mjs is what catches a missing one. */
const ISSUE_LINES = {
  1: "The ship is in more than one piece.",
  2: "Not enough bunks for the crew.",
  3: "Not enough chairs for the crew.",
  4: "No table to eat at.",
  5: "No cold store to keep food in.",
  6: "No worktop to prepare it on.",
  7: "No hob to cook it on.",
  8: "No dishwasher to clear up with.",
  9: "No toilet.",
  10: "No basin to wash at.",
  11: "Somewhere a Bim has to stand is blocked.",
  12: "Parts nobody could walk between.",
  20: "No engine — the ship goes nowhere.",
  21: "No engine pushes it forward, so it cannot set off.",
  22: "No hydroponic bay. The food aboard is all the food there will be.",
  23: "No broom locker, so nothing to sweep the deck with.",
  24: "The outside can see in. The crew will be irradiated here.",
  25: "Nothing to eat aboard.",
  26: "No helm, so nobody can fly it.",
  27: "No thruster, so nothing turns the ship.",
  28: "No airlock, so no way off it — a station can only be held beside.",
  29: "No sensor array. Nothing will be seen beyond eyesight.",
  30: "No fuel aboard, so no trip can be started.",
};

/** Why a trip could not be planned. Indexed by `flight::PlanError`; 0 never
 * appears because 0 is "nothing went wrong". */
const PLAN_ERRORS = {
  1: "No engine pushes the ship forward.",
  2: "Nothing to turn with — it cannot aim or stop.",
  3: "No helm to fly it from.",
  4: "Not enough fuel that is not already spoken for.",
  5: "Nobody has found that yet.",
  6: "The ship is already there.",
  7: "No fuel aboard at all.",
};

/** Why an order did nothing. Indexed by `world::Refusal`. Separate from the
 * list above on purpose: "you are not docked" and "you have no fuel" are
 * different things to be told, and a player given the wrong one goes and
 * solves the wrong problem. */
const REFUSALS = {
  1: "not while the ship is away from a station",
  2: "there is not the money",
  3: "there is nowhere aboard to put it",
  4: "there is not that much aboard to sell",
  5: "that is not your order to give",
  6: "there is no trip to stop",
  7: "the sum will not go",
};

/** Which part of a trip the ship is in. Indexed by `flight::Phase`. */
const PHASE_NAMES = ["Aligning", "Burning", "Turning", "Braking", "Holding"];

/** What happened. Indexed by the code `world::WorldEvent::code` gives, and
 * each one is handed the one number the event carries.
 *
 * An event whose code has no line here is **dropped** rather than shown as a
 * placeholder — the same rule the diary follows in web/bims.js — and the count
 * check in scratchpad/ship-check.mjs is what catches a missing one. */
const EVENT_LINES = {
  1: () => "Under way.",
  2: (id) => `Docked at station ${id}.`,
  3: () => "Holding station.",
  4: () => "Stopping.",
  5: (why) => `Cannot fly there — ${PLAN_ERRORS[why] ?? "no reason given."}`,
  6: () => "Something new on the scanner.",
  7: (kind) => (kind === 0 ? "Out into open space." : "Alongside."),
  8: (units) => `${units} aboard.`,
  9: (units) => `${-units} sold.`,
  10: (why) => `That could not be done — ${REFUSALS[why] ?? "no reason given"}.`,
};

/** What each kind of body is called. Indexed by `worldgen::BodyKind`. */
const BODY_KIND_NAMES = [
  "Rocky planet",
  "Gas giant",
  "Ice world",
  "Asteroid belt",
];

/** And each kind of station, by `worldgen::StationKind`. */
const STATION_KIND_NAMES = [
  "Orbital",
  "Refinery",
  "Mining outpost",
  "Derelict",
  "Relay",
];

/** `physics::ResourceId::Fuel`. The one resource the trade panel has to treat
 * differently: what is held against a trip under way is not the crew's to
 * sell. */
const FUEL = 2;

/** The two views. Indexed by `ship::game::ViewMode`. */
const VIEW_NAMES = ["Ship", "System map"];

/** How near a click has to come to a map icon to count as picking it, in CSS
 * pixels. Measured on screen rather than in world units: the thing being aimed
 * at is an icon, and at a map scale where a whole system fits on a laptop a
 * world-unit tolerance is either the entire screen or a thousandth of a pixel. */
const MAP_PICK_SLOP = 14;

/** `ship::game::ViewMode`, the host's half of the pair. */
const VIEW_SHIP = 0;
const VIEW_MAP = 1;

/** Ceiling on world steps per frame.
 *
 * It has to be at least `TOP_SPEED * 60 / 30`, or the top of the speed range
 * stops being reachable on a display that is keeping up at 30fps and the world
 * quietly runs slower than the button says. 24x at 60Hz is 24 steps a frame
 * and at 30Hz is 48, so this is that with a third of headroom — the same
 * arrangement, and the same reasoning, as MAX_STEPS_PER_FRAME in web/bims.js. */
const MAX_STEPS_PER_FRAME = 64;

/** How many lines of what-just-happened stay on screen. */
const LOG_LINES = 4;

/** The default galaxy shape: `worldgen::GalaxyType::SpiralTwoArm`. The seed's
 * default is not here — it is `world::data::DEFAULT_SEED` and the page asks
 * wasm for it, because a constant written down in two places is a constant
 * that will disagree with itself. */
const DEFAULT_GALAXY = 0;

/** The one issue that is not just another row.
 *
 * Radiation is the only fault on the list that kills people, it is always
 * first, and it is styled louder than the errors — which is a deliberate
 * exception to "an error blocks Accept and a warning does not". It does not
 * block; it shouts. */
const ISSUE_GRAVE = 24;

/** What the lobby hands over when nobody chose anything. Opening ship.html
 * with no query is a solo game on a standard ship, with the standard purse —
 * which, one player being one player, is that plus the solo bonus once wasm
 * has worked the pool out. */
const DEFAULTS = { area: 40, money: 100000, players: 1, slot: 0 };

/** Sane bounds on what the query string may say. A hand-typed `?area=9000`
 * is a build area with a million tiles in it, and a hand-typed `?money=1e30`
 * is a pool that would not survive the trip into a 64-bit integer. */
const AREA_MIN = 8;
const AREA_MAX = 120;
const PLAYERS_MAX = 4;
const MONEY_MAX = 1000000000;
/** A seed crosses in two halves, so each of them is a `u32`. */
const U32_MAX = 4294967295;
/** `worldgen::GalaxyType` has four shapes in it. */
const GALAXY_MAX = 3;

/** A number of euros, as words. The **only** place either the sign or the
 * grouping exists — the same rule, and the same function, as web/builder.js:
 * what crosses the boundary is a bare count of euros. */
function euros(value) {
  return `€${grouped(value)}`;
}

/** A big number with its digits in threes. Split out of `euros` because
 * distances want it too and a distance is not money — but the grouping itself
 * is one rule, and one rule goes in one place. */
function grouped(value) {
  return String(value).replace(/\B(?=(\d{3})+(?!\d))/g, "\u00a0");
}

/** How long a refusal stays on screen. */
const SAID_SECONDS = 4;

/** WASD, in CSS pixels a second. */
const PAN_SPEED = 900;

/** How much one notch of wheel is worth. Applied by the host rather than in
 * wasm so no exponential has to be linked in for the sake of a scroll. */
const ZOOM_PER_PIXEL = 0.0016;

/** Longest a frame may pretend to be, so a backgrounded tab does not come
 * back and pan the view across the room. */
const MAX_FRAME_DT = 0.1;

/** `Phase` in crates/ship. The design is still being laid out at 0 and
 * settled at 1, and that is the only question the page asks about it: "is it
 * finished", "may I still edit" and "which phase is it" are the same thing,
 * and three ways of asking would be three things that can disagree. */
const PHASE_DESIGN = 0;

/** Floats per shape, if the wasm somehow will not say. */
const STRIDE_FALLBACK = 12;

const KIND_RECT = 0;
const KIND_ELLIPSE = 1;

boot().catch((err) => {
  console.error(err);
  const box = document.getElementById("error");
  if (box && !box.textContent) box.textContent = `Could not start: ${err.message}`;
});

async function boot() {
  const byId = (id) => document.getElementById(id);

  // Two canvases, one for each half of the page, and exactly one of them is
  // ever being painted — a screen is hidden by the `hidden` attribute, and a
  // hidden canvas has no size worth measuring. `stage` is whichever is up.
  const canvas = byId("stage");
  const ctx = canvas.getContext("2d");
  const gameCanvas = byId("game-stage");
  const gameCtx = gameCanvas.getContext("2d");
  let stage = canvas;
  let stageCtx = ctx;
  const said = byId("said");

  // --- what the lobby chose ---------------------------------------------
  //
  // Numbers only, all four of them, straight off the query string. The
  // builder writes it and this reads it; nothing but numbers crosses into
  // wasm, and starting as we mean to go on keeps that honest.

  const chosen = readSettings();

  /** Numbers, clamped, with the defaults for anything missing or silly.
   * `location` is guarded rather than assumed: it is absent under the node
   * harness's stub for a page, and a designer that throws on line one is a
   * blank screen with no message. */
  function readSettings() {
    const query =
      typeof location !== "undefined" && location.search ? location.search : "";
    const asked = new Map();
    for (const pair of query.replace(/^\?/, "").split("&")) {
      if (!pair) continue;
      const [key, value] = pair.split("=");
      asked.set(decodeURIComponent(key), Number(decodeURIComponent(value ?? "")));
    }
    const number = (key, fallback) => {
      const value = asked.get(key);
      return Number.isFinite(value) ? value : fallback;
    };
    const players = whole(number("players", DEFAULTS.players), 1, PLAYERS_MAX);
    return {
      area: whole(number("area", DEFAULTS.area), AREA_MIN, AREA_MAX),
      // A whole number of euros, exactly as the lobby wrote it. Rounded
      // rather than trusted: two players have to end up with the same pool
      // down to the last euro, and a fraction is a promise about rounding
      // that nobody made.
      money: whole(number("money", DEFAULTS.money), 0, MONEY_MAX),
      players,
      slot: whole(number("slot", DEFAULTS.slot), 0, players - 1),
      // Which world the game will open in. **Temporary**: the lobby's World
      // tab will pick these, and then they come from there instead of off a
      // query string. `null` rather than a number when nobody said, so the
      // fallback can be the one in wasm rather than a second copy here.
      seedHi: asked.has("seedHi") ? whole(number("seedHi", 0), 0, U32_MAX) : null,
      seedLo: asked.has("seedLo") ? whole(number("seedLo", 0), 0, U32_MAX) : null,
      galaxy: whole(number("galaxy", DEFAULT_GALAXY), 0, GALAXY_MAX),
    };
  }

  function whole(value, lo, hi) {
    return Math.round(clampNumber(value, lo, hi));
  }

  function clampNumber(value, lo, hi) {
    return value < lo ? lo : value > hi ? hi : value;
  }

  // --- the module --------------------------------------------------------

  // Plain `instantiate` rather than `instantiateStreaming`: it works no matter
  // how the static server labels .wasm files. The same cache-busting token the
  // page was loaded with, so the wasm always matches the script that fetched
  // it.
  const build = window.BIMS_BUILD ?? Date.now();
  const bytes = await fetch(`ship.wasm?v=${build}`).then((r) => r.arrayBuffer());
  const { instance } = await WebAssembly.instantiate(bytes, {});
  const wasm = instance.exports;

  const stride = wasm.ship_stride ? wasm.ship_stride() : STRIDE_FALLBACK;
  const partCount = wasm.ship_part_count();
  const resourceCount = wasm.ship_resource_count();

  /** A money figure, out of the two `u32` halves the boundary carries it in.
   * Every amount on this page comes back through here; a JS number holds a
   * whole euro exactly well past anything a pool will ever be. */
  const amount = (hi, lo) => hi * 2 ** 32 + lo;

  let canvasSize = { w: 0, h: 0 };
  let dpr = 1;

  function resize() {
    dpr = window.devicePixelRatio || 1;
    const w = Math.max(64, stage.clientWidth);
    const h = Math.max(64, stage.clientHeight);
    stage.width = Math.round(w * dpr);
    stage.height = Math.round(h * dpr);
    canvasSize = { w, h };
    return canvasSize;
  }

  resize();
  wasm.ship_init(
    chosen.area,
    // The same two halves, the other way about. wasm adds the solo bonus and
    // multiplies by the crew: what goes in is what one Bim brings.
    Math.floor(chosen.money / 2 ** 32),
    chosen.money >>> 0,
    chosen.players,
    chosen.slot,
    // The world. Falling back to the wasm's own default rather than to a
    // number written down here, so there is one copy of it.
    chosen.seedHi ?? wasm.ship_default_seed_hi(),
    chosen.seedLo ?? wasm.ship_default_seed_lo(),
    chosen.galaxy,
    canvasSize.w,
    canvasSize.h,
  );

  window.addEventListener("resize", () => {
    resize();
    wasm.ship_resize(canvasSize.w, canvasSize.h);
  });

  // --- the network that is not there yet ---------------------------------
  //
  // Same shape a real one will have, and the same seam as web/builder.js: you
  // ask it to do something and it tells you what happened by firing an event.
  // Nothing below reaches past it.
  //
  // Locally both ends are here. `net.send` is the client half — it stamps
  // every message with the design hash it was made against — and `net.host`
  // is the other end, which applies messages **in arrival order** and reports
  // a refusal back to whoever sent it. That is the whole protocol, and it is
  // written out now rather than discovered later inside a click handler.

  const net = {
    slot: wasm.ship_local_slot(),
    players: wasm.ship_players(),
    handlers: new Map(),

    on(event, fn) {
      if (!net.handlers.has(event)) net.handlers.set(event, []);
      net.handlers.get(event).push(fn);
    },

    emit(event, payload) {
      for (const fn of net.handlers.get(event) ?? []) fn(payload);
    },

    /** Put a part down. */
    place(kind, x, y, rotation) {
      return net.send({ place: { kind, x, y, rotation } });
    },

    /** Take one off. */
    remove(partId) {
      return net.send({ remove: partId });
    },

    /** Take goods aboard, or put them back. Edits like any other: stamped
     * with the design they were made against, applied by the host end, and
     * they clear everybody's Accept because the ship they accepted is now
     * carrying something else. */
    buy(resource, units) {
      return net.send({ trade: { resource, units, buying: true } });
    },

    sell(resource, units) {
      return net.send({ trade: { resource, units, buying: false } });
    },

    /** Accept, or take an Accept back. */
    accept(on) {
      return net.send({ accept: on === true });
    },

    /** Stamp a message with the design it was made against and post it. With
     * a transport behind this, that stamp is the whole reason an Edit sent
     * while somebody else was building can be reported as stale rather than
     * landing somewhere unintended. */
    send(message) {
      return net.host.receive({ from: net.slot, at: designHash(), ...message });
    },

    /** An order to the ship, once the game has started.
     *
     * Stamped with the **step** it applies at rather than with a design hash.
     * The two stamps are two different questions and both are the right one
     * for what they are on: an Edit has to be judged against the ship it was
     * made for, and an order has to be judged against *when* — the world moves
     * on its own, so a message that arrived late is a message about a moment
     * that has gone. A transport behind this is what makes either mean
     * anything; locally both ends are here. */
    order(what) {
      return net.host.receive({
        from: net.slot,
        at: designHash(),
        step: wasm.ship_world_steps() + 1,
        // The world this player believes they are in. Nothing reads it yet —
        // there is no other end to disagree with — and it is stamped on now
        // because the thing a transport will need is a number to compare, and
        // a loop that was not built to be comparable is a loop that has to be
        // rebuilt to be.
        world: { hi: wasm.ship_world_checksum_hi(), lo: wasm.ship_world_checksum_lo() },
        order: what,
      });
    },

    fly(target) {
      return net.order({ fly: target });
    },

    stop() {
      return net.order({ stop: true });
    },

    setSpeed(code) {
      return net.order({ speed: code });
    },

    /** Take goods aboard, or put them back, at a station. The design phase's
     * `buy` and `sell` above are the *other* transaction — instant, against a
     * design, out of a budget — and they stop working the moment the ship is
     * real. */
    deal(resource, units, buying) {
      return net.order({ deal: { resource, units, buying } });
    },

    /** Somebody dropped out. A stub, deliberately: the crew count is frozen
     * at Start and stays frozen for this step, so a ship designed for four
     * still wants four bunks after one leaves. Sizing the ship down under the
     * designers' feet is a decision, not a cleanup. */
    playerLeft(slot) {
      net.emit("left", { slot });
    },

    host: {
      /** The other end of the wire. Messages are applied in the order they
       * arrive and nothing is queued or reordered; one that `apply` refuses
       * is rejected and reported to its sender. */
      receive(message) {
        let why = 0;
        let took = true;
        if (message.place) {
          const put = message.place;
          why = wasm.ship_place(put.kind, put.x, put.y, put.rotation);
          took = why === 0;
        } else if (message.remove !== undefined) {
          why = wasm.ship_remove(message.remove);
          took = why === 0;
        } else if (message.trade) {
          const deal = message.trade;
          why = deal.buying
            ? wasm.ship_buy(deal.resource, deal.units)
            : wasm.ship_sell(deal.resource, deal.units);
          took = why === 0;
        } else if (message.accept !== undefined) {
          if (message.accept) {
            const at = message.at;
            took = wasm.ship_accept(message.from, at.hi, at.lo) !== 0;
          } else {
            wasm.ship_unaccept(message.from);
            took = true;
          }
        } else if (message.order) {
          applyOrder(message.from, message.order);
          took = true;
        }
        const result = { to: message.from, ok: took, why, at: message.at };
        net.emit(took ? "applied" : "rejected", result);
        return result;
      },
    },
  };

  /** The design's identity, in the two halves the boundary carries it in. */
  function designHash() {
    return { hi: wasm.ship_hash_hi(), lo: wasm.ship_hash_lo() };
  }

  /** Put an order on the world's queue.
   *
   * **Queued, never applied.** Every one of these lands at the step the
   * message was stamped for, which is what makes the stamp mean something —
   * a command carried out the instant a button was pressed would work
   * perfectly and would have nowhere for a transport to fit. */
  function applyOrder(slot, what) {
    if (what.fly) {
      const target = what.fly;
      if (target.node) wasm.ship_cmd_confirm_node(slot, target.node.kind, target.node.id);
      else wasm.ship_cmd_confirm_point(slot, target.x, target.y);
    } else if (what.stop) {
      wasm.ship_cmd_abort(slot);
    } else if (what.speed !== undefined) {
      wasm.ship_cmd_speed(slot, what.speed);
    } else if (what.deal) {
      const deal = what.deal;
      if (deal.buying) wasm.ship_cmd_buy(slot, deal.resource, deal.units);
      else wasm.ship_cmd_sell(slot, deal.resource, deal.units);
    }
  }

  /** Whether the ship can still be changed. */
  function stillDesigning() {
    return wasm.ship_phase() === PHASE_DESIGN;
  }

  // --- the palette -------------------------------------------------------

  const partButtons = new Map();

  function buildPalette() {
    const groups = [];
    const placed = new Set();
    for (const group of PART_GROUPS) {
      const kinds = group.kinds.filter((kind) => kind < partCount);
      if (kinds.length === 0) continue;
      groups.push(partGroup(group.name, kinds));
      for (const kind of kinds) placed.add(kind);
    }
    // Whatever the enum has that the groups above have not. Empty in a
    // healthy build; a heading nobody meant to see is the point.
    const rest = [];
    for (let kind = 0; kind < partCount; kind++) {
      if (!placed.has(kind)) rest.push(kind);
    }
    if (rest.length > 0) groups.push(partGroup("Anything else", rest));
    byId("palette").replaceChildren(...groups);
  }

  function partGroup(name, kinds) {
    const box = document.createElement("div");
    box.className = "group";
    const head = document.createElement("h2");
    head.textContent = name;
    box.appendChild(head);
    for (const kind of kinds) box.appendChild(partButton(kind));
    return box;
  }

  function partButton(kind) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "part";
    button.dataset.part = String(kind);

    const swatch = document.createElement("span");
    swatch.className = "swatch";
    // The colour comes out of wasm, which is what paints the part on the
    // canvas. One table, so a button cannot be a different colour from the
    // thing it places.
    const r = wasm.ship_part_color(kind, 0);
    const g = wasm.ship_part_color(kind, 1);
    const b = wasm.ship_part_color(kind, 2);
    swatch.style.background = `rgb(${r},${g},${b})`;

    const name = document.createElement("span");
    name.className = "name";
    name.textContent = PART_NAMES[kind] ?? `Part ${kind}`;

    const size = document.createElement("span");
    size.className = "size";
    size.dataset.size = String(kind);

    button.append(swatch, name, size);
    button.addEventListener("click", () => {
      wasm.ship_set_tool(kind);
      paintPalette();
      // The money readout marks a pool the *current tool* would overdraw, so
      // it follows the tool as well as the ship. Nothing was edited, so the
      // rest of afterChange has nothing to do.
      paintMoney();
    });
    partButtons.set(kind, button);
    return button;
  }

  /** Which tool is on, what each part's footprint is at the current turn, and
   * whether any of it can be touched at all. */
  function paintPalette() {
    const tool = wasm.ship_tool();
    const editable = stillDesigning();
    for (const [kind, button] of partButtons) {
      button.className = kind === tool ? "part on" : "part";
      button.disabled = !editable;
      const size = button.querySelector(".size");
      const w = wasm.ship_part_w(kind);
      const h = wasm.ship_part_h(kind);
      size.textContent = w === 1 && h === 1 ? "" : `${w}×${h}`;
    }
  }

  // --- the money ---------------------------------------------------------
  //
  // One pool, and there is no per-player purse anywhere: everybody's money is
  // in it, a lone player's bonus is in it, and every part anybody places
  // comes out of it.

  let moneyLeft = null;

  function buildMoney() {
    const row = document.createElement("div");
    row.className = "money";
    row.dataset.money = "pool";
    const name = document.createElement("span");
    name.className = "name";
    name.textContent = "Money";
    const left = document.createElement("span");
    left.className = "left";
    // What is left, beside what there was. The pool is fixed for the whole
    // phase, so it is written once here rather than repainted.
    const all = document.createElement("span");
    all.className = "of";
    all.textContent = `/ ${euros(pool())}`;
    row.append(name, left, all);
    moneyLeft = { row, left };
    byId("money").replaceChildren(row);
  }

  function pool() {
    return amount(wasm.ship_pool_hi(), wasm.ship_pool_lo());
  }

  function remaining() {
    return amount(wasm.ship_remaining_hi(), wasm.ship_remaining_lo());
  }

  function partPrice(kind) {
    return amount(wasm.ship_part_price_hi(kind), wasm.ship_part_price_lo(kind));
  }

  /** What is left, and whether the tool under the ghost still fits in it. A
   * pool the current tool would overdraw is marked, which is the only warning
   * a player gets before a click is simply refused. */
  function paintMoney() {
    const left = remaining();
    moneyLeft.left.textContent = euros(left);
    moneyLeft.row.className = partPrice(wasm.ship_tool()) > left ? "money short" : "money";
  }

  // --- the station's goods -----------------------------------------------
  //
  // One row per resource: what it costs, how much is aboard, and the buttons
  // that move it. Everything goes through `net` like every other edit, so a
  // purchase is applied and reported the same way a wall is.

  const goodRows = [];
  const holdRows = [];

  function tradePrice(resource) {
    return amount(wasm.ship_trade_price_hi(resource), wasm.ship_trade_price_lo(resource));
  }

  function buildTrade() {
    const goods = [];
    for (let id = 0; id < resourceCount; id++) {
      const row = document.createElement("div");
      row.className = "good";
      row.dataset.resource = String(id);

      const name = document.createElement("span");
      name.className = "name";
      name.textContent = RESOURCE_NAMES[id] ?? `Resource ${id}`;
      const price = document.createElement("span");
      price.className = "price";
      // Fixed for the whole phase — one price list, every station — so it is
      // written once here rather than repainted.
      price.textContent = euros(tradePrice(id));
      const units = document.createElement("span");
      units.className = "units";

      const deal = document.createElement("span");
      deal.className = "deal";
      for (const step of TRADE_STEPS) {
        deal.appendChild(dealButton(id, step, true));
      }
      for (const step of TRADE_STEPS) {
        deal.appendChild(dealButton(id, step, false));
      }

      row.append(name, price, units, deal);
      goods.push(row);
      goodRows.push({ row, units, id });
    }
    byId("goods").replaceChildren(...goods);

    const holds = [];
    for (let class_ = 0; class_ < wasm.ship_storage_count(); class_++) {
      const row = document.createElement("div");
      row.className = "hold";
      row.dataset.storage = String(class_);
      const name = document.createElement("span");
      name.className = "name";
      name.textContent = STORAGE_NAMES[class_] ?? `Storage ${class_}`;
      const used = document.createElement("span");
      used.className = "used";
      row.append(name, used);
      holds.push(row);
      holdRows.push({ row, used, class: class_ });
    }
    byId("holds").replaceChildren(...holds);
  }

  function dealButton(resource, step, buying) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = buying ? "deal-on buy" : "deal-on sell";
    button.dataset[buying ? "buy" : "sell"] = String(step);
    button.textContent = buying ? `+${step}` : `\u2212${step}`;
    button.addEventListener("click", () => {
      if (button.disabled) return;
      const done = buying ? net.buy(resource, step) : net.sell(resource, step);
      remark(done.ok ? "" : (EDIT_LINES[done.why] ?? "That could not be done."));
      afterChange();
    });
    return button;
  }

  /** What is aboard, what it would cost to take more, and how full each hold
   * is. A hold over its capacity cannot happen — `apply` refuses — so the
   * mark is for the one that is *full*, which is the thing a player is about
   * to be refused for. */
  function paintTrade() {
    const editable = stillDesigning();
    const left = remaining();
    for (const cell of goodRows) {
      const aboard = wasm.ship_cargo(cell.id);
      cell.units.textContent = String(aboard);
      const class_ = wasm.ship_storage_of(cell.id);
      const room = wasm.ship_storage_capacity(class_) - wasm.ship_storage_used(class_);
      const price = tradePrice(cell.id);
      for (const button of cell.row.querySelectorAll("button")) {
        const buy = button.dataset.buy;
        const step = Number(buy ?? button.dataset.sell);
        button.disabled = !editable || (buy ? step * price > left || step > room : step > aboard);
      }
      cell.row.className = aboard > 0 ? "good carried" : "good";
    }
    for (const cell of holdRows) {
      const total = wasm.ship_storage_capacity(cell.class);
      const used = wasm.ship_storage_used(cell.class);
      cell.used.textContent = `${used} / ${total}`;
      cell.row.className = total > 0 && used >= total ? "hold full" : "hold";
    }
  }

  // --- what is wrong with it ---------------------------------------------

  const issueList = byId("issue-list");

  /** One row per issue, worst first.
   *
   * An issue whose code has no line in ISSUE_LINES is **dropped** rather than
   * rendered as a placeholder — the same rule the diary follows. What catches
   * a missing line is the row count against `ship_issue_count()`, which is
   * what scratchpad/ship-check.mjs asserts. */
  function paintIssues() {
    const count = wasm.ship_issue_count();
    const rows = [];
    for (let i = 0; i < count; i++) {
      const code = wasm.ship_issue_code(i);
      const line = ISSUE_LINES[code];
      if (!line) continue;
      const error = wasm.ship_issue_severity(i) === 0;
      const row = document.createElement("li");
      // Radiation outranks the error styling on purpose: it is the one thing
      // on this list that kills the crew, and it does not block Accept, so
      // the only way it can be heard is by being louder.
      row.className = code === ISSUE_GRAVE ? "grave" : error ? "error" : "warning";
      row.dataset.issue = String(code);
      const text = document.createElement("span");
      text.textContent = line;
      const where = document.createElement("span");
      where.className = "where";
      const tiles = wasm.ship_issue_tile_count(i);
      where.textContent = tiles > 0 ? `${tiles}` : "";
      row.append(text, where);
      // Resting on a row rings the tiles it names. A highlight, not a
      // tooltip: nothing is said, and it goes the moment the pointer moves.
      row.addEventListener("pointerenter", () => wasm.ship_focus_issue(i));
      row.addEventListener("pointerleave", () => wasm.ship_clear_focus());
      rows.push(row);
    }
    if (rows.length === 0) {
      const clean = document.createElement("li");
      clean.className = "clean";
      clean.textContent = "Nothing wrong with it.";
      rows.push(clean);
    }
    issueList.replaceChildren(...rows);
  }

  // --- the crew, and their accepts ---------------------------------------

  const slotList = byId("slots");
  const acceptButton = byId("accept");
  const acceptNote = byId("accept-note");

  function paintCrew() {
    const rows = [];
    for (let slot = 0; slot < net.players; slot++) {
      const row = document.createElement("li");
      const yes = wasm.ship_accepted(slot) !== 0;
      row.className = yes ? "yes" : "no";
      row.dataset.slot = String(slot);
      const pip = document.createElement("i");
      pip.className = "pip";
      const who = document.createElement("span");
      who.textContent = slot === net.slot ? `Player ${slot + 1} (you)` : `Player ${slot + 1}`;
      const role = document.createElement("span");
      role.className = "role";
      role.textContent = yes ? "Accepted" : "Designing";
      row.append(pip, who, role);
      rows.push(row);
    }
    slotList.replaceChildren(...rows);
  }

  function paintAccept() {
    const blocked = wasm.ship_has_errors() !== 0;
    const mine = wasm.ship_accepted(net.slot) !== 0;
    const editable = stillDesigning();
    acceptButton.disabled = blocked || !editable;
    acceptButton.className = mine ? "big on" : "big";
    acceptButton.textContent = mine ? "Accepted" : "Accept";

    let agreed = 0;
    for (let slot = 0; slot < net.players; slot++) {
      if (wasm.ship_accepted(slot) !== 0) agreed++;
    }
    acceptNote.textContent = blocked
      ? "Put the errors above right first."
      : net.players === 1
        ? "Accepting settles the ship."
        : `${agreed} of ${net.players} have accepted.`;
  }

  acceptButton.addEventListener("click", () => {
    if (acceptButton.disabled) return;
    const mine = wasm.ship_accepted(net.slot) !== 0;
    const done = net.accept(!mine);
    if (!done.ok) remark("That Accept was for a ship that has since changed.");
    afterChange();
  });

  // --- the handoff -------------------------------------------------------

  const screens = new Map();
  for (const el of document.querySelectorAll("[data-screen]")) {
    screens.set(el.dataset.screen, el);
  }

  function show(name) {
    for (const [id, el] of screens) {
      if (id === name) el.removeAttribute("hidden");
      else el.setAttribute("hidden", "");
    }
  }

  // --- the game ----------------------------------------------------------
  //
  // One world, one clock, and the page turning the crank on it. The design
  // phase above is over by the time any of this runs: the ship is settled,
  // it is docked at the spawn station, and what was left of the pool is in
  // the crew's hands.

  /** Whether the world is open. One question, one export — see `ship_phase`. */
  function playing() {
    return !stillDesigning() && wasm.ship_world_ready() !== 0;
  }

  /** What the helm is aimed at, for the local player and nobody else. Either
   * `{node: {kind, id}}` or `{x, y}`, which is exactly what `net.fly` takes. */
  let aimed = null;

  /** Whether the page has already moved over. */
  let gameShown = false;

  /** Move the page over to the game: swap the canvas, size it, and build the
   * panels that only exist out here.
   *
   * Called from two places on purpose. The local player's own Accept comes
   * through `afterChange`, which is instant; **anybody else's** arrives as a
   * message and the page finds out on the next frame. Only one of those two
   * routes exists today and the other one is the whole point of the seam, so
   * both are wired up and the guard is what keeps it to once. */
  function startGame() {
    if (gameShown) return;
    gameShown = true;
    show("game");
    stage = gameCanvas;
    stageCtx = gameCtx;
    resize();
    wasm.ship_resize(canvasSize.w, canvasSize.h);
    buildSpeeds();
    buildGameTrade();
    paintGame();
  }

  const speedButtons = [];

  function buildSpeeds() {
    if (speedButtons.length > 0) return;
    const row = byId("speed-buttons");
    const made = [];
    for (let code = 0; code < wasm.ship_speed_count(); code++) {
      const button = document.createElement("button");
      button.type = "button";
      button.dataset.speed = String(code);
      // The label is the multiplier wasm gives, not a table here: a name list
      // that could disagree with the thing it names is a name list that will.
      const times = wasm.ship_speed_multiplier(code);
      button.textContent = times === 0 ? "‖" : `${times}×`;
      button.addEventListener("click", () => net.setSpeed(code));
      made.push(button);
      speedButtons.push({ button, code });
    }
    row.replaceChildren(...made);
  }

  const gameGoodRows = [];
  const gameHoldRows = [];

  function buildGameTrade() {
    if (gameGoodRows.length > 0) return;
    const goods = [];
    for (let id = 0; id < resourceCount; id++) {
      const row = document.createElement("div");
      row.className = "good";
      row.dataset.resource = String(id);
      const name = document.createElement("span");
      name.className = "name";
      name.textContent = RESOURCE_NAMES[id] ?? `Resource ${id}`;
      const price = document.createElement("span");
      price.className = "price";
      price.textContent = euros(tradePrice(id));
      const units = document.createElement("span");
      units.className = "units";
      const deal = document.createElement("span");
      deal.className = "deal";
      for (const step of TRADE_STEPS) deal.appendChild(gameDealButton(id, step, true));
      for (const step of TRADE_STEPS) deal.appendChild(gameDealButton(id, step, false));
      row.append(name, price, units, deal);
      goods.push(row);
      gameGoodRows.push({ row, units, id });
    }
    byId("game-goods").replaceChildren(...goods);

    const holds = [];
    for (let class_ = 0; class_ < wasm.ship_storage_count(); class_++) {
      const row = document.createElement("div");
      row.className = "hold";
      row.dataset.storage = String(class_);
      const name = document.createElement("span");
      name.className = "name";
      name.textContent = STORAGE_NAMES[class_] ?? `Storage ${class_}`;
      const used = document.createElement("span");
      used.className = "used";
      row.append(name, used);
      holds.push(row);
      gameHoldRows.push({ row, used, class: class_ });
    }
    byId("game-holds").replaceChildren(...holds);
  }

  function gameDealButton(resource, step, buying) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = buying ? "deal-on buy" : "deal-on sell";
    button.dataset[buying ? "buy" : "sell"] = String(step);
    button.textContent = buying ? `+${step}` : `−${step}`;
    button.addEventListener("click", () => {
      if (button.disabled) return;
      net.deal(resource, step, buying);
    });
    return button;
  }

  /** Everything on the game screen. Repainted every frame, because almost all
   * of it changes every frame: a clock, a velocity, a fuel gauge. */
  function paintGame() {
    paintClock();
    paintHelm();
    paintSpeeds();
    paintGameTrade();
    paintShipFacts();
    paintGameReadout();
  }

  function paintClock() {
    const minutes = wasm.ship_world_minutes();
    const hour = Math.floor(minutes / 60);
    const minute = Math.floor(minutes % 60);
    byId("clock").textContent =
      `Day ${wasm.ship_world_day()} · ` +
      `${String(hour).padStart(2, "0")}:${String(minute).padStart(2, "0")}`;
    byId("purse").textContent = euros(remaining());

    const docked = wasm.ship_docked_at();
    const frame = wasm.ship_frame_kind() !== 0 ? nodeName(wasm.ship_frame_node_kind(), wasm.ship_frame_node_id()) : "";
    byId("where").textContent = docked
      ? `Docked · ${nodeName(1, docked - 1)}`
      : frame
        ? `Alongside ${frame}`
        : "Open space";
  }

  /** What a thing on the map is called. A kind and a number, because a kind is
   * a fixed table and an identity is a number — no strings cross the boundary,
   * so this is the most the page can honestly say. */
  function nodeName(kind, id) {
    return kind === 1
      ? `${STATION_KIND_NAMES[wasm.ship_map_type(mapIndexOf(kind, id))] ?? "Station"} ${id}`
      : `${BODY_KIND_NAMES[wasm.ship_map_type(mapIndexOf(kind, id))] ?? "Body"} ${id}`;
  }

  function mapIndexOf(kind, id) {
    for (let i = 0; i < wasm.ship_map_count(); i++) {
      if (wasm.ship_map_kind(i) === kind && wasm.ship_map_id(i) === id) return i;
    }
    return 0;
  }

  /** The trip the local player is looking at, or the one being flown.
   *
   * The preview is worked out here and is **never** a command: two players
   * hovering over different planets must not be an argument about where the
   * ship is going. */
  function paintHelm() {
    // Re-quoted every frame while the player is aiming at something. A quote
    // goes stale the moment the ship moves, and while a trip is already under
    // way most of the bill is *stopping first* — which shrinks as the ship
    // slows. A number that was right when it was worked out and is wrong now
    // is worse than no number.
    if (aimed === null) wasm.ship_preview_clear();
    else if (aimed.node) wasm.ship_preview_node(mapIndexOf(aimed.node.kind, aimed.node.id));
    else wasm.ship_preview_point(aimed.x, aimed.y);

    const plan = byId("plan");
    const state = wasm.ship_preview_state();
    const rows = [];
    if (state === 2) {
      plan.className = "refused";
      rows.push(["", PLAN_ERRORS[wasm.ship_preview_error()] ?? "That cannot be flown."]);
    } else if (state === 1) {
      plan.className = "";
      rows.push(["Going to", describeAim()]);
      const minutes = wasm.ship_preview_minutes();
      rows.push(["Arrives in", spell(minutes)]);
      const stopping = wasm.ship_preview_stopping();
      // A redirect stops first, and that is usually most of the bill. Saying
      // the total without saying that reads as the new trip being enormous.
      if (stopping > 0) rows.push(["Stopping first", spell(stopping)]);
      rows.push(["Fuel", `${Math.ceil(wasm.ship_preview_fuel())} of ${wasm.ship_fuel_aboard() - wasm.ship_fuel_reserved()} spare`]);
      rows.push(["Ends", wasm.ship_preview_docks() !== 0 ? "Docked" : "Holding"]);
    } else {
      plan.className = "";
      rows.push(["", "Open the map and click somewhere to plot a trip."]);
    }
    plan.replaceChildren(...rows.map(([label, value]) => {
      const line = document.createElement("div");
      if (label) {
        const b = document.createElement("b");
        b.textContent = label;
        line.appendChild(b);
      } else {
        line.className = "empty";
      }
      const span = document.createElement("span");
      span.textContent = value;
      line.appendChild(span);
      return line;
    }));

    byId("confirm").disabled = aimed === null || state !== 1;
    byId("abort").disabled = wasm.ship_world_state() !== 2;
  }

  /** What the helm is pointed at, and how far off it is.
   *
   * The distance comes from the map list rather than from the plan: a plan is
   * a route, and "how far away is it" is a question about the thing. */
  function describeAim() {
    if (aimed === null) return "Nowhere";
    const here = { x: wasm.ship_world_x(), y: wasm.ship_world_y() };
    if (aimed.node) {
      const i = mapIndexOf(aimed.node.kind, aimed.node.id);
      const away = Math.hypot(wasm.ship_map_x(i) - here.x, wasm.ship_map_y(i) - here.y);
      return `${nodeName(aimed.node.kind, aimed.node.id)} · ${grouped(Math.round(away))} units`;
    }
    const away = Math.hypot(aimed.x - here.x, aimed.y - here.y);
    return `A point in space · ${grouped(Math.round(away))} units`;
  }

  /** Game minutes as something a person can read. Days and hours, because a
   * trip across a system is days and a trip across a dock is minutes. */
  function spell(minutes) {
    const whole_ = Math.round(minutes);
    const days = Math.floor(whole_ / 1440);
    const hours = Math.floor((whole_ % 1440) / 60);
    const mins = whole_ % 60;
    if (days > 0) return `${days}d ${hours}h`;
    if (hours > 0) return `${hours}h ${mins}m`;
    return `${mins}m`;
  }

  function paintSpeeds() {
    const mine = wasm.ship_speed_request(net.slot);
    const effective = wasm.ship_effective_speed();
    for (const { button, code } of speedButtons) {
      // Two marks, and they mean different things: what *you* asked for, and
      // what the world is running at. A player held at 1x by somebody else
      // needs to be able to see that is what happened.
      const marks = [];
      if (code === mine) marks.push("on");
      if (code === effective) marks.push("effective");
      button.className = marks.join(" ");
    }
    const asks = [];
    for (let slot = 0; slot < net.players; slot++) {
      const line = document.createElement("div");
      const who = document.createElement("b");
      who.textContent = slot === net.slot ? `Player ${slot + 1} (you)` : `Player ${slot + 1}`;
      const wants = document.createElement("span");
      const times = wasm.ship_speed_multiplier(wasm.ship_speed_request(slot));
      wants.textContent = times === 0 ? "paused" : `${times}×`;
      line.append(who, wants);
      asks.push(line);
    }
    byId("speed-asks").replaceChildren(...asks);
  }

  /** The station, which is only there while the ship is tied to one. Money
   * works at a dock and nowhere else — see `shipdesign::materials`. */
  function paintGameTrade() {
    const docked = wasm.ship_world_state() === 0;
    const panel = byId("game-trade");
    if (docked) panel.removeAttribute("hidden");
    else panel.setAttribute("hidden", "");
    if (!docked) return;

    const left = remaining();
    for (const cell of gameGoodRows) {
      const aboard = wasm.ship_cargo(cell.id);
      cell.units.textContent = String(aboard);
      const class_ = wasm.ship_storage_of(cell.id);
      const room = wasm.ship_storage_capacity(class_) - wasm.ship_storage_used(class_);
      const price = tradePrice(cell.id);
      // Fuel held against a trip under way is not the crew's to sell.
      const reserved = cell.id === FUEL ? wasm.ship_fuel_reserved() : 0;
      for (const button of cell.row.querySelectorAll("button")) {
        const buy = button.dataset.buy;
        const step = Number(buy ?? button.dataset.sell);
        button.disabled = buy
          ? step * price > left || step > room
          : step > aboard - reserved;
      }
      cell.row.className = aboard > 0 ? "good carried" : "good";
    }
    for (const cell of gameHoldRows) {
      const total = wasm.ship_storage_capacity(cell.class);
      const used = wasm.ship_storage_used(cell.class);
      cell.used.textContent = `${used} / ${total}`;
      cell.row.className = total > 0 && used >= total ? "hold full" : "hold";
    }
  }

  function paintShipFacts() {
    const by = wasm.ship_destination_by();
    const rows = [
      ["Fuel", `${wasm.ship_fuel_aboard()} aboard, ${wasm.ship_fuel_reserved()} held`],
      ["Mass", wasm.ship_mass().toFixed(0)],
      ["Acceleration", wasm.ship_acceleration(0).toFixed(4)],
      ["Parts", String(wasm.ship_part_total())],
      // Which tiles the outside can see into. A number rather than a warning:
      // the design phase already shouted about it, and out here it is a fact
      // about the ship the crew are living on.
      ["Exposed tiles", String(wasm.ship_exposed_count())],
      ["Position", `${grouped(Math.round(wasm.ship_world_x()))}, ${grouped(Math.round(wasm.ship_world_y()))}`],
      ["Found", `${wasm.ship_map_count()} in this system`],
      ["Scanner", `${grouped(Math.round(wasm.ship_detection_range()))} units`],
      ["Route set by", by === 0 ? "Nobody" : `Player ${by}`],
      ["Crew", String(net.players)],
    ];
    byId("ship-facts").replaceChildren(...rows.map(([label, value]) => {
      const line = document.createElement("div");
      const b = document.createElement("b");
      b.textContent = label;
      const span = document.createElement("span");
      span.textContent = value;
      line.append(b, span);
      return line;
    }));
  }

  function paintGameReadout() {
    byId("view-name").textContent = VIEW_NAMES[wasm.ship_view_mode()] ?? "View";
    byId("trip-phase").textContent =
      wasm.ship_world_state() === 2
        ? wasm.ship_trip_aborting() !== 0
          ? "Stopping"
          : (PHASE_NAMES[wasm.ship_trip_phase()] ?? "Under way")
        : wasm.ship_world_state() === 0
          ? "Docked"
          : "Holding";
    byId("velocity").textContent = `${wasm.ship_world_speed().toFixed(1)} u/min`;
    // Degrees, because a heading in radians is a number nobody can steer by.
    const degrees = ((wasm.ship_world_heading() * 180) / Math.PI + 360) % 360;
    // Which tile the pointer is over, turned back through the heading. Only in
    // the ship view, and only when it is actually over the hull — a pointer
    // out in the black is out in the black rather than on the nearest edge.
    const tile =
      wasm.ship_view_mode() === VIEW_SHIP && wasm.ship_game_tile_inside() !== 0
        ? ` · ${wasm.ship_game_tile_x()}, ${wasm.ship_game_tile_y()}`
        : "";
    byId("heading").textContent = `${degrees.toFixed(0)}°${tile}`;
  }

  // --- what just happened --------------------------------------------------

  const logLines = [];

  /** Read the events the last batch of steps threw up, and put them on the
   * screen.
   *
   * An event whose code has no line in EVENT_LINES is **dropped** rather than
   * shown blank — the same rule the diary follows — and the count check in
   * scratchpad/ship-check.mjs is what catches a missing one. */
  function drainEvents() {
    const count = wasm.ship_event_count();
    if (count === 0) return;
    for (let i = 0; i < count; i++) {
      const line = EVENT_LINES[wasm.ship_event_code(i)];
      if (!line) continue;
      logLines.push(line(wasm.ship_event_value(i)));
    }
    wasm.ship_events_clear();
    while (logLines.length > LOG_LINES) logLines.shift();
    byId("log").replaceChildren(...logLines.map((text) => {
      const div = document.createElement("div");
      div.textContent = text;
      return div;
    }));
  }

  // --- flying it ------------------------------------------------------------

  function gameAt(event) {
    const box = gameCanvas.getBoundingClientRect();
    return { x: event.clientX - box.left, y: event.clientY - box.top };
  }

  /** Plot a trip to whatever a click on the map landed on — a thing, or the
   * empty space beside it, which is a perfectly good place to go. */
  function aimAt(x, y) {
    const picked = wasm.ship_map_pick(x, y, MAP_PICK_SLOP);
    if (picked !== 0) {
      const i = picked - 1;
      aimed = { node: { kind: wasm.ship_map_kind(i), id: wasm.ship_map_id(i) } };
      wasm.ship_preview_node(i);
      return;
    }
    const at = { x: wasm.ship_map_point_x(x, y), y: wasm.ship_map_point_y(x, y) };
    aimed = at;
    wasm.ship_preview_point(at.x, at.y);
  }

  gameCanvas.addEventListener("pointerdown", (event) => {
    const p = gameAt(event);
    if (event.button === 1) {
      panning = true;
      panFrom = p;
      gameCanvas.setPointerCapture?.(event.pointerId);
      event.preventDefault?.();
      return;
    }
    if (wasm.ship_view_mode() === VIEW_MAP) aimAt(p.x, p.y);
  });

  gameCanvas.addEventListener("pointermove", (event) => {
    const p = gameAt(event);
    if (panning) {
      wasm.ship_pan(p.x - panFrom.x, p.y - panFrom.y);
      panFrom = p;
      return;
    }
    if (wasm.ship_view_mode() === VIEW_SHIP) wasm.ship_game_hover(p.x, p.y);
  });

  gameCanvas.addEventListener("pointerup", (event) => {
    if (!panning) return;
    panning = false;
    gameCanvas.releasePointerCapture?.(event.pointerId);
  });

  gameCanvas.addEventListener("pointerleave", () => {
    if (!panning) wasm.ship_game_leave();
  });

  gameCanvas.addEventListener("wheel", (event) => {
    event.preventDefault?.();
    const p = gameAt(event);
    wasm.ship_zoom(p.x, p.y, Math.exp(-event.deltaY * ZOOM_PER_PIXEL));
  });

  byId("confirm").addEventListener("click", () => {
    if (aimed === null) return;
    net.fly(aimed);
  });

  byId("abort").addEventListener("click", () => {
    net.stop();
  });

  // --- saying something --------------------------------------------------

  let saidUntil = 0;
  let saying = "";

  /** A refusal, on screen for a moment. Kept off the issue list: that list is
   * about the ship, and this is about the click. An empty string clears it,
   * which is what a drag that went through entirely wants. */
  function remark(text) {
    saying = text;
    said.textContent = text;
    saidUntil = performance.now() + SAID_SECONDS * 1000;
  }

  function ageRemark(now) {
    if (saying && now > saidUntil) {
      saying = "";
      said.textContent = "";
    }
  }

  // --- the pointer -------------------------------------------------------
  //
  // Left drags place, right drags clear, middle drags pan. The geometry of a
  // drag is worked out in wasm — which tiles a rectangle covers, which parts a
  // clearing drag would take off and in what order — and what comes back here
  // is a list of Edits, each of which goes out through `net` on its own.
  // There is deliberately no bulk operation for a transport to have to learn.

  let panning = false;
  let panFrom = { x: 0, y: 0 };

  function at(event) {
    const box = canvas.getBoundingClientRect();
    return { x: event.clientX - box.left, y: event.clientY - box.top };
  }

  canvas.addEventListener("pointerdown", (event) => {
    const p = at(event);
    if (event.button === 1) {
      panning = true;
      panFrom = p;
      canvas.setPointerCapture?.(event.pointerId);
      event.preventDefault?.();
      return;
    }
    if (!stillDesigning()) return;
    canvas.setPointerCapture?.(event.pointerId);
    wasm.ship_drag_begin(p.x, p.y, event.button === 2 ? 1 : 0);
  });

  canvas.addEventListener("pointermove", (event) => {
    const p = at(event);
    if (panning) {
      wasm.ship_pan(p.x - panFrom.x, p.y - panFrom.y);
      panFrom = p;
      return;
    }
    if (wasm.ship_dragging() !== 0) wasm.ship_drag_update(p.x, p.y);
    else wasm.ship_hover(p.x, p.y);
  });

  canvas.addEventListener("pointerup", (event) => {
    if (panning) {
      panning = false;
      canvas.releasePointerCapture?.(event.pointerId);
      return;
    }
    if (wasm.ship_dragging() === 0) return;
    canvas.releasePointerCapture?.(event.pointerId);
    commitDrag();
  });

  canvas.addEventListener("pointerleave", () => {
    if (wasm.ship_dragging() === 0 && !panning) wasm.ship_leave();
  });

  canvas.addEventListener("pointercancel", () => {
    panning = false;
    wasm.ship_drag_cancel();
  });

  // A right-drag is a tool, so the browser's own menu has to stay out of it.
  canvas.addEventListener("contextmenu", (event) => event.preventDefault());

  canvas.addEventListener("wheel", (event) => {
    event.preventDefault?.();
    const p = at(event);
    // The factor is worked out here rather than in wasm: an exponential is a
    // lot of binary to carry for the sake of a scroll wheel.
    const factor = Math.exp(-event.deltaY * ZOOM_PER_PIXEL);
    wasm.ship_zoom(p.x, p.y, factor);
  });

  /** Turn the drag under the pointer into a run of Edits and post them.
   *
   * Everything is read out of wasm **before** the first Edit is applied: the
   * parts a clearing drag names are looked up in the design it was drawn
   * over, and applying as you go would have the list shifting under itself.
   *
   * A failing Edit is skipped and counted, never fatal. A rectangle of deck
   * over a half-floored room is *meant* to fill the gaps and pass over the
   * rest. */
  function commitDrag() {
    const removing = wasm.ship_drag_removing() !== 0;
    const edits = [];
    if (removing) {
      const count = wasm.ship_drag_part_count();
      for (let i = 0; i < count; i++) edits.push({ remove: wasm.ship_drag_part(i) });
    } else {
      const kind = wasm.ship_tool();
      const rotation = wasm.ship_ghost_rotation();
      const count = wasm.ship_drag_tile_count();
      for (let i = 0; i < count; i++) {
        edits.push({
          kind,
          rotation,
          x: wasm.ship_drag_tile_x(i),
          y: wasm.ship_drag_tile_y(i),
        });
      }
    }
    wasm.ship_drag_cancel();

    let skipped = 0;
    // The **first** refusal, not the last. A removing drag goes from the top
    // of the stack down, so the first thing to refuse is the thing the
    // player was pointing at; everything under it then refuses as well,
    // because it is holding that up. Reporting the last one would answer a
    // question nobody asked — "take what is standing on it off first" about
    // the frame, when what actually said no was the shelf with a hundred
    // units of ore in it.
    let why = 0;
    for (const edit of edits) {
      const done =
        edit.remove !== undefined
          ? net.remove(edit.remove)
          : net.place(edit.kind, edit.x, edit.y, edit.rotation);
      if (!done.ok) {
        skipped++;
        if (why === 0) why = done.why;
      }
    }

    if (skipped === 0) {
      remark("");
    } else if (edits.length === 1) {
      remark(EDIT_LINES[why] ?? "That could not be done.");
    } else {
      remark(`${skipped} of ${edits.length} skipped — ${EDIT_LINES[why] ?? "refused"}`);
    }
    afterChange();
  }

  // --- the keyboard ------------------------------------------------------

  const held = new Set();

  window.addEventListener("keydown", (event) => {
    const key = String(event.key ?? "").toLowerCase();
    if (key === "m" && playing()) {
      wasm.ship_set_view_mode(wasm.ship_view_mode() === VIEW_MAP ? VIEW_SHIP : VIEW_MAP);
      return;
    }
    if (key === "r") {
      wasm.ship_rotate_ghost();
      paintPalette();
      return;
    }
    if (key === "escape") {
      wasm.ship_drag_cancel();
      // Stop aiming. The quote goes with the aim rather than outliving it:
      // `paintHelm` puts one back the moment there is something to quote.
      aimed = null;
      wasm.ship_preview_clear();
      return;
    }
    if ("wasd".includes(key)) held.add(key);
  });

  window.addEventListener("keyup", (event) => {
    held.delete(String(event.key ?? "").toLowerCase());
  });

  // A window that loses focus keeps no keys down, or the view slides away on
  // its own while you are somewhere else.
  window.addEventListener("blur", () => {
    held.clear();
    panning = false;
    wasm.ship_drag_cancel();
    wasm.ship_clear_focus();
  });

  function pumpKeys(dt) {
    if (held.size === 0) return;
    const step = PAN_SPEED * dt;
    let dx = 0;
    let dy = 0;
    if (held.has("a")) dx += step;
    if (held.has("d")) dx -= step;
    if (held.has("w")) dy += step;
    if (held.has("s")) dy -= step;
    if (dx !== 0 || dy !== 0) wasm.ship_pan(dx, dy);
  }

  // --- the readout -------------------------------------------------------

  const readout = byId("readout");
  const readoutWhat = byId("readout-what");
  const readoutPrice = byId("readout-price");
  const readoutTile = byId("readout-tile");

  /** What is under the pointer and what the tool would cost. Repainted every
   * frame because the pointer moves every frame; everything else on the page
   * is repainted only when the ship changes. */
  function paintReadout() {
    const hovered = wasm.ship_hovered_part();
    const tool = wasm.ship_tool();
    readoutWhat.textContent = hovered
      ? (PART_NAMES[wasm.ship_part_kind(hovered)] ?? "Something")
      : (PART_NAMES[tool] ?? "Something");

    readoutPrice.textContent = euros(partPrice(tool));
    readoutTile.textContent =
      wasm.ship_hover_inside() !== 0 ? `${wasm.ship_hover_x()}, ${wasm.ship_hover_y()}` : "";

    // Said before the click rather than after it. The ghost is already red on
    // the deck; this is the same fact where the pointer is looking, so a
    // refusal is never the first the player hears of it.
    readout.className = wasm.ship_ghost_ok() !== 0 ? "" : "refused";
  }

  // --- painting the grid --------------------------------------------------

  function paint() {
    // Re-read the view every frame: growing the wasm heap detaches the old one.
    const shapes = new Float32Array(
      wasm.memory.buffer,
      wasm.ship_draw_ptr(),
      wasm.ship_draw_len(),
    );

    // Whichever canvas is up. One replay loop for both halves of the page,
    // because the draw format is the same and so is the transform — what
    // differs is what the origin is, and that is worked out in wasm.
    const ctx = stageCtx;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, canvasSize.w, canvasSize.h);

    const s = wasm.ship_view_scale();
    ctx.setTransform(dpr * s, 0, 0, dpr * s, dpr * wasm.ship_view_x(), dpr * wasm.ship_view_y());

    for (let i = 0; i < shapes.length; i += stride) {
      const kind = shapes[i];
      const x = shapes[i + 1];
      const y = shapes[i + 2];
      const w = shapes[i + 3];
      const h = shapes[i + 4];
      const rot = shapes[i + 5];
      const radius = shapes[i + 6];
      const line = shapes[i + 7];
      const r = Math.round(shapes[i + 8] * 255);
      const g = Math.round(shapes[i + 9] * 255);
      const b = Math.round(shapes[i + 10] * 255);
      const a = shapes[i + 11];

      const style = `rgba(${r},${g},${b},${a})`;
      if (line > 0) {
        ctx.strokeStyle = style;
        // A hairline at this scale would vanish; the widths are in world
        // units and the transform is already scaling them.
        ctx.lineWidth = line;
      } else {
        ctx.fillStyle = style;
      }

      if (kind === KIND_ELLIPSE) {
        ctx.beginPath();
        ctx.ellipse(x, y, Math.abs(w) / 2, Math.abs(h) / 2, rot, 0, Math.PI * 2);
        if (line > 0) ctx.stroke();
        else ctx.fill();
        continue;
      }

      if (rot === 0 && radius === 0 && line === 0) {
        ctx.fillRect(x - w / 2, y - h / 2, w, h);
        continue;
      }
      ctx.save();
      ctx.translate(x, y);
      ctx.rotate(rot);
      ctx.beginPath();
      if (radius > 0 && ctx.roundRect) {
        ctx.roundRect(-w / 2, -h / 2, w, h, Math.min(radius, Math.abs(w) / 2, Math.abs(h) / 2));
      } else {
        ctx.rect(-w / 2, -h / 2, w, h);
      }
      if (line > 0) ctx.stroke();
      else ctx.fill();
      ctx.restore();
    }
  }

  // --- putting it together ------------------------------------------------

  /** Everything that depends on the ship rather than on the pointer. Called
   * after an edit or an Accept and at no other time — validation walks the
   * whole grid and is not something to do sixty times a second. */
  function afterChange() {
    paintPalette();
    paintMoney();
    paintTrade();
    paintIssues();
    paintCrew();
    paintAccept();
    // The last Accept settles the ship *and* opens the world — one event, and
    // `ship_accept` is where both halves of it happen.
    if (!stillDesigning()) startGame();
  }

  net.on("rejected", (result) => {
    // A single refused Edit inside a drag is reported by `commitDrag`, which
    // knows how many there were. This is for anything else that fails.
    if (result.why && !saying) remark(EDIT_LINES[result.why] ?? "That could not be done.");
  });

  byId("area").textContent = `${chosen.area} × ${chosen.area} tiles`;
  buildPalette();
  buildMoney();
  buildTrade();
  show("design");
  afterChange();

  let last = performance.now();
  /** Steps owed to the world, carried between frames.
   *
   * The world advances in fixed steps and never in stretched ones, exactly as
   * the room does: a 24x step would move the ship several times its own length
   * and a trip would skip straight past its own braking phase. Speed is more
   * steps, never bigger ones. */
  let backlog = 0;

  function frame(now) {
    const dt = Math.min((now - last) / 1000, MAX_FRAME_DT);
    last = now;

    if (playing()) {
      startGame();
      runWorld(dt);
    } else {
      pumpKeys(dt);
      ageRemark(now);
    }

    wasm.ship_render();
    paint();
    if (playing()) paintGame();
    else paintReadout();
    requestAnimationFrame(frame);
  }

  /** Turn `dt` seconds of real time into world steps, and take them.
   *
   * Capped, because a tab that was backgrounded for a minute would otherwise
   * try to catch up in one frame and lock the page. The cap has to be at least
   * `TOP_SPEED * 60 / 30` or the top of the range stops being reachable — see
   * MAX_STEPS_PER_FRAME above. */
  function runWorld(dt) {
    pumpKeys(dt);
    const times = wasm.ship_speed_multiplier(wasm.ship_effective_speed());
    backlog += dt * wasm.ship_steps_per_second() * times;
    let steps = 0;
    while (backlog >= 1 && steps < MAX_STEPS_PER_FRAME) {
      wasm.ship_world_step();
      backlog -= 1;
      steps += 1;
    }
    if (backlog > MAX_STEPS_PER_FRAME) backlog = 0; // gave up catching up
    drainEvents();
  }

  requestAnimationFrame(frame);
}
