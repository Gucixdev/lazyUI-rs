//! A layer-shell strip at the bottom-left of the screen for three seconds, drawn by lazyui. Needs a Wayland compositor
//! with zwlr_layer_shell_v1. `cargo run --example panel`
use lazyui::gui::{panel, raster::Raster};
use lazyui::prelude::*;
use lazyui::wl::{self, panel::*, wire};

fn main() -> std::io::Result<()> {
    let mut conn = wire::connect()?;
    let (reg, globals) = wl::registry(&mut conn)?;
    let mut raster = Raster::system(14.0, None).map_err(std::io::Error::other)?;
    let spec = Spec {
        namespace: "lazyui-example".into(),
        layer: LAYER_TOP,
        anchor: ANCHOR_BOTTOM | ANCHOR_LEFT,
        size: ((raster.cw * 28) as u32, (raster.ch * 2) as u32),
        exclusive: 0,
    };
    let mut p = Panel::open(&mut conn, reg, &globals, &spec)?;
    let mut ui = Ui::new(col([text("lazyui in a layer-shell panel"), text("closes in 3 s").dim()]));
    panel::show(&mut p, &mut conn, &mut ui, &mut raster)?;
    std::thread::sleep(std::time::Duration::from_secs(3));
    p.close(&mut conn)
}
