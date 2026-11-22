# ratatui - what matters

Terminal UI in Rust. Source: `ratatui-main.zip` (v0.30, workspace of core / widgets / backends). Read for: ARCHITECTURE.md,
`ratatui-core/src/{buffer,layout,terminal,backend}`.

## Game changers

1. **Cell buffer + diff = the whole rendering model.** A widget never writes escape codes. It writes `Cell`s (symbol, fg, bg,
   modifiers) into a `Buffer`; `Terminal::draw` renders into the *next* buffer, `BufferDiff` yields `(x, y, &Cell)` only for cells
   that differ from the *previous* one, the backend prints that, buffers swap. The diff iterator is zero-allocation and handles wide
   glyphs: when a wide glyph is overwritten its trailing cells are re-emitted, otherwise the terminal shows stale cells.
   -> lazytui C has the same idea (`lt_canvas_diff`). This is the right core, copy the shape, not the code.
2. **`Widget` is one function: `render(self, area: Rect, buf: &mut Buffer)`**; `StatefulWidget` adds `&mut State`. No widget owns
   a terminal, so everything is testable against a bare `Buffer`. Widgets are cheap values rebuilt every frame (immediate-mode
   feel on top of a retained buffer).
3. **`Backend` trait is tiny** (`draw(cells)`, cursor, size, clear, flush) and there is a `TestBackend` that is just a `Buffer`.
   crossterm/termion/termwiz are interchangeable plug-ins, split into their own crates in 0.30. Tests assert on buffer text.
4. **Viewports: Fullscreen, Fixed, Inline(n).** Inline draws N rows under the prompt without the alternate screen;
   `insert_before(height, draw_fn)` prints finished lines *above* the live area into scrollback (progress UIs, REPL-ish tools);
   `set_viewport_height` resizes inline live. This is what makes a TUI usable as one command in a shell pipeline session.
5. **Layout: `Layout::default().direction().constraints([..]).split(area)`** with `Constraint::{Length, Percentage, Ratio, Min,
   Max, Fill(weight)}` and `Flex::{Start, Center, End, SpaceBetween, SpaceAround, Legacy}`; solved by a Cassowary solver
   (kasuari) with an LRU cache of results. `Rect` is the one geometry type with `inner`, `intersection`, `split`, `centered`.
6. **Text model: `Span` -> `Line` -> `Text`** with `Style` (Stylize: `"x".red().bold()`), alignment, wrapping, truncation. Width
   uses unicode-width; cell width of every symbol is cached in the cell.
7. **Widget set worth knowing:** Block (borders+title), Paragraph (wrap+scroll), List(+state), Table(+state), Tabs, Gauge,
   LineGauge, Sparkline, BarChart, Chart (axes, datasets, markers), Canvas (Braille/half-block/dot markers, shapes), Scrollbar,
   Calendar, Clear (popup backdrop).
8. **Stability by layering:** `ratatui-core` (traits, buffer, layout, style) is kept stable for third-party widget crates; widgets
   and backends churn separately.

## What we take

| ratatui idea | in lazyui |
|---|---|
| Buffer of cells + diff | `canvas::Canvas` (cells), `canvas::diff` -> one ANSI string, run-merged cursor moves |
| `render(area, buf)` widgets | `Widget` trait over `&mut Canvas`, stateless where possible |
| TestBackend | `Canvas::to_text()`; the headless driver is a first-class mode, not a test hack |
| Inline viewport + `insert_before` | `Viewport::{Full, Inline(rows)}`; `lazyui select/progress` must work inline |
| Constraint set + Fill weights | `layout::Size::{Fixed, Pct, Fill(w), Min, Max}` |
| Span/Line/Text | `text::{Span, Line}` with Style; ASCII fallback on the Linux console (lazytui rule) |
| Canvas markers (Braille) | later: sparkline/graph widgets use braille sub-cells |

## What we leave

- Cassowary dependency: our layout is a two-pass weighted split (fixed -> min/max clamp -> fill by weight); it sums exactly at
  every width, and the C lazytui keeps the full solver for when that is not enough.
- Multiple backend crates: one tty backend (own termios via `libc`) + the headless one.
- `Flex::Space*` modes and the macro crate: not needed for the first slice.
