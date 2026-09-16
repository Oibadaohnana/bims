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

const { byId, root, click, dispatch, advance } = bootPage();

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

check("stores start at ×1", chosenIn(setupTool, "stockpile") === "base", chosenIn(setupTool, "stockpile"));
check("the ship starts at 40 × 40", chosenIn(setupTool, "ship") === "standard", chosenIn(setupTool, "ship"));

const optionIn = (tool, field, id) =>
  tool.querySelector(`[data-field=${field}] [data-option=${id}]`);

// Every option offered is one of the three the game knows about, in order.
const storeLabels = setupTool
  .querySelectorAll("[data-field=stockpile] .option .sub")
  .map((s) => s.textContent);
check("stores offer ×0.5, ×1, ×2", storeLabels.join(" ") === "×0.5 ×1 ×2", storeLabels.join(" "));
const shipLabels = setupTool
  .querySelectorAll("[data-field=ship] .option .sub")
  .map((s) => s.textContent);
check(
  "ships offer 30, 40 and 60 tiles a side",
  shipLabels.join(" ") === "30 × 30 40 × 40 60 × 60",
  shipLabels.join(" "),
);

optionIn(setupTool, "stockpile", "double").dispatch("click");
optionIn(setupTool, "ship", "large").dispatch("click");
check("picking ×2 sticks", chosenIn(setupTool, "stockpile") === "double", chosenIn(setupTool, "stockpile"));
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
check("the lobby carries the ×2 picked earlier", chosenIn(lobbyTool, "stockpile") === "double", chosenIn(lobbyTool, "stockpile"));
check("and the 60 × 60 ship", chosenIn(lobbyTool, "ship") === "large", chosenIn(lobbyTool, "ship"));

// As host you can still change them.
optionIn(lobbyTool, "stockpile", "half").dispatch("click");
check("the host can change a setting", chosenIn(lobbyTool, "stockpile") === "half", chosenIn(lobbyTool, "stockpile"));

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
check("Start opens the builder placeholder", showing().join() === "build", showing().join());

const said = byId.get("chosen").textContent;
check("it names the ship that was chosen", said.includes("60 × 60"), said);
check("and the stores", said.includes("×0.5"), said);

screen("build").querySelector("[data-goto]").dispatch("click");
check("and it goes back to the menu", showing().join() === "menu", showing().join());
check("still no error box", !byId.get("error").textContent, byId.get("error").textContent);

console.log(fails.length ? `\n${fails.length} FAILED` : "\nall passed");
process.exit(fails.length ? 1 : 0);
