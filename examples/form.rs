//! A form with a reusable component. Two `Field`s share one definition and keep their own ids (`first.v`, `last.v`).
//! `cargo run --example form`
use lazyui::prelude::*;

/// A labelled input. Names starting with `.` are local: `.at("first")` turns `.v` into `first.v`.
pub struct Field(pub &'static str);

impl Component for Field {
    fn view(&self) -> El {
        row([text(self.0).dim().fixed(8), input(".v", "")])
    }
}

#[derive(Default)]
pub struct Form {
    pub first: String,
    pub last: String,
    pub saved: Vec<String>,
}

impl App for Form {
    fn view(&self) -> El {
        col([
            Field("first").at("first"),
            Field("last").at("last"),
            buttons("act", ["Save", "Clear"]),
            text(format!("saved: {}", self.saved.join(", "))),
        ])
    }
    fn on(&mut self, e: &UiEvent, _: &mut Cx) {
        match (e.local("first"), e.local("last")) {
            (Some("v"), _) if e.kind == "change" => self.first = e.text.clone(),
            (_, Some("v")) if e.kind == "change" => self.last = e.text.clone(),
            _ if e.activated("act") && e.index == 0 => self.saved.push(format!("{} {}", self.first, self.last).trim().to_string()),
            _ if e.activated("act") => self.saved.clear(),
            _ => {}
        }
    }
}

fn main() -> std::io::Result<()> {
    run(Form::default())
}
