//! Widgets. One contract: state + `draw(canvas, rect, theme, focused)` + `event(rect, &Event) -> Res`.
//! Every function accepts any rectangle (empty, negative, outside the canvas) and any number.

use crate::canvas::{char_width, str_width, Canvas, Color, Rect, Style, BOLD, REVERSE};
use crate::input::{Event, Key, MouseKind};
use crate::theme::Theme;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Res {
    Ignored,
    Handled,
    Changed,
    Activate,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Align {
    Left,
    Center,
    Right,
}

/// Bordered box with a title; returns the inside.
pub fn draw_panel(c: &mut Canvas, r: Rect, title: &str, focused: bool, t: &Theme) -> Rect {
    if r.is_empty() {
        return Rect::default();
    }
    let old = c.set_clip(r);
    c.border(r, title, t.border_style(focused), t.title_style(focused));
    c.set_clip(old);
    r.inset(1)
}

pub fn draw_text(c: &mut Canvas, r: Rect, s: &str, style: Style, align: Align) {
    if r.is_empty() {
        return;
    }
    let old = c.set_clip(r);
    for (i, line) in s.lines().enumerate().take(r.h as usize) {
        let w = str_width(line) as i32;
        let x = match align {
            Align::Left => r.x,
            Align::Center => r.x + ((r.w - w) / 2).max(0),
            Align::Right => r.x + (r.w - w).max(0),
        };
        c.text_fit(x, r.y + i as i32, r.w - (x - r.x), line, style);
    }
    c.set_clip(old);
}

/// One-row bar: filled part, empty part, and `label` centred over it.
pub fn draw_progress(c: &mut Canvas, r: Rect, frac: f64, label: &str, t: &Theme) {
    if r.is_empty() {
        return;
    }
    let old = c.set_clip(r);
    let g = c.glyphs();
    let frac = if frac.is_finite() { frac.clamp(0.0, 1.0) } else { 0.0 };
    let filled = (frac * r.w as f64).round() as i32;
    let on = Style::fg(t.accent).on(t.bg);
    let off = Style::fg(t.border).on(t.bg);
    for i in 0..r.w {
        if i < filled {
            c.put(r.x + i, r.y, g.full, on);
        } else {
            c.put(r.x + i, r.y, g.empty, off);
        }
    }
    if !label.is_empty() {
        let w = str_width(label) as i32;
        let x = r.x + ((r.w - w) / 2).max(0);
        for (i, ch) in label.chars().enumerate() {
            let cx = x + i as i32;
            let inside = cx - r.x < filled;
            let st = if inside { Style::new(t.sel_fg, t.accent).with(BOLD) } else { Style::new(t.fg, t.bg).with(BOLD) };
            c.put(cx, r.y, ch, st);
        }
    }
    c.set_clip(old);
}

// ---- list ---------------------------------------------------------------------------------------

#[derive(Clone, Default, Debug)]
pub struct List {
    pub items: Vec<String>,
    /// index into `shown`
    pub cursor: usize,
    pub top: usize,
    filter: String,
    shown: Vec<usize>,
    lazy: Option<Lazy>,
    /// `multi`: space, ctrl-click and ctrl-a mark rows; the marks are indices into `items`
    pub multi: bool,
    marks: std::collections::BTreeSet<usize>,
}

/// A list whose rows live elsewhere (a file, a database, a socket): it knows how many there are and keeps only the rows it
/// was handed, the visible window plus a margin. Drawing asks for what is missing through `take_need`.
#[derive(Clone, Default, Debug)]
struct Lazy {
    total: usize,
    rows: std::collections::BTreeMap<usize, String>,
    /// the range last asked for (start, len), so a slow source is not asked again on every frame
    asked: Option<(usize, usize)>,
    need: Option<(usize, usize)>,
}

/// Loaded rows beyond this are dropped, farthest from the window first.
const LAZY_KEEP: usize = 4096;

impl List {
    /// A list of `total` rows, none loaded yet. The filter does not apply: the source filters and says the new total.
    pub fn lazy(total: usize) -> List {
        List { lazy: Some(Lazy { total, ..Default::default() }), ..Default::default() }
    }
    pub fn is_lazy(&self) -> bool {
        self.lazy.is_some()
    }
    /// New row count (the source changed). Rows past the end are forgotten, the cursor stays inside.
    pub fn set_total(&mut self, total: usize) {
        let l = self.lazy.get_or_insert_with(Lazy::default);
        l.total = total;
        l.rows.retain(|&i, _| i < total);
        l.asked = None;
        self.cursor = self.cursor.min(total.saturating_sub(1));
        self.top = self.top.min(self.cursor);
    }
    /// Forget every loaded row (the source changed under the same total).
    pub fn forget_rows(&mut self) {
        if let Some(l) = &mut self.lazy {
            l.rows.clear();
            l.asked = None;
        }
    }
    /// Rows `start..` arrived. Far rows are dropped when the cache grows past its limit.
    pub fn put_rows(&mut self, start: usize, rows: Vec<String>) {
        let top = self.top;
        let Some(l) = &mut self.lazy else { return };
        for (k, r) in rows.into_iter().enumerate() {
            if start + k < l.total {
                l.rows.insert(start + k, r);
            }
        }
        l.asked = None;
        while l.rows.len() > LAZY_KEEP {
            let first = *l.rows.keys().next().unwrap();
            let last = *l.rows.keys().next_back().unwrap();
            let far = if top.saturating_sub(first) > last.saturating_sub(top) { first } else { last };
            l.rows.remove(&far);
        }
    }
    /// The range (start, len) the last draw found missing and nobody has been asked for yet. Once per range.
    pub fn take_need(&mut self) -> Option<(usize, usize)> {
        let l = self.lazy.as_mut()?;
        let n = l.need.take()?;
        l.asked = Some(n);
        Some(n)
    }
    /// Row position under screen row `y` of a list drawn in `r`, `None` below the last row. `clamp`: the nearest row instead.
    pub fn row_at(&self, r: Rect, y: i32, clamp: bool) -> Option<usize> {
        let n = self.n();
        if n == 0 {
            return None;
        }
        let pos = self.top as i64 + (y - r.y) as i64;
        match (pos < 0, pos as usize >= n) {
            (true, _) => clamp.then_some(0),
            (_, true) => clamp.then_some(n - 1),
            _ => Some(pos as usize),
        }
    }
    /// The text of row `pos` (for a lazy list only when it is loaded).
    pub fn row_text(&self, pos: usize) -> Option<&str> {
        self.row(pos)
    }
    /// Move row `from` to position `to`, the cursor follows. Only for a plain list without a filter: with a filter the
    /// positions are not the items', with a lazy list the rows are not here. False when nothing moved.
    pub fn move_row(&mut self, from: usize, to: usize) -> bool {
        if self.lazy.is_some() || !self.filter.is_empty() || from == to || from >= self.items.len() || to >= self.items.len() {
            return false;
        }
        let it = self.items.remove(from);
        self.items.insert(to, it);
        self.refilter();
        self.cursor = to;
        true
    }
    fn n(&self) -> usize {
        self.lazy.as_ref().map_or(self.shown.len(), |l| l.total)
    }
    fn row(&self, pos: usize) -> Option<&str> {
        match &self.lazy {
            Some(l) => l.rows.get(&pos).map(String::as_str),
            None => self.shown.get(pos).map(|&i| self.items[i].as_str()),
        }
    }
    /// Look at the window `top .. top + 2h` (the second screenful is the margin) and note the first hole in it.
    fn note_need(&mut self, h: usize) {
        let top = self.top;
        let Some(l) = &mut self.lazy else { return };
        let end = l.total.min(top + 2 * h.max(1));
        let Some(a) = (top..end).find(|i| !l.rows.contains_key(i)) else { return };
        let b = (a..end).find(|i| l.rows.contains_key(i)).unwrap_or(end);
        let want = (a, b - a);
        if l.asked != Some(want) {
            l.need = Some(want);
        }
    }

    pub fn new(items: Vec<String>) -> List {
        let mut l = List { items, ..Default::default() };
        l.refilter();
        l
    }
    /// Marked rows as indices into `items`, ascending.
    pub fn marks(&self) -> Vec<usize> {
        self.marks.iter().copied().collect()
    }
    pub fn clear_marks(&mut self) {
        self.marks.clear();
    }
    fn toggle(&mut self, pos: usize) {
        if let Some(&i) = self.shown.get(pos) {
            if !self.marks.remove(&i) {
                self.marks.insert(i);
            }
        }
    }
    pub fn set_items(&mut self, items: Vec<String>) {
        self.marks.clear();
        self.items = items;
        self.refilter();
    }
    pub fn set_filter(&mut self, f: &str) {
        if self.lazy.is_none() && f != self.filter {
            self.filter = f.to_string();
            self.cursor = 0;
            self.top = 0;
            self.refilter();
        }
    }
    fn refilter(&mut self) {
        if self.lazy.is_some() {
            return;
        }
        let f = self.filter.to_lowercase();
        self.shown = (0..self.items.len()).filter(|&i| f.is_empty() || self.items[i].to_lowercase().contains(&f)).collect();
        self.cursor = self.cursor.min(self.shown.len().saturating_sub(1));
        self.top = self.top.min(self.cursor);
    }
    pub fn len(&self) -> usize {
        self.n()
    }
    pub fn is_empty(&self) -> bool {
        self.n() == 0
    }
    /// Index into `items` of the row under the cursor (the row number itself for a lazy list).
    pub fn selected(&self) -> Option<usize> {
        match &self.lazy {
            Some(l) => (self.cursor < l.total).then_some(self.cursor),
            None => self.shown.get(self.cursor).copied(),
        }
    }
    /// The rows that pass the filter, in order (a lazy list: the rows loaded so far).
    pub fn shown_items(&self) -> Vec<&str> {
        match &self.lazy {
            Some(l) => l.rows.values().map(String::as_str).collect(),
            None => self.shown.iter().map(|&i| self.items[i].as_str()).collect(),
        }
    }
    pub fn selected_text(&self) -> Option<&str> {
        self.row(self.cursor)
    }
    pub fn select(&mut self, item_index: usize) {
        if let Some(l) = &self.lazy {
            if item_index < l.total {
                self.cursor = item_index;
            }
        } else if let Some(p) = self.shown.iter().position(|&i| i == item_index) {
            self.cursor = p;
        }
    }
    fn keep_visible(&mut self, h: usize) {
        if h == 0 {
            return;
        }
        if self.cursor < self.top {
            self.top = self.cursor;
        }
        if self.cursor >= self.top + h {
            self.top = self.cursor + 1 - h;
        }
        let n = self.n();
        self.top = self.top.min(n.saturating_sub(h.min(n)));
    }
    pub fn draw(&mut self, c: &mut Canvas, r: Rect, t: &Theme, focused: bool) {
        if r.is_empty() {
            return;
        }
        self.keep_visible(r.h as usize);
        let old = c.set_clip(r);
        let g = c.glyphs();
        self.note_need(r.h as usize);
        let more = self.n() > r.h as usize;
        let tw = r.w - if more { 1 } else { 0 };
        for row in 0..r.h {
            let pos = self.top + row as usize;
            let y = r.y + row;
            if pos >= self.n() {
                break;
            }
            let sel = pos == self.cursor;
            let st = if sel { t.selected(focused) } else { t.base() };
            c.fill(Rect::new(r.x, y, tw, 1), ' ', st);
            if sel {
                c.put(r.x, y, g.pointer, st);
            }
            if self.lazy.is_none() && self.shown.get(pos).is_some_and(|i| self.marks.contains(i)) {
                c.put(r.x + 1, y, '*', Style::fg(t.accent).on(st.bg).with(BOLD));
            }
            match self.row(pos) {
                Some(text) => {
                    // a tab splits a row into a left part and one aligned to the right edge (a name and its size)
                    let (left, right) = text.split_once('\t').unwrap_or((text, ""));
                    c.text_fit(r.x + 2, y, tw - 2, left, st);
                    let rw = crate::canvas::str_width(right) as i32;
                    let lw = crate::canvas::str_width(left) as i32;
                    if rw > 0 && 2 + lw + 2 + rw < tw {
                        c.text_fit(r.x + tw - rw - 1, y, rw, right, st);
                    }
                }
                None => {
                    c.text_fit(r.x + 2, y, tw - 2, "...", Style::fg(t.border).on(t.bg));
                }
            }
        }
        if more {
            // scrollbar: the thumb is proportional
            let n = self.n() as f64;
            let h = r.h as f64;
            let th = ((h * h / n).round() as i32).clamp(1, r.h);
            let ty = ((self.top as f64 / (n - h).max(1.0)) * (r.h - th) as f64).round() as i32;
            for row in 0..r.h {
                let on = row >= ty && row < ty + th;
                c.put(r.x + r.w - 1, r.y + row, if on { g.full } else { g.v }, Style::fg(if on { t.accent } else { t.border }).on(t.bg));
            }
        }
        c.set_clip(old);
    }
    pub fn event(&mut self, r: Rect, ev: &Event) -> Res {
        let n = self.n();
        let h = r.h.max(1) as usize;
        let before = self.cursor;
        let moved = |me: &mut List, to: usize| -> Res {
            me.cursor = to.min(n.saturating_sub(1));
            if me.cursor != before {
                Res::Changed
            } else {
                Res::Handled
            }
        };
        match ev {
            Event::Key(Key::Up, _) => moved(self, self.cursor.saturating_sub(1)),
            Event::Key(Key::Down, _) => moved(self, self.cursor + 1),
            Event::Key(Key::PageUp, _) => moved(self, self.cursor.saturating_sub(h)),
            Event::Key(Key::PageDown, _) => moved(self, self.cursor + h),
            Event::Key(Key::Home, _) => moved(self, 0),
            Event::Key(Key::End, _) => moved(self, n.saturating_sub(1)),
            Event::Key(Key::Enter, _) if n > 0 => Res::Activate,
            Event::Key(Key::Char(' '), m) if self.multi && !m.ctrl && n > 0 => {
                self.toggle(self.cursor);
                self.cursor = (self.cursor + 1).min(n - 1);
                Res::Changed
            }
            Event::Key(Key::Char('a'), m) if self.multi && m.ctrl => {
                if self.marks.len() == self.shown.len() {
                    self.marks.clear();
                } else {
                    self.marks = self.shown.iter().copied().collect();
                }
                Res::Changed
            }
            Event::Mouse(MouseKind::Down(0), x, y, m) if self.multi && m.ctrl && r.contains(*x, *y) => {
                let pos = self.top + (*y - r.y) as usize;
                if pos < n {
                    self.toggle(pos);
                    self.cursor = pos;
                }
                Res::Changed
            }
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
                if again {
                    Res::Activate
                } else {
                    Res::Changed
                }
            }
            _ => Res::Ignored,
        }
    }
}

// ---- text input -----------------------------------------------------------------------------------

#[derive(Clone, Default, Debug)]
pub struct Input {
    pub text: String,
    /// char index
    pub cur: usize,
    scroll: usize,
}

impl Input {
    pub fn new(text: &str) -> Input {
        Input { text: text.to_string(), cur: text.chars().count(), scroll: 0 }
    }
    pub fn set(&mut self, s: &str) {
        if s != self.text {
            self.text = s.to_string();
            self.cur = self.cur.min(self.text.chars().count());
        }
    }
    fn byte(&self, ci: usize) -> usize {
        self.text.char_indices().nth(ci).map(|(b, _)| b).unwrap_or(self.text.len())
    }
    fn len(&self) -> usize {
        self.text.chars().count()
    }
    /// Draws `prompt` then the text; the caret is a reversed cell while focused.
    pub fn draw(&mut self, c: &mut Canvas, r: Rect, prompt: &str, t: &Theme, focused: bool) {
        if r.is_empty() {
            return;
        }
        let old = c.set_clip(r);
        c.fill(Rect::new(r.x, r.y, r.w, 1), ' ', t.base());
        let pw = c.text(r.x, r.y, prompt, Style::fg(t.accent).on(t.bg).with(BOLD));
        let avail = (r.w - pw).max(1) as usize;
        if self.cur < self.scroll {
            self.scroll = self.cur;
        }
        // scroll so the caret column is inside; widths counted in columns
        loop {
            let cols: usize = self.text.chars().skip(self.scroll).take(self.cur - self.scroll).map(char_width).sum();
            if cols >= avail && self.scroll < self.cur {
                self.scroll += 1;
            } else {
                break;
            }
        }
        let mut x = r.x + pw;
        for ch in self.text.chars().skip(self.scroll) {
            if x >= r.x + r.w {
                break;
            }
            x += c.text(x, r.y, &ch.to_string(), t.base());
        }
        if focused {
            let cols: i32 = self.text.chars().skip(self.scroll).take(self.cur - self.scroll).map(|c| char_width(c) as i32).sum();
            let cx = r.x + pw + cols;
            let ch = self.text.chars().nth(self.cur).unwrap_or(' ');
            c.put(cx, r.y, ch, Style::new(t.fg, t.bg).with(REVERSE));
        } else if self.text.is_empty() {
            // nothing: the prompt is the hint
        }
        c.set_clip(old);
    }
    pub fn event(&mut self, r: Rect, prompt_w: i32, ev: &Event) -> Res {
        match ev {
            Event::Key(Key::Char(ch), m) if !m.ctrl && !m.alt => {
                let b = self.byte(self.cur);
                self.text.insert(b, *ch);
                self.cur += 1;
                Res::Changed
            }
            Event::Key(Key::Backspace, _) => {
                if self.cur == 0 {
                    return Res::Handled;
                }
                let (a, b) = (self.byte(self.cur - 1), self.byte(self.cur));
                self.text.replace_range(a..b, "");
                self.cur -= 1;
                Res::Changed
            }
            Event::Key(Key::Delete, _) => {
                if self.cur >= self.len() {
                    return Res::Handled;
                }
                let (a, b) = (self.byte(self.cur), self.byte(self.cur + 1));
                self.text.replace_range(a..b, "");
                Res::Changed
            }
            Event::Key(Key::Left, _) => {
                self.cur = self.cur.saturating_sub(1);
                Res::Handled
            }
            Event::Key(Key::Right, _) => {
                self.cur = (self.cur + 1).min(self.len());
                Res::Handled
            }
            Event::Key(Key::Home, _) | Event::Key(Key::Char('a'), crate::input::Mods { ctrl: true, .. }) => {
                self.cur = 0;
                Res::Handled
            }
            Event::Key(Key::End, _) | Event::Key(Key::Char('e'), crate::input::Mods { ctrl: true, .. }) => {
                self.cur = self.len();
                Res::Handled
            }
            Event::Key(Key::Char('u'), crate::input::Mods { ctrl: true, .. }) => {
                let b = self.byte(self.cur);
                self.text.replace_range(..b, "");
                self.cur = 0;
                Res::Changed
            }
            Event::Key(Key::Char('k'), crate::input::Mods { ctrl: true, .. }) => {
                let b = self.byte(self.cur);
                self.text.truncate(b);
                Res::Changed
            }
            Event::Key(Key::Char('w'), crate::input::Mods { ctrl: true, .. }) => {
                let chars: Vec<char> = self.text.chars().collect();
                let mut i = self.cur;
                while i > 0 && chars[i - 1] == ' ' {
                    i -= 1;
                }
                while i > 0 && chars[i - 1] != ' ' {
                    i -= 1;
                }
                let (a, b) = (self.byte(i), self.byte(self.cur));
                self.text.replace_range(a..b, "");
                self.cur = i;
                Res::Changed
            }
            Event::Paste(s) => {
                let one: String = s.chars().filter(|c| *c != '\n' && *c != '\r').collect();
                let b = self.byte(self.cur);
                self.text.insert_str(b, &one);
                self.cur += one.chars().count();
                Res::Changed
            }
            Event::Key(Key::Enter, _) => Res::Activate,
            Event::Mouse(MouseKind::Down(0), x, y, _) if r.contains(*x, *y) => {
                let mut col = *x - r.x - prompt_w;
                let mut i = self.scroll;
                for ch in self.text.chars().skip(self.scroll) {
                    let w = char_width(ch) as i32;
                    if col < w {
                        break;
                    }
                    col -= w;
                    i += 1;
                }
                self.cur = i.min(self.len());
                Res::Handled
            }
            _ => Res::Ignored,
        }
    }
}

// ---- button bar (dmenu strip) -------------------------------------------------------------------------

#[derive(Clone, Default, Debug)]
pub struct Buttons {
    pub labels: Vec<String>,
    pub cursor: usize,
    pub active: Option<usize>,
}

impl Buttons {
    pub fn new(labels: Vec<String>) -> Buttons {
        Buttons { labels, cursor: 0, active: None }
    }
    /// (x0, x1) column span of each label within `r`; labels that do not fit get an empty span.
    fn spans(&self, r: Rect, prompt_w: i32) -> Vec<(i32, i32)> {
        let mut x = r.x + prompt_w;
        let mut out = Vec::new();
        for l in &self.labels {
            let w = str_width(l) as i32 + 2;
            if x + w > r.x + r.w {
                out.push((0, 0));
            } else {
                out.push((x, x + w));
                x += w;
            }
        }
        out
    }
    pub fn draw(&self, c: &mut Canvas, r: Rect, prompt: &str, t: &Theme, focused: bool) {
        if r.is_empty() {
            return;
        }
        let old = c.set_clip(r);
        c.fill(Rect::new(r.x, r.y, r.w, 1), ' ', t.base());
        let pw = c.text(r.x, r.y, prompt, t.dim_style());
        for (i, (x0, x1)) in self.spans(r, pw).into_iter().enumerate() {
            if x1 == 0 {
                continue;
            }
            let st = if i == self.cursor && focused {
                t.selected(true)
            } else if Some(i) == self.active {
                Style::fg(t.accent).on(t.bg).with(BOLD)
            } else {
                t.base()
            };
            c.fill(Rect::new(x0, r.y, x1 - x0, 1), ' ', st);
            c.text(x0 + 1, r.y, &self.labels[i], st);
        }
        c.set_clip(old);
    }
    pub fn event(&mut self, r: Rect, prompt_w: i32, ev: &Event) -> Res {
        let n = self.labels.len();
        if n == 0 {
            return Res::Ignored;
        }
        let before = self.cursor;
        match ev {
            Event::Key(Key::Left, _) | Event::Key(Key::Up, _) => {
                self.cursor = self.cursor.saturating_sub(1);
            }
            Event::Key(Key::Right, _) | Event::Key(Key::Down, _) => {
                self.cursor = (self.cursor + 1).min(n - 1);
            }
            Event::Key(Key::Home, _) => self.cursor = 0,
            Event::Key(Key::End, _) => self.cursor = n - 1,
            Event::Key(Key::Enter, _) | Event::Key(Key::Char(' '), _) => return Res::Activate,
            Event::Mouse(MouseKind::Down(0), x, y, _) if r.contains(*x, *y) => {
                for (i, (x0, x1)) in self.spans(r, prompt_w).into_iter().enumerate() {
                    if x1 > 0 && *x >= x0 && *x < x1 {
                        self.cursor = i;
                        return Res::Activate;
                    }
                }
                return Res::Handled;
            }
            _ => return Res::Ignored,
        }
        if self.cursor != before {
            Res::Changed
        } else {
            Res::Handled
        }
    }
}

// ---- key bar -----------------------------------------------------------------------------------------

/// `key` is a name as `input::parse_key` knows it (`ctrl-r`); each modifier is its own coloured cap.
pub fn draw_keybar(c: &mut Canvas, r: Rect, hints: &[(String, String)], t: &Theme) {
    if r.is_empty() {
        return;
    }
    let old = c.set_clip(r);
    c.fill(Rect::new(r.x, r.y, r.w, 1), ' ', t.base());
    let mut x = r.x;
    let cap = |bg: Color| Style::new(t.sel_fg, bg).with(BOLD);
    for (key, label) in hints {
        let mut parts: Vec<&str> = key.split('-').collect();
        let last = parts.pop().unwrap_or("");
        let need: usize = parts.iter().map(|p| p.len() + 1).sum::<usize>() + last.len() + 1 + str_width(label) + 2;
        if x + need as i32 > r.x + r.w {
            break;
        }
        for p in parts {
            let bg = match p {
                "ctrl" => Color::Rgb(90, 140, 230),
                "alt" => t.warn,
                _ => t.ok,
            };
            x += c.text(x, r.y, &format!("{p} "), cap(bg));
        }
        x += c.text(x, r.y, &format!("{last} "), cap(t.accent));
        x += c.text(x + 1, r.y, label, t.base()) + 1;
        x += 2;
    }
    c.set_clip(old);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::Caps;
    use crate::input::Mods;

    #[test]
    fn a_lazy_list_keeps_a_bounded_cache_and_drops_the_far_end() {
        let mut l = List::lazy(100_000);
        l.put_rows(0, (0..LAZY_KEEP + 100).map(|i| i.to_string()).collect());
        assert_eq!(l.shown_items().len(), LAZY_KEEP, "over the limit, rows are dropped");
        l.top = 0;
        assert_eq!(l.row(0), Some("0"), "rows near the window stay");
        assert_eq!(l.row(LAZY_KEEP + 99), None, "the far end went first");
        l.set_total(10);
        assert_eq!((l.len(), l.shown_items().len()), (10, 10));
        l.set_filter("zzz");
        assert_eq!(l.len(), 10, "a lazy list is filtered by its source, not here");
    }

    fn cv(w: i32, h: i32) -> Canvas {
        Canvas::new(w, h, Caps::ASCII)
    }

    #[test]
    fn progress_snapshot() {
        let mut c = cv(10, 1);
        draw_progress(&mut c, Rect::new(0, 0, 10, 1), 0.5, "50%", &Theme::green());
        assert_eq!(c.to_text(), "###50%....\n");
    }

    #[test]
    fn list_cursor_scroll_filter_and_mouse() {
        let t = Theme::green();
        let mut l = List::new((0..20).map(|i| format!("row{i}")).collect());
        let r = Rect::new(0, 0, 12, 4);
        for _ in 0..6 {
            assert!(matches!(l.event(r, &Event::key(Key::Down)), Res::Changed));
        }
        let mut c = cv(12, 4);
        l.draw(&mut c, r, &t, true);
        assert!(c.to_text().contains("> row6"), "{}", c.to_text());
        assert_eq!(l.top, 3);
        l.set_filter("row1");
        assert_eq!(l.len(), 11);
        assert_eq!(l.selected_text(), Some("row1"));
        // click moves, second click activates
        l.set_filter("");
        assert_eq!(l.event(r, &Event::Mouse(MouseKind::Down(0), 2, 1, Mods::NONE)), Res::Changed);
        assert_eq!(l.event(r, &Event::Mouse(MouseKind::Down(0), 2, 1, Mods::NONE)), Res::Activate);
        assert_eq!(l.event(r, &Event::key(Key::Enter)), Res::Activate);
    }

    #[test]
    fn input_editing_keys() {
        let mut i = Input::new("");
        let r = Rect::new(0, 0, 20, 1);
        for ch in "héllo wörld".chars() {
            i.event(r, 0, &Event::ch(ch));
        }
        assert_eq!(i.text, "héllo wörld");
        assert_eq!(i.event(r, 0, &Event::Key(Key::Char('w'), Mods::CTRL)), Res::Changed);
        assert_eq!(i.text, "héllo ");
        i.event(r, 0, &Event::key(Key::Left));
        i.event(r, 0, &Event::key(Key::Backspace));
        assert_eq!(i.text, "héll ");
        i.event(r, 0, &Event::Paste("a\nb".into()));
        assert_eq!(i.text, "héllab ");
        i.event(r, 0, &Event::Key(Key::Char('u'), Mods::CTRL));
        assert_eq!(i.text, " ");
        assert_eq!(i.event(r, 0, &Event::key(Key::Enter)), Res::Activate);
    }

    #[test]
    fn buttons_click_and_keys() {
        let mut b = Buttons::new(vec!["main".into(), "extras".into()]);
        let r = Rect::new(0, 0, 30, 1);
        // "main" spans x 0..6, "extras" 6..14
        assert_eq!(b.event(r, 0, &Event::Mouse(MouseKind::Down(0), 8, 0, Mods::NONE)), Res::Activate);
        assert_eq!(b.cursor, 1);
        assert_eq!(b.event(r, 0, &Event::key(Key::Left)), Res::Changed);
        assert_eq!(b.cursor, 0);
        let mut c = cv(30, 1);
        b.active = Some(1);
        b.draw(&mut c, r, "", &Theme::green(), true);
        assert_eq!(c.to_text(), " main  extras\n");
    }

    #[test]
    fn keybar_draws_hints_that_fit() {
        let mut c = cv(24, 1);
        draw_keybar(&mut c, Rect::new(0, 0, 24, 1), &[("ctrl-r".into(), "run".into()), ("f1".into(), "help".into())], &Theme::green());
        assert_eq!(c.to_text(), "ctrl r  run  f1  help\n".replace("r  run  f1  help", "r  run  f1  help").as_str());
    }

    #[test]
    fn nonsense_rects_never_panic() {
        let t = Theme::green();
        let mut c = cv(10, 4);
        let weird = [Rect::new(-5, -5, 100, 100), Rect::new(3, 3, -2, 5), Rect::new(50, 50, 4, 4), Rect::default()];
        let mut l = List::new(vec!["a".into(), "b".into()]);
        let mut i = Input::new("x");
        let b = Buttons::new(vec!["q".into()]);
        for r in weird {
            draw_panel(&mut c, r, "t", true, &t);
            draw_text(&mut c, r, "hello\nworld", t.base(), Align::Center);
            draw_progress(&mut c, r, f64::NAN, "x", &t);
            l.draw(&mut c, r, &t, true);
            i.draw(&mut c, r, "> ", &t, true);
            b.draw(&mut c, r, "p", &t, true);
            draw_keybar(&mut c, r, &[("f1".into(), "x".into())], &t);
            l.event(r, &Event::Mouse(MouseKind::Down(0), 1, 1, Mods::NONE));
        }
    }
}
