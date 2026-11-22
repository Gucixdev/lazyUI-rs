# iced - what matters

Cross-platform GUI in Rust, Elm architecture. Source: `iced-master.zip`. Read for: README (overview), `runtime/src/task.rs`,
`futures/src/subscription.rs`, `core/src/{widget.rs,animation.rs}`, `devtools/src/time_machine.rs`, `tester/`, `selector/`,
`program/`.

## Game changers

1. **State / Message / update / view - and nothing else.** `update(&mut State, Message) -> Task<Message>` is the *only* place state
   changes; `view(&State) -> Element<Message>` is a pure function. Widgets never touch state, they *produce messages*
   (`button("+").on_press(Message::Inc)`). `iced::run(update, view)` is the entire entry point. Because every change is a message,
   everything below falls out for free.
2. **`Task<T>` = effects as values.** `update` returns a description of work, not work: `Task::perform(future, map)`, `batch`,
   `then`/`chain`, `map`, `abortable()` (handle with `abort_on_drop`), `stream`, `done(value)`, `none()`. The runtime runs it and
   feeds the result back as a Message. Widget operations (focus, scroll) are Tasks too.
3. **`Subscription` = long-lived event sources declared from state.** `subscription(&State) -> Subscription<Message>`; the runtime
   diffs the declared set by identity (hash) and starts/stops streams. Timers, key events, sockets are the same mechanism. Declarative
   "while X is true I want ticks" instead of start/stop bookkeeping.
4. **Time travel for free** (`devtools/time_machine.rs`): the debug build records every Message; to rewind to message *n* it boots
   fresh state and replays messages 0..n through `update` with side effects disabled. Possible only because update is the sole
   mutation path and Message is `Clone`.
5. **Headless end-to-end tests** (`tester`, `selector`, `iced_test`): record an interaction ("ice" file of instructions), replay it with
   an `Emulator`, select widgets by text/id/bounds with a `Selector` operating on the *widget tree* (depth-first `select(candidate)`),
   assert on state or a screenshot. UI test = list of instructions + selectors, not coordinates.
6. **`Widget` trait with a tree of per-widget state** (`tag`, `state`, `diff`, `layout(limits)`, `draw`, `operate`, `update`):
   the view is rebuilt each time, `diff` reconciles it against a persistent state tree so scroll position / text cursor survive.
   Layout is constraint `Limits` -> `Node` (min/max box in, concrete size out), `Length::{Fill, FillPortion, Shrink, Fixed}`.
7. **`Animation<T>`** (core): value + easing + duration (`quick()`, `slow()`), sampled with an `Instant`; the view reads
   `animation.interpolate(now)`. Pure data, no timer thread.
8. **Renderer-agnostic runtime.** `core` (types), `runtime` (Task, window actions), `widget`, `program`, then renderers (wgpu,
   tiny-skia) and a winit shell are separate crates. Same program, any renderer.
9. **Extras worth stealing as ideas:** `lazy` (re-run view only when a dependency changed), `responsive` (view depends on size),
   `pane_grid` (tiling with drag), `keyed` columns (stable identity), `markdown`, `table`, `toast`/`modal` via `stack`, `canvas` with
   cached geometry.

## What we take

| iced idea | in lazyui |
|---|---|
| State/Message/update/view | `app::Program` trait: `update(&mut self, Msg) -> Task<Msg>`, `view(&self) -> El<Msg>`; spec apps use the same loop with string events |
| Task | `app::Task<M>`: `none`, `done`, `perform(thread fn)`, `batch`, `after(secs)`; std threads + channel, no async runtime |
| Subscription | `Program::subscriptions()` -> `Sub::{Every(secs, Msg), Keys(fn)}`, diffed by key |
| Time travel / replay | message log + `--replay` in the headless driver (needs `Msg: Clone`) |
| Selector + recorded tests | `headless::Query::{text, id, role}`; scripts as `key`/`click label`/`type` lines |
| Animation value | `anim::Tween` next to `Spring`; springs preferred (see rune), tween for fixed-time fades |
| `lazy`, `responsive` | `El::lazy(key, f)` skips rebuild when key equal; `El::responsive(|size| ..)` |

## What we leave

- async/await and futures (iced pulls a runtime): effects run on plain threads, results come back on one channel.
- GPU renderers, fonts, SVG, images, window shell: not a terminal concern.
- The `Widget` state-tree machinery (`tag`/`diff`): our retained state lives in the keyed element tree (see rune) or in the model.
