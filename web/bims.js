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
const HIT_LOCKER = 8;

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
  Rest: "Runs down all day. The timetable is what sends the Bim to bed on an ordinary night — the level only decides whether a scheduled night is worth taking. The trigger under the timetable is the floor beneath that: past it the Bim turns in whatever the hour, unless a meal or the heads comes first.",
  Food: "Past its trigger — a tenth, until you move it — the Bim goes and cooks itself a meal. Empty for eight hours and malnutrition sets in; a day of it is fatal.",
  Restroom:
    "Under 10% the Bim takes itself to the toilet. If it cannot — shut in, under orders — it fidgets, then risks wetting itself, and an hour after the bar empties it has an accident.",
  Cleanliness:
    "Not a clock like the others: it follows the mess within three tiles of the Bim and whatever the Bim has on itself. At nothing it treads carefully, then keeps away from the mess, then is sick in it every half hour.",
  Socializing:
    "The one need that wants another Bim rather than a fixture. Past its trigger the Bim goes and finds the other one, and they stand and talk about whatever they have been doing. This bar is the comfortable end of it; what matters is the count of days underneath, because going without runs on a far longer clock — three days alone and a Bim is low and slow, five and it sits down on the deck, seven and it starts hurting itself.",
};

const HEALTH_TIP =
  "Only the worst stage of malnutrition actually costs health, and eating properly walks it back. The lines underneath name whatever is wrong.";

const AUTONOMY_TIP =
  "Off, the Bim starts nothing by itself — no meals, no sleep, no trips to the toilet — but still does everything it is told. The levels carry on moving either way.";

const SPEED_TIP =
  "How fast the simulation runs. At 24x a whole game day goes by in about a minute.";

const FOOD_TIP =
  "How much food the place keeps in stock, in units of two thirds vegetables to one third tofu — ask for 99 and it holds 66 and 33. The hydroponic bay plants to this and sleeps once it is met.";

/** Who is aboard, in the order wasm indexes them.
 *
 * The names live here and only here. No strings cross the wasm boundary, so
 * the simulation knows crew member 0 and crew member 1 and nothing else about
 * them; James and Kate are the host's business, the same as "Cold store" and
 * "Making food" are. */
const CREW_NAMES = ["James", "Kate"];

/** Months of the ship's calendar. Twelve of them and no leap years — see
 * `src/clock.rs`, which does the arithmetic; these are only the words. */
const MONTH_NAMES = [
  "January",
  "February",
  "March",
  "April",
  "May",
  "June",
  "July",
  "August",
  "September",
  "October",
  "November",
  "December",
];

/** How a Bim says each thing it remembers, by the code from `src/memory.rs`.
 *
 * First person, because it is its diary. `d` is the one detail that came with
 * the entry — a stage, which of the crew — and each of these is free to
 * ignore it.
 *
 * Only things that actually went wrong are in here, because only those are
 * written down any more: the ordinary run of a day left a wall of "Went to the
 * heads." to scroll past. An entry with no line here is dropped from the page
 * rather than padded out — see `paintDiary`. */
const MEMORY_LINES = {
  20: (d) =>
    d === 1
      ? "Could not hold it. I would rather not talk about it."
      : "Did not quite make it to the heads.",
  21: () => "Was sick on the deck.",
  22: () => "Dropped off where I was standing.",
  23: (d) =>
    [
      "",
      "Getting hungry. Properly hungry.",
      "I have not eaten in a long time.",
      "I am starving. I can feel it in my hands.",
    ][d] ?? "Going hungry.",
  24: (d) =>
    [
      "",
      "Tired. I should sleep.",
      "I have not slept in far too long.",
      "I cannot keep my eyes open.",
    ][d] ?? "Going without sleep.",
  25: (d) => `Saw ${CREW_NAMES[d] ?? "one of the crew"} have an accident.`,
  26: (d) => `Saw ${CREW_NAMES[d] ?? "one of the crew"} being sick.`,
  27: (d) =>
    d >= 7
      ? "Nobody has spoken to me in a week."
      : d >= 5
        ? "The quiet is starting to get to me."
        : "Feeling low. It has been a few days since anyone said anything.",
  28: () => "Sat down on the deck and could not get up for a while.",
  29: (d) => `Hurt myself. ${d} points of it.`,
  30: (d) => `${CREW_NAMES[d] ?? "One of the crew"} died today.`,
};

/** What a Bim says it is talking about, by the code that came back from
 * `bims_chat_topic`. Third person and short: this goes in a bubble over its
 * head, not in its diary, so it has to fit.
 *
 * **Two code spaces, and they do not overlap.** Small talk — what the Bim has
 * actually been doing — comes back as a `JOB_` code, which runs from 1. The
 * things that happened *to* it come out of its diary as a `memory::What` code,
 * which starts at 20. Keeping the table flat over both is what lets a
 * conversation move between "the sweeping" and "an accident" without the
 * simulation having to say which sort of thing it is handing over.
 *
 * The wording is deliberately not derived from `MEMORY_LINES`: "Was sick on
 * the deck." is a diary entry, and "being sick" is what you say about it. */
const CHAT_TOPICS = {
  1: "cooking",
  2: "the cooker",
  3: "that nap",
  4: "the night",
  5: "the heads",
  6: "the fridge",
  7: "that door",
  8: "the lock",
  9: "the dishwasher",
  10: "cooking",
  11: "the bay",
  12: "leftovers",
  13: "the sweeping",
  20: "an accident",
  21: "being sick",
  22: "dropping off",
  23: "being hungry",
  24: "being tired",
  25: "what happened",
  26: "what happened",
  27: "how it has been",
  28: "a bad day",
  29: "a bad day",
  30: "the one who died",
};

/** What a Bim with nobody to talk to has in the bubble. Nothing to report is
 * still a conversation. */
const CHAT_NOTHING = "nothing much";

/** The needs, in the order wasm indexes them. */
const NEED_NAMES = ["Rest", "Food", "Restroom", "Cleanliness", "Socializing"];

/** Which of those get a trigger in the schedule tab. Restroom and cleanliness
 * are left alone: the first is not something a player should be able to talk
 * the Bim out of, and the second has no errand behind it to start. */
const TRIGGER_NEEDS = [0, 1];

const TRIGGER_TIP =
  "How low a need may get before the Bim breaks off and does something about it: past the mark on a row it takes itself to bed, or goes and cooks, on its own account. The timetable above says when it may sleep; the Rest threshold is the floor under that — past it the Bim turns in whatever the hour, unless a meal or the heads comes first. Untick one and that need still runs down and still tells on the Bim; only the errand stops.";

/** What `bims_spot_at` says is under the pointer. Must match the `SPOT_`
 * codes in src/room.rs. */
const SPOT_NAMES = [
  "Outside the hull",
  "Deck plating",
  "Bulkhead",
  "Worktop",
  "Chopping board",
  "Cold store",
  "Hob",
  "Dishwasher",
  "Table",
  "Chair",
  "Bunk",
  "Hydroponic bay",
  "Toilet",
  "Washbasin",
  "Bathroom door",
  "Deck plating",
  "Broom locker",
];

/** The few of those the panels point at by name. Indices into SPOT_NAMES, so
 * they are the same codes `bims_set_highlight` takes. */
const SPOT_NOTHING = 0;
const SPOT_FRIDGE = 5;
const SPOT_HOB = 6;
const SPOT_BAY = 11;
const SPOT_LOCKER = 16;

/** Which of those are deck: the only ones a mess can be lying on. The filth
 * grid covers the whole room, so asking about the tile under the fridge would
 * report whatever was spilt on the floor the fridge is standing on. */
const DECK_SPOTS = new Set([1, 15]);

/** What is on the deck there, by the code from `bims_spot_mess`. Ordered
 * least bad first, the same as `filth::Mess`, and indexed by the code — so a
 * new kind of mess goes in at its own rank rather than on the end. */
const MESS_NAMES = ["", "Grime", "Wet", "Soiled", "Vomit"];

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

/** And for want of anybody to talk to. Days, not hours: this one is slow, and
 * saying so is the only warning a player gets before it turns serious. */
const LONELINESS = [
  "",
  "Desocialized — low, and a tenth slower at everything",
  "Badly desocialized — sits down on the deck every few hours",
  "Isolated — hurting itself, and past ten days it may stop altogether",
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
  13: "Sweeping up",
  14: "Talking",
};

/** The same errands as the status line says them. A lie-down is left out: the
 * countdown from bims_rest_left() is more use than the name. */
const ACTIVITY = {
  1: "Making food…",
  2: "Off to the cooker…",
  5: "Using the toilet — a wash to follow",
  11: "In the hydroponics…",
  12: "Helping itself to the pot…",
  13: "Sweeping the deck…",
  14: "Having a word with the other one…",
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

/** The name over a Bim's head: how big, and how far above the body it sits.
 * The lift is in room units and scaled with the view, so the name stays over
 * the head at any window size; the size is in CSS pixels, because type that
 * scaled with the room would go illegible on a small window. */
const NAME_SIZE = 12;
const NAME_LIFT = 46;
/** The player's own, in the colour everything steerable uses, and the rest of
 * the crew in plain ink. */
/// The bubble over a talking Bim. Sits above the name, so the two do not
/// fight, and in the panels' own colours so it reads as part of the interface
/// rather than as something painted on the deck.
const BUBBLE_SIZE = 12;
const BUBBLE_LIFT = 68;
const BUBBLE_PAD = 8;
const BUBBLE_FILL = "rgba(20, 29, 25, 0.94)";
const BUBBLE_EDGE = "rgba(127, 209, 168, 0.55)";
const BUBBLE_INK = "#e7efe9";

const NAME_YOURS = "#7fd1a8";
const NAME_THEIRS = "#e7efe9";

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
  const agendas = document.getElementById("agendas");
  const side = document.getElementById("side");
  const recruited = document.getElementById("recruited");
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

  /** Who has the run of a shared part of the ship, or null for nobody.
   *
   * wasm counts from 1 so that 0 can mean "free"; this turns that back into a
   * name, and into null when it is the player's own Bim — being told you
   * cannot cook because you are already cooking is no help. */
  function heldBy(code) {
    if (code === 0 || code - 1 === player) return null;
    return crewName(code - 1);
  }

  /** Build a dropdown for the fixture that was clicked, at the cursor. */
  function openMenu(fixture, event) {
    if (wasm.bims_is_alive(player) === 0) return;
    const busy = wasm.bims_is_busy(player) !== 0;
    const items = [];
    // Nothing here is remote: every item walks the Bim over to do it by hand.
    // Nor does anything wait for the Bim to be free — a new errand takes over,
    // and what it displaced goes on the agenda to be finished afterwards.
    //
    // Every menu acts on the player's Bim. The other crew take no orders at
    // all, which is why there is no "whose" here to choose.
    const takesOver = busy ? "takes over — the rest waits its turn" : null;
    // One pot, one board, one pan: while somebody else's hands are in them the
    // errand will not start, so say whose rather than leaving an item that
    // quietly does nothing.
    const galley = heldBy(wasm.bims_galley_held_by());
    const heads = heldBy(wasm.bims_heads_held_by());

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
        hint: galley
          ? `${galley} is in the galley`
          : noStew
            ? "needs two vegetables"
            : (takesOver ?? "two vegetables, chopped and cooked"),
        disabled: noStew || galley !== null,
        run: () => wasm.bims_make_stew(),
      });
      items.push({
        label: "Make a bowl",
        hint: galley
          ? `${galley} is in the galley`
          : noBowl
            ? "needs a block of tofu and a salad"
            : (takesOver ?? "tofu chopped in with the salad, no cooking"),
        disabled: noBowl || galley !== null,
        run: () => wasm.bims_make_bowl(),
      });
      items.push({
        label: wasm.bims_fridge_is_open() ? "Close door" : "Open door",
        hint: galley
          ? `${galley} is in the galley`
          : (takesOver ?? "the Bim walks over to it"),
        disabled: galley !== null,
        run: () => wasm.bims_toggle_fridge(),
      });
    } else if (fixture === HIT_STOVE) {
      // The pot lives on the hob, so what is left in it belongs on this menu.
      const left = wasm.bims_pot_servings();
      if (left > 0) {
        items.push({
          label: "Eat from the pot",
          hint: galley
            ? `${galley} is in the galley`
            : (takesOver ??
              `${left} of ${wasm.bims_pot_capacity()} helpings left — no cooking`),
          disabled: galley !== null,
          run: () => wasm.bims_eat_leftovers(),
        });
      }
      // A hob left lit shuts itself off; say so rather than letting it look
      // like the game changed its mind.
      const idleLeft = wasm.bims_stove_idle_left();
      items.push({
        label: wasm.bims_stove_is_on() ? "Turn off" : "Turn on",
        hint: galley
          ? `${galley} is in the galley`
          : (takesOver ??
            (idleLeft > 0
              ? `left on — cuts out in ${spanText(idleLeft)}`
              : "the Bim walks over to it")),
        disabled: galley !== null,
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
        hint: galley
          ? `${galley} is in the galley`
          : left > 0
            ? "already running"
            : loaded === 0
              ? "nothing in it"
              : (takesOver ??
                `${loaded} of ${capacity} stowed — the Bim goes and presses it`),
        disabled: left > 0 || loaded === 0 || galley !== null,
        run: () => wasm.bims_run_dishwasher(),
      });
    } else if (fixture === HIT_TOILET) {
      // A locked door only stops a Bim on the wrong side of it, so ask whether
      // this one can actually get there rather than whether the door is shut.
      const canUse = wasm.bims_can_use_toilet() !== 0;
      items.push({
        label: "Use",
        hint: heads
          ? `${heads} is in there`
          : canUse
            ? (takesOver ?? "and wash at the basin after")
            : "can't get to it — the door is locked",
        disabled: !canUse || heads !== null,
        run: () => wasm.bims_use_toilet(),
      });
    } else if (fixture === HIT_DOOR) {
      const open = wasm.bims_door_is_open() !== 0;
      const locked = wasm.bims_door_is_locked() !== 0;
      items.push({
        label: open ? "Close door" : "Open door",
        hint: heads
          ? `${heads} is in there`
          : locked
            ? "unlock it first"
            : (takesOver ?? "the Bim walks over to it"),
        disabled: locked || heads !== null,
        run: () => wasm.bims_toggle_door(),
      });
      items.push({
        label: locked ? "Unlock" : "Lock",
        hint: heads
          ? `${heads} is in there`
          : (takesOver ?? (locked ? "at the panel" : "shuts it as well")),
        disabled: heads !== null,
        run: () => wasm.bims_toggle_door_lock(),
      });
    } else if (fixture === HIT_LOCKER) {
      // The one errand that undoes a mess. The Bim gets round to it by itself
      // when it has nothing else on; this is for when you would rather it did
      // so now.
      const dirty = wasm.bims_dirty_tiles();
      const broom = heldBy(wasm.bims_broom_held_by());
      items.push({
        label: "Sweep up",
        hint: broom
          ? `${broom} has the broom`
          : dirty === 0
            ? "the deck is clean"
            : (takesOver ??
              `${dirty} patch${dirty === 1 ? "" : "es"} of deck want it`),
        disabled: dirty === 0 || broom !== null,
        run: () => wasm.bims_sweep_up(),
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

  // --- what the crew want -------------------------------------------------
  //
  // One panel per crew member, always on show: these are what set everything
  // else going, so watching a bar run down is watching the next errand arrive.
  // The rows never change, only their widths.
  //
  // Kate gets a panel even though the player cannot order her about. That is
  // the point of showing it: the first you should hear of her going hungry is
  // her bar, not her lying on the deck.

  const crew = [];

  /** The name of crew member `who`. Nothing but the host knows these — no
   * strings cross the wasm boundary, so wasm has crew 0 and crew 1. */
  function crewName(who) {
    return CREW_NAMES[who] ?? `Crew ${who + 1}`;
  }

  /** The header that says whose panel this is. */
  function whoHeader(who) {
    const head = document.createElement("div");
    head.className = who === player ? "who yours" : "who";
    const tag = document.createElement("span");
    tag.className = "tag";
    tag.textContent = crewName(who);
    const note = document.createElement("span");
    note.className = "aside";
    note.textContent = who === player ? "yours" : "her own";
    head.append(tag, note);
    return head;
  }

  function buildCrew() {
    const count = wasm.bims_need_count();
    for (let who = 0; who < crewCount; who++) {
      const column = document.createElement("section");
      column.className = "crew";

      const needs = document.createElement("ul");
      needs.className = "needs";
      needs.setAttribute("aria-label", `What ${crewName(who)} wants`);

      const rows = [];
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
        // Only the first panel's words are affordances. The explanation is
        // the same for both and two sets of underlines down one edge of the
        // screen is noise, not help.
        if (who === 0) explainWord(name, NEED_TIPS[name.textContent]);
        rows.push({ li, fill, pct, urgent: false });
        needs.appendChild(li);
      }

      const health = document.createElement("div");
      health.className = "health";
      health.setAttribute("aria-label", `How ${crewName(who)} is bearing up`);
      const row = document.createElement("div");
      row.className = "row";
      const hname = document.createElement("span");
      hname.className = "name";
      hname.textContent = "Health";
      const hbar = document.createElement("span");
      hbar.className = "bar";
      const hfill = document.createElement("i");
      hbar.appendChild(hfill);
      const hpct = document.createElement("span");
      hpct.className = "pct";
      row.append(hname, hbar, hpct);
      if (who === 0) explainWord(hname, HEALTH_TIP);

      const lines = {};
      for (const kind of [
        "condition",
        "drowsiness",
        "urge",
        "discomfort",
        "loneliness",
      ]) {
        const p = document.createElement("p");
        p.className = kind;
        lines[kind] = { el: p, kind, shown: null };
      }
      health.append(row, ...Object.values(lines).map((l) => l.el));

      const sheet = buildSheet(who);

      column.className = who === player ? "crew" : "crew theirs";
      column.hidden = true;
      column.append(whoHeader(who), needs, health, sheet.el);
      side.appendChild(column);
      crew.push({ who, column, rows, health, hfill, hpct, lines, sheet });
    }
  }

  // --- who they are, and what they remember -------------------------------
  //
  // The character sheet: a tabbed panel under the bars, laid out the way the
  // tray at the bottom left is, because it is the same kind of thing — a few
  // pages of detail you open when you want them rather than a readout that
  // has to be watched.

  function buildSheet(who) {
    const el = document.createElement("section");
    el.className = "sheet";
    el.setAttribute("aria-label", `About ${crewName(who)}`);

    const tabs = document.createElement("div");
    tabs.className = "sheet-tabs";
    const body = document.createElement("div");
    body.className = "sheet-body";

    const pages = {};
    for (const [key, label] of [
      ["about", "About"],
      ["memory", "Memory"],
    ]) {
      const tab = document.createElement("button");
      tab.type = "button";
      tab.className = key === "about" ? "tab on" : "tab";
      tab.dataset.sheetTab = key;
      tab.textContent = label;
      tabs.appendChild(tab);

      const page = document.createElement("div");
      page.className = key === "about" ? "about" : "diary";
      page.dataset.sheet = key;
      page.hidden = key !== "about";
      body.appendChild(page);
      pages[key] = page;

      tab.addEventListener("click", () => {
        for (const other of tabs.children) {
          other.className = other === tab ? "tab on" : "tab";
        }
        for (const p of body.children) {
          p.hidden = p.dataset.sheet !== key;
        }
      });
    }

    // The About page never changes after the first paint — a name, a birthday
    // and an age, and only the last of those can move, once a year.
    const about = {};
    for (const [key, label] of [
      ["name", "Name"],
      ["age", "Age"],
      ["born", "Born"],
    ]) {
      const row = document.createElement("div");
      row.className = "row";
      const l = document.createElement("span");
      l.className = "label";
      l.textContent = label;
      const v = document.createElement("span");
      v.className = "value";
      row.append(l, v);
      pages.about.appendChild(row);
      about[key] = { el: v, shown: null };
    }

    el.append(tabs, body);
    return { el, pages, about, diaryShown: "" };
  }

  /** A date the way a person would write it. */
  function dateText(date, month, year) {
    return `${date} ${MONTH_NAMES[month - 1] ?? "?"} ${year}`;
  }

  function paintSheet(panel) {
    const { who, sheet } = panel;
    set(sheet.about.name, crewName(who));
    set(sheet.about.age, `${wasm.bims_age(who)}`);
    set(
      sheet.about.born,
      dateText(
        wasm.bims_born_date(who),
        wasm.bims_born_month(who),
        wasm.bims_born_year(who),
      ),
    );

    // The diary is rebuilt only when it has actually grown. It is a list of
    // paragraphs, and rebuilding it every frame would fight the scrollbar the
    // player is holding.
    const count = wasm.bims_memory_len(who);
    const shape = `${count}`;
    if (shape === sheet.diaryShown) return;
    sheet.diaryShown = shape;
    paintDiary(sheet.pages.memory, who, count);
  }

  function set(field, text) {
    if (text === field.shown) return;
    field.shown = text;
    field.el.textContent = text;
  }

  /** The Bim's own account of its days: newest day first, and within a day in
   * the order it happened. */
  function paintDiary(page, who, count) {
    // An empty page is the *good* outcome now, not an early-game one: the
    // diary keeps only what went wrong, so a crew getting on with their work
    // writes nothing at all. Worded so that reads as reassurance rather than
    // as the panel not having loaded.
    if (count === 0) {
      const p = document.createElement("p");
      p.className = "nothing";
      p.textContent = "Nothing has gone wrong.";
      page.replaceChildren(p);
      return;
    }

    // Gather into days first, then emit newest day at the top. wasm hands
    // them over oldest first, which is the order they read in within a day.
    const days = new Map();
    for (let i = 0; i < count; i++) {
      const day = wasm.bims_memory_day(who, i);
      if (!days.has(day)) days.set(day, []);
      days.get(day).push({
        at: wasm.bims_memory_at(who, i),
        what: wasm.bims_memory_what(who, i),
        detail: wasm.bims_memory_detail(who, i),
      });
    }

    const out = [];
    for (const day of [...days.keys()].sort((a, b) => b - a)) {
      // Built into a holding list first, because a day whose every entry
      // turned out to be unsayable must not leave its heading behind with
      // nothing under it.
      const said = [];
      for (const moment of days.get(day)) {
        // A code with no words for it is left out altogether rather than
        // padded with a placeholder. A diary line that says nothing is not a
        // line, and "Something happened." reads as the Bim having had a
        // mysterious experience rather than as the table being short an
        // entry. It shows up instead as a missing row, which is what
        // `scratchpad/smoke.mjs` counts.
        const words = MEMORY_LINES[moment.what]?.(moment.detail);
        if (!words) continue;
        const line = document.createElement("p");
        line.className = "line";
        const when = document.createElement("span");
        when.className = "when";
        when.textContent = clockText(moment.at);
        const text = document.createElement("span");
        text.textContent = words;
        line.append(when, text);
        said.push(line);
      }
      if (!said.length) continue;
      const head = document.createElement("p");
      head.className = "day";
      head.textContent = `Day ${day}`;
      out.push(head, ...said);
    }
    page.replaceChildren(...out);
  }

  function paintCrew() {
    const most = wasm.bims_max_health();
    for (const panel of crew) {
      const { who } = panel;
      // One crew sheet at a time, and only when somebody is picked. Kate's
      // bars are hers until you click on her: the right-hand side answers
      // "who am I looking at", not "what is everybody up to".
      const showing = wasm.bims_is_selected(who) !== 0;
      if (showing === panel.column.hidden) panel.column.hidden = !showing;
      if (!showing) continue;

      const alive = wasm.bims_is_alive(who) !== 0;

      for (let i = 0; i < panel.rows.length; i++) {
        const row = panel.rows[i];
        const level = wasm.bims_need_level(who, i);
        const done = Math.round(level * 100);
        row.fill.style.width = `${done}%`;
        row.pct.textContent = `${done}%`;
        // The trigger comes from wasm, per need, so the bar cannot disagree
        // with the behaviour it is meant to be predicting — including when
        // the player has moved it or switched it off in the schedule tab.
        const urgent =
          wasm.bims_need_trigger_on(i) !== 0 && level < wasm.bims_need_trigger(i);
        if (urgent !== row.urgent) {
          row.urgent = urgent;
          row.li.className = urgent ? "urgent" : "";
        }
      }

      const points = wasm.bims_health(who);
      panel.hfill.style.width = `${Math.round((points / most) * 100)}%`;
      panel.hpct.textContent = `${Math.round(points)}`;

      const stage = wasm.bims_malnutrition(who);
      const tired = wasm.bims_drowsiness(who);
      say(
        panel.lines.condition,
        alive ? CONDITIONS[stage] ?? "" : `${crewName(who)} has died.`,
        !alive ? "gone" : stage > 0 ? "warn" : "",
      );
      panel.health.className = stage >= 3 || !alive ? "health hurt" : "health";

      say(
        panel.lines.drowsiness,
        alive ? DROWSINESS[tired] ?? "" : "",
        tired > 0 ? "warn" : "",
      );

      // Both of these are stages reached by a clock rather than levels, so
      // the bars above cannot show them: a Bim at nothing per cent on the
      // restroom bar is either fidgeting or about to have an accident, and
      // which it is only the line says.
      const urgeStage = alive ? wasm.bims_urge(who) : 0;
      say(panel.lines.urge, URGES[urgeStage] ?? "", urgeStage >= 2 ? "warn" : "");

      // The mess is the deck's and shared; how long *this* one has been
      // standing in it is its own, so this reads per Bim. One that has just
      // come in from the heads is not as far gone as the one that has been
      // beside it for two hours.
      const mess = alive ? wasm.bims_discomfort(who) : 0;
      say(panel.lines.discomfort, DISCOMFORTS[mess] ?? "", mess >= 2 ? "warn" : "");

      // The same shape again on a far longer clock. The days are spelt out as
      // well as the stage, because four days alone and nine days alone look
      // identical on the bar — it is at nothing either way — and the
      // difference between them is the difference between low and in danger.
      const alone = alive ? wasm.bims_loneliness(who) : 0;
      const days = Math.floor(wasm.bims_days_alone(who));
      say(
        panel.lines.loneliness,
        alone ? `${LONELINESS[alone] ?? ""} · ${days} days` : "",
        alone >= 2 ? "warn" : "",
      );

      paintSheet(panel);
    }
  }

  /** Put `text` in a line, only when it has actually changed.
   *
   * The line keeps the class that says *which* line it is — the stylesheet
   * hangs the "empty lines collapse" rule off that — and `cls` is the state
   * on top of it. Writing only `cls` would take the identity away with it. */
  function say(line, text, cls) {
    if (text === line.shown) return;
    line.shown = text;
    line.el.textContent = text;
    line.el.className = `${line.kind} ${cls}`.trim();
  }

  let recruitedShown = null;

  // Being under orders outlasts everything else on the status line, so it gets
  // a badge of its own in the header rather than competing for that one line.
  function paintRecruited() {
    const on = wasm.bims_is_recruited() !== 0;
    if (on === recruitedShown) return;
    recruitedShown = on;
    recruited.hidden = !on;
  }

  buildCrew();
  // Somebody has to be picked to begin with, or the game opens with a blank
  // right-hand side and no hint that clicking a Bim is what fills it.
  wasm.bims_select_group(1);
  // These hang off the word that names them, underlined so they are plainly
  // something you can ask about.
  explainWord(document.querySelector("#auto-control span"), AUTONOMY_TIP);
  explainWord(document.querySelector("#speed-control span"), SPEED_TIP);
  explainWord(document.querySelector("#food-control span"), FOOD_TIP);

  // --- the agenda -------------------------------------------------------
  //
  // One row per chain: the one running, then the ones waiting behind it.
  // Rows only change when the Bim takes something else on, so they are rebuilt
  // on that and nothing else; the bars move every frame.

  // One per crew member. Each carries the name of whose it is, because two
  // lists of errands side by side with nothing to tell them apart is worse
  // than one list.
  const agendaFor = [];

  function buildAgendas() {
    for (let who = 0; who < crewCount; who++) {
      const head = whoHeader(who);
      const list = document.createElement("ul");
      list.className = "agenda";
      list.setAttribute("aria-label", `What ${crewName(who)} is doing`);
      const box = document.createElement("div");
      box.hidden = true;
      box.append(head, list);
      agendas.appendChild(box);
      agendaFor.push({ who, box, list, shape: "", rows: [] });
    }
  }

  function buildAgendaRows(panel, jobs) {
    panel.rows = jobs.map((job, i) => {
      const li = document.createElement("li");
      if (wasm.bims_agenda_active(panel.who, i)) li.className = "active";

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
    panel.list.replaceChildren(...panel.rows.map((r) => r.li));
  }

  function paintAgendas() {
    for (const panel of agendaFor) {
      const count = wasm.bims_agenda_len(panel.who);
      if (count === 0) {
        panel.box.hidden = true;
        panel.shape = "";
        panel.rows = [];
        continue;
      }

      const jobs = [];
      for (let i = 0; i < count; i++) {
        jobs.push(wasm.bims_agenda_job(panel.who, i));
      }
      const shape = `${jobs.join(",")}|${wasm.bims_agenda_active(panel.who, 0)}`;
      if (shape !== panel.shape) {
        panel.shape = shape;
        buildAgendaRows(panel, jobs);
        panel.box.hidden = false;
      }

      for (let i = 0; i < panel.rows.length; i++) {
        const done = Math.round(wasm.bims_agenda_progress(panel.who, i) * 100);
        panel.rows[i].fill.style.width = `${done}%`;
        panel.rows[i].pct.textContent = `${done}%`;
      }
    }
  }

  buildAgendas();

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
    // Folding the tray away takes the row the pointer was on with it, and no
    // `pointerleave` follows.
    ringSpot(SPOT_NOTHING);
    const open = tray.className !== "open";
    tray.className = open ? "open" : "";
    trayToggle.textContent = open ? "▾" : "▴";
    trayToggle.setAttribute("aria-expanded", String(open));
    trayToggle.title = open ? "Hide the panel" : "Show the panel";
  });

  for (const tab of document.querySelectorAll("#tray .tab")) {
    tab.addEventListener("click", () => {
      // Same again: the panel the pointer was over is about to be hidden.
      ringSpot(SPOT_NOTHING);
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

  // --- when the Bim sees to itself --------------------------------------
  //
  // One row per need that has an errand behind it: a tick box for whether the
  // Bim watches that need at all, and a slider for the level it acts on. Both
  // are read back out of wasm rather than echoed, because wasm clamps.
  //
  // These sit under the timetable because they are the other half of the same
  // question. The strip says when the Bim *may* sleep; the Rest row says when
  // it has gone long enough without that it should go anyway.

  const thresholds = document.getElementById("thresholds");
  const triggerRows = [];

  function buildTriggers() {
    const head = document.createElement("div");
    head.className = "head";
    const label = document.createElement("span");
    label.textContent = "Action threshold";
    head.append(label, questionMark(TRIGGER_TIP, "What an action threshold is"));

    for (const i of TRIGGER_NEEDS) {
      const row = document.createElement("div");
      row.className = "trigger";

      const box = document.createElement("input");
      box.type = "checkbox";
      box.id = `trigger-on-${i}`;

      // A label rather than a bare span, so clicking the word works the tick
      // box — and so the word is not itself an affordance, which would put a
      // tooltip somewhere the player was only crossing. The "?" above speaks
      // for the whole block.
      const name = document.createElement("label");
      name.className = "name";
      name.htmlFor = box.id;
      name.textContent = NEED_NAMES[i] ?? `Need ${i + 1}`;

      const slider = document.createElement("input");
      slider.type = "range";
      slider.min = "0";
      slider.max = "100";
      slider.step = "1";
      slider.id = `trigger-at-${i}`;
      slider.setAttribute("aria-label", `${name.textContent} trigger level`);

      const out = document.createElement("output");
      out.htmlFor = slider.id;

      box.addEventListener("change", () => {
        wasm.bims_set_need_trigger_on(i, box.checked ? 1 : 0);
        showTriggers();
      });
      slider.addEventListener("input", () => {
        wasm.bims_set_need_trigger(i, Number(slider.value) / 100);
        showTriggers();
      });

      row.append(box, name, slider, out);
      triggerRows.push({ need: i, row, box, slider, out, on: null });
    }

    thresholds.replaceChildren(head, ...triggerRows.map((t) => t.row));
    showTriggers();
  }

  function showTriggers() {
    for (const t of triggerRows) {
      const on = wasm.bims_need_trigger_on(t.need) !== 0;
      const at = Math.round(wasm.bims_need_trigger(t.need) * 100);
      t.box.checked = on;
      t.slider.value = String(at);
      t.out.textContent = `${at}%`;
      if (on !== t.on) {
        t.on = on;
        t.row.className = on ? "trigger" : "trigger off";
      }
    }
  }

  buildTriggers();

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

  // The target is the bay's orders, so resting on it rings the bay.
  points(document.getElementById("food-control"), SPOT_BAY);

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

  // --- pointing at the thing itself ---------------------------------------
  //
  // A panel that names a place should be able to show you the place. Resting
  // on a management row rings the actual fixture on the deck, so "Cold store"
  // and "Pot on the hob" do not have to be matched against the furniture by
  // eye — which is a real problem in a room where several grey rectangles
  // stand against the same wall.
  //
  // This is not a tooltip and does not go through `explain`: nothing pops up,
  // nothing is said, and a pointer crossing the panel on its way somewhere
  // else lights a fixture for a moment and leaves nothing behind. So it hangs
  // off the whole row rather than needing an affordance of its own — every
  // row is about exactly one place.

  let pointingAt = SPOT_NOTHING;

  function ringSpot(spot) {
    if (spot === pointingAt) return;
    pointingAt = spot;
    wasm.bims_set_highlight(spot);
  }

  /** Ring `spot` on the deck while the pointer is on `el`. */
  function points(el, spot) {
    if (!el || !spot) return;
    el.className = `${el.className} points`.trim();
    el.addEventListener("pointerenter", () => ringSpot(spot));
    el.addEventListener("pointerleave", () => ringSpot(SPOT_NOTHING));
    // Keyboard and focus reach it too, and a pointer that leaves the window
    // mid-row would otherwise leave a fixture lit for ever.
    el.addEventListener("focus", () => ringSpot(spot));
    el.addEventListener("blur", () => ringSpot(SPOT_NOTHING));
  }

  // The tray folding away, a tab changing under the pointer, the window losing
  // focus: none of these fire `pointerleave` reliably, and a ring left burning
  // on the fridge reads as a state the game is in rather than a thing the
  // pointer is doing.
  window.addEventListener("blur", () => ringSpot(SPOT_NOTHING));

  // --- what is aboard ------------------------------------------------------
  //
  // One row per thing: what it is, how many there are, and where they are.
  // Everything automated will want a row here eventually, so the table is
  // built from a list rather than written out in the markup — adding a thing
  // is adding a line to `stockRows`.

  const stockBody = document.querySelector("#stock tbody");
  const stockRows = [
    ["Vegetables", () => wasm.bims_store_veg(), "Cold store", SPOT_FRIDGE],
    ["Tofu", () => wasm.bims_store_tofu(), "Cold store", SPOT_FRIDGE],
    ["Stew", () => wasm.bims_pot_servings(), "Pot on the hob", SPOT_HOB],
  ];
  const stockCells = stockRows.map(([name, , where, spot]) => {
    const tr = document.createElement("tr");
    const item = document.createElement("td");
    item.textContent = name;
    const qty = document.createElement("td");
    qty.className = "qty";
    const place = document.createElement("td");
    place.className = "where";
    place.textContent = where;
    tr.append(item, qty, place);
    points(tr, spot);
    stockBody.appendChild(tr);
    return qty;
  });

  function paintStock() {
    for (let i = 0; i < stockRows.length; i++) {
      const held = String(stockRows[i][1]());
      if (stockCells[i].textContent !== held) stockCells[i].textContent = held;
    }
  }

  // --- the order the work gets done in ------------------------------------
  //
  // One row per job, built from the count wasm reports rather than from the
  // length of the table below: a job added on that side and not this one then
  // shows up as a blank row rather than silently going missing.
  //
  // The names live here and nowhere else. No strings cross the boundary, so
  // the ship knows job 0 and this is the only place that knows it is called
  // "Cleaning" — the same arrangement as `SPOT_NAMES` and `MEMORY_LINES`.

  const WORK_NAMES = ["Cleaning", "Planting", "Plant cutting", "Hauling", "Cook"];

  /** Which fixture each job is about, so resting on a row rings the place it
   * happens. Hauling is the crop carry, and where a haul *ends* is the thing
   * worth pointing at. */
  const WORK_SPOTS = [SPOT_LOCKER, SPOT_BAY, SPOT_BAY, SPOT_FRIDGE, SPOT_HOB];

  const workBody = document.querySelector("#work tbody");
  const workRows = [];

  /** The number in the box and the colour that says what it means. */
  function paintPriority(box, level) {
    box.textContent = String(level);
    box.className = `pri p${level}`;
    // Which way round the scale runs is read off wasm rather than written out
    // here, so the panel cannot come to disagree with the sorting. It is worth
    // saying out loud: "1 is most important" is not obvious from a column of
    // numbers, and it is the one thing a player has to know to use this.
    box.title = `Priority ${level} — ${wasm.bims_work_highest()} is done first, ${wasm.bims_work_lowest()} last. Click to change.`;
  }

  /** Sorting is pressed rather than left on, so nothing moves under the
   * pointer on the click that changed it. The mark on the button is cleared
   * whenever a box is clicked, because the list may no longer be in that
   * order and a mark that lies is worse than no mark. */
  function forgetSort() {
    for (const button of document.querySelectorAll("#work-sort .sort")) {
      button.className = "sort";
    }
  }

  function buildWork() {
    const count = wasm.bims_work_count();
    for (let job = 0; job < count; job++) {
      const name = WORK_NAMES[job] ?? "";
      const row = document.createElement("tr");
      const what = document.createElement("td");
      what.textContent = name;
      const held = document.createElement("td");
      const box = document.createElement("button");
      box.type = "button";
      box.dataset.job = String(job);
      // The cycling is wasm's, and what comes back is what actually landed —
      // the range lives on that side, so the box cannot come to disagree with
      // it by counting to five itself.
      box.addEventListener("click", () => {
        paintPriority(box, wasm.bims_cycle_work_priority(job));
        forgetSort();
      });
      paintPriority(box, wasm.bims_work_priority(job));
      held.appendChild(box);
      row.append(what, held);
      points(row, WORK_SPOTS[job] ?? SPOT_NOTHING);
      workBody.appendChild(row);
      workRows.push({ job, row, box, name });
    }
  }

  const byPriority = (row) => wasm.bims_work_priority(row.job);
  const byName = (a, b) =>
    a.name.toLowerCase() < b.name.toLowerCase()
      ? -1
      : a.name.toLowerCase() > b.name.toLowerCase()
        ? 1
        : 0;
  // Equal priorities keep the order the jobs are declared in, so the list
  // never reshuffles for no reason.
  const WORK_SORTS = {
    up: (a, b) => byPriority(a) - byPriority(b) || a.job - b.job,
    down: (a, b) => byPriority(b) - byPriority(a) || a.job - b.job,
    name: byName,
  };

  function sortWork(how) {
    const order = [...workRows].sort(WORK_SORTS[how] ?? WORK_SORTS.up);
    workBody.replaceChildren(...order.map((r) => r.row));
  }

  for (const button of document.querySelectorAll("#work-sort .sort")) {
    button.addEventListener("click", () => {
      forgetSort();
      button.className = "sort on";
      sortWork(button.dataset.sort);
    });
  }

  buildWork();

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

  /** The state worth naming beside a fixture, or "" for the things that have
   * none. Read out of wasm, so the readout cannot disagree with the deck. */
  function spotState(spot) {
    switch (spot) {
      case 5:
        return wasm.bims_fridge_is_open() ? "open" : "";
      case 6:
        return wasm.bims_stove_is_on() ? "lit" : "";
      case 7:
        if (wasm.bims_dishwasher_cycle_left() > 0) return "running";
        return wasm.bims_dishwasher_loaded() > 0
          ? `${wasm.bims_dishwasher_loaded()} plates in it`
          : "";
      case 11: {
        const ripe = wasm.bims_hydro_ripe();
        return ripe > 0 ? `${ripe} ready to lift` : "";
      }
      case 14:
        if (wasm.bims_door_is_locked()) return "locked";
        return wasm.bims_door_is_open() ? "open" : "shut";
      default:
        return "";
    }
  }

  function paintHover() {
    let thing = "—";
    let onIt = "";
    if (hoverAt) {
      const spot = wasm.bims_spot_at(hoverAt.x, hoverAt.y);
      thing = SPOT_NAMES[spot] ?? "Something";
      const state = spotState(spot);
      if (state) thing = `${thing} · ${state}`;
      // Only the deck can have anything on it. Everywhere else the tile
      // underneath is furniture's business, not the player's.
      if (DECK_SPOTS.has(spot)) {
        const mess = wasm.bims_spot_mess(hoverAt.x, hoverAt.y);
        const name = MESS_NAMES[mess] ?? "";
        if (name) {
          const deep = Math.round(
            wasm.bims_spot_mess_depth(hoverAt.x, hoverAt.y) * 100,
          );
          onIt = `${name} — ${deep}% fouled`;
        }
      }
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
    paintRecruited();
    paintHours();
    paintCrew();
    paintAgendas();
    paintStock();
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
