//! A `Ui` drawn into a layer-shell panel (`crate::wl::panel`): cells to pixels with the same rasteriser as the window,
//! pixels to 0xAARRGGBB for the shared-memory buffer.
use super::raster::{Pixels, Raster};
use crate::canvas::{Caps, Canvas, Color};
use crate::widgets::theme::Theme;
use crate::ui::Ui;
use crate::wl::panel::Panel;
use crate::wl::wire::Conn;

/// RGBA bytes (the rasteriser's layout) to the compositor's ARGB8888 words.
pub fn argb(rgba: &[u8]) -> Vec<u32> {
    rgba.chunks_exact(4).map(|p| u32::from_be_bytes([p[3], p[0], p[1], p[2]])).collect()
}

/// One frame of `ui` at `w` x `h` pixels: as many whole cells as fit, the rest filled with the background.
pub fn frame(ui: &mut Ui, r: &mut Raster, w: usize, h: usize) -> Vec<u32> {
    let mut px = Pixels::new(w, h, r.default_bg);
    let mut c = Canvas::new((w / r.cw) as i32, (h / r.ch) as i32, Caps::FULL);
    ui.render(&mut c);
    r.paint(&mut px, None, &c);
    argb(&px.rgba)
}

/// Put a theme on the `Ui` and make the area the cells do not cover (the pixel remainder) match its background.
pub fn apply_theme(ui: &mut Ui, r: &mut Raster, t: Theme) {
    if let Color::Rgb(a, b, c) = t.bg {
        r.default_bg = [a, b, c];
    }
    if let Color::Rgb(a, b, c) = t.fg {
        r.default_fg = [a, b, c];
    }
    ui.theme = t;
}

/// What a finedesktop event line asks of a panel: the theme name, for a `theme` event only.
pub fn theme_of(e: &crate::fdctl::Event) -> Option<&str> {
    (e.kind == "theme" && !e.rest.is_empty()).then_some(e.rest.as_str())
}

/// Draw `ui` once into an open panel.
pub fn show(p: &mut Panel, conn: &mut Conn, ui: &mut Ui, r: &mut Raster) -> std::io::Result<()> {
    let f = frame(ui, r, p.width as usize, p.height as usize);
    p.draw(conn, &f)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argb_orders_the_channels_for_the_compositor() {
        assert_eq!(argb(&[0x11, 0x22, 0x33, 0xff, 0, 0, 0, 0x80]), vec![0xff112233, 0x80000000]);
    }

    #[test]
    fn a_ui_frame_has_the_size_asked_and_text_pixels() {
        let Ok(mut r) = Raster::system(14.0, None) else { return };
        let mut ui = Ui::new(crate::prelude::text("hello"));
        let (w, h) = (r.cw * 10 + 3, r.ch * 2);
        let f = frame(&mut ui, &mut r, w, h);
        assert_eq!(f.len(), w * h);
        let bg = f[w * h - 1];
        assert!(f.iter().any(|&p| p != bg), "some pixel differs from the background: the text was drawn");
        assert!(f.iter().all(|&p| p >> 24 == 0xff), "opaque");
    }

    #[test]
    fn only_a_theme_event_with_a_name_changes_the_theme() {
        use crate::fdctl::Event;
        assert_eq!(theme_of(&Event::parse("theme dracula")), Some("dracula"));
        assert_eq!(theme_of(&Event::parse("focus 3")), None);
        assert_eq!(theme_of(&Event::parse("theme")), None);
    }

    #[test]
    fn apply_theme_sets_the_background_the_remainder_uses() {
        let Ok(mut r) = Raster::system(14.0, None) else { return };
        let mut ui = Ui::new(crate::prelude::text("x"));
        let mut t = Theme::from_finedesktop("nonexistent-theme").unwrap_or_else(|_| ui.theme.clone());
        t.bg = Color::Rgb(1, 2, 3);
        apply_theme(&mut ui, &mut r, t);
        assert_eq!(r.default_bg, [1, 2, 3]);
        let (w, h) = (r.cw * 4 + 5, r.ch);
        let f = frame(&mut ui, &mut r, w, h);
        assert_eq!(f[r.cw * 4 + 4], 0xff010203, "the remainder column carries the theme background");
    }
}
