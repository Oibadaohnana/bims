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
//   * **No strings cross the wasm boundary.** The parts, the resources, the
//     reasons an edit was refused and the faults in a design are all numbers.
//     The four tables below are where every word on this page lives.
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
];

/** The palette, grouped the way a ship is thought about rather than the way
 * the enum is numbered.
 *
 * The rows are built off `ship_part_count()` rather than off this list, so a
 * kind added to the enum and forgotten here still gets a button — under
 * "Anything else", where it is obvious — instead of quietly not existing.
 * Same arrangement as the room's work panel, and for the same reason. */
const PART_GROUPS = [
  { name: "Structure", kinds: [0, 1, 2] },
  { name: "Engines", kinds: [3] },
  { name: "Crew", kinds: [4, 10, 9] },
  { name: "Galley", kinds: [5, 6, 7, 8] },
  { name: "Heads", kinds: [11, 12] },
  { name: "Bay", kinds: [13, 14] },
];

/** Indexed by `physics::ResourceId`. */
const RESOURCE_NAMES = ["Ore", "Metal", "Fuel", "Components"];

/** Why an edit was refused. Indexed by `EditError`; 0 never appears here
 * because 0 is "it took". */
const EDIT_LINES = {
  1: "That falls outside the build area.",
  2: "Something is already standing there.",
  3: "There is no deck under it.",
  4: "There is deck there already.",
  5: "The station does not have the stores for that.",
  6: "That part is not there any more.",
  7: "Take what is standing on it off first.",
  8: "The page asked for something that is not a part.",
  9: "The design is settled — nothing can be moved now.",
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
  21: "Nothing pushes on every axis: it cannot stop or cannot steer.",
  22: "No hydroponic bay. The food aboard is all the food there will be.",
  23: "No broom locker, so nothing to sweep the deck with.",
};

/** What the lobby hands over when nobody chose anything. Opening ship.html
 * with no query is a solo game on a standard ship. */
const DEFAULTS = { area: 40, stock: 1, players: 1, slot: 0 };

/** Sane bounds on what the query string may say. A hand-typed `?area=9000`
 * is a build area with a million tiles in it. */
const AREA_MIN = 8;
const AREA_MAX = 120;
const PLAYERS_MAX = 4;

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

  const canvas = byId("stage");
  const ctx = canvas.getContext("2d");
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
      // The factor arrives as the lobby wrote it — 0.5, 1, 2 — and crosses
      // into wasm as thousandths, because two players have to end up with the
      // same stockpile down to the unit and float rounding is a promise
      // nobody made.
      stock: Math.round(clampNumber(number("stock", DEFAULTS.stock), 0, 100) * 1000),
      players,
      slot: whole(number("slot", DEFAULTS.slot), 0, players - 1),
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

  let canvasSize = { w: 0, h: 0 };
  let dpr = 1;

  function resize() {
    dpr = window.devicePixelRatio || 1;
    const w = Math.max(64, canvas.clientWidth);
    const h = Math.max(64, canvas.clientHeight);
    canvas.width = Math.round(w * dpr);
    canvas.height = Math.round(h * dpr);
    canvasSize = { w, h };
    return canvasSize;
  }

  resize();
  wasm.ship_init(
    chosen.area,
    chosen.stock,
    chosen.players,
    chosen.slot,
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
        } else if (message.accept !== undefined) {
          if (message.accept) {
            const at = message.at;
            took = wasm.ship_accept(message.from, at.hi, at.lo) !== 0;
          } else {
            wasm.ship_unaccept(message.from);
            took = true;
          }
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

  // --- the stockpile -----------------------------------------------------

  const storeCells = [];

  function buildStores() {
    const rows = [];
    for (let id = 0; id < resourceCount; id++) {
      const row = document.createElement("div");
      row.className = "store";
      row.dataset.resource = String(id);
      const name = document.createElement("span");
      name.className = "name";
      name.textContent = RESOURCE_NAMES[id] ?? `Resource ${id}`;
      const left = document.createElement("span");
      left.className = "left";
      // What there was to start with, beside what is left of it. Fixed for
      // the whole phase — the lobby's factor decided it — so it is written
      // once here rather than repainted.
      const all = document.createElement("span");
      all.className = "of";
      all.textContent = `/ ${wasm.ship_stock(id)}`;
      row.append(name, left, all);
      rows.push(row);
      storeCells.push({ row, left, id });
    }
    byId("stores").replaceChildren(...rows);
  }

  /** What is left, and whether the part under the ghost would still fit in
   * it. A resource the current tool would overdraw is marked, which is the
   * only warning a player gets before a click is simply refused. */
  function paintStores() {
    const tool = wasm.ship_tool();
    for (const cell of storeCells) {
      const left = wasm.ship_remaining(cell.id);
      const want = wasm.ship_part_cost(tool, cell.id);
      cell.left.textContent = String(left);
      cell.row.className = want > left ? "store short" : "store";
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
      row.className = error ? "error" : "warning";
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

  /** What the play phase will be handed. Nothing consumes it yet, so the
   * numbers are shown instead — a mass and an acceleration nobody has looked
   * at is a mass and an acceleration that is quietly wrong. */
  function paintHandoff() {
    const rows = [
      ["Parts", String(wasm.ship_part_total())],
      ["Crew aboard", String(net.players)],
      ["Ship mass", wasm.ship_mass().toFixed(1)],
    ];
    const axes = ["Forward", "Backward", "Left", "Right"];
    for (let axis = 0; axis < axes.length; axis++) {
      rows.push([`Acceleration ${axes[axis].toLowerCase()}`, wasm.ship_acceleration(axis).toFixed(4)]);
    }
    const made = [];
    for (const [name, value] of rows) {
      const line = document.createElement("div");
      const label = document.createElement("b");
      label.textContent = name;
      const held = document.createElement("span");
      held.textContent = value;
      line.append(label, held);
      made.push(line);
    }
    byId("handoff").replaceChildren(...made);
  }

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
    let last = 0;
    for (const edit of edits) {
      const done =
        edit.remove !== undefined
          ? net.remove(edit.remove)
          : net.place(edit.kind, edit.x, edit.y, edit.rotation);
      if (!done.ok) {
        skipped++;
        last = done.why;
      }
    }

    if (skipped === 0) {
      remark("");
    } else if (edits.length === 1) {
      remark(EDIT_LINES[last] ?? "That could not be done.");
    } else {
      remark(`${skipped} of ${edits.length} skipped — ${EDIT_LINES[last] ?? "refused"}`);
    }
    afterChange();
  }

  // --- the keyboard ------------------------------------------------------

  const held = new Set();

  window.addEventListener("keydown", (event) => {
    const key = String(event.key ?? "").toLowerCase();
    if (key === "r") {
      wasm.ship_rotate_ghost();
      paintPalette();
      return;
    }
    if (key === "escape") {
      wasm.ship_drag_cancel();
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

    const price = [];
    for (let id = 0; id < resourceCount; id++) {
      const units = wasm.ship_part_cost(tool, id);
      if (units > 0) price.push(`${units} ${RESOURCE_NAMES[id] ?? id}`);
    }
    readoutPrice.textContent = price.join(" · ");
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
    paintStores();
    paintIssues();
    paintCrew();
    paintAccept();
    if (!stillDesigning()) {
      paintHandoff();
      show("done");
    }
  }

  net.on("rejected", (result) => {
    // A single refused Edit inside a drag is reported by `commitDrag`, which
    // knows how many there were. This is for anything else that fails.
    if (result.why && !saying) remark(EDIT_LINES[result.why] ?? "That could not be done.");
  });

  byId("area").textContent = `${chosen.area} × ${chosen.area} tiles`;
  buildPalette();
  buildStores();
  show("design");
  afterChange();

  let last = performance.now();

  function frame(now) {
    const dt = Math.min((now - last) / 1000, MAX_FRAME_DT);
    last = now;
    pumpKeys(dt);
    ageRemark(now);
    wasm.ship_render();
    paint();
    paintReadout();
    requestAnimationFrame(frame);
  }

  requestAnimationFrame(frame);
}
