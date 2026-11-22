# tokio - what matters

Async runtime. Source: `tokio-master.zip`. Read: `tokio/src/{runtime,sync,time,task}`, `tokio-util`, `tokio-test`.

## Game changers (for a UI library)

1. **Work-stealing scheduler + an I/O driver on the OS event queue + a hashed timer wheel** - one `select`-able world for sockets,
   timers, signals, processes.
2. **Channels as the glue**: `mpsc` (events into the loop), `oneshot` (one answer), `watch` (latest value wins - the exact shape of a
   model value or of a debounced search query), `broadcast`. `select!` over several sources.
3. **`spawn_blocking` and `JoinHandle::abort`**: CPU or blocking work off the loop, cancellable.
4. **`tokio-test` + paused time (`time::pause`, `advance`)**: virtual clock for deterministic tests - same trick as rune's virtual clock.
5. **Cancellation by drop** as the language-level contract.

## Decision

Not a dependency of lazyui. A terminal UI needs one loop that waits for *tty bytes or a deadline*; `poll(2)` with a timeout does that
with zero allocation and no runtime, and the C lazytui already works that way. Effects run on plain `std::thread`s and report through
one `std::sync::mpsc` channel plus a self-pipe `Waker` that wakes `poll`. Taken as ideas:

| tokio idea | in lazyui |
|---|---|
| `watch` (latest wins) | `task::Latest` (also from rune), model keys |
| paused/advanced time | `Clock` trait: `RealClock`, `ManualClock` for headless + tests |
| abort on drop | `Task` handle cancels the pending message when dropped |
| `select` over tty + timer + channel | one `poll()` with fds [tty, waker] and timeout = next deadline |

A program that already runs tokio can embed lazyui by sending messages through the `Waker` from its own tasks (documented, no feature
flag needed).
