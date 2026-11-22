//! Terminal bytes -> events. Pure: feed bytes, get events. Handles UTF-8 cut by a read, CSI/SS3 keys with modifiers,
//! SGR mouse, bracketed paste. A lone ESC is only a key once `flush` says no more bytes are coming.

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Mods {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

impl Mods {
    pub const NONE: Mods = Mods { ctrl: false, alt: false, shift: false };
    pub const CTRL: Mods = Mods { ctrl: true, alt: false, shift: false };
    pub const ALT: Mods = Mods { ctrl: false, alt: true, shift: false };
    pub const SHIFT: Mods = Mods { ctrl: false, alt: false, shift: true };
    pub fn any(&self) -> bool {
        self.ctrl || self.alt || self.shift
    }
    fn from_param(p: u32) -> Mods {
        let m = p.saturating_sub(1);
        Mods { shift: m & 1 != 0, alt: m & 2 != 0, ctrl: m & 4 != 0 }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Key {
    Char(char),
    Enter,
    Tab,
    BackTab,
    Backspace,
    Delete,
    Insert,
    Esc,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    F(u8),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MouseKind {
    Down(u8),
    Up(u8),
    Drag(u8),
    Move,
    WheelUp,
    WheelDown,
}

#[derive(Clone, PartialEq, Debug)]
pub enum Event {
    Key(Key, Mods),
    Mouse(MouseKind, i32, i32, Mods),
    Paste(String),
    Resize(i32, i32),
}

impl Event {
    pub fn key(k: Key) -> Event {
        Event::Key(k, Mods::NONE)
    }
    pub fn ch(c: char) -> Event {
        Event::Key(Key::Char(c), Mods::NONE)
    }
}

/// `ctrl-r`, `alt-enter`, `f5`, `space`, `tab`, `shift-tab`, `q`
pub fn parse_key(name: &str) -> Option<(Key, Mods)> {
    let mut mods = Mods::NONE;
    let mut rest = name.trim();
    loop {
        if let Some(r) = rest.strip_prefix("ctrl-") {
            mods.ctrl = true;
            rest = r;
        } else if let Some(r) = rest.strip_prefix("alt-") {
            mods.alt = true;
            rest = r;
        } else if let Some(r) = rest.strip_prefix("shift-") {
            mods.shift = true;
            rest = r;
        } else {
            break;
        }
    }
    let key = match rest {
        "enter" | "return" => Key::Enter,
        "tab" if mods.shift => {
            mods.shift = false;
            Key::BackTab
        }
        "tab" => Key::Tab,
        "esc" | "escape" => Key::Esc,
        "space" => Key::Char(' '),
        "backspace" | "bs" => Key::Backspace,
        "delete" | "del" => Key::Delete,
        "insert" => Key::Insert,
        "up" => Key::Up,
        "down" => Key::Down,
        "left" => Key::Left,
        "right" => Key::Right,
        "home" => Key::Home,
        "end" => Key::End,
        "pgup" | "pageup" => Key::PageUp,
        "pgdn" | "pagedown" => Key::PageDown,
        f if f.len() >= 2 && f.starts_with('f') && f[1..].chars().all(|c| c.is_ascii_digit()) => {
            let n: u8 = f[1..].parse().ok()?;
            if !(1..=24).contains(&n) {
                return None;
            }
            Key::F(n)
        }
        s if s.chars().count() == 1 => Key::Char(s.chars().next()?),
        _ => return None,
    };
    Some((key, mods))
}

/// The same name `parse_key` accepts (used by the keybar and by script output).
pub fn key_name(k: Key, m: Mods) -> String {
    let mut s = String::new();
    if m.ctrl {
        s.push_str("ctrl-");
    }
    if m.alt {
        s.push_str("alt-");
    }
    if m.shift {
        s.push_str("shift-");
    }
    match k {
        Key::Char(' ') => s.push_str("space"),
        Key::Char(c) => s.push(c),
        Key::Enter => s.push_str("enter"),
        Key::Tab => s.push_str("tab"),
        Key::BackTab => s.push_str("shift-tab"),
        Key::Backspace => s.push_str("backspace"),
        Key::Delete => s.push_str("delete"),
        Key::Insert => s.push_str("insert"),
        Key::Esc => s.push_str("esc"),
        Key::Up => s.push_str("up"),
        Key::Down => s.push_str("down"),
        Key::Left => s.push_str("left"),
        Key::Right => s.push_str("right"),
        Key::Home => s.push_str("home"),
        Key::End => s.push_str("end"),
        Key::PageUp => s.push_str("pgup"),
        Key::PageDown => s.push_str("pgdn"),
        Key::F(n) => s.push_str(&format!("f{n}")),
    }
    s
}

#[derive(Default)]
pub struct Parser {
    buf: Vec<u8>,
    paste: Option<Vec<u8>>,
}

impl Parser {
    pub fn new() -> Parser {
        Parser::default()
    }

    /// Bytes arrived. Incomplete sequences wait for the next call (or `flush`).
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<Event> {
        self.buf.extend_from_slice(bytes);
        let mut out = Vec::new();
        self.drain(&mut out, false);
        out
    }

    /// Nothing more is coming right now: a half-read ESC is the Esc key; a cut UTF-8 sequence is dropped.
    /// True while bytes wait for more input (a lone ESC may be the Escape key or the start of a sequence).
    pub fn pending(&self) -> bool {
        !self.buf.is_empty()
    }

    pub fn flush(&mut self) -> Vec<Event> {
        let mut out = Vec::new();
        self.drain(&mut out, true);
        self.buf.clear();
        out
    }

    fn drain(&mut self, out: &mut Vec<Event>, flush: bool) {
        loop {
            if let Some(p) = &mut self.paste {
                // inside a paste: everything up to ESC[201~
                p.extend_from_slice(&self.buf);
                self.buf.clear();
                if let Some(pos) = find(p, b"\x1b[201~") {
                    let rest: Vec<u8> = p[pos + 6..].to_vec();
                    let text = String::from_utf8_lossy(&p[..pos]).into_owned();
                    self.paste = None;
                    out.push(Event::Paste(text));
                    self.buf = rest;
                    continue;
                }
                if flush {
                    let text = String::from_utf8_lossy(p).into_owned();
                    self.paste = None;
                    out.push(Event::Paste(text));
                }
                return;
            }
            if self.buf.is_empty() {
                return;
            }
            match self.one(flush) {
                Step::Done(n, ev) => {
                    self.buf.drain(..n);
                    out.extend(ev);
                }
                Step::Paste(n) => {
                    self.buf.drain(..n);
                    self.paste = Some(Vec::new());
                }
                Step::Need => return,
            }
        }
    }

    fn one(&self, flush: bool) -> Step {
        let b = &self.buf;
        let c = b[0];
        if c == 0x1b {
            if b.len() == 1 {
                return if flush { Step::Done(1, Some(Event::key(Key::Esc))) } else { Step::Need };
            }
            match b[1] {
                b'[' => return self.csi(flush),
                b'O' => {
                    if b.len() < 3 {
                        return if flush { Step::Done(2, Some(Event::Key(Key::Char('O'), Mods::ALT))) } else { Step::Need };
                    }
                    let k = match b[2] {
                        b'P' => Key::F(1),
                        b'Q' => Key::F(2),
                        b'R' => Key::F(3),
                        b'S' => Key::F(4),
                        b'A' => Key::Up,
                        b'B' => Key::Down,
                        b'C' => Key::Right,
                        b'D' => Key::Left,
                        b'H' => Key::Home,
                        b'F' => Key::End,
                        _ => return Step::Done(3, None),
                    };
                    return Step::Done(3, Some(Event::key(k)));
                }
                0x1b => return Step::Done(1, Some(Event::key(Key::Esc))),
                _ => {
                    // alt + whatever follows
                    return match self.utf8(1) {
                        Utf8::Ok(ch, n) => {
                            let (k, m) = ctrl_map(ch);
                            Step::Done(1 + n, Some(Event::Key(k, Mods { alt: true, ..m })))
                        }
                        Utf8::Need => {
                            if flush {
                                Step::Done(b.len(), None)
                            } else {
                                Step::Need
                            }
                        }
                        Utf8::Bad => Step::Done(2, None),
                    };
                }
            }
        }
        match self.utf8(0) {
            Utf8::Ok(ch, n) => {
                let (k, m) = ctrl_map(ch);
                Step::Done(n, Some(Event::Key(k, m)))
            }
            Utf8::Need => {
                if flush {
                    Step::Done(b.len(), None)
                } else {
                    Step::Need
                }
            }
            Utf8::Bad => Step::Done(1, None),
        }
    }

    fn utf8(&self, at: usize) -> Utf8 {
        let b = &self.buf[at..];
        let first = b[0];
        let n = if first < 0x80 {
            1
        } else if first >> 5 == 0b110 {
            2
        } else if first >> 4 == 0b1110 {
            3
        } else if first >> 3 == 0b11110 {
            4
        } else {
            return Utf8::Bad;
        };
        if b.len() < n {
            return Utf8::Need;
        }
        match std::str::from_utf8(&b[..n]).ok().and_then(|s| s.chars().next()) {
            Some(c) => Utf8::Ok(c, n),
            None => Utf8::Bad,
        }
    }

    fn csi(&self, flush: bool) -> Step {
        let b = &self.buf;
        // ESC [ params final ; the parameter part is bounded so garbage cannot grow without limit
        let mut i = 2;
        while i < b.len() && (0x20..=0x3f).contains(&b[i]) {
            i += 1;
            if i > 40 {
                return Step::Done(i, None);
            }
        }
        if i >= b.len() {
            return if flush { Step::Done(b.len(), None) } else { Step::Need };
        }
        let fin = b[i];
        let len = i + 1;
        let params = std::str::from_utf8(&b[2..i]).unwrap_or("");
        if !(0x40..=0x7e).contains(&fin) {
            return Step::Done(len, None);
        }
        if (fin == b'M' || fin == b'm') && params.starts_with('<') {
            let nums: Vec<i64> = params[1..].split(';').map(|s| s.parse().unwrap_or(-1)).collect();
            if nums.len() == 3 && nums.iter().all(|&n| n >= 0) {
                let (btn, x, y) = (nums[0] as u32, nums[1] as i32 - 1, nums[2] as i32 - 1);
                let mods = Mods { shift: btn & 4 != 0, alt: btn & 8 != 0, ctrl: btn & 16 != 0 };
                let low = (btn & 3) as u8;
                let kind = if btn & 64 != 0 {
                    if low == 0 { MouseKind::WheelUp } else { MouseKind::WheelDown }
                } else if btn & 32 != 0 {
                    if low == 3 { MouseKind::Move } else { MouseKind::Drag(low) }
                } else if fin == b'm' {
                    MouseKind::Up(low)
                } else {
                    MouseKind::Down(low)
                };
                return Step::Done(len, Some(Event::Mouse(kind, x, y, mods)));
            }
            return Step::Done(len, None);
        }
        let nums: Vec<u32> = params.split(';').map(|s| s.parse().unwrap_or(0)).collect();
        let n0 = nums.first().copied().unwrap_or(0);
        let mods = if nums.len() >= 2 { Mods::from_param(nums[1]) } else { Mods::NONE };
        let key = match fin {
            b'A' => Key::Up,
            b'B' => Key::Down,
            b'C' => Key::Right,
            b'D' => Key::Left,
            b'H' => Key::Home,
            b'F' => Key::End,
            b'Z' => return Step::Done(len, Some(Event::Key(Key::BackTab, Mods::NONE))),
            b'P' => Key::F(1),
            b'Q' => Key::F(2),
            b'S' => Key::F(4),
            b'~' => match n0 {
                1 | 7 => Key::Home,
                2 => Key::Insert,
                3 => Key::Delete,
                4 | 8 => Key::End,
                5 => Key::PageUp,
                6 => Key::PageDown,
                11..=15 => Key::F((n0 - 10) as u8),
                17..=21 => Key::F((n0 - 11) as u8),
                23 | 24 => Key::F((n0 - 12) as u8),
                200 => return Step::Paste(len),
                _ => return Step::Done(len, None),
            },
            _ => return Step::Done(len, None),
        };
        Step::Done(len, Some(Event::Key(key, mods)))
    }
}

enum Step {
    Done(usize, Option<Event>),
    Paste(usize),
    Need,
}

enum Utf8 {
    Ok(char, usize),
    Need,
    Bad,
}

fn ctrl_map(ch: char) -> (Key, Mods) {
    match ch as u32 {
        0x0d | 0x0a => (Key::Enter, Mods::NONE),
        0x09 => (Key::Tab, Mods::NONE),
        0x08 | 0x7f => (Key::Backspace, Mods::NONE),
        0x00 => (Key::Char(' '), Mods::CTRL),
        n @ 0x01..=0x1a => (Key::Char((b'a' + n as u8 - 1) as char), Mods::CTRL),
        n @ 0x1c..=0x1f => (Key::Char((b'4' + n as u8 - 0x1c) as char), Mods::CTRL),
        _ => (Key::Char(ch), Mods::NONE),
    }
}

fn find(h: &[u8], n: &[u8]) -> Option<usize> {
    h.windows(n.len()).position(|w| w == n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_and_ctrl_and_alt() {
        let mut p = Parser::new();
        assert_eq!(p.feed(b"a\x03\x1bx\r"), vec![Event::ch('a'), Event::Key(Key::Char('c'), Mods::CTRL), Event::Key(Key::Char('x'), Mods::ALT), Event::key(Key::Enter)]);
    }

    #[test]
    fn arrows_with_modifiers_and_fkeys() {
        let mut p = Parser::new();
        assert_eq!(p.feed(b"\x1b[A\x1b[1;5C\x1bOP\x1b[15~\x1b[Z"), vec![
            Event::key(Key::Up),
            Event::Key(Key::Right, Mods::CTRL),
            Event::key(Key::F(1)),
            Event::key(Key::F(5)),
            Event::key(Key::BackTab),
        ]);
    }

    #[test]
    fn esc_waits_for_flush() {
        let mut p = Parser::new();
        assert!(p.feed(b"\x1b").is_empty());
        assert_eq!(p.flush(), vec![Event::key(Key::Esc)]);
        // split CSI is put back together
        assert!(p.feed(b"\x1b[").is_empty());
        assert_eq!(p.feed(b"B"), vec![Event::key(Key::Down)]);
    }

    #[test]
    fn utf8_cut_by_a_read() {
        let mut p = Parser::new();
        let bytes = "ą".as_bytes();
        assert!(p.feed(&bytes[..1]).is_empty());
        assert_eq!(p.feed(&bytes[1..]), vec![Event::ch('ą')]);
    }

    #[test]
    fn sgr_mouse() {
        let mut p = Parser::new();
        assert_eq!(p.feed(b"\x1b[<0;10;5M\x1b[<0;10;5m\x1b[<64;3;4M\x1b[<32;7;8M"), vec![
            Event::Mouse(MouseKind::Down(0), 9, 4, Mods::NONE),
            Event::Mouse(MouseKind::Up(0), 9, 4, Mods::NONE),
            Event::Mouse(MouseKind::WheelUp, 2, 3, Mods::NONE),
            Event::Mouse(MouseKind::Drag(0), 6, 7, Mods::NONE),
        ]);
    }

    #[test]
    fn bracketed_paste_across_reads() {
        let mut p = Parser::new();
        assert!(p.feed(b"\x1b[200~hel").is_empty());
        assert_eq!(p.feed(b"lo\nworld\x1b[201~x"), vec![Event::Paste("hello\nworld".into()), Event::ch('x')]);
    }

    #[test]
    fn garbage_never_panics_or_grows() {
        let mut p = Parser::new();
        let junk: Vec<u8> = (0..20000u32).map(|i| (i.wrapping_mul(2654435761) >> 13) as u8).collect();
        for chunk in junk.chunks(7) {
            let _ = p.feed(chunk);
        }
        let _ = p.flush();
        assert!(p.buf.is_empty());
        let mut q = Parser::new();
        let long: Vec<u8> = std::iter::once(0x1b).chain(std::iter::once(b'[')).chain(std::iter::repeat(b'1').take(5000)).collect();
        let _ = q.feed(&long);
        assert!(q.buf.len() < 5100);
    }

    #[test]
    fn key_names_roundtrip() {
        for n in ["ctrl-r", "alt-enter", "f5", "space", "tab", "shift-tab", "q", "pgup"] {
            let (k, m) = parse_key(n).unwrap();
            assert_eq!(key_name(k, m), n);
        }
        assert!(parse_key("f99").is_none());
        assert!(parse_key("nonsense").is_none());
    }
}
