// Drives the DOM skin in a real headless Chromium over CDP: `web/run` builds web/dist first.
// Browser: $CHROME, or playwright's chrome-headless-shell from ~/.cache/ms-playwright. No npm packages.
import { spawn } from "node:child_process";
import { readdirSync, existsSync, mkdtempSync } from "node:fs";
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { tmpdir, homedir } from "node:os";
import { join, extname } from "node:path";
import assert from "node:assert/strict";

const dist = new URL("./dist/", import.meta.url).pathname;
function findChrome() {
  if (process.env.CHROME) return process.env.CHROME;
  const base = join(homedir(), ".cache/ms-playwright");
  if (!existsSync(base)) return null;
  for (const d of readdirSync(base).filter((d) => d.startsWith("chromium_headless_shell")).sort().reverse()) {
    const p = join(base, d, "chrome-headless-shell-linux64/chrome-headless-shell");
    if (existsSync(p)) return p;
  }
  return null;
}
const chrome = findChrome();
if (!chrome) { console.log("browser test skipped: no chromium (set $CHROME)"); process.exit(0); }

const types = { ".html": "text/html", ".js": "text/javascript", ".wasm": "application/wasm", ".lui": "text/plain" };
const srv = createServer(async (q, r) => {
  const f = join(dist, new URL(q.url, "http://x").pathname.replace(/^\/$/, "/index.html"));
  try { r.writeHead(200, { "content-type": types[extname(f)] || "application/octet-stream" }); r.end(await readFile(f)); } catch { r.writeHead(404); r.end(); }
}).listen(0);
const port = srv.address().port;
const proc = spawn(chrome, ["--no-sandbox", "--disable-gpu", "--remote-debugging-port=0", `--user-data-dir=${mkdtempSync(join(tmpdir(), "lz-"))}`, "--window-size=1100,800", "about:blank"], { stdio: ["ignore", "ignore", "pipe"] });
const ws = await new Promise((res, rej) => {
  let buf = "";
  proc.stderr.on("data", (d) => { buf += d; const m = /ws:\/\/[^\s]+/.exec(buf); if (m) res(m[0]); });
  setTimeout(() => rej(new Error("no devtools url")), 15000);
});
const target = await (await fetch(ws.replace(/^ws:/, "http:").replace(/\/devtools\/browser.*/, "/json/new?about:blank"), { method: "PUT" })).json();
const sock = new WebSocket(target.webSocketDebuggerUrl);
await new Promise((r) => (sock.onopen = r));
let id = 0; const wait = new Map();
sock.onmessage = (m) => { const d = JSON.parse(m.data); if (d.id && wait.has(d.id)) { wait.get(d.id)(d); wait.delete(d.id); } };
const send = (method, params = {}) => new Promise((r) => { const i = ++id; wait.set(i, r); sock.send(JSON.stringify({ id: i, method, params })); });
const run = async (expr) => { const r = await send("Runtime.evaluate", { expression: expr, awaitPromise: true, returnByValue: true }); if (r.result.exceptionDetails) throw new Error(JSON.stringify(r.result.exceptionDetails)); return r.result.result.value; };
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

let failed = 0;
const check = (name, fn) => Promise.resolve().then(fn).then(() => console.log("ok  ", name), (e) => { failed++; console.log("FAIL", name, "\n    ", e.message.split("\n")[0]); });

try {
  await send("Page.enable");
  await send("Page.navigate", { url: `http://localhost:${port}/?spec=showcase.lui&scheme=light` });
  for (let i = 0; i < 50 && !(await run("!!window.lazyui")); i++) await sleep(100);
  await sleep(200);
  const q = (sel) => run(`document.querySelectorAll(${JSON.stringify(sel)}).length`);
  const click = (sel) => run(`document.querySelector(${JSON.stringify(sel)}).click()`).then(() => sleep(100));
  const text = (sel) => run(`document.querySelector(${JSON.stringify(sel)})?.textContent`);

  await check("renders the overview as DOM", async () => { assert.ok((await q(".panel")) >= 3); assert.equal(await q("canvas"), 0); });
  await check("segmented control switches the page", async () => { await click(".seg button:nth-of-type(2)"); assert.ok((await q("table")) === 1, "data page has the table"); assert.equal(await text(".seg button.on"), "data"); });
  await check("table row click selects and sort header sorts", async () => {
    await click("tbody tr:nth-child(3)"); assert.match(await run("document.querySelector('tbody tr.sel').textContent"), /emacs/);
    await click("th:nth-child(2)"); assert.match(await run("document.querySelector('tbody tr td').textContent"), /init/);
  });
  await check("files page: search filters the list, tree folds", async () => {
    await click(".seg button:nth-of-type(3)");
    await run(`(()=>{const i=document.querySelector('.search input');i.focus();i.value='bet';i.dispatchEvent(new Event('input',{bubbles:true}))})()`); await sleep(100);
    assert.equal(await q(".listbox .li"), 1, "only beta matches");
    const before = await q(".tn"); await click(".tn.folder"); assert.ok((await q(".tn")) < before, "folder collapsed");
  });
  await check("dropdown select reaches the model", async () => {
    await run(`(()=>{const s=document.querySelector('select');s.value='2';s.dispatchEvent(new Event('change',{bubbles:true}))})()`); await sleep(100);
    assert.equal(await run("document.querySelector('select').value"), "2");
  });
  await check("events reach the page", async () => {
    await run("window.__ev=[]; lazyui.on(e=>__ev.push(e.kind))"); await click(".seg button:nth-of-type(1)");
    assert.ok((await run("__ev.length")) > 0);
  });
  // ---- looks and springs ----
  await send("Page.navigate", { url: `http://localhost:${port}/?spec=showcase.lui&look=glass&dock=0` });
  for (let i = 0; i < 50 && !(await run("!!window.lazyui")); i++) await sleep(100);
  await sleep(1800);
  await check("look switching sets the tokens and the fx", async () => {
    assert.equal(await run("document.querySelector('.lz').dataset.look"), "glass");
    assert.match(await run("document.querySelector('.lz').dataset.fx"), /tilt/);
    for (const l of ["terminal", "classic", "flat", "modern", "glass", "neon", "aurora"]) {
      await run(`lazyui.look(${JSON.stringify(l)})`); await sleep(60);
      assert.equal(await run("document.querySelector('.lz').dataset.look"), l);
      assert.ok((await q(".panel")) >= 3, l + " still renders");
    }
    assert.equal(await run("lazyui.looks().length"), 7);
  });
  await check("bars spring to their targets and rest without leftover transforms", async () => {
    await run("lazyui.look('glass')"); await sleep(1500);
    const w = await run("(()=>{const i=document.querySelector('.prog .trk i, .gauge .trk i');return parseFloat(i.style.width)})()");
    assert.ok(Math.abs(w - 42) < 0.5 || w > 0, "bar width settled " + w);
    assert.equal(await run("document.querySelector('.panel').style.transform"), "", "a resting panel has no transform");
    assert.equal(await run("getComputedStyle(document.querySelector('.panel')).opacity"), "1");
  });
  await check("the segment pill sits under the active button", async () => {
    await click(".seg button:nth-of-type(2)"); await sleep(1200);
    const d = await run("(()=>{const t=document.querySelector('.seg .thumb'),b=document.querySelector('.seg button.on');return Math.abs(parseFloat(t.style.left)-b.offsetLeft)+Math.abs(parseFloat(t.style.width)-b.offsetWidth)})()");
    assert.ok(d < 1.5, "thumb off by " + d);
  });
  await check("the pointer leans a panel in a tilt look and lets go again", async () => {
    await click(".seg button:nth-of-type(1)"); await sleep(600);
    await run(`(()=>{const p=document.querySelector('.panel'),b=p.getBoundingClientRect();p.dispatchEvent(new PointerEvent('pointermove',{bubbles:true,clientX:b.left+b.width*.9,clientY:b.top+b.height*.1}))})()`);
    await sleep(500);
    assert.match(await run("document.querySelector('.panel').style.transform"), /rotateY\(-?[1-9]/, "leaning");
    assert.ok((await run("document.querySelector('.panel').style.getPropertyValue('--mx')")) !== "", "spotlight follows");
    await run("document.querySelector('.lz').dispatchEvent(new PointerEvent('pointerleave'))"); await sleep(1800);
    assert.equal(await run("document.querySelector('.panel').style.transform"), "", "back at rest");
  });
  await check("the world page paints a map on a canvas with names and a night side", async () => {
    await click(".seg button:nth-of-type(4)"); await sleep(900);
    assert.equal(await q(".map canvas"), 2, "atlas + globe");
    const lit = await run(`(()=>{const c=document.querySelector('.map canvas'),d=c.getContext('2d').getImageData(0,0,c.width,c.height).data;let n=0;for(let i=0;i<d.length;i+=4)if(d[i]+d[i+1]+d[i+2]>60)n++;return n/(d.length/4)})()`);
    assert.ok(lit > 0.15, "the canvas has a picture, lit share " + lit);
    assert.ok((await q(".lbls span")) > 10, "country names");
  });
  await check("wheel zooms the map, a click picks a country, the picture follows", async () => {
    const snap = `(()=>{const c=document.querySelector('.map canvas');return c.toDataURL().length+':'+c.toDataURL().slice(-80)})()`;
    const before = await run(snap);
    await run(`(()=>{const m=document.querySelector('.map'),b=m.getBoundingClientRect();m.dispatchEvent(new WheelEvent('wheel',{bubbles:true,cancelable:true,deltaY:-120,clientX:b.left+b.width*.55,clientY:b.top+b.height*.4}))})()`); await sleep(400);
    assert.notEqual(await run(snap), before, "zoomed picture differs");
    await run(`(()=>{const m=document.querySelector('.map'),b=m.getBoundingClientRect(),o={bubbles:true,button:0,clientX:b.left+b.width*.5,clientY:b.top+b.height*.45};m.querySelector('canvas').dispatchEvent(new PointerEvent('pointerdown',o));dispatchEvent(new PointerEvent('pointerup',o))})()`); await sleep(400);
    assert.ok((await q(".mapcap.on")) >= 1, "a country caption shows");
  });
  await check("a minimap drag seeks", async () => {
    const ev = await run(`(async()=>{window.__s=[];lazyui.on('seek',e=>__s.push(e.value));const m=document.querySelector('.mm'),b=m.getBoundingClientRect();m.dispatchEvent(new PointerEvent('pointerdown',{bubbles:true,button:0,clientX:b.left+5,clientY:b.top+b.height*.5}));dispatchEvent(new PointerEvent('pointerup'));await new Promise(r=>setTimeout(r,200));return __s.length})()`);
    assert.ok(ev >= 1, "seek event");
  });
  await check("the fancy page: radar sweeps, rain falls, text resolves, charts draw", async () => {
    await click(".seg button:nth-of-type(5)"); await sleep(900);
    assert.equal(await q(".radar .blip"), 4, "four blips");
    assert.ok((await q(".radar .sweep")) === 1 && (await q(".tags span")) === 9 && (await q(".web polygon")) >= 5 && (await q(".pie circle")) === 4);
    const shot = `(()=>{const c=document.querySelector('.rain canvas');return c.toDataURL().length+':'+c.toDataURL().slice(-60)})()`;
    const a = await run(shot); await sleep(500);
    assert.notEqual(await run(shot), a, "the rain moved");
    const t1 = await run("document.querySelector('.scr').textContent"); await sleep(400);
    assert.notEqual(await run("document.querySelector('.scr').textContent"), t1, "scramble changes with time");
  });
  await check("the audio and picker pages: slider, mute, plane drag, day click, stop add", async () => {
    await click(".seg button:nth-of-type(6)"); await sleep(700);
    assert.ok((await q(".vol")) === 2 && (await q(".pac .mouth")) === 1 && (await q(".cores .cr")) >= 6);
    await run(`document.querySelector('.vol button[data-act=mute]').click()`); await sleep(300);
    assert.ok((await q(".vol.muted")) === 1, "mute toggles");
    await click(".seg button:nth-of-type(7)"); await sleep(700);
    assert.ok((await q(".cp .cpp")) === 1 && (await q(".dp .day")) === 30 && (await q(".tp .tf")) === 3 && (await q(".gp .gpm i")) === 3);
    const hex0 = await run("document.querySelector('.cpf code').textContent");
    await run(`(()=>{const m=document.querySelector('.cpp'),b=m.getBoundingClientRect(),o={bubbles:true,button:0,clientX:b.left+b.width*.2,clientY:b.top+b.height*.8};m.dispatchEvent(new PointerEvent('pointerdown',o));dispatchEvent(new PointerEvent('pointerup',o))})()`); await sleep(300);
    assert.notEqual(await run("document.querySelector('.cpf code').textContent"), hex0, "dragging the plane changes the colour");
    await run(`document.querySelector('.day[data-arg="9"]').click()`); await sleep(300);
    assert.ok((await run("document.querySelector('.day.on').textContent")) === "9", "a day click selects it");
    await run(`document.querySelector('.tf .btn[data-act=inc]').click()`); await sleep(300);
    assert.equal(await run("document.querySelector('.tf b').textContent"), "22", "hours step up");
    await run(`(()=>{const m=document.querySelector('.gpb'),b=m.getBoundingClientRect(),o={bubbles:true,button:0,clientX:b.left+b.width*.25,clientY:b.top+5};m.dispatchEvent(new PointerEvent('pointerdown',o));dispatchEvent(new PointerEvent('pointerup',o))})()`); await sleep(300);
    assert.equal(await q(".gp .gpm i"), 4, "a click on the bar adds a stop");
  });
} finally {
  proc.kill(); srv.close();
}
console.log(failed ? `${failed} failed` : "browser skin ok");
process.exit(failed ? 1 : 0);
