// lazyui in a browser tab: the wasm engine decides what is on screen, this file paints cells on a <canvas> and turns
// DOM events into lazyui events. No dependencies, no build step. See README "Web".
//
//   import { mount } from "./lazyui.js";
//   const ui = await mount(canvas, { wasm: "lazyui.wasm", spec, widgets: { mycard: "params t\npanel title=$t\n  slot" } });
//   ui.set("cpu", 42); ui.on("activate", e => ...); ui.key("ctrl-r"); ui.theme("green"); ui.reload(newSpec);

const CELL_WORDS = 4;
const BOLD = 1, DIM = 2, ITALIC = 4, UNDERLINE = 8, REVERSE = 16;

const ANSI = ["#000000", "#cd3131", "#0dbc79", "#e5e510", "#2472c8", "#bc3fbc", "#11a8cd", "#e5e5e5",
              "#666666", "#f14c4c", "#23d18b", "#f5f543", "#3b8eea", "#d670d6", "#29b8db", "#ffffff"];
function palette(i) {
  if (i < 16) return ANSI[i];
  if (i < 232) {
    const n = i - 16, v = [0, 95, 135, 175, 215, 255];
    return `rgb(${v[(n / 36) | 0]},${v[((n / 6) | 0) % 6]},${v[n % 6]})`;
  }
  const g = 8 + (i - 232) * 10;
  return `rgb(${g},${g},${g})`;
}
function css(word, dflt) {
  if (word === 0) return dflt;
  if ((word >>> 24) === 1) return palette(word & 255);
  return "#" + (word & 0xffffff).toString(16).padStart(6, "0");
}

const KEYS = { Enter: "enter", Tab: "tab", Escape: "esc", Backspace: "backspace", Delete: "delete", Insert: "insert",
  ArrowUp: "up", ArrowDown: "down", ArrowLeft: "left", ArrowRight: "right", Home: "home", End: "end",
  PageUp: "pgup", PageDown: "pgdn" };

export async function mount(canvas, o = {}) {
  const enc = new TextEncoder(), dec = new TextDecoder();
  const bytes = await (await fetch(o.wasm || "lazyui.wasm")).arrayBuffer();
  const { instance } = await WebAssembly.instantiate(bytes, {});
  const x = instance.exports;
  const mem = () => new Uint8Array(x.memory.buffer);

  const put = (s) => { const b = enc.encode(s); const p = x.lz_alloc(b.length); mem().set(b, p); return [p, b.length]; };
  const call = (f, s, ...rest) => { const [p, n] = put(s); try { return f(p, n, ...rest); } finally { x.lz_free(p, n); } };
  const err = () => { const n = x.lz_error(); return dec.decode(mem().subarray(x.lz_out_ptr(), x.lz_out_ptr() + n)); };

  const font = o.font || "ui-monospace, 'DejaVu Sans Mono', Menlo, Consolas, monospace";
  const px = o.fontSize || 16;
  const dpr = () => window.devicePixelRatio || 1;
  const ctx = canvas.getContext("2d", { alpha: false });
  const style = getComputedStyle(canvas);
  const dfg = o.fg || style.color || "#cde6cd", dbg = o.bg || "#0b0f0c";
  let cw = 0, ch = 0, cols = 0, rows = 0;
  const handlers = [];

  function metrics() {
    ctx.font = `${px}px ${font}`;
    cw = Math.max(1, Math.round(ctx.measureText("M").width * dpr())) / dpr();
    ch = Math.ceil(px * 1.25);
  }
  function fit() {
    const r = canvas.getBoundingClientRect();
    const c = Math.max(1, Math.floor(r.width / cw)), l = Math.max(1, Math.floor(r.height / ch));
    canvas.width = Math.round(r.width * dpr());
    canvas.height = Math.round(r.height * dpr());
    cols = c; rows = l;
    x.lz_resize(cols, rows);
    paintAll = true;
  }

  for (const [name, text] of Object.entries(o.widgets || {})) {
    const [a, an] = put(name), [b, bn] = put(text);
    const ok = x.lz_widget(a, an, b, bn); x.lz_free(a, an); x.lz_free(b, bn);
    if (ok !== 1) throw new Error(`lazyui widget ${name}: ` + err());
  }
  if (call(x.lz_init, o.spec || "text text=\"no spec\"") !== 1) throw new Error("lazyui spec: " + err());
  if (o.theme) call(x.lz_theme, o.theme);
  metrics();
  if (!canvas.hasAttribute("tabindex")) canvas.tabIndex = 0;
  canvas.style.outline = "none";

  let paintAll = true, prev = null;
  function paint() {
    const n = x.lz_frame_len(), f = new Uint32Array(x.memory.buffer, x.lz_frame_ptr(), n);
    const full = paintAll || !prev || prev.length !== n;
    ctx.setTransform(dpr(), 0, 0, dpr(), 0, 0);
    ctx.textBaseline = "alphabetic";
    for (let r = 0; r < rows; r++) {
      for (let c = 0; c < cols; c++) {
        const i = (r * cols + c) * CELL_WORDS;
        if (!full && f[i] === prev[i] && f[i + 1] === prev[i + 1] && f[i + 2] === prev[i + 2] && f[i + 3] === prev[i + 3]) continue;
        const code = f[i], mods = f[i + 3];
        let fg = css(f[i + 1], dfg), bg = css(f[i + 2], dbg);
        if (mods & REVERSE) [fg, bg] = [bg, fg];
        ctx.fillStyle = bg;
        ctx.fillRect(c * cw, r * ch, cw + 0.5, ch);
        if (code > 32) {
          ctx.globalAlpha = mods & DIM ? 0.6 : 1;
          ctx.fillStyle = fg;
          ctx.font = `${mods & ITALIC ? "italic " : ""}${mods & BOLD ? "bold " : ""}${px}px ${font}`;
          ctx.fillText(String.fromCodePoint(code), c * cw, r * ch + px);
          ctx.globalAlpha = 1;
        }
        if (mods & UNDERLINE) { ctx.fillStyle = fg; ctx.fillRect(c * cw, r * ch + ch - 2, cw, 1); }
      }
    }
    prev = f.slice();
    paintAll = false;
  }

  let last = performance.now(), timer = 0, raf = 0, dead = false;
  function pump() {
    if (dead) return;
    clearTimeout(timer); cancelAnimationFrame(raf); timer = raf = 0;
    const now = performance.now();
    x.lz_tick(now - last); last = now;
    if (x.lz_render() || paintAll) paint();
    const n = x.lz_events();
    if (n) {
      const text = dec.decode(mem().slice(x.lz_out_ptr(), x.lz_out_ptr() + n));
      for (const line of text.split("\n")) {
        if (!line) continue;
        const e = JSON.parse(line);
        for (const h of handlers) if (!h.kind || h.kind === e.kind) h.fn(e);
        canvas.dispatchEvent(new CustomEvent("lazyui", { detail: e }));
      }
    }
    const w = x.lz_wake_ms();
    if (w < 0) return;               // nothing will happen without input: sleep
    if (w <= 17) raf = requestAnimationFrame(pump);
    else timer = setTimeout(pump, w);
  }

  const mods = (e) => (e.ctrlKey ? 1 : 0) | (e.altKey ? 2 : 0) | (e.shiftKey ? 4 : 0);
  function onKey(e) {
    if (e.isComposing || e.metaKey) return;
    let handled = 0;
    const named = KEYS[e.key] || (/^F\d+$/.test(e.key) ? e.key.toLowerCase() : null);
    if (named) {
      const name = (e.ctrlKey ? "ctrl-" : "") + (e.altKey ? "alt-" : "") + (e.shiftKey ? "shift-" : "") + named;
      handled = call(x.lz_key, name);
    } else if (e.key.length === 1 || e.key === "Dead") {
      if (e.ctrlKey || e.altKey) {
        if (e.ctrlKey && (e.key === "v" || e.key === "c" || e.key === "x")) return; // the browser's own clipboard
        const k = e.key === " " ? "space" : e.key.toLowerCase();
        handled = call(x.lz_key, (e.ctrlKey ? "ctrl-" : "") + (e.altKey ? "alt-" : "") + k);
      } else {
        call(x.lz_text, e.key); handled = 1;
      }
    }
    if (handled) { e.preventDefault(); pump(); }
  }
  function cell(e) {
    const r = canvas.getBoundingClientRect();
    return [Math.max(0, Math.min(cols - 1, Math.floor((e.clientX - r.left) / cw))), Math.max(0, Math.min(rows - 1, Math.floor((e.clientY - r.top) / ch)))];
  }
  const btn = (e) => (e.button === 1 ? 1 : e.button === 2 ? 2 : 0);
  const on = (t, ev, fn, opt) => { t.addEventListener(ev, fn, opt); return () => t.removeEventListener(ev, fn, opt); };
  let drag = false;
  const off = [
    on(canvas, "keydown", onKey),
    on(canvas, "mousedown", (e) => { canvas.focus(); const [c, r] = cell(e); drag = true; x.lz_mouse(0, btn(e), c, r, mods(e)); pump(); e.preventDefault(); }),
    on(window, "mouseup", (e) => { if (!drag) return; drag = false; const [c, r] = cell(e); x.lz_mouse(1, btn(e), c, r, mods(e)); pump(); }),
    on(canvas, "mousemove", (e) => { const [c, r] = cell(e); x.lz_mouse(drag ? 2 : 3, 0, c, r, mods(e)); pump(); }),
    on(canvas, "wheel", (e) => { const [c, r] = cell(e); x.lz_mouse(e.deltaY < 0 ? 4 : 5, 0, c, r, mods(e)); pump(); e.preventDefault(); }, { passive: false }),
    on(canvas, "contextmenu", (e) => e.preventDefault()),
    on(canvas, "paste", (e) => { const t = e.clipboardData && e.clipboardData.getData("text"); if (t) { call(x.lz_paste, t); pump(); } e.preventDefault(); }),
    on(window, "resize", () => { metrics(); fit(); pump(); }),
    on(document, "visibilitychange", () => { if (!document.hidden) pump(); }),
  ];
  const ro = typeof ResizeObserver === "function" ? new ResizeObserver(() => { fit(); pump(); }) : null;
  if (ro) ro.observe(canvas);

  fit();
  pump();

  return {
    get size() { return [cols, rows]; },
    set(key, value) { const [a, an] = put(key), [b, bn] = put(String(value)); x.lz_set(a, an, b, bn); x.lz_free(a, an); x.lz_free(b, bn); pump(); },
    key(name) { const ok = call(x.lz_key, name) === 1; pump(); return ok; },
    text(s) { call(x.lz_text, s); pump(); },
    theme(name) { const ok = call(x.lz_theme, name) === 1; paintAll = true; pump(); return ok; },
    reload(spec) { const ok = call(x.lz_reload, spec) === 1; paintAll = true; pump(); if (!ok) throw new Error("lazyui spec: " + err()); },
    on(kind, fn) { handlers.push(typeof kind === "function" ? { fn: kind } : { kind, fn }); },
    frameText() {
      const n = x.lz_frame_len(), f = new Uint32Array(x.memory.buffer, x.lz_frame_ptr(), n);
      let s = "";
      for (let r = 0; r < rows; r++) { let l = ""; for (let c = 0; c < cols; c++) { const k = f[(r * cols + c) * CELL_WORDS]; if (k) l += String.fromCodePoint(k); } s += l.trimEnd() + "\n"; }
      return s;
    },
    destroy() { dead = true; clearTimeout(timer); cancelAnimationFrame(raf); off.forEach((f) => f()); if (ro) ro.disconnect(); },
  };
}
