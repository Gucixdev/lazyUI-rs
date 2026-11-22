//! lazyui as a library: an Elm-shaped program with a slow effect. `cargo run --example counter`
//! keys: + / - change the count, l loads a "slow" value on a thread, ctrl-q quits.

use lazyui::runtime::{run, Cx, Latest, Opts, Program};
use lazyui::ui::{buttons, col, panel, progress, text, El, Ui, UiEvent};

pub enum Msg {
    Inc,
    Dec,
    Load,
    Loaded(u64, i64),
}

pub struct Counter {
    pub n: i64,
    pub status: String,
    pub latest: Latest,
}

impl Counter {
    pub fn new() -> Counter {
        Counter { n: 0, status: "idle".into(), latest: Latest::new() }
    }
}

impl Program for Counter {
    type Msg = Msg;

    fn update(&mut self, msg: Msg, cx: &mut Cx<Msg>) {
        match msg {
            Msg::Inc => self.n += 1,
            Msg::Dec => self.n -= 1,
            Msg::Load => {
                let token = self.latest.next();
                self.status = "loading".into();
                cx.spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(300));
                    Msg::Loaded(token, 42)
                });
            }
            Msg::Loaded(token, v) => {
                // an older request finishing after a newer one started is dropped
                if self.latest.is_current(token) {
                    self.n = v;
                    self.status = "loaded".into();
                }
            }
        }
    }

    fn view(&self) -> El {
        col([
            panel("counter", [text(format!("count: {}   ({})", self.n, self.status)), progress((self.n.rem_euclid(101)) as f64 / 100.0)]).size("5"),
            buttons("act", &["+", "-", "load"]),
        ])
    }

    fn event(&mut self, e: &UiEvent) -> Option<Msg> {
        match (e.id.as_str(), e.kind.as_str(), e.text.as_str()) {
            ("act", "activate", "+") => Some(Msg::Inc),
            ("act", "activate", "-") => Some(Msg::Dec),
            ("act", "activate", "load") | ("app", "load", _) => Some(Msg::Load),
            ("app", "inc", _) => Some(Msg::Inc),
            ("app", "dec", _) => Some(Msg::Dec),
            _ => None,
        }
    }

    fn setup(&mut self, ui: &mut Ui) {
        ui.hotkey("+", "inc");
        ui.hotkey("-", "dec");
        ui.hotkey("l", "load");
        ui.hotkey("ctrl-q", "quit");
    }
}

fn main() -> std::io::Result<()> {
    run(&mut Counter::new(), Opts::default())
}
