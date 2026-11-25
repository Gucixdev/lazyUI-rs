//! Drag and drop with other programs, through `wl_data_device_manager`. Out: `start` hands the compositor a drag whose
//! data is a `text/uri-list` (paths become `file://` URIs) and the same text as `text/plain`; the drop target asks for
//! the bytes with `send`, which writes them to the pipe it gave. In: an offer that has `text/uri-list` is accepted
//! while the pointer is over the surface; on `drop` the list is read from a pipe (at most half a second of waiting)
//! and handed out as `Drop` with the paths and the pointer position in surface units.
use super::wire::{Arg, Conn, Msg};
use super::{find, Global};
use std::io::{Error, Read, Result};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

const URI_LIST: &str = "text/uri-list";
const PLAIN: &str = "text/plain;charset=utf-8";
const ACTION_COPY: u32 = 1;
const ACTION_MOVE: u32 = 2;

/// Something dropped on the surface from another program.
#[derive(Clone, Debug, PartialEq)]
pub struct Drop {
    pub paths: Vec<String>,
    pub x: f64,
    pub y: f64,
}

struct Offer {
    id: u32,
    uri_list: bool,
}

pub struct Dnd {
    manager: u32,
    device: u32,
    /// data of the drag we started: the source object and the bytes to write for each mime type
    source: Option<(u32, Vec<u8>)>,
    offers: Vec<Offer>,
    /// the offer the pointer is over, with the last position and the enter serial
    over: Option<(u32, u32, f64, f64)>,
    version: u32,
}

/// `file:///a%20b` for `/a b`: everything but unreserved characters and `/` is percent-encoded.
pub fn to_uri(path: &str) -> String {
    let mut s = String::from("file://");
    for b in path.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => s.push(b as char),
            _ => s.push_str(&format!("%{b:02X}")),
        }
    }
    s
}

/// The path of a `file://` URI (percent-decoded); None for any other line, comments (`#`) included.
pub fn from_uri(line: &str) -> Option<String> {
    let rest = line.trim().strip_prefix("file://")?;
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    let b = rest.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() + 0 && b.len() >= i + 3 {
            if let Ok(v) = u8::from_str_radix(&rest[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8(out).ok()
}

impl Dnd {
    /// None when the compositor has no data device manager (then there is simply no drag to other programs).
    pub fn bind(c: &mut Conn, registry: u32, globals: &[Global], seat: u32) -> Result<Option<Dnd>> {
        let Some(g) = find(globals, "wl_data_device_manager") else { return Ok(None) };
        let version = g.version.min(3);
        let manager = super::bind(c, registry, g, version)?;
        let device = c.new_id();
        c.send(manager, 1, &[Arg::New(device), Arg::Obj(seat)], &[])?;
        Ok(Some(Dnd { manager, device, source: None, offers: Vec::new(), over: None, version }))
    }

    /// Start a drag of `payload` (paths, one per line) from `surface`; `serial` is the one of the button press.
    pub fn start(&mut self, c: &mut Conn, surface: u32, serial: u32, payload: &str) -> Result<()> {
        let lines: Vec<&str> = payload.lines().filter(|l| !l.is_empty()).collect();
        if lines.is_empty() {
            return Ok(());
        }
        let uris: String = lines.iter().map(|l| if l.starts_with('/') { to_uri(l) } else { l.to_string() }).map(|u| u + "\r\n").collect();
        let src = c.new_id();
        c.send(self.manager, 0, &[Arg::New(src)], &[])?;
        c.send(src, 0, &[Arg::Str(URI_LIST)], &[])?;
        c.send(src, 0, &[Arg::Str(PLAIN)], &[])?;
        if self.version >= 3 {
            c.send(src, 2, &[Arg::U32(ACTION_COPY | ACTION_MOVE)], &[])?;
        }
        c.send(self.device, 0, &[Arg::Obj(src), Arg::Obj(surface), Arg::Obj(0), Arg::U32(serial)], &[])?;
        self.source = Some((src, uris.into_bytes()));
        Ok(())
    }

    pub fn dragging(&self) -> bool {
        self.source.is_some()
    }

    pub fn owns(&self, obj: u32) -> bool {
        obj == self.device || self.source.as_ref().is_some_and(|s| s.0 == obj) || self.offers.iter().any(|o| o.id == obj)
    }

    /// Feed one message; a `Drop` when one was completed.
    pub fn handle(&mut self, c: &mut Conn, m: &Msg) -> Result<Option<Drop>> {
        let mut r = m.reader();
        if self.source.as_ref().is_some_and(|s| s.0 == m.obj) {
            let (src, data) = self.source.clone().unwrap();
            match m.opcode {
                // send(mime, fd)
                1 => {
                    let _mime = r.string()?;
                    if let Some(fd) = m.fds.first() {
                        let mut f = unsafe { std::fs::File::from_raw_fd(libc::dup(fd.as_raw_fd())) };
                        std::io::Write::write_all(&mut f, &data)?;
                    }
                }
                // cancelled, dnd_finished: the drag is over
                2 | 4 => {
                    c.send(src, 1, &[], &[])?;
                    self.source = None;
                }
                _ => {}
            }
            return Ok(None);
        }
        if let Some(i) = self.offers.iter().position(|o| o.id == m.obj) {
            // offer(mime)
            if m.opcode == 0 && r.string()? == URI_LIST {
                self.offers[i].uri_list = true;
            }
            return Ok(None);
        }
        if m.obj != self.device {
            return Ok(None);
        }
        match m.opcode {
            // data_offer(new id)
            0 => self.offers.push(Offer { id: r.u32()?, uri_list: false }),
            // enter(serial, surface, x, y, offer)
            1 => {
                let (serial, _, x, y, offer) = (r.u32()?, r.u32()?, r.fixed()?, r.fixed()?, r.u32()?);
                match self.offers.iter().find(|o| o.id == offer) {
                    Some(o) if o.uri_list => {
                        c.send(offer, 0, &[Arg::U32(serial), Arg::Str(URI_LIST)], &[])?;
                        if self.version >= 3 {
                            c.send(offer, 4, &[Arg::U32(ACTION_COPY), Arg::U32(ACTION_COPY)], &[])?;
                        }
                        self.over = Some((offer, serial, x, y));
                    }
                    _ => self.over = None,
                }
            }
            // leave
            2 => {
                if let Some((o, ..)) = self.over.take() {
                    self.destroy_offer(c, o)?;
                }
            }
            // motion(time, x, y)
            3 => {
                let (_, x, y) = (r.u32()?, r.fixed()?, r.fixed()?);
                if let Some(o) = self.over.as_mut() {
                    o.2 = x;
                    o.3 = y;
                }
            }
            // drop
            4 => {
                let Some((offer, _, x, y)) = self.over.take() else { return Ok(None) };
                let text = receive(c, offer)?;
                if self.version >= 3 {
                    c.send(offer, 3, &[], &[])?;
                }
                self.destroy_offer(c, offer)?;
                let paths: Vec<String> = text.lines().filter_map(from_uri).collect();
                return Ok((!paths.is_empty()).then_some(Drop { paths, x, y }));
            }
            // selection(offer): the clipboard, not ours
            5 => {
                let id = r.u32()?;
                if id != 0 {
                    self.destroy_offer(c, id)?;
                }
            }
            _ => {}
        }
        Ok(None)
    }

    fn destroy_offer(&mut self, c: &mut Conn, id: u32) -> Result<()> {
        if let Some(i) = self.offers.iter().position(|o| o.id == id) {
            self.offers.remove(i);
            c.send(id, 2, &[], &[])?;
        }
        Ok(())
    }
}

/// `wl_data_offer.receive` through a pipe; waits at most half a second for the sender to finish.
fn receive(c: &mut Conn, offer: u32) -> Result<String> {
    let mut fds = [0i32; 2];
    if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) } != 0 {
        return Err(Error::last_os_error());
    }
    let (rd, wr) = unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) };
    c.send(offer, 1, &[Arg::Str(URI_LIST), Arg::Fd], &[wr.as_raw_fd()])?;
    drop(wr);
    let mut f = std::fs::File::from(rd);
    let mut out = Vec::new();
    let mut buf = [0u8; 4096];
    let start = std::time::Instant::now();
    loop {
        let left = 500i64 - start.elapsed().as_millis() as i64;
        let mut p = libc::pollfd { fd: f.as_raw_fd(), events: libc::POLLIN, revents: 0 };
        if left <= 0 || unsafe { libc::poll(&mut p, 1, left as i32) } <= 0 {
            break;
        }
        match f.read(&mut buf)? {
            0 => break,
            n => out.extend_from_slice(&buf[..n]),
        }
        if out.len() > 1 << 20 {
            break;
        }
    }
    Ok(String::from_utf8_lossy(&out).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wl::wire::encode;
    use std::io::Write;
    use std::os::unix::net::UnixStream;

    fn pair() -> (Conn, Conn) {
        let (a, b) = UnixStream::pair().unwrap();
        (Conn::from_stream(a), Conn::from_stream(b))
    }

    fn msg(obj: u32, opcode: u16, args: &[Arg]) -> Msg {
        Msg { obj, opcode, payload: encode(0, 0, args)[8..].to_vec(), fds: vec![] }
    }

    fn fx(v: f64) -> Arg<'static> {
        Arg::I32((v * 256.0) as i32)
    }

    fn ready(c: &mut Conn, s: &mut Conn) -> Dnd {
        let g = vec![Global { name: 9, interface: "wl_data_device_manager".into(), version: 3 }];
        let d = Dnd::bind(c, 2, &g, 5).unwrap().unwrap();
        assert_eq!(s.read().unwrap().opcode, 0, "bind");
        let get = s.read().unwrap();
        assert_eq!((get.obj, get.opcode), (d.manager, 1), "get_data_device");
        d
    }

    #[test]
    fn paths_become_uris_and_back() {
        assert_eq!(to_uri("/home/a b/ż.txt"), "file:///home/a%20b/%C5%BC.txt");
        assert_eq!(from_uri("file:///home/a%20b/%C5%BC.txt\r"), Some("/home/a b/ż.txt".into()));
        assert_eq!(from_uri("file://localhost/x"), Some("/x".into()));
        assert_eq!(from_uri("# comment"), None);
        assert_eq!(from_uri("https://x/y"), None);
        assert_eq!(from_uri("file:///100%"), Some("/100%".into()), "a stray percent stays");
    }

    #[test]
    fn no_manager_means_no_dnd() {
        let (mut c, _s) = pair();
        assert!(Dnd::bind(&mut c, 2, &[], 5).unwrap().is_none());
    }

    #[test]
    fn a_drag_out_offers_two_types_starts_with_the_serial_and_writes_the_list_on_send() {
        let (mut c, mut s) = pair();
        let mut d = ready(&mut c, &mut s);
        d.start(&mut c, 6, 77, "/a/b c\n/a/d").unwrap();
        let create = s.read().unwrap();
        assert_eq!((create.obj, create.opcode), (d.manager, 0));
        let src = create.reader().u32().unwrap();
        let (m1, m2, acts) = (s.read().unwrap(), s.read().unwrap(), s.read().unwrap());
        assert_eq!((m1.obj, m1.opcode, m1.reader().string().unwrap()), (src, 0, URI_LIST.to_string()));
        assert_eq!(m2.reader().string().unwrap(), PLAIN);
        assert_eq!((acts.opcode, acts.reader().u32().unwrap()), (2, 3));
        let start = s.read().unwrap();
        let mut r = start.reader();
        assert_eq!((start.obj, start.opcode, r.u32().unwrap(), r.u32().unwrap(), r.u32().unwrap(), r.u32().unwrap()), (d.device, 0, src, 6, 0, 77));
        assert!(d.dragging() && d.owns(src));
        // the target asks for the bytes: a pipe whose read end we keep
        let mut fds = [0i32; 2];
        assert_eq!(unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) }, 0);
        let (rd, wr) = unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) };
        let mut send = msg(src, 1, &[Arg::Str(URI_LIST)]);
        send.fds = vec![wr];
        d.handle(&mut c, &send).unwrap();
        drop(send);
        let mut got = String::new();
        std::fs::File::from(rd).read_to_string(&mut got).unwrap();
        assert_eq!(got, "file:///a/b%20c\r\nfile:///a/d\r\n");
        d.handle(&mut c, &msg(src, 4, &[])).unwrap();
        assert!(!d.dragging());
        let destroy = s.read().unwrap();
        assert_eq!((destroy.obj, destroy.opcode), (src, 1));
    }

    #[test]
    fn a_drop_in_accepts_the_uri_list_reads_it_and_finishes() {
        let (mut c, mut s) = pair();
        let mut d = ready(&mut c, &mut s);
        let dev = d.device;
        d.handle(&mut c, &msg(dev, 0, &[Arg::U32(30)])).unwrap();
        d.handle(&mut c, &msg(30, 0, &[Arg::Str(URI_LIST)])).unwrap();
        d.handle(&mut c, &msg(dev, 1, &[Arg::U32(5), Arg::U32(6), fx(10.5), fx(20.0), Arg::Obj(30)])).unwrap();
        let accept = s.read().unwrap();
        let mut r = accept.reader();
        assert_eq!((accept.obj, accept.opcode, r.u32().unwrap(), r.string().unwrap()), (30, 0, 5, URI_LIST.to_string()));
        assert_eq!(s.read().unwrap().opcode, 4, "set_actions");
        d.handle(&mut c, &msg(dev, 3, &[Arg::U32(0), fx(12.0), fx(22.0)])).unwrap();
        // the "other program": answer the receive request from the far end after the drop message
        let t = std::thread::spawn(move || {
            let recv = s.read().unwrap();
            assert_eq!((recv.obj, recv.opcode), (30, 1));
            let mut f = std::fs::File::from(recv.fds.into_iter().next().unwrap());
            f.write_all(b"file:///x/a%20b\r\n# c\r\nfile:///x/c\r\n").unwrap();
            drop(f);
            let fin = s.read().unwrap();
            let gone = s.read().unwrap();
            (fin.opcode, gone.opcode)
        });
        let dropped = d.handle(&mut c, &msg(dev, 4, &[])).unwrap().unwrap();
        assert_eq!(dropped, Drop { paths: vec!["/x/a b".into(), "/x/c".into()], x: 12.0, y: 22.0 });
        assert_eq!(t.join().unwrap(), (3, 2), "finish, then destroy");
    }

    #[test]
    fn an_offer_without_the_uri_list_is_not_accepted() {
        let (mut c, mut s) = pair();
        let mut d = ready(&mut c, &mut s);
        let dev = d.device;
        d.handle(&mut c, &msg(dev, 0, &[Arg::U32(31)])).unwrap();
        d.handle(&mut c, &msg(31, 0, &[Arg::Str("text/plain")])).unwrap();
        d.handle(&mut c, &msg(dev, 1, &[Arg::U32(5), Arg::U32(6), fx(1.0), fx(1.0), Arg::Obj(31)])).unwrap();
        assert_eq!(d.handle(&mut c, &msg(dev, 4, &[])).unwrap(), None);
        c.send(1, 0, &[], &[]).unwrap();
        assert_eq!(s.read().unwrap().obj, 1, "nothing was sent for the offer, the next message is ours");
    }
}
