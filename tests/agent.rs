//! The agent loop on real apps: build with the prelude, drive by meaning (`press`, `type_into`), read the frame and the tree.
use lazyui::prelude::*;

struct Form {
    name: String,
    saved: Vec<String>,
}

impl App for Form {
    fn view(&self) -> El {
        col([
            row([text("name").dim().fixed(6), input("name", "who: ")]),
            buttons("act", ["Save", "Clear"]).size("1"),
            text(format!("saved: {}", self.saved.join(","))),
        ])
    }
    fn on(&mut self, e: &UiEvent, _: &mut Cx) {
        if e.is("name", "change") {
            self.name = e.text.clone();
        } else if e.activated("act") && e.index == 0 {
            self.saved.push(self.name.clone());
        } else if e.activated("act") {
            self.saved.clear();
        }
    }
}

fn form() -> Harness<Sketch<Form>> {
    test(Form { name: String::new(), saved: vec![] }, 40, 6)
}

#[test]
fn type_press_and_read_back() {
    let mut h = form();
    assert!(h.type_into("#name", "ann"));
    assert!(h.press("save")); // case-insensitive label
    assert!(h.type_into("who", "bob")); // by prompt text
    assert!(h.press("#act")); // button under the cursor: still "Save"
    assert_eq!(h.program.0.saved, ["ann", "bob"]);
    h.expect_text("saved: ann,bob").unwrap();
    assert!(h.press("Clear"));
    h.expect_no_text("saved: ann").unwrap();
    h.expect_text("saved:").unwrap();
}

#[test]
fn misses_report_false_and_failures_show_the_frame() {
    let mut h = form();
    assert!(!h.press("nonexistent"));
    assert!(!h.press("#nope"));
    assert!(!h.type_into("#act", "x")); // a button is not an input
    assert!(!h.key("no-such-key"));
    let err = h.expect_text("zzz").unwrap_err();
    assert!(err.contains("zzz") && err.contains("saved:"), "{err}");
    assert!(h.expect_no_text("saved:").is_err());
}

#[test]
fn tree_is_json_with_ids_kinds_and_state() {
    let mut h = form();
    h.type_into("#name", "q");
    let t = h.tree();
    assert!(t.starts_with('{') && t.ends_with('}') && !t.contains('\n'));
    for needle in ["\"id\":\"name\"", "\"kind\":\"searchbar\"", "\"value\":\"q\"", "\"items\":[\"Save\",\"Clear\"]", "\"text\":\"saved: \""] {
        assert!(t.contains(needle), "{needle} missing in {t}");
    }
    let hits = h.find("#act");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].1, "buttonbar");
    assert!(!h.find("~saved").is_empty());
    assert!(h.find("~nothing like this").is_empty());
}

#[test]
fn event_log_records_what_fired() {
    let mut h = form();
    h.type_into("#name", "z");
    h.press("Save");
    let kinds: Vec<(&str, &str)> = h.log.iter().map(|e| (e.id.as_str(), e.kind.as_str())).collect();
    assert_eq!(kinds, [("name", "change"), ("act", "activate")]);
}

struct Files {
    rows: Vec<&'static str>,
    sel: usize,
}

impl App for Files {
    fn view(&self) -> El {
        row([list("files", self.rows.clone()).weight(1), panel("detail", [text(format!("selected: {}", self.rows[self.sel]))]).weight(2)])
    }
    fn on(&mut self, e: &UiEvent, _: &mut Cx) {
        if e.is("files", "change") {
            self.sel = e.index as usize;
        }
    }
}

#[test]
fn list_and_detail_follow_the_selection() {
    let mut h = test(Files { rows: vec!["alpha", "beta", "gamma"], sel: 0 }, 40, 6);
    h.expect_text("selected: alpha").unwrap();
    assert!(h.press("gam")); // substring of a row
    h.expect_text("selected: gamma").unwrap();
    assert!(h.key("up"));
    h.expect_text("selected: beta").unwrap();
}
