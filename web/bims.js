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
const HIT_DISHWASHER = 6;
const HIT_HYDRO = 7;

/** What bims_order_move() made of a right-click. Only the refusals are worth
 * saying out loud; the rest the Bim shows you by walking. */
const ORDER_REFUSED = {
  3: "Can't get there — the bathroom door is locked.",
  4: "Can't get there at all.",
};

/** How long a refusal stays on the status line. */
const REFUSAL_SECONDS = 4;

/** How long the pointer has to rest on something before it explains itself.
 * Long enough that crossing a panel on the way somewhere else sets nothing
 * off, short enough that asking feels like no wait at all. */
const TIP_DELAY = 300;

/** What each bar is, for the tooltip on its row. Keyed by the name above, so
 * a need cannot end up with the wrong explanation. */
const NEED_TIPS = {
  Rest: "Runs down all day. The timetable is what sends the Bim to bed, not this bar — the level only decides whether a scheduled night is worth taking.",
  Food: "Under 10% the Bim goes and cooks itself a meal. Empty for eight hours and malnutrition sets in; a day of it is fatal.",
  Restroom:
    "Under 10% the Bim takes itself to the toilet. If it cannot — shut in, under orders — it fidgets, then risks wetting itself, and an hour after the bar empties it has an accident.",
  Cleanliness:
    "Not a clock like the others: it follows the mess within three tiles of the Bim and whatever the Bim has on itself. At nothing it treads carefully, then keeps away from the mess, then is sick in it every half hour.",
};

const HEALTH_TIP =
  "Only the worst stage of malnutrition actually costs health, and eating properly walks it back. The lines underneath name whatever is wrong.";

const AUTONOMY_TIP =
  "Off, the Bim starts nothing by itself — no meals, no sleep, no trips to the toilet — but still does everything it is told. The levels carry on moving either way.";

const SPEED_TIP =
  "How fast the simulation runs. At 24x a whole game day goes by in about a minute.";

const FOOD_TIP =
  "How much food the place keeps in stock, in units of two thirds vegetables to one third tofu — ask for 99 and it holds 66 and 33. The hydroponic bay plants to this and sleeps once it is met.";

/** The needs, in the order wasm indexes them. */
const NEED_NAMES = ["Rest", "Food", "Restroom", "Cleanliness"];

/** The three stages of going without sleep, by the number wasm reports. */
const DROWSINESS = [
  "",
  "Sleepy — fumbling, errands a quarter longer",
  "Sleep deprived — errands half as long again",
  "Past it — errands twice as long, and dropping off on its feet",
];

/** The three stages of going without, by the number wasm reports. */
const CONDITIONS = [
  "",
  "Mild malnutrition — moving slowly",
  "Malnutrition — slower, and tiring twice as fast",
  "Extreme malnutrition — losing health",
];

/** How badly the Bim needs the toilet, by the number wasm reports. */
const URGES = [
  "",
  "Needs the toilet — fidgeting",
  "Needs the toilet badly — may not make it",
  "Bursting — an accident within the hour",
];

/** How far gone it is for want of a clean place to stand. */
const DISCOMFORTS = [
  "",
  "Uneasy about the mess — treading carefully",
  "Sickened by the mess — keeping away from it",
  "Sickened by the mess — being sick in it every half hour",
];

/** The errand codes shared by bims_activity() and bims_agenda_job(). */
const JOB_NAMES = {
  1: "Making food",
  2: "Working the cooker",
  3: "Nap",
  4: "Sleep",
  5: "Using the toilet",
  6: "Fridge door",
  7: "Bathroom door",
  8: "Door lock",
  9: "Starting the wash",
  11: "Tending the bay",
  12: "Eating leftovers",
};

/** The same errands as the status line says them. A lie-down is left out: the
 * countdown from bims_rest_left() is more use than the name. */
const ACTIVITY = {
  1: "Making food…",
  2: "Off to the cooker…",
  5: "Using the toilet — a wash to follow",
  11: "In the hydroponics…",
  12: "Helping itself to the pot…",
};

/** Minutes in a game day, for wrapping a wake-up time round midnight. */
const MINUTES_PER_DAY = 24 * 60;

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
  const tip = document.getElementById("tip");
  const clock = document.getElementById("clock");
  const autonomy = document.getElementById("autonomy");
  const agenda = document.getElementById("agenda");
  const needs = document.getElementById("needs");
  const healthBox = document.getElementById("health");
  const condition = document.getElementById("condition");
  const recruited = document.getElementById("recruited");
  const drowsiness = document.getElementById("drowsiness");
  const urgeLine = document.getElementById("urge");
  const discomfortLine = document.getElementById("discomfort");
  const healthBar = healthBox.querySelector(".bar i");
  const healthPct = healthBox.querySelector(".pct");
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

  // --- tooltips ---------------------------------------------------------
  //
  // Everything explanatory is kept off the page and shown on hover instead:
  // the panels stay small, and the deck keeps the room it needs. The delay is
  // what makes that bearable — sweeping the pointer across a panel on the way
  // somewhere else sets nothing off, and only resting on a thing asks it what
  // it is.
  //
  // Listeners go on each element rather than one delegated handler, because
  // `pointerenter` does not bubble and this way nothing has to be matched
  // against a selector on every move of the mouse.

  let tipTimer = null;
  let tipFor = null;

  function hideTip() {
    if (tipTimer !== null) {
      clearTimeout(tipTimer);
      tipTimer = null;
    }
    tipFor = null;
    tip.hidden = true;
  }

  function showTip(el, text) {
    tipFor = el;
    tip.textContent = text;
    tip.hidden = false;
    // Under the thing it explains, nudged back inside the window if that
    // would hang it off an edge. Measured after it is shown, or it has no
    // size to measure.
    const box = el.getBoundingClientRect();
    const own = tip.getBoundingClientRect();
    const margin = 8;
    let left = box.left;
    let top = box.bottom + 6;
    if (left + own.width > window.innerWidth - margin) {
      left = window.innerWidth - margin - own.width;
    }
    if (top + own.height > window.innerHeight - margin) {
      top = box.top - 6 - own.height;
    }
    tip.style.left = `${Math.max(margin, left)}px`;
    tip.style.top = `${Math.max(margin, top)}px`;
  }

  /** Give `el` something to say, after TIP_DELAY of resting on it.
   *
   * Only ever attached to something that *looks* like it has something to say
   * — an underlined word or a "?" — so a tooltip is always answered a question
   * the player asked rather than appearing out of a panel they were only
   * crossing. The two wrappers under this are the whole vocabulary. */
  function explain(el, text) {
    if (!el || !text) return;
    el.setAttribute("data-tip", "");
    const open = () => {
      if (tipTimer !== null) clearTimeout(tipTimer);
      tipTimer = setTimeout(() => {
        tipTimer = null;
        showTip(el, text);
      }, TIP_DELAY);
    };
    const shut = () => {
      if (tipFor === el || tipTimer !== null) hideTip();
    };
    el.addEventListener("pointerenter", open);
    el.addEventListener("pointerleave", shut);
    // Keyboard and touch: a focused control says its piece too, and a tap
    // anywhere dismisses whatever is showing rather than leaving it stuck.
    el.addEventListener("focus", open);
    el.addEventListener("blur", shut);
    el.addEventListener("pointerdown", shut);
  }

  /** Underline a word that already names the thing, and hang its explanation
   * off that. The word is the affordance: dotted underline, help cursor. */
  function explainWord(el, text) {
    if (!el) return;
    el.className = `${el.className} asks`.trim();
    explain(el, text);
  }

  /** A "?" to put beside something that has no word of its own to underline.
   * `label` is what a screen reader reads instead of the bare mark. */
  function questionMark(text, label) {
    const mark = document.createElement("button");
    mark.type = "button";
    mark.className = "what";
    mark.textContent = "?";
    mark.setAttribute("aria-label", label);
    explain(mark, text);
    return mark;
  }

  window.addEventListener("blur", hideTip);
  window.addEventListener("resize", hideTip);

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
    if (wasm.bims_is_alive() === 0) return;
    const busy = wasm.bims_is_busy() !== 0;
    const items = [];
    // Nothing here is remote: every item walks the Bim over to do it by hand.
    // Nor does anything wait for the Bim to be free — a new errand takes over,
    // and what it displaced goes on the agenda to be finished afterwards.
    const takesOver = busy ? "takes over — the rest waits its turn" : null;

    if (fixture === HIT_FRIDGE) {
      // Two counts, not one: a stew is two vegetables and a bowl is a block of
      // tofu with a salad, so either can be the thing that runs out.
      const veg = wasm.bims_store_veg();
      const tofu = wasm.bims_store_tofu();
      const noStew = veg < 2;
      const noBowl = tofu < 1 || veg < 1;
      items.push({
        label: "Cold store",
        hint: `${veg} veg, ${tofu} tofu — target ${wasm.bims_target_veg()} and ${wasm.bims_target_tofu()}`,
        disabled: true,
        run: () => {},
      });
      items.push({
        label: "Make a stew",
        hint: noStew
          ? "needs two vegetables"
          : (takesOver ?? "two vegetables, chopped and cooked"),
        disabled: noStew,
        run: () => wasm.bims_make_stew(),
      });
      items.push({
        label: "Make a bowl",
        hint: noBowl
          ? "needs a block of tofu and a salad"
          : (takesOver ?? "tofu chopped in with the salad, no cooking"),
        disabled: noBowl,
        run: () => wasm.bims_make_bowl(),
      });
      items.push({
        label: wasm.bims_fridge_is_open() ? "Close door" : "Open door",
        hint: takesOver ?? "the Bim walks over to it",
        run: () => wasm.bims_toggle_fridge(),
      });
    } else if (fixture === HIT_STOVE) {
      // The pot lives on the hob, so what is left in it belongs on this menu.
      const left = wasm.bims_pot_servings();
      if (left > 0) {
        items.push({
          label: "Eat from the pot",
          hint:
            takesOver ??
            `${left} of ${wasm.bims_pot_capacity()} helpings left — no cooking`,
          run: () => wasm.bims_eat_leftovers(),
        });
      }
      // A hob left lit shuts itself off; say so rather than letting it look
      // like the game changed its mind.
      const idleLeft = wasm.bims_stove_idle_left();
      items.push({
        label: wasm.bims_stove_is_on() ? "Turn off" : "Turn on",
        hint:
          takesOver ??
          (idleLeft > 0
            ? `left on — cuts out in ${spanText(idleLeft)}`
            : "the Bim walks over to it"),
        run: () => wasm.bims_toggle_stove(),
      });
    } else if (fixture === HIT_DISHWASHER) {
      const loaded = wasm.bims_dishwasher_loaded();
      const capacity = wasm.bims_dishwasher_capacity();
      const left = wasm.bims_dishwasher_cycle_left();
      const now = wasm.bims_clock_minutes();
      if (left > 0) {
        items.push({
          label: "Running",
          hint: `${spanText(left)} left — done at ${clockText(now + left)}`,
          disabled: true,
          run: () => {},
        });
      }
      items.push({
        label: "Run now",
        hint:
          left > 0
            ? "already running"
            : loaded === 0
              ? "nothing in it"
              : (takesOver ?? `${loaded} of ${capacity} stowed — the Bim goes and presses it`),
        disabled: left > 0 || loaded === 0,
        run: () => wasm.bims_run_dishwasher(),
      });
    } else if (fixture === HIT_TOILET) {
      // A locked door only stops a Bim on the wrong side of it, so ask whether
      // this one can actually get there rather than whether the door is shut.
      const canUse = wasm.bims_can_use_toilet() !== 0;
      items.push({
        label: "Use",
        hint: canUse
          ? (takesOver ?? "and wash at the basin after")
          : "can't get to it — the door is locked",
        disabled: !canUse,
        run: () => wasm.bims_use_toilet(),
      });
    } else if (fixture === HIT_DOOR) {
      const open = wasm.bims_door_is_open() !== 0;
      const locked = wasm.bims_door_is_locked() !== 0;
      items.push({
        label: open ? "Close door" : "Open door",
        hint: locked ? "unlock it first" : (takesOver ?? "the Bim walks over to it"),
        disabled: locked,
        run: () => wasm.bims_toggle_door(),
      });
      items.push({
        label: locked ? "Unlock" : "Lock",
        hint: takesOver ?? (locked ? "at the panel" : "shuts it as well"),
        run: () => wasm.bims_toggle_door_lock(),
      });
    } else if (fixture === HIT_HYDRO) {
      // The bay's two controls: follow the manager's target, or a standing
      // order that ignores it. Both are settings rather than errands — the
      // Bim still does every bit of the planting and lifting on foot.
      const spots = wasm.bims_hydro_spots();
      const automated = wasm.bims_hydro_automated() !== 0;
      const asleep = wasm.bims_hydro_hibernating() !== 0;
      const forced = wasm.bims_hydro_forced();
      const ripe = wasm.bims_hydro_ripe();
      let growing = 0;
      let furthest = 0;
      for (let i = 0; i < spots; i++) {
        if (wasm.bims_hydro_crop(i) === 0) continue;
        growing++;
        furthest = Math.max(furthest, wasm.bims_hydro_growth(i));
      }
      const along =
        ripe || !growing ? "" : `, furthest ${Math.round(furthest * 100)}% grown`;

      items.push({
        label: "Trays",
        hint: `${growing} of ${spots} planted${ripe ? `, ${ripe} ready to lift` : along}`,
        disabled: true,
        run: () => {},
      });
      items.push({
        label: "Store",
        hint: `${wasm.bims_store_veg()} veg, ${wasm.bims_store_tofu()} tofu — target ${wasm.bims_target_veg()} and ${wasm.bims_target_tofu()}`,
        disabled: true,
        run: () => {},
      });
      items.push({
        label: automated ? "Stop automating" : "Automate",
        hint: automated
          ? asleep
            ? "at target — holding what is planted"
            : "following the manager's target"
          : "grow whatever the store is short of",
        run: () => wasm.bims_set_hydro_automated(automated ? 0 : 1),
      });
      for (const [code, name, what] of [
        [1, "Plant greens in every tray", "two of these in a stew"],
        [2, "Plant soy in every tray", "a day and a half, and it presses into tofu"],
      ]) {
        const on = forced === code;
        items.push({
          label: on ? `${name} ✓` : name,
          hint: on ? "standing order — click to lift it" : `no matter the target · ${what}`,
          run: () => wasm.bims_set_hydro_forced(on ? 0 : code),
        });
      }
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
          hint: takesOver ?? `up around ${clockText(now + rest.minutes)}`,
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

  // --- what the Bim wants -----------------------------------------------
  //
  // Always on show: these are what set everything else going, so watching a
  // bar run down is watching the next errand arrive. The rows never change,
  // only their widths.

  const needRows = [];

  function buildNeeds() {
    const count = wasm.bims_need_count();
    for (let i = 0; i < count; i++) {
      const li = document.createElement("li");

      const name = document.createElement("span");
      name.className = "name";
      name.textContent = NEED_NAMES[i] ?? `Need ${i + 1}`;

      const bar = document.createElement("span");
      bar.className = "bar";
      const fill = document.createElement("i");
      bar.appendChild(fill);

      const pct = document.createElement("span");
      pct.className = "pct";

      li.append(name, bar, pct);
      explainWord(name, NEED_TIPS[name.textContent]);
      needRows.push({ li, fill, pct, urgent: false });
    }
    needs.replaceChildren(...needRows.map((row) => row.li));
  }

  function paintNeeds() {
    // The threshold comes from wasm so the bar cannot disagree with the
    // behaviour it is meant to be predicting.
    const trigger = wasm.bims_need_threshold();
    for (let i = 0; i < needRows.length; i++) {
      const row = needRows[i];
      const level = wasm.bims_need_level(i);
      const done = Math.round(level * 100);
      row.fill.style.width = `${done}%`;
      row.pct.textContent = `${done}%`;
      const urgent = level < trigger;
      if (urgent !== row.urgent) {
        row.urgent = urgent;
        row.li.className = urgent ? "urgent" : "";
      }
    }
  }

  let conditionShown = null;
  let drowsyShown = null;
  let urgeShown = null;
  let discomfortShown = null;
  let recruitedShown = null;

  // Being under orders outlasts everything else on the status line, so it gets
  // a badge of its own in the header rather than competing for that one line.
  function paintRecruited() {
    const on = wasm.bims_is_recruited() !== 0;
    if (on === recruitedShown) return;
    recruitedShown = on;
    recruited.hidden = !on;
  }

  function paintHealth() {
    const points = wasm.bims_health();
    const most = wasm.bims_max_health();
    const share = Math.round((points / most) * 100);
    healthBar.style.width = `${share}%`;
    healthPct.textContent = `${Math.round(points)}`;

    const stage = wasm.bims_malnutrition();
    const tired = wasm.bims_drowsiness();
    const alive = wasm.bims_is_alive() !== 0;
    const text = alive ? CONDITIONS[stage] ?? "" : "The Bim has died.";
    if (text !== conditionShown) {
      conditionShown = text;
      condition.textContent = text;
      condition.className = !alive ? "gone" : stage > 0 ? "warn" : "";
      healthBox.className = stage >= 3 || !alive ? "hurt" : "";
    }

    const drowsy = alive ? DROWSINESS[tired] ?? "" : "";
    if (drowsy !== drowsyShown) {
      drowsyShown = drowsy;
      drowsiness.textContent = drowsy;
      drowsiness.className = tired > 0 ? "warn" : "";
    }

    // Both of these are stages reached by a clock rather than levels, so the
    // bars above cannot show them: a Bim at nothing per cent on the restroom
    // bar is either fidgeting or about to have an accident, and which it is
    // only the line says.
    const urgeStage = alive ? wasm.bims_urge() : 0;
    const urge = URGES[urgeStage] ?? "";
    if (urge !== urgeShown) {
      urgeShown = urge;
      urgeLine.textContent = urge;
      urgeLine.className = urgeStage >= 2 ? "warn" : "";
    }

    const mess = alive ? wasm.bims_discomfort() : 0;
    const text2 = DISCOMFORTS[mess] ?? "";
    if (text2 !== discomfortShown) {
      discomfortShown = text2;
      discomfortLine.textContent = text2;
      discomfortLine.className = mess >= 2 ? "warn" : "";
    }
  }

  buildNeeds();
  // Every one of these hangs off the word that names it, underlined so it is
  // plainly something you can ask about.
  explainWord(healthBox.querySelector(".row .name"), HEALTH_TIP);
  explainWord(document.querySelector("#auto-control span"), AUTONOMY_TIP);
  explainWord(document.querySelector("#speed-control span"), SPEED_TIP);
  explainWord(document.querySelector("#food-control span"), FOOD_TIP);

  // --- the agenda -------------------------------------------------------
  //
  // One row per chain: the one running, then the ones waiting behind it.
  // Rows only change when the Bim takes something else on, so they are rebuilt
  // on that and nothing else; the bars move every frame.

  let agendaShape = "";
  let agendaRows = [];

  function buildAgenda(jobs) {
    agendaRows = jobs.map((job, i) => {
      const li = document.createElement("li");
      if (wasm.bims_agenda_active(i)) li.className = "active";

      const bar = document.createElement("span");
      bar.className = "bar";
      const fill = document.createElement("i");
      bar.appendChild(fill);

      const box = document.createElement("span");
      box.className = "box";

      const name = document.createElement("span");
      name.className = "name";
      name.textContent = JOB_NAMES[job] ?? "Busy";

      const pct = document.createElement("span");
      pct.className = "pct";

      li.append(bar, box, name, pct);
      return { li, fill, pct };
    });
    agenda.replaceChildren(...agendaRows.map((r) => r.li));
  }

  function paintAgenda() {
    const count = wasm.bims_agenda_len();
    if (count === 0) {
      agenda.hidden = true;
      agendaShape = "";
      agendaRows = [];
      return;
    }

    const jobs = [];
    for (let i = 0; i < count; i++) jobs.push(wasm.bims_agenda_job(i));
    const shape = `${jobs.join(",")}|${wasm.bims_agenda_active(0)}`;
    if (shape !== agendaShape) {
      agendaShape = shape;
      buildAgenda(jobs);
      agenda.hidden = false;
    }

    for (let i = 0; i < agendaRows.length; i++) {
      const done = Math.round(wasm.bims_agenda_progress(i) * 100);
      agendaRows[i].fill.style.width = `${done}%`;
      agendaRows[i].pct.textContent = `${done}%`;
    }
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

  // --- the tray ---------------------------------------------------------
  //
  // Bottom-left, two tabs, and it folds away. The schedule tab paints a slot
  // per hour; the management tab holds the controls that used to sit in the
  // page header.

  const tray = document.getElementById("tray");
  const trayToggle = document.getElementById("tray-toggle");
  const hours = document.getElementById("hours");

  trayToggle.addEventListener("click", () => {
    const open = tray.className !== "open";
    tray.className = open ? "open" : "";
    trayToggle.textContent = open ? "▾" : "▴";
    trayToggle.setAttribute("aria-expanded", String(open));
    trayToggle.title = open ? "Hide the panel" : "Show the panel";
  });

  for (const tab of document.querySelectorAll("#tray .tab")) {
    tab.addEventListener("click", () => {
      for (const other of document.querySelectorAll("#tray .tab")) {
        other.className = other === tab ? "tab on" : "tab";
      }
      for (const panel of document.querySelectorAll("#tray [data-panel]")) {
        panel.hidden = panel.dataset.panel !== tab.dataset.tab;
      }
      // Folding it shut and picking a tab should both leave it open.
      tray.className = "open";
    });
  }

  // --- painting the day -------------------------------------------------

  let brush = 1;
  for (const button of document.querySelectorAll("#brushes .brush")) {
    button.addEventListener("click", () => {
      brush = Number(button.dataset.slot);
      for (const other of document.querySelectorAll("#brushes .brush")) {
        other.className = other === button ? "brush on" : "brush";
      }
    });
  }

  const slots = [];
  let painting = false;

  // Not `paint`: the canvas renderer already has that name in this scope, and
  // a second declaration of it would quietly win.
  function setHour(hour) {
    wasm.bims_set_schedule_slot(hour, brush);
    paintHours();
  }

  function buildHours() {
    const count = wasm.bims_schedule_hours();
    for (let hour = 0; hour < count; hour++) {
      const cell = document.createElement("li");
      cell.textContent = String(hour);
      cell.title = `${String(hour).padStart(2, "0")}:00`;
      cell.addEventListener("pointerdown", (e) => {
        e.preventDefault();
        painting = true;
        setHour(hour);
      });
      // Dragging across the strip paints the whole run in one gesture.
      cell.addEventListener("pointerenter", () => {
        if (painting) setHour(hour);
      });
      slots.push({ cell, shown: null, now: false });
    }
    hours.replaceChildren(...slots.map((s) => s.cell));
    // How the strip works, asked for rather than sitting under it in a
    // paragraph: a "?" at the end of the brush row, beside Sleep and
    // Everything. The percentage comes from wasm, so the explanation cannot
    // drift from the rule it is explaining.
    const above = Math.round(wasm.bims_schedule_ignore_above() * 100);
    const brushes = document.getElementById("brushes");
    if (brushes && !brushes.querySelector(".what")) {
      brushes.appendChild(
        questionMark(
          `Paint the hours the Bim should be asleep. It goes to bed when one comes round — unless it is already more than ${above}% rested, in which case it ignores that one — and gets up as soon as it is fully rested.`,
          "What the schedule does",
        ),
      );
    }
  }

  window.addEventListener("pointerup", () => {
    painting = false;
  });

  function paintHours() {
    const now = Math.floor(wasm.bims_clock_minutes() / 60) % slots.length;
    for (let hour = 0; hour < slots.length; hour++) {
      const slot = slots[hour];
      const asleep = wasm.bims_schedule_slot(hour) === 1;
      const here = hour === now;
      if (asleep === slot.shown && here === slot.now) continue;
      slot.shown = asleep;
      slot.now = here;
      slot.cell.className = `${asleep ? "sleep" : ""}${here ? " now" : ""}`.trim();
    }
  }

  buildHours();

  // --- letting the Bim get on with it -----------------------------------

  autonomy.checked = wasm.bims_is_autonomous() !== 0;
  autonomy.addEventListener("change", () => {
    wasm.bims_set_autonomous(autonomy.checked ? 1 : 0);
  });

  // --- what the place keeps in stock --------------------------------------
  //
  // The manager's one number so far. It is typed in food units and wasm does
  // the dividing, so the split shown here is the split the bay is actually
  // planting to rather than the same arithmetic written out twice.

  const foodTarget = document.getElementById("food-target");
  const foodSplit = document.getElementById("food-split");

  function showFoodTarget() {
    foodTarget.value = String(wasm.bims_food_target());
    foodSplit.textContent = `= ${wasm.bims_target_veg()} veg + ${wasm.bims_target_tofu()} tofu`;
  }

  foodTarget.max = String(wasm.bims_food_target_max());
  foodTarget.addEventListener("input", () => {
    const asked = Number.parseInt(foodTarget.value, 10);
    if (Number.isNaN(asked)) return;
    wasm.bims_set_food_target(Math.max(0, asked));
    // Read it back rather than echoing what was typed: wasm clamps, and the
    // split is its arithmetic.
    foodSplit.textContent = `= ${wasm.bims_target_veg()} veg + ${wasm.bims_target_tofu()} tofu`;
  });
  // A field left mid-edit ("", "0012") is tidied to whatever wasm holds.
  foodTarget.addEventListener("change", showFoodTarget);
  showFoodTarget();

  // --- what is aboard ------------------------------------------------------
  //
  // One row per thing: what it is, how many there are, and where they are.
  // Everything automated will want a row here eventually, so the table is
  // built from a list rather than written out in the markup — adding a thing
  // is adding a line to `stockRows`.

  const stockBody = document.querySelector("#stock tbody");
  const stockRows = [
    ["Vegetables", () => wasm.bims_store_veg(), "Cold store"],
    ["Tofu", () => wasm.bims_store_tofu(), "Cold store"],
    ["Stew", () => wasm.bims_pot_servings(), "Pot on the hob"],
  ];
  const stockCells = stockRows.map(([name, , where]) => {
    const tr = document.createElement("tr");
    const item = document.createElement("td");
    item.textContent = name;
    const qty = document.createElement("td");
    qty.className = "qty";
    const place = document.createElement("td");
    place.className = "where";
    place.textContent = where;
    tr.append(item, qty, place);
    stockBody.appendChild(tr);
    return qty;
  });

  function paintStock() {
    for (let i = 0; i < stockRows.length; i++) {
      const held = String(stockRows[i][1]());
      if (stockCells[i].textContent !== held) stockCells[i].textContent = held;
    }
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
    } else if (e.key === "r" || e.key === "R") {
      wasm.bims_toggle_recruited();
      paintRecruited();
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
    const resting = wasm.bims_rest_left();
    paintRecruited();
    paintHours();
    paintNeeds();
    paintHealth();
    paintAgenda();
    paintStock();

    // A stall looks like the game has hung unless it says what it is.
    const doing = wasm.bims_is_napping()
      ? "Dropped off where it stands…"
      : wasm.bims_is_stalled()
        ? "Lost the thread of it…"
        : ACTIVITY[wasm.bims_activity()];
    const refusing = refusal !== null && now < refusalUntil;
    const state = refusing
      ? refusal
      : wasm.bims_is_alive() === 0
      ? "The Bim has died."
      : resting > 0
      ? `In bed · ${spanText(resting)} to go, up at ${clockText(minutes + resting)}`
      : doing
        ? doing
        : selected
          ? "Bim selected — right-click the floor to send it there"
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
