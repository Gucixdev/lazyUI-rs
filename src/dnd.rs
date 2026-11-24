//! Drag and drop as a small state machine, apart from the Ui so it can be tested on its own.
//!
//! In a spec: `drag="PAYLOAD"` on an element makes it a source (`dragkind=KIND`, default `any`); `drop="kind,kind"`
//! (or `*`) on an element makes it a target for those kinds. The Ui turns the mouse into these calls and tells the program:
//! `dragstart` (on the source), `drop` (on the target, text = payload), `dragend` (on the source, number 1 = dropped
//! on a target, 0 = cancelled or dropped nowhere).

/// Cells (or pixels, the unit of the mouse) the pointer must move after the press before it counts as a drag, so a
/// plain click is never taken for one.
pub const THRESHOLD: i32 = 2;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Drag {
    pub from: String,
    pub payload: String,
    pub kind: String,
    pub start: (i32, i32),
    pub pos: (i32, i32),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    /// nothing happens (yet)
    None,
    /// the press became a drag
    Started,
    /// the pointer moved during a drag
    Moved,
}

#[derive(Default, Debug)]
pub struct Dnd {
    pending: Option<Drag>,
    active: Option<Drag>,
}

impl Dnd {
    /// The button went down on a source.
    pub fn press(&mut self, from: &str, payload: &str, kind: &str, x: i32, y: i32) {
        self.active = None;
        self.pending = Some(Drag { from: from.into(), payload: payload.into(), kind: kind.into(), start: (x, y), pos: (x, y) });
    }
    /// The pointer moved with the button held.
    pub fn motion(&mut self, x: i32, y: i32) -> Step {
        if let Some(d) = self.active.as_mut() {
            d.pos = (x, y);
            return Step::Moved;
        }
        let Some(mut d) = self.pending.take() else { return Step::None };
        d.pos = (x, y);
        if (x - d.start.0).abs().max((y - d.start.1).abs()) >= THRESHOLD {
            self.active = Some(d);
            Step::Started
        } else {
            self.pending = Some(d);
            Step::None
        }
    }
    /// The button went up: the finished drag, if there was one (a press that never moved is just a click).
    pub fn release(&mut self) -> Option<Drag> {
        self.pending = None;
        self.active.take()
    }
    /// Esc or a lost window: the drag that was cancelled, if any.
    pub fn cancel(&mut self) -> Option<Drag> {
        self.pending = None;
        self.active.take()
    }
    pub fn active(&self) -> Option<&Drag> {
        self.active.as_ref()
    }
}

/// Does a target accepting `accept` (`*`, or kinds separated by commas) take something of `kind`?
pub fn accepts(accept: &str, kind: &str) -> bool {
    accept.split(',').map(str::trim).any(|a| a == "*" || a == kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_click_is_not_a_drag() {
        let mut d = Dnd::default();
        d.press("a", "x", "any", 5, 5);
        assert_eq!(d.motion(6, 5), Step::None);
        assert!(d.release().is_none());
    }

    #[test]
    fn moving_far_enough_starts_then_follows_then_drops() {
        let mut d = Dnd::default();
        d.press("a", "file1", "file", 5, 5);
        assert_eq!(d.motion(5, 7), Step::Started);
        assert_eq!(d.motion(9, 9), Step::Moved);
        assert_eq!(d.active().unwrap().pos, (9, 9));
        let done = d.release().unwrap();
        assert_eq!((done.from.as_str(), done.payload.as_str(), done.kind.as_str()), ("a", "file1", "file"));
        assert!(d.active().is_none());
        assert!(d.release().is_none());
    }

    #[test]
    fn cancel_ends_it_and_a_new_press_starts_over() {
        let mut d = Dnd::default();
        d.press("a", "p", "any", 0, 0);
        d.motion(10, 0);
        assert!(d.cancel().is_some());
        assert_eq!(d.motion(20, 0), Step::None);
        d.press("b", "q", "any", 0, 0);
        assert_eq!(d.motion(0, 5), Step::Started);
        assert_eq!(d.active().unwrap().from, "b");
    }

    #[test]
    fn accept_lists() {
        assert!(accepts("*", "file"));
        assert!(accepts("file, text", "text"));
        assert!(!accepts("file", "text"));
        assert!(!accepts("", "file"));
    }
}
