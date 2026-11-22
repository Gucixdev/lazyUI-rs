//! Size sweep, garbage bytes and spec fuzz: nothing may panic, whatever the terminal or the input does.
use lazyui::canvas::{diff, Caps, Canvas};
use lazyui::input::Parser;
use lazyui::ui::{parse_spec, Ui};

const DEMO: &str = include_str!("../examples/demo.lui");

fn rng(seed: &mut u64) -> u64 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    *seed
}

#[test]
fn every_size_renders_and_diffs() {
    let mut ui = Ui::from_spec(DEMO).unwrap();
    ui.set("cpu", 50.0);
    for caps in [Caps::FULL, Caps::ASCII] {
        for h in 0..32 {
            let mut prev: Option<Canvas> = None;
            for w in 0..90 {
                let mut c = Canvas::new(w, h, caps);
                ui.render(&mut c);
                let mut out = String::new();
                diff(prev.as_ref(), &c, &mut out);
                prev = Some(c);
            }
        }
    }
}

#[test]
fn garbage_input_never_panics_and_ui_survives() {
    let mut ui = Ui::from_spec(DEMO).unwrap();
    let mut p = Parser::new();
    let mut seed = 0x9e3779b97f4a7c15u64;
    let mut c = Canvas::new(40, 12, Caps::FULL);
    for round in 0..3000 {
        let n = (rng(&mut seed) % 24) as usize;
        let bytes: Vec<u8> = (0..n).map(|_| if rng(&mut seed) % 3 == 0 { 0x1b } else { rng(&mut seed) as u8 }).collect();
        for ev in p.feed(&bytes) {
            ui.dispatch(&ev);
        }
        if round % 7 == 0 {
            for ev in p.flush() {
                ui.dispatch(&ev);
            }
        }
        if round % 100 == 0 {
            ui.render(&mut c);
        }
        ui.take_events();
    }
    ui.render(&mut c);
}

#[test]
fn spec_fuzz_returns_errors_not_panics() {
    let mut seed = 42u64;
    let words = ["col", "row", "panel", "text", "list", "pages", "  ", "    ", "\t", "size=3", "size=fill:2", "id=a", "items=\"a,b", "\"", "=", "\n", "#", "keybar", "x=y"];
    let mut ok = 0;
    for _ in 0..5000 {
        let n = rng(&mut seed) % 14;
        let s: String = (0..n).map(|_| words[(rng(&mut seed) % words.len() as u64) as usize]).collect::<Vec<_>>().join(if rng(&mut seed) % 2 == 0 { " " } else { "\n" });
        if let Ok(root) = parse_spec(&s) {
            ok += 1;
            let mut ui = Ui::new(root);
            let mut c = Canvas::new(20, 6, Caps::ASCII);
            ui.render(&mut c);
        }
    }
    assert!(ok > 0, "the fuzzer should hit some valid specs too");
}
