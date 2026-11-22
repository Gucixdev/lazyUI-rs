//! More lazytui bars and monitor rows: volumebar (keys + mouse, mute), pacmanbar (a pacman -Sy download line),
//! heatdither (the heat grid as ordered-dither braille), corebars and ratepanel (rows of load / rate with a bar).

use crate::canvas::{str_width, Canvas, Color, Rect, Style, BOLD, UNDERLINE};
use crate::gfx::{clamp01, eighth, mix};
use crate::input::{Event, Key, MouseKind};
use crate::native::{Cx, Def, EvCx, Pure, Widget};
use crate::ui::{jstr, Value};
use crate::widgets::Res;

fn pure(f: fn(&mut Canvas, Rect, &Cx)) -> Box<dyn Widget> {
    Box::new(Pure(f))
}

pub static DEFS: &[Def] = &[
    Def { name: "volumebar", doc: "a level bar with mute, keys and mouse: bind=KEY label max=1 step=0.05 vertical=1 muted=KEY; emits change and mute", focus: true, rows: Some(1), live: 0.0, make: || Box::<Volume>::default() },
    Def { name: "pacmanbar", doc: "a pacman -Sy style download line: name done total rate eta (seconds, -1 unknown)", focus: false, rows: Some(1), live: 0.0, make: || pure(pacman) },
    Def { name: "heatdither", doc: "heat grid where every cell is a dither patch of braille dots: values cols min max cell=2 rows=\"a,b\"", focus: false, rows: None, live: 0.0, make: || pure(heatdither) },
    Def { name: "corebars", doc: "one row per core: labels values (percent) infos, with a load bar", focus: false, rows: None, live: 0.0, make: || pure(corebars) },
    Def { name: "ratepanel", doc: "one row per rate: labels values (bytes/s) peaks, with a bar", focus: false, rows: None, live: 0.0, make: || pure(ratepanel) },
];

fn style(cx: &Cx, fg: Color) -> Style {
    Style::fg(fg).on(cx.theme.bg)
}

/// green, yellow, red by load percent
fn load_colour(cx: &Cx, pct: f64) -> Color {
    let t = cx.theme;
    if pct > 85.0 {
        t.err
    } else if pct > 60.0 {
        t.warn
    } else {
        t.ok
    }
}

// ---- volumebar -------------------------------------------------------------------------------------------

#[derive(Default)]
struct Volume {
    level: f64,
    muted: bool,
    started: bool,
    dragging: bool,
}

/// where the parts sit: (bar start, bar length, text start) along the bar's axis, and whether it is vertical
struct Geo {
    vertical: bool,
    bar0: i32,
    len: i32,
    text: i32,
}

fn pct_text(level: f64, max: f64, muted: bool) -> String {
    if muted {
        "MUTE".into()
    } else {
        format!("{}%", (level / max.max(0.01) * 100.0).round() as i32)
    }
}

impl Volume {
    fn max(cx: &Cx) -> f64 {
        cx.num_attr("max", 1.0).clamp(0.1, 4.0)
    }

    fn pull(&mut self, cx: &Cx) {
        let max = Self::max(cx);
        if !self.started {
            self.started = true;
            self.level = cx.num_attr("value", 0.0);
            self.muted = cx.flag("muted");
        }
        if let Some(v) = cx.el.attr("bind").and_then(|k| cx.model_num(k)) {
            self.level = v; // the model is the truth: another widget or the program may have moved it
        }
        self.level = if self.level.is_finite() { self.level.clamp(0.0, max) } else { 0.0 };
    }

    fn geo(r: Rect, cx: &Cx, max: f64, level: f64, muted: bool) -> Geo {
        let label = cx.text_attr("label", "");
        let lw = if label.is_empty() { 0 } else { str_width(&label) as i32 + 1 };
        let tw = str_width(&pct_text(level, max, muted)) as i32 + 1;
        if cx.flag("vertical") {
            // label under the bar, percent above it
            Geo { vertical: true, bar0: r.y + 1, len: (r.h - 2).max(0), text: r.y }
        } else {
            let len = (r.w - lw - tw - 2).max(0);
            Geo { vertical: false, bar0: r.x + lw + 1, len, text: r.x + lw + 1 + len + 1 }
        }
    }

    fn set(&mut self, v: f64, cx: &mut EvCx) {
        let max = cx.attr("max").and_then(|s| s.parse::<f64>().ok()).unwrap_or(1.0).clamp(0.1, 4.0);
        let v = if v.is_finite() { v.clamp(0.0, max) } else { 0.0 };
        if (v - self.level).abs() > 1e-9 {
            self.level = v;
            cx.set_bound(Value::Num(v));
            cx.emit("change", "", -1, v);
        }
    }

    fn mute(&mut self, cx: &mut EvCx) {
        self.muted = !self.muted;
        if let Some(k) = cx.attr("muted").map(|s| s.to_string()) {
            cx.set.push((k, Value::Num(if self.muted { 1.0 } else { 0.0 })));
        }
        cx.emit("mute", "", self.muted as i64, self.level);
    }
}

impl Widget for Volume {
    fn draw(&mut self, c: &mut Canvas, r: Rect, cx: &Cx) {
        self.pull(cx);
        if let Some(m) = cx.el.attr("muted").and_then(|k| cx.model_num(k)) {
            self.muted = m != 0.0;
        }
        if r.w < 4 || r.h < 1 {
            return;
        }
        let t = cx.theme;
        let max = Self::max(cx);
        let g = Self::geo(r, cx, max, self.level, self.muted);
        let f = clamp01(self.level / max);
        let col = if self.muted { t.dim } else { load_colour(cx, f * 100.0 / max.max(1.0)) };
        let label = cx.text_attr("label", "");
        let lab = if cx.focused { style(cx, t.focus).with(UNDERLINE) } else { style(cx, t.dim) };
        let txt = pct_text(self.level, max, self.muted);
        if g.vertical {
            c.text_fit(r.x, r.y, r.w, &txt, style(cx, if self.muted { t.dim } else { t.fg }));
            if r.h >= 2 {
                c.text_fit(r.x, r.y + r.h - 1, r.w, &label, lab);
            }
            let eighths = (f * g.len as f64 * 8.0).round() as i32;
            for i in 0..g.len {
                let row_from_bottom = i;
                let below = row_from_bottom * 8;
                let here = (eighths - below).clamp(0, 8);
                let y = g.bar0 + g.len - 1 - i;
                let ch = if here == 0 { if cx.unicode { '░' } else { '.' } } else { eighth(here, cx.unicode) };
                c.put(r.x, y, ch, style(cx, if here == 0 { t.dim } else { col }));
            }
            return;
        }
        c.text_fit(r.x, r.y, g.bar0 - r.x, &label, lab);
        let eighths = (f * g.len as f64 * 8.0).round() as i32;
        for i in 0..g.len {
            let here = (eighths - i * 8).clamp(0, 8);
            let ch = if here == 0 { if cx.unicode { '░' } else { '.' } } else if here == 8 { if cx.unicode { '█' } else { '#' } } else { part_h(here, cx.unicode) };
            c.put(g.bar0 + i, r.y, ch, style(cx, if here == 0 { t.dim } else { col }));
        }
        c.text_fit(g.text, r.y, r.x + r.w - g.text, &txt, style(cx, if self.muted { t.dim } else { t.fg }).with(if self.muted { BOLD } else { 0 }));
    }

    fn event(&mut self, r: Rect, ev: &Event, cx: &mut EvCx) -> Res {
        let max = cx.attr("max").and_then(|s| s.parse::<f64>().ok()).unwrap_or(1.0).clamp(0.1, 4.0);
        let step = cx.attr("step").and_then(|s| s.parse::<f64>().ok()).filter(|s| *s > 0.0 && *s <= 1.0).unwrap_or(0.05) * max;
        let vertical = cx.attr("vertical").is_some_and(|v| v != "0");
        let level = self.level;
        match ev {
            Event::Key(k, _) => {
                let to = match k {
                    Key::Right | Key::Char('+') | Key::Char('=') if !vertical || matches!(k, Key::Char(_)) => level + step,
                    Key::Left | Key::Char('-') if !vertical || matches!(k, Key::Char(_)) => level - step,
                    Key::Up if vertical => level + step,
                    Key::Down if vertical => level - step,
                    Key::PageUp => level + 5.0 * step,
                    Key::PageDown => level - 5.0 * step,
                    Key::Home => 0.0,
                    Key::End => max,
                    Key::Char('m') | Key::Char('M') => {
                        self.mute(cx);
                        return Res::Changed;
                    }
                    _ => return Res::Ignored,
                };
                self.set(to, cx);
                Res::Changed
            }
            Event::Mouse(kind, x, y, _) => {
                let want = |g: &Geo| -> f64 {
                    let pos = if g.vertical { g.bar0 + g.len - 1 - *y } else { *x - g.bar0 };
                    (pos as f64 + 0.5) / g.len.max(1) as f64 * max
                };
                // the geometry needs the label and text widths: rebuild it from the element's own attributes
                let label = cx.attr("label").unwrap_or("");
                let lw = if label.is_empty() { 0 } else { str_width(label) as i32 + 1 };
                let tw = str_width(&pct_text(level, max, self.muted)) as i32 + 1;
                let g = if vertical { Geo { vertical: true, bar0: r.y + 1, len: (r.h - 2).max(0), text: r.y } } else { let len = (r.w - lw - tw - 2).max(0); Geo { vertical: false, bar0: r.x + lw + 1, len, text: r.x + lw + 1 + len + 1 } };
                let on_bar = if vertical { *x == r.x && *y >= g.bar0 && *y < g.bar0 + g.len } else { *y == r.y && *x >= g.bar0 && *x < g.bar0 + g.len };
                match kind {
                    MouseKind::Down(0) if on_bar => {
                        self.dragging = true;
                        self.set(want(&g), cx);
                        Res::Changed
                    }
                    MouseKind::Down(0) if (if vertical { *y == r.y } else { *y == r.y && *x >= g.text }) => {
                        self.mute(cx);
                        Res::Changed
                    }
                    MouseKind::Drag(0) if self.dragging && g.len > 0 => {
                        self.set(want(&g), cx);
                        Res::Changed
                    }
                    MouseKind::Up(_) => {
                        self.dragging = false;
                        Res::Handled
                    }
                    MouseKind::WheelUp => {
                        self.set(level + step, cx);
                        Res::Changed
                    }
                    MouseKind::WheelDown => {
                        self.set(level - step, cx);
                        Res::Changed
                    }
                    _ => Res::Ignored,
                }
            }
            _ => Res::Ignored,
        }
    }

    fn describe(&self, cx: &Cx) -> String {
        let max = Self::max(cx);
        format!("{{\"level\":{},\"max\":{},\"muted\":{},\"text\":{}}}", self.level, max, self.muted, jstr(&pct_text(self.level, max, self.muted)))
    }

    fn act(&mut self, action: &str, arg: &str, cx: &mut EvCx) -> Res {
        match (action, arg.trim().parse::<f64>()) {
            ("set", Ok(v)) => {
                self.set(v, cx);
                Res::Changed
            }
            ("mute" | "toggle", _) => {
                self.mute(cx);
                Res::Changed
            }
            _ => Res::Ignored,
        }
    }
}

/// the left `n` eighths of a cell as a block (n 1..=7), or a coarse stand-in
fn part_h(n: i32, unicode: bool) -> char {
    const PART: [char; 8] = [' ', '▏', '▎', '▍', '▌', '▋', '▊', '▉'];
    if unicode {
        PART[n.clamp(0, 7) as usize]
    } else if n >= 4 {
        '='
    } else {
        '-'
    }
}

// ---- pacmanbar -------------------------------------------------------------------------------------------

/// pacman's own arithmetic (callback.c): sizes with one decimal, rates with two/one/zero
fn humanize(mut v: f64, rate: bool) -> (String, &'static str) {
    const UNITS: [&str; 9] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB", "EiB", "ZiB", "YiB"];
    let mut u = 0;
    while v > 2048.0 && u < 8 {
        v /= 1024.0;
        u += 1;
    }
    let s = if rate { format!("{:.*}", if v < 10.0 { 2 } else if v < 100.0 { 1 } else { 0 }, v) } else { format!("{v:.1}") };
    (s, UNITS[u])
}

pub fn pacman_line(cols: i32, name: &str, done: f64, total: f64, rate: f64, eta: i64) -> String {
    let infolen = (cols * 6 / 10).max(50);
    let f = (infolen - 30).max(1) as usize; // the name column
    let nm: String = if name.chars().count() > f - 1 { format!("{}...", name.chars().take(f.saturating_sub(4)).collect::<String>()) } else { name.to_string() };
    let (done, rate) = (if done.is_finite() { done } else { 0.0 }, if rate.is_finite() { rate.max(0.0) } else { 0.0 });
    let (sn, su) = humanize(done, false);
    let (rn, ru) = humanize(rate, true);
    let eta = if eta < 0 {
        "--:--".to_string()
    } else if eta >= 3600 {
        format!("{:02}:{:02}:{:02}", eta / 3600, eta / 60 % 60, eta % 60)
    } else {
        format!("{:02}:{:02}", eta / 60, eta % 60)
    };
    let sz = format!("{sn} {su:>3}");
    let mut out = format!(" {nm:<f$} {sz:>10} {rn:>5} {ru:>3}/s {eta}");
    let pf = if total > 0.0 { done * 100.0 / total } else { 0.0 };
    let percent = if pf > 0.0 { pf.min(100.0) as i32 } else { 0 };
    let proglen = cols - infolen;
    let hashlen = if proglen > 8 { proglen - 8 } else { 0 };
    let hash = percent * hashlen / 100;
    if hashlen > 0 {
        out.push_str(" [");
        for i in (1..=hashlen).rev() {
            out.push(if i > hashlen - hash {
                '-'
            } else if i == hashlen - hash {
                if hash % 2 == 1 { 'c' } else { 'C' } // the mouth works as it advances
            } else if i % 3 == 0 {
                'o'
            } else {
                ' '
            });
        }
        out.push(']');
    }
    if proglen >= 5 {
        out.push_str(&format!(" {percent:>3}%"));
    }
    out
}

fn pacman(c: &mut Canvas, r: Rect, cx: &Cx) {
    if r.w < 1 || r.h < 1 {
        return;
    }
    let line = pacman_line(r.w, &cx.text_attr("name", ""), cx.num_attr("done", 0.0), cx.num_attr("total", 0.0), cx.num_attr("rate", 0.0), cx.num_attr("eta", -1.0) as i64);
    let t = cx.theme;
    c.text_fit(r.x, r.y, r.w, &line, style(cx, t.fg));
    // the only colour is the chomping mouth: the characters themselves are pacman's
    let chars: Vec<char> = line.chars().collect();
    if let (Some(a), Some(b)) = (chars.iter().position(|ch| *ch == '['), chars.iter().rposition(|ch| *ch == ']')) {
        for (i, ch) in chars.iter().enumerate().take(b).skip(a + 1) {
            if (*ch == 'c' || *ch == 'C') && (i as i32) < r.w {
                c.put(r.x + i as i32, r.y, *ch, style(cx, t.warn).with(BOLD));
            }
        }
    }
}

// ---- heatdither ------------------------------------------------------------------------------------------

fn heatdither(c: &mut Canvas, r: Rect, cx: &Cx) {
    let v = cx.series();
    if r.w < 2 || r.h < 1 || v.is_empty() {
        return;
    }
    const BAYER: [[f64; 4]; 4] = [[0.0, 8.0, 2.0, 10.0], [12.0, 4.0, 14.0, 6.0], [3.0, 11.0, 1.0, 9.0], [15.0, 7.0, 13.0, 5.0]];
    const BIT: [[u8; 2]; 4] = [[0x01, 0x08], [0x02, 0x10], [0x04, 0x20], [0x40, 0x80]];
    let t = cx.theme;
    let cols = (cx.num_attr("cols", (v.len() as f64).sqrt().ceil()) as usize).max(1);
    let rows = v.len().div_ceil(cols);
    let lo = cx.num_attr("min", v.iter().cloned().fold(f64::INFINITY, f64::min));
    let hi = cx.num_attr("max", v.iter().cloned().fold(f64::NEG_INFINITY, f64::max));
    let cell_w = cx.num_attr("cell", 2.0).clamp(1.0, 4.0) as i32;
    let names = cx.list_attr("rows");
    let lw = names.iter().map(|s| str_width(s) as i32).max().map_or(0, |w| w + 1);
    let base = cx.colour("color", t.accent);
    for y in 0..rows.min(r.h as usize) {
        if let Some(n) = names.get(y) {
            c.text_fit(r.x, r.y + y as i32, lw, n, style(cx, t.dim));
        }
        for x in 0..cols {
            let cx0 = r.x + lw + x as i32 * (cell_w + 1);
            if cx0 + cell_w > r.x + r.w {
                break;
            }
            let Some(val) = v.get(y * cols + x) else { continue };
            let f = if hi > lo && val.is_finite() { clamp01((val - lo) / (hi - lo)) } else { 0.0 };
            let col = if f > 0.0 { mix(t.dim, base, f) } else { t.dim };
            for k in 0..cell_w {
                let mut bits = 0u8;
                for (d, brow) in BIT.iter().enumerate() {
                    for (h, b) in brow.iter().enumerate() {
                        if f > 0.0 && f * 16.0 > BAYER[d][((k * 2) as usize + h) & 3] {
                            bits |= b;
                        }
                    }
                }
                const RAMP: &[u8] = b" .:-=+*#%@";
                let ch = if cx.unicode { char::from_u32(0x2800 + bits as u32).unwrap_or(' ') } else { RAMP[(f * 9.0 + 0.5) as usize] as char };
                c.put(cx0 + k, r.y + y as i32, ch, style(cx, col));
            }
        }
    }
}

// ---- corebars / ratepanel --------------------------------------------------------------------------------

/// a bar of `w` cells filled to `f`, eighths and all
fn hbar(c: &mut Canvas, x: i32, y: i32, w: i32, f: f64, fg: Color, cx: &Cx) {
    let eighths = (clamp01(f) * w as f64 * 8.0).round() as i32;
    for i in 0..w {
        let here = (eighths - i * 8).clamp(0, 8);
        let (ch, col) = if here == 0 { (if cx.unicode { '░' } else { '.' }, cx.theme.dim) } else if here == 8 { (if cx.unicode { '█' } else { '#' }, fg) } else { (part_h(here, cx.unicode), fg) };
        c.put(x + i, y, ch, style(cx, col));
    }
}

fn corebars(c: &mut Canvas, r: Rect, cx: &Cx) {
    let (v, labels, infos) = (cx.series(), cx.list_attr("labels"), cx.list_attr("infos"));
    if r.w < 1 || r.h < 1 || v.is_empty() {
        return;
    }
    let first = cx.num_attr("first", 0.0).max(0.0) as usize;
    for (row, i) in (first..v.len()).take(r.h as usize).enumerate() {
        let pct = if v[i].is_finite() { v[i] } else { 0.0 };
        let info = infos.get(i).map(String::as_str).unwrap_or("");
        let label = labels.get(i).cloned().unwrap_or_else(|| format!("c{i}"));
        let fixed = format!("{label:<4}{pct:5.1}% {info}{}", if info.is_empty() { "" } else { " " });
        let mut fw = str_width(&fixed) as i32;
        let mut gw = (r.w - fw).min(16);
        if gw < 4 {
            fw = r.w; // too narrow for a bar: the text takes the row
            gw = 0;
        }
        let col = load_colour(cx, pct);
        c.text_fit(r.x, r.y + row as i32, fw, &fixed, style(cx, col));
        if gw > 0 {
            hbar(c, r.x + fw, r.y + row as i32, gw, pct / 100.0, col, cx);
        }
    }
}

fn human_rate(v: f64) -> String {
    let (n, u) = humanize(v.max(0.0), true);
    format!("{n}{u}/s")
}

fn ratepanel(c: &mut Canvas, r: Rect, cx: &Cx) {
    let (v, labels, peaks) = (cx.series(), cx.list_attr("labels"), crate::gfx::numbers(cx.el.attr("peaks").unwrap_or("")));
    if r.w < 1 || r.h < 1 || v.is_empty() {
        return;
    }
    let t = cx.theme;
    let lw = labels.iter().map(|l| str_width(l) as i32).max().unwrap_or(0).min(r.w / 3);
    let top = peaks.iter().chain(v.iter()).cloned().filter(|x| x.is_finite()).fold(1.0, f64::max);
    let per = if r.h >= 2 * v.len() as i32 { 2 } else { 1 };
    for (i, x) in v.iter().enumerate() {
        if (i as i32 + 1) * per > r.h {
            break;
        }
        let y = r.y + i as i32 * per;
        c.text_fit(r.x, y, lw, labels.get(i).map(String::as_str).unwrap_or(""), style(cx, t.dim));
        let rate = format!("{:>9}", human_rate(if x.is_finite() { *x } else { 0.0 }));
        let mut xx = r.x + lw + 1;
        c.text_fit(xx, y, r.x + r.w - xx, &rate, style(cx, t.fg));
        xx += str_width(&rate) as i32;
        if let Some(p) = peaks.get(i).filter(|p| **p > 0.0) {
            if r.x + r.w - xx > 14 {
                let s = format!(" pk {}", human_rate(*p));
                c.text_fit(xx, y, r.x + r.w - xx, &s, style(cx, t.dim));
                xx += str_width(&s) as i32;
            }
        }
        let (gx, gy, gw) = if per == 2 { (r.x + lw + 1, y + 1, r.w - lw - 1) } else { (xx + 1, y, r.x + r.w - xx - 1) };
        if gw > 0 {
            hbar(c, gx, gy, gw, x / top, t.accent, cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::headless::snapshot;
    use crate::input::Mods;
    use crate::ui::Ui;

    fn snap(spec: &str, w: i32, h: i32) -> String {
        let mut ui = Ui::from_spec(spec).unwrap();
        snapshot(&mut ui, w, h)
    }

    #[test]
    fn all_draw_at_any_size() {
        for spec in [
            "volumebar label=spk value=0.5",
            "volumebar label=mic vertical=1 value=0.3",
            "pacmanbar name=linux done=1000000 total=4000000 rate=2500000 eta=61",
            "pacmanbar name=x",
            "heatdither values=1,2,3,4,5,6,7,8,9 cols=3 rows=\"a,b,c\"",
            "corebars labels=c0,c1 values=12.5,88 infos=\"3400MHz,3000MHz\"",
            "ratepanel labels=\"eth0 rx,eth0 tx\" values=1200000,40000 peaks=3000000,100000",
        ] {
            for (w, h) in [(0, 0), (1, 1), (5, 1), (40, 3), (120, 12)] {
                let _ = snap(spec, w, h);
            }
        }
    }

    #[test]
    fn pacman_line_matches_pacman_arithmetic() {
        let l = pacman_line(100, "core", 22.0 * 1024.0 * 1024.0, 44.0 * 1024.0 * 1024.0, 4.15 * 1024.0 * 1024.0, 5);
        assert!(l.contains("22.0 MiB") && l.contains("4.15 MiB/s") && l.contains("00:05") && l.trim_end().ends_with("50%"), "{l}");
        assert!(l.contains('[') && (l.contains('c') || l.contains('C')), "{l}");
        assert!(pacman_line(100, "x", 0.0, 0.0, 0.0, -1).contains("--:--"));
    }

    #[test]
    fn volume_keys_mouse_and_mute() {
        let mut ui = Ui::from_spec("col\n  volumebar id=v label=spk bind=vol value=0.5").unwrap();
        let t = snapshot(&mut ui, 40, 2);
        assert!(t.contains("spk") && t.contains("50%"), "{t}");
        ui.dispatch(&Event::Key(Key::Right, Mods::NONE));
        assert!(snapshot(&mut ui, 40, 2).contains("55%"));
        ui.dispatch(&Event::Key(Key::Home, Mods::NONE));
        assert!(snapshot(&mut ui, 40, 2).contains("0%"));
        ui.dispatch(&Event::Key(Key::End, Mods::NONE));
        assert!(snapshot(&mut ui, 40, 2).contains("100%"));
        ui.dispatch(&Event::ch('m'));
        assert!(snapshot(&mut ui, 40, 2).contains("MUTE"));
        // a click in the middle of the bar sets about half
        ui.dispatch(&Event::ch('m'));
        ui.dispatch(&Event::Mouse(MouseKind::Down(0), 16, 0, Mods::NONE));
        let after = snapshot(&mut ui, 40, 2);
        assert!(after.contains("%") && !after.contains("100%"), "{after}");
    }

    #[test]
    fn corebars_colour_by_load_and_ratepanel_shows_rates() {
        let s = snap("corebars labels=c0,c1 values=12.5,88 infos=\"3400MHz,3000MHz\"", 50, 2);
        assert!(s.contains("12.5%") && s.contains("88.0%") && s.contains("3400MHz"), "{s}");
        let r = snap("ratepanel labels=\"rx,tx\" values=1200000,40000 peaks=3000000,100000", 60, 2);
        assert!(r.contains("1172KiB/s") && r.contains("pk"), "{r}");
    }
}
