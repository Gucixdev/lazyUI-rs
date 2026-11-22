// The DOM skin: the same wasm engine, drawn as a modern app (cards, segmented controls, SVG charts, icons, light and
// dark) instead of a character grid. The engine still owns state, focus, keys and events; this file only turns its
// `describe` tree into HTML and sends semantic actions back. No dependencies, no build step.
//
//   import { mountSkin } from "./skin.js";
//   const ui = await mountSkin(document.getElementById("app"), { wasm: "lazyui.wasm", spec, flavor: "mac", scheme: "auto" });
//   ui.set("cpu", 42); ui.on("activate", e => ...); ui.key("ctrl-r"); ui.scheme("dark");
//
// look: terminal | classic | flat | modern | glass | neon | aurora (see looks.js); `flavor` "mac"/"win" still works as an
// alias for modern/flat. scheme: "light" | "dark" | "auto" for the looks that have both. Everything that moves is a
// spring (spring.js): bars, counters, charts, the sliding highlight, panel entrances, hover tilt.

import { engine } from "./lazyui.js";
import { ambient } from "./fancy.js";
import { Motion } from "./spring.js";
import { LOOKS, CSS } from "./looks.js";

const ICON = {
  search: "M11 4a7 7 0 1 0 0 14 7 7 0 0 0 0-14zM21 21l-4.3-4.3",
  folder: "M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z",
  file: "M6 3h8l5 5v13H6zM14 3v5h5",
  chevR: "M9 6l6 6-6 6",
  chevD: "M6 9l6 6 6-6",
  updown: "M8 9l4-4 4 4M8 15l4 4 4-4",
  check: "M5 12l5 5 9-10",
  play: "M8 5v14l11-7z",
  pause: "M8 5v14M16 5v14",
  reset: "M4 12a8 8 0 1 0 3-6.2M4 4v5h5",
  clock: "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM12 7v5l3 2",
  up: "M6 15l6-6 6 6",
  down: "M6 9l6 6 6-6",
  home: "M4 11l8-7 8 7M6 10v10h12V10",
  gear: "M12 9a3 3 0 1 0 0 6 3 3 0 0 0 0-6zM12 3v3M12 18v3M3 12h3M18 12h3M5.6 5.6l2.1 2.1M16.3 16.3l2.1 2.1M18.4 5.6l-2.1 2.1M7.7 16.3l-2.1 2.1",
  chart: "M4 20V10M10 20V4M16 20v-7M22 20H2",
  list: "M8 6h13M8 12h13M8 18h13M3.5 6h.01M3.5 12h.01M3.5 18h.01",
  user: "M12 4a4 4 0 1 0 0 8 4 4 0 0 0 0-8zM4 21a8 8 0 0 1 16 0",
  star: "M12 3l2.7 5.6 6.1.9-4.4 4.3 1 6.1L12 17l-5.4 2.9 1-6.1L3.2 9.5l6.1-.9z",
  bolt: "M13 3L5 14h6l-1 7 8-11h-6z",
  cpu: "M7 7h10v10H7zM10 3v4M14 3v4M10 17v4M14 17v4M3 10h4M3 14h4M17 10h4M17 14h4",
  terminal: "M4 5h16v14H4zM7 9l3 3-3 3M12 15h5",
  info: "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM12 11v5M12 8h.01",
  keyboard: "M3 6h18v12H3zM7 10h.01M11 10h.01M15 10h.01M7 14h10",
  globe: "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM3 12h18M12 3c3 3 3 15 0 18M12 3c-3 3-3 15 0 18",
};
const icon = (n, cls = "") => (ICON[n] ? `<svg class="ic ${cls}" viewBox="0 0 24 24"><path d="${ICON[n]}"/></svg>` : "");

const esc = (s) => String(s ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
const num = (v, d = 0) => { const n = parseFloat(v); return Number.isFinite(n) ? n : d; };
const csv = (s) => (s ? String(s).split(",").map((x) => x.trim()) : []);
const clamp01 = (v) => Math.max(0, Math.min(1, v));

const NAMED = { accent: "var(--accent)", ok: "var(--ok)", warn: "var(--warn)", err: "var(--err)", dim: "var(--dim)", fg: "var(--fg)" };
const CYCLE = ["var(--accent)", "var(--ok)", "var(--warn)", "var(--err)", "var(--c5)", "var(--c6)"];
const tone = (c, d = "accent") => (c && c.startsWith("#") ? c : NAMED[c] || NAMED[d]);
const level = (f, bad = false) => { const v = bad ? 1 - f : f; return v < 0.6 ? "var(--ok)" : v < 0.85 ? "var(--warn)" : "var(--err)"; };

function styleOnce() {
  if (document.getElementById("lz-skin-css")) return;
  const s = document.createElement("style");
  s.id = "lz-skin-css";
  s.textContent = CSS;
  document.head.appendChild(s);
}

// ---- tree -> HTML -------------------------------------------------------------------------------------------------

const ONE_ROW = new Set(["scramble", "text", "progressbar", "searchbar", "buttonbar", "keybar", "gauge", "ringbar", "spinner", "clock", "timer", "dropdown", "segbar"]);
const CELL_H = 22;

// does this subtree want the leftover space? (a list does, a row of one-line widgets does not)
function flexy(n) {
  const sz = (n.attrs || {}).size;
  if (sz != null && sz !== "") return !/^\d+$/.test(sz);
  if (n.kind === "spacer") return true;
  if (n.kids && n.kids.length) return n.kids.some(flexy);
  return ["list", "tree", "table", "worldmap", "globe", "minimap", "rain"].includes(n.kind);
}

function sizeStyle(n, dir) {
  const a = n.attrs || {};
  const sz = a.size;
  if (sz == null || sz === "") return flexy(n) ? "flex:1 1 0" : "flex:none";
  if (sz === "fill") return "flex:1 1 0";
  let m;
  if ((m = /^fill:(\d+)$/.exec(sz))) return `flex:${m[1]} 1 0`;
  if ((m = /^pct:(\d+)$/.exec(sz))) return `flex:0 0 ${m[1]}%`;
  if ((m = /^(?:min|max)?:?(\d+)$/.exec(sz))) {
    const v = +m[1];
    return dir === "col" ? `flex:none;min-height:${v * CELL_H}px` : `flex:none;min-width:${v}ch`;
  }
  return "flex:1 1 0";
}

function rowsOf(n, dflt) {
  const s = (n.attrs || {}).size;
  return /^\d+$/.test(s || "") ? +s : dflt;
}

function renderNode(n, dir, st) {
  const a = n.attrs || {}, s = n.st || {};
  const id = esc(n.id), foc = n.focused ? " focus" : "";
  const gap = a.gap != null ? `gap:${6 + num(a.gap) * 6}px;` : "";
  const sz = n.kind === "spacer" ? "flex:1 1 0" : sizeStyle(n, dir);
  const kids = (cls, d) => (n.kids || []).map((k) => renderNode(k, d, st)).join("");
  switch (n.kind) {
    case "col": return `<div class="col" style="${sz};${gap}">${kids("col", "col")}</div>`;
    case "row": return `<div class="row" style="${sz};${gap}">${kids("row", "row")}</div>`;
    case "pages": return `<div class="pages" style="${sz}">${(n.kids || []).map((k) => `<div class="pg" data-blk="${esc(k.id)}">${renderNode(k, "col", st)}</div>`).join("")}</div>`;
    case "panel": {
      const head = a.title || a.icon ? `<div class="ph">${icon(a.icon)}${esc(a.title)}</div>` : "";
      const k = Math.min(st.n++, 8);
      return `<div class="panel" style="${sz}" data-sp="ty:0;o:1" data-from="ty:20;o:0" data-delay="${k * 65}">${head}<div class="pb" style="${gap}">${kids("panel", "col")}</div></div>`;
    }
    case "spacer": return `<div class="spacer" style="${sz}"></div>`;
    case "text": {
      const cls = ["text", a.style || "", a.align || ""].join(" ");
      return `<div class="${cls}" style="${sz}">${esc(s.text)}</div>`;
    }
    case "progressbar": {
      const v = clamp01(s.v);
      const col = a.color ? `--c:${tone(a.color)}` : "";
      return `<div class="prog" style="${sz}"><div class="trk"><i data-sp="w:${(v * 100).toFixed(2)}" style="${col}"></i></div><span>${esc(s.label)}</span></div>`;
    }
    case "searchbar":
      return `<label class="search${foc}" style="${sz}" data-id="${id}" data-act="focus">${icon("search")}<input data-id="${id}" data-act="text" value="${esc(s.value)}" placeholder="${esc((a.prompt || "").replace(/[>:]\s*$/, "") || "Search")}" spellcheck="false" autocomplete="off"></label>`;
    case "buttonbar": {
      const icons = csv(a.icons);
      const b = (s.items || []).map((t, i) => `<button class="${i === s.active ? "on" : ""}" data-id="${id}" data-act="activate" data-arg="${i}">${icon(icons[i])}${esc(t)}</button>`).join("");
      return `<div class="seg${foc}" style="${sz}"><u class="thumb"></u>${b}</div>`;
    }
    case "list": {
      const icons = csv(a.icons);
      const items = s.items || [];
      const rows = items.map((t, i) => `<div class="li${i === s.cursor ? " sel" : ""}" data-id="${id}" data-act="select" data-dbl="activate" data-arg="${i}" data-pos="${i}">${icon(icons[0] || (a.icon || ""))}${esc(t)}</div>`).join("");
      return `<div class="listbox${foc}" style="${sz}" data-id="${id}" data-cursor="${s.cursor}" data-hl><u class="hl"></u>${rows || '<div class="empty">nothing here</div>'}</div>`;
    }
    case "keybar": {
      const hints = (a.hints || "").split(";").filter(Boolean).map((h) => { const [k, ...r] = h.split(":"); return [k, r.join(":")]; });
      return `<div class="keybar" style="${sz}">${hints.map(([k, d]) => `<b data-key="${esc(k)}"><kbd>${esc(k)}</kbd>${esc(d)}</b>`).join("")}</div>`;
    }
    default: return renderCustom(n, dir, sz, st);
  }
}

function renderCustom(n, dir, sz, st) {
  const a = n.attrs || {}, s = n.st || {}, id = esc(n.id), foc = n.focused ? " focus" : "";
  const series = s.series || [];
  const lo = a.min != null ? num(a.min) : Math.min(0, ...series, 0), hi = a.max != null ? num(a.max) : Math.max(...series, lo + 1e-9);
  const norm = (v) => clamp01((v - lo) / (hi - lo || 1));
  const H = rowsOf(n, 4) * CELL_H;
  const col = tone(a.color);
  const labels = csv(a.labels), colors = csv(a.colors);
  const seriesColor = (i) => (colors[i] ? tone(colors[i]) : CYCLE[i % CYCLE.length]);
  switch (n.kind) {
    case "graph": {
      const w = Math.max(series.length, 1), bw = 100 / w;
      const bars = series.map((v, i) => `<rect x="${(i * bw + bw * 0.12).toFixed(2)}" y="${(100 - norm(v) * 100).toFixed(2)}" width="${(bw * 0.76).toFixed(2)}" height="${(norm(v) * 100).toFixed(2)}" rx="1.2" fill="${col}" opacity="${(0.4 + 0.6 * (i + 1) / w).toFixed(2)}"/>`).join("");
      return `<div class="chart" style="${sz};height:${H}px"><svg class="chart" viewBox="0 0 100 100" preserveAspectRatio="none" style="height:100%">${bars}</svg></div>`;
    }
    case "linechart": {
      const w = Math.max(series.length - 1, 1);
      const pts = series.map((v, i) => `${(i / w * 100).toFixed(2)},${(96 - norm(v) * 92).toFixed(2)}`);
      const gid = "g" + id.replace(/\W/g, "");
      return `<div class="chart" style="${sz};height:${H}px" data-pts="${series.map((v) => norm(v).toFixed(4)).join(",")}"><svg class="chart" viewBox="0 0 100 100" preserveAspectRatio="none" style="height:100%"><defs><linearGradient id="${gid}" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="${col}" stop-opacity=".35"/><stop offset="1" stop-color="${col}" stop-opacity="0"/></linearGradient></defs>${pts.length ? `<polygon class="ar" points="0,100 ${pts.join(" ")} 100,100" fill="url(#${gid})"/><polyline class="ln" points="${pts.join(" ")}" fill="none" stroke="${col}" stroke-width="2" vector-effect="non-scaling-stroke" stroke-linejoin="round" stroke-linecap="round"/>` : ""}</svg></div>`;
    }
    case "barchart": {
      const bars = series.map((v, i) => `<div class="bc"><span class="bv">${a.show ? esc(v) : ""}</span><i data-sp="h:${(norm(v) * 100).toFixed(2)}" data-delay="${Math.min(i, 24) * 35}" style="--c:${col}"></i><span class="bl">${esc(labels[i] ?? "")}</span></div>`).join("");
      return `<div class="bars" style="${sz};height:${Math.max(H, 110)}px">${bars}</div>`;
    }
    case "histogram": {
      const bins = Math.max(2, Math.min(40, num(a.bins, 8))), cnt = new Array(bins).fill(0);
      const l = a.min != null ? num(a.min) : Math.min(...series, 0), h = a.max != null ? num(a.max) : Math.max(...series, l + 1);
      for (const v of series) cnt[Math.min(bins - 1, Math.max(0, Math.floor((v - l) / (h - l || 1) * bins)))]++;
      const m = Math.max(...cnt, 1), bw = 100 / bins;
      const bars = cnt.map((c, i) => `<rect x="${(i * bw + 0.8).toFixed(2)}" y="${(92 - c / m * 86).toFixed(2)}" width="${(bw - 1.6).toFixed(2)}" height="${(c / m * 86).toFixed(2)}" rx="1.5" fill="${col}"/>${a.show ? `<text x="${(i * bw + bw / 2).toFixed(2)}" y="${(89 - c / m * 86).toFixed(2)}" text-anchor="middle" style="font-size:4px">${c || ""}</text>` : ""}`).join("");
      return `<div class="chart" style="${sz};height:${Math.max(H, 90)}px"><svg class="chart" viewBox="0 0 100 100" preserveAspectRatio="none" style="height:100%">${bars}</svg></div>`;
    }
    case "heatgrid": {
      const c = Math.max(1, num(a.cols, Math.ceil(Math.sqrt(series.length)))), r = Math.ceil(series.length / c) || 1;
      const cells = series.map((v, i) => `<rect x="${(i % c) * 10}" y="${Math.floor(i / c) * 10}" width="9" height="9" rx="2" fill="${col}" opacity="${(0.08 + 0.92 * norm(v)).toFixed(2)}"/>`).join("");
      return `<div class="chart" style="${sz};height:${Math.max(H, r * 18)}px"><svg class="chart" viewBox="0 0 ${c * 10} ${r * 10}" preserveAspectRatio="xMinYMin meet" style="height:100%">${cells}</svg></div>`;
    }
    case "zonebar": case "piebar": {
      const tot = series.reduce((p, v) => p + Math.max(0, v), 0) || 1;
      const legend = a.legend || n.kind === "piebar" ? `<div class="legend">${series.map((v, i) => `<span><i style="background:${seriesColor(i)}"></i>${esc(labels[i] ?? i + 1)} ${(v / tot * 100).toFixed(0)}%</span>`).join("")}</div>` : "";
      if (n.kind === "piebar") {
        let acc = 0;
        const arcs = series.map((v, i) => {
          const f = Math.max(0, v) / tot, r = 15.9;
          const d = `<circle cx="20" cy="20" r="${r}" fill="none" stroke="${seriesColor(i)}" stroke-width="6" stroke-dasharray="${(f * 100).toFixed(2)} ${(100 - f * 100).toFixed(2)}" stroke-dashoffset="${(-acc * 100).toFixed(2)}" transform="rotate(-90 20 20)" pathLength="100"/>`;
          acc += f; return d;
        }).join("");
        return `<div class="col" style="${sz};gap:8px"><svg class="chart" viewBox="0 0 40 40" style="height:${Math.max(H, 96)}px">${arcs}</svg>${legend}</div>`;
      }
      return `<div class="col" style="${sz};gap:8px"><div class="zone">${series.map((v, i) => `<i data-sp="fg:${Math.max(0, v)}" style="--c:${seriesColor(i)}"></i>`).join("")}</div>${legend}</div>`;
    }
    case "segbar": {
      const total = a.total != null ? num(a.total) : series.reduce((p, v) => p + v, 0) || 1;
      return `<div class="prog" style="${sz}"><div class="zone" style="flex:1">${series.map((v, i) => `<i data-sp="fg:${Math.max(0, v)}" style="--c:${seriesColor(i)}"></i>`).join("")}<i data-sp="fg:${Math.max(0, total - series.reduce((p, v) => p + v, 0))}" style="--c:transparent"></i></div><span>${esc(a.suffix || "")}</span></div>`;
    }
    case "gauge": {
      const v = a.bind != null || a.value != null ? s.value : series[series.length - 1] ?? 0;
      const f = clamp01((v - num(a.min, 0)) / ((a.max != null ? num(a.max) : 100) - num(a.min, 0) || 1));
      const label = a.label || "", text = a.text != null ? a.text : `${Math.round(f * 100)}%`;
      return `<div class="gauge" style="${sz}"><span class="lab">${esc(label)}</span><div class="trk"><i data-sp="w:${(f * 100).toFixed(2)}" style="--c:${a.color ? tone(a.color) : level(f, a.low === "bad" || a.low === "1")}"></i></div><span class="val"${a.text != null ? "" : ` data-sp="cnt:${Math.round(f * 100)}" data-suf="%"`}>${esc(text)}</span></div>`;
    }
    case "ringbar": return `<div class="ring" style="${sz}"><span style="--d:${Math.max(4, (a.text || "").length / Math.max(1, num(a.speed, 6)) * 1.2).toFixed(1)}s;color:${a.color ? tone(a.color) : "inherit"}">${esc(a.text || "")}</span></div>`;
    case "spinner": return `<div class="spin" style="${sz}"><i></i><span>${esc(a.text || "")}</span></div>`;
    case "clock": return `<div class="clock ${esc(a.align || "")}" style="${sz}" data-clock="${esc(a.format || "hms")}" data-bound="${a.bind != null ? "1" : ""}">${esc(s.text || "")}</div>`;
    case "timer":
      return `<div class="timer${foc}${s.done ? " done" : ""}" style="${sz}"><button class="btn pri" data-id="${id}" data-act="toggle" title="start / stop">${s.running ? icon("pause", "fill") : icon("play", "fill")}</button><span class="t">${esc(s.text)}</span><button class="btn" data-id="${id}" data-act="reset" title="reset">${icon("reset")}</button><span class="l">${esc(s.label || a.label || "")}</span></div>`;
    case "dropdown": {
      const opts = (s.items || []).map((t, i) => `<option value="${i}"${i === s.sel ? " selected" : ""}>${esc(t)}</option>`).join("");
      return `<div class="sel-wrap${foc}" style="${sz}">${a.label ? `<label>${esc(a.label)}</label>` : ""}<span class="sw"><select data-id="${id}" data-act="select" data-change="1">${opts}</select>${icon("updown")}</span></div>`;
    }
    case "table": {
      const num_ = s.numeric || [], sort = s.sort;
      const head = (s.cols || []).map((c, i) => `<th class="${num_[i] ? "n" : ""}" data-id="${id}" data-act="sort" data-arg="${i}">${esc(c)}${sort && sort[0] === i ? icon(sort[1] ? "down" : "up") : ""}</th>`).join("");
      const body = (s.rows || []).map((r, p) => `<tr class="${p === s.cursor ? "sel" : ""}" data-id="${id}" data-act="select" data-dbl="activate" data-arg="${p}" data-pos="${p}">${r.map((c, i) => `<td class="${num_[i] ? "n" : ""}">${esc(c)}</td>`).join("")}</tr>`).join("");
      return `<div class="tbl${foc}" style="${sz}" data-id="${id}" data-cursor="${s.cursor}"><table><thead><tr>${head}</tr></thead><tbody>${body}</tbody></table></div>`;
    }
    case "tree": {
      const rows = (s.nodes || []).map((t, p) => {
        const sel = p === s.cursor;
        const chev = t.folder ? icon(t.open ? "chevD" : "chevR") : "";
        return `<div class="tn${t.folder ? " folder" : ""}${sel ? " sel" : ""}" style="padding-left:${8 + t.depth * 18}px" data-id="${id}" data-act="${t.folder ? "toggle" : "select"}" data-dbl="activate" data-arg="${esc(t.path)}" data-pos="${p}"><span class="tw">${chev}</span>${icon(t.folder ? "folder" : "file")}<span>${esc(t.name)}</span></div>`;
      }).join("");
      return `<div class="tree${foc}" style="${sz}" data-id="${id}" data-cursor="${s.cursor}" data-hl><u class="hl"></u>${rows}</div>`;
    }
    case "sysbar": {
      const pills = (s.items || []).map((it) => {
        const f = it.frac, c = f == null ? "var(--accent)" : level(f, false);
        const bar = f == null ? "" : `<em><i data-sp="w:${(f * 100).toFixed(1)}" style="--c:${c}"></i></em>`;
        return `<span class="pill" title="${esc(it.name)}"><span class="pl">${esc(it.label)}</span><b style="color:${f == null && it.text.includes("--") ? "var(--dim)" : f == null ? "var(--fg)" : c}">${esc(it.text)}</b>${bar}</span>`;
      }).join("");
      return `<div class="sysbar ${esc(a.align || "")}" style="${sz}">${pills}</div>`;
    }
    case "tagcloud": {
      const ws = csv(a.words), wt = csv(a.weights).map(Number), fin = wt.filter(Number.isFinite), mn = Math.min(...fin), mx = Math.max(...fin);
      const f = (i) => (Number.isFinite(wt[i]) && mx > mn ? (wt[i] - mn) / (mx - mn) : 0.5);
      return `<div class="tags" style="${sz}">${ws.map((w, i) => `<span style="font-size:${(12 + f(i) * 20).toFixed(1)}px;opacity:${(0.45 + f(i) * 0.55).toFixed(2)};color:${f(i) > 0.66 ? "var(--accent)" : f(i) > 0.33 ? "var(--fg)" : "var(--dim)"};font-weight:${f(i) > 0.66 ? 700 : 500}">${esc(w)}</span>`).join(" ")}</div>`;
    }
    case "webchart": {
      const nn = series.length, max = a.max != null ? num(a.max) : Math.max(...series, 0), rings = Math.max(1, Math.min(12, num(a.rings, 4)));
      if (nn < 3 || !(max > 0)) return `<div class="chart" style="${sz}"></div>`;
      const ang = (i) => -Math.PI / 2 + 2 * Math.PI * i / nn, pt = (i, f) => [50 + Math.cos(ang(i)) * 34 * f, 50 + Math.sin(ang(i)) * 34 * f];
      const poly = (f) => series.map((_, i) => pt(i, f).map((v) => v.toFixed(2)).join(",")).join(" ");
      const data = series.map((v, i) => pt(i, clamp01(v / max)).map((x) => x.toFixed(2)).join(",")).join(" ");
      const lab = series.map((_, i) => { const [x, y] = pt(i, 1.2), c = Math.cos(ang(i)); return `<text x="${x.toFixed(2)}" y="${(y + 1.5).toFixed(2)}" text-anchor="${c > 0.3 ? "start" : c < -0.3 ? "end" : "middle"}">${esc(labels[i] ?? "")}</text>`; }).join("");
      const grid = Array.from({ length: rings }, (_, k) => `<polygon points="${poly((k + 1) / rings)}" class="gr"/>`).join("") + series.map((_, i) => { const [x, y] = pt(i, 1); return `<line x1="50" y1="50" x2="${x.toFixed(2)}" y2="${y.toFixed(2)}" class="gr"/>`; }).join("");
      return `<div class="chart web" style="${sz};height:${Math.max(H, 180)}px"><svg viewBox="0 0 100 100" style="height:100%">${grid}<polygon points="${data}" fill="${col}" fill-opacity=".28" stroke="${col}" stroke-width="1.4" stroke-linejoin="round"/>${series.map((v, i) => `<circle cx="${pt(i, clamp01(v / max))[0].toFixed(2)}" cy="${pt(i, clamp01(v / max))[1].toFixed(2)}" r="1.5" fill="${col}"/>`).join("")}${lab}</svg></div>`;
    }
    case "piechart": {
      const tot = series.reduce((p, v) => p + Math.max(0, v), 0) || 1, donut = a.donut && a.donut !== "0", r = donut ? 15.9 : 8, sw = donut ? 6 : 16;
      let acc = 0;
      const arcs = series.map((v, i) => { const f = Math.max(0, v) / tot; const d = `<circle cx="20" cy="20" r="${r}" fill="none" stroke="${seriesColor(i)}" stroke-width="${sw}" stroke-dasharray="${(f * 100).toFixed(2)} ${(100 - f * 100).toFixed(2)}" stroke-dashoffset="${(-acc * 100).toFixed(2)}" transform="rotate(-90 20 20)" pathLength="100"/>`; acc += f; return d; }).join("");
      const legend = a.legend !== "0" ? `<div class="legend col">${series.map((v, i) => `<span><i style="background:${seriesColor(i)}"></i>${esc(labels[i] ?? i + 1)} ${(Math.max(0, v) / tot * 100).toFixed(0)}%</span>`).join("")}</div>` : "";
      return `<div class="row pie" style="${sz};gap:16px;align-items:center"><svg class="chart" viewBox="0 0 40 40" style="height:${Math.max(H, 120)}px;flex:none">${arcs}</svg>${legend}</div>`;
    }
    case "radar": {
      const speed = Math.max(1, num(a.speed, 90)), T = 360 / speed;
      const blips = (a.blips || "").split(";").map((b) => b.split(":")).filter((b) => b.length >= 2 && Number.isFinite(+b[0]) && Number.isFinite(+b[1])).map(([b, r, ...l]) => ({ b: ((+b % 360) + 360) % 360, r: clamp01(+r) * 0.97, l: l.join(":") }));
      const dots = blips.map((p) => { const ang = p.b * Math.PI / 180, x = 50 + Math.sin(ang) * 46 * p.r, y = 50 - Math.cos(ang) * 46 * p.r; return `<span class="blip" style="left:${x.toFixed(2)}%;top:${y.toFixed(2)}%;animation-duration:${T.toFixed(2)}s;animation-delay:${(p.b / speed - T).toFixed(2)}s"><i></i>${p.l ? `<em>${esc(p.l)}</em>` : ""}</span>`; }).join("");
      const rings = [1, 2].map((k) => `<circle cx="50" cy="50" r="${(46 * k / 3).toFixed(2)}" class="gr"/>`).join("") + Array.from({ length: 12 }, (_, i) => { const t = i * 30 * Math.PI / 180; return `<line x1="50" y1="50" x2="${(50 + Math.sin(t) * 46).toFixed(2)}" y2="${(50 - Math.cos(t) * 46).toFixed(2)}" class="gr"/>`; }).join("");
      return `<div class="radar" style="${sz};height:${Math.max(H, 220)}px;--col:${a.color ? tone(a.color) : "var(--ok)"}"><div class="scope"><svg viewBox="0 0 100 100"><circle cx="50" cy="50" r="46" class="rim"/>${rings}</svg><div class="sweep" style="animation-duration:${T.toFixed(2)}s"></div>${dots}<b class="n">N</b><b class="e">E</b><b class="s">S</b><b class="w">W</b></div></div>`;
    }
    case "rain": return `<div class="rain" style="${sz};height:${Math.max(H, 120)}px" data-keep="1"><canvas data-seed="${esc(a.seed || 1)}" data-speed="${esc(a.speed || 1)}"></canvas></div>`;
    case "scramble": return `<div class="scr" style="${sz}" data-text="${esc(a.text || (a.bind != null ? s.text : "") || "")}" data-progress="${esc(a.progress ?? "1")}" data-keep="1"></div>`;
    case "worldmap":
    case "globe":
      // the picture is a canvas the skin fills from the engine (maps()); the engine's view state rides along as data-st
      return `<div class="map${foc}" style="${sz}" data-id="${id}" data-map="${n.kind}" data-keep="1" data-labels="${a.labels ? 1 : ""}" data-lang="${esc(a.lang || "")}" data-st="${esc(JSON.stringify(s))}"><canvas></canvas><div class="lbls"></div><div class="mapcap"></div></div>`;
    case "minimap": {
      const d = s.series || [], rows = Math.min(d.length, 96), per = d.length / (rows || 1);
      const bars = Array.from({ length: rows }, (_, i) => {
        const a0 = Math.floor(i * per), a1 = Math.max(a0 + 1, Math.floor((i + 1) * per));
        let m = 0; for (let k = a0; k < a1 && k < d.length; k++) m += d[k]; m /= a1 - a0;
        const hit = (s.marks || []).some((q) => q >= a0 && q < a1);
        return `<i style="width:${(m * 100).toFixed(1)}%"${hit ? ' class="mk"' : ""}></i>`;
      }).join("");
      const n = Math.max(1, d.length);
      return `<div class="mm${foc}" style="${sz}" data-id="${id}" data-mm="${d.length}"><div class="mmr">${bars}</div><u class="mmv" data-sp="tp:${(s.offset / n * 100).toFixed(2)};h:${Math.min(100, s.page / n * 100).toFixed(2)}"></u></div>`;
    }
    default: return `<div class="unk" style="${sz}">${esc(n.kind)}</div>`;
  }
}

// ---- tiny DOM morph: keep focus, caret and scroll while the tree changes ------------------------------------------

let MOTION = null; // set by mountSkin: morph re-applies live spring values after it syncs attributes

function morph(from, to) {
  if (from.nodeType !== to.nodeType || from.nodeName !== to.nodeName) { from.replaceWith(to.cloneNode(true)); return; }
  // a different page / block (data-blk) is a different thing: replace it, so its parts enter fresh
  if (from.nodeType === 1 && from.getAttribute("data-blk") !== to.getAttribute("data-blk")) { from.replaceWith(to.cloneNode(true)); return; }
  if (from.nodeType === 3) { if (from.nodeValue !== to.nodeValue) from.nodeValue = to.nodeValue; return; }
  if (from.nodeType === 1 && from.hasAttribute("data-keep")) { // the map's insides belong to maps(); only its attributes follow
    for (const { name, value } of [...to.attributes]) if (from.getAttribute(name) !== value) from.setAttribute(name, value);
    return;
  }
  const mx = from.style && from.style.getPropertyValue("--mx"), my = from.style && from.style.getPropertyValue("--my");
  for (const { name } of [...from.attributes]) if (!to.hasAttribute(name)) from.removeAttribute(name);
  for (const { name, value } of [...to.attributes]) if (from.getAttribute(name) !== value) from.setAttribute(name, value);
  if (from.nodeName === "INPUT" && from.value !== to.getAttribute("value")) from.value = to.getAttribute("value") || "";
  if (from.nodeName === "SELECT") { for (const o of [...from.children]) if (!to.querySelector(`option[value="${o.value}"]`)) o.remove(); }
  const a = [...from.childNodes], b = [...to.childNodes];
  for (let i = 0; i < b.length; i++) {
    if (i < a.length) morph(a[i], b[i]); else from.appendChild(b[i].cloneNode(true));
  }
  for (let i = a.length - 1; i >= b.length; i--) a[i].remove();
  if (from.nodeName === "SELECT") from.value = [...to.children].find((o) => o.hasAttribute("selected"))?.value ?? from.value;
  if (mx) { from.style.setProperty("--mx", mx); from.style.setProperty("--my", my); }
  if (MOTION && (from._m || from._pts)) { MOTION.apply(from); if (from._pts && from._draw) from._draw(from._pts.map((q) => q.x)); }
}

// ---- the mount ------------------------------------------------------------------------------------------------------

const KEYS = { Enter: "enter", Tab: "tab", Escape: "esc", Backspace: "backspace", Delete: "delete", Insert: "insert", ArrowUp: "up", ArrowDown: "down", ArrowLeft: "left", ArrowRight: "right", Home: "home", End: "end", PageUp: "pgup", PageDown: "pgdn" };

export async function mountSkin(root, o = {}) {
  styleOnce();
  const eng = await engine(o);
  const { x, call, put, read, dec } = eng;
  x.lz_resize(120, 40);
  root.classList.add("lz");
  root.tabIndex = 0;
  const motion = new Motion();
  MOTION = motion;
  let look = "modern", fxList = [];
  function setLook(name, fx) {
    look = LOOKS[name] ? name : "modern";
    const L = LOOKS[look];
    root.dataset.look = look;
    fxList = fx || L.fx;
    root.dataset.fx = fxList.join(" ");
    motion.preset = o.motion || L.motion;
    applyScheme();
    prevJson = ""; // thumbs and highlights depend on the look's metrics
    if (typeof pump === "function" && !dead) requestAnimationFrame(() => { track(); });
  }
  const mq = window.matchMedia ? window.matchMedia("(prefers-color-scheme: dark)") : null;
  let mode = o.scheme || "auto";
  const applyScheme = () => { const dark = LOOKS[look].scheme && (mode === "dark" || (mode === "auto" && mq && mq.matches)); if (dark) root.setAttribute("data-dark", ""); else root.removeAttribute("data-dark"); };
  if (mq && mq.addEventListener) mq.addEventListener("change", applyScheme);
  if (o.accent) root.style.setProperty("--accent", o.accent);

  const handlers = [];
  let last = performance.now(), timer = 0, raf = 0, dead = false, prevJson = "", model = {};
  setLook(o.look || (o.flavor === "win" ? "flat" : "modern"), o.fx);
  const cursors = new Map();

  function act(id, action, arg = "") {
    const [a, an] = put(id), [b, bn] = put(action), [c, cn] = put(arg);
    try { return x.lz_act(a, an, b, bn, c, cn) === 1; } finally { x.lz_free(a, an); x.lz_free(b, bn); x.lz_free(c, cn); }
  }

  function clocks() {
    for (const el of root.querySelectorAll("[data-clock]")) {
      if (el.dataset.bound) continue;
      const d = new Date(), p = (n) => String(n).padStart(2, "0");
      const f = el.dataset.clock;
      el.textContent = f === "hm" ? `${p(d.getHours())}:${p(d.getMinutes())}` : f === "date" ? `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}` : f === "full" ? `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}` : `${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
    }
  }

  const kv = (t) => Object.fromEntries((t || "").split(";").filter(Boolean).map((p) => { const [k, v] = p.split(":"); return [k, parseFloat(v)]; }));

  // every element that declares data-sp="prop:target;..." chases its target; a brand-new one starts from data-from
  // (bars and counters start empty) and enters after data-delay ms, so a page builds itself in a wave.
  function springs() {
    for (const el of root.querySelectorAll("[data-sp]")) {
      const fresh = !el._m, to = kv(el.dataset.sp), from = kv(el.dataset.from), delay = fresh ? +el.dataset.delay || 0 : 0;
      for (const [k, v] of Object.entries(to)) {
        const f0 = k in from ? from[k] : fresh && ["w", "h", "cnt", "fg"].includes(k) ? 0 : undefined;
        if (fresh && f0 !== undefined) {
          motion.snap(el, k, f0);
          if (delay) setTimeout(() => { if (el.isConnected) motion.to(el, k, kv(el.dataset.sp)[k]); }, delay); else motion.to(el, k, v);
        } else motion.to(el, k, v);
      }
    }
    for (const el of root.querySelectorAll("[data-pts]")) {
      const t = el.dataset.pts.split(",").map(parseFloat).filter(Number.isFinite);
      if (t.length) motion.pts(el, t, (vals) => {
        const w = Math.max(vals.length - 1, 1), p = vals.map((v, i) => `${(i / w * 100).toFixed(2)},${(96 - v * 92).toFixed(2)}`).join(" ");
        el.querySelector(".ln")?.setAttribute("points", p);
        el.querySelector(".ar")?.setAttribute("points", `0,100 ${p} 100,100`);
      }, { flat: true });
    }
  }

  // the sliding segment pill and the sliding row highlight chase the active element's box
  function track() {
    for (const seg of root.querySelectorAll(".seg")) {
      const t = seg.querySelector(":scope>.thumb"), b = seg.querySelector("button.on");
      if (t && b) { motion.to(t, "l", b.offsetLeft); motion.to(t, "pw", b.offsetWidth); }
    }
    for (const box of root.querySelectorAll("[data-hl]")) {
      const h = box.querySelector(":scope>.hl"), r = box.querySelector(".sel");
      if (!h) continue;
      if (r) { motion.to(h, "o", 1); motion.to(h, "t", r.offsetTop); motion.to(h, "ph", r.offsetHeight); } else motion.to(h, "o", 0);
    }
  }

  // ---- maps: the engine shades the planet, the skin puts the pixels on a canvas ------------------------------------
  const MAXPX = 340000; // pixels per picture: the shader is per pixel, the canvas is scaled up by CSS
  function maps() {
    for (const m of root.querySelectorAll(".map")) {
      const cv = m.querySelector("canvas"), cw = m.clientWidth, ch = m.clientHeight;
      if (!cv || cw < 8 || ch < 8) continue;
      const k = Math.min(1, Math.sqrt(MAXPX / (cw * ch))) * Math.min(2, window.devicePixelRatio || 1), w = Math.max(8, Math.round(cw * k)), h = Math.max(8, Math.round(ch * k));
      const key = m.dataset.st + "|" + w + "x" + h + "|" + m.dataset.labels;
      let st = {}; try { st = JSON.parse(m.dataset.st); } catch {}
      if (m._key !== key) {
        m._key = key;
        const [p, pn] = put(m.dataset.id);
        const n = x.lz_pixels(p, pn, w, h); x.lz_free(p, pn);
        if (n === w * h * 4) {
          if (cv.width !== w || cv.height !== h) { cv.width = w; cv.height = h; }
          const ctx = cv.getContext("2d");
          ctx.putImageData(new ImageData(new Uint8ClampedArray(x.memory.buffer.slice(x.lz_pix_ptr(), x.lz_pix_ptr() + n)), w, h), 0, 0);
          let names = [];
          try { names = JSON.parse(dec.decode(new Uint8Array(x.memory.buffer, x.lz_aux_ptr(), x.lz_aux_len()))); } catch {}
          m.querySelector(".lbls").innerHTML = names.map((l) => `<span style="left:${(l.x / w * 100).toFixed(2)}%;top:${(l.y / h * 100).toFixed(2)}%">${esc(l.t)}</span>`).join("");
        }
      }
      const cap = m.querySelector(".mapcap"), sel = st.sel;
      if (sel) {
        const pl = m.dataset.lang === "pl";
        cap.innerHTML = `<b>${esc(pl ? sel.pl : sel.en)}</b><span>${esc(sel.continent)}</span><span>${sel.pop_m} M · $${sel.gdp_bn} B</span>`;
        cap.classList.add("on");
      } else cap.classList.remove("on");
    }
  }

  // drag pans the map or turns the globe, the wheel zooms, a click that did not move picks a country: all in canvas pixels
  const amb = ambient(root);
  let mapDrag = null, utcT = 0;
  const mapPt = (m, e) => { const cv = m.querySelector("canvas"), r = cv.getBoundingClientRect(); return [(e.clientX - r.left) / r.width * cv.width, (e.clientY - r.top) / r.height * cv.height]; };
  root.addEventListener("pointerdown", (e) => {
    const m = e.target.closest ? e.target.closest(".map") : null;
    if (!m || e.button !== 0) return;
    mapDrag = { m, p: mapPt(m, e), x: e.clientX, y: e.clientY, moved: false };
    try { m.setPointerCapture(e.pointerId); } catch {}
    act(m.dataset.id, "focus"); m.classList.add("grab");
  });
  root.addEventListener("pointermove", (e) => {
    if (!mapDrag) return;
    const { m } = mapDrag, to = mapPt(m, e);
    if (!mapDrag.moved && Math.hypot(e.clientX - mapDrag.x, e.clientY - mapDrag.y) < 4) return;
    mapDrag.moved = true;
    act(m.dataset.id, "pan", `${mapDrag.p[0]},${mapDrag.p[1]},${to[0]},${to[1]}`);
    mapDrag.p = to;
    pump();
  });
  addEventListener("pointerup", (e) => {
    if (!mapDrag) return;
    const { m, moved } = mapDrag; mapDrag = null; m.classList.remove("grab");
    if (!moved) { const p = mapPt(m, e); act(m.dataset.id, "pick", `${p[0]},${p[1]}`); pump(); }
  });
  root.addEventListener("wheel", (e) => {
    const m = e.target.closest ? e.target.closest(".map") : null;
    if (!m) return;
    e.preventDefault();
    const p = mapPt(m, e);
    act(m.dataset.id, "zoom", `${e.deltaY < 0 ? 1.25 : 0.8},${p[0]},${p[1]}`);
    pump();
  }, { passive: false });
  root.addEventListener("dblclick", (e) => {
    const m = e.target.closest ? e.target.closest(".map") : null;
    if (!m) return;
    const p = mapPt(m, e);
    act(m.dataset.id, "zoom", `2,${p[0]},${p[1]}`); pump();
  });
  // a minimap: press or drag anywhere on it seeks to that line
  let mmDrag = null;
  const mmSeek = (m, e) => { const r = m.getBoundingClientRect(), n = +m.dataset.mm || 1; act(m.dataset.id, "seek", String(Math.floor(Math.max(0, Math.min(0.9999, (e.clientY - r.top) / r.height)) * n))); pump(); };
  root.addEventListener("pointerdown", (e) => { const m = e.target.closest ? e.target.closest(".mm") : null; if (m && e.button === 0) { mmDrag = m; try { m.setPointerCapture(e.pointerId); } catch {} mmSeek(m, e); } });
  root.addEventListener("pointermove", (e) => { if (mmDrag) mmSeek(mmDrag, e); });
  addEventListener("pointerup", () => { mmDrag = null; });

  function draw() {
    const tree = JSON.parse(read(x.lz_describe()));
    const json = JSON.stringify(tree);
    if (json === prevJson) return;
    prevJson = json;
    const holder = document.createElement("div");
    holder.innerHTML = tree ? renderNode(tree, "col", { model, n: 0 }) : "";
    const target = tree ? holder.firstElementChild : null;
    const want = target ? [target] : [];
    const have = [...root.children];
    want.forEach((w, i) => { if (have[i]) morph(have[i], w); else root.appendChild(w.cloneNode(true)); });
    for (let i = have.length - 1; i >= want.length; i--) have[i].remove();
    // keep the selected row in view when the cursor moved
    for (const el of root.querySelectorAll("[data-cursor]")) {
      const key = el.dataset.id, cur = el.dataset.cursor;
      if (cursors.get(key) !== cur) { cursors.set(key, cur); el.querySelector(".sel")?.scrollIntoView({ block: "nearest" }); }
    }
    // the engine's focus decides which input has the caret
    const f = root.querySelector(".search.focus input");
    if (f && f !== document.activeElement && (root.contains(document.activeElement) || document.activeElement === document.body)) f.focus({ preventScroll: true });
    clocks();
    springs();
    track();
    maps();
    amb.wake();
    if (root.querySelector(".map") && !utcT) { utcT = setInterval(() => feed({ utc: Date.now() / 1000 }), 30000); feed({ utc: Date.now() / 1000 }); }
    if (root.querySelector(".sysbar")) startFeed();
  }

  function pump() {
    if (dead) return;
    clearTimeout(timer); cancelAnimationFrame(raf); timer = raf = 0;
    const now = performance.now();
    x.lz_tick(now - last); last = now;
    x.lz_render();
    draw();
    const n = x.lz_events();
    if (n) {
      for (const line of read(n).split("\n")) {
        if (!line) continue;
        const e = JSON.parse(line);
        for (const h of handlers) if (!h.kind || h.kind === e.kind) h.fn(e);
        root.dispatchEvent(new CustomEvent("lazyui", { detail: e }));
      }
    }
    const w = x.lz_wake_ms();
    if (w < 0) return;
    if (w <= 17) raf = requestAnimationFrame(pump); else timer = setTimeout(pump, Math.min(w, 1000));
  }

  const clockTimer = setInterval(clocks, 1000);

  // what the browser can measure for a sysbar: real animation frames per second and the round trip to this origin.
  // Everything else (cpu, mem, swap, disk, net) stays "--" until the page calls ui.feed({cpu: 12, ...}).
  let feeding = false, feedStop = () => {};
  function feed(vals) { for (const [k, v] of Object.entries(vals)) { const key = k.startsWith("sys.") ? k : "sys." + k; const [a, an] = put(key), [b, bn] = put(String(v)); x.lz_set(a, an, b, bn); x.lz_free(a, an); x.lz_free(b, bn); } pump(); }
  function startFeed() {
    if (feeding || o.sys === false) return;
    feeding = true;
    let frames = 0, t0 = performance.now(), r = 0, stop = false;
    const loop = () => { frames++; if (!stop) r = requestAnimationFrame(loop); };
    r = requestAnimationFrame(loop);
    const fpsT = setInterval(() => { const t = performance.now(); feed({ fps: frames / ((t - t0) / 1000) }); frames = 0; t0 = t; }, 1000);
    const ping = async () => {
      const t = performance.now();
      try { await fetch(o.ping || location.pathname, { method: "HEAD", cache: "no-store" }); feed({ ping: performance.now() - t }); } catch { feed({ ping: -1 }); }
    };
    ping(); const pingT = setInterval(ping, 2000);
    feedStop = () => { stop = true; cancelAnimationFrame(r); clearInterval(fpsT); clearInterval(pingT); };
  }

  // cursor-driven effects: a light that follows the pointer and a 3D lean, both only where the look asks for them
  const fx = (k) => fxList.includes(k);
  const settle = () => (motion.preset === "stiff" ? "stiff" : "bouncy");
  let hot = null, pressed = null;
  const unlean = () => { if (hot) { motion.to(hot, "rx", 0, { preset: "gentle" }); motion.to(hot, "ry", 0, { preset: "gentle" }); motion.to(hot, "s", 1, { preset: "gentle" }); hot = null; } };
  root.addEventListener("pointermove", (e) => {
    const p = e.target.closest ? e.target.closest(".panel") : null;
    if (hot && hot !== p) unlean();
    if (!p || e.target.closest(".map,.mm")) { if (hot) unlean(); return; } // a map under the pointer holds still
    const r = p.getBoundingClientRect(), mx = e.clientX - r.left, my = e.clientY - r.top;
    if (fx("spot")) { p.style.setProperty("--mx", mx + "px"); p.style.setProperty("--my", my + "px"); }
    if (fx("tilt") && r.width > 40 && r.height > 40) {
      const k = Math.min(1, 560 / r.width) * 4.5;
      motion.to(p, "ry", (mx / r.width - 0.5) * 2 * k, { preset: "gentle" });
      motion.to(p, "rx", -(my / r.height - 0.5) * 2 * k, { preset: "gentle" });
      motion.to(p, "s", 1.01, { preset: "gentle" });
      hot = p;
    }
  });
  root.addEventListener("pointerleave", unlean);
  root.addEventListener("pointerdown", (e) => {
    const b = e.target.closest ? e.target.closest(".btn,.seg button,.keybar b") : null;
    if (b) { pressed = b; motion.to(b, "s", 0.92, { preset: "snappy" }); }
  });
  addEventListener("pointerup", () => { if (pressed) { motion.to(pressed, "s", 1, { preset: settle() }); pressed = null; } });
  if (typeof ResizeObserver === "function") new ResizeObserver(() => track()).observe(root);
  if (document.fonts && document.fonts.ready) document.fonts.ready.then(track);

  const sendKey = (name) => { const ok = call(x.lz_key, name) === 1; pump(); return ok; };
  root.addEventListener("click", (e) => {
    const k = e.target.closest("[data-key]");
    if (k) { sendKey(k.dataset.key); return; }
    const t = e.target.closest("[data-act]");
    if (!t || t.tagName === "INPUT" || t.dataset.change) return;
    act(t.dataset.id, t.dataset.act, t.dataset.arg || "");
    pump();
  });
  root.addEventListener("dblclick", (e) => {
    const t = e.target.closest("[data-dbl]");
    if (!t) return;
    act(t.dataset.id, t.dataset.dbl, t.dataset.arg || "");
    pump();
  });
  root.addEventListener("input", (e) => {
    const t = e.target;
    if (t.tagName === "INPUT" && t.dataset.act === "text") { act(t.dataset.id, "text", t.value); pump(); }
  });
  root.addEventListener("change", (e) => {
    const t = e.target;
    if (t.tagName === "SELECT" && t.dataset.change) { act(t.dataset.id, t.dataset.act, t.value); pump(); }
  });
  root.addEventListener("focusin", (e) => {
    const t = e.target.closest("[data-id]");
    if (t && e.target.tagName === "INPUT") { act(t.dataset.id, "focus"); pump(); }
  });
  root.addEventListener("keydown", (e) => {
    if (e.isComposing || e.metaKey) return;
    const tag = e.target.tagName;
    const named = KEYS[e.key] || (/^F\d+$/.test(e.key) ? e.key.toLowerCase() : null);
    const typing = tag === "INPUT" || tag === "SELECT";
    if (typing) {
      // text keys belong to the field; navigation goes to the engine
      if (!named || ["backspace", "delete", "left", "right", "home", "end"].includes(named)) return;
    }
    let handled = 0;
    if (named) handled = call(x.lz_key, (e.ctrlKey ? "ctrl-" : "") + (e.altKey ? "alt-" : "") + (e.shiftKey ? "shift-" : "") + named);
    else if (e.key.length === 1 && (e.ctrlKey || e.altKey)) {
      if (e.ctrlKey && (e.key === "v" || e.key === "c" || e.key === "x")) return;
      handled = call(x.lz_key, (e.ctrlKey ? "ctrl-" : "") + (e.altKey ? "alt-" : "") + (e.key === " " ? "space" : e.key.toLowerCase()));
    } else if (e.key.length === 1) {
      handled = call(x.lz_key, e.key === " " ? "space" : e.key);
    }
    if (handled) { e.preventDefault(); pump(); }
  });
  document.addEventListener("visibilitychange", () => { if (!document.hidden) pump(); });

  pump();

  return {
    set(key, value) { const [a, an] = put(key), [b, bn] = put(String(value)); x.lz_set(a, an, b, bn); x.lz_free(a, an); x.lz_free(b, bn); model[key] = value; pump(); },
    key: sendKey,
    act(id, action, arg) { const ok = act(id, action, arg); pump(); return ok; },
    on(kind, fn) { handlers.push(typeof kind === "function" ? { fn: kind } : { kind, fn }); },
    reload(spec) { const ok = call(x.lz_reload, spec) === 1; prevJson = ""; pump(); if (!ok) throw new Error("lazyui spec: " + eng.err()); },
    scheme(m) { mode = m; applyScheme(); },
    look(name, fx) { setLook(name, fx); pump(); return look; },
    looks: () => Object.entries(LOOKS).map(([name, l]) => ({ name, ...l })),
    motion(name) { motion.preset = name; },
    flavor(f) { setLook(f === "win" ? "flat" : "modern"); pump(); },
    accent(c) { root.style.setProperty("--accent", c); },
    feed,
    describe() { return JSON.parse(read(x.lz_describe())); },
    destroy() { dead = true; clearTimeout(timer); cancelAnimationFrame(raf); clearInterval(clockTimer); clearInterval(utcT); amb.stop(); feedStop(); if (MOTION === motion) MOTION = null; },
  };
}
