// Things that move on their own in the DOM skin: matrix rain and scrambling text. Both are pure functions of the clock,
// like their terminal twins in src/fancy.rs, so one loop paints every instance on the page.

const hash = (x) => { x >>>= 0; x ^= x >>> 16; x = Math.imul(x, 0x7feb352d); x ^= x >>> 15; x = Math.imul(x, 0x846ca68b); x ^= x >>> 16; return x >>> 0; };
const KANA = (k) => String.fromCharCode(0xff71 + (k - 14) % 45);
export function glyph(h) {
  const k = h % 60;
  return k < 10 ? String(k) : k < 14 ? "=+*:"[k - 10] : KANA(k);
}

function rain(cv, t, css) {
  const dpr = Math.min(2, window.devicePixelRatio || 1), fs = 14, w = cv.clientWidth, h = cv.clientHeight;
  if (w < 4 || h < 4) return;
  if (cv.width !== Math.round(w * dpr) || cv.height !== Math.round(h * dpr)) { cv.width = Math.round(w * dpr); cv.height = Math.round(h * dpr); }
  const g = cv.getContext("2d"), cw = fs * 0.9, cols = Math.floor(w / cw), rows = Math.floor(h / fs), seed = +cv.dataset.seed || 1, k = +cv.dataset.speed || 1;
  g.setTransform(dpr, 0, 0, dpr, 0, 0);
  g.clearRect(0, 0, w, h);
  g.font = `${fs}px ui-monospace, monospace`; g.textBaseline = "top";
  for (let x = 0; x < cols; x++) {
    const hh = hash(Math.imul(seed, 2654435761) + x * 97 + 13), speed = (4 + (hh % 900) / 100) * k, trail = 5 + ((hh >>> 10) % 10), gap = 4 + ((hh >>> 16) % 14);
    const period = rows + trail + gap, hy = Math.floor((((t * speed + ((hh >>> 20) % 1000) / 1000 * period) % period) + period) % period);
    for (let j = 0; j <= trail; j++) {
      const y = hy - j;
      if (y < 0 || y >= rows) continue;
      const gl = glyph(hash(hh + y * 31 + Math.floor(t * (j === 0 ? 12 : 2.5)) * 7));
      g.globalAlpha = j === 0 ? 1 : Math.max(0.05, 1 - (j / (trail + 1)) * 0.92);
      g.fillStyle = j === 0 ? css.head : css.body;
      g.fillText(gl, x * cw, y * fs);
    }
  }
  g.globalAlpha = 1;
}

function scramble(el, t) {
  const text = el.dataset.text || "", auto = el.dataset.progress === "auto";
  const p = Math.max(0, Math.min(1, auto ? ((t % 5) / 2.5) : +el.dataset.progress || 0));
  const done = Math.floor(p * text.length + 1e-9);
  let out = "";
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (i < done) out += `<b>${c.replace(/[&<]/g, (m) => (m === "&" ? "&amp;" : "&lt;"))}</b>`;
    else if (i === done && p < 1) out += `<i class="cur">${glyph(hash(i * 977 + Math.floor(t * 20)))}</i>`;
    else if (c !== " ") out += `<i>${glyph(hash(i * 977 + Math.floor(t * 12)))}</i>`;
    else out += " ";
  }
  if (el.dataset.out !== out) { el.dataset.out = out; el.innerHTML = out; }
}

// starts one animation-frame loop for a skin root; it sleeps (no frames) while the page has neither widget
export function ambient(root) {
  let raf = 0, dead = false;
  const loop = (now) => {
    if (dead) return;
    const rains = root.querySelectorAll(".rain canvas"), scr = root.querySelectorAll(".scr");
    if (!rains.length && !scr.length) { raf = 0; return; }
    const t = now / 1000, st = getComputedStyle(root);
    const css = { head: "#e8fff0", body: st.getPropertyValue("--ok").trim() || "#5ae17d" };
    for (const cv of rains) rain(cv, t, css);
    for (const el of scr) scramble(el, t);
    raf = requestAnimationFrame(loop);
  };
  return { wake() { if (!raf && !dead) raf = requestAnimationFrame(loop); }, stop() { dead = true; cancelAnimationFrame(raf); } };
}
