// Looks for the DOM skin. A look is a bundle of design tokens (CSS variables: radius, font, palette, shadow, blur...), a
// spring personality and a list of effects. The skin's structure CSS only ever reads tokens, so a new look is one block
// of variables plus, when it wants something exotic (bevels, scanlines, blobs), a few extra rules. Nothing else changes.
//
//   terminal  green phosphor CRT: monospace, square, scanlines, glow, blocky bars, stiff springs
//   classic   1995: grey bevels, navy title bars on a teal desktop
//   flat      squarer, hairlines, Fluent-like, no extras
//   modern    rounded, soft shadows, sliding thumb and highlight, cursor spotlight
//   glass     frosted glass over drifting colour, big radius, 3D tilt, bouncy springs
//   neon      dark with glowing edges and bars
//   aurora    animated gradient sky, glass cards, film grain, slow gentle springs
//
// fx flags (data-fx on the root): spot (cursor light on panels) tilt (3D lean toward the cursor) thumb (the active segment
// is one sliding pill) hl (the selected row is one sliding bar) blobs (drifting colour behind) grain scan glow.

export const LOOKS = {
  terminal: { motion: "stiff", fx: ["scan", "glow"], scheme: false, desc: "green phosphor CRT" },
  classic: { motion: "stiff", fx: [], scheme: false, desc: "1995 bevels and title bars" },
  flat: { motion: "snappy", fx: [], scheme: true, desc: "squarer, hairlines" },
  modern: { motion: "snappy", fx: ["spot", "thumb", "hl"], scheme: true, desc: "rounded, soft, sliding highlights" },
  glass: { motion: "bouncy", fx: ["spot", "tilt", "thumb", "hl", "blobs"], scheme: true, desc: "frosted glass, 3D tilt" },
  neon: { motion: "bouncy", fx: ["spot", "tilt", "thumb", "hl", "glow"], scheme: false, desc: "glowing edges" },
  aurora: { motion: "gentle", fx: ["spot", "tilt", "thumb", "hl", "blobs", "grain"], scheme: false, desc: "animated sky, film grain" },
};

const GRAIN = "url(\"data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' width='160' height='160'><filter id='n'><feTurbulence type='fractalNoise' baseFrequency='.9' numOctaves='2' stitchTiles='stitch'/></filter><rect width='160' height='160' filter='url(%23n)' opacity='.55'/></svg>\")";

export const CSS = `
/* ---- tokens: the modern look is the default, every other look overrides some of these ---- */
.lz{--r:14px;--r2:calc(var(--r)*.62);--r3:calc(var(--r)*.4);--rp:99px;--gap:12px;--pad:14px;--bw:1px;
  --font:-apple-system,BlinkMacSystemFont,"SF Pro Text","Segoe UI Variable","Segoe UI",system-ui,Roboto,sans-serif;--fs:14px;
  --bg:#f2f2f7;--card:#fff;--card2:#fff;--fg:#1d1d1f;--dim:#6e6e73;--line:rgba(0,0,0,.09);--hover:rgba(0,0,0,.05);
  --accent:#0a84ff;--on:#fff;--ok:#2ea44f;--warn:#d99a00;--err:#e5484d;--c5:#8e5bd6;--c6:#10a3a3;--sel:rgba(10,132,255,.14);
  --shadow:0 1px 2px rgba(0,0,0,.05),0 8px 24px rgba(0,0,0,.06);--blur:none;--glow:rgba(10,132,255,.10);--ts:none;
  font:var(--fs)/1.4 var(--font);color:var(--fg);background:var(--bg);text-shadow:var(--ts);
  box-sizing:border-box;height:100%;display:flex;flex-direction:column;padding:var(--pad);gap:var(--gap);outline:none;
  -webkit-font-smoothing:antialiased;position:relative;isolation:isolate}
.lz[data-dark]{--bg:#161618;--card:#242427;--card2:#38383c;--fg:#f5f5f7;--dim:#98989d;--line:rgba(255,255,255,.1);--hover:rgba(255,255,255,.07);
  --ok:#32d74b;--warn:#ffd60a;--err:#ff453a;--c5:#bf5af2;--c6:#64d2ff;--sel:rgba(10,132,255,.32);--shadow:0 1px 2px rgba(0,0,0,.4),0 8px 24px rgba(0,0,0,.3);--glow:rgba(10,132,255,.16)}

/* ---- structure (tokens only) ---- */
.lz *{box-sizing:border-box}
.lz .ic{width:1.15em;height:1.15em;fill:none;stroke:currentColor;stroke-width:1.8;stroke-linecap:round;stroke-linejoin:round;flex:none;vertical-align:-.2em}
.lz .ic.fill{fill:currentColor;stroke:none}
.lz .row,.lz .col{display:flex;gap:var(--gap);min-width:0;min-height:0}
.lz .row{flex-direction:row;align-items:stretch}.lz .row>.clock,.lz .row>.timer,.lz .row>.spin{align-self:center}.lz .col{flex-direction:column}
.lz .pages{display:flex;flex-direction:column;min-height:0;min-width:0}.lz .pg{display:contents}
.lz .grow{flex:1 1 0}.lz .auto{flex:none}
.lz .panel{position:relative;background:var(--card);-webkit-backdrop-filter:var(--blur);backdrop-filter:var(--blur);border:var(--bw) solid var(--line);border-radius:var(--r);box-shadow:var(--shadow);display:flex;flex-direction:column;min-height:0;min-width:0;overflow:hidden;transition:border-color .25s,box-shadow .25s}
.lz .panel::before{content:"";position:absolute;inset:0;pointer-events:none;border-radius:inherit;opacity:0;transition:opacity .35s;background:radial-gradient(300px circle at var(--mx,50%) var(--my,0%),var(--glow),transparent 70%)}
.lz[data-fx~=spot] .panel:hover::before{opacity:1}
.lz .panel>.ph{display:flex;gap:.5em;align-items:center;padding:10px 16px 0;font-weight:600;font-size:12px;letter-spacing:.02em;color:var(--dim)}
.lz .panel>.pb{display:flex;flex-direction:column;gap:var(--gap);padding:10px 16px 14px;min-height:0;flex:1}
.lz .text{white-space:pre-wrap;overflow-wrap:anywhere}.lz .text.dim{color:var(--dim)}.lz .text.accent{color:var(--accent)}
.lz .text.ok{color:var(--ok)}.lz .text.warn{color:var(--warn)}.lz .text.err{color:var(--err)}
.lz .text.center{text-align:center}.lz .text.right{text-align:right}
.lz .spacer{flex:1}
.lz .prog{display:flex;align-items:center;gap:10px}
.lz .trk{flex:1;height:6px;border-radius:var(--rp);background:var(--hover);overflow:hidden}
.lz .trk i{display:block;height:100%;border-radius:var(--rp);background:var(--c,var(--accent))}
.lz .prog span{color:var(--dim);font-variant-numeric:tabular-nums;min-width:3.2em;text-align:right;font-size:12px}
.lz .search{display:flex;align-items:center;gap:8px;background:var(--hover);border-radius:var(--r2);padding:0 12px;height:36px;border:1px solid transparent;transition:border-color .2s,box-shadow .2s}
.lz .search.focus{border-color:var(--accent);box-shadow:0 0 0 3px var(--sel)}
.lz .search .ic{color:var(--dim)}
.lz .search input{flex:1;min-width:0;border:0;outline:0;background:none;color:inherit;font:inherit;height:100%;text-shadow:inherit}
.lz .search input::placeholder{color:var(--dim)}
.lz .seg{position:relative;display:inline-flex;align-self:flex-start;background:var(--hover);border-radius:var(--r2);padding:3px;gap:2px;max-width:100%;overflow:auto}
.lz .seg button{all:unset;position:relative;z-index:1;cursor:default;display:flex;align-items:center;gap:6px;padding:5px 14px;border-radius:calc(var(--r2) - 3px);color:var(--dim);font-weight:500;white-space:nowrap;transition:color .2s}
.lz .seg button:hover{color:var(--fg)}
.lz .seg button.on{background:var(--card2);color:var(--fg);box-shadow:0 1px 3px rgba(0,0,0,.18)}
.lz[data-fx~=thumb] .seg button.on{background:none;box-shadow:none}
.lz .thumb{position:absolute;top:3px;bottom:3px;left:0;width:0;border-radius:calc(var(--r2) - 3px);background:var(--card2);box-shadow:0 1px 3px rgba(0,0,0,.2),0 4px 16px var(--glow);z-index:0;pointer-events:none}
.lz:not([data-fx~=thumb]) .thumb{display:none}
.lz .seg.focus{box-shadow:0 0 0 3px var(--sel)}
.lz .listbox,.lz .tree{position:relative;background:var(--card);border:1px solid var(--line);border-radius:var(--r2);overflow:auto;min-height:0;padding:4px;flex:1;display:flex;flex-direction:column;gap:1px}
.lz .tree{display:block}
.lz .listbox.focus,.lz .tree.focus,.lz .tbl.focus{border-color:var(--accent);box-shadow:0 0 0 3px var(--sel)}
.lz .hl{position:absolute;left:4px;right:4px;top:0;height:0;border-radius:var(--r3);background:var(--accent);pointer-events:none;z-index:0}
.lz .listbox:not(.focus) .hl,.lz .tree:not(.focus) .hl{background:var(--sel)}
.lz:not([data-fx~=hl]) .hl{display:none}
.lz .li,.lz .tn{position:relative;z-index:1;display:flex;align-items:center;gap:8px;padding:5px 10px;border-radius:var(--r3);cursor:default;white-space:nowrap;flex:none}
.lz .li:hover,.lz .tn:hover{background:var(--hover)}
.lz .li.sel,.lz .tn.sel{background:var(--accent);color:var(--on)}
.lz .listbox:not(.focus) .li.sel,.lz .tree:not(.focus) .tn.sel{background:var(--sel);color:var(--fg)}
.lz[data-fx~=hl] .li.sel,.lz[data-fx~=hl] .tn.sel,.lz[data-fx~=hl] .listbox:not(.focus) .li.sel,.lz[data-fx~=hl] .tree:not(.focus) .tn.sel{background:none}
.lz[data-fx~=hl] .listbox:not(.focus) .li.sel,.lz[data-fx~=hl] .tree:not(.focus) .tn.sel{color:var(--fg)}
.lz .li .ic{opacity:.8}.lz .empty{color:var(--dim);padding:8px 10px}
.lz .keybar{display:flex;flex-wrap:wrap;gap:6px 14px;color:var(--dim);font-size:12px;margin-top:auto;align-items:center}
.lz .keybar b{all:unset;cursor:default;display:inline-flex;gap:6px;align-items:center}
.lz .keybar kbd{font:600 11px ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;background:var(--card2);border:1px solid var(--line);border-bottom-width:2px;border-radius:var(--r3);padding:1px 6px;color:var(--fg)}
.lz .chart{position:relative;min-width:0;min-height:0;flex:1 1 auto}.lz svg.chart{display:block;width:100%;overflow:visible}
.lz .chart text{fill:var(--dim);font-size:11px}
.lz .bars{display:flex;gap:8px;align-items:stretch;min-width:0}.lz .bc{flex:1;display:flex;flex-direction:column;justify-content:flex-end;align-items:center;gap:3px;min-width:0}
.lz .bc i{display:block;width:70%;border-radius:var(--r3) var(--r3) 2px 2px;min-height:2px;background:var(--c,var(--accent))}.lz .bc span{font-size:11px;color:var(--dim);height:1.3em}
.lz .legend{display:flex;flex-wrap:wrap;gap:4px 14px;font-size:12px;color:var(--dim)}
.lz .legend i{display:inline-block;width:9px;height:9px;border-radius:calc(var(--r3)*.7);margin-right:6px;vertical-align:-1px}
.lz .zone{display:flex;height:14px;border-radius:var(--rp);overflow:hidden;gap:2px;background:var(--hover)}.lz .zone i{display:block;flex:0 1 0;min-width:0;background:var(--c)}
.lz .gauge{display:flex;align-items:center;gap:10px}.lz .gauge .lab{color:var(--dim);min-width:3em;font-size:12px}
.lz .gauge .trk{height:10px;transition:none}.lz .gauge .trk i{transition:background .3s}
.lz .gauge .val{min-width:3.4em;text-align:right;font-variant-numeric:tabular-nums;font-size:12px}
.lz .ring{overflow:hidden;white-space:nowrap}.lz .ring span{display:inline-block;padding-left:100%;animation:lzring var(--d,12s) linear infinite}
@keyframes lzring{to{transform:translateX(-100%)}}
.lz .spin{display:flex;align-items:center;gap:10px;color:var(--dim)}
.lz .spin i{width:16px;height:16px;border-radius:50%;border:2px solid var(--hover);border-top-color:var(--accent);animation:lzspin .8s linear infinite}
@keyframes lzspin{to{transform:rotate(360deg)}}
.lz .clock{font-variant-numeric:tabular-nums;font-size:1.6em;font-weight:300;letter-spacing:.02em}
.lz .clock.center{text-align:center}.lz .clock.right{text-align:right}
.lz .timer{display:flex;align-items:center;gap:12px;padding:6px 0}.lz .timer.focus{box-shadow:0 0 0 3px var(--sel);border-radius:var(--r2)}
.lz .timer .t{font-size:1.6em;font-weight:300;font-variant-numeric:tabular-nums;min-width:3.4em}.lz .timer.done .t{color:var(--ok)}
.lz .timer .l{color:var(--dim)}
.lz .btn{all:unset;cursor:default;width:32px;height:32px;border-radius:var(--rp);display:inline-grid;place-items:center;background:var(--hover);color:var(--fg);transition:background .2s,box-shadow .2s}
.lz .btn:hover{background:var(--sel)}.lz .btn.pri{background:var(--accent);color:var(--on)}.lz .btn.pri:hover{box-shadow:0 0 0 4px var(--sel)}
.lz .sel-wrap{display:flex;align-items:center;gap:10px;align-self:flex-start}.lz .sw{position:relative;display:inline-flex;align-items:center}.lz .sel-wrap label{color:var(--dim)}
.lz .sel-wrap select{appearance:none;-webkit-appearance:none;border:1px solid var(--line);background:var(--card2);color:var(--fg);font:inherit;border-radius:var(--r2);padding:6px 30px 6px 12px;min-width:9em;outline:none;transition:border-color .2s,box-shadow .2s}
.lz .sel-wrap select:focus,.lz .sel-wrap.focus select{border-color:var(--accent);box-shadow:0 0 0 3px var(--sel)}
.lz .sel-wrap .ic{position:absolute;right:9px;pointer-events:none;color:var(--dim)}
.lz .tbl{background:var(--card);border:1px solid var(--line);border-radius:var(--r2);overflow:auto;min-height:0;flex:1}
.lz table{border-collapse:collapse;width:100%}.lz th{position:sticky;top:0;background:var(--card);text-align:left;font-weight:600;font-size:12px;color:var(--dim);padding:8px 12px;border-bottom:1px solid var(--line);cursor:default;white-space:nowrap}
.lz th .ic{width:.9em;height:.9em;margin-left:4px}.lz td{padding:6px 12px;white-space:nowrap}.lz td.n,.lz th.n{text-align:right;font-variant-numeric:tabular-nums}
.lz tbody tr:nth-child(even) td{background:var(--hover)}.lz tbody tr.sel td{background:var(--sel)}.lz .tbl.focus tbody tr.sel td{background:var(--accent);color:var(--on)}
.lz .tn .tw{width:1em;display:inline-grid;place-items:center;opacity:.7}.lz .tn.folder .ic:nth-child(2){color:var(--accent)}.lz .tree.focus .tn.sel .ic{color:inherit}
.lz .sysbar{display:flex;flex-wrap:wrap;gap:6px 8px}.lz .sysbar.right{justify-content:flex-end}
.lz .pill{display:inline-flex;align-items:center;gap:7px;background:var(--card);-webkit-backdrop-filter:var(--blur);backdrop-filter:var(--blur);border:1px solid var(--line);border-radius:var(--rp);padding:3px 12px;font-size:12px;box-shadow:var(--shadow);font-variant-numeric:tabular-nums}
.lz .pill .pl{color:var(--dim)}.lz .pill b{font-weight:600}.lz .pill em{display:inline-block;width:28px;height:4px;border-radius:var(--rp);background:var(--hover);overflow:hidden}.lz .pill em i{display:block;height:100%;border-radius:var(--rp);background:var(--c)}
.lz .unk{color:var(--dim);border:1px dashed var(--line);border-radius:var(--r2);padding:6px 10px;font-size:12px}

/* ---- effects ---- */
.lz[data-fx~=blobs]::before{content:"";position:absolute;inset:0;z-index:-1;pointer-events:none;filter:blur(30px);background-size:210% 210%;
  background-image:radial-gradient(26% 30% at 20% 24%,var(--b1),transparent 72%),radial-gradient(24% 30% at 78% 30%,var(--b2),transparent 72%),radial-gradient(28% 32% at 56% 84%,var(--b3),transparent 72%);animation:lzblob 22s ease-in-out infinite alternate}
@keyframes lzblob{0%{background-position:0% 0%}50%{background-position:100% 40%}100%{background-position:30% 100%}}
.lz[data-fx~=grain]::after{content:"";position:absolute;inset:0;pointer-events:none;z-index:60;opacity:.09;mix-blend-mode:overlay;background-image:${GRAIN}}
.lz[data-fx~=scan]::after{content:"";position:absolute;inset:0;pointer-events:none;z-index:60;background:repeating-linear-gradient(0deg,rgba(0,0,0,.3) 0 1px,transparent 1px 3px);box-shadow:inset 0 0 140px rgba(0,0,0,.65);animation:lzflick 7s steps(1) infinite}
@keyframes lzflick{0%,92%,100%{opacity:1}93%{opacity:.82}95%{opacity:1}97%{opacity:.9}}
.lz[data-fx~=glow] .trk i,.lz[data-fx~=glow] .bc i,.lz[data-fx~=glow] .zone i,.lz[data-fx~=glow] .pill em i{box-shadow:0 0 10px var(--c,var(--accent))}
.lz[data-fx~=glow] .panel:hover{border-color:var(--accent);box-shadow:var(--shadow),0 0 22px var(--glow)}

/* ---- flat ---- */
.lz[data-look=flat]{--r:6px;--rp:4px;--gap:8px;--pad:12px;--shadow:none;--font:"Segoe UI Variable","Segoe UI",system-ui,sans-serif}
.lz[data-look=flat] .panel>.ph{padding:8px 14px 0}
.lz[data-look=flat] .seg{background:none;padding:0;gap:4px}
.lz[data-look=flat] .seg button{border-radius:4px}.lz[data-look=flat] .seg button.on{background:var(--sel);color:var(--fg);box-shadow:inset 0 -2px 0 var(--accent)}
.lz[data-look=flat] .search{background:var(--card);border-color:var(--line);border-bottom-color:var(--dim)}

/* ---- glass ---- */
.lz[data-look=glass]{--r:24px;--gap:14px;--pad:18px;--blur:blur(22px) saturate(1.7);--b1:rgba(110,130,255,.8);--b2:rgba(255,110,200,.7);--b3:rgba(80,220,230,.75);
  --bg:#e9ecff;--card:rgba(255,255,255,.5);--card2:rgba(255,255,255,.8);--line:rgba(255,255,255,.65);--hover:rgba(40,50,120,.07);--glow:rgba(255,255,255,.55);
  --shadow:0 10px 40px rgba(60,70,160,.16),inset 0 1px 0 rgba(255,255,255,.8)}
.lz[data-look=glass][data-dark]{--bg:#0d0f24;--card:rgba(255,255,255,.07);--card2:rgba(255,255,255,.16);--line:rgba(255,255,255,.16);--hover:rgba(255,255,255,.08);--glow:rgba(160,170,255,.22);
  --b1:rgba(90,80,255,.7);--b2:rgba(255,60,170,.5);--b3:rgba(0,200,220,.5);--shadow:0 10px 40px rgba(0,0,0,.4),inset 0 1px 0 rgba(255,255,255,.14)}

/* ---- neon ---- */
.lz[data-look=neon]{--r:16px;--gap:12px;--font:"Inter","SF Pro Text",system-ui,sans-serif;--bg:radial-gradient(120% 80% at 50% -10%,#1b1040,#06060d 62%);--card:rgba(14,14,30,.82);--card2:#1b1b3a;
  --fg:#e8eaff;--dim:#8b8fb8;--line:rgba(110,100,255,.35);--hover:rgba(120,110,255,.1);--accent:#00f0ff;--on:#00121a;--ok:#39ff88;--warn:#ffe14d;--err:#ff3b6b;--c5:#ff2bd6;--c6:#8a6bff;
  --sel:rgba(0,240,255,.2);--glow:rgba(0,240,255,.2);--shadow:0 0 0 1px rgba(0,240,255,.06),0 0 28px rgba(110,100,255,.14)}
.lz[data-look=neon] .clock,.lz[data-look=neon] .timer .t{text-shadow:0 0 14px var(--accent)}

/* ---- aurora ---- */
.lz[data-look=aurora]{--r:28px;--gap:16px;--pad:20px;--blur:blur(26px) saturate(1.5);--bg:#07060f;--card:rgba(255,255,255,.055);--card2:rgba(255,255,255,.14);
  --fg:#f1efff;--dim:#a39fc7;--line:rgba(255,255,255,.13);--hover:rgba(255,255,255,.07);--accent:#8f7bff;--on:#0b0720;--ok:#5fe3a1;--warn:#ffd36b;--err:#ff6b8b;--c5:#ff7ad9;--c6:#6be6ff;
  --sel:rgba(143,123,255,.3);--glow:rgba(170,150,255,.2);--b1:rgba(110,70,255,.85);--b2:rgba(0,190,255,.6);--b3:rgba(255,60,160,.6);
  --shadow:0 18px 60px rgba(0,0,0,.45),inset 0 1px 0 rgba(255,255,255,.12)}
.lz[data-look=aurora] .panel>.ph{font-size:11px;letter-spacing:.14em;text-transform:uppercase}
.lz[data-look=aurora] .btn.pri{background:linear-gradient(135deg,#8f7bff,#ff7ad9)}

/* ---- terminal ---- */
.lz[data-look=terminal]{--r:0px;--rp:0px;--gap:10px;--pad:12px;--font:ui-monospace,"JetBrains Mono","Fira Code","Cascadia Code",Menlo,Consolas,monospace;
  --bg:#010804;--card:#010804;--card2:#04200d;--fg:#58ff85;--dim:#27a24c;--line:#1f8f41;--hover:rgba(88,255,133,.09);--accent:#58ff85;--on:#001a08;
  --ok:#58ff85;--warn:#ffd24d;--err:#ff5d5d;--c5:#ff7bff;--c6:#4dffe8;--sel:rgba(88,255,133,.22);--shadow:0 0 14px rgba(88,255,133,.1);--glow:rgba(88,255,133,.1);--ts:0 0 6px rgba(88,255,133,.5)}
.lz[data-look=terminal] .panel>.ph{text-transform:uppercase;letter-spacing:.14em;padding-top:8px}
.lz[data-look=terminal] .panel>.ph::before{content:"\\258C"}
.lz[data-look=terminal] .trk{background:transparent;border:1px solid var(--line);padding:1px}
.lz[data-look=terminal] .trk i,.lz[data-look=terminal] .bc i{background:repeating-linear-gradient(90deg,var(--c,var(--accent)) 0 .55em,transparent .55em .7em)}
.lz[data-look=terminal] .bc i{background:repeating-linear-gradient(0deg,var(--c,var(--accent)) 0 .5em,transparent .5em .65em)}
.lz[data-look=terminal] .seg{background:none;padding:0;gap:6px}
.lz[data-look=terminal] .seg button{border:1px solid var(--line)}.lz[data-look=terminal] .seg button.on{background:var(--accent);color:var(--on)}
.lz[data-look=terminal] .clock,.lz[data-look=terminal] .timer .t{font-weight:400}
.lz[data-look=terminal] .search input{caret-color:var(--accent)}
.lz[data-look=terminal] .btn{border:1px solid var(--line)}

/* ---- classic ---- */
.lz[data-look=classic]{--r:0px;--rp:0px;--gap:10px;--pad:10px;--bw:0px;--font:"MS Sans Serif",Tahoma,"Segoe UI",Verdana,sans-serif;--fs:12px;
  --bg:#008080;--card:#c0c0c0;--card2:#c0c0c0;--fg:#000;--dim:#404040;--line:#808080;--hover:rgba(0,0,0,.08);--accent:#000080;--on:#fff;
  --ok:#006400;--warn:#806000;--err:#a00000;--c5:#800080;--c6:#008080;--sel:#a8a8d0;--shadow:none;--glow:transparent;
  --bevel:inset -1px -1px #0a0a0a,inset 1px 1px #fff,inset -2px -2px #808080,inset 2px 2px #dfdfdf;
  --sunk:inset -1px -1px #fff,inset 1px 1px #808080,inset -2px -2px #dfdfdf,inset 2px 2px #0a0a0a}
.lz[data-look=classic]{color:#fff;--dim:#d8f0f0;text-shadow:none}
.lz[data-look=classic] .panel,.lz[data-look=classic] .pill,.lz[data-look=classic] .search,.lz[data-look=classic] .listbox,.lz[data-look=classic] .tree,.lz[data-look=classic] .tbl{color:#000;--dim:#404040}
.lz[data-look=classic] .panel{box-shadow:var(--bevel);padding:3px}
.lz[data-look=classic] .panel>.ph{background:linear-gradient(90deg,#000080,#1084d0);color:#fff;padding:2px 6px;font-size:12px;letter-spacing:0}
.lz[data-look=classic] .panel>.pb{padding:8px 8px 6px}
.lz[data-look=classic] .search,.lz[data-look=classic] .listbox,.lz[data-look=classic] .tree,.lz[data-look=classic] .tbl,.lz[data-look=classic] .trk,.lz[data-look=classic] .zone{background:#fff;border:0;box-shadow:var(--sunk)}
.lz[data-look=classic] .search.focus,.lz[data-look=classic] .listbox.focus,.lz[data-look=classic] .tree.focus,.lz[data-look=classic] .tbl.focus{box-shadow:var(--sunk),0 0 0 1px #000}
.lz[data-look=classic] .trk i,.lz[data-look=classic] .bc i{background:repeating-linear-gradient(90deg,var(--c,var(--accent)) 0 8px,transparent 8px 10px)}
.lz[data-look=classic] .bc i{background:var(--c,var(--accent))}
.lz[data-look=classic] .btn,.lz[data-look=classic] .seg button,.lz[data-look=classic] .sel-wrap select,.lz[data-look=classic] .keybar kbd{background:#c0c0c0;box-shadow:var(--bevel);border:0;color:#000}
.lz[data-look=classic] .btn.pri{background:#c0c0c0;color:#000}.lz[data-look=classic] .btn.pri:hover,.lz[data-look=classic] .btn:hover{box-shadow:var(--bevel)}
.lz[data-look=classic] .seg{background:none;padding:0;gap:4px}.lz[data-look=classic] .seg button.on{box-shadow:var(--sunk);background:#d8d8d8;color:#000}
.lz[data-look=classic] th{background:#c0c0c0;box-shadow:var(--bevel);border:0}
.lz[data-look=classic] .clock,.lz[data-look=classic] .timer .t{font-weight:400}
.lz[data-look=classic] .pill{box-shadow:var(--bevel);border:0}

.lz .map{position:relative;min-height:180px;border-radius:var(--r2);overflow:hidden;background:#07101c;border:1px solid var(--line);flex:1}
.lz .map.focus{border-color:var(--accent);box-shadow:0 0 0 3px var(--sel)}
.lz .map canvas{position:absolute;inset:0;width:100%;height:100%;display:block;touch-action:none;cursor:grab;image-rendering:auto}
.lz .map.grab canvas{cursor:grabbing}
.lz .lbls{position:absolute;inset:0;pointer-events:none;overflow:hidden}
.lz .lbls span{position:absolute;transform:translate(-50%,0);font-size:11px;line-height:14px;color:#f4f6ee;white-space:nowrap;text-shadow:0 0 3px #000,0 0 5px #000,0 1px 2px #000}
.lz .mapcap{position:absolute;left:10px;bottom:10px;display:none;flex-direction:column;gap:1px;padding:8px 12px;border-radius:var(--r2);background:var(--card);border:1px solid var(--line);box-shadow:var(--shadow);-webkit-backdrop-filter:blur(var(--blur));backdrop-filter:blur(var(--blur));font-size:12px;color:var(--fg);pointer-events:none}
.lz .mapcap.on{display:flex}.lz .mapcap b{font-size:13px}.lz .mapcap span{color:var(--dim)}
.lz .mm{position:relative;background:var(--card);border:1px solid var(--line);border-radius:var(--r2);overflow:hidden;min-height:60px;flex:1;cursor:pointer;touch-action:none}
.lz .mmr{position:absolute;inset:3px 3px 3px 3px;display:flex;flex-direction:column;gap:1px}
.lz .mmr i{flex:1;min-height:1px;background:var(--dim);opacity:.55;border-radius:1px}
.lz .mmr i.mk{background:var(--warn);opacity:1}
.lz .mmv{position:absolute;left:0;right:0;background:var(--sel);border-block:1px solid var(--accent);pointer-events:none}
`;
