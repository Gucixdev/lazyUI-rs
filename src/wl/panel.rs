//! A layer-shell panel: a surface the compositor places at a screen edge (bar, launcher, overlay), pixels through
//! shared memory. Protocols: wl_compositor, wl_shm, zwlr_layer_shell_v1 (wlr-layer-shell-unstable-v1).
use super::wire::{Arg, Conn};
use super::{find, Global};
use std::io::{Error, Result};
use std::os::fd::{FromRawFd, OwnedFd};

pub const LAYER_BACKGROUND: u32 = 0;
pub const LAYER_BOTTOM: u32 = 1;
pub const LAYER_TOP: u32 = 2;
pub const LAYER_OVERLAY: u32 = 3;
pub const ANCHOR_TOP: u32 = 1;
pub const ANCHOR_BOTTOM: u32 = 2;
pub const ANCHOR_LEFT: u32 = 4;
pub const ANCHOR_RIGHT: u32 = 8;

const FORMAT_ARGB8888: u32 = 0;

#[derive(Clone, Debug)]
pub struct Spec {
    pub namespace: String,
    pub layer: u32,
    pub anchor: u32,
    /// requested size; 0 on an axis lets the compositor stretch it between the anchored edges
    pub size: (u32, u32),
    /// space other windows keep clear of: the height of a top bar, -1 for none
    pub exclusive: i32,
}

pub struct Panel {
    surface: u32,
    layer_surface: u32,
    shm: u32,
    pub width: u32,
    pub height: u32,
    map: Option<(*mut u8, usize)>,
    pool: u32,
    buffer: u32,
    _fd: Option<OwnedFd>,
}

fn bind(c: &mut Conn, registry: u32, g: &Global, version: u32) -> Result<u32> {
    let id = c.new_id();
    c.send(registry, 0, &[Arg::U32(g.name), Arg::Str(&g.interface), Arg::U32(version.min(g.version)), Arg::New(id)], &[])?;
    Ok(id)
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
                return Ok(Panel { surface, layer_surface, shm, width: if w > 0 { w } else { spec.size.0 }, height: if h > 0 { h } else { spec.size.1 }, map: None, pool: 0, buffer: 0, _fd: None });
            }
        }
    }

    /// Put one frame on screen: `argb` is `width * height` pixels, 0xAARRGGBB. Makes the shared-memory pool on the
    /// first call; the buffer is reused, so call again only after the compositor released it (or for a single frame).
    pub fn draw(&mut self, c: &mut Conn, argb: &[u32]) -> Result<()> {
        let (w, h) = (self.width as usize, self.height as usize);
        if argb.len() != w * h {
            return Err(Error::other(format!("frame has {} pixels, the panel needs {}", argb.len(), w * h)));
        }
        let bytes = w * h * 4;
        if self.map.is_none() {
            let fd = unsafe { libc::memfd_create(c"lazyui-panel".as_ptr(), libc::MFD_CLOEXEC) };
            if fd < 0 {
                return Err(Error::last_os_error());
            }
            let fd = unsafe { OwnedFd::from_raw_fd(fd) };
            use std::os::fd::AsRawFd;
            if unsafe { libc::ftruncate(fd.as_raw_fd(), bytes as i64) } != 0 {
                return Err(Error::last_os_error());
            }
            let p = unsafe { libc::mmap(std::ptr::null_mut(), bytes, libc::PROT_READ | libc::PROT_WRITE, libc::MAP_SHARED, fd.as_raw_fd(), 0) };
            if p == libc::MAP_FAILED {
                return Err(Error::last_os_error());
            }
            self.pool = c.new_id();
            c.send(self.shm, 0, &[Arg::New(self.pool), Arg::Fd, Arg::I32(bytes as i32)], &[fd.as_raw_fd()])?;
            self.buffer = c.new_id();
            c.send(self.pool, 0, &[Arg::New(self.buffer), Arg::I32(0), Arg::I32(w as i32), Arg::I32(h as i32), Arg::I32(w as i32 * 4), Arg::U32(FORMAT_ARGB8888)], &[])?;
            self.map = Some((p as *mut u8, bytes));
            self._fd = Some(fd);
        }
        let (p, len) = self.map.unwrap();
        unsafe { std::ptr::copy_nonoverlapping(argb.as_ptr() as *const u8, p, len) };
        c.send(self.surface, 1, &[Arg::Obj(self.buffer), Arg::I32(0), Arg::I32(0)], &[])?;
        c.send(self.surface, 9, &[Arg::I32(0), Arg::I32(0), Arg::I32(w as i32), Arg::I32(h as i32)], &[])?;
        c.send(self.surface, 6, &[], &[])
    }

    /// Take the panel down: destroy the layer role, then the surface.
    pub fn close(mut self, c: &mut Conn) -> Result<()> {
        c.send(self.layer_surface, 7, &[], &[])?;
        c.send(self.surface, 0, &[], &[])?;
        if let Some((p, len)) = self.map.take() {
            unsafe { libc::munmap(p as *mut _, len) };
        }
        Ok(())
    }
}

impl Drop for Panel {
    fn drop(&mut self) {
        if let Some((p, len)) = self.map.take() {
            unsafe { libc::munmap(p as *mut _, len) };
        }
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
        Spec { namespace: "bar".into(), layer: LAYER_TOP, anchor: ANCHOR_TOP | ANCHOR_LEFT | ANCHOR_RIGHT, size: (0, 24), exclusive: 24 }
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
        assert_eq!((p.width, p.height), (1920, 24), "the compositor's size wins over the requested 0 x 24");
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
        let mut p = Panel { surface: 6, layer_surface: 7, shm: 4, width: 4, height: 2, map: None, pool: 0, buffer: 0, _fd: None };
        assert!(p.draw(&mut c, &[0; 3]).is_err());
        let frame = [0xff112233u32; 8];
        p.draw(&mut c, &frame).unwrap();
        p.draw(&mut c, &frame).unwrap();
        let mut s = Conn::from_stream(server);
        let first = s.read().unwrap();
        assert_eq!((first.obj, first.opcode, first.fds.len()), (4, 0, 1), "create_pool carries the memfd");
        let mut r = first.reader();
        assert_eq!((r.u32().unwrap(), r.i32().unwrap()), (p.pool, 32));
        let buf = s.read().unwrap();
        assert_eq!((buf.obj, buf.opcode), (p.pool, 0));
        let ops: Vec<(u32, u16)> = (0..6).map(|_| { let m = s.read().unwrap(); (m.obj, m.opcode) }).collect();
        assert_eq!(ops, vec![(6, 1), (6, 9), (6, 6), (6, 1), (6, 9), (6, 6)], "attach, damage_buffer, commit, twice, no second pool");
        // the pixels reached the shared memory that was sent
        use std::os::fd::AsRawFd;
        let mut got = [0u32; 8];
        let n = unsafe { libc::pread(first.fds[0].as_raw_fd(), got.as_mut_ptr() as *mut _, 32, 0) };
        assert_eq!((n, got), (32, frame));
    }

    /// Needs a running compositor with layer-shell: shows a 240 x 16 strip at the bottom-left for one second, taking
    /// no screen space (`cargo test --lib wl::panel -- --ignored --nocapture`).
    #[test]
    #[ignore]
    fn live_panel_shows_for_one_second() {
        let mut c = crate::wl::wire::connect().unwrap();
        let (reg, g) = crate::wl::registry(&mut c).unwrap();
        let spec = Spec { namespace: "lazyui-test".into(), layer: LAYER_TOP, anchor: ANCHOR_BOTTOM | ANCHOR_LEFT, size: (240, 16), exclusive: 0 };
        let mut p = Panel::open(&mut c, reg, &g, &spec).unwrap();
        println!("configured {}x{}", p.width, p.height);
        let frame = vec![0xff1e8c3au32; (p.width * p.height) as usize];
        p.draw(&mut c, &frame).unwrap();
        std::thread::sleep(std::time::Duration::from_secs(1));
        p.close(&mut c).unwrap();
        // a roundtrip after the destroy: the compositor answers with no error if all requests were valid
        let (_, again) = crate::wl::registry(&mut c).unwrap();
        assert!(!again.is_empty());
    }
}
