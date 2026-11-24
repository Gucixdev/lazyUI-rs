//! The attributes of one element, packed: every key and value in one `String`, the end of each in a `Vec<u32>`.
//!
//! An element has two to six attributes, so a hash table per element costs a table allocation plus two strings per
//! attribute to build and again to clone, and a SipHash per lookup. Here a clone is two allocations whatever the count
//! (none for an element without attributes), and a lookup is a scan over a few dozen contiguous bytes. Insertion order is
//! kept; code that needs a stable order sorts (`tree` does).
//!
//! Layout: `buf = k0 v0 k1 v1 ...`, `ends = [end(k0), end(v0), end(k1), end(v1), ...]`.

#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct Attrs {
    buf: String,
    ends: Vec<u32>,
}

fn at(n: usize) -> u32 {
    u32::try_from(n).expect("an element's attributes are over 4 GB")
}

impl Attrs {
    pub fn len(&self) -> usize {
        self.ends.len() / 2
    }

    pub fn is_empty(&self) -> bool {
        self.ends.is_empty()
    }

    /// Where pair `i` sits: (key start, key end, value end).
    fn span(&self, i: usize) -> (usize, usize, usize) {
        let start = if i == 0 { 0 } else { self.ends[2 * i - 1] as usize };
        (start, self.ends[2 * i] as usize, self.ends[2 * i + 1] as usize)
    }

    /// (pair index, key end, value end) of `key`. Bytes, not `str` slices: no char-boundary checks in the scan, and a key of
    /// another length is passed over without comparing.
    fn find(&self, key: &str) -> Option<(usize, usize, usize)> {
        let (b, kb) = (self.buf.as_bytes(), key.as_bytes());
        let mut start = 0usize;
        for (i, pair) in self.ends.chunks_exact(2).enumerate() {
            let (k, v) = (pair[0] as usize, pair[1] as usize);
            if k - start == kb.len() && &b[start..k] == kb {
                return Some((i, k, v));
            }
            start = v;
        }
        None
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        let (_, k, v) = self.find(key)?;
        Some(&self.buf[k..v])
    }

    pub fn contains(&self, key: &str) -> bool {
        self.find(key).is_some()
    }

    /// Set `key`, replacing its value when it is there (its place in the order stays).
    pub fn insert(&mut self, key: &str, value: &str) {
        match self.find(key) {
            Some((i, k, v)) => {
                if &self.buf[k..v] == value {
                    return;
                }
                self.buf.replace_range(k..v, value);
                let new_end = k + value.len();
                for e in &mut self.ends[2 * i + 1..] {
                    *e = at(*e as usize - v + new_end);
                }
            }
            None => {
                if self.buf.capacity() == 0 {
                    self.buf.reserve(32);
                }
                self.buf.push_str(key);
                self.ends.push(at(self.buf.len()));
                self.buf.push_str(value);
                self.ends.push(at(self.buf.len()));
            }
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> + '_ {
        (0..self.len()).map(move |i| {
            let (s, k, v) = self.span(i);
            (&self.buf[s..k], &self.buf[k..v])
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_has_no_allocation_and_finds_nothing() {
        let a = Attrs::default();
        assert!(a.is_empty() && a.get("id").is_none() && a.iter().next().is_none());
        assert_eq!(a.buf.capacity() + a.ends.capacity(), 0);
    }

    #[test]
    fn insert_get_keeps_order_and_replaces_in_place() {
        let mut a = Attrs::default();
        a.insert("id", "r1");
        a.insert("text", "item one");
        a.insert("size", "8");
        assert_eq!(a.get("text"), Some("item one"));
        a.insert("text", "a much longer replacement");
        a.insert("id", "x");
        assert_eq!(a.get("id"), Some("x"));
        assert_eq!(a.get("size"), Some("8"), "later pairs shift with the replaced one");
        assert_eq!(a.len(), 3);
        let order: Vec<_> = a.iter().map(|(k, _)| k).collect();
        assert_eq!(order, ["id", "text", "size"]);
        a.insert("text", "t");
        assert_eq!(a.iter().collect::<Vec<_>>(), [("id", "x"), ("text", "t"), ("size", "8")]);
    }

    #[test]
    fn empty_values_unicode_and_prefix_keys_do_not_confuse_the_scan() {
        let mut a = Attrs::default();
        a.insert("a", "");
        a.insert("ab", "zażółć gęślą");
        a.insert("b", "");
        assert_eq!((a.get("a"), a.get("ab"), a.get("b"), a.get("abc")), (Some(""), Some("zażółć gęślą"), Some(""), None));
        a.insert("a", "now");
        assert_eq!((a.get("a"), a.get("ab")), (Some("now"), Some("zażółć gęślą")));
    }

    #[test]
    fn a_clone_is_independent() {
        let mut a = Attrs::default();
        a.insert("id", "one");
        let mut b = a.clone();
        b.insert("id", "two");
        assert_eq!((a.get("id"), b.get("id")), (Some("one"), Some("two")));
    }

    #[test]
    fn matches_a_hash_map_on_random_operations() {
        use std::collections::HashMap;
        let (mut a, mut m) = (Attrs::default(), HashMap::new());
        let mut x: u64 = 0x9e3779b97f4a7c15;
        for _ in 0..2000 {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            let k = format!("k{}", x % 7);
            let v = "v".repeat((x >> 8) as usize % 5) + &(x % 3).to_string();
            a.insert(&k, &v);
            m.insert(k, v);
            assert_eq!(a.len(), m.len());
            for (k, v) in &m {
                assert_eq!(a.get(k), Some(v.as_str()));
            }
        }
    }
}
