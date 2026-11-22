//! Splitting a rectangle among children. Never fails: any rectangle, any sizes, results always sum to the whole.

use crate::canvas::Rect;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Size {
    Fixed(i32),
    Pct(i32),
    /// share of what is left, by weight
    Fill(u32),
    /// like `Fill(1)` but never below n
    Min(i32),
    /// like `Fill(1)` but never above n
    Max(i32),
}

impl Size {
    /// `5`, `fill`, `fill:3`, `pct:30`, `min:10`, `max:20`
    pub fn parse(s: &str) -> Option<Size> {
        let s = s.trim();
        if s == "fill" {
            return Some(Size::Fill(1));
        }
        if let Ok(n) = s.parse::<i32>() {
            return Some(Size::Fixed(n.max(0)));
        }
        let (k, v) = s.split_once(':')?;
        match k {
            "fill" => v.parse::<u32>().ok().map(|w| Size::Fill(w.max(1))),
            "pct" => v.parse::<i32>().ok().map(|p| Size::Pct(p.clamp(0, 100))),
            "min" => v.parse::<i32>().ok().map(|n| Size::Min(n.max(0))),
            "max" => v.parse::<i32>().ok().map(|n| Size::Max(n.max(0))),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Dir {
    Row,
    Col,
}

/// Lengths along the main axis (before rounding).
fn solve(total: i32, sizes: &[Size]) -> Vec<f64> {
    let total = total.max(0) as f64;
    let mut out = vec![0.0f64; sizes.len()];
    let mut left = total;
    // fixed and percentage first, in order; they get cut when there is not enough room
    for (i, s) in sizes.iter().enumerate() {
        let want = match *s {
            Size::Fixed(n) => n.max(0) as f64,
            Size::Pct(p) => (total * p.clamp(0, 100) as f64 / 100.0).floor(),
            _ => continue,
        };
        out[i] = want.min(left);
        left -= out[i];
    }
    // the flexible ones: distribute by weight, freeze whoever hits a bound, repeat
    let flex: Vec<usize> = (0..sizes.len()).filter(|&i| matches!(sizes[i], Size::Fill(_) | Size::Min(_) | Size::Max(_))).collect();
    let weight = |s: Size| match s {
        Size::Fill(w) => w as f64,
        _ => 1.0,
    };
    let mut frozen = vec![false; sizes.len()];
    for _ in 0..=flex.len() {
        let wsum: f64 = flex.iter().filter(|&&i| !frozen[i]).map(|&i| weight(sizes[i])).sum();
        if wsum <= 0.0 {
            break;
        }
        // the first one whose even share breaks its bound is frozen at the bound; then the rest share what is left
        let violator = flex.iter().copied().find(|&i| {
            let share = left.max(0.0) * weight(sizes[i]) / wsum;
            !frozen[i]
                && match sizes[i] {
                    Size::Min(n) => share < n.max(0) as f64,
                    Size::Max(n) => share > n.max(0) as f64,
                    _ => false,
                }
        });
        match violator {
            Some(i) => {
                let bound = match sizes[i] {
                    Size::Min(n) | Size::Max(n) => n.max(0) as f64,
                    _ => 0.0,
                };
                let g = bound.min(left.max(0.0));
                out[i] = g;
                left -= g;
                frozen[i] = true;
            }
            None => {
                for &i in &flex {
                    if !frozen[i] {
                        out[i] = left.max(0.0) * weight(sizes[i]) / wsum;
                    }
                }
                break;
            }
        }
    }
    out
}

/// Split `area` along `dir`. `gap` cells separate neighbours. Both edges of every rectangle are rounded (never the size),
/// so neighbours touch exactly and the sizes always add up.
pub fn split(area: Rect, dir: Dir, sizes: &[Size], gap: i32) -> Vec<Rect> {
    let n = sizes.len() as i32;
    if n == 0 {
        return Vec::new();
    }
    let gap = gap.max(0);
    let main = if dir == Dir::Row { area.w } else { area.h };
    let gaps = (gap * (n - 1)).min(main.max(0));
    let lens = solve(main - gaps, sizes);
    let mut rects = Vec::with_capacity(sizes.len());
    let origin = if dir == Dir::Row { area.x } else { area.y };
    let mut acc = 0.0f64; // sum of lengths before this one
    let gap_eff = if n > 1 { gaps as f64 / (n - 1) as f64 } else { 0.0 };
    for (i, l) in lens.iter().enumerate() {
        let start = acc + gap_eff * i as f64;
        let end = start + l;
        let (s, e) = (start.round() as i32, end.round() as i32);
        acc += l;
        let (s, e) = (s.min(main.max(0)), e.min(main.max(0)));
        rects.push(match dir {
            Dir::Row => Rect::new(origin + s, area.y, (e - s).max(0), area.h.max(0)),
            Dir::Col => Rect::new(area.x, origin + s, area.w.max(0), (e - s).max(0)),
        });
    }
    rects
}

#[cfg(test)]
mod tests {
    use super::*;

    fn widths(r: &[Rect]) -> Vec<i32> {
        r.iter().map(|r| r.w).collect()
    }

    #[test]
    fn one_two_one_sums_at_every_width() {
        for w in 4..=250 {
            let r = split(Rect::new(0, 0, w, 1), Dir::Row, &[Size::Fill(1), Size::Fill(2), Size::Fill(1)], 0);
            assert_eq!(widths(&r).iter().sum::<i32>(), w, "width {w}");
            // neighbours touch
            assert_eq!(r[0].x + r[0].w, r[1].x);
            assert_eq!(r[1].x + r[1].w, r[2].x);
        }
    }

    #[test]
    fn fixed_then_fill_and_pct() {
        let r = split(Rect::new(0, 0, 100, 1), Dir::Row, &[Size::Fixed(10), Size::Fill(1), Size::Pct(20)], 0);
        assert_eq!(widths(&r), vec![10, 70, 20]);
    }

    #[test]
    fn min_and_max_bounds() {
        let r = split(Rect::new(0, 0, 30, 1), Dir::Row, &[Size::Max(5), Size::Fill(1)], 0);
        assert_eq!(widths(&r), vec![5, 25]);
        let r = split(Rect::new(0, 0, 10, 1), Dir::Row, &[Size::Min(8), Size::Fill(1)], 0);
        assert_eq!(widths(&r), vec![8, 2]);
    }

    #[test]
    fn gaps_and_degrade() {
        let r = split(Rect::new(2, 3, 11, 4), Dir::Row, &[Size::Fill(1), Size::Fill(1)], 1);
        assert_eq!(widths(&r), vec![5, 5]);
        assert_eq!(r[1].x, 8);
        // too small: nothing negative, nothing outside
        for w in -3..6 {
            for h in -3..6 {
                let a = Rect::new(0, 0, w, h);
                for dir in [Dir::Row, Dir::Col] {
                    let r = split(a, dir, &[Size::Fixed(7), Size::Min(4), Size::Pct(50), Size::Fill(2)], 2);
                    for q in r {
                        assert!(q.w >= 0 && q.h >= 0);
                        assert!(q.w <= w.max(0) && q.h <= h.max(0));
                    }
                }
            }
        }
    }

    #[test]
    fn parse_sizes() {
        assert_eq!(Size::parse("5"), Some(Size::Fixed(5)));
        assert_eq!(Size::parse("fill:3"), Some(Size::Fill(3)));
        assert_eq!(Size::parse("pct:30"), Some(Size::Pct(30)));
        assert_eq!(Size::parse("max:9"), Some(Size::Max(9)));
        assert_eq!(Size::parse("wat"), None);
    }
}
