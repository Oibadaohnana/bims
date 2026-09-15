// Renderer and host loop for Bims.
//
// All game logic lives in the wasm module. This file hands the module a
// canvas, feeds it input, replays the shape buffer it produces, and draws the
// fixture menus as real DOM so they behave like menus.

const STRIDE_FALLBACK = 12;

const KIND_RECT = 0;
const KIND_ELLIPSE = 1;

/** Fixture codes returned by bims_hit_at and bims_drag_end; must match
 * src/room.rs. */
const HIT_FRIDGE = 1;
const HIT_STOVE = 2;
const HIT_BED = 3;
const HIT_TOILET = 4;
const HIT_DOOR = 5;

/** What bims_activity() reports the Bim is busy with. A lie-down is left out:
 * the status line has the countdown from bims_rest_left() to show instead. */
const ACTIVITY = {
  1: "Making food…",
  2: "Off to the cooker…",
  4: "In the heads — a wash to follow",
};

/** Minutes in a game day, for wrapping a wake-up time round midnight. */
const MINUTES_PER_DAY = 24 * 60;

/** The simulation always advances in steps of this size, whatever the display
 * refresh rate or the speed multiplier. Fixed steps keep the walk cycle, the
 * cooking timers and the collision push-out behaving identically at 1x and 12x. */
const SIM_STEP = 1 / 60;

/** Ceiling on steps per frame. Without it, a tab that was backgrounded for a
 * minute would try to catch up in one frame and lock the page up. */
const MAX_STEPS_PER_FRAME = 16;

/** Fastest the simulation will run. 12 steps a frame at 60Hz is comfortably
 * inside MAX_STEPS_PER_FRAME, so the top of the range is really 12x. */
const MAX_SPEED = 12;

/** Largest wall-clock gap we believe. Anything longer is a stall, not real time. */
const MAX_FRAME_DT = 1 / 20;

/** Below this the pointer moved so little that it counts as a click, not a sweep. */
const CLICK_SLOP = 4;

/** A clock reading, wrapped into one day. Time itself is kept in wasm; the
 * only thing that happens here is the formatting. */
function clockText(minutes) {
  const m = ((minutes % MINUTES_PER_DAY) + MINUTES_PER_DAY) % MINUTES_PER_DAY;
  const hh = String(Math.floor(m / 60)).padStart(2, "0");
  const mm = String(Math.floor(m % 60)).padStart(2, "0");
  return `${hh}:${mm}`;
}

/** A length of time, said the way a person would say it. */
function spanText(minutes) {
  const total = Math.round(minutes);
  if (total < 60) return `${total} min`;
  const hours = Math.floor(total / 60);
  const rest = total % 60;
  if (rest === 0) return `${hours} hour${hours === 1 ? "" : "s"}`;
  return `${hours}h ${rest}m`;
}

async function boot() {
  const canvas = document.getElementById("stage");
  const status = document.getElementById("status");
  const menu = document.getElementById("menu");
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

  // --- fixture menus ----------------------------------------------------

  function closeMenu() {
    menu.hidden = true;
    menu.replaceChildren();
  }

  /** Build a dropdown for the fixture that was clicked, at the cursor. */
  function openMenu(fixture, event) {
    const busy = wasm.bims_is_busy() !== 0;
    const items = [];

    if (fixture === HIT_FRIDGE) {
      items.push({
        label: "Make food",
        hint: busy ? "already cooking" : "a whole meal, start to finish",
        disabled: busy,
        run: () => wasm.bims_make_food(),
      });
      items.push({
        label: wasm.bims_fridge_is_open() ? "Close door" : "Open door",
        disabled: busy,
        run: () => wasm.bims_toggle_fridge(),
      });
    } else if (fixture === HIT_STOVE) {
      items.push({
        label: wasm.bims_stove_is_on() ? "Turn off" : "Turn on",
        hint: busy ? "busy" : "the Bim walks over to it",
        disabled: busy,
        run: () => wasm.bims_toggle_stove(),
      });
    } else if (fixture === HIT_TOILET) {
      const locked = wasm.bims_door_is_locked() !== 0;
      items.push({
        label: "Use",
        hint: locked
          ? "the door is locked"
          : busy
            ? "busy"
            : "and wash at the basin after",
        disabled: busy || locked,
        run: () => wasm.bims_use_toilet(),
      });
    } else if (fixture === HIT_DOOR) {
      const open = wasm.bims_door_is_open() !== 0;
      const locked = wasm.bims_door_is_locked() !== 0;
      items.push({
        label: open ? "Close door" : "Open door",
        hint: locked ? "unlock it first" : "it is powered — no walk needed",
        disabled: busy || locked,
        run: () => wasm.bims_toggle_door(),
      });
      items.push({
        label: locked ? "Unlock" : "Lock",
        hint: locked ? "" : "shuts it as well",
        disabled: busy,
        run: () => wasm.bims_toggle_door_lock(),
      });
    } else if (fixture === HIT_BED) {
      // Both lengths come from wasm, so the menu cannot promise half an hour
      // and have the Bim sleep for something else.
      const now = wasm.bims_clock_minutes();
      for (const rest of [
        { minutes: wasm.bims_nap_minutes(), label: "Nap", run: wasm.bims_nap },
        {
          minutes: wasm.bims_sleep_minutes(),
          label: "Sleep",
          run: wasm.bims_sleep,
        },
      ]) {
        items.push({
          label: `${rest.label} — ${spanText(rest.minutes)}`,
          hint: busy ? "busy" : `up around ${clockText(now + rest.minutes)}`,
          disabled: busy,
          run: rest.run,
        });
      }
    }
    if (!items.length) return;

    menu.replaceChildren(
      ...items.map((item) => {
        const button = document.createElement("button");
        button.type = "button";
        button.disabled = !!item.disabled;
        const label = document.createElement("span");
        label.textContent = item.label;
        button.appendChild(label);
        if (item.hint) {
          const hint = document.createElement("small");
          hint.textContent = item.hint;
          button.appendChild(hint);
        }
        button.addEventListener("click", () => {
          item.run();
          closeMenu();
        });
        return button;
      }),
    );

    // Place at the cursor, then pull back inside the window if it would spill.
    menu.hidden = false;
    const pad = 8;
    const box = menu.getBoundingClientRect();
    const x = Math.min(event.clientX, window.innerWidth - box.width - pad);
    const y = Math.min(event.clientY, window.innerHeight - box.height - pad);
    menu.style.left = `${Math.max(pad, x)}px`;
    menu.style.top = `${Math.max(pad, y)}px`;
    menu.querySelector("button:not([disabled])")?.focus();
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
      if (!wasm.bims_hit_at(p.x, p.y)) wasm.bims_order_move(p.x, p.y);
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
    if (e.pointerId !== dragPointer) return;
    const p = at(e);
    wasm.bims_drag_update(p.x, p.y);
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
    } else if (e.key === "Escape") {
      if (!menu.hidden) closeMenu();
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

  // Clicking away from an open menu dismisses it.
  window.addEventListener("pointerdown", (e) => {
    if (!menu.hidden && !menu.contains(e.target)) closeMenu();
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
  }

  const IDLE_HINT =
    "Click a fixture for its menu · 1 or drag to select · right-click the floor to move";

  let last = performance.now();
  let shown = null;
  let clockShown = null;
  let backlog = 0;

  function frame(now) {
    const elapsed = Math.min((now - last) / 1000, MAX_FRAME_DT);
    last = now;

    // Speed is applied by running more fixed steps, never by stretching one.
    // A 12x step would move the Bim further than its own body in a frame, and
    // it would walk through the counter.
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
    const resting = wasm.bims_rest_left();
    const doing = ACTIVITY[wasm.bims_activity()];
    const state = resting > 0
      ? `In bed · ${spanText(resting)} to go, up at ${clockText(minutes + resting)}`
      : doing
        ? doing
        : selected
          ? "Bim selected — right-click the floor to send it there"
          : IDLE_HINT;
    if (state !== shown) {
      shown = state;
      status.textContent = state;
    }
    requestAnimationFrame(frame);
  }

  requestAnimationFrame(frame);
}

boot().catch((err) => {
  console.error(err);
  document.getElementById("error").textContent = `Could not start Bims: ${err}`;
});
