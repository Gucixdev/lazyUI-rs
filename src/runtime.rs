//! The event loop. One thread draws and handles input; effects run on their own threads and come back as messages.
//! Idle costs nothing: the loop sleeps in `poll` until input, a message, a signal, a timer or an animation frame.

use crate::canvas::{diff, Canvas, Caps};
use crate::input::{Event, Key, Mods, Parser};
use crate::term::{Mode, Term, Waker};
use crate::ui::{El, Ui, UiEvent};
use std::io;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Clone, Copy)]
pub struct Opts {
    pub mode: Mode,
    pub mouse: bool,
    pub caps: Caps,
}

impl Default for Opts {
    fn default() -> Opts {
        Opts { mode: Mode::Full, mouse: true, caps: Caps::detect() }
    }
}

/// What a program can ask of the loop. Effects are plain closures run on a thread; their result returns as a message.
pub struct Cx<M> {
    tx: Sender<M>,
    waker: Waker,
    quit: bool,
}

impl<M: Send + 'static> Cx<M> {
    pub fn spawn<F: FnOnce() -> M + Send + 'static>(&self, f: F) {
        let (tx, w) = (self.tx.clone(), self.waker);
        std::thread::spawn(move || {
            let m = f();
            if tx.send(m).is_ok() {
                w.wake();
            }
        });
    }
    /// Hand out a sender plus waker for a long-lived thread (a file watcher, a socket).
    pub fn channel(&self) -> (Sender<M>, Waker) {
        (self.tx.clone(), self.waker)
    }
    pub fn send(&self, m: M) {
        if self.tx.send(m).is_ok() {
            self.waker.wake();
        }
    }
    pub fn quit(&mut self) {
        self.quit = true;
    }
}

/// "Only the answer to the newest request counts": `next()` before starting a request, tag the answer with the token,
/// `is_current(token)` when it arrives. Older answers are dropped, never applied out of order.
#[derive(Clone, Default)]
pub struct Latest(Arc<AtomicU64>);

impl Latest {
    pub fn new() -> Latest {
        Latest::default()
    }
    pub fn next(&self) -> u64 {
        self.0.fetch_add(1, Ordering::SeqCst) + 1
    }
    pub fn is_current(&self, token: u64) -> bool {
        self.0.load(Ordering::SeqCst) == token
    }
}

pub enum Incoming<M> {
    Start,
    Ui(UiEvent),
    Msg(M),
}

/// The loop itself. `handler` sees every named UI event and every message and may change the `Ui` freely.
pub fn drive<M: Send + 'static>(ui: &mut Ui, opts: Opts, mut handler: impl FnMut(&mut Ui, Incoming<M>, &mut Cx<M>)) -> io::Result<()> {
    let mut term = Term::open(opts.mode, opts.mouse)?;
    let (tx, rx) = channel::<M>();
    let mut cx = Cx { tx, waker: term.waker(), quit: false };
    let (mut w, mut h) = term.area();
    if let Mode::Inline(n) = opts.mode {
        term.reserve_inline(n.clamp(1, term.size().1))?;
    }
    let mut parser = Parser::new();
    let mut prev: Option<Canvas> = None;
    let mut last = Instant::now();
    handler(ui, Incoming::Start, &mut cx);
    let mut buf = [0u8; 4096];
    let mut out = String::new();
    loop {
        // named events first: they may change the model, which must show in this frame
        loop {
            let evs = ui.take_events();
            if evs.is_empty() {
                break;
            }
            for e in evs {
                if e.id == "app" && e.kind == "quit" {
                    cx.quit = true;
                }
                handler(ui, Incoming::Ui(e), &mut cx);
            }
        }
        if cx.quit || ui.quit || term.terminated() {
            break;
        }
        if ui.dirty || prev.is_none() {
            let mut c = Canvas::new(w, h, opts.caps);
            ui.render(&mut c);
            out.clear();
            diff(prev.as_ref(), &c, &mut out);
            if !out.is_empty() {
                term.write_str(&out)?;
            }
            prev = Some(c);
        }
        let timeout = if ui.animating() {
            16
        } else if parser.pending() {
            25
        } else {
            match ui.next_deadline() {
                Some(s) => ((s * 1000.0).ceil() as i32).clamp(1, 60_000),
                None => -1,
            }
        };
        let (readable, woken) = term.poll(timeout);
        if readable {
            let n = term.read(&mut buf)?;
            if n == 0 {
                break; // the terminal is gone
            }
            for ev in parser.feed(&buf[..n]) {
                feed(ui, &mut cx, &ev);
            }
        } else if parser.pending() && !woken {
            for ev in parser.flush() {
                feed(ui, &mut cx, &ev);
            }
        }
        if woken {
            while let Ok(m) = rx.try_recv() {
                handler(ui, Incoming::Msg(m), &mut cx);
            }
        }
        if term.take_resize() {
            let (nw, nh) = term.area();
            w = nw;
            h = nh;
            prev = None;
            ui.dirty = true;
        }
        let now = Instant::now();
        ui.tick(now.duration_since(last).as_secs_f64().min(0.25));
        last = now;
    }
    Ok(())
}

fn feed<M>(ui: &mut Ui, cx: &mut Cx<M>, ev: &Event) {
    let used = ui.dispatch(ev);
    // ctrl-c quits unless the program bound it
    if !used {
        if let Event::Key(Key::Char('c'), Mods { ctrl: true, .. }) = ev {
            cx.quit = true;
        }
    }
}

// ---- the embedding API ------------------------------------------------------------------------------------

/// Elm shape: state in `self`, a pure `view`, one `update`. `event` maps what the Ui reports to a message.
pub trait Program {
    type Msg: Send + 'static;
    fn init(&mut self, _cx: &mut Cx<Self::Msg>) {}
    fn update(&mut self, msg: Self::Msg, cx: &mut Cx<Self::Msg>);
    fn view(&self) -> El;
    fn event(&mut self, _e: &UiEvent) -> Option<Self::Msg> {
        None
    }
    /// Called once with the Ui before the first frame: hotkeys, `every` timers, theme.
    fn setup(&mut self, _ui: &mut Ui) {}
}

pub fn run<P: Program>(p: &mut P, opts: Opts) -> io::Result<()> {
    let mut ui = Ui::new(p.view());
    p.setup(&mut ui);
    drive(&mut ui, opts, |ui, inc, cx| {
        let msg = match inc {
            Incoming::Start => {
                p.init(cx);
                None
            }
            Incoming::Ui(e) => p.event(&e),
            Incoming::Msg(m) => Some(m),
        };
        if let Some(m) = msg {
            p.update(m, cx);
            ui.set_tree(p.view());
        }
    })
}

/// The same loop headless, for tests of an embedding: feed events, get the frame. No terminal, virtual time.
pub struct Harness<P: Program> {
    pub program: P,
    pub ui: Ui,
    rx: std::sync::mpsc::Receiver<P::Msg>,
    cx: Cx<P::Msg>,
    pub size: (i32, i32),
}

impl<P: Program> Harness<P> {
    pub fn new(mut program: P, w: i32, h: i32) -> Harness<P> {
        let mut ui = Ui::new(program.view());
        program.setup(&mut ui);
        let (tx, rx) = channel();
        // the waker is a dummy: a pipe nobody polls
        let mut fds = [0 as libc::c_int; 2];
        unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_NONBLOCK | libc::O_CLOEXEC) };
        let mut cx = Cx { tx, waker: Waker::from_fd(fds[1]), quit: false };
        program.init(&mut cx);
        let mut hn = Harness { program, ui, rx, cx, size: (w, h) };
        hn.pump();
        hn
    }
    /// Deliver queued UI events and finished effects until nothing is left.
    pub fn pump(&mut self) {
        for _ in 0..1000 {
            let mut any = false;
            for e in self.ui.take_events() {
                any = true;
                if e.id == "app" && e.kind == "quit" {
                    self.cx.quit = true;
                }
                if let Some(m) = self.program.event(&e) {
                    self.program.update(m, &mut self.cx);
                    self.ui.set_tree(self.program.view());
                }
            }
            while let Ok(m) = self.rx.try_recv() {
                any = true;
                self.program.update(m, &mut self.cx);
                self.ui.set_tree(self.program.view());
            }
            if !any {
                break;
            }
        }
    }
    /// Block (up to a second) until an effect's message arrives, then pump.
    pub fn wait_msg(&mut self) {
        if let Ok(m) = self.rx.recv_timeout(Duration::from_secs(1)) {
            self.program.update(m, &mut self.cx);
            self.ui.set_tree(self.program.view());
        }
        self.pump();
    }
    pub fn send(&mut self, ev: Event) {
        let _ = self.ui.dispatch(&ev);
        self.pump();
    }
    pub fn tick(&mut self, s: f64) {
        self.ui.tick(s);
        self.pump();
    }
    pub fn frame(&mut self) -> String {
        crate::headless::snapshot(&mut self.ui, self.size.0, self.size.1)
    }
    pub fn quit_requested(&self) -> bool {
        self.cx.quit
    }
}
