//! Wayland wire format: every message is `object u32, (size << 16 | opcode) u32, arguments`, all native endian,
//! arguments padded to 4 bytes; a string is `len+1 u32`, the bytes, a NUL, padding. File descriptors travel
//! out of band (`SCM_RIGHTS`) with the message that owns them.
use std::io::{Error, ErrorKind, Result};
use std::os::fd::{AsRawFd, OwnedFd, RawFd};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;

pub enum Arg<'a> {
    U32(u32),
    I32(i32),
    Str(&'a str),
    New(u32),
    Obj(u32),
    /// takes a place in the message stream only through the ancillary data, not in the bytes
    Fd,
}

#[derive(Debug)]
pub struct Msg {
    pub obj: u32,
    pub opcode: u16,
    pub payload: Vec<u8>,
    pub fds: Vec<OwnedFd>,
}

pub struct Reader<'a>(&'a [u8]);

impl Msg {
    pub fn reader(&self) -> Reader<'_> {
        Reader(&self.payload)
    }
}

fn short() -> Error {
    Error::new(ErrorKind::UnexpectedEof, "wayland message too short")
}

impl Reader<'_> {
    pub fn u32(&mut self) -> Result<u32> {
        let (h, rest) = self.0.split_first_chunk::<4>().ok_or_else(short)?;
        self.0 = rest;
        Ok(u32::from_ne_bytes(*h))
    }
    pub fn i32(&mut self) -> Result<i32> {
        Ok(self.u32()? as i32)
    }
    pub fn string(&mut self) -> Result<String> {
        let len = self.u32()? as usize;
        if len == 0 {
            return Ok(String::new());
        }
        let padded = len.div_ceil(4) * 4;
        if self.0.len() < padded {
            return Err(short());
        }
        let s = String::from_utf8_lossy(&self.0[..len - 1]).into_owned();
        self.0 = &self.0[padded..];
        Ok(s)
    }
}

/// The bytes of one message. A `Fd` argument adds nothing here.
pub fn encode(obj: u32, opcode: u16, args: &[Arg]) -> Vec<u8> {
    let mut body = Vec::new();
    for a in args {
        match a {
            Arg::U32(v) | Arg::New(v) | Arg::Obj(v) => body.extend_from_slice(&v.to_ne_bytes()),
            Arg::I32(v) => body.extend_from_slice(&v.to_ne_bytes()),
            Arg::Str(s) => {
                body.extend_from_slice(&(s.len() as u32 + 1).to_ne_bytes());
                body.extend_from_slice(s.as_bytes());
                body.push(0);
                while body.len() % 4 != 0 {
                    body.push(0);
                }
            }
            Arg::Fd => {}
        }
    }
    let mut out = Vec::with_capacity(8 + body.len());
    out.extend_from_slice(&obj.to_ne_bytes());
    out.extend_from_slice(&(((8 + body.len()) as u32) << 16 | opcode as u32).to_ne_bytes());
    out.extend_from_slice(&body);
    out
}

pub struct Conn {
    sock: UnixStream,
    next: u32,
    buf: Vec<u8>,
    fds: Vec<OwnedFd>,
}

/// `$WAYLAND_DISPLAY` (default `wayland-0`) in `$XDG_RUNTIME_DIR`, or an absolute path.
pub fn socket_path() -> Option<PathBuf> {
    let name = std::env::var_os("WAYLAND_DISPLAY").filter(|v| !v.is_empty()).unwrap_or_else(|| "wayland-0".into());
    let p = PathBuf::from(&name);
    if p.is_absolute() {
        return Some(p);
    }
    Some(PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR")?).join(p))
}

pub fn connect() -> Result<Conn> {
    let p = socket_path().ok_or_else(|| Error::other("no XDG_RUNTIME_DIR"))?;
    Ok(Conn::from_stream(UnixStream::connect(p)?))
}

impl Conn {
    pub fn from_stream(sock: UnixStream) -> Conn {
        // id 1 is wl_display; ours start at 2
        Conn { sock, next: 2, buf: Vec::new(), fds: Vec::new() }
    }

    pub fn new_id(&mut self) -> u32 {
        self.next += 1;
        self.next - 1
    }

    /// Send one request. `fds` go with it as ancillary data (the shared-memory pool of `wl_shm.create_pool`).
    pub fn send(&mut self, obj: u32, opcode: u16, args: &[Arg], fds: &[RawFd]) -> Result<()> {
        let bytes = encode(obj, opcode, args);
        let mut iov = libc::iovec { iov_base: bytes.as_ptr() as *mut _, iov_len: bytes.len() };
        let mut cmsg = vec![0u8; unsafe { libc::CMSG_SPACE((fds.len() * 4) as u32) } as usize];
        let mut hdr: libc::msghdr = unsafe { std::mem::zeroed() };
        hdr.msg_iov = &mut iov;
        hdr.msg_iovlen = 1;
        if !fds.is_empty() {
            hdr.msg_control = cmsg.as_mut_ptr() as *mut _;
            hdr.msg_controllen = cmsg.len() as _;
            unsafe {
                let c = libc::CMSG_FIRSTHDR(&hdr);
                (*c).cmsg_level = libc::SOL_SOCKET;
                (*c).cmsg_type = libc::SCM_RIGHTS;
                (*c).cmsg_len = libc::CMSG_LEN((fds.len() * 4) as u32) as _;
                std::ptr::copy_nonoverlapping(fds.as_ptr() as *const u8, libc::CMSG_DATA(c), fds.len() * 4);
            }
        }
        let n = unsafe { libc::sendmsg(self.sock.as_raw_fd(), &hdr, libc::MSG_NOSIGNAL) };
        if n < 0 {
            return Err(Error::last_os_error());
        }
        if n as usize != bytes.len() {
            return Err(Error::new(ErrorKind::WriteZero, "short write to the compositor"));
        }
        Ok(())
    }

    fn fill(&mut self) -> Result<()> {
        let mut data = [0u8; 4096];
        let mut iov = libc::iovec { iov_base: data.as_mut_ptr() as *mut _, iov_len: data.len() };
        let mut cmsg = vec![0u8; unsafe { libc::CMSG_SPACE(16 * 4) } as usize];
        let mut hdr: libc::msghdr = unsafe { std::mem::zeroed() };
        hdr.msg_iov = &mut iov;
        hdr.msg_iovlen = 1;
        hdr.msg_control = cmsg.as_mut_ptr() as *mut _;
        hdr.msg_controllen = cmsg.len() as _;
        let n = unsafe { libc::recvmsg(self.sock.as_raw_fd(), &mut hdr, libc::MSG_CMSG_CLOEXEC) };
        if n < 0 {
            return Err(Error::last_os_error());
        }
        if n == 0 {
            return Err(Error::new(ErrorKind::UnexpectedEof, "the compositor closed the connection"));
        }
        unsafe {
            let mut c = libc::CMSG_FIRSTHDR(&hdr);
            while !c.is_null() {
                if (*c).cmsg_level == libc::SOL_SOCKET && (*c).cmsg_type == libc::SCM_RIGHTS {
                    let count = ((*c).cmsg_len as usize - libc::CMSG_LEN(0) as usize) / 4;
                    let p = libc::CMSG_DATA(c) as *const RawFd;
                    for i in 0..count {
                        use std::os::fd::FromRawFd;
                        self.fds.push(OwnedFd::from_raw_fd(*p.add(i)));
                    }
                }
                c = libc::CMSG_NXTHDR(&hdr, c);
            }
        }
        self.buf.extend_from_slice(&data[..n as usize]);
        Ok(())
    }

    /// The next message, blocking. Descriptors received so far belong to the messages in order; they are handed to the
    /// message that follows them, which is exact for the one event that carries one (the keyboard keymap).
    pub fn read(&mut self) -> Result<Msg> {
        loop {
            if self.buf.len() >= 8 {
                let obj = u32::from_ne_bytes(self.buf[0..4].try_into().unwrap());
                let w = u32::from_ne_bytes(self.buf[4..8].try_into().unwrap());
                let (size, opcode) = ((w >> 16) as usize, (w & 0xffff) as u16);
                if size < 8 || size % 4 != 0 {
                    return Err(Error::new(ErrorKind::InvalidData, format!("bad message size {size}")));
                }
                if self.buf.len() >= size {
                    let payload = self.buf[8..size].to_vec();
                    self.buf.drain(..size);
                    return Ok(Msg { obj, opcode, payload, fds: std::mem::take(&mut self.fds) });
                }
            }
            self.fill()?;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn string_is_padded_and_length_counts_the_nul() {
        let b = encode(5, 3, &[Arg::Str("wl_shm"), Arg::U32(1)]);
        // header 8 + len 4 + "wl_shm\0" padded to 8 + u32 4
        assert_eq!(b.len(), 24);
        assert_eq!(u32::from_ne_bytes(b[4..8].try_into().unwrap()), (24 << 16) | 3);
        assert_eq!(u32::from_ne_bytes(b[8..12].try_into().unwrap()), 7);
        assert_eq!(&b[12..19], b"wl_shm\0");
    }

    #[test]
    fn reader_decodes_what_encode_wrote() {
        let b = encode(9, 0, &[Arg::U32(42), Arg::Str("zwlr_layer_shell_v1"), Arg::I32(-3)]);
        let m = Msg { obj: 9, opcode: 0, payload: b[8..].to_vec(), fds: vec![] };
        let mut r = m.reader();
        assert_eq!((r.u32().unwrap(), r.string().unwrap().as_str(), r.i32().unwrap()), (42, "zwlr_layer_shell_v1", -3));
        assert!(r.u32().is_err(), "reading past the end is an error, not a panic");
    }

    #[test]
    fn messages_split_across_reads_and_descriptors_arrive() {
        let (a, b) = UnixStream::pair().unwrap();
        let mut server = a;
        let mut c = Conn::from_stream(b);
        let msg = encode(7, 1, &[Arg::U32(5)]);
        server.write_all(&msg[..5]).unwrap();
        let t = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(50));
            server.write_all(&msg[5..]).unwrap();
            server
        });
        let m = c.read().unwrap();
        assert_eq!((m.obj, m.opcode, m.reader().u32().unwrap()), (7, 1, 5));
        let _server = t.join().unwrap();
    }

    #[test]
    fn a_descriptor_travels_with_its_request() {
        let (a, b) = UnixStream::pair().unwrap();
        let mut sender = Conn::from_stream(a);
        let mut receiver = Conn::from_stream(b);
        let f = std::fs::File::open("/proc/self/exe").unwrap();
        sender.send(4, 0, &[Arg::U32(1), Arg::Fd], &[f.as_raw_fd()]).unwrap();
        let m = receiver.read().unwrap();
        assert_eq!(m.fds.len(), 1);
        assert!(m.fds[0].as_raw_fd() >= 0 && m.fds[0].as_raw_fd() != f.as_raw_fd());
    }

    #[test]
    fn a_closed_connection_is_an_error() {
        let (a, b) = UnixStream::pair().unwrap();
        drop(a);
        assert_eq!(Conn::from_stream(b).read().unwrap_err().kind(), ErrorKind::UnexpectedEof);
    }
}
