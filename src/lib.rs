#[cfg(not(target_arch = "wasm32"))]
pub mod app;
pub mod build;
pub mod components;
#[cfg(feature = "gui")]
pub mod gui;
pub mod headless;
pub mod input;
pub mod layout;
pub mod lists;
pub mod native;
pub mod prelude;
#[cfg(not(target_arch = "wasm32"))]
pub mod runtime;
pub mod sys;
#[cfg(not(target_arch = "wasm32"))]
pub mod term;
pub mod ui;
#[cfg(feature = "web")]
pub mod web;
pub mod fx;
pub mod widgets;

// the old flat paths (`lazyui::canvas`, `lazyui::gfx`, ...) keep working
pub use fx::{anim, fancy, gfx};
#[cfg(not(target_arch = "wasm32"))]
pub use widgets::session;
pub use widgets::{bars, canvas, charts, maps, pickers, theme, timers, world, worlddata};
