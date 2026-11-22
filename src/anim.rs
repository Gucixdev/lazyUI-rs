//! Animation as maths: springs (RK4) and tweens. No clock inside: the caller says how much time passed.

/// x'' = -k (x - target) - c x'. Retargeting keeps the velocity, so motion never jumps.
#[derive(Clone, Copy, Debug)]
pub struct Spring {
    pub x: f64,
    pub v: f64,
    pub target: f64,
    pub k: f64,
    pub c: f64,
}

impl Spring {
    /// Critically damped: reaches the target without overshoot.
    pub fn smooth(x: f64) -> Spring {
        Spring::with(x, 120.0, 1.0)
    }
    pub fn firm(x: f64) -> Spring {
        Spring::with(x, 300.0, 1.0)
    }
    /// `ratio` < 1 overshoots (bouncy), 1 is critical, > 1 is sluggish.
    pub fn with(x: f64, k: f64, ratio: f64) -> Spring {
        Spring { x, v: 0.0, target: x, k, c: 2.0 * k.sqrt() * ratio }
    }
    pub fn bouncy(x: f64) -> Spring {
        Spring::with(x, 200.0, 0.4)
    }
    pub fn retarget(&mut self, t: f64) {
        self.target = t;
    }
    pub fn snap(&mut self, t: f64) {
        self.x = t;
        self.target = t;
        self.v = 0.0;
    }
    pub fn at_rest(&self) -> bool {
        (self.x - self.target).abs() < 1e-3 && self.v.abs() < 1e-3
    }
    fn accel(&self, x: f64, v: f64) -> f64 {
        -self.k * (x - self.target) - self.c * v
    }
    /// Advance by `dt` seconds (clamped to 0.25 so a stalled process cannot blow it up), in steps of at most 1/240 s.
    pub fn step(&mut self, dt: f64) {
        if !dt.is_finite() || dt <= 0.0 || !self.x.is_finite() || !self.target.is_finite() {
            if !self.x.is_finite() || !self.v.is_finite() {
                self.snap(if self.target.is_finite() { self.target } else { 0.0 });
            }
            return;
        }
        let mut left = dt.min(0.25);
        while left > 1e-9 {
            let h = left.min(1.0 / 240.0);
            let (x, v) = (self.x, self.v);
            let (k1x, k1v) = (v, self.accel(x, v));
            let (k2x, k2v) = (v + 0.5 * h * k1v, self.accel(x + 0.5 * h * k1x, v + 0.5 * h * k1v));
            let (k3x, k3v) = (v + 0.5 * h * k2v, self.accel(x + 0.5 * h * k2x, v + 0.5 * h * k2v));
            let (k4x, k4v) = (v + h * k3v, self.accel(x + h * k3x, v + h * k3v));
            self.x += h / 6.0 * (k1x + 2.0 * k2x + 2.0 * k3x + k4x);
            self.v += h / 6.0 * (k1v + 2.0 * k2v + 2.0 * k3v + k4v);
            left -= h;
        }
        if self.at_rest() {
            self.x = self.target;
            self.v = 0.0;
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ease {
    Linear,
    In,
    Out,
    InOut,
}

/// A fixed-time fade from one value to another.
#[derive(Clone, Copy, Debug)]
pub struct Tween {
    pub from: f64,
    pub to: f64,
    pub dur: f64,
    pub t: f64,
    pub ease: Ease,
}

impl Tween {
    pub fn new(from: f64, to: f64, dur: f64, ease: Ease) -> Tween {
        Tween { from, to, dur: dur.max(1e-6), t: 0.0, ease }
    }
    pub fn step(&mut self, dt: f64) {
        if dt.is_finite() && dt > 0.0 {
            self.t = (self.t + dt).min(self.dur);
        }
    }
    pub fn done(&self) -> bool {
        self.t >= self.dur
    }
    pub fn value(&self) -> f64 {
        let p = (self.t / self.dur).clamp(0.0, 1.0);
        let e = match self.ease {
            Ease::Linear => p,
            Ease::In => p * p,
            Ease::Out => 1.0 - (1.0 - p) * (1.0 - p),
            Ease::InOut => {
                if p < 0.5 {
                    2.0 * p * p
                } else {
                    1.0 - (-2.0 * p + 2.0).powi(2) / 2.0
                }
            }
        };
        self.from + (self.to - self.from) * e
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn critically_damped_matches_closed_form() {
        // x(t) = target + (x0-target)(1 + w t) e^{-w t}, w = sqrt(k)
        let mut s = Spring::with(0.0, 100.0, 1.0);
        s.retarget(1.0);
        let w = 10.0f64;
        let t = 0.3;
        for _ in 0..30 {
            s.step(0.01);
        }
        let exact = 1.0 + (0.0 - 1.0) * (1.0 + w * t) * (-w * t).exp();
        assert!((s.x - exact).abs() < 1e-4, "{} vs {}", s.x, exact);
    }

    #[test]
    fn no_overshoot_when_smooth_and_rests() {
        let mut s = Spring::smooth(0.0);
        s.retarget(1.0);
        let mut max: f64 = 0.0;
        for _ in 0..600 {
            s.step(1.0 / 60.0);
            max = max.max(s.x);
        }
        assert!(max <= 1.0 + 1e-9);
        assert!(s.at_rest());
        assert_eq!(s.x, 1.0);
    }

    #[test]
    fn retarget_keeps_velocity() {
        let mut s = Spring::smooth(0.0);
        s.retarget(1.0);
        for _ in 0..10 {
            s.step(1.0 / 60.0);
        }
        let v = s.v;
        s.retarget(0.0);
        assert_eq!(s.v, v);
    }

    #[test]
    fn stall_and_nan_are_safe() {
        let mut s = Spring::bouncy(0.0);
        s.retarget(5.0);
        s.step(1e9);
        assert!(s.x.is_finite());
        s.step(f64::NAN);
        s.x = f64::NAN;
        s.step(0.1);
        assert!(s.x.is_finite());
    }

    #[test]
    fn tween_reaches_end() {
        let mut t = Tween::new(0.0, 10.0, 0.5, Ease::InOut);
        t.step(0.25);
        assert!((t.value() - 5.0).abs() < 1e-9);
        t.step(1.0);
        assert!(t.done());
        assert_eq!(t.value(), 10.0);
    }
}
