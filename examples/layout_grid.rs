//! Prints `layout::split` for a fixed grid of inputs, one line each, for lean4/run to diff against the Lean oracle.
//! Line: `W GAP SPEC.. -> x:w x:w ..`; SPEC is f3 fixed, p33 percent, F2 fill weight, m6 min, M6 max.
use lazyui::canvas::Rect;
use lazyui::layout::{split, Dir, Size};

fn main() {
    let alphabet: Vec<(String, Size)> = [
        ("f0", Size::Fixed(0)), ("f3", Size::Fixed(3)), ("f7", Size::Fixed(7)),
        ("p0", Size::Pct(0)), ("p33", Size::Pct(33)), ("p50", Size::Pct(50)), ("p100", Size::Pct(100)),
        ("F1", Size::Fill(1)), ("F2", Size::Fill(2)),
        ("m2", Size::Min(2)), ("m6", Size::Min(6)),
        ("M2", Size::Max(2)), ("M6", Size::Max(6)),
    ]
    .into_iter()
    .map(|(n, s)| (n.to_string(), s))
    .collect();
    let mut lists: Vec<Vec<usize>> = Vec::new();
    for a in 0..alphabet.len() {
        lists.push(vec![a]);
        for b in 0..alphabet.len() {
            lists.push(vec![a, b]);
            for c in 0..alphabet.len() {
                lists.push(vec![a, b, c]);
            }
        }
    }
    let mut out = String::new();
    for l in &lists {
        let sizes: Vec<Size> = l.iter().map(|&i| alphabet[i].1).collect();
        let spec: Vec<&str> = l.iter().map(|&i| alphabet[i].0.as_str()).collect();
        for w in 0..=40 {
            for gap in 0..=2 {
                let r = split(Rect::new(0, 0, w, 1), Dir::Row, &sizes, gap);
                let cells: Vec<String> = r.iter().map(|q| format!("{}:{}", q.x, q.w)).collect();
                out.push_str(&format!("{w} {gap} {} -> {}\n", spec.join(" "), cells.join(" ")));
            }
        }
    }
    print!("{out}");
}
