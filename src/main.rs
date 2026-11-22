//! `lazyui`: run, snapshot, inspect and script a UI described in a `.lui` spec. The same engine is the library.

use lazyui::canvas::Caps;
use lazyui::headless;
use lazyui::runtime::{drive, Incoming, Opts};
use lazyui::session::{self, SpecMsg};
use lazyui::term::Mode;
use lazyui::theme::Theme;
use lazyui::ui::Ui;
use std::io::Read;
use std::process::exit;

const HELP: &str = "\
lazyui - terminal UI from a spec file, as a program or as a library

usage:
  lazyui run    SPEC [options]     show the UI on the terminal
  lazyui snap   SPEC [options]     print one settled frame as text
  lazyui tree   SPEC [options]     print the element tree as JSON (ids, rects, state)
  lazyui script SPEC FILE|-        run a script (see below), print what it prints, exit 1 on a failed expect
  lazyui select SPEC QUERY         find elements: #id, kind (list) or ~text; prints id kind x y w h
  lazyui themes                    list the built-in themes

options:
  --size WxH         headless size (default 80x24)
  --set KEY=VALUE    model value (repeatable)
  --theme NAME|FILE  built-in name or a theme file
  --inline N         run: draw N rows under the prompt instead of the alternate screen
  --no-mouse         run: do not capture the mouse
  --hotkey KEY=NAME  run: KEY (ctrl-r, f5, q) emits event app/NAME (repeatable); ctrl-q always quits
  --every SEC=NAME   run: emit app/NAME every SEC seconds (repeatable)
  --watch            run: reload SPEC when the file changes; a bad edit keeps the old UI
  --gui              run: open a window instead of using the terminal (same spec, same pipes)
  --render MODE      gui: auto (default: CPU, GPU when frames measure slow and it helps) | cpu | gpu
  --font FILE.ttf    gui/--png: monospace font (default: DejaVu Sans Mono via fontconfig)
  --font-px N        gui/--png: font size in logical pixels (default 17)
  --png FILE         snap: also write the frame as a PNG rendered with the window's rasteriser

run talks on the pipes while the UI is on /dev/tty:
  stdout  one JSON object per event:  {\"id\":\"q\",\"kind\":\"change\",\"text\":\"ab\",\"index\":-1,\"number\":0}
  stdin   KEY=VALUE sets a model value; list:ID=a,b,c sets the rows of a list; `quit` ends

script commands: size W H, set k=v, key NAME.., type TEXT, paste TEXT, click X Y, wheel X Y up|down, tick SEC,
  hotkey KEY NAME, every SEC NAME, focus ID, snap, tree, events,
  expect text \"..\" | no-text \"..\" | row N \"..\" | focus ID | model KEY VALUE | event ID KIND [TEXT]

spec: indented elements, `kind attr=value attr=\"with spaces\"`; kinds: row col panel pages spacer text progressbar
  searchbar buttonbar list keybar. Sizes: size=N|fill|fill:W|pct:P|min:N|max:N.
";

struct Args {
    cmd: String,
    pos: Vec<String>,
    size: (i32, i32),
    sets: Vec<(String, String)>,
    theme: Option<String>,
    inline: Option<i32>,
    mouse: bool,
    hotkeys: Vec<(String, String)>,
    every: Vec<(f64, String)>,
    watch: bool,
    gui: bool,
    render: String,
    font: Option<String>,
    font_px: f32,
    png: Option<String>,
}

fn die(msg: &str) -> ! {
    eprintln!("lazyui: {msg}");
    exit(2);
}

fn parse_args() -> Args {
    let mut it = std::env::args().skip(1);
    let cmd = it.next().unwrap_or_else(|| {
        print!("{HELP}");
        exit(0)
    });
    let mut a = Args { cmd, pos: vec![], size: (80, 24), sets: vec![], theme: None, inline: None, mouse: true, hotkeys: vec![], every: vec![], watch: false, gui: false, render: "auto".into(), font: None, font_px: 17.0, png: None };
    let mut rest: Vec<String> = it.collect();
    rest.reverse();
    while let Some(x) = rest.pop() {
        let mut val = |name: &str| rest.pop().unwrap_or_else(|| die(&format!("{name} needs a value")));
        match x.as_str() {
            "--size" => {
                let v = val("--size");
                let (w, h) = v.split_once('x').unwrap_or_else(|| die("--size WxH"));
                a.size = (w.parse().unwrap_or_else(|_| die("--size WxH")), h.parse().unwrap_or_else(|_| die("--size WxH")));
            }
            "--set" => {
                let v = val("--set");
                let (k, v) = v.split_once('=').unwrap_or_else(|| die("--set KEY=VALUE"));
                a.sets.push((k.to_string(), v.to_string()));
            }
            "--theme" => a.theme = Some(val("--theme")),
            "--inline" => a.inline = Some(val("--inline").parse().unwrap_or_else(|_| die("--inline N"))),
            "--no-mouse" => a.mouse = false,
            "--hotkey" => {
                let v = val("--hotkey");
                let (k, n) = v.split_once('=').unwrap_or_else(|| die("--hotkey KEY=NAME"));
                a.hotkeys.push((k.to_string(), n.to_string()));
            }
            "--every" => {
                let v = val("--every");
                let (s, n) = v.split_once('=').unwrap_or_else(|| die("--every SEC=NAME"));
                a.every.push((s.parse().unwrap_or_else(|_| die("--every SEC=NAME")), n.to_string()));
            }
            "--watch" => a.watch = true,
            "--gui" => a.gui = true,
            "--render" => a.render = val("--render"),
            "--font" => a.font = Some(val("--font")),
            "--font-px" => a.font_px = val("--font-px").parse().unwrap_or_else(|_| die("--font-px N")),
            "--png" => a.png = Some(val("--png")),
            "-h" | "--help" | "help" => {
                print!("{HELP}");
                exit(0)
            }
            s if s.starts_with("--") => die(&format!("unknown option {s}")),
            _ => a.pos.push(x),
        }
    }
    a
}

fn load_ui(a: &Args, spec_path: &str) -> Ui {
    let src = if spec_path == "-" {
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s).unwrap_or_else(|e| die(&format!("stdin: {e}")));
        s
    } else {
        std::fs::read_to_string(spec_path).unwrap_or_else(|e| die(&format!("{spec_path}: {e}")))
    };
    let mut ui = Ui::from_spec(&src).unwrap_or_else(|e| {
        eprintln!("{spec_path}: {e}");
        exit(1)
    });
    if let Some(t) = &a.theme {
        ui.theme = match Theme::by_name(t) {
            Some(th) => th,
            None => {
                let txt = std::fs::read_to_string(t).unwrap_or_else(|_| die(&format!("no theme named `{t}` and no file by that name")));
                Theme::parse(&txt).unwrap_or_else(|e| die(&format!("{t}: {e}")))
            }
        };
    }
    for (k, v) in &a.sets {
        ui.set_auto(k, v);
    }
    ui
}

fn main() {
    let a = parse_args();
    match a.cmd.as_str() {
        "themes" => {
            for n in ["green", "matrix", "light"] {
                println!("{n}");
            }
        }
        "snap" | "tree" => {
            let p = a.pos.first().unwrap_or_else(|| die("a SPEC file is needed"));
            let mut ui = load_ui(&a, p);
            let (w, h) = a.size;
            if a.cmd == "snap" {
                print!("{}", headless::snapshot(&mut ui, w, h));
                if let Some(out) = &a.png {
                    write_png(&mut ui, a.size, out, &a);
                }
            } else {
                println!("{}", headless::tree(&mut ui, w, h));
            }
        }
        "select" => {
            let (p, q) = match (a.pos.first(), a.pos.get(1)) {
                (Some(p), Some(q)) => (p, q),
                _ => die("lazyui select SPEC QUERY"),
            };
            let mut ui = load_ui(&a, p);
            let (w, h) = a.size;
            headless::settle(&mut ui, w, h, 10.0);
            let found = ui.find(q);
            for (id, kind, r) in &found {
                println!("{id} {kind} {} {} {} {}", r.x, r.y, r.w, r.h);
            }
            if found.is_empty() {
                exit(1);
            }
        }
        "script" => {
            let (p, f) = match (a.pos.first(), a.pos.get(1)) {
                (Some(p), Some(f)) => (p, f),
                _ => die("lazyui script SPEC FILE|-"),
            };
            if p == "-" && f == "-" {
                die("only one of SPEC and FILE can be stdin");
            }
            let mut ui = load_ui(&a, p);
            let script = if f == "-" {
                let mut s = String::new();
                std::io::stdin().read_to_string(&mut s).unwrap_or_else(|e| die(&format!("stdin: {e}")));
                s
            } else {
                std::fs::read_to_string(f).unwrap_or_else(|e| die(&format!("{f}: {e}")))
            };
            match headless::run_script(&mut ui, &script) {
                Ok(out) => print!("{out}"),
                Err(e) => {
                    eprintln!("{f}: {e}");
                    exit(1);
                }
            }
        }
        "run" => run(&a),
        other => die(&format!("unknown command `{other}` (try `lazyui help`)")),
    }
}

fn run(a: &Args) {
    let p = a.pos.first().unwrap_or_else(|| die("a SPEC file is needed")).clone();
    if p == "-" {
        die("run needs a spec file (stdin carries the model)");
    }
    let mut ui = load_ui(a, &p);
    for (k, n) in &a.hotkeys {
        if !ui.hotkey(k, n) {
            die(&format!("--hotkey: unknown key `{k}`"));
        }
    }
    for (s, n) in &a.every {
        ui.every(*s, n);
    }
    ui.hotkey("ctrl-q", "quit");
    if a.gui {
        run_gui(a, ui, p);
        return;
    }
    let opts = Opts { mode: a.inline.map(Mode::Inline).unwrap_or(Mode::Full), mouse: a.mouse, caps: Caps::detect() };
    let path = p.clone();
    let watch = a.watch;
    let res = drive::<SpecMsg>(&mut ui, opts, |ui, inc, cx| match inc {
        Incoming::Start => {
            let (tx, w) = cx.channel();
            session::spawn_feeds(&path, watch, move |m| {
                let ok = tx.send(m).is_ok();
                w.wake();
                ok
            });
        }
        Incoming::Ui(e) => session::print_event(&e),
        Incoming::Msg(m) => {
            if session::handle(ui, &path, m) {
                cx.quit();
            }
        }
    });
    if let Err(e) = res {
        eprintln!("lazyui: {e}");
        exit(1);
    }
}

#[cfg(feature = "gui")]
fn write_png(ui: &mut Ui, (w, h): (i32, i32), out: &str, a: &Args) {
    use lazyui::canvas::Canvas;
    use lazyui::gui::{png, raster::{Pixels, Raster}};
    let mut c = Canvas::new(w, h, Caps::FULL);
    ui.render(&mut c);
    let mut r = Raster::system(a.font_px, a.font.as_deref()).unwrap_or_else(|e| die(&e));
    let mut px = Pixels::new(w as usize * r.cw, h as usize * r.ch, r.default_bg);
    r.paint(&mut px, None, &c);
    std::fs::write(out, png::encode(px.w as u32, px.h as u32, &px.rgba)).unwrap_or_else(|e| die(&format!("{out}: {e}")));
    eprintln!("{out}: {}x{} px, cell {}x{}", px.w, px.h, r.cw, r.ch);
}

#[cfg(not(feature = "gui"))]
fn write_png(_: &mut Ui, _: (i32, i32), _: &str, _: &Args) {
    die("this build has no window backend (cargo feature `gui`)");
}

#[cfg(feature = "gui")]
fn run_gui(a: &Args, mut ui: Ui, path: String) {
    use lazyui::gui::{drive_gui, GuiOpts, Render};
    let render = Render::parse(&a.render).unwrap_or_else(|| die("--render auto|cpu|gpu"));
    let (cols, rows) = if a.size == (80, 24) { (100, 30) } else { (a.size.0 as u32, a.size.1 as u32) };
    let opts = GuiOpts { title: format!("lazyui - {path}"), font_px: a.font_px, font: a.font.clone(), cols, rows, render };
    let watch = a.watch;
    let res = drive_gui::<SpecMsg>(&mut ui, opts, |ui, inc, cx| match inc {
        Incoming::Start => {
            let (tx, w) = cx.channel();
            let p = path.clone();
            session::spawn_feeds(&p, watch, move |m| {
                let ok = tx.send(m).is_ok();
                w.wake();
                ok
            });
        }
        Incoming::Ui(e) => session::print_event(&e),
        Incoming::Msg(m) => {
            if session::handle(ui, &path, m) {
                cx.quit();
            }
        }
    });
    if let Err(e) = res {
        eprintln!("lazyui: {e}");
        exit(1);
    }
}

#[cfg(not(feature = "gui"))]
fn run_gui(_: &Args, _: Ui, _: String) {
    die("this build has no window backend (cargo feature `gui`)");
}
