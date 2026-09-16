// Speech bubbles: the one part of a conversation the player actually reads.
//
// No strings cross the wasm boundary, so what a Bim is saying is a number on
// that side and a sentence on this one. Nothing in the build checks that the
// number has a sentence to go with it, and nothing but this executes the code
// that paints it — a missing entry in `CHAT_TOPICS` is a bubble reading
// "undefined…" over somebody's head and no error anywhere.
//
// The stub records `fillText`, which is the whole of what a stub can see of
// the canvas. That is enough: the names are drawn the same way, so a bubble is
// a third string appearing at the right moment in the right place.

import { boot, settle } from "./stub.mjs";

const fails = [];
function check(what, ok, extra = "") {
  if (ok) console.log(`  ok   ${what}`);
  else {
    console.log(`  FAIL ${what}${extra ? ` — ${extra}` : ""}`);
    fails.push(what);
  }
}

const { byId, root, step, wasm, drawn } = await boot({ frames: 5 });
check("no error box", !byId.get("error").textContent, byId.get("error").textContent);

const crewCount = wasm.bims_crew();

// Nothing is being said yet: the company bar starts most of the way full and
// the first conversation is half a day off.
drawn.length = 0;
step(1);
check(
  "nothing is said before anybody wants a word",
  drawn.length === crewCount,
  drawn.map((d) => d.text).join(" | "),
);

// Run the day forward. At 1x this would be forty thousand frames; the speed
// control multiplies the simulation steps per frame, which is exactly what it
// is for. Driven through the real input so the harness exercises the host's
// own handler rather than reaching past it.
const speed = byId.get("speed");
speed.value = "24";
speed.dispatch("input", {});

const JOB_CHAT = 14;
const talking = () => {
  for (let who = 0; who < crewCount; who++) {
    if (wasm.bims_activity(who) === JOB_CHAT) return true;
  }
  return false;
};

let frames = 0;
try {
  frames = settle(step, talking, 6000);
} catch {
  check("somebody gets round to having a word", false, "no conversation in 6000 frames");
}
if (talking()) {
  check(`somebody gets round to having a word (${frames} frames)`, true);

  // Both are in the conversation; only one of them has the floor at a time.
  // Two bubbles over two Bims standing a body's width apart would sit on top
  // of each other, which is why wasm decides whose turn it is off the ship's
  // clock rather than off either chain.
  let sawBubble = 0;
  let mostAtOnce = 0;
  let saidWhat = new Set();
  // The geometry has to be caught *while* the conversation is going on. A
  // second pass over it afterwards finds nothing: the chat is six game
  // minutes long and at 24x that is a handful of frames.
  let shot = null;
  for (let i = 0; i < 400; i++) {
    drawn.length = 0;
    step(1);
    const extra = drawn.filter((d) => !d.text || d.text.endsWith("…"));
    mostAtOnce = Math.max(mostAtOnce, extra.length);
    for (const d of extra) {
      sawBubble++;
      saidWhat.add(d.text);
    }
    if (!shot && extra.length === 1) {
      let speaker = -1;
      for (let who = 0; who < crewCount; who++) {
        if (wasm.bims_chat_topic(who)) speaker = who;
      }
      const name = drawn.find((d) => d.text && !d.text.endsWith("…"));
      // The Bim's position is read *now*, on the frame the bubble was
      // painted. Reading it afterwards compares a bubble from one frame with
      // a body from four hundred frames later, by which time the pair have
      // finished talking and walked to opposite ends of the deck.
      if (speaker >= 0) {
        const s = wasm.bims_view_scale();
        shot = {
          bubble: extra[0],
          name,
          x: wasm.bims_view_x() + wasm.bims_bim_x(speaker) * s,
          y: wasm.bims_view_y() + wasm.bims_bim_y(speaker) * s,
        };
      }
    }
  }
  check("a bubble goes up over one of them", sawBubble > 0, sawBubble);
  check("and never over both at once", mostAtOnce <= 1, mostAtOnce);

  // The whole point of this file. A code with no entry in `CHAT_TOPICS`
  // renders as "undefined…" and nothing else would ever notice.
  const said = [...saidWhat];
  check(
    "everything said is a sentence, not a hole in a table",
    said.length > 0 && said.every((t) => t.length > 1 && !t.includes("undefined")),
    said.join(" | "),
  );
  console.log(`       heard: ${said.join(" | ")}`);

  // Over the head, above the name, and over the Bim that is talking.
  if (shot) {
    const { bubble, name, x, y } = shot;
    check(
      "the bubble is over whoever is talking",
      Math.abs(bubble.x - x) < 0.001,
      `${bubble.x} vs ${x}`,
    );
    check("and above the body", bubble.y < y, `${bubble.y} vs ${y}`);
    // Above the name, too, or the two are written across each other.
    if (name) {
      check("and clear of the names", bubble.y < name.y, `${bubble.y} vs ${name.y}`);
    }
  } else {
    check("a bubble could be found to measure", false);
  }
}

// --- the fifth need has a row and a name ----------------------------------

// One panel per crew member, each with a row per need — so the rows are
// counted per Bim rather than across the lot.
const rows = root.querySelectorAll("#side .needs li");
const needNames = rows.map((r) => r.querySelector(".name")?.textContent ?? "");
const want = wasm.bims_need_count() * crewCount;
check("there is a row per need wasm reports", rows.length === want, `${rows.length} vs ${want}`);
check(
  "and the new one is among them",
  needNames.includes("Socializing"),
  needNames.join(", "),
);
check(
  "every need row is named",
  needNames.every((n) => n.trim().length > 0),
  needNames.join(", "),
);

console.log();
if (fails.length) {
  console.log(`${fails.length} FAILED`);
  process.exit(1);
}
console.log("all passed");
