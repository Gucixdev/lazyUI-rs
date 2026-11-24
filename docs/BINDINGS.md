# Calling lazyui from other languages

lazyui has four doors, pick the lightest that fits.

| door | for | what you get |
|---|---|---|
| the `lazyui` binary | any language that can start a process | spec file in, one JSON line per event out, `KEY=VALUE` lines in to change the model. `lazyui snap / tree / script` for headless checks |
| `lazyui serve` | agents, scripts, anything with HTTP | the same UI on `127.0.0.1` with a token: `GET /tree /describe /snap /events`, `POST /do` (a script: key, type, click, act, set, tick, expect) |
| the C ABI (`include/lazyui.h`) | Python, Go, C, C++, Zig, Nim, Lua, anything with an FFI | the whole engine in a shared library, you feed keys and read a frame or text |
| WebAssembly (`web/lazyui.js`) | JS in a browser or Node | the same C ABI compiled to wasm, painted on a canvas or as DOM |

Rust programs use the crate directly (`use lazyui::prelude::*;`, see `AGENTS.md`); the doors below are for everyone else.

## 1. The binary (works today, no build step beyond `cargo build --release`)

```
lazyui run ui.lui --catalog ui.tsv --locale pl     # events on stdout, model updates on stdin
lazyui snap ui.lui --size 60x10 --locale pl        # one frame as text
lazyui tree ui.lui                                 # the element tree as JSON
```

```python
import json, subprocess
p = subprocess.Popen(["lazyui", "run", "ui.lui"], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
p.stdin.write("cpu=64\n"); p.stdin.flush()            # model value
for line in p.stdout:                                  # {"id":"q","kind":"change","text":"ab",...}
    ev = json.loads(line)
```

Same idea in Go (`exec.Command` + `bufio.Scanner`), Node (`child_process.spawn`), shell (`while read ev`). No library to install.

## 2. The C ABI

```
cargo build --release --no-default-features --features web      # target/release/liblazyui.so
```

Functions are declared in `include/lazyui.h`. The contract is small:

1. `lz_init(spec)` once; `lz_resize(cols, rows)`.
2. Feed input: `lz_key("enter")`, `lz_text("abc")`, `lz_mouse(...)`, `lz_act(id, "activate", "0")`, `lz_set(key, value)`.
3. Call `lz_tick(ms)` with the elapsed time, `lz_render()`, then read the frame: `lz_snapshot()` + `lz_out_ptr()` for plain text,
   or `lz_frame_ptr()/lz_frame_len()` for cells (char, fg, bg, mods as u32 words), `lz_events()` for what the user did.
4. Strings are `(pointer, length)` UTF-8. Text results come back through one out buffer: read `lz_out_ptr()` before the next call.
5. Single thread, one UI per library instance.

Python (ctypes), about 60 lines, tested: `bindings/python/lazyui.py`.

```python
from lazyui import Lazyui
ui = Lazyui("libs/liblazyui.so", "col\n  text text=hello\n  buttonbar id=b items=\"go\"\n", 20, 4)
ui.act("b", "activate", "0")
print(ui.snapshot()); print(ui.events())
```

Other languages, same five steps:

- **C / C++**: `#include "lazyui.h"`, link `-llazyui`; see the header for the exact types.
- **Go**: `cgo` with `#include "lazyui.h"` and `#cgo LDFLAGS: -llazyui`, or `purego` to call the shared library without cgo.
- **Zig / Nim / V / Odin**: import the header (`@cImport`, `c2nim`) or declare the 30 functions by hand.
- **Lua / Julia / Ruby / Java (Panama)**: their FFI loads the shared library the same way ctypes does.
- **JS**: `web/lazyui.js` is the reference binding (`engine()` is the thin part, `mount()` paints a canvas). Node can load the `.wasm` directly: `web/test.mjs` does.

Not written: bindings for any language other than Python. They would be copies of the ~60 lines above.

## 3. Translation and many languages

UI text is translated by `Catalog` (`src/i18n.rs`): a TSV file, `key<TAB>en<TAB>pl...`, with `{name}` placeholders and plural forms
(`files#one`, `files#few`...). In a spec write `text="@hello"`; from any door load the catalog and switch the language at run time:
`--catalog/--locale` on the binary, `lz_catalog` / `lz_locale` in the C ABI, `ui.set_catalog(..)` / `ui.set_locale("pl")` in Rust.
Lookup falls back `pl-PL -> pl -> en -> the key itself`. Match events on ids and `index`, never on translated labels.
