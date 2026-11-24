//! The builder side of the tree: chainable methods on `El`, a few ready elements and the `Component` trait.
//! A whole screen is a handful of lines of Rust, no spec file:
//!
//! ```
//! use lazyui::prelude::*;
//! let screen = col([text("hello").accent(), row([input("q", "> "), button("go").on("go").size("6")]), list("rows", ["a", "b"])]);
//! # let _ = screen;
//! ```
//!
//! Everything here only adds: it sets attributes the spec already knows, so a built tree and a parsed `.lui` are the same tree.

use crate::ui::{El, Kind};

// ---- chainable attributes ----------------------------------------------------------------------------

impl El {
    /// The event id: widgets report `(id, kind)`; a button with `.on("save")` sends `("save", "activate")`.
    pub fn on(self, id: &str) -> El {
        self.id(id)
    }
    /// Model key the element shows or edits (text, progress, input, pages).
    pub fn bind(self, key: &str) -> El {
        self.set("bind", key)
    }
    /// Model key holding the filter text of a list.
    pub fn filter(self, key: &str) -> El {
        self.set("filter", key)
    }
    /// Model key holding the active index of a button bar.
    pub fn active(self, key: &str) -> El {
        self.set("active", key)
    }
    pub fn title(self, s: impl ToString) -> El {
        self.set("title", s)
    }
    pub fn prompt(self, s: impl ToString) -> El {
        self.set("prompt", s)
    }
    /// Replace the rows or labels of a list / button bar.
    pub fn items<S: ToString>(self, items: impl IntoIterator<Item = S>) -> El {
        self.set("items", items.into_iter().map(|s| s.to_string()).collect::<Vec<_>>().join(","))
    }
    /// Upper bound of a progress bar.
    pub fn max(self, v: f64) -> El {
        self.set("max", v)
    }
    // style: one colour word per text, the theme decides the actual colour
    pub fn dim(self) -> El {
        self.set("style", "dim")
    }
    pub fn accent(self) -> El {
        self.set("style", "accent")
    }
    pub fn ok(self) -> El {
        self.set("style", "ok")
    }
    pub fn warn(self) -> El {
        self.set("style", "warn")
    }
    pub fn err(self) -> El {
        self.set("style", "err")
    }
    pub fn center(self) -> El {
        self.set("align", "center")
    }
    pub fn right(self) -> El {
        self.set("align", "right")
    }
    // size along the parent's direction: rows in a col, columns in a row
    pub fn fixed(self, n: u32) -> El {
        self.size(&n.to_string())
    }
    pub fn fill(self) -> El {
        self.size("fill")
    }
    pub fn weight(self, w: u32) -> El {
        self.size(&format!("fill:{}", w.max(1)))
    }
    pub fn pct(self, p: u32) -> El {
        self.size(&format!("pct:{}", p.min(100)))
    }

    /// Make a reusable subtree safe to use twice: every `id`, `bind`, `active` and `filter` that starts with `.` gets
    /// `prefix` in front (`.q` becomes `left.q`). Names without the dot stay shared.
    pub fn scope(mut self, prefix: &str) -> El {
        for k in ["id", "bind", "active", "filter", "heatbind"] {
            if let Some(v) = self.attrs.get_mut(k) {
                if let Some(rest) = v.strip_prefix('.') {
                    *v = format!("{prefix}.{rest}");
                }
            }
        }
        self.children = self.children.into_iter().map(|c| c.scope(prefix)).collect();
        self
    }
}

// ---- ready elements ----------------------------------------------------------------------------------

/// A text input. Events: `(id, "change", text)` while typing, `(id, "activate", text)` on Enter.
pub fn input(id: &str, prompt: &str) -> El {
    crate::ui::search(id, prompt)
}

/// One button. Give it an event id with `.on("id")`; it sends `(id, "activate", label)`.
pub fn button(label: impl ToString) -> El {
    El::new(Kind::Buttons).set("items", label)
}

/// A bar of buttons. Events: `(id, "activate", label)` with `index` set; match on the id and the index, not the label (labels may be translated).
pub fn buttons<S: ToString>(id: &str, labels: impl IntoIterator<Item = S>) -> El {
    El::new(Kind::Buttons).id(id).items(labels)
}

/// A list of rows. Events: `(id, "change" | "activate", row text)` with `index`.
pub fn list<S: ToString>(id: &str, rows: impl IntoIterator<Item = S>) -> El {
    El::new(Kind::List).id(id).items(rows)
}

/// Tabs that switch `pages`: the active index lives in model key `key`.
pub fn tabs<S: ToString>(id: &str, labels: impl IntoIterator<Item = S>, key: &str) -> El {
    buttons(id, labels).active(key)
}

/// Pages, one visible at a time, chosen by the number in model key `key`.
pub fn pages(key: &str, children: impl IntoIterator<Item = El>) -> El {
    El::new(Kind::Pages).bind(key).children(children)
}

/// Any native widget by registered name (`sysbar`, `linechart`, `gauge`, ... see `lazyui widgets ls`).
/// An unknown name draws a visible error line instead of failing, so a typo shows up in the first snapshot.
pub fn widget(name: &str) -> El {
    match Kind::parse(name) {
        Some(k) => El::new(k),
        None => crate::ui::text(format!("unknown widget `{name}`")).err(),
    }
}

/// `label  value` on one row.
pub fn stat(label: &str, value: impl ToString) -> El {
    crate::ui::row([crate::ui::text(label).dim().fixed(12), crate::ui::text(value)])
}

/// A labelled progress bar bound to model key `key`.
pub fn meter(label: &str, key: &str, max: f64) -> El {
    crate::ui::progress(0.0).set("name", label).bind(key).max(max)
}

// ---- components --------------------------------------------------------------------------------------

/// A reusable piece of UI. Implement it on a small struct (its fields are the parameters) or just write a function
/// returning `El`; a closure `Fn() -> El` is a component too.
///
/// Use ids and model keys that start with `.` inside, and call `.scope("name")` at the use site so two copies do not collide.
pub trait Component {
    fn view(&self) -> El;
    /// `view` with local names prefixed: the usual way to place a component.
    fn at(&self, prefix: &str) -> El {
        self.view().scope(prefix)
    }
}

impl<F: Fn() -> El> Component for F {
    fn view(&self) -> El {
        self()
    }
}

impl Component for El {
    fn view(&self) -> El {
        self.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::{col, row, text, Ui};

    #[test]
    fn chain_sets_the_attributes_the_spec_uses() {
        let e = text("x").accent().center().fixed(3).on("t");
        assert_eq!(e.attr("style"), Some("accent"));
        assert_eq!(e.attr("align"), Some("center"));
        assert_eq!(e.attr("size"), Some("3"));
        assert_eq!(e.attr("id"), Some("t"));
        assert_eq!(text("y").weight(0).attr("size"), Some("fill:1"));
        assert_eq!(text("y").pct(400).attr("size"), Some("pct:100"));
    }

    #[test]
    fn built_tree_equals_parsed_spec() {
        let built = col([text("hi"), list("rows", ["a", "b"]), input("q", "> ")]);
        let mut a = Ui::new(built);
        let mut b = Ui::from_spec("col\n  text text=hi\n  list id=rows items=\"a,b\"\n  searchbar id=q prompt=\"> \"\n").unwrap();
        assert_eq!(crate::headless::snapshot(&mut a, 20, 6), crate::headless::snapshot(&mut b, 20, 6));
    }

    #[test]
    fn items_accept_strings_numbers_and_arrays() {
        let rows: Vec<String> = vec!["x".into(), "y".into()];
        assert_eq!(list("l", &rows).attr("items"), Some("x,y"));
        assert_eq!(list("l", 1..=3).attr("items"), Some("1,2,3"));
        assert_eq!(buttons("b", &["+", "-"]).attr("items"), Some("+,-"));
        assert_eq!(list("l", Vec::<String>::new()).attr("items"), Some(""));
    }

    #[test]
    fn button_is_clickable_and_reports_its_id() {
        let mut ui = Ui::new(col([text("t"), button("go").on("go")]));
        crate::headless::settle(&mut ui, 20, 4, 1.0);
        assert!(ui.act("go", "activate", "0"));
        let ev = ui.take_events();
        assert_eq!((ev[0].id.as_str(), ev[0].kind.as_str(), ev[0].text.as_str()), ("go", "activate", "go"));
    }

    #[test]
    fn unknown_widget_is_visible_not_fatal() {
        let mut ui = Ui::new(widget("nosuchwidget"));
        assert!(crate::headless::snapshot(&mut ui, 40, 2).contains("unknown widget `nosuchwidget`"));
        assert_eq!(widget("sysbar").kind.name(), "sysbar");
    }

    struct Field(&'static str);
    impl Component for Field {
        fn view(&self) -> El {
            row([text(self.0).dim().fixed(8), input(".v", "")])
        }
    }

    #[test]
    fn a_component_used_twice_keeps_separate_ids() {
        let f = Field("name");
        let g = Field("mail");
        let mut ui = Ui::new(col([f.at("a"), g.at("b")]));
        crate::headless::settle(&mut ui, 30, 4, 1.0);
        assert!(ui.act("a.v", "text", "ann"));
        assert!(ui.act("b.v", "text", "x@y"));
        let ev = ui.take_events();
        assert_eq!((ev[0].id.as_str(), ev[0].text.as_str()), ("a.v", "ann"));
        assert_eq!((ev[1].id.as_str(), ev[1].text.as_str()), ("b.v", "x@y"));
        let snap = crate::headless::snapshot(&mut ui, 30, 4);
        assert!(snap.contains("name") && snap.contains("ann") && snap.contains("mail") && snap.contains("x@y"), "{snap}");
    }

    #[test]
    fn scope_only_touches_dotted_names_and_closures_are_components() {
        let e = col([text("t").bind("shared"), text("u").bind(".mine").on(".id")]).scope("p");
        assert_eq!(e.children[0].attr("bind"), Some("shared"));
        assert_eq!(e.children[1].attr("bind"), Some("p.mine"));
        assert_eq!(e.children[1].attr("id"), Some("p.id"));
        let c = || text("closure");
        assert_eq!(c.view().attr("text"), Some("closure"));
    }
}
