//! A layer-shell strip at the bottom-left of the screen for ten seconds, drawn by lazyui; under finedesktop it follows `ctl theme NAME`. Needs a Wayland compositor
//! with zwlr_layer_shell_v1. `cargo run --example panel`
use lazyui::gui::{panel, raster::Raster};
use lazyui::prelude::*;
use lazyui::widgets::theme::Theme;
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
    let mut ui = Ui::new(col([text("lazyui in a layer-shell panel"), text("closes in 10 s").dim()]));
    panel::show(&mut p, &mut conn, &mut ui, &mut raster)?;
    let (tx, rx) = std::sync::mpsc::channel();
    if let Some(sock) = lazyui::fdctl::socket() {
        if let Ok(sub) = lazyui::fdctl::subscribe(&sock, &["theme"]) {
            std::thread::spawn(move || sub.for_each(|e| drop(tx.send(e))));
        }
    }
    let end = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while let Some(left) = end.checked_duration_since(std::time::Instant::now()) {
        let Ok(e) = rx.recv_timeout(left) else { break };
        if let Some(t) = panel::theme_of(&e).and_then(|n| Theme::from_finedesktop(n).ok()) {
            panel::apply_theme(&mut ui, &mut raster, t);
            panel::show(&mut p, &mut conn, &mut ui, &mut raster)?;
        }
    }
    p.close(&mut conn)
}
