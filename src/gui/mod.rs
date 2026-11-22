//! The window backend, next to the terminal one: the same `Ui`, drawn with a real font.
//!
//! - `raster`: cells to pixels (CPU, deterministic, also the PNG snapshot);
//! - `present`: pixels to the screen, either through shared memory (CPU, works everywhere) or wgpu (GPU, feature `gpu`);
//! - `tune`: picks and switches between the two from measured frame cost, and slows animation when frames are slow;
//! - `window`: winit loop and input mapping, `drive_gui` is the counterpart of `runtime::drive`.
pub mod png;
mod present;
pub mod raster;
pub mod tune;
pub mod window;

pub use tune::{Path, Render};
pub use window::{drive_gui, GuiOpts};
