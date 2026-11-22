//! Cell canvas: what widgets draw on. No I/O. A frame is the diff of two canvases turned into one ANSI string.

use std::fmt::Write as _;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Color {
    Default,
    Idx(u8),
    Rgb(u8, u8, u8),
}

pub const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::Rgb(r, g, b)
}

pub const BOLD: u8 = 1;
pub const DIM: u8 = 2;
pub const ITALIC: u8 = 4;
pub const UNDERLINE: u8 = 8;
pub const REVERSE: u8 = 16;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Style {
    pub fg: Color,
    pub bg: Color,
    pub mods: u8,
}

impl Style {
    pub const DEFAULT: Style = Style { fg: Color::Default, bg: Color::Default, mods: 0 };
    pub const fn fg(c: Color) -> Style {
        Style { fg: c, bg: Color::Default, mods: 0 }
    }
    pub const fn new(fg: Color, bg: Color) -> Style {
        Style { fg, bg, mods: 0 }
    }
    pub const fn with(self, mods: u8) -> Style {
        Style { mods: self.mods | mods, ..self }
    }
    pub const fn on(self, bg: Color) -> Style {
        Style { bg, ..self }
    }
}

impl Default for Style {
    fn default() -> Self {
        Style::DEFAULT
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Cell {
    /// `'\0'` marks the right half of a wide glyph.
    pub ch: char,
    pub style: Style,
}

const BLANK: Cell = Cell { ch: ' ', style: Style::DEFAULT };

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect { x, y, w, h }
    }
    pub fn is_empty(&self) -> bool {
        self.w <= 0 || self.h <= 0
    }
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.w && y < self.y + self.h
    }
    /// Shrink by `n` on every side; never negative.
    pub fn inset(&self, n: i32) -> Rect {
        Rect { x: self.x + n, y: self.y + n, w: (self.w - 2 * n).max(0), h: (self.h - 2 * n).max(0) }
    }
    pub fn intersect(&self, o: &Rect) -> Rect {
        let x0 = self.x.max(o.x);
        let y0 = self.y.max(o.y);
        let x1 = (self.x + self.w).min(o.x + o.w);
        let y1 = (self.y + self.h).min(o.y + o.h);
        Rect { x: x0, y: y0, w: (x1 - x0).max(0), h: (y1 - y0).max(0) }
    }
}

/// How many columns a char takes: 0 (combining, control), 1, or 2 (CJK, emoji).
pub fn char_width(c: char) -> usize {
    let u = c as u32;
    if u < 0x20 || (0x7f..0xa0).contains(&u) {
        return 0;
    }
    if (0x300..=0x36f).contains(&u) || (0x200b..=0x200f).contains(&u) || u == 0xfe0f || u == 0x200d {
        return 0;
    }
    let wide = (0x1100..=0x115f).contains(&u)
        || (0x2e80..=0xa4cf).contains(&u)
        || (0xac00..=0xd7a3).contains(&u)
        || (0xf900..=0xfaff).contains(&u)
        || (0xfe30..=0xfe6f).contains(&u)
        || (0xff00..=0xff60).contains(&u)
        || (0xffe0..=0xffe6).contains(&u)
        || (0x1f300..=0x1f64f).contains(&u)
        || (0x1f900..=0x1f9ff).contains(&u)
        || (0x20000..=0x3fffd).contains(&u);
    if wide {
        2
    } else {
        1
    }
}

pub fn str_width(s: &str) -> usize {
    s.chars().map(char_width).sum()
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Colors {
    True,
    C256,
    C16,
    C8,
}

/// What the terminal can show. Widgets only ask `unicode`; the diff only asks `colors`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Caps {
    pub unicode: bool,
    pub colors: Colors,
}

impl Caps {
    pub const FULL: Caps = Caps { unicode: true, colors: Colors::True };
    pub const ASCII: Caps = Caps { unicode: false, colors: Colors::C16 };

    pub fn detect() -> Caps {
        let term = std::env::var("TERM").unwrap_or_default();
        let console = term == "linux" || term == "vt100" || term.is_empty();
        let locale = ["LC_ALL", "LC_CTYPE", "LANG"]
            .iter()
            .find_map(|k| std::env::var(k).ok().filter(|v| !v.is_empty()))
            .unwrap_or_default()
            .to_ascii_lowercase();
        let utf8 = locale.contains("utf-8") || locale.contains("utf8");
        let ct = std::env::var("COLORTERM").unwrap_or_default();
        let colors = if ct == "truecolor" || ct == "24bit" {
            Colors::True
        } else if console {
            Colors::C16
        } else if term.contains("256") {
            Colors::C256
        } else if term == "dumb" {
            Colors::C8
        } else {
            Colors::C256
        };
        Caps { unicode: utf8 && !console, colors }
    }
}

/// Box and bar glyphs, chosen once from the caps.
#[derive(Clone, Copy)]
pub struct Glyphs {
    pub h: char,
    pub v: char,
    pub tl: char,
    pub tr: char,
    pub bl: char,
    pub br: char,
    pub full: char,
    pub empty: char,
    pub pointer: char,
    pub ellipsis: char,
}

impl Glyphs {
    pub const UNICODE: Glyphs = Glyphs { h: '─', v: '│', tl: '╭', tr: '╮', bl: '╰', br: '╯', full: '█', empty: '░', pointer: '▶', ellipsis: '…' };
    pub const ASCII: Glyphs = Glyphs { h: '-', v: '|', tl: '+', tr: '+', bl: '+', br: '+', full: '#', empty: '.', pointer: '>', ellipsis: '~' };
}

#[derive(Clone)]
pub struct Canvas {
    pub w: i32,
    pub h: i32,
    pub caps: Caps,
    cells: Vec<Cell>,
    clip: Rect,
}

impl Canvas {
    pub fn new(w: i32, h: i32, caps: Caps) -> Canvas {
        let (w, h) = (w.clamp(0, 10_000), h.clamp(0, 10_000));
        Canvas { w, h, caps, cells: vec![BLANK; (w * h) as usize], clip: Rect::new(0, 0, w, h) }
    }
    pub fn glyphs(&self) -> Glyphs {
        if self.caps.unicode {
            Glyphs::UNICODE
        } else {
            Glyphs::ASCII
        }
    }
    pub fn area(&self) -> Rect {
        Rect::new(0, 0, self.w, self.h)
    }
    pub fn clear(&mut self, style: Style) {
        self.clip = self.area();
        for c in &mut self.cells {
            *c = Cell { ch: ' ', style };
        }
    }
    /// Drawing outside `r` is dropped until the clip is changed again. Returns the old clip.
    pub fn set_clip(&mut self, r: Rect) -> Rect {
        let old = self.clip;
        self.clip = r.intersect(&self.area());
        old
    }
    pub fn get(&self, x: i32, y: i32) -> Option<Cell> {
        if x < 0 || y < 0 || x >= self.w || y >= self.h {
            None
        } else {
            Some(self.cells[(y * self.w + x) as usize])
        }
    }
    fn idx(&self, x: i32, y: i32) -> Option<usize> {
        if self.clip.contains(x, y) {
            Some((y * self.w + x) as usize)
        } else {
            None
        }
    }
    /// Put one char; a wide char that does not fit or is cut by the clip is replaced by a blank.
    pub fn put(&mut self, x: i32, y: i32, ch: char, style: Style) {
        let cw = char_width(ch);
        if cw == 0 {
            return;
        }
        let Some(i) = self.idx(x, y) else { return };
        // overwriting half of an old wide glyph blanks the other half
        if self.cells[i].ch == '\0' && x > 0 {
            self.cells[i - 1] = Cell { ch: ' ', style: self.cells[i - 1].style };
        }
        if cw == 2 {
            if self.idx(x + 1, y).is_none() {
                self.cells[i] = Cell { ch: ' ', style };
                return;
            }
            if self.cells[i + 1].ch == '\0' && x + 2 < self.w {
                self.cells[i + 2] = Cell { ch: ' ', style: self.cells[i + 2].style };
            }
            self.cells[i] = Cell { ch, style };
            self.cells[i + 1] = Cell { ch: '\0', style };
        } else {
            if i + 1 < self.cells.len() && x + 1 < self.w && self.cells[i + 1].ch == '\0' {
                self.cells[i + 1] = Cell { ch: ' ', style: self.cells[i + 1].style };
            }
            self.cells[i] = Cell { ch, style };
        }
    }
    /// Write text on one row; returns the columns used. Control chars are shown as `?`.
    pub fn text(&mut self, x: i32, y: i32, s: &str, style: Style) -> i32 {
        let mut cx = x;
        for ch in s.chars() {
            let ch = if (ch as u32) < 0x20 || ch as u32 == 0x7f { '?' } else { ch };
            let w = char_width(ch) as i32;
            if w == 0 {
                continue;
            }
            self.put(cx, y, ch, style);
            cx += w;
            if cx >= self.clip.x + self.clip.w {
                break;
            }
        }
        cx - x
    }
    /// Text cut to `max` columns with an ellipsis.
    pub fn text_fit(&mut self, x: i32, y: i32, max: i32, s: &str, style: Style) {
        if max <= 0 {
            return;
        }
        if str_width(s) as i32 <= max {
            self.text(x, y, s, style);
            return;
        }
        let g = self.glyphs().ellipsis;
        let mut out = String::new();
        let mut w = 0;
        for ch in s.chars() {
            let cw = char_width(ch) as i32;
            if w + cw > max - 1 {
                break;
            }
            out.push(ch);
            w += cw;
        }
        out.push(g);
        self.text(x, y, &out, style);
    }
    pub fn fill(&mut self, r: Rect, ch: char, style: Style) {
        let r = r.intersect(&self.clip);
        for y in r.y..r.y + r.h {
            for x in r.x..r.x + r.w {
                self.put(x, y, ch, style);
            }
        }
    }
    pub fn border(&mut self, r: Rect, title: &str, style: Style, title_style: Style) {
        if r.w < 2 || r.h < 2 {
            return;
        }
        let g = self.glyphs();
        let (x1, y1) = (r.x + r.w - 1, r.y + r.h - 1);
        for x in r.x + 1..x1 {
            self.put(x, r.y, g.h, style);
            self.put(x, y1, g.h, style);
        }
        for y in r.y + 1..y1 {
            self.put(r.x, y, g.v, style);
            self.put(x1, y, g.v, style);
        }
        self.put(r.x, r.y, g.tl, style);
        self.put(x1, r.y, g.tr, style);
        self.put(r.x, y1, g.bl, style);
        self.put(x1, y1, g.br, style);
        if !title.is_empty() && r.w > 4 {
            let t = format!(" {} ", title);
            self.text_fit(r.x + 2, r.y, r.w - 4, &t, title_style);
        }
    }
    /// The canvas as plain text: one line per row, trailing blanks trimmed. Used by snapshots and tests.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        for y in 0..self.h {
            let mut line = String::new();
            for x in 0..self.w {
                let c = self.cells[(y * self.w + x) as usize];
                if c.ch != '\0' {
                    line.push(c.ch);
                }
            }
            out.push_str(line.trim_end());
            out.push('\n');
        }
        out
    }
}

// ---- colours -----------------------------------------------------------------------------------

const ANSI16: [(u8, u8, u8); 16] = [
    (0, 0, 0), (205, 49, 49), (13, 188, 121), (229, 229, 16), (36, 114, 200), (188, 63, 188), (17, 168, 205), (229, 229, 229),
    (102, 102, 102), (241, 76, 76), (35, 209, 139), (245, 245, 67), (59, 142, 234), (214, 112, 214), (41, 184, 219), (255, 255, 255),
];

fn dist(a: (u8, u8, u8), b: (u8, u8, u8)) -> i32 {
    let d = |x: u8, y: u8| (x as i32 - y as i32).pow(2);
    d(a.0, b.0) + d(a.1, b.1) + d(a.2, b.2)
}

fn nearest(rgb: (u8, u8, u8), n: usize) -> u8 {
    (0..n).min_by_key(|&i| dist(rgb, ANSI16[i])).unwrap_or(0) as u8
}

fn to_256(r: u8, g: u8, b: u8) -> u8 {
    let cube = |v: u8| -> u8 { if v < 48 { 0 } else if v < 115 { 1 } else { (v - 35) / 40 } };
    let (cr, cg, cb) = (cube(r), cube(g), cube(b));
    let lv = |c: u8| -> u8 { if c == 0 { 0 } else { 55 + 40 * c } };
    let cube_rgb = (lv(cr), lv(cg), lv(cb));
    let gray = ((r as u32 + g as u32 + b as u32) / 3) as u8;
    let gi = if gray < 8 { 0 } else { ((gray as i32 - 8) / 10).min(23) as u8 };
    let gv = 8 + 10 * gi;
    let gray_rgb = (gv, gv, gv);
    if dist((r, g, b), gray_rgb) < dist((r, g, b), cube_rgb) {
        232 + gi
    } else {
        16 + 36 * cr + 6 * cg + cb
    }
}

fn sgr_color(out: &mut String, c: Color, fg: bool, colors: Colors) {
    let base = if fg { 30 } else { 40 };
    match c {
        Color::Default => {
            let _ = write!(out, ";{}", base + 9);
        }
        Color::Idx(n) => match colors {
            Colors::C8 | Colors::C16 if n >= 16 => {
                let (r, g, b) = idx_rgb(n);
                let q = nearest((r, g, b), if colors == Colors::C8 { 8 } else { 16 });
                push16(out, q, fg);
            }
            _ if n < 16 => push16(out, if colors == Colors::C8 { n % 8 } else { n }, fg),
            _ => {
                let _ = write!(out, ";{};5;{}", base + 8, n);
            }
        },
        Color::Rgb(r, g, b) => match colors {
            Colors::True => {
                let _ = write!(out, ";{};2;{};{};{}", base + 8, r, g, b);
            }
            Colors::C256 => {
                let _ = write!(out, ";{};5;{}", base + 8, to_256(r, g, b));
            }
            Colors::C16 => push16(out, nearest((r, g, b), 16), fg),
            Colors::C8 => push16(out, nearest((r, g, b), 8), fg),
        },
    }
}

fn push16(out: &mut String, n: u8, fg: bool) {
    let base = if fg { 30 } else { 40 };
    if n < 8 {
        let _ = write!(out, ";{}", base + n as u32);
    } else {
        let _ = write!(out, ";{}", base + 60 + (n - 8) as u32);
    }
}

fn idx_rgb(n: u8) -> (u8, u8, u8) {
    if n < 16 {
        return ANSI16[n as usize];
    }
    if n >= 232 {
        let v = 8 + 10 * (n - 232);
        return (v, v, v);
    }
    let n = n - 16;
    let lv = |c: u8| -> u8 { if c == 0 { 0 } else { 55 + 40 * c } };
    (lv(n / 36), lv((n / 6) % 6), lv(n % 6))
}

fn sgr(out: &mut String, s: Style, colors: Colors) {
    out.push_str("\x1b[0");
    if s.mods & BOLD != 0 {
        out.push_str(";1");
    }
    if s.mods & DIM != 0 {
        out.push_str(";2");
    }
    if s.mods & ITALIC != 0 {
        out.push_str(";3");
    }
    if s.mods & UNDERLINE != 0 {
        out.push_str(";4");
    }
    if s.mods & REVERSE != 0 {
        out.push_str(";7");
    }
    if s.fg != Color::Default {
        sgr_color(out, s.fg, true, colors);
    }
    if s.bg != Color::Default {
        sgr_color(out, s.bg, false, colors);
    }
    out.push('m');
}

/// The shortest escape stream that turns `prev` into `cur`. The cursor is assumed at column 0 of row 0 of the
/// canvas before and is left there after, so the same stream works on the alternate screen and inline (rows below
/// the prompt). `prev == None` (or a different size) repaints everything.
pub fn diff(prev: Option<&Canvas>, cur: &Canvas, out: &mut String) {
    let full = !matches!(prev, Some(p) if p.w == cur.w && p.h == cur.h);
    let mut row = 0i32;
    let mut col = -1i32; // unknown until the first move
    let mut last: Option<Style> = None;
    for y in 0..cur.h {
        let mut x = 0;
        while x < cur.w {
            let i = (y * cur.w + x) as usize;
            let c = cur.cells[i];
            if c.ch == '\0' {
                x += 1;
                continue;
            }
            let same = !full && prev.map(|p| p.cells[i] == c).unwrap_or(false);
            // a changed right half of a wide glyph forces the whole glyph
            let tail_changed = c.ch != '\0' && char_width(c.ch) == 2 && !full && prev.map(|p| p.cells[i + 1] != cur.cells[i + 1]).unwrap_or(false);
            if same && !tail_changed {
                x += 1;
                continue;
            }
            if row != y {
                if y > row {
                    let _ = write!(out, "\x1b[{}B", y - row);
                } else {
                    let _ = write!(out, "\x1b[{}A", row - y);
                }
                row = y;
                col = -1;
            }
            if col != x {
                let _ = write!(out, "\x1b[{}G", x + 1);
                col = x;
            }
            if last != Some(c.style) {
                sgr(out, c.style, cur.caps.colors);
                last = Some(c.style);
            }
            out.push(c.ch);
            let w = char_width(c.ch).max(1) as i32;
            col += w;
            x += w;
        }
    }
    if last.is_some() {
        out.push_str("\x1b[0m");
    }
    if row > 0 {
        let _ = write!(out, "\x1b[{}A", row);
    }
    if col >= 0 || row > 0 {
        out.push('\r');
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_and_border_snapshot() {
        let mut c = Canvas::new(10, 3, Caps::FULL);
        c.border(Rect::new(0, 0, 10, 3), "hi", Style::DEFAULT, Style::DEFAULT);
        assert_eq!(c.to_text(), "╭─ hi ───╮\n│        │\n╰────────╯\n");
        let mut a = Canvas::new(10, 3, Caps::ASCII);
        a.border(Rect::new(0, 0, 10, 3), "", Style::DEFAULT, Style::DEFAULT);
        assert_eq!(a.to_text(), "+--------+\n|        |\n+--------+\n");
    }

    #[test]
    fn wide_glyph_overwrite_blanks_other_half() {
        let mut c = Canvas::new(6, 1, Caps::FULL);
        c.text(0, 0, "漢字", Style::DEFAULT);
        assert_eq!(c.to_text(), "漢字\n");
        c.put(1, 0, 'x', Style::DEFAULT);
        assert_eq!(c.to_text(), " x字\n");
        // a wide glyph that does not fit becomes a blank
        let mut d = Canvas::new(3, 1, Caps::FULL);
        d.text(2, 0, "漢", Style::DEFAULT);
        assert_eq!(d.to_text(), "\n");
    }

    #[test]
    fn clip_drops_outside_and_nonsense_is_safe() {
        let mut c = Canvas::new(5, 2, Caps::FULL);
        c.set_clip(Rect::new(1, 0, 2, 1));
        c.text(0, 0, "abcde", Style::DEFAULT);
        c.text(-5, 9, "zzz", Style::DEFAULT);
        c.fill(Rect::new(-9, -9, 99, 99), '#', Style::DEFAULT);
        assert_eq!(c.to_text(), " ##\n\n");
    }

    #[test]
    fn diff_emits_only_changes_and_returns_home() {
        let mut a = Canvas::new(8, 3, Caps::FULL);
        let b0 = a.clone();
        a.text(2, 1, "ab", Style::fg(rgb(1, 2, 3)));
        let mut s = String::new();
        diff(Some(&b0), &a, &mut s);
        assert_eq!(s, "\x1b[1B\x1b[3G\x1b[0;38;2;1;2;3mab\x1b[0m\x1b[1A\r");
        let mut none = String::new();
        diff(Some(&a), &a, &mut none);
        assert_eq!(none, "");
    }

    #[test]
    fn quantise_to_16_and_256() {
        let mut s = String::new();
        sgr_color(&mut s, rgb(241, 76, 76), true, Colors::C16);
        assert_eq!(s, ";91");
        let mut t = String::new();
        sgr_color(&mut t, rgb(0, 0, 0), false, Colors::C256);
        assert_eq!(t, ";48;5;16");
    }

    #[test]
    fn sizes_are_clamped() {
        let c = Canvas::new(-4, 99999, Caps::FULL);
        assert_eq!((c.w, c.h), (0, 10000));
    }
}
