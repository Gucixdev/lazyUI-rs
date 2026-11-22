# egui - what matters

Immediate-mode GUI in pure Rust. Source: `egui-main.zip`. Read: README ("Why immediate mode"), `crates/egui_kittest`, `crates/epaint`,
`crates/egui_inspection`.

## Game changers

1. **No callbacks, no widget objects**: `if ui.button("x").clicked() { .. }`. GUI code is a function over current state, so GUI and app
   state cannot drift apart and nothing dangles. The cost: layout needs sizes before it knows them. egui's answer: remember last
   frame's size by `Id`, accept one frame of jitter, offer `request_discard` (a second pass) only when needed.
2. **`Response` as the return value of every widget** (clicked, hovered, changed, rect, id) - the widget *reports*, the caller decides.
3. **IDs for retained state** (scroll, collapsed headers, window position) kept in a context memory map keyed by `Id` hash - the
   one place where immediate mode keeps state.
4. **`egui_kittest`**: UI tests through the AccessKit tree: `harness.get_by_label("Check me!").click(); harness.run();` and snapshot
   tests (image diff). Query by *meaning*, not coordinates. Same idea as iced's selector and rune's `--tree`.
5. **Goals worth copying as rules:** responsive in debug builds, "difficult to make mistakes and shouldn't panic", minimal
   dependencies, no `unsafe`, modular (use small parts), easy to integrate (you feed input, take output, paint the mesh).
6. **epaint**: its own 2D painter (shapes -> mesh) separate from the UI, so integrations only paint triangles.

## What we take

| egui idea | in lazyui |
|---|---|
| widget returns a `Response` | interactive widgets return `Res::{Ignored, Handled, Changed, Activate}` (same as lazytui's `lt_wres`) |
| query by label in tests | `headless::Query::by_text("OK").click()` over the El tree |
| ids keep retained state | `El::id(..)` -> per-id state (scroll, cursor, spring) survives view rebuild |
| "no panic" contract | every widget accepts any rect (zero, negative, outside) and any NaN: fuzz test is part of the suite |
| integration = input in, mesh out | `Surface` trait: `size()`, `present(&Canvas)`, `poll_event(timeout)` - tty and headless are two impls |
| escape hatch for custom drawing | `custom` spec kind / `El::custom(draw, event)` callbacks (immediate-mode inside retained tree) |

## What we leave

- Pure immediate mode as the public API: we use Elm update/view (iced) so effects and replay stay possible.
- Windows, floating panels, mesh painting, text editing engine, AccessKit.
