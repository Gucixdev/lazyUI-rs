// Spring physics for the DOM skin. A value chases its target like a mass on a damped spring, so things overshoot,
// settle and can be interrupted mid-flight without a jump. One requestAnimationFrame loop drives every spring and
// sleeps when all of them are at rest. No dependencies.
//
//   const m = new Motion();
//   m.to(el, "w", 42);              // width in %, from wherever it is now
//   m.to(el, "ty", 0, { from: 14, preset: "bouncy" });
//
// Props: w h tp (%: width height top), l t pw ph (px: left top width height), tx ty (px), s (scale), rx ry (deg), o (opacity), fg (flex-grow).
// `cnt` counts a number into el.textContent (data-suf is appended). `pts` springs a list of 0..1 values and calls
// el._draw(values) every frame (line charts).

export const PRESETS = {
  stiff: { k: 700, c: 53 },     // no overshoot: retro, terminals
  snappy: { k: 320, c: 28 },    // quick, a hint of life
  gentle: { k: 120, c: 20 },    // slow and smooth
  bouncy: { k: 210, c: 13 },    // clear overshoot
  wobbly: { k: 150, c: 8 },     // jelly
};

export class Motion {
  constructor(preset = "snappy") {
    this.preset = preset;
    this.live = new Set();
    this.raf = 0;
    this.last = 0;
    this.reduced = typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;
  }

  /// Chase `target` for one property of `el`. `from` is the start for a property seen the first time.
  to(el, prop, target, o = {}) {
    if (!Number.isFinite(target)) return;
    const m = el._m || (el._m = {});
    const p = PRESETS[o.preset || el._preset || this.preset] || PRESETS.snappy;
    let s = m[prop];
    if (!s) s = m[prop] = { x: Number.isFinite(o.from) ? o.from : target, v: 0, t: target, k: p.k, c: p.c };
    if (s.t === target && s.x === target && s.v === 0) return; // already there: no wake-up, no style write
    s.t = target; s.k = p.k; s.c = p.c;
    if (this.reduced) { s.x = target; s.v = 0; this.apply(el); return; }
    this.live.add(el);
    this.apply(el);
    this.wake();
  }

  /// Spring a whole list of values (a line chart): `draw(values)` gets the current ones each frame.
  pts(el, targets, draw, o = {}) {
    const p = PRESETS[o.preset || el._preset || this.preset] || PRESETS.snappy;
    let s = el._pts;
    if (!s || s.length !== targets.length) {
      const old = s || [];
      s = el._pts = targets.map((t, i) => ({ x: old[i] ? old[i].x : o.flat ? 0 : t, v: 0, t, k: p.k, c: p.c }));
    }
    targets.forEach((t, i) => { s[i].t = t; s[i].k = p.k; s[i].c = p.c; });
    el._draw = draw;
    if (this.reduced) { s.forEach((q) => { q.x = q.t; q.v = 0; }); draw(s.map((q) => q.x)); return; }
    this.live.add(el);
    draw(s.map((q) => q.x));
    this.wake();
  }

  /// Set a property now, cancel its spring (used when something must not animate, e.g. a hidden start state).
  snap(el, prop, value) {
    const m = el._m || (el._m = {});
    m[prop] = { x: value, v: 0, t: value, k: 1, c: 1 };
    this.apply(el);
  }

  wake() {
    if (this.raf) return;
    this.last = performance.now();
    this.raf = requestAnimationFrame((t) => this.frame(t));
  }

  frame(now) {
    this.raf = 0;
    const dt = Math.min(0.033, Math.max(0.001, (now - this.last) / 1000));
    this.last = now;
    for (const el of [...this.live]) {
      if (!el.isConnected) { this.live.delete(el); continue; }
      let moving = false;
      for (const s of [...Object.values(el._m || {}), ...(el._pts || [])]) if (this.step(s, dt)) moving = true;
      this.apply(el);
      if (el._pts && el._draw) el._draw(el._pts.map((q) => q.x));
      if (!moving) this.live.delete(el);
    }
    if (this.live.size) this.raf = requestAnimationFrame((t) => this.frame(t));
  }

  /// Semi-implicit Euler in small slices (stable at any frame time). True while still moving.
  step(s, dt) {
    const n = Math.ceil(dt / 0.004), h = dt / n;
    for (let i = 0; i < n; i++) {
      s.v += (-s.k * (s.x - s.t) - s.c * s.v) * h;
      s.x += s.v * h;
    }
    const eps = 0.0008 * Math.max(1, Math.abs(s.t));
    if (Math.abs(s.x - s.t) < eps && Math.abs(s.v) < eps * 8) { s.x = s.t; s.v = 0; return false; }
    return true;
  }

  apply(el) {
    const m = el._m;
    if (!m) return;
    const st = el.style;
    let tf = false;
    for (const [k, s] of Object.entries(m)) {
      const v = s.x;
      switch (k) {
        case "w": st.width = v + "%"; break;
        case "h": st.height = v + "%"; break;
        case "fg": st.flexGrow = v; break;
        case "l": st.left = v + "px"; break;
        case "t": st.top = v + "px"; break;
        case "tp": st.top = v + "%"; break;
        case "pw": st.width = Math.max(0, v) + "px"; break;
        case "ph": st.height = Math.max(0, v) + "px"; break;
        case "o": st.opacity = Math.max(0, Math.min(1, v)); break;
        case "cnt": el.textContent = Math.round(v) + (el.dataset.suf || ""); break;
        default: tf = true;
      }
    }
    if (tf) {
      const g = (k, d) => (m[k] ? m[k].x : d);
      const persp = m.rx || m.ry ? "perspective(700px) " : "";
      const idle = !g("tx", 0) && !g("ty", 0) && !g("rx", 0) && !g("ry", 0) && g("s", 1) === 1;
      if (idle) { st.transform = ""; return; } // a resting element gets no transform (no stacking context, crisp text)
      st.transform = `${persp}translate(${g("tx", 0)}px,${g("ty", 0)}px) rotateX(${g("rx", 0)}deg) rotateY(${g("ry", 0)}deg) scale(${g("s", 1)})`;
    }
  }
}
