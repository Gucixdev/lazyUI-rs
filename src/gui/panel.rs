//! A `Ui` drawn into a layer-shell panel (`crate::wl::panel`) or an xdg window (`crate::wl::window`): cells to pixels with the same rasteriser as the window,
//! pixels to 0xAARRGGBB for the shared-memory buffer.
use super::raster::{Pixels, Raster};
use crate::canvas::{Caps, Canvas, Color};
use crate::widgets::theme::Theme;
use crate::ui::Ui;
use crate::fdctl;
use crate::ui::UiEvent;
use crate::wl::panel::State;
use crate::wl::Surface;
use crate::wl::seat::Seat;
use crate::wl::wire::Conn;
use std::os::fd::RawFd;
use std::sync::mpsc::Receiver;
use std::time::Instant;

/// RGBA bytes (the rasteriser's layout) to the compositor's ARGB8888 words.
pub fn argb(rgba: &[u8]) -> Vec<u32> {
    rgba.chunks_exact(4).map(|p| u32::from_be_bytes([p[3], p[0], p[1], p[2]])).collect()
}

/// One frame of `ui` at `w` x `h` pixels: as many whole cells as fit, the rest filled with the background.
pub fn frame(ui: &mut Ui, r: &mut Raster, w: usize, h: usize) -> Vec<u32> {
    let mut px = Pixels::new(w, h, r.default_bg);
    let mut c = Canvas::new((w / r.cw) as i32, (h / r.ch) as i32, Caps::FULL);
    ui.render(&mut c);
    r.paint(&mut px, None, &c);
    argb(&px.rgba)
}

/// Put a theme on the `Ui` and make the area the cells do not cover (the pixel remainder) match its background.
pub fn apply_theme(ui: &mut Ui, r: &mut Raster, t: Theme) {
    if let Color::Rgb(a, b, c) = t.bg {
        r.default_bg = [a, b, c];
    }
    if let Color::Rgb(a, b, c) = t.fg {
        r.default_fg = [a, b, c];
    }
    ui.theme = t;
}

/// What a finedesktop event line asks of a panel: the theme name, for a `theme` event only.
pub fn theme_of(e: &crate::fdctl::Event) -> Option<&str> {
    (e.kind == "theme" && !e.rest.is_empty()).then_some(e.rest.as_str())
}

/// Draw `ui` once into an open panel or window.
pub fn show(p: &mut impl Surface, conn: &mut Conn, ui: &mut Ui, r: &mut Raster) -> std::io::Result<()> {
    let (w, h) = p.size();
    let f = frame(ui, r, w as usize, h as usize);
    p.draw(conn, &f)
}

/// Theme events from a running finedesktop, delivered on a channel plus a pipe the loop can `poll` on.
pub struct Follow {
    rx: Receiver<String>,
    rd: RawFd,
}

impl Follow {
    /// None when there is no finedesktop to follow (no socket, or it refuses).
    pub fn start() -> Option<Follow> {
        let sock = fdctl::socket()?;
        let sub = fdctl::subscribe(&sock, &["theme"]).ok()?;
        let mut fds = [0 as RawFd; 2];
        if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC | libc::O_NONBLOCK) } != 0 {
            return None;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let wr = fds[1];
        std::thread::spawn(move || {
            for e in sub {
                if let Some(name) = theme_of(&e) {
                    if tx.send(name.to_string()).is_err() {
                        break;
                    }
                    unsafe { libc::write(wr, b"x".as_ptr() as *const _, 1) };
                }
            }
        });
        Some(Follow { rx, rd: fds[0] })
    }

    fn drain(&self) -> Vec<String> {
        let mut b = [0u8; 64];
        while unsafe { libc::read(self.rd, b.as_mut_ptr() as *mut _, b.len()) } > 0 {}
        self.rx.try_iter().collect()
    }
}

/// The event loop of a panel or window: Wayland messages and finedesktop theme events in, pointer events into the `Ui`, a frame out
/// whenever the `Ui` is dirty. Sleeps in `poll` until a message, a timer or an animation frame; returns when the
/// compositor closes the panel or the `Ui` quits (the events of the batch that held the close are still delivered). `on` gets the events the `Ui` raises (clicks on buttons and so on).
pub fn run(conn: &mut Conn, p: &mut impl Surface, seat: &mut Seat, ui: &mut Ui, r: &mut Raster, follow: Option<&Follow>, on: &mut dyn FnMut(&mut Ui, UiEvent)) -> std::io::Result<()> {
    let mut last = Instant::now();
    let mut closed = false;
    loop {
        while let Some(m) = conn.take()? {
            match p.handle(conn, &m)? {
                State::Closed => closed = true,
                State::Resized => ui.dirty = true,
                State::Open => {}
            }
            for e in seat.handle(conn, &m)? {
                ui.dispatch(&e);
            }
        }
        let now = Instant::now();
        for e in seat.repeat(now) {
            ui.dispatch(&e);
        }
        ui.tick(now.duration_since(last).as_secs_f64().min(0.25));
        last = now;
        loop {
            let evs = ui.take_events();
            if evs.is_empty() {
                break;
            }
            for e in evs {
                on(ui, e);
            }
        }
        if ui.quit || closed {
            return Ok(());
        }
        if ui.dirty {
            show(p, conn, ui, r)?;
        }
        let mut timeout_ms: i32 = if ui.animating() {
            16
        } else if let Some(s) = ui.next_deadline() {
            (s * 1000.0).ceil().clamp(1.0, 60_000.0) as i32
        } else {
            -1
        };
        if let Some(d) = seat.next_repeat(Instant::now()) {
            let ms = (d.as_secs_f64() * 1000.0).ceil().clamp(1.0, 60_000.0) as i32;
            timeout_ms = if timeout_ms < 0 { ms } else { timeout_ms.min(ms) };
        }
        let mut fds = vec![libc::pollfd { fd: conn.raw_fd(), events: libc::POLLIN, revents: 0 }];
        if let Some(f) = follow {
            fds.push(libc::pollfd { fd: f.rd, events: libc::POLLIN, revents: 0 });
        }
        let n = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as _, timeout_ms) };
        if n < 0 {
            let e = std::io::Error::last_os_error();
            if e.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(e);
        }
        if fds[0].revents != 0 {
            conn.fill()?;
        }
        if let Some(f) = follow {
            for name in f.drain() {
                if let Ok(t) = Theme::from_finedesktop(&name) {
                    apply_theme(ui, r, t);
                    ui.dirty = true;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::wl::panel::Panel;
    use super::*;

    #[test]
    fn argb_orders_the_channels_for_the_compositor() {
        assert_eq!(argb(&[0x11, 0x22, 0x33, 0xff, 0, 0, 0, 0x80]), vec![0xff112233, 0x80000000]);
    }

    #[test]
    fn a_ui_frame_has_the_size_asked_and_text_pixels() {
        let Ok(mut r) = Raster::system(14.0, None) else { return };
        let mut ui = Ui::new(crate::prelude::text("hello"));
        let (w, h) = (r.cw * 10 + 3, r.ch * 2);
        let f = frame(&mut ui, &mut r, w, h);
        assert_eq!(f.len(), w * h);
        let bg = f[w * h - 1];
        assert!(f.iter().any(|&p| p != bg), "some pixel differs from the background: the text was drawn");
        assert!(f.iter().all(|&p| p >> 24 == 0xff), "opaque");
    }

    #[test]
    fn only_a_theme_event_with_a_name_changes_the_theme() {
        use crate::fdctl::Event;
        assert_eq!(theme_of(&Event::parse("theme dracula")), Some("dracula"));
        assert_eq!(theme_of(&Event::parse("focus 3")), None);
        assert_eq!(theme_of(&Event::parse("theme")), None);
    }

    #[test]
    fn apply_theme_sets_the_background_the_remainder_uses() {
        let Ok(mut r) = Raster::system(14.0, None) else { return };
        let mut ui = Ui::new(crate::prelude::text("x"));
        let mut t = Theme::from_finedesktop("nonexistent-theme").unwrap_or_else(|_| ui.theme.clone());
        t.bg = Color::Rgb(1, 2, 3);
        apply_theme(&mut ui, &mut r, t);
        assert_eq!(r.default_bg, [1, 2, 3]);
        let (w, h) = (r.cw * 4 + 5, r.ch);
        let f = frame(&mut ui, &mut r, w, h);
        assert_eq!(f[r.cw * 4 + 4], 0xff010203, "the remainder column carries the theme background");
    }

    #[test]
    fn the_loop_turns_a_pointer_click_into_a_ui_event_and_ends_when_the_panel_is_closed() {
        use crate::wl::wire::{encode, Arg};
        use crate::wl::Global;
        use std::io::Write;
        use std::os::unix::net::UnixStream;
        let Ok(mut r) = Raster::system(14.0, None) else { return };
        let (a, b) = UnixStream::pair().unwrap();
        let (mut conn, mut server) = (Conn::from_stream(a), Conn::from_stream(b.try_clone().unwrap()));
        let g = vec![Global { name: 7, interface: "wl_seat".into(), version: 9 }];
        let mut seat = Seat::bind(&mut conn, 1, &g, r.cw, r.ch).unwrap();
        let mut p = Panel::fake(7, r.cw as u32 * 20, r.ch as u32);
        let (x, y) = (r.cw as f64 * 1.5, r.ch as f64 * 0.5);
        let t = std::thread::spawn(move || {
            let mut w = b;
            let fx = |v: f64| Arg::I32((v * 256.0) as i32);
            w.write_all(&encode(2, 0, &[Arg::U32(1)])).unwrap();
            let ptr = loop {
                let m = server.read().unwrap();
                if (m.obj, m.opcode) == (2, 0) {
                    break m.reader().u32().unwrap(); // get_pointer: the id the client gave the pointer
                }
            };
            w.write_all(&encode(ptr, 0, &[Arg::U32(1), Arg::Obj(9), fx(x), fx(y)])).unwrap();
            w.write_all(&encode(ptr, 3, &[Arg::U32(1), Arg::U32(0), Arg::U32(0x110), Arg::U32(1)])).unwrap();
            w.write_all(&encode(ptr, 3, &[Arg::U32(2), Arg::U32(0), Arg::U32(0x110), Arg::U32(0)])).unwrap();
            w.write_all(&encode(7, 1, &[])).unwrap();
            w
        });
        let mut ui = Ui::new(crate::prelude::buttons("b", ["go"]));
        let mut got = Vec::new();
        run(&mut conn, &mut p, &mut seat, &mut ui, &mut r, None, &mut |_, e| got.push((e.id.clone(), e.kind.clone()))).unwrap();
        let _w = t.join().unwrap();
        assert!(got.contains(&("b".to_string(), "activate".to_string())), "{got:?}");
    }

    #[test]
    fn a_window_answers_the_ping_and_the_loop_ends_on_the_close_request() {
        use crate::wl::wire::{encode, Arg};
        use crate::wl::window::Window;
        use crate::wl::Global;
        use std::io::Write;
        use std::os::unix::net::UnixStream;
        let Ok(mut r) = Raster::system(14.0, None) else { return };
        let (a, b) = UnixStream::pair().unwrap();
        let (mut conn, mut server) = (Conn::from_stream(a), Conn::from_stream(b.try_clone().unwrap()));
        let g = vec![Global { name: 7, interface: "wl_seat".into(), version: 9 }];
        let mut seat = Seat::bind(&mut conn, 1, &g, r.cw, r.ch).unwrap();
        let mut w = Window::fake((r.cw as u32 * 10, r.ch as u32));
        let t = std::thread::spawn(move || {
            let mut out = b;
            out.write_all(&encode(3, 0, &[Arg::U32(77)])).unwrap();
            out.write_all(&encode(8, 1, &[])).unwrap();
            let mut pong = None;
            while pong.is_none() {
                let m = server.read().unwrap();
                if (m.obj, m.opcode) == (3, 3) {
                    pong = Some(m.reader().u32().unwrap());
                }
            }
            (pong, out)
        });
        let mut ui = Ui::new(crate::prelude::text("x"));
        run(&mut conn, &mut w, &mut seat, &mut ui, &mut r, None, &mut |_, _| {}).unwrap();
        let (pong, _out) = t.join().unwrap();
        assert_eq!(pong, Some(77));
    }
}
