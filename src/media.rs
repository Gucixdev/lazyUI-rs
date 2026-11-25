//! Pictures inside the cell grid. An `Image` is decoded RGBA pixels (decoding is the host's job: filem8 has its own
//! codecs, a web page has the browser); `Canvas::image` puts one into a rectangle of cells. Every cell of it also gets
//! a half-block fallback (`▀`, top pixel as foreground, bottom as background), so the terminal and the snapshots show
//! the picture too; the native window's rasteriser recognises those cells and paints real pixels instead, unless
//! something was drawn over them afterwards.

use crate::canvas::{rgb, Canvas, Cell, Rect, Style};
use crate::native::{Cx, Def, Pure};
use std::sync::Arc;

pub static DEFS: &[Def] = &[Def {
    name: "image",
    doc: "a picture registered with Ui::set_image(KEY): bind=KEY, alt= shown until it is there; fitted whole and centred",
    focus: false,
    rows: None,
    live: 0.0,
    make: || Box::new(Pure(draw)),
}];

fn draw(c: &mut Canvas, r: Rect, cx: &Cx) {
    match cx.attr("bind").and_then(|k| cx.image(k)) {
        Some(img) => c.image(r, img),
        None => {
            let alt = cx.text_attr("alt", "");
            let w = crate::canvas::str_width(&alt) as i32;
            c.text_fit(r.x + ((r.w - w) / 2).max(0), r.y + r.h / 2, r.w, &alt, Style::fg(cx.theme.dim).on(cx.theme.bg));
        }
    }
}

pub struct Image {
    pub w: usize,
    pub h: usize,
    /// `w * h * 4` bytes, straight (not premultiplied) alpha
    pub rgba: Vec<u8>,
}

impl Image {
    pub fn new(w: usize, h: usize, rgba: Vec<u8>) -> Option<Image> {
        (w > 0 && h > 0 && rgba.len() == w * h * 4).then_some(Image { w, h, rgba })
    }

    /// The colour at pixel (x, y) of a `dw` x `dh` area the picture is fitted into whole and centred (so a wide picture
    /// leaves bars above and below). None outside the fitted picture and where it is mostly transparent.
    pub fn sample(&self, dw: usize, dh: usize, x: usize, y: usize) -> Option<[u8; 3]> {
        if dw == 0 || dh == 0 {
            return None;
        }
        let scale = (dw as f64 / self.w as f64).min(dh as f64 / self.h as f64);
        let (fw, fh) = (self.w as f64 * scale, self.h as f64 * scale);
        let (ox, oy) = ((dw as f64 - fw) / 2.0, (dh as f64 - fh) / 2.0);
        let (fx, fy) = (x as f64 + 0.5 - ox, y as f64 + 0.5 - oy);
        if fx < 0.0 || fy < 0.0 || fx >= fw || fy >= fh {
            return None;
        }
        let (sx, sy) = (((fx / scale) as usize).min(self.w - 1), ((fy / scale) as usize).min(self.h - 1));
        let p = &self.rgba[(sy * self.w + sx) * 4..][..4];
        (p[3] >= 128).then_some([p[0], p[1], p[2]])
    }
}

/// An image put on a canvas: its whole rectangle (cells drawn over it later are not painted as picture).
#[derive(Clone)]
pub struct Placement {
    pub rect: Rect,
    pub img: Arc<Image>,
}

/// The cell the fallback leaves at canvas cell (x, y) of a placement, or None where the picture does not reach (the bars).
/// Each cell is one column by two picture rows.
pub fn half_cell(img: &Image, rect: Rect, x: i32, y: i32, unicode: bool) -> Option<Cell> {
    let (dw, dh) = (rect.w.max(0) as usize, (rect.h * 2).max(0) as usize);
    let (cx, cy) = ((x - rect.x) as usize, ((y - rect.y) * 2) as usize);
    let top = img.sample(dw, dh, cx, cy);
    let bottom = img.sample(dw, dh, cx, cy + 1);
    let c = |p: [u8; 3]| rgb(p[0], p[1], p[2]);
    match (top, bottom, unicode) {
        (None, None, _) => None,
        (Some(t), Some(b), true) => Some(Cell { ch: '▀', style: Style::new(c(t), c(b)) }),
        (Some(t), None, true) => Some(Cell { ch: '▀', style: Style::fg(c(t)) }),
        (None, Some(b), true) => Some(Cell { ch: '▄', style: Style::fg(c(b)) }),
        (Some(t), _, false) => Some(Cell { ch: ' ', style: Style::new(c(t), c(t)) }),
        (None, Some(b), false) => Some(Cell { ch: ' ', style: Style::new(c(b), c(b)) }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(w: usize, h: usize, p: [u8; 4]) -> Image {
        Image::new(w, h, p.iter().copied().cycle().take(w * h * 4).collect()).unwrap()
    }

    #[test]
    fn a_wrong_length_or_empty_picture_is_refused() {
        assert!(Image::new(2, 2, vec![0; 15]).is_none());
        assert!(Image::new(0, 2, vec![]).is_none());
    }

    #[test]
    fn a_wide_picture_is_fitted_whole_with_bars_above_and_below() {
        let img = flat(4, 2, [10, 20, 30, 255]);
        // 8 x 8 area: the picture is 8 wide, 4 tall, so rows 0-1 and 6-7 are bars
        assert_eq!(img.sample(8, 8, 3, 0), None);
        assert_eq!(img.sample(8, 8, 3, 2), Some([10, 20, 30]));
        assert_eq!(img.sample(8, 8, 3, 5), Some([10, 20, 30]));
        assert_eq!(img.sample(8, 8, 3, 6), None);
        assert_eq!(img.sample(0, 8, 0, 0), None);
    }

    #[test]
    fn pixels_map_to_the_right_source_pixel_and_transparent_ones_are_left_out() {
        let mut px = vec![0u8; 2 * 2 * 4];
        px[..4].copy_from_slice(&[255, 0, 0, 255]);
        px[4..8].copy_from_slice(&[0, 255, 0, 255]);
        px[8..12].copy_from_slice(&[0, 0, 255, 255]);
        px[12..16].copy_from_slice(&[0, 0, 0, 0]);
        let img = Image::new(2, 2, px).unwrap();
        assert_eq!(img.sample(2, 2, 0, 0), Some([255, 0, 0]));
        assert_eq!(img.sample(2, 2, 1, 0), Some([0, 255, 0]));
        assert_eq!(img.sample(2, 2, 0, 1), Some([0, 0, 255]));
        assert_eq!(img.sample(2, 2, 1, 1), None);
        assert_eq!(img.sample(4, 4, 3, 3), None, "scaled up, the transparent corner stays transparent");
        assert_eq!(img.sample(4, 4, 2, 0), Some([0, 255, 0]));
    }

    #[test]
    fn the_fallback_cell_has_the_top_pixel_in_front_and_the_bottom_behind() {
        let mut px = vec![255u8; 1 * 2 * 4];
        px[..4].copy_from_slice(&[1, 2, 3, 255]);
        px[4..8].copy_from_slice(&[4, 5, 6, 255]);
        let img = Image::new(1, 2, px).unwrap();
        let r = Rect::new(5, 7, 1, 1);
        assert_eq!(half_cell(&img, r, 5, 7, true), Some(Cell { ch: '▀', style: Style::new(rgb(1, 2, 3), rgb(4, 5, 6)) }));
        assert_eq!(half_cell(&img, r, 5, 7, false).map(|c| c.ch), Some(' '));
    }

    #[test]
    fn the_image_element_shows_a_registered_picture_and_its_alt_text_before_that() {
        use crate::headless::{run_script, snapshot};
        let mut ui = crate::ui::Ui::from_spec("col\n  image id=p bind=thumb alt=loading\n").unwrap();
        run_script(&mut ui, "size 10 4").unwrap();
        assert!(snapshot(&mut ui, 10, 4).contains("loading"));
        ui.set_image("thumb", flat(2, 2, [250, 0, 0, 255]));
        let s = snapshot(&mut ui, 10, 4);
        assert!(!s.contains("loading"), "{s}");
        let mut c = Canvas::new(10, 4, crate::canvas::Caps::FULL);
        ui.render(&mut c);
        assert_eq!(c.images().len(), 1);
        assert_eq!(c.get(5, 1).map(|x| x.ch), Some('▀'));
        ui.remove_image("thumb");
        assert!(snapshot(&mut ui, 10, 4).contains("loading"));
    }
}
