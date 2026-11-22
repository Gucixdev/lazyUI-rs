//! Cells to pixels. One font, one cell size; only cells that changed since the last frame are repainted.
//! Box-drawing and block glyphs are drawn procedurally so they join across cells at any size.

use fontdue::{Font, FontSettings};
use crate::canvas::{Canvas, Cell, Color, Style, BOLD, DIM, REVERSE, UNDERLINE};
use std::collections::HashMap;

pub struct Pixels {
    pub w: usize,
    pub h: usize,
    pub rgba: Vec<u8>,
}

impl Pixels {
    pub fn new(w: usize, h: usize, bg: [u8; 3]) -> Pixels {
        let mut rgba = vec![255u8; w * h * 4];
        for p in rgba.chunks_exact_mut(4) {
            p[..3].copy_from_slice(&bg);
        }
        Pixels { w, h, rgba }
    }
}

pub struct Raster {
    font: Font,
    px: f32,
    pub cw: usize,
    pub ch: usize,
    ascent: i32,
    cache: HashMap<char, (fontdue::Metrics, Vec<u8>)>,
    pub default_fg: [u8; 3],
    pub default_bg: [u8; 3],
}

const ANSI16: [[u8; 3]; 16] = [
    [0, 0, 0], [205, 49, 49], [13, 188, 121], [229, 229, 16], [36, 114, 200], [188, 63, 188], [17, 168, 205], [229, 229, 229],
    [102, 102, 102], [241, 76, 76], [35, 209, 139], [245, 245, 67], [59, 142, 234], [214, 112, 214], [41, 184, 219], [255, 255, 255],
];

pub fn palette(i: u8) -> [u8; 3] {
    match i {
        0..=15 => ANSI16[i as usize],
        16..=231 => {
            let n = i as usize - 16;
            let lv = |v: usize| if v == 0 { 0 } else { (55 + v * 40) as u8 };
            [lv(n / 36), lv(n / 6 % 6), lv(n % 6)]
        }
        _ => {
            let g = (8 + (i as usize - 232) * 10) as u8;
            [g, g, g]
        }
    }
}

fn mix(a: [u8; 3], b: [u8; 3], t: f32) -> [u8; 3] {
    let f = |x: u8, y: u8| (x as f32 * (1.0 - t) + y as f32 * t).round() as u8;
    [f(a[0], b[0]), f(a[1], b[1]), f(a[2], b[2])]
}

impl Raster {
    pub fn from_bytes(data: &[u8], px: f32) -> Result<Raster, String> {
        let font = Font::from_bytes(data, FontSettings::default()).map_err(|e| e.to_string())?;
        let lm = font.horizontal_line_metrics(px).ok_or("the font has no line metrics")?;
        let ch = (lm.ascent - lm.descent + lm.line_gap).ceil().max(1.0) as usize;
        let cw = font.metrics('M', px).advance_width.round().max(1.0) as usize;
        Ok(Raster { font, px, cw, ch, ascent: lm.ascent.round() as i32, cache: HashMap::new(), default_fg: [205, 230, 205], default_bg: [10, 20, 12] })
    }

    /// A monospace font from the system (`fc-match`), or the first of a few well-known paths.
    pub fn system(px: f32, path: Option<&str>) -> Result<Raster, String> {
        let mut tried = Vec::new();
        let mut cands: Vec<String> = Vec::new();
        if let Some(p) = path {
            cands.push(p.to_string());
        } else {
            if let Ok(o) = std::process::Command::new("fc-match").args(["-f", "%{file}", "DejaVu Sans Mono"]).output() {
                cands.push(String::from_utf8_lossy(&o.stdout).to_string());
            }
            cands.push("/usr/share/fonts/TTF/DejaVuSansMono.ttf".into());
            cands.push("/usr/share/fonts/dejavu/DejaVuSansMono.ttf".into());
            cands.push("/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf".into());
            cands.push("/usr/share/fonts/liberation/LiberationMono-Regular.ttf".into());
        }
        for c in cands {
            if c.is_empty() {
                continue;
            }
            match std::fs::read(&c) {
                Ok(b) => return Raster::from_bytes(&b, px).map_err(|e| format!("{c}: {e}")),
                Err(_) => tried.push(c),
            }
        }
        Err(format!("no monospace font found (tried {}); pass --font FILE.ttf", tried.join(", ")))
    }

    fn rgb(&self, c: Color, fallback: [u8; 3]) -> [u8; 3] {
        match c {
            Color::Default => fallback,
            Color::Idx(i) => palette(i),
            Color::Rgb(r, g, b) => [r, g, b],
        }
    }

    fn colors(&self, s: Style) -> ([u8; 3], [u8; 3]) {
        let mut fg = self.rgb(s.fg, self.default_fg);
        let mut bg = self.rgb(s.bg, self.default_bg);
        if s.mods & REVERSE != 0 {
            std::mem::swap(&mut fg, &mut bg);
        }
        if s.mods & DIM != 0 {
            fg = mix(fg, bg, 0.45);
        }
        if s.mods & BOLD != 0 {
            fg = mix(fg, [255, 255, 255], 0.18);
        }
        (fg, bg)
    }

    /// Repaint into `px` every cell of `cur` that differs from `prev` (all of them when `prev` is None or another size).
    /// Returns how many cells were painted.
    pub fn paint(&mut self, px: &mut Pixels, prev: Option<&Canvas>, cur: &Canvas) -> usize {
        let same = matches!(prev, Some(p) if p.w == cur.w && p.h == cur.h);
        let mut n = 0;
        for y in 0..cur.h {
            for x in 0..cur.w {
                let Some(c) = cur.get(x, y) else { continue };
                if c.ch == '\0' {
                    continue;
                }
                let wide = crate::canvas::char_width(c.ch) == 2;
                let tail_changed = wide && same && prev.map(|p| p.get(x + 1, y) != cur.get(x + 1, y)).unwrap_or(false);
                if same && prev.map(|p| p.get(x, y) == Some(c)).unwrap_or(false) && !tail_changed {
                    continue;
                }
                self.cell(px, x as usize, y as usize, c, wide);
                n += 1;
            }
        }
        n
    }

    fn fill(&self, px: &mut Pixels, x0: usize, y0: usize, w: usize, h: usize, col: [u8; 3]) {
        for y in y0..(y0 + h).min(px.h) {
            for x in x0..(x0 + w).min(px.w) {
                let o = (y * px.w + x) * 4;
                px.rgba[o..o + 3].copy_from_slice(&col);
            }
        }
    }

    fn blend(&self, px: &mut Pixels, x: i64, y: i64, col: [u8; 3], cov: u8) {
        if x < 0 || y < 0 || x as usize >= px.w || y as usize >= px.h || cov == 0 {
            return;
        }
        let o = (y as usize * px.w + x as usize) * 4;
        let t = cov as f32 / 255.0;
        for k in 0..3 {
            px.rgba[o + k] = (px.rgba[o + k] as f32 * (1.0 - t) + col[k] as f32 * t).round() as u8;
        }
    }

    fn cell(&mut self, px: &mut Pixels, cx: usize, cy: usize, c: Cell, wide: bool) {
        let (fg, bg) = self.colors(c.style);
        let (x0, y0) = (cx * self.cw, cy * self.ch);
        let span = if wide { self.cw * 2 } else { self.cw };
        self.fill(px, x0, y0, span, self.ch, bg);
        if c.ch != ' ' && !self.procedural(px, x0, y0, c.ch, fg, bg) {
            let (px_size, ascent) = (self.px, self.ascent);
            let font = &self.font;
            let entry = self.cache.entry(c.ch).or_insert_with(|| font.rasterize(c.ch, px_size));
            let (m, bitmap) = (entry.0, entry.1.clone());
            let top = y0 as i64 + ascent as i64 - m.height as i64 - m.ymin as i64;
            for gy in 0..m.height {
                for gx in 0..m.width {
                    self.blend(px, x0 as i64 + m.xmin as i64 + gx as i64, top + gy as i64, fg, bitmap[gy * m.width + gx]);
                }
            }
        }
        if c.style.mods & UNDERLINE != 0 {
            self.fill(px, x0, y0 + self.ch - 2, span, 1, fg);
        }
    }

    /// Lines, corners and blocks that must touch their neighbours. True when handled.
    fn procedural(&self, px: &mut Pixels, x0: usize, y0: usize, ch: char, fg: [u8; 3], bg: [u8; 3]) -> bool {
        let (w, h) = (self.cw, self.ch);
        let t = (h / 14).max(1); // line thickness
        let (mx, my) = (x0 + w / 2 - t / 2, y0 + h / 2 - t / 2);
        // (left, right, up, down) arms from the centre
        let arms = match ch {
            '─' => Some((1, 1, 0, 0)),
            '│' => Some((0, 0, 1, 1)),
            '╭' | '┌' => Some((0, 1, 0, 1)),
            '╮' | '┐' => Some((1, 0, 0, 1)),
            '╰' | '└' => Some((0, 1, 1, 0)),
            '╯' | '┘' => Some((1, 0, 1, 0)),
            '├' => Some((0, 1, 1, 1)),
            '┤' => Some((1, 0, 1, 1)),
            '┬' => Some((1, 1, 0, 1)),
            '┴' => Some((1, 1, 1, 0)),
            '┼' => Some((1, 1, 1, 1)),
            _ => None,
        };
        if let Some((l, r, u, d)) = arms {
            if l == 1 {
                self.fill(px, x0, my, mx - x0 + t, t, fg);
            }
            if r == 1 {
                self.fill(px, mx, my, x0 + w - mx, t, fg);
            }
            if u == 1 {
                self.fill(px, mx, y0, t, my - y0 + t, fg);
            }
            if d == 1 {
                self.fill(px, mx, my, t, y0 + h - my, fg);
            }
            return true;
        }
        let shade = match ch {
            '█' => Some(1.0),
            '▓' => Some(0.75),
            '▒' => Some(0.5),
            '░' => Some(0.25),
            _ => None,
        };
        if let Some(s) = shade {
            self.fill(px, x0, y0, w, h, mix(bg, fg, s));
            return true;
        }
        match ch {
            '▀' => self.fill(px, x0, y0, w, h / 2, fg),
            '▄' => self.fill(px, x0, y0 + h / 2, w, h - h / 2, fg),
            '▌' => self.fill(px, x0, y0, w / 2, h, fg),
            '▐' => self.fill(px, x0 + w / 2, y0, w - w / 2, h, fg),
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::{Caps, Rect};

    fn raster() -> Option<Raster> {
        Raster::system(16.0, None).ok()
    }

    #[test]
    fn cell_size_is_sane_and_text_leaves_ink() {
        let Some(mut r) = raster() else { return };
        assert!(r.cw >= 6 && r.cw <= 16 && r.ch >= 12 && r.ch <= 30, "{}x{}", r.cw, r.ch);
        let mut c = Canvas::new(6, 2, Caps::FULL);
        c.text(0, 0, "Hi", Style::new(crate::canvas::rgb(255, 255, 255), crate::canvas::rgb(0, 0, 0)));
        let mut px = Pixels::new(6 * r.cw, 2 * r.ch, [0, 0, 0]);
        assert_eq!(r.paint(&mut px, None, &c), 12);
        let lit = px.rgba.chunks_exact(4).filter(|p| p[0] > 128).count();
        assert!(lit > 20, "no glyph ink: {lit}");
    }

    #[test]
    fn only_changed_cells_repaint() {
        let Some(mut r) = raster() else { return };
        let mut a = Canvas::new(8, 3, Caps::FULL);
        let mut px = Pixels::new(8 * r.cw, 3 * r.ch, [0, 0, 0]);
        r.paint(&mut px, None, &a);
        let mut b = a.clone();
        b.text(2, 1, "ab", Style::DEFAULT);
        assert_eq!(r.paint(&mut px, Some(&a), &b), 2);
        a = b.clone();
        assert_eq!(r.paint(&mut px, Some(&a), &b), 0);
    }

    #[test]
    fn box_lines_join_across_cells() {
        let Some(mut r) = raster() else { return };
        let mut c = Canvas::new(3, 3, Caps::FULL);
        c.border(Rect::new(0, 0, 3, 3), "", Style::fg(crate::canvas::rgb(255, 255, 255)), Style::DEFAULT);
        let mut px = Pixels::new(3 * r.cw, 3 * r.ch, [0, 0, 0]);
        r.paint(&mut px, None, &c);
        // the vertical line at the left edge column must be continuous over the full height of the middle cell
        let x = r.cw / 2;
        for y in 0..r.ch {
            let o = ((r.ch + y) * px.w + x) * 4;
            assert!(px.rgba[o] > 200 || px.rgba[o - 4] > 200, "gap at row {y}");
        }
    }
}
