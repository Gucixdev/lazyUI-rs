//! Display widgets ported from lazytui: graph, barchart, histogram, zonebar/piebar, segbar, gauge, ringbar, spinner.
//! Each draws from the element's attributes and the model only; sizes that cannot hold a picture draw nothing.

use crate::canvas::{str_width, Canvas, Color, Rect, Style};
use crate::gfx::{clamp01, colour, eighth, ink_for, level, mix, slice};
use crate::native::{Cx, Def, Pure, Widget};

fn pure(f: fn(&mut Canvas, Rect, &Cx)) -> Box<dyn Widget> {
    Box::new(Pure(f))
}

pub static DEFS: &[Def] = &[
    Def { name: "graph", doc: "bars of the latest samples, one to many rows: bind=KEY|values=1,2,3 min max color", focus: false, rows: None, live: false, make: || pure(graph) },
    Def { name: "barchart", doc: "vertical bars with labels under them: values labels max show=1 color", focus: false, rows: None, live: false, make: || pure(barchart) },
    Def { name: "histogram", doc: "counts of samples per bin: values bins min max show=1", focus: false, rows: None, live: false, make: || pure(histogram) },
    Def { name: "zonebar", doc: "one bar split by weights (used/cache/free): values labels colors legend=1", focus: false, rows: None, live: false, make: || pure(zonebar) },
    Def { name: "piebar", doc: "like zonebar, legend on by default: values labels colors", focus: false, rows: None, live: false, make: || pure(piebar) },
    Def { name: "segbar", doc: "one row stacked bar of values out of total, then a suffix: values total suffix colors", focus: false, rows: Some(1), live: false, make: || pure(segbar) },
    Def { name: "gauge", doc: "a level bar that goes green to red: bind|value min max label text color low=bad", focus: false, rows: Some(1), live: false, make: || pure(gauge) },
    Def { name: "ringbar", doc: "text scrolling round as an endless ring: text speed color", focus: false, rows: Some(1), live: true, make: || pure(ringbar) },
    Def { name: "spinner", doc: "something is working: style=dots|line|bounce text speed color", focus: false, rows: Some(1), live: true, make: || pure(spinner) },
];

fn base_style(cx: &Cx, fg: Color) -> Style {
    Style::fg(fg).on(cx.theme.bg)
}

fn auto_max(v: &[f64], lo: f64, given: f64) -> f64 {
    let hi = if given.is_finite() { given } else { v.iter().cloned().fold(lo, f64::max) };
    if hi > lo {
        hi
    } else {
        lo + 1.0
    }
}

/// A short number: whole when whole, else at most two decimals.
fn g3(x: f64) -> String {
    if x.fract() == 0.0 && x.abs() < 1e9 {
        return format!("{}", x as i64);
    }
    let s = format!("{x:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

// ---- graph --------------------------------------------------------------------------------------------

fn graph(c: &mut Canvas, r: Rect, cx: &Cx) {
    let v = cx.series();
    if r.w < 1 || r.h < 1 || v.is_empty() {
        return;
    }
    let t = cx.theme;
    let lo = cx.num_attr("min", 0.0);
    let hi = auto_max(&v, lo, cx.num_attr("max", f64::NAN));
    let base = cx.colour("color", t.accent);
    let shade = |f: f64| mix(t.dim, base, 0.35 + 0.65 * clamp01(f));
    let first = v.len().saturating_sub(r.w as usize); // once full, the oldest samples scroll off to the left
    const ASCII: [char; 8] = ['_', '.', ':', '-', '=', '+', '*', '#'];
    for (i, x) in v[first..].iter().enumerate() {
        let f = clamp01((x - lo) / (hi - lo));
        let col = r.x + i as i32;
        if r.h == 1 {
            let lv = (f * 7.0 + 0.5).floor() as i32;
            let ch = if cx.unicode { eighth(lv + 1, true) } else { ASCII[lv.clamp(0, 7) as usize] };
            c.put(col, r.y, ch, base_style(cx, shade(f)));
            continue;
        }
        let h8 = ((f * r.h as f64 * 8.0 + 0.5).floor() as i32).max(1); // a zero still shows the low bar
        for row in 0..r.h {
            let below = row * 8;
            if h8 <= below {
                break;
            }
            let here = (h8 - below).min(8);
            c.put(col, r.y + r.h - 1 - row, eighth(here, cx.unicode), base_style(cx, shade((below + here) as f64 / (r.h * 8) as f64)));
        }
    }
}

// ---- barchart, histogram ------------------------------------------------------------------------------

fn bars(c: &mut Canvas, r: Rect, cx: &Cx, labels: &[String], values: &[f64], hi: f64) {
    if r.w < 1 || r.h < 1 || values.is_empty() {
        return;
    }
    let t = cx.theme;
    let labels_on = r.h >= 3 && labels.iter().any(|l| !l.is_empty());
    let ph = r.h - if labels_on { 1 } else { 0 };
    let mut n = values.len() as i32;
    let mut slot = r.w / n;
    if slot < 1 {
        n = r.w;
        slot = 1;
    }
    let bw = (if slot > 1 { slot - 1 } else { 1 }).min(8);
    let x0 = r.x + (r.w - slot * n) / 2;
    let base = cx.colour("color", t.accent);
    let show = cx.flag("show");
    for i in 0..n {
        let v = values[i as usize];
        let v = if v.is_finite() && v > 0.0 { v } else { 0.0 };
        let f = clamp01(v / hi);
        let mut h8 = (f * ph as f64 * 8.0 + 0.5).floor() as i32;
        if v > 0.0 && h8 < 1 {
            h8 = 1;
        }
        let col = mix(t.dim, base, 0.4 + 0.6 * f);
        let bx = x0 + i * slot + (slot - bw) / 2;
        for row in 0..ph {
            let below = row * 8;
            if h8 <= below {
                break;
            }
            let here = (h8 - below).min(8);
            for k in 0..bw {
                c.put(bx + k, r.y + ph - 1 - row, eighth(here, cx.unicode), base_style(cx, col));
            }
        }
        if labels_on {
            if let Some(l) = labels.get(i as usize) {
                let lw = (str_width(l) as i32).min(slot);
                c.text_fit(x0 + i * slot + (slot - lw) / 2, r.y + ph, lw, l, base_style(cx, t.dim));
            }
        }
        if show && slot >= 4 && h8 / 8 + 1 < ph {
            let s = g3(v);
            let w = str_width(&s) as i32;
            if w <= slot {
                c.text_fit(x0 + i * slot + (slot - w) / 2, r.y + ph - 1 - (h8 + 7) / 8, w, &s, base_style(cx, t.fg));
            }
        }
    }
}

fn barchart(c: &mut Canvas, r: Rect, cx: &Cx) {
    let v = cx.series();
    let mut hi = cx.num_attr("max", f64::NAN);
    if !hi.is_finite() {
        hi = v.iter().cloned().fold(0.0, f64::max);
    }
    if hi <= 0.0 {
        hi = 1.0;
    }
    bars(c, r, cx, &cx.list_attr("labels"), &v, hi);
}

fn histogram(c: &mut Canvas, r: Rect, cx: &Cx) {
    let v = cx.series();
    let bins = cx.num_attr("bins", 10.0).clamp(1.0, 256.0) as usize;
    if v.is_empty() {
        return;
    }
    let mut lo = cx.num_attr("min", f64::NAN);
    let mut hi = cx.num_attr("max", f64::NAN);
    if !(lo.is_finite() && hi.is_finite() && hi > lo) {
        lo = v.iter().cloned().fold(f64::INFINITY, f64::min);
        hi = v.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        if hi <= lo {
            hi = lo + 1.0;
        }
    }
    let mut cnt = vec![0.0f64; bins];
    for x in &v {
        if *x < lo || *x > hi {
            continue;
        }
        let b = (((x - lo) / (hi - lo)) * bins as f64).floor() as usize;
        cnt[b.min(bins - 1)] += 1.0;
    }
    let labels: Vec<String> = (0..bins).map(|b| g3(lo + (hi - lo) * (b as f64 + 0.5) / bins as f64)).collect();
    let top = cnt.iter().cloned().fold(0.0, f64::max).max(1.0);
    bars(c, r, cx, &labels, &cnt, top);
}

// ---- zonebar, piebar, segbar --------------------------------------------------------------------------

fn palette(cx: &Cx, n: usize) -> Vec<Color> {
    let t = cx.theme;
    let def = [t.accent, t.ok, t.warn, t.err, mix(t.accent, t.fg, 0.5), mix(t.ok, t.fg, 0.5), t.dim];
    let named = cx.list_attr("colors");
    (0..n).map(|i| named.get(i).and_then(|s| colour(t, s)).unwrap_or(def[i % def.len()])).collect()
}

const FILL: [char; 6] = ['#', '=', '+', '*', '%', '@'];

fn zones(c: &mut Canvas, r: Rect, cx: &Cx, legend: bool) {
    let v = cx.series();
    if r.w < 1 || r.h < 1 || v.is_empty() {
        return;
    }
    let t = cx.theme;
    let pos: Vec<f64> = v.iter().map(|x| if *x > 0.0 { *x } else { 0.0 }).collect();
    let total: f64 = pos.iter().sum();
    if total <= 0.0 {
        c.fill(Rect::new(r.x, r.y, r.w, 1), ' ', Style::fg(t.dim).on(mix(t.bg, t.dim, 0.4)));
        return;
    }
    let cols = palette(cx, v.len());
    let w = slice(&pos, r.w);
    let mut x = r.x;
    for (i, wi) in w.iter().enumerate() {
        let st = if cx.unicode { Style::fg(ink_for(cols[i], t)).on(cols[i]) } else { Style::fg(cols[i]).on(t.bg) };
        c.fill(Rect::new(x, r.y, *wi, 1), if cx.unicode { ' ' } else { FILL[i % FILL.len()] }, st);
        x += wi;
    }
    if !legend || r.h < 2 {
        return;
    }
    let labels = cx.list_attr("labels");
    let (y, end) = (r.y + 1, r.x + r.w);
    let mut x = r.x;
    for i in 0..v.len() {
        if x >= end {
            break;
        }
        c.put(x, y, if cx.unicode { '■' } else { FILL[i % FILL.len()] }, base_style(cx, cols[i]));
        x += 2;
        if x >= end {
            break;
        }
        let pct = (pos[i] / total * 100.0).round() as i32;
        let seg = match labels.get(i).filter(|l| !l.is_empty()) {
            Some(l) => format!("{l} {pct}%"),
            None => format!("{pct}%"),
        };
        let sw = (str_width(&seg) as i32).min(end - x); // cut rather than run into the next swatch
        c.text_fit(x, y, sw, &seg, base_style(cx, t.fg));
        x += sw + 2;
    }
}

fn zonebar(c: &mut Canvas, r: Rect, cx: &Cx) {
    zones(c, r, cx, cx.flag("legend"));
}

fn piebar(c: &mut Canvas, r: Rect, cx: &Cx) {
    zones(c, r, cx, cx.attr("legend") != Some("0"));
}

fn segbar(c: &mut Canvas, r: Rect, cx: &Cx) {
    let v = cx.series();
    if r.w < 1 || r.h < 1 || v.is_empty() {
        return;
    }
    let t = cx.theme;
    let suffix = cx.text_attr("suffix", "");
    let mut sw = str_width(&suffix) as i32;
    if r.w - sw < 4 {
        sw = 0; // the bar matters more than its caption
    }
    let bw = r.w - sw;
    let sum: f64 = v.iter().filter(|x| **x > 0.0).sum();
    let total = cx.num_attr("total", sum);
    let full = total > 0.0 && (sum - total).abs() <= total * 0.005;
    let def = [t.accent, t.warn, t.err, t.ok, t.dim];
    let named = cx.list_attr("colors");
    let mut used = 0;
    if total > 0.0 {
        for (i, x) in v.iter().enumerate() {
            if used >= bw {
                break;
            }
            let f = if *x > 0.0 { *x } else { 0.0 };
            let mut w = (bw as f64 * f / total + 0.5).floor() as i32;
            if i == v.len() - 1 && full {
                w = bw - used; // the last takes what is left, so rounding leaves no gap
            }
            w = w.min(bw - used);
            let col = named.get(i).and_then(|s| colour(t, s)).unwrap_or(def[i % def.len()]);
            for k in 0..w {
                c.put(r.x + used + k, r.y, if cx.unicode { '█' } else { '#' }, base_style(cx, col));
            }
            used += w;
        }
    }
    for k in used..bw {
        c.put(r.x + k, r.y, if total > 0.0 { if cx.unicode { '░' } else { '.' } } else { ' ' }, base_style(cx, t.dim));
    }
    if sw > 0 {
        c.text_fit(r.x + bw, r.y, sw, &suffix, base_style(cx, t.fg));
    }
}

// ---- gauge --------------------------------------------------------------------------------------------

fn gauge(c: &mut Canvas, r: Rect, cx: &Cx) {
    if r.w < 1 || r.h < 1 {
        return;
    }
    let t = cx.theme;
    let (lo, hi) = (cx.num_attr("min", 0.0), cx.num_attr("max", 100.0));
    let f = if hi > lo { clamp01((cx.value() - lo) / (hi - lo)) } else { 0.0 };
    let label = cx.text_attr("label", "");
    let lw = if label.is_empty() { 0 } else { (str_width(&label) as i32 + 1).min(r.w / 2) };
    c.text_fit(r.x, r.y, lw, &label, base_style(cx, t.dim));
    let right = cx.text_attr("text", &format!("{}%", (f * 100.0).round() as i32));
    let rw = (str_width(&right) as i32 + 1).min((r.w - lw) / 2);
    let bw = r.w - lw - rw;
    if rw > 0 {
        c.text_fit(r.x + r.w - rw + 1, r.y, rw - 1, &right, base_style(cx, t.fg));
    }
    if bw < 1 {
        return;
    }
    let col = cx.attr("color").and_then(|n| colour(t, n)).unwrap_or_else(|| level(t, if cx.flag("low") { 1.0 - f } else { f }));
    let track = mix(t.bg, t.dim, 0.35);
    let eighths = (f * bw as f64 * 8.0 + 0.5).floor() as i32;
    let (whole, part) = (eighths / 8, eighths % 8);
    const LEFT: [char; 7] = ['▏', '▎', '▍', '▌', '▋', '▊', '▉'];
    for i in 0..bw {
        let (ch, st) = if i < whole {
            (if cx.unicode { '█' } else { '#' }, Style::fg(col).on(t.bg))
        } else if i == whole && part > 0 {
            (if cx.unicode { LEFT[part as usize - 1] } else if part >= 4 { '=' } else { '.' }, Style::fg(col).on(track))
        } else {
            (if cx.unicode { ' ' } else { '.' }, Style::fg(t.dim).on(track))
        };
        c.put(r.x + lw + i, r.y, ch, st);
    }
}

// ---- ringbar, spinner ---------------------------------------------------------------------------------

fn ringbar(c: &mut Canvas, r: Rect, cx: &Cx) {
    let text = cx.text_attr("text", "");
    if r.w < 1 || r.h < 1 || text.is_empty() {
        return;
    }
    let mut ring: Vec<char> = text.chars().collect();
    ring.extend([' ', ' ', ' ']); // the text, three blanks, and again
    let cells: i32 = ring.iter().map(|ch| crate::canvas::char_width(*ch).max(1) as i32).sum();
    let phase = cx.time * cx.num_attr("speed", 8.0);
    let mut skip = (phase.floor() as i64).rem_euclid(cells as i64) as i32;
    let st = base_style(cx, cx.colour("color", cx.theme.accent));
    let (mut idx, mut col) = (0usize, 0);
    while col < r.w {
        let ch = ring[idx % ring.len()];
        let w = crate::canvas::char_width(ch).max(1) as i32;
        idx += 1;
        if skip >= w {
            skip -= w;
            continue;
        }
        if skip > 0 || col + w > r.w {
            // a wide character cut by the edge shows as a blank
            c.put(r.x + col, r.y, ' ', st);
            col += 1;
            skip = 0;
            continue;
        }
        c.put(r.x + col, r.y, ch, st);
        col += w;
    }
}

fn spinner(c: &mut Canvas, r: Rect, cx: &Cx) {
    if r.w < 1 || r.h < 1 {
        return;
    }
    let frames: &[char] = match (cx.unicode, cx.attr("style")) {
        (false, _) | (_, Some("line")) => &['|', '/', '-', '\\'],
        (_, Some("bounce")) => &['⠁', '⠂', '⠄', '⠂'],
        _ => &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'],
    };
    let i = ((cx.time * cx.num_attr("speed", 10.0)).floor() as i64).rem_euclid(frames.len() as i64) as usize;
    c.put(r.x, r.y, frames[i], base_style(cx, cx.colour("color", cx.theme.accent)));
    let label = cx.text_attr("text", "");
    if !label.is_empty() && r.w > 2 {
        c.text_fit(r.x + 2, r.y, r.w - 2, &label, base_style(cx, cx.theme.fg));
    }
}

#[cfg(test)]
mod tests {
    use crate::canvas::{Canvas, Caps};
    use crate::headless::{run_script, snapshot};
    use crate::ui::Ui;

    fn snap(spec: &str, w: i32, h: i32) -> String {
        let mut ui = Ui::from_spec(spec).unwrap();
        snapshot(&mut ui, w, h)
    }

    fn rows(s: &str) -> Vec<&str> {
        s.lines().map(|l| l.trim_end()).collect()
    }

    const EVERY: &[&str] = &[
        "graph values=1,5,2,8,3,9,4 max=10",
        "graph values=0,0,0",
        "barchart values=3,9,4,1 labels=a,b,c,d show=1",
        "barchart values=0,0 labels=x,y",
        "histogram values=1,2,2,3,3,3,9 bins=4 show=1",
        "histogram values=5,5,5 bins=3",
        "zonebar values=5,3,2 labels=used,cache,free legend=1",
        "zonebar values=0,0",
        "piebar values=1,1,1,1,1,1,1,1,1",
        "segbar values=30,20 total=100 suffix=\" 50%\"",
        "gauge value=64 label=cpu",
        "gauge value=-5 min=10 max=10",
        "ringbar text=\"héllo wörld 日本語\"",
        "spinner text=loading style=bounce",
    ];

    /// every widget at every small size, in both glyph sets: nothing panics, nothing draws outside its rectangle
    #[test]
    fn no_size_breaks_a_widget() {
        for spec in EVERY {
            for caps in [Caps::ASCII, Caps::FULL] {
                for (w, h) in (0..=14).flat_map(|w| (0..=6).map(move |h| (w, h))).chain([(80, 24), (200, 3)]) {
                    let mut ui = Ui::from_spec(&format!("col\n  text text=top\n  {spec} size=fill\n  text text=bottom\n")).unwrap();
                    let mut c = Canvas::new(w, h, caps);
                    ui.render(&mut c);
                    ui.tick(0.37);
                    ui.render(&mut c);
                }
            }
        }
    }

    #[test]
    fn graph_shows_the_latest_samples_and_scrolls() {
        let s = snap("graph values=0,10,0,10,5 max=10", 5, 1);
        assert_eq!(rows(&s)[0], "_#_#=", "{s}");
        let s = snap("graph values=1,2,3,4,5,6,7,8 max=8", 4, 1);
        assert_eq!(rows(&s)[0].len(), 4, "{s}"); // only the newest four
        let tall = snap("graph values=0,8,4 max=8", 3, 2);
        assert_eq!(rows(&tall), vec![" #", ".##"], "{tall}");
    }

    #[test]
    fn graph_follows_the_model_and_a_push() {
        let mut ui = Ui::from_spec("graph bind=cpu max=10").unwrap();
        run_script(&mut ui, "size 6 1\nset cpu=10,0").unwrap();
        assert_eq!(rows(&snapshot(&mut ui, 6, 1))[0], "#_");
        ui.push("cpu", 10.0, 3);
        ui.push("cpu", 10.0, 3);
        assert_eq!(ui.text_of("cpu"), "0,10,10", "capped at the newest 3");
        assert_eq!(rows(&snapshot(&mut ui, 6, 1))[0], "_##");
    }

    #[test]
    fn barchart_and_histogram_draw_bars_and_labels() {
        let s = snap("barchart values=4,8 labels=a,b show=1", 8, 5);
        let r = rows(&s);
        assert!(r[4].contains('a') && r[4].contains('b'), "{s}");
        assert!(!r[0].contains('8') && r[1].contains("4"), "the shorter bar carries its value above it, the full one has no room: {s}");
        assert!(r[3].contains('#'), "{s}");
        let h = snap("histogram values=1,1,1,1,9 bins=2", 10, 4);
        assert!(h.contains('#'), "{h}");
        assert!(rows(&h)[3].contains("3") && rows(&h)[3].contains("7"), "bin centres label the bars: {h}");
    }

    #[test]
    fn zones_fill_the_whole_width_with_a_legend() {
        let s = snap("zonebar values=5,3,2 labels=used,cache,free legend=1", 40, 2);
        let r = rows(&s);
        assert_eq!(r[0].len(), 40, "no gap, no overflow: {s}");
        assert_eq!(r[0].matches('#').count(), 20, "{s}");
        assert!(r[1].contains("used 50%") && r[1].contains("cache 30%") && r[1].contains("free 20%"), "{s}");
        // a legend that does not fit is cut, never wrapped over the next swatch
        let narrow = snap("zonebar values=5,3,2 labels=used,cache,free legend=1", 12, 2);
        assert!(rows(&narrow)[1].len() <= 12, "{narrow}");
        // piebar shows its legend without being asked, zonebar does not
        assert!(rows(&snap("piebar values=1,1", 10, 2))[1].contains("50%"));
        assert_eq!(rows(&snap("zonebar values=1,1", 10, 2))[1], "");
    }

    #[test]
    fn segbar_takes_the_rest_for_the_last_segment() {
        let s = snap("segbar values=33,33,34 total=100 suffix=\" ok\"", 22, 1);
        let r = rows(&s)[0];
        assert_eq!(r.len(), 22, "{s}");
        assert!(r.ends_with(" ok") && r[..19].chars().all(|c| c == '#'), "{s}");
        let part = snap("segbar values=10 total=100", 10, 1);
        assert_eq!(rows(&part)[0], "#.........", "{part}");
    }

    #[test]
    fn gauge_levels_and_label() {
        let s = snap("gauge value=50 label=cpu", 20, 1);
        let r = rows(&s)[0];
        assert!(r.starts_with("cpu ") && r.ends_with("50%"), "{s}");
        assert_eq!(snap("gauge value=100", 10, 1).lines().next().unwrap().matches('#').count(), 5, "{}", snap("gauge value=100", 10, 1));
        let mut ui = Ui::from_spec("gauge bind=x").unwrap();
        // the colour goes from ok to err with the level
        ui.set("x", 0.0);
        let mut a = Canvas::new(10, 1, Caps::FULL);
        ui.render(&mut a);
        ui.set("x", 100.0);
        let mut b = Canvas::new(10, 1, Caps::FULL);
        ui.render(&mut b);
        assert_ne!(a.get(0, 0).unwrap().style.fg, b.get(0, 0).unwrap().style.fg);
        assert_eq!(b.get(0, 0).unwrap().style.fg, ui.theme.err);
    }

    #[test]
    fn ringbar_and_spinner_move_with_the_clock_only() {
        let mut ui = Ui::from_spec("col\n  ringbar text=abc speed=1\n  spinner text=wait\n").unwrap();
        let a = snapshot(&mut ui, 8, 2);
        assert_eq!(snapshot(&mut ui, 8, 2), a, "no tick, no change: snapshots are repeatable");
        assert!(ui.animating() && !ui.settling(), "live, but nothing to wait for");
        ui.tick(1.0);
        let b = snapshot(&mut ui, 8, 2);
        assert_ne!(a, b);
        assert!(rows(&a)[0].starts_with("abc   abc") || rows(&a)[0].starts_with("abc   ab"), "{a}");
        assert!(rows(&b)[0].starts_with("bc   abc"), "{b}");
        assert!(rows(&b)[1].contains("wait"), "{b}");
        assert!(!Ui::from_spec("text text=hi").unwrap().animating(), "a still screen needs no frames");
    }
}
