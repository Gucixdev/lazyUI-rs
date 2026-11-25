//! `splitter`: the draggable divider between two panes. It is one cell wide (`axis=x`, between the children of a row) or
//! one cell tall (`axis=y`, in a column) and writes the size of the pane before it into `bind=KEY` in cells; that pane
//! says `size=@KEY`. Drag it with the mouse, or focus it and use the arrow keys (`step=` cells, default 2).
//! `min=` and `max=` limit the value (3 and 200 by default); `value=` is the start when the model has no number yet, and
//! the pane says the same start as `size=@KEY:N`.

use crate::canvas::{Canvas, Rect, Style};
use crate::input::{Event, Key, MouseKind};
use crate::native::{Cx, Def, EvCx, Widget};
use crate::ui::Value;
use crate::widgets::Res;

pub static DEFS: &[Def] = &[Def {
    name: "splitter",
    doc: "draggable divider: bind=KEY gets the cells of the pane before it (give that pane size=@KEY:START); axis=x|y, min= max= step= value=; drag or arrow keys",
    focus: true,
    rows: None,
    live: 0.0,
    make: || Box::new(Splitter::default()),
}];

#[derive(Default)]
pub struct Splitter {
    /// (pointer coordinate, value) when the press landed
    press: Option<(i32, f64)>,
    last: f64,
}

fn axis_y(el_axis: Option<&str>) -> bool {
    el_axis == Some("y")
}

impl Splitter {
    fn limits(cx: &EvCx) -> (f64, f64) {
        let get = |k: &str, d: f64| cx.attr(k).and_then(|s| s.trim().parse::<f64>().ok()).unwrap_or(d);
        let lo = get("min", 3.0);
        (lo, get("max", 200.0).max(lo))
    }

    fn set(&mut self, v: f64, cx: &mut EvCx) -> Res {
        let (lo, hi) = Self::limits(cx);
        let v = v.round().clamp(lo, hi);
        if v == self.last {
            return Res::Handled;
        }
        self.last = v;
        cx.set_bound(Value::Num(v));
        cx.emit("change", "", v as i64, v);
        Res::Changed
    }

    fn current(&self, cx: &EvCx) -> f64 {
        match cx.attr("bind").and_then(|k| cx.model_num(k)) {
            Some(v) => v,
            None => cx.attr("value").and_then(|s| s.trim().parse().ok()).unwrap_or(self.last),
        }
    }
}

impl Widget for Splitter {
    fn draw(&mut self, c: &mut Canvas, r: Rect, cx: &Cx) {
        if r.w < 1 || r.h < 1 {
            return;
        }
        self.last = cx.value();
        let t = cx.theme;
        let st = if cx.focused || self.press.is_some() { Style::fg(t.accent).on(t.bg) } else { Style::fg(t.dim).on(t.bg) };
        let (glyph_x, glyph_y) = if cx.unicode { ('│', '─') } else { ('|', '-') };
        if axis_y(cx.attr("axis")) {
            c.fill(Rect::new(r.x, r.y, r.w, 1), glyph_y, st);
        } else {
            c.fill(Rect::new(r.x, r.y, 1, r.h), glyph_x, st);
        }
    }

    fn event(&mut self, _r: Rect, ev: &Event, cx: &mut EvCx) -> Res {
        let y_axis = axis_y(cx.attr("axis"));
        let step: f64 = cx.attr("step").and_then(|s| s.trim().parse().ok()).unwrap_or(2.0);
        match ev {
            Event::Mouse(MouseKind::Down(0), x, y, _) => {
                self.press = Some((if y_axis { *y } else { *x }, self.current(cx)));
                Res::Handled
            }
            Event::Mouse(MouseKind::Drag(0), x, y, _) => match self.press {
                Some((from, start)) => {
                    let at = if y_axis { *y } else { *x };
                    self.set(start + (at - from) as f64, cx)
                }
                None => Res::Ignored,
            },
            Event::Mouse(MouseKind::Up(0), ..) => {
                self.press = None;
                Res::Handled
            }
            Event::Key(Key::Left, _) if !y_axis => self.set(self.current(cx) - step, cx),
            Event::Key(Key::Right, _) if !y_axis => self.set(self.current(cx) + step, cx),
            Event::Key(Key::Up, _) if y_axis => self.set(self.current(cx) - step, cx),
            Event::Key(Key::Down, _) if y_axis => self.set(self.current(cx) + step, cx),
            _ => Res::Ignored,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::headless::{run_script, snapshot};
    use crate::ui::Ui;

    const SPEC: &str = "row\n  text id=left text=LEFT size=@w:10\n  splitter id=s bind=w value=10 min=4 max=30\n  text id=right text=RIGHT\n";

    fn col_of(s: &str, word: &str) -> usize {
        s.lines().next().unwrap().find(word).unwrap_or_else(|| panic!("{word} not in\n{s}"))
    }

    #[test]
    fn the_divider_starts_at_value_and_a_drag_moves_it_within_the_limits() {
        let mut ui = Ui::from_spec(SPEC).unwrap();
        run_script(&mut ui, "size 40 3").unwrap();
        // nothing is in the model before the first move; the pane's own `@w:10` and the splitter's `value=10` agree
        assert_eq!(col_of(&snapshot(&mut ui, 40, 3), "RIGHT"), 11, "divider in column 10");
        run_script(&mut ui, "drag 10 1 16 1\nexpect model w 16").unwrap();
        assert_eq!(col_of(&snapshot(&mut ui, 40, 3), "RIGHT"), 17);
        // the pointer may leave the divider and the screen: the value stops at max, then at min
        run_script(&mut ui, "drag 16 1 39 1\nexpect model w 30").unwrap();
        run_script(&mut ui, "drag 30 1 0 1\nexpect model w 4").unwrap();
    }

    #[test]
    fn arrow_keys_move_it_by_step_when_focused() {
        let mut ui = Ui::from_spec(SPEC).unwrap();
        run_script(&mut ui, "size 40 3\nfocus s\nkey right\nexpect model w 12\nkey left\nkey left\nkey left\nkey left\nexpect model w 4").unwrap();
    }

    #[test]
    fn a_column_divider_moves_along_y() {
        let spec = "col\n  text text=TOP size=@h:3\n  splitter bind=h axis=y value=3 min=1 max=8\n  text text=BOTTOM\n";
        let mut ui = Ui::from_spec(spec).unwrap();
        run_script(&mut ui, "size 20 12\ndrag 5 3 5 6\nexpect model h 6").unwrap();
        let s = snapshot(&mut ui, 20, 12);
        assert!(s.lines().nth(7).unwrap().starts_with("BOTTOM"), "{s}");
    }
}
