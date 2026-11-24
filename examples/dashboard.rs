//! A dashboard: a native `sysbar`, tabs with pages, and a timer. `cargo run --example dashboard`
use lazyui::prelude::*;

#[derive(Default)]
pub struct Dash {
    pub ticks: u32,
}

impl App for Dash {
    fn view(&self) -> El {
        col([
            widget("sysbar").fixed(1),
            tabs("t", ["Overview", "Log"], "tab").fixed(1),
            pages("tab", [
                col([stat("ticks", self.ticks), progress((self.ticks % 11) as f64 / 10.0).set("name", "load")]),
                text("nothing logged yet").dim(),
            ])
            .fill(),
            keybar("tab:focus  ctrl-q:quit"),
        ])
    }
    fn on(&mut self, e: &UiEvent, _: &mut Cx) {
        if e.is("app", "tick") {
            self.ticks += 1;
        }
    }
    fn setup(&mut self, ui: &mut Ui) {
        ui.every(1.0, "tick");
    }
}

fn main() -> std::io::Result<()> {
    run(Dash::default())
}
