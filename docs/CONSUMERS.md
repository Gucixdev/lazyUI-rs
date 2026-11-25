# Consumers

Decided 2026-11-24: lazyUI-rs is the one UI standard. Programs move over only when the gap list of their toolkit is empty.

| program | has now | goes to lazyUI-rs when |
|---|---|---|
| filem8 | own `fm8-ui` (1.9k lines, wgpu+winit, immediate mode) | every row below is "has" |
| steamdeck_launcher | iced | window backend + gamepad focus + lists |
| fastkit | Go + TypeScript web stack | the web backend (wasm/htmx) covers its forms and tables |
| finedesktop | already a client (themes, ctl) | stays additive-only; shared theme file done (`Theme::from_finedesktop`), ctl client done (`lazyui::fdctl`: send, subscribe), Wayland wire client (`lazyui::wl`: messages, fd passing, registry, `wl::panel` layer-shell surface with shm frame; shown for 1 s on the running desktop without a protocol error), `gui::panel::show` draws a lazyui `Ui` into it (example `panel`, exit 0 on the running desktop; not seen on screen: the compositor has no screencopy, so grim cannot capture it), the example follows `theme` events (event form `kind rest` confirmed live with an emitted `user` event; an actual theme switch not run, it would change his desktop); pointer input done (`wl::seat`: enter, motion, buttons, wheel into lazyui mouse events; `gui::panel::run` is the event loop, tested over a fake compositor, real clicks not done: nobody at the screen), keyboard done (`wl::xkb`: libxkbcommon loaded with dlopen, the live compositor accepted its keymap; real key presses not done), key repeat done in `Seat::repeat` (rate and delay from the compositor; tested with injected time, not heard on a real keyboard), no xdg-shell |

## filem8: fm8-ui primitive -> lazyUI

Status words: **has**, **gap**, **check** (exists, not yet shown to be enough). Nothing here was measured; `check` rows need a test with 100k entries and real icons.

| fm8-ui | lazyUI |
|---|---|
| `virt` (virtual list/grid) | check: `list`, `table`; grid view of thumbnails is a gap |
| `tabs` | has: `pages` |
| `split` + `drag_frac` (draggable divider) | check: `layout::split` is fixed; draggable divider is a gap |
| `menu`, context menu | gap: only `dropdown` |
| `tooltip` | gap |
| `dialog_frame`, overlays | gap |
| `text_input` | has: `searchbar`; path bar with completion is a gap |
| `progress` | has: `progressbar` |
| `icon`, `icon_button`, thumbnails, images | gap: media layer (PNG/JPEG/WebP, icon themes) |
| drag and drop between panes and out | gap: in progress |
| live-reloaded theme file | check |
| DPI scale | check |
| editable key bindings | has: BINDINGS.md |

Order of work: media layer, drag and drop, menus and dialogs, draggable split, tooltip, thumbnail grid.

## The split

- lazyUI-rs: every UI, native, terminal, CLI and web, also CRUD and SaaS apps.
- lazytui-c: the TUI toolkit for programs written in C. It grows too, but stays lighter than lazyUI-rs: terminal only, minimal.

For CRUD and SaaS (fastkit) lazyUI still needs: forms with validation, tables with sort, filter and paging, a login screen, and the htmx backend that serves them.
