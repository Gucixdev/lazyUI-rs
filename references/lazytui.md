# lazytui (C) - what matters

The author's C library (`Gucixdev/lazytui`). Read: `DESIGN.md`, `README.md`, `include/lazytui.h`.
This is the *base* of the mix: lazyui is the Rust sibling, and the spec language is shared.

## Game changers

1. **`lt_app`: the whole UI is data.** An indented text spec (`col`, `panel title=..`, `progressbar bind=progress`, `searchbar id=q`)
   + a key/value *model* + named events `(id, kind)`. "Nothing in a spec is code." Errors carry the line number. `lt_app_render` and
   `lt_app_dispatch` run with no terminal, so every UI is testable as text. This is already what rune (`--tree`) and slint (`.slint`)
   are reaching for; it came first.
2. **Canvas of cells + diff**, colour quantised to the terminal (truecolour -> 256 -> 16 -> 8), ASCII fallback, works on the Linux
   console (`TERM=linux`: no alternate screen, 16 colours). Wide glyphs handled.
3. **Cassowary layout** with strengths: required constraints always hold, a minimum width is a strong *wish*, so tiny terminals degrade
   instead of failing. Both edges are rounded, never the size, so neighbours meet exactly at every width.
4. **Springs (RK4) for animation**: retarget keeps velocity, stalled dt clamped, snaps to rest below 1e-3 so an idle UI stops drawing
   (zero CPU when idle: blocks in `poll`).
5. **Sub-cell graphics**: shapes and shader fit each cell with a block glyph + two colours (least squares), braille wireframes,
   globe/world map with country picking.
6. **~60 widgets** with one contract (`state + draw + event`, result IGNORED/HANDLED/CHANGED/ACTIVATE): list/tree/dropdown/table,
   pickers (colour, date, file, gradient), sequencer, node editor, pixel editor, monitor widgets (histgraph, segbar, proctable).
7. **Keybar**: fzf-style key names, modifier caps (Ctrl blue, Alt yellow, Shift green) filtered by the modifiers currently held (kitty
   keyboard protocol), mouse-clickable. **pacmanbar** matches `pacman -Sy` character for character (golden tests).
8. **Five search modes** in one place: filter, fuzzy, `tag:`/`#tag`, regex, MCTS ranking; `lt_search_auto` picks by prefix.
9. **Robustness culture**: ASAN/UBSAN, 200k random events, every widget at every canvas size 1..70 x 1..12, NaN/inf, nonsense rects.

## What lazyui takes

Everything above that is not C-specific, in this order: (1) cell canvas + diff + caps, (2) layout, (3) springs, (4) the spec language
and event model, (5) keybar + a small first widget set, (6) fuzz/size-sweep tests. Sub-cell shaders, maps and the big widget set stay in
C for now; the Rust crate must let a widget be added without touching core (`Widget` trait + `custom` spec kind).

**Spec compatibility:** a lazyui spec is a subset of the lazytui spec (same kinds and attributes: `row col stack panel scroll pages spacer
text textbar progressbar searchbar buttonbar list keybar custom`; `id size gap pad title name bind text prompt items active max hints`;
size = `N | fill | fill:W | pct:P | min:N | max:N`; events `change activate toggle click key tick`). A spec written for one should
load in the other.
