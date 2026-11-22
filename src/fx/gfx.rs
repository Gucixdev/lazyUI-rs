//! Small drawing helpers shared by the native widgets: colour mixing, ramps, eighth blocks, integer slicing.

use crate::canvas::{idx_rgb, Color};
use crate::theme::{parse_color, Theme};

pub fn rgb_of(c: Color) -> Option<(u8, u8, u8)> {
    match c {
        Color::Rgb(r, g, b) => Some((r, g, b)),
        Color::Idx(n) => Some(idx_rgb(n)),
        Color::Default => None,
    }
}

/// `a` towards `b` by `t` (0..1). A default colour has no value to mix, so `a` is kept.
pub fn mix(a: Color, b: Color, t: f64) -> Color {
    let (Some(x), Some(y)) = (rgb_of(a), rgb_of(b)) else { return a };
    let t = clamp01(t);
    let f = |p: u8, q: u8| (p as f64 + (q as f64 - p as f64) * t).round().clamp(0.0, 255.0) as u8;
    Color::Rgb(f(x.0, y.0), f(x.1, y.1), f(x.2, y.2))
}

pub fn clamp01(x: f64) -> f64 {
    if x.is_finite() {
        x.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// Green to yellow to red along the theme's own ok/warn/err colours: how bad is `f` (0 fine .. 1 bad).
pub fn level(t: &Theme, f: f64) -> Color {
    let f = clamp01(f);
    if f < 0.5 {
        mix(t.ok, t.warn, f * 2.0)
    } else {
        mix(t.warn, t.err, (f - 0.5) * 2.0)
    }
}

/// A theme colour by name, or `#rrggbb`.
pub fn colour(t: &Theme, name: &str) -> Option<Color> {
    Some(match name.trim() {
        "accent" => t.accent,
        "ok" => t.ok,
        "warn" => t.warn,
        "err" => t.err,
        "dim" => t.dim,
        "fg" | "text" => t.fg,
        other => return parse_color(other),
    })
}

/// Black or white text, whichever reads better on `bg`.
pub fn ink_for(bg: Color, t: &Theme) -> Color {
    match rgb_of(bg) {
        Some((r, g, b)) => {
            if 0.299 * r as f64 + 0.587 * g as f64 + 0.114 * b as f64 > 140.0 {
                Color::Rgb(0, 0, 0)
            } else {
                Color::Rgb(255, 255, 255)
            }
        }
        None => t.bg,
    }
}

/// The lower `n` eighths of a cell (n 1..=8) as a block character, or a coarse ASCII stand-in.
pub fn eighth(n: i32, unicode: bool) -> char {
    const PART: [char; 9] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let n = n.clamp(0, 8) as usize;
    if unicode {
        PART[n]
    } else if n == 0 {
        ' '
    } else if n >= 7 {
        '#'
    } else if n >= 4 {
        '='
    } else {
        '.'
    }
}

/// Widths in whole cells that follow `weights` and add up to exactly `total` (largest remainder).
pub fn slice(weights: &[f64], total: i32) -> Vec<i32> {
    let w: Vec<f64> = weights.iter().map(|x| if x.is_finite() && *x > 0.0 { *x } else { 0.0 }).collect();
    let sum: f64 = w.iter().sum();
    if sum <= 0.0 || total <= 0 {
        return vec![0; w.len()];
    }
    let q: Vec<f64> = w.iter().map(|x| x / sum * total as f64).collect();
    let mut out: Vec<i32> = q.iter().map(|x| x.floor() as i32).collect();
    let mut rem: Vec<f64> = q.iter().zip(&out).map(|(a, b)| a - *b as f64).collect();
    let mut left = total - out.iter().sum::<i32>();
    while left > 0 {
        let mut best = 0;
        for i in 1..rem.len() {
            if rem[i] > rem[best] {
                best = i;
            }
        }
        out[best] += 1;
        rem[best] = -1.0;
        left -= 1;
    }
    out
}

/// A grid of braille dots: each terminal cell holds 2 x 4 dots, so a line chart gets eight times the resolution.
pub struct Braille {
    pub cw: i32,
    pub ch: i32,
    bits: Vec<u8>,
}

impl Braille {
    pub fn new(cw: i32, ch: i32) -> Braille {
        let (cw, ch) = (cw.max(0), ch.max(0));
        Braille { cw, ch, bits: vec![0; (cw * ch) as usize] }
    }
    pub fn dots_w(&self) -> i32 {
        self.cw * 2
    }
    pub fn dots_h(&self) -> i32 {
        self.ch * 4
    }
    pub fn set(&mut self, x: i32, y: i32) {
        if x < 0 || y < 0 || x >= self.dots_w() || y >= self.dots_h() {
            return;
        }
        const BIT: [[u8; 4]; 2] = [[0x01, 0x02, 0x04, 0x40], [0x08, 0x10, 0x20, 0x80]];
        self.bits[((y / 4) * self.cw + x / 2) as usize] |= BIT[(x % 2) as usize][(y % 4) as usize];
    }
    /// A straight line between two dots (Bresenham).
    pub fn line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32) {
        let (dx, dy) = ((x1 - x0).abs(), -(y1 - y0).abs());
        let (sx, sy) = (if x0 < x1 { 1 } else { -1 }, if y0 < y1 { 1 } else { -1 });
        let (mut x, mut y, mut err) = (x0, y0, dx + dy);
        loop {
            self.set(x, y);
            if x == x1 && y == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }
    pub fn bits(&self, cx: i32, cy: i32) -> u8 {
        self.bits[(cy * self.cw + cx) as usize]
    }
    /// The character for a cell: braille, or `*` / `.` / `'` on a terminal without it.
    pub fn glyph(bits: u8, unicode: bool) -> char {
        if bits == 0 {
            ' '
        } else if unicode {
            char::from_u32(0x2800 + bits as u32).unwrap_or(' ')
        } else if bits & 0x0F != 0 && bits & 0xF0 != 0 || bits & 0xC0 != 0 && bits & 0x3F != 0 {
            '*'
        } else {
            ':'
        }
    }
}

/// A comma separated list of numbers; what does not parse is skipped.
pub fn numbers(s: &str) -> Vec<f64> {
    s.split(',').filter_map(|x| x.trim().parse::<f64>().ok()).filter(|x| x.is_finite()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slices_add_up_exactly() {
        for total in 0..60 {
            for w in [vec![1.0, 1.0, 1.0], vec![5.0, 0.0, 2.0, 9.0], vec![0.1, 0.2], vec![7.0]] {
                let s = slice(&w, total);
                assert_eq!(s.iter().sum::<i32>(), total, "{w:?} into {total}: {s:?}");
                assert!(s.iter().all(|x| *x >= 0));
            }
        }
        assert_eq!(slice(&[0.0, -3.0, f64::NAN], 10), vec![0, 0, 0]);
    }

    #[test]
    fn mixing_and_ramps() {
        assert_eq!(mix(Color::Rgb(0, 0, 0), Color::Rgb(100, 200, 50), 0.5), Color::Rgb(50, 100, 25));
        assert_eq!(mix(Color::Default, Color::Rgb(1, 2, 3), 0.5), Color::Default);
        let t = Theme::green();
        assert_eq!(level(&t, 0.0), t.ok);
        assert_eq!(level(&t, 1.0), t.err);
        assert_eq!(level(&t, f64::NAN), t.ok);
    }

    #[test]
    fn braille_dots_and_lines() {
        let mut b = Braille::new(2, 1);
        b.set(0, 0);
        b.set(1, 3);
        assert_eq!(b.bits(0, 0), 0x01 | 0x80);
        b.set(99, 99); // outside: ignored
        b.line(0, 0, 3, 3);
        assert!(b.bits(1, 0) != 0);
        assert_eq!(Braille::glyph(0x01, true), '\u{2801}');
        assert_eq!(Braille::glyph(0, false), ' ');
    }

    #[test]
    fn eighths_and_numbers() {
        assert_eq!(eighth(8, true), '█');
        assert_eq!(eighth(8, false), '#');
        assert_eq!(eighth(0, true), ' ');
        assert_eq!(numbers("1, 2.5,x,,NaN, -3"), vec![1.0, 2.5, -3.0]);
    }
}
