# lazyui

A terminal UI engine in Rust that works two ways from the same code: as a **library** you embed, and as **one binary**
you call. One dependency (`libc`), no async runtime, no serde.

It is a mix of ideas: the Elm loop of iced, the retained keyed tree and headless-first testing of rune, the cell buffer
and diff of ratatui, and the spec language, springs and fallbacks of lazytui. What was taken from each, and what was
left out, is in [`references/`](references/) (one file per project) and [`DESIGN.md`](DESIGN.md).

## Try it

`./run` - one key picks the backend for the demo: terminal, window, browser (wasm) or a text snapshot.

## As a binary

Describe the UI in a spec file (`examples/demo.lui`):

```
col
  buttonbar id=tabs items="main,extras" active=tab
  pages bind=tab
    col
      panel title="load" size=3
        progressbar name=cpu bind=cpu max=100
      searchbar id=q prompt="> " bind=query
      list id=files items="alpha,beta,gamma" filter=query
    text text="nothing on this page yet" style=dim
  keybar hints="tab:focus;ctrl-q:quit"
```

```
lazyui run  examples/demo.lui --watch      # on the terminal; edits to the file reload live
lazyui snap examples/demo.lui --size 40x12 # one frame as text
lazyui tree examples/demo.lui              # element tree as JSON: ids, rects, state
lazyui select examples/demo.lui '~gamma'   # find elements: #id, kind, ~text
lazyui script examples/demo.lui examples/demo.script
```

`run` draws on `/dev/tty` and keeps the pipes for data, so it composes in a shell:

- **stdout**: one JSON line per event, `{"id":"q","kind":"change","text":"ab","index":-1,"number":0}`
- **stdin**: `cpu=64` sets a model value, `list:files=a,b,c` replaces the rows of a list, `quit` ends

```
(while sleep 1; do echo "cpu=$(( RANDOM % 100 ))"; done) | lazyui run examples/demo.lui | while read ev; do echo "$ev" >&2; done
```

`--inline N` draws N rows under the prompt instead of taking the screen. `--hotkey f5=refresh` and `--every 2=poll`
turn keys and timers into events. A spec with an error reports `file: line N: message`; with `--watch` a bad edit keeps
the old UI running.

Elements: `row col panel pages spacer text progressbar searchbar buttonbar list keybar`.
Sizes: `size=N | fill | fill:W | pct:P | min:N | max:N`.

## As a library

```rust
use lazyui::runtime::{run, Cx, Opts, Program};
use lazyui::ui::{buttons, col, text, El, Ui, UiEvent};

struct App { n: i64 }
enum Msg { Inc }

impl Program for App {
    type Msg = Msg;
    fn update(&mut self, m: Msg, _: &mut Cx<Msg>) { match m { Msg::Inc => self.n += 1 } }
    fn view(&self) -> El { col([text(format!("n = {}", self.n)), buttons("b", &["+"])]) }
    fn event(&mut self, e: &UiEvent) -> Option<Msg> {
        (e.id == "b" && e.kind == "activate").then_some(Msg::Inc)
    }
}

fn main() -> std::io::Result<()> { run(&mut App { n: 0 }, Opts::default()) }
```

State lives in your struct, `view` is pure, effects run on threads (`cx.spawn(|| Msg::Done(..))`) and return as
messages; `Latest` drops answers to superseded requests. `examples/counter.rs` shows an effect.
`Ui::from_spec` gives the same tree from a spec file when you prefer that over code.

## Testing without a terminal

Everything is testable headless, on a virtual clock: `Harness` (embedding), `headless::run_script` (spec), and the
`script` command. A script is a few lines of `key`, `type`, `click`, `tick`, then `expect text/row/focus/model/event`;
a failure names the line. `tests/pty_run.py` drives the real binary in a pseudo-terminal.

## Layers

`canvas` cell grid + diff + colour/ASCII fallback, `layout` fixed/pct/fill/min/max solver, `anim` RK4 springs,
`theme` data themes, `input` byte decoder (utf-8, CSI, SGR mouse, paste), `widgets`, `ui` tree + spec + model + focus,
`headless`, `term` raw mode via libc, `runtime` the loop.

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
(smooth progress, rounded rects, proportional fonts), touch/IME input in the web page, `lazyui serve`. See `DESIGN.md` for the honest list.

MIT.

## Web skins

Two ways to show the same engine in a browser tab, picked by what you mount:

- `mount(canvas, ...)` in `web/lazyui.js`: the character grid, looks like the terminal.
- `mountSkin(div, ...)` in `web/skin.js`: a DOM app - cards, segmented controls, SVG charts, icons, with a choice of
  looks (below). The engine still owns state, focus, keys and events; the skin reads
  `lz_describe` (the active elements with attributes and live state, widgets add their own data through
  `Widget::describe`) and sends semantic actions back with `lz_act` (`focus select activate text toggle sort reset`).

`web/index.html` takes `?skin=grid`, `?look=`, `?scheme=`, `?motion=`, `?dock=0`, `?spec=`, `?set=key=value`; the chip row in
the corner flips looks live. `examples/showcase.lui` has every
widget. `node web/browser-test.mjs` drives the skin in headless Chromium (no npm packages).

### Looks and motion

A look is a block of design tokens in `web/looks.js` (radius, font, palette, shadow, blur), a spring personality and a
list of effects; the skin's structure CSS reads only tokens, so a new look is a few variables. Seven ship:
`terminal` (green phosphor CRT, scanlines, blocky bars), `classic` (1995 bevels and title bars), `flat`, `modern`,
`glass` (frosted, 3D tilt), `neon` (glowing edges), `aurora` (animated sky, film grain). `mountSkin(div, {look: "glass",
scheme: "auto"})`, `ui.look("neon")` at runtime; `scheme` applies to the looks that have both (modern, flat, glass).
Old `flavor: "mac"|"win"` still works as `modern`/`flat`.

Everything that moves is a spring (`web/spring.js`, no dependencies, one rAF loop that sleeps at rest): bars and gauges
chase their value and overshoot or not depending on the preset (`stiff snappy gentle bouncy wobbly`), counters count,
line charts morph point by point, panels enter in a staggered wave, the active segment and the selected row are one
pill that slides, buttons squash on press, panels lean toward the cursor and a light follows it. `prefers-reduced-motion`
snaps everything. Effects are flags on the root (`data-fx`: `spot tilt thumb hl blobs grain scan glow`) and can be
overridden: `mountSkin(div, {look: "glass", fx: ["spot"]})`. `node web/spring.test.mjs` tests the maths; `node
web/shot.mjs out.png 'look=aurora&spec=showcase.lui'` screenshots a look in real time.

### sysbar

One line of vitals: `sysbar items=cpu,mem,swap,disk,net,ping,fps every=1 disk=/ ping=1.1.1.1:53`. The widget keeps its
numbers in the model (`sys.cpu sys.mem sys.swap sys.disk` percent, `sys.rx sys.tx` bytes/s, `sys.ping` ms, `sys.fps`), so
the same keys can be read by the program or fed from outside. Natively it samples `/proc` and `statvfs` once a second and
pings with a TCP connect on a background thread (`ping=off` to disable); `fps` is frames drawn per second. In the browser
there is no `/proc`: the skin measures fps (animation frames) and the round trip to the page's origin, and anything else
comes from `ui.feed({cpu: 12, mem: 40})`.

### worldmap, globe, minimap

Ported from lazytui. The planet is shaded per pixel in `world.rs` (a 0.5 degree raster of 177 countries, generated from
lazytui's `worlddata.h` by `tools/gen_world` into `worlddata.rs`, do not edit): soft coasts, climate / political / heat /
grey colouring, borders, graticule, the night side from the sun's position, and on the globe sun, specular, rim and
atmosphere glow. `worldmap`: wheel or `+ -` zoom (to 64x), drag pans, click picks a country (`bind=KEY` gets its ISO
code, event `pick`), `0` resets; `mode=climate|political|grey borders=0 night=1 grid=1 labels=1 lang=pl|en select=ISO
heat="PL:38,DE:83"` (or `heatbind=KEY lo hi ramp=c1,c2,c3`) `sun=DEG|auto`. `globe` turns with arrows or drag, tilt up to
85 degrees, same attributes. `sun=auto` reads `sys.utc` (seconds; the browser skin feeds it, natively the clock is used).
`minimap` is a page as a density strip with the viewport marked: `values offset page marks seek=KEY`, click or drag seeks.

The terminal draws the pixels as half blocks (two pixels per cell, ASCII fallback is a background fill); the browser
asks the engine for RGBA (`lz_pixels`, `Widget::pixels`) and puts it on a `<canvas>`, with the country names as DOM text
on top so they stay sharp.

### radar, rain, scramble, tagcloud, webchart, piechart

The fancy ones from lazytui, all pure functions of attributes and the Ui clock (`src/fancy.rs`). `radar blips="bearing:range:label;..."
speed=deg/s` sweeps and lights blips that fade over a rotation; `rain seed speed` is matrix rain; `scramble text progress=0..1|auto`
resolves text out of random glyphs; `tagcloud words weights`; `webchart values labels max rings`; `piechart values labels colors
donut=1 legend=0`. In the terminal round shapes are braille wires and half-block fills; in the browser they are SVG/CSS (radar
sweep and blip fade are CSS animations with delays computed from the bearing) and, for the two that never rest, `web/fancy.js`
runs one frame loop that sleeps when no rain or scramble is on the page.

### volumebar, pacmanbar, heatdither, corebars, ratepanel

More lazytui bars (`src/bars.rs`). `volumebar label bind=KEY value max=1 step=0.05 vertical=1 muted=KEY` is interactive: arrows,
PgUp/PgDn, Home/End, `m` mutes, click/drag/wheel on the bar, click the percent to mute; it emits `change` and `mute` and writes
`bind`/`muted`. In the browser it is a styled range slider with a mute button. `pacmanbar name done total rate eta` is pacman's own
`-Sy` line with the same arithmetic (the mouth chomps as the hash advances; a chomping Pac-Man eats the dots on the web).
`heatdither` is the heat grid as ordered-dither braille. `corebars labels values infos` and `ratepanel labels values peaks` are
monitor rows with a load bar (green/yellow/red by percent).
