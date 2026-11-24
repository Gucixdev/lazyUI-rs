# lazyui for agents

Build a UI in Rust in ~15 lines, check it headless, reuse pieces. Never open a terminal to verify: use `test(app, w, h)`.

## Quickstart

```rust
use lazyui::prelude::*;                                  // everything below comes from here

struct Counter(i64);
impl App for Counter {
    fn view(&self) -> El {                                // pure: state in, tree out, re-run after every event
        col([text(format!("n = {}", self.0)).accent(), buttons("b", ["+", "-"]), keybar("ctrl-q:quit")])
    }
    fn on(&mut self, e: &UiEvent, _: &mut Cx) {           // every named event: (id, kind, text, index)
        if e.activated("b") { self.0 += if e.index == 0 { 1 } else { -1 } }
    }
}
fn main() -> std::io::Result<()> { run(Counter(0)) }      // ctrl-q always quits
```

Run the examples: `cargo run --example counter_app | form | files | dashboard | counter_i18n` (a terminal is needed; for a snapshot see below).

## Cookbook (each is an example in `examples/`)

1. **State + view + events**: `examples/counter_app.rs`. Match events with `e.activated("id")` / `e.is("id", "change")`; use `e.index` for which button or row, not the label.
2. **Form**: `examples/form.rs`. `input("id", "prompt: ")` sends `("id","change",text)` while typing and `activate` on Enter; keep the text in your state.
3. **List + detail**: `examples/files.rs`. `list("id", rows)` sends `change` with `index` when the cursor moves; the detail is a function of the selected index.
4. **Reusable component**: `examples/form.rs` (`Field`). A struct with `impl Component` (or a function returning `El`). Inside, name ids with a leading dot (`".v"`), place it with `Field(..).at("email")`; events come back as `email.v` (`e.local("email") == Some("v")`).
5. **Background work**: `Program` + `cx.spawn(|| Msg)` in `examples/counter.rs`; with `App`, `cx.spawn(|| UiEvent::of("loaded","done","42"))` and handle it in `on`. Use `Latest` to drop stale answers.
6. **Timers, tabs, sysbar**: `examples/dashboard.rs`. `ui.every(1.0, "tick")` in `App::setup` emits `("app","tick")`; `tabs` + `pages`; `widget("sysbar")` for any native widget by name (`lazyui widgets ls`). Translation: `examples/counter_i18n.rs`.

Chain words: `.on("id")` `.bind(key)` `.dim() .accent() .ok() .warn() .err()` `.center() .right()` `.fixed(n) .fill() .weight(n) .pct(n)` `.title(..)` `.items([..])` `.scope("p")`. Anything else: `.set("attr", value)`.

## Verify without a terminal

```rust
let mut h = lazyui::prelude::test(Counter(0), 30, 6);     // Harness: same loop, virtual screen and clock
h.press("+");                                             // click by label/row text, or "#id"
h.type_into("who", "ann");                                // input by "#id", prompt text, or "" = focused
h.key("tab");                                             // key by name
h.expect_text("n = 1").unwrap();                          // Err carries the whole frame
println!("{}", h.frame());                                // plain-text snapshot (ASCII glyphs, stable everywhere)
println!("{}", h.tree());                                  // element tree as one line of JSON: id, kind, rect, text, value, rows, focused
h.find("~count"); h.log;                                  // elements by #id / ~text / kind; every event that fired
h.tick(1.0); h.wait_msg();                                // advance the virtual clock; wait for a spawned effect
```

From the shell, for a `.lui` spec: `lazyui snap ui.lui --size 60x10`, `lazyui tree ui.lui`, `lazyui script ui.lui test.script`.
Over HTTP: `lazyui serve ui.lui` prints a token and listens on 127.0.0.1:7878; `curl -H "X-Token: $T" localhost:7878/tree`, `curl -H "X-Token: $T" -d $'focus q\ntype hi' localhost:7878/do` (a script, one command per line). One request at a time, a wrong token or a non-localhost `Host` is refused.
Always run `cargo test -j2` with `nice` on a busy machine; the default features pull in wgpu (slow to build), use `--no-default-features` while iterating.

## Gotchas

- **Sizes go along the parent**: in a `col` `.fixed(3)` is 3 rows, in a `row` 3 columns.
- **Do not match on translated text.** Match `e.id` and `e.index`. A string starting with `@` followed by a key is translated when a catalog is set; write `@@` for a literal `@`.
- `buttons`/`list` labels are comma-joined: a label with a comma splits in two.
- `press`, `type_into` and `find` need a drawn frame; the Harness draws first, a bare `Ui` needs `headless::snapshot` before `act`/`press`.
- Model keys (`bind`) are shared by name; ids and keys inside a `Component` start with `.` so `scope` can separate two copies.
- Additive API only: never rename or remove widgets, attributes, actions, events or the `lz_*` exports; the owner runs lazyui in a real program.
- Specs (`.lui`) still work and build the same tree; prefer the Rust builder unless the UI must be edited live (`lazyui run ui.lui --watch`).
- References for the design: `references/*.md`. `webui` ([`references/webui.md`](references/webui.md)) = the browser you already have as the window (`webui-dev/webui`); `gpuui` ([`references/gpuui.md`](references/gpuui.md)) = GPU-rendered UI frameworks (GPUI, rune, Slint, iced+wgpu, Vello); rune is also the model for the agent loop. Overall design: `DESIGN.md`.
