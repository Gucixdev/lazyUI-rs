# Consumers

Decided 2026-11-24: lazyUI-rs is the one UI standard. Programs move over only when the gap list of their toolkit is empty.

| program | has now | goes to lazyUI-rs when |
|---|---|---|
| filem8 | own `fm8-ui` (1.9k lines, wgpu+winit, immediate mode) | every row below is "has" |
| steamdeck_launcher | iced | window backend + gamepad focus + lists |
| fastkit | Go + TypeScript web stack | the web backend (wasm/htmx) covers its forms and tables |
| finedesktop | already a client (themes, ctl) | stays additive-only; shared theme file done (`Theme::from_finedesktop`), ctl client done (`lazyui::fdctl`: send, subscribe), Wayland wire client (`lazyui::wl`: messages, fd passing, registry, `wl::panel` layer-shell surface with shm frame; shown for 1 s on the running desktop without a protocol error), no input and no lazyui Ui rendering into it yet |

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
