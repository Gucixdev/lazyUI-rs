//! A layer-shell panel: a surface the compositor places at a screen edge (bar, launcher, overlay), pixels through
//! shared memory. Protocols: wl_compositor, wl_shm, zwlr_layer_shell_v1 (wlr-layer-shell-unstable-v1).
use super::shm::Frame;
use super::wire::{Arg, Conn, Msg};
use super::{bind, find, Global};
use std::io::{Error, Result};

pub const LAYER_BACKGROUND: u32 = 0;
pub const LAYER_BOTTOM: u32 = 1;
pub const LAYER_TOP: u32 = 2;
pub const LAYER_OVERLAY: u32 = 3;
pub const ANCHOR_TOP: u32 = 1;
pub const ANCHOR_BOTTOM: u32 = 2;
pub const ANCHOR_LEFT: u32 = 4;
pub const ANCHOR_RIGHT: u32 = 8;

pub const KEYBOARD_NONE: u32 = 0;
pub const KEYBOARD_EXCLUSIVE: u32 = 1;
pub const KEYBOARD_ON_DEMAND: u32 = 2;

/// What a message did to the panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Open,
    Resized,
    Closed,
}

#[derive(Clone, Debug)]
pub struct Spec {
    pub namespace: String,
    pub layer: u32,
    pub anchor: u32,
    /// requested size; 0 on an axis lets the compositor stretch it between the anchored edges
    pub size: (u32, u32),
    /// space other windows keep clear of: the height of a top bar, -1 for none
    pub exclusive: i32,
    /// keyboard focus: `KEYBOARD_NONE`, `KEYBOARD_EXCLUSIVE` (takes every key while shown) or `KEYBOARD_ON_DEMAND` (focus by click)
    pub keyboard: u32,
}

pub struct Panel {
    surface: u32,
    layer_surface: u32,
    frame: Frame,
}

impl Panel {
    /// Bind the three interfaces, make the surface and its layer role, commit once, and wait for the compositor's
    /// first `configure` (which says the real size); then the panel is ready for `draw`.
    pub fn open(c: &mut Conn, registry: u32, globals: &[Global], spec: &Spec) -> Result<Panel> {
        let need = |n: &str| find(globals, n).ok_or_else(|| Error::other(format!("the compositor has no {n}")));
        let compositor = bind(c, registry, need("wl_compositor")?, 4)?;
        let shm = bind(c, registry, need("wl_shm")?, 1)?;
        let shell = bind(c, registry, need("zwlr_layer_shell_v1")?, 4)?;
        let surface = c.new_id();
        c.send(compositor, 0, &[Arg::New(surface)], &[])?;
        let layer_surface = c.new_id();
        c.send(shell, 0, &[Arg::New(layer_surface), Arg::Obj(surface), Arg::Obj(0), Arg::U32(spec.layer), Arg::Str(&spec.namespace)], &[])?;
        c.send(layer_surface, 0, &[Arg::U32(spec.size.0), Arg::U32(spec.size.1)], &[])?;
        c.send(layer_surface, 1, &[Arg::U32(spec.anchor)], &[])?;
        c.send(layer_surface, 2, &[Arg::I32(spec.exclusive)], &[])?;
        if spec.keyboard != KEYBOARD_NONE {
            c.send(layer_surface, 4, &[Arg::U32(spec.keyboard)], &[])?;
        }
        c.send(surface, 6, &[], &[])?;
        loop {
            let m = c.read()?;
            if m.obj == 1 && m.opcode == 0 {
                let mut r = m.reader();
                let (obj, code, text) = (r.u32()?, r.u32()?, r.string()?);
                return Err(Error::other(format!("wayland error on object {obj} code {code}: {text}")));
            }
            if m.obj == layer_surface && m.opcode == 1 {
                return Err(Error::other("the compositor closed the panel before it was shown"));
            }
            if m.obj == layer_surface && m.opcode == 0 {
                let mut r = m.reader();
                let (serial, w, h) = (r.u32()?, r.u32()?, r.u32()?);
                c.send(layer_surface, 6, &[Arg::U32(serial)], &[])?;
                return Ok(Panel { surface, layer_surface, frame: Frame::new(surface, shm, if w > 0 { w } else { spec.size.0 }, if h > 0 { h } else { spec.size.1 }) });
            }
        }
    }

    /// A panel with no surface behind it, for tests of code that drives one over a fake connection.
    #[cfg(test)]
    pub(crate) fn fake(layer_surface: u32, width: u32, height: u32) -> Panel {
        Panel { surface: 6, layer_surface, frame: Frame::new(6, 4, width, height) }
    }

    pub fn surface(&self) -> u32 {
        self.surface
    }

    pub fn size(&self) -> (u32, u32) {
        (self.frame.width, self.frame.height)
    }

    /// Put one frame on screen: `argb` is `width * height` pixels, 0xAARRGGBB.
    pub fn draw(&mut self, c: &mut Conn, argb: &[u32]) -> Result<()> {
        self.frame.draw(c, argb)
    }

    /// React to a message of the layer surface: a new size (the buffer is dropped, the next `draw` makes one that
    /// fits) or the compositor closing the panel. Anything else is not ours and changes nothing.
    pub fn handle(&mut self, c: &mut Conn, m: &Msg) -> Result<State> {
        if m.obj == 1 && m.opcode == 0 {
            let mut r = m.reader();
            let (obj, code, text) = (r.u32()?, r.u32()?, r.string()?);
            return Err(Error::other(format!("wayland error on object {obj} code {code}: {text}")));
        }
        if m.obj != self.layer_surface {
            return Ok(State::Open);
        }
        match m.opcode {
            0 => {
                let mut r = m.reader();
                let (serial, w, h) = (r.u32()?, r.u32()?, r.u32()?);
                c.send(self.layer_surface, 6, &[Arg::U32(serial)], &[])?;
                let (cw, ch) = self.size();
                if (w > 0 && w != cw) || (h > 0 && h != ch) {
                    self.frame.resize(c, if w > 0 { w } else { cw }, if h > 0 { h } else { ch })?;
                    return Ok(State::Resized);
                }
                Ok(State::Open)
            }
            1 => Ok(State::Closed),
            _ => Ok(State::Open),
        }
    }

    /// Take the panel down: destroy the layer role, then the surface.
    pub fn close(self, c: &mut Conn) -> Result<()> {
        c.send(self.layer_surface, 7, &[], &[])?;
        c.send(self.surface, 0, &[], &[])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wl::wire::encode;
    use std::io::Write;
    use std::os::unix::net::UnixStream;

    fn globals() -> Vec<Global> {
        ["wl_compositor", "wl_shm", "zwlr_layer_shell_v1"].iter().enumerate().map(|(i, n)| Global { name: i as u32 + 1, interface: n.to_string(), version: 5 }).collect()
    }

    fn spec() -> Spec {
        Spec { namespace: "bar".into(), layer: LAYER_TOP, anchor: ANCHOR_TOP | ANCHOR_LEFT | ANCHOR_RIGHT, size: (0, 24), exclusive: 24, keyboard: KEYBOARD_NONE }
    }

    #[test]
    fn open_sends_the_role_requests_in_order_and_acks_the_configure() {
        let (client, mut server) = UnixStream::pair().unwrap();
        let mut c = Conn::from_stream(client);
        let t = std::thread::spawn(move || {
            let mut s = Conn::from_stream(server.try_clone().unwrap());
            let mut seen = Vec::new();
            // 3 binds, create_surface, get_layer_surface, set_size, set_anchor, set_exclusive_zone, commit
            for _ in 0..9 {
                let m = s.read().unwrap();
                seen.push((m.obj, m.opcode));
            }
            // ids: registry 2 given by the test, compositor 3, shm 4, shell 5, surface 6, layer_surface 7
            server.write_all(&encode(7, 0, &[Arg::U32(77), Arg::U32(1920), Arg::U32(24)])).unwrap();
            let ack = s.read().unwrap();
            (seen, ack.obj, ack.opcode, ack.reader().u32().unwrap())
        });
        let registry = c.new_id();
        let p = Panel::open(&mut c, registry, &globals(), &spec()).unwrap();
        let (seen, ack_obj, ack_op, serial) = t.join().unwrap();
        assert_eq!(seen, vec![(2, 0), (2, 0), (2, 0), (3, 0), (5, 0), (7, 0), (7, 1), (7, 2), (6, 6)]);
        assert_eq!((ack_obj, ack_op, serial), (7, 6, 77));
        assert_eq!(p.size(), (1920, 24), "the compositor's size wins over the requested 0 x 24");
    }

    #[test]
    fn keyboard_interactivity_is_requested_only_when_asked() {
        let (client, server) = UnixStream::pair().unwrap();
        let mut c = Conn::from_stream(client);
        let mut s = Conn::from_stream(server);
        let t = std::thread::spawn(move || {
            let mut seen = Vec::new();
            for _ in 0..10 {
                let m = s.read().unwrap();
                seen.push((m.obj, m.opcode));
            }
            let mut w = s;
            w.send(7, 0, &[Arg::U32(1), Arg::U32(10), Arg::U32(10)], &[]).unwrap();
            w.read().unwrap(); // ack_configure: the socket must stay open until the client has sent it
            seen
        });
        let registry = c.new_id();
        Panel::open(&mut c, registry, &globals(), &Spec { keyboard: KEYBOARD_ON_DEMAND, ..spec() }).unwrap();
        let seen = t.join().unwrap();
        assert_eq!(seen[8], (7, 4), "set_keyboard_interactivity comes after the exclusive zone, before the commit");
        assert_eq!(seen[9], (6, 6));
    }

    #[test]
    fn a_missing_interface_is_said_by_name() {
        let (client, _server) = UnixStream::pair().unwrap();
        let mut c = Conn::from_stream(client);
        let g: Vec<Global> = globals().into_iter().filter(|g| g.interface != "zwlr_layer_shell_v1").collect();
        let e = Panel::open(&mut c, 2, &g, &spec()).err().unwrap().to_string();
        assert!(e.contains("zwlr_layer_shell_v1"), "{e}");
    }

    #[test]
    fn draw_makes_the_pool_once_and_checks_the_frame_size() {
        let (client, server) = UnixStream::pair().unwrap();
        let mut c = Conn::from_stream(client);
        let mut p = Panel::fake(7, 4, 2);
        assert!(p.draw(&mut c, &[0; 3]).is_err());
        let frame = [0xff112233u32; 8];
        p.draw(&mut c, &frame).unwrap();
        p.draw(&mut c, &frame).unwrap();
        let mut s = Conn::from_stream(server);
        let first = s.read().unwrap();
        assert_eq!((first.obj, first.opcode, first.fds.len()), (4, 0, 1), "create_pool carries the memfd");
        let mut r = first.reader();
        assert_eq!((r.u32().unwrap(), r.i32().unwrap()), (p.frame.ids().0, 32));
        let buf = s.read().unwrap();
        assert_eq!((buf.obj, buf.opcode), (p.frame.ids().0, 0));
        let ops: Vec<(u32, u16)> = (0..6).map(|_| { let m = s.read().unwrap(); (m.obj, m.opcode) }).collect();
        assert_eq!(ops, vec![(6, 1), (6, 9), (6, 6), (6, 1), (6, 9), (6, 6)], "attach, damage_buffer, commit, twice, no second pool");
        // the pixels reached the shared memory that was sent
        use std::os::fd::AsRawFd;
        let mut got = [0u32; 8];
        let n = unsafe { libc::pread(first.fds[0].as_raw_fd(), got.as_mut_ptr() as *mut _, 32, 0) };
        assert_eq!((n, got), (32, frame));
    }

    fn msg(obj: u32, opcode: u16, args: &[Arg]) -> Msg {
        Msg { obj, opcode, payload: encode(0, 0, args)[8..].to_vec(), fds: vec![] }
    }

    #[test]
    fn a_new_size_acks_and_drops_the_buffer_and_close_is_reported() {
        let (client, server) = UnixStream::pair().unwrap();
        let mut c = Conn::from_stream(client);
        let mut p = Panel::fake(7, 4, 2);
        p.draw(&mut c, &[0u32; 8]).unwrap();
        let mut s = Conn::from_stream(server);
        for _ in 0..5 {
            s.read().unwrap();
        }
        assert_eq!(p.handle(&mut c, &msg(7, 0, &[Arg::U32(9), Arg::U32(4), Arg::U32(2)])).unwrap(), State::Open, "same size");
        assert_eq!(p.handle(&mut c, &msg(7, 0, &[Arg::U32(10), Arg::U32(8), Arg::U32(3)])).unwrap(), State::Resized);
        assert_eq!(p.size(), (8, 3));
        let ops: Vec<(u32, u16)> = (0..4).map(|_| { let m = s.read().unwrap(); (m.obj, m.opcode) }).collect();
        assert_eq!(ops, vec![(7, 6), (7, 6), (p.frame.ids().1, 0), (p.frame.ids().0, 1)], "two acks, then buffer and pool destroyed");
        assert!(!p.frame.ids().2);
        assert_eq!(p.handle(&mut c, &msg(7, 1, &[])).unwrap(), State::Closed);
        assert_eq!(p.handle(&mut c, &msg(55, 0, &[])).unwrap(), State::Open, "not our object");
        let e = p.handle(&mut c, &msg(1, 0, &[Arg::U32(7), Arg::U32(2), Arg::Str("bad")])).unwrap_err().to_string();
        assert!(e.contains("code 2: bad"), "{e}");
    }

    /// Needs a running compositor with layer-shell: shows a 240 x 16 strip at the bottom-left for one second, taking
    /// no screen space (`cargo test --lib wl::panel -- --ignored --nocapture`).
    #[test]
    #[ignore]
    fn live_panel_shows_for_one_second() {
        let mut c = crate::wl::wire::connect().unwrap();
        let (reg, g) = crate::wl::registry(&mut c).unwrap();
        let spec = Spec { namespace: "lazyui-test".into(), layer: LAYER_TOP, anchor: ANCHOR_BOTTOM | ANCHOR_LEFT, size: (240, 16), exclusive: 0, keyboard: KEYBOARD_NONE };
        let mut p = Panel::open(&mut c, reg, &g, &spec).unwrap();
        println!("configured {:?}", p.size());
        let frame = vec![0xff1e8c3au32; (p.size().0 * p.size().1) as usize];
        p.draw(&mut c, &frame).unwrap();
        std::thread::sleep(std::time::Duration::from_secs(1));
        p.close(&mut c).unwrap();
        // a roundtrip after the destroy: the compositor answers with no error if all requests were valid
        let (_, again) = crate::wl::registry(&mut c).unwrap();
        assert!(!again.is_empty());
    }
}
