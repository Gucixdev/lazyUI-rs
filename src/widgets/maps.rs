//! Map widgets: `worldmap` (flat, zoom and pan), `globe` (a sphere you turn), `minimap` (a page as braille density).
//! The planet itself is `world.rs`; here a shader's pixels become half-block cells (two pixels per cell, `▀` with a
//! colour for each half), or real pixels for the web skin through `Widget::pixels`.

use crate::canvas::{rgb, Canvas, Color, Rect, Style};
use crate::gfx;
use crate::input::{Event, Key, MouseKind};
use crate::native::{Cx, Def, EvCx, Widget};
use crate::ui::{jstr, Value};
use crate::widgets::Res;
use crate::world::{self, Globe, Look, View};

pub const DEFS: &[Def] = &[
    Def {
        name: "worldmap",
        doc: "the flat world: wheel/+ - zoom, drag pans, click picks a country (bind=KEY gets its ISO code, emits pick); mode=climate|political|grey borders=0 night=1 grid=1 labels=1 lang=pl|en select=ISO heat=\"PL:38,DE:83\"|heatbind=KEY lo hi ramp=c1,c2,c3 sun=DEG|auto",
        focus: true,
        rows: None,
        live: 0.0,
        make: || Box::new(MapW::new(false)),
    },
    Def {
        name: "globe",
        doc: "the planet as a sphere: drag or arrows turn it, wheel zooms, click picks a country (bind=KEY, emits pick); lon tilt zoom, and the options of worldmap",
        focus: true,
        rows: None,
        live: 0.0,
        make: || Box::new(MapW::new(true)),
    },
    Def {
        name: "minimap",
        doc: "a page as braille density with the viewport marked: bind=KEY|values (0..max per line) offset page marks=3,9 (numbers or model keys) max; click or drag seeks: seek=KEY gets the line, emits seek",
        focus: true,
        rows: None,
        live: 0.0,
        make: || Box::new(Minimap::default()),
    },
];

/// seconds since the epoch: the model's `sys.utc` when the page feeds it, else the clock (not in the browser)
fn utc(cx: &Cx) -> f64 {
    if let Some(v) = cx.model_num("sys.utc") {
        return v;
    }
    #[cfg(not(target_arch = "wasm32"))]
    if let Ok(d) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        return d.as_secs_f64();
    }
    0.0
}

/// the longitude the sun stands over at that time
fn sun_lon_at(secs: f64) -> f64 {
    (12.0 - (secs.rem_euclid(86400.0)) / 3600.0) * 15.0
}

fn polish(cx: &Cx) -> bool {
    match cx.attr("lang") {
        Some(l) => l.to_ascii_lowercase().starts_with("pl"),
        #[cfg(not(target_arch = "wasm32"))]
        None => std::env::var("LC_ALL").ok().filter(|s| !s.is_empty()).or_else(|| std::env::var("LANG").ok()).is_some_and(|l| l.to_ascii_lowercase().starts_with("pl")),
        #[cfg(target_arch = "wasm32")]
        None => false,
    }
}

fn rgb_of(cx: &Cx, name: &str) -> Option<world::Rgb> {
    gfx::colour(cx.theme, name).and_then(gfx::rgb_of).map(|(r, g, b)| [r as f64, g as f64, b as f64])
}

/// `PL:38,DE:83` -> a value per country
fn parse_heat(s: &str) -> Vec<Option<f64>> {
    let mut heat = vec![None; world::count()];
    for part in s.split(',') {
        if let Some((iso, v)) = part.split_once(':') {
            if let (Some(i), Ok(v)) = (world::find_iso(iso), v.trim().parse::<f64>()) {
                if v.is_finite() {
                    heat[i] = Some(v);
                }
            }
        }
    }
    heat
}

fn look_of(cx: &Cx, globe: bool) -> Look {
    let heat_text = cx.attr("heatbind").and_then(|k| cx.model_text(k)).or_else(|| cx.attr("heat").map(|s| s.to_string()));
    let heat = heat_text.map(|t| parse_heat(&t)).unwrap_or_default();
    let vals: Vec<f64> = heat.iter().flatten().copied().collect();
    let (dlo, dhi) = (vals.iter().cloned().fold(f64::MAX, f64::min), vals.iter().cloned().fold(f64::MIN, f64::max));
    let sel = cx.attr("select").map(|s| s.to_string()).or_else(|| cx.attr("bind").and_then(|k| cx.model_text(k))).and_then(|s| world::find_iso(&s));
    let mode = cx.attr("mode").unwrap_or("climate");
    let sun_auto = cx.attr("sun") == Some("auto");
    let sun_lon = if sun_auto { sun_lon_at(utc(cx)) } else if globe { 0.0 } else { cx.num_attr("sun", 0.0) };
    Look {
        political: mode == "political",
        grey: mode == "grey",
        borders: cx.attr("borders").map(|v| !matches!(v, "0" | "false" | "no" | "off")).unwrap_or(true),
        night: cx.flag("night"),
        grid: cx.flag("grid"),
        sel,
        lo: cx.num_attr("lo", if vals.is_empty() { 0.0 } else { dlo }),
        hi: cx.num_attr("hi", if vals.is_empty() { 1.0 } else { dhi }),
        heat,
        ramp: cx.list_attr("ramp").iter().filter_map(|n| rgb_of(cx, n)).collect(),
        sun_lon,
    }
}

fn pick_json(i: Option<usize>) -> String {
    match i.and_then(world::info) {
        Some(c) => format!("{{\"iso\":{},\"en\":{},\"pl\":{},\"continent\":{},\"pop_m\":{},\"gdp_bn\":{}}}", jstr(c.iso), jstr(c.en), jstr(c.pl), jstr(c.continent), c.pop_m, c.gdp_bn),
        None => "null".into(),
    }
}

struct MapW {
    globe: bool,
    view: View,
    gl: Globe,
    started: bool,
    /// where a drag last was, in pixels; whether it moved (a press that did not move is a click)
    drag: Option<(f64, f64)>,
    moved: bool,
    /// the size of the last web canvas: actions from the page are in its pixels
    px: (i32, i32),
    /// the last picture: its inputs, and the RGBA bytes
    cache: Option<(Look, View, Globe, usize, usize)>,
    buf: Vec<u8>,
}

impl MapW {
    fn new(globe: bool) -> MapW {
        MapW { globe, view: View::default(), gl: Globe::default(), started: false, drag: None, moved: false, px: (0, 0), cache: None, buf: Vec::new() }
    }

    /// the starting view comes from the attributes once; after that the user owns it
    fn start(&mut self, cx: &Cx) {
        if self.started {
            return;
        }
        self.started = true;
        self.view = View { zoom: cx.num_attr("zoom", 1.0), clon: cx.num_attr("clon", 0.0), clat: cx.num_attr("clat", 0.0) }.clamped();
        self.gl = Globe { lon0: cx.num_attr("lon", 0.0), tilt: cx.num_attr("tilt", 15.0), sun_deg: 0.0, zoom: cx.num_attr("zoom", 1.0) }.clamped();
    }

    fn sun(&mut self, cx: &Cx) {
        if cx.attr("sun") == Some("auto") {
            self.gl.sun_deg = sun_lon_at(utc(cx)) - self.gl.lon0 + 51.57;
        } else {
            self.gl.sun_deg = cx.num_attr("sun", 0.0);
        }
    }

    fn render(&mut self, look: &Look, w: usize, h: usize) {
        let key = (look.clone(), self.view, self.gl, w, h);
        if self.cache.as_ref() == Some(&key) {
            return;
        }
        if self.globe {
            world::render_globe(look, &self.gl, w, h, &mut self.buf);
        } else {
            world::render_map(look, &self.view, w, h, &mut self.buf);
        }
        self.cache = Some(key);
    }

    fn country_at(&self, pw: f64, ph: f64, px: f64, py: f64) -> Option<usize> {
        if self.globe {
            world::globe_country_at(&self.gl, pw, ph, px, py)
        } else {
            world::map_country_at(pw, ph, &self.view, px, py)
        }
    }

    fn zoom(&mut self, pw: f64, ph: f64, factor: f64, px: f64, py: f64) {
        if self.globe {
            self.gl.zoom *= factor;
            self.gl = self.gl.clamped();
        } else {
            self.view.zoom_at(pw, ph, factor, px, py);
        }
    }

    fn rotate(&mut self, dlon: f64, dtilt: f64) {
        self.gl.lon0 += dlon;
        self.gl.tilt += dtilt;
        self.gl = self.gl.clamped();
    }

    /// a drag from one pixel to another
    fn drag_to(&mut self, pw: f64, ph: f64, from: (f64, f64), to: (f64, f64)) {
        if self.globe {
            let d = self.gl.deg_per_px(pw, ph);
            self.rotate(-(to.0 - from.0) * d, (to.1 - from.1) * d);
        } else {
            self.view.grab(pw, ph, from, to);
        }
    }

    fn picked(&self, i: Option<usize>, cx: &mut EvCx) {
        let iso = i.and_then(world::info).map(|c| c.iso).unwrap_or("");
        cx.emit("pick", iso, i.map(|i| i as i64).unwrap_or(-1), 0.0);
        cx.set_bound(Value::Text(iso.to_string()));
    }
}

fn lum(c: &Canvas, x: i32, y: i32) -> f64 {
    let Some(cell) = c.get(x, y) else { return 0.0 };
    let (f, b) = (gfx::rgb_of(cell.style.fg), gfx::rgb_of(cell.style.bg));
    let l = |c: (u8, u8, u8)| (299.0 * c.0 as f64 + 587.0 * c.1 as f64 + 114.0 * c.2 as f64) / 1000.0;
    match (f, b) {
        (Some(f), Some(b)) => (l(f) + l(b)) / 2.0,
        (Some(f), None) => l(f),
        (None, Some(b)) => l(b),
        _ => 0.0,
    }
}

impl Widget for MapW {
    fn draw(&mut self, c: &mut Canvas, r: Rect, cx: &Cx) {
        if r.w < 4 || r.h < 2 {
            return;
        }
        self.start(cx);
        self.sun(cx);
        let look = look_of(cx, self.globe);
        let (w, h) = (r.w as usize, (r.h * 2) as usize);
        self.render(&look, w, h);
        let back = gfx::rgb_of(cx.theme.bg).map(|(r, g, b)| [r as f64, g as f64, b as f64]).unwrap_or([0.0; 3]);
        let px = |x: usize, y: usize| -> Option<Color> {
            let i = (y * w + x) * 4;
            let a = self.buf[i + 3] as f64 / 255.0;
            (a > 0.03).then(|| {
                let m = |k: usize| (self.buf[i + k] as f64 * a + back[k] * (1.0 - a)) as u8;
                rgb(m(0), m(1), m(2))
            })
        };
        let avg = |a: Color, b: Color| match (gfx::rgb_of(a), gfx::rgb_of(b)) {
            (Some(a), Some(b)) => rgb(((a.0 as u16 + b.0 as u16) / 2) as u8, ((a.1 as u16 + b.1 as u16) / 2) as u8, ((a.2 as u16 + b.2 as u16) / 2) as u8),
            _ => a,
        };
        for y in 0..r.h as usize {
            for x in 0..w {
                let (top, bot) = (px(x, 2 * y), px(x, 2 * y + 1));
                let (cx_, cy_) = (r.x + x as i32, r.y + y as i32);
                match (top, bot, c.caps.unicode) {
                    (None, None, _) => {}
                    (Some(t), Some(b), true) => c.put(cx_, cy_, '▀', Style::new(t, b)),
                    (Some(t), None, true) => c.put(cx_, cy_, '▀', Style::new(t, Color::Default)),
                    (None, Some(b), true) => c.put(cx_, cy_, '▄', Style::new(b, Color::Default)),
                    (t, b, false) => {
                        let col = match (t, b) {
                            (Some(t), Some(b)) => avg(t, b),
                            (Some(t), None) => t,
                            (None, Some(b)) => b,
                            _ => continue,
                        };
                        c.put(cx_, cy_, ' ', Style::new(Color::Default, col));
                    }
                }
            }
        }
        if cx.flag("labels") {
            let (pw, ph) = (w as f64, h as f64);
            let ls = if self.globe { world::globe_labels(&self.gl, pw, ph, polish(cx), 1.0, 2.0) } else { world::map_labels(&self.view, pw, ph, polish(cx), 1.0, 2.0) };
            for l in ls {
                let n = l.text.chars().count() as i32;
                let (x0, row) = (r.x + (l.x - n as f64 / 2.0).floor() as i32, r.y + (l.y / 2.0).floor() as i32);
                for (k, ch) in l.text.chars().enumerate() {
                    let x = x0 + k as i32;
                    let dark = lum(c, x, row) > 150.0;
                    let bg = c.get(x, row).map(|cell| cell.style.bg).unwrap_or(Color::Default);
                    c.put(x, row, ch, Style::new(if dark { rgb(14, 24, 16) } else { rgb(240, 244, 232) }, bg));
                }
            }
        }
    }

    fn event(&mut self, r: Rect, ev: &Event, cx: &mut EvCx) -> Res {
        let (pw, ph) = (r.w as f64, (r.h * 2) as f64);
        let at = |x: i32, y: i32| ((x - r.x) as f64 + 0.5, 2.0 * (y - r.y) as f64 + 1.0);
        match ev {
            Event::Mouse(MouseKind::WheelUp | MouseKind::WheelDown, x, y, _) if r.contains(*x, *y) => {
                let f = if matches!(ev, Event::Mouse(MouseKind::WheelUp, ..)) { 1.35 } else { 1.0 / 1.35 };
                let (px, py) = at(*x, *y);
                self.zoom(pw, ph, f, px, py);
                Res::Changed
            }
            Event::Mouse(MouseKind::Down(0), x, y, _) if r.contains(*x, *y) => {
                self.drag = Some(at(*x, *y));
                self.moved = false;
                Res::Handled
            }
            Event::Mouse(MouseKind::Drag(0), x, y, _) => match self.drag {
                Some(from) => {
                    let to = at(*x, *y);
                    if (to.0 - from.0).abs() + (to.1 - from.1).abs() < 1.0 {
                        return Res::Handled;
                    }
                    self.drag_to(pw, ph, from, to);
                    self.drag = Some(to);
                    self.moved = true;
                    Res::Changed
                }
                None => Res::Ignored,
            },
            Event::Mouse(MouseKind::Up(0), x, y, _) if self.drag.is_some() => {
                self.drag = None;
                if self.moved {
                    return Res::Handled;
                }
                let (px, py) = at(*x, *y);
                let i = self.country_at(pw, ph, px, py);
                self.picked(i, cx);
                Res::Activate
            }
            Event::Key(k, m) if !m.ctrl && !m.alt => {
                let step = if self.globe { 10.0 } else { 20.0 / self.view.zoom };
                match k {
                    Key::Char('+' | '=') => self.zoom(pw, ph, 1.35, pw / 2.0, ph / 2.0),
                    Key::Char('-' | '_') => self.zoom(pw, ph, 1.0 / 1.35, pw / 2.0, ph / 2.0),
                    Key::Char('0') | Key::Home => {
                        self.view = View::default();
                        self.gl.zoom = 1.0;
                        self.gl.tilt = 15.0;
                        self.gl.lon0 = 0.0;
                    }
                    Key::Left if self.globe => self.rotate(-step, 0.0),
                    Key::Right if self.globe => self.rotate(step, 0.0),
                    Key::Up if self.globe => self.rotate(0.0, step),
                    Key::Down if self.globe => self.rotate(0.0, -step),
                    Key::Left => self.view.clon -= step,
                    Key::Right => self.view.clon += step,
                    Key::Up => self.view.clat += step / 2.0,
                    Key::Down => self.view.clat -= step / 2.0,
                    _ => return Res::Ignored,
                }
                self.view = self.view.clamped();
                Res::Changed
            }
            _ => Res::Ignored,
        }
    }

    fn describe(&self, cx: &Cx) -> String {
        let look = look_of(cx, self.globe);
        format!(
            "{{\"globe\":{},\"zoom\":{},\"clon\":{},\"clat\":{},\"lon0\":{},\"tilt\":{},\"sun\":{:.1},\"sel\":{}}}",
            self.globe,
            if self.globe { self.gl.zoom } else { self.view.zoom },
            self.view.clon,
            self.view.clat,
            self.gl.lon0,
            self.gl.tilt,
            // the picture changes with the clock too, so the page redraws when the sun has moved half a degree
            if cx.attr("sun") == Some("auto") { (sun_lon_at(utc(cx)) * 2.0).round() / 2.0 } else { cx.num_attr("sun", 0.0) },
            pick_json(look.sel)
        )
    }

    /// `semantic actions` from the page, in pixels of its canvas: zoom "factor,x,y", pan "x0,y0,x1,y1", pick "x,y", rotate "dlon,dtilt", reset
    fn act(&mut self, action: &str, arg: &str, cx: &mut EvCx) -> Res {
        let n = gfx::numbers(arg);
        let (pw, ph) = (self.px.0.max(1) as f64, self.px.1.max(1) as f64);
        match (action, n.as_slice()) {
            ("zoom", [f, x, y]) => self.zoom(pw, ph, *f, *x, *y),
            ("pan", [x0, y0, x1, y1]) => self.drag_to(pw, ph, (*x0, *y0), (*x1, *y1)),
            ("rotate", [a, b]) => self.rotate(*a, *b),
            ("reset", _) => {
                self.view = View::default();
                self.gl = Globe { sun_deg: self.gl.sun_deg, ..Globe::default() };
            }
            ("pick", [x, y]) => {
                let i = self.country_at(pw, ph, *x, *y);
                self.picked(i, cx);
                return Res::Activate;
            }
            _ => return Res::Ignored,
        }
        Res::Changed
    }

    /// The picture for a canvas of w x h pixels: RGBA bytes, and the country names that fit as JSON `[{"x","y","t"}]`.
    fn pixels(&mut self, cx: &Cx, w: i32, h: i32) -> Option<(Vec<u8>, String)> {
        let (w, h) = (w.clamp(8, 4096), h.clamp(8, 4096));
        self.start(cx);
        self.sun(cx);
        self.px = (w, h);
        let look = look_of(cx, self.globe);
        self.render(&look, w as usize, h as usize);
        let mut names = String::from("[");
        if cx.flag("labels") {
            let (pw, ph) = (w as f64, h as f64);
            let ls = if self.globe { world::globe_labels(&self.gl, pw, ph, polish(cx), 7.0, 14.0) } else { world::map_labels(&self.view, pw, ph, polish(cx), 7.0, 14.0) };
            let parts: Vec<String> = ls.iter().map(|l| format!("{{\"x\":{:.1},\"y\":{:.1},\"t\":{}}}", l.x, l.y, jstr(&l.text))).collect();
            names.push_str(&parts.join(","));
        }
        names.push(']');
        Some((self.buf.clone(), names))
    }
}

// ---- minimap -----------------------------------------------------------------------------------------------------------

#[derive(Default)]
struct Minimap {
    drag: bool,
    /// the lines the density list had at the last draw (the page's click maps rows back to lines)
    n: usize,
}

/// a number attribute that may also name a model key: `offset=12` or `offset=scroll`
fn num_or_key(cx: &Cx, k: &str) -> f64 {
    match cx.attr(k) {
        Some(s) => s.trim().parse::<f64>().ok().or_else(|| cx.model_num(s.trim())).filter(|x| x.is_finite()).unwrap_or(0.0),
        None => 0.0,
    }
}

fn density(cx: &Cx) -> Vec<f64> {
    let v = cx.series();
    let top = cx.num_attr("max", if v.iter().all(|x| *x <= 1.0) { 1.0 } else { 255.0 }).max(1e-9);
    v.iter().map(|x| if x.is_finite() { (x / top).clamp(0.0, 1.0) } else { 0.0 }).collect()
}

fn marks(cx: &Cx) -> Vec<usize> {
    match cx.attr("marks") {
        Some(s) => s.split(',').filter_map(|p| p.trim().parse::<f64>().ok().or_else(|| cx.model_num(p.trim()))).filter(|x| x.is_finite() && *x >= 0.0).map(|x| x as usize).collect(),
        None => Vec::new(),
    }
}

impl Widget for Minimap {
    fn draw(&mut self, c: &mut Canvas, r: Rect, cx: &Cx) {
        let d = density(cx);
        self.n = d.len();
        if r.w < 1 || r.h < 1 || d.is_empty() {
            return;
        }
        let (offset, page) = (num_or_key(cx, "offset"), num_or_key(cx, "page"));
        let marks = marks(cx);
        const BIT: [[u8; 2]; 4] = [[0x01, 0x08], [0x02, 0x10], [0x04, 0x20], [0x40, 0x80]];
        let per_row = d.len() as f64 / r.h as f64;
        let t = cx.theme;
        for y in 0..r.h {
            let l0 = (y as f64 * per_row).floor() as usize;
            let l1 = (((y + 1) as f64 * per_row).floor() as usize).max(l0 + 1);
            let inview = page > 0.0 && l1 as f64 > offset && (l0 as f64) < offset + page;
            let st = if inview { Style::new(t.accent, t.bg) } else { Style::fg(t.dim) };
            for x in 0..r.w {
                let mut bits = 0u8;
                for (dd, row) in BIT.iter().enumerate() {
                    let (a, b) = (l0 + (l1 - l0) * dd / 4, (l0 + (l1 - l0) * (dd + 1) / 4).max(l0 + (l1 - l0) * dd / 4 + 1));
                    let (a, b) = (a.min(d.len()), b.min(d.len()));
                    let mean = if b > a { d[a..b].iter().sum::<f64>() / (b - a) as f64 } else { 0.0 }; // a minimap row is a bar as long as its text
                    for (h, bit) in row.iter().enumerate() {
                        if (x as f64 * 2.0 + h as f64 + 0.5) / (r.w as f64 * 2.0) < mean {
                            bits |= bit;
                        }
                    }
                }
                c.put(r.x + x, r.y + y, gfx::Braille::glyph(bits, c.caps.unicode), st);
            }
            if marks.iter().any(|&m| m >= l0 && m < l1) {
                c.put(r.x + r.w - 1, r.y + y, if c.caps.unicode { '◀' } else { '<' }, Style::fg(t.warn));
            }
        }
    }

    fn event(&mut self, r: Rect, ev: &Event, cx: &mut EvCx) -> Res {
        let seek = |me: &Minimap, y: i32, cx: &mut EvCx| {
            let line = (((y - r.y).clamp(0, r.h - 1) as f64 + 0.5) / r.h as f64 * me.n as f64).floor();
            cx.emit("seek", "", line as i64, line);
            if let Some(k) = cx.attr("seek") {
                let k = k.to_string();
                cx.set.push((k, Value::Num(line)));
            }
        };
        match ev {
            Event::Mouse(MouseKind::Down(0), x, y, _) if r.contains(*x, *y) && self.n > 0 => {
                self.drag = true;
                seek(self, *y, cx);
                Res::Changed
            }
            Event::Mouse(MouseKind::Drag(0), _, y, _) if self.drag && self.n > 0 => {
                seek(self, *y, cx);
                Res::Changed
            }
            Event::Mouse(MouseKind::Up(0), ..) if self.drag => {
                self.drag = false;
                Res::Handled
            }
            _ => Res::Ignored,
        }
    }

    fn describe(&self, cx: &Cx) -> String {
        let d: Vec<String> = density(cx).iter().map(|x| format!("{:.3}", x)).collect();
        let m: Vec<String> = marks(cx).iter().map(|x| x.to_string()).collect();
        format!("{{\"series\":[{}],\"offset\":{},\"page\":{},\"marks\":[{}]}}", d.join(","), num_or_key(cx, "offset"), num_or_key(cx, "page"), m.join(","))
    }

    fn act(&mut self, action: &str, arg: &str, cx: &mut EvCx) -> Res {
        match (action, arg.trim().parse::<f64>()) {
            ("seek", Ok(line)) if line.is_finite() => {
                let line = line.floor().max(0.0);
                cx.emit("seek", "", line as i64, line);
                if let Some(k) = cx.attr("seek") {
                    let k = k.to_string();
                    cx.set.push((k, Value::Num(line)));
                }
                Res::Changed
            }
            _ => Res::Ignored,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::canvas::{Canvas, Caps};
    use crate::input::{Event, Key, Mods, MouseKind};
    use crate::ui::Ui;

    fn frame(ui: &mut Ui, w: i32, h: i32) -> String {
        let mut c = Canvas::new(w, h, Caps::FULL);
        ui.render(&mut c);
        c.to_text()
    }

    /// every cell with its colours: a turned globe has the same glyphs but other colours
    fn cells(ui: &mut Ui, w: i32, h: i32) -> String {
        let mut c = Canvas::new(w, h, Caps::FULL);
        ui.render(&mut c);
        (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).map(|(x, y)| format!("{:?}", c.get(x, y))).collect()
    }

    fn click(ui: &mut Ui, x: i32, y: i32) {
        ui.dispatch(&Event::Mouse(MouseKind::Down(0), x, y, Mods::NONE));
        ui.dispatch(&Event::Mouse(MouseKind::Up(0), x, y, Mods::NONE));
    }

    #[test]
    fn worldmap_draws_half_blocks_and_picks_a_country() {
        let mut ui = Ui::from_spec("col\n  worldmap id=w bind=pick labels=1 lang=en").unwrap();
        let t = frame(&mut ui, 100, 30);
        assert!(t.contains('▀') || t.contains('▄'), "{t}");
        assert!(t.contains("Russia") || t.contains("Canada") || t.contains("China"), "big countries are named:\n{t}");
        // 100x30 cells = 100x60 pixels, the 2:1 map is 100x50 pixels with a 5 pixel margin: Poland (19E 52N) is pixel (55, 15.6)
        click(&mut ui, 55, 7);
        assert_eq!(ui.text_of("pick"), "POL");
        click(&mut ui, 41, 10); // mid Atlantic (30W 30N)
        assert_eq!(ui.text_of("pick"), "", "sea picks nothing");
        let ev = ui.take_events();
        assert!(ev.iter().any(|e| e.kind == "pick" && e.text == "POL"));
    }

    #[test]
    fn worldmap_zooms_with_the_wheel_and_keys() {
        let mut ui = Ui::from_spec("col\n  worldmap id=w").unwrap();
        let a = cells(&mut ui, 80, 24);
        ui.dispatch(&Event::Mouse(MouseKind::WheelUp, 40, 12, Mods::NONE));
        let b = cells(&mut ui, 80, 24);
        assert_ne!(a, b, "zoomed in");
        ui.dispatch(&Event::Key(Key::Char('0'), Mods::NONE));
        assert_eq!(cells(&mut ui, 80, 24), a, "0 resets");
    }

    #[test]
    fn globe_draws_and_turns() {
        let mut ui = Ui::from_spec("col\n  globe id=g lon=19 tilt=20").unwrap();
        assert!(frame(&mut ui, 60, 20).contains('▀'));
        let a = cells(&mut ui, 60, 20);
        ui.dispatch(&Event::Key(Key::Right, Mods::NONE));
        assert_ne!(cells(&mut ui, 60, 20), a, "arrows turn it");
        // a drag turns it too
        ui.dispatch(&Event::Mouse(MouseKind::Down(0), 30, 10, Mods::NONE));
        ui.dispatch(&Event::Mouse(MouseKind::Drag(0), 36, 10, Mods::NONE));
        ui.dispatch(&Event::Mouse(MouseKind::Up(0), 36, 10, Mods::NONE));
        assert!(ui.take_events().iter().all(|e| e.kind != "pick"), "a drag is not a click");
    }

    #[test]
    fn maps_survive_tiny_and_huge_canvases() {
        for spec in ["worldmap labels=1 night=1 grid=1", "globe labels=1 mode=political", "minimap values=1,2,3"] {
            for (w, h) in [(1, 1), (3, 1), (5, 2), (200, 60)] {
                let mut ui = Ui::from_spec(&format!("col\n  {spec}")).unwrap();
                frame(&mut ui, w, h);
            }
        }
    }

    #[test]
    fn heat_and_select_colour_countries() {
        let mut ui = Ui::from_spec("col\n  worldmap heat=\"POL:1,DEU:0\" select=POL").unwrap();
        let mut c = Canvas::new(100, 30, Caps::FULL);
        ui.render(&mut c);
        let cell = c.get(55, 5 + 10).unwrap();
        assert!(matches!(cell.style.fg, crate::canvas::Color::Rgb(..)));
    }

    #[test]
    fn minimap_marks_the_viewport_and_seeks() {
        let mut ui = Ui::from_spec("col\n  minimap id=m values=255,255,255,255,128,128,128,128,10,10,10,10 offset=4 page=4 marks=1 seek=line size=6 max=255").unwrap();
        let t = frame(&mut ui, 20, 6);
        assert!(t.contains('◀'), "{t}");
        assert!(t.chars().any(|c| ('\u{2800}'..='\u{28ff}').contains(&c)));
        click(&mut ui, 3, 5);
        assert_eq!(ui.text_of("line"), "11", "the bottom row is the last lines");
    }
}

#[cfg(test)]
mod pixel_tests {
    use crate::canvas::{Canvas, Caps};
    use crate::ui::Ui;

    #[test]
    fn web_pixels_carry_labels() {
        let mut ui = Ui::from_spec("col\n  worldmap id=atlas labels=1 lang=pl").unwrap();
        let mut c = Canvas::new(60, 20, Caps::FULL);
        ui.render(&mut c);
        let (bytes, names) = ui.pixels("atlas", 544, 504).expect("pixels");
        assert_eq!(bytes.len(), 544 * 504 * 4);
        assert!(names.contains("Rosja") || names.contains("RUS"), "{names}");
    }
}
