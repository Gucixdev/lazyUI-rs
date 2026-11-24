//! The smallest app: state, view, events. `cargo run --example counter_app` (ctrl-q quits). This file is the README's first example.
use lazyui::prelude::*;

pub struct Counter(pub i64);
impl App for Counter {
    fn view(&self) -> El {
        col([text(format!("n = {}", self.0)).accent(), buttons("b", ["+", "-"]), keybar("tab:focus  ctrl-q:quit")])
    }
    fn on(&mut self, e: &UiEvent, _: &mut Cx) {
        if e.activated("b") { self.0 += if e.index == 0 { 1 } else { -1 } }
    }
}
fn main() -> std::io::Result<()> { run(Counter(0)) }
