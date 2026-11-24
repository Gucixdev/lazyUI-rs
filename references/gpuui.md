# gpuui - GPU-rendered UI frameworks, what matters

One file for the family: how UIs are drawn on the GPU and what a cell-grid engine can learn from each. Sections: GPUI (Zed), rune,
Slint's renderers, iced + wgpu, Vello. Sources were read on 2026-11-24 from upstream READMEs and files (not from memory); what was
not read is marked "not checked". Nothing here was built or run.

## 1. GPUI (Zed)

Zed's UI framework, crate `gpui` 0.2.2 (`crates/gpui/Cargo.toml`, `license = "Apache-2.0"`; the file is `LICENSE-APACHE` in the zed
repo). README: "a hybrid immediate and retained mode, GPU accelerated, UI framework for Rust", pre-1.0 with "breaking changes between
versions". Read: `crates/gpui/README.md`, `src/_ownership_and_data_flow.rs`, `examples/hello_world.rs`, `examples/testing.rs`,
`docs/contexts.md`, the file list of `src/`. Not checked: the render pipeline (`scene.rs`, `platform/`), the keymap and focus code in depth,
the behaviour of any example (nothing was built or run), the inspector and hot reload beyond their file headers.

### Game changers

1. **Three registers** (README "The Big Picture"): `Entity` for state, views for declarative UI, `Element` for low-level imperative
   control (big lists, custom layout). You drop down a level only where you must.
2. **State is owned by one `App`; you hold handles.** `cx.new(|_| Counter{..})` returns `Entity<Counter>`, "an inert identifier plus a
   compile-time type tag" with a refcount; access only through `entity.update(cx, |state, cx| ..)`. `cx.notify()` tells observers, which
   marks the view for re-render. Contexts (`cx`) are the one parameter that gives access to everything (`docs/contexts.md`).
3. **A view is an `Entity` that implements `Render`**: `fn render(&mut self, window, cx) -> impl IntoElement`, called on the root
   view each frame it is dirty. Compare lazyui's `Program::view(&self) -> El`.
4. **Tailwind-like styling by method chain** on `div()`: `.flex().flex_col().gap_3().bg(rgb(..)).size(px(500.)).justify_center()
   .items_center().shadow_lg().border_1().rounded_md().text_xl()` then `.child(..)` (from `examples/hello_world.rs`). Layout is flexbox
   (`taffy.rs` is in `src/`). The whole screen reads as one expression.
5. **Actions**: `actions!(counter, [Increment, Decrement])` declares user structs; a keymap turns keystrokes into actions and a focus
   handle routes them to the focused view (`action.rs`, `key_dispatch.rs`, `keymap/`, `examples/tab_stop.rs`). Keys are data, not code.
6. **Testing in the framework**: `#[gpui::test]` with a `TestAppContext` that "simulates common platform input" (README, `examples/testing.rs`:
   run with `--features test-support`). A fake clock/executor makes async tests deterministic (`executor.rs`, `platform_scheduler.rs`).
7. **Async integrated with the platform loop**: `cx.spawn(async move |this, cx| ..)` returns a `Task`; the weak `this` handle updates the
   entity when the work ends (example `testing.rs`). Dropping the task cancels it.
8. **Animation as springs**: `src/spring.rs` has `SpringConfig {stiffness, damping, mass}`, a damped harmonic oscillator, plus `animation.rs`
   example. The same idea as rune and our `anim::Spring`.
9. **Virtualised lists** (`uniform_list` example), `img`/`svg` elements, drag and drop, popovers, an element `inspector` and `hot_reload`
   (`src/inspector.rs`, `src/hot_reload.rs`, behind features/debug builds; what exactly they do: not checked).

### What we take

| gpui idea | in lazyui |
|---|---|
| a screen is one chained expression | the Rust builder (`ui::col([...])`, `text("x").bold().fill()`), chainable style methods, no spec file needed |
| tailwind-ish short style words | short style methods on `El` (`dim`, `bold`, `accent`, `size`, `fill`, `gap`) instead of long attribute strings |
| three registers, drop down only when needed | builder (fast) > spec (data, hot reload) > `trait Widget` / custom element (full control) |
| keys are data (actions + keymap) | `Ui::hotkey(key, action)` already; `Component` can declare its own keys by name |
| test through a context that simulates input | `Harness` (virtual clock) plus `find` by text/key then `click` / `type` |
| notify-then-rerender | handlers return `Msg`, `update` mutates, `view` re-runs: the same dirty-flag loop in Elm form |

### What we leave

- GPU renderer, glyph atlas, text shaping, windowing, macOS/Windows platform layers: pixel-world (see wgpu.md, winit.md).
- `Entity`/handle ownership model: lazyui keeps state in your own struct; a component is a function or a small trait, no `cx` to thread.
- Pure Rust-only styling vocabulary in the thousands (every Tailwind class): we keep a few dozen names that terminals can honour.
- The async executor and its `Task` types: effects run on plain threads (see tokio.md).
- Accessibility tree (`_accessibility.rs`): our `tree_json` is the only structured dump.

### Licence

Apache-2.0 for the `gpui` crate (`Cargo.toml`). Permissive, so adapting with notice is allowed, but it is one big tightly coupled
framework with no code that fits a cell canvas: only techniques are used, no code is copied.

## 2. rune (anomalyco/rune)

What it is: a native GPU Rust UI framework with its own renderer, text, layout and springs, made "to be written and checked by
agents". The full notes (headless `--snap/--tree/--script`, spring tracks, tokens, `machine::step`, `Latest`) are in
[`rune.md`](rune.md); this section only adds the GPU angle.

### Game changers (GPU side)

1. The renderer is part of the framework, not a plug-in, so animation, layout and drawing share one clock and one tree.
2. Headless rendering to PNG (`--snap`) uses the same pipeline as the window, so "what the agent saw" equals "what the user sees".
3. Idle = zero cost: nothing is drawn unless state changed (see rune.md point 8).

### What we take / leave

Take: the agent loop (snapshot + tree JSON + script on a virtual clock), already the core of lazyui's `Harness` and `snap/tree/script`.
Leave: the renderer itself (glyph atlas, shaping, materials): our canvas is a cell grid, the GUI window only rasterises cells.

### Licence

MIT (rune.md, from the zip that was read then). Technique only here, no code.

## 3. Slint's renderers

What it is: declarative UI toolkit; the README ("Architecture" part, lines about renderers) lists three back-ends chosen at build/run
time: `femtovg` "uses OpenGL ES 2.0", `skia` "uses Skia", `software` "uses the CPU with no additional dependencies". More in
[`slint.md`](slint.md). Not checked: how Slint picks a renderer at run time, its wgpu integration, performance numbers.

### Game changers

1. **Renderer is a build-time choice behind one item tree**: the same `.slint` runs on a GPU renderer or on the CPU-only one (down to
   microcontrollers, per slint.md).
2. **Software renderer is first class**, not a fallback hack: UI must work with no GPU at all.

### What we take / leave

Take: the rule that CPU and GPU are two ways to *present* the same retained tree, and the CPU one must always work. lazyui's
`src/gui/tune.rs` already measures and picks CPU or wgpu at run time and never requires the GPU.
Leave: the `.slint` compiler and property system (our spec is plain text), Skia and OpenGL dependencies.

### Licence

README "License": use under any of (1) a Royalty-free license (proprietary apps allowed), (2) GNU GPLv3, (3) a commercial license;
GitHub reports `NOASSERTION`. Treat as copyleft/proprietary mix: **technique only, no code is copied**.

## 4. iced + wgpu

What it is: Elm-architecture GUI (iced.md). README: "Two built-in renderers leveraging `wgpu` and `tiny-skia`", `iced_wgpu` supports
Vulkan, Metal and DX12, and the runtime is "renderer-agnostic". Licence: MIT (GitHub API). wgpu itself in
[`wgpu.md`](wgpu.md). Not checked: the quad/text/triangle layer split inside `iced_wgpu`, the tiny-skia fallback selection logic.

### Game changers

1. **Renderer-agnostic runtime + two renderers**: GPU (wgpu) with a CPU (tiny-skia) fallback chosen at startup; the program does not
   change. The same shape lazyui's GUI backend has (softbuffer CPU, optional wgpu).
2. **State/message/update/view** stays pure regardless of renderer, so tests never need a GPU.

### What we take / leave

Take: the fallback pattern (CPU renderer always available, GPU optional) and keeping all logic out of the renderer.
Leave: iced widgets/layout engine, fonts and text shaping; async runtime.

### Licence

MIT. Copying with notice is allowed; there is nothing small enough to copy, only the pattern is used.

## 5. Vello (linebender)

What it is: README: "High-performance 2D renderers written in Rust", three implementations. `vello_cpu`: CPU-only, "optimized for
multithreading and SIMD", the most mature. `vello_gpu`: preprocesses paths on the CPU, GPU does rasterization and compositing, has a
native WebGL2 backend and a wgpu backend "without requiring compute shaders". `vello` (under `research/`): the original compute-centric
renderer, "experimental", needs compute-capable GPUs. CPU and GPU variants share the "Sparse Strips" architecture (ARCHITECTURE.md:
paths flattened, binned into tiles, kept as sparse horizontal strips so only boundary coverage is stored). Version/MSRV: Rust 1.89+.
Not checked: benchmarks, the Sparse Strips paper (named in ARCHITECTURE.md as "Potato"), text support in `glifo` beyond its mention.

### Game changers

1. **Same scene, three back-ends** (CPU SIMD, GPU raster, GPU compute): the scene description is the stable contract, the rasteriser is
   swappable by hardware.
2. **Sparse strips**: store coverage only where edges are; fully covered spans stay compact. Big win for UI shapes (mostly flat fills).
3. **Compatibility is a feature**: `vello_gpu` avoids compute shaders so it runs on WebGL2 and older GPUs; the compute version is kept as
   research.
4. A shared snapshot test suite across renderers (`vello_tests`): one expected image for every back-end.

### What we take / leave

Take: swappable back-ends behind one scene, and the snapshot-per-backend test idea (our cell grid snapshot is the same for tui, gui
and web). Sparse coverage thinking is relevant if the GUI ever draws pixel-native widgets (charts, maps) instead of cells.
Leave: the whole vector renderer: a cell grid has no paths. Revisit only for phase 2 of the GUI (smooth shapes, glyph atlas).

### Licence

"Apache-2.0 OR MIT" (README badge, GitHub: Apache-2.0). Permissive; technique only, no code is copied.
