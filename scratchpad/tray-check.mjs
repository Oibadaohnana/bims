// Panels and screens are switched with the `hidden` attribute, and the UA rule
// behind it is only `display: none` at the lowest specificity. A rule like
// `#tray [data-panel="management"] { display: flex }` outranks it, and the
// panel then shows under *every* tab however correctly the attribute is set.
//
// No harness can see that: the stub DOM has no layout engine, and the
// attribute it *can* see is set correctly all along. So this is a text check
// on the stylesheets — any rule keyed on one of the switched attributes that
// also sets `display` has to carry `:not([hidden])`.

import { readFileSync } from "node:fs";

/** The attributes a page switches with `hidden`: the room's tray panels, the
 * builder's screens and its tool's tabs, and the ship designer's two
 * screens. */
const SWITCHED = ["data-panel", "data-screen", "data-tab"];
const PAGES = ["web/index.html", "web/builder.html", "web/ship.html"];

const fails = [];
let looked = 0;

for (const page of PAGES) {
  // Comments out first. A `/* … */` explaining the rule mentions the selector,
  // and a check that matches the prose instead of the rule reads as passing
  // whatever the rule says.
  const css = readFileSync(page, "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
  const keyed = new RegExp(`([^{}]*\\[(?:${SWITCHED.join("|")})[^{}]*)\\{([^}]*)\\}`, "g");
  for (const m of css.matchAll(keyed)) {
    const selector = m[1].trim();
    const body = m[2];
    if (!/(^|[;\s])display\s*:/.test(body)) continue;
    looked++;
    if (!selector.includes(":not([hidden])")) fails.push(`${page}: ${selector}`);
    console.log(`  checked  ${page}  ${selector.replace(/\s+/g, " ")}`);
  }
}

// A page that switches nothing has nothing to check — but a page that *does*
// and comes back with no rules at all means the regex missed, which reads as
// a pass. Say so rather than sit there looking clean.
if (looked === 0) {
  console.log("  FAIL  no display rules keyed on a switched attribute were found at all");
  fails.push("nothing checked");
}

for (const bad of fails) {
  console.log(`  FAIL  ${bad} sets display without :not([hidden])`);
}
console.log(fails.length ? `\n${fails.length} FAILED` : "\nall passed");
process.exit(fails.length ? 1 : 0);
