// The front of the game: the start menu, the setup screen, and the lobby.
//
// This is the page `nix run .#builder` serves. It is deliberately separate
// from web/bims.js — that file is the room, and knows nothing about menus;
// this one is everything before the room, and touches no wasm at all yet.
//
// The multiplayer half is surface only. Nothing here opens a socket: `net`
// below is a local stand-in with the shape a transport will have, and it is
// the one seam to replace. Every part of the UI that will eventually be told
// something by the network is already told it by `net` instead, so wiring one
// up means implementing `net` and not touching the screens.

/** What the settings come out as, and the only things that leave this page:
 * plain numbers. The simulation takes numbers across the wasm boundary and no
 * strings at all, and starting as we mean to go on keeps the boundary honest
 * when the builder actually starts a game. */
const STOCKPILES = [
  { id: "half", label: "Lean", factor: 0.5, sub: "×0.5" },
  { id: "base", label: "Standard", factor: 1, sub: "×1" },
  { id: "double", label: "Full", factor: 2, sub: "×2" },
];

/** Starting ship, in tiles a side. */
const SHIPS = [
  { id: "small", label: "Small", tiles: 30, sub: "30 × 30" },
  { id: "standard", label: "Standard", tiles: 40, sub: "40 × 40" },
  { id: "large", label: "Large", tiles: 60, sub: "60 × 60" },
];

/** The defaults a game starts from if nobody touches anything. */
const DEFAULT_STOCKPILE = 1;
const DEFAULT_SHIP = 40;

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

boot();

function boot() {
  const byId = (id) => document.getElementById(id);

  // --- what a game would be started with ------------------------------
  //
  // One object, shared by every settings tool on the page. A tool writes into
  // it and re-reads it when it is shown, so the solo screen and the lobby
  // cannot disagree about what was picked.
  const settings = {
    stockpile: DEFAULT_STOCKPILE,
    ship: DEFAULT_SHIP,
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

    leave() {
      net.code = null;
      net.host = false;
      net.players = [];
      net.emit("players", net.players);
    },
  };

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
        key: "stockpile",
        name: "Starting stores",
        note: "What is aboard on day one",
        options: STOCKPILES,
        value: (o) => o.factor,
      });
      addChoice(panel, {
        key: "ship",
        name: "Ship size",
        note: "Tiles a side",
        options: SHIPS,
        value: (o) => o.tiles,
      });
    });

    addTab("world", "World", (panel) => {
      const empty = document.createElement("div");
      empty.className = "empty";
      const head = document.createElement("strong");
      head.textContent = "Nothing out there yet.";
      const said = document.createElement("span");
      said.textContent =
        "Where the ship is, what it can reach, and what it meets on the way " +
        "will be set here once the world module exists.";
      empty.append(head, said);
      panel.appendChild(empty);
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
    lobbyTool.setEditable(room.host);
    byId("lobby-note").textContent = room.host
      ? "Read the code out to whoever is joining."
      : "The host decides the settings.";
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
  // of its own with its own wasm. What crosses is four numbers in a query
  // string and nothing else: the stockpile factor, the build area in tiles,
  // how many players there are, and which slot you are.
  //
  // Numbers rather than ids on purpose. `"double"` and `"large"` exist for the
  // buttons; what a game is *started with* is what will cross into wasm, and
  // no strings do. The designer reads the same four, clamps them, and falls
  // back to its own defaults for anything missing.

  const chosen = byId("chosen");

  /** Everything the designer needs, as numbers. */
  function started() {
    return {
      stock: settings.stockpile,
      area: settings.ship,
      players: net.code ? Math.max(1, net.players.length) : 1,
      slot: 0,
    };
  }

  function startQuery() {
    const it = started();
    return `?stock=${it.stock}&area=${it.area}&players=${it.players}&slot=${it.slot}`;
  }

  function describe() {
    const ship = SHIPS.find((s) => s.tiles === settings.ship);
    const store = STOCKPILES.find((s) => s.factor === settings.stockpile);
    return [
      ["Ship", `${ship ? ship.label : "Custom"} — ${settings.ship} × ${settings.ship} tiles`],
      ["Stores", `${store ? store.label : "Custom"} — ×${settings.stockpile}`],
      ["Crew", net.code ? `${net.players.length} in the lobby` : "One"],
      ["Room", net.code ?? "Solo"],
    ];
  }

  function startGame() {
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

  function makeCode() {
    const out = [];
    const bytes = new Uint8Array(CODE_LENGTH);
    if (typeof crypto !== "undefined" && crypto.getRandomValues) {
      crypto.getRandomValues(bytes);
    } else {
      for (let i = 0; i < CODE_LENGTH; i++) {
        bytes[i] = Math.floor(Math.random() * 256);
      }
    }
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
}
