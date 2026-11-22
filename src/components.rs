//! Data widgets ("components"): a widget written as a spec file, shared by name.
//!
//! ```text
//! # widgets/meter.lui   - a labelled bar
//! params label bind max=100
//! row
//!   text text="$label" size=8 style=dim
//!   progressbar bind=$bind max=$max
//! ```
//!
//! then `meter label=CPU bind=cpu` in any spec. Rules, small on purpose (a C implementation is a page of code):
//! - the optional first line `params a b=default c="two words"` names the parameters (bare = required);
//! - `$name` in the body is replaced by the value given at the use site (or the default);
//! - attributes the use site gives that are not parameters (`size`, `id`, `gap`...) go on the root element;
//! - a line `slot` in the body is replaced by the children written under the use site;
//! - components may use components (depth-limited); a component never shadows a built-in element.
//!
//! Where they come from, later wins: `Builtin` (compiled in, always there, works offline) < `Global` (the user's
//! shared folder, also what `lazyui widgets add URL` downloads into) < `Local` (a `widgets/` folder next to the spec).
//! The expansion is plain text before parsing, so errors name the component file and the line it was used at.

use crate::ui::{tokenize, Kind};
use std::collections::BTreeMap;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scope {
    Builtin,
    Global,
    Local,
}

impl Scope {
    pub fn name(self) -> &'static str {
        match self {
            Scope::Builtin => "builtin",
            Scope::Global => "global",
            Scope::Local => "local",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Component {
    pub name: String,
    pub scope: Scope,
    /// file path, or `builtin`
    pub origin: String,
    /// first comment line of the file
    pub doc: String,
    /// (name, default); None = required
    params: Vec<(String, Option<String>)>,
    /// the file as written, for `lazyui widgets show`
    pub text: String,
    /// (line number in the file, text)
    body: Vec<(usize, String)>,
}

#[derive(Clone, Default, Debug)]
pub struct Widgets {
    map: BTreeMap<String, Component>,
}

/// A source line with where it came from, so an error in an expanded line points at the component, not at nothing.
#[derive(Clone, Debug)]
pub struct SrcLine {
    pub text: String,
    pub label: String,
}

pub fn plain_lines(src: &str) -> Vec<SrcLine> {
    src.lines().enumerate().map(|(i, l)| SrcLine { text: l.to_string(), label: format!("line {}", i + 1) }).collect()
}

const BUILTIN: &[(&str, &str)] = &[
    ("card", include_str!("../widgets/card.lui")),
    ("meter", include_str!("../widgets/meter.lui")),
    ("stat", include_str!("../widgets/stat.lui")),
    ("titlebar", include_str!("../widgets/titlebar.lui")),
    ("statusbar", include_str!("../widgets/statusbar.lui")),
];

fn valid_name(n: &str) -> bool {
    let mut c = n.chars();
    matches!(c.next(), Some('a'..='z')) && c.all(|x| x.is_ascii_lowercase() || x.is_ascii_digit() || x == '_' || x == '-')
}

fn indent_of(s: &str) -> usize {
    s.len() - s.trim_start().len()
}

fn blank(s: &str) -> bool {
    let t = s.trim();
    t.is_empty() || t.starts_with('#')
}

impl Widgets {
    pub fn empty() -> Widgets {
        Widgets::default()
    }

    pub fn builtin() -> Widgets {
        let mut w = Widgets::default();
        for (n, src) in BUILTIN {
            w.add_source(n, src, Scope::Builtin, "builtin").expect("a built-in component must parse");
        }
        w
    }

    /// Add one component; replaces a same-named one (later scope wins at the call sites that add in order).
    pub fn add_source(&mut self, name: &str, text: &str, scope: Scope, origin: &str) -> Result<(), String> {
        if !valid_name(name) {
            return Err(format!("`{name}`: a component name is lowercase letters, digits, - and _, starting with a letter"));
        }
        if Kind::parse(name).is_some() {
            return Err(format!("`{name}` is a built-in element and cannot be redefined"));
        }
        let lines: Vec<&str> = text.lines().collect();
        let mut skip: Option<usize> = None;
        let doc = lines.iter().find(|l| l.trim_start().starts_with('#')).map(|l| l.trim_start().trim_start_matches('#').trim().to_string()).unwrap_or_default();
        let mut params = Vec::new();
        if let Some(i) = lines.iter().position(|l| !blank(l)) {
            let t = lines[i].trim_start();
            if t == "params" || t.starts_with("params ") {
                for tok in tokenize(t).map_err(|e| format!("{origin}: params: {e}"))?.into_iter().skip(1) {
                    match tok.split_once('=') {
                        Some((k, v)) => params.push((k.to_string(), Some(v.to_string()))),
                        None => params.push((tok, None)),
                    }
                }
                skip = Some(i);
            }
        }
        for (k, _) in &params {
            if k.is_empty() || !k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                return Err(format!("{origin}: bad parameter name `{k}`"));
            }
        }
        let body: Vec<(usize, String)> = lines.iter().enumerate().filter(|(i, _)| Some(*i) != skip).map(|(i, l)| (i + 1, l.trim_end().to_string())).collect();
        if !body.iter().any(|(_, l)| !blank(l)) {
            return Err(format!("{origin}: the component has no body"));
        }
        // the same text again (a project folder holding a copy of a built-in) keeps the earlier, more basic scope
        if self.map.get(name).is_some_and(|old| old.text == text) {
            return Ok(());
        }
        self.map.insert(name.to_string(), Component { name: name.to_string(), scope, origin: origin.to_string(), doc, text: text.to_string(), params, body });
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&Component> {
        self.map.get(name)
    }

    /// `(name, default)` of every parameter; None = required.
    pub fn param_list(c: &Component) -> Vec<(String, Option<String>)> {
        c.params.clone()
    }

    pub fn names(&self) -> Vec<&Component> {
        self.map.values().collect()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Load every `*.lui` in a folder as components named after the files. Returns what could not be loaded.
    pub fn add_dir(&mut self, dir: &std::path::Path, scope: Scope) -> Vec<String> {
        let mut problems = Vec::new();
        let Ok(rd) = std::fs::read_dir(dir) else { return problems };
        let mut files: Vec<_> = rd.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "lui")).collect();
        files.sort();
        for p in files {
            let Some(name) = p.file_stem().and_then(|s| s.to_str()) else { continue };
            match std::fs::read_to_string(&p) {
                Ok(t) => {
                    if let Err(e) = self.add_source(name, &t, scope, &p.display().to_string()) {
                        problems.push(e);
                    }
                }
                Err(e) => problems.push(format!("{}: {e}", p.display())),
            }
        }
        problems
    }

    /// Builtin, then the global folder, then local folders (next to the spec, then `./widgets`).
    pub fn standard(spec_dir: Option<&std::path::Path>, extra: &[std::path::PathBuf]) -> (Widgets, Vec<String>) {
        let mut w = Widgets::builtin();
        let mut problems = Vec::new();
        if let Some(g) = global_dir() {
            problems.extend(w.add_dir(&g, Scope::Global));
        }
        if let Some(d) = spec_dir {
            problems.extend(w.add_dir(&d.join("widgets"), Scope::Local));
        }
        problems.extend(w.add_dir(std::path::Path::new("widgets"), Scope::Local));
        for d in extra {
            problems.extend(w.add_dir(d, Scope::Local));
        }
        (w, problems)
    }

    /// Replace every use of a component by its body. Lines of the result keep labels for error messages.
    pub fn expand(&self, lines: Vec<SrcLine>) -> Result<Vec<SrcLine>, String> {
        if self.map.is_empty() {
            return Ok(lines);
        }
        self.expand_at(lines, 0)
    }

    fn expand_at(&self, lines: Vec<SrcLine>, depth: usize) -> Result<Vec<SrcLine>, String> {
        if depth > 32 {
            return Err("components nest too deep (a component uses itself?)".into());
        }
        let mut out = Vec::with_capacity(lines.len());
        let mut i = 0;
        while i < lines.len() {
            let l = &lines[i];
            if blank(&l.text) {
                out.push(l.clone());
                i += 1;
                continue;
            }
            let t = l.text.trim_start();
            let indent = indent_of(&l.text);
            let first = t.split_whitespace().next().unwrap_or("");
            let comp = if first.contains('=') || Kind::parse(first).is_some() { None } else { self.map.get(first) };
            let Some(comp) = comp else {
                out.push(l.clone());
                i += 1;
                continue;
            };
            let mut j = i + 1;
            while j < lines.len() && (blank(&lines[j].text) || indent_of(&lines[j].text) > indent) {
                j += 1;
            }
            let mut end = j;
            while end > i + 1 && blank(&lines[end - 1].text) {
                end -= 1;
            }
            let toks = tokenize(t).map_err(|e| format!("{}: {e}", l.label))?;
            let mut given: Vec<(String, String)> = Vec::new();
            for tok in toks.iter().skip(1) {
                let (k, v) = tok.split_once('=').ok_or_else(|| format!("{}: `{tok}` is not key=value", l.label))?;
                given.push((k.to_string(), v.to_string()));
            }
            let made = comp.instantiate(&given, &lines[i + 1..end], indent, &l.label)?;
            out.extend(self.expand_at(made, depth + 1)?);
            i = j;
        }
        Ok(out)
    }
}

impl Component {
    fn instantiate(&self, given: &[(String, String)], children: &[SrcLine], indent: usize, used_at: &str) -> Result<Vec<SrcLine>, String> {
        let here = |what: String| format!("{used_at}: `{}`: {what}", self.name);
        let mut values: Vec<(&str, String)> = Vec::new();
        for (p, d) in &self.params {
            let v = given.iter().rev().find(|(k, _)| k == p).map(|(_, v)| v.clone()).or_else(|| d.clone());
            match v {
                Some(v) if v.contains('"') => return Err(here(format!("the value of `{p}` contains a double quote"))),
                Some(v) => values.push((p.as_str(), v)),
                None => return Err(here(format!("missing parameter `{p}`"))),
            }
        }
        let extra: Vec<&(String, String)> = given.iter().filter(|(k, _)| !self.params.iter().any(|(p, _)| p == k)).collect();
        for (k, v) in &extra {
            if v.contains('"') || k.is_empty() {
                return Err(here(format!("the value of `{k}` contains a double quote")));
            }
        }
        let root_line = self.body.iter().position(|(_, l)| !blank(l)).unwrap_or(0);
        let has_slot = self.body.iter().any(|(_, l)| l.trim() == "slot");
        if !has_slot && children.iter().any(|c| !blank(&c.text)) {
            return Err(here("was given children but has no `slot` line".into()));
        }
        let pad = " ".repeat(indent);
        let mut out = Vec::new();
        for (k, (lineno, raw)) in self.body.iter().enumerate() {
            let label = format!("{} line {lineno} (used at {used_at})", self.name);
            if raw.trim() == "slot" {
                let base = children.iter().filter(|c| !blank(&c.text)).map(|c| indent_of(&c.text)).min().unwrap_or(0);
                let slot_indent = indent_of(raw);
                for c in children {
                    if blank(&c.text) {
                        continue;
                    }
                    let rel = indent_of(&c.text) - base;
                    out.push(SrcLine { text: format!("{pad}{}{}{}", " ".repeat(slot_indent), " ".repeat(rel), c.text.trim_start()), label: c.label.clone() });
                }
                continue;
            }
            let mut text = substitute(raw, &values);
            if k == root_line {
                for (key, v) in &extra {
                    text.push_str(&format!(" {key}=\"{v}\""));
                }
            }
            if blank(&text) {
                continue;
            }
            out.push(SrcLine { text: format!("{pad}{text}"), label });
        }
        Ok(out)
    }
}

fn substitute(line: &str, values: &[(&str, String)]) -> String {
    let b = line.as_bytes();
    let mut out = String::with_capacity(line.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'$' {
            let mut j = i + 1;
            while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'_') {
                j += 1;
            }
            if let Some((_, v)) = values.iter().find(|(k, _)| *k == &line[i + 1..j]) {
                out.push_str(v);
                i = j;
                continue;
            }
        }
        let ch = line[i..].chars().next().unwrap_or(' ');
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// The shared per-user folder: `$LAZYUI_WIDGETS`, else `$XDG_CONFIG_HOME/lazyui/widgets`, else `~/.config/lazyui/widgets`.
pub fn global_dir() -> Option<std::path::PathBuf> {
    let env = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    if let Some(d) = env("LAZYUI_WIDGETS") {
        return Some(d.into());
    }
    if let Some(x) = env("XDG_CONFIG_HOME") {
        return Some(std::path::Path::new(&x).join("lazyui/widgets"));
    }
    env("HOME").map(|h| std::path::Path::new(&h).join(".config/lazyui/widgets"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::headless::snapshot;
    use crate::ui::{parse_spec_with, Ui};

    fn ui(spec: &str, w: &Widgets) -> Ui {
        Ui::from_spec_with(spec, w.clone()).unwrap_or_else(|e| panic!("{e}"))
    }

    #[test]
    fn a_component_is_used_by_name_with_parameters() {
        let mut u = ui("col\n  meter label=CPU bind=cpu max=10\n", &Widgets::builtin());
        u.set("cpu", 5.0);
        let s = snapshot(&mut u, 40, 3);
        assert!(s.starts_with("CPU"), "{s}");
        assert!(s.contains("50%"), "{s}");
    }

    #[test]
    fn children_go_into_the_slot_and_extra_attributes_onto_the_root() {
        let w = Widgets::builtin();
        let mut u = ui("col\n  card title=\"Hello world\" size=5\n    text text=inside\n    text text=again\n  text text=after\n", &w);
        let s = snapshot(&mut u, 30, 8);
        assert!(s.contains("Hello world") && s.contains("inside") && s.contains("again"), "{s}");
        assert_eq!(s.lines().nth(5).unwrap_or("").trim(), "after", "size=5 reached the panel:\n{s}");
    }

    #[test]
    fn errors_name_the_component_file_and_the_use_site() {
        let w = Widgets::builtin();
        let e = parse_spec_with("col\n  meter label=x\n", &w).unwrap_err();
        assert_eq!(e, "line 2: `meter`: missing parameter `bind`");
        let e = parse_spec_with("col\n  stat label=a bind=b\n    text text=x\n", &w).unwrap_err();
        assert!(e.contains("has no `slot`"), "{e}");
        let mut w2 = Widgets::builtin();
        w2.add_source("bad", "row\n  wat\n", Scope::Local, "bad.lui").unwrap();
        let e = parse_spec_with("col\n  text text=x\n  bad\n", &w2).unwrap_err();
        assert_eq!(e, "bad line 2 (used at line 3): unknown element `wat`");
    }

    #[test]
    fn components_nest_and_cycles_are_caught() {
        let mut w = Widgets::builtin();
        w.add_source("pair", "params a\ncol\n  meter label=$a bind=x\n  meter label=two bind=x\n", Scope::Local, "pair.lui").unwrap();
        let mut u = ui("col\n  pair a=one\n", &w);
        u.set("x", 50.0);
        let s = snapshot(&mut u, 30, 4);
        assert!(s.contains("one") && s.contains("two"), "{s}");
        w.add_source("ping", "col\n  pong\n", Scope::Local, "ping.lui").unwrap();
        w.add_source("pong", "col\n  ping\n", Scope::Local, "pong.lui").unwrap();
        assert!(parse_spec_with("col\n  ping\n", &w).unwrap_err().contains("nest too deep"));
    }

    #[test]
    fn names_are_checked_and_builtin_elements_cannot_be_shadowed() {
        let mut w = Widgets::empty();
        assert!(w.add_source("list", "text\n", Scope::Local, "x").unwrap_err().contains("built-in element"));
        assert!(w.add_source("Bad Name", "text\n", Scope::Local, "x").is_err());
        assert!(w.add_source("empty", "# nothing\n", Scope::Local, "x").is_err());
        assert!(w.add_source("ok_1-x", "text\n", Scope::Local, "x").is_ok());
    }

    #[test]
    fn later_scopes_win_and_folders_load() {
        let dir = std::env::temp_dir().join(format!("lazyui-widgets-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("stat.lui"), "# mine\nparams label bind\ntext text=\"MINE $label\"\n").unwrap();
        std::fs::write(dir.join("broken.lui"), "params a=\"x\ntext\n").unwrap();
        let mut w = Widgets::builtin();
        let problems = w.add_dir(&dir, Scope::Local);
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert_eq!(w.get("stat").unwrap().scope, Scope::Local);
        assert_eq!(w.get("stat").unwrap().doc, "mine");
        assert_eq!(w.get("meter").unwrap().scope, Scope::Builtin);
        let mut u = ui("col\n  stat label=q bind=z\n", &w);
        assert!(snapshot(&mut u, 20, 2).contains("MINE q"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_spec_without_components_parses_exactly_as_before() {
        let src = "col\n  text text=a\n  list items=\"x,y\"\n";
        assert_eq!(format!("{:?}", parse_spec_with(src, &Widgets::builtin()).unwrap()), format!("{:?}", crate::ui::parse_spec(src).unwrap()));
    }
}
