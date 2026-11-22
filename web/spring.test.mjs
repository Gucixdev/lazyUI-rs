// The spring maths without a browser: converges, overshoots only when it should, and an interrupted spring keeps its speed.
import assert from "node:assert/strict";
import { Motion, PRESETS } from "./spring.js";

const m = new Motion();
const run = (name, from, to) => {
  const s = { x: from, v: 0, t: to, k: PRESETS[name].k, c: PRESETS[name].c };
  let peak = from, t = 0;
  while (m.step(s, 1 / 60) && t < 10) { t += 1 / 60; peak = Math.max(peak, s.x); }
  return { s, t, peak };
};
for (const name of Object.keys(PRESETS)) { const r = run(name, 0, 100); assert.equal(r.s.x, 100, name + " lands exactly"); assert.ok(r.t < 4, name + " settles in " + r.t.toFixed(2) + "s"); }
assert.ok(run("stiff", 0, 100).peak <= 100.5, "stiff does not overshoot");
assert.ok(run("bouncy", 0, 100).peak > 105, "bouncy overshoots");
assert.ok(run("wobbly", 0, 100).peak > run("bouncy", 0, 100).peak, "wobbly overshoots more");
// a huge frame stalls (tab in background) must not explode
const s = { x: 0, v: 0, t: 1, k: 320, c: 28 };
m.step(s, 5); assert.ok(Number.isFinite(s.x) && Math.abs(s.x) < 5, "stable on long frames: " + s.x);
// retargeting mid-flight keeps velocity (no jump)
const a = { x: 0, v: 0, t: 100, k: 320, c: 28 };
for (let i = 0; i < 6; i++) m.step(a, 1 / 60);
const x0 = a.x, v0 = a.v; a.t = 0; m.step(a, 1 / 600);
assert.ok(Math.abs(a.x - x0) < Math.abs(v0) / 600 * 1.5 + 0.01, "continuous on retarget");
console.log("spring ok");
