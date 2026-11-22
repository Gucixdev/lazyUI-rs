// Drives the built wasm through its C ABI, the same calls lazyui.js makes. No browser needed: `node web/test.mjs`
import { readFileSync } from "node:fs";
import assert from "node:assert/strict";

const wasm = new URL("./dist/lazyui.wasm", import.meta.url);
const { instance } = await WebAssembly.instantiate(readFileSync(wasm), {});
const x = instance.exports, enc = new TextEncoder(), dec = new TextDecoder();
const mem = () => new Uint8Array(x.memory.buffer);
const call = (f, s, ...r) => { const b = enc.encode(s), p = x.lz_alloc(b.length); mem().set(b, p); try { return f(p, b.length, ...r); } finally { x.lz_free(p, b.length); } };
const out = (n) => dec.decode(mem().slice(x.lz_out_ptr(), x.lz_out_ptr() + n));
const text = (w, h) => { const f = new Uint32Array(x.memory.buffer, x.lz_frame_ptr(), x.lz_frame_len()); let s = ""; for (let r = 0; r < h; r++) { let l = ""; for (let c = 0; c < w; c++) { const k = f[(r * w + c) * 4]; if (k) l += String.fromCodePoint(k); } s += l.trimEnd() + "\n"; } return s; };

const spec = readFileSync(new URL("../examples/demo.lui", import.meta.url), "utf8");
assert.equal(call(x.lz_init, spec), 1);
x.lz_resize(40, 12);
assert.equal(x.lz_render(), 1, "first frame is new");
assert.equal(x.lz_frame_len(), 40 * 12 * 4);
assert.ok(text(40, 12).includes("alpha"), text(40, 12));

assert.equal(call(x.lz_key, "tab"), 1); // first focus is the tab bar; the search field is next
call(x.lz_text, "ta");
x.lz_render();
const t = text(40, 12);
assert.ok(t.includes("beta") && !t.includes("alpha"), t);
let n = x.lz_events();
assert.match(out(n), /"id":"q","kind":"change","text":"ta"/);

assert.equal(call(x.lz_key, "tab"), 1);
assert.equal(call(x.lz_key, "hyper-x"), 0, "unknown key names are refused");
{ const k = enc.encode("cpu"), v = enc.encode("50"), a = x.lz_alloc(3), b = x.lz_alloc(2); mem().set(k, a); mem().set(v, b); x.lz_set(a, 3, b, 2); x.lz_free(a, 3); x.lz_free(b, 2); }
for (let i = 0; i < 90; i++) { x.lz_tick(16); x.lz_render(); }
assert.ok(text(40, 12).includes("50%"), text(40, 12));

assert.equal(call(x.lz_reload, "bogus"), 0);
n = x.lz_error(); assert.ok(out(n).length > 0, "a bad spec reports why");
assert.equal(x.lz_render(), 0, "and keeps the old frame");

// idle: nothing animating, no timers -> the page may sleep
for (let i = 0; i < 100; i++) x.lz_tick(16);
x.lz_render();
assert.equal(x.lz_wake_ms(), -1);
console.log("web wasm ok:", x.lz_frame_len(), "words,", readFileSync(wasm).length, "bytes");
