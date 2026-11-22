// Screenshot the DOM skin in real time (animations run, unlike chrome --screenshot's virtual time).
//   node web/shot.mjs OUT.png 'look=glass&spec=showcase.lui' [waitMs=1800] [WxH=1100x800] ['js to run after load']
// Serves web/dist; browser is $CHROME or playwright's chrome-headless-shell. No npm packages.
import { spawn } from "node:child_process";
import { readdirSync, existsSync, mkdtempSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { tmpdir, homedir } from "node:os";
import { join, extname } from "node:path";

const [out, query = "", waitMs = "1800", size = "1100x800", after = ""] = process.argv.slice(2);
if (!out) { console.error("usage: shot.mjs OUT.png 'query' [waitMs] [WxH] [js]"); process.exit(2); }
const [W, H] = size.split("x").map(Number);
const dist = new URL("./dist/", import.meta.url).pathname;
function findChrome() {
  if (process.env.CHROME) return process.env.CHROME;
  const base = join(homedir(), ".cache/ms-playwright");
  for (const d of existsSync(base) ? readdirSync(base).filter((d) => d.startsWith("chromium_headless_shell")).sort().reverse() : []) {
    const p = join(base, d, "chrome-headless-shell-linux64/chrome-headless-shell");
    if (existsSync(p)) return p;
  }
  return null;
}
const chrome = findChrome();
if (!chrome) { console.error("no chromium (set $CHROME)"); process.exit(1); }
const types = { ".html": "text/html", ".js": "text/javascript", ".wasm": "application/wasm", ".lui": "text/plain" };
const srv = createServer(async (q, r) => {
  const f = join(dist, new URL(q.url, "http://x").pathname.replace(/^\/$/, "/index.html"));
  try { r.writeHead(200, { "content-type": types[extname(f)] || "application/octet-stream" }); r.end(await readFile(f)); } catch { r.writeHead(404); r.end(); }
}).listen(0);
const port = srv.address().port;
const proc = spawn(chrome, ["--no-sandbox", "--disable-gpu", "--remote-debugging-port=0", `--user-data-dir=${mkdtempSync(join(tmpdir(), "lz-"))}`, `--window-size=${W},${H}`, "about:blank"], { stdio: ["ignore", "ignore", "pipe"] });
const ws = await new Promise((res, rej) => { let b = ""; proc.stderr.on("data", (d) => { b += d; const m = /ws:\/\/[^\s]+/.exec(b); if (m) res(m[0]); }); setTimeout(() => rej(new Error("no devtools url")), 15000); });
const target = await (await fetch(ws.replace(/^ws:/, "http:").replace(/\/devtools\/browser.*/, "/json/new?about:blank"), { method: "PUT" })).json();
const sock = new WebSocket(target.webSocketDebuggerUrl);
await new Promise((r) => (sock.onopen = r));
let id = 0; const wait = new Map();
sock.onmessage = (m) => { const d = JSON.parse(m.data); if (d.id && wait.has(d.id)) { wait.get(d.id)(d); wait.delete(d.id); } };
const send = (method, params = {}) => new Promise((r) => { const i = ++id; wait.set(i, r); sock.send(JSON.stringify({ id: i, method, params })); });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
try {
  await send("Emulation.setDeviceMetricsOverride", { width: W, height: H, deviceScaleFactor: 1, mobile: false });
  await send("Page.enable");
  await send("Page.navigate", { url: `http://localhost:${port}/?${query}` });
  for (let i = 0; i < 60; i++) { const r = await send("Runtime.evaluate", { expression: "!!window.lazyui", returnByValue: true }); if (r.result.result.value) break; await sleep(100); }
  if (after) { const r = await send("Runtime.evaluate", { expression: after, awaitPromise: true, returnByValue: true }); if (r.result.exceptionDetails) console.error(JSON.stringify(r.result.exceptionDetails)); else if (r.result.result.value !== undefined) console.log(JSON.stringify(r.result.result.value)); }
  await sleep(+waitMs);
  const shot = await send("Page.captureScreenshot", { format: "png" });
  writeFileSync(out, Buffer.from(shot.result.data, "base64"));
  console.log(out);
} finally { proc.kill(); srv.close(); }
