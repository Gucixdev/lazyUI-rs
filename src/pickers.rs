//! The pickers of lazytui: colorpicker (HSV plane + hue bar), timepicker (HH:MM[:SS] spinner), datepicker (one month
//! grid) and gradientpicker (a bar with draggable stops). Each keeps its own state, reads `value=` once (or `bind=KEY`
//! whenever the model changes it) and writes its text value back to `bind`: `#rrggbb`, `HH:MM:SS`, `YYYY-MM-DD`,
//! `0:#000000,1:#ffffff`. They emit `change` with that text.

use crate::canvas::{str_width, Canvas, Color, Rect, Style, BOLD};
use crate::gfx::{clamp01, mix, rgb_of};
use crate::input::{Event, Key, MouseKind};
use crate::native::{Cx, Def, EvCx, Widget};
use crate::theme::parse_color;
use crate::ui::{jstr, Value};
use crate::widgets::Res;

pub static DEFS: &[Def] = &[
    Def { name: "colorpicker", doc: "HSV colour picker: value=#rrggbb bind=KEY; arrows move saturation/value, [ ] hue, r resets; mouse drags the plane and hue bar", focus: true, rows: None, live: 0.0, make: || Box::<ColorPicker>::default() },
    Def { name: "timepicker", doc: "time field spinner: value=HH:MM:SS seconds=1 look=fields|spinbox|h12 bind=KEY; left/right field, up/down or wheel change", focus: true, rows: Some(1), live: 0.0, make: || Box::<TimePicker>::default() },
    Def { name: "datepicker", doc: "month grid: value=YYYY-MM-DD bind=KEY; arrows move days/weeks, PgUp/PgDn months, [ ] years, click a day", focus: true, rows: Some(8), live: 0.0, make: || Box::<DatePicker>::default() },
    Def { name: "gradientpicker", doc: "gradient bar with stops: value=\"0:#000000,1:#ffffff\" bind=KEY; tab picks a stop, left/right move it, click adds, del removes, r resets", focus: true, rows: Some(3), live: 0.0, make: || Box::<GradientPicker>::default() },
];

fn ink(c: Color) -> Color {
    match rgb_of(c) {
        Some((r, g, b)) if 0.299 * r as f64 + 0.587 * g as f64 + 0.114 * b as f64 > 140.0 => Color::Rgb(0, 0, 0),
        _ => Color::Rgb(255, 255, 255),
    }
}

fn hex(c: Color) -> String {
    let (r, g, b) = rgb_of(c).unwrap_or((0, 0, 0));
    format!("#{r:02x}{g:02x}{b:02x}")
}

fn sel_style(cx: &Cx, focused: bool) -> Style {
    let t = cx.theme;
    if focused {
        Style::fg(t.bg).on(t.focus).with(BOLD)
    } else {
        Style::fg(t.accent).on(t.bg).with(BOLD)
    }
}

fn plain(cx: &Cx) -> Style {
    Style::fg(cx.theme.fg).on(cx.theme.bg)
}

fn dim(cx: &Cx) -> Style {
    Style::fg(cx.theme.dim).on(cx.theme.bg)
}

fn put_text(c: &mut Canvas, x: i32, y: i32, w: i32, s: &str, st: Style) {
    c.text_fit(x, y, w, s, st);
}

// ---- colorpicker -----------------------------------------------------------------------------------------

pub fn hsv_to_rgb(h: f64, s: f64, v: f64) -> (f64, f64, f64) {
    let h = h.rem_euclid(360.0);
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match (h / 60.0) as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    (r + m, g + m, b + m)
}

pub fn rgb_to_hsv(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    let mx = r.max(g).max(b);
    let d = mx - r.min(g).min(b);
    let s = if mx > 0.0 { d / mx } else { 0.0 };
    if d <= 0.0 {
        return (0.0, s, mx);
    }
    let h = if mx == r { ((g - b) / d) % 6.0 } else if mx == g { (b - r) / d + 2.0 } else { (r - g) / d + 4.0 } * 60.0;
    (h.rem_euclid(360.0), s, mx)
}

fn rgb8(h: f64, s: f64, v: f64) -> Color {
    let (r, g, b) = hsv_to_rgb(h, s, v);
    Color::Rgb((r * 255.0 + 0.5) as u8, (g * 255.0 + 0.5) as u8, (b * 255.0 + 0.5) as u8)
}

#[derive(Default)]
struct ColorPicker {
    h: f64,
    s: f64,
    v: f64,
    initial: (f64, f64, f64),
    started: bool,
    plane: Rect,
    bar: Rect,
    drag_plane: bool,
    drag_hue: bool,
}

impl ColorPicker {
    fn current(&self) -> Color {
        rgb8(self.h, self.s, self.v)
    }

    fn set_color(&mut self, c: Color) {
        if let Some((r, g, b)) = rgb_of(c) {
            (self.h, self.s, self.v) = rgb_to_hsv(r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0);
        }
    }

    fn pull(&mut self, cx: &Cx) {
        if !self.started {
            self.started = true;
            self.set_color(cx.colour("value", Color::Rgb(255, 0, 0)));
            self.initial = (self.h, self.s, self.v);
        }
        // the model is the truth, but a grey/black colour has no hue of its own: keep ours unless the hex differs
        if let Some(c) = cx.el.attr("bind").and_then(|k| cx.model_text(k)).and_then(|t| parse_color(t.trim())) {
            if hex(c) != hex(self.current()) {
                self.set_color(c);
            }
        }
    }

    fn hsv(&mut self, h: f64, s: f64, v: f64, cx: &mut EvCx) {
        if h.is_finite() {
            self.h = h.rem_euclid(360.0);
        }
        if s.is_finite() {
            self.s = s.clamp(0.0, 1.0);
        }
        if v.is_finite() {
            self.v = v.clamp(0.0, 1.0);
        }
        let hx = hex(self.current());
        cx.set_bound(Value::Text(hx.clone()));
        cx.emit("change", &hx, -1, self.h);
    }
}

impl Widget for ColorPicker {
    fn draw(&mut self, c: &mut Canvas, r: Rect, cx: &Cx) {
        self.pull(cx);
        self.plane = Rect::new(0, 0, 0, 0);
        self.bar = Rect::new(0, 0, 0, 0);
        if r.w < 10 || r.h < 4 {
            return;
        }
        let t = cx.theme;
        self.plane = Rect::new(r.x, r.y, r.w - 4, r.h - 2);
        self.bar = Rect::new(r.x + self.plane.w + 1, r.y, 3, self.plane.h);
        let p = self.plane;
        for y in 0..p.h {
            for x in 0..p.w {
                let s = if p.w > 1 { x as f64 / (p.w - 1) as f64 } else { 0.0 };
                let v = |half: i32| 1.0 - (2 * y + half) as f64 / (2 * p.h - 1).max(1) as f64;
                let (top, bot) = (rgb8(self.h, s, v(0)), rgb8(self.h, s, v(1)));
                if cx.unicode {
                    c.put(p.x + x, p.y + y, '▀', Style::new(top, bot));
                } else {
                    c.put(p.x + x, p.y + y, ' ', Style::fg(bot).on(bot));
                }
            }
        }
        let b = self.bar;
        for y in 0..b.h {
            let col = rgb8(if b.h > 1 { y as f64 / (b.h - 1) as f64 * 360.0 } else { 0.0 }, 1.0, 1.0);
            for x in 0..b.w {
                c.put(b.x + x, b.y + y, ' ', Style::fg(col).on(col));
            }
        }
        let hy = if b.h > 1 { (self.h / 360.0 * (b.h - 1) as f64).round() as i32 } else { 0 };
        let mark = Style::fg(if cx.focused { t.focus } else { Color::Rgb(255, 255, 255) }).with(BOLD);
        c.put(b.x, b.y + hy, if cx.unicode { '◄' } else { '<' }, mark);
        c.put(b.x + b.w - 1, b.y + hy, if cx.unicode { '►' } else { '>' }, mark);
        let cur = self.current();
        let mx = p.x + (self.s * (p.w - 1).max(0) as f64).round() as i32;
        let my = p.y + ((1.0 - self.v) * (p.h - 1).max(0) as f64).round() as i32;
        c.put(mx, my, if cx.unicode { '◎' } else { 'o' }, Style::fg(ink(cur)).on(cur).with(BOLD));
        let y = r.y + r.h - 1;
        for x in 0..r.w {
            c.put(r.x + x, y, ' ', plain(cx));
        }
        for x in 0..3.min(r.w) {
            c.put(r.x + x, y, ' ', Style::fg(cur).on(cur));
        }
        let (rr, gg, bb) = rgb_of(cur).unwrap_or((0, 0, 0));
        put_text(c, r.x + 3, y, r.w - 3, &format!(" {}  r:{rr:3} g:{gg:3} b:{bb:3}", hex(cur)), plain(cx));
    }

    fn event(&mut self, _r: Rect, ev: &Event, cx: &mut EvCx) -> Res {
        let (p, b) = (self.plane, self.bar);
        match ev {
            Event::Mouse(kind, x, y, _) => {
                match kind {
                    MouseKind::Down(0) if p.contains(*x, *y) => self.drag_plane = true,
                    MouseKind::Down(0) if b.contains(*x, *y) => self.drag_hue = true,
                    MouseKind::Up(_) => {
                        let was = self.drag_plane || self.drag_hue;
                        self.drag_plane = false;
                        self.drag_hue = false;
                        return if was { Res::Handled } else { Res::Ignored };
                    }
                    MouseKind::Down(0) | MouseKind::Drag(0) => {}
                    _ => return Res::Ignored,
                }
                if self.drag_plane && p.w > 0 && p.h > 0 {
                    let s = (*x - p.x) as f64 / (p.w - 1).max(1) as f64;
                    let v = 1.0 - (*y - p.y) as f64 / (p.h - 1).max(1) as f64;
                    self.hsv(f64::NAN, s, v, cx);
                    return Res::Changed;
                }
                if self.drag_hue && b.h > 0 {
                    let h = (*y - b.y) as f64 / (b.h - 1).max(1) as f64 * 360.0;
                    self.hsv(h.clamp(0.0, 360.0), f64::NAN, f64::NAN, cx);
                    return Res::Changed;
                }
                Res::Ignored
            }
            Event::Key(k, _) => {
                let (h, s, v) = (self.h, self.s, self.v);
                match k {
                    Key::Left => self.hsv(h, s - 0.02, v, cx),
                    Key::Right => self.hsv(h, s + 0.02, v, cx),
                    Key::Up => self.hsv(h, s, v + 0.02, cx),
                    Key::Down => self.hsv(h, s, v - 0.02, cx),
                    Key::Char('[') => self.hsv(h - 5.0, s, v, cx),
                    Key::Char(']') => self.hsv(h + 5.0, s, v, cx),
                    Key::Char('r') => {
                        let (h, s, v) = self.initial;
                        self.hsv(h, s, v, cx)
                    }
                    _ => return Res::Ignored,
                }
                Res::Changed
            }
            _ => Res::Ignored,
        }
    }

    fn describe(&self, _cx: &Cx) -> String {
        format!("{{\"h\":{:.2},\"s\":{:.4},\"v\":{:.4},\"hex\":{}}}", self.h, self.s, self.v, jstr(&hex(self.current())))
    }

    fn act(&mut self, action: &str, arg: &str, cx: &mut EvCx) -> Res {
        let nums: Vec<f64> = arg.split(',').filter_map(|s| s.trim().parse().ok()).collect();
        match (action, nums.as_slice()) {
            ("sv", [s, v]) => self.hsv(f64::NAN, *s, *v, cx),
            ("hue", [h]) => self.hsv(*h, f64::NAN, f64::NAN, cx),
            ("hsv", [h, s, v]) => self.hsv(*h, *s, *v, cx),
            ("set", _) => match parse_color(arg.trim()) {
                Some(c) => {
                    self.set_color(c);
                    self.hsv(f64::NAN, f64::NAN, f64::NAN, cx);
                }
                None => return Res::Ignored,
            },
            ("reset", _) => {
                let (h, s, v) = self.initial;
                self.hsv(h, s, v, cx)
            }
            _ => return Res::Ignored,
        }
        Res::Changed
    }
}

// ---- timepicker ------------------------------------------------------------------------------------------

#[derive(Default)]
struct TimePicker {
    t: [i32; 3],
    field: usize,
    started: bool,
}

const LIM: [i32; 3] = [23, 59, 59];

fn parse_hms(s: &str) -> Option<[i32; 3]> {
    let mut it = s.trim().split(':').map(|p| p.trim().parse::<i32>().ok());
    let h = it.next()??;
    let m = it.next().flatten().unwrap_or(0);
    let sec = it.next().flatten().unwrap_or(0);
    Some([h.clamp(0, 23), m.clamp(0, 59), sec.clamp(0, 59)])
}

impl TimePicker {
    fn pull(&mut self, cx: &Cx) {
        if !self.started {
            self.started = true;
            self.t = parse_hms(&cx.text_attr("value", "00:00:00")).unwrap_or([0; 3]);
        }
        if let Some(t) = cx.el.attr("bind").and_then(|k| cx.model_text(k)).and_then(|s| parse_hms(&s)) {
            self.t = t;
        }
    }

    fn fields(el: &crate::ui::El) -> (usize, bool) {
        let h12 = el.attr("look") == Some("h12");
        let secs = matches!(el.attr("seconds"), Some("1" | "true" | "yes" | "on"));
        ((if secs { 3 } else { 2 }) + h12 as usize, h12)
    }

    fn text(&self, seconds: bool) -> String {
        if seconds {
            format!("{:02}:{:02}:{:02}", self.t[0], self.t[1], self.t[2])
        } else {
            format!("{:02}:{:02}", self.t[0], self.t[1])
        }
    }

    fn nudge(&mut self, field: usize, up: bool, base_nf: usize, cx: &mut EvCx) {
        if field >= base_nf {
            self.t[0] = (self.t[0] + 12) % 24; // the AM/PM segment: any nudge flips it
        } else {
            let m = LIM[field] + 1;
            self.t[field] = (self.t[field] + if up { 1 } else { m - 1 }) % m;
        }
        let txt = self.text(base_nf >= 3);
        cx.set_bound(Value::Text(txt.clone()));
        cx.emit("change", &txt, field as i64, self.t[field.min(2)] as f64);
    }
}

impl Widget for TimePicker {
    fn draw(&mut self, c: &mut Canvas, r: Rect, cx: &Cx) {
        self.pull(cx);
        if r.w < 5 || r.h < 1 {
            return;
        }
        let (nf, h12) = Self::fields(cx.el);
        let base = if h12 { nf - 1 } else { nf };
        self.field = self.field.min(nf - 1);
        let spin = cx.attr("look") == Some("spinbox") && r.h >= 3;
        let mut x = r.x;
        let mut vals = self.t;
        if h12 {
            vals[0] = if self.t[0] % 12 == 0 { 12 } else { self.t[0] % 12 };
        }
        let (up, dn) = if cx.unicode { ('▲', '▼') } else { ('^', 'v') };
        for i in 0..base {
            let sel = cx.focused && i == self.field;
            let vst = if sel { sel_style(cx, true) } else { plain(cx) };
            if spin {
                let ast = if sel { Style::fg(cx.theme.accent).on(cx.theme.bg) } else { dim(cx) };
                for k in 0..2 {
                    c.put(x + k, r.y, up, ast);
                    c.put(x + k, r.y + 2, dn, ast);
                }
                put_text(c, x, r.y + 1, 2, &format!("{:02}", vals[i]), vst);
                if i + 1 < base {
                    c.put(x + 2, r.y + 1, ':', dim(cx));
                }
            } else {
                put_text(c, x, r.y, 2, &format!("{:02}", vals[i]), vst);
                if i + 1 < base {
                    c.put(x + 2, r.y, ':', dim(cx));
                }
            }
            x += 3;
        }
        if h12 {
            let sel = cx.focused && self.field == base;
            put_text(c, x, r.y, 2, if self.t[0] < 12 { "AM" } else { "PM" }, if sel { sel_style(cx, true) } else { plain(cx) });
        }
    }

    fn event(&mut self, r: Rect, ev: &Event, cx: &mut EvCx) -> Res {
        let (nf, h12) = Self::fields(cx.el);
        let base = if h12 { nf - 1 } else { nf };
        match ev {
            Event::Mouse(kind, x, y, _) if r.contains(*x, *y) => {
                let f = ((*x - r.x) / 3).max(0) as usize;
                match kind {
                    MouseKind::Down(0) => {
                        if f < nf {
                            self.field = f;
                        }
                        Res::Handled
                    }
                    MouseKind::WheelUp | MouseKind::WheelDown => {
                        let f = if f < nf { f } else { self.field };
                        self.nudge(f, *kind == MouseKind::WheelUp, base, cx);
                        Res::Changed
                    }
                    _ => Res::Ignored,
                }
            }
            Event::Key(k, _) => match k {
                Key::Left => {
                    self.field = (self.field + nf - 1) % nf;
                    Res::Handled
                }
                Key::Right => {
                    self.field = (self.field + 1) % nf;
                    Res::Handled
                }
                Key::Up | Key::Down => {
                    self.nudge(self.field, *k == Key::Up, base, cx);
                    Res::Changed
                }
                _ => Res::Ignored,
            },
            _ => Res::Ignored,
        }
    }

    fn describe(&self, cx: &Cx) -> String {
        let (nf, h12) = Self::fields(cx.el);
        let base = if h12 { nf - 1 } else { nf };
        format!("{{\"h\":{},\"m\":{},\"s\":{},\"seconds\":{},\"h12\":{},\"text\":{}}}", self.t[0], self.t[1], self.t[2], base >= 3, h12, jstr(&self.text(base >= 3)))
    }

    fn act(&mut self, action: &str, arg: &str, cx: &mut EvCx) -> Res {
        let (nf, h12) = Self::fields(cx.el);
        let base = if h12 { nf - 1 } else { nf };
        let f = arg.trim().parse::<usize>().unwrap_or(self.field).min(nf - 1);
        match action {
            "inc" | "dec" => {
                self.field = f;
                self.nudge(f, action == "inc", base, cx);
                Res::Changed
            }
            "set" => match parse_hms(arg) {
                Some(t) => {
                    self.t = t;
                    let txt = self.text(base >= 3);
                    cx.set_bound(Value::Text(txt.clone()));
                    cx.emit("change", &txt, -1, 0.0);
                    Res::Changed
                }
                None => Res::Ignored,
            },
            _ => Res::Ignored,
        }
    }
}

// ---- datepicker ------------------------------------------------------------------------------------------

fn leap(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

pub fn month_days(y: i32, m: i32) -> i32 {
    [31, if leap(y) { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31][(m - 1).clamp(0, 11) as usize]
}

/// days since 1970-01-01 (Howard Hinnant), valid for any year
fn civil_days(y: i32, m: i32, d: i32) -> i64 {
    let y = (y - (m <= 2) as i32) as i64;
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m as i64 + if m > 2 { -3 } else { 9 }) + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// 0 = Sunday
pub fn weekday(y: i32, m: i32, d: i32) -> i32 {
    (civil_days(y, m, d) + 4).rem_euclid(7) as i32
}

const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
const DOW: [&str; 7] = ["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"];

#[derive(Default)]
struct DatePicker {
    y: i32,
    m: i32,
    d: i32,
    started: bool,
    gx: i32,
    gy: i32,
    first: i32,
    prev: Rect,
    next: Rect,
}

fn parse_ymd(s: &str) -> Option<(i32, i32, i32)> {
    let mut it = s.trim().split('-').map(|p| p.trim().parse::<i32>().ok());
    let (y, m, d) = (it.next()??, it.next()??, it.next()??);
    ((1..=12).contains(&m) && (1..=31).contains(&d)).then_some((y, m, d.min(month_days(y, m))))
}

impl DatePicker {
    fn pull(&mut self, cx: &Cx) {
        if !self.started {
            self.started = true;
            (self.y, self.m, self.d) = parse_ymd(&cx.text_attr("value", "")).unwrap_or((2026, 1, 1));
        }
        if let Some(v) = cx.el.attr("bind").and_then(|k| cx.model_text(k)).and_then(|s| parse_ymd(&s)) {
            (self.y, self.m, self.d) = v;
        }
    }

    fn text(&self) -> String {
        format!("{:04}-{:02}-{:02}", self.y, self.m, self.d)
    }

    fn clamp(&mut self) {
        self.d = self.d.clamp(1, month_days(self.y, self.m));
    }

    fn add_days(&mut self, n: i32) {
        let step = n.signum();
        for _ in 0..n.abs() {
            self.d += step;
            if self.d < 1 {
                self.add_months(-1);
                self.d = month_days(self.y, self.m);
            } else if self.d > month_days(self.y, self.m) {
                self.add_months(1);
                self.d = 1;
            }
        }
    }

    fn add_months(&mut self, n: i32) {
        let total = self.y * 12 + (self.m - 1) + n;
        self.y = total.div_euclid(12);
        self.m = total.rem_euclid(12) + 1;
        self.clamp();
    }

    fn changed(&mut self, cx: &mut EvCx) -> Res {
        let t = self.text();
        cx.set_bound(Value::Text(t.clone()));
        cx.emit("change", &t, self.d as i64, self.y as f64 * 10000.0 + self.m as f64 * 100.0 + self.d as f64);
        Res::Changed
    }
}

impl Widget for DatePicker {
    fn draw(&mut self, c: &mut Canvas, r: Rect, cx: &Cx) {
        self.pull(cx);
        self.prev = Rect::new(0, 0, 0, 0);
        self.next = Rect::new(0, 0, 0, 0);
        if r.w < 21 || r.h < 3 {
            return;
        }
        let t = cx.theme;
        let gx = r.x + (r.w - 20) / 2;
        let title = format!("{} {}", MONTHS[(self.m - 1) as usize], self.y);
        let tw = str_width(&title) as i32;
        put_text(c, r.x + (r.w - tw) / 2, r.y, tw + 1, &title, Style::fg(t.accent).on(t.bg).with(BOLD));
        self.prev = Rect::new(gx, r.y, 2, 1);
        self.next = Rect::new(gx + 18, r.y, 2, 1);
        put_text(c, gx, r.y, 2, if cx.unicode { "‹" } else { "<" }, dim(cx));
        put_text(c, gx + 19, r.y, 1, if cx.unicode { "›" } else { ">" }, dim(cx));
        for (i, n) in DOW.iter().enumerate() {
            put_text(c, gx + i as i32 * 3, r.y + 1, 2, n, dim(cx));
        }
        self.gx = gx;
        self.gy = r.y + 2;
        self.first = weekday(self.y, self.m, 1);
        let (mut row, mut col) = (0, self.first);
        for day in 1..=month_days(self.y, self.m) {
            let y = self.gy + row;
            if y >= r.y + r.h {
                break;
            }
            let st = if day == self.d { sel_style(cx, cx.focused) } else { plain(cx) };
            put_text(c, gx + col * 3, y, 2, &format!("{day:2}"), st);
            col += 1;
            if col > 6 {
                col = 0;
                row += 1;
            }
        }
    }

    fn event(&mut self, _r: Rect, ev: &Event, cx: &mut EvCx) -> Res {
        match ev {
            Event::Mouse(MouseKind::Down(0), x, y, _) => {
                if self.prev.contains(*x, *y) {
                    self.add_months(-1);
                    return self.changed(cx);
                }
                if self.next.contains(*x, *y) {
                    self.add_months(1);
                    return self.changed(cx);
                }
                let (col, row) = ((*x - self.gx).div_euclid(3), *y - self.gy);
                let day = row * 7 + col - self.first + 1;
                if !(0..=6).contains(&col) || row < 0 || day < 1 || day > month_days(self.y, self.m) || (*x - self.gx) % 3 == 2 {
                    return Res::Ignored;
                }
                if day == self.d {
                    return Res::Handled;
                }
                self.d = day;
                self.changed(cx)
            }
            Event::Key(k, _) => {
                match k {
                    Key::Left => self.add_days(-1),
                    Key::Right => self.add_days(1),
                    Key::Up => self.add_days(-7),
                    Key::Down => self.add_days(7),
                    Key::PageUp => self.add_months(-1),
                    Key::PageDown => self.add_months(1),
                    Key::Char('[') => {
                        self.y -= 1;
                        self.clamp();
                    }
                    Key::Char(']') => {
                        self.y += 1;
                        self.clamp();
                    }
                    _ => return Res::Ignored,
                }
                self.changed(cx)
            }
            _ => Res::Ignored,
        }
    }

    fn describe(&self, _cx: &Cx) -> String {
        format!("{{\"y\":{},\"m\":{},\"d\":{},\"first\":{},\"days\":{},\"month\":{},\"text\":{}}}", self.y, self.m, self.d, weekday(self.y, self.m, 1), month_days(self.y, self.m), jstr(MONTHS[(self.m - 1) as usize]), jstr(&self.text()))
    }

    fn act(&mut self, action: &str, arg: &str, cx: &mut EvCx) -> Res {
        match action {
            "day" => match arg.trim().parse::<i32>() {
                Ok(d) if (1..=month_days(self.y, self.m)).contains(&d) => {
                    self.d = d;
                    self.changed(cx)
                }
                _ => Res::Ignored,
            },
            "month" => {
                self.add_months(arg.trim().parse().unwrap_or(0));
                self.changed(cx)
            }
            "year" => {
                self.y += arg.trim().parse::<i32>().unwrap_or(0);
                self.clamp();
                self.changed(cx)
            }
            "set" => match parse_ymd(arg) {
                Some((y, m, d)) => {
                    (self.y, self.m, self.d) = (y, m, d);
                    self.changed(cx)
                }
                None => Res::Ignored,
            },
            _ => Res::Ignored,
        }
    }
}

// ---- gradientpicker --------------------------------------------------------------------------------------

#[derive(Default)]
struct GradientPicker {
    stops: Vec<(f64, Color)>,
    sel: usize,
    dragging: bool,
    started: bool,
    bar: Rect,
    last: String,
}

fn default_stops() -> Vec<(f64, Color)> {
    vec![(0.0, Color::Rgb(0, 0, 0)), (1.0, Color::Rgb(255, 255, 255))]
}

fn parse_stops(s: &str) -> Option<Vec<(f64, Color)>> {
    let mut v: Vec<(f64, Color)> = s
        .split(',')
        .filter_map(|p| {
            let (pos, col) = p.trim().split_once(':')?;
            Some((pos.trim().parse::<f64>().ok()?.clamp(0.0, 1.0), parse_color(col.trim())?))
        })
        .collect();
    if v.len() < 2 {
        return None;
    }
    v.sort_by(|a, b| a.0.total_cmp(&b.0));
    Some(v)
}

fn sample(stops: &[(f64, Color)], f: f64) -> Color {
    let f = clamp01(f);
    let (Some(first), Some(last)) = (stops.first(), stops.last()) else { return Color::Default };
    if f <= first.0 {
        return first.1;
    }
    if f >= last.0 {
        return last.1;
    }
    for w in stops.windows(2) {
        if f >= w[0].0 && f <= w[1].0 {
            let span = w[1].0 - w[0].0;
            return mix(w[0].1, w[1].1, if span > 0.0 { (f - w[0].0) / span } else { 0.0 });
        }
    }
    last.1
}

impl GradientPicker {
    fn text(&self) -> String {
        self.stops.iter().map(|(p, c)| format!("{}:{}", (p * 1000.0).round() / 1000.0, hex(*c))).collect::<Vec<_>>().join(",")
    }

    fn pull(&mut self, cx: &Cx) {
        if !self.started {
            self.started = true;
            self.stops = parse_stops(&cx.text_attr("value", "")).unwrap_or_else(default_stops);
        }
        if let Some(t) = cx.el.attr("bind").and_then(|k| cx.model_text(k)) {
            if t != self.last && t != self.text() {
                if let Some(s) = parse_stops(&t) {
                    self.stops = s;
                    self.sel = self.sel.min(self.stops.len() - 1);
                }
            }
        }
        self.sel = self.sel.min(self.stops.len().saturating_sub(1));
    }

    /// re-sort after a stop moved, keeping the selection on the same stop
    fn resort(&mut self) {
        let cur = self.stops[self.sel];
        self.stops.sort_by(|a, b| a.0.total_cmp(&b.0));
        self.sel = self.stops.iter().position(|s| s.0 == cur.0 && s.1 == cur.1).unwrap_or(0);
    }

    fn changed(&mut self, cx: &mut EvCx) -> Res {
        let t = self.text();
        self.last = t.clone();
        cx.set_bound(Value::Text(t.clone()));
        cx.emit("change", &t, self.sel as i64, self.stops[self.sel].0);
        Res::Changed
    }

    fn near(&self, f: f64) -> Option<usize> {
        let w = (self.bar.w - 1).max(1) as f64;
        self.stops.iter().enumerate().map(|(i, s)| (i, (s.0 - f).abs())).filter(|(_, d)| d * w <= 1.5).min_by(|a, b| a.1.total_cmp(&b.1)).map(|(i, _)| i)
    }

    fn at(&self, x: i32) -> f64 {
        clamp01((x - self.bar.x) as f64 / (self.bar.w - 1).max(1) as f64)
    }

    fn add(&mut self, pos: f64, col: Color) {
        self.stops.push((pos, col));
        self.stops.sort_by(|a, b| a.0.total_cmp(&b.0));
        self.sel = self.stops.iter().position(|s| s.0 == pos && s.1 == col).unwrap_or(0);
    }
}

impl Widget for GradientPicker {
    fn draw(&mut self, c: &mut Canvas, r: Rect, cx: &Cx) {
        self.pull(cx);
        self.bar = Rect::new(0, 0, 0, 0);
        if r.w < 3 || r.h < 1 {
            return;
        }
        self.bar = Rect::new(r.x, r.y, r.w, 1);
        for x in 0..r.w {
            let col = sample(&self.stops, if r.w > 1 { x as f64 / (r.w - 1) as f64 } else { 0.0 });
            c.put(r.x + x, r.y, ' ', Style::fg(col).on(col));
        }
        if r.h < 2 {
            return;
        }
        for x in 0..r.w {
            c.put(r.x + x, r.y + 1, ' ', plain(cx));
        }
        for (i, (p, _)) in self.stops.iter().enumerate() {
            let x = r.x + (p * (r.w - 1) as f64).round() as i32;
            let sel = i == self.sel;
            let st = if sel { sel_style(cx, cx.focused) } else { dim(cx) };
            c.put(x, r.y + 1, if cx.unicode { '◆' } else if sel { '#' } else { '^' }, st);
        }
        if r.h >= 3 {
            let (p, col) = self.stops[self.sel];
            put_text(c, r.x, r.y + 2, r.w, &format!("stop {}/{}  pos {:.2}  {}", self.sel + 1, self.stops.len(), p, hex(col)), dim(cx));
        }
    }

    fn event(&mut self, r: Rect, ev: &Event, cx: &mut EvCx) -> Res {
        let area = Rect::new(r.x, r.y, r.w, if r.h >= 2 { 2 } else { 1 });
        match ev {
            Event::Mouse(kind, x, y, _) => match kind {
                MouseKind::Down(0) if area.contains(*x, *y) && self.bar.w > 1 => {
                    let f = self.at(*x);
                    match self.near(f) {
                        Some(i) => self.sel = i,
                        None => {
                            let col = sample(&self.stops, f);
                            self.add(f, col);
                        }
                    }
                    self.dragging = true;
                    self.changed(cx)
                }
                MouseKind::Drag(0) if self.dragging && self.bar.w > 1 => {
                    self.stops[self.sel].0 = self.at(*x);
                    self.resort();
                    self.changed(cx)
                }
                MouseKind::Up(_) => {
                    let was = self.dragging;
                    self.dragging = false;
                    if was { Res::Handled } else { Res::Ignored }
                }
                _ => Res::Ignored,
            },
            Event::Key(k, _) => match k {
                Key::Left | Key::Right => {
                    let d = if *k == Key::Left { -0.02 } else { 0.02 };
                    self.stops[self.sel].0 = clamp01(self.stops[self.sel].0 + d);
                    self.resort();
                    self.changed(cx)
                }
                Key::Tab if self.stops.len() > 1 && self.sel + 1 < self.stops.len() => {
                    self.sel += 1;
                    Res::Handled
                }
                Key::Delete | Key::Backspace if self.stops.len() > 2 => {
                    self.stops.remove(self.sel);
                    self.sel = self.sel.min(self.stops.len() - 1);
                    self.changed(cx)
                }
                Key::Char('r') => {
                    self.stops = default_stops();
                    self.sel = 0;
                    self.changed(cx)
                }
                _ => Res::Ignored,
            },
            _ => Res::Ignored,
        }
    }

    fn describe(&self, _cx: &Cx) -> String {
        let stops: Vec<String> = self.stops.iter().map(|(p, c)| format!("[{p:.4},{}]", jstr(&hex(*c)))).collect();
        format!("{{\"stops\":[{}],\"sel\":{},\"text\":{}}}", stops.join(","), self.sel, jstr(&self.text()))
    }

    fn act(&mut self, action: &str, arg: &str, cx: &mut EvCx) -> Res {
        match action {
            "select" => match arg.trim().parse::<usize>() {
                Ok(i) if i < self.stops.len() => {
                    self.sel = i;
                    Res::Handled
                }
                _ => Res::Ignored,
            },
            "move" => match arg.trim().parse::<f64>() {
                Ok(p) if p.is_finite() => {
                    self.stops[self.sel].0 = clamp01(p);
                    self.resort();
                    self.changed(cx)
                }
                _ => Res::Ignored,
            },
            "color" => match parse_color(arg.trim()) {
                Some(col) => {
                    self.stops[self.sel].1 = col;
                    self.changed(cx)
                }
                None => Res::Ignored,
            },
            "add" => match arg.trim().parse::<f64>() {
                Ok(p) if p.is_finite() => {
                    let p = clamp01(p);
                    let col = sample(&self.stops, p);
                    self.add(p, col);
                    self.changed(cx)
                }
                _ => Res::Ignored,
            },
            "remove" if self.stops.len() > 2 => {
                self.stops.remove(self.sel);
                self.sel = self.sel.min(self.stops.len() - 1);
                self.changed(cx)
            }
            "reset" => {
                self.stops = default_stops();
                self.sel = 0;
                self.changed(cx)
            }
            _ => Res::Ignored,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::headless::snapshot;
    use crate::input::Mods;
    use crate::ui::Ui;

    #[test]
    fn all_draw_at_any_size() {
        for spec in [
            "colorpicker value=#336699",
            "timepicker value=13:45:09 seconds=1 look=spinbox",
            "timepicker value=00:05 look=h12",
            "datepicker value=2026-02-28",
            "gradientpicker value=\"0:#ff0000,0.5:#00ff00,1:#0000ff\"",
        ] {
            for (w, h) in [(0, 0), (1, 1), (6, 2), (24, 9), (80, 20)] {
                let mut ui = Ui::from_spec(spec).unwrap();
                let _ = snapshot(&mut ui, w, h);
            }
        }
    }

    #[test]
    fn hsv_roundtrip_and_calendar_math() {
        for (r, g, b) in [(1.0, 0.0, 0.0), (0.2, 0.4, 0.6), (0.9, 0.9, 0.1), (0.5, 0.5, 0.5)] {
            let (h, s, v) = rgb_to_hsv(r, g, b);
            let (r2, g2, b2) = hsv_to_rgb(h, s, v);
            assert!((r - r2).abs() < 1e-9 && (g - g2).abs() < 1e-9 && (b - b2).abs() < 1e-9);
        }
        assert_eq!(weekday(1970, 1, 1), 4); // a Thursday
        assert_eq!(weekday(2026, 11, 22), 0); // a Sunday
        assert_eq!(month_days(2024, 2), 29);
        assert_eq!(month_days(1900, 2), 28);
        assert_eq!(parse_ymd("2025-02-31"), Some((2025, 2, 28)));
        assert_eq!(parse_ymd("2025-13-01"), None);
    }

    #[test]
    fn colorpicker_keys_write_the_bound_hex() {
        let mut ui = Ui::from_spec("col\n  colorpicker id=c bind=col value=#ff0000").unwrap();
        let t = snapshot(&mut ui, 40, 10);
        assert!(t.contains("#ff0000") && t.contains("r:255"), "{t}");
        ui.dispatch(&Event::Key(Key::Left, Mods::NONE));
        let t = snapshot(&mut ui, 40, 10);
        assert!(t.contains("#ff0505") || t.contains("#ff0404") || t.contains("#ff0505"), "{t}");
        ui.dispatch(&Event::ch(']'));
        assert!(!snapshot(&mut ui, 40, 10).contains("#ff0505"));
        ui.dispatch(&Event::ch('r'));
        assert!(snapshot(&mut ui, 40, 10).contains("#ff0000"));
    }

    #[test]
    fn timepicker_fields_wrap_and_h12_flips() {
        let mut ui = Ui::from_spec("col\n  timepicker id=t bind=when value=23:59:59 seconds=1").unwrap();
        assert!(snapshot(&mut ui, 30, 2).contains("23:59:59"));
        ui.dispatch(&Event::Key(Key::Up, Mods::NONE)); // hours wrap
        assert!(snapshot(&mut ui, 30, 2).contains("00:59:59"));
        ui.dispatch(&Event::Key(Key::Right, Mods::NONE));
        ui.dispatch(&Event::Key(Key::Down, Mods::NONE)); // minutes 59 -> 58
        assert!(snapshot(&mut ui, 30, 2).contains("00:58:59"));
        let mut ui = Ui::from_spec("col\n  timepicker value=09:30 look=h12").unwrap();
        assert!(snapshot(&mut ui, 30, 2).contains("09:30 AM"));
        ui.dispatch(&Event::Key(Key::Right, Mods::NONE));
        ui.dispatch(&Event::Key(Key::Right, Mods::NONE));
        ui.dispatch(&Event::Key(Key::Up, Mods::NONE));
        assert!(snapshot(&mut ui, 30, 2).contains("09:30 PM"));
    }

    #[test]
    fn datepicker_moves_across_months_and_clicks() {
        let mut ui = Ui::from_spec("col\n  datepicker id=d bind=day value=2026-01-31").unwrap();
        let t = snapshot(&mut ui, 30, 9);
        assert!(t.contains("January 2026") && t.contains("Su Mo Tu"), "{t}");
        ui.dispatch(&Event::Key(Key::Right, Mods::NONE)); // Jan 31 -> Feb 1
        assert!(snapshot(&mut ui, 30, 9).contains("February 2026"));
        ui.dispatch(&Event::Key(Key::PageDown, Mods::NONE)); // -> March 1
        assert!(snapshot(&mut ui, 30, 9).contains("March 2026"));
        ui.dispatch(&Event::Key(Key::Char('['), Mods::NONE));
        assert!(snapshot(&mut ui, 30, 9).contains("March 2025"));
        // click a day: March 2025 starts on a Saturday, so the 1st is in the last column of the first row
        let gx = (30 - 20) / 2;
        ui.dispatch(&Event::Mouse(MouseKind::Down(0), gx + 6 * 3, 2, Mods::NONE));
        assert_eq!(ui.text_of("day"), "2025-03-01");
    }

    #[test]
    fn gradientpicker_adds_moves_and_removes_stops() {
        let mut ui = Ui::from_spec("col\n  gradientpicker id=g bind=grad").unwrap();
        let t = snapshot(&mut ui, 30, 3);
        assert!(t.contains("stop 1/2"), "{t}");
        ui.dispatch(&Event::Mouse(MouseKind::Down(0), 15, 0, Mods::NONE)); // between the ends: a new stop
        ui.dispatch(&Event::Mouse(MouseKind::Up(0), 15, 0, Mods::NONE));
        assert!(snapshot(&mut ui, 30, 3).contains("stop 2/3"));
        ui.dispatch(&Event::Key(Key::Delete, Mods::NONE));
        assert!(snapshot(&mut ui, 30, 3).contains("/2"));
        assert_eq!(parse_stops("0:#000000,1:#ffffff").unwrap().len(), 2);
        assert_eq!(sample(&parse_stops("0:#000000,1:#ffffff").unwrap(), 0.5), Color::Rgb(128, 128, 128));
    }
}
