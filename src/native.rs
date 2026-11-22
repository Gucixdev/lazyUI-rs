//! Native widgets: the ones that need code (charts, pickers, trees). A widget is a `Widget` value kept in the Ui next to the
//! element's id; the registry maps a spec name (`graph`, `zonebar`...) to how it is made. Everything a widget may read comes
//! through `Cx`; everything it may change goes through `EvCx`, so no widget can reach into the Ui.

use crate::canvas::{Canvas, Rect};
use crate::gfx;
use crate::input::Event;
use crate::theme::Theme;
use crate::ui::{El, Value};
use crate::widgets::Res;
use std::collections::HashMap;

/// What a widget sees while drawing.
pub struct Cx<'a> {
    pub el: &'a El,
    pub theme: &'a Theme,
    pub(crate) model: &'a HashMap<String, Value>,
    pub focused: bool,
    pub unicode: bool,
    /// seconds on the Ui clock (it only moves with `tick`, so headless runs are repeatable)
    pub time: f64,
}

impl Cx<'_> {
    pub fn attr(&self, k: &str) -> Option<&str> {
        self.el.attr(k)
    }
    pub fn text_attr(&self, k: &str, default: &str) -> String {
        self.el.attr(k).unwrap_or(default).to_string()
    }
    pub fn num_attr(&self, k: &str, default: f64) -> f64 {
        self.el.attr(k).and_then(|s| s.trim().parse::<f64>().ok()).filter(|x| x.is_finite()).unwrap_or(default)
    }
    pub fn model_text(&self, k: &str) -> Option<String> {
        self.model.get(k).map(|v| match v {
            Value::Text(t) => t.clone(),
            Value::Num(n) => format!("{n}"),
        })
    }
    pub fn model_num(&self, k: &str) -> Option<f64> {
        self.model.get(k).map(|v| match v {
            Value::Num(n) => *n,
            Value::Text(t) => t.trim().parse().unwrap_or(0.0),
        })
    }
    /// The element's number: `bind=key` from the model when it is there, else `value=`.
    pub fn value(&self) -> f64 {
        match self.el.attr("bind").and_then(|k| self.model_num(k)) {
            Some(v) if v.is_finite() => v,
            Some(_) => 0.0,
            None => self.num_attr("value", 0.0),
        }
    }
    /// The element's list of numbers: `bind=key` (a comma separated text in the model) when set, else `values=`.
    pub fn series(&self) -> Vec<f64> {
        match self.el.attr("bind").and_then(|k| self.model_text(k)) {
            Some(t) => gfx::numbers(&t),
            None => gfx::numbers(self.el.attr("values").unwrap_or("")),
        }
    }
    /// Comma separated words: labels, colours.
    pub fn list_attr(&self, k: &str) -> Vec<String> {
        match self.el.attr(k) {
            Some(s) if !s.is_empty() => s.split(',').map(|x| x.trim().to_string()).collect(),
            _ => Vec::new(),
        }
    }
    /// Items of a list-like widget: `from=KEY` (comma separated text in the model) when that key exists, else `items=`.
    pub fn items(&self) -> Vec<String> {
        match self.el.attr("from").and_then(|k| self.model_text(k)) {
            Some(t) if !t.is_empty() => t.split(',').map(|x| x.trim().to_string()).collect(),
            Some(_) => Vec::new(),
            None => self.list_attr("items"),
        }
    }
    pub fn flag(&self, k: &str) -> bool {
        matches!(self.el.attr(k), Some("1" | "true" | "yes" | "on"))
    }
    /// The colour named by attribute `k` (`accent`, `ok`, `#rrggbb`...), else `default`.
    pub fn colour(&self, k: &str, default: crate::canvas::Color) -> crate::canvas::Color {
        self.el.attr(k).and_then(|n| gfx::colour(self.theme, n)).unwrap_or(default)
    }
}

/// What a widget may do with an event: say something to the program, or set a model key.
pub struct EvCx<'a> {
    pub el: &'a El,
    pub(crate) id: &'a str,
    pub(crate) model: &'a HashMap<String, Value>,
    pub(crate) out: Vec<crate::ui::UiEvent>,
    pub(crate) set: Vec<(String, Value)>,
}

impl EvCx<'_> {
    pub fn emit(&mut self, kind: &str, text: &str, index: i64, number: f64) {
        let mut e = crate::ui::UiEvent::new(self.id, kind);
        e.text = text.to_string();
        e.index = index;
        e.number = number;
        self.out.push(e);
    }
    pub fn attr(&self, k: &str) -> Option<&str> {
        self.el.attr(k)
    }
    /// Write the widget's value to its `bind` key (nothing when it has none).
    pub fn set_bound(&mut self, v: Value) {
        if let Some(k) = self.el.attr("bind") {
            self.set.push((k.to_string(), v));
        }
    }
    pub fn model_num(&self, k: &str) -> Option<f64> {
        self.model.get(k).map(|v| match v {
            Value::Num(n) => *n,
            Value::Text(t) => t.trim().parse().unwrap_or(0.0),
        })
    }
}

pub trait Widget {
    fn draw(&mut self, c: &mut Canvas, r: Rect, cx: &Cx);
    fn event(&mut self, _r: Rect, _ev: &Event, _cx: &mut EvCx) -> Res {
        Res::Ignored
    }
    /// What a web skin needs to draw this widget as HTML/SVG: a JSON object. The default carries the element's own data.
    fn describe(&self, cx: &Cx) -> String {
        let nums: Vec<String> = cx.series().iter().map(|x| format!("{x}")).collect();
        let items: Vec<String> = cx.items().iter().map(|x| crate::ui::jstr(x)).collect();
        format!(
            "{{\"series\":[{}],\"value\":{},\"items\":[{}],\"text\":{}}}",
            nums.join(","),
            cx.value(),
            items.join(","),
            crate::ui::jstr(&cx.attr("bind").and_then(|k| cx.model_text(k)).unwrap_or_default())
        )
    }
    /// A semantic action from a skin that has no character grid: `select`, `activate`, `toggle`... with one argument.
    fn act(&mut self, _action: &str, _arg: &str, _cx: &mut EvCx) -> Res {
        Res::Ignored
    }
    /// Time passed (virtual clock). May emit events and set bound keys, like a key press.
    fn tick(&mut self, _dt: f64, _cx: &mut EvCx) {}
    /// Drawn after everything else, so a popup can lie over its neighbours.
    fn overlay(&mut self, _c: &mut Canvas, _r: Rect, _cx: &Cx) {}
    /// The area that takes the mouse: `r`, or more while a popup is open.
    fn extent(&self, r: Rect) -> Rect {
        r
    }
    /// True while it is still settling towards its picture (the headless snapshot waits for this).
    fn animating(&self) -> bool {
        false
    }
}

/// A widget with no state of its own: a function that draws from the element's attributes and the model.
pub struct Pure(pub fn(&mut Canvas, Rect, &Cx));

impl Widget for Pure {
    fn draw(&mut self, c: &mut Canvas, r: Rect, cx: &Cx) {
        (self.0)(c, r, cx)
    }
}

#[derive(Clone, Copy)]
pub struct Def {
    pub name: &'static str,
    /// one line of help for `lazyui widgets ls`
    pub doc: &'static str,
    /// takes keys and the mouse, takes part in tab order
    pub focus: bool,
    /// Some(n): n rows tall inside a column unless `size` says otherwise
    pub rows: Option<i32>,
    /// its picture changes with time alone: seconds between frames (0 = only when something happens).
    /// 0.1 or less keeps the loop drawing; longer is a timer deadline
    pub live: f64,
    pub make: fn() -> Box<dyn Widget>,
}

const TABLES: [&[Def]; 3] = [crate::charts::DEFS, crate::lists::DEFS, crate::timers::DEFS];

pub fn find(name: &str) -> Option<&'static Def> {
    all().find(|d| d.name == name)
}

pub fn all() -> impl Iterator<Item = &'static Def> {
    TABLES.into_iter().flat_map(|t| t.iter())
}
