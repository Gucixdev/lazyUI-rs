# Consumers

Decided 2026-11-24: lazyUI-rs is the one UI standard. Programs move over only when the gap list of their toolkit is empty.

| program | has now | goes to lazyUI-rs when |
|---|---|---|
| filem8 | own `fm8-ui` (1.9k lines, wgpu+winit, immediate mode) | every row below is "has" |
| steamdeck_launcher | iced | window backend + gamepad focus + lists; has: native window, `gamepad` module (evdev to keys, mapper tested, no real pad tried: no `input` group here) |
| fastkit | Go + TypeScript web stack | the web backend (wasm/htmx) covers its forms and tables |
| finedesktop | already a client (themes, ctl) | stays additive-only; shared theme file done (`Theme::from_finedesktop`), ctl client done (`lazyui::fdctl`: send, subscribe), Wayland wire client (`lazyui::wl`: messages, fd passing, registry, `wl::panel` layer-shell surface with shm frame; shown for 1 s on the running desktop without a protocol error), `gui::panel::show` draws a lazyui `Ui` into it (example `panel`, exit 0 on the running desktop; not seen on screen: the compositor has no screencopy, so grim cannot capture it), the example follows `theme` events (event form `kind rest` confirmed live with an emitted `user` event; an actual theme switch not run, it would change his desktop); pointer input done (`wl::seat`: enter, motion, buttons, wheel into lazyui mouse events; `gui::panel::run` is the event loop, tested over a fake compositor, real clicks not done: nobody at the screen), keyboard done (`wl::xkb`: libxkbcommon loaded with dlopen, the live compositor accepted its keymap; real key presses not done), key repeat done in `Seat::repeat` (rate and delay from the compositor; tested with injected time, not heard on a real keyboard); xdg-shell done (`wl::window`: toplevel with title and app id, ping, resize, close; a real window was configured at 958x1059 by the running desktop for one second; example `window`; `Surface` trait so one loop drives panels and windows); hooks: `hook=NAME` on an element forwards its events as `emit NAME KIND ID TEXT` (line format tested, not seen reaching a real `on-user` script); output scale followed |

## filem8: fm8-ui primitive -> lazyUI

Status words: **has**, **gap**, **check** (exists, not yet shown to be enough). Nothing here was measured; `check` rows need a test with 100k entries and real icons.

| fm8-ui | lazyUI |
|---|---|
| `virt` (virtual list/grid) | has: `grid` (virtual tiles with pictures and labels, `need` events for the tiles about to show; 100k tiles: first frame 0.18 ms, scrolled frames 0.18 ms each in release, `cargo test --release grid::timing -- --ignored --nocapture`; pictures and disk not included); `list` with 100k rows: set 5.3 ms, 100 page-down frames 7.5 ms; `table` with 100k rows: first frame 35 ms, 100 frames 23 ms (release, `cargo test --release grid::timing_lists -- --ignored --nocapture`); real icons not included |
| `tabs` | has: `pages` |
| `split` + `drag_frac` (draggable divider) | has: `splitter` (drag or arrow keys writes cells into the model; the pane before it says `size=@KEY:START`; a press keeps hearing the drag outside the divider) |
| `menu`, context menu | has: `menu` (popup at the label or at the right-click, separators, greyed items, outside click closes); `menubar` (titles, drop-downs, one level of sub-menus, keys and mouse, `activate` carries `File/Recent/a`) |
| `tooltip` | has: `tip="text"` on any element (half a second of rest, one line; multi-line is a gap) |
| `dialog_frame`, overlays | has: `dialog` (modal, keys and clicks belong to it while `bind=KEY` is not 0; buttons, default, first-letter keys, esc cancels); a non-modal floating window is a gap |
| `text_input` | has: `searchbar`; `pathbar` (Tab completion on the file system or from the host, `~/`, directories first, popup) |
| `progress` | has: `progressbar` |
| `icon`, `icon_button`, thumbnails, images | has: `media::Image` (decoded RGBA, the host decodes: filem8 has `fm8-codec`), `Canvas::image` (half-block fallback in terminals, real pixels in the window; text drawn over a picture wins), element `image bind=KEY`; gap: decoding inside lazyUI, icon themes |
| drag and drop between panes and out | has: `drag=`/`drop=` and list reorder inside the window; in the native window a drag that leaves it becomes a `wl_data_device` drag of `text/uri-list` (`drag=` paths, one per line), and files dropped from other programs reach the `drop=file` element under the pointer (`wl::dnd`, tested over a fake compositor; never run against a real one, no second client to drop onto) |
| live-reloaded theme file | has: the native window and panel follow finedesktop `theme` events (`panel::Follow`, `Theme::from_finedesktop`); a real switch on the desktop was not watched |
| DPI scale | has: the native window follows `wl_output.scale` of the outputs it is on (`set_buffer_scale`, font and pointer cells rescaled); fractional scales and the winit window's own `ScaleFactorChanged` are separate; not seen on a real 2x output |
| editable key bindings | has: BINDINGS.md |

Left before filem8 can move: icon themes (fm8-lz asks fm8-core for PNG icons on its worker thread, but this machine has no theme with PNG icons that `IconThemes::auto` accepts, so it was never seen working; SVG icons need a rasteriser), real icons and thumbnails from a slow disk; a non-modal floating window is `floatwin` (inside the window; a second native toplevel is not done); question in claude-config/questions.md.

## The split

- lazyUI-rs: every UI, native, terminal, CLI and web, also CRUD and SaaS apps.
- lazytui-c: the TUI toolkit for programs written in C. It grows too, but stays lighter than lazyUI-rs: terminal only, minimal.

For CRUD and SaaS (fastkit) lazyUI still needs: forms with validation, tables with sort, filter and paging, a login screen, and the htmx backend that serves them.
