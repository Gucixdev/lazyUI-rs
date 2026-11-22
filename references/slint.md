# slint - what matters

Declarative UI toolkit (Rust/C++/JS/Python). Source: `slint-master.zip`. Read: README "Architecture", `docs/`, `internal/compiler`,
`tools/viewer`, `tools/lsp`.

## Game changers

1. **UI is a separate declarative language** (`.slint`) compiled ahead of time; expressions are pure functions so the compiler can
   inline properties and delete constant ones. Business logic stays in the host language: UI designers and programmers work in
   parallel. Same idea as lazytui's spec text, with a real grammar and a compiler.
2. **Properties with bindings**: `width: parent.width * 0.5;` re-evaluates only when its dependencies change (dependency tracking at
   runtime). `callback`s go out to the host, `property <T>` comes in. This is the model/event boundary: host sets properties, UI emits
   callbacks.
3. **One component model, many host languages**: compile `.slint` to Rust, C++ header, or interpret it dynamically (JS, Python). One
   source, different API surfaces; there is an interpreter for loading at run time.
4. **Live preview + `slint-viewer --auto-reload`**: edit the file, the window updates. LSP gives completion + preview in editors.
5. **Memory layout**: components, items and properties in one memory region -> few allocations; renderers are build-time
   pluggable (software renderer needs no GPU - runs on microcontrollers).
6. **Stable 1.x API policy**, REUSE-compliant licensing (a lesson in how to publish).

## What we take

| slint idea | in lazyui |
|---|---|
| declarative file separate from code | `.lui` spec text (lazytui spec dialect); a program or the `lazyui` binary loads it at run time (interpreter-style) |
| model keys = properties, events = callbacks | `App::set/get(key)`, `App::on(id, kind, f)`; the binary prints events as JSON lines |
| live reload | `lazyui run spec.lui --watch`: re-parse on mtime change, keep model values, show the parse error in a bar with the line number |
| software renderer, no GPU | our only renderer *is* the cell canvas |
| viewer as a tool | `lazyui snap spec.lui 80 24` renders one frame as text, no tty |

## What we leave

- A real expression language and compile-to-Rust step (v1: attributes are literals or `bind=key`; arithmetic of keys later).
- Figma plugin, LSP, WASM playground, the Qt style, licensing apparatus.
