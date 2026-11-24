//! The counter in three languages, switched while it runs. `cargo run --example counter_i18n`
//! Texts are keys (`tr("title")`) resolved by the catalog; events use ids and `index`, never the translated label.
use lazyui::prelude::*;

pub const LOCALES: [&str; 3] = ["en", "pl", "de"];

pub struct Counter(pub i64);

impl App for Counter {
    fn view(&self) -> El {
        col([
            text(tr("title")).accent(),
            text(trn("count", self.0)),
            buttons("b", [tr("plus")]),
            text(tr("lang")).dim(),
            buttons("lang", LOCALES),
        ])
    }
    fn on(&mut self, e: &UiEvent, cx: &mut Cx) {
        if e.activated("b") {
            self.0 += 1;
        } else if e.activated("lang") {
            cx.set_locale(LOCALES[e.index as usize]);
        }
    }
    fn setup(&mut self, ui: &mut Ui) {
        ui.set_catalog(Some(Catalog::from_tsv(include_str!("counter_i18n.tsv")).expect("catalog")));
    }
}

fn main() -> std::io::Result<()> {
    run(Counter(0))
}
