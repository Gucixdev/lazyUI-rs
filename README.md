# lazyui

A UI library for people in a hurry and for AI agents: describe a screen in a few lines of Rust, check it without a
terminal, reuse the pieces. The same code runs on four backends, and the engine is also one binary you can call from any
language.

```rust
use lazyui::prelude::*;

struct Counter(i64);
impl App for Counter {
    fn view(&self) -> El {
        col([text(format!("n = {}", self.0)).accent(), buttons("b", ["+", "-"]), keybar("tab:focus  ctrl-q:quit")])
    }
    fn on(&mut self, e: &UiEvent, _: &mut Cx) {
        if e.activated("b") { self.0 += if e.index == 0 { 1 } else { -1 } }
    }
}
fn main() -> std::io::Result<()> { run(Counter(0)) }
```

`cargo run --example counter_app`. Everything comes from `lazyui::prelude`; state lives in your struct, `view` is pure,
events carry an id and an index. More in [`AGENTS.md`](AGENTS.md): a 6-pattern cookbook (form, list and detail, reusable
component, background work, dashboard, translation), and how to check a UI headless:
`test(app, 30, 6)` then `h.press("+")`, `h.type_into("who", "ann")`, `h.expect_text("n = 1")`, `h.frame()`, `h.tree()`.

## Four backends, one tree

| backend | how | needs |
|---|---|---|
| **tui** | `run(app)` or `lazyui run spec.lui` | any terminal; cell grid, diffed, ASCII fallback |
| **cli** | `lazyui snap / tree / select / script`, JSON events on stdout, `KEY=VALUE` on stdin | nothing: for agents, scripts, CI |
| **gui window** | `lazyui run spec.lui --gui` (feature `gui`, default) | a display; CPU first, GPU only when it measures faster |
| **web** | feature `web`: the engine as WebAssembly (`web/run`), canvas or DOM skin | a browser, no server, no npm |

Other languages call the same engine through a C ABI (`include/lazyui.h`, Python binding in `bindings/python`, how-to in
[`docs/BINDINGS.md`](docs/BINDINGS.md)). Texts can be translation keys (`@title`, `tr("title")`) with a TSV catalog,
plurals, `{name}` placeholders and a locale switch while the app runs (`src/i18n.rs`, `examples/counter_i18n.rs`).

## Where `webui` and `gpuui` fit

Two references say where lazyui goes next, both in [`references/`](references/):

- [`webui`](references/webui.md): `webui-dev/webui`, a C library that uses the browser you already have as the window.
  What we take: the web backend as a no-GUI-crates way to show the same UI (a later `lazyui run --browser`), a private
  temp profile, one call to show; what we leave: its own wire protocol and C-first API.
- [`gpuui`](references/gpuui.md): GPU-rendered UI frameworks in general (GPUI from Zed, rune, Slint's renderers, iced
  with wgpu, Vello). What we take: a screen as one chained expression (the builder), the agent loop on a virtual clock
  (the Harness), and CPU as the always-working renderer with the GPU optional; what we leave: a GPU-only design.

lazyui is a mix of ideas: the Elm loop of iced, the retained keyed tree and headless-first testing of rune, the cell
buffer and diff of ratatui, and the spec language, springs and fallbacks of lazytui. What was taken from each, and what
was left out, is in [`references/`](references/) (one file per project) and [`DESIGN.md`](DESIGN.md).

## Try it

`./run` - one key picks the backend for the demo: terminal, window, browser (wasm) or a text snapshot.

## More

- [`docs/GUIDE.md`](docs/GUIDE.md) - the binary, the library, testing without a terminal, layers
- [`docs/WIDGETS.md`](docs/WIDGETS.md) - native and shared widgets, the window, the web build, what is not done yet
- [`docs/WEB-SKINS.md`](docs/WEB-SKINS.md) - the web widgets and their looks
- [`docs/BINDINGS.md`](docs/BINDINGS.md) - C ABI, Python and other languages; [`DESIGN.md`](DESIGN.md) - the honest list of gaps

## Proofs

`lean4/run` checks a Lean 4 model of the layout (the rounding step is proved: neighbours touch, widths never negative,
never wider than the area, every edge within half a cell) and diffs `layout::split` against the exact-rational model on
292617 inputs. `ampl/run` holds the same code to the optimum an AMPL model gives (worst edge error, 492 cases).
Both exit 0 when they pass; `lean4/` has no Mathlib, only elan.

MIT.
