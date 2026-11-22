//! `sysbar`: one line of machine vitals - cpu, mem, swap, disk, net, ping, fps.
//!
//! The widget keeps its numbers in the model under `sys.cpu sys.mem sys.swap sys.disk` (percent), `sys.rx sys.tx`
//! (bytes per second), `sys.ping` (milliseconds, negative when unknown) and `sys.fps`. On a native build it samples
//! `/proc` itself once a second; where there is no `/proc` (the browser) the page sets those keys and the bar only
//! shows them, so any program can feed or read the same keys.

use crate::canvas::{str_width, Canvas, Rect, Style};
use crate::gfx::level;
use crate::native::{Cx, Def, EvCx, Widget};
use crate::ui::{jstr, Value};

pub static DEFS: &[Def] = &[Def {
    name: "sysbar",
    doc: "vitals in one line: items=cpu,mem,swap,disk,net,ping,fps every=1 (seconds) disk=/ ping=host:port|off align=left|right; fills model keys sys.*",
    focus: false,
    rows: Some(1),
    live: 1.0,
    make: || Box::new(Sysbar::default()),
}];

const ALL: &str = "cpu,mem,swap,disk,net,ping,fps";

// ---- parsers: plain text in, numbers out, so they are testable without a machine ------------------------------

/// `cpu  user nice system idle iowait irq softirq steal ...` -> (busy jiffies, total jiffies).
pub fn parse_cpu(stat: &str) -> Option<(u64, u64)> {
    let line = stat.lines().find(|l| l.starts_with("cpu "))?;
    let v: Vec<u64> = line.split_whitespace().skip(1).take(8).filter_map(|x| x.parse().ok()).collect();
    if v.len() < 4 {
        return None;
    }
    let total: u64 = v.iter().sum();
    let idle = v[3] + v.get(4).copied().unwrap_or(0);
    Some((total.saturating_sub(idle), total))
}

/// `/proc/meminfo` -> (memory used %, swap used %).
pub fn parse_mem(info: &str) -> Option<(f64, f64)> {
    let get = |k: &str| -> Option<f64> { info.lines().find_map(|l| l.strip_prefix(k)?.trim_start_matches(':').split_whitespace().next()?.parse().ok()) };
    let (total, avail) = (get("MemTotal")?, get("MemAvailable").or_else(|| get("MemFree"))?);
    let (st, sf) = (get("SwapTotal").unwrap_or(0.0), get("SwapFree").unwrap_or(0.0));
    let mem = if total > 0.0 { (total - avail) / total * 100.0 } else { 0.0 };
    let swap = if st > 0.0 { (st - sf) / st * 100.0 } else { 0.0 };
    Some((mem.clamp(0.0, 100.0), swap.clamp(0.0, 100.0)))
}

/// `/proc/net/dev` -> (received bytes, sent bytes) summed over every interface but `lo`.
pub fn parse_net(dev: &str) -> (u64, u64) {
    let (mut rx, mut tx) = (0u64, 0u64);
    for l in dev.lines().skip(2) {
        let Some((name, rest)) = l.split_once(':') else { continue };
        if name.trim() == "lo" {
            continue;
        }
        let f: Vec<u64> = rest.split_whitespace().filter_map(|x| x.parse().ok()).collect();
        if f.len() >= 9 {
            rx += f[0];
            tx += f[8];
        }
    }
    (rx, tx)
}

/// 1234 -> "1K", 1_500_000 -> "1.4M": short enough for a bar.
pub fn rate(b: f64) -> String {
    let b = if b.is_finite() { b.max(0.0) } else { 0.0 };
    if b < 1000.0 {
        format!("{}B", b as u64)
    } else if b < 1024.0 * 1000.0 {
        format!("{:.0}K", b / 1024.0)
    } else if b < 1024.0 * 1024.0 * 1000.0 {
        format!("{:.1}M", b / 1048576.0)
    } else {
        format!("{:.1}G", b / 1073741824.0)
    }
}

// ---- the native sampler -----------------------------------------------------------------------------------------

#[cfg(all(unix, not(target_arch = "wasm32")))]
mod native {
    use super::*;
    use std::sync::atomic::{AtomicI64, Ordering};
    use std::sync::Arc;

    pub struct Sampler {
        cpu: Option<(u64, u64)>,
        net: Option<(u64, u64)>,
        disk: String,
        ping: Arc<AtomicI64>,
    }

    impl Sampler {
        pub fn new(disk: &str, ping: &str) -> Sampler {
            let cell = Arc::new(AtomicI64::new(-1));
            if ping != "off" && !ping.is_empty() {
                let (c, addr) = (cell.clone(), ping.to_string());
                // a failed spawn only means no ping: the bar must not die for it
                let _ = std::thread::Builder::new().name("lazyui-ping".into()).spawn(move || pinger(c, addr));
            }
            Sampler { cpu: None, net: None, disk: disk.to_string(), ping: cell }
        }

        /// `dt` is the time since the last call, for the byte rates.
        pub fn sample(&mut self, dt: f64) -> Vec<(&'static str, f64)> {
            let mut out = Vec::new();
            if let Some(now) = std::fs::read_to_string("/proc/stat").ok().and_then(|s| parse_cpu(&s)) {
                if let Some(prev) = self.cpu {
                    let (db, dt_) = (now.0.saturating_sub(prev.0), now.1.saturating_sub(prev.1));
                    out.push(("sys.cpu", if dt_ > 0 { db as f64 / dt_ as f64 * 100.0 } else { 0.0 }));
                }
                self.cpu = Some(now);
            }
            if let Some((m, s)) = std::fs::read_to_string("/proc/meminfo").ok().and_then(|s| parse_mem(&s)) {
                out.push(("sys.mem", m));
                out.push(("sys.swap", s));
            }
            if let Ok(s) = std::fs::read_to_string("/proc/net/dev") {
                let now = parse_net(&s);
                if let Some(prev) = self.net {
                    let d = dt.max(0.001);
                    out.push(("sys.rx", now.0.saturating_sub(prev.0) as f64 / d));
                    out.push(("sys.tx", now.1.saturating_sub(prev.1) as f64 / d));
                }
                self.net = Some(now);
            }
            if let Some(p) = disk_used(&self.disk) {
                out.push(("sys.disk", p));
            }
            let us = self.ping.load(Ordering::Relaxed);
            out.push(("sys.ping", if us < 0 { -1.0 } else { us as f64 / 1000.0 }));
            out
        }
    }

    fn disk_used(path: &str) -> Option<f64> {
        let c = std::ffi::CString::new(path).ok()?;
        let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
        if unsafe { libc::statvfs(c.as_ptr(), &mut s) } != 0 || s.f_blocks == 0 {
            return None;
        }
        // like df: used / (used + what a user may still take)
        let used = (s.f_blocks - s.f_bfree) as f64;
        let avail = s.f_bavail as f64;
        Some(if used + avail > 0.0 { used / (used + avail) * 100.0 } else { 0.0 })
    }

    /// Time a TCP connect every two seconds; ends when the widget (the only other holder of the cell) is gone.
    fn pinger(cell: Arc<AtomicI64>, addr: String) {
        use std::net::{TcpStream, ToSocketAddrs};
        use std::time::{Duration, Instant};
        while Arc::strong_count(&cell) > 1 {
            let t = Instant::now();
            let ok = addr.to_socket_addrs().ok().and_then(|mut a| a.next()).map(|a| TcpStream::connect_timeout(&a, Duration::from_millis(1500)).is_ok()).unwrap_or(false);
            cell.store(if ok { t.elapsed().as_micros() as i64 } else { -1 }, Ordering::Relaxed);
            std::thread::sleep(Duration::from_secs(2));
        }
    }
}

// ---- the widget --------------------------------------------------------------------------------------------------

struct Field {
    name: String,
    label: String,
    text: String,
    /// 0..1 when the value is a fill level, for colour and the web bar
    frac: Option<f64>,
}

fn fields(cx: &Cx) -> Vec<Field> {
    let names = {
        let l = cx.list_attr("items");
        if l.is_empty() { ALL.split(',').map(String::from).collect() } else { l }
    };
    let up = if cx.unicode { "↑" } else { "^" };
    let down = if cx.unicode { "↓" } else { "v" };
    names
        .into_iter()
        .filter_map(|n| {
            let m = |k: &str| cx.model_num(&format!("sys.{k}")).filter(|v| v.is_finite());
            let pct = |label: &str, k: &str| -> Field {
                match m(k) {
                    Some(v) => Field { name: n.clone(), label: label.into(), text: format!("{}%", v.round() as i64), frac: Some((v / 100.0).clamp(0.0, 1.0)) },
                    None => Field { name: n.clone(), label: label.into(), text: "--".into(), frac: None },
                }
            };
            Some(match n.as_str() {
                "cpu" => pct("cpu", "cpu"),
                "mem" => pct("mem", "mem"),
                "swap" => pct("swap", "swap"),
                "disk" => pct("disk", "disk"),
                "net" => {
                    let t = match (m("rx"), m("tx")) {
                        (Some(r), Some(t)) => format!("{down}{} {up}{}", rate(r), rate(t)),
                        _ => format!("{down}-- {up}--"),
                    };
                    Field { name: n.clone(), label: "net".into(), text: t, frac: None }
                }
                "ping" => match m("ping") {
                    Some(v) if v >= 0.0 => Field { name: n.clone(), label: "ping".into(), text: format!("{}ms", v.round() as i64), frac: Some((v / 200.0).clamp(0.0, 1.0)) },
                    _ => Field { name: n.clone(), label: "ping".into(), text: "--".into(), frac: None },
                },
                "fps" => Field { name: n.clone(), label: "fps".into(), text: m("fps").map(|v| format!("{}", v.round() as i64)).unwrap_or_else(|| "--".into()), frac: None },
                _ => return None,
            })
        })
        .collect()
}

struct Sysbar {
    acc: f64,
    window: f64,
    frames: u32,
    started: bool,
    #[cfg(all(unix, not(target_arch = "wasm32")))]
    sampler: Option<native::Sampler>,
}

impl Default for Sysbar {
    fn default() -> Sysbar {
        Sysbar {
            acc: 0.0,
            window: 0.0,
            frames: 0,
            started: false,
            #[cfg(all(unix, not(target_arch = "wasm32")))]
            sampler: None,
        }
    }
}

impl Widget for Sysbar {
    fn draw(&mut self, c: &mut Canvas, r: Rect, cx: &Cx) {
        self.frames += 1;
        if r.w < 1 || r.h < 1 {
            return;
        }
        let t = cx.theme;
        let f = fields(cx);
        let widths: Vec<i32> = f.iter().map(|x| (str_width(&x.label) + 1 + str_width(&x.text)) as i32).collect();
        let total: i32 = widths.iter().sum::<i32>() + 2 * (f.len() as i32 - 1).max(0);
        let mut x = if cx.attr("align") == Some("right") { r.x + (r.w - total).max(0) } else { r.x };
        let end = r.x + r.w;
        for (fl, w) in f.iter().zip(widths) {
            if x + w > end {
                break;
            }
            c.text(x, r.y, &fl.label, Style::fg(t.dim).on(t.bg));
            let col = match fl.frac {
                Some(v) => level(t, v),
                None if fl.name == "net" => t.accent,
                None => t.fg,
            };
            c.text(x + str_width(&fl.label) as i32 + 1, r.y, &fl.text, Style::fg(col).on(t.bg));
            x += w + 2;
        }
    }

    fn describe(&self, cx: &Cx) -> String {
        let items: Vec<String> = fields(cx)
            .iter()
            .map(|f| {
                let frac = f.frac.map(|v| format!("{v:.3}")).unwrap_or_else(|| "null".into());
                format!("{{\"name\":{},\"label\":{},\"text\":{},\"frac\":{}}}", jstr(&f.name), jstr(&f.label), jstr(&f.text), frac)
            })
            .collect();
        format!("{{\"items\":[{}]}}", items.join(","))
    }

    fn tick(&mut self, dt: f64, cx: &mut EvCx) {
        let every = cx.attr("every").and_then(|s| s.parse::<f64>().ok()).filter(|e| *e > 0.05).unwrap_or(1.0);
        if !self.started {
            self.started = true;
            self.acc = every; // sample on the first tick
            #[cfg(all(unix, not(target_arch = "wasm32")))]
            {
                let ping = cx.attr("ping").unwrap_or("1.1.1.1:53").to_string();
                self.sampler = Some(native::Sampler::new(cx.attr("disk").unwrap_or("/"), &ping));
            }
        }
        self.acc += dt;
        self.window += dt;
        if self.acc < every {
            return;
        }
        let span = self.acc.min(every * 4.0).max(dt);
        self.acc = 0.0;
        // the page may feed these keys itself (the browser has no /proc): only touch what we can measure
        #[cfg(all(unix, not(target_arch = "wasm32")))]
        if let Some(s) = self.sampler.as_mut() {
            for (k, v) in s.sample(span) {
                cx.set.push((k.to_string(), Value::Num(v)));
            }
        }
        let _ = span;
        let fps = if self.window > 0.0 { self.frames as f64 / self.window } else { 0.0 };
        self.frames = 0;
        self.window = 0.0;
        // in the browser the page counts its real animation frames (the skin may not draw at all)
        #[cfg(not(target_arch = "wasm32"))]
        if cx.attr("fps") != Some("off") {
            cx.set.push(("sys.fps".to_string(), Value::Num(fps)));
        }
        #[cfg(target_arch = "wasm32")]
        let _ = fps;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::headless::{run_script, snapshot};
    use crate::ui::Ui;

    #[test]
    fn parses_proc_text() {
        assert_eq!(parse_cpu("cpu  100 0 50 800 50 0 0 0 0 0\ncpu0 1 2 3 4\n"), Some((150, 1000)));
        assert_eq!(parse_cpu("nothing"), None);
        let (m, s) = parse_mem("MemTotal: 1000 kB\nMemFree: 100 kB\nMemAvailable: 400 kB\nSwapTotal: 200 kB\nSwapFree: 150 kB\n").unwrap();
        assert!((m - 60.0).abs() < 1e-9 && (s - 25.0).abs() < 1e-9, "{m} {s}");
        let (m, s) = parse_mem("MemTotal: 10 kB\nMemAvailable: 10 kB\n").unwrap();
        assert!(m == 0.0 && s == 0.0, "no swap is 0%");
        let dev = "Inter-|   Receive\n face |bytes packets\n    lo: 500 1 0 0 0 0 0 0 500 1 0 0 0 0 0 0\n  eth0: 1000 1 0 0 0 0 0 0 2000 1 0 0 0 0 0 0\n  wlan0: 10 1 0 0 0 0 0 0 20 1 0 0 0 0 0 0\n";
        assert_eq!(parse_net(dev), (1010, 2020));
    }

    #[test]
    fn rates_are_short() {
        assert_eq!(rate(0.0), "0B");
        assert_eq!(rate(999.0), "999B");
        assert_eq!(rate(2048.0), "2K");
        assert_eq!(rate(1.5 * 1048576.0), "1.5M");
        assert_eq!(rate(f64::NAN), "0B");
        assert!(rate(1e15).ends_with('G'));
    }

    #[test]
    fn bar_shows_what_the_model_holds_and_dashes_for_the_rest() {
        let mut ui = Ui::from_spec("sysbar items=\"cpu,swap,net,ping,fps\" ping=off fps=off\n").unwrap();
        // an unmeasured bar shows dashes, never panics
        let f = snapshot(&mut ui, 60, 1);
        assert!(f.contains("cpu --") && f.contains("ping --"), "{f}");
        run_script(&mut ui, "set sys.cpu=42\nset sys.swap=3\nset sys.rx=2048\nset sys.tx=100\nset sys.ping=12").unwrap();
        let f = snapshot(&mut ui, 60, 1);
        assert!(f.contains("cpu 42%") && f.contains("swap 3%") && f.contains("v2K ^100B") && f.contains("ping 12ms"), "{f}");
        // too narrow: items drop from the right instead of overflowing
        let f = snapshot(&mut ui, 12, 1);
        assert!(f.starts_with("cpu 42%") && !f.contains("ping"), "{f}");
    }

    #[test]
    fn ticks_fill_the_model_and_fps_counts_frames() {
        let mut ui = Ui::from_spec("sysbar ping=off\n").unwrap();
        run_script(&mut ui, "size 80 1\nsnap\ntick 2.5\nsnap").unwrap();
        // fps is always written; on linux the sampler also fills mem and disk
        assert!(ui.text_of("sys.fps").parse::<f64>().is_ok(), "fps {:?}", ui.text_of("sys.fps"));
        if cfg!(target_os = "linux") {
            let mem: f64 = ui.text_of("sys.mem").parse().unwrap();
            assert!(mem > 0.0 && mem <= 100.0, "{mem}");
            let disk: f64 = ui.text_of("sys.disk").parse().unwrap();
            assert!(disk >= 0.0 && disk <= 100.0, "{disk}");
        }
    }

    #[test]
    fn describe_carries_items_for_the_web_skin() {
        let mut ui = Ui::from_spec("sysbar items=\"cpu,ping\" ping=off fps=off\n").unwrap();
        let o = run_script(&mut ui, "set sys.cpu=50\nsize 40 1\ndescribe").unwrap();
        assert!(o.contains("\"name\":\"cpu\"") && o.contains("\"text\":\"50%\"") && o.contains("\"frac\":0.500") && o.contains("\"frac\":null"), "{o}");
    }
}
