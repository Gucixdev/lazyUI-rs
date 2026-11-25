//! A surface's pixels through shared memory: one `wl_shm` pool and buffer of the surface's size, made on the first
//! draw, dropped when the size changes. Shared by layer-shell panels and xdg windows.
use super::wire::{Arg, Conn};
use std::io::{Error, Result};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

const FORMAT_ARGB8888: u32 = 0;

pub struct Frame {
    pub surface: u32,
    shm: u32,
    pub width: u32,
    pub height: u32,
    /// buffer scale: the buffer is `width * scale` by `height * scale` pixels for a surface of `width` by `height`
    pub scale: u32,
    map: Option<(*mut u8, usize)>,
    pool: u32,
    buffer: u32,
    _fd: Option<OwnedFd>,
}

impl Frame {
    pub fn new(surface: u32, shm: u32, width: u32, height: u32) -> Frame {
        Frame { surface, shm, width, height, scale: 1, map: None, pool: 0, buffer: 0, _fd: None }
    }

    /// Put one frame on screen: `argb` is `width * height` pixels, 0xAARRGGBB. Makes the pool on the first call; the
    /// buffer is reused, so call again only after the compositor released it (or for a single frame).
    pub fn draw(&mut self, c: &mut Conn, argb: &[u32]) -> Result<()> {
        let (w, h) = ((self.width * self.scale) as usize, (self.height * self.scale) as usize);
        if argb.len() != w * h {
            return Err(Error::other(format!("frame has {} pixels, the surface needs {}", argb.len(), w * h)));
        }
        let bytes = w * h * 4;
        if self.map.is_none() {
            let fd = unsafe { libc::memfd_create(c"lazyui-frame".as_ptr(), libc::MFD_CLOEXEC) };
            if fd < 0 {
                return Err(Error::last_os_error());
            }
            let fd = unsafe { OwnedFd::from_raw_fd(fd) };
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

    /// A new output scale: the compositor is told, the buffer is dropped so the next `draw` makes one of the new size.
    pub fn set_scale(&mut self, c: &mut Conn, scale: u32) -> Result<()> {
        let scale = scale.max(1);
        if scale == self.scale {
            return Ok(());
        }
        self.scale = scale;
        c.send(self.surface, 8, &[Arg::I32(scale as i32)], &[])?;
        self.resize(c, self.width, self.height)
    }

    /// A new size: the buffer and pool are destroyed, the next `draw` makes ones that fit.
    pub fn resize(&mut self, c: &mut Conn, width: u32, height: u32) -> Result<()> {
        self.width = width;
        self.height = height;
        if let Some((p, len)) = self.map.take() {
            c.send(self.buffer, 0, &[], &[])?;
            c.send(self.pool, 1, &[], &[])?;
            unsafe { libc::munmap(p as *mut _, len) };
            self._fd = None;
        }
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn ids(&self) -> (u32, u32, bool) {
        (self.pool, self.buffer, self.map.is_some())
    }
}

impl Drop for Frame {
    fn drop(&mut self) {
        if let Some((p, len)) = self.map.take() {
            unsafe { libc::munmap(p as *mut _, len) };
        }
    }
}
