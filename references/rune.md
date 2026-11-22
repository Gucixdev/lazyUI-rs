# rune - what matters

NOT the Rune scripting language. This is `anomalyco/rune`: a native GPU Rust UI framework with its own renderer, text, layout and
springs, built "to be written and checked by agents". Source: `rune-dev.zip` (MIT). Read for: `README.md`, `PLAN.md`,
`crates/rune/README.md` (3000 lines, the real manual), `crates/rune-core/src/{machine,latest,tok,theme}.rs`, `crates/rune-motion`,
`crates/rune-kit` (component library).

## Game changers

1. **Every app runs headless, and that is the primary way to verify it.** Flags `--snap` (render PNG), `--frames`, `--tree` (dump the
   element tree as JSON: rects, roles, labels, state, open popups, dialogs), `--script` (scripted input: `click`, `type`, `key`,
   `paste`, `scroll`, `fling`, `secondary-click`) all on a **virtual clock** so animations are deterministic. An agent (or a CI job)
   can build, drive and inspect an app without a display. Highest-value idea in the whole reference set.
2. **View is `fn view(&State, &mut Cx) -> El`; handlers are queued and run between frames with `&mut State`.** After handlers run the
   view re-runs and a **diff retargets property tracks**. Hover/press never re-run the view (only retarget tracks). Elements carry a
   `.key(..)`: keyed identity makes the diff, animation and layout cache work.
3. **Animation is the default, not a feature.** Every property (position, size, colour, opacity) is an analytic spring track
   `Spring::new(duration, bounce)` that **retargets keeping position and velocity** (no jump when interrupted). Nothing animates on the
   first frame. `Tx::instant()` for drags. Reduced-motion mode snaps travel and keeps a short fade. Layout animates per edge; entering
   and leaving elements animate (presence). Shared-element "flight" between keyed ids. Animation clock has a time scale.
4. **Design tokens as data.** Colours in OKLCH, space/radius/type/motion palettes as tokens; `Theme::from_json(src, dark)`; themes switch
   at runtime with colour springs (text colour leads background so contrast survives the transition). Hot reload of tokens without
   rebuild.
5. **State machines as plain enums** (`rune::machine`): one `enum State` whose variants carry only valid data, one
   `step(&mut seq, &mut state, event) -> Effects<E>` that *returns* effects instead of doing them; timers are events on the virtual
   clock; `Attempt` tokens fence stale answers (an answer for an old attempt falls through). Illegal states are unrepresentable.
6. **`Latest` requests**: background work where the newest request wins, unstarted older ones never run, one wake per answer,
   optional debounce, bounded shared worker pool, cancel on drop. Exactly what a search-as-you-type box needs.
7. **Virtualised, sticky, anchored scrolling**: `.virtualize(estimated_row_height)` for long lists, `.scroll_anchor()` keeps reading
   position, `.stick_bottom` for logs/chat, sticky headers.
8. **Idle = zero cost**: draw nothing unless `poll` says visible state changed; wake only on deadline.
9. **`rune-kit`**: ready component library (controls, containers, overlays, diff view, markdown, syntax, tree, search, scroll bar).
10. **Agent-oriented repo hygiene:** AGENTS.md/CLAUDE.md, `research/` notes behind every claim, "target" numbers marked as targets.

## What we take

| rune idea | in lazyui |
|---|---|
| headless-first | the headless driver is built *before* the tty backend; `lazyui snap/tree/script` are core commands |
| `--tree` JSON | `El` tree dump: id, kind, rect, text, focused, bound key, state (our agents verify UIs by reading it) |
| script on virtual clock | `.lscript` lines (`key tab`, `type hello`, `click id`, `wait 0.3`, `expect id text`), `Clock` trait with a manual impl |
| spring tracks retargeting with velocity | `anim::Spring` (RK4 like lazytui) per animated property; zero redraws when all at rest |
| tokens/theme as data | `Theme` loaded from a small text file; named slots; matrix/green-on-dark presets |
| `machine::step` + Effects + Attempt | `lazyui::machine` (tiny): same shape, used by widgets with modes (dropdown, drag, search debounce) |
| `Latest` | `task::Latest<I,O>`: newest wins, debounce, one wake |
| keyed identity | every `El` may have `id`; diff keeps per-id state (scroll, cursor, spring) across rebuilds |
| virtualised list | list widget draws only the visible window of a 100k model (lazytui already does) |
| idle = zero cost | loop blocks in `poll()` with a timeout = next spring/timer deadline |

## What we leave

- GPU renderer, glyph atlas, text shaping, IME, video/audio, system materials, window chrome: all pixel-world.
- OKLCH maths in v1 (plain RGB with a perceptual lerp later).
- Shared-element flight (needs sub-cell motion; revisit with half-block/sixel).
