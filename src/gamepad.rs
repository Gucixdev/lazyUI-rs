//! Gamepad as keys, for a handheld without a keyboard (steamdeck_launcher): the evdev gamepads are read from
//! `/dev/input/event*` (the user needs read access, normally the `input` group) and turned into the keys a list or a
//! form already understands. D-pad and left stick move (with repeat), A (south) is enter, B (east) is esc, X (west)
//! is space, the shoulders are tab and shift-tab, Y (north) is `f` for a filter field, start is ctrl-q.
//! `Mapper` is pure (bytes in, events out) so it is tested without a device; `Pad` opens the devices.

use crate::input::{Event, Key, Mods};
use std::os::fd::{AsRawFd, RawFd};
use std::time::{Duration, Instant};

const EV_KEY: u16 = 1;
const EV_ABS: u16 = 3;
const BTN_SOUTH: u16 = 304;
const BTN_EAST: u16 = 305;
const BTN_NORTH: u16 = 307;
const BTN_WEST: u16 = 308;
const BTN_TL: u16 = 310;
const BTN_TR: u16 = 311;
const BTN_START: u16 = 315;
const ABS_X: u16 = 0;
const ABS_Y: u16 = 1;
const ABS_HAT0X: u16 = 16;
const ABS_HAT0Y: u16 = 17;

const FIRST: Duration = Duration::from_millis(400);
const EVERY: Duration = Duration::from_millis(90);
/// the size of one `struct input_event` on a 64-bit Linux: timeval (16), type, code, value
const EV_SIZE: usize = 24;

/// One raw event of the kernel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Raw {
    pub kind: u16,
    pub code: u16,
    pub value: i32,
}

/// The events in a read buffer; a trailing partial event is dropped.
pub fn parse(buf: &[u8]) -> Vec<Raw> {
    buf.as_chunks::<EV_SIZE>().0.iter()
        .map(|c| Raw { kind: u16::from_le_bytes([c[16], c[17]]), code: u16::from_le_bytes([c[18], c[19]]), value: i32::from_le_bytes([c[20], c[21], c[22], c[23]]) })
        .collect()
}

/// Turns raw events into keys. `range` is the (min, max) of the stick axes, a stick counts as pushed past 50 %.
pub struct Mapper {
    range: [(i32, i32); 2],
    /// the direction each source holds: hat x, hat y, stick x, stick y
    held: [i32; 4],
    repeating: Option<(Key, Instant)>,
}

fn dir_key(i: usize, v: i32) -> Key {
    match (i % 2, v < 0) {
        (0, true) => Key::Left,
        (0, false) => Key::Right,
        (_, true) => Key::Up,
        (_, false) => Key::Down,
    }
}

impl Mapper {
    pub fn new(range: [(i32, i32); 2]) -> Mapper {
        Mapper { range, held: [0; 4], repeating: None }
    }

    fn key(k: Key) -> Event {
        Event::Key(k, Mods::NONE)
    }

    fn stick(&self, axis: usize, v: i32) -> i32 {
        let (lo, hi) = self.range[axis];
        let mid = (lo as i64 + hi as i64) / 2;
        let half = ((hi as i64 - lo as i64) / 2).max(1);
        let off = v as i64 - mid;
        if off * 2 > half {
            1
        } else if off * 2 < -half {
            -1
        } else {
            0
        }
    }

    pub fn feed(&mut self, e: Raw, now: Instant) -> Vec<Event> {
        let mut out = Vec::new();
        let slot = match (e.kind, e.code) {
            (EV_ABS, ABS_HAT0X) => Some((0, e.value.signum())),
            (EV_ABS, ABS_HAT0Y) => Some((1, e.value.signum())),
            (EV_ABS, ABS_X) => Some((2, self.stick(0, e.value))),
            (EV_ABS, ABS_Y) => Some((3, self.stick(1, e.value))),
            _ => None,
        };
        if let Some((i, v)) = slot {
            if self.held[i] != v {
                self.held[i] = v;
                if v != 0 {
                    let k = dir_key(i, v);
                    out.push(Self::key(k));
                    self.repeating = Some((k, now + FIRST));
                } else if self.repeating.is_some_and(|(k, _)| k == dir_key(i, -1) || k == dir_key(i, 1)) {
                    self.repeating = None;
                    // another source may still hold a direction
                    if let Some(j) = self.held.iter().position(|&h| h != 0) {
                        self.repeating = Some((dir_key(j, self.held[j]), now + FIRST));
                    }
                }
            }
            return out;
        }
        if e.kind == EV_KEY && e.value == 1 {
            let ev = match e.code {
                BTN_SOUTH => Some(Self::key(Key::Enter)),
                BTN_EAST => Some(Self::key(Key::Esc)),
                BTN_WEST => Some(Self::key(Key::Char(' '))),
                BTN_NORTH => Some(Self::key(Key::Char('f'))),
                BTN_TL => Some(Self::key(Key::BackTab)),
                BTN_TR => Some(Self::key(Key::Tab)),
                BTN_START => Some(Event::Key(Key::Char('q'), Mods::CTRL)),
                _ => None,
            };
            out.extend(ev);
        }
        out
    }

    /// How long until a held direction repeats.
    pub fn next_repeat(&self, now: Instant) -> Option<Duration> {
        self.repeating.map(|(_, at)| at.saturating_duration_since(now))
    }

    /// The repeats due at `now`.
    pub fn repeat(&mut self, now: Instant) -> Vec<Event> {
        let mut out = Vec::new();
        if let Some((k, at)) = &mut self.repeating {
            let mut n = 0;
            while *at <= now && n < 8 {
                out.push(Self::key(*k));
                *at += EVERY;
                n += 1;
            }
            if *at <= now {
                *at = now + EVERY;
            }
        }
        out
    }
}

struct Device {
    file: std::fs::File,
    map: Mapper,
}

/// The open gamepads.
pub struct Pad {
    devs: Vec<Device>,
    scanned: Instant,
}

fn ioctl_read(fd: RawFd, nr: u8, len: usize, buf: &mut [u8]) -> bool {
    // _IOR('E', nr, len): direction read (2) in the top bits, size, type 'E', number
    let req = (2u64 << 30) | ((len as u64) << 16) | ((b'E' as u64) << 8) | nr as u64;
    unsafe { libc::ioctl(fd, req as _, buf.as_mut_ptr()) >= 0 }
}

fn has_bit(bits: &[u8], n: usize) -> bool {
    bits.get(n / 8).is_some_and(|b| b & (1 << (n % 8)) != 0)
}

fn open_one(path: &std::path::Path) -> Option<Device> {
    use std::os::unix::fs::OpenOptionsExt;
    let file = std::fs::OpenOptions::new().read(true).custom_flags(libc::O_NONBLOCK).open(path).ok()?;
    let fd = file.as_raw_fd();
    let mut keys = [0u8; 96];
    if !ioctl_read(fd, 0x20 + EV_KEY as u8, keys.len(), &mut keys) || !has_bit(&keys, BTN_SOUTH as usize) {
        return None;
    }
    let mut range = [(-32768, 32767); 2];
    for (i, axis) in [ABS_X, ABS_Y].into_iter().enumerate() {
        let mut info = [0u8; 24];
        if ioctl_read(fd, 0x40 + axis as u8, info.len(), &mut info) {
            let (lo, hi) = (i32::from_le_bytes(info[4..8].try_into().unwrap()), i32::from_le_bytes(info[8..12].try_into().unwrap()));
            if hi > lo {
                range[i] = (lo, hi);
            }
        }
    }
    Some(Device { file, map: Mapper::new(range) })
}

impl Pad {
    /// Every readable evdev device that has a south button. Empty when none, or no permission.
    pub fn open() -> Pad {
        let mut devs = Vec::new();
        if let Ok(rd) = std::fs::read_dir("/dev/input") {
            let mut paths: Vec<_> = rd.flatten().map(|e| e.path()).filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("event"))).collect();
            paths.sort();
            devs.extend(paths.iter().filter_map(|p| open_one(p)));
        }
        Pad { devs, scanned: Instant::now() }
    }

    pub fn is_empty(&self) -> bool {
        self.devs.is_empty()
    }

    /// Look again for devices (a pad plugged in later) when there is none and the last look is older than 5 s.
    pub fn rescan(&mut self, now: Instant) {
        if self.devs.is_empty() && now.duration_since(self.scanned) > Duration::from_secs(5) {
            *self = Pad::open();
        }
    }

    pub fn fds(&self) -> Vec<RawFd> {
        self.devs.iter().map(|d| d.file.as_raw_fd()).collect()
    }

    /// Read what is waiting on every device. A device that went away (an error other than "would block") is dropped.
    pub fn read(&mut self, now: Instant) -> Vec<Event> {
        use std::io::Read;
        let mut out = Vec::new();
        self.devs.retain_mut(|d| {
            let mut buf = [0u8; EV_SIZE * 64];
            loop {
                match d.file.read(&mut buf) {
                    Ok(0) => return false,
                    Ok(n) => out.extend(parse(&buf[..n]).into_iter().flat_map(|r| d.map.feed(r, now))),
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => return true,
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(_) => return false,
                }
            }
        });
        out
    }

    pub fn next_repeat(&self, now: Instant) -> Option<Duration> {
        self.devs.iter().filter_map(|d| d.map.next_repeat(now)).min()
    }

    pub fn repeat(&mut self, now: Instant) -> Vec<Event> {
        self.devs.iter_mut().flat_map(|d| d.map.repeat(now)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(kind: u16, code: u16, value: i32) -> Raw {
        Raw { kind, code, value }
    }

    fn bytes(kind: u16, code: u16, value: i32) -> Vec<u8> {
        let mut b = vec![0u8; 16];
        b.extend(kind.to_le_bytes());
        b.extend(code.to_le_bytes());
        b.extend(value.to_le_bytes());
        b
    }

    fn keys(v: Vec<Event>) -> Vec<Key> {
        v.into_iter().filter_map(|e| if let Event::Key(k, _) = e { Some(k) } else { None }).collect()
    }

    #[test]
    fn parse_reads_events_and_drops_a_partial_tail() {
        let mut b = bytes(EV_KEY, BTN_SOUTH, 1);
        b.extend(bytes(EV_ABS, ABS_HAT0X, -1));
        b.extend([1, 2, 3]);
        assert_eq!(parse(&b), vec![raw(EV_KEY, BTN_SOUTH, 1), raw(EV_ABS, ABS_HAT0X, -1)]);
    }

    #[test]
    fn buttons_press_keys_and_releases_do_nothing() {
        let mut m = Mapper::new([(-32768, 32767); 2]);
        let t = Instant::now();
        let mut pressed = |code| keys(m.feed(raw(EV_KEY, code, 1), t));
        assert_eq!(pressed(BTN_SOUTH), [Key::Enter]);
        assert_eq!(pressed(BTN_EAST), [Key::Esc]);
        assert_eq!(pressed(BTN_WEST), [Key::Char(' ')]);
        assert_eq!(pressed(BTN_TR), [Key::Tab]);
        assert_eq!(pressed(BTN_TL), [Key::BackTab]);
        assert_eq!(pressed(999), Vec::<Key>::new());
        assert!(m.feed(raw(EV_KEY, BTN_SOUTH, 0), t).is_empty());
        assert!(m.feed(raw(EV_KEY, BTN_SOUTH, 2), t).is_empty(), "the kernel's own repeat is ignored");
        assert_eq!(m.feed(raw(EV_KEY, BTN_START, 1), t), vec![Event::Key(Key::Char('q'), Mods::CTRL)]);
    }

    #[test]
    fn hat_moves_once_then_repeats_after_the_delay_until_released() {
        let mut m = Mapper::new([(-32768, 32767); 2]);
        let t = Instant::now();
        assert_eq!(keys(m.feed(raw(EV_ABS, ABS_HAT0Y, 1), t)), [Key::Down]);
        assert!(m.repeat(t + Duration::from_millis(300)).is_empty());
        assert_eq!(keys(m.repeat(t + Duration::from_millis(400))), [Key::Down]);
        assert_eq!(keys(m.repeat(t + Duration::from_millis(400 + 180))), [Key::Down, Key::Down]);
        assert!(m.feed(raw(EV_ABS, ABS_HAT0Y, 0), t).is_empty());
        assert!(m.next_repeat(t).is_none() && m.repeat(t + Duration::from_secs(5)).is_empty());
    }

    #[test]
    fn stick_needs_more_than_half_its_range_and_uses_the_real_range() {
        let mut m = Mapper::new([(0, 255); 2]);
        let t = Instant::now();
        assert!(m.feed(raw(EV_ABS, ABS_X, 150), t).is_empty(), "128 is the middle, 150 is a nudge");
        assert_eq!(keys(m.feed(raw(EV_ABS, ABS_X, 230), t)), [Key::Right]);
        assert!(m.feed(raw(EV_ABS, ABS_X, 240), t).is_empty(), "still held: no second key");
        assert!(m.feed(raw(EV_ABS, ABS_X, 128), t).is_empty());
        assert_eq!(keys(m.feed(raw(EV_ABS, ABS_Y, 10), t)), [Key::Up]);
    }

    #[test]
    fn releasing_one_source_keeps_repeating_the_other() {
        let mut m = Mapper::new([(-100, 100); 2]);
        let t = Instant::now();
        m.feed(raw(EV_ABS, ABS_HAT0X, 1), t);
        m.feed(raw(EV_ABS, ABS_Y, 90), t);
        m.feed(raw(EV_ABS, ABS_Y, 0), t);
        assert_eq!(keys(m.repeat(t + Duration::from_secs(1))).first(), Some(&Key::Right));
    }

    #[test]
    fn no_devices_is_an_empty_pad_not_an_error() {
        let mut p = Pad { devs: Vec::new(), scanned: Instant::now() };
        assert!(p.is_empty() && p.fds().is_empty() && p.read(Instant::now()).is_empty() && p.next_repeat(Instant::now()).is_none());
    }
}
