//! List and detail: the right side is a function of the selected row. `cargo run --example files`
use lazyui::prelude::*;

pub struct Files {
    pub rows: Vec<(&'static str, &'static str)>,
    pub sel: usize,
}

impl Default for Files {
    fn default() -> Files {
        Files { rows: vec![("README.md", "start here"), ("Cargo.toml", "package manifest"), ("src/lib.rs", "the library")], sel: 0 }
    }
}

impl App for Files {
    fn view(&self) -> El {
        let (name, note) = self.rows[self.sel];
        row([
            panel("files", [list("files", self.rows.iter().map(|r| r.0))]).weight(1),
            panel("detail", [text(format!("selected: {name}")).accent(), text(note).dim()]).weight(2),
        ])
    }
    fn on(&mut self, e: &UiEvent, _: &mut Cx) {
        if e.is("files", "change") {
            self.sel = e.index.max(0) as usize % self.rows.len();
        }
    }
}

fn main() -> std::io::Result<()> {
    run(Files::default())
}
