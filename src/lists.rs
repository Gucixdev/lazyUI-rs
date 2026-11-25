//! Widgets that hold rows and take keys: tree, dropdown, table.

use crate::canvas::{str_width, Canvas, Rect, Style};
use crate::input::{Event, Key, MouseKind};
use crate::native::{Cx, Def, EvCx, Widget};
use crate::ui::{jstr, Value};
use crate::widgets::Res;
use std::collections::HashSet;

pub static DEFS: &[Def] = &[
    Def { name: "tree", doc: "folders and leaves from paths: items=\"src/a.rs,src/b.rs,README\" (or from=KEY) open=1; right/left fold, enter activates", focus: true, rows: None, live: 0.0, make: || Box::new(Tree::default()) },
    Def { name: "dropdown", doc: "pick one of items=\"a,b,c\" (or from=KEY): bind=KEY gets the index, label=; enter/space opens, a letter jumps to the next item starting with it, wheel and hover move the highlight", focus: true, rows: Some(1), live: 0.0, make: || Box::new(Dropdown::default()) },
    Def { name: "table", doc: "columns=\"a,b\" rows=\"1,2;3,4\" (or from=KEY): up/down, left/right pick a column, s sorts, enter activates", focus: true, rows: None, live: 0.0, make: || Box::new(Table::default()) },
];

fn keep_visible(cursor: usize, top: &mut usize, h: usize) {
    if h == 0 {
        return;
    }
    if cursor < *top {
        *top = cursor;
    } else if cursor >= *top + h {
        *top = cursor + 1 - h;
    }
}

fn sel_style(cx: &Cx) -> Style {
    cx.theme.selected(cx.focused)
}

// ---- tree -------------------------------------------------------------------------------------------

struct Node {
    path: String,
    name: String,
    depth: usize,
    folder: bool,
    parent: Option<usize>,
}

#[derive(Default)]
pub struct Tree {
    src: String,
    nodes: Vec<Node>,
    open: HashSet<String>,
    cursor: usize,
    top: usize,
    seeded: bool,
}

impl Tree {
    fn build(&mut self, items: &[String]) {
        self.nodes.clear();
        let mut seen: std::collections::HashMap<String, usize> = Default::default();
        for it in items {
            let mut prefix = String::new();
            let parts: Vec<&str> = it.split('/').filter(|p| !p.is_empty()).collect();
            let mut parent = None;
            for (d, part) in parts.iter().enumerate() {
                if !prefix.is_empty() {
                    prefix.push('/');
                }
                prefix.push_str(part);
                let last = d + 1 == parts.len();
                let idx = match seen.get(&prefix) {
                    Some(i) => *i,
                    None => {
                        self.nodes.push(Node { path: prefix.clone(), name: part.to_string(), depth: d, folder: !last, parent });
                        seen.insert(prefix.clone(), self.nodes.len() - 1);
                        self.nodes.len() - 1
                    }
                };
                if !last {
                    self.nodes[idx].folder = true; // something lives under it
                }
                parent = Some(idx);
            }
        }
    }

    /// Indices of the nodes that show: every ancestor is open.
    fn visible(&self) -> Vec<usize> {
        let mut out = Vec::new();
        let mut skip_below: Option<usize> = None;
        for (i, n) in self.nodes.iter().enumerate() {
            if let Some(d) = skip_below {
                if n.depth > d {
                    continue;
                }
                skip_below = None;
            }
            out.push(i);
            if n.folder && !self.open.contains(&n.path) {
                skip_below = Some(n.depth);
            }
        }
        out
    }

    fn sync(&mut self, cx: &Cx) {
        let items = cx.items();
        let src = items.join(",");
        if src != self.src || !self.seeded {
            let sel = self.visible().get(self.cursor).map(|i| self.nodes[*i].path.clone());
            self.build(&items);
            if !self.seeded && cx.flag("open") {
                self.open = self.nodes.iter().filter(|n| n.folder).map(|n| n.path.clone()).collect();
            }
            self.src = src;
            self.seeded = true;
            let vis = self.visible();
            self.cursor = sel.and_then(|p| vis.iter().position(|i| self.nodes[*i].path == p)).unwrap_or(0).min(vis.len().saturating_sub(1));
        }
    }

    fn report(&self, cx: &mut EvCx, kind: &str) {
        if let Some(i) = self.visible().get(self.cursor) {
            let n = &self.nodes[*i];
            cx.emit(kind, &n.path, self.cursor as i64, self.cursor as f64);
            if kind == "change" {
                cx.set_bound(Value::Text(n.path.clone()));
            }
        }
    }
}

impl Widget for Tree {
    fn describe(&self, _cx: &Cx) -> String {
        let vis = self.visible();
        let nodes: Vec<String> = vis
            .iter()
            .map(|&i| {
                let n = &self.nodes[i];
                format!("{{\"path\":{},\"name\":{},\"depth\":{},\"folder\":{},\"open\":{}}}", jstr(&n.path), jstr(&n.name), n.depth, n.folder, self.open.contains(&n.path))
            })
            .collect();
        format!("{{\"nodes\":[{}],\"cursor\":{}}}", nodes.join(","), self.cursor)
    }

    fn act(&mut self, action: &str, arg: &str, cx: &mut EvCx) -> Res {
        let vis = self.visible();
        let Some(pos) = vis.iter().position(|i| self.nodes[*i].path == arg) else { return Res::Ignored };
        let before = self.cursor;
        self.cursor = pos;
        let n = &self.nodes[vis[pos]];
        let folder = n.folder;
        let path = n.path.clone();
        let res = match action {
            "toggle" if folder => {
                if !self.open.remove(&path) {
                    self.open.insert(path);
                }
                Res::Handled
            }
            "activate" if !folder => Res::Activate,
            "select" | "activate" | "toggle" => Res::Handled,
            _ => return Res::Ignored,
        };
        let res = if res == Res::Handled && self.cursor != before { Res::Changed } else { res };
        match res {
            Res::Changed => self.report(cx, "change"),
            Res::Activate => self.report(cx, "activate"),
            _ => {}
        }
        res
    }

    fn draw(&mut self, c: &mut Canvas, r: Rect, cx: &Cx) {
        self.sync(cx);
        if r.w < 1 || r.h < 1 {
            return;
        }
        let t = cx.theme;
        let vis = self.visible();
        keep_visible(self.cursor, &mut self.top, r.h as usize);
        for row in 0..r.h {
            let Some(&i) = vis.get(self.top + row as usize) else { break };
            let n = &self.nodes[i];
            let on = self.top + row as usize == self.cursor;
            let mark = if !n.folder {
                "  "
            } else if self.open.contains(&n.path) {
                if cx.unicode { "▾ " } else { "- " }
            } else if cx.unicode {
                "▸ "
            } else {
                "+ "
            };
            let base = if n.folder { Style::fg(t.accent).on(t.bg) } else { t.base() };
            let st = if on { sel_style(cx) } else { base };
            c.fill(Rect::new(r.x, r.y + row, r.w, 1), ' ', st);
            let line = format!("{}{}{}", "  ".repeat(n.depth), mark, n.name);
            c.text_fit(r.x, r.y + row, r.w, &line, st);
        }
    }

    fn event(&mut self, r: Rect, ev: &Event, cx: &mut EvCx) -> Res {
        let vis = self.visible();
        let n = vis.len();
        if n == 0 {
            return Res::Ignored;
        }
        let h = r.h.max(1) as usize;
        let before = self.cursor;
        let cur = vis[self.cursor.min(n - 1)];
        let (folder, open) = (self.nodes[cur].folder, self.open.contains(&self.nodes[cur].path));
        let path = self.nodes[cur].path.clone();
        let mut res = match ev {
            Event::Key(Key::Up, _) => {
                self.cursor = self.cursor.saturating_sub(1);
                Res::Handled
            }
            Event::Key(Key::Down, _) => {
                self.cursor = (self.cursor + 1).min(n - 1);
                Res::Handled
            }
            Event::Key(Key::PageUp, _) => {
                self.cursor = self.cursor.saturating_sub(h);
                Res::Handled
            }
            Event::Key(Key::PageDown, _) => {
                self.cursor = (self.cursor + h).min(n - 1);
                Res::Handled
            }
            Event::Key(Key::Home, _) => {
                self.cursor = 0;
                Res::Handled
            }
            Event::Key(Key::End, _) => {
                self.cursor = n - 1;
                Res::Handled
            }
            Event::Key(Key::Right, _) if folder && !open => {
                self.open.insert(path);
                Res::Handled
            }
            Event::Key(Key::Right, _) => {
                if folder && open && self.cursor + 1 < n {
                    self.cursor += 1; // into the folder
                }
                Res::Handled
            }
            Event::Key(Key::Left, _) => {
                if folder && open {
                    self.open.remove(&path);
                } else if let Some(p) = self.nodes[cur].parent {
                    self.cursor = vis.iter().position(|i| *i == p).unwrap_or(self.cursor);
                }
                Res::Handled
            }
            Event::Key(Key::Char(' '), m) if !m.ctrl && folder => {
                if open {
                    self.open.remove(&path);
                } else {
                    self.open.insert(path);
                }
                Res::Handled
            }
            Event::Key(Key::Enter, _) if folder => {
                if open {
                    self.open.remove(&path);
                } else {
                    self.open.insert(path);
                }
                Res::Handled
            }
            Event::Key(Key::Enter, _) => Res::Activate,
            Event::Mouse(MouseKind::WheelUp, x, y, _) if r.contains(*x, *y) => {
                self.top = self.top.saturating_sub(3);
                Res::Handled
            }
            Event::Mouse(MouseKind::WheelDown, x, y, _) if r.contains(*x, *y) => {
                self.top = (self.top + 3).min(n.saturating_sub(h));
                Res::Handled
            }
            Event::Mouse(MouseKind::Down(0), x, y, _) if r.contains(*x, *y) => {
                let pos = self.top + (*y - r.y) as usize;
                if pos >= n {
                    return Res::Handled;
                }
                let again = pos == self.cursor;
                self.cursor = pos;
                let node = &self.nodes[vis[pos]];
                if node.folder {
                    let p = node.path.clone();
                    if !self.open.remove(&p) {
                        self.open.insert(p);
                    }
                    Res::Handled
                } else if again {
                    Res::Activate
                } else {
                    Res::Handled
                }
            }
            _ => Res::Ignored,
        };
        if res == Res::Handled && self.cursor != before {
            res = Res::Changed;
        }
        match res {
            Res::Changed => self.report(cx, "change"),
            Res::Activate => self.report(cx, "activate"),
            _ => {}
        }
        res
    }
}

// ---- dropdown ---------------------------------------------------------------------------------------

#[derive(Default)]
pub struct Dropdown {
    items: Vec<String>,
    sel: usize,
    hi: usize,
    open: bool,
    popup: Rect,
}

impl Dropdown {
    /// The next item after `from` (wrapping) that starts with `ch`, ignoring case: type-ahead.
    fn jump(&self, from: usize, ch: char) -> Option<usize> {
        let n = self.items.len();
        let first = |s: &String| s.chars().next().is_some_and(|c| c.to_lowercase().eq(ch.to_lowercase()));
        (1..=n).map(|k| (from + k) % n).find(|&i| first(&self.items[i]))
    }

    fn pick(&mut self, i: usize, cx: &mut EvCx) -> Res {
        let changed = i != self.sel;
        self.sel = i;
        self.hi = i;
        self.open = false;
        self.popup = Rect::new(0, 0, 0, 0);
        let text = self.items.get(i).cloned().unwrap_or_default();
        if changed {
            cx.emit("change", &text, i as i64, i as f64);
            cx.set_bound(Value::Num(i as f64));
        }
        cx.emit("activate", &text, i as i64, i as f64);
        Res::Changed
    }
}

impl Widget for Dropdown {
    fn describe(&self, cx: &Cx) -> String {
        let items: Vec<String> = self.items.iter().map(|x| jstr(x)).collect();
        format!("{{\"items\":[{}],\"sel\":{},\"label\":{}}}", items.join(","), self.sel, jstr(&cx.text_attr("label", "")))
    }

    fn act(&mut self, action: &str, arg: &str, cx: &mut EvCx) -> Res {
        match (action, arg.parse::<usize>()) {
            ("select" | "activate", Ok(i)) if i < self.items.len() => self.pick(i, cx),
            _ => Res::Ignored,
        }
    }

    fn draw(&mut self, c: &mut Canvas, r: Rect, cx: &Cx) {
        self.items = cx.items();
        if !cx.focused {
            self.open = false;
        }
        if let Some(k) = cx.attr("bind").and_then(|k| cx.model_num(k)) {
            if !self.open && k >= 0.0 {
                self.sel = k as usize;
            }
        }
        self.sel = self.sel.min(self.items.len().saturating_sub(1));
        if r.w < 1 || r.h < 1 {
            return;
        }
        let t = cx.theme;
        let label = cx.text_attr("label", "");
        let lw = if label.is_empty() { 0 } else { (str_width(&label) as i32 + 1).min(r.w / 2) };
        c.text_fit(r.x, r.y, lw, &label, Style::fg(t.dim).on(t.bg));
        let st = if cx.focused { sel_style(cx) } else { Style::fg(t.fg).on(crate::gfx::mix(t.bg, t.dim, 0.25)) };
        let box_r = Rect::new(r.x + lw, r.y, r.w - lw, 1);
        c.fill(box_r, ' ', st);
        let arrow = if cx.unicode { "▾" } else { "v" };
        let text = self.items.get(self.sel).cloned().unwrap_or_default();
        c.text_fit(box_r.x + 1, r.y, (box_r.w - 3).max(0), &text, st);
        if box_r.w >= 2 {
            c.text_fit(box_r.x + box_r.w - 2, r.y, 1, arrow, st);
        }
    }

    fn overlay(&mut self, c: &mut Canvas, r: Rect, cx: &Cx) {
        if !self.open || self.items.is_empty() {
            self.popup = Rect::new(0, 0, 0, 0);
            return;
        }
        let t = cx.theme;
        let label = cx.text_attr("label", "");
        let lw = if label.is_empty() { 0 } else { (str_width(&label) as i32 + 1).min(r.w / 2) };
        let rows = (self.items.len() as i32).min(8).min((c.h - 1).max(1));
        let below = r.y + 1 + rows <= c.h;
        let y = if below { r.y + 1 } else { (r.y - rows).max(0) };
        let w = (r.w - lw).max(4);
        self.popup = Rect::new(r.x + lw, y, w, rows);
        let top = (self.hi + 1).saturating_sub(rows as usize);
        c.fill(self.popup, ' ', Style::fg(t.fg).on(crate::gfx::mix(t.bg, t.dim, 0.25)));
        for row in 0..rows {
            let i = top + row as usize;
            let Some(txt) = self.items.get(i) else { break };
            let st = if i == self.hi { sel_style(cx) } else { Style::fg(t.fg).on(crate::gfx::mix(t.bg, t.dim, 0.25)) };
            c.fill(Rect::new(self.popup.x, y + row, w, 1), ' ', st);
            c.text_fit(self.popup.x + 1, y + row, w - 2, txt, st);
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

    fn event(&mut self, r: Rect, ev: &Event, cx: &mut EvCx) -> Res {
        let n = self.items.len();
        if n == 0 {
            return Res::Ignored;
        }
        if self.open {
            return match ev {
                Event::Key(Key::Up, _) => {
                    self.hi = self.hi.saturating_sub(1);
                    Res::Handled
                }
                Event::Key(Key::Down, _) => {
                    self.hi = (self.hi + 1).min(n - 1);
                    Res::Handled
                }
                Event::Key(Key::Home, _) => {
                    self.hi = 0;
                    Res::Handled
                }
                Event::Key(Key::End, _) => {
                    self.hi = n - 1;
                    Res::Handled
                }
                Event::Key(Key::Enter | Key::Char(' '), _) => {
                    let i = self.hi;
                    self.pick(i, cx)
                }
                Event::Key(Key::Esc, _) => {
                    self.open = false;
                    self.popup = Rect::new(0, 0, 0, 0);
                    Res::Handled
                }
                Event::Mouse(MouseKind::Down(0), x, y, _) if self.popup.contains(*x, *y) => {
                    let top = (self.hi + 1).saturating_sub(self.popup.h as usize);
                    let i = top + (*y - self.popup.y) as usize;
                    if i < n {
                        self.pick(i, cx)
                    } else {
                        Res::Handled
                    }
                }
                Event::Mouse(MouseKind::Down(0), ..) => {
                    self.open = false;
                    self.popup = Rect::new(0, 0, 0, 0);
                    Res::Handled
                }
                Event::Key(Key::Tab | Key::BackTab, _) => {
                    self.open = false;
                    Res::Ignored
                }
                Event::Key(Key::Char(ch), m) if !m.ctrl && !m.alt => {
                    if let Some(i) = self.jump(self.hi, *ch) {
                        self.hi = i;
                    }
                    Res::Handled
                }
                Event::Mouse(MouseKind::Move, x, y, _) if self.popup.contains(*x, *y) => {
                    let top = (self.hi + 1).saturating_sub(self.popup.h as usize);
                    self.hi = (top + (*y - self.popup.y) as usize).min(n - 1);
                    Res::Handled
                }
                Event::Mouse(MouseKind::WheelUp, ..) => {
                    self.hi = self.hi.saturating_sub(1);
                    Res::Handled
                }
                Event::Mouse(MouseKind::WheelDown, ..) => {
                    self.hi = (self.hi + 1).min(n - 1);
                    Res::Handled
                }
                _ => Res::Handled, // a popup swallows keys so they do not reach hotkeys by accident
            };
        }
        match ev {
            Event::Key(Key::Enter | Key::Char(' ') | Key::Down, _) => {
                self.open = true;
                self.hi = self.sel;
                Res::Handled
            }
            // closed, a letter selects the next item that starts with it, like an HTML select
            Event::Key(Key::Char(ch), m) if !m.ctrl && !m.alt => match self.jump(self.sel, *ch) {
                Some(i) if i != self.sel => {
                    self.sel = i;
                    self.hi = i;
                    let text = self.items[i].clone();
                    cx.emit("change", &text, i as i64, i as f64);
                    cx.set_bound(Value::Num(i as f64));
                    Res::Changed
                }
                _ => Res::Ignored,
            },
            Event::Mouse(MouseKind::Down(0), x, y, _) if r.contains(*x, *y) => {
                self.open = true;
                self.hi = self.sel;
                Res::Handled
            }
            _ => Res::Ignored,
        }
    }
}

// ---- table ------------------------------------------------------------------------------------------

#[derive(Default)]
pub struct Table {
    src: (String, String),
    /// the widest cell of each column (the header counts), kept from the last parse
    natural: Vec<i32>,
    cols: Vec<String>,
    rows: Vec<Vec<String>>,
    numeric: Vec<bool>,
    order: Vec<usize>,
    cursor: usize,
    top: usize,
    col: usize,
    sort: Option<(usize, bool)>,
}

impl Table {
    fn sync(&mut self, cx: &Cx) {
        let cols = cx.list_attr("columns");
        let raw = match cx.attr("from").and_then(|k| cx.model_text(k)) {
            Some(t) => t,
            None => cx.text_attr("rows", ""),
        };
        let joined = cols.join(",");
        if self.src.0 == joined && self.src.1 == raw {
            return;
        }
        self.src = (joined, raw.clone());
        self.rows = raw.split(';').filter(|r| !r.trim().is_empty()).map(|r| r.split(',').map(|c| c.trim().to_string()).collect()).collect();
        let n = cols.len().max(self.rows.iter().map(|r| r.len()).max().unwrap_or(0));
        self.cols = (0..n).map(|i| cols.get(i).cloned().unwrap_or_default()).collect();
        self.numeric = (0..n).map(|c| !self.rows.is_empty() && self.rows.iter().all(|r| r.get(c).map_or(true, |s| s.is_empty() || s.parse::<f64>().is_ok()))).collect();
        self.natural = (0..n).map(|c| self.rows.iter().map(|r| r.get(c).map_or(0, |s| str_width(s)) as i32).chain([str_width(&self.cols[c]) as i32 + 1]).max().unwrap_or(1)).collect();
        self.col = self.col.min(n.saturating_sub(1));
        self.resort();
        self.cursor = self.cursor.min(self.rows.len().saturating_sub(1));
    }

    fn resort(&mut self) {
        self.order = (0..self.rows.len()).collect();
        if let Some((c, desc)) = self.sort {
            let num = self.numeric.get(c).copied().unwrap_or(false);
            let rows = &self.rows;
            let cell = |i: usize| rows[i].get(c).cloned().unwrap_or_default();
            self.order.sort_by(|a, b| {
                let o = if num {
                    cell(*a).parse::<f64>().unwrap_or(0.0).total_cmp(&cell(*b).parse::<f64>().unwrap_or(0.0))
                } else {
                    cell(*a).to_lowercase().cmp(&cell(*b).to_lowercase())
                };
                if desc { o.reverse() } else { o }
            });
        }
    }

    fn widths(&self, total: i32) -> Vec<i32> {
        let mut w = self.natural.clone();
        let gaps = (w.len() as i32 - 1).max(0);
        while w.iter().sum::<i32>() + gaps > total {
            let (i, m) = w.iter().enumerate().max_by_key(|(_, v)| **v).map(|(i, v)| (i, *v)).unwrap_or((0, 0));
            if m <= 3 {
                break;
            }
            w[i] -= 1;
        }
        w
    }

    fn report(&self, cx: &mut EvCx, kind: &str) {
        if let Some(&i) = self.order.get(self.cursor) {
            cx.emit(kind, &self.rows[i].join(","), i as i64, i as f64);
            if kind == "change" {
                cx.set_bound(Value::Num(i as f64));
            }
        }
    }
}

impl Widget for Table {
    fn describe(&self, _cx: &Cx) -> String {
        let cols: Vec<String> = self.cols.iter().map(|c| jstr(c)).collect();
        let rows: Vec<String> = self.order.iter().map(|&i| format!("[{}]", self.rows[i].iter().map(|c| jstr(c)).collect::<Vec<_>>().join(","))).collect();
        let num: Vec<String> = self.numeric.iter().map(|b| b.to_string()).collect();
        let sort = match self.sort {
            Some((c, d)) => format!("[{c},{d}]"),
            None => "null".into(),
        };
        format!("{{\"cols\":[{}],\"rows\":[{}],\"numeric\":[{}],\"cursor\":{},\"sort\":{}}}", cols.join(","), rows.join(","), num.join(","), self.cursor, sort)
    }

    fn act(&mut self, action: &str, arg: &str, cx: &mut EvCx) -> Res {
        match (action, arg.parse::<usize>()) {
            ("select" | "activate", Ok(pos)) if pos < self.order.len() => {
                let changed = pos != self.cursor;
                self.cursor = pos;
                if action == "activate" {
                    self.report(cx, "activate");
                    Res::Activate
                } else if changed {
                    self.report(cx, "change");
                    Res::Changed
                } else {
                    Res::Handled
                }
            }
            ("sort", Ok(col)) if col < self.cols.len() => {
                self.col = col;
                self.sort = match self.sort {
                    Some((c, false)) if c == col => Some((col, true)),
                    Some((c, true)) if c == col => None,
                    _ => Some((col, false)),
                };
                let keep = self.order.get(self.cursor).copied();
                self.resort();
                self.cursor = keep.and_then(|k| self.order.iter().position(|i| *i == k)).unwrap_or(0);
                Res::Handled
            }
            _ => Res::Ignored,
        }
    }

    fn draw(&mut self, c: &mut Canvas, r: Rect, cx: &Cx) {
        self.sync(cx);
        if r.w < 1 || r.h < 1 || self.cols.is_empty() {
            return;
        }
        let t = cx.theme;
        let w = self.widths(r.w);
        let head = Style::fg(t.accent).on(t.bg).with(crate::canvas::BOLD);
        let mut x = r.x;
        for (ci, cw) in w.iter().enumerate() {
            let mark = match self.sort {
                Some((sc, desc)) if sc == ci => if desc { if cx.unicode { "▼" } else { "v" } } else if cx.unicode { "▲" } else { "^" },
                _ => "",
            };
            let st = if ci == self.col && cx.focused { head.with(crate::canvas::UNDERLINE | crate::canvas::BOLD) } else { head };
            c.text_fit(x, r.y, *cw, &format!("{}{}", self.cols[ci], mark), st);
            x += cw + 1;
        }
        let bh = (r.h - 1).max(0) as usize;
        keep_visible(self.cursor, &mut self.top, bh);
        for row in 0..bh {
            let Some(&ri) = self.order.get(self.top + row) else { break };
            let on = self.top + row == self.cursor;
            let st = if on { sel_style(cx) } else { t.base() };
            let y = r.y + 1 + row as i32;
            c.fill(Rect::new(r.x, y, r.w, 1), ' ', st);
            let mut x = r.x;
            for (ci, cw) in w.iter().enumerate() {
                let s = self.rows[ri].get(ci).map(|s| s.as_str()).unwrap_or("");
                let pad = if self.numeric[ci] { (*cw - str_width(s) as i32).max(0) } else { 0 };
                c.text_fit(x + pad, y, *cw - pad, s, st);
                x += cw + 1;
            }
        }
    }

    fn event(&mut self, r: Rect, ev: &Event, cx: &mut EvCx) -> Res {
        let n = self.order.len();
        let h = (r.h - 1).max(1) as usize;
        let before = self.cursor;
        let mut res = match ev {
            Event::Key(Key::Up, _) => {
                self.cursor = self.cursor.saturating_sub(1);
                Res::Handled
            }
            Event::Key(Key::Down, _) => {
                self.cursor = (self.cursor + 1).min(n.saturating_sub(1));
                Res::Handled
            }
            Event::Key(Key::PageUp, _) => {
                self.cursor = self.cursor.saturating_sub(h);
                Res::Handled
            }
            Event::Key(Key::PageDown, _) => {
                self.cursor = (self.cursor + h).min(n.saturating_sub(1));
                Res::Handled
            }
            Event::Key(Key::Home, _) => {
                self.cursor = 0;
                Res::Handled
            }
            Event::Key(Key::End, _) => {
                self.cursor = n.saturating_sub(1);
                Res::Handled
            }
            Event::Key(Key::Left, _) => {
                self.col = self.col.saturating_sub(1);
                Res::Handled
            }
            Event::Key(Key::Right, _) => {
                self.col = (self.col + 1).min(self.cols.len().saturating_sub(1));
                Res::Handled
            }
            Event::Key(Key::Char('s'), m) if !m.ctrl && !m.alt => {
                // ascending, then descending, then the order as given
                let col = self.col;
                self.sort = match self.sort {
                    Some((c, false)) if c == col => Some((col, true)),
                    Some((c, true)) if c == col => None,
                    _ => Some((col, false)),
                };
                let keep = self.order.get(self.cursor).copied();
                self.resort();
                self.cursor = keep.and_then(|k| self.order.iter().position(|i| *i == k)).unwrap_or(0);
                Res::Handled
            }
            Event::Key(Key::Enter, _) if n > 0 => Res::Activate,
            Event::Mouse(MouseKind::WheelUp, x, y, _) if r.contains(*x, *y) => {
                self.top = self.top.saturating_sub(3);
                Res::Handled
            }
            Event::Mouse(MouseKind::WheelDown, x, y, _) if r.contains(*x, *y) => {
                self.top = (self.top + 3).min(n.saturating_sub(h));
                Res::Handled
            }
            Event::Mouse(MouseKind::Down(0), x, y, _) if r.contains(*x, *y) && *y > r.y => {
                let pos = self.top + (*y - r.y - 1) as usize;
                if pos >= n {
                    return Res::Handled;
                }
                let again = pos == self.cursor;
                self.cursor = pos;
                if again { Res::Activate } else { Res::Changed }
            }
            _ => Res::Ignored,
        };
        if res == Res::Handled && self.cursor != before {
            res = Res::Changed;
        }
        match res {
            Res::Changed => self.report(cx, "change"),
            Res::Activate => self.report(cx, "activate"),
            _ => {}
        }
        res
    }
}

#[cfg(test)]
mod tests {
    use crate::headless::{run_script, snapshot};
    use crate::ui::Ui;

    fn rows(s: &str) -> Vec<&str> {
        s.lines().map(|l| l.trim_end()).collect()
    }

    #[test]
    fn tree_folds_and_activates() {
        let mut ui = Ui::from_spec("tree id=t items=\"src/a.rs,src/b.rs,doc/x.md,README\" bind=sel").unwrap();
        run_script(&mut ui, "size 20 8").unwrap();
        let s = snapshot(&mut ui, 20, 8);
        assert_eq!(&rows(&s)[..3], &["+ src", "+ doc", "  README"], "{s}");
        run_script(&mut ui, "key right").unwrap();
        let s = snapshot(&mut ui, 20, 8);
        assert_eq!(&rows(&s)[..4], &["- src", "    a.rs", "    b.rs", "+ doc"], "{s}");
        // down into the folder, enter on a leaf activates and the path is bound
        run_script(&mut ui, "key down\nexpect event t change src/a.rs\nkey enter\nexpect event t activate src/a.rs\nexpect model sel src/a.rs").unwrap();
        // left on a leaf goes to its folder, left again folds it
        run_script(&mut ui, "key left\nexpect event t change src\nkey left").unwrap();
        assert_eq!(rows(&snapshot(&mut ui, 20, 8))[1], "+ doc");
        run_script(&mut ui, "click 0 2\nexpect event t activate README").unwrap_err(); // first click only selects
        run_script(&mut ui, "click 3 2\nclick 3 2").unwrap();
        assert_eq!(ui.text_of("sel"), "README");
    }

    #[test]
    fn tree_open_all_and_same_names() {
        let mut ui = Ui::from_spec("tree items=\"a/b/c,a/b/d,a/e\" open=1").unwrap();
        let s = snapshot(&mut ui, 16, 6);
        assert_eq!(&rows(&s)[..5], &["- a", "  - b", "      c", "      d", "    e"], "{s}");
    }

    #[test]
    fn dropdown_opens_over_its_neighbours_and_picks() {
        let mut ui = Ui::from_spec("col\n  dropdown id=d items=\"red,green,blue\" label=colour bind=pick\n  text text=below1\n  text text=below2\n  text text=below3\n  text text=below4\n").unwrap();
        run_script(&mut ui, "size 24 6").unwrap();
        assert!(rows(&snapshot(&mut ui, 24, 6))[0].starts_with("colour  red"), "{}", snapshot(&mut ui, 24, 6));
        run_script(&mut ui, "key enter\nkey down\nkey down").unwrap();
        let s = snapshot(&mut ui, 24, 6);
        assert!(rows(&s)[1].contains("red") && rows(&s)[2].contains("green") && rows(&s)[3].contains("blue"), "popup lies over the texts:\n{s}");
        run_script(&mut ui, "key enter\nexpect event d change blue\nexpect event d activate blue\nexpect model pick 2").unwrap();
        let s = snapshot(&mut ui, 24, 6);
        assert!(rows(&s)[0].contains("blue") && rows(&s)[1] == "below1", "closed again:\n{s}");
        // escape closes without picking; a click on a popup row picks it
        run_script(&mut ui, "key enter\nkey up\nkey esc\nexpect model pick 2\nclick 10 0\nclick 8 2\nexpect model pick 1").unwrap();
    }

    #[test]
    fn dropdown_type_ahead_wheel_hover_and_a_long_popup_that_flips_up() {
        let items = "apple,apricot,banana,blueberry,cherry,date,elder,fig,grape,honeydew";
        let spec = format!("col\n  text text=top size=1\n  spacer\n  dropdown id=d items=\"{items}\" bind=pick size=1\n");
        let mut ui = Ui::from_spec(&spec).unwrap();
        run_script(&mut ui, "size 24 6\nfocus d").unwrap();
        // closed: a letter selects the next item that starts with it, no popup
        run_script(&mut ui, "size 24 6\nkey b\nexpect event d change banana\nexpect model pick 2\nkey b\nexpect event d change blueberry\nkey a\nexpect event d change apple").unwrap();
        // the dropdown is on the last row: the popup opens upwards, 8 rows at most
        run_script(&mut ui, "size 24 6\nkey enter").unwrap();
        let s = snapshot(&mut ui, 24, 6);
        let r = rows(&s);
        assert!(r[0].contains("apple") && r[4].contains("cherry") && !r[0].contains("top"), "five rows, over the text above the box:\n{s}");
        assert!(r[5].contains("apple"), "{s}");
        // open: type-ahead moves the highlight only, the wheel steps it, enter picks it
        run_script(&mut ui, "size 24 6\nkey g\nwheel 3 5 down\nkey enter\nexpect event d change honeydew\nexpect model pick 9").unwrap();
        // a click on the top row of the popup picks what is there: the window follows the highlight, so it shows date..honeydew
        run_script(&mut ui, "size 24 6\nkey enter\nclick 0 0\nexpect event d change date\nexpect model pick 5").unwrap();
        let s = snapshot(&mut ui, 24, 6);
        assert!(rows(&s)[0].starts_with("top") && rows(&s)[5].contains("date"), "closed again:\n{s}");
    }

    #[test]
    fn table_sorts_numbers_as_numbers() {
        let mut ui = Ui::from_spec("table id=t columns=\"name,n\" rows=\"b,10;a,9;c,100\" bind=row").unwrap();
        let s = snapshot(&mut ui, 12, 5);
        assert_eq!(&rows(&s)[..4], &["name  n", "b      10", "a       9", "c     100"], "{s}");
        run_script(&mut ui, "key right\nkey s").unwrap();
        let s = snapshot(&mut ui, 12, 5);
        assert_eq!(&rows(&s)[1..4], &["a       9", "b      10", "c     100"], "9 before 10: {s}");
        assert!(rows(&s)[0].contains('^'), "{s}");
        run_script(&mut ui, "key s").unwrap();
        assert_eq!(rows(&snapshot(&mut ui, 12, 5))[1], "c     100");
        run_script(&mut ui, "key s").unwrap();
        assert_eq!(rows(&snapshot(&mut ui, 12, 5))[1], "b      10", "third press: back to the given order");
        run_script(&mut ui, "key down\nexpect event t change a,9\nexpect model row 1\nkey enter\nexpect event t activate a,9").unwrap();
    }

    #[test]
    fn table_squeezes_wide_cells_and_survives_nothing() {
        let mut ui = Ui::from_spec("table columns=\"a,b\" rows=\"0123456789abcdef,x\"").unwrap();
        let s = snapshot(&mut ui, 10, 3);
        assert!(rows(&s).iter().all(|l| l.len() <= 10), "{s}");
        let mut empty = Ui::from_spec("table columns=\"a,b\"").unwrap();
        assert!(snapshot(&mut empty, 10, 3).starts_with("a"));
        let mut none = Ui::from_spec("table").unwrap();
        snapshot(&mut none, 10, 3);
    }
}
