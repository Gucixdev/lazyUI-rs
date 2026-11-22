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
