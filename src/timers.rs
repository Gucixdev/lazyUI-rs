//! Widgets about time: clock (wall time or a bound text), timer (stopwatch / countdown on the Ui clock).

use crate::canvas::{str_width, Canvas, Rect, Style};
use crate::input::{Event, Key, MouseKind};
use crate::native::{Cx, Def, EvCx, Pure, Widget};
use crate::ui::{jstr, Value};
use crate::widgets::Res;

pub static DEFS: &[Def] = &[
    Def { name: "clock", doc: "the time: format=hms|hm|date|full align=left|center|right; bind=KEY shows that text instead of the system time", focus: false, rows: Some(1), live: 1.0, make: || Box::new(Pure(clock)) },
    Def { name: "timer", doc: "stopwatch or countdown: mode=up|down seconds=60 autostart=1; space starts/stops, r resets; bind=KEY gets the seconds shown, emits done", focus: true, rows: Some(1), live: 0.1, make: || Box::new(Timer::default()) },
];

/// (y, m, d, h, min, s) in local time; None where there is no system clock to ask (the browser build).
#[cfg(all(unix, not(target_arch = "wasm32")))]
fn local_now() -> Option<(i32, u32, u32, u32, u32, u32)> {
    unsafe {
        let t = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&t, &mut tm).is_null() {
            return None;
        }
        Some((tm.tm_year + 1900, tm.tm_mon as u32 + 1, tm.tm_mday as u32, tm.tm_hour as u32, tm.tm_min as u32, tm.tm_sec as u32))
    }
}

#[cfg(not(all(unix, not(target_arch = "wasm32"))))]
fn local_now() -> Option<(i32, u32, u32, u32, u32, u32)> {
    None
}

pub fn format_time(fmt: &str, now: Option<(i32, u32, u32, u32, u32, u32)>) -> String {
    let Some((y, mo, d, h, mi, s)) = now else {
        return match fmt {
            "hm" => "--:--".into(),
            "date" => "----/--/--".into(),
            "full" => "----/--/-- --:--:--".into(),
            _ => "--:--:--".into(),
        };
    };
    match fmt {
        "hm" => format!("{h:02}:{mi:02}"),
        "date" => format!("{y:04}-{mo:02}-{d:02}"),
        "full" => format!("{y:04}-{mo:02}-{d:02} {h:02}:{mi:02}:{s:02}"),
        _ => format!("{h:02}:{mi:02}:{s:02}"),
    }
}

fn clock(c: &mut Canvas, r: Rect, cx: &Cx) {
    if r.w < 1 || r.h < 1 {
        return;
    }
    let s = match cx.attr("bind").and_then(|k| cx.model_text(k)) {
        Some(t) => t,
        None => format_time(cx.attr("format").unwrap_or("hms"), local_now()),
    };
    let w = str_width(&s) as i32;
    let x = match cx.attr("align") {
        Some("center") => r.x + (r.w - w).max(0) / 2,
        Some("right") => r.x + (r.w - w).max(0),
        _ => r.x,
    };
    c.text_fit(x, r.y, r.w.min(w), &s, Style::fg(cx.colour("color", cx.theme.accent)).on(cx.theme.bg));
}

#[derive(Default)]
pub struct Timer {
    elapsed: f64,
    running: bool,
    done: bool,
    started: bool,
    total: f64,
    down: bool,
    last_whole: i64,
}

impl Timer {
    fn shown(&self) -> f64 {
        if self.down {
            (self.total - self.elapsed).max(0.0)
        } else {
            self.elapsed
        }
    }
}

pub fn mmss(secs: f64) -> String {
    let s = secs.max(0.0).floor() as i64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{:02}:{:02}", s / 60, s % 60)
    }
}

impl Widget for Timer {
    fn describe(&self, cx: &Cx) -> String {
        format!("{{\"text\":{},\"running\":{},\"done\":{},\"label\":{}}}", jstr(&mmss(self.shown())), self.running, self.done, jstr(&cx.text_attr("label", "")))
    }

    fn act(&mut self, action: &str, _arg: &str, cx: &mut EvCx) -> Res {
        match action {
            "toggle" | "activate" => {
                if self.done {
                    self.elapsed = 0.0;
                    self.done = false;
                }
                self.running = !self.running;
                cx.emit(if self.running { "start" } else { "stop" }, "", -1, self.shown());
                Res::Handled
            }
            "reset" => {
                self.elapsed = 0.0;
                self.done = false;
                self.running = false;
                self.last_whole = -1;
                cx.emit("reset", "", -1, 0.0);
                Res::Handled
            }
            _ => Res::Ignored,
        }
    }

    fn draw(&mut self, c: &mut Canvas, r: Rect, cx: &Cx) {
        self.down = cx.attr("mode") != Some("up");
        self.total = cx.num_attr("seconds", 60.0).max(0.0);
        if !self.started {
            self.started = true;
            self.running = cx.flag("autostart");
        }
        if r.w < 1 || r.h < 1 {
            return;
        }
        let t = cx.theme;
        let col = if self.done { t.err } else if self.running { t.ok } else { t.fg };
        let st = if cx.focused { Style::fg(col).on(t.bg).with(crate::canvas::UNDERLINE) } else { Style::fg(col).on(t.bg) };
        let glyph = if self.done { "!" } else if self.running { if cx.unicode { "▶" } else { ">" } } else if cx.unicode { "⏸" } else { "=" };
        let label = cx.text_attr("label", "");
        let s = if label.is_empty() { format!("{glyph} {}", mmss(self.shown())) } else { format!("{glyph} {} {label}", mmss(self.shown())) };
        c.text_fit(r.x, r.y, r.w, &s, st);
    }

    fn tick(&mut self, dt: f64, cx: &mut EvCx) {
        if !self.running {
            return;
        }
        self.elapsed += dt;
        if self.down && self.elapsed >= self.total {
            self.elapsed = self.total;
            self.running = false;
            self.done = true;
            cx.emit("done", "", -1, self.total);
        }
        let whole = self.shown().floor() as i64;
        if whole != self.last_whole {
            self.last_whole = whole;
            cx.set_bound(Value::Num(whole as f64));
        }
    }

    fn event(&mut self, r: Rect, ev: &Event, cx: &mut EvCx) -> Res {
        match ev {
            Event::Key(Key::Char(' '), m) | Event::Key(Key::Enter, m) if !m.ctrl => {
                if self.done {
                    self.elapsed = 0.0;
                    self.done = false;
                }
                self.running = !self.running;
                cx.emit(if self.running { "start" } else { "stop" }, "", -1, self.shown());
                Res::Handled
            }
            Event::Mouse(MouseKind::Down(0), x, y, _) if r.contains(*x, *y) => {
                self.running = !self.running;
                cx.emit(if self.running { "start" } else { "stop" }, "", -1, self.shown());
                Res::Handled
            }
            Event::Key(Key::Char('r'), m) if !m.ctrl && !m.alt => {
                self.elapsed = 0.0;
                self.done = false;
                self.running = false;
                self.last_whole = -1;
                cx.set_bound(Value::Num(self.shown().floor()));
                cx.emit("reset", "", -1, 0.0);
                Res::Handled
            }
            _ => Res::Ignored,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats() {
        assert_eq!(format_time("hms", Some((2026, 11, 22, 9, 5, 7))), "09:05:07");
        assert_eq!(format_time("date", Some((2026, 11, 22, 9, 5, 7))), "2026-11-22");
        assert_eq!(format_time("hm", None), "--:--");
        assert_eq!(mmss(59.9), "00:59");
        assert_eq!(mmss(3725.0), "1:02:05");
        assert_eq!(mmss(-4.0), "00:00");
    }
}

#[cfg(test)]
mod widget_tests {
    use crate::headless::{run_script, snapshot};
    use crate::ui::Ui;

    #[test]
    fn clock_shows_bound_text_and_is_a_slow_live_widget() {
        let mut ui = Ui::from_spec("clock bind=t align=right").unwrap();
        ui.set_text("t", "12:34:56");
        assert_eq!(snapshot(&mut ui, 12, 1).trim_end(), "    12:34:56");
        assert!(!ui.animating(), "one frame a second is a deadline, not a busy loop");
        assert!(ui.next_deadline().is_some_and(|d| d <= 1.0 + 1e-9));
        let mut c = Ui::from_spec("clock").unwrap();
        snapshot(&mut c, 8, 1);
        assert_eq!(c.live(), Some(1.0));
        let mut sys = Ui::from_spec("clock format=hm").unwrap();
        assert_eq!(snapshot(&mut sys, 8, 1).trim_end().len(), 5);
    }

    #[test]
    fn timer_counts_down_emits_done_and_resets() {
        let mut ui = Ui::from_spec("timer id=tm seconds=3 bind=left").unwrap();
        run_script(&mut ui, "size 20 1").unwrap();
        assert!(snapshot(&mut ui, 20, 1).contains("00:03"));
        run_script(&mut ui, "key space\ntick 1.1\nexpect event tm start").unwrap();
        let s = snapshot(&mut ui, 20, 1);
        assert!(s.contains("00:01") || s.contains("00:02"), "{s}");
        assert_eq!(ui.text_of("left"), "1", "the bound key holds the whole seconds left");
        run_script(&mut ui, "tick 5\nexpect event tm done").unwrap();
        assert!(snapshot(&mut ui, 20, 1).contains("00:00"));
        run_script(&mut ui, "key r\nexpect event tm reset").unwrap();
        assert!(snapshot(&mut ui, 20, 1).contains("00:03"));
        let mut up = Ui::from_spec("timer mode=up autostart=1").unwrap();
        run_script(&mut up, "size 20 1").unwrap();
        snapshot(&mut up, 20, 1);
        run_script(&mut up, "tick 65").unwrap();
        assert!(snapshot(&mut up, 20, 1).contains("01:05"));
    }
}
