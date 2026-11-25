# lazyui guide

Moved out of the README: the binary, the library, testing without a terminal, layers.

## As a binary

Describe the UI in a spec file (`examples/demo.lui`):

```
col
  buttonbar id=tabs items="main,extras" active=tab
  pages bind=tab
    col
      panel title="load" size=3
        progressbar name=cpu bind=cpu max=100
      searchbar id=q prompt="> " bind=query
      list id=files items="alpha,beta,gamma" filter=query
    text text="nothing on this page yet" style=dim
  keybar hints="tab:focus;ctrl-q:quit"
```

```
lazyui run  examples/demo.lui --watch      # on the terminal; edits to the file reload live
lazyui snap examples/demo.lui --size 40x12 # one frame as text
lazyui tree examples/demo.lui              # element tree as JSON: ids, rects, state
lazyui select examples/demo.lui '~gamma'   # find elements: #id, kind, ~text
lazyui script examples/demo.lui examples/demo.script
```

`run` draws on `/dev/tty` and keeps the pipes for data, so it composes in a shell:

- **stdout**: one JSON line per event, `{"id":"q","kind":"change","text":"ab","index":-1,"number":0}`
- **stdin**: `cpu=64` sets a model value, `list:files=a,b,c` replaces the rows of a list, `quit` ends

```
(while sleep 1; do echo "cpu=$(( RANDOM % 100 ))"; done) | lazyui run examples/demo.lui | while read ev; do echo "$ev" >&2; done
```

`--inline N` draws N rows under the prompt instead of taking the screen. `--hotkey f5=refresh` and `--every 2=poll`
turn keys and timers into events. A spec with an error reports `file: line N: message`; with `--watch` a bad edit keeps
the old UI running.

Elements: `row col panel pages spacer text progressbar searchbar buttonbar list keybar`.
Sizes: `size=N | fill | fill:W | pct:P | min:N | max:N | @KEY | @KEY:N` (`@KEY` takes the cells from the model number KEY, `N` until it is set: the pane before a `splitter` uses it).
Hooks: `hook=NAME` on an element sends every event it raises to a running finedesktop as `emit NAME KIND ID TEXT` (a script in `hooks/on-user` or a subscriber sees it); nothing is sent without the attribute or without a desktop. Only the native window and panel loop do it (`gui::panel::run`).

Tooltip: `tip="text"` on any element; it appears next to the pointer after it rests on the element for half a second, and goes when the pointer leaves or a button is pressed.

## As a library

```rust
use lazyui::runtime::{run, Cx, Opts, Program};
use lazyui::ui::{buttons, col, text, El, Ui, UiEvent};

struct App { n: i64 }
enum Msg { Inc }

impl Program for App {
    type Msg = Msg;
    fn update(&mut self, m: Msg, _: &mut Cx<Msg>) { match m { Msg::Inc => self.n += 1 } }
    fn view(&self) -> El { col([text(format!("n = {}", self.n)), buttons("b", &["+"])]) }
    fn event(&mut self, e: &UiEvent) -> Option<Msg> {
        (e.id == "b" && e.kind == "activate").then_some(Msg::Inc)
    }
}

fn main() -> std::io::Result<()> { run(&mut App { n: 0 }, Opts::default()) }
```

State lives in your struct, `view` is pure, effects run on threads (`cx.spawn(|| Msg::Done(..))`) and return as
messages; `Latest` drops answers to superseded requests. `examples/counter.rs` shows an effect.
`Ui::from_spec` gives the same tree from a spec file when you prefer that over code.

## Testing without a terminal

Everything is testable headless, on a virtual clock: `Harness` (embedding), `headless::run_script` (spec), and the
`script` command. A script is a few lines of `key`, `type`, `click`, `tick`, then `expect text/row/focus/model/event`;
a failure names the line. `tests/pty_run.py` drives the real binary in a pseudo-terminal.

## Layers

`canvas` cell grid + diff + colour/ASCII fallback, `layout` fixed/pct/fill/min/max solver, `anim` RK4 springs,
`theme` data themes, `input` byte decoder (utf-8, CSI, SGR mouse, paste), `widgets`, `ui` tree + spec + model + focus,
`headless`, `term` raw mode via libc, `runtime` the loop.
