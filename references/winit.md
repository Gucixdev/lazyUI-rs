# winit - what matters

Window creation and input events. Source: `winit-master.zip` (split per platform: `winit-x11`, `winit-wayland`, `winit-win32`,
`winit-web`, ...; `dpi`). Read: `FEATURES.md` (the scope document), README.

## Game changers

1. **Scope discipline.** Support tiers: *Core* (abstracted everywhere), *Platform* (exposed per platform), *Usability* (optional
   cargo features). "Winit does not draw and does not do menus; it commits to APIs that higher layers use." A model for how to say no.
2. **`ApplicationHandler` event model**: the loop calls you (`resumed`, `window_event`, `about_to_wait`) instead of you pulling events.
   Events are typed (`KeyEvent` with physical vs logical key, `Ime`, `Touch`, scale-factor change).
3. **Physical vs logical keys**: layout-independent key codes beside layout-aware text - same problem lazytui solves with kitty protocol
   flags (text, shifted alternate key, US-layout fallback).
4. **`dpi` as its own crate** (logical/physical units, never mixed by accident).

## Decision

Not used: lazyui draws cells in a terminal, no windows. Taken as ideas:

| winit idea | in lazyui |
|---|---|
| typed event enum with key + mods + text | `input::Event::{Key{code,mods,text}, Mouse{..}, Paste, Resize, Focus}` decoded from the tty (kitty protocol when answered) |
| tiers in the docs | README says what is core, what is optional (`feature = "spec"`, `"cli"`), what is never done |
| split per platform | `term::unix` only now; the `Surface` trait keeps a Windows console or a GUI window possible later |
| logical vs physical units | `Pt` (cells) vs sub-cell units kept distinct in the shader/braille code (later) |

If a GUI surface is ever added, winit + wgpu (see `wgpu.md`) is the first choice, drawing the same `Canvas` as a cell grid with a bitmap font.
