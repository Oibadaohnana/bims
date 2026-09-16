// The front of the game: the start menu, the setup screen, and the lobby.
//
// This is the page `nix run .` opens on. It is deliberately separate
// from web/bims.js — that file is the room, and knows nothing about menus —
// and from web/ship.js, the designer. It has a wasm of its own now:
// `lobby.wasm`, out of `crates/lobby`, which is the World tab's galaxy —
// nothing else on the page touches it, and the page runs without it, saying
// so on that tab, if it fails to load.
//
// The multiplayer half is surface only. Nothing here opens a socket: `net`
// below is a local stand-in with the shape a transport will have, and it is
// the one seam to replace. Every part of the UI that will eventually be told
// something by the network is already told it by `net` instead, so wiring one
// up means implementing `net` and not touching the screens.

/** What the settings come out as, and the only things that leave this page:
 * plain numbers. The simulation takes numbers across the wasm boundary and no
 * strings at all, and starting as we mean to go on keeps the boundary honest
 * when the builder actually starts a game.
 *
 * Money is a **whole number of euros**. Nobody starts with stores any more:
 * each of the crew brings this much, it all goes into one pool, and the
 * designer spends that pool. The euro sign and the digit grouping below are
 * the host's business — `euros()` is the only place either exists, and what
 * crosses in the query string is the bare number. */
const MONEY = [
  { id: "lean", label: "Lean", amount: 50000, sub: euros(50000) },
  { id: "standard", label: "Standard", amount: 100000, sub: euros(100000) },
  { id: "full", label: "Full", amount: 200000, sub: euros(200000) },
];

/** Starting ship, in tiles a side. */
const SHIPS = [
  { id: "small", label: "Small", tiles: 30, sub: "30 × 30" },
  { id: "standard", label: "Standard", tiles: 40, sub: "40 × 40" },
  { id: "large", label: "Large", tiles: 60, sub: "60 × 60" },
];

/** The shape of the galaxy, by `worldgen::GalaxyType`. The discriminant is
 * what crosses into wasm and what goes in the query string; the id is for the
 * button. */
const GALAXIES = [
  { id: "two-arm", label: "Two-arm spiral", type: 0, sub: "Two arms, easy to read" },
  { id: "spiral", label: "Spiral", type: 1, sub: "Four arms, busier" },
  { id: "elliptical", label: "Elliptical", type: 2, sub: "A squashed cloud" },
  { id: "round", label: "Round", type: 3, sub: "Dense in the middle" },
];

/** The defaults a game starts from if nobody touches anything. */
const DEFAULT_MONEY = 100000;
const DEFAULT_SHIP = 40;
const DEFAULT_GALAXY = 0;

/** Berths in a lobby. Four is a guess at the eventual crew ceiling; it is one
 * number here and in no other place, so moving it is moving it. */
const LOBBY_SLOTS = 4;

/** Room codes are read out loud down a phone line, so the alphabet leaves out
 * the pairs that are heard wrong: O/0, I/1. */
const CODE_ALPHABET = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
const CODE_LENGTH = 6;

/** Who you are until there are accounts. The room calls crew 0 James. */
const YOU = "James";

/** How long a passing remark stays on screen. */
const NOTE_SECONDS = 4;

// --- the words for what the world module hands over as numbers -----------
//
// No strings cross the wasm boundary, and a star's name is three numbers out
// of `worldgen::name`: a word, a catalogue number and a further part. These
// tables are where the words are. **Their lengths are the generator's
// `STAR_WORDS` and `STATION_WORDS`**, and a table shorter than the generator
// expects renders a star as `undefined-284` — which reads as the star not
// existing rather than as a table being short. scratchpad/builder-check.mjs
// counts them.

/** Star words: 48, the generator's `STAR_WORDS`. A star is "Word-Number". */
const STAR_WORDS = [
  "Tanis", "Vesper", "Halden", "Orrin", "Cassel", "Marrow", "Ilex", "Sorrel",
  "Brannoc", "Kestrel", "Ashby", "Corvane", "Dunmere", "Ferris", "Galt", "Harrow",
  "Isolde", "Jarrah", "Kell", "Lorne", "Maund", "Nerys", "Ostrel", "Perrin",
  "Quill", "Rath", "Selk", "Tarn", "Ulric", "Varn", "Wendel", "Yarrow",
  "Ambrel", "Bright", "Calder", "Dray", "Elm", "Fenwick", "Gorse", "Hollin",
  "Ivory", "Juniper", "Kirsch", "Lund", "Mossley", "Nook", "Orme", "Pell",
];

/** Station words: 32, the generator's `STATION_WORDS`. A station is
 * "Word Number", with its mark after if it has one. */
const STATION_WORDS = [
  "Cordell Yard", "Meridian Dock", "Hask Platform", "Verity Ring",
  "Stannard Halt", "Lowry Anchorage", "Pike Terminal", "Dawes Berth",
  "Kite Reach", "Fallow Point", "Greave Works", "Ashlar Hub",
  "Tolley Landing", "Wren Gantry", "Copper Cross", "Sable Moor",
  "Harkin Depot", "Ember Station", "Ninefold Yard", "Quarry Spur",
  "Rook Haven", "Tamsin Dock", "Ullage Post", "Vane Outlook",
  "Wick Refuge", "Yeoman Reach", "Zephyr Hold", "Brindle Wharf",
  "Carrow Deep", "Dolan Rest", "Eyrie Platform", "Fisk Terminus",
];

/** `worldgen::StarClass`, hottest first. Said after a star's name so a
 * player learns that a G is like home. */
const STAR_CLASS_NAMES = ["O", "B", "A", "F", "G", "K", "M"];

/** What each kind of body is called, by `worldgen::BodyKind`. */
const BODY_KIND_NAMES = ["Rocky planet", "Gas giant", "Ice world", "Asteroid belt"];

/** And each kind of station, by `worldgen::StationKind`. */
const STATION_KIND_NAMES = ["Orbital", "Refinery", "Mining outpost", "Derelict", "Relay"];

/** A seed is a `u64`, and a `u64` crosses the boundary in two `u32` halves.
 * The field takes the whole thing as decimal, so this is the biggest thing it
 * will accept. */
const SEED_MAX = 2n ** 64n - 1n;
const U32 = 2n ** 32n;

/** How much one notch of wheel is worth on the preview. Applied by the host
 * rather than in wasm so no exponential has to be linked in for a scroll. */
const ZOOM_PER_PIXEL = 0.0016;

/** A press that moves less than this before it lets go is a click on a star,
 * not a drag of the map. */
const DRAG_SLOP = 4;

/** Longest a frame may pretend to be, so a backgrounded tab does not come
 * back and finish every ping at once. */
const MAX_FRAME_DT = 0.1;

/** Floats per shape, if the wasm somehow will not say. */
const STRIDE_FALLBACK = 12;

const KIND_RECT = 0;
const KIND_ELLIPSE = 1;

/** A number of euros, as words. The **only** place either the sign or the
 * grouping exists: everything that leaves this page — the query string, and
 * one day the wasm boundary — is a bare count of euros. */
function euros(amount) {
  const grouped = String(amount).replace(/\B(?=(\d{3})+(?!\d))/g, "\u00a0");
  return `€${grouped}`;
}

/** A small whole number as a roman numeral — which is how a body's ordinal
 * is read: the third planet out from Tanis-284 is Tanis-284 III. */
function roman(n) {
  const digits = [
    [10, "X"],
    [9, "IX"],
    [5, "V"],
    [4, "IV"],
    [1, "I"],
  ];
  let left = n;
  let out = "";
  for (const [value, glyph] of digits) {
    while (left >= value) {
      out += glyph;
      left -= value;
    }
  }
  return out;
}

boot();

function boot() {
  const byId = (id) => document.getElementById(id);

  // --- what a game would be started with ------------------------------
  //
  // One object, shared by every settings tool on the page. A tool writes into
  // it and re-reads it when it is shown, so the solo screen and the lobby
  // cannot disagree about what was picked.
  //
  // `spawnStar` and `spawnStation` are **absent** until a station has been
  // picked, rather than present as a null: a game cannot start without them,
  // and their absence is what disables Start.
  const seed = randomSeed();
  const settings = {
    moneyPerBim: DEFAULT_MONEY,
    ship: DEFAULT_SHIP,
    seedHi: seed.hi,
    seedLo: seed.lo,
    galaxy: DEFAULT_GALAXY,
  };

  // --- the network that is not there yet -------------------------------
  //
  // Same shape a real one will have: you ask it for things, and it tells you
  // what happened by firing an event. Nothing in the screens below reaches
  // past it, so the day this grows a socket the screens do not change.
  const net = {
    code: null,
    host: false,
    players: [],
    handlers: new Map(),

    on(event, fn) {
      if (!net.handlers.has(event)) net.handlers.set(event, []);
      net.handlers.get(event).push(fn);
    },

    emit(event, payload) {
      for (const fn of net.handlers.get(event) ?? []) fn(payload);
    },

    /** The inbound half: what a transport calls when a message arrives.
     * Locally it is `emit` under another name, and the name is the point —
     * a `room` delivered here is a lobby somebody else opened, and the
     * page reacts to it exactly as it will when one really is. */
    deliver(event, payload) {
      if (event === "room") {
        net.code = payload.code ?? null;
        net.host = Boolean(payload.host);
      }
      if (event === "players") net.players = payload;
      net.emit(event, payload);
    },

    /** Open a lobby. Locally this only mints a code and seats you; with a
     * transport behind it, it is the call that creates the room. */
    create() {
      net.code = makeCode();
      net.host = true;
      net.players = [{ name: YOU, host: true, you: true }];
      net.emit("room", { code: net.code, host: net.host });
      net.emit("players", net.players);
      return { ok: true, code: net.code };
    },

    /** Walk into somebody else's lobby. There is nothing to walk into yet,
     * and saying so is better than pretending: the field, the button and the
     * refusal are all real, only the wire is missing. */
    join(code) {
      if (!isCode(code)) return { ok: false, why: "That is not a room code." };
      return { ok: false, why: `No way to reach ${code} yet — nothing is listening.` };
    },

    /** The host changed a setting. With a transport this is the broadcast;
     * without one it is where the call already sits, so the tool is written
     * as if the other end were listening. */
    push(what) {
      if (!net.host) return;
      net.emit("settings", what);
    },

    /** A guest cannot set the start, but can point at one. With a transport
     * this goes to the host; without one it comes straight back as the
     * event the host would see. */
    suggest(star, station) {
      net.emit("suggestion", { from: YOU, star, station });
    },

    leave() {
      net.code = null;
      net.host = false;
      net.players = [];
      net.emit("players", net.players);
    },
  };

  // The one way in from outside this closure, for a transport script — or a
  // harness standing in for one — to hand the page what came down the wire.
  if (typeof window !== "undefined") window.bimsDeliver = net.deliver;

  // --- screens ---------------------------------------------------------

  const screens = new Map();
  for (const el of document.querySelectorAll("[data-screen]")) {
    screens.set(el.dataset.screen, el);
  }

  let showing = "menu";

  function show(name) {
    for (const [id, el] of screens) {
      if (id === name) el.removeAttribute("hidden");
      else el.setAttribute("hidden", "");
    }
    showing = name;
    if (name === "setup") setupTool.sync();
    if (name === "lobby") lobbyTool.sync();
  }

  // --- the world -------------------------------------------------------
  //
  // Built **once** and moved into whichever tool is on screen, rather than
  // built into each tool the way the choice rows are. There is one galaxy in
  // wasm, one camera over it and one canvas to paint it on, and two copies
  // of the panel would be two views of one camera that could not both be
  // right. `makeWorld` returns the panel and the three things a tool needs
  // from it.

  const world = makeWorld();

  function makeWorld() {
    const el = document.createElement("div");
    el.className = "world";

    // The header names the start, or says there is none yet.
    const head = document.createElement("div");
    head.className = "world-head";
    const headName = document.createElement("span");
    headName.className = "name";
    headName.textContent = "Start at";
    const spawnSaid = document.createElement("span");
    spawnSaid.className = "spawn";
    spawnSaid.id = "spawn-said";
    head.append(headName, spawnSaid);

    // The seed: a decimal field for a u64.
    const seedField = document.createElement("div");
    seedField.className = "field";
    seedField.dataset.field = "seed";
    const seedWhat = document.createElement("div");
    seedWhat.className = "what";
    const seedName = document.createElement("span");
    seedName.className = "name";
    seedName.textContent = "Seed";
    const seedNote = document.createElement("span");
    seedNote.className = "note";
    seedNote.textContent = "A whole number; the same one is the same galaxy";
    seedWhat.append(seedName, seedNote);
    const seedForm = document.createElement("form");
    seedForm.className = "seed-row";
    seedForm.id = "seed-form";
    const seedInput = document.createElement("input");
    seedInput.type = "text";
    seedInput.id = "seed";
    seedInput.setAttribute("inputmode", "numeric");
    seedInput.setAttribute("autocomplete", "off");
    seedInput.setAttribute("spellcheck", "false");
    seedInput.setAttribute("aria-label", "Galaxy seed");
    const seedApply = document.createElement("button");
    seedApply.type = "submit";
    seedApply.id = "seed-apply";
    seedApply.className = "small";
    seedApply.textContent = "Set";
    const newSeed = document.createElement("button");
    newSeed.type = "button";
    newSeed.id = "new-seed";
    newSeed.className = "small";
    newSeed.textContent = "New seed";
    seedForm.append(seedInput, seedApply, newSeed);
    seedField.append(seedWhat, seedForm);

    // The galaxy type: a row of options, the same shape as the money row.
    const galaxyField = document.createElement("div");
    galaxyField.className = "field";
    galaxyField.dataset.field = "galaxy";
    const galaxyWhat = document.createElement("div");
    galaxyWhat.className = "what";
    const galaxyName = document.createElement("span");
    galaxyName.className = "name";
    galaxyName.textContent = "Galaxy";
    const galaxyNote = document.createElement("span");
    galaxyNote.className = "note";
    galaxyNote.textContent = "Its shape; a thousand stars either way";
    galaxyWhat.append(galaxyName, galaxyNote);
    const galaxyRow = document.createElement("div");
    galaxyRow.className = "options";
    const galaxyButtons = [];
    for (const option of GALAXIES) {
      const button = document.createElement("button");
      button.type = "button";
      button.className = "option";
      button.dataset.option = option.id;
      const label = document.createElement("span");
      label.textContent = option.label;
      const sub = document.createElement("span");
      sub.className = "sub";
      sub.textContent = option.sub;
      button.append(label, sub);
      button.addEventListener("click", () => {
        if (button.disabled) return;
        chooseGalaxy(option.type);
      });
      galaxyRow.appendChild(button);
      galaxyButtons.push({ button, option });
    }
    galaxyField.append(galaxyWhat, galaxyRow);

    // What the module has to say for itself: loading, or why not.
    const said = document.createElement("p");
    said.className = "muted";
    said.id = "world-said";
    said.textContent = "Loading the galaxy…";

    // The preview on the left, the system on the right.
    const map = document.createElement("div");
    map.className = "world-map";
    const previewBox = document.createElement("div");
    previewBox.className = "preview-box";
    const canvas = document.createElement("canvas");
    canvas.id = "galaxy";
    canvas.setAttribute("aria-label", "The galaxy");
    const previewFoot = document.createElement("div");
    previewFoot.className = "preview-foot";
    const hoverSaid = document.createElement("span");
    hoverSaid.id = "hover-said";
    const randomStart = document.createElement("button");
    randomStart.type = "button";
    randomStart.id = "random-start";
    randomStart.className = "small";
    randomStart.textContent = "Random start";
    previewFoot.append(hoverSaid, randomStart);
    const worldNote = document.createElement("p");
    worldNote.id = "world-note";
    worldNote.className = "world-note";
    previewBox.append(canvas, previewFoot, worldNote);

    const system = document.createElement("div");
    system.className = "card system";
    system.id = "system";
    const systemName = document.createElement("h3");
    systemName.id = "system-name";
    const systemCanvas = document.createElement("canvas");
    systemCanvas.id = "system-canvas";
    systemCanvas.setAttribute("aria-label", "The system");
    const bodies = document.createElement("ul");
    bodies.id = "bodies";
    const stations = document.createElement("ul");
    stations.id = "stations";
    system.append(systemName, systemCanvas, bodies, stations);
    map.append(previewBox, system);

    el.append(head, seedField, galaxyField, said, map);

    let editable = true;
    let wasm = null;
    let stride = STRIDE_FALLBACK;
    let NONE = 4294967295;
    let NO_NUMBER = 65535;
    let dirty = true;
    let systemDirty = true;
    let inspected = null;
    let mounted = null;

    /** Put the panel in `host`, taking it out of wherever it was. */
    function mount(host) {
      if (mounted === host) return;
      host.appendChild(el);
      mounted = host;
      dirty = true;
      systemDirty = true;
    }

    // --- names ------------------------------------------------------

    const starName = (star) =>
      `${STAR_WORDS[wasm.lobby_star_name(star, 0)]}-${wasm.lobby_star_name(star, 1)}`;

    const bodyName = (star, body) => `${starName(star)} ${roman(wasm.lobby_body_name(body, 2))}`;

    function stationName(station) {
      const word = STATION_WORDS[wasm.lobby_station_name(station, 0)];
      const number = wasm.lobby_station_name(station, 1);
      const mark = wasm.lobby_station_name(station, 2);
      return mark === NO_NUMBER ? `${word} ${number}` : `${word} ${number} Mk ${mark}`;
    }

    /** What a station at a star is called, "Cordell Yard 7 at Tanis-284".
     * Reads the inspected system if it is the right one and opens the other
     * for a moment otherwise — the name is the wasm's to give, and the page
     * keeps no copy of any. */
    function placeName(star, station) {
      if (!wasm) return null;
      if (wasm.lobby_inspected() === star) {
        return `${stationName(station)} at ${starName(star)}`;
      }
      wasm.lobby_inspect(star);
      const name = `${stationName(station)} at ${starName(star)}`;
      wasm.lobby_inspect(inspected ?? NONE);
      return name;
    }

    /** What the pending start is called, or nothing if there is none. */
    function spawnName() {
      if (settings.spawnStar === undefined) return null;
      return placeName(settings.spawnStar, settings.spawnStation);
    }

    // --- what the panel shows --------------------------------------

    /** Paint `settings` onto the controls. */
    function sync() {
      seedInput.value = seedText(settings.seedHi, settings.seedLo);
      for (const { button, option } of galaxyButtons) {
        button.className = option.type === settings.galaxy ? "option on" : "option";
        button.disabled = !editable;
      }
      seedInput.disabled = !editable;
      seedApply.disabled = !editable;
      newSeed.disabled = !editable;
      randomStart.disabled = !editable || !wasm;
      const name = spawnName();
      spawnSaid.textContent = name ?? "nowhere yet — pick a station";
      spawnSaid.className = name ? "spawn" : "spawn none";
      paintStations();
      refreshStart();
    }

    function setEditable(can) {
      editable = can;
      sync();
    }

    /** The module is not coming. Said on the tab, and Start says why. */
    function failed(why) {
      wasm = null;
      said.textContent = why;
      said.className = "warn";
      sync();
    }

    // --- choosing ---------------------------------------------------

    /** The seed as it is typed: the whole u64 in decimal. */
    function seedText(hi, lo) {
      return (BigInt(hi) * U32 + BigInt(lo)).toString();
    }

    /** What was typed, as two halves — or null if it is not a seed. Every
     * digit and nothing else, and no bigger than a u64 holds. */
    function parseSeed(text) {
      const trimmed = text.trim().replace(/[\s\u00a0_]/g, "");
      if (!/^\d+$/.test(trimmed)) return null;
      const value = BigInt(trimmed);
      if (value > SEED_MAX) return null;
      return { hi: Number(value / U32), lo: Number(value % U32) };
    }

    function chooseSeed(hi, lo) {
      if (hi === settings.seedHi && lo === settings.seedLo) return;
      settings.seedHi = hi;
      settings.seedLo = lo;
      worldChanged();
      net.push({ seedHi: hi, seedLo: lo, spawnStar: null, spawnStation: null });
    }

    function chooseGalaxy(type) {
      if (type === settings.galaxy) return;
      settings.galaxy = type;
      worldChanged();
      net.push({ galaxy: type, spawnStar: null, spawnStation: null });
    }

    /** The seed or the type moved: a different galaxy, and nothing chosen in
     * the old one means anything in it. */
    function worldChanged() {
      delete settings.spawnStar;
      delete settings.spawnStation;
      inspected = null;
      if (wasm) {
        wasm.lobby_set_world(settings.seedHi, settings.seedLo, settings.galaxy);
        wasm.lobby_clear_spawn();
      }
      dirty = true;
      paintSystem();
      setupTool.sync();
      lobbyTool.sync();
      refreshStart();
    }

    function selectStation(star, station) {
      settings.spawnStar = star;
      settings.spawnStation = station;
      if (wasm) wasm.lobby_set_spawn(star, station);
      dirty = true;
      systemDirty = true;
      sync();
      refreshStart();
      net.push({ spawnStar: star, spawnStation: station });
    }

    /** A random station among every star that has one. Drawn on the host,
     * because wasm has no dice of its own and does not want any. */
    function pickRandomStart() {
      if (!wasm) return;
      const withStation = [];
      const count = wasm.lobby_star_count();
      for (let star = 0; star < count; star++) {
        if (wasm.lobby_star_has_station(star)) withStation.push(star);
      }
      if (withStation.length === 0) return;
      const star = withStation[Math.floor(Math.random() * withStation.length)];
      inspect(star);
      const station = Math.floor(Math.random() * wasm.lobby_station_count());
      selectStation(star, station);
    }

    /** What is arriving from the host, for a guest. Applied without pushing
     * anything back, and only what came: a spawn of null means "cleared". */
    function applyFromHost(what) {
      let moved = false;
      for (const key of ["seedHi", "seedLo", "galaxy"]) {
        if (typeof what[key] === "number" && what[key] !== settings[key]) {
          settings[key] = what[key];
          moved = true;
        }
      }
      if (moved) worldChanged();
      if ("spawnStar" in what) {
        if (what.spawnStar === null) {
          delete settings.spawnStar;
          delete settings.spawnStation;
          if (wasm) wasm.lobby_clear_spawn();
        } else {
          settings.spawnStar = what.spawnStar;
          settings.spawnStation = what.spawnStation;
          if (wasm) wasm.lobby_set_spawn(what.spawnStar, what.spawnStation);
        }
        dirty = true;
        systemDirty = true;
      }
      setupTool.sync();
      lobbyTool.sync();
    }

    /** What was typed becomes the seed, or is refused out loud and put back.
     * Reached by Set, by Enter, and by the field losing focus with something
     * new in it. */
    function applySeed() {
      const parsed = seedApply.disabled ? null : parseSeed(seedInput.value);
      if (seedApply.disabled) {
        // Read-only: whatever got into the field goes back to what it was.
        // The browser will not let a guest type here; the guard is for
        // anything else that can — see the option buttons.
        seedInput.value = seedText(settings.seedHi, settings.seedLo);
        return;
      }
      if (parsed === null) {
        remark(worldNote, "That is not a seed: a whole number, up to twenty digits.", true);
        seedInput.value = seedText(settings.seedHi, settings.seedLo);
        return;
      }
      chooseSeed(parsed.hi, parsed.lo);
    }

    seedForm.addEventListener("submit", (event) => {
      event.preventDefault?.();
      applySeed();
    });
    seedInput.addEventListener("change", applySeed);

    newSeed.addEventListener("click", () => {
      if (newSeed.disabled) return;
      const drawn = randomSeed();
      chooseSeed(drawn.hi, drawn.lo);
    });

    randomStart.addEventListener("click", () => {
      if (randomStart.disabled) return;
      pickRandomStart();
    });

    // --- the side panel ---------------------------------------------

    function inspect(star) {
      if (!wasm) return;
      wasm.lobby_inspect(star);
      inspected = wasm.lobby_inspected() === NONE ? null : star;
      paintSystem();
    }

    /** Everything in the side panel that depends on which star is open. */
    function paintSystem() {
      systemDirty = true;
      if (!wasm || inspected === null) {
        systemName.textContent = "Pick a star";
        bodies.replaceChildren();
        stations.replaceChildren();
        return;
      }
      const star = inspected;
      systemName.textContent = `${starName(star)} · class ${STAR_CLASS_NAMES[wasm.lobby_star_class(star)] ?? "?"}`;

      const bodyRows = [];
      const bodyCount = wasm.lobby_body_count();
      for (let i = 0; i < bodyCount; i++) {
        const row = document.createElement("li");
        row.dataset.body = String(i);
        const name = document.createElement("span");
        name.className = "who";
        name.textContent = bodyName(star, i);
        const kind = document.createElement("span");
        kind.className = "kind";
        kind.textContent = BODY_KIND_NAMES[wasm.lobby_body_kind(i)] ?? "Body";
        row.append(name, kind);
        bodyRows.push(row);
      }
      bodies.replaceChildren(...bodyRows);

      const stationRows = [];
      const stationCount = wasm.lobby_station_count();
      for (let i = 0; i < stationCount; i++) {
        const row = document.createElement("li");
        row.dataset.station = String(i);
        const name = document.createElement("span");
        name.className = "who";
        name.textContent = stationName(i);
        const where = document.createElement("span");
        where.className = "where";
        const parent = wasm.lobby_station_parent(i);
        where.textContent = `${STATION_KIND_NAMES[wasm.lobby_station_kind(i)] ?? "Station"} · ${
          parent === NONE ? "deep space" : bodyName(star, parent)
        }`;
        const button = document.createElement("button");
        button.type = "button";
        button.className = "small";
        button.dataset.pick = String(i);
        button.addEventListener("click", () => {
          if (editable) selectStation(star, i);
          else net.suggest(star, i);
        });
        row.append(name, where, button);
        stationRows.push(row);
      }
      if (stationCount === 0) {
        const row = document.createElement("li");
        row.className = "none";
        row.textContent = "No station here — nowhere to start from.";
        stationRows.push(row);
      }
      stations.replaceChildren(...stationRows);
      paintStations();
    }

    /** The bits of the station rows that change without the star changing:
     * which one is the start, and whether the buttons select or suggest. */
    function paintStations() {
      for (const row of stations.querySelectorAll("[data-station]")) {
        const i = Number(row.dataset.station);
        const on = settings.spawnStar === inspected && settings.spawnStation === i;
        row.className = on ? "on" : "";
        const button = row.querySelector("[data-pick]");
        if (!button) continue;
        button.textContent = editable ? (on ? "Starting here" : "Start here") : "Suggest";
        button.disabled = editable && on;
      }
    }

    // --- the preview ------------------------------------------------

    const ctx = canvas.getContext("2d");
    const systemCtx = systemCanvas.getContext("2d");
    let size = { w: 0, h: 0 };
    let systemSize = { w: 0, h: 0 };
    let dpr = 1;

    /** Match a canvas's pixels to its CSS size. Returns whether it changed. */
    function fit(target, held) {
      dpr = window.devicePixelRatio || 1;
      const w = Math.max(64, target.clientWidth);
      const h = Math.max(64, target.clientHeight);
      if (held.w === w && held.h === h && target.width === Math.round(w * dpr)) return false;
      target.width = Math.round(w * dpr);
      target.height = Math.round(h * dpr);
      held.w = w;
      held.h = h;
      return true;
    }

    /** A point on a canvas in CSS pixels, from a pointer event. */
    function at(target, event) {
      const rect = target.getBoundingClientRect();
      return { x: event.clientX - rect.left, y: event.clientY - rect.top };
    }

    let pressed = null;
    let dragging = false;

    canvas.addEventListener("pointerdown", (event) => {
      if (!wasm) return;
      const p = at(canvas, event);
      pressed = p;
      dragging = event.button === 1;
      canvas.setPointerCapture?.(event.pointerId);
      event.preventDefault?.();
    });

    canvas.addEventListener("pointermove", (event) => {
      if (!wasm) return;
      const p = at(canvas, event);
      if (pressed) {
        const dx = p.x - pressed.x;
        const dy = p.y - pressed.y;
        if (dragging || Math.abs(dx) > DRAG_SLOP || Math.abs(dy) > DRAG_SLOP) {
          dragging = true;
          wasm.lobby_pan(dx, dy);
          pressed = p;
          dirty = true;
          return;
        }
      }
      wasm.lobby_hover(p.x, p.y);
      paintHover();
      dirty = true;
    });

    canvas.addEventListener("pointerup", (event) => {
      if (!wasm) return;
      canvas.releasePointerCapture?.(event.pointerId);
      const wasClick = pressed !== null && !dragging;
      pressed = null;
      dragging = false;
      if (!wasClick) return;
      const p = at(canvas, event);
      wasm.lobby_hover(p.x, p.y);
      const star = wasm.lobby_hovered();
      if (star !== NONE) inspect(star);
      dirty = true;
    });

    canvas.addEventListener("pointerleave", () => {
      if (!wasm) return;
      if (pressed === null) {
        wasm.lobby_leave();
        paintHover();
      }
      dirty = true;
    });

    canvas.addEventListener("wheel", (event) => {
      if (!wasm) return;
      event.preventDefault?.();
      const p = at(canvas, event);
      wasm.lobby_zoom(p.x, p.y, Math.exp(-event.deltaY * ZOOM_PER_PIXEL));
      wasm.lobby_hover(p.x, p.y);
      paintHover();
      dirty = true;
    });

    /** The readout under the preview: what the pointer is over. */
    function paintHover() {
      const star = wasm.lobby_hovered();
      if (star === NONE) {
        hoverSaid.textContent = "";
        return;
      }
      const cls = STAR_CLASS_NAMES[wasm.lobby_star_class(star)] ?? "?";
      const station = wasm.lobby_star_has_station(star) ? "has a station" : "no station";
      hoverSaid.textContent = `${starName(star)} · class ${cls} · ${station}`;
    }

    // --- painting ---------------------------------------------------

    /** Replay the shape buffer onto a canvas. The shapes are already in CSS
     * pixels — the lobby projects them itself — so the only transform is the
     * pixel ratio. */
    function replay(target, held) {
      const shapes = new Float32Array(
        wasm.memory.buffer,
        wasm.lobby_draw_ptr(),
        wasm.lobby_draw_len(),
      );
      target.setTransform(dpr, 0, 0, dpr, 0, 0);
      target.clearRect(0, 0, held.w, held.h);

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
          target.strokeStyle = style;
          target.lineWidth = line;
        } else {
          target.fillStyle = style;
        }

        if (kind === KIND_ELLIPSE) {
          target.beginPath();
          target.ellipse(x, y, Math.abs(w) / 2, Math.abs(h) / 2, rot, 0, Math.PI * 2);
          if (line > 0) target.stroke();
          else target.fill();
          continue;
        }

        if (rot === 0 && radius === 0 && line === 0) {
          target.fillRect(x - w / 2, y - h / 2, w, h);
          continue;
        }
        target.save();
        target.translate(x, y);
        target.rotate(rot);
        target.beginPath();
        if (radius > 0 && target.roundRect) {
          target.roundRect(-w / 2, -h / 2, w, h, Math.min(radius, Math.abs(w) / 2, Math.abs(h) / 2));
        } else {
          target.rect(-w / 2, -h / 2, w, h);
        }
        if (line > 0) target.stroke();
        else target.fill();
        target.restore();
      }
    }

    /** The labels on the system diagram. Text is the host's — the buffer
     * holds rectangles and ellipses and nothing else — so it goes on after
     * the shapes, where the wasm says each thing landed. */
    function labelSystem() {
      if (inspected === null) return;
      systemCtx.font = "11px ui-sans-serif, system-ui, sans-serif";
      systemCtx.textBaseline = "middle";
      systemCtx.fillStyle = "#8fa89a";
      const bodyCount = wasm.lobby_body_count();
      for (let i = 0; i < bodyCount; i++) {
        systemCtx.fillText(
          roman(wasm.lobby_body_name(i, 2)),
          wasm.lobby_body_panel_x(i) + 9,
          wasm.lobby_body_panel_y(i) - 9,
        );
      }
      systemCtx.fillStyle = "#e7efe9";
      const stationCount = wasm.lobby_station_count();
      for (let i = 0; i < stationCount; i++) {
        systemCtx.fillText(
          STATION_KIND_NAMES[wasm.lobby_station_kind(i)] ?? "Station",
          wasm.lobby_station_panel_x(i) + 9,
          wasm.lobby_station_panel_y(i) + 9,
        );
      }
    }

    let last = 0;

    function frame(now) {
      const dt = Math.min((now - last) / 1000, MAX_FRAME_DT);
      last = now;

      if (fit(canvas, size)) {
        wasm.lobby_resize(size.w, size.h);
        dirty = true;
      }
      if (fit(systemCanvas, systemSize)) systemDirty = true;

      if (wasm.lobby_animating()) {
        wasm.lobby_advance(dt);
        dirty = true;
      }
      if (dirty) {
        dirty = false;
        wasm.lobby_render();
        replay(ctx, size);
      }
      if (systemDirty) {
        systemDirty = false;
        wasm.lobby_render_system(systemSize.w, systemSize.h);
        replay(systemCtx, systemSize);
        labelSystem();
      }
      requestAnimationFrame(frame);
    }

    // --- the module -------------------------------------------------

    /** Fetch and start `lobby.wasm`. Plain `instantiate` rather than
     * `instantiateStreaming`: it works no matter how the static server
     * labels .wasm files. The same cache-busting token the page was loaded
     * with, so the wasm always matches the script that fetched it. */
    async function load() {
      const build = window.BIMS_BUILD ?? Date.now();
      const bytes = await fetch(`lobby.wasm?v=${build}`).then((r) => r.arrayBuffer());
      const { instance } = await WebAssembly.instantiate(bytes, {});
      const exports = instance.exports;
      stride = exports.lobby_stride ? exports.lobby_stride() : STRIDE_FALLBACK;
      NONE = exports.lobby_none();
      NO_NUMBER = exports.lobby_no_number();
      fit(canvas, size);
      exports.lobby_init(settings.seedHi, settings.seedLo, settings.galaxy, size.w, size.h);
      wasm = exports;
      said.textContent = "Drag to pan, scroll to zoom, click a star to look at its system. Dim stars have no station.";
      said.className = "muted";
      sync();
      last = performance.now();
      requestAnimationFrame(frame);
    }

    return {
      el,
      mount,
      sync,
      setEditable,
      load,
      failed,
      applyFromHost,
      placeName,
      ping(star) {
        if (wasm) wasm.lobby_ping(star);
        dirty = true;
      },
      /** For the handover: what the galaxy is called and where it starts. */
      describe() {
        const shape = GALAXIES.find((g) => g.type === settings.galaxy);
        return [
          ["World", `${shape ? shape.label : "Custom"}, seed ${seedText(settings.seedHi, settings.seedLo)}`],
          ["Start", spawnName() ?? "not chosen"],
        ];
      },
    };
  }

  // --- the settings tool ------------------------------------------------
  //
  // One tabbed panel, built twice: once on its own for a solo game, once
  // inside the lobby. Both write to the same `settings`, and `sync()` reads
  // it back, so whichever you touched last is what the other shows.

  function makeTool(host, { onChange = () => {} } = {}) {
    const tool = document.createElement("div");
    tool.className = "tool";

    const tabs = document.createElement("div");
    tabs.className = "tool-tabs";
    const body = document.createElement("div");
    body.className = "tool-body";
    tool.append(tabs, body);

    const panels = new Map();
    const buttons = new Map();

    function addTab(id, label, build) {
      const button = document.createElement("button");
      button.type = "button";
      button.className = "tab";
      button.dataset.tab = id;
      button.textContent = label;
      button.addEventListener("click", () => pick(id));
      tabs.appendChild(button);
      buttons.set(id, button);

      const panel = document.createElement("div");
      panel.dataset.tab = id;
      panel.setAttribute("hidden", "");
      build(panel);
      body.appendChild(panel);
      panels.set(id, panel);
    }

    function pick(id) {
      for (const [name, panel] of panels) {
        if (name === id) panel.removeAttribute("hidden");
        else panel.setAttribute("hidden", "");
      }
      for (const [name, button] of buttons) {
        button.className = name === id ? "tab on" : "tab";
      }
    }

    // A row of mutually exclusive choices, each one a number written into
    // `settings` under `key`.
    const rows = [];

    function addChoice(panel, { key, name, note, options, value }) {
      const field = document.createElement("div");
      field.className = "field";
      field.dataset.field = key;

      const what = document.createElement("div");
      what.className = "what";
      const title = document.createElement("span");
      title.className = "name";
      title.textContent = name;
      const hint = document.createElement("span");
      hint.className = "note";
      hint.textContent = note;
      what.append(title, hint);

      const row = document.createElement("div");
      row.className = "options";
      const made = [];
      for (const option of options) {
        const button = document.createElement("button");
        button.type = "button";
        button.className = "option";
        button.dataset.option = option.id;
        const label = document.createElement("span");
        label.textContent = option.label;
        const sub = document.createElement("span");
        sub.className = "sub";
        sub.textContent = option.sub;
        button.append(label, sub);
        button.addEventListener("click", () => {
          // A guest in somebody else's lobby watches the settings rather than
          // setting them, and `setEditable` disables every option to say so.
          // The guard is here as well because `disabled` is the *browser's*
          // half of that rule: anything else that can reach a handler — a
          // harness, a script, a transport replaying a click — has to be
          // refused by the page itself.
          if (button.disabled) return;
          settings[key] = value(option);
          sync();
          onChange(key, settings[key]);
        });
        row.appendChild(button);
        made.push({ button, option });
      }

      field.append(what, row);
      panel.appendChild(field);
      rows.push({ key, made, value });
    }

    addTab("setup", "Game setup", (panel) => {
      addChoice(panel, {
        key: "moneyPerBim",
        name: "Money per Bim",
        note: "What each of you brings; it all goes into one pool",
        options: MONEY,
        value: (o) => o.amount,
      });
      addChoice(panel, {
        key: "ship",
        name: "Ship size",
        note: "Tiles a side",
        options: SHIPS,
        value: (o) => o.tiles,
      });
    });

    // The World tab is the one panel that is shared: `sync()` moves it in.
    let worldHost = null;
    addTab("world", "World", (panel) => {
      worldHost = panel;
    });

    /** Paint the current `settings` onto the buttons. */
    function sync() {
      for (const { key, made, value } of rows) {
        for (const { button, option } of made) {
          const on = value(option) === settings[key];
          button.className = on ? "option on" : "option";
          if (editable) button.disabled = false;
          else button.disabled = true;
        }
      }
      if (worldHost && tool.parentNode && !hiddenAbove(tool)) {
        world.mount(worldHost);
        world.setEditable(editable);
      }
    }

    /** A guest in somebody else's lobby watches the settings rather than
     * setting them. Nothing decides who is a guest yet — the host is always
     * you — but the tool already knows how to be read-only. */
    let editable = true;
    function setEditable(can) {
      editable = can;
      sync();
    }

    host.appendChild(tool);
    pick("setup");
    sync();
    return { el: tool, sync, setEditable, pick };
  }

  /** Whether anything above `el` is hidden — which is how "is this tool the
   * one on screen" is asked, so the shared World panel goes to the tool the
   * player can see rather than to whichever synced last. */
  function hiddenAbove(el) {
    // Up to the body and no further: `document.hidden` is whether the *tab*
    // is in the background, which is a different question.
    for (let node = el; node && node !== document.body; node = node.parentNode) {
      if (node.hidden) return true;
    }
    return false;
  }

  const setupTool = makeTool(byId("setup-tool"));
  const lobbyTool = makeTool(byId("lobby-tool"), {
    onChange: (key, value) => net.push({ [key]: value }),
  });

  // --- the lobby --------------------------------------------------------

  const slots = byId("slots");
  const code = byId("code");

  function renderSlots(players) {
    const rows = [];
    for (let i = 0; i < LOBBY_SLOTS; i++) {
      const player = players[i];
      const row = document.createElement("li");
      const pip = document.createElement("i");
      pip.className = "pip";
      const who = document.createElement("span");
      who.className = "who";
      const role = document.createElement("span");
      role.className = "role";
      if (player) {
        row.className = "taken";
        who.textContent = player.you ? `${player.name} (you)` : player.name;
        role.textContent = player.host ? "Host" : "Crew";
      } else {
        row.className = "open";
        who.textContent = "Open";
        role.textContent = "Waiting";
      }
      row.append(pip, who, role);
      rows.push(row);
    }
    slots.replaceChildren(...rows);
  }

  net.on("players", renderSlots);
  net.on("room", (room) => {
    code.textContent = room.code ?? "——————";
    byId("lobby-note").textContent = room.host
      ? "Read the code out to whoever is joining."
      : "The host decides the settings.";
    // A room that arrived rather than one you opened is a lobby you have
    // joined, and the page goes there.
    if (room.code && showing !== "lobby") show("lobby");
    lobbyTool.setEditable(room.host);
  });

  // What the host chose, for a guest. The host's own pushes come back
  // through here too, and are already applied.
  net.on("settings", (what) => {
    if (net.host) return;
    for (const key of ["moneyPerBim", "ship"]) {
      if (typeof what[key] === "number") settings[key] = what[key];
    }
    world.applyFromHost(what);
  });

  // A guest pointing at a station: a ring on the map for everybody, and a
  // word about who.
  net.on("suggestion", (said) => {
    world.ping(said.star);
    const place = world.placeName(said.star, said.station) ?? `star ${said.star}`;
    remark(byId("world-note"), `${said.from} suggests starting at ${place}.`);
  });

  // Said out loud in the lobby footer: the multiplayer surface has no wire
  // behind it and the screen should not imply otherwise.
  const LOBBY_SAID = "Nobody can reach this lobby yet.";
  byId("lobby-said").textContent = LOBBY_SAID;

  renderSlots([]);

  // --- the menu ---------------------------------------------------------

  const note = byId("join-note");

  // Per element, so a remark in the lobby cannot wipe one on the menu and the
  // two cannot cancel each other's timer.
  const remarks = new Map();
  const dressed = new Map();

  /** Say something on `el` for a moment, then put `back` there. The element's
   * own classes are kept: `warn` is added to them rather than replacing them,
   * or a styled row loses its styling for four seconds. */
  function remark(el, text, warn = false, back = "") {
    if (!dressed.has(el)) dressed.set(el, el.className);
    const base = dressed.get(el);
    el.textContent = text;
    el.className = warn ? `${base} warn`.trim() : base;
    const mine = (remarks.get(el) ?? 0) + 1;
    remarks.set(el, mine);
    setTimeout(() => {
      if (remarks.get(el) !== mine) return;
      el.textContent = back;
      el.className = base;
    }, NOTE_SECONDS * 1000);
  }

  function say(text, warn = false) {
    remark(note, text, warn);
  }

  byId("play").addEventListener("click", () => show("setup"));

  byId("create-lobby").addEventListener("click", () => {
    const made = net.create();
    if (!made.ok) {
      say(made.why, true);
      return;
    }
    show("lobby");
  });

  byId("join").addEventListener("submit", (event) => {
    event.preventDefault();
    const typed = byId("join-code").value.trim().toUpperCase();
    const tried = net.join(typed);
    if (!tried.ok) say(tried.why, true);
  });

  byId("copy").addEventListener("click", () => {
    const text = code.textContent;
    // Guarded rather than assumed: the clipboard is absent over plain http on
    // some browsers, and a menu button that throws takes the page with it.
    if (typeof navigator !== "undefined" && navigator.clipboard) {
      navigator.clipboard.writeText(text);
      remark(byId("lobby-said"), `Copied ${text}.`, false, LOBBY_SAID);
    }
  });

  for (const button of document.querySelectorAll("[data-goto]")) {
    button.addEventListener("click", () => {
      if (showing === "lobby") net.leave();
      show(button.dataset.goto);
    });
  }

  // --- starting ---------------------------------------------------------
  //
  // Start hands the game over to the ship designer — `web/ship.html`, a page
  // of its own with its own wasm. What crosses is a query string of numbers
  // and nothing else: the money each Bim brings, the build area in tiles,
  // how many players there are, which slot you are, the seed in its two
  // halves, the galaxy type, and the star and station the game starts at.
  //
  // Numbers rather than ids on purpose. `"full"` and `"large"` exist for the
  // buttons; what a game is *started with* is what will cross into wasm, and
  // no strings do — which goes for the euro sign as well. The designer reads
  // the same numbers, clamps them, and falls back to its own defaults for
  // anything missing.
  //
  // **Start needs a station.** Until one is picked the buttons are disabled
  // and say why; a game with nowhere to start is not a game.

  const chosen = byId("chosen");

  /** Why Start cannot be pressed, or null if it can. */
  function startRefusal() {
    if (settings.spawnStar === undefined) return "Pick a station to start at on the World tab.";
    return null;
  }

  function refreshStart() {
    const why = startRefusal();
    for (const button of document.querySelectorAll("[data-start]")) {
      button.disabled = why !== null;
    }
    for (const said of document.querySelectorAll("[data-start-why]")) {
      said.textContent = why ?? "";
    }
  }

  /** Everything the designer needs, as numbers. */
  function started() {
    return {
      money: settings.moneyPerBim,
      area: settings.ship,
      players: net.code ? Math.max(1, net.players.length) : 1,
      slot: 0,
      seedHi: settings.seedHi,
      seedLo: settings.seedLo,
      galaxy: settings.galaxy,
      star: settings.spawnStar,
      station: settings.spawnStation,
    };
  }

  function startQuery() {
    const it = started();
    return (
      `?money=${it.money}&area=${it.area}&players=${it.players}&slot=${it.slot}` +
      `&seedHi=${it.seedHi}&seedLo=${it.seedLo}&galaxy=${it.galaxy}` +
      `&star=${it.star}&station=${it.station}`
    );
  }

  function describe() {
    const ship = SHIPS.find((s) => s.tiles === settings.ship);
    const purse = MONEY.find((m) => m.amount === settings.moneyPerBim);
    return [
      ["Ship", `${ship ? ship.label : "Custom"} — ${settings.ship} × ${settings.ship} tiles`],
      ["Money", `${purse ? purse.label : "Custom"} — ${euros(settings.moneyPerBim)} each`],
      ...world.describe(),
      ["Crew", net.code ? `${net.players.length} in the lobby` : "One"],
      ["Room", net.code ?? "Solo"],
    ];
  }

  function startGame() {
    // Refused here as well as by `disabled`, for the reason the option
    // buttons give: the attribute is the browser's half of the rule.
    if (startRefusal() !== null) return;

    // The page it is going to, said out loud first. `location` is guarded
    // rather than assumed — it is missing under the node harness, the same
    // way `navigator` and `crypto` are, and a Start that throws leaves the
    // player looking at a lobby that has stopped responding.
    const lines = [];
    for (const [name, said] of describe()) {
      const line = document.createElement("div");
      const label = document.createElement("b");
      label.textContent = `${name}: `;
      const value = document.createElement("span");
      value.textContent = said;
      line.append(label, value);
      lines.push(line);
    }
    chosen.replaceChildren(...lines);
    show("build");

    const going = `ship.html${startQuery()}`;
    if (typeof location !== "undefined" && typeof location.assign === "function") {
      location.assign(going);
    }
  }

  for (const button of document.querySelectorAll("[data-start]")) {
    button.addEventListener("click", startGame);
  }

  // --- odds and ends ----------------------------------------------------

  /** Random bytes, from the browser when it has them. `crypto` is absent
   * over plain http on some engines and under the node harness. */
  function randomBytes(n) {
    const bytes = new Uint8Array(n);
    if (typeof crypto !== "undefined" && crypto.getRandomValues) {
      crypto.getRandomValues(bytes);
    } else {
      for (let i = 0; i < n; i++) bytes[i] = Math.floor(Math.random() * 256);
    }
    return bytes;
  }

  /** A random u64, as the two u32 halves it crosses the boundary in. */
  function randomSeed() {
    const b = randomBytes(8);
    const half = (o) => ((b[o] << 24) | (b[o + 1] << 16) | (b[o + 2] << 8) | b[o + 3]) >>> 0;
    return { hi: half(0), lo: half(4) };
  }

  function makeCode() {
    const out = [];
    const bytes = randomBytes(CODE_LENGTH);
    for (let i = 0; i < CODE_LENGTH; i++) {
      out.push(CODE_ALPHABET[bytes[i] % CODE_ALPHABET.length]);
    }
    return out.join("");
  }

  function isCode(text) {
    if (text.length !== CODE_LENGTH) return false;
    for (const ch of text) {
      if (!CODE_ALPHABET.includes(ch)) return false;
    }
    return true;
  }

  show("menu");
  refreshStart();

  // The world module last, and guarded: a page with no `fetch` or no
  // `WebAssembly` — the node stub for a page that is only a page — still
  // has a menu, a setup screen and a lobby, and a World tab that says what
  // is missing rather than sitting there loading for ever.
  if (typeof fetch !== "function" || typeof WebAssembly === "undefined") {
    world.failed("There is no world module here — the galaxy cannot be shown.");
  } else {
    world.load().catch((err) => {
      console.error(err);
      world.failed(`Could not load the world: ${err.message}`);
    });
  }
}
