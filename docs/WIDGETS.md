# lazyui widgets, window and web

## Native widgets

Charts and the like need code, so they are Rust, registered by name (`lazyui widgets ls` lists them with their attributes).
Data comes from `values="1,2,3"` or `bind=KEY` (a comma list in the model; `ui.push(key, sample, cap)` appends and
keeps the newest `cap`). Done: `graph linechart barchart histogram heatgrid zonebar piebar segbar gauge ringbar spinner clock timer tree dropdown table sysbar worldmap globe minimap radar rain scramble tagcloud webchart piechart`. They draw with
block/braille glyphs on a Unicode terminal and with `# = . :` on an ASCII one, so a snapshot is the same everywhere.
`ringbar` and `spinner` move with the Ui clock (`tick`), never the wall clock.

## Shared widgets

A widget is a spec file with parameters, used by its file name:

```text
# meter.lui
params label bind max=100
row
  text text="$label" size=8 style=dim
  progressbar bind=$bind max=$max
```

`meter label=CPU bind=cpu` in any spec. `$name` is replaced by the use-site value (or the default), other attributes
(`size`, `id`...) go on the root, a `slot` line takes the children written under the use. Where they live, later wins:
**builtin** (compiled in, works offline: `card meter stat titlebar statusbar`), **global** (`$LAZYUI_WIDGETS` or
`~/.config/lazyui/widgets`, shared by every program; `lazyui widgets add URL|FILE` fetches into it), **local**
(`widgets/` next to the spec, `./widgets`, `--widgets DIR`). `lazyui widgets ls|show|dirs`. In the browser the page hands
its widgets over: `mount(canvas, { widgets: { name: text } })`. The format is plain text on purpose: lazytui (C) can read it.

## Window (feature `gui`, on by default)

`lazyui run SPEC --gui` is the same spec and the same pipes in a window; `lazyui snap SPEC --png out.png` renders the
settled frame with the window's rasteriser (no display, no GPU). Library: `lazyui::gui::drive_gui`, the counterpart of
`runtime::drive`. Use `default-features = false` for the terminal-only engine that needs just `libc`.

Pixels reach the screen on the CPU (shared memory, works on any compositor or X11) or on the GPU (wgpu, feature `gpu`).
`--render auto` (default) starts on the CPU and moves to the GPU only when frames measure slow and the GPU makes them
clearly cheaper; if not, it goes back and stops trying. `--render cpu|gpu` forces one; a GPU that cannot be created
falls back to the CPU. Animation slows toward 10 Hz when frames are slow. Idle costs nothing.

## Web (feature `web`, WebAssembly)

The same `Ui` in a browser tab, as a static page: no server, no framework, no npm. `web/run` builds the engine to a
170 KB `.wasm` (`wasm32-unknown-unknown`) and serves `web/dist` on :8000; `web/lazyui.js` paints the cells on a
`<canvas>` with the browser's fonts and turns keys, mouse, wheel and paste into events.

```js
import { mount } from "./lazyui.js";
const ui = await mount(canvas, { wasm: "lazyui.wasm", spec });
ui.set("cpu", 42);  ui.on("activate", e => ...);  ui.key("ctrl-r");  ui.theme("green");  ui.reload(spec2);
```

The page sleeps when nothing moves (no timer, no animation frame). `node web/test.mjs` drives the built module without a
browser. The terminal-only parts (`term`, `runtime`, `session`, `libc`) are not compiled for wasm.

## Not yet

Kitty keyboard protocol, tree/dropdown/scroll containers, the lazytui bars (ring/zone/pacman), pixel-native widgets
(smooth progress, rounded rects, proportional fonts), touch/IME input in the web page. See `DESIGN.md` for the honest list.
