#[cfg(not(target_arch = "wasm32"))]
pub mod app;
pub mod attrs;
pub mod build;
pub mod components;
pub mod dialog;
pub mod dnd;
#[cfg(not(target_arch = "wasm32"))]
pub mod fdctl;
pub mod grid;
#[cfg(feature = "gui")]
pub mod gui;
pub mod headless;
pub mod i18n;
pub mod input;
pub mod layout;
pub mod lists;
pub mod media;
pub mod menu;
pub mod menubar;
pub mod pathbar;
pub mod splitter;
pub mod native;
pub mod plan;
pub mod prelude;
#[cfg(not(target_arch = "wasm32"))]
pub mod runtime;
pub mod serve;
pub mod sys;
#[cfg(not(target_arch = "wasm32"))]
pub mod term;
pub mod ui;
#[cfg(feature = "web")]
pub mod web;
#[cfg(not(target_arch = "wasm32"))]
pub mod wl;
pub mod fx;
pub mod widgets;

// the old flat paths (`lazyui::canvas`, `lazyui::gfx`, ...) keep working
pub use fx::{anim, fancy, gfx, motion};
#[cfg(not(target_arch = "wasm32"))]
pub use widgets::session;
pub use widgets::{bars, canvas, charts, maps, pickers, theme, timers, world, worlddata};
