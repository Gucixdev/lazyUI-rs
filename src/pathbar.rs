//! `pathbar`: a one-line path editor with Tab completion, the address bar of a file manager. `bind=KEY` holds the text.
//! Tab completes the last path component: one candidate fills it in, several extend the text to their common prefix
//! and open a popup (Up/Down, Tab or Enter picks, Esc closes). Enter without a popup emits `activate` with the path,
//! every edit emits `change`. `~/` is read as the home directory but kept as typed. `dirs=1` offers directories only.
//!
//! `source=fs` (default) lists the directory itself on Tab: one `read_dir`, capped at 5000 entries, on the UI thread
//! because the user asked for it with a key press. `source=host` is for virtual file systems (archives, remote
//! machines): Tab emits `complete` with the text and the host answers with the model key named by `options=KEY`, one
//! candidate per line, each the full text the bar should become.

use crate::canvas::{str_width, Canvas, Rect, Style};
use crate::input::{Event, Key, MouseKind};
use crate::native::{Cx, Def, EvCx, Widget};
use crate::ui::Value;
use crate::widgets::Input;
use crate::widgets::Res;

pub static DEFS: &[Def] = &[Def {
    name: "pathbar",
    doc: "path editor with Tab completion: bind=KEY prompt dirs=1 source=fs|host options=KEY; change, activate (Enter), complete (host source) events",
    focus: true,
    rows: Some(1),
    live: 0.0,
    make: || Box::new(PathBar::default()),
}];

const MAX_ENTRIES: usize = 5000;
const MAX_ROWS: usize = 10;

/// What the bar's text could become: the directory part of `text` is listed, entries starting with the last component
/// are returned as full texts (directories end in `/`), directories first. Hidden entries only when asked for with a dot.
pub fn complete_fs(text: &str, dirs_only: bool, home: &str) -> Vec<String> {
    let (head, base) = match text.rfind('/') {
        Some(i) => (&text[..=i], &text[i + 1..]),
        None => ("", text),
    };
    let dir = if head.is_empty() {
        ".".to_string()
    } else if let Some(rest) = head.strip_prefix("~/") {
        format!("{}/{rest}", home.trim_end_matches('/'))
    } else {
        head.to_string()
    };
    let Ok(rd) = std::fs::read_dir(&dir) else { return Vec::new() };
    let mut dirs = Vec::new();
    let mut files = Vec::new();
    for e in rd.flatten().take(MAX_ENTRIES) {
        let name = e.file_name().to_string_lossy().into_owned();
        if !name.starts_with(base) || (name.starts_with('.') && !base.starts_with('.')) {
            continue;
        }
        let is_dir = e.metadata().map(|m| m.is_dir()).unwrap_or(false) || e.path().is_dir();
        if is_dir {
            dirs.push(format!("{head}{name}/"));
        } else if !dirs_only {
            files.push(format!("{head}{name}"));
        }
    }
    dirs.sort();
    files.sort();
    dirs.extend(files);
    dirs
}

/// What the popup shows of a candidate: the last component, with its closing `/` for a directory.
fn last_part(s: &str) -> &str {
    let t = s.strip_suffix('/').unwrap_or(s);
    match t.rfind('/') {
        Some(i) => &s[i + 1..],
        None => s,
    }
}

/// The longest text all candidates start with.
pub fn common_prefix(c: &[String]) -> String {
    let Some(first) = c.first() else { return String::new() };
    let mut n = first.len();
    for s in &c[1..] {
        n = n.min(first.bytes().zip(s.bytes()).take_while(|(a, b)| a == b).count());
    }
    while !first.is_char_boundary(n) {
        n -= 1;
    }
    first[..n].to_string()
}

#[derive(Default)]
pub struct PathBar {
    input: Input,
    cands: Vec<String>,
    open: bool,
    hi: usize,
    popup: Rect,
    /// a `complete` event is out and the host's answer is awaited
    pending: bool,
    dirs_only: bool,
    host: bool,
}

impl PathBar {
    fn close(&mut self) {
        self.open = false;
        self.cands.clear();
        self.popup = Rect::new(0, 0, 0, 0);
    }

    fn changed(&mut self, cx: &mut EvCx) {
        cx.set_bound(Value::Text(self.input.text.clone()));
        cx.emit("change", &self.input.text, -1, 0.0);
    }

    fn set_text(&mut self, s: &str, cx: &mut EvCx) {
        self.input.text = s.to_string();
        self.input.cur = s.chars().count();
        self.changed(cx);
    }

    fn pick(&mut self, i: usize, cx: &mut EvCx) -> Res {
        if let Some(s) = self.cands.get(i).cloned() {
            self.close();
            self.set_text(&s, cx);
        }
        Res::Changed
    }

    fn tab(&mut self, cx: &mut EvCx) -> Res {
        if self.host {
            self.pending = true;
            cx.emit("complete", &self.input.text, -1, 0.0);
            return Res::Handled;
        }
        let home = std::env::var("HOME").unwrap_or_default();
        let c = complete_fs(&self.input.text, self.dirs_only, &home);
        match c.len() {
            0 => Res::Handled,
            1 => {
                let s = c[0].clone();
                self.set_text(&s, cx);
                Res::Changed
            }
            _ => {
                let p = common_prefix(&c);
                if p.chars().count() > self.input.text.chars().count() {
                    self.set_text(&p, cx);
                }
                self.cands = c;
                self.open = true;
                self.hi = 0;
                Res::Changed
            }
        }
    }

    fn row_at(&self, x: i32, y: i32) -> Option<usize> {
        self.popup.contains(x, y).then(|| (y - self.popup.y) as usize + self.first_row())
    }

    fn first_row(&self) -> usize {
        (self.hi + 1).saturating_sub(MAX_ROWS)
    }
}

impl Widget for PathBar {
    fn draw(&mut self, c: &mut Canvas, r: Rect, cx: &Cx) {
        self.dirs_only = cx.num_attr("dirs", 0.0) != 0.0;
        self.host = cx.attr("source") == Some("host");
        if let Some(t) = cx.attr("bind").and_then(|k| cx.model_text(k)).filter(|t| *t != self.input.text) {
            self.input.cur = t.chars().count();
            self.input.text = t;
        }
        if self.pending {
            if let Some(o) = cx.attr("options").and_then(|k| cx.model_text(k)) {
                let c: Vec<String> = o.lines().filter(|l| !l.is_empty()).map(str::to_string).collect();
                if !c.is_empty() {
                    self.cands = c;
                    self.open = true;
                    self.hi = 0;
                }
                self.pending = false;
            }
        }
        if !cx.focused {
            self.close();
        }
        let prompt = cx.text_attr("prompt", "");
        self.input.draw(c, r, &prompt, cx.theme, cx.focused);
    }

    fn overlay(&mut self, c: &mut Canvas, r: Rect, cx: &Cx) {
        if !self.open || self.cands.is_empty() {
            self.popup = Rect::new(0, 0, 0, 0);
            return;
        }
        let t = cx.theme;
        let rows = self.cands.len().min(MAX_ROWS) as i32;
        let w = (self.cands.iter().map(|s| str_width(last_part(s)) as i32).max().unwrap_or(0) + 2).max(r.w.min(c.w)).min(c.w);
        let x = r.x.min(c.w - w).max(0);
        let y = if r.y + 1 + rows <= c.h { r.y + 1 } else { (r.y - rows).max(0) };
        self.popup = Rect::new(x, y, w, rows.min(c.h));
        let base = Style::fg(t.fg).on(crate::gfx::mix(t.bg, t.dim, 0.25));
        c.fill(self.popup, ' ', base);
        let first = self.first_row();
        for (row, item) in self.cands.iter().skip(first).take(rows as usize).enumerate() {
            let st = if first + row == self.hi { t.selected(true) } else { base };
            c.fill(Rect::new(x, y + row as i32, w, 1), ' ', st);
            c.text_fit(x + 1, y + row as i32, w - 2, last_part(item), st);
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
        if self.open {
            match ev {
                Event::Key(Key::Up, _) => {
                    self.hi = self.hi.saturating_sub(1);
                    return Res::Handled;
                }
                Event::Key(Key::Down, _) => {
                    self.hi = (self.hi + 1).min(self.cands.len() - 1);
                    return Res::Handled;
                }
                Event::Key(Key::Tab | Key::Enter, _) => {
                    let i = self.hi;
                    return self.pick(i, cx);
                }
                Event::Key(Key::Esc, _) => {
                    self.close();
                    return Res::Handled;
                }
                Event::Mouse(MouseKind::Down(0), x, y, _) => {
                    match self.row_at(*x, *y) {
                        Some(i) => return self.pick(i, cx),
                        None => self.close(),
                    }
                    if !r.contains(*x, *y) {
                        return Res::Handled;
                    }
                }
                Event::Mouse(MouseKind::Move, x, y, _) => {
                    if let Some(i) = self.row_at(*x, *y) {
                        self.hi = i.min(self.cands.len() - 1);
                    }
                    return Res::Handled;
                }
                _ => self.close(),
            }
        }
        match ev {
            Event::Key(Key::Tab, _) => self.tab(cx),
            Event::Key(Key::Enter, _) => {
                cx.emit("activate", &self.input.text, -1, 0.0);
                Res::Activate
            }
            _ => {
                let res = self.input.event(r, 0, ev);
                if res == Res::Changed {
                    self.changed(cx);
                }
                res
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::headless::{run_script, snapshot};
    use crate::ui::Ui;

    fn tree(name: &str) -> String {
        let d = std::env::temp_dir().join(format!("lazyui-pathbar-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        for sub in ["music/rock", "music/jazz", "docs", ".hidden"] {
            std::fs::create_dir_all(d.join(sub)).unwrap();
        }
        for f in ["music/readme.txt", "music/rock.m3u", "notes.txt"] {
            std::fs::write(d.join(f), "x").unwrap();
        }
        format!("{}/", d.display())
    }

    fn gone(d: &str) {
        let _ = std::fs::remove_dir_all(d.trim_end_matches('/'));
    }

    #[test]
    fn fs_candidates_are_directories_first_hidden_only_on_request() {
        let d = tree("fs");
        assert_eq!(complete_fs(&d, false, ""), vec![format!("{d}docs/"), format!("{d}music/"), format!("{d}notes.txt")]);
        assert_eq!(complete_fs(&format!("{d}m"), false, ""), vec![format!("{d}music/")]);
        assert_eq!(complete_fs(&format!("{d}music/r"), false, ""), vec![format!("{d}music/rock/"), format!("{d}music/readme.txt"), format!("{d}music/rock.m3u")]);
        assert_eq!(complete_fs(&format!("{d}music/r"), true, ""), vec![format!("{d}music/rock/")], "dirs only");
        assert_eq!(complete_fs(&format!("{d}."), false, ""), vec![format!("{d}.hidden/")]);
        assert!(complete_fs(&format!("{d}nope/x"), false, "").is_empty());
        gone(&d);
    }

    #[test]
    fn tilde_is_read_as_home_but_kept_as_typed() {
        let d = tree("home");
        let home = d.trim_end_matches('/').to_string();
        assert_eq!(complete_fs("~/mu", false, &home), vec!["~/music/".to_string()]);
        gone(&d);
    }

    #[test]
    fn common_prefix_is_char_safe() {
        let v = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(common_prefix(&v(&["/a/rock/", "/a/readme"])), "/a/r");
        assert_eq!(common_prefix(&v(&["żaba", "żółw"])), "ż");
        assert_eq!(common_prefix(&v(&["x"])), "x");
        assert_eq!(common_prefix(&[]), "");
    }

    #[test]
    fn tab_fills_in_one_candidate_and_extends_then_lists_several() {
        let d = tree("tab");
        let mut ui = Ui::from_spec("col\n  pathbar id=p bind=path\n  spacer\n").unwrap();
        run_script(&mut ui, &format!("size 40 8\nfocus p\nset path={d}no\ntype xx\nset path={d}mu\nkey tab\nexpect model path {d}music/")).unwrap();
        run_script(&mut ui, "size 40 8\nkey tab").unwrap();
        let s = snapshot(&mut ui, 40, 8);
        assert!(s.contains("rock/") && s.contains("jazz/") && s.contains("readme.txt"), "popup lists the entries:\n{s}");
        run_script(&mut ui, "size 40 8\nkey down\nkey enter").unwrap();
        assert!(ui.take_events().iter().any(|e| e.kind == "change" && e.text.ends_with("music/rock/")));
        gone(&d);
    }

    #[test]
    fn typing_enter_and_escape() {
        let d = tree("keys");
        let mut ui = Ui::from_spec("col\n  pathbar id=p bind=path prompt=\"> \"\n  spacer\n").unwrap();
        run_script(&mut ui, &format!("size 40 8\nfocus p\ntype /tmp\nexpect model path /tmp\nkey enter\nexpect event p activate /tmp\nset path={d}music/r\nkey tab\nkey esc\nkey backspace\nexpect model path {d}music/")).unwrap();
        gone(&d);
    }

    #[test]
    fn host_source_asks_and_shows_what_the_host_answers() {
        let mut ui = Ui::from_spec("col\n  pathbar id=p bind=path source=host options=cand\n  spacer\n").unwrap();
        run_script(&mut ui, "size 40 8\nfocus p\nset path=zip:/a.zip!/d\nkey tab\nexpect event p complete zip:/a.zip!/d").unwrap();
        ui.set_text("cand", "zip:/a.zip!/docs/\nzip:/a.zip!/data/");
        let s = snapshot(&mut ui, 40, 8);
        assert!(s.contains("docs/") && s.contains("data/"), "{s}");
        run_script(&mut ui, "size 40 8\nkey enter\nexpect model path zip:/a.zip!/docs/").unwrap();
    }
}
