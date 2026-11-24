//! Every example runs headless: the same file the user runs with `cargo run --example`, driven by meaning.
#[path = "../examples/counter_app.rs"]
#[allow(dead_code)]
mod counter_app;
#[path = "../examples/counter_i18n.rs"]
#[allow(dead_code)]
mod counter_i18n;
#[path = "../examples/dashboard.rs"]
#[allow(dead_code)]
mod dashboard;
#[path = "../examples/files.rs"]
#[allow(dead_code)]
mod files;
#[path = "../examples/form.rs"]
#[allow(dead_code)]
mod form;

use lazyui::prelude::*;

#[test]
fn counter_app_counts() {
    let mut h = test(counter_app::Counter(0), 40, 5);
    h.expect_text("n = 0").unwrap();
    h.press("+");
    h.press("+");
    h.press("-");
    h.expect_text("n = 1").unwrap();
}

#[test]
fn form_components_keep_separate_ids() {
    let mut h = test(form::Form::default(), 40, 6);
    assert!(h.type_into("#first.v", "Ada"));
    assert!(h.type_into("#last.v", "Lovelace"));
    h.press("Save");
    h.expect_text("saved: Ada Lovelace").unwrap();
    h.press("Clear");
    h.expect_no_text("Ada Lovelace").unwrap();
    let t = h.tree();
    assert!(t.contains("\"id\":\"first.v\"") && t.contains("\"id\":\"last.v\""), "{t}");
}

#[test]
fn files_detail_follows_selection() {
    let mut h = test(files::Files::default(), 50, 8);
    h.expect_text("selected: README.md").unwrap();
    h.press("lib.rs");
    h.expect_text("selected: src/lib.rs").unwrap();
    h.expect_text("the library").unwrap();
}

#[test]
fn dashboard_ticks_and_has_sysbar_and_tabs() {
    let mut h = test(dashboard::Dash::default(), 60, 8);
    h.expect_text("ticks").unwrap();
    assert!(h.tree().contains("\"kind\":\"sysbar\""));
    h.tick(1.0);
    h.tick(1.0);
    assert_eq!(h.program.0.ticks, 2);
    h.expect_text("Overview").unwrap();
    h.press("Log");
    h.expect_text("nothing logged yet").unwrap();
}

#[test]
fn counter_i18n_switches_language_while_running() {
    let mut h = test(counter_i18n::Counter(0), 40, 8);
    h.expect_text("0 clicks").unwrap();
    h.expect_text("Counter").unwrap();
    h.press("add");
    h.expect_text("1 click").unwrap();
    h.press("pl");
    h.expect_text("Licznik").unwrap();
    h.expect_text("1 klik").unwrap();
    h.press("#b"); // by id: the label is "dodaj" now
    h.press("#b");
    h.expect_text("3 kliki").unwrap(); // Polish few
    h.press("de");
    h.expect_text("Zaehler").unwrap();
    h.expect_text("3 Klicks").unwrap();
    assert_eq!(h.program.0 .0, 3); // state untouched by the language
}
