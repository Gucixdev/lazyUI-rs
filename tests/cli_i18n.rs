//! `lazyui snap --catalog --locale`: the binary translates `@key` texts, falls back, and rejects bad input.
use std::process::Command;

fn files() -> (String, String) {
    let d = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"));
    let spec = d.join("i18n.lui");
    let tsv = d.join("i18n.tsv");
    std::fs::write(&spec, "col\n  text text=@hi\n  text text=@missing\n  text text=@@raw\n").unwrap();
    std::fs::write(&tsv, "key\ten\tpl\nhi\tHello\tCzesc\n").unwrap();
    (spec.display().to_string(), tsv.display().to_string())
}

fn snap(args: &[&str]) -> (bool, String, String) {
    let o = Command::new(env!("CARGO_BIN_EXE_lazyui")).arg("snap").args(args).output().unwrap();
    (o.status.success(), String::from_utf8_lossy(&o.stdout).into(), String::from_utf8_lossy(&o.stderr).into())
}

#[test]
fn locale_switches_text_and_missing_keys_show_their_name() {
    let (spec, tsv) = files();
    let (ok, out, err) = snap(&[&spec, "--catalog", &tsv, "--locale", "pl", "--size", "20x4"]);
    assert!(ok, "{err}");
    assert!(out.contains("Czesc") && out.contains("missing") && out.contains("@raw") && !out.contains("@@"), "{out}");
    let (_, en, _) = snap(&[&spec, "--catalog", &tsv, "--size", "20x4"]);
    assert!(en.contains("Hello"), "{en}");
    let (_, de, _) = snap(&[&spec, "--catalog", &tsv, "--locale", "de", "--size", "20x4"]);
    assert!(de.contains("Hello"), "unknown language falls back to en: {de}");
}

#[test]
fn bad_catalog_or_locale_without_catalog_fails_clearly() {
    let (spec, _) = files();
    let (ok, _, err) = snap(&[&spec, "--locale", "pl"]);
    assert!(!ok && err.contains("--catalog"), "{err}");
    let (ok, _, err) = snap(&[&spec, "--catalog", "/nonexistent.tsv"]);
    assert!(!ok && err.contains("nonexistent"), "{err}");
}
