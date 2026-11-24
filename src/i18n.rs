//! Translations as data: a `Catalog` maps `key -> text` per locale, read from a plain tab-separated file.
//!
//! ```text
//! # ui.tsv - first column is the key, the header names the locales, `#` starts a comment, \n \t \\ are escapes
//! key	en	pl
//! hello	Hello, {name}!	Czesc, {name}!
//! files#one	{n} file	{n} plik
//! files#few	{n} files	{n} pliki
//! files#many	{n} files	{n} plikow
//! files#other	{n} files	{n} plikow
//! ```
//!
//! Lookup goes `pl-PL` -> `pl` -> the fallback locales (default `en`) -> the key itself, so a missing translation shows
//! English and a missing key shows its own name instead of nothing.
//!
//! In a tree, write `text("@hello")` (or `text(tr("hello"))`): a whole attribute that starts with `@` is a key, resolved by
//! `Ui` when the tree is drawn, so switching the locale re-translates the screen with no rebuild. `@@x` is a literal `@x`.
//! Arguments for `{name}` and the count for plurals travel in the marker: `trf("hello", &[("name", "Ann")])`, `trn("files", 3)`.

use crate::ui::El;
use std::borrow::Cow;
use std::collections::HashMap;

/// Separates the key from its arguments inside a marker (a control character, so no spec text can contain it by accident).
const SEP: char = '\u{1f}';
/// Attributes whose text is shown to the user and may be a key. `items` and `hints` are lists and handled per element.
const TEXT_ATTRS: [&str; 7] = ["text", "title", "prompt", "label", "placeholder", "items", "hints"];

#[derive(Clone, Debug)]
pub struct Catalog {
    locale: String,
    fallback: Vec<String>,
    /// locale -> key -> text
    texts: HashMap<String, HashMap<String, String>>,
}

impl Default for Catalog {
    fn default() -> Catalog {
        Catalog { locale: "en".into(), fallback: vec!["en".into()], texts: HashMap::new() }
    }
}

fn unescape(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c != '\\' {
            o.push(c);
            continue;
        }
        match it.next() {
            Some('n') => o.push('\n'),
            Some('t') => o.push('\t'),
            Some('\\') => o.push('\\'),
            Some(x) => {
                o.push('\\');
                o.push(x);
            }
            None => o.push('\\'),
        }
    }
    o
}

impl Catalog {
    pub fn new() -> Catalog {
        Catalog::default()
    }

    /// Parse a TSV catalog (see the module doc). Errors name the line.
    pub fn from_tsv(src: &str) -> Result<Catalog, String> {
        let mut c = Catalog::new();
        c.add_tsv(src)?;
        Ok(c)
    }

    /// Merge more rows in; a later row for the same key and locale wins, so a user file can override a built-in one.
    pub fn add_tsv(&mut self, src: &str) -> Result<(), String> {
        let mut head: Option<Vec<String>> = None;
        for (n, raw) in src.lines().enumerate() {
            let n = n + 1;
            let line = raw.trim_end_matches('\r');
            if line.trim().is_empty() || line.trim_start().starts_with('#') {
                continue;
            }
            let cells: Vec<&str> = line.split('\t').collect();
            match &head {
                None => {
                    if cells[0].trim() != "key" || cells.len() < 2 {
                        return Err(format!("line {n}: the header must be `key<TAB>locale...`"));
                    }
                    let locales: Vec<String> = cells[1..].iter().map(|l| l.trim().to_string()).collect();
                    if locales.iter().any(|l| l.is_empty()) {
                        return Err(format!("line {n}: empty locale name in the header"));
                    }
                    head = Some(locales);
                }
                Some(locales) => {
                    let key = cells[0].trim();
                    if key.is_empty() {
                        return Err(format!("line {n}: empty key"));
                    }
                    if cells.len() > locales.len() + 1 {
                        return Err(format!("line {n}: {} columns but the header has {} locales", cells.len() - 1, locales.len()));
                    }
                    for (i, loc) in locales.iter().enumerate() {
                        self.texts.entry(loc.clone()).or_default(); // declared in the header counts as a known locale
                        // a missing or empty cell means "not translated": the fallback chain answers
                        if let Some(t) = cells.get(i + 1).filter(|t| !t.is_empty()) {
                            self.add(loc, key, &unescape(t));
                        }
                    }
                }
            }
        }
        if head.is_none() && !src.trim().is_empty() && src.lines().any(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#')) {
            return Err("no header".into());
        }
        Ok(())
    }

    pub fn add(&mut self, locale: &str, key: &str, text: &str) {
        self.texts.entry(locale.to_string()).or_default().insert(key.to_string(), text.to_string());
    }

    /// Every locale that has at least one text, sorted.
    pub fn locales(&self) -> Vec<String> {
        let mut v: Vec<String> = self.texts.keys().cloned().collect();
        v.sort();
        v
    }

    pub fn locale(&self) -> &str {
        &self.locale
    }

    /// Switch the language: `pl`, `pl-PL`, `pl_PL.UTF-8` all work (the part after the first `.` or `@` is dropped).
    pub fn set_locale(&mut self, locale: &str) {
        let l = locale.split(['.', '@']).next().unwrap_or("").trim().replace('_', "-");
        self.locale = if l.is_empty() { "en".into() } else { l };
    }

    /// Locales tried after the current one and its base language. Default `["en"]`.
    pub fn set_fallback(&mut self, chain: &[&str]) {
        self.fallback = chain.iter().map(|s| s.to_string()).collect();
    }

    /// The locales tried for a lookup, in order: `pl-PL`, `pl`, then the fallbacks.
    pub fn chain(&self) -> Vec<String> {
        let mut v: Vec<String> = Vec::new();
        let mut push = |s: &str| {
            if !s.is_empty() && !v.iter().any(|x| x == s) {
                v.push(s.to_string());
            }
        };
        push(&self.locale);
        push(self.locale.split('-').next().unwrap_or(""));
        for f in &self.fallback {
            push(f);
            push(f.split('-').next().unwrap_or(""));
        }
        v
    }

    /// The text of `key` in the first locale of the chain that has it.
    pub fn lookup(&self, key: &str) -> Option<&str> {
        self.chain().iter().find_map(|l| self.texts.get(l)?.get(key)).map(|s| s.as_str())
    }

    /// The translated text, or the key itself when no locale has it.
    pub fn tr(&self, key: &str) -> String {
        self.lookup(key).unwrap_or(key).to_string()
    }

    /// `tr` with `{name}` placeholders filled from `args`. `{{` and `}}` are literal braces; an unknown `{x}` stays as written.
    pub fn fmt(&self, key: &str, args: &[(&str, &str)]) -> String {
        subst(self.lookup(key).unwrap_or(key), args)
    }

    /// The plural form for count `n`: tries `key#<category>` (`one`, `few`, `many`, `other`... by the rules of the locale
    /// that has the text), then `key#other`, then `key`. `{n}` is filled in, other placeholders from `args`.
    pub fn trn(&self, key: &str, n: i64, args: &[(&str, &str)]) -> String {
        let ns = n.to_string();
        let mut all: Vec<(&str, &str)> = vec![("n", ns.as_str())];
        all.extend_from_slice(args);
        for loc in self.chain() {
            let Some(map) = self.texts.get(&loc) else { continue };
            let lang = loc.split('-').next().unwrap_or("");
            let cat = plural_category(lang, n);
            for k in [format!("{key}#{cat}"), format!("{key}#other"), key.to_string()] {
                if let Some(t) = map.get(&k) {
                    return subst(t, &all);
                }
            }
        }
        subst(key, &all)
    }

    /// Resolve one attribute value: a marker is translated, `@@x` loses one `@`, anything else is returned as is.
    pub fn resolve<'a>(&self, s: &'a str) -> Cow<'a, str> {
        if let Some(rest) = s.strip_prefix("@@") {
            return Cow::Owned(format!("@{rest}"));
        }
        let Some((key, args)) = parse_marker(s) else { return Cow::Borrowed(s) };
        let args: Vec<(&str, &str)> = args.iter().map(|(a, b)| (*a, *b)).collect();
        match args.iter().find(|(k, _)| *k == "n").and_then(|(_, v)| v.parse::<i64>().ok()) {
            Some(n) => Cow::Owned(self.trn(key, n, &args)),
            None => Cow::Owned(self.fmt(key, &args)),
        }
    }

    /// A copy of the tree with every visible text attribute resolved.
    pub fn resolve_tree(&self, el: &El) -> El {
        let mut out = el.clone();
        self.resolve_in_place(&mut out);
        out
    }

    fn resolve_in_place(&self, el: &mut El) {
        for k in TEXT_ATTRS {
            let Some(v) = el.attrs.get(k) else { continue };
            let new = match k {
                "items" => v.split(',').map(|x| self.resolve(x).into_owned()).collect::<Vec<_>>().join(","),
                "hints" => v
                    .split(';')
                    .map(|h| match h.split_once(':') {
                        Some((key, label)) => format!("{key}:{}", self.resolve(label.trim())),
                        None => h.to_string(),
                    })
                    .collect::<Vec<_>>()
                    .join(";"),
                _ => self.resolve(v).into_owned(),
            };
            if &new != v {
                el.attrs.insert(k.to_string(), new);
            }
        }
        for c in &mut el.children {
            self.resolve_in_place(c);
        }
    }
}

fn subst(t: &str, args: &[(&str, &str)]) -> String {
    let mut o = String::with_capacity(t.len() + 8);
    let mut rest = t;
    while let Some(i) = rest.find(['{', '}']) {
        o.push_str(&rest[..i]);
        let tail = &rest[i..];
        if tail.starts_with("{{") || tail.starts_with("}}") {
            o.push_str(&tail[..1]);
            rest = &tail[2..];
        } else if tail.starts_with('{') {
            match tail.find('}') {
                Some(j) => {
                    let name = &tail[1..j];
                    match args.iter().find(|(k, _)| *k == name) {
                        Some((_, v)) => o.push_str(v),
                        None => o.push_str(&tail[..=j]),
                    }
                    rest = &tail[j + 1..];
                }
                None => {
                    o.push_str(tail);
                    rest = "";
                }
            }
        } else {
            o.push('}');
            rest = &tail[1..];
        }
    }
    o.push_str(rest);
    o
}

/// CLDR-style plural category of the integer `n` for a language code. Unknown languages use the English rule.
pub fn plural_category(lang: &str, n: i64) -> &'static str {
    let a = n.unsigned_abs();
    let (m10, m100) = (a % 10, a % 100);
    match lang {
        "ja" | "zh" | "ko" | "vi" | "th" | "id" | "ms" => "other",
        "fr" | "pt" => {
            if a <= 1 {
                "one"
            } else {
                "other"
            }
        }
        "pl" => {
            if a == 1 {
                "one"
            } else if (2..=4).contains(&m10) && !(12..=14).contains(&m100) {
                "few"
            } else {
                "many"
            }
        }
        "ru" | "uk" | "be" => {
            if m10 == 1 && m100 != 11 {
                "one"
            } else if (2..=4).contains(&m10) && !(12..=14).contains(&m100) {
                "few"
            } else {
                "many"
            }
        }
        "cs" | "sk" => {
            if a == 1 {
                "one"
            } else if (2..=4).contains(&a) {
                "few"
            } else {
                "other"
            }
        }
        _ => {
            if a == 1 {
                "one"
            } else {
                "other"
            }
        }
    }
}

// ---- markers: how a key travels inside an attribute --------------------------------------------------

/// `"@key"` or `"@key<US>name=value<US>n=3"` -> (key, args). A key is letters, digits and `_ . - #`.
fn parse_marker(s: &str) -> Option<(&str, Vec<(&str, &str)>)> {
    let body = s.strip_prefix('@')?;
    let mut parts = body.split(SEP);
    let key = parts.next()?;
    if key.is_empty() || !key.chars().all(|c| c.is_alphanumeric() || matches!(c, '_' | '.' | '-' | '#')) {
        return None;
    }
    let mut args = Vec::new();
    for p in parts {
        let (k, v) = p.split_once('=')?;
        args.push((k, v));
    }
    Some((key, args))
}

/// A key to be translated when the tree is drawn: `text(tr("hello"))`.
pub fn tr(key: &str) -> String {
    format!("@{key}")
}

/// A key with `{name}` arguments: `text(trf("hello", &[("name", "Ann")]))`.
pub fn trf(key: &str, args: &[(&str, &str)]) -> String {
    let mut s = format!("@{key}");
    for (k, v) in args {
        s.push(SEP);
        s.push_str(k);
        s.push('=');
        s.push_str(&v.replace(SEP, " "));
    }
    s
}

/// A plural key with its count: `text(trn("files", 3))` picks `files#few` in Polish, `files#other` in English.
pub fn trn(key: &str, n: i64) -> String {
    trf(key, &[("n", &n.to_string())])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::{col, text, Ui};

    const TSV: &str = "# demo\nkey\ten\tpl\tde\nhello\tHello, {name}!\tCzesc, {name}!\t\nonly_en\tEnglish only\t\t\nfiles#one\t{n} file\t{n} plik\t\nfiles#few\t{n} files\t{n} pliki\t\nfiles#many\t{n} files\t{n} plikow\t\nfiles#other\t{n} files\t{n} plikow\t\nbrace\t{{x}} and {name}\t\t\nlines\ta\\nb\t\t\n";

    fn cat() -> Catalog {
        Catalog::from_tsv(TSV).unwrap()
    }

    #[test]
    fn fallback_chain_pl_to_en_to_key() {
        let mut c = cat();
        c.set_locale("pl_PL.UTF-8");
        assert_eq!(c.locale(), "pl-PL");
        assert_eq!(c.chain(), vec!["pl-PL", "pl", "en"]);
        assert_eq!(c.tr("only_en"), "English only"); // pl has no text: english answers
        assert_eq!(c.fmt("hello", &[("name", "Ann")]), "Czesc, Ann!");
        assert_eq!(c.tr("nosuchkey"), "nosuchkey"); // nothing anywhere: the key itself
        c.set_locale("de"); // de column is empty: all english
        assert_eq!(c.fmt("hello", &[("name", "Ann")]), "Hello, Ann!");
        c.set_locale("xx");
        assert_eq!(c.tr("only_en"), "English only");
        c.set_fallback(&["pl"]);
        assert_eq!(c.fmt("hello", &[("name", "A")]), "Czesc, A!");
    }

    #[test]
    fn placeholders_escape_and_unknown() {
        let c = cat();
        assert_eq!(c.fmt("brace", &[("name", "Z")]), "{x} and Z");
        assert_eq!(c.fmt("hello", &[]), "Hello, {name}!"); // unknown placeholder stays visible
        assert_eq!(c.tr("lines"), "a\nb");
        assert_eq!(subst("open { never", &[]), "open { never");
        assert_eq!(subst("stray } brace", &[]), "stray } brace");
    }

    #[test]
    fn plural_rules_per_language() {
        let mut c = cat();
        assert_eq!(c.trn("files", 1, &[]), "1 file");
        assert_eq!(c.trn("files", 0, &[]), "0 files");
        assert_eq!(c.trn("files", 5, &[]), "5 files");
        c.set_locale("pl");
        let got: Vec<String> = [1, 2, 4, 5, 12, 22, 25, 112].iter().map(|n| c.trn("files", *n, &[])).collect();
        assert_eq!(got, ["1 plik", "2 pliki", "4 pliki", "5 plikow", "12 plikow", "22 pliki", "25 plikow", "112 plikow"]);
        assert_eq!(plural_category("ru", 21), "one");
        assert_eq!(plural_category("ru", 11), "many");
        assert_eq!(plural_category("fr", 0), "one");
        assert_eq!(plural_category("ja", 1), "other");
        assert_eq!(plural_category("en", -1), "one");
    }

    #[test]
    fn tsv_errors_name_the_line_and_later_rows_win() {
        assert!(Catalog::from_tsv("hello\ten\n").unwrap_err().contains("line 1"));
        assert!(Catalog::from_tsv("key\ten\n\tx\n").unwrap_err().contains("line 2"));
        assert!(Catalog::from_tsv("key\ten\na\tb\tc\n").unwrap_err().contains("line 2"));
        assert!(Catalog::from_tsv("# only comments\n\n").is_ok());
        let mut c = cat();
        c.add_tsv("key\ten\nonly_en\toverridden\n").unwrap();
        assert_eq!(c.tr("only_en"), "overridden");
        assert_eq!(c.locales(), vec!["de", "en", "pl"]);
    }

    #[test]
    fn markers_resolve_and_literals_survive() {
        let c = cat();
        assert_eq!(c.resolve("plain text"), "plain text");
        assert_eq!(c.resolve("@hello"), "Hello, {name}!");
        assert_eq!(c.resolve(&trf("hello", &[("name", "Bo")])), "Hello, Bo!");
        assert_eq!(c.resolve(&trn("files", 2)), "2 files");
        assert_eq!(c.resolve("@@bob"), "@bob");
        assert_eq!(c.resolve("mail me @ home"), "mail me @ home"); // not a marker: has spaces
        assert_eq!(c.resolve("@nokey"), "nokey");
    }

    #[test]
    fn ui_retranslates_on_locale_switch_without_rebuilding() {
        let mut ui = Ui::new(col([text(trf("hello", &[("name", "Ann")])), text(trn("files", 3)), text("@@raw")]));
        // without a catalog nothing is touched
        assert!(crate::headless::snapshot(&mut ui, 30, 4).contains("@hello"));
        ui.set_catalog(Some(cat()));
        let en = crate::headless::snapshot(&mut ui, 30, 4);
        assert!(en.contains("Hello, Ann!") && en.contains("3 files") && en.contains("@raw") && !en.contains("@@raw"), "{en}");
        assert!(ui.set_locale("pl"));
        let pl = crate::headless::snapshot(&mut ui, 30, 4);
        assert!(pl.contains("Czesc, Ann!") && pl.contains("3 pliki"), "{pl}");
        ui.set_tree(col([text("@only_en")])); // a rebuilt tree is translated too
        assert!(crate::headless::snapshot(&mut ui, 30, 4).contains("English only"));
        ui.set_catalog(None);
        assert!(crate::headless::snapshot(&mut ui, 30, 4).contains("@only_en"));
    }

    #[test]
    fn items_and_hints_resolve_per_element() {
        let mut c = Catalog::new();
        c.add("en", "ok", "OK");
        c.add("pl", "ok", "Dobrze");
        c.add("pl", "quit", "wyjscie");
        c.set_locale("pl");
        let el = crate::ui::buttons("b", &["@ok", "plain"]).set("hints", "ctrl-q:@quit;tab:focus");
        let r = c.resolve_tree(&el);
        assert_eq!(r.attr("items"), Some("Dobrze,plain"));
        assert_eq!(r.attr("hints"), Some("ctrl-q:wyjscie;tab:focus"));
    }
}
