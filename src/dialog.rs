//! `dialog`: a modal box with a title, a message and buttons. It is open while its `bind=KEY` model number is not 0; the
//! app opens it with `ui.set("KEY", 1.0)`. While it is open it takes every key and every click (the `Ui` routes them
//! here), Tab and the arrows move between the buttons, enter presses one, a button's first letter presses it, esc
//! cancels. Pressing emits `activate` (text and index of the button) or `cancel`, and sets KEY back to 0.
//! `title=`, `text=` (or `from=KEY2`, a model text; `\n` breaks lines), `buttons="Yes,No"`, `default=` index of the
//! button that starts selected. `field=KEY` adds a one-line text field above the buttons, filled from the model text
//! KEY when the box opens; typing goes to the field, enter presses the selected button, tab moves between field and
//! buttons' choice, and `activate` then carries the field's text instead of the button's label (the index tells which).

use crate::canvas::{str_width, Canvas, Rect, Style};
use crate::input::{Event, Key, MouseKind};
use crate::native::{Cx, Def, EvCx, Widget};
use crate::ui::Value;
use crate::widgets::{Input, Res};

pub static DEFS: &[Def] = &[Def {
    name: "dialog",
    doc: "modal box, open while bind=KEY is not 0: title= text= (or from=KEY) buttons=\"Yes,No\" default=N; keys and clicks belong to it; emits activate or cancel and sets KEY to 0",
    focus: true,
    rows: Some(0),
    live: 0.0,
    make: || Box::new(Dialog::default()),
}];

#[derive(Default)]
pub struct Dialog {
    buttons: Vec<String>,
    sel: usize,
    was_open: bool,
    /// where each button was drawn, for clicks
    boxes: Vec<Rect>,
    field: Option<Input>,
}

impl Dialog {
    fn press(&mut self, i: usize, cx: &mut EvCx) -> Res {
        let text = match &self.field {
            Some(f) => f.text.clone(),
            None => self.buttons.get(i).cloned().unwrap_or_default(),
        };
        self.was_open = false;
        cx.set_bound(Value::Num(0.0));
        cx.emit("activate", &text, i as i64, i as f64);
        Res::Changed
    }
}

pub(crate) fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.split("\\n").flat_map(|l| l.split('\n')) {
        let mut cur = String::new();
        for word in line.split_whitespace() {
            if !cur.is_empty() && str_width(&cur) + 1 + str_width(word) > width {
                out.push(std::mem::take(&mut cur));
            }
            if !cur.is_empty() {
                cur.push(' ');
            }
            // a word wider than the line (a path) is cut at the width instead of being hidden by the clip
            for ch in word.chars() {
                if str_width(&cur) + crate::canvas::str_width(ch.encode_utf8(&mut [0; 4])) > width.max(1) {
                    out.push(std::mem::take(&mut cur));
                }
                cur.push(ch);
            }
        }
        out.push(cur);
    }
    out
}

impl Widget for Dialog {
    fn draw(&mut self, _c: &mut Canvas, _r: Rect, cx: &Cx) {
        self.buttons = cx.list_attr("buttons");
        if self.buttons.is_empty() {
            self.buttons.push("OK".into());
        }
        let open = cx.value() != 0.0;
        if open && !self.was_open {
            self.sel = (cx.num_attr("default", 0.0).max(0.0) as usize).min(self.buttons.len() - 1);
            self.field = cx.attr("field").map(|k| Input::new(&cx.model_text(k).unwrap_or_default()));
        }
        self.was_open = open;
    }

    fn overlay(&mut self, c: &mut Canvas, _r: Rect, cx: &Cx) {
        self.boxes.clear();
        if cx.value() == 0.0 || c.w < 12 || c.h < 5 {
            return;
        }
        let t = cx.theme;
        let message = match cx.attr("from").and_then(|k| cx.model_text(k)) {
            Some(m) => m,
            None => cx.text_attr("text", ""),
        };
        let buttons_w: usize = self.buttons.iter().map(|b| str_width(b) + 4).sum::<usize>() + self.buttons.len().saturating_sub(1);
        let title = cx.text_attr("title", "");
        let inner_w = (buttons_w.max(str_width(&title) + 2).max(30).min((c.w - 4).max(8) as usize)).max(8);
        let lines = wrap(&message, inner_w);
        let extra = i32::from(self.field.is_some());
        let h = (lines.len() as i32 + 4 + extra).min(c.h);
        let w = inner_w as i32 + 2;
        let rect = Rect::new((c.w - w) / 2, (c.h - h) / 2, w, h);
        let base = Style::fg(t.fg).on(t.bg);
        c.fill(rect, ' ', base);
        c.border(rect, &title, t.accent_style(), t.accent_style());
        for (i, line) in lines.iter().take((h - 4 - extra).max(0) as usize).enumerate() {
            c.text_fit(rect.x + 1, rect.y + 1 + i as i32, inner_w as i32, line, base);
        }
        if let Some(f) = self.field.as_mut() {
            f.draw(c, Rect::new(rect.x + 1, rect.y + rect.h - 3, inner_w as i32, 1), "> ", &t, true);
        }
        let mut x = rect.x + 1 + ((inner_w - buttons_w.min(inner_w)) / 2) as i32;
        let y = rect.y + rect.h - 2;
        for (i, b) in self.buttons.iter().enumerate() {
            let bw = str_width(b) as i32 + 4;
            let st = if i == self.sel { t.selected(true) } else { Style::fg(t.fg).on(crate::gfx::mix(t.bg, t.dim, 0.25)) };
            let r = Rect::new(x, y, bw, 1);
            c.fill(r, ' ', st);
            c.text_fit(x + 2, y, bw - 2, b, st);
            self.boxes.push(r);
            x += bw + 1;
        }
    }

    fn extent(&self, r: Rect) -> Rect {
        r
    }

    fn act(&mut self, action: &str, arg: &str, cx: &mut EvCx) -> Res {
        match (action, arg.parse::<usize>()) {
            ("select" | "activate", Ok(i)) if i < self.buttons.len() => self.press(i, cx),
            ("cancel", _) => {
                cx.set_bound(Value::Num(0.0));
                cx.emit("cancel", "", -1, -1.0);
                Res::Changed
            }
            _ => Res::Ignored,
        }
    }

    fn event(&mut self, _r: Rect, ev: &Event, cx: &mut EvCx) -> Res {
        if cx.attr("bind").and_then(|k| cx.model_num(k)).unwrap_or(0.0) == 0.0 {
            return Res::Ignored;
        }
        let n = self.buttons.len().max(1);
        match ev {
            Event::Key(Key::BackTab, _) => {
                self.sel = (self.sel + n - 1) % n;
                Res::Handled
            }
            Event::Key(Key::Tab, _) => {
                self.sel = (self.sel + 1) % n;
                Res::Handled
            }
            Event::Key(Key::Left, _) if self.field.is_none() => {
                self.sel = (self.sel + n - 1) % n;
                Res::Handled
            }
            Event::Key(Key::Right, _) if self.field.is_none() => {
                self.sel = (self.sel + 1) % n;
                Res::Handled
            }
            Event::Key(Key::Enter, _) => {
                let i = self.sel;
                self.press(i, cx)
            }
            Event::Key(Key::Esc, _) => self.act("cancel", "", cx),
            _ if self.field.is_some() && !matches!(ev, Event::Mouse(..)) => {
                let r = Rect::new(0, 0, 1000, 1);
                match self.field.as_mut().map(|f| f.event(r, 0, ev)) {
                    Some(Res::Ignored) | None => Res::Handled,
                    Some(res) => res,
                }
            }
            Event::Key(Key::Char(ch), m) if !m.ctrl && !m.alt => {
                match self.buttons.iter().position(|b| b.chars().next().is_some_and(|f| f.to_lowercase().eq(ch.to_lowercase()))) {
                    Some(i) => self.press(i, cx),
                    None => Res::Handled,
                }
            }
            Event::Mouse(MouseKind::Down(0), x, y, _) => match self.boxes.iter().position(|b| b.contains(*x, *y)) {
                Some(i) => self.press(i, cx),
                None => Res::Handled,
            },
            _ => Res::Handled,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::headless::{run_script, snapshot};
    use crate::ui::Ui;

    #[test]
    fn a_field_takes_typing_and_activate_carries_its_text() {
        let mut ui = Ui::from_spec("col\n  text id=t text=back size=1\n  dialog id=d bind=ask title=Rename field=name buttons=\"OK,Cancel\"\n").unwrap();
        run_script(&mut ui, "size 40 10\nset name=old.txt\nset ask=1").unwrap();
        let s = snapshot(&mut ui, 40, 10);
        assert!(s.contains("> old.txt"), "{s}");
        run_script(&mut ui, "key backspace backspace backspace\ntype rs\nkey left\ntype X").unwrap();
        assert!(snapshot(&mut ui, 40, 10).contains("> old.rXs"), "{}", snapshot(&mut ui, 40, 10));
        run_script(&mut ui, "key enter").unwrap();
        run_script(&mut ui, "expect event d activate old.rXs").unwrap();
        assert_eq!(ui.number("ask"), 0.0);
        run_script(&mut ui, "set ask=1\nkey esc\nexpect event d cancel").unwrap();
    }

    #[test]
    fn a_word_wider_than_the_line_is_cut_at_the_width() {
        assert_eq!(super::wrap("path: /aaaa/bbbb/cccc", 8), vec!["path:", "/aaaa/bb", "bb/cccc"]);
        assert_eq!(super::wrap("short words fit", 20), vec!["short words fit"]);
        assert_eq!(super::wrap("a\\nb", 5), vec!["a", "b"]);
    }

    const SPEC: &str = "col\n  text text=background size=1\n  searchbar id=q\n  dialog id=d bind=ask title=\"Delete\" text=\"Really delete 3 files?\" buttons=\"Yes,No\" default=1\n";

    #[test]
    fn closed_it_is_invisible_and_skipped_by_tab() {
        let mut ui = Ui::from_spec(SPEC).unwrap();
        run_script(&mut ui, "size 50 10\nfocus q\nkey tab\nexpect focus q").unwrap();
        assert!(!snapshot(&mut ui, 50, 10).contains("Really"));
    }

    #[test]
    fn open_it_owns_the_keys_and_enter_presses_the_selected_button() {
        let mut ui = Ui::from_spec(SPEC).unwrap();
        run_script(&mut ui, "size 50 10\nfocus q").unwrap();
        ui.set("ask", 1.0);
        let s = snapshot(&mut ui, 50, 10);
        assert!(s.contains("Really delete 3 files?") && s.contains("Delete") && s.contains("Yes") && s.contains("No"), "{s}");
        // typing goes to the dialog, not the search box; the default button is No
        run_script(&mut ui, "type zzz\nkey enter\nexpect event d activate No\nexpect model ask 0").unwrap();
        assert!(!snapshot(&mut ui, 50, 10).contains("Really"), "closed");
        run_script(&mut ui, "type a\nexpect model ask 0").unwrap();
    }

    #[test]
    fn letters_arrows_escape_and_clicks() {
        let mut ui = Ui::from_spec(SPEC).unwrap();
        run_script(&mut ui, "size 50 10").unwrap();
        ui.set("ask", 1.0);
        run_script(&mut ui, "key y\nexpect event d activate Yes").unwrap();
        ui.set("ask", 1.0);
        run_script(&mut ui, "key left\nkey enter\nexpect event d activate Yes").unwrap();
        ui.set("ask", 1.0);
        run_script(&mut ui, "key esc\nexpect event d cancel\nexpect model ask 0").unwrap();
        ui.set("ask", 1.0);
        let s = snapshot(&mut ui, 50, 10);
        let (row, col) = s.lines().enumerate().find_map(|(r, l)| l.find("Yes").map(|c| (r, c))).unwrap();
        run_script(&mut ui, &format!("size 50 10\nclick 0 0\nexpect model ask 1\nclick {} {row}\nexpect event d activate Yes", col + 1)).unwrap();
    }
}
