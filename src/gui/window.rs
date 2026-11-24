//! The window backend: winit for the window and input, wgpu to put the rasterised frame on screen.
//! Same contract as the terminal loop: `drive_gui(ui, opts, handler)`; idle costs nothing (`ControlFlow::Wait`).

use super::present::{Cpu, Present};
use super::raster::{Pixels, Raster};
use super::tune::{Path, Render, Tuner};
use crate::canvas::{Canvas, Caps};
use crate::input::{Event, Key, Mods, MouseKind};
use crate::runtime::{Cx, Incoming};
use crate::term::Waker;
use crate::ui::Ui;
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalSize};
use winit::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key as WKey, ModifiersState, NamedKey};
use winit::window::{Window, WindowId};

#[derive(Clone)]
pub struct GuiOpts {
    pub title: String,
    /// font size in logical pixels (multiplied by the display scale)
    pub font_px: f32,
    pub font: Option<String>,
    /// initial window size in cells
    pub cols: u32,
    pub rows: u32,
    /// CPU, GPU, or choose from measured cost
    pub render: Render,
}

impl Default for GuiOpts {
    fn default() -> GuiOpts {
        GuiOpts { title: "lazyui".into(), font_px: 17.0, font: None, cols: 100, rows: 30, render: Render::Auto }
    }
}

struct App<'a, M: Send + 'static, F: FnMut(&mut Ui, Incoming<M>, &mut Cx<M>)> {
    ui: &'a mut Ui,
    handler: F,
    cx: Cx<M>,
    rx: Receiver<M>,
    opts: GuiOpts,
    window: Option<Arc<Window>>,
    present: Option<Box<dyn Present>>,
    tuner: Tuner,
    raster: Option<Raster>,
    pixels: Pixels,
    prev: Option<Canvas>,
    spare: Option<Canvas>,
    cells: (i32, i32),
    mods: Mods,
    cursor: (i32, i32),
    held: Option<u8>,
    wheel: f64,
    started: bool,
    last: Instant,
    error: Option<String>,
}

fn mods_of(m: ModifiersState) -> Mods {
    Mods { ctrl: m.control_key(), alt: m.alt_key(), shift: m.shift_key() }
}

fn map_key(ev: &KeyEvent, mods: Mods) -> Option<Event> {
    let named = |k: Key, m: Mods| Some(Event::Key(k, m));
    match &ev.logical_key {
        WKey::Named(n) => match n {
            NamedKey::Enter => named(Key::Enter, mods),
            NamedKey::Tab if mods.shift => named(Key::BackTab, Mods { shift: false, ..mods }),
            NamedKey::Tab => named(Key::Tab, mods),
            NamedKey::Backspace => named(Key::Backspace, mods),
            NamedKey::Delete => named(Key::Delete, mods),
            NamedKey::Insert => named(Key::Insert, mods),
            NamedKey::Escape => named(Key::Esc, mods),
            NamedKey::ArrowUp => named(Key::Up, mods),
            NamedKey::ArrowDown => named(Key::Down, mods),
            NamedKey::ArrowLeft => named(Key::Left, mods),
            NamedKey::ArrowRight => named(Key::Right, mods),
            NamedKey::Home => named(Key::Home, mods),
            NamedKey::End => named(Key::End, mods),
            NamedKey::PageUp => named(Key::PageUp, mods),
            NamedKey::PageDown => named(Key::PageDown, mods),
            NamedKey::Space => named(Key::Char(' '), Mods { shift: false, ..mods }),
            NamedKey::F1 => named(Key::F(1), mods),
            NamedKey::F2 => named(Key::F(2), mods),
            NamedKey::F3 => named(Key::F(3), mods),
            NamedKey::F4 => named(Key::F(4), mods),
            NamedKey::F5 => named(Key::F(5), mods),
            NamedKey::F6 => named(Key::F(6), mods),
            NamedKey::F7 => named(Key::F(7), mods),
            NamedKey::F8 => named(Key::F(8), mods),
            NamedKey::F9 => named(Key::F(9), mods),
            NamedKey::F10 => named(Key::F(10), mods),
            NamedKey::F11 => named(Key::F(11), mods),
            NamedKey::F12 => named(Key::F(12), mods),
            _ => None,
        },
        WKey::Character(s) => {
            if mods.ctrl || mods.alt {
                // a chord: the key itself, lowercase, with its modifiers (the text would be a control code)
                let c = s.chars().next()?.to_lowercase().next()?;
                Some(Event::Key(Key::Char(c), Mods { shift: false, ..mods }))
            } else {
                // plain typing: whatever the layout produced (shift already applied)
                let t = ev.text.as_ref().map(|t| t.as_str()).unwrap_or(s.as_str());
                let c = t.chars().next()?;
                Some(Event::Key(Key::Char(c), Mods::NONE))
            }
        }
        _ => None,
    }
}

impl<'a, M: Send + 'static, F: FnMut(&mut Ui, Incoming<M>, &mut Cx<M>)> App<'a, M, F> {
    fn resize(&mut self, w: u32, h: u32) {
        let Some(r) = &self.raster else { return };
        if let Some(p) = &mut self.present {
            p.resize(w, h);
        }
        let (w, h) = (w.max(1) as usize, h.max(1) as usize);
        self.pixels = Pixels::new(w, h, r.default_bg);
        self.cells = ((w / r.cw).max(1) as i32, (h / r.ch).max(1) as i32);
        self.prev = None;
        self.ui.dirty = true;
        if let Some(win) = &self.window {
            win.request_redraw();
        }
    }

    fn redraw(&mut self) {
        let (Some(r), Some(p)) = (self.raster.as_mut(), self.present.as_mut()) else { return };
        let t0 = Instant::now();
        let mut c = Canvas::reuse(self.spare.take(), self.cells.0, self.cells.1, Caps::FULL);
        self.ui.render(&mut c);
        r.paint(&mut self.pixels, self.prev.as_ref(), &c);
        self.spare = self.prev.replace(c);
        p.present(&self.pixels);
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        if let Some(to) = self.tuner.frame(ms) {
            let cost = self.tuner.cost();
            if self.switch(to) {
                eprintln!("lazyui: render {} ({cost:.1} ms/frame before)", if to == Path::Gpu { "cpu -> gpu" } else { "gpu -> cpu" });
            }
        }
    }

    /// Replace the presenter. A window can have only one surface at a time, so the old one goes first; if the new one
    /// cannot be made the old kind is rebuilt and the tuner is told the GPU is no option.
    fn switch(&mut self, to: Path) -> bool {
        let Some(win) = self.window.clone() else { return false };
        let size = win.inner_size();
        self.present = None;
        let made = make_present(to, &win, size.width, size.height);
        match made {
            Ok(p) => {
                self.present = Some(p);
                self.tuner.switched(to);
            }
            Err(e) => {
                eprintln!("lazyui: {e}; staying on the cpu");
                self.tuner.gpu_failed();
                self.present = make_present(Path::Cpu, &win, size.width, size.height).ok();
            }
        }
        self.prev = None;
        self.ui.dirty = true;
        win.request_redraw();
        self.present.as_ref().map(|p| p.path()) == Some(to)
    }

    fn cell_at(&self, x: f64, y: f64) -> (i32, i32) {
        match &self.raster {
            Some(r) => ((x / r.cw as f64).floor() as i32, (y / r.ch as f64).floor() as i32),
            None => (0, 0),
        }
    }

    fn feed(&mut self, ev: Event) {
        self.ui.dispatch(&ev);
    }

    /// After every batch of events: time, named events, finished effects, then what to wait for next.
    fn pump(&mut self, el: &ActiveEventLoop) {
        let now = Instant::now();
        self.ui.tick(now.duration_since(self.last).as_secs_f64().min(0.25));
        self.last = now;
        let mut quit = false;
        loop {
            let evs = self.ui.take_events();
            if evs.is_empty() {
                break;
            }
            for e in evs {
                if e.id == "app" && e.kind == "quit" {
                    quit = true;
                }
                (self.handler)(self.ui, Incoming::Ui(e), &mut self.cx);
            }
        }
        while let Ok(m) = self.rx.try_recv() {
            (self.handler)(self.ui, Incoming::Msg(m), &mut self.cx);
        }
        if quit || self.cx.quit_requested() || self.ui.quit {
            el.exit();
            return;
        }
        if self.ui.dirty {
            if let Some(w) = &self.window {
                w.request_redraw();
            }
        }
        el.set_control_flow(if self.ui.animating() {
            ControlFlow::WaitUntil(now + self.tuner.anim_interval())
        } else if let Some(s) = self.ui.next_deadline() {
            ControlFlow::WaitUntil(now + Duration::from_secs_f64(s.max(0.001)))
        } else {
            ControlFlow::Wait
        });
    }
}

impl<'a, M: Send + 'static, F: FnMut(&mut Ui, Incoming<M>, &mut Cx<M>)> ApplicationHandler for App<'a, M, F> {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        // the window is sized in cells, so the font must be known first (at scale 1; corrected below)
        let r0 = match Raster::system(self.opts.font_px, self.opts.font.as_deref()) {
            Ok(r) => r,
            Err(e) => {
                self.error = Some(e);
                el.exit();
                return;
            }
        };
        let size = LogicalSize::new((r0.cw as u32 * self.opts.cols) as f64, (r0.ch as u32 * self.opts.rows) as f64);
        let win = match el.create_window(Window::default_attributes().with_title(self.opts.title.clone()).with_inner_size(size)) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                self.error = Some(format!("window: {e}"));
                el.exit();
                return;
            }
        };
        let scale = win.scale_factor() as f32;
        let r = match Raster::system(self.opts.font_px * scale, self.opts.font.as_deref()) {
            Ok(r) => r,
            Err(e) => {
                self.error = Some(e);
                el.exit();
                return;
            }
        };
        let inner = win.inner_size();
        let mut path = self.tuner.path;
        let made = match make_present(path, &win, inner.width, inner.height) {
            Ok(p) => Ok(p),
            Err(e) if path == Path::Gpu => {
                eprintln!("lazyui: {e}; using the cpu");
                self.tuner.gpu_failed();
                path = Path::Cpu;
                make_present(path, &win, inner.width, inner.height)
            }
            Err(e) => Err(e),
        };
        match made {
            Ok(p) => {
                eprintln!("lazyui: window, render {}", if p.path() == Path::Gpu { "gpu" } else { "cpu" });
                self.present = Some(p)
            }
            Err(e) => {
                self.error = Some(e);
                el.exit();
                return;
            }
        }
        self.raster = Some(r);
        self.window = Some(win);
        self.resize(inner.width, inner.height);
        if !self.started {
            self.started = true;
            (self.handler)(self.ui, Incoming::Start, &mut self.cx);
        }
        self.pump(el);
    }

    fn user_event(&mut self, el: &ActiveEventLoop, _: ()) {
        self.pump(el);
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        self.pump(el);
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _: WindowId, ev: WindowEvent) {
        match ev {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Resized(PhysicalSize { width, height }) => self.resize(width, height),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                if let Ok(r) = Raster::system(self.opts.font_px * scale_factor as f32, self.opts.font.as_deref()) {
                    self.raster = Some(r);
                    if let Some(w) = &self.window {
                        let s = w.inner_size();
                        self.resize(s.width, s.height);
                    }
                }
            }
            WindowEvent::RedrawRequested => self.redraw(),
            WindowEvent::ModifiersChanged(m) => self.mods = mods_of(m.state()),
            WindowEvent::KeyboardInput { event, is_synthetic: false, .. } if event.state == ElementState::Pressed => {
                if let Some(e) = map_key(&event, self.mods) {
                    self.feed(e);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let (cx, cy) = self.cell_at(position.x, position.y);
                if (cx, cy) != self.cursor {
                    self.cursor = (cx, cy);
                    if let Some(b) = self.held {
                        self.feed(Event::Mouse(MouseKind::Drag(b), cx, cy, self.mods));
                    }
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let b = match button {
                    MouseButton::Left => 0,
                    MouseButton::Middle => 1,
                    MouseButton::Right => 2,
                    _ => return,
                };
                let (x, y) = self.cursor;
                if state == ElementState::Pressed {
                    self.held = Some(b);
                    self.feed(Event::Mouse(MouseKind::Down(b), x, y, self.mods));
                } else {
                    self.held = None;
                    self.feed(Event::Mouse(MouseKind::Up(b), x, y, self.mods));
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let lines = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y as f64,
                    MouseScrollDelta::PixelDelta(p) => p.y / self.raster.as_ref().map(|r| r.ch as f64).unwrap_or(16.0),
                };
                self.wheel += lines;
                while self.wheel.abs() >= 1.0 {
                    let up = self.wheel > 0.0;
                    self.wheel -= self.wheel.signum();
                    let (x, y) = self.cursor;
                    self.feed(Event::Mouse(if up { MouseKind::WheelUp } else { MouseKind::WheelDown }, x, y, self.mods));
                }
            }
            WindowEvent::Ime(winit::event::Ime::Commit(s)) => {
                self.feed(Event::Paste(s));
            }
            _ => {}
        }
    }
}

fn make_present(path: Path, win: &Arc<Window>, w: u32, h: u32) -> Result<Box<dyn Present>, String> {
    match path {
        Path::Cpu => Cpu::new(win.clone(), w, h).map(|p| Box::new(p) as Box<dyn Present>),
        #[cfg(feature = "gpu")]
        Path::Gpu => super::present::Gpu::new(win.clone(), w, h).map(|p| Box::new(p) as Box<dyn Present>),
        #[cfg(not(feature = "gpu"))]
        Path::Gpu => Err("this build has no gpu path (feature `gpu`)".into()),
    }
}

/// Run `ui` in a window until it quits. `handler` gets the same `Incoming` values as the terminal loop.
pub fn drive_gui<M: Send + 'static>(ui: &mut Ui, opts: GuiOpts, handler: impl FnMut(&mut Ui, Incoming<M>, &mut Cx<M>)) -> Result<(), String> {
    let render = opts.render;
    let event_loop = EventLoop::<()>::with_user_event().build().map_err(|e| format!("event loop: {e}"))?;
    let proxy = event_loop.create_proxy();
    let (tx, rx) = channel::<M>();
    let cx = Cx::new(tx, Waker::from_fn(move || {
        let _ = proxy.send_event(());
    }));
    let mut app = App {
        ui,
        handler,
        cx,
        rx,
        opts,
        window: None,
        present: None,
        tuner: Tuner::new(render, cfg!(feature = "gpu")),
        raster: None,
        pixels: Pixels::new(1, 1, [0, 0, 0]),
        prev: None,
        spare: None,
        cells: (1, 1),
        mods: Mods::NONE,
        cursor: (0, 0),
        held: None,
        wheel: 0.0,
        started: false,
        last: Instant::now(),
        error: None,
    };
    event_loop.run_app(&mut app).map_err(|e| format!("event loop: {e}"))?;
    match app.error {
        Some(e) => Err(e),
        None => Ok(()),
    }
}
