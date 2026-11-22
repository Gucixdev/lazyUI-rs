//! The terminal: raw mode, alternate screen or inline region, mouse, bracketed paste, resize wake-up. Only `libc`.
//! Always restored: on drop, on panic (hook), on SIGTERM/SIGHUP (flag the loop checks).

use std::io;
use std::os::unix::io::RawFd;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

static WAKE_FD: AtomicI32 = AtomicI32::new(-1);
static WINCH: AtomicBool = AtomicBool::new(false);
static TERMINATE: AtomicBool = AtomicBool::new(false);

extern "C" fn on_winch(_: libc::c_int) {
    WINCH.store(true, Ordering::SeqCst);
    wake_raw();
}
extern "C" fn on_term(_: libc::c_int) {
    TERMINATE.store(true, Ordering::SeqCst);
    wake_raw();
}

fn wake_raw() {
    let fd = WAKE_FD.load(Ordering::SeqCst);
    if fd >= 0 {
        let b = 1u8;
        unsafe {
            libc::write(fd, &b as *const u8 as *const libc::c_void, 1);
        }
    }
}

/// Wakes the loop from any thread (effects, signal handlers). Cheap to clone.
#[derive(Clone, Copy)]
pub struct Waker(RawFd);

impl Waker {
    pub(crate) fn from_fd(fd: RawFd) -> Waker {
        Waker(fd)
    }
    pub fn wake(&self) {
        let b = 1u8;
        unsafe {
            libc::write(self.0, &b as *const u8 as *const libc::c_void, 1);
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    /// alternate screen, whole terminal
    Full,
    /// the last `n` rows below the shell prompt, scrollback kept
    Inline(i32),
}

pub struct Term {
    fd: RawFd,
    owned: bool,
    saved: libc::termios,
    mode: Mode,
    mouse: bool,
    wake_r: RawFd,
    wake_w: RawFd,
    restored: bool,
    pub inline_rows: i32,
}

fn tty_fd() -> io::Result<(RawFd, bool)> {
    let path = b"/dev/tty\0";
    let fd = unsafe { libc::open(path.as_ptr() as *const libc::c_char, libc::O_RDWR) };
    if fd >= 0 {
        return Ok((fd, true));
    }
    if unsafe { libc::isatty(0) } == 1 {
        return Ok((0, false));
    }
    Err(io::Error::new(io::ErrorKind::NotConnected, "no terminal: /dev/tty cannot be opened and stdin is not a tty"))
}

impl Term {
    pub fn open(mode: Mode, mouse: bool) -> io::Result<Term> {
        let (fd, owned) = tty_fd()?;
        let mut saved: libc::termios = unsafe { std::mem::zeroed() };
        if unsafe { libc::tcgetattr(fd, &mut saved) } != 0 {
            return Err(io::Error::last_os_error());
        }
        let mut raw = saved;
        unsafe { libc::cfmakeraw(&mut raw) };
        raw.c_cc[libc::VMIN] = 1;
        raw.c_cc[libc::VTIME] = 0;
        if unsafe { libc::tcsetattr(fd, libc::TCSAFLUSH, &raw) } != 0 {
            return Err(io::Error::last_os_error());
        }
        let mut p = [0 as libc::c_int; 2];
        if unsafe { libc::pipe2(p.as_mut_ptr(), libc::O_NONBLOCK | libc::O_CLOEXEC) } != 0 {
            let e = io::Error::last_os_error();
            unsafe { libc::tcsetattr(fd, libc::TCSAFLUSH, &saved) };
            return Err(e);
        }
        WAKE_FD.store(p[1], Ordering::SeqCst);
        WINCH.store(false, Ordering::SeqCst);
        TERMINATE.store(false, Ordering::SeqCst);
        unsafe {
            let mut sa: libc::sigaction = std::mem::zeroed();
            sa.sa_sigaction = on_winch as extern "C" fn(libc::c_int) as usize;
            libc::sigemptyset(&mut sa.sa_mask);
            libc::sigaction(libc::SIGWINCH, &sa, std::ptr::null_mut());
            sa.sa_sigaction = on_term as extern "C" fn(libc::c_int) as usize;
            libc::sigaction(libc::SIGTERM, &sa, std::ptr::null_mut());
            libc::sigaction(libc::SIGHUP, &sa, std::ptr::null_mut());
        }
        let mut t = Term { fd, owned, saved, mode, mouse, wake_r: p[0], wake_w: p[1], restored: false, inline_rows: 0 };
        // the terminal must come back even when the program panics
        let hook = std::panic::take_hook();
        let (sfd, ssaved, smode, smouse) = (fd, saved, mode, mouse);
        std::panic::set_hook(Box::new(move |info| {
            restore_raw(sfd, &ssaved, smode, smouse);
            hook(info);
        }));
        let mut s = String::new();
        match mode {
            Mode::Full => s.push_str("\x1b[?1049h\x1b[2J\x1b[H"),
            Mode::Inline(_) => {}
        }
        s.push_str("\x1b[?25l\x1b[?2004h");
        if mouse {
            s.push_str("\x1b[?1000h\x1b[?1002h\x1b[?1006h");
        }
        t.write_str(&s)?;
        Ok(t)
    }

    pub fn waker(&self) -> Waker {
        Waker(self.wake_w)
    }

    pub fn size(&self) -> (i32, i32) {
        let mut ws: libc::winsize = unsafe { std::mem::zeroed() };
        if unsafe { libc::ioctl(self.fd, libc::TIOCGWINSZ, &mut ws) } == 0 && ws.ws_col > 0 && ws.ws_row > 0 {
            (ws.ws_col as i32, ws.ws_row as i32)
        } else {
            (80, 24)
        }
    }

    /// Size of the drawing area: the whole screen, or `n` rows (at most the screen) for inline.
    pub fn area(&self) -> (i32, i32) {
        let (w, h) = self.size();
        match self.mode {
            Mode::Full => (w, h),
            Mode::Inline(n) => (w, n.clamp(1, h)),
        }
    }

    /// Inline: scroll the shell up so `rows` lines exist below the cursor, leave the cursor on the first of them.
    pub fn reserve_inline(&mut self, rows: i32) -> io::Result<()> {
        if let Mode::Inline(_) = self.mode {
            let mut s = "\n".repeat((rows - 1).max(0) as usize);
            if rows > 1 {
                s.push_str(&format!("\x1b[{}A", rows - 1));
            }
            s.push('\r');
            self.write_str(&s)?;
            self.inline_rows = rows;
        }
        Ok(())
    }

    pub fn take_resize(&self) -> bool {
        WINCH.swap(false, Ordering::SeqCst)
    }

    pub fn terminated(&self) -> bool {
        TERMINATE.load(Ordering::SeqCst)
    }

    pub fn write_str(&mut self, s: &str) -> io::Result<()> {
        let mut b = s.as_bytes();
        while !b.is_empty() {
            let n = unsafe { libc::write(self.fd, b.as_ptr() as *const libc::c_void, b.len()) };
            if n < 0 {
                let e = io::Error::last_os_error();
                match e.kind() {
                    io::ErrorKind::Interrupted => continue,
                    io::ErrorKind::WouldBlock => {
                        self.wait(false, 100);
                        continue;
                    }
                    _ => return Err(e),
                }
            }
            b = &b[n as usize..];
        }
        Ok(())
    }

    fn wait(&self, for_read: bool, timeout_ms: i32) -> bool {
        let mut p = libc::pollfd { fd: self.fd, events: if for_read { libc::POLLIN } else { libc::POLLOUT }, revents: 0 };
        unsafe { libc::poll(&mut p, 1, timeout_ms) > 0 }
    }

    /// Wait for the terminal or the waker. `timeout_ms < 0` waits forever. Returns (tty readable, woken).
    pub fn poll(&self, timeout_ms: i32) -> (bool, bool) {
        let mut fds = [libc::pollfd { fd: self.fd, events: libc::POLLIN, revents: 0 }, libc::pollfd { fd: self.wake_r, events: libc::POLLIN, revents: 0 }];
        let n = unsafe { libc::poll(fds.as_mut_ptr(), 2, timeout_ms) };
        if n <= 0 {
            return (false, false);
        }
        let woken = fds[1].revents & libc::POLLIN != 0;
        if woken {
            let mut sink = [0u8; 64];
            while unsafe { libc::read(self.wake_r, sink.as_mut_ptr() as *mut libc::c_void, sink.len()) } > 0 {}
        }
        (fds[0].revents & (libc::POLLIN | libc::POLLHUP) != 0, woken)
    }

    /// Bytes available now (call after `poll` said readable). Zero means the terminal went away.
    pub fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            let n = unsafe { libc::read(self.fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) };
            if n >= 0 {
                return Ok(n as usize);
            }
            let e = io::Error::last_os_error();
            if e.kind() != io::ErrorKind::Interrupted {
                return Err(e);
            }
        }
    }

    pub fn restore(&mut self) {
        if self.restored {
            return;
        }
        self.restored = true;
        restore_raw(self.fd, &self.saved, self.mode, self.mouse);
        if self.mode != Mode::Full && self.inline_rows > 0 {
            // leave the shell below what we drew
            let s = format!("\x1b[{}B\r\n", self.inline_rows - 1);
            let _ = self.write_str(&s);
        }
        WAKE_FD.store(-1, Ordering::SeqCst);
        // the wake pipe is deliberately not closed: an effect thread may still hold a Waker, and a write to a
        // closed (or reused) fd would hit something else. Two fds per run is the price.
        unsafe {
            if self.owned {
                libc::close(self.fd);
            }
        }
    }
}

fn restore_raw(fd: RawFd, saved: &libc::termios, mode: Mode, mouse: bool) {
    let mut s = String::new();
    if mouse {
        s.push_str("\x1b[?1006l\x1b[?1002l\x1b[?1000l");
    }
    s.push_str("\x1b[?2004l\x1b[0m\x1b[?25h");
    if mode == Mode::Full {
        s.push_str("\x1b[?1049l");
    }
    let b = s.as_bytes();
    unsafe {
        libc::write(fd, b.as_ptr() as *const libc::c_void, b.len());
        libc::tcsetattr(fd, libc::TCSAFLUSH, saved);
    }
}

impl Drop for Term {
    fn drop(&mut self) {
        self.restore();
    }
}
