//! Where every element of the tree went: the result of a layout, stored by column.
//!
//! A frame asks three things of it: which rectangles contain the pointer (hit tests scan `rect` and nothing else), which
//! element has this id, and what is the parent of this one. So `rect`, `kind` and `parent` are plain arrays, and the two
//! variable-length things, the id and the path from the root, live in one arena each instead of one `String` and one `Vec`
//! per element. A layout of 6000 elements used to make 22 000 allocations; into a warm `Plan` it makes none.
//! `get(i)` and `iter()` give a `Placed`, a small `Copy` view with the fields the old row struct had.

use crate::canvas::Rect;
use crate::ui::Kind;
use std::fmt::Write;

/// One element's row, read out of the columns. `kind`, `rect` and `parent` are copied (three loads); the id and the path are
/// sliced out of their arenas only when asked, so a scan that tests `rect` first never touches them.
#[derive(Clone, Copy, Debug)]
pub struct Placed<'a> {
    plan: &'a Plan,
    pub i: usize,
    pub kind: Kind,
    pub rect: Rect,
    pub parent: Option<usize>,
}

impl<'a> Placed<'a> {
    pub fn id(&self) -> &'a str {
        self.plan.id(self.i)
    }
    /// child indices from the root to this element
    pub fn path(&self) -> &'a [u32] {
        self.plan.path(self.i)
    }
}

#[derive(Default, Debug)]
pub struct Plan {
    rect: Vec<Rect>,
    kind: Vec<Kind>,
    /// `NONE` for the root
    parent: Vec<u32>,
    id_end: Vec<u32>,
    ids: String,
    path_end: Vec<u32>,
    paths: Vec<u32>,
}

const NONE: u32 = u32::MAX;

fn at(n: usize) -> u32 {
    u32::try_from(n).expect("more than 4 billion layout entries")
}

impl Plan {
    pub fn len(&self) -> usize {
        self.rect.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rect.is_empty()
    }

    /// Forget the contents, keep the buffers.
    pub fn clear(&mut self) {
        self.rect.clear();
        self.kind.clear();
        self.parent.clear();
        self.id_end.clear();
        self.ids.clear();
        self.path_end.clear();
        self.paths.clear();
    }

    /// Append an element. Its id is `id` when that is there and not empty, else `kind@path` (`row@0.2`).
    pub fn push(&mut self, kind: Kind, rect: Rect, parent: Option<usize>, path: &[u32], id: Option<&str>) {
        self.rect.push(rect);
        self.kind.push(kind);
        self.parent.push(parent.map_or(NONE, at));
        match id {
            Some(i) if !i.is_empty() => self.ids.push_str(i),
            _ => {
                let _ = write!(self.ids, "{}@", kind.name());
                for (n, p) in path.iter().enumerate() {
                    if n > 0 {
                        self.ids.push('.');
                    }
                    let _ = write!(self.ids, "{p}");
                }
            }
        }
        self.id_end.push(at(self.ids.len()));
        self.paths.extend_from_slice(path);
        self.path_end.push(at(self.paths.len()));
    }

    pub fn id(&self, i: usize) -> &str {
        let s = if i == 0 { 0 } else { self.id_end[i - 1] as usize };
        &self.ids[s..self.id_end[i] as usize]
    }

    pub fn path(&self, i: usize) -> &[u32] {
        let s = if i == 0 { 0 } else { self.path_end[i - 1] as usize };
        &self.paths[s..self.path_end[i] as usize]
    }

    pub fn rect(&self, i: usize) -> Rect {
        self.rect[i]
    }

    pub fn kind(&self, i: usize) -> Kind {
        self.kind[i]
    }

    pub fn parent(&self, i: usize) -> Option<usize> {
        let p = self.parent[i];
        (p != NONE).then_some(p as usize)
    }

    pub fn get(&self, i: usize) -> Placed<'_> {
        Placed { plan: self, i, kind: self.kind[i], rect: self.rect[i], parent: self.parent(i) }
    }

    pub fn iter(&self) -> impl DoubleEndedIterator<Item = Placed<'_>> + ExactSizeIterator + '_ {
        (0..self.len()).map(move |i| self.get(i))
    }

    /// Index of the element with this id (the first, if a program reused one). The lengths are compared first, as numbers
    /// in one column; only an id of the same length is compared byte by byte.
    pub fn index_of(&self, id: &str) -> Option<usize> {
        let (b, want) = (self.ids.as_bytes(), id.as_bytes());
        let mut start = 0usize;
        for (i, &end) in self.id_end.iter().enumerate() {
            let end = end as usize;
            if end - start == want.len() && &b[start..end] == want {
                return Some(i);
            }
            start = end;
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_paths_and_parents_round_trip() {
        let mut p = Plan::default();
        p.push(Kind::Col, Rect::new(0, 0, 10, 4), None, &[], None);
        p.push(Kind::Text, Rect::new(0, 0, 10, 1), Some(0), &[0], Some("title"));
        p.push(Kind::Row, Rect::new(0, 1, 10, 3), Some(0), &[1], Some(""));
        p.push(Kind::Text, Rect::new(0, 1, 5, 3), Some(2), &[1, 0], None);
        assert_eq!((p.id(0), p.id(1), p.id(2), p.id(3)), ("col@", "title", "row@1", "text@1.0"));
        assert_eq!((p.path(0), p.path(1), p.path(3)), (&[][..], &[0][..], &[1, 0][..]));
        assert_eq!((p.parent(0), p.parent(3)), (None, Some(2)));
        assert_eq!(p.index_of("text@1.0"), Some(3));
        assert_eq!(p.index_of("nope"), None);
        let ids: Vec<_> = p.iter().rev().map(|q| q.id()).collect();
        assert_eq!(ids, ["text@1.0", "row@1", "title", "col@"]);
        assert_eq!(p.get(3).rect, Rect::new(0, 1, 5, 3));
    }

    #[test]
    fn clear_keeps_the_buffers_and_starts_over() {
        let mut p = Plan::default();
        for i in 0..50 {
            p.push(Kind::Text, Rect::new(i, 0, 1, 1), Some(0), &[i as u32], None);
        }
        let cap = p.ids.capacity();
        p.clear();
        assert!(p.is_empty());
        p.push(Kind::Text, Rect::new(0, 0, 1, 1), None, &[7], Some("a"));
        assert_eq!((p.len(), p.id(0), p.path(0)), (1, "a", &[7u32][..]));
        assert_eq!(p.ids.capacity(), cap);
    }
}
