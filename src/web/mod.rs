//! The web backend: the same `Ui` in a browser tab, as WebAssembly. No server, no JS framework, no dependencies.
//!
//! The wasm module is only the engine: JS feeds it events (`key`, `text`, `mouse`, `tick`), asks for a frame, and gets
//! back a flat array of cells (4 `u32` each: char, fg, bg, mods) that `lazyui.js` paints on a `<canvas>` with the
//! browser's own fonts. `Web` is plain safe Rust, so it is tested natively; `ffi` (wasm only) is the thin C-ABI shell.
//!
//! Colour words: 0 = the page default, `0x01000000 | n` = palette index n, `0x02000000 | 0xRRGGBB` = true colour.

use crate::canvas::{Canvas, Caps, Color};
use crate::components::{Scope, Widgets};
use crate::input::{parse_key, Event, Mods, MouseKind};
use crate::theme::Theme;
use crate::ui::Ui;

#[cfg(target_arch = "wasm32")]
mod ffi;

/// u32 words per cell in the frame array.
pub const CELL_WORDS: usize = 4;

pub struct Web {
    ui: Ui,
    w: i32,
    h: i32,
    frame: Vec<u32>,
    events: String,
    pub error: String,
}

fn color_word(c: Color) -> u32 {
    match c {
        Color::Default => 0,
        Color::Idx(i) => 0x0100_0000 | i as u32,
        Color::Rgb(r, g, b) => 0x0200_0000 | (r as u32) << 16 | (g as u32) << 8 | b as u32,
    }
}

fn mods_of(bits: u32) -> Mods {
    Mods { ctrl: bits & 1 != 0, alt: bits & 2 != 0, shift: bits & 4 != 0 }
}

impl Web {
    pub fn new(spec: &str) -> Result<Web, String> {
        Web::with_widgets(spec, Widgets::builtin())
    }

    /// With extra components (the page has no folders: it hands them over by name).
    pub fn with_widgets(spec: &str, widgets: Widgets) -> Result<Web, String> {
        Ok(Web { ui: Ui::from_spec_with(spec, widgets)?, w: 80, h: 24, frame: Vec::new(), events: String::new(), error: String::new() })
    }

    /// Replace the spec, keeping the model values; a bad spec keeps the old UI and reports why.
    pub fn reload(&mut self, spec: &str) -> Result<(), String> {
        self.ui.reload_spec(spec)
    }

    pub fn resize(&mut self, w: i32, h: i32) {
        self.w = w.clamp(1, 1000);
        self.h = h.clamp(1, 1000);
        self.ui.dispatch(&Event::Resize(self.w, self.h));
    }

    pub fn size(&self) -> (i32, i32) {
        (self.w, self.h)
    }

    /// A key by lazyui's names (`enter`, `ctrl-r`, `f5`, `shift-tab`, `a`). False when the name is unknown.
    pub fn key(&mut self, name: &str) -> bool {
        match parse_key(name) {
            Some((k, m)) => {
                self.sync();
                self.ui.dispatch(&Event::Key(k, m))
            }
            None => false,
        }
    }

    /// Typed text, one character at a time.
    pub fn text(&mut self, s: &str) {
        self.sync();
        for c in s.chars() {
            self.ui.dispatch(&Event::ch(c));
        }
    }

    pub fn paste(&mut self, s: &str) {
        self.sync();
        self.ui.dispatch(&Event::Paste(s.to_string()));
    }

    /// kind: 0 down, 1 up, 2 drag, 3 move, 4 wheel up, 5 wheel down. mods: 1 ctrl, 2 alt, 4 shift.
    pub fn mouse(&mut self, kind: u32, button: u8, x: i32, y: i32, mods: u32) -> bool {
        let k = match kind {
            0 => MouseKind::Down(button),
            1 => MouseKind::Up(button),
            2 => MouseKind::Drag(button),
            3 => MouseKind::Move,
            4 => MouseKind::WheelUp,
            5 => MouseKind::WheelDown,
            _ => return false,
        };
        self.sync();
        self.ui.dispatch(&Event::Mouse(k, x, y, mods_of(mods)))
    }

    /// Advance the clock. Long gaps (a sleeping page, a hidden tab) are stepped in 50 ms pieces so springs stay
    /// stable and timers still fire in order; more than 5 s at once is cut off.
    pub fn tick(&mut self, dt_ms: f64) {
        let mut left = (dt_ms / 1000.0).clamp(0.0, 5.0);
        while left > 0.0 {
            let d = left.min(0.05);
            self.ui.tick(d);
            left -= d;
        }
    }

    pub fn set(&mut self, key: &str, value: &str) {
        self.ui.set_auto(key, value);
    }

    pub fn set_list(&mut self, id: &str, items: &str) {
        self.ui.set_list(id, items.split(',').map(|s| s.to_string()).collect());
    }

    pub fn hotkey(&mut self, key: &str, event: &str) -> bool {
        self.ui.hotkey(key, event)
    }

    pub fn every(&mut self, seconds: f64, event: &str) {
        self.ui.every(seconds, event);
    }

    pub fn theme(&mut self, name: &str) -> bool {
        match Theme::by_name(name) {
            Some(t) => {
                self.ui.theme = t;
                self.ui.dirty = true;
                true
            }
            None => false,
        }
    }

    /// Milliseconds until the page must call `tick` again: 16 while something moves, the next timer otherwise,
    /// -1 when nothing will happen without input (the page sleeps, idle costs nothing).
    pub fn wake_ms(&self) -> f64 {
        if self.ui.animating() {
            16.0
        } else if let Some(s) = self.ui.next_deadline() {
            (s * 1000.0).max(1.0)
        } else {
            -1.0
        }
    }

    /// Named events since the last call, one JSON object per line.
    pub fn take_events(&mut self) -> &str {
        self.events.clear();
        for e in self.ui.take_events() {
            self.events.push_str(&e.to_json());
            self.events.push('\n');
        }
        &self.events
    }

    /// Draw into the frame array. True when it differs from the previous frame (or the size changed).
    pub fn render(&mut self) -> bool {
        let mut c = Canvas::new(self.w, self.h, Caps::FULL);
        self.ui.render(&mut c);
        let mut next = Vec::with_capacity((self.w * self.h) as usize * CELL_WORDS);
        for y in 0..self.h {
            for x in 0..self.w {
                let cell = c.get(x, y).unwrap_or(crate::canvas::Cell { ch: ' ', style: Default::default() });
                next.extend_from_slice(&[cell.ch as u32, color_word(cell.style.fg), color_word(cell.style.bg), cell.style.mods as u32]);
            }
        }
        let changed = next != self.frame;
        self.frame = next;
        changed
    }

    pub fn frame(&self) -> &[u32] {
        &self.frame
    }

    /// The frame as plain text (wide-glyph tails skipped): for tests and for the no-canvas fallback.
    pub fn frame_text(&self) -> String {
        let mut s = String::new();
        for row in self.frame.chunks((self.w as usize) * CELL_WORDS) {
            let line: String = row.chunks(CELL_WORDS).filter(|c| c[0] != 0).map(|c| char::from_u32(c[0]).unwrap_or(' ')).collect();
            s.push_str(line.trim_end());
            s.push('\n');
        }
        s
    }

    /// Layout is computed on render; events need the current one.
    fn sync(&mut self) {
        if self.ui.size() != (self.w, self.h) || self.ui.dirty {
            let mut c = Canvas::new(self.w, self.h, Caps::FULL);
            self.ui.render(&mut c);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPEC: &str = "col\n  searchbar id=q prompt=\"> \" bind=query\n  list id=files items=\"alpha,beta,gamma\" filter=query\n  progressbar bind=load max=10 size=1\n";

    fn web() -> Web {
        let mut w = Web::new(SPEC).unwrap();
        w.resize(30, 6);
        w.render();
        w
    }

    #[test]
    fn frame_has_the_cells_and_text() {
        let w = web();
        assert_eq!(w.frame().len(), 30 * 6 * CELL_WORDS);
        assert!(w.frame_text().starts_with('>'), "{}", w.frame_text());
        assert!(w.frame_text().contains("alpha"));
    }

    #[test]
    fn typing_filters_and_emits_events() {
        let mut w = web();
        w.text("ta");
        assert!(w.render());
        let t = w.frame_text();
        assert!(t.contains("beta") && !t.contains("alpha"), "{t}");
        assert!(w.take_events().contains("\"kind\":\"change\",\"text\":\"ta\""));
        assert_eq!(w.take_events(), "", "events are drained");
    }

    #[test]
    fn keys_use_lazyui_names_and_unknown_ones_are_refused() {
        let mut w = web();
        assert!(w.key("tab"));
        assert!(w.key("down"));
        assert!(w.key("enter"));
        assert!(w.take_events().contains("\"id\":\"files\",\"kind\":\"activate\""));
        assert!(!w.key("hyper-x"));
    }

    #[test]
    fn mouse_click_activates_a_row() {
        let mut w = web();
        w.mouse(0, 0, 2, 2, 0);
        w.mouse(1, 0, 2, 2, 0);
        w.render();
        assert!(w.take_events().contains("files"), "a click on a list row is an event");
    }

    #[test]
    fn identical_frames_report_no_change_and_a_model_set_does() {
        let mut w = web();
        assert!(!w.render());
        w.set("load", "5");
        for _ in 0..120 {
            w.tick(16.0);
            w.render();
        }
        assert!(w.frame_text().contains("50%"), "{}", w.frame_text());
    }

    #[test]
    fn idle_sleeps_and_timers_wake() {
        let mut w = web();
        for _ in 0..200 {
            w.tick(16.0);
        }
        w.render();
        assert_eq!(w.wake_ms(), -1.0);
        w.every(2.0, "beat");
        assert!(w.wake_ms() > 100.0 && w.wake_ms() <= 2000.0);
        for _ in 0..130 {
            w.tick(16.0);
        }
        assert!(w.take_events().contains("\"id\":\"app\",\"kind\":\"beat\""));
    }

    #[test]
    fn resize_changes_the_frame_and_reload_keeps_the_model() {
        let mut w = web();
        w.text("be");
        w.resize(40, 8);
        w.render();
        assert_eq!(w.frame().len(), 40 * 8 * CELL_WORDS);
        assert!(w.reload("col\n  searchbar id=q prompt=\"? \" bind=query\n").is_ok());
        w.render();
        assert!(w.frame_text().contains("? be"), "{}", w.frame_text());
        assert!(w.reload("bogus").is_err());
        w.render();
        assert!(w.frame_text().contains("? be"), "a bad spec keeps the old UI");
    }

    #[test]
    fn a_long_sleep_still_fires_every_timer_in_order() {
        let mut w = web();
        w.every(1.0, "beat");
        w.tick(3200.0);
        assert_eq!(w.take_events().matches("\"kind\":\"beat\"").count(), 3);
    }

    #[test]
    fn the_page_can_hand_over_its_own_widgets() {
        let mut ws = Widgets::builtin();
        ws.add_source("hello", "params who=world\ntext text=\"hi $who\"\n", Scope::Global, "page").unwrap();
        let mut w = Web::with_widgets("col\n  hello who=you\n  meter label=a bind=x\n", ws).unwrap();
        w.resize(20, 3);
        w.render();
        assert!(w.frame_text().starts_with("hi you"), "{}", w.frame_text());
        assert!(w.reload("col\n  hello\n").is_ok(), "reload keeps the page's widgets");
    }

    #[test]
    fn colour_words_are_distinct_per_kind() {
        assert_eq!(color_word(Color::Default), 0);
        assert_eq!(color_word(Color::Idx(5)), 0x0100_0005);
        assert_eq!(color_word(Color::Rgb(1, 2, 3)), 0x0200_0000 | 0x010203);
    }
}
