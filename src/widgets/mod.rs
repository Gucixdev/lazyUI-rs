//! The widgets and what they stand on. `widgets.rs` (the `Widget` base types) is flattened into this module, so
//! `lazyui::widgets::Res` stays what it was; the rest are submodules, re-exported at the crate root by `lib.rs`.
mod widgets;
pub use widgets::*;

pub mod bars;
pub mod canvas;
pub mod charts;
pub mod maps;
pub mod pickers;
#[cfg(not(target_arch = "wasm32"))]
pub mod session;
pub mod theme;
pub mod timers;
pub mod world;
pub mod worlddata;
