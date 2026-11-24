//! `use lazyui::prelude::*;` is all a screen needs: elements, the chain methods, `Component`, `App`, `run`, `test`.

pub use crate::build::{button, buttons, input, list, meter, pages, stat, tabs, widget, Component};
pub use crate::ui::{col, keybar, panel, progress, row, spacer, text, El, Ui, UiEvent};

#[cfg(not(target_arch = "wasm32"))]
pub use crate::app::{run, test, App, Cx, Sketch};
#[cfg(not(target_arch = "wasm32"))]
pub use crate::runtime::{Harness, Latest, Program};
