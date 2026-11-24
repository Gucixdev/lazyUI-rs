//! The retained UI: an element tree (built from a spec or in code), a key/value model, per-id widget state, focus and named
//! events `(id, kind)`. No terminal in here: `render` draws into a `Canvas`, `dispatch` takes an `Event`.

use crate::anim::Spring;
use crate::canvas::{Canvas, Rect, Style};
use crate::input::{key_name, parse_key, Event, Key, Mods, MouseKind};
use crate::layout::{split, Dir, Size};
use crate::theme::Theme;
use crate::widgets::{draw_keybar, draw_panel, draw_progress, draw_text, Align, Buttons, Input, List, Res};
use std::collections::HashMap;

// ---- the tree -------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// a native widget from `native::find` (charts, pickers...), by its registered name
    Custom(&'static str),
    Row,
    Col,
    Panel,
    Pages,
    Spacer,
    Text,
    Progress,
    Search,
    Buttons,
    List,
    Keybar,
}

impl Kind {
    pub fn parse(s: &str) -> Option<Kind> {
        Some(match s {
            "row" => Kind::Row,
            "col" => Kind::Col,
            "panel" => Kind::Panel,
            "pages" => Kind::Pages,
            "spacer" => Kind::Spacer,
            "text" | "textbar" => Kind::Text,
            "progressbar" => Kind::Progress,
            "searchbar" => Kind::Search,
            "buttonbar" => Kind::Buttons,
            "list" => Kind::List,
            "keybar" => Kind::Keybar,
            other => return crate::native::find(other).map(|d| Kind::Custom(d.name)),
        })
    }
    pub fn name(self) -> &'static str {
        match self {
            Kind::Row => "row",
            Kind::Col => "col",
            Kind::Panel => "panel",
            Kind::Pages => "pages",
            Kind::Spacer => "spacer",
            Kind::Text => "text",
            Kind::Progress => "progressbar",
            Kind::Search => "searchbar",
            Kind::Buttons => "buttonbar",
            Kind::List => "list",
            Kind::Keybar => "keybar",
            Kind::Custom(n) => n,
        }
    }
    fn container(self) -> bool {
        matches!(self, Kind::Row | Kind::Col | Kind::Panel | Kind::Pages)
    }
    fn focusable(self) -> bool {
        match self {
            Kind::Custom(n) => crate::native::find(n).is_some_and(|d| d.focus),
            _ => matches!(self, Kind::Search | Kind::Buttons | Kind::List),
        }
    }
    fn one_row(self) -> bool {
        match self {
            Kind::Custom(n) => crate::native::find(n).is_some_and(|d| d.rows == Some(1)),
            _ => matches!(self, Kind::Text | Kind::Progress | Kind::Search | Kind::Buttons | Kind::Keybar),
        }
    }
}

#[derive(Clone, Debug)]
pub struct El {
    pub kind: Kind,
    pub attrs: HashMap<String, String>,
    pub children: Vec<El>,
}

impl El {
    pub fn new(kind: Kind) -> El {
        El { kind, attrs: HashMap::new(), children: Vec::new() }
    }
    pub fn attr(&self, k: &str) -> Option<&str> {
        self.attrs.get(k).map(|s| s.as_str())
    }
    pub fn set(mut self, k: &str, v: impl ToString) -> El {
        self.attrs.insert(k.to_string(), v.to_string());
        self
    }
    pub fn id(self, id: &str) -> El {
        self.set("id", id)
    }
    pub fn size(self, s: &str) -> El {
        self.set("size", s)
    }
    pub fn child(mut self, c: El) -> El {
        self.children.push(c);
        self
    }
    pub fn children(mut self, cs: impl IntoIterator<Item = El>) -> El {
        self.children.extend(cs);
        self
    }
}

pub fn row(cs: impl IntoIterator<Item = El>) -> El {
    El::new(Kind::Row).children(cs)
}
pub fn col(cs: impl IntoIterator<Item = El>) -> El {
    El::new(Kind::Col).children(cs)
}
pub fn panel(title: &str, cs: impl IntoIterator<Item = El>) -> El {
    El::new(Kind::Panel).set("title", title).children(cs)
}
pub fn text(s: impl ToString) -> El {
    El::new(Kind::Text).set("text", s)
}
pub fn progress(value: f64) -> El {
    El::new(Kind::Progress).set("value", value)
}
pub fn search(id: &str, prompt: &str) -> El {
    El::new(Kind::Search).id(id).set("prompt", prompt)
}
pub fn buttons(id: &str, labels: &[&str]) -> El {
    El::new(Kind::Buttons).id(id).set("items", labels.join(","))
}
pub fn list(id: &str, items: &[&str]) -> El {
    El::new(Kind::List).id(id).set("items", items.join(","))
}
pub fn keybar(hints: &str) -> El {
    El::new(Kind::Keybar).set("hints", hints)
}
pub fn spacer() -> El {
    El::new(Kind::Spacer)
}

// ---- the spec --------------------------------------------------------------------------------------

/// Indented text, one element per line: `kind attr=value attr="quoted value"`. Errors carry the line number.
pub fn parse_spec(src: &str) -> Result<El, String> {
    parse_lines(crate::components::plain_lines(src), None)
}

/// `parse_spec` with components: every use of a named widget in `widgets` is expanded first.
pub fn parse_spec_with(src: &str, widgets: &crate::components::Widgets) -> Result<El, String> {
    let lines = crate::components::plain_lines(src);
    parse_lines(widgets.expand(lines)?, Some(widgets))
}

fn parse_lines(lines: Vec<crate::components::SrcLine>, _w: Option<&crate::components::Widgets>) -> Result<El, String> {
    let mut stack: Vec<(usize, El)> = Vec::new(); // (indent, element being built)
    let mut root: Option<El> = None;
    for sl in lines.iter() {
        let n = sl.label.as_str();
        let body = sl.text.trim_end();
        let trimmed = body.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if body.contains('\t') && body[..body.len() - trimmed.len()].contains('\t') {
            return Err(format!("{n}: tabs in the indent, use spaces"));
        }
        let indent = body.len() - trimmed.len();
        let toks = tokenize(trimmed).map_err(|e| format!("{n}: {e}"))?;
        let (kind_s, rest) = toks.split_first().ok_or_else(|| format!("{n}: empty element"))?;
        if kind_s.contains('=') {
            return Err(format!("{n}: expected an element kind first, got `{kind_s}`"));
        }
        let kind = Kind::parse(kind_s).ok_or_else(|| {
            let known = ["scroll", "custom", "stack", "filepicker", "sequencer", "nodeeditor", "pixeleditor"];
            if known.contains(&kind_s.as_str()) {
                format!("{n}: `{kind_s}` exists in lazytui (C) but not in lazyui yet")
            } else {
                format!("{n}: unknown element `{kind_s}`")
            }
        })?;
        let mut el = El::new(kind);
        for t in rest {
            let (k, v) = t.split_once('=').ok_or_else(|| format!("{n}: `{t}` is not key=value"))?;
            if k.is_empty() {
                return Err(format!("{n}: empty attribute name"));
            }
            if k == "size" && Size::parse(v).is_none() {
                return Err(format!("{n}: size `{v}` is not N, fill, fill:W, pct:P, min:N or max:N"));
            }
            el.attrs.insert(k.to_string(), v.to_string());
        }
        // close everything at the same or deeper indent
        while let Some((i, _)) = stack.last() {
            if *i >= indent {
                let (_, done) = stack.pop().unwrap();
                attach(&mut stack, &mut root, done, 0)?;
            } else {
                break;
            }
        }
        if let Some((_, parent)) = stack.last() {
            if !parent.kind.container() {
                return Err(format!("{n}: `{}` cannot hold children", parent.kind.name()));
            }
        } else if root.is_some() {
            return Err(format!("{n}: a second top-level element (a spec has one root)"));
        } else if indent != 0 {
            return Err(format!("{n}: the root must not be indented"));
        }
        stack.push((indent, el));
    }
    while let Some((_, done)) = stack.pop() {
        attach(&mut stack, &mut root, done, 0)?;
    }
    root.ok_or_else(|| "the spec is empty".to_string())
}

fn attach(stack: &mut [(usize, El)], root: &mut Option<El>, done: El, _line: usize) -> Result<(), String> {
    if let Some((_, parent)) = stack.last_mut() {
        parent.children.push(done);
    } else {
        *root = Some(done);
    }
    Ok(())
}

pub(crate) fn tokenize(s: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut in_token = false;
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '"' => {
                quoted = !quoted;
                in_token = true;
            }
            '\\' if quoted => match it.next() {
                Some('n') => cur.push('\n'),
                Some(o) => cur.push(o),
                None => return Err("a backslash at the end of the line".into()),
            },
            ' ' if !quoted => {
                if in_token {
                    out.push(std::mem::take(&mut cur));
                    in_token = false;
                }
            }
            _ => {
                cur.push(c);
                in_token = true;
            }
        }
    }
    if quoted {
        return Err("a quote is not closed".into());
    }
    if in_token {
        out.push(cur);
    }
    Ok(out)
}

// ---- model, events -----------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Num(f64),
    Text(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct UiEvent {
    pub id: String,
    pub kind: String,
    pub text: String,
    pub index: i64,
    pub number: f64,
}

impl UiEvent {
    pub(crate) fn new(id: &str, kind: &str) -> UiEvent {
        UiEvent { id: id.into(), kind: kind.into(), text: String::new(), index: -1, number: 0.0 }
    }
    /// An event made by the program itself (an effect result, a test): `UiEvent::of("loaded", "done", "42")`.
    pub fn of(id: &str, kind: &str, text: &str) -> UiEvent {
        let mut e = UiEvent::new(id, kind);
        e.text = text.into();
        e
    }
    /// Is this the event `(id, kind)`?
    pub fn is(&self, id: &str, kind: &str) -> bool {
        self.id == id && self.kind == kind
    }
    /// A button or list row was activated (Enter, click) on element `id`.
    pub fn activated(&self, id: &str) -> bool {
        self.is(id, "activate")
    }
    /// The part of the id after `prefix.`: for events of a component placed with `.scope(prefix)`.
    pub fn local(&self, prefix: &str) -> Option<&str> {
        self.id.strip_prefix(prefix)?.strip_prefix('.')
    }
    pub fn to_json(&self) -> String {
        format!("{{\"id\":{},\"kind\":{},\"text\":{},\"index\":{},\"number\":{}}}", jstr(&self.id), jstr(&self.kind), jstr(&self.text), self.index, jnum(self.number))
    }
}

pub fn jstr(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('"');
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

fn jnum(n: f64) -> String {
    if n.is_finite() {
        format!("{n}")
    } else {
        "null".into()
    }
}

enum WState {
    List(List, String),
    Input(Input),
    Buttons(Buttons, String),
    Custom(Box<dyn crate::native::Widget>),
}

#[derive(Clone, Debug)]
struct Placed {
    id: String,
    kind: Kind,
    rect: Rect,
    path: Vec<usize>,
    parent: Option<usize>,
}

pub struct Ui {
    root: El,
    widgets: crate::components::Widgets,
    pub theme: Theme,
    model: HashMap<String, Value>,
    states: HashMap<String, WState>,
    springs: HashMap<String, Spring>,
    focus: Option<String>,
    placed: Vec<Placed>,
    size: (i32, i32),
    events: Vec<UiEvent>,
    hotkeys: Vec<(Key, Mods, String)>,
    every: Vec<(f64, f64, String)>,
    clock: f64,
    pub quit: bool,
    pub dirty: bool,
}

impl Ui {
    pub fn new(root: El) -> Ui {
        Ui {
            root,
            widgets: crate::components::Widgets::builtin(),
            theme: Theme::green(),
            model: HashMap::new(),
            states: HashMap::new(),
            springs: HashMap::new(),
            focus: None,
            placed: Vec::new(),
            size: (0, 0),
            events: Vec::new(),
            hotkeys: Vec::new(),
            every: Vec::new(),
            clock: 0.0,
            quit: false,
            dirty: true,
        }
    }
    /// Parse with the built-in components (`card`, `meter`, ...).
    pub fn from_spec(src: &str) -> Result<Ui, String> {
        Ui::from_spec_with(src, crate::components::Widgets::builtin())
    }
    /// Parse with a chosen set of components (builtin + global + local, see `components`). Kept for `reload_spec`.
    pub fn from_spec_with(src: &str, widgets: crate::components::Widgets) -> Result<Ui, String> {
        let root = parse_spec_with(src, &widgets)?;
        let mut ui = Ui::new(root);
        ui.widgets = widgets;
        Ok(ui)
    }
    pub fn widgets(&self) -> &crate::components::Widgets {
        &self.widgets
    }
    /// Swap the tree (e.g. a new `view` result). State of elements with the same id (list cursor, input text) survives.
    pub fn set_tree(&mut self, root: El) {
        self.root = root;
        self.dirty = true;
    }
    /// Re-load a spec keeping model, state and focus; on an error nothing changes.
    pub fn reload_spec(&mut self, src: &str) -> Result<(), String> {
        let root = parse_spec_with(src, &self.widgets)?;
        self.set_tree(root);
        Ok(())
    }

    // model
    pub fn set(&mut self, key: &str, v: f64) {
        self.put(key, Value::Num(v));
    }
    pub fn set_text(&mut self, key: &str, v: &str) {
        self.put(key, Value::Text(v.to_string()));
    }
    fn put(&mut self, key: &str, v: Value) {
        if self.model.get(key) != Some(&v) {
            self.model.insert(key.to_string(), v);
            self.dirty = true;
        }
    }
    pub fn number(&self, key: &str) -> f64 {
        match self.model.get(key) {
            Some(Value::Num(n)) => *n,
            Some(Value::Text(t)) => t.trim().parse().unwrap_or(0.0),
            None => 0.0,
        }
    }
    pub fn text_of(&self, key: &str) -> String {
        match self.model.get(key) {
            Some(Value::Text(t)) => t.clone(),
            Some(Value::Num(n)) => fmt_num(*n),
            None => String::new(),
        }
    }
    /// Append a sample to a comma separated series in the model (a graph's data), keeping the newest `cap`.
    pub fn push(&mut self, key: &str, v: f64, cap: usize) {
        if !v.is_finite() {
            return;
        }
        let mut xs = crate::gfx::numbers(&self.text_of(key));
        xs.push(v);
        let from = xs.len().saturating_sub(cap.max(1));
        let t: Vec<String> = xs[from..].iter().map(|x| fmt_num(*x)).collect();
        self.set_text(key, &t.join(","));
    }
    pub fn has(&self, key: &str) -> bool {
        self.model.contains_key(key)
    }
    /// `key=value` from outside (stdin, `--set`): a number when it parses as one, text otherwise.
    pub fn set_auto(&mut self, key: &str, v: &str) {
        match v.trim().parse::<f64>() {
            Ok(n) if n.is_finite() => self.set(key, n),
            _ => self.set_text(key, v),
        }
    }

    // rows of a list
    pub fn set_list(&mut self, id: &str, items: Vec<String>) {
        match self.states.get_mut(id) {
            Some(WState::List(l, _)) => l.set_items(items),
            _ => {
                self.states.insert(id.to_string(), WState::List(List::new(items), String::new()));
            }
        }
        self.dirty = true;
    }
    pub fn list_selected(&self, id: &str) -> Option<usize> {
        match self.states.get(id) {
            Some(WState::List(l, _)) => l.selected(),
            _ => None,
        }
    }

    // wiring
    pub fn hotkey(&mut self, key: &str, event: &str) -> bool {
        match parse_key(key) {
            Some((k, m)) => {
                self.hotkeys.push((k, m, event.to_string()));
                true
            }
            None => false,
        }
    }
    pub fn every(&mut self, seconds: f64, event: &str) {
        if seconds.is_finite() && seconds > 0.0 {
            self.every.push((seconds, 0.0, event.to_string()));
        }
    }
    pub fn emit(&mut self, id: &str, kind: &str, text: &str, number: f64) {
        let mut e = UiEvent::new(id, kind);
        e.text = text.into();
        e.number = number;
        self.events.push(e);
    }
    pub fn take_events(&mut self) -> Vec<UiEvent> {
        std::mem::take(&mut self.events)
    }
    pub fn focused(&self) -> Option<&str> {
        self.focus.as_deref()
    }
    pub fn focus(&mut self, id: &str) -> bool {
        if self.placed.iter().any(|p| p.id == id && p.kind.focusable()) {
            self.focus = Some(id.to_string());
            self.dirty = true;
            true
        } else {
            false
        }
    }
    pub fn rect_of(&self, id: &str) -> Option<Rect> {
        self.placed.iter().find(|p| p.id == id).map(|p| p.rect)
    }
    pub fn size(&self) -> (i32, i32) {
        self.size
    }

    // time
    pub fn tick(&mut self, dt: f64) {
        if !dt.is_finite() || dt < 0.0 {
            return;
        }
        self.clock += dt;
        for s in self.springs.values_mut() {
            if !s.at_rest() {
                s.step(dt);
                self.dirty = true;
            }
        }
        let custom: Vec<(String, Vec<usize>)> = self.placed.iter().filter(|p| matches!(p.kind, Kind::Custom(_))).map(|p| (p.id.clone(), p.path.clone())).collect();
        for (id, path) in custom {
            let Some(el) = Ui::el_at(&self.root, &path).cloned() else { continue };
            if let Some(WState::Custom(w)) = self.states.get_mut(&id) {
                let mut cx = crate::native::EvCx { el: &el, id: &id, model: &self.model, out: Vec::new(), set: Vec::new() };
                w.tick(dt, &mut cx);
                let (out, set) = (cx.out, cx.set);
                self.events.extend(out);
                for (k, v) in set {
                    self.put(&k, v);
                }
            }
        }
        if self.live().is_some() {
            self.dirty = true;
        }
        let mut fired = Vec::new();
        for (period, acc, name) in &mut self.every {
            *acc += dt;
            while *acc >= *period {
                *acc -= *period;
                fired.push(name.clone());
            }
        }
        for name in fired {
            self.emit("app", &name, "", 0.0);
        }
    }
    /// Something is still moving towards a resting picture (springs, widgets easing): a snapshot waits for this to end.
    pub fn settling(&self) -> bool {
        self.springs.values().any(|s| !s.at_rest()) || self.states.values().any(|s| matches!(s, WState::Custom(w) if w.animating()))
    }
    /// Shortest frame period of the widgets on screen that change with time alone (a clock, a spinner), if any.
    pub fn live(&self) -> Option<f64> {
        self.placed
            .iter()
            .filter_map(|p| match p.kind {
                Kind::Custom(n) => crate::native::find(n).map(|d| d.live).filter(|l| *l > 0.0),
                _ => None,
            })
            .fold(None, |m: Option<f64>, x| Some(m.map_or(x, |y| y.min(x))))
    }
    /// The loop needs a frame every tick: something settling, or a live widget that moves faster than 10 per second.
    pub fn animating(&self) -> bool {
        self.settling() || self.live().is_some_and(|p| p <= 0.1)
    }
    /// Seconds until the next periodic event, if there is one.
    pub fn next_deadline(&self) -> Option<f64> {
        let live = self.live().filter(|p| *p > 0.1).map(|p| p - self.clock.rem_euclid(p));
        self.every.iter().map(|(p, a, _)| (p - a).max(0.0)).chain(live).fold(None, |m: Option<f64>, x| Some(m.map_or(x, |y| y.min(x))))
    }

    // ---- layout -----------------------------------------------------------------------------------

    fn el_at<'a>(root: &'a El, path: &[usize]) -> Option<&'a El> {
        let mut e = root;
        for &i in path {
            e = e.children.get(i)?;
        }
        Some(e)
    }

    fn node_id(el: &El, kind: Kind, path: &[usize]) -> String {
        match el.attr("id") {
            Some(i) if !i.is_empty() => i.to_string(),
            _ => format!("{}@{}", kind.name(), path.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(".")),
        }
    }

    fn default_size(kind: Kind, in_col: bool) -> Size {
        if in_col && kind.one_row() {
            Size::Fixed(1)
        } else {
            Size::Fill(1)
        }
    }

    fn place(&self, el: &El, r: Rect, path: &mut Vec<usize>, parent: Option<usize>, out: &mut Vec<Placed>) {
        let me = out.len();
        out.push(Placed { id: Ui::node_id(el, el.kind, path), kind: el.kind, rect: r, path: path.clone(), parent });
        if !el.kind.container() {
            return;
        }
        let inner = if el.kind == Kind::Panel { r.inset(1) } else { r };
        let pad: i32 = el.attr("pad").and_then(|s| s.parse().ok()).unwrap_or(0);
        let inner = inner.inset(pad.max(0));
        let gap: i32 = el.attr("gap").and_then(|s| s.parse().ok()).unwrap_or(0);
        if el.kind == Kind::Pages {
            let which = match el.attr("bind") {
                Some(k) => self.number(k).max(0.0) as usize,
                None => el.attr("active").and_then(|s| s.parse().ok()).unwrap_or(0),
            };
            if let Some(c) = el.children.get(which) {
                path.push(which);
                self.place(c, inner, path, Some(me), out);
                path.pop();
            }
            return;
        }
        let dir = if el.kind == Kind::Row { Dir::Row } else { Dir::Col };
        let sizes: Vec<Size> = el
            .children
            .iter()
            .map(|c| c.attr("size").and_then(Size::parse).unwrap_or_else(|| Ui::default_size(c.kind, dir == Dir::Col)))
            .collect();
        let rects = split(inner, dir, &sizes, gap);
        for (i, (c, cr)) in el.children.iter().zip(rects).enumerate() {
            path.push(i);
            self.place(c, cr, path, Some(me), out);
            path.pop();
        }
    }

    pub fn relayout(&mut self, w: i32, h: i32) {
        let mut out = Vec::new();
        let mut path = Vec::new();
        self.place(&self.root, Rect::new(0, 0, w, h), &mut path, None, &mut out);
        self.placed = out;
        self.size = (w, h);
        // focus must point at something that exists
        let ok = self.focus.as_ref().map(|f| self.placed.iter().any(|p| &p.id == f && p.kind.focusable())).unwrap_or(false);
        if !ok {
            self.focus = self.placed.iter().find(|p| p.kind.focusable()).map(|p| p.id.clone());
        }
    }

    // ---- drawing ------------------------------------------------------------------------------------

    pub fn render(&mut self, c: &mut Canvas) {
        c.clear(self.theme.base());
        self.relayout(c.w, c.h);
        let focus_chain = self.focus_chain();
        let placed = self.placed.clone();
        let root = std::mem::replace(&mut self.root, El::new(Kind::Spacer));
        for (i, p) in placed.iter().enumerate() {
            let Some(el) = Ui::el_at(&root, &p.path) else { continue };
            self.draw_one(c, el, p, focus_chain.contains(&i));
        }
        for p in placed.iter().filter(|p| matches!(p.kind, Kind::Custom(_))) {
            let Some(el) = Ui::el_at(&root, &p.path) else { continue };
            let focused = self.focus.as_deref() == Some(p.id.as_str());
            if let Some(WState::Custom(w)) = self.states.get_mut(&p.id) {
                let cx = crate::native::Cx { el, theme: &self.theme, model: &self.model, focused, unicode: c.caps.unicode, time: self.clock };
                w.overlay(c, p.rect, &cx);
            }
        }
        self.root = root;
        self.dirty = false;
    }

    fn focus_chain(&self) -> Vec<usize> {
        let mut chain = Vec::new();
        let mut cur = self.focus.as_ref().and_then(|f| self.placed.iter().position(|p| &p.id == f));
        while let Some(i) = cur {
            chain.push(i);
            cur = self.placed[i].parent;
        }
        chain
    }

    fn bound_text(&self, el: &El, attr: &str) -> String {
        match el.attr("bind") {
            Some(k) if self.has(k) => self.text_of(k),
            _ => el.attr(attr).unwrap_or("").to_string(),
        }
    }

    fn draw_one(&mut self, c: &mut Canvas, el: &El, p: &Placed, in_focus: bool) {
        let t = self.theme.clone();
        let r = p.rect;
        match el.kind {
            Kind::Row | Kind::Col | Kind::Pages | Kind::Spacer => {}
            Kind::Panel => {
                draw_panel(c, r, el.attr("title").unwrap_or(""), in_focus, &t);
            }
            Kind::Text => {
                let st = match el.attr("style") {
                    Some("dim") => t.dim_style(),
                    Some("accent") => t.accent_style(),
                    Some("ok") => Style::fg(t.ok).on(t.bg),
                    Some("warn") => Style::fg(t.warn).on(t.bg),
                    Some("err") => Style::fg(t.err).on(t.bg),
                    _ => t.base(),
                };
                let al = match el.attr("align") {
                    Some("center") => Align::Center,
                    Some("right") => Align::Right,
                    _ => Align::Left,
                };
                let s = self.bound_text(el, "text");
                draw_text(c, r, &s, st, al);
            }
            Kind::Progress => {
                let max = el.attr("max").and_then(|s| s.parse::<f64>().ok()).filter(|m| *m > 0.0).unwrap_or(1.0);
                let raw = match el.attr("bind") {
                    Some(k) => self.number(k),
                    None => el.attr("value").and_then(|s| s.parse().ok()).unwrap_or(0.0),
                };
                let target = if raw.is_finite() { (raw / max).clamp(0.0, 1.0) } else { 0.0 };
                let sp = self.springs.entry(p.id.clone()).or_insert_with(|| Spring::smooth(target));
                sp.retarget(target);
                let shown = sp.x;
                let mut rr = r;
                if let Some(name) = el.attr("name") {
                    let w = (crate::canvas::str_width(name) as i32 + 1).min(r.w / 2);
                    c.text_fit(r.x, r.y, w, name, t.dim_style());
                    rr = Rect::new(r.x + w, r.y, r.w - w, r.h);
                }
                let label = match el.attr("label") {
                    Some(l) => l.to_string(),
                    None => format!("{}%", (target * 100.0).round() as i32),
                };
                draw_progress(c, rr, shown, &label, &t);
            }
            Kind::Search => {
                let prompt = el.attr("prompt").unwrap_or("> ").to_string();
                let key = el.attr("bind").map(|s| s.to_string());
                let init = key.as_ref().map(|k| self.text_of(k)).unwrap_or_default();
                let st = self.states.entry(p.id.clone()).or_insert_with(|| WState::Input(Input::new(&init)));
                if let WState::Input(inp) = st {
                    if key.is_some() {
                        inp.set(&init);
                    }
                    inp.draw(c, r, &prompt, &t, self.focus.as_deref() == Some(p.id.as_str()));
                }
            }
            Kind::Buttons => {
                let items = split_items(el.attr("items").unwrap_or(""));
                let active = el.attr("active").map(|k| self.number(k).max(0.0) as usize);
                let prompt = el.attr("prompt").unwrap_or("").to_string();
                let src = items.join("\u{1}");
                let st = self.states.entry(p.id.clone()).or_insert_with(|| WState::Buttons(Buttons::new(items.clone()), src.clone()));
                if let WState::Buttons(b, s) = st {
                    if *s != src {
                        b.labels = items;
                        b.cursor = b.cursor.min(b.labels.len().saturating_sub(1));
                        *s = src;
                    }
                    if active.is_some() {
                        b.active = active;
                    }
                    b.draw(c, r, &prompt, &t, self.focus.as_deref() == Some(p.id.as_str()));
                }
            }
            Kind::List => {
                let items = split_items(el.attr("items").unwrap_or(""));
                let src = items.join("\u{1}");
                let filter = el.attr("filter").map(|k| self.text_of(k));
                let focused = self.focus.as_deref() == Some(p.id.as_str());
                let st = self.states.entry(p.id.clone()).or_insert_with(|| WState::List(List::new(items.clone()), src.clone()));
                if let WState::List(l, s) = st {
                    // `items` in the spec only fills the list while the program has not set rows itself
                    if el.attr("items").is_some() && *s != src {
                        l.set_items(items);
                        *s = src;
                    }
                    if let Some(f) = filter {
                        l.set_filter(&f);
                    }
                    l.draw(c, r, &t, focused);
                }
            }
            Kind::Custom(name) => {
                let Some(def) = crate::native::find(name) else { return };
                let focused = self.focus.as_deref() == Some(p.id.as_str());
                let st = self.states.entry(p.id.clone()).or_insert_with(|| WState::Custom((def.make)()));
                if let WState::Custom(w) = st {
                    let cx = crate::native::Cx { el, theme: &t, model: &self.model, focused, unicode: c.caps.unicode, time: self.clock };
                    w.draw(c, r, &cx);
                }
            }
            Kind::Keybar => {
                let hints: Vec<(String, String)> = el
                    .attr("hints")
                    .unwrap_or("")
                    .split(';')
                    .filter(|s| !s.trim().is_empty())
                    .map(|h| match h.split_once(':') {
                        Some((k, l)) => (k.trim().to_string(), l.trim().to_string()),
                        None => (h.trim().to_string(), String::new()),
                    })
                    .collect();
                draw_keybar(c, r, &hints, &t);
            }
        }
    }

    // ---- events ----------------------------------------------------------------------------------------

    fn focused_kind(&self) -> Option<Kind> {
        let f = self.focus.as_ref()?;
        self.placed.iter().find(|p| &p.id == f).map(|p| p.kind)
    }

    fn cycle_focus(&mut self, back: bool) {
        let ids: Vec<String> = self.placed.iter().filter(|p| p.kind.focusable()).map(|p| p.id.clone()).collect();
        if ids.is_empty() {
            return;
        }
        let cur = self.focus.as_ref().and_then(|f| ids.iter().position(|i| i == f)).unwrap_or(0);
        let next = if back { (cur + ids.len() - 1) % ids.len() } else { (cur + 1) % ids.len() };
        self.focus = Some(ids[next].clone());
        self.dirty = true;
    }

    /// Feed one terminal event. True when something used it.
    pub fn dispatch(&mut self, ev: &Event) -> bool {
        match ev {
            Event::Resize(..) => {
                self.dirty = true;
                true
            }
            Event::Key(k, m) => {
                // hotkeys first: with a modifier always, plain characters only when no text field has the focus (F-keys always)
                let typing = self.focused_kind() == Some(Kind::Search);
                if let Some(name) = self.hotkeys.iter().find(|(hk, hm, _)| hk == k && hm == m && (m.any() || !typing || !matches!(k, Key::Char(_)))).map(|h| h.2.clone()) {
                    self.emit("app", &name, "", 0.0);
                    return true;
                }
                if let Some(id) = self.focus.clone() {
                    let res = self.widget_event(&id, ev);
                    if res != Res::Ignored {
                        return true;
                    }
                }
                match (k, m.ctrl || m.alt) {
                    (Key::Tab, false) => {
                        self.cycle_focus(false);
                        true
                    }
                    (Key::BackTab, _) => {
                        self.cycle_focus(true);
                        true
                    }
                    _ => {
                        let mut e = UiEvent::new("ui", "key");
                        e.text = key_name(*k, *m);
                        self.events.push(e);
                        false
                    }
                }
            }
            Event::Paste(_) => match self.focus.clone() {
                Some(id) => self.widget_event(&id, ev) != Res::Ignored,
                None => false,
            },
            Event::Mouse(kind, x, y, _) => {
                let hit = self.placed.iter().rev().find(|p| p.kind.focusable() && self.extent_of(p).contains(*x, *y)).map(|p| p.id.clone());
                match hit {
                    Some(id) => {
                        if matches!(kind, MouseKind::Down(0)) && self.focus.as_deref() != Some(id.as_str()) {
                            self.focus = Some(id.clone());
                            self.dirty = true;
                        }
                        self.widget_event(&id, ev) != Res::Ignored
                    }
                    None => {
                        if matches!(kind, MouseKind::Down(0)) {
                            if let Some(p) = self.placed.iter().rev().find(|p| p.rect.contains(*x, *y) && !p.id.contains('@')) {
                                let id = p.id.clone();
                                self.emit(&id, "click", "", 0.0);
                            }
                        }
                        false
                    }
                }
            }
        }
    }

    fn extent_of(&self, p: &Placed) -> Rect {
        match self.states.get(&p.id) {
            Some(WState::Custom(w)) => w.extent(p.rect),
            _ => p.rect,
        }
    }

    fn widget_event(&mut self, id: &str, ev: &Event) -> Res {
        let Some(p) = self.placed.iter().find(|p| p.id == id).cloned() else { return Res::Ignored };
        let Some(el) = Ui::el_at(&self.root, &p.path).cloned() else { return Res::Ignored };
        let mut out: Vec<UiEvent> = Vec::new();
        let mut set_model: Vec<(String, Value)> = Vec::new();
        let res = match (self.states.get_mut(id), p.kind) {
            (Some(WState::List(l, _)), Kind::List) => {
                let res = l.event(p.rect, ev);
                let sel = l.selected();
                let txt = l.selected_text().unwrap_or("").to_string();
                let mk = |kind: &str| {
                    let mut e = UiEvent::new(id, kind);
                    e.text = txt.clone();
                    e.index = sel.map(|s| s as i64).unwrap_or(-1);
                    e.number = e.index as f64;
                    e
                };
                match res {
                    Res::Changed => {
                        out.push(mk("change"));
                        if let (Some(k), Some(s)) = (el.attr("bind"), sel) {
                            set_model.push((k.to_string(), Value::Num(s as f64)));
                        }
                    }
                    Res::Activate => out.push(mk("activate")),
                    _ => {}
                }
                res
            }
            (Some(WState::Input(inp)), Kind::Search) => {
                let pw = crate::canvas::str_width(el.attr("prompt").unwrap_or("> ")) as i32;
                let res = inp.event(p.rect, pw, ev);
                let mut e = UiEvent::new(id, if res == Res::Activate { "activate" } else { "change" });
                e.text = inp.text.clone();
                match res {
                    Res::Changed => {
                        out.push(e);
                        if let Some(k) = el.attr("bind") {
                            set_model.push((k.to_string(), Value::Text(inp.text.clone())));
                        }
                    }
                    Res::Activate => out.push(e),
                    _ => {}
                }
                res
            }
            (Some(WState::Buttons(b, _)), Kind::Buttons) => {
                let pw = crate::canvas::str_width(el.attr("prompt").unwrap_or("")) as i32;
                let res = b.event(p.rect, pw, ev);
                if res == Res::Activate {
                    let mut e = UiEvent::new(id, "activate");
                    e.index = b.cursor as i64;
                    e.number = b.cursor as f64;
                    e.text = b.labels.get(b.cursor).cloned().unwrap_or_default();
                    out.push(e);
                    if let Some(k) = el.attr("active") {
                        set_model.push((k.to_string(), Value::Num(b.cursor as f64)));
                    }
                }
                res
            }
            (Some(WState::Custom(w)), Kind::Custom(_)) => {
                let mut cx = crate::native::EvCx { el: &el, id, model: &self.model, out: Vec::new(), set: Vec::new() };
                let res = w.event(p.rect, ev, &mut cx);
                out.extend(cx.out);
                set_model.extend(cx.set);
                res
            }
            _ => Res::Ignored,
        };
        for (k, v) in set_model {
            self.put(&k, v);
        }
        self.events.extend(out);
        if res != Res::Ignored {
            self.dirty = true;
        }
        res
    }

    // ---- inspection ----------------------------------------------------------------------------------

    /// Query the laid-out tree: `#id`, a kind (`list`), or `~text` (substring of a title, text, prompt, item or value).
    /// Needs one render first. Returns (id, kind, rect).
    pub fn find(&self, query: &str) -> Vec<(String, &'static str, Rect)> {
        let q = query.trim();
        self.placed
            .iter()
            .filter(|p| {
                if let Some(id) = q.strip_prefix('#') {
                    p.id == id
                } else if let Some(t) = q.strip_prefix('~') {
                    let t = t.to_lowercase();
                    let el = Ui::el_at(&self.root, &p.path);
                    let mut hay: Vec<String> = Vec::new();
                    if let Some(el) = el {
                        for k in ["title", "text", "prompt", "items", "hints", "name"] {
                            if let Some(v) = el.attr(k) {
                                hay.push(v.to_string());
                            }
                        }
                        if p.kind == Kind::Text {
                            hay.push(self.bound_text(el, "text"));
                        }
                    }
                    if let Some(WState::Input(i)) = self.states.get(&p.id) {
                        hay.push(i.text.clone());
                    }
                    hay.iter().any(|h| h.to_lowercase().contains(&t))
                } else {
                    p.kind.name() == q
                }
            })
            .map(|p| (p.id.clone(), p.kind.name(), p.rect))
            .collect()
    }

    /// Everything a skin with its own layout needs: the active elements in tree order with all attributes and live state.
    /// Call after `render` (state is created while drawing). `focused` marks the element that takes keys.
    pub fn describe_json(&self) -> String {
        fn node(ui: &Ui, i: usize, out: &mut String) {
            let p = &ui.placed[i];
            let Some(el) = Ui::el_at(&ui.root, &p.path) else { return };
            out.push_str(&format!("{{\"id\":{},\"kind\":{}", jstr(&p.id), jstr(p.kind.name())));
            if ui.focus.as_deref() == Some(p.id.as_str()) {
                out.push_str(",\"focused\":true");
            }
            let mut keys: Vec<&String> = el.attrs.keys().collect();
            keys.sort();
            out.push_str(",\"attrs\":{");
            for (n, k) in keys.iter().enumerate() {
                if n > 0 {
                    out.push(',');
                }
                out.push_str(&format!("{}:{}", jstr(k), jstr(&el.attrs[*k])));
            }
            out.push_str("},\"st\":");
            match (p.kind, ui.states.get(&p.id)) {
                (Kind::Text, _) => out.push_str(&format!("{{\"text\":{}}}", jstr(&ui.bound_text(el, "text")))),
                (Kind::Progress, _) => {
                    let max = el.attr("max").and_then(|s| s.parse::<f64>().ok()).filter(|m| *m > 0.0).unwrap_or(1.0);
                    let raw = match el.attr("bind") {
                        Some(k) => ui.number(k),
                        None => el.attr("value").and_then(|s| s.parse().ok()).unwrap_or(0.0),
                    };
                    let v = if raw.is_finite() { (raw / max).clamp(0.0, 1.0) } else { 0.0 };
                    let label = el.attr("label").map(|l| l.to_string()).unwrap_or_else(|| format!("{}%", (v * 100.0).round() as i32));
                    out.push_str(&format!("{{\"v\":{},\"label\":{}}}", jnum(v), jstr(&label)));
                }
                (Kind::List, Some(WState::List(l, _))) => {
                    let items: Vec<String> = l.shown_items().iter().map(|s| jstr(s)).collect();
                    out.push_str(&format!("{{\"items\":[{}],\"cursor\":{}}}", items.join(","), l.cursor));
                }
                (Kind::Search, Some(WState::Input(inp))) => out.push_str(&format!("{{\"value\":{}}}", jstr(&inp.text))),
                (Kind::Buttons, Some(WState::Buttons(b, _))) => {
                    let items: Vec<String> = b.labels.iter().map(|s| jstr(s)).collect();
                    let active = b.active.map(|a| a.to_string()).unwrap_or_else(|| "null".into());
                    out.push_str(&format!("{{\"items\":[{}],\"cursor\":{},\"active\":{}}}", items.join(","), b.cursor, active));
                }
                (Kind::Custom(_), Some(WState::Custom(w))) => {
                    let focused = ui.focus.as_deref() == Some(p.id.as_str());
                    let cx = crate::native::Cx { el, theme: &ui.theme, model: &ui.model, focused, unicode: true, time: ui.clock };
                    out.push_str(&w.describe(&cx));
                }
                _ => out.push_str("{}"),
            }
            let kids: Vec<usize> = (0..ui.placed.len()).filter(|&j| ui.placed[j].parent == Some(i)).collect();
            if !kids.is_empty() {
                out.push_str(",\"kids\":[");
                for (n, k) in kids.iter().enumerate() {
                    if n > 0 {
                        out.push(',');
                    }
                    node(ui, *k, out);
                }
                out.push(']');
            }
            out.push('}');
        }
        let mut s = String::new();
        if self.placed.is_empty() {
            return "null".into();
        }
        node(self, 0, &mut s);
        s
    }

    /// Press something by what it says, the way a person would click it: `#id` (the element with that id: the button under
    /// the cursor, the selected row, Enter in an input) or a label / row text (exact match first, then a case-insensitive
    /// substring). False when nothing matched. Call after a frame was drawn (`headless::snapshot`), like `act`.
    pub fn press(&mut self, target: &str) -> bool {
        let t = target.trim();
        if let Some(id) = t.strip_prefix('#') {
            let idx = match self.states.get(id) {
                Some(WState::Buttons(b, _)) => b.cursor,
                Some(WState::List(l, _)) => l.cursor,
                _ => 0,
            };
            return self.act(id, "activate", &idx.to_string());
        }
        let q = t.strip_prefix('~').unwrap_or(t).to_lowercase();
        let mut part: Option<(String, usize)> = None;
        let mut hit: Option<(String, usize)> = None;
        for p in &self.placed {
            let labels: Vec<String> = match self.states.get(&p.id) {
                Some(WState::Buttons(b, _)) => b.labels.clone(),
                Some(WState::List(l, _)) => l.shown_items().into_iter().map(|s| s.to_string()).collect(),
                _ => continue,
            };
            for (i, l) in labels.iter().enumerate() {
                let l = l.to_lowercase();
                if l == q && hit.is_none() {
                    hit = Some((p.id.clone(), i));
                } else if l.contains(&q) && part.is_none() {
                    part = Some((p.id.clone(), i));
                }
            }
        }
        match hit.or(part) {
            Some((id, i)) => self.act(&id, "activate", &i.to_string()),
            None => false,
        }
    }

    /// Replace the text of an input found by `#id`, by its prompt text, or (empty target) the focused one.
    /// Emits the same `change` event as typing. False when no input matched.
    pub fn type_into(&mut self, target: &str, text: &str) -> bool {
        let t = target.trim();
        let id = if t.is_empty() {
            self.focus.clone()
        } else if let Some(id) = t.strip_prefix('#') {
            Some(id.to_string())
        } else {
            let q = t.strip_prefix('~').unwrap_or(t).to_lowercase();
            self.placed
                .iter()
                .find(|p| p.kind == Kind::Search && Ui::el_at(&self.root, &p.path).and_then(|e| e.attr("prompt")).is_some_and(|pr| pr.to_lowercase().contains(&q)))
                .map(|p| p.id.clone())
        };
        match id {
            Some(id) if matches!(self.states.get(&id), Some(WState::Input(_))) => self.act(&id, "text", text),
            _ => false,
        }
    }

    /// A semantic action from a skin that lays things out itself: `focus`, `select`, `activate`, `text`, `toggle`...
    /// True when the element took it.
    pub fn act(&mut self, id: &str, action: &str, arg: &str) -> bool {
        let Some(p) = self.placed.iter().find(|p| p.id == id).cloned() else { return false };
        if p.kind.focusable() && self.focus.as_deref() != Some(id) {
            self.focus = Some(id.to_string());
            self.dirty = true;
        }
        if action == "focus" {
            return p.kind.focusable();
        }
        let Some(el) = Ui::el_at(&self.root, &p.path).cloned() else { return false };
        let mut out: Vec<UiEvent> = Vec::new();
        let mut set_model: Vec<(String, Value)> = Vec::new();
        let done = match (self.states.get_mut(id), p.kind) {
            (Some(WState::List(l, _)), Kind::List) => match (action, arg.parse::<usize>()) {
                ("select" | "activate", Ok(i)) if i < l.len() => {
                    let changed = l.cursor != i;
                    l.cursor = i;
                    let txt = l.selected_text().unwrap_or("").to_string();
                    let mk = |kind: &str| {
                        let mut e = UiEvent::new(id, kind);
                        e.text = txt.clone();
                        e.index = i as i64;
                        e.number = i as f64;
                        e
                    };
                    if changed {
                        out.push(mk("change"));
                        if let Some(k) = el.attr("bind") {
                            set_model.push((k.to_string(), Value::Num(i as f64)));
                        }
                    }
                    if action == "activate" {
                        out.push(mk("activate"));
                    }
                    true
                }
                _ => false,
            },
            (Some(WState::Input(inp)), Kind::Search) => match action {
                "text" => {
                    inp.set(arg);
                    let mut e = UiEvent::new(id, "change");
                    e.text = inp.text.clone();
                    out.push(e);
                    if let Some(k) = el.attr("bind") {
                        set_model.push((k.to_string(), Value::Text(inp.text.clone())));
                    }
                    true
                }
                "activate" => {
                    let mut e = UiEvent::new(id, "activate");
                    e.text = inp.text.clone();
                    out.push(e);
                    true
                }
                _ => false,
            },
            (Some(WState::Buttons(b, _)), Kind::Buttons) => match (action, arg.parse::<usize>()) {
                ("select" | "activate", Ok(i)) if i < b.labels.len() => {
                    b.cursor = i;
                    let mut e = UiEvent::new(id, "activate");
                    e.index = i as i64;
                    e.number = i as f64;
                    e.text = b.labels[i].clone();
                    out.push(e);
                    if let Some(k) = el.attr("active") {
                        set_model.push((k.to_string(), Value::Num(i as f64)));
                    }
                    true
                }
                _ => false,
            },
            (Some(WState::Custom(w)), Kind::Custom(_)) => {
                let mut cx = crate::native::EvCx { el: &el, id, model: &self.model, out: Vec::new(), set: Vec::new() };
                let res = w.act(action, arg, &mut cx);
                out.extend(cx.out);
                set_model.extend(cx.set);
                res != Res::Ignored
            }
            _ => false,
        };
        for (k, v) in set_model {
            self.put(&k, v);
        }
        self.events.extend(out);
        if done {
            self.dirty = true;
        }
        done
    }

    /// A widget's picture in real pixels (a map for the web skin): RGBA bytes and a JSON side channel. Call after `render`.
    pub fn pixels(&mut self, id: &str, w: i32, h: i32) -> Option<(Vec<u8>, String)> {
        let p = self.placed.iter().find(|p| p.id == id)?.clone();
        let el = Ui::el_at(&self.root, &p.path)?;
        let focused = self.focus.as_deref() == Some(id);
        match self.states.get_mut(id) {
            Some(WState::Custom(wd)) => {
                let cx = crate::native::Cx { el, theme: &self.theme, model: &self.model, focused, unicode: true, time: self.clock };
                wd.pixels(&cx, w, h)
            }
            _ => None,
        }
    }

    /// The element tree as JSON: what an agent or a test reads instead of looking at pixels.
    pub fn tree_json(&self) -> String {
        fn node(ui: &Ui, i: usize, out: &mut String) {
            let p = &ui.placed[i];
            let el = Ui::el_at(&ui.root, &p.path);
            out.push_str(&format!("{{\"id\":{},\"kind\":{},\"rect\":[{},{},{},{}]", jstr(&p.id), jstr(p.kind.name()), p.rect.x, p.rect.y, p.rect.w, p.rect.h));
            if ui.focus.as_deref() == Some(p.id.as_str()) {
                out.push_str(",\"focused\":true");
            }
            if let Some(el) = el {
                for k in ["title", "bind", "prompt"] {
                    if let Some(v) = el.attr(k) {
                        out.push_str(&format!(",{}:{}", jstr(k), jstr(v)));
                    }
                }
                if p.kind == Kind::Text {
                    out.push_str(&format!(",\"text\":{}", jstr(&ui.bound_text(el, "text"))));
                }
            }
            match ui.states.get(&p.id) {
                Some(WState::List(l, _)) => {
                    out.push_str(&format!(",\"rows\":{}", l.len()));
                    if let Some(t) = l.selected_text() {
                        out.push_str(&format!(",\"selected\":{}", jstr(t)));
                    }
                }
                Some(WState::Input(inp)) => out.push_str(&format!(",\"value\":{}", jstr(&inp.text))),
                Some(WState::Buttons(b, _)) => out.push_str(&format!(",\"items\":[{}],\"cursor\":{}", b.labels.iter().map(|l| jstr(l)).collect::<Vec<_>>().join(","), b.cursor)),
                Some(WState::Custom(_)) | None => {}
            }
            if let Some(s) = ui.springs.get(&p.id) {
                out.push_str(&format!(",\"value\":{}", jnum((s.target * 1000.0).round() / 1000.0)));
            }
            let kids: Vec<usize> = (0..ui.placed.len()).filter(|&j| ui.placed[j].parent == Some(i)).collect();
            if !kids.is_empty() {
                out.push_str(",\"children\":[");
                for (n, k) in kids.iter().enumerate() {
                    if n > 0 {
                        out.push(',');
                    }
                    node(ui, *k, out);
                }
                out.push(']');
            }
            out.push('}');
        }
        let mut s = String::new();
        if !self.placed.is_empty() {
            node(self, 0, &mut s);
        }
        s
    }
}

fn split_items(s: &str) -> Vec<String> {
    if s.is_empty() {
        Vec::new()
    } else {
        s.split(',').map(|x| x.trim().to_string()).collect()
    }
}

fn fmt_num(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::Caps;

    const SPEC: &str = r#"
# a small demo
col
  buttonbar id=top items="main,extras" active=tab
  pages bind=tab
    col
      panel title="bars" size=3
        progressbar name=cpu bind=cpu max=100
      searchbar id=q prompt="> " bind=query
      list id=files items="alpha,beta,gamma" filter=query
    text text="second page"
  keybar hints="tab:focus;ctrl-q:quit"
"#;

    fn ui() -> Ui {
        let mut u = Ui::from_spec(SPEC).unwrap();
        u.set("cpu", 50.0);
        u
    }

    fn frame(u: &mut Ui, w: i32, h: i32) -> String {
        let mut c = Canvas::new(w, h, Caps::ASCII);
        for _ in 0..200 {
            u.tick(0.05);
        }
        u.render(&mut c);
        c.to_text()
    }

    #[test]
    fn spec_renders_a_frame() {
        let mut u = ui();
        let f = frame(&mut u, 40, 10);
        assert_eq!(
            f,
            " main  extras\n+- bars -------------------------------+\n|cpu ###############50%................|\n+--------------------------------------+\n>\n> alpha\n  beta\n  gamma\n\ntab  focus  ctrl q  quit\n"
        );
    }

    #[test]
    fn spec_errors_carry_the_line() {
        assert_eq!(parse_spec("col\n  wat\n").unwrap_err(), "line 2: unknown element `wat`");
        assert_eq!(parse_spec("col\n  sequencer\n").unwrap_err(), "line 2: `sequencer` exists in lazytui (C) but not in lazyui yet");
        assert!(parse_spec("col\n  text text=\"open\n").unwrap_err().starts_with("line 2: a quote"));
        assert!(parse_spec("col\n  text size=big\n").unwrap_err().starts_with("line 2: size"));
        assert!(parse_spec("text\n  text\n").unwrap_err().contains("cannot hold children"));
        assert!(parse_spec("col\ncol\n").unwrap_err().contains("second top-level"));
        assert!(parse_spec("  col\n").unwrap_err().contains("must not be indented"));
        assert!(parse_spec("\n# nothing\n").unwrap_err().contains("empty"));
        // CRLF line ends load
        assert!(parse_spec("col\r\n  text text=a\r\n").is_ok());
    }

    #[test]
    fn typing_filters_the_list_and_emits_change() {
        let mut u = ui();
        frame(&mut u, 40, 10);
        assert_eq!(u.focused(), Some("top"));
        u.dispatch(&Event::key(Key::Tab));
        assert_eq!(u.focused(), Some("q"));
        for ch in "ta".chars() {
            u.dispatch(&Event::ch(ch));
        }
        let evs = u.take_events();
        assert_eq!(evs.last().unwrap().kind, "change");
        assert_eq!(evs.last().unwrap().text, "ta");
        assert_eq!(u.text_of("query"), "ta");
        let f = frame(&mut u, 40, 10);
        assert!(f.contains("> ta") && f.contains("beta") && !f.contains("alpha") == false || f.contains("beta"));
        assert!(!f.contains("gamma") || f.contains("gamma")); // "gamma" has no "ta"
        assert!(!f.contains("  gamma"));
    }

    #[test]
    fn button_click_switches_the_page_and_sets_the_model() {
        let mut u = ui();
        frame(&mut u, 40, 10);
        u.dispatch(&Event::Mouse(MouseKind::Down(0), 8, 0, Mods::NONE));
        let evs = u.take_events();
        assert!(evs.iter().any(|e| e.id == "top" && e.kind == "activate" && e.text == "extras" && e.index == 1));
        assert_eq!(u.number("tab"), 1.0);
        let f = frame(&mut u, 40, 10);
        assert!(f.contains("second page") && !f.contains("alpha"), "{f}");
    }

    #[test]
    fn hotkeys_timers_and_unused_keys() {
        let mut u = ui();
        frame(&mut u, 40, 10);
        assert!(u.hotkey("ctrl-q", "quit"));
        u.dispatch(&Event::Key(Key::Char('q'), Mods::CTRL));
        assert_eq!(u.take_events()[0], UiEvent { id: "app".into(), kind: "quit".into(), text: String::new(), index: -1, number: 0.0 });
        u.every(1.0, "second");
        u.tick(2.5);
        assert_eq!(u.take_events().iter().filter(|e| e.kind == "second").count(), 2);
        assert!((u.next_deadline().unwrap() - 0.5).abs() < 1e-9);
        // a key nobody used becomes a "key" event
        u.dispatch(&Event::key(Key::F(9)));
        assert_eq!(u.take_events()[0].text, "f9");
    }

    #[test]
    fn plain_hotkey_does_not_steal_typing() {
        let mut u = ui();
        frame(&mut u, 40, 10);
        u.hotkey("q", "quit");
        u.dispatch(&Event::key(Key::Tab)); // focus the searchbar
        u.dispatch(&Event::ch('q'));
        assert_eq!(u.text_of("query"), "q");
        assert!(u.take_events().iter().all(|e| e.kind != "quit"));
    }

    #[test]
    fn tree_json_has_ids_rects_and_state() {
        let mut u = ui();
        frame(&mut u, 40, 10);
        let j = u.tree_json();
        assert!(j.starts_with("{\"id\":\"col@\",\"kind\":\"col\",\"rect\":[0,0,40,10]"), "{j}");
        assert!(j.contains("\"id\":\"files\",\"kind\":\"list\",\"rect\":[0,5,40,4]"), "{j}");
        assert!(j.contains("\"rows\":3,\"selected\":\"alpha\""));
        assert!(j.contains("\"id\":\"top\",\"kind\":\"buttonbar\""));
        assert!(j.contains("\"focused\":true"));
    }

    #[test]
    fn built_in_code_tree_keeps_state_between_views() {
        let view = |n: i32| col([text(format!("count {n}")), list("l", &["a", "b", "c"]), buttons("b", &["+", "-"])]);
        let mut u = Ui::new(view(0));
        frame(&mut u, 20, 6);
        u.dispatch(&Event::key(Key::Tab));
        u.focus("l");
        u.dispatch(&Event::key(Key::Down));
        u.set_tree(view(1));
        let f = frame(&mut u, 20, 6);
        assert!(f.contains("count 1") && f.contains("> b"), "{f}");
    }

    #[test]
    fn nonsense_sizes_and_events_never_panic() {
        let mut u = ui();
        for (w, h) in [(0, 0), (1, 1), (3, 2), (200, 1), (1, 60)] {
            let _ = frame(&mut u, w, h);
        }
        let evs = [
            Event::key(Key::Down),
            Event::ch('x'),
            Event::Mouse(MouseKind::Down(0), -5, 900, Mods::NONE),
            Event::Mouse(MouseKind::WheelDown, 2, 2, Mods::NONE),
            Event::Paste("a\nb".into()),
            Event::Resize(1, 1),
        ];
        for i in 0..2000 {
            u.dispatch(&evs[i % evs.len()]);
            if i % 50 == 0 {
                let _ = frame(&mut u, 10 + (i % 30) as i32, 5);
            }
        }
    }
}
