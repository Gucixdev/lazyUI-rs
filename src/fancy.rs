//! The fancy widgets of lazytui: radar, rain, scramble, tagcloud, webchart, piechart. All are pure functions of the
//! element's attributes and the Ui clock, so a headless run is repeatable. Round shapes are drawn on a grid of square
//! pixels (a cell is one wide, two tall): lines as braille dots, fills as half blocks.

use crate::canvas::{str_width, Canvas, Color, Rect, Style, BOLD, REVERSE};
use crate::gfx::{clamp01, mix, Braille};
use crate::native::{Cx, Def, Pure, Widget};
use std::f64::consts::PI;

fn pure(f: fn(&mut Canvas, Rect, &Cx)) -> Box<dyn Widget> {
    Box::new(Pure(f))
}

pub static DEFS: &[Def] = &[
    Def { name: "radar", doc: "ship radar, a sweep lights the blips: blips=\"bearing:range:label;...\" (bearing deg from north, range 0..1) speed=deg/s color", focus: false, rows: None, live: 0.05, make: || pure(radar) },
    Def { name: "rain", doc: "matrix rain, falling columns with a bright head: seed speed=1", focus: false, rows: None, live: 0.05, make: || pure(rain) },
    Def { name: "scramble", doc: "text that resolves out of random glyphs: text progress=0..1|auto|bind=KEY", focus: false, rows: Some(1), live: 0.05, make: || pure(scramble) },
    Def { name: "tagcloud", doc: "words centred on wrapped lines, weight is emphasis: words weights", focus: false, rows: None, live: 0.0, make: || pure(tagcloud) },
    Def { name: "webchart", doc: "spider chart, one axis per value: values labels max rings color", focus: false, rows: None, live: 0.0, make: || pure(webchart) },
    Def { name: "piechart", doc: "a circle or donut of sectors with a legend: values labels colors donut=1 legend=1", focus: false, rows: None, live: 0.0, make: || pure(piechart) },
];

fn hash32(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846ca68b);
    x ^= x >> 16;
    x
}

fn tick(x: f64) -> u32 {
    if x.is_finite() {
        (x % 1e9).floor() as i64 as u32
    } else {
        0
    }
}

/// digits, a few signs and half-width katakana: the glyphs of falling code (ASCII only on a plain terminal)
fn glyph(h: u32, unicode: bool) -> char {
    const ASCII: &[u8] = b"01234567890:.=*+-<>|/\\^~";
    if !unicode {
        return ASCII[h as usize % ASCII.len()] as char;
    }
    let k = h % 60;
    if k < 10 {
        (b'0' + k as u8) as char
    } else if k < 14 {
        b"=+*:"[(k - 10) as usize] as char
    } else {
        char::from_u32(0xFF71 + (k - 14) % 45).unwrap_or('*')
    }
}

fn ground(cx: &Cx) -> Color {
    if matches!(cx.theme.bg, Color::Default) {
        Color::Rgb(0, 0, 0)
    } else {
        cx.theme.bg
    }
}

// ---- wire: braille dots that remember a colour per cell ------------------------------------------------

struct Wire {
    b: Braille,
    col: Vec<Color>,
}

impl Wire {
    fn new(cw: i32, ch: i32) -> Wire {
        Wire { b: Braille::new(cw, ch), col: vec![Color::Default; (cw.max(0) * ch.max(0)) as usize] }
    }

    /// one dot at cell coordinates (x columns, y rows, fractions allowed)
    fn dot(&mut self, x: f64, y: f64, col: Color) {
        let (dx, dy) = ((x * 2.0).floor() as i32, (y * 4.0).floor() as i32);
        if dx < 0 || dy < 0 || dx >= self.b.dots_w() || dy >= self.b.dots_h() {
            return;
        }
        self.b.set(dx, dy);
        self.col[((dy / 4) * self.b.cw + dx / 2) as usize] = col;
    }

    fn line(&mut self, x0: f64, y0: f64, x1: f64, y1: f64, col: Color) {
        let n = (((x1 - x0) * 2.0).abs().max(((y1 - y0) * 4.0).abs()).ceil() as i32).clamp(1, 4000);
        for i in 0..=n {
            let t = i as f64 / n as f64;
            self.dot(x0 + (x1 - x0) * t, y0 + (y1 - y0) * t, col);
        }
    }

    /// a circle of radius `r` pixels around (cx, cy) pixels (pixels are square: y counts two per row)
    fn circle(&mut self, cx: f64, cy: f64, r: f64, col: Color) {
        let seg = ((r * 7.0) as i32 + 24).min(900);
        for i in 0..seg {
            let (a0, a1) = (2.0 * PI * i as f64 / seg as f64, 2.0 * PI * (i + 1) as f64 / seg as f64);
            self.line(cx + r * a0.cos(), (cy + r * a0.sin()) / 2.0, cx + r * a1.cos(), (cy + r * a1.sin()) / 2.0, col);
        }
    }

    fn flush(&self, c: &mut Canvas, at: (i32, i32), unicode: bool, bg: &dyn Fn(i32, i32) -> Color) {
        for j in 0..self.b.ch {
            for i in 0..self.b.cw {
                let bits = self.b.bits(i, j);
                if bits != 0 {
                    c.put(at.0 + i, at.1 + j, Braille::glyph(bits, unicode), Style::fg(self.col[(j * self.b.cw + i) as usize]).on(bg(i, j)));
                }
            }
        }
    }
}

// ---- radar -----------------------------------------------------------------------------------------------

struct Blip {
    bearing: f64,
    range: f64,
    label: String,
}

fn blips(cx: &Cx) -> Vec<Blip> {
    cx.text_attr("blips", "")
        .split(';')
        .filter_map(|s| {
            let mut p = s.splitn(3, ':');
            let (b, r) = (p.next()?.trim().parse::<f64>().ok()?, p.next()?.trim().parse::<f64>().ok()?);
            (b.is_finite() && r.is_finite()).then(|| Blip { bearing: b, range: r, label: p.next().unwrap_or("").trim().to_string() })
        })
        .collect()
}

/// a point on the scope: bearing in degrees from north, clockwise; radius 0..1
fn polar(cx: f64, cy: f64, r: f64, bearing: f64, rad: f64) -> (f64, f64) {
    let a = bearing.to_radians();
    (cx + a.sin() * r * rad, cy - a.cos() * r * rad)
}

fn radar(c: &mut Canvas, r: Rect, cx: &Cx) {
    if r.w < 8 || r.h < 5 {
        return;
    }
    let t = cx.theme;
    let sweep = (cx.time * cx.num_attr("speed", 90.0)).rem_euclid(360.0);
    let mut pr = Rect::new(r.x + 2, r.y + 1, r.w - 4, r.h - 2); // room for N E S W
    if pr.w < 4 || pr.h < 3 {
        pr = r;
    }
    let (pw, ph) = (pr.w as f64, 2.0 * pr.h as f64);
    let (rad, mx, my) = (pw.min(ph) / 2.0, pw / 2.0, ph / 2.0);
    let accent = cx.colour("color", t.ok);
    let g = ground(cx);
    let face = mix(g, accent, 0.07);
    let dim = mix(g, accent, 0.38);
    let faint = mix(g, accent, 0.2);
    // the face: cells whose centre is inside the circle
    let inside = |i: i32, j: i32| {
        let (dx, dy) = (i as f64 + 0.5 - mx, 2.0 * j as f64 + 1.0 - my);
        dx * dx + dy * dy <= rad * rad
    };
    for j in 0..pr.h {
        for i in 0..pr.w {
            if inside(i, j) {
                c.put(pr.x + i, pr.y + j, ' ', Style::fg(accent).on(face));
            }
        }
    }
    let mut w = Wire::new(pr.w, pr.h);
    w.circle(mx, my, rad, accent);
    for k in 1..=2 {
        w.circle(mx, my, rad * k as f64 / 3.0, dim);
    }
    for b in (0..360).step_by(30) {
        let (x1, y1) = polar(mx, my, rad, b as f64, 0.98);
        w.line(mx, my / 2.0, x1, y1 / 2.0, if b % 90 == 0 { dim } else { faint });
    }
    for b in (0..360).step_by(10) {
        let (x0, y0) = polar(mx, my, rad, b as f64, if b % 30 == 0 { 0.90 } else { 0.94 });
        let (x1, y1) = polar(mx, my, rad, b as f64, 0.985);
        w.line(x0, y0 / 2.0, x1, y1 / 2.0, accent);
    }
    const K: i32 = 44; // the trail: oldest first so the newest, brightest lines end on top of shared cells
    for k in (1..=K).rev() {
        let f = k as f64 / K as f64;
        let (x1, y1) = polar(mx, my, rad, sweep - k as f64 * 1.25, 0.97);
        w.line(mx, my / 2.0, x1, y1 / 2.0, mix(accent, face, 0.12 + 0.86 * f.powf(0.6)));
    }
    let (hx, hy) = polar(mx, my, rad, sweep, 0.97);
    w.line(mx, my / 2.0, hx, hy / 2.0, mix(accent, Color::Rgb(255, 255, 255), 0.55));
    w.flush(c, (pr.x, pr.y), cx.unicode, &|i, j| if inside(i, j) { face } else { t.bg });
    // blips are bright right after the beam passed and fade over about a rotation
    for b in blips(cx) {
        let age = (sweep - b.bearing + 720.0).rem_euclid(360.0);
        let glow = (-age / 110.0).exp();
        if glow < 0.06 {
            continue;
        }
        let (x, y) = polar(mx, my, rad, b.bearing, b.range.clamp(0.0, 0.97));
        let (col, row) = (pr.x + x.floor() as i32, pr.y + (y / 2.0).floor() as i32);
        let bc = mix(face, mix(accent, Color::Rgb(255, 255, 255), 0.35), glow);
        c.put(col, row, if cx.unicode { '●' } else { 'o' }, Style::new(bc, face).with(if glow > 0.55 { BOLD } else { 0 }));
        if glow > 0.55 {
            let h = Style::new(dim, face);
            c.put(col - 1, row, if cx.unicode { '·' } else { '.' }, h);
            c.put(col + 1, row, if cx.unicode { '·' } else { '.' }, h);
        }
        if !b.label.is_empty() && glow > 0.3 {
            c.text_fit(col + 2, row, pr.x + pr.w + 2 - (col + 2), &b.label, Style::new(bc, face));
        }
    }
    let ls = Style::fg(t.dim).on(t.bg);
    c.put(pr.x + pr.w / 2, r.y, 'N', ls);
    c.put(pr.x + pr.w / 2, r.y + r.h - 1, 'S', ls);
    c.put(r.x, pr.y + pr.h / 2, 'W', ls);
    c.put(r.x + r.w - 1, pr.y + pr.h / 2, 'E', ls);
}

// ---- rain ------------------------------------------------------------------------------------------------

fn rain(c: &mut Canvas, r: Rect, cx: &Cx) {
    if r.w < 1 || r.h < 1 || !cx.time.is_finite() {
        return;
    }
    let (head, body, g) = (Style::fg(cx.theme.accent).on(cx.theme.bg), cx.theme.fg, ground(cx));
    let (seed, speed_k) = (cx.num_attr("seed", 1.0) as i64 as u32, cx.num_attr("speed", 1.0));
    for x in 0..r.w {
        let h = hash32(seed.wrapping_mul(2654435761).wrapping_add(x as u32 * 97 + 13));
        let speed = (4.0 + (h % 900) as f64 / 100.0) * speed_k;
        let trail = 5 + ((h >> 10) % 10) as i32;
        let gap = 4 + ((h >> 16) % 14) as i32;
        let period = (r.h + trail + gap) as f64;
        let phase = (cx.time * speed + ((h >> 20) % 1000) as f64 / 1000.0 * period).rem_euclid(period);
        let hy = phase.floor() as i32;
        for k in 0..=trail {
            let y = hy - k;
            if y < 0 || y >= r.h {
                continue;
            }
            // the head flickers, the trail changes slowly
            let gl = glyph(hash32(h.wrapping_add(y as u32 * 31).wrapping_add(tick(cx.time * if k == 0 { 12.0 } else { 2.5 }).wrapping_mul(7))), cx.unicode);
            let st = if k == 0 {
                head
            } else {
                let f = k as f64 / (trail + 1) as f64;
                Style::new(mix(body, g, f * 0.92), cx.theme.bg).with(if k == 1 { BOLD } else { 0 })
            };
            c.put(r.x + x, r.y + y, gl, st);
        }
    }
}

// ---- scramble --------------------------------------------------------------------------------------------

fn scramble(c: &mut Canvas, r: Rect, cx: &Cx) {
    let text = match cx.el.attr("text") {
        Some(t) => t.to_string(),
        None => cx.attr("bind").and_then(|k| cx.model_text(k)).unwrap_or_default(),
    };
    if r.w < 1 || r.h < 1 || text.is_empty() {
        return;
    }
    let progress = match cx.el.attr("progress") {
        Some("auto") => ((cx.time % 5.0) / 2.5).min(1.0), // resolves in 2.5 s, holds, starts over
        Some(s) => s.trim().parse::<f64>().ok().filter(|x| x.is_finite()).unwrap_or(1.0),
        None => 1.0,
    }
    .clamp(0.0, 1.0);
    let total = text.chars().count();
    let done = (progress * total as f64 + 1e-9).floor() as usize;
    let t = cx.theme;
    for (i, ch) in text.chars().enumerate() {
        if i as i32 >= r.w {
            break;
        }
        let x = r.x + i as i32;
        if i < done {
            c.put(x, r.y, ch, Style::fg(t.fg).on(t.bg).with(BOLD));
        } else if i == done && progress < 1.0 {
            c.put(x, r.y, glyph(hash32(i as u32 * 977 + tick(cx.time * 20.0)), cx.unicode), Style::fg(t.accent).on(t.bg).with(REVERSE));
        } else if ch != ' ' {
            c.put(x, r.y, glyph(hash32(i as u32 * 977 + tick(cx.time * 12.0)), cx.unicode), Style::fg(t.dim).on(t.bg));
        }
    }
}

// ---- tagcloud --------------------------------------------------------------------------------------------

/// breathing room around a word: heavy words get more, light ones are crammed (a size signal that survives a mono theme)
fn pad(f: f64) -> i32 {
    if f > 0.66 {
        3
    } else if f > 0.33 {
        2
    } else {
        1
    }
}

fn tagcloud(c: &mut Canvas, r: Rect, cx: &Cx) {
    let words = cx.list_attr("words");
    if r.w < 4 || r.h < 1 || words.is_empty() {
        return;
    }
    let ws: Vec<f64> = cx.list_attr("weights").iter().map(|s| s.parse::<f64>().unwrap_or(f64::NAN)).collect();
    let fin: Vec<f64> = ws.iter().cloned().filter(|x| x.is_finite()).collect();
    let (mn, mx) = (fin.iter().cloned().fold(f64::INFINITY, f64::min), fin.iter().cloned().fold(f64::NEG_INFINITY, f64::max));
    let f_of = |i: usize| match ws.get(i) {
        Some(w) if w.is_finite() && mx > mn => (w - mn) / (mx - mn),
        _ => 0.5,
    };
    let t = cx.theme;
    let (mut i, mut y) = (0, r.y);
    while i < words.len() && y < r.y + r.h {
        let (mut j, mut used) = (i, 0);
        while j < words.len() {
            let w = str_width(&words[j]) as i32 + if j > i { pad(f_of(j)) } else { 0 };
            if used + w > r.w && j > i {
                break;
            }
            used += w;
            j += 1;
        }
        let mut x = r.x + (r.w - used).max(0) / 2;
        for k in i..j {
            let f = f_of(k);
            let st = Style::fg(if f > 0.66 { t.accent } else if f > 0.33 { t.fg } else { t.dim }).on(t.bg).with(if f > 0.66 { BOLD } else { 0 });
            let w = (str_width(&words[k]) as i32).min(r.w);
            c.text_fit(x, y, w, &words[k], st);
            x += w + pad(f);
        }
        i = j;
        y += 1;
    }
}

// ---- webchart --------------------------------------------------------------------------------------------

fn in_polygon(p: &[(f64, f64)], x: f64, y: f64) -> bool {
    let mut inside = false;
    let mut j = p.len() - 1;
    for i in 0..p.len() {
        let ((xi, yi), (xj, yj)) = (p[i], p[j]);
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// two pixels per cell: a half block when the halves differ, a space with a background when they agree
fn halves(c: &mut Canvas, at: (i32, i32), top: Option<Color>, bot: Option<Color>, unicode: bool) {
    match (top, bot, unicode) {
        (Some(t), Some(b), true) if t == b => c.put(at.0, at.1, ' ', Style::fg(t).on(b)),
        (Some(t), Some(b), true) => c.put(at.0, at.1, '▀', Style::new(t, b)),
        (Some(t), None, true) => c.put(at.0, at.1, '▀', Style::fg(t)),
        (None, Some(b), true) => c.put(at.0, at.1, '▄', Style::fg(b)),
        (Some(t), _, false) | (_, Some(t), false) => c.put(at.0, at.1, ' ', Style::fg(t).on(t)),
        _ => {}
    }
}

fn webchart(c: &mut Canvas, r: Rect, cx: &Cx) {
    let v = cx.series();
    let labels = cx.list_attr("labels");
    let n = v.len();
    let max = cx.num_attr("max", v.iter().cloned().fold(0.0, f64::max));
    if r.w < 6 || r.h < 4 || !(3..=32).contains(&n) || !(max > 0.0) {
        return;
    }
    let rings = cx.num_attr("rings", 4.0).clamp(1.0, 12.0) as i32;
    let t = cx.theme;
    let lw = labels.iter().map(|l| str_width(l) as i32).max().unwrap_or(0);
    let mut mx = lw + 1;
    if r.w - 2 * mx < 4 {
        mx = ((r.w - 4) / 2).max(0);
    }
    let pr = Rect::new(r.x + mx, r.y + 1, r.w - 2 * mx, r.h - 2);
    if pr.w < 2 || pr.h < 2 {
        return;
    }
    let (pw, ph) = (pr.w as f64, 2.0 * pr.h as f64);
    let (rad, cxp, cyp) = (pw.min(ph) / 2.0, pw / 2.0, ph / 2.0);
    let accent = cx.colour("color", t.accent);
    let fill = mix(ground(cx), accent, 0.38);
    let dirs: Vec<(f64, f64)> = (0..n).map(|i| { let a = -PI / 2.0 + 2.0 * PI * i as f64 / n as f64; (a.cos(), a.sin()) }).collect();
    let poly: Vec<(f64, f64)> = dirs.iter().zip(&v).map(|(d, x)| { let f = if x.is_finite() { (x / max).clamp(0.0, 1.0) } else { 0.0 }; (cxp + d.0 * rad * f, cyp + d.1 * rad * f) }).collect();
    for j in 0..pr.h {
        for i in 0..pr.w {
            let pick = |y: f64| in_polygon(&poly, i as f64 + 0.5, y).then_some(fill);
            halves(c, (pr.x + i, pr.y + j), pick(2.0 * j as f64 + 0.5), pick(2.0 * j as f64 + 1.5), cx.unicode);
        }
    }
    let grid = t.dim;
    let mut w = Wire::new(pr.w, pr.h);
    for k in 1..=rings {
        let rr = rad * k as f64 / rings as f64;
        for i in 0..n {
            let j = (i + 1) % n;
            w.line(cxp + dirs[i].0 * rr, (cyp + dirs[i].1 * rr) / 2.0, cxp + dirs[j].0 * rr, (cyp + dirs[j].1 * rr) / 2.0, grid);
        }
    }
    for d in &dirs {
        w.line(cxp, cyp / 2.0, cxp + d.0 * rad, (cyp + d.1 * rad) / 2.0, grid);
    }
    for i in 0..n { // the data outline last, in the accent colour
        let j = (i + 1) % n;
        w.line(poly[i].0, poly[i].1 / 2.0, poly[j].0, poly[j].1 / 2.0, accent);
    }
    w.flush(c, (pr.x, pr.y), cx.unicode, &|i, j| if in_polygon(&poly, i as f64 + 0.5, 2.0 * j as f64 + 1.0) { fill } else { t.bg });
    for (i, d) in dirs.iter().enumerate() {
        let Some(lab) = labels.get(i).filter(|l| !l.is_empty()) else { continue };
        let (px, py) = (pr.x as f64 + cxp + d.0 * (rad + 1.5), pr.y as f64 + (cyp + d.1 * (rad + 1.5)) / 2.0);
        let w = str_width(lab) as i32;
        let x = if d.0 > 0.3 { px.floor() as i32 } else if d.0 < -0.3 { px.floor() as i32 - w + 1 } else { (px + 0.5).floor() as i32 - w / 2 };
        c.text_fit(x.max(r.x), (py.floor() as i32).clamp(r.y, r.y + r.h - 1), w.min(r.w), lab, Style::fg(t.dim).on(t.bg));
    }
}

// ---- piechart --------------------------------------------------------------------------------------------

fn piechart(c: &mut Canvas, r: Rect, cx: &Cx) {
    let v: Vec<f64> = cx.series().iter().map(|x| if x.is_finite() && *x > 0.0 { *x } else { 0.0 }).collect();
    let total: f64 = v.iter().sum();
    if r.w < 4 || r.h < 2 || total <= 0.0 {
        return;
    }
    let t = cx.theme;
    let def = [t.accent, t.ok, t.warn, t.err, mix(t.accent, t.fg, 0.5), mix(t.ok, t.fg, 0.5), t.dim];
    let named = cx.list_attr("colors");
    let cols: Vec<Color> = (0..v.len()).map(|i| named.get(i).and_then(|s| crate::gfx::colour(t, s)).unwrap_or(def[i % def.len()])).collect();
    let legend = cx.attr("legend") != Some("0") && r.w >= 26;
    let mut area = r;
    let mut leg = Rect::new(0, 0, 0, 0);
    if legend {
        let lw = (r.w / 2).min(26);
        area.w = r.w - lw - 1;
        let rows = (v.len() as i32).min(r.h);
        leg = Rect::new(r.x + area.w + 1, r.y + (r.h - rows) / 2, lw, rows);
    }
    let mut edge = Vec::with_capacity(v.len());
    let mut acc = 0.0;
    for x in &v {
        acc += x / total;
        edge.push(acc);
    }
    let (pw, ph) = (area.w as f64, 2.0 * area.h as f64);
    let rad = pw.min(ph) / 2.0;
    let hole = if cx.flag("donut") { rad * 0.52 } else { 0.0 };
    let sample = |x: f64, y: f64| -> Option<Color> {
        let (dx, dy) = (x - pw / 2.0, y - ph / 2.0);
        let d2 = dx * dx + dy * dy;
        if d2 > rad * rad || d2 < hole * hole {
            return None;
        }
        let a = (dy.atan2(dx) + PI / 2.0).rem_euclid(2.0 * PI) / (2.0 * PI); // 0 at the top, clockwise
        let i = edge.iter().position(|e| a < *e).unwrap_or(v.len() - 1);
        Some(cols[i])
    };
    for j in 0..area.h {
        for i in 0..area.w {
            let (x, y) = (i as f64 + 0.5, 2.0 * j as f64);
            halves(c, (area.x + i, area.y + j), sample(x, y + 0.5), sample(x, y + 1.5), cx.unicode);
        }
    }
    if leg.w > 0 {
        let labels = cx.list_attr("labels");
        for i in 0..leg.h as usize {
            let y = leg.y + i as i32;
            c.put(leg.x, y, if cx.unicode { '■' } else { '#' }, Style::fg(cols[i]).on(t.bg));
            let pct = (v[i] / total * 100.0).round() as i32;
            let s = match labels.get(i).filter(|l| !l.is_empty()) {
                Some(l) => format!("{l} {pct}%"),
                None => format!("{pct}%"),
            };
            c.text_fit(leg.x + 2, y, leg.w - 2, &s, Style::fg(t.fg).on(t.bg));
        }
    }
    let _ = clamp01;
}

#[cfg(test)]
mod tests {
    use crate::headless::snapshot;
    use crate::ui::Ui;

    fn snap(spec: &str, w: i32, h: i32) -> String {
        let mut ui = Ui::from_spec(spec).unwrap();
        snapshot(&mut ui, w, h)
    }

    #[test]
    fn every_widget_draws_and_survives_any_size() {
        for spec in [
            "radar blips=\"45:0.6:alpha;200:0.3:bravo\" speed=60",
            "rain seed=3",
            "scramble text=\"hello world\" progress=0.5",
            "scramble text=\"hello\" progress=auto",
            "tagcloud words=rust,c,go,mojo,lean weights=9,5,3,7,1",
            "webchart values=3,5,2,8,6 labels=a,b,c,d,e max=10",
            "piechart values=4,3,2,1 labels=a,b,c,d donut=1",
            "piechart values=0,0",
            "webchart values=1,2",
            "tagcloud words=",
        ] {
            for (w, h) in [(0, 0), (1, 1), (7, 4), (40, 12), (120, 40)] {
                let _ = snap(spec, w, h);
            }
        }
    }

    #[test]
    fn scramble_resolves_left_to_right() {
        let s = snap("scramble text=\"hello world\" progress=0.5", 30, 1);
        assert!(s.starts_with("hello"), "{s}");
        assert!(!s.contains("world"), "{s}");
        assert!(snap("scramble text=\"hello\" progress=1", 10, 1).starts_with("hello"));
        assert!(!snap("scramble text=\"hello\" progress=0", 10, 1).contains("hello"));
    }

    #[test]
    fn tagcloud_wraps_and_centres() {
        let s = snap("tagcloud words=alpha,beta,gamma,delta,epsilon weights=1,9,5,3,7", 14, 6);
        let lines: Vec<&str> = s.lines().filter(|l| !l.trim().is_empty()).collect();
        assert!(lines.len() >= 2, "{s}");
        assert!(s.contains("beta") && s.contains("epsilon"), "{s}");
    }

    #[test]
    fn piechart_colours_follow_the_values_and_the_legend_shows_percent() {
        let s = snap("piechart values=1,1 labels=yes,no", 40, 8);
        assert!(s.contains("yes 50%") && s.contains("no 50%"), "{s}");
        assert!(s.contains('▀') || s.contains('▄') || s.contains('█') || s.contains(' '), "{s}");
    }

    #[test]
    fn radar_has_compass_letters_and_a_moving_sweep() {
        let mut ui = Ui::from_spec("radar blips=\"10:0.5:x\" speed=90").unwrap();
        let a = snapshot(&mut ui, 40, 16);
        for l in ['N', 'S', 'E', 'W'] {
            assert!(a.contains(l), "{a}");
        }
        ui.tick(1.0);
        let b = snapshot(&mut ui, 40, 16);
        assert_ne!(a, b, "the beam moved");
    }

    #[test]
    fn rain_falls() {
        let mut ui = Ui::from_spec("rain seed=1").unwrap();
        let a = snapshot(&mut ui, 30, 12);
        ui.tick(0.5);
        let b = snapshot(&mut ui, 30, 12);
        assert!(a.chars().any(|c| !c.is_whitespace()) && a != b);
    }
}
