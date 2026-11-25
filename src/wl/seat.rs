//! The pointer of a `wl_seat`, turned into lazyui mouse events in cells. Keyboard is not here: its keys arrive as
//! scancodes plus an xkb keymap file, which needs a keymap parser.
use super::wire::{Arg, Conn, Msg};
use super::{find, Global};
use crate::input::{Event, Mods, MouseKind};
use std::io::{Error, Result};

const CAP_POINTER: u32 = 1;
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
}

impl Seat {
    /// Bind `wl_seat`; `cw` x `ch` is one cell in pixels.
    pub fn bind(c: &mut Conn, registry: u32, globals: &[Global], cw: usize, ch: usize) -> Result<Seat> {
        let g = find(globals, "wl_seat").ok_or_else(|| Error::other("the compositor has no wl_seat"))?;
        let id = c.new_id();
        c.send(registry, 0, &[Arg::U32(g.name), Arg::Str("wl_seat"), Arg::U32(g.version.min(5)), Arg::New(id)], &[])?;
        Ok(Seat { id, pointer: None, pos: (0.0, 0.0), cell: (cw.max(1) as f64, ch.max(1) as f64), last: (0, 0), held: None, wheel: 0.0 })
    }

    fn cell_at(&self) -> (i32, i32) {
        ((self.pos.0 / self.cell.0).floor() as i32, (self.pos.1 / self.cell.1).floor() as i32)
    }

    /// Feed one message from the connection; the events it makes, if it was this seat's or its pointer's.
    pub fn handle(&mut self, c: &mut Conn, m: &Msg) -> Result<Vec<Event>> {
        let mut out = Vec::new();
        let mut r = m.reader();
        if m.obj == self.id {
            // capabilities: ask for the pointer once, release it when the seat loses it
            if m.opcode == 0 {
                let has = r.u32()? & CAP_POINTER != 0;
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
                let (_, _, code, state) = (r.u32()?, r.u32()?, r.u32()?, r.u32()?);
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
        let caps = Msg { obj: seat.id, opcode: 0, payload: encode(0, 0, &[A::U32(3)])[8..].to_vec(), fds: vec![] };
        seat.handle(&mut c, &caps).unwrap();
        let get = server.read().unwrap();
        assert_eq!((get.obj, get.opcode), (seat.id, 0), "get_pointer");
        assert_eq!(seat.pointer, Some(get.reader().u32().unwrap()));
        (seat, c, server)
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
}
