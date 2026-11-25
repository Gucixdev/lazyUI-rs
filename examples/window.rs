//! A native Wayland window (xdg-shell) drawn by lazyui, with pointer and keyboard, until its button is clicked or the
//! compositor closes it. Under finedesktop it follows `ctl theme NAME`. `cargo run --example window`
use lazyui::gui::{panel, raster::Raster};
use lazyui::prelude::*;
use lazyui::wl::{self, seat::Seat, window::Window, wire};

fn main() -> std::io::Result<()> {
    let mut conn = wire::connect()?;
    let (reg, globals) = wl::registry(&mut conn)?;
    let mut raster = Raster::system(14.0, None).map_err(std::io::Error::other)?;
    let mut win = Window::open(&mut conn, reg, &globals, "lazyui", "lazyui.example", ((raster.cw * 60) as u32, (raster.ch * 12) as u32))?;
    let mut seat = Seat::bind(&mut conn, reg, &globals, raster.cw, raster.ch)?;
    let mut ui = Ui::new(col([text("lazyui in a native Wayland window"), text("tab moves the focus, enter presses").dim(), buttons("quit", ["quit"])]));
    let follow = panel::Follow::start();
    panel::run(&mut conn, &mut win, &mut seat, &mut ui, &mut raster, follow.as_ref(), &mut |ui, e| {
        if e.id == "quit" {
            ui.quit = true;
        }
    })?;
    win.close(&mut conn)
}
