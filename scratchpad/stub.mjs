// The only stub DOM there is. Every harness imports `boot` and `settle` from
// here; there are no copies, because copies go stale the moment web/bims.js
// starts using a DOM method one of them has not got, and that reads like an
// application failure when it is nothing of the kind.
//
// It fakes just enough of a browser for web/bims.js to run: elements with
// classes and attributes, a no-op 2D context, captured event listeners, a
// frame clock, and a timer queue driven off that clock rather than off Node's,
// so "300ms" is a number of steps and a harness stays synchronous.

import { readFileSync } from "node:fs";
import vm from "node:vm";

const HTML = "web/index.html";
const HOST = "web/bims.js";
const WASM = "web/bims.wasm";

class El {
  constructor(tag, doc) {
    this.tagName = String(tag).toUpperCase();
    this.doc = doc;
    this.children = [];
    this.parentNode = null;
    this.attrs = new Map();
    this.style = {};
    this.dataset = {};
    this.listeners = new Map();
    this._class = "";
    this._text = "";
    this.hidden = false;
    this.value = "";
    this.checked = false;
    this.type = "";
    this.htmlFor = "";
    this.id = "";
    this.title = "";
    this.disabled = false;
    this.min = "";
    this.max = "";
    this.step = "";
  }

  get className() {
    return this._class;
  }
  set className(v) {
    this._class = String(v);
  }

  get textContent() {
    if (this.children.length === 0) return this._text;
    return this.children.map((c) => c.textContent).join("");
  }
  set textContent(v) {
    this.children = [];
    this._text = String(v);
  }

  setAttribute(name, value) {
    this.attrs.set(name, String(value));
    if (name === "hidden") this.hidden = true;
    if (name === "id") this.id = String(value);
  }
  getAttribute(name) {
    return this.attrs.has(name) ? this.attrs.get(name) : null;
  }
  removeAttribute(name) {
    this.attrs.delete(name);
    if (name === "hidden") this.hidden = false;
  }

  /** Appending takes the child out of wherever it was, as the real DOM
   * does. The builder *moves* its World panel between two tools this way,
   * and a stub that left a copy behind would answer a `querySelector` off
   * whichever copy it found first. */
  appendChild(child) {
    if (child.parentNode) {
      child.parentNode.children = child.parentNode.children.filter((c) => c !== child);
    }
    child.parentNode = this;
    this.children.push(child);
    this.doc.index(child);
    return child;
  }
  append(...kids) {
    for (const k of kids) this.appendChild(k);
  }
  replaceChildren(...kids) {
    for (const c of this.children) c.parentNode = null;
    this.children = [];
    this._text = "";
    this.append(...kids);
  }
  contains(other) {
    if (other === this) return true;
    return this.children.some((c) => c.contains(other));
  }

  setPointerCapture() {
    this._captured = true;
  }
  hasPointerCapture() {
    return this._captured === true;
  }
  releasePointerCapture() {
    this._captured = false;
  }

  addEventListener(type, fn) {
    if (!this.listeners.has(type)) this.listeners.set(type, []);
    this.listeners.get(type).push(fn);
  }
  removeEventListener(type, fn) {
    const list = this.listeners.get(type);
    if (list) this.listeners.set(type, list.filter((f) => f !== fn));
  }
  dispatch(type, event = {}) {
    const e = { type, target: this, preventDefault() {}, ...event };
    for (const fn of this.listeners.get(type) ?? []) fn(e);
    return e;
  }

  /** A whole rect. `bottom` and `right` included — positioning code that only
   * gets `left`/`top`/`width` silently computes NaN, and a harness cannot see
   * that go wrong. */
  getBoundingClientRect() {
    const w = this.clientWidth;
    const h = this.clientHeight;
    return { left: 0, top: 0, right: w, bottom: h, width: w, height: h, x: 0, y: 0 };
  }

  get clientWidth() {
    return this.tagName === "CANVAS" ? 960 : 200;
  }
  get clientHeight() {
    return this.tagName === "CANVAS" ? 640 : 40;
  }

  /** Only what the host actually uses: `#id`, `.class`, `tag`, `[attr]`, and
   * a space-separated descendant chain of those. */
  querySelector(sel) {
    return this.querySelectorAll(sel)[0] ?? null;
  }
  querySelectorAll(sel) {
    let pool = [this];
    for (const part of sel.trim().split(/\s+/)) {
      const next = [];
      for (const node of pool) {
        for (const d of node.descendants()) {
          if (matches(d, part) && !next.includes(d)) next.push(d);
        }
      }
      pool = next;
    }
    return pool;
  }
  descendants() {
    const out = [];
    for (const c of this.children) out.push(c, ...c.descendants());
    return out;
  }
}

function matches(el, part) {
  for (const bit of part.split(/(?=[.#[])/)) {
    if (bit.startsWith("#")) {
      if (el.id !== bit.slice(1)) return false;
    } else if (bit.startsWith(".")) {
      if (!el.className.split(/\s+/).includes(bit.slice(1))) return false;
    } else if (bit.startsWith("[")) {
      // The value may be quoted or bare — CSS allows both, and a stub that
      // only took `[data-x="y"]` silently matched nothing for `[data-x=y]`,
      // which reads as the element not existing.
      const m = /^\[([^\]=]+)(?:=(?:"([^"]*)"|([^\]"]*)))?\]$/.exec(bit);
      if (!m) return false;
      const want = m[2] ?? m[3];
      const key = m[1];
      const held = el.attrs.has(key)
        ? el.attrs.get(key)
        : key.startsWith("data-")
          ? el.dataset[camel(key.slice(5))]
          : undefined;
      if (held === undefined) return false;
      if (want !== undefined && want !== "" && held !== want) return false;
    } else if (bit) {
      if (el.tagName !== bit.toUpperCase()) return false;
    }
  }
  return true;
}

function camel(s) {
  return s.replace(/-([a-z])/g, (_, c) => c.toUpperCase());
}

/** A no-op 2D context that remembers the one thing a harness can usefully
 * check: the text drawn on the canvas.
 *
 * Everything else the renderer does ends up as pixels, which a stub cannot
 * see. Text does not — the names over the crew are the host's own work rather
 * than shapes out of the draw buffer, so they are worth asserting. Written as
 * a plain object with a catch-all fallback rather than a Proxy, so a method
 * the host newly uses is still a silent no-op rather than a crash. */
function makeCtx(drawn) {
  const ctx = {
    fillText(text, x, y) {
      drawn.push({ text: String(text), x, y, fill: ctx.fillStyle });
    },
    strokeText() {},
    /** A real object rather than the Proxy's no-op, because the host reads
     * `.width` off it to size a speech bubble — and `undefined.width` throws
     * before the first frame, which reads as the page failing to start. The
     * number is made up; there are no glyphs here to measure. */
    measureText(text) {
      return { width: String(text).length * 6 };
    },
  };
  return new Proxy(ctx, {
    get(target, name) {
      if (name in target) return target[name];
      // Style properties are read back by the host; anything else is a call.
      return () => {};
    },
    set(target, name, value) {
      target[name] = value;
      return true;
    },
  });
}

/** Every element the markup declares, with its tag, id, classes, data-* and
 * whether it starts hidden — read off index.html rather than written out
 * again here, so the stub cannot drift from the page. */
function parseMarkup(html) {
  const body = html.slice(html.indexOf("<body>"), html.indexOf("</body>"));
  const out = [];
  const stack = [];
  const tagRe = /<(\/?)([a-z0-9]+)([^>]*?)(\/?)>/gi;
  let m;
  while ((m = tagRe.exec(body))) {
    const [, closing, tag, attrText, selfClose] = m;
    if (closing) {
      stack.pop();
      continue;
    }
    const attrs = {};
    for (const a of attrText.matchAll(/([a-z-]+)(?:="([^"]*)")?/gi)) {
      attrs[a[1]] = a[2] ?? "";
    }
    const node = {
      tag,
      attrs,
      parent: stack.length ? stack[stack.length - 1] : null,
    };
    out.push(node);
    const empty = ["input", "br", "img", "meta", "link"].includes(tag.toLowerCase());
    if (!selfClose && !empty) stack.push(node);
  }
  return out;
}

/** The page itself: every element the markup declares, indexed by id, hung
 * off a stub `document`. Shared by both entry points below — the room boots
 * against wasm and a frame loop, the builder against neither, and a second
 * copy of this is exactly the drift the file exists to avoid. */
function makeDom(html, ctx2d) {
  const byId = new Map();
  const doc = {
    index(el) {
      if (el.id && !byId.has(el.id)) byId.set(el.id, el);
      for (const c of el.children) doc.index(c);
    },
    createElement(tag) {
      const el = new El(tag, doc);
      if (String(tag).toLowerCase() === "canvas") el.getContext = () => ctx2d;
      return el;
    },
    getElementById(id) {
      return byId.get(id) ?? null;
    },
    querySelector(sel) {
      return root.querySelector(sel);
    },
    querySelectorAll(sel) {
      return root.querySelectorAll(sel);
    },
    addEventListener() {},
  };

  const root = new El("body", doc);
  doc.body = root;

  const made = new Map();
  for (const node of parseMarkup(html)) {
    if (node.tag.toLowerCase() === "script") continue;
    const el = doc.createElement(node.tag);
    for (const [k, v] of Object.entries(node.attrs)) {
      if (k === "id") el.id = v;
      else if (k === "class") el.className = v;
      else if (k === "hidden") el.hidden = true;
      else if (k.startsWith("data-")) el.dataset[camel(k.slice(5))] = v;
      else if (k === "type") el.type = v;
      else if (k === "value") el.value = v;
      else if (k === "checked") el.checked = true;
      else el.setAttribute(k, v);
    }
    made.set(node, el);
    (node.parent ? made.get(node.parent) : root).appendChild(el);
  }

  return { byId, doc, root };
}


/** The clock, and the timer queue hung off it.
 *
 * One of these per booted page, and **not** Node's: a timer fires when the
 * harness says enough time has gone by, so a harness stays synchronous and
 * "300ms" is a number of frames rather than a real wait.
 *
 * Both ways of moving it on share the queue. `step(n)` walks whole frames,
 * which is what a page with a render loop wants; `advance(ms)` jumps to each
 * timer in turn, which is what a page that is only a page wants. A page with
 * both — the ship designer has a frame loop *and* timed remarks — uses
 * whichever fits the thing being checked. */
function makeClock() {
  let now = 0;
  let next = 1;
  const timers = new Map();

  const clock = {
    timers,
    now: () => now,

    setTimeout(fn, ms) {
      const id = next++;
      timers.set(id, { at: now + (ms ?? 0), fn });
      return id;
    },

    clearTimeout(id) {
      timers.delete(id);
    },

    /** Fire everything due at or before `now`. */
    fire() {
      for (const [id, t] of [...timers]) {
        if (t.at <= now) {
          timers.delete(id);
          t.fn();
        }
      }
    },

    /** Move to `at` and fire what falls due on the way. */
    to(at) {
      now = at;
      clock.fire();
    },

    /** Let `ms` go by, stopping at each timer so one can set another. */
    advance(ms) {
      const until = now + ms;
      for (;;) {
        let soonest = null;
        for (const [id, t] of timers) {
          if (t.at <= until && (soonest === null || t.at < timers.get(soonest).at)) {
            soonest = id;
          }
        }
        if (soonest === null) break;
        const t = timers.get(soonest);
        timers.delete(soonest);
        now = t.at;
        t.fn();
      }
      now = until;
    },
  };
  return clock;
}

/** A recording wrapper round a page's wasm exports.
 *
 * The host keeps its exports inside its own `boot()`'s closure, so the only
 * way to get at the same instance is to hand it over as it is made. What the
 * host gets is a copy in which every export it calls is remembered, which is
 * how a harness checks something the host only ever *tells* wasm — "ring this
 * fixture" — without wasm needing a getter nothing in the game would want.
 *
 * It has to be a **plain copied object**, not a `Proxy`. An exports object
 * holds its entries as read-only non-configurable data properties, and a
 * `get` trap that returns anything but the real function is a `TypeError` on
 * the first read — which kills `boot()` before the first frame and reports
 * as "boot never asked for a frame". */
function recordingWasm(bytes) {
  const calls = new Map();
  const held = { exports: null };

  const instantiate = async (buf, imports) => {
    const result = await WebAssembly.instantiate(buf, imports);
    const real = result.instance.exports;
    held.exports = real;
    const watched = {};
    for (const name of Object.keys(real)) {
      const member = real[name];
      watched[name] =
        typeof member === "function"
          ? (...args) => {
              calls.set(name, args);
              return member(...args);
            }
          : member;
    }
    return { ...result, instance: { ...result.instance, exports: watched } };
  };

  return {
    calls,
    instantiate,
    bytes,
    get exports() {
      return held.exports;
    },
  };
}

/** The window object a page gets. Enough of one for a host to hang listeners
 * off and read a pixel ratio from; the harness fires those listeners itself. */
function makeWindow() {
  const win = {
    devicePixelRatio: 1,
    innerWidth: 1280,
    innerHeight: 800,
    listeners: new Map(),
    addEventListener(type, fn) {
      if (!win.listeners.has(type)) win.listeners.set(type, []);
      win.listeners.get(type).push(fn);
    },
    removeEventListener(type, fn) {
      const list = win.listeners.get(type);
      if (list) win.listeners.set(type, list.filter((f) => f !== fn));
    },
    BIMS_BUILD: 1,
  };
  return win;
}

/** A `location` a page can read a query string off and navigate with.
 *
 * Real browsers always have one, so unlike `navigator` and `crypto` — which
 * are deliberately missing below — this is provided. `assign` records where
 * it was sent rather than going there, which is how a harness checks that
 * Start hands the designer the right numbers.
 *
 * The pages still guard it. A host that assumed `location` would be there
 * cannot be booted by anything but a browser, and the one place that matters
 * is exactly the one a harness needs to exercise. */
function makeLocation(page, search) {
  const visited = [];
  const location = {
    search,
    pathname: `/${page}`,
    href: `http://stub/${page}${search}`,
    assign(url) {
      visited.push(String(url));
      location.href = String(url);
    },
    replace(url) {
      location.assign(url);
    },
  };
  return { location, visited };
}

/** Everything a page's script runs against, bar the page itself.
 *
 * Deliberately **no `navigator` and no `crypto`**. Both are genuinely missing
 * in real browsers often enough — the clipboard over plain http, older
 * engines — that a host has to cope, and the harness is where that gets found
 * out rather than in somebody's browser. */
function makeSandbox({ doc, win, clock, location }) {
  const sandbox = {
    document: doc,
    window: win,
    location,
    console,
    Math,
    JSON,
    Date,
    Set,
    Map,
    Number,
    String,
    Array,
    Object,
    Uint8Array,
    Float32Array,
    BigInt,
    Error,
    decodeURIComponent,
    encodeURIComponent,
    performance: { now: () => clock.now() },
    setTimeout: clock.setTimeout,
    clearTimeout: clock.clearTimeout,
  };
  sandbox.globalThis = sandbox;
  return sandbox;
}

/** Build a page, run its script, and hand back the harness's handle on it
 * along with whatever still has to be waited for.
 *
 * `wasm` is a path or null. With one, the page is given `WebAssembly`, a
 * `fetch` that hands back those bytes, and a `requestAnimationFrame` whose
 * callback the harness drives through `step()`; `ready()` then waits until
 * the page has asked for its first frame. Without one, the page is only a
 * page — markup and a script — there is nothing to instantiate or step, and
 * `ready` is null so the caller can stay synchronous.
 *
 * Both go through the same `makeDom`, the same clock and the same sandbox,
 * which is the point of this file: a second copy of any of them is how a stub
 * drifts from the host it is meant to stand in for. */
function startPage({ html, host, wasm = null, search = "" }) {
  // Text the host paints onto the canvas, newest last. A stub has no glyphs,
  // so this is the only part of the rendering it can see at all.
  const drawn = [];
  const ctx2d = makeCtx(drawn);
  const { byId, doc, root } = makeDom(readFileSync(html, "utf8"), ctx2d);

  const clock = makeClock();
  const win = makeWindow();
  const page = host.replace(/^.*\//, "").replace(/\.js$/, ".html");
  const { location, visited } = makeLocation(page, search);
  const sandbox = makeSandbox({ doc, win, clock, location });

  let frameCb = null;
  const module = wasm === null ? null : recordingWasm(readFileSync(wasm));
  if (module) {
    sandbox.WebAssembly = { ...WebAssembly, instantiate: module.instantiate };
    sandbox.fetch = async () => ({ arrayBuffer: async () => module.bytes });
    sandbox.requestAnimationFrame = (fn) => {
      frameCb = fn;
      return 1;
    };
  }

  const ctx = vm.createContext(sandbox);
  vm.runInContext(readFileSync(host, "utf8"), ctx, { filename: host });

  /** Wait until the page's own `boot()` has asked for its first frame.
   *
   * On a real clock rather than a count of ticks. `WebAssembly.instantiate`
   * resolves off-thread, so a fixed number of `setImmediate` turns is a race
   * it sometimes loses — and losing it reads exactly like the page failing to
   * start, which is an afternoon on the wrong file. `setTimeout` here is
   * Node's own; the sandbox's is a different function and only exists inside
   * `vm`. */
  const ready = module
    ? async () => {
        const deadline = Date.now() + 15000;
        while (frameCb === null && Date.now() < deadline) {
          await new Promise((r) => setTimeout(r, 1));
        }
        if (frameCb === null) {
          const said = byId.get("error")?.textContent;
          throw new Error(
            `boot() never asked for a frame in 15s: ${said || "and said nothing about why"}`,
          );
        }
      }
    : null;

  const MS_PER_FRAME = 1000 / 60;

  function step(n = 1) {
    if (!module) throw new Error("this page has no frame loop to step");
    for (let i = 0; i < n; i++) {
      // Anything due fires from inside the frame, so a harness stays
      // synchronous and a 300ms tooltip delay is 19 steps rather than a wait.
      clock.to(clock.now() + MS_PER_FRAME);
      const cb = frameCb;
      frameCb = null;
      cb(clock.now());
      if (frameCb === null) throw new Error("the frame loop stopped");
    }
  }

  function dispatch(id, type, event) {
    const el = byId.get(id);
    if (!el) throw new Error(`no element #${id}`);
    return el.dispatch(type, event);
  }

  function click(id, event = {}) {
    return dispatch(id, "click", event);
  }

  /** Fire a window listener — `keydown`, `blur`, `resize`. Pages hang those
   * off `window` rather than off an element, so there is nothing to
   * `dispatch` on. */
  function fire(type, event = {}) {
    const e = { type, preventDefault() {}, ...event };
    for (const fn of win.listeners.get(type) ?? []) fn(e);
    return e;
  }

  /** The arguments of the last call the host made to `name`, or undefined. */
  function lastCall(name) {
    return module?.calls.get(name);
  }

  /** The harness's handle. Built on demand rather than up front because
   * `wasm` is not there until the module has finished instantiating, and a
   * handle handed out before that would carry a null in the one field every
   * assertion goes through. */
  const handle = () => ({
    byId,
    doc,
    root,
    win,
    location,
    visited,
    step,
    dispatch,
    click,
    fire,
    advance: clock.advance,
    timers: clock.timers,
    wasm: module?.exports ?? null,
    lastCall,
    drawn,
  });

  return { handle, ready };
}

/** Boot web/bims.js — the room — against the real wasm and a stub page. */
export async function boot(options = {}) {
  return bootWasmPage({ html: HTML, host: HOST, wasm: WASM, ...options });
}

/** Boot a page as if it were only a page: markup and one script, no wasm
 * and no frame loop. `web/builder.html` used to be exactly that, and can
 * still be booted this way to check the half of it that needs no galaxy —
 * the menu, the setup screen and the lobby — and that the World tab says
 * the module is missing rather than sitting there loading. For the World
 * tab itself it is `bootWasmPage` with `web/lobby.wasm`.
 *
 * Synchronous, because a page with no wasm has nothing to wait for. Time
 * still has to pass, though: the builder hangs remarks off `setTimeout`, and
 * `advance(ms)` fires whatever is due. */
export function bootPage({ html = "web/builder.html", host = "web/builder.js", search = "" } = {}) {
  const { handle, ready } = startPage({ html, host, search });
  if (ready !== null) {
    throw new Error("bootPage: that page has wasm behind it — use bootWasmPage");
  }
  return handle();
}

/** Boot a page that has its own wasm. `web/ship.html` was the first — a
 * third front end, with `ship.wasm` behind it rather than the room's — and
 * `web/builder.html` with `web/lobby.wasm` is the second.
 *
 * Everything `boot()` returns, plus `advance` for the timed remarks and
 * `fire` for the keyboard. Async, because instantiating resolves off-thread. */
export async function bootWasmPage({ frames = 0, ...options }) {
  const { handle, ready } = startPage(options);
  await ready();
  const page = handle();
  if (frames) page.step(frames);
  return page;
}

/** Run frames until `check()` is true, or give up. */
export function settle(step, check, limit = 20000) {
  for (let i = 0; i < limit; i++) {
    if (check()) return i;
    step(1);
  }
  throw new Error("never settled");
}
