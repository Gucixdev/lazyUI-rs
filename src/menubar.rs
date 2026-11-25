//! `menubar`: a row of menu titles with drop-down menus and one level of sub-menus. `menus="File=Open,Save,-,Recent>a|b,~Quit;Edit=Copy,Paste"`:
//! groups split on `;`, title and items on `=`, items on `,`; inside an item `>` starts a sub-menu whose entries split on `|`.
//! `-` is a separator, a leading `~` greys an entry out. Keys: left/right between menus (or into and out of a sub-menu),
//! up/down inside, enter picks or opens the sub-menu, esc closes one level. A pick emits `activate` with the path
//! (`File/Recent/a`), the menu number as index and the entry number (of the level picked from) as number.

use crate::canvas::{str_width, Canvas, Rect, Style};
use crate::input::{Event, Key, MouseKind};
use crate::native::{Cx, Def, EvCx, Widget};
use crate::widgets::Res;

pub static DEFS: &[Def] = &[Def {
    name: "menubar",
    doc: "menu bar menus=\"File=Open,-,Recent>a|b,~Quit;Edit=Copy\" (- separator, ~ greyed, > sub-menu with | entries): left/right switch menu, enter picks, activate carries File/Recent/a",
    focus: true,
    rows: Some(1),
    live: 0.0,
    make: || Box::new(MenuBar::default()),
}];

#[derive(Clone, Debug, PartialEq)]
struct Entry {
    text: String,
    sub: Vec<String>,
}

#[derive(Default)]
pub struct MenuBar {
    titles: Vec<String>,
    menus: Vec<Vec<Entry>>,
    raw: String,
    cur: usize,
    open: bool,
    hi: usize,
    sub_open: bool,
    sub_hi: usize,
    title_rects: Vec<Rect>,
    popup: Rect,
    sub_popup: Rect,
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
fn pickable(s: &str) -> bool {
    !separator(s) && !disabled(s)
}

fn parse(spec: &str) -> (Vec<String>, Vec<Vec<Entry>>) {
    let mut titles = Vec::new();
    let mut menus = Vec::new();
    for group in spec.split(';').filter(|g| !g.trim().is_empty()) {
        let (title, items) = group.split_once('=').unwrap_or((group, ""));
        titles.push(title.trim().to_string());
        menus.push(
            items
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| match s.split_once('>') {
                    Some((t, sub)) => Entry { text: t.trim().to_string(), sub: sub.split('|').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect() },
                    None => Entry { text: s.to_string(), sub: Vec::new() },
                })
                .collect(),
        );
    }
    (titles, menus)
}

impl MenuBar {
    fn items(&self) -> &[Entry] {
        self.menus.get(self.cur).map(Vec::as_slice).unwrap_or(&[])
    }

    fn step_in<'a>(len: usize, ok: impl Fn(usize) -> bool + 'a, from: usize, dir: i32) -> usize {
        let mut i = from as i32 + dir;
        while i >= 0 && (i as usize) < len {
            if ok(i as usize) {
                return i as usize;
            }
            i += dir;
        }
        from
    }

    fn first_item(&self) -> usize {
        let items = self.items();
        (0..items.len()).find(|&i| pickable(&items[i].text)).unwrap_or(0)
    }

    fn close(&mut self) {
        self.open = false;
        self.sub_open = false;
        self.popup = Rect::new(0, 0, 0, 0);
        self.sub_popup = Rect::new(0, 0, 0, 0);
    }

    fn open_menu(&mut self, i: usize) {
        if i < self.menus.len() {
            self.cur = i;
            self.open = true;
            self.sub_open = false;
            self.hi = self.first_item();
        }
    }

    fn has_sub(&self, i: usize) -> bool {
        self.items().get(i).is_some_and(|e| !e.sub.is_empty() && pickable(&e.text))
    }

    fn open_sub(&mut self) {
        if self.has_sub(self.hi) {
            self.sub_open = true;
            self.sub_hi = self.items()[self.hi].sub.iter().position(|s| pickable(s)).unwrap_or(0);
        }
    }

    /// Pick entry `i` of the menu, or `j` of its sub-menu when given; an entry with a sub-menu opens it instead.
    fn pick(&mut self, i: usize, j: Option<usize>, cx: &mut EvCx) -> Res {
        let Some(e) = self.items().get(i).cloned() else { return Res::Handled };
        if !pickable(&e.text) {
            return Res::Handled;
        }
        let title = self.titles[self.cur].clone();
        let (path, n) = match j {
            Some(j) => match e.sub.get(j).filter(|s| pickable(s)) {
                Some(s) => (format!("{title}/{}/{}", shown(&e.text), shown(s)), j),
                None => return Res::Handled,
            },
            None if !e.sub.is_empty() => {
                self.hi = i;
                self.open_sub();
                return Res::Handled;
            }
            None => (format!("{title}/{}", shown(&e.text)), i),
        };
        let m = self.cur;
        self.close();
        cx.emit("activate", &path, m as i64, n as f64);
        Res::Changed
    }

    fn title_at(&self, x: i32, y: i32) -> Option<usize> {
        self.title_rects.iter().position(|r| r.contains(x, y))
    }

    fn row_in(rect: Rect, x: i32, y: i32) -> Option<usize> {
        rect.contains(x, y).then(|| (y - rect.y) as usize)
    }
}

impl Widget for MenuBar {
    fn draw(&mut self, c: &mut Canvas, r: Rect, cx: &Cx) {
        let spec = cx.text_attr("menus", "");
        if spec != self.raw {
            (self.titles, self.menus) = parse(&spec);
            self.raw = spec;
            self.cur = self.cur.min(self.menus.len().saturating_sub(1));
            self.close();
        }
        if !cx.focused {
            self.close();
        }
        let t = cx.theme;
        let base = Style::fg(t.fg).on(crate::gfx::mix(t.bg, t.dim, 0.25));
        self.title_rects.clear();
        if r.w < 1 || r.h < 1 {
            return;
        }
        c.fill(Rect::new(r.x, r.y, r.w, 1), ' ', base);
        let mut x = r.x;
        for (i, title) in self.titles.iter().enumerate() {
            let w = str_width(title) as i32 + 2;
            let rect = Rect::new(x, r.y, w.min((r.x + r.w - x).max(0)), 1);
            let st = if cx.focused && i == self.cur { cx.theme.selected(true) } else { base };
            c.fill(rect, ' ', st);
            c.text_fit(x + 1, r.y, (rect.w - 2).max(0), title, st);
            self.title_rects.push(rect);
            x += w;
            if x >= r.x + r.w {
                break;
            }
        }
    }

    fn overlay(&mut self, c: &mut Canvas, r: Rect, cx: &Cx) {
        self.popup = Rect::new(0, 0, 0, 0);
        self.sub_popup = Rect::new(0, 0, 0, 0);
        if !self.open || self.items().is_empty() {
            return;
        }
        let t = cx.theme;
        let base = Style::fg(t.fg).on(crate::gfx::mix(t.bg, t.dim, 0.25));
        let items = self.items().to_vec();
        let w = (items.iter().map(|e| str_width(shown(&e.text)) as i32 + if e.sub.is_empty() { 0 } else { 2 }).max().unwrap_or(0) + 4).min(c.w).max(4);
        let h = (items.len() as i32).min(c.h);
        let ax = self.title_rects.get(self.cur).map_or(r.x, |t| t.x);
        let x = ax.min(c.w - w).max(0);
        self.popup = Rect::new(x, r.y + 1, w, h);
        c.fill(self.popup, ' ', base);
        for (row, e) in items.iter().take(h as usize).enumerate() {
            let yy = r.y + 1 + row as i32;
            if separator(&e.text) {
                c.fill(Rect::new(x, yy, w, 1), if cx.unicode { '─' } else { '-' }, Style::fg(t.dim).on(base.bg));
                continue;
            }
            let st = if disabled(&e.text) {
                Style::fg(t.dim).on(base.bg)
            } else if row == self.hi {
                cx.theme.selected(true)
            } else {
                base
            };
            c.fill(Rect::new(x, yy, w, 1), ' ', st);
            c.text_fit(x + 1, yy, w - 2, shown(&e.text), st);
            if !e.sub.is_empty() {
                c.text_fit(x + w - 2, yy, 1, if cx.unicode { "▸" } else { ">" }, st);
            }
        }
        if self.sub_open {
            let Some(e) = items.get(self.hi).filter(|e| !e.sub.is_empty()) else { return };
            let sw = (e.sub.iter().map(|s| str_width(shown(s)) as i32).max().unwrap_or(0) + 4).min(c.w).max(4);
            let sh = (e.sub.len() as i32).min(c.h);
            let sx = if x + w + sw <= c.w { x + w } else { (x - sw).max(0) };
            let sy = (r.y + 1 + self.hi as i32).min(c.h - sh).max(0);
            self.sub_popup = Rect::new(sx, sy, sw, sh);
            c.fill(self.sub_popup, ' ', base);
            for (row, s) in e.sub.iter().take(sh as usize).enumerate() {
                let yy = sy + row as i32;
                if separator(s) {
                    c.fill(Rect::new(sx, yy, sw, 1), if cx.unicode { '─' } else { '-' }, Style::fg(t.dim).on(base.bg));
                    continue;
                }
                let st = if disabled(s) {
                    Style::fg(t.dim).on(base.bg)
                } else if row == self.sub_hi {
                    cx.theme.selected(true)
                } else {
                    base
                };
                c.fill(Rect::new(sx, yy, sw, 1), ' ', st);
                c.text_fit(sx + 1, yy, sw - 2, shown(s), st);
            }
        }
    }

    fn extent(&self, r: Rect) -> Rect {
        let mut out = r;
        for p in [self.popup, self.sub_popup].into_iter().filter(|p| !p.is_empty()) {
            let (x0, y0) = (out.x.min(p.x), out.y.min(p.y));
            let (x1, y1) = ((out.x + out.w).max(p.x + p.w), (out.y + out.h).max(p.y + p.h));
            out = Rect::new(x0, y0, x1 - x0, y1 - y0);
        }
        out
    }

    fn act(&mut self, action: &str, arg: &str, cx: &mut EvCx) -> Res {
        match (action, arg.parse::<usize>()) {
            ("open", Ok(i)) if i < self.menus.len() => {
                self.open_menu(i);
                Res::Handled
            }
            _ => {
                let _ = cx;
                Res::Ignored
            }
        }
    }

    fn event(&mut self, r: Rect, ev: &Event, cx: &mut EvCx) -> Res {
        if self.menus.is_empty() {
            return Res::Ignored;
        }
        let n = self.menus.len();
        if !self.open {
            return match ev {
                Event::Key(Key::Enter | Key::Char(' ') | Key::Down, _) => {
                    self.open_menu(self.cur);
                    Res::Handled
                }
                Event::Key(Key::Left, _) if self.cur > 0 => {
                    self.cur -= 1;
                    Res::Handled
                }
                Event::Key(Key::Right, _) if self.cur + 1 < n => {
                    self.cur += 1;
                    Res::Handled
                }
                Event::Mouse(MouseKind::Down(0), x, y, _) if r.contains(*x, *y) => match self.title_at(*x, *y) {
                    Some(i) => {
                        self.open_menu(i);
                        Res::Handled
                    }
                    None => Res::Ignored,
                },
                _ => Res::Ignored,
            };
        }
        let len = self.items().len();
        match ev {
            Event::Key(Key::Up | Key::Down, _) => {
                let dir = if matches!(ev, Event::Key(Key::Up, _)) { -1 } else { 1 };
                if self.sub_open {
                    let sub = self.items()[self.hi].sub.clone();
                    self.sub_hi = Self::step_in(sub.len(), |i| pickable(&sub[i]), self.sub_hi, dir);
                } else {
                    let items = self.items().to_vec();
                    self.hi = Self::step_in(len, |i| pickable(&items[i].text), self.hi, dir);
                }
                Res::Handled
            }
            Event::Key(Key::Right, _) => {
                if self.sub_open {
                    // already inside: nothing further right
                } else if self.has_sub(self.hi) {
                    self.open_sub();
                } else {
                    self.open_menu((self.cur + 1) % n);
                }
                Res::Handled
            }
            Event::Key(Key::Left, _) => {
                if self.sub_open {
                    self.sub_open = false;
                } else {
                    self.open_menu((self.cur + n - 1) % n);
                }
                Res::Handled
            }
            Event::Key(Key::Enter | Key::Char(' '), _) => {
                let (i, j) = (self.hi, self.sub_open.then_some(self.sub_hi));
                self.pick(i, j, cx)
            }
            Event::Key(Key::Esc, _) => {
                if self.sub_open {
                    self.sub_open = false;
                } else {
                    self.close();
                }
                Res::Handled
            }
            Event::Key(Key::Tab | Key::BackTab, _) => {
                self.close();
                Res::Ignored
            }
            Event::Mouse(MouseKind::Down(0), x, y, _) => {
                if let Some(j) = Self::row_in(self.sub_popup, *x, *y) {
                    let i = self.hi;
                    return self.pick(i, Some(j), cx);
                }
                if let Some(i) = Self::row_in(self.popup, *x, *y) {
                    return self.pick(i, None, cx);
                }
                match self.title_at(*x, *y) {
                    Some(i) if i != self.cur => self.open_menu(i),
                    _ => self.close(),
                }
                Res::Handled
            }
            Event::Mouse(MouseKind::Move, x, y, _) => {
                if let Some(j) = Self::row_in(self.sub_popup, *x, *y) {
                    if self.items()[self.hi].sub.get(j).is_some_and(|s| pickable(s)) {
                        self.sub_hi = j;
                    }
                } else if let Some(i) = Self::row_in(self.popup, *x, *y).filter(|&i| i < len && pickable(&self.items()[i].text)) {
                    if i != self.hi {
                        self.hi = i;
                        self.sub_open = false;
                    }
                } else if let Some(i) = self.title_at(*x, *y).filter(|&i| i != self.cur) {
                    self.open_menu(i);
                }
                Res::Handled
            }
            _ => Res::Handled,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::headless::{run_script, snapshot};
    use crate::ui::Ui;

    const SPEC: &str = "col\n  menubar id=mb menus=\"File=Open,Save,-,Recent>a.txt|b.txt,~Quit;Edit=Copy,Paste;Help=About\"\n  text text=below1\n  text text=below2\n  text text=below3\n  text text=below4\n  text text=below5\n  text text=below6\n";

    fn rows(s: &str) -> Vec<&str> {
        s.lines().collect()
    }

    #[test]
    fn parse_menus_entries_and_sub_menus() {
        let (t, m) = parse("File=Open,Recent>a|b;Edit=Copy");
        assert_eq!(t, ["File", "Edit"]);
        assert_eq!(m[0][1], Entry { text: "Recent".into(), sub: vec!["a".into(), "b".into()] });
        assert_eq!(m[1][0].text, "Copy");
    }

    #[test]
    fn titles_in_a_row_and_enter_opens_the_menu_under_its_title() {
        let mut ui = Ui::from_spec(SPEC).unwrap();
        run_script(&mut ui, "size 40 10\nfocus mb").unwrap();
        assert!(rows(&snapshot(&mut ui, 40, 10))[0].contains("File") && rows(&snapshot(&mut ui, 40, 10))[0].contains("Edit"));
        run_script(&mut ui, "key right\nkey enter").unwrap();
        let s = snapshot(&mut ui, 40, 10);
        assert!(rows(&s)[1].contains("Copy") && rows(&s)[2].contains("Paste"), "{s}");
        // the Edit popup starts under the Edit title, not at column 0
        assert_eq!(rows(&s)[1].find("Copy").unwrap(), rows(&s)[0].find("Edit").unwrap());
    }

    #[test]
    fn right_opens_a_sub_menu_enter_picks_with_the_full_path_and_left_steps_back() {
        let mut ui = Ui::from_spec(SPEC).unwrap();
        run_script(&mut ui, "size 40 10\nfocus mb\nkey enter\nkey down\nkey down").unwrap();
        // from Save the separator is skipped and Recent is next; the greyed Quit is never reached
        run_script(&mut ui, "key right").unwrap();
        let s = snapshot(&mut ui, 40, 10);
        assert!(s.contains("a.txt") && s.contains("b.txt"), "{s}");
        run_script(&mut ui, "key left").unwrap();
        assert!(!snapshot(&mut ui, 40, 10).contains("a.txt"));
        run_script(&mut ui, "key right\nkey down\nkey enter\nexpect event mb activate File/Recent/b.txt").unwrap();
        assert!(!snapshot(&mut ui, 40, 10).contains("Open"), "closed after the pick");
    }

    #[test]
    fn plain_pick_greyed_entry_and_esc_levels() {
        let mut ui = Ui::from_spec(SPEC).unwrap();
        run_script(&mut ui, "size 40 10\nfocus mb\nkey enter\nkey enter\nexpect event mb activate File/Open").unwrap();
        run_script(&mut ui, "key enter\nkey down\nkey down\nkey right\nkey esc").unwrap();
        let s = snapshot(&mut ui, 40, 10);
        assert!(s.contains("Recent") && !s.contains("a.txt"), "esc closes the sub-menu first:\n{s}");
        run_script(&mut ui, "key esc").unwrap();
        assert!(!snapshot(&mut ui, 40, 10).contains("Recent"));
        // clicking the greyed Quit does nothing and keeps the menu open
        run_script(&mut ui, "key enter\nclick 2 5").unwrap();
        assert!(snapshot(&mut ui, 40, 10).contains("Quit"));
    }

    #[test]
    fn right_in_a_menu_without_sub_goes_to_the_next_menu_and_wraps() {
        let mut ui = Ui::from_spec(SPEC).unwrap();
        run_script(&mut ui, "size 40 10\nfocus mb\nkey enter\nkey right").unwrap();
        assert!(snapshot(&mut ui, 40, 10).contains("Paste") || snapshot(&mut ui, 40, 10).contains("Copy"));
        run_script(&mut ui, "key right\nkey right").unwrap();
        assert!(snapshot(&mut ui, 40, 10).contains("Open"), "wrapped to File");
    }

    #[test]
    fn mouse_title_switch_and_sub_menu_click() {
        let mut ui = Ui::from_spec(SPEC).unwrap();
        run_script(&mut ui, "size 40 10\nclick 1 0").unwrap();
        assert!(snapshot(&mut ui, 40, 10).contains("Recent"));
        // Recent is row 4 (after Open, Save, separator); a click on it opens the sub-menu, a second on b.txt picks
        run_script(&mut ui, "click 3 4").unwrap();
        assert!(snapshot(&mut ui, 40, 10).contains("b.txt"));
        let s = snapshot(&mut ui, 40, 10);
        let (row, col) = rows(&s).iter().enumerate().find_map(|(i, l)| l.find("b.txt").map(|c| (i, c))).unwrap();
        run_script(&mut ui, &format!("click {col} {row}\nexpect event mb activate File/Recent/b.txt")).unwrap();
    }
}
