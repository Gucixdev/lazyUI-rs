# lazyui - design (vibemix)

Rust sibling of the C lazytui. One crate, **embeddable** as a library and shipped as **one binary** (`lazyui`) that any program or
shell script can call. Notes per reference: `references/*.md`.

## The mix

| liked feature | from | how it looks here |
|---|---|---|
| cell canvas + diff, inline viewport | ratatui, lazytui | `canvas`, `Viewport::{Full, Inline(n)}` |
| constraint layout that never fails | ratatui, lazytui | `layout`: Fixed / Pct / Fill(w) / Min / Max, edges rounded |
| state / message / update / view, Task, Subscription, replay | iced | `app::Program`, `Task`, `Sub`, message log |
| headless first: snap, tree JSON, scripts on a virtual clock | rune | `headless`; `lazyui snap/tree/script` |
| springs that retarget with velocity, idle = zero cost | rune, lazytui | `anim::Spring`, loop blocks in `poll` |
| tokens as data, runtime theme switch | rune | `theme` loaded from text; slots |
| state machines with returned effects, `Latest` | rune | `machine`, `task::Latest` |
| UI as data + live reload | slint, lazytui | `.lui` spec (lazytui dialect), `--watch` |
| query by label in tests | egui, iced | `headless::Query` |
| widget returns a response | egui, lazytui | `Res::{Ignored, Handled, Changed, Activate}` |
| no async runtime, `poll` + waker | tokio (as a contrast) | `std::thread` + `mpsc` + self-pipe |

One model underneath: **Elm (iced) over a retained keyed tree (rune) painted into a cell canvas (ratatui/lazytui)**; the spec is just
another way to build the same tree, with string events instead of typed messages.

## Layers (each usable alone)

```
canvas   cells, style, colour quantising, diff -> ANSI           no I/O
layout   rect splitting by constraints                          no I/O
anim     Spring (RK4), Tween                                    no clock
theme    named colour slots, presets, text format
widget   draw + event for: text, panel, progress, list, input, buttons, keybar, tabs
el       the element tree (kind, id, children, attrs) + diff of per-id state
spec     .lui text -> El tree + model + bindings (lazytui dialect)
app      Program trait, Task, Sub, Runtime (loop), Clock
term     tty: raw mode, caps, input decoding, resize, waker      unix only
headless Surface without a tty: snapshot text, tree JSON, scripts, ManualClock
cli      the `lazyui` binary                                    feature "cli" (default)
```

Rule: a layer above never leaks into one below; `canvas` and `layout` compile and test without `term`.

## Two ways to use it

**Embedded**

```rust
use lazyui::prelude::*;
struct Counter(i32);
enum Msg { Inc, Dec }
impl Program for Counter {
    type Msg = Msg;
    fn update(&mut self, m: Msg) -> Task<Msg> { match m { Msg::Inc => self.0 += 1, Msg::Dec => self.0 -= 1 } Task::none() }
    fn view(&self) -> El<Msg> { col([text(format!("{}", self.0)), buttons(["+", "-"]).on_activate(|i| if i == 0 { Msg::Inc } else { Msg::Dec })]) }
}
fn main() { lazyui::run(Counter(0)).unwrap(); }
```

or with a spec: `App::from_spec(text)?; app.set("progress", 0.4); app.on("q", "change", |ev| ..); app.run()`.

**Standalone binary** (`lazyui`), for scripts and for programs that are not Rust:

```
lazyui run  ui.lui [--set key=value]... [--watch]   interactive; every event -> one JSON line on stdout; model updates read as JSON lines on stdin
lazyui snap ui.lui W H [--set k=v] [--theme name]   one frame as text, no tty
lazyui tree ui.lui W H                              element tree as JSON (ids, rects, text, focus)
lazyui script ui.lui test.lscript                   headless run: keys/clicks/typing, `expect` lines; exit 0 = all held
lazyui select [--multi] [--prompt P] < lines        inline picker; the chosen line(s) on stdout (the lt_select of the roadmap)
```

Inline mode is the default for `select`; `run` takes `--inline N` (rows) or the alternate screen.

## Decisions taken without asking (defaults; changeable, see claude-config/questions.md)

1. Crate and binary name `lazyui`; repo dir stays `lazyUI-rs`. MIT.
2. Only dependency: `libc` (termios, poll, ioctl). No async runtime, no serde (a 150-line JSON writer is enough; reading model updates
   uses a tiny flat-object reader).
3. Look: lazytui's green-on-dark (`theme::green`), 16-colour/ASCII fallback on `TERM=linux`, matrix theme preset.
4. Keys: arrows + Enter + Tab/Shift-Tab + mouse (SGR) + wheel; `ctrl-c` quits; every widget shows its keys in the keybar.
5. Scope cut if it grows: no sub-cell shaders, maps, node editor in the first release (they stay in C lazytui).
6. First slice: canvas, layout, anim, theme, spec, widgets {text, panel, progress, list, input, buttons, keybar}, headless, `snap`,
   `tree`, `script`, `run`, `select`.

## Honest gaps

Written down so nobody has to discover them: no Windows console, no IME, no kitty graphics, wide/emoji clusters follow `unicode-width`
rules only as far as a small built-in table goes (CJK + common emoji), no accessibility tree beyond `tree`.
Also not in v1 (decided while building, 2026-11-22):

- **No kitty keyboard protocol.** Keys are decoded from classic CSI/SS3 sequences, so ctrl-shift combos and key-release do not exist yet.
- **`run` stdin protocol is `key=value` lines** (plus `list:ID=a,b,c` and `quit`), not JSON; JSON is only on stdout. A richer protocol waits for a real use.
- **Spec subset.** `row col panel pages spacer text progressbar searchbar buttonbar list keybar`. The lazytui ringbar/zonebar/pacmanbar,
  dropdown, tree and scroll containers are rejected by name with a clear message, not silently ignored.
- **Wake pipe fds are not closed** when the terminal is restored (a late effect thread must not write to a reused fd). Two fds leak per `run` call; irrelevant for a binary, worth knowing for a host that runs the loop many times.
- **Hotkeys vs typing:** plain characters never fire a hotkey while a searchbar has focus; F-keys and anything with ctrl/alt do.
- **One painter, not a widget trait.** Widgets are concrete types the `ui` module knows; a user-defined widget has to go through
  `Program::view` (elements) rather than drawing into the canvas itself. A `Custom` element that gets `(canvas, rect)` is the likely next step.

## Damage (2026-11-24)

A frame pays only for what changed. `canvas::damage(prev, cur)` is the box of cells that differ (24 us on 240x70, whole canvas
when there is no previous frame or the size changed, `None` when identical). The window paints only that box
(`Raster::paint_in`, wide glyphs included), turns it into pixels (`px_rect`, one cell of margin) and hands it to the presenter:
the CPU path copies only those pixels into the softbuffer and reports them with `present_with_damage`, using the buffer `age` and
the damage of the last four frames so a double-buffered surface stays right; the GPU path uploads only that sub-rectangle of the
texture (the first upload after a resize is whole). An identical frame paints and copies nothing. The terminal `diff` skips rows
that did not change in one comparison (122 -> 37 us on 240x70, one cell changed). Damage is derived from the two canvases, so no
widget can forget to report it; the cost is one cheap row-wise compare.

## GUI backend (2026-11-22)

`src/gui/`, next to the terminal backend: `raster` (cells to pixels with fontdue; box lines and blocks are drawn
procedurally so they join; only changed cells repaint), `present` (CPU shared memory through softbuffer, or wgpu),
`tune` (pure policy, unit-tested), `window` (winit loop, input mapping, `drive_gui`), `png` (own writer, stored
deflate). The `Ui` is unchanged: a backend is "show the grid" plus "turn input into `Event`s".

Self-tuning: CPU first, because a terminal-style frame touches a few cells and a copy beats waking a GPU. After 20
frames averaging over 6 ms it tries the GPU; after 30 frames it keeps it only if frames are at least 20% cheaper,
otherwise it goes back and never retries. A GPU that stays under 1 ms for 240 frames goes back to the CPU. Animation
frame time follows the measured cost (16 ms to 100 ms).

Honest gaps: the GPU path is compiled and falls back correctly but has **never presented a frame** here (Vulkan on this
Wayland session cannot present, `vkcube` crashes too), so its speed-up is unmeasured and the tuner thresholds are
guesses. Today the CPU still rasterises glyphs on both paths; the GPU only presents. Real gain needs phase 2
(glyph atlas on the GPU, pixel-native widgets). Keyboard and mouse mapping is written but was only exercised without
real input events. Ime commit is delivered as `Paste`. kmscon/VT console support are still open.

## Web backend (2026-11-22)

`src/web/`, feature `web`, compiled only for the page: `Web` is plain safe Rust around `Ui` (events in, a flat `u32`
cell array out: char, fg, bg, mods; colour words 0 default / 0x01000000|idx / 0x02000000|rgb), so it is unit-tested
natively; `ffi.rs` (wasm only) is a C-ABI shell with `lz_alloc/free`, `lz_init/reload`, `lz_key/text/mouse/paste`,
`lz_tick/render/wake_ms`, `lz_events/error`. No wasm-bindgen: nothing to version. `web/lazyui.js` paints and maps DOM
events; `lz_wake_ms` tells it to sleep (-1), use rAF (animating) or one timeout (next `every`). Long gaps are ticked in
50 ms steps (capped at 5 s) so a hidden tab does not lose timers. `term`, `runtime`, `session` and `libc` are
`cfg(not(wasm32))`.

Verified: 9 native tests of `Web`, `node web/test.mjs` on the built `.wasm`, and headless Chromium over CDP (render,
typing + filter, click, `set`, screenshot). Not verified: Firefox/Safari, touch and IME (no hidden text input yet, so
phones cannot type), high-DPI beyond a scale factor of 1, wide (CJK) glyphs are drawn one cell wide.

## Shared widgets (2026-11-22)

`src/components.rs`: text expansion before parsing, so the parser and every backend are unchanged and errors keep their
place (`meter line 3 (used at line 7): ...`). One format for three scopes (builtin / global / local) and for the page.
Not a code plugin system: a widget that needs logic (charts, pickers) is native Rust, registered in the crate; a widget
that is a composition of existing elements is a file anyone can share. Online sharing is just `widgets add URL` (curl,
256 KB limit, never overwrites); there is no index or registry server. Gaps: no versioning or checksums of downloaded
widgets (they are data, but a spec can still show text you did not expect), no `$` escape, values with a double quote
are refused, a component cannot take attributes for inner elements other than through parameters.

## Native widgets (2026-11-22)

`native.rs`: `trait Widget {draw, event, tick, animating}`, a registry of `Def{name, doc, focus, rows, live, make}` and
`Kind::Custom(&'static str)`; the Ui keeps one `Box<dyn Widget>` per element id (state survives reloads like the list
cursor does). A widget reads through `Cx` (attributes, model, theme, clock) and writes through `EvCx` (events, bound
keys), so it cannot reach the Ui. `live` widgets (spinner) keep the loop drawing without counting as unsettled: the
headless snapshot waits for `settling()`, the loops use `animating()`. The registry is a static table, not a plugin
loader: a widget that is not in the crate is a data widget (a file) instead. Gaps: barchart/histogram have no axis frame
yet, zonebar has no bold ink, colours of `graph` follow one accent shade instead of the theme gradient.

## Web skin direction (2026-11-22, not built yet)
The web backend currently paints the cell grid on a canvas. By request the browser should look like a web/OS app (Apple/Windows
feel, icons, light and dark), not like a terminal. So: a DOM renderer fed by `tree_json` + model, the same events back into
`Ui`. Widgets get `describe()` (semantic data: series, rows, tree nodes) so charts become SVG and lists/tables real HTML.
The grid canvas stays as the TUI/GUI-window path and as a fallback (`mount({skin:"grid"})`).

Batch two added `overlay` (drawn after everything, so a dropdown popup lies over its neighbours) and `extent` (the area
that takes the mouse, larger while a popup is open), and `Def.live` became a period: a spinner (0.1 s) keeps the loop
drawing, a clock (1 s) is only a deadline for `next_deadline`. A timer widget ticks on the Ui clock and emits `done`.
