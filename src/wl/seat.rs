//! The pointer and keyboard of a `wl_seat`, turned into lazyui events: the pointer as mouse events in cells, the
//! keyboard through libxkbcommon (`xkb`, loaded at run time; without it there is no keyboard). The compositor does not
//! repeat keys for clients: the seat does, with the rate and delay from `wl_keyboard.repeat_info` (`next_repeat`, `repeat`).
use super::wire::{Arg, Conn, Msg};
use super::xkb::Xkb;
use super::{find, Global};
use crate::input::{Event, Mods, MouseKind};
use std::io::{Error, Result};
use std::time::{Duration, Instant};

const CAP_POINTER: u32 = 1;
const CAP_KEYBOARD: u32 = 2;
const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;
const BTN_MIDDLE: u32 = 0x112;
/// one wheel notch is 10 units in `wl_pointer.axis`
const NOTCH: f64 = 10.0;

pub struct Seat {
    id: u32,
    pointer: Option<u32>,
    /// pointer position in surface pixels
    pos: (f64, f64),
    cell: (f64, f64),
    last: (i32, i32),
    held: Option<u8>,
    wheel: f64,
    keyboard: Option<u32>,
    xkb: Option<Xkb>,
    mods: Mods,
    /// why there is no keyboard, when there is none
    pub keyboard_note: Option<String>,
    /// repeats per second and the wait before the first one; rate 0 turns repeating off
    repeat_rate: (u32, u32),
    held_key: Option<Held>,
    /// serial of the last button press: a drag to another program must quote it
    button_serial: u32,
    dnd: Option<super::dnd::Dnd>,
    drops: Vec<super::dnd::Drop>,
}

struct Held {
    key: u32,
    ev: Event,
    due: Instant,
}

impl Seat {
    /// Bind `wl_seat`; `cw` x `ch` is one cell in pixels.
    pub fn bind(c: &mut Conn, registry: u32, globals: &[Global], cw: usize, ch: usize) -> Result<Seat> {
        let g = find(globals, "wl_seat").ok_or_else(|| Error::other("the compositor has no wl_seat"))?;
        let id = c.new_id();
        c.send(registry, 0, &[Arg::U32(g.name), Arg::Str("wl_seat"), Arg::U32(g.version.min(5)), Arg::New(id)], &[])?;
        Ok(Seat { id, pointer: None, pos: (0.0, 0.0), cell: (cw.max(1) as f64, ch.max(1) as f64), last: (0, 0), held: None, wheel: 0.0, keyboard: None, xkb: None, mods: Mods::NONE, keyboard_note: None, repeat_rate: (25, 600), held_key: None, button_serial: 0, dnd: super::dnd::Dnd::bind(c, registry, globals, id)?, drops: Vec::new() })
    }

    /// A new cell size in surface units (the cell in pixels divided by the output scale).
    pub fn set_cell(&mut self, cw: f64, ch: f64) {
        self.cell = (cw.max(1.0), ch.max(1.0));
    }

    /// Pointer position in surface units.
    pub fn pointer(&self) -> (f64, f64) {
        self.pos
    }

    /// Hand the drag in progress to the compositor so other programs can take it: `payload` is paths, one per line.
    /// False when there is no data device or the pointer was never pressed.
    pub fn start_drag(&mut self, c: &mut Conn, surface: u32, payload: &str) -> Result<bool> {
        match self.dnd.as_mut() {
            Some(d) if self.button_serial != 0 && !d.dragging() => d.start(c, surface, self.button_serial, payload).map(|_| true),
            _ => Ok(false),
        }
    }

    pub fn dragging_out(&self) -> bool {
        self.dnd.as_ref().is_some_and(|d| d.dragging())
    }

    /// What other programs dropped on the surface since the last call.
    pub fn take_drops(&mut self) -> Vec<super::dnd::Drop> {
        std::mem::take(&mut self.drops)
    }

    fn cell_at(&self) -> (i32, i32) {
        ((self.pos.0 / self.cell.0).floor() as i32, (self.pos.1 / self.cell.1).floor() as i32)
    }

    /// How long until the held key repeats; None when no key is held.
    pub fn next_repeat(&self, now: Instant) -> Option<Duration> {
        self.held_key.as_ref().map(|h| h.due.saturating_duration_since(now))
    }

    /// The repeats of the held key that are due at `now`, one per interval. After a long stall (a loop that was not
    /// running) the missed ones are dropped, not replayed.
    pub fn repeat(&mut self, now: Instant) -> Vec<Event> {
        let interval = Duration::from_secs_f64(1.0 / self.repeat_rate.0.max(1) as f64);
        let Some(h) = self.held_key.as_mut() else { return Vec::new() };
        let mut out = Vec::new();
        if now.saturating_duration_since(h.due) > Duration::from_secs(1) {
            h.due = now;
        }
        while h.due <= now && out.len() < 8 {
            out.push(h.ev.clone());
            h.due += interval;
        }
        out
    }

    // capabilities: ask for the pointer once, release it when the seat loses it
    fn pointer_caps(&mut self, c: &mut Conn, has: bool) -> Result<()> {
        match (has, self.pointer) {
            (true, None) => {
                let p = c.new_id();
                c.send(self.id, 0, &[Arg::New(p)], &[])?;
                self.pointer = Some(p);
            }
            (false, Some(p)) => {
                // wl_pointer.release is a destructor since version 3; the seat is bound at 5
                c.send(p, 1, &[], &[])?;
                self.pointer = None;
                self.held = None;
            }
            _ => {}
        }
        Ok(())
    }

    fn keyboard_caps(&mut self, c: &mut Conn, has: bool) -> Result<()> {
        match (has, self.keyboard) {
            (true, None) => {
                match Xkb::load() {
                    Ok(x) => self.xkb = Some(x),
                    Err(e) => {
                        self.keyboard_note = Some(e.to_string());
                        return Ok(());
                    }
                }
                let k = c.new_id();
                c.send(self.id, 1, &[Arg::New(k)], &[])?;
                self.keyboard = Some(k);
            }
            (false, Some(k)) => {
                c.send(k, 3, &[], &[])?;
                self.keyboard = None;
                self.mods = Mods::NONE;
                self.held_key = None;
            }
            _ => {}
        }
        Ok(())
    }

    /// keymap(format, fd, size), enter, leave, key(serial, time, key, state), modifiers(serial, dep, lat, locked, group)
    fn keyboard_event(&mut self, m: &Msg) -> Result<Option<Event>> {
        let mut r = m.reader();
        let Some(x) = self.xkb.as_mut() else { return Ok(None) };
        match m.opcode {
            0 => {
                let (format, size) = (r.u32()?, r.u32()? as usize);
                if format != 1 {
                    self.keyboard_note = Some(format!("keymap format {format} is not xkb_v1"));
                    return Ok(None);
                }
                let fd = m.fds.first().ok_or_else(|| Error::other("keymap event without a file descriptor"))?;
                use std::os::fd::AsRawFd;
                let p = unsafe { libc::mmap(std::ptr::null_mut(), size, libc::PROT_READ, libc::MAP_PRIVATE, fd.as_raw_fd(), 0) };
                if p == libc::MAP_FAILED {
                    return Err(Error::last_os_error());
                }
                let text = unsafe { std::slice::from_raw_parts(p as *const u8, size) }.to_vec();
                unsafe { libc::munmap(p, size) };
                if let Err(e) = x.set_keymap(&text) {
                    self.keyboard_note = Some(e.to_string());
                }
            }
            2 => {
                self.mods = Mods::NONE;
                self.held_key = None;
            }
            3 => {
                let (_, _, key, state) = (r.u32()?, r.u32()?, r.u32()?, r.u32()?);
                if state == 1 {
                    let ev = x.key(key, self.mods);
                    let (rate, delay) = self.repeat_rate;
                    self.held_key = match &ev {
                        Some(e) if rate > 0 && x.key_repeats(key) => Some(Held { key, ev: e.clone(), due: Instant::now() + Duration::from_millis(delay as u64) }),
                        _ => None,
                    };
                    return Ok(ev);
                }
                if self.held_key.as_ref().is_some_and(|h| h.key == key) {
                    self.held_key = None;
                }
            }
            5 => {
                let (rate, delay) = (r.i32()?, r.i32()?);
                self.repeat_rate = (rate.max(0) as u32, delay.max(0) as u32);
                if rate <= 0 {
                    self.held_key = None;
                }
            }
            4 => {
                let (_, dep, lat, locked, group) = (r.u32()?, r.u32()?, r.u32()?, r.u32()?, r.u32()?);
                self.mods = x.update_mask(dep, lat, locked, group);
            }
            _ => {}
        }
        Ok(None)
    }

    /// Feed one message from the connection; the events it makes, if it was this seat's or its pointer's.
    pub fn handle(&mut self, c: &mut Conn, m: &Msg) -> Result<Vec<Event>> {
        let mut out = Vec::new();
        let mut r = m.reader();
        if let Some(d) = self.dnd.as_mut().filter(|d| d.owns(m.obj)) {
            if let Some(drop) = d.handle(c, m)? {
                self.drops.push(drop);
            }
            return Ok(out);
        }
        if m.obj == self.id {
            if m.opcode == 0 {
                let caps = r.u32()?;
                self.pointer_caps(c, caps & CAP_POINTER != 0)?;
                self.keyboard_caps(c, caps & CAP_KEYBOARD != 0)?;
            }
            return Ok(out);
        }
        if Some(m.obj) == self.keyboard {
            if let Some(e) = self.keyboard_event(m)? {
                out.push(e);
            }
            return Ok(out);
        }
        if Some(m.obj) != self.pointer {
            return Ok(out);
        }
        match m.opcode {
            // enter(serial, surface, x, y)
            0 => {
                let (_, _, x, y) = (r.u32()?, r.u32()?, r.fixed()?, r.fixed()?);
                self.pos = (x, y);
                self.last = self.cell_at();
            }
            // leave
            1 => self.held = None,
            // motion(time, x, y)
            2 => {
                let (_, x, y) = (r.u32()?, r.fixed()?, r.fixed()?);
                self.pos = (x, y);
                let cell = self.cell_at();
                if cell != self.last {
                    self.last = cell;
                    if let Some(b) = self.held {
                        out.push(Event::Mouse(MouseKind::Drag(b), cell.0, cell.1, Mods::NONE));
                    }
                }
            }
            // button(serial, time, button, state)
            3 => {
                let (serial, _, code, state) = (r.u32()?, r.u32()?, r.u32()?, r.u32()?);
                if state == 1 {
                    self.button_serial = serial;
                }
                let b = match code {
                    BTN_LEFT => 0,
                    BTN_MIDDLE => 1,
                    BTN_RIGHT => 2,
                    _ => return Ok(out),
                };
                let (x, y) = self.cell_at();
                if state == 1 {
                    self.held = Some(b);
                    out.push(Event::Mouse(MouseKind::Down(b), x, y, Mods::NONE));
                } else {
                    self.held = None;
                    out.push(Event::Mouse(MouseKind::Up(b), x, y, Mods::NONE));
                }
            }
            // axis(time, axis, value): axis 0 is vertical, positive is down
            4 => {
                let (_, axis, v) = (r.u32()?, r.u32()?, r.fixed()?);
                if axis == 0 {
                    self.wheel += v / NOTCH;
                    let (x, y) = self.cell_at();
                    while self.wheel.abs() >= 1.0 {
                        let down = self.wheel > 0.0;
                        self.wheel -= self.wheel.signum();
                        out.push(Event::Mouse(if down { MouseKind::WheelDown } else { MouseKind::WheelUp }, x, y, Mods::NONE));
                    }
                }
            }
            _ => {}
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wl::wire::{encode, Arg as A};
    use std::os::unix::net::UnixStream;

    fn fx(v: f64) -> A<'static> {
        A::I32((v * 256.0) as i32)
    }

    /// A seat (id 3) whose pointer (id 4) has been granted, over a connection whose far end is returned.
    fn ready() -> (Seat, Conn, Conn) {
        let (a, b) = UnixStream::pair().unwrap();
        let (mut c, mut server) = (Conn::from_stream(a), Conn::from_stream(b));
        let g = vec![Global { name: 7, interface: "wl_seat".into(), version: 9 }];
        let mut seat = Seat::bind(&mut c, 2, &g, 10, 20).unwrap();
        let bind = server.read().unwrap();
        assert_eq!((bind.obj, bind.opcode), (2, 0));
        let caps = Msg { obj: seat.id, opcode: 0, payload: encode(0, 0, &[A::U32(1)])[8..].to_vec(), fds: vec![] };
        seat.handle(&mut c, &caps).unwrap();
        let get = server.read().unwrap();
        assert_eq!((get.obj, get.opcode), (seat.id, 0), "get_pointer");
        assert_eq!(seat.pointer, Some(get.reader().u32().unwrap()));
        (seat, c, server)
    }

    #[test]
    fn a_drag_out_quotes_the_serial_of_the_last_press_and_needs_a_data_device() {
        let (mut seat, mut c, mut server) = ready();
        assert!(!seat.start_drag(&mut c, 9, "/a").unwrap(), "no data device manager was advertised");
        assert!(!seat.dragging_out());
        let (a, b) = UnixStream::pair().unwrap();
        let (mut c2, mut s2) = (Conn::from_stream(a), Conn::from_stream(b));
        let g = vec![Global { name: 7, interface: "wl_seat".into(), version: 9 }, Global { name: 8, interface: "wl_data_device_manager".into(), version: 3 }];
        let mut seat2 = Seat::bind(&mut c2, 2, &g, 10, 20).unwrap();
        for _ in 0..3 {
            s2.read().unwrap();
        }
        assert!(!seat2.start_drag(&mut c2, 9, "/a").unwrap(), "no button was pressed yet");
        seat2.button_serial = 41;
        assert!(seat2.start_drag(&mut c2, 9, "/a").unwrap());
        assert!(seat2.dragging_out());
        let mut last = None;
        for _ in 0..5 {
            last = Some(s2.read().unwrap());
        }
        let start = last.unwrap();
        let mut r = start.reader();
        assert_eq!(start.opcode, 0);
        let (_, surface, icon, serial) = (r.u32().unwrap(), r.u32().unwrap(), r.u32().unwrap(), r.u32().unwrap());
        assert_eq!((surface, icon, serial), (9, 0, 41));
        let _ = (&mut server, &mut c);
    }

    fn ev(seat: &mut Seat, c: &mut Conn, opcode: u16, args: &[A]) -> Vec<Event> {
        let m = Msg { obj: seat.pointer.unwrap(), opcode, payload: encode(0, 0, args)[8..].to_vec(), fds: vec![] };
        seat.handle(c, &m).unwrap()
    }

    #[test]
    fn click_is_down_then_up_at_the_cell_under_the_pointer() {
        let (mut s, mut c, _srv) = ready();
        ev(&mut s, &mut c, 0, &[A::U32(1), A::Obj(9), fx(35.5), fx(41.0)]);
        let d = ev(&mut s, &mut c, 3, &[A::U32(1), A::U32(0), A::U32(BTN_LEFT), A::U32(1)]);
        let u = ev(&mut s, &mut c, 3, &[A::U32(2), A::U32(0), A::U32(BTN_LEFT), A::U32(0)]);
        assert_eq!(d, vec![Event::Mouse(MouseKind::Down(0), 3, 2, Mods::NONE)]);
        assert_eq!(u, vec![Event::Mouse(MouseKind::Up(0), 3, 2, Mods::NONE)]);
    }

    #[test]
    fn drag_only_when_held_and_only_on_a_new_cell() {
        let (mut s, mut c, _srv) = ready();
        ev(&mut s, &mut c, 0, &[A::U32(1), A::Obj(9), fx(1.0), fx(1.0)]);
        assert!(ev(&mut s, &mut c, 2, &[A::U32(0), fx(12.0), fx(1.0)]).is_empty(), "no button held: no drag");
        ev(&mut s, &mut c, 3, &[A::U32(1), A::U32(0), A::U32(BTN_RIGHT), A::U32(1)]);
        assert!(ev(&mut s, &mut c, 2, &[A::U32(0), fx(13.0), fx(2.0)]).is_empty(), "same cell: nothing");
        assert_eq!(ev(&mut s, &mut c, 2, &[A::U32(0), fx(25.0), fx(2.0)]), vec![Event::Mouse(MouseKind::Drag(2), 2, 0, Mods::NONE)]);
        ev(&mut s, &mut c, 1, &[A::U32(1), A::Obj(9)]);
        assert!(ev(&mut s, &mut c, 2, &[A::U32(0), fx(55.0), fx(2.0)]).is_empty(), "leaving drops the held button");
    }

    #[test]
    fn wheel_notches_accumulate_into_whole_steps() {
        let (mut s, mut c, _srv) = ready();
        assert!(ev(&mut s, &mut c, 4, &[A::U32(0), A::U32(0), fx(4.0)]).is_empty());
        let down = ev(&mut s, &mut c, 4, &[A::U32(0), A::U32(0), fx(16.0)]);
        assert_eq!(down.len(), 2, "20 units = two notches");
        assert!(matches!(down[0], Event::Mouse(MouseKind::WheelDown, ..)));
        let up = ev(&mut s, &mut c, 4, &[A::U32(0), A::U32(0), fx(-10.0)]);
        assert!(matches!(up[..], [Event::Mouse(MouseKind::WheelUp, ..)]));
        assert!(ev(&mut s, &mut c, 4, &[A::U32(0), A::U32(1), fx(30.0)]).is_empty(), "horizontal axis ignored");
    }

    #[test]
    fn losing_the_pointer_capability_releases_it() {
        let (mut s, mut c, mut srv) = ready();
        let p = s.pointer.unwrap();
        let caps = Msg { obj: s.id, opcode: 0, payload: encode(0, 0, &[A::U32(0)])[8..].to_vec(), fds: vec![] };
        s.handle(&mut c, &caps).unwrap();
        let rel = srv.read().unwrap();
        assert_eq!((rel.obj, rel.opcode), (p, 1));
        assert_eq!(s.pointer, None);
    }

    #[test]
    fn other_objects_and_unknown_buttons_make_nothing() {
        let (mut s, mut c, _srv) = ready();
        let other = Msg { obj: 99, opcode: 3, payload: vec![0; 16], fds: vec![] };
        assert!(s.handle(&mut c, &other).unwrap().is_empty());
        assert!(ev(&mut s, &mut c, 3, &[A::U32(1), A::U32(0), A::U32(0x113), A::U32(1)]).is_empty());
    }

    #[test]
    fn a_keymap_from_the_compositor_turns_scancodes_into_keys() {
        use std::io::Write;
        use std::os::fd::AsRawFd;
        let Ok(probe) = Xkb::load() else { return };
        let Ok(text) = probe.default_keymap_text() else { return };
        if text.is_empty() {
            return;
        }
        drop(probe);
        let (a, b) = UnixStream::pair().unwrap();
        let (mut c, mut server) = (Conn::from_stream(a), Conn::from_stream(b));
        let g = vec![Global { name: 7, interface: "wl_seat".into(), version: 9 }];
        let mut seat = Seat::bind(&mut c, 1, &g, 10, 20).unwrap();
        server.read().unwrap();
        let caps = Msg { obj: seat.id, opcode: 0, payload: encode(0, 0, &[A::U32(2)])[8..].to_vec(), fds: vec![] };
        seat.handle(&mut c, &caps).unwrap();
        let get = server.read().unwrap();
        assert_eq!((get.obj, get.opcode), (seat.id, 1), "get_keyboard");
        let kb = get.reader().u32().unwrap();
        // the keymap travels as a file the compositor made
        let fd = unsafe { libc::memfd_create(c"keymap".as_ptr(), 0) };
        let mut f = unsafe { <std::fs::File as std::os::fd::FromRawFd>::from_raw_fd(fd) };
        f.write_all(&text).unwrap();
        server.send(kb, 0, &[A::U32(1), A::Fd, A::U32(text.len() as u32)], &[f.as_raw_fd()]).unwrap();
        let km = c.read().unwrap();
        assert!(seat.handle(&mut c, &km).unwrap().is_empty());
        assert_eq!(seat.keyboard_note, None);
        let key = |seat: &mut Seat, c: &mut Conn, code: u32, state: u32| {
            let m = Msg { obj: kb, opcode: 3, payload: encode(0, 0, &[A::U32(1), A::U32(0), A::U32(code), A::U32(state)])[8..].to_vec(), fds: vec![] };
            seat.handle(c, &m).unwrap()
        };
        let modifiers = |seat: &mut Seat, c: &mut Conn, dep: u32| {
            let m = Msg { obj: kb, opcode: 4, payload: encode(0, 0, &[A::U32(1), A::U32(dep), A::U32(0), A::U32(0), A::U32(0)])[8..].to_vec(), fds: vec![] };
            seat.handle(c, &m).unwrap()
        };
        let plain = key(&mut seat, &mut c, 30, 1);
        assert_eq!(plain.len(), 1, "a press makes one event");
        assert!(key(&mut seat, &mut c, 30, 0).is_empty(), "a release makes none");
        if plain == vec![Event::Key(crate::input::Key::Char('a'), Mods::NONE)] {
            modifiers(&mut seat, &mut c, 1);
            assert_eq!(key(&mut seat, &mut c, 30, 1), vec![Event::Key(crate::input::Key::Char('A'), Mods::NONE)]);
            modifiers(&mut seat, &mut c, 4);
            assert_eq!(key(&mut seat, &mut c, 30, 1), vec![Event::Key(crate::input::Key::Char('a'), Mods::CTRL)]);
        }
        // holding a letter repeats it after the delay; shift never does; a release stops it
        modifiers(&mut seat, &mut c, 0);
        let info = Msg { obj: kb, opcode: 5, payload: encode(0, 0, &[A::I32(25), A::I32(600)])[8..].to_vec(), fds: vec![] };
        seat.handle(&mut c, &info).unwrap();
        let pressed = key(&mut seat, &mut c, 30, 1);
        let t0 = Instant::now();
        assert_eq!(seat.repeat(t0), vec![], "nothing before the delay");
        assert!(seat.next_repeat(t0).unwrap() > Duration::from_millis(500));
        let later = seat.repeat(t0 + Duration::from_millis(700));
        assert!(later.len() >= 2 && later.iter().all(|e| Some(e) == pressed.first()), "{later:?}");
        key(&mut seat, &mut c, 30, 0);
        assert_eq!(seat.repeat(t0 + Duration::from_secs(5)), vec![]);
        assert_eq!(seat.next_repeat(t0), None);
        key(&mut seat, &mut c, 42, 1);
        assert_eq!(seat.next_repeat(t0), None, "left shift does not repeat");
        let off = Msg { obj: kb, opcode: 5, payload: encode(0, 0, &[A::I32(0), A::I32(600)])[8..].to_vec(), fds: vec![] };
        seat.handle(&mut c, &off).unwrap();
        key(&mut seat, &mut c, 30, 1);
        assert_eq!(seat.next_repeat(t0), None, "rate 0 turns repeating off");
        // losing the capability releases the keyboard
        let none = Msg { obj: seat.id, opcode: 0, payload: encode(0, 0, &[A::U32(0)])[8..].to_vec(), fds: vec![] };
        seat.handle(&mut c, &none).unwrap();
        let rel = server.read().unwrap();
        assert_eq!((rel.obj, rel.opcode), (kb, 3));
    }

    /// Needs a running compositor with a keyboard: binds the seat and reads its capabilities and keymap (two round
    /// trips), then says whether the compositor's own keymap was accepted. Presses nothing.
    /// `cargo test --lib wl::seat -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn live_seat_accepts_the_compositors_keymap() {
        let mut c = crate::wl::wire::connect().unwrap();
        let (reg, g) = crate::wl::registry(&mut c).unwrap();
        let mut seat = Seat::bind(&mut c, reg, &g, 10, 20).unwrap();
        for _ in 0..2 {
            let cb = c.new_id();
            c.send(1, 0, &[A::New(cb)], &[]).unwrap();
            loop {
                let m = c.read().unwrap();
                if m.obj == cb {
                    break;
                }
                seat.handle(&mut c, &m).unwrap();
            }
        }
        println!("pointer {:?}, keyboard {:?}, note {:?}, keymap loaded {:?}", seat.pointer, seat.keyboard, seat.keyboard_note, seat.xkb.as_ref().map(|x| x.has_keymap()));
        assert_eq!(seat.keyboard_note, None);
        assert_eq!(seat.xkb.as_ref().map(|x| x.has_keymap()), Some(true));
    }
}
