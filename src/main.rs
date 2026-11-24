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
  lazyui serve  SPEC [--port N] [--token T]  the UI over HTTP on 127.0.0.1 for agents: GET /tree /snap /describe /events, POST /do (script)
  lazyui select SPEC QUERY         find elements: #id, kind (list) or ~text; prints id kind x y w h
  lazyui themes                    list the built-in themes
  lazyui widgets ls|dirs|show NAME|add SRC [NAME]   shared widgets (see below)

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
  --catalog FILE.tsv translations (repeatable, later wins): texts written @key are translated
  --locale NAME      language of the catalog (pl, de, pt-BR); falls back to en, then to the key
  --widgets DIR      a folder of extra widget files (*.lui), counted as local (repeatable)
  --png FILE         snap: also write the frame as a PNG rendered with the window's rasteriser

run talks on the pipes while the UI is on /dev/tty:
  stdout  one JSON object per event:  {\"id\":\"q\",\"kind\":\"change\",\"text\":\"ab\",\"index\":-1,\"number\":0}
  stdin   KEY=VALUE sets a model value; list:ID=a,b,c sets the rows of a list; `quit` ends

script commands: size W H, set k=v, key NAME.., type TEXT, paste TEXT, click X Y, wheel X Y up|down, tick SEC,
  hotkey KEY NAME, every SEC NAME, focus ID, snap, tree, events,
  expect text \"..\" | no-text \"..\" | row N \"..\" | focus ID | model KEY VALUE | event ID KIND [TEXT]

widgets: a widget is a spec file with parameters (`params label bind max=100`, `$label` in the body, `slot` for children),
  used by its file name: `meter label=CPU bind=cpu`. Looked up in this order, later wins: built-in (compiled in, offline),
  global ($LAZYUI_WIDGETS or ~/.config/lazyui/widgets; `widgets add URL|FILE` downloads here), local (widgets/ next to
  the spec, ./widgets, --widgets DIR). `widgets ls` shows what is visible and where each came from.

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
    widget_dirs: Vec<std::path::PathBuf>,
    catalog: Vec<String>,
    locale: Option<String>,
    port: u16,
    token: Option<String>,
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
    let mut a = Args { cmd, pos: vec![], size: (80, 24), sets: vec![], theme: None, inline: None, mouse: true, hotkeys: vec![], every: vec![], watch: false, gui: false, render: "auto".into(), font: None, font_px: 17.0, png: None, widget_dirs: vec![], catalog: vec![], locale: None, port: 7878, token: None };
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
            "--catalog" => a.catalog.push(val("--catalog")),
            "--locale" => a.locale = Some(val("--locale")),
            "--port" => a.port = val("--port").parse().unwrap_or_else(|_| die("--port N")),
            "--token" => a.token = Some(val("--token")),
            "--widgets" => a.widget_dirs.push(val("--widgets").into()),
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
    let spec_dir = if spec_path == "-" { None } else { std::path::Path::new(spec_path).parent().map(|p| p.to_path_buf()) };
    let (widgets, problems) = lazyui::components::Widgets::standard(spec_dir.as_deref(), &a.widget_dirs);
    for p in problems {
        eprintln!("lazyui: widget skipped: {p}");
    }
    let mut ui = Ui::from_spec_with(&src, widgets).unwrap_or_else(|e| {
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
    if !a.catalog.is_empty() {
        let mut cat = lazyui::i18n::Catalog::new();
        for f in &a.catalog {
            let txt = std::fs::read_to_string(f).unwrap_or_else(|e| die(&format!("{f}: {e}")));
            cat.add_tsv(&txt).unwrap_or_else(|e| die(&format!("{f}: {e}")));
        }
        if let Some(l) = &a.locale {
            cat.set_locale(l);
        }
        ui.set_catalog(Some(cat));
    } else if a.locale.is_some() {
        die("--locale needs --catalog");
    }
    for (k, v) in &a.sets {
        ui.set_auto(k, v);
    }
    ui
}

fn widgets_cmd(a: &Args) {
    use lazyui::components::{global_dir, Scope, Widgets};
    let spec_dir = std::path::PathBuf::from(".");
    let (w, problems) = Widgets::standard(Some(&spec_dir), &a.widget_dirs);
    match a.pos.first().map(|s| s.as_str()).unwrap_or("ls") {
        "ls" => {
            for d in lazyui::native::all() {
                println!("{:<14} {:<8} {}", d.name, "native", d.doc);
            }
            for c in w.names() {
                let params: Vec<String> = Widgets::param_list(c).into_iter().map(|(k, d)| match d { Some(d) => format!("{k}={d}"), None => k }).collect();
                println!("{:<14} {:<8} {}  [{}]", c.name, c.scope.name(), c.doc, params.join(" "));
            }
            for p in problems {
                eprintln!("skipped: {p}");
            }
        }
        "dirs" => {
            println!("builtin  (compiled in)");
            println!("global   {}", global_dir().map(|d| d.display().to_string()).unwrap_or_else(|| "(no HOME)".into()));
            println!("local    ./widgets, widgets/ next to the spec, --widgets DIR");
        }
        "show" => {
            let n = a.pos.get(1).unwrap_or_else(|| die("widgets show NAME"));
            let c = w.get(n).unwrap_or_else(|| die(&format!("no widget named `{n}` (lazyui widgets ls)")));
            eprintln!("# {} ({}, {})", c.name, c.scope.name(), c.origin);
            print!("{}", c.text);
        }
        "add" => {
            let src = a.pos.get(1).unwrap_or_else(|| die("widgets add URL|FILE [NAME]"));
            let text = if src.starts_with("http://") || src.starts_with("https://") {
                let o = std::process::Command::new("curl").args(["-fsSL", "--max-time", "20", "--max-filesize", "262144", src]).output().unwrap_or_else(|e| die(&format!("curl: {e}")));
                if !o.status.success() {
                    die(&format!("could not download {src}"));
                }
                String::from_utf8(o.stdout).unwrap_or_else(|_| die("the download is not text"))
            } else {
                std::fs::read_to_string(src).unwrap_or_else(|e| die(&format!("{src}: {e}")))
            };
            let name = a.pos.get(2).cloned().unwrap_or_else(|| {
                let last = src.rsplit('/').next().unwrap_or("");
                last.strip_suffix(".lui").unwrap_or(last).to_string()
            });
            Widgets::empty().add_source(&name, &text, Scope::Global, src).unwrap_or_else(|e| die(&e));
            let dir = global_dir().unwrap_or_else(|| die("no global folder: set LAZYUI_WIDGETS or HOME"));
            let to = dir.join(format!("{name}.lui"));
            if to.exists() {
                die(&format!("{} already exists; move it away first (nothing is overwritten)", to.display()));
            }
            std::fs::create_dir_all(&dir).unwrap_or_else(|e| die(&format!("{}: {e}", dir.display())));
            std::fs::write(&to, text).unwrap_or_else(|e| die(&format!("{}: {e}", to.display())));
            println!("{name} -> {}", to.display());
        }
        other => die(&format!("widgets: unknown subcommand `{other}` (ls, dirs, show NAME, add SRC [NAME])")),
    }
}

fn main() {
    let a = parse_args();
    match a.cmd.as_str() {
        "widgets" => widgets_cmd(&a),
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
        "serve" => {
            let p = a.pos.first().unwrap_or_else(|| die("a SPEC file is needed"));
            let mut ui = load_ui(&a, p);
            let token = a.token.clone().or_else(|| std::env::var("LAZYUI_TOKEN").ok()).or_else(lazyui::serve::random_token).unwrap_or_else(|| die("no random source here: pass --token T"));
            let l = std::net::TcpListener::bind(("127.0.0.1", a.port)).unwrap_or_else(|e| die(&format!("port {}: {e}", a.port)));
            eprintln!("lazyui: serving {p} on http://127.0.0.1:{}  token: {token}", a.port);
            lazyui::serve::serve(&mut ui, &l, a.size, &token, None);
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
