//! Self-tuning: which way to put pixels on the screen, and how fast to animate, from what frames really cost.
//!
//! Pure logic (no window, no clock): feed it the measured milliseconds of each frame, it answers with a path to switch
//! to, or nothing. CPU is the start: a terminal-style frame repaints a few cells and a memcpy is cheaper than waking a
//! GPU. When frames stay slow (a big window, a lot changing) it tries the GPU; if that does not make frames clearly
//! cheaper it goes back and stops trying, so a machine where the GPU is no help never pays for the experiment twice.

use std::time::Duration;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Render {
    /// start on the CPU, move to the GPU when it measurably helps
    Auto,
    Cpu,
    Gpu,
}

impl Render {
    pub fn parse(s: &str) -> Option<Render> {
        match s {
            "auto" => Some(Render::Auto),
            "cpu" => Some(Render::Cpu),
            "gpu" => Some(Render::Gpu),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Path {
    Cpu,
    Gpu,
}

/// ms per frame above which the CPU path counts as slow (60 Hz leaves 16.7; the app needs room too)
const SLOW_MS: f64 = 6.0;
/// ms per frame below which the GPU path is not worth keeping on
const CHEAP_MS: f64 = 1.0;
const SLOW_FRAMES: u32 = 20;
const CHEAP_FRAMES: u32 = 240;
/// frames on a new path before judging it
const TRIAL: u32 = 30;
/// the new path must be at least this much cheaper than the old one
const GAIN: f64 = 0.8;

pub struct Tuner {
    gpu_ok: bool,
    pub path: Path,
    ema: f64,
    slow: u32,
    cheap: u32,
    trial: Option<(Path, f64, u32)>, // (path we came from, its cost, frames so far)
    locked: bool,
}

impl Tuner {
    /// `gpu_compiled`: whether this build has a GPU path at all.
    pub fn new(mode: Render, gpu_compiled: bool) -> Tuner {
        let gpu_ok = gpu_compiled && mode != Render::Cpu;
        let path = if mode == Render::Gpu && gpu_ok { Path::Gpu } else { Path::Cpu };
        Tuner { gpu_ok, path, ema: 0.0, slow: 0, cheap: 0, trial: None, locked: mode != Render::Auto || !gpu_ok }
    }

    /// The GPU could not be created (or died): stay on the CPU for good.
    pub fn gpu_failed(&mut self) {
        self.gpu_ok = false;
        self.locked = true;
        self.path = Path::Cpu;
        self.trial = None;
    }

    pub fn cost(&self) -> f64 {
        self.ema
    }

    /// One frame took `ms`. Returns the path to switch to, if any; the caller switches and then calls `switched`.
    pub fn frame(&mut self, ms: f64) -> Option<Path> {
        self.ema = if self.ema == 0.0 { ms } else { self.ema * 0.85 + ms * 0.15 };
        if let Some((from, before, n)) = self.trial {
            if n + 1 < TRIAL {
                self.trial = Some((from, before, n + 1));
                return None;
            }
            self.trial = None;
            if self.ema > before * GAIN {
                // no real gain: back to where we were, and do not experiment again
                self.locked = true;
                return Some(from);
            }
            return None;
        }
        if self.locked {
            return None;
        }
        match self.path {
            Path::Cpu => {
                self.slow = if self.ema > SLOW_MS { self.slow + 1 } else { 0 };
                (self.slow >= SLOW_FRAMES && self.gpu_ok).then_some(Path::Gpu)
            }
            Path::Gpu => {
                self.cheap = if self.ema < CHEAP_MS { self.cheap + 1 } else { 0 };
                (self.cheap >= CHEAP_FRAMES).then_some(Path::Cpu)
            }
        }
    }

    /// The caller changed the path; a move the tuner suggested is judged after `TRIAL` frames.
    pub fn switched(&mut self, to: Path) {
        let from = self.path;
        if self.trial.is_none() && !self.locked && from != to {
            self.trial = Some((from, self.ema, 0));
        }
        self.path = to;
        self.slow = 0;
        self.cheap = 0;
    }

    /// Time between animation frames: 60 Hz while frames are cheap, slower when they are not, never past 10 Hz.
    pub fn anim_interval(&self) -> Duration {
        Duration::from_secs_f64((self.ema * 2.0).clamp(16.0, 100.0) / 1000.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(t: &mut Tuner, ms: f64, n: u32) -> Option<Path> {
        let mut last = None;
        for _ in 0..n {
            if let Some(p) = t.frame(ms) {
                last = Some(p);
                break;
            }
        }
        last
    }

    #[test]
    fn cheap_frames_stay_on_the_cpu() {
        let mut t = Tuner::new(Render::Auto, true);
        assert_eq!(feed(&mut t, 0.5, 1000), None);
        assert_eq!(t.path, Path::Cpu);
    }

    #[test]
    fn slow_frames_move_to_the_gpu_and_stay_when_it_helps() {
        let mut t = Tuner::new(Render::Auto, true);
        assert_eq!(feed(&mut t, 12.0, 100), Some(Path::Gpu));
        t.switched(Path::Gpu);
        assert_eq!(feed(&mut t, 2.0, 200), None, "a clear gain keeps the GPU");
        assert_eq!(t.path, Path::Gpu);
    }

    #[test]
    fn a_gpu_that_does_not_help_is_dropped_and_not_retried() {
        let mut t = Tuner::new(Render::Auto, true);
        assert_eq!(feed(&mut t, 12.0, 100), Some(Path::Gpu));
        t.switched(Path::Gpu);
        assert_eq!(feed(&mut t, 12.0, 200), Some(Path::Cpu));
        t.switched(Path::Cpu);
        assert_eq!(feed(&mut t, 12.0, 1000), None, "locked after a failed trial");
    }

    #[test]
    fn idle_gpu_goes_back_to_the_cpu() {
        let mut t = Tuner::new(Render::Gpu, true);
        assert_eq!(t.path, Path::Gpu);
        // forced mode never moves
        assert_eq!(feed(&mut t, 0.1, 1000), None);
        let mut a = Tuner::new(Render::Auto, true);
        assert_eq!(feed(&mut a, 12.0, 100), Some(Path::Gpu));
        a.switched(Path::Gpu);
        assert_eq!(feed(&mut a, 0.2, 1000), Some(Path::Cpu));
    }

    #[test]
    fn no_gpu_build_or_forced_cpu_never_asks() {
        for mut t in [Tuner::new(Render::Auto, false), Tuner::new(Render::Cpu, true)] {
            assert_eq!(feed(&mut t, 50.0, 500), None);
            assert_eq!(t.path, Path::Cpu);
        }
    }

    #[test]
    fn failed_gpu_locks_to_cpu() {
        let mut t = Tuner::new(Render::Gpu, true);
        t.gpu_failed();
        assert_eq!(t.path, Path::Cpu);
        assert_eq!(feed(&mut t, 50.0, 500), None);
    }

    #[test]
    fn animation_slows_down_when_frames_are_slow() {
        let mut t = Tuner::new(Render::Auto, false);
        t.frame(1.0);
        assert_eq!(t.anim_interval(), Duration::from_millis(16));
        for _ in 0..100 {
            t.frame(30.0);
        }
        assert!(t.anim_interval() > Duration::from_millis(50));
        for _ in 0..100 {
            t.frame(500.0);
        }
        assert_eq!(t.anim_interval(), Duration::from_millis(100));
    }
}
