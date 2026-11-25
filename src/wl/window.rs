//! An ordinary application window: `xdg_wm_base` -> `xdg_surface` -> `xdg_toplevel`, pixels through shared memory.
//! The compositor decides the size (tiling, floating, fullscreen); the first `configure` says it, later ones resize.
use super::panel::State;
use super::shm::Frame;
use super::wire::{Arg, Conn, Msg};
use super::{bind, find, Global};
use std::io::{Error, Result};

pub struct Window {
    surface: u32,
    base: u32,
    xdg_surface: u32,
    toplevel: u32,
    frame: Frame,
    /// the size the last toplevel `configure` asked for; applied when its `xdg_surface.configure` arrives
    asked: (u32, u32),
    /// every `wl_output` with its scale, and the ones the surface is on now
    outputs: Vec<(u32, u32)>,
    entered: Vec<u32>,
}

fn protocol_error(m: &Msg) -> Result<()> {
    if m.obj == 1 && m.opcode == 0 {
        let mut r = m.reader();
        let (obj, code, text) = (r.u32()?, r.u32()?, r.string()?);
        return Err(Error::other(format!("wayland error on object {obj} code {code}: {text}")));
    }
    Ok(())
}

impl Window {
    /// Make the window and wait for the first configure. `size` is used when the compositor has no opinion (0 x 0).
    pub fn open(c: &mut Conn, registry: u32, globals: &[Global], title: &str, app_id: &str, size: (u32, u32)) -> Result<Window> {
        let need = |n: &str| find(globals, n).ok_or_else(|| Error::other(format!("the compositor has no {n}")));
        let compositor = bind(c, registry, need("wl_compositor")?, 4)?;
        let shm = bind(c, registry, need("wl_shm")?, 1)?;
        let base = bind(c, registry, need("xdg_wm_base")?, 2)?;
        let surface = c.new_id();
        c.send(compositor, 0, &[Arg::New(surface)], &[])?;
        let xdg_surface = c.new_id();
        c.send(base, 2, &[Arg::New(xdg_surface), Arg::Obj(surface)], &[])?;
        let toplevel = c.new_id();
        c.send(xdg_surface, 1, &[Arg::New(toplevel)], &[])?;
        c.send(toplevel, 2, &[Arg::Str(title)], &[])?;
        c.send(toplevel, 3, &[Arg::Str(app_id)], &[])?;
        c.send(surface, 6, &[], &[])?;
        let mut outputs = Vec::new();
        for g in globals.iter().filter(|g| g.interface == "wl_output") {
            outputs.push((bind(c, registry, g, 2)?, 1));
        }
        let mut w = Window { surface, base, xdg_surface, toplevel, frame: Frame::new(surface, shm, size.0, size.1), asked: (0, 0), outputs, entered: Vec::new() };
        loop {
            let m = c.read()?;
            protocol_error(&m)?;
            if w.handle(c, &m)? == State::Closed {
                return Err(Error::other("the compositor closed the window before it was shown"));
            }
            if m.obj == xdg_surface && m.opcode == 0 {
                return Ok(w);
            }
        }
    }

    pub fn size(&self) -> (u32, u32) {
        (self.frame.width, self.frame.height)
    }

    /// The buffer scale: the largest scale among the outputs the window is on (1 until one is known).
    pub fn scale(&self) -> u32 {
        self.entered.iter().filter_map(|id| self.outputs.iter().find(|o| o.0 == *id)).map(|o| o.1).max().unwrap_or(1).max(1)
    }

    pub fn draw(&mut self, c: &mut Conn, argb: &[u32]) -> Result<()> {
        self.frame.draw(c, argb)
    }

    /// Ping (answered), a configure (acked; a new size drops the buffer) and the close request.
    pub fn handle(&mut self, c: &mut Conn, m: &Msg) -> Result<State> {
        protocol_error(m)?;
        if m.obj == self.surface && m.opcode <= 1 {
            let id = m.reader().u32()?;
            self.entered.retain(|x| *x != id);
            if m.opcode == 0 {
                self.entered.push(id);
            }
            return self.follow_scale(c);
        }
        if let Some(o) = self.outputs.iter_mut().find(|o| o.0 == m.obj) {
            if m.opcode == 3 {
                o.1 = m.reader().i32()?.max(1) as u32;
                return self.follow_scale(c);
            }
            return Ok(State::Open);
        }
        if m.obj == self.base && m.opcode == 0 {
            let serial = m.reader().u32()?;
            c.send(self.base, 3, &[Arg::U32(serial)], &[])?;
        } else if m.obj == self.toplevel && m.opcode == 0 {
            let mut r = m.reader();
            self.asked = (r.u32()?, r.u32()?);
        } else if m.obj == self.toplevel && m.opcode == 1 {
            return Ok(State::Closed);
        } else if m.obj == self.xdg_surface && m.opcode == 0 {
            let serial = m.reader().u32()?;
            c.send(self.xdg_surface, 4, &[Arg::U32(serial)], &[])?;
            let (cw, ch) = self.size();
            let (w, h) = (if self.asked.0 > 0 { self.asked.0 } else { cw }, if self.asked.1 > 0 { self.asked.1 } else { ch });
            if (w, h) != (cw, ch) {
                self.frame.resize(c, w, h)?;
                return Ok(State::Resized);
            }
        }
        Ok(State::Open)
    }

    fn follow_scale(&mut self, c: &mut Conn) -> Result<State> {
        let s = self.scale();
        if s == self.frame.scale {
            return Ok(State::Open);
        }
        self.frame.set_scale(c, s)?;
        Ok(State::Resized)
    }

    /// Destroy the role objects from the inside out, then the surface.
    pub fn close(self, c: &mut Conn) -> Result<()> {
        c.send(self.toplevel, 0, &[], &[])?;
        c.send(self.xdg_surface, 0, &[], &[])?;
        c.send(self.surface, 0, &[], &[])
    }

    #[cfg(test)]
    pub(crate) fn fake(size: (u32, u32)) -> Window {
        Window { surface: 6, base: 3, xdg_surface: 7, toplevel: 8, frame: Frame::new(6, 4, size.0, size.1), asked: (0, 0), outputs: Vec::new(), entered: Vec::new() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wl::wire::encode;
    use std::io::Write;
    use std::os::unix::net::UnixStream;

    fn globals() -> Vec<Global> {
        ["wl_compositor", "wl_shm", "xdg_wm_base"].iter().enumerate().map(|(i, n)| Global { name: i as u32 + 1, interface: n.to_string(), version: 6 }).collect()
    }

    fn msg(obj: u32, opcode: u16, args: &[Arg]) -> Msg {
        Msg { obj, opcode, payload: encode(0, 0, args)[8..].to_vec(), fds: vec![] }
    }

    #[test]
    fn open_builds_the_role_in_order_and_takes_the_size_from_the_configure() {
        let (client, mut server) = UnixStream::pair().unwrap();
        let mut c = Conn::from_stream(client);
        let registry = c.new_id(); // 2; compositor 3, shm 4, base 5, surface 6, xdg_surface 7, toplevel 8
        let t = std::thread::spawn(move || {
            let mut s = Conn::from_stream(server.try_clone().unwrap());
            let mut seen = Vec::new();
            // 3 binds, create_surface, get_xdg_surface, get_toplevel, set_title, set_app_id, commit
            let mut title = String::new();
            for i in 0..9 {
                let m = s.read().unwrap();
                if i == 6 {
                    title = m.reader().string().unwrap();
                }
                seen.push((m.obj, m.opcode));
            }
            server.write_all(&encode(8, 0, &[Arg::I32(640), Arg::I32(480), Arg::U32(0)])).unwrap();
            server.write_all(&encode(7, 0, &[Arg::U32(55)])).unwrap();
            let ack = s.read().unwrap();
            (seen, title, (ack.obj, ack.opcode, ack.reader().u32().unwrap()))
        });
        let w = Window::open(&mut c, registry, &globals(), "files", "lazyui.files", (100, 100)).unwrap();
        let (seen, title, ack) = t.join().unwrap();
        assert_eq!(seen, vec![(2, 0), (2, 0), (2, 0), (3, 0), (5, 2), (7, 1), (8, 2), (8, 3), (6, 6)]);
        assert_eq!(title, "files");
        assert_eq!(ack, (7, 4, 55));
        assert_eq!(w.size(), (640, 480), "the compositor's size wins over the requested 100 x 100");
    }

    #[test]
    fn ping_is_answered_a_new_size_resizes_and_close_is_reported() {
        let (client, server) = UnixStream::pair().unwrap();
        let mut c = Conn::from_stream(client);
        let mut s = Conn::from_stream(server);
        let mut w = Window::fake((100, 50));
        assert_eq!(w.handle(&mut c, &msg(3, 0, &[Arg::U32(9)])).unwrap(), State::Open);
        let pong = s.read().unwrap();
        assert_eq!((pong.obj, pong.opcode, pong.reader().u32().unwrap()), (3, 3, 9));
        w.handle(&mut c, &msg(8, 0, &[Arg::I32(300), Arg::I32(0), Arg::U32(0)])).unwrap();
        assert_eq!(w.handle(&mut c, &msg(7, 0, &[Arg::U32(10)])).unwrap(), State::Resized);
        assert_eq!(w.size(), (300, 50), "0 on an axis keeps the old size");
        assert_eq!((s.read().unwrap().opcode, 0), (4, 0));
        assert_eq!(w.handle(&mut c, &msg(7, 0, &[Arg::U32(11)])).unwrap(), State::Open, "same size again");
        assert_eq!(w.handle(&mut c, &msg(8, 1, &[])).unwrap(), State::Closed);
        let e = w.handle(&mut c, &msg(1, 0, &[Arg::U32(5), Arg::U32(1), Arg::Str("nope")])).unwrap_err().to_string();
        assert!(e.contains("code 1: nope"), "{e}");
    }

    #[test]
    fn the_scale_follows_the_outputs_the_surface_is_on_and_sizes_the_buffer() {
        let (client, server) = UnixStream::pair().unwrap();
        let mut c = Conn::from_stream(client);
        let mut s = Conn::from_stream(server);
        let mut w = Window::fake((100, 50));
        w.outputs = vec![(20, 1), (21, 1)];
        assert_eq!(w.scale(), 1, "on no output yet");
        assert_eq!(w.handle(&mut c, &msg(6, 0, &[Arg::Obj(21)])).unwrap(), State::Open, "entering a 1x output changes nothing");
        assert_eq!(w.handle(&mut c, &msg(21, 3, &[Arg::I32(2)])).unwrap(), State::Resized);
        let set = s.read().unwrap();
        assert_eq!((set.obj, set.opcode, set.reader().i32().unwrap()), (6, 8, 2), "set_buffer_scale 2");
        assert_eq!(w.scale(), 2);
        assert_eq!(w.size(), (100, 50), "the size stays in surface units");
        assert!(w.draw(&mut c, &vec![0; 100 * 50]).is_err(), "a 2x buffer needs 200 x 100 pixels");
        assert_eq!(w.handle(&mut c, &msg(6, 1, &[Arg::Obj(21)])).unwrap(), State::Resized, "leaving the 2x output");
        assert_eq!(w.scale(), 1);
        assert_eq!(w.handle(&mut c, &msg(20, 1, &[Arg::I32(0), Arg::I32(0)])).unwrap(), State::Open, "other output events are ignored");
    }

    #[test]
    fn a_missing_xdg_wm_base_is_said_by_name() {
        let (client, _server) = UnixStream::pair().unwrap();
        let mut c = Conn::from_stream(client);
        let g: Vec<Global> = globals().into_iter().filter(|g| g.interface != "xdg_wm_base").collect();
        let e = Window::open(&mut c, 2, &g, "t", "a", (1, 1)).err().unwrap().to_string();
        assert!(e.contains("xdg_wm_base"), "{e}");
    }

    /// Needs a running compositor with xdg_wm_base: opens a window for one second (a tiling compositor will tile it
    /// for that second) and closes it. `cargo test --lib wl::window -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn live_window_is_configured_and_closed_without_a_protocol_error() {
        let mut c = crate::wl::wire::connect().unwrap();
        let (reg, g) = crate::wl::registry(&mut c).unwrap();
        let mut w = Window::open(&mut c, reg, &g, "lazyui test", "lazyui.test", (320, 200)).unwrap();
        println!("configured {:?}", w.size());
        let (cw, ch) = w.size();
        w.draw(&mut c, &vec![0xff203040u32; (cw * ch) as usize]).unwrap();
        std::thread::sleep(std::time::Duration::from_secs(1));
        w.close(&mut c).unwrap();
        let (_, again) = crate::wl::registry(&mut c).unwrap();
        assert!(!again.is_empty());
    }
}
