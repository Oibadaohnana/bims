// Does the builder page actually run? `cargo build` says nothing about a page
// of plain JavaScript, and a parse error there is a blank screen with no
// message — so this boots web/builder.js against the stub DOM and walks the
// whole path a player walks: menu → setup → back → lobby → start.
//
// The multiplayer half is surface only, and that is asserted too: a join has
// to *refuse* out loud rather than appear to work.

import { bootPage } from "./stub.mjs";

const fails = [];
function check(what, ok, extra = "") {
  if (ok) console.log(`  ok   ${what}`);
  else {
    console.log(`  FAIL ${what}${extra ? ` — ${extra}` : ""}`);
    fails.push(what);
  }
}

const { byId, root, click, dispatch, advance, visited } = bootPage();

const screen = (name) => root.querySelector(`[data-screen=${name}]`);
const showing = () =>
  [...root.querySelectorAll("[data-screen]")]
    .filter((s) => !s.hidden)
    .map((s) => s.dataset.screen);

console.log("builder booted");
check("no error box", !byId.get("error").textContent, byId.get("error").textContent);

// --- the start menu ----------------------------------------------------

check("the menu is what you land on", showing().join() === "menu", showing().join());
check("Play is there", !!byId.get("play"));
check("Create lobby is beside it", !!byId.get("create-lobby"));

// --- Play, and the settings tool ---------------------------------------

click("play");
check("Play opens the setup screen", showing().join() === "setup", showing().join());

const setupTool = byId.get("setup-tool");
const tabsOf = (tool) =>
  tool.querySelectorAll(".tool-tabs .tab").map((t) => t.textContent);
check(
  "the tool has a setup tab and a world tab",
  tabsOf(setupTool).join("|") === "Game setup|World",
  tabsOf(setupTool).join("|"),
);

const chosenIn = (tool, field) => {
  const on = tool.querySelector(`[data-field=${field}] .option.on`);
  return on ? on.dataset.option : "(none)";
};

check(
  "money starts at €100 000 a Bim",
  chosenIn(setupTool, "moneyPerBim") === "standard",
  chosenIn(setupTool, "moneyPerBim"),
);
check("the ship starts at 40 × 40", chosenIn(setupTool, "ship") === "standard", chosenIn(setupTool, "ship"));

const optionIn = (tool, field, id) =>
  tool.querySelector(`[data-field=${field}] [data-option=${id}]`);

// Every option offered is one of the three the game knows about, in order.
// The euro sign and the grouping are read off the page rather than written
// out here: they exist in exactly one place in the host and a copy down here
// would be a second one to keep in step.
const moneyLabels = setupTool
  .querySelectorAll("[data-field=moneyPerBim] .option .sub")
  .map((s) => s.textContent);
const bare = moneyLabels.map((t) => t.replace(/[^0-9]/g, ""));
check("money offers 50 000, 100 000 and 200 000", bare.join(" ") === "50000 100000 200000", moneyLabels.join(" "));
check(
  "and says so in euros",
  moneyLabels.every((t) => t.startsWith("€")),
  moneyLabels.join(" "),
);
const shipLabels = setupTool
  .querySelectorAll("[data-field=ship] .option .sub")
  .map((s) => s.textContent);
check(
  "ships offer 30, 40 and 60 tiles a side",
  shipLabels.join(" ") === "30 × 30 40 × 40 60 × 60",
  shipLabels.join(" "),
);

optionIn(setupTool, "moneyPerBim", "full").dispatch("click");
optionIn(setupTool, "ship", "large").dispatch("click");
check(
  "picking €200 000 sticks",
  chosenIn(setupTool, "moneyPerBim") === "full",
  chosenIn(setupTool, "moneyPerBim"),
);
check("picking 60 × 60 sticks", chosenIn(setupTool, "ship") === "large", chosenIn(setupTool, "ship"));

// The World tab is a real tab with a placeholder behind it, not an empty box.
const worldTab = setupTool.querySelectorAll(".tool-tabs .tab")[1];
worldTab.dispatch("click");
const worldPanel = setupTool.querySelector(".tool-body [data-tab=world]");
const setupPanel = setupTool.querySelector(".tool-body [data-tab=setup]");
check("the world tab shows its panel", !worldPanel.hidden);
check("and puts the setup panel away", setupPanel.hidden);
check("the world tab says what it is for", worldPanel.textContent.length > 40, worldPanel.textContent);

// --- back, then the lobby ----------------------------------------------

screen("setup").querySelector("[data-goto]").dispatch("click");
check("Back returns to the menu", showing().join() === "menu", showing().join());

click("create-lobby");
check("Create lobby opens the lobby", showing().join() === "lobby", showing().join());

const code = byId.get("code").textContent;
check("the lobby has a room code", /^[A-HJ-NP-Z2-9]{6}$/.test(code), code);

const slots = byId.get("slots").children;
check("there are four slots", slots.length === 4, String(slots.length));
check("you are in the first one", slots[0].textContent.includes("James (you)"), slots[0].textContent);
check("and you are the host", slots[0].textContent.includes("Host"), slots[0].textContent);
check(
  "the rest are open",
  slots.slice(1).every((s) => s.className === "open"),
  slots.map((s) => s.className).join(" "),
);

// One settings object behind both tools: what was picked on the setup screen
// is what the lobby shows.
const lobbyTool = byId.get("lobby-tool");
check(
  "the lobby carries the €200 000 picked earlier",
  chosenIn(lobbyTool, "moneyPerBim") === "full",
  chosenIn(lobbyTool, "moneyPerBim"),
);
check("and the 60 × 60 ship", chosenIn(lobbyTool, "ship") === "large", chosenIn(lobbyTool, "ship"));

// As host you can still change them.
optionIn(lobbyTool, "moneyPerBim", "lean").dispatch("click");
check(
  "the host can change a setting",
  chosenIn(lobbyTool, "moneyPerBim") === "lean",
  chosenIn(lobbyTool, "moneyPerBim"),
);

// A guest cannot. Nothing decides who is a guest yet — the host is always
// you — but `setEditable` disables every option for one, and the click
// handler refuses a disabled button rather than leaning on the browser to
// swallow the event. Disabling the button is exactly what a guest's lobby
// does to it, so this is that path and not a mock of it.
const full = optionIn(lobbyTool, "moneyPerBim", "full");
full.disabled = true;
full.dispatch("click");
check(
  "a guest cannot change the money",
  chosenIn(lobbyTool, "moneyPerBim") === "lean",
  chosenIn(lobbyTool, "moneyPerBim"),
);
full.disabled = false;

check("the lobby admits nothing can reach it", byId.get("lobby-said").textContent.length > 0);

// --- joining, which has nothing behind it yet --------------------------

screen("lobby").querySelector("[data-goto]").dispatch("click");
check("Leave returns to the menu", showing().join() === "menu", showing().join());

const note = byId.get("join-note");
byId.get("join-code").value = "nope";
dispatch("join", "submit");
check("a malformed code is refused", note.textContent.includes("not a room code"), note.textContent);
check("and the refusal is marked", note.className.includes("warn"), note.className);

byId.get("join-code").value = "abc234";
dispatch("join", "submit");
check(
  "a well-formed code is refused too, by name",
  note.textContent.includes("ABC234"),
  note.textContent,
);

advance(5000);
check("the refusal clears itself", note.textContent === "", note.textContent);

// --- starting -----------------------------------------------------------

click("play");
screen("setup").querySelector("[data-start]").dispatch("click");
check("Start shows the handover", showing().join() === "build", showing().join());

const said = byId.get("chosen").textContent;
check("it names the ship that was chosen", said.includes("60 × 60"), said);
check("and the money each Bim brings", /50.?000/.test(said) && said.includes("€"), said);

// And then it actually goes there. The designer is a page of its own, so
// Start is a navigation — and what crosses is **numbers in a query string**,
// which is the same rule the wasm boundary is under. A query carrying "large"
// and "half" would be a string setting that had got out of the builder.
const went = visited[visited.length - 1] ?? "";
check("Start navigates to the ship designer", went.startsWith("ship.html?"), went);

const query = new Map(
  went
    .slice(went.indexOf("?") + 1)
    .split("&")
    .map((pair) => pair.split("=")),
);
check("it carries the money each Bim brings", query.get("money") === "50000", went);
check("and the build area in tiles", query.get("area") === "60", went);
check("and how many are playing", query.get("players") === "1", went);
check("and which slot you are", query.get("slot") === "0", went);
check(
  "and nothing that is not a number",
  [...query.values()].every((v) => Number.isFinite(Number(v))),
  went,
);

screen("build").querySelector("[data-goto]").dispatch("click");
check("and it goes back to the menu", showing().join() === "menu", showing().join());
check("still no error box", !byId.get("error").textContent, byId.get("error").textContent);

console.log(fails.length ? `\n${fails.length} FAILED` : "\nall passed");
process.exit(fails.length ? 1 : 0);
