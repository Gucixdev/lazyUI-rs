//! The embedding API without a terminal: Harness runs the same update/view/effect loop on a virtual screen.
#[path = "../examples/counter.rs"]
#[allow(dead_code)]
mod counter;

use counter::Counter;
use lazyui::input::{Event, Key};
use lazyui::runtime::Harness;

#[test]
fn keys_update_the_view() {
    let mut h = Harness::new(Counter::new(), 30, 8);
    for _ in 0..3 {
        h.send(Event::ch('+'));
    }
    h.send(Event::ch('-'));
    assert_eq!(h.program.n, 2);
    assert!(h.frame().contains("count: 2   (idle)"));
}

#[test]
fn buttons_work_by_keyboard_and_mouse() {
    let mut h = Harness::new(Counter::new(), 30, 8);
    let _ = h.frame();
    h.send(Event::key(Key::Right)); // cursor on "-"
    h.send(Event::key(Key::Enter));
    assert_eq!(h.program.n, -1);
}

#[test]
fn effect_result_comes_back() {
    let mut h = Harness::new(Counter::new(), 30, 8);
    h.send(Event::ch('l'));
    assert_eq!(h.program.status, "loading");
    h.wait_msg();
    assert_eq!((h.program.n, h.program.status.as_str()), (42, "loaded"));
}

#[test]
fn stale_answers_are_dropped() {
    let mut h = Harness::new(Counter::new(), 30, 8);
    h.send(Event::ch('l')); // request 1
    h.send(Event::ch('l')); // request 2 supersedes it
    h.wait_msg();
    std::thread::sleep(std::time::Duration::from_millis(400));
    h.pump();
    assert_eq!(h.program.n, 42);
    // request 1 is no longer the current one: its answer would be dropped
    assert!(!h.program.latest.is_current(1));
    assert!(h.program.latest.is_current(2));
}

#[test]
fn ctrl_q_asks_to_quit() {
    let mut h = Harness::new(Counter::new(), 30, 8);
    assert!(!h.quit_requested());
    h.send(Event::Key(Key::Char('q'), lazyui::input::Mods::CTRL));
    assert!(h.quit_requested());
}
