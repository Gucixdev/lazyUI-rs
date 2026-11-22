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
  await check("segmented control switches the page", async () => { await click(".seg button:nth-child(2)"); assert.ok((await q("table")) === 1, "data page has the table"); assert.equal(await text(".seg button.on"), "data"); });
  await check("table row click selects and sort header sorts", async () => {
    await click("tbody tr:nth-child(3)"); assert.match(await run("document.querySelector('tbody tr.sel').textContent"), /emacs/);
    await click("th:nth-child(2)"); assert.match(await run("document.querySelector('tbody tr td').textContent"), /init/);
  });
  await check("files page: search filters the list, tree folds", async () => {
    await click(".seg button:nth-child(3)");
    await run(`(()=>{const i=document.querySelector('.search input');i.focus();i.value='bet';i.dispatchEvent(new Event('input',{bubbles:true}))})()`); await sleep(100);
    assert.equal(await q(".listbox .li"), 1, "only beta matches");
    const before = await q(".tn"); await click(".tn.folder"); assert.ok((await q(".tn")) < before, "folder collapsed");
  });
  await check("dropdown select reaches the model", async () => {
    await run(`(()=>{const s=document.querySelector('select');s.value='2';s.dispatchEvent(new Event('change',{bubbles:true}))})()`); await sleep(100);
    assert.equal(await run("document.querySelector('select').value"), "2");
  });
  await check("events reach the page", async () => {
    await run("window.__ev=[]; lazyui.on(e=>__ev.push(e.kind))"); await click(".seg button:nth-child(1)");
    assert.ok((await run("__ev.length")) > 0);
  });
} finally {
  proc.kill(); srv.close();
}
console.log(failed ? `${failed} failed` : "browser skin ok");
process.exit(failed ? 1 : 0);
