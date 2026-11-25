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

/// A rectangle of pixels (x, y, width, height), what changed since the last frame.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PxRect {
    pub x: usize,
    pub y: usize,
    pub w: usize,
    pub h: usize,
}

impl PxRect {
    pub fn is_empty(&self) -> bool {
        self.w == 0 || self.h == 0
    }
    pub fn union(self, o: PxRect) -> PxRect {
        if self.is_empty() {
            return o;
        }
        if o.is_empty() {
            return self;
        }
        let (x, y) = (self.x.min(o.x), self.y.min(o.y));
        let (r, b) = ((self.x + self.w).max(o.x + o.w), (self.y + self.h).max(o.y + o.h));
        PxRect { x, y, w: r - x, h: b - y }
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

    /// The pixels a cell rectangle covers, one cell wider on every side (a glyph or a wide character may spill over its
    /// cell), cut to the picture.
    pub fn px_rect(&self, cells: crate::canvas::Rect, px: &Pixels) -> PxRect {
        let (cw, ch) = (self.cw as i32, self.ch as i32);
        let x0 = ((cells.x - 1).max(0) * cw) as usize;
        let y0 = ((cells.y - 1).max(0) * ch) as usize;
        let x1 = (((cells.x + cells.w + 1) * cw) as usize).min(px.w);
        let y1 = (((cells.y + cells.h + 1) * ch) as usize).min(px.h);
        PxRect { x: x0.min(px.w), y: y0.min(px.h), w: x1.saturating_sub(x0), h: y1.saturating_sub(y0) }
    }

    /// Repaint into `px` every cell of `cur` that differs from `prev` (all of them when `prev` is None or another size).
    /// Returns how many cells were painted.
    pub fn paint(&mut self, px: &mut Pixels, prev: Option<&Canvas>, cur: &Canvas) -> usize {
        self.paint_in(px, prev, cur, None)
    }

    /// `paint`, looking only inside `region` (cells; `crate::canvas::damage` gives the box around every difference). A wide
    /// glyph whose right half changed is found because the scan starts one column to the left of the region.
    pub fn paint_in(&mut self, px: &mut Pixels, prev: Option<&Canvas>, cur: &Canvas, region: Option<crate::canvas::Rect>) -> usize {
        let same = matches!(prev, Some(p) if p.w == cur.w && p.h == cur.h);
        let (x0, x1, y0, y1) = match region.filter(|_| same) {
            Some(r) => ((r.x - 1).max(0), (r.x + r.w).min(cur.w), r.y.max(0), (r.y + r.h).min(cur.h)),
            None => (0, cur.w, 0, cur.h),
        };
        let mut n = 0;
        for y in y0..y1 {
            for x in x0..x1 {
                let Some(c) = cur.get(x, y) else { continue };
                if c.ch == '\0' {
                    continue;
                }
                let wide = crate::canvas::char_width(c.ch) == 2;
                let tail_changed = wide && same && prev.map(|p| p.get(x + 1, y) != cur.get(x + 1, y)).unwrap_or(false);
                if same && prev.map(|p| p.get(x, y) == Some(c)).unwrap_or(false) && !tail_changed {
                    continue;
                }
                match cur.images().iter().find(|p| p.rect.contains(x, y) && crate::media::half_cell(&p.img, p.rect, x, y, cur.caps.unicode) == Some(c)) {
                    Some(p) => self.image_cell(px, x as usize, y as usize, p),
                    None => self.cell(px, x as usize, y as usize, c, wide),
                }
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

    /// One cell of a placed picture in real pixels: the part of the fitted picture that falls in this cell, the window
    /// background where the picture does not reach.
    fn image_cell(&self, px: &mut Pixels, cx: usize, cy: usize, p: &crate::media::Placement) {
        let (dw, dh) = (p.rect.w as usize * self.cw, p.rect.h as usize * self.ch);
        let (ox, oy) = ((cx - p.rect.x as usize) * self.cw, (cy - p.rect.y as usize) * self.ch);
        for y in 0..self.ch {
            for x in 0..self.cw {
                let (gx, gy) = (cx * self.cw + x, cy * self.ch + y);
                if gx >= px.w || gy >= px.h {
                    continue;
                }
                let col = p.img.sample(dw, dh, ox + x, oy + y).unwrap_or(self.default_bg);
                let o = (gy * px.w + gx) * 4;
                px.rgba[o..o + 3].copy_from_slice(&col);
            }
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
    fn damage_in_pixels_is_the_cell_box_one_cell_wider_and_cut_to_the_picture() {
        let Some(r) = raster() else { return };
        let px = Pixels::new(10 * r.cw, 6 * r.ch, [0, 0, 0]);
        let a = r.px_rect(Rect::new(4, 2, 2, 1), &px);
        assert_eq!((a.x, a.y, a.w, a.h), (3 * r.cw, r.ch, 4 * r.cw, 3 * r.ch));
        let edge = r.px_rect(Rect::new(0, 0, 10, 6), &px);
        assert_eq!((edge.x, edge.y, edge.w, edge.h), (0, 0, px.w, px.h), "never past the picture");
        assert_eq!(PxRect { x: 0, y: 0, w: 0, h: 0 }.union(a), a);
    }

    #[test]
    fn painting_inside_the_damage_box_gives_the_same_pixels_as_painting_everything() {
        let Some(mut r) = raster() else { return };
        let (w, h) = (24, 8);
        let mut a = Canvas::new(w, h, Caps::FULL);
        a.text(0, 0, "hello wide 世界 end", Style::default());
        a.text(2, 5, "below", Style::default());
        let mut b = Canvas::new(w, h, Caps::FULL);
        b.text(0, 0, "hello wide 世界 end", Style::default());
        b.text(2, 5, "below", Style::default());
        b.text(11, 0, "世", Style::new(crate::canvas::rgb(255, 0, 0), crate::canvas::rgb(0, 0, 80)));
        b.text(3, 3, "x", Style::new(crate::canvas::rgb(0, 255, 0), crate::canvas::rgb(0, 0, 0)));
        let (mut full, mut part) = (Pixels::new(w as usize * r.cw, h as usize * r.ch, [0, 0, 0]), Pixels::new(w as usize * r.cw, h as usize * r.ch, [0, 0, 0]));
        r.paint(&mut full, None, &a);
        r.paint(&mut part, None, &a);
        let n_full = r.paint(&mut full, Some(&a), &b);
        let d = crate::canvas::damage(Some(&a), &b);
        let n_part = r.paint_in(&mut part, Some(&a), &b, d);
        assert_eq!(n_full, n_part);
        assert!(full.rgba == part.rgba, "pixels differ");
        // the right half of a wide glyph changing alone is still found
        let mut c = b.clone();
        c.text(12, 0, "x", Style::default());
        let n = r.paint_in(&mut part, Some(&b), &c, crate::canvas::damage(Some(&b), &c));
        let mut again = full;
        r.paint(&mut again, Some(&b), &c);
        assert!(n >= 1 && again.rgba == part.rgba, "wide tail change lost");
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

    #[test]
    fn a_placed_picture_is_painted_in_pixels_and_text_over_it_wins() {
        use crate::canvas::{Caps, Canvas, Rect, Style};
        use crate::media::Image;
        let Ok(mut r) = Raster::system(14.0, None) else { return };
        let img = std::sync::Arc::new(Image::new(2, 2, [200u8, 10, 10, 255].iter().copied().cycle().take(16).collect()).unwrap());
        let mut c = Canvas::new(6, 4, Caps::FULL);
        c.image(Rect::new(0, 0, 6, 2), &img);
        let mut px = Pixels::new(6 * r.cw, 4 * r.ch, [0, 0, 0]);
        r.paint(&mut px, None, &c);
        let at = |px: &Pixels, x: usize, y: usize| { let o = (y * px.w + x) * 4; [px.rgba[o], px.rgba[o + 1], px.rgba[o + 2]] };
        // a square picture in 6 x 2 cells (6 cw by about 4 cw): bars of about one cell at the left and right
        assert_eq!(at(&px, r.cw * 3 + 1, r.ch / 2), [200, 10, 10], "inside the picture");
        assert_eq!(at(&px, 2, r.ch / 2), r.default_bg, "the bar at the left");
        // a letter drawn over the picture afterwards is a glyph, not picture
        c.put(2, 0, 'W', Style::fg(crate::canvas::rgb(255, 255, 255)));
        let mut again = Pixels::new(6 * r.cw, 4 * r.ch, [0, 0, 0]);
        r.paint(&mut again, None, &c);
        let lit = (0..r.ch).flat_map(|y| (0..r.cw).map(move |x| (x, y))).filter(|&(x, y)| at(&again, r.cw * 2 + x, y) != [200, 10, 10]).count();
        assert!(lit as usize > r.cw * r.ch / 2, "the cell is a glyph on the window background, not picture: {lit}");
        assert_eq!(at(&again, r.cw * 3 + 2, r.ch / 2), [200, 10, 10], "the neighbour cell is still picture");
    }
}
