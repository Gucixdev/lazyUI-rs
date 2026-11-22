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
    fn eighths_and_numbers() {
        assert_eq!(eighth(8, true), '█');
        assert_eq!(eighth(8, false), '#');
        assert_eq!(eighth(0, true), ' ');
        assert_eq!(numbers("1, 2.5,x,,NaN, -3"), vec![1.0, 2.5, -3.0]);
    }
}
