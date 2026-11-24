//! The short way to a program: one struct, `view`, and an `on` that sees every UI event. No message enum.
//! Use `runtime::Program` when you want typed messages; both run the same loop.
//!
//! ```no_run
//! use lazyui::prelude::*;
//! struct Counter(i64);
//! impl App for Counter {
//!     fn view(&self) -> El { col([text(format!("n = {}", self.0)), button("+").on("inc")]) }
//!     fn on(&mut self, e: &UiEvent, _: &mut Cx) { if e.activated("inc") { self.0 += 1 } }
//! }
//! fn main() -> std::io::Result<()> { run(Counter(0)) }
//! ```

use crate::runtime::{Cx as RtCx, Harness, Opts, Program};
use crate::ui::{El, Ui, UiEvent};

/// The context an `App` gets: `cx.spawn(|| UiEvent::of("loaded", "done", "42"))` runs on a thread and comes back to `on`,
/// `cx.quit()` ends the loop.
pub type Cx = RtCx<UiEvent>;

pub trait App {
    fn view(&self) -> El;
    /// Every named event: widget events `(id, kind)`, hotkeys `("app", name)`, effect results, `("app", "quit")`.
    fn on(&mut self, _e: &UiEvent, _cx: &mut Cx) {}
    /// Once, before the first frame: register hotkeys and timers (`ui.hotkey("ctrl-s", "save")`, `ui.every(1.0, "tick")`).
    fn setup(&mut self, _ui: &mut Ui) {}
    /// Once, when the loop starts: start background work.
    fn init(&mut self, _cx: &mut Cx) {}
}

/// Adapter from `App` to `Program` (the message is the event itself).
pub struct Sketch<A>(pub A);

impl<A: App> Program for Sketch<A> {
    type Msg = UiEvent;
    fn update(&mut self, m: UiEvent, cx: &mut Cx) {
        self.0.on(&m, cx);
    }
    fn view(&self) -> El {
        self.0.view()
    }
    fn event(&mut self, e: &UiEvent) -> Option<UiEvent> {
        Some(e.clone())
    }
    fn setup(&mut self, ui: &mut Ui) {
        self.0.setup(ui);
    }
    fn init(&mut self, cx: &mut Cx) {
        self.0.init(cx);
    }
}

/// Run an `App` on the terminal (`Ctrl-Q` always quits).
pub fn run<A: App>(app: A) -> std::io::Result<()> {
    let mut s = Sketch(app);
    crate::runtime::run(&mut s, Opts::default())
}

/// An `App` on a virtual screen: for tests and agents. See `Harness`.
pub fn test<A: App>(app: A, w: i32, h: i32) -> Harness<Sketch<A>> {
    Harness::new(Sketch(app), w, h)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::Event;
    use crate::prelude::*;

    struct Counter {
        n: i64,
        loaded: bool,
    }
    impl App for Counter {
        fn view(&self) -> El {
            col([text(format!("n = {}", self.n)), buttons("b", ["+", "-", "load"]), text(if self.loaded { "loaded" } else { "-" }).dim()])
        }
        fn on(&mut self, e: &UiEvent, cx: &mut Cx) {
            if e.is("app", "inc") {
                self.n += 1;
            } else if e.activated("b") && e.index == 2 {
                cx.spawn(|| UiEvent::of("fx", "done", "42"));
            } else if e.is("fx", "done") {
                self.n = e.text.parse().unwrap();
                self.loaded = true;
            } else if e.activated("b") {
                self.n += if e.index == 0 { 1 } else { -1 };
            }
        }
        fn setup(&mut self, ui: &mut Ui) {
            ui.hotkey("+", "inc");
        }
    }

    #[test]
    fn app_runs_without_a_message_enum() {
        let mut h = test(Counter { n: 0, loaded: false }, 20, 5);
        h.send(Event::ch('+'));
        h.send(Event::ch('+'));
        assert_eq!(h.program.0.n, 2);
        assert!(h.frame().contains("n = 2"));
    }

    #[test]
    fn button_events_and_effect_results_reach_on() {
        let mut h = test(Counter { n: 0, loaded: false }, 20, 5);
        let _ = h.frame();
        assert!(h.ui.act("b", "activate", "1"));
        h.pump();
        assert_eq!(h.program.0.n, -1);
        assert!(h.ui.act("b", "activate", "2"));
        h.pump();
        h.wait_msg();
        assert_eq!((h.program.0.n, h.program.0.loaded), (42, true));
        assert!(h.frame().contains("loaded"));
    }

    #[test]
    fn event_helpers() {
        let e = UiEvent::of("form.name", "change", "x");
        assert!(e.is("form.name", "change") && !e.activated("form.name"));
        assert_eq!(e.local("form"), Some("name"));
        assert_eq!(e.local("fo"), None);
        assert_eq!(e.local("other"), None);
    }
}
