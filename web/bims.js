// Renderer and host loop for Bims.
//
// All game logic lives in the wasm module. This file hands the module a
// canvas, feeds it input, replays the shape buffer it produces, and draws the
// fixture menus as real DOM so they behave like menus.

const STRIDE_FALLBACK = 12;

const KIND_RECT = 0;
const KIND_ELLIPSE = 1;
/** The bottom-left half of the box, spun about the box's centre. The room
 * never emits one; the ship's pages do, and one replay loop reads either. */
const KIND_TRIANGLE = 2;

const SPEED_TIP =
  "How fast the simulation runs. At 24x a whole game day goes by in about a minute.";

/** The simulation always advances in steps of this size, whatever the display
 * refresh rate or the speed multiplier. Fixed steps keep the walk cycle, the
 * cooking timers and the collision push-out behaving identically at 1x and 24x. */
const SIM_STEP = 1 / 60;

/** Ceiling on steps per frame. Without it, a tab that was backgrounded for a
 * minute would try to catch up in one frame and lock the page up. */
const MAX_STEPS_PER_FRAME = 32;

/** Fastest the simulation will run. 24 steps a frame at 60Hz stays inside
 * MAX_STEPS_PER_FRAME with the same third of headroom the old 12x cap had, so
 * the top of the range really is 24x on a display that keeps up. At 24x a game
 * day — 24 minutes of real time at 1x — goes by in one minute. */
const MAX_SPEED = 24;

/** Largest wall-clock gap we believe. Anything longer is a stall, not real time. */
const MAX_FRAME_DT = 1 / 20;

/// The bubble over a talking Bim. Sits above the name, so the two do not
/// fight, and in the panels' own colours so it reads as part of the interface
/// rather than as something painted on the deck.
const BUBBLE_SIZE = 12;
const BUBBLE_LIFT = 68;
const BUBBLE_PAD = 8;
const BUBBLE_FILL = "rgba(20, 29, 25, 0.94)";
const BUBBLE_EDGE = "rgba(127, 209, 168, 0.55)";
const BUBBLE_INK = "#e7efe9";

async function boot() {
  const canvas = document.getElementById("stage");
  const status = document.getElementById("status");
  const clock = document.getElementById("clock");
  const speedInput = document.getElementById("speed");
  const speedLabel = document.getElementById("speed-value");
  const ctx = canvas.getContext("2d");

  // Plain `instantiate` rather than `instantiateStreaming`: it works no matter
  // how the static server labels .wasm files.
  // Same cache-busting token the page was loaded with, so the wasm always
  // matches the script that fetched it.
  const build = window.BIMS_BUILD ?? Date.now();
  const bytes = await fetch(`bims.wasm?v=${build}`).then((r) => r.arrayBuffer());
  const { instance } = await WebAssembly.instantiate(bytes, {});
  const wasm = instance.exports;

  const stride = wasm.bims_stride ? wasm.bims_stride() : STRIDE_FALLBACK;

  // How many are aboard, and which of them the mouse steers. Read out of wasm
  // rather than assumed, so a third crew member would be a change in one place
  // and a name added to CREW_NAMES.
  const crewCount = wasm.bims_crew();
  const player = wasm.bims_player();

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
  const seedHi = (Math.random() * 0x100000000) >>> 0;
  const seedLo = (Math.random() * 0x100000000) >>> 0;
  wasm.bims_init(seedHi, seedLo, canvasSize.w, canvasSize.h);

  // The crew's panels — the tray, the side, the agendas, the menus and the
  // tooltips — are `crew.js`, shared with the ship, which runs this same room
  // aboard. What is left in this file is the deck: the canvas, the pointer
  // over it, and the speed, which the ship has its own buttons for.
  const deck = crewHost({ wasm, player, crewCount });
  const { openMenu, closeMenu, crewName } = deck;
  deck.explainWord(document.querySelector("#speed-control span"), SPEED_TIP);

  window.addEventListener("resize", () => {
    resize();
    wasm.bims_resize(canvasSize.w, canvasSize.h);
    closeMenu();
  });

  // --- coordinates ------------------------------------------------------
  //
  // The room is a fixed size; wasm gives us the transform that fits it to the
  // canvas. Drawing applies it, and pointer handling inverts it.

  /** Pointer position in room coordinates. */
  function at(event) {
    const rect = canvas.getBoundingClientRect();
    const s = wasm.bims_view_scale();
    return {
      x: (event.clientX - rect.left - wasm.bims_view_x()) / s,
      y: (event.clientY - rect.top - wasm.bims_view_y()) / s,
    };
  }

  // --- simulation speed -------------------------------------------------

  let speed = 1;

  function setSpeed(value) {
    speed = Math.min(MAX_SPEED, Math.max(1, Math.round(value)));
    speedInput.value = String(speed);
    speedLabel.textContent = `${speed}x`;
  }

  speedInput.addEventListener("input", () => setSpeed(Number(speedInput.value)));
  setSpeed(1);

  // --- what the pointer is over -------------------------------------------
  //
  // Where the pointer is, kept in room coordinates and asked about every
  // frame rather than only when it moves: the hob gets lit, the door gets
  // locked, someone is sick on the tile — all of that happens under a
  // stationary pointer and the readout should say so.

  const hoverBox = document.getElementById("hover");
  const hoverThing = hoverBox.querySelector(".thing");
  const hoverOnIt = hoverBox.querySelector(".on-it");

  let hoverAt = null;
  let hoverShown = null;

  function paintHover() {
    let thing = "—";
    let onIt = "";
    if (hoverAt) {
      // The words are the crew's (`spotReadout` in crew.js), so the ship
      // page says the same thing about the same deck.
      ({ thing, onIt } = deck.spotReadout(hoverAt.x, hoverAt.y));
    }
    const state = `${thing} | ${onIt}`;
    if (state === hoverShown) return;
    hoverShown = state;
    hoverThing.textContent = thing;
    hoverOnIt.textContent = onIt;
  }

  // --- input ------------------------------------------------------------

  let dragPointer = null;
  let dragStart = null;

  // Right-clicking a fixture opens its menu. That happens here rather than in
  // the pointerdown handler below because a menu opened on pointerdown would
  // be shut again by the click-away handler for the very same event.
  canvas.addEventListener("contextmenu", (e) => {
    e.preventDefault();
    const p = at(e);
    const fixture = wasm.bims_hit_at(p.x, p.y);
    if (fixture) openMenu(fixture, e);
  });

  canvas.addEventListener("pointerdown", (e) => {
    const p = at(e);
    if (e.button === 2) {
      e.preventDefault();
      closeMenu();
      // On bare floor a right-click is an order; on furniture the contextmenu
      // handler above takes it instead.
      if (!wasm.bims_hit_at(p.x, p.y)) {
        const refused = ORDER_REFUSED[wasm.bims_order_move(p.x, p.y)];
        if (refused) {
          refusal = refused;
          refusalUntil = performance.now() + REFUSAL_SECONDS * 1000;
        }
      }
      return;
    }
    if (e.button !== 0) return;
    closeMenu();
    // Capture so a drag that leaves the canvas still finishes cleanly.
    dragPointer = e.pointerId;
    dragStart = { x: e.clientX, y: e.clientY };
    canvas.setPointerCapture(e.pointerId);
    wasm.bims_drag_begin(p.x, p.y);
  });

  canvas.addEventListener("pointermove", (e) => {
    // Tracked whatever else is going on, so the readout keeps up during a
    // marquee as well as when the pointer is just wandering about.
    const p = at(e);
    hoverAt = p;
    if (e.pointerId !== dragPointer) return;
    wasm.bims_drag_update(p.x, p.y);
  });

  canvas.addEventListener("pointerleave", () => {
    hoverAt = null;
  });

  function endDrag(e, apply) {
    if (e.pointerId !== dragPointer) return;
    dragPointer = null;
    if (canvas.hasPointerCapture(e.pointerId)) {
      canvas.releasePointerCapture(e.pointerId);
    }
    if (!apply) {
      wasm.bims_drag_cancel();
      return;
    }
    const p = at(e);
    const fixture = wasm.bims_drag_end(p.x, p.y);
    // A click that landed on a fixture opens its menu instead of selecting.
    const moved =
      dragStart &&
      Math.hypot(e.clientX - dragStart.x, e.clientY - dragStart.y) > CLICK_SLOP;
    if (fixture && !moved) openMenu(fixture, e);
  }

  canvas.addEventListener("pointerup", (e) => endDrag(e, true));
  canvas.addEventListener("pointercancel", (e) => endDrag(e, false));

  window.addEventListener("keydown", (e) => {
    if (e.repeat || e.ctrlKey || e.metaKey || e.altKey) return;
    if (e.key === "1") {
      wasm.bims_select_group(1);
    } else if (e.key === "r" || e.key === "R") {
      wasm.bims_toggle_recruited();
      deck.paintRecruited();
    } else if (e.key === "Escape") {
      if (deck.menuOpen()) closeMenu();
      else wasm.bims_clear_selection();
    }
  });

  // Losing focus mid-drag would otherwise leave a marquee stuck on screen.
  window.addEventListener("blur", () => {
    if (dragPointer !== null) {
      dragPointer = null;
      wasm.bims_drag_cancel();
    }
  });

  // --- rendering --------------------------------------------------------

  function paint() {
    // Re-read the view every frame: growing the wasm heap detaches the old one.
    const shapes = new Float32Array(
      wasm.memory.buffer,
      wasm.bims_draw_ptr(),
      wasm.bims_draw_len(),
    );

    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, canvasSize.w, canvasSize.h);

    // Everything below is in room coordinates.
    const s = wasm.bims_view_scale();
    ctx.setTransform(
      dpr * s,
      0,
      0,
      dpr * s,
      dpr * wasm.bims_view_x(),
      dpr * wasm.bims_view_y(),
    );

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

      // A non-zero line width means stroke the outline instead of filling.
      const paintShape = line > 0 ? () => ctx.stroke() : () => ctx.fill();
      const style = `rgba(${r},${g},${b},${a})`;
      if (line > 0) {
        ctx.strokeStyle = style;
        ctx.lineWidth = line;
      } else {
        ctx.fillStyle = style;
      }

      if (kind === KIND_ELLIPSE) {
        ctx.beginPath();
        ctx.ellipse(x, y, Math.abs(w) / 2, Math.abs(h) / 2, rot, 0, Math.PI * 2);
        paintShape();
        continue;
      }

      // A right-angled triangle: the bottom-left half of the box, spun
      // about the box's centre. The room draws none of these; the ship's
      // pages do, and the format is one format.
      if (kind === KIND_TRIANGLE) {
        ctx.save();
        ctx.translate(x, y);
        ctx.rotate(rot);
        ctx.beginPath();
        ctx.moveTo(-w / 2, -h / 2);
        ctx.lineTo(-w / 2, h / 2);
        ctx.lineTo(w / 2, h / 2);
        ctx.closePath();
        paintShape();
        ctx.restore();
        continue;
      }

      // Rectangles are centred on (x, y) and spun about that centre.
      if (rot === 0 && radius === 0 && line === 0) {
        ctx.fillRect(x - w / 2, y - h / 2, w, h);
        continue;
      }
      ctx.save();
      ctx.translate(x, y);
      ctx.rotate(rot);
      ctx.beginPath();
      if (radius > 0 && ctx.roundRect) {
        ctx.roundRect(-w / 2, -h / 2, w, h, Math.min(radius, w / 2, h / 2));
      } else {
        ctx.rect(-w / 2, -h / 2, w, h);
      }
      paintShape();
      ctx.restore();
    }

    paintNames();
    paintChatter();
  }

  /** Each crew member's name, over their head, on the deck.
   *
   * Drawn here rather than in wasm because it is text, and no strings cross
   * that boundary — the shape buffer has rectangles and ellipses in it and
   * nothing else. So the host asks where each Bim is, applies the same
   * transform the shapes went through, and writes the name itself.
   *
   * Back in CSS pixels first: at room scale the type would be stretched with
   * the furniture, and a name is not part of the room. */
  function paintNames() {
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.font = `600 ${NAME_SIZE}px ui-sans-serif, system-ui, sans-serif`;
    ctx.textAlign = "center";
    ctx.textBaseline = "alphabetic";
    ctx.lineJoin = "round";
    ctx.lineWidth = 3;
    const s = wasm.bims_view_scale();
    for (let who = 0; who < crewCount; who++) {
      const x = wasm.bims_view_x() + wasm.bims_bim_x(who) * s;
      const y = wasm.bims_view_y() + wasm.bims_bim_y(who) * s - NAME_LIFT * s;
      // Stroked first in the deck's own darkness, so a name stays legible
      // over a pale worktop as readily as over the floor.
      ctx.strokeStyle = "rgba(6, 10, 9, 0.85)";
      ctx.strokeText(crewName(who), x, y);
      ctx.fillStyle = who === player ? NAME_YOURS : NAME_THEIRS;
      ctx.fillText(crewName(who), x, y);
    }
  }

  /** What one of them is saying, in a bubble over its head.
   *
   * Painted here for the same reason the names are: the draw buffer holds
   * rectangles and ellipses and nothing else, so text cannot come through it.
   * `bims_chat_topic` hands back a `memory::What` code and the wording is
   * entirely this side of the boundary.
   *
   * Only one of them ever has a bubble up — they take turns, decided by wasm
   * off the ship's clock — so two of these never overlap however close
   * together the pair are standing. */
  function paintChatter() {
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.font = `${BUBBLE_SIZE}px ui-sans-serif, system-ui, sans-serif`;
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    const s = wasm.bims_view_scale();
    for (let who = 0; who < crewCount; who++) {
      const topic = wasm.bims_chat_topic(who);
      if (!topic) continue;
      const said = `${CHAT_TOPICS[topic] ?? CHAT_NOTHING}…`;
      const x = wasm.bims_view_x() + wasm.bims_bim_x(who) * s;
      const y = wasm.bims_view_y() + wasm.bims_bim_y(who) * s - BUBBLE_LIFT * s;
      const w = ctx.measureText(said).width + BUBBLE_PAD * 2;
      const h = BUBBLE_SIZE + BUBBLE_PAD * 1.6;
      ctx.fillStyle = BUBBLE_FILL;
      ctx.strokeStyle = BUBBLE_EDGE;
      ctx.lineWidth = 1;
      ctx.beginPath();
      ctx.roundRect(x - w / 2, y - h / 2, w, h, 7);
      ctx.fill();
      ctx.stroke();
      // The tail, pointing down at whoever is talking.
      ctx.beginPath();
      ctx.moveTo(x - 5, y + h / 2 - 1);
      ctx.lineTo(x + 5, y + h / 2 - 1);
      ctx.lineTo(x, y + h / 2 + 7);
      ctx.closePath();
      ctx.fill();
      ctx.fillStyle = BUBBLE_INK;
      ctx.fillText(said, x, y);
    }
  }

  const IDLE_HINT =
    "Click a fixture for its menu · 1 or drag to select · right-click the floor to move · r to recruit";

  let last = performance.now();
  let shown = null;
  let clockShown = null;
  // A refused order has to say so: the marker on the deck fades in a moment,
  // and an order that quietly does nothing reads as a broken click.
  let refusal = null;
  let refusalUntil = 0;
  let backlog = 0;

  function frame(now) {
    const elapsed = Math.min((now - last) / 1000, MAX_FRAME_DT);
    last = now;

    // Speed is applied by running more fixed steps, never by stretching one.
    // A 24x step would move the Bim several times its own body in a frame,
    // and it would walk through the counter.
    backlog += elapsed * speed;
    let steps = 0;
    while (backlog >= SIM_STEP && steps < MAX_STEPS_PER_FRAME) {
      wasm.bims_update(SIM_STEP);
      backlog -= SIM_STEP;
      steps += 1;
    }
    if (backlog > SIM_STEP * MAX_STEPS_PER_FRAME) backlog = 0; // gave up catching up
    paint();

    // Not `now`: that name is taken by the frame timestamp above.
    const minutes = wasm.bims_clock_minutes();
    const stamp = `Day ${wasm.bims_clock_day()} · ${clockText(minutes)}`;
    if (stamp !== clockShown) {
      clockShown = stamp;
      clock.textContent = stamp;
    }

    const selected = wasm.bims_selected_count() !== 0;
    const resting = wasm.bims_rest_left(player);
    deck.paint();
    paintHover();

    // The status line is the player's Bim and nobody else's: it is where an
    // order lands and where a refusal is said, and Kate takes no orders. What
    // she is up to is on her own panels.
    const me = crewName(player);
    // A stall looks like the game has hung unless it says what it is.
    // A stall looks like the game has hung unless it says what it is.
    const doing = wasm.bims_is_napping(player)
      ? `${me} has dropped off where he stands…`
      : wasm.bims_is_stalled(player)
        ? `${me} has lost the thread of it…`
        : ACTIVITY[wasm.bims_activity(player)];
    const refusing = refusal !== null && now < refusalUntil;
    const state = refusing
      ? refusal
      : wasm.bims_is_alive(player) === 0
      ? `${me} has died.`
      : resting > 0
      ? `In bed · ${spanText(resting)} to go, up at ${clockText(minutes + resting)}`
      : doing
        ? doing
        : selected
          ? `${me} selected — right-click the floor to send him there`
          : IDLE_HINT;
    if (state !== shown) {
      shown = state;
      status.textContent = state;
      status.className = refusing ? "refused" : "";
    }
    requestAnimationFrame(frame);
  }

  requestAnimationFrame(frame);
}

boot().catch((err) => {
  console.error(err);
  document.getElementById("error").textContent = `Could not start Bims: ${err}`;
});
