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
 * screens. The tray's rules live in crew.css, shared by the room and the
 * ship, so that is read as well as the pages. */
const SWITCHED = ["data-panel", "data-screen", "data-tab"];
const PAGES = ["web/index.html", "web/builder.html", "web/ship.html", "web/crew.css"];

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

// The other way two pages sharing a stylesheet bite: a class web/crew.css
// styles **bare** — `.what { … }`, the ? button — lands on every element
// of that class on either page. A page that carries crew's own markup (the
// timetable legend's swatches) wants that; a page that has a rule of its
// *own* for the same class does not — it has two stylesheets fighting over
// one element, and that is how the ship page's readouts came to be
// seventeen pixels wide. So every bare class rule in crew.css is checked
// against each page's own <style>: a rule there naming the class is a
// collision.
{
  const crew = readFileSync("web/crew.css", "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
  const bare = new Set();
  for (const m of crew.matchAll(/(?:^|\n)\.([a-z][a-z-]*)(?::[a-z-]+)*\s*[,{]/g)) bare.add(m[1]);
  if (bare.size === 0) {
    console.log("  FAIL  no bare class rules found in web/crew.css");
    fails.push("no bare classes");
  }
  for (const page of ["web/ship.html", "web/index.html"]) {
    const html = readFileSync(page, "utf8");
    const own = html
      .slice(html.indexOf("<style>"), html.indexOf("</style>"))
      .replace(/\/\*[\s\S]*?\*\//g, "");
    for (const m of own.matchAll(/([^{}]*)\{/g)) {
      const selector = m[1];
      for (const c of selector.matchAll(/\.([a-z][a-z-]*)/g)) {
        if (bare.has(c[1])) {
          console.log(`  FAIL  ${page}: its own rule "${selector.trim()}" styles .${c[1]}, which web/crew.css styles bare`);
          fails.push(`${page} ${c[1]}`);
        }
      }
    }
  }
  console.log(`  checked  ${bare.size} bare classes of web/crew.css against both pages' own styles`);
}
console.log(fails.length ? `\n${fails.length} FAILED` : "\nall passed");
process.exit(fails.length ? 1 : 0);
