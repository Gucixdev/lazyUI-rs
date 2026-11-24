//! The classic animations as pure maths: easing curves, slide / fade / grow transitions, enter-and-exit lifecycles,
//! and ways to join animations (in parallel, one after another, staggered over a list). No clock inside: the caller
//! says how much time passed, like `anim`. Everything maps progress 0..1 to a value and is safe for any input.

use std::f64::consts::PI;

/// A shape of progress: `at(p)` maps 0..1 to 0..1 (the back, elastic and bounce curves leave 0..1 on the way).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Curve {
    Linear,
    InSine,
    OutSine,
    InOutSine,
    InQuad,
    OutQuad,
    InOutQuad,
    InCubic,
    OutCubic,
    InOutCubic,
    InQuart,
    OutQuart,
    InOutQuart,
    InQuint,
    OutQuint,
    InOutQuint,
    InExpo,
    OutExpo,
    InOutExpo,
    InCirc,
    OutCirc,
    InOutCirc,
    InBack,
    OutBack,
    InOutBack,
    InElastic,
    OutElastic,
    InOutElastic,
    InBounce,
    OutBounce,
    InOutBounce,
}

use Curve::*;

/// Every curve, in the order of the enum: for tests, previews and `Curve::parse` listings.
pub const ALL: [Curve; 31] = [
    Linear, InSine, OutSine, InOutSine, InQuad, OutQuad, InOutQuad, InCubic, OutCubic, InOutCubic, InQuart, OutQuart,
    InOutQuart, InQuint, OutQuint, InOutQuint, InExpo, OutExpo, InOutExpo, InCirc, OutCirc, InOutCirc, InBack, OutBack,
    InOutBack, InElastic, OutElastic, InOutElastic, InBounce, OutBounce, InOutBounce,
];

fn out_bounce(p: f64) -> f64 {
    let (n, d) = (7.5625, 2.75);
    if p < 1.0 / d {
        n * p * p
    } else if p < 2.0 / d {
        let p = p - 1.5 / d;
        n * p * p + 0.75
    } else if p < 2.5 / d {
        let p = p - 2.25 / d;
        n * p * p + 0.9375
    } else {
        let p = p - 2.625 / d;
        n * p * p + 0.984375
    }
}

/// The "in-out" curve made from an "in" half: it is run twice as fast on the first half and mirrored on the second.
fn mirrored(f: fn(f64) -> f64, p: f64) -> f64 {
    if p < 0.5 {
        f(2.0 * p) / 2.0
    } else {
        1.0 - f(2.0 - 2.0 * p) / 2.0
    }
}

impl Curve {
    /// Progress through the curve; `p` outside 0..1 is clamped, NaN counts as 0.
    pub fn at(self, p: f64) -> f64 {
        let p = if p.is_nan() { 0.0 } else { p.clamp(0.0, 1.0) };
        if p == 0.0 || p == 1.0 {
            return p;
        }
        let c1 = 1.70158;
        let c3 = c1 + 1.0;
        let c2 = c1 * 1.525;
        let c4 = 2.0 * PI / 3.0;
        let c5 = 2.0 * PI / 4.5;
        match self {
            Linear => p,
            InSine => 1.0 - (p * PI / 2.0).cos(),
            OutSine => (p * PI / 2.0).sin(),
            InOutSine => -((PI * p).cos() - 1.0) / 2.0,
            InQuad => p * p,
            OutQuad => 1.0 - (1.0 - p) * (1.0 - p),
            InOutQuad => mirrored(|x| x * x, p),
            InCubic => p * p * p,
            OutCubic => 1.0 - (1.0 - p).powi(3),
            InOutCubic => mirrored(|x| x * x * x, p),
            InQuart => p.powi(4),
            OutQuart => 1.0 - (1.0 - p).powi(4),
            InOutQuart => mirrored(|x| x.powi(4), p),
            InQuint => p.powi(5),
            OutQuint => 1.0 - (1.0 - p).powi(5),
            InOutQuint => mirrored(|x| x.powi(5), p),
            InExpo => 2f64.powf(10.0 * p - 10.0),
            OutExpo => 1.0 - 2f64.powf(-10.0 * p),
            InOutExpo => mirrored(|x| 2f64.powf(10.0 * x - 10.0), p),
            InCirc => 1.0 - (1.0 - p * p).sqrt(),
            OutCirc => (1.0 - (p - 1.0) * (p - 1.0)).sqrt(),
            InOutCirc => mirrored(|x| 1.0 - (1.0 - x * x).sqrt(), p),
            InBack => c3 * p * p * p - c1 * p * p,
            OutBack => 1.0 + c3 * (p - 1.0).powi(3) + c1 * (p - 1.0).powi(2),
            InOutBack => {
                if p < 0.5 {
                    ((2.0 * p).powi(2) * ((c2 + 1.0) * 2.0 * p - c2)) / 2.0
                } else {
                    ((2.0 * p - 2.0).powi(2) * ((c2 + 1.0) * (2.0 * p - 2.0) + c2) + 2.0) / 2.0
                }
            }
            InElastic => -(2f64.powf(10.0 * p - 10.0)) * ((p * 10.0 - 10.75) * c4).sin(),
            OutElastic => 2f64.powf(-10.0 * p) * ((p * 10.0 - 0.75) * c4).sin() + 1.0,
            InOutElastic => {
                if p < 0.5 {
                    -(2f64.powf(20.0 * p - 10.0) * ((20.0 * p - 11.125) * c5).sin()) / 2.0
                } else {
                    2f64.powf(-20.0 * p + 10.0) * ((20.0 * p - 11.125) * c5).sin() / 2.0 + 1.0
                }
            }
            InBounce => 1.0 - out_bounce(1.0 - p),
            OutBounce => out_bounce(p),
            InOutBounce => {
                if p < 0.5 {
                    (1.0 - out_bounce(1.0 - 2.0 * p)) / 2.0
                } else {
                    (1.0 + out_bounce(2.0 * p - 1.0)) / 2.0
                }
            }
        }
    }

    /// Does the curve stay inside 0..1 (so it never overshoots)? Back, elastic stay out of it; bounce does not.
    pub fn overshoots(self) -> bool {
        matches!(self, InBack | OutBack | InOutBack | InElastic | OutElastic | InOutElastic)
    }

    /// `ease-out-cubic`, `out-cubic`, `outcubic`, `linear`, `ease` (= out-cubic), `ease-in`, `ease-out`, `ease-in-out`
    /// (the CSS names map to the quad curves).
    pub fn parse(s: &str) -> Option<Curve> {
        let k: String = s.trim().to_ascii_lowercase().chars().filter(|c| c.is_ascii_alphanumeric()).collect();
        let k = k.strip_prefix("ease").unwrap_or(&k);
        Some(match k {
            "" => OutCubic,
            "linear" => Linear,
            "in" => InQuad,
            "out" => OutQuad,
            "inout" => InOutQuad,
            "insine" => InSine,
            "outsine" => OutSine,
            "inoutsine" => InOutSine,
            "inquad" => InQuad,
            "outquad" => OutQuad,
            "inoutquad" => InOutQuad,
            "incubic" => InCubic,
            "outcubic" => OutCubic,
            "inoutcubic" => InOutCubic,
            "inquart" => InQuart,
            "outquart" => OutQuart,
            "inoutquart" => InOutQuart,
            "inquint" => InQuint,
            "outquint" => OutQuint,
            "inoutquint" => InOutQuint,
            "inexpo" => InExpo,
            "outexpo" => OutExpo,
            "inoutexpo" => InOutExpo,
            "incirc" => InCirc,
            "outcirc" => OutCirc,
            "inoutcirc" => InOutCirc,
            "inback" => InBack,
            "outback" | "back" => OutBack,
            "inoutback" => InOutBack,
            "inelastic" => InElastic,
            "outelastic" | "elastic" => OutElastic,
            "inoutelastic" => InOutElastic,
            "inbounce" => InBounce,
            "outbounce" | "bounce" => OutBounce,
            "inoutbounce" => InOutBounce,
            _ => return None,
        })
    }
}

/// Where an element comes from or goes to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    Left,
    Right,
    Top,
    Bottom,
}

/// What a transition does to an element, as three numbers the drawing code applies: `alpha` 0..1 (blend toward the
/// background), `dx` and `dy` in the element's own width/height fractions (0 = in place, -1 = one width to the left),
/// and `scale` 0..1+ (grow from the middle).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    pub alpha: f64,
    pub dx: f64,
    pub dy: f64,
    pub scale: f64,
}

pub const SHOWN: Look = Look { alpha: 1.0, dx: 0.0, dy: 0.0, scale: 1.0 };

/// The classic transitions. `look(p)`: `p` = 0 is fully away, 1 is fully shown, so an exit is the same one run backwards.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Transition {
    Fade,
    Slide(Edge),
    /// slide and fade together
    SlideFade(Edge),
    Grow,
    /// grow and fade
    Pop,
}

impl Transition {
    pub fn look(self, p: f64) -> Look {
        let p = if p.is_nan() { 0.0 } else { p };
        let a = p.clamp(0.0, 1.0);
        let away = 1.0 - p; // may go below 0 with an overshooting curve: the element then sails past its place
        let shift = |e: Edge| match e {
            Edge::Left => (-away, 0.0),
            Edge::Right => (away, 0.0),
            Edge::Top => (0.0, -away),
            Edge::Bottom => (0.0, away),
        };
        match self {
            Transition::Fade => Look { alpha: a, ..SHOWN },
            Transition::Slide(e) => {
                let (dx, dy) = shift(e);
                Look { dx, dy, ..SHOWN }
            }
            Transition::SlideFade(e) => {
                let (dx, dy) = shift(e);
                Look { alpha: a, dx, dy, scale: 1.0 }
            }
            Transition::Grow => Look { scale: p.max(0.0), ..SHOWN },
            Transition::Pop => Look { alpha: a, scale: p.max(0.0), ..SHOWN },
        }
    }

    /// `fade`, `slide-left`, `slide-fade-top`, `grow`, `pop`; `slide` alone is `slide-left`.
    pub fn parse(s: &str) -> Option<Transition> {
        let k = s.trim().to_ascii_lowercase();
        let edge = |e: &str| match e {
            "" | "left" => Some(Edge::Left),
            "right" => Some(Edge::Right),
            "top" | "up" => Some(Edge::Top),
            "bottom" | "down" => Some(Edge::Bottom),
            _ => None,
        };
        match k.as_str() {
            "fade" => Some(Transition::Fade),
            "grow" => Some(Transition::Grow),
            "pop" => Some(Transition::Pop),
            _ => {
                if let Some(rest) = k.strip_prefix("slide-fade") {
                    return edge(rest.trim_start_matches('-')).map(Transition::SlideFade);
                }
                k.strip_prefix("slide").and_then(|rest| edge(rest.trim_start_matches('-'))).map(Transition::Slide)
            }
        }
    }
}

/// The life of something that comes and goes (a toast, a menu, a list row): enters, stays, leaves, is gone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Entering,
    Shown,
    Leaving,
    Gone,
}

/// Enter and exit with their own durations and curves. `show()` / `hide()` can come at any time: a hide in the middle of
/// an enter turns around from where it is, with no jump.
#[derive(Clone, Copy, Debug)]
pub struct Presence {
    /// 0 = away, 1 = shown
    pub p: f64,
    pub want: bool,
    pub enter: (f64, Curve),
    pub exit: (f64, Curve),
    /// the linear position that the curve is applied to
    t: f64,
    /// a turn-around leaves a gap between the two curves: it is carried here and fades out by the end of the travel
    off: f64,
    t_flip: f64,
}

impl Presence {
    pub fn new(shown: bool, enter: (f64, Curve), exit: (f64, Curve)) -> Presence {
        let t = if shown { 1.0 } else { 0.0 };
        Presence { p: t, want: shown, enter, exit, t, off: 0.0, t_flip: t }
    }
    /// 0.25 s in with out-cubic, 0.18 s out with in-cubic.
    pub fn standard(shown: bool) -> Presence {
        Presence::new(shown, (0.25, OutCubic), (0.18, InCubic))
    }
    fn turn(&mut self, want: bool) {
        if self.want == want {
            return;
        }
        self.want = want;
        let c = if want { self.enter.1 } else { self.exit.1 };
        self.off = self.p - c.at(self.t);
        self.t_flip = self.t;
    }
    pub fn show(&mut self) {
        self.turn(true);
    }
    pub fn hide(&mut self) {
        self.turn(false);
    }
    pub fn phase(&self) -> Phase {
        match (self.want, self.t) {
            (true, t) if t >= 1.0 => Phase::Shown,
            (true, _) => Phase::Entering,
            (false, t) if t <= 0.0 => Phase::Gone,
            (false, _) => Phase::Leaving,
        }
    }
    pub fn step(&mut self, dt: f64) {
        if !dt.is_finite() || dt <= 0.0 {
            return;
        }
        if self.want {
            self.t = (self.t + dt / self.enter.0.max(1e-6)).min(1.0);
            let w = if self.t_flip >= 1.0 { 0.0 } else { (1.0 - self.t) / (1.0 - self.t_flip) };
            self.p = self.enter.1.at(self.t) + self.off * w.clamp(0.0, 1.0);
        } else {
            self.t = (self.t - dt / self.exit.0.max(1e-6)).max(0.0);
            let w = if self.t_flip <= 0.0 { 0.0 } else { self.t / self.t_flip };
            self.p = self.exit.1.at(self.t) + self.off * w.clamp(0.0, 1.0);
        }
    }
    pub fn animating(&self) -> bool {
        matches!(self.phase(), Phase::Entering | Phase::Leaving)
    }
}

/// Joining animations: every one is a function from time to progress (0..1) of one item.
#[derive(Clone, Debug)]
pub enum Timeline {
    /// one animation: `dur` seconds through `curve`
    One { dur: f64, curve: Curve },
    /// all at once: ends when the longest ends, each at its own speed
    Par(Vec<Timeline>),
    /// one after another
    Seq(Vec<Timeline>),
    /// `n` items, item i starts `delay` seconds after item i-1, each is `inner`
    Stagger { n: usize, delay: f64, inner: Box<Timeline> },
}

impl Timeline {
    pub fn one(dur: f64, curve: Curve) -> Timeline {
        Timeline::One { dur: dur.max(1e-6), curve }
    }
    /// Length in seconds.
    pub fn duration(&self) -> f64 {
        match self {
            Timeline::One { dur, .. } => *dur,
            Timeline::Par(v) => v.iter().map(Timeline::duration).fold(0.0, f64::max),
            Timeline::Seq(v) => v.iter().map(Timeline::duration).sum(),
            Timeline::Stagger { n, delay, inner } => {
                inner.duration() + delay.max(0.0) * n.saturating_sub(1) as f64
            }
        }
    }
    /// Progress of the whole at time `t`, 0..1 (a Seq reports the mean of its parts, a Par the mean too): for
    /// `Stagger` use `item`.
    pub fn at(&self, t: f64) -> f64 {
        match self {
            Timeline::One { dur, curve } => curve.at(t / dur),
            Timeline::Par(v) if v.is_empty() => 1.0,
            Timeline::Par(v) => v.iter().map(|x| x.at(t)).sum::<f64>() / v.len() as f64,
            Timeline::Seq(v) if v.is_empty() => 1.0,
            Timeline::Seq(v) => {
                let mut left = t;
                let mut sum = 0.0;
                for x in v {
                    let d = x.duration();
                    sum += x.at(left.max(0.0));
                    left -= d;
                }
                sum / v.len() as f64
            }
            Timeline::Stagger { n, .. } if *n == 0 => 1.0,
            Timeline::Stagger { n, .. } => (0..*n).map(|i| self.item(i, t)).sum::<f64>() / *n as f64,
        }
    }
    /// Progress of item `i` of a Stagger at time `t` (for other shapes: the same as `at`).
    pub fn item(&self, i: usize, t: f64) -> f64 {
        match self {
            Timeline::Stagger { delay, inner, .. } => inner.at(t - delay.max(0.0) * i as f64),
            _ => self.at(t),
        }
    }
    pub fn done(&self, t: f64) -> bool {
        t >= self.duration()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_curve_starts_at_0_ends_at_1_and_is_finite() {
        for c in ALL {
            assert_eq!(c.at(0.0), 0.0, "{c:?}");
            assert_eq!(c.at(1.0), 1.0, "{c:?}");
            for i in 0..=1000 {
                let v = c.at(i as f64 / 1000.0);
                assert!(v.is_finite(), "{c:?} at {i}");
                if !c.overshoots() {
                    assert!((-1e-9..=1.0 + 1e-9).contains(&v), "{c:?} at {i}: {v}");
                }
            }
            assert_eq!(c.at(f64::NAN), 0.0);
            assert_eq!(c.at(-3.0), 0.0);
            assert_eq!(c.at(9.0), 1.0);
        }
    }

    #[test]
    fn the_smooth_curves_never_go_backwards() {
        for c in ALL.iter().filter(|c| !c.overshoots() && !matches!(c, InBounce | OutBounce | InOutBounce)) {
            let mut last = 0.0;
            for i in 0..=2000 {
                let v = c.at(i as f64 / 2000.0);
                assert!(v >= last - 1e-12, "{c:?} goes back at {i}");
                last = v;
            }
        }
    }

    #[test]
    fn out_is_the_mirror_of_in() {
        let pairs = [(InSine, OutSine), (InQuad, OutQuad), (InCubic, OutCubic), (InQuart, OutQuart), (InQuint, OutQuint),
            (InExpo, OutExpo), (InCirc, OutCirc), (InBack, OutBack), (InBounce, OutBounce)];
        for (i, o) in pairs {
            for k in 1..100 {
                let p = k as f64 / 100.0;
                assert!((i.at(p) - (1.0 - o.at(1.0 - p))).abs() < 1e-9, "{i:?} {o:?} {p}");
            }
        }
    }

    #[test]
    fn in_out_curves_pass_through_the_middle() {
        for c in ALL {
            if matches!(c, InOutSine | InOutQuad | InOutCubic | InOutQuart | InOutQuint | InOutCirc | InOutBounce | InOutBack) {
                assert!((c.at(0.5) - 0.5).abs() < 1e-9, "{c:?} {}", c.at(0.5));
            }
        }
    }

    #[test]
    fn names_parse() {
        assert_eq!(Curve::parse("ease-out-cubic"), Some(OutCubic));
        assert_eq!(Curve::parse("Ease In Out"), Some(InOutQuad));
        assert_eq!(Curve::parse("bounce"), Some(OutBounce));
        assert_eq!(Curve::parse("linear"), Some(Linear));
        assert_eq!(Curve::parse("wobble"), None);
        assert_eq!(Transition::parse("slide-fade-bottom"), Some(Transition::SlideFade(Edge::Bottom)));
        assert_eq!(Transition::parse("slide"), Some(Transition::Slide(Edge::Left)));
        assert_eq!(Transition::parse("pop"), Some(Transition::Pop));
        assert_eq!(Transition::parse("zoom"), None);
    }

    #[test]
    fn transitions_are_in_place_when_shown_and_away_at_zero() {
        for t in [Transition::Fade, Transition::Slide(Edge::Left), Transition::SlideFade(Edge::Top), Transition::Grow, Transition::Pop] {
            assert_eq!(t.look(1.0), SHOWN, "{t:?}");
            let z = t.look(0.0);
            assert!(z.alpha == 0.0 || z.dx != 0.0 || z.dy != 0.0 || z.scale == 0.0, "{t:?} {z:?}");
        }
        assert_eq!(Transition::Slide(Edge::Right).look(0.0).dx, 1.0);
        assert_eq!(Transition::Slide(Edge::Top).look(0.5).dy, -0.5);
    }

    #[test]
    fn presence_turns_around_without_a_jump() {
        let mut p = Presence::standard(false);
        assert_eq!(p.phase(), Phase::Gone);
        p.show();
        p.step(0.1);
        assert_eq!(p.phase(), Phase::Entering);
        let mid = p.p;
        assert!(mid > 0.0 && mid < 1.0);
        p.hide();
        p.step(0.001);
        assert!((p.p - mid).abs() < 0.1, "no jump: {} vs {}", p.p, mid);
        assert_eq!(p.phase(), Phase::Leaving);
        for _ in 0..100 {
            p.step(0.05);
        }
        assert_eq!(p.phase(), Phase::Gone);
        assert!(!p.animating());
        p.show();
        for _ in 0..100 {
            p.step(0.05);
        }
        assert_eq!((p.phase(), p.p), (Phase::Shown, 1.0));
    }

    #[test]
    fn timelines_join() {
        let a = Timeline::one(0.5, Linear);
        let b = Timeline::one(1.0, Linear);
        let par = Timeline::Par(vec![a.clone(), b.clone()]);
        assert_eq!(par.duration(), 1.0);
        assert!((par.at(0.5) - 0.75).abs() < 1e-9);
        let seq = Timeline::Seq(vec![a.clone(), b.clone()]);
        assert_eq!(seq.duration(), 1.5);
        assert!((seq.at(0.5) - 0.5).abs() < 1e-9); // first done (1), second not started (0)
        assert!(seq.done(1.5) && !seq.done(1.4));
        assert_eq!(seq.at(1.5), 1.0);
        let st = Timeline::Stagger { n: 4, delay: 0.1, inner: Box::new(Timeline::one(0.2, Linear)) };
        assert!((st.duration() - 0.5).abs() < 1e-9);
        assert_eq!(st.item(0, 0.1), 0.5);
        assert_eq!(st.item(3, 0.1), 0.0); // item 3 starts at 0.3
        assert!((st.item(3, 0.5) - 1.0).abs() < 1e-9);
        assert_eq!(st.at(10.0), 1.0);
    }
}
