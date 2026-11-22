//! What `lazyui run` does around a spec, shared by every backend (terminal, window): the stdin model protocol,
//! hot reload of the spec file, and JSON events on stdout.

use crate::ui::{Ui, UiEvent};
use std::io::{BufRead, Write};
use std::time::Duration;

#[derive(Debug, Clone)]
pub enum SpecMsg {
    /// a line from stdin: `key=value`, `list:ID=a,b,c` or `quit`
    Line(String),
    /// the spec file changed on disk
    Reload,
}

/// Start the stdin reader (unless stdin is a terminal) and the file watcher. `send` must wake the backend's loop.
pub fn spawn_feeds(path: &str, watch: bool, send: impl Fn(SpecMsg) -> bool + Send + Clone + 'static) {
    if unsafe { libc::isatty(0) } != 1 {
        let send = send.clone();
        std::thread::spawn(move || {
            for line in std::io::stdin().lock().lines().map_while(Result::ok) {
                if !send(SpecMsg::Line(line)) {
                    break;
                }
            }
        });
    }
    if watch {
        let path = path.to_string();
        std::thread::spawn(move || {
            let mt = |p: &str| std::fs::metadata(p).and_then(|m| m.modified()).ok();
            let mut last = mt(&path);
            loop {
                std::thread::sleep(Duration::from_millis(250));
                let now = mt(&path);
                if now.is_some() && now != last {
                    last = now;
                    if !send(SpecMsg::Reload) {
                        break;
                    }
                }
            }
        });
    }
}

/// Apply one message. Returns true when the program should quit.
pub fn handle(ui: &mut Ui, path: &str, msg: SpecMsg) -> bool {
    match msg {
        SpecMsg::Line(l) => {
            let l = l.trim();
            if l == "quit" {
                return true;
            }
            if let Some(rest) = l.strip_prefix("list:") {
                if let Some((id, items)) = rest.split_once('=') {
                    ui.set_list(id, items.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect());
                }
            } else if let Some((k, v)) = l.split_once('=') {
                ui.set_auto(k.trim(), v);
            }
        }
        SpecMsg::Reload => {
            if let Ok(src) = std::fs::read_to_string(path) {
                if let Err(e) = ui.reload_spec(&src) {
                    ui.emit("app", "spec-error", &e, 0.0);
                }
            }
        }
    }
    false
}

/// One JSON line on stdout per event; the UI itself is on the terminal or in a window, the pipes carry data.
pub fn print_event(e: &UiEvent) {
    let stdout = std::io::stdout();
    let mut o = stdout.lock();
    let _ = writeln!(o, "{}", e.to_json());
    let _ = o.flush();
}
