//! `menu`: a popup list of commands. Items come from `items="Open,Rename,-,~Delete"`: `-` is a separator, a leading `~`
//! greys the item out (it cannot be picked). Closed it shows its `label`; enter, space or a left click opens the popup
//! under it, a right click opens it at the pointer. Picking emits `activate` with the item's text and index.

use crate::canvas::{str_width, Canvas, Rect, Style};
use crate::input::{Event, Key, MouseKind};
use crate::native::{Cx, Def, EvCx, Widget};
use crate::widgets::Res;

pub static DEFS: &[Def] = &[Def {
    name: "menu",
    doc: "popup command list items=\"Open,Rename,-,~Delete\" (- separator, ~ greyed): enter or click opens under the label, right click opens at the pointer; activate carries the item",
    focus: true,
    rows: Some(1),
    live: 0.0,
    make: || Box::new(Menu::default()),
}];

#[derive(Default)]
pub struct Menu {
    items: Vec<String>,
    open: bool,
    hi: usize,
    /// where the popup wants its top-left corner; None = under the label
    at: Option<(i32, i32)>,
    popup: Rect,
}

fn separator(s: &str) -> bool {
    s == "-"
}
fn disabled(s: &str) -> bool {
    s.starts_with('~')
}
fn shown(s: &str) -> &str {
    s.strip_prefix('~').unwrap_or(s)
}

impl Menu {
    fn pickable(&self, i: usize) -> bool {
        self.items.get(i).is_some_and(|s| !separator(s) && !disabled(s))
    }

    /// The pickable item nearest to `from` in direction `dir` (+1/-1), staying put at the ends.
    fn step(&self, from: usize, dir: i32) -> usize {
        let mut i = from as i32 + dir;
        while i >= 0 && (i as usize) < self.items.len() {
            if self.pickable(i as usize) {
                return i as usize;
            }
            i += dir;
        }
        from
    }

    fn first(&self) -> usize {
        (0..self.items.len()).find(|&i| self.pickable(i)).unwrap_or(0)
    }

    fn close(&mut self) {
        self.open = false;
        self.at = None;
        self.popup = Rect::new(0, 0, 0, 0);
    }

    fn pick(&mut self, i: usize, cx: &mut EvCx) -> Res {
        if !self.pickable(i) {
            return Res::Handled;
        }
        let text = self.items[i].clone();
        self.close();
        cx.emit("activate", &text, i as i64, i as f64);
        Res::Changed
    }

    fn row_at(&self, x: i32, y: i32) -> Option<usize> {
        if self.popup.contains(x, y) {
            Some((y - self.popup.y) as usize)
        } else {
            None
        }
    }
}

impl Widget for Menu {
    fn draw(&mut self, c: &mut Canvas, r: Rect, cx: &Cx) {
        self.items = cx.items();
        if !cx.focused {
            self.close();
        }
        if r.w < 1 || r.h < 1 {
            return;
        }
        let t = cx.theme;
        let st = if cx.focused { cx.theme.selected(true) } else { Style::fg(t.fg).on(crate::gfx::mix(t.bg, t.dim, 0.25)) };
        c.fill(Rect::new(r.x, r.y, r.w, 1), ' ', st);
        let label = cx.text_attr("label", "menu");
        c.text_fit(r.x + 1, r.y, (r.w - 3).max(0), &label, st);
        if r.w >= 2 {
            c.text_fit(r.x + r.w - 2, r.y, 1, if cx.unicode { "≡" } else { "=" }, st);
        }
    }

    fn overlay(&mut self, c: &mut Canvas, r: Rect, cx: &Cx) {
        if !self.open || self.items.is_empty() {
            self.popup = Rect::new(0, 0, 0, 0);
            return;
        }
        let t = cx.theme;
        let w = (self.items.iter().map(|s| str_width(shown(s)) as i32).max().unwrap_or(0) + 4).min(c.w).max(4);
        let h = (self.items.len() as i32).min(c.h);
        let (ax, ay) = self.at.unwrap_or((r.x, r.y + 1));
        let x = ax.min(c.w - w).max(0);
        let y = if ay + h <= c.h { ay } else { (ay - h - if self.at.is_some() { 0 } else { 1 }).max(0) };
        self.popup = Rect::new(x, y, w, h);
        let base = Style::fg(t.fg).on(crate::gfx::mix(t.bg, t.dim, 0.25));
        c.fill(self.popup, ' ', base);
        for (row, item) in self.items.iter().take(h as usize).enumerate() {
            let yy = y + row as i32;
            if separator(item) {
                c.fill(Rect::new(x, yy, w, 1), if cx.unicode { '─' } else { '-' }, Style::fg(t.dim).on(base.bg));
                continue;
            }
            let st = if disabled(item) {
                Style::fg(t.dim).on(base.bg)
            } else if row == self.hi {
                cx.theme.selected(true)
            } else {
                base
            };
            c.fill(Rect::new(x, yy, w, 1), ' ', st);
            c.text_fit(x + 1, yy, w - 2, shown(item), st);
        }
    }

    fn extent(&self, r: Rect) -> Rect {
        if self.popup.is_empty() {
            return r;
        }
        let (x0, y0) = (r.x.min(self.popup.x), r.y.min(self.popup.y));
        let (x1, y1) = ((r.x + r.w).max(self.popup.x + self.popup.w), (r.y + r.h).max(self.popup.y + self.popup.h));
        Rect::new(x0, y0, x1 - x0, y1 - y0)
    }

    fn act(&mut self, action: &str, arg: &str, cx: &mut EvCx) -> Res {
        match (action, arg.parse::<usize>()) {
            ("select" | "activate", Ok(i)) if i < self.items.len() => self.pick(i, cx),
            _ => Res::Ignored,
        }
    }

    fn event(&mut self, r: Rect, ev: &Event, cx: &mut EvCx) -> Res {
        if self.items.is_empty() {
            return Res::Ignored;
        }
        if self.open {
            return match ev {
                Event::Key(Key::Up, _) => {
                    self.hi = self.step(self.hi, -1);
                    Res::Handled
                }
                Event::Key(Key::Down, _) => {
                    self.hi = self.step(self.hi, 1);
                    Res::Handled
                }
                Event::Key(Key::Home, _) => {
                    self.hi = self.first();
                    Res::Handled
                }
                Event::Key(Key::Enter | Key::Char(' '), _) => {
                    let i = self.hi;
                    self.pick(i, cx)
                }
                Event::Key(Key::Esc, _) => {
                    self.close();
                    Res::Handled
                }
                Event::Mouse(MouseKind::Down(0), x, y, _) => match self.row_at(*x, *y) {
                    Some(i) => self.pick(i, cx),
                    None => {
                        self.close();
                        Res::Handled
                    }
                },
                Event::Mouse(MouseKind::Move, x, y, _) => {
                    if let Some(i) = self.row_at(*x, *y).filter(|&i| self.pickable(i)) {
                        self.hi = i;
                    }
                    Res::Handled
                }
                Event::Mouse(MouseKind::Down(2), x, y, _) => {
                    self.at = Some((*x, *y));
                    self.hi = self.first();
                    Res::Handled
                }
                Event::Key(Key::Tab | Key::BackTab, _) => {
                    self.close();
                    Res::Ignored
                }
                _ => Res::Handled,
            };
        }
        match ev {
            Event::Key(Key::Enter | Key::Char(' ') | Key::Down, _) => {
                self.open = true;
                self.at = None;
                self.hi = self.first();
                Res::Handled
            }
            Event::Mouse(MouseKind::Down(0), x, y, _) if r.contains(*x, *y) => {
                self.open = true;
                self.at = None;
                self.hi = self.first();
                Res::Handled
            }
            Event::Mouse(MouseKind::Down(2), x, y, _) if r.contains(*x, *y) => {
                self.open = true;
                self.at = Some((*x, *y));
                self.hi = self.first();
                Res::Handled
            }
            _ => Res::Ignored,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::headless::{run_script, snapshot};
    use crate::ui::Ui;

    fn rows(s: &str) -> Vec<&str> {
        s.lines().collect()
    }

    const SPEC: &str = "col\n  menu id=m items=\"Open,Rename,-,~Delete,Quit\" label=file\n  text text=below1\n  text text=below2\n  text text=below3\n  text text=below4\n  text text=below5\n";

    #[test]
    fn opens_under_the_label_skips_separators_and_greyed_items() {
        let mut ui = Ui::from_spec(SPEC).unwrap();
        run_script(&mut ui, "size 24 8\nfocus m\nkey enter").unwrap();
        let s = snapshot(&mut ui, 24, 8);
        assert!(rows(&s)[1].contains("Open") && rows(&s)[2].contains("Rename") && rows(&s)[4].contains("Delete"), "{s}");
        // down from Rename skips the separator and the greyed Delete, landing on Quit
        run_script(&mut ui, "key down\nkey down\nkey enter\nexpect event m activate Quit").unwrap();
        let s = snapshot(&mut ui, 24, 8);
        assert_eq!(rows(&s)[1].trim_end(), "below1", "closed again:\n{s}");
    }

    #[test]
    fn a_click_on_a_greyed_row_or_separator_picks_nothing_and_outside_closes() {
        let mut ui = Ui::from_spec(SPEC).unwrap();
        run_script(&mut ui, "size 24 8\nfocus m\nkey enter\nclick 2 3\nclick 2 4").unwrap();
        assert!(rows(&snapshot(&mut ui, 24, 8))[1].contains("Open"), "still open");
        run_script(&mut ui, "click 20 7").unwrap();
        assert_eq!(rows(&snapshot(&mut ui, 24, 8))[1].trim_end(), "below1");
        run_script(&mut ui, "focus m\nkey enter\nclick 2 2\nexpect event m activate Rename").unwrap();
    }

    #[test]
    fn right_click_opens_at_the_pointer_and_the_popup_stays_on_screen() {
        let mut ui = Ui::from_spec(SPEC).unwrap();
        run_script(&mut ui, "size 24 8\nrclick 5 0").unwrap();
        let s = snapshot(&mut ui, 24, 8);
        assert!(rows(&s)[0][5..].starts_with(" Open") && rows(&s)[1][5..].starts_with(" Rename"), "popup corner at the pointer:\n{s}");
        run_script(&mut ui, "key esc\nrclick 23 0").unwrap();
        let s = snapshot(&mut ui, 24, 8);
        let at = rows(&s)[0].find("Open").unwrap();
        assert!(at < 23 && rows(&s)[1].contains("Rename"), "moved left to fit:\n{s}");
    }
}
