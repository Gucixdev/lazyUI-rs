//! `floatwin`: a non-modal window floating over the UI, for a properties or preview box that stays beside the work. It
//! is open while its `bind=KEY` model number is not 0 (`ui.set("KEY", 1.0)`), shows `image=KEY` (a picture registered
//! with `Ui::set_image`, the upper `ih=` rows) and `from=KEY2` (a model text, wrapped), and has a title bar:
//! drag it to move the window, `x` at its right end (or esc while it has the focus) closes it and emits `close`.
//! Everything behind it keeps working; a click on the window gives it the focus, up/down/pgup/pgdn/wheel scroll the
//! text. `title=`, `x=` `y=` (first position, centred without them), `w=` `h=` (cells, 40 x 12 by default).

use crate::canvas::{Canvas, Rect, Style};
use crate::input::{Event, Key, MouseKind};
use crate::native::{Cx, Def, EvCx, Widget};
use crate::ui::Value;
use crate::widgets::Res;

pub static DEFS: &[Def] = &[Def {
    name: "floatwin",
    doc: "non-modal floating window, open while bind=KEY is not 0: title= from=KEY2 (text) image=KEY (picture) ih=N x= y= w= h=; drag the title to move, x or esc closes (emits close)",
    focus: true,
    rows: Some(0),
    live: 0.0,
    make: || Box::new(FloatWin::default()),
}];

#[derive(Default)]
pub struct FloatWin {
    /// top-left once placed (kept between openings), the grab offset while the title is dragged
    pos: Option<(i32, i32)>,
    grab: Option<(i32, i32)>,
    scroll: usize,
    lines: usize,
    view: i32,
    was_open: bool,
    rect: Rect,
    screen: (i32, i32),
}

impl FloatWin {
    fn close(&mut self, cx: &mut EvCx) -> Res {
        self.was_open = false;
        self.grab = None;
        self.rect = Rect::new(0, 0, 0, 0);
        cx.set_bound(Value::Num(0.0));
        cx.emit("close", "", -1, -1.0);
        Res::Changed
    }

    fn close_mark(&self) -> Rect {
        Rect::new(self.rect.x + self.rect.w - 4, self.rect.y, 3, 1)
    }

    fn scroll_by(&mut self, d: i32) {
        let max = self.lines.saturating_sub(self.view.max(0) as usize) as i32;
        self.scroll = (self.scroll as i32 + d).clamp(0, max.max(0)) as usize;
    }
}

impl Widget for FloatWin {
    fn draw(&mut self, _c: &mut Canvas, _r: Rect, cx: &Cx) {
        let open = cx.value() != 0.0;
        if open && !self.was_open {
            self.scroll = 0;
        }
        if !open {
            self.rect = Rect::new(0, 0, 0, 0);
            self.grab = None;
        }
        self.was_open = open;
    }

    fn overlay(&mut self, c: &mut Canvas, _r: Rect, cx: &Cx) {
        if cx.value() == 0.0 || c.w < 10 || c.h < 4 {
            self.rect = Rect::new(0, 0, 0, 0);
            return;
        }
        self.screen = (c.w, c.h);
        let w = (cx.num_attr("w", 40.0) as i32).clamp(10, c.w);
        let h = (cx.num_attr("h", 12.0) as i32).clamp(4, c.h);
        let (x, y) = self.pos.unwrap_or_else(|| (cx.num_attr("x", ((c.w - w) / 2) as f64) as i32, cx.num_attr("y", ((c.h - h) / 2) as f64) as i32));
        let (x, y) = (x.clamp(0, c.w - w), y.clamp(0, c.h - h));
        self.pos = Some((x, y));
        self.rect = Rect::new(x, y, w, h);
        let t = cx.theme;
        let base = Style::fg(t.fg).on(t.bg);
        c.fill(self.rect, ' ', base);
        c.border(self.rect, &cx.text_attr("title", ""), t.accent_style(), t.accent_style());
        c.text_fit(x + w - 4, y, 3, if cx.unicode { "[×]" } else { "[x]" }, t.accent_style());
        let inner = Rect::new(x + 1, y + 1, w - 2, h - 2);
        let mut top = inner.y;
        if let Some(img) = cx.attr("image").and_then(|k| cx.image(k)) {
            let ih = (cx.num_attr("ih", (inner.h / 2) as f64) as i32).clamp(1, inner.h);
            c.image(Rect::new(inner.x, inner.y, inner.w, ih), img);
            top += ih;
        }
        let text = cx.attr("from").and_then(|k| cx.model_text(k)).unwrap_or_default();
        let lines = crate::dialog::wrap(&text, inner.w.max(1) as usize);
        self.lines = lines.len();
        self.view = inner.y + inner.h - top;
        self.scroll_by(0);
        for (i, line) in lines.iter().skip(self.scroll).take(self.view.max(0) as usize).enumerate() {
            c.text_fit(inner.x, top + i as i32, inner.w, line, base);
        }
    }

    fn extent(&self, r: Rect) -> Rect {
        if self.rect.is_empty() {
            r
        } else {
            self.rect
        }
    }

    fn act(&mut self, action: &str, _arg: &str, cx: &mut EvCx) -> Res {
        match action {
            "close" | "cancel" => self.close(cx),
            _ => Res::Ignored,
        }
    }

    fn event(&mut self, _r: Rect, ev: &Event, cx: &mut EvCx) -> Res {
        if self.rect.is_empty() {
            return Res::Ignored;
        }
        match ev {
            Event::Key(Key::Esc, _) => self.close(cx),
            Event::Key(Key::Up, _) => {
                self.scroll_by(-1);
                Res::Handled
            }
            Event::Key(Key::Down, _) => {
                self.scroll_by(1);
                Res::Handled
            }
            Event::Key(Key::PageUp, _) => {
                self.scroll_by(-self.view.max(1));
                Res::Handled
            }
            Event::Key(Key::PageDown, _) => {
                self.scroll_by(self.view.max(1));
                Res::Handled
            }
            Event::Mouse(MouseKind::WheelUp, x, y, _) if self.rect.contains(*x, *y) => {
                self.scroll_by(-3);
                Res::Handled
            }
            Event::Mouse(MouseKind::WheelDown, x, y, _) if self.rect.contains(*x, *y) => {
                self.scroll_by(3);
                Res::Handled
            }
            Event::Mouse(MouseKind::Down(0), x, y, _) if self.rect.contains(*x, *y) => {
                if self.close_mark().contains(*x, *y) {
                    return self.close(cx);
                }
                if *y == self.rect.y {
                    self.grab = Some((*x - self.rect.x, 0));
                }
                Res::Handled
            }
            Event::Mouse(MouseKind::Drag(0), x, y, _) => match self.grab {
                Some((dx, _)) => {
                    let (sw, sh) = self.screen;
                    self.pos = Some(((*x - dx).clamp(0, (sw - self.rect.w).max(0)), (*y).clamp(0, (sh - self.rect.h).max(0))));
                    Res::Changed
                }
                None => Res::Ignored,
            },
            Event::Mouse(MouseKind::Up(0), _, _, _) => {
                self.grab = None;
                Res::Handled
            }
            Event::Key(Key::Tab | Key::BackTab, _) => Res::Ignored,
            _ => Res::Ignored,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::headless::{run_script, snapshot};
    use crate::ui::Ui;

    #[test]
    fn an_overlay_takes_no_room_in_the_column() {
        use crate::headless::{run_script, snapshot};
        let mut ui = Ui::from_spec("col\n  list id=l items=\"one\" \n  text id=s text=status size=1\n  floatwin id=f bind=show title=P w=20 h=6\n  dialog id=d bind=ask title=D text=x buttons=\"Ok\"\n").unwrap();
        run_script(&mut ui, "size 30 10").unwrap();
        let s = snapshot(&mut ui, 30, 10);
        assert_eq!(s.lines().nth(9).map(str::trim_end), Some("status"), "the status line is on the last row:\n{s}");
    }

    const SPEC: &str = "col\n  list id=l items=\"one,two,three\" size=fill\n  floatwin id=f bind=show title=Props from=info w=20 h=6 x=2 y=1\n";

    fn rows(s: &str) -> Vec<&str> {
        s.lines().collect()
    }

    fn open(ui: &mut Ui) {
        ui.set("show", 1.0);
        ui.set_text("info", "line one\nline two\nline three\nline four\nline five\nline six\nline seven");
    }

    #[test]
    fn closed_it_draws_nothing_open_it_floats_over_the_list_without_taking_it_over() {
        let mut ui = Ui::from_spec(SPEC).unwrap();
        run_script(&mut ui, "size 40 10").unwrap();
        assert!(!snapshot(&mut ui, 40, 10).contains("Props"));
        open(&mut ui);
        let s = snapshot(&mut ui, 40, 10);
        assert!(rows(&s)[1][2..].starts_with('┌') || rows(&s)[1].contains("Props"), "{s}");
        assert!(s.contains("line one") && !s.contains("line six"), "only what fits:\n{s}");
        // the list behind still takes keys while the list has the focus
        run_script(&mut ui, "focus l\nkey down\nkey enter\nexpect event l activate two").unwrap();
    }

    #[test]
    fn click_focuses_it_and_the_arrows_scroll_its_text() {
        let mut ui = Ui::from_spec(SPEC).unwrap();
        run_script(&mut ui, "size 40 10").unwrap();
        open(&mut ui);
        run_script(&mut ui, "click 5 3\nkey down\nkey down\nkey down").unwrap();
        let s = snapshot(&mut ui, 40, 10);
        assert!(!s.contains("line one") && s.contains("line four"), "{s}");
        run_script(&mut ui, "key pgdn").unwrap();
        assert!(snapshot(&mut ui, 40, 10).contains("line seven"));
    }

    #[test]
    fn dragging_the_title_moves_it_and_it_stays_inside_the_screen() {
        let mut ui = Ui::from_spec(SPEC).unwrap();
        run_script(&mut ui, "size 40 10").unwrap();
        open(&mut ui);
        run_script(&mut ui, "drag 8 1 20 4").unwrap();
        let s = snapshot(&mut ui, 40, 10);
        assert!(rows(&s)[4].contains("Props") && rows(&s)[4].find("Props").unwrap() > 10, "moved:\n{s}");
        run_script(&mut ui, "drag 20 4 90 40").unwrap();
        let s = snapshot(&mut ui, 40, 10);
        assert!(rows(&s)[4..].iter().any(|l| l.contains("Props")), "clamped on screen:\n{s}");
    }

    #[test]
    fn the_x_and_esc_close_it_and_the_model_flag_goes_back_to_zero() {
        let mut ui = Ui::from_spec(SPEC).unwrap();
        run_script(&mut ui, "size 40 10").unwrap();
        open(&mut ui);
        run_script(&mut ui, "click 19 1\nexpect event f close").unwrap();
        assert_eq!(ui.number("show"), 0.0);
        assert!(!snapshot(&mut ui, 40, 10).contains("Props"));
        open(&mut ui);
        run_script(&mut ui, "click 5 3\nkey esc\nexpect event f close").unwrap();
        assert_eq!(ui.number("show"), 0.0);
    }

    #[test]
    fn a_closed_window_is_not_in_the_tab_ring() {
        let mut ui = Ui::from_spec(SPEC).unwrap();
        run_script(&mut ui, "size 40 10\nfocus l\nkey tab").unwrap();
        assert_eq!(ui.focused(), Some("l"));
    }
}
