//! `cargo run --release --no-default-features --example bench [ROWS]`: where a frame's time goes with a big tree.
//! Prints median microseconds per phase over repeated runs, plus the size of the tree.
use lazyui::canvas::{self, Caps, Canvas, Style};
use lazyui::input::{Event, Mods, MouseKind};
use lazyui::ui::Ui;
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
use std::time::Instant;

/// Counts allocations so a phase can say "N per element", the number the layout work is aimed at.
struct Counting;
static ALLOCS: AtomicUsize = AtomicUsize::new(0);
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Relaxed);
        System.alloc(l)
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        System.dealloc(p, l)
    }
}
#[global_allocator]
static A: Counting = Counting;

fn allocs<F: FnMut()>(mut f: F) -> usize {
    let before = ALLOCS.load(Relaxed);
    f();
    ALLOCS.load(Relaxed) - before
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

fn time<F: FnMut()>(reps: usize, mut f: F) -> f64 {
    let mut v = Vec::with_capacity(reps);
    for _ in 0..reps {
        let t = Instant::now();
        f();
        v.push(t.elapsed().as_secs_f64() * 1e6);
    }
    median(v)
}

fn main() {
    let rows: usize = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(2000);
    let mut spec = String::from("col\n");
    for i in 0..rows {
        spec.push_str(&format!("  row id=r{i} size=1\n    text id=a{i} text=\"item {i}\" size=fill\n    text id=b{i} text=\"{}\" size=8\n", i % 97));
    }
    let mut u = Ui::from_spec(&spec).unwrap_or_else(|e| panic!("{e}"));
    let (w, h) = (120, 40);
    let mut c = Canvas::new(w, h, Caps::ASCII);
    u.relayout(w, h);
    u.render(&mut c);
    let tree = lazyui::ui::parse_spec(&spec).unwrap();
    let n = 1 + rows * 3;
    println!("elements {n}  canvas {w}x{h}");
    println!("relayout cached {:>9.1} us", time(30, || u.relayout(w, h)));
    let clone = time(30, || {
        std::hint::black_box(tree.clone());
    });
    let cold = time(30, || {
        u.set_tree(tree.clone());
        u.relayout(w, h);
    });
    let (a_clone, a_cold) = (allocs(|| drop(tree.clone())), allocs(|| {
        let t = tree.clone();
        u.set_tree(t);
        u.relayout(w, h);
    }));
    println!("allocations     tree clone {} ({:.2}/element), clone+set_tree+layout {} -> layout alone {} ({:.2}/element)", a_clone, a_clone as f64 / n as f64, a_cold, a_cold - a_clone, (a_cold - a_clone) as f64 / n as f64);
    println!("relayout cold   {:>9.1} us  (tree clone {:.1} us not counted)", cold - clone, clone);
    println!("cold frame      {:>9.1} us  (what an Elm-style view pays per event)", time(30, || {
        u.set_tree(tree.clone());
        u.render(&mut c);
    }) - clone);
    println!("render          {:>9.1} us", time(30, || u.render(&mut c)));
    let last = format!("a{}", rows - 1);
    println!("find #id (last) {:>9.1} us", time(30, || {
        std::hint::black_box(u.find(&format!("#{last}")));
    }));
    println!("click (hit)     {:>9.1} us", time(30, || {
        u.dispatch(&Event::Mouse(MouseKind::Down(0), 3, 3, Mods::NONE));
        u.dispatch(&Event::Mouse(MouseKind::Up(0), 3, 3, Mods::NONE));
        u.take_events();
    }));
    println!("tick+render     {:>9.1} us", time(30, || {
        u.tick(1.0 / 60.0);
        u.render(&mut c);
    }));
    // the cost a frame pays to find out what changed, on a bigger screen with one cell different
    let (bw, bh) = (240, 70);
    let (mut a, mut b) = (Canvas::new(bw, bh, Caps::FULL), Canvas::new(bw, bh, Caps::FULL));
    for y in 0..bh {
        a.text(0, y, &"the quick brown fox jumps over the lazy dog ".repeat(6), Style::default());
        b.text(0, y, &"the quick brown fox jumps over the lazy dog ".repeat(6), Style::default());
    }
    b.text(100, 30, "X", Style::default());
    println!("damage {bw}x{bh}, one cell changed   {:>7.1} us  -> {:?}", time(200, || {
        std::hint::black_box(canvas::damage(Some(&a), &b));
    }), canvas::damage(Some(&a), &b));
    let mut out = String::new();
    println!("diff   {bw}x{bh}, one cell changed   {:>7.1} us", time(200, || {
        out.clear();
        canvas::diff(Some(&a), &b, &mut out);
    }));
    println!("damage {bw}x{bh}, nothing changed    {:>7.1} us", time(200, || {
        std::hint::black_box(canvas::damage(Some(&b), &b));
    }));
}
