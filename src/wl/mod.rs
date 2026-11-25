//! A Wayland client written against the wire protocol, `libc` only, for panels (layer-shell) that winit cannot make.
//! `wire` is the message format and the socket (with file descriptors for shared memory); `globals` lists what the
//! compositor offers. Nothing here draws yet.
pub mod wire;

use wire::{Arg, Conn};

const DISPLAY: u32 = 1;

/// One advertised interface of the compositor.
#[derive(Clone, Debug, PartialEq)]
pub struct Global {
    pub name: u32,
    pub interface: String,
    pub version: u32,
}

/// Ask the registry what exists: `get_registry`, then a `sync` round trip so every `global` event has arrived.
pub fn globals(c: &mut Conn) -> std::io::Result<Vec<Global>> {
    let registry = c.new_id();
    c.send(DISPLAY, 1, &[Arg::New(registry)], &[])?;
    let callback = c.new_id();
    c.send(DISPLAY, 0, &[Arg::New(callback)], &[])?;
    let mut out = Vec::new();
    loop {
        let m = c.read()?;
        if m.obj == DISPLAY && m.opcode == 0 {
            let mut r = m.reader();
            let (obj, code, text) = (r.u32()?, r.u32()?, r.string()?);
            return Err(std::io::Error::other(format!("wayland error on object {obj} code {code}: {text}")));
        }
        if m.obj == registry && m.opcode == 0 {
            let mut r = m.reader();
            out.push(Global { name: r.u32()?, interface: r.string()?, version: r.u32()? });
        } else if m.obj == callback {
            return Ok(out);
        }
    }
}

pub fn find<'a>(g: &'a [Global], interface: &str) -> Option<&'a Global> {
    g.iter().find(|x| x.interface == interface)
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::os::unix::net::UnixStream;

    #[test]
    fn globals_are_collected_until_the_sync_callback() {
        let (client, mut server) = UnixStream::pair().unwrap();
        let mut c = Conn::from_stream(client);
        let t = std::thread::spawn(move || {
            let mut s = Conn::from_stream(server.try_clone().unwrap());
            let get_registry = s.read().unwrap();
            assert_eq!((get_registry.obj, get_registry.opcode), (1, 1));
            let sync = s.read().unwrap();
            assert_eq!((sync.obj, sync.opcode), (1, 0));
            let (reg, cb) = (get_registry.reader().u32().unwrap(), sync.reader().u32().unwrap());
            for (n, i, v) in [(1u32, "wl_compositor", 6u32), (2, "zwlr_layer_shell_v1", 4)] {
                server.write_all(&wire::encode(reg, 0, &[Arg::U32(n), Arg::Str(i), Arg::U32(v)])).unwrap();
            }
            server.write_all(&wire::encode(cb, 0, &[Arg::U32(0)])).unwrap();
        });
        let g = globals(&mut c).unwrap();
        t.join().unwrap();
        assert_eq!(g.len(), 2);
        assert_eq!(find(&g, "zwlr_layer_shell_v1"), Some(&Global { name: 2, interface: "zwlr_layer_shell_v1".into(), version: 4 }));
        assert!(find(&g, "wl_shm").is_none());
    }

    #[test]
    fn a_protocol_error_from_the_compositor_is_returned() {
        let (client, mut server) = UnixStream::pair().unwrap();
        let mut c = Conn::from_stream(client);
        server.write_all(&wire::encode(1, 0, &[Arg::U32(3), Arg::U32(1), Arg::Str("bad")])).unwrap();
        let e = globals(&mut c).unwrap_err().to_string();
        assert!(e.contains("object 3 code 1: bad"), "{e}");
    }

    /// Needs a running Wayland compositor: `cargo test --lib wl -- --ignored --nocapture`. Only reads the registry.
    #[test]
    #[ignore]
    fn live_compositor_lists_layer_shell() {
        let mut c = wire::connect().unwrap();
        let g = globals(&mut c).unwrap();
        for x in &g {
            println!("{} v{}", x.interface, x.version);
        }
        assert!(find(&g, "wl_shm").is_some());
    }
}
