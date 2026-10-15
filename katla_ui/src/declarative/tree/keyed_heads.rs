//! Key-to-head lookup with bounded inline storage for short child lists.

use std::collections::HashMap;

const INLINE_CAPACITY: usize = 16;

pub(super) struct KeyedHeads {
    inline: [(u64, usize); INLINE_CAPACITY],
    len: usize,
    heap: Option<HashMap<u64, usize>>,
}

impl KeyedHeads {
    /// Reserve an index for at most one distinct key per old child.
    #[inline]
    pub(super) fn new(old_child_count: usize) -> Self {
        Self {
            inline: [(0, 0); INLINE_CAPACITY],
            len: 0,
            heap: (old_child_count > INLINE_CAPACITY)
                .then(|| HashMap::with_capacity(old_child_count)),
        }
    }

    /// Replace a key's head, returning its previous head when present.
    #[inline]
    pub(super) fn insert(&mut self, key: u64, head: usize) -> Option<usize> {
        if let Some(heap) = &mut self.heap {
            return heap.insert(key, head);
        }
        if let Some((_, previous)) = self.inline[..self.len]
            .iter_mut()
            .find(|(entry, _)| *entry == key)
        {
            return Some(std::mem::replace(previous, head));
        }
        self.inline[self.len] = (key, head);
        self.len += 1;
        None
    }

    /// Remove a key and return its current head.
    #[inline]
    pub(super) fn remove(&mut self, key: &u64) -> Option<usize> {
        if let Some(heap) = &mut self.heap {
            return heap.remove(key);
        }
        let index = self.inline[..self.len]
            .iter()
            .position(|(entry, _)| entry == key)?;
        let head = self.inline[index].1;
        self.len -= 1;
        self.inline[index] = self.inline[self.len];
        Some(head)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inline_and_heap_head_indexes_preserve_map_semantics() {
        for capacity in [INLINE_CAPACITY, INLINE_CAPACITY + 1] {
            let mut heads = KeyedHeads::new(capacity);
            let mut reference = HashMap::new();
            for key in 0..capacity as u64 {
                assert_eq!(
                    heads.insert(key, key as usize),
                    reference.insert(key, key as usize)
                );
            }
            for key in (0..capacity as u64).step_by(2) {
                assert_eq!(heads.insert(key, 100), reference.insert(key, 100));
                assert_eq!(heads.remove(&key), reference.remove(&key));
                assert_eq!(heads.remove(&key), None);
                assert_eq!(heads.insert(key, 200), reference.insert(key, 200));
            }
            assert_eq!(heads.remove(&u64::MAX), None);
            for key in (0..capacity as u64).rev() {
                assert_eq!(heads.remove(&key), reference.remove(&key));
            }
        }
    }
}
