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

  appendChild(child) {
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

/** Boot web/bims.js against the real wasm and a stub page. */
export async function boot({ frames = 0 } = {}) {
  const html = readFileSync(HTML, "utf8");
  // Text the host paints onto the canvas, newest last. Cleared each frame by
  // the harness when it wants only this frame's worth.
  const drawn = [];
  const ctx2d = makeCtx(drawn);

  const { byId, doc, root } = makeDom(html, ctx2d);

  // --- the clock, and the timers hung off it ---------------------------

  let clock = 0;
  const timers = new Map();
  let nextTimer = 1;
  let frameCb = null;

  const win = {
    devicePixelRatio: 1,
    innerWidth: 1280,
    innerHeight: 800,
    listeners: new Map(),
    addEventListener(type, fn) {
      if (!win.listeners.has(type)) win.listeners.set(type, []);
      win.listeners.get(type).push(fn);
    },
    removeEventListener() {},
    BIMS_BUILD: 1,
  };

  const bytes = readFileSync(WASM);

  // The host keeps its wasm exports inside boot()'s closure, so the only way
  // to get at the same instance is to hand it over as it is made.
  //
  // What is handed to the host is a recording wrapper: every export it calls
  // goes through and its arguments are kept. That is how a harness checks
  // something the host only ever *tells* wasm — "ring this fixture" — without
  // wasm needing a getter that nothing in the game would otherwise want.
  let wasm = null;
  const calls = new Map();
  const instantiate = async (buf, imports) => {
    const result = await WebAssembly.instantiate(buf, imports);
    const real = result.instance.exports;
    wasm = real;
    // A plain copy, not a Proxy: an exports object holds its entries as
    // read-only non-configurable data properties, and a `get` trap that
    // returns anything but the real function is a TypeError on the first
    // read. `memory` and anything else that is not a function is passed
    // straight through, so the draw buffer is still the live one.
    const watched = {};
    for (const name of Object.keys(real)) {
      const held = real[name];
      watched[name] =
        typeof held === "function"
          ? (...args) => {
              calls.set(name, args);
              return held(...args);
            }
          : held;
    }
    return { ...result, instance: { ...result.instance, exports: watched } };
  };

  const sandbox = {
    document: doc,
    window: win,
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
    Float32Array,
    Error,
    WebAssembly: { ...WebAssembly, instantiate },
    performance: { now: () => clock },
    requestAnimationFrame(fn) {
      frameCb = fn;
      return 1;
    },
    setTimeout(fn, ms) {
      const id = nextTimer++;
      timers.set(id, { at: clock + (ms ?? 0), fn });
      return id;
    },
    clearTimeout(id) {
      timers.delete(id);
    },
    fetch: async () => ({ arrayBuffer: async () => bytes }),
  };
  sandbox.globalThis = sandbox;

  const ctx = vm.createContext(sandbox);
  vm.runInContext(readFileSync(HOST, "utf8"), ctx, { filename: HOST });

  // boot() is async: let its promise chain run to the first frame request.
  //
  // Waited on a real clock rather than a count of ticks. `WebAssembly
  // .instantiate` resolves off-thread, so a fixed number of `setImmediate`
  // turns is a race it sometimes loses — and losing it reads exactly like the
  // page failing to boot, which is a whole afternoon wasted on the wrong file.
  // `setTimeout` here is Node's own; the sandbox's fake one is a different
  // function and only exists inside `vm`.
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

  const MS_PER_FRAME = 1000 / 60;

  function step(n = 1) {
    for (let i = 0; i < n; i++) {
      clock += MS_PER_FRAME;
      // Anything due fires from inside the frame, so a harness stays
      // synchronous and a 300ms tooltip delay is 19 steps rather than a wait.
      for (const [id, t] of [...timers]) {
        if (t.at <= clock) {
          timers.delete(id);
          t.fn();
        }
      }
      const cb = frameCb;
      frameCb = null;
      cb(clock);
      if (frameCb === null) throw new Error("the frame loop stopped");
    }
  }

  function dispatch(id, type, event) {
    const el = byId.get(id);
    if (!el) throw new Error(`no element #${id}`);
    return el.dispatch(type, event);
  }

  /** The arguments of the last call the host made to `name`, or undefined. */
  function lastCall(name) {
    return calls.get(name);
  }

  step(frames);
  return { byId, doc, root, win, step, dispatch, timers, wasm, lastCall, drawn };
}

/** Run frames until `check()` is true, or give up. */
export function settle(step, check, limit = 20000) {
  for (let i = 0; i < limit; i++) {
    if (check()) return i;
    step(1);
  }
  throw new Error("never settled");
}

/** Boot a page that is only a page: markup and one script, no wasm and no
 * frame loop. `web/builder.html` is the first of them — the start menu, the
 * setup screen and the lobby run entirely in the host, so there is nothing
 * for the room's `boot()` above to instantiate or to step.
 *
 * Time still has to pass, though: the builder hangs remarks off `setTimeout`.
 * `advance(ms)` fires whatever is due, so a harness stays synchronous and
 * "four seconds" is a number rather than a wait. */
export function bootPage({
  html = "web/builder.html",
  host = "web/builder.js",
} = {}) {
  const { byId, doc, root } = makeDom(readFileSync(html, "utf8"), makeCtx([]));

  let clock = 0;
  const timers = new Map();
  let nextTimer = 1;

  const win = {
    devicePixelRatio: 1,
    innerWidth: 1280,
    innerHeight: 800,
    listeners: new Map(),
    addEventListener(type, fn) {
      if (!win.listeners.has(type)) win.listeners.set(type, []);
      win.listeners.get(type).push(fn);
    },
    removeEventListener() {},
    BIMS_BUILD: 1,
  };

  const sandbox = {
    document: doc,
    window: win,
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
    Error,
    // Deliberately no `navigator` and no `crypto`: both are missing in real
    // browsers often enough (http, older engines) that the host has to cope,
    // and the harness is where that is found out.
    performance: { now: () => clock },
    setTimeout(fn, ms) {
      const id = nextTimer++;
      timers.set(id, { at: clock + (ms ?? 0), fn });
      return id;
    },
    clearTimeout(id) {
      timers.delete(id);
    },
  };
  sandbox.globalThis = sandbox;

  const ctx = vm.createContext(sandbox);
  vm.runInContext(readFileSync(host, "utf8"), ctx, { filename: host });

  /** Let `ms` go by, firing anything due on the way. */
  function advance(ms) {
    const until = clock + ms;
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
      clock = t.at;
      t.fn();
    }
    clock = until;
  }

  function click(id, event = {}) {
    const el = byId.get(id);
    if (!el) throw new Error(`no element #${id}`);
    return el.dispatch("click", event);
  }

  function dispatch(id, type, event) {
    const el = byId.get(id);
    if (!el) throw new Error(`no element #${id}`);
    return el.dispatch(type, event);
  }

  return { byId, doc, root, win, click, dispatch, advance, timers };
}
