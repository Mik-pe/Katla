//! Sparse set implementation for O(1) lookup, insert, remove operations
//! while maintaining contiguous storage for fast iteration.
//!
//! Uses a `Vec<Option<usize>>` sparse array indexed by key, providing true O(1)
//! lookups with zero hashing overhead.

use std::collections::HashSet;

/// Trait for keys that can be used as indices into the sparse array.
///
/// Each key maps to a unique `usize` index used to look up its position
/// in the dense array. Implementors must ensure that `sparse_index()` returns
/// a unique value per distinct key.
pub(crate) trait SparseKey: Copy {
    fn sparse_index(&self) -> usize;
}

impl SparseKey for u32 {
    #[inline]
    fn sparse_index(&self) -> usize {
        *self as usize
    }
}

impl SparseKey for crate::entity::EntityId {
    #[inline]
    fn sparse_index(&self) -> usize {
        self.index() as usize
    }
}

impl SparseKey for usize {
    #[inline]
    fn sparse_index(&self) -> usize {
        *self
    }
}

/// A sparse set data structure that provides O(1) operations while
/// maintaining contiguous storage for iteration.
///
/// Internally uses:
/// - `dense`: Stores (K, V) pairs contiguously for iteration
/// - `sparse`: Vec indexed by key's sparse_index(), mapping to dense array index
///
/// # Type Parameters
/// - `K`: Key type (must implement `SparseKey`)
/// - `V`: Value type
///
/// # Performance
/// - Insert: O(1) amortized (sparse vec may grow)
/// - Remove: O(1)
/// - Get/Contains: O(1) with zero hashing
/// - Iterate: O(n) with excellent cache locality
///
/// # Example
/// ```rust,ignore
/// // SparseSet is internal API - this demonstrates usage
/// let mut set = SparseSet::new();
/// set.insert(0, "value1");
/// set.insert(1, "value2");
///
/// assert_eq!(set.get(0), Some(&"value1"));
/// assert!(set.contains(1));
/// set.remove(0);
/// assert!(!set.contains(0));
/// ```
pub struct SparseSet<K, V>
where
    K: SparseKey,
{
    /// Dense array storing (Key, Value) pairs contiguously
    dense: Vec<(K, V)>,

    /// Sparse vec mapping key index → index in dense array.
    /// `None` means the key is not present.
    sparse: Vec<Option<usize>>,
}

impl<K, V> SparseSet<K, V>
where
    K: SparseKey,
{
    /// Creates a new empty SparseSet.
    pub fn new() -> Self {
        Self {
            dense: Vec::new(),
            sparse: Vec::new(),
        }
    }

    /// Ensures the sparse vec is large enough to hold the given index.
    #[inline]
    fn ensure_sparse_capacity(&mut self, index: usize) {
        if index >= self.sparse.len() {
            self.sparse.resize(index + 1, None);
        }
    }

    /// Inserts or updates a key-value pair.
    ///
    /// If the key already exists, the value is updated.
    /// If the key doesn't exist, a new entry is created.
    #[inline]
    pub fn insert(&mut self, key: K, value: V) {
        let idx = key.sparse_index();
        self.ensure_sparse_capacity(idx);

        if let Some(&dense_idx) = self.sparse[idx].as_ref() {
            self.dense[dense_idx].1 = value;
        } else {
            let dense_idx = self.dense.len();
            self.dense.push((key, value));
            self.sparse[idx] = Some(dense_idx);
        }
    }

    /// Removes the value for the given key.
    ///
    /// Returns true if the key existed and was removed, false otherwise.
    #[inline]
    pub fn remove(&mut self, key: K) -> bool {
        let idx = key.sparse_index();
        if idx < self.sparse.len()
            && let Some(dense_idx) = self.sparse[idx].take()
        {
            self.dense.swap_remove(dense_idx);

            if let Some((moved_key, _)) = self.dense.get(dense_idx) {
                self.sparse[moved_key.sparse_index()] = Some(dense_idx);
            }

            return true;
        }
        false
    }

    /// Gets a reference to the value for the given key.
    #[inline]
    pub fn get(&self, key: K) -> Option<&V> {
        let idx = key.sparse_index();
        self.sparse
            .get(idx)
            .and_then(|opt| opt.as_ref())
            .and_then(|&dense_idx| self.dense.get(dense_idx))
            .map(|(_, value)| value)
    }

    /// Gets a mutable reference to the value for the given key.
    #[inline]
    pub fn get_mut(&mut self, key: K) -> Option<&mut V> {
        let idx = key.sparse_index();
        if let Some(&dense_idx) = self.sparse.get(idx).and_then(|opt| opt.as_ref()) {
            self.dense.get_mut(dense_idx).map(|(_, value)| value)
        } else {
            None
        }
    }

    /// Returns true if the key exists in the set.
    #[inline]
    pub fn contains(&self, key: K) -> bool {
        let idx = key.sparse_index();
        self.sparse
            .get(idx)
            .map(|opt| opt.is_some())
            .unwrap_or(false)
    }

    /// Returns an iterator over all (Key, &Value) pairs.
    pub fn iter(&self) -> impl Iterator<Item = (K, &V)> {
        self.dense.iter().map(|(key, value)| (*key, value))
    }

    /// Returns a mutable iterator over all (Key, &mut Value) pairs.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (K, &mut V)> {
        self.dense.iter_mut().map(|(key, value)| (*key, value))
    }

    /// Returns an iterator over just the values.
    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.dense.iter().map(|(_, value)| value)
    }

    /// Returns a mutable iterator over just the values.
    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut V> {
        self.dense.iter_mut().map(|(_, value)| value)
    }

    /// Returns a reference to the internal dense storage.
    pub fn dense(&self) -> &Vec<(K, V)> {
        &self.dense
    }

    /// Returns a mutable reference to the internal dense storage.
    pub fn dense_mut(&mut self) -> &mut Vec<(K, V)> {
        &mut self.dense
    }

    /// Returns an iterator over just the keys.
    pub fn keys(&self) -> impl Iterator<Item = K> + '_ {
        self.dense.iter().map(|(key, _)| *key)
    }

    /// Returns the number of entries in the set.
    pub fn len(&self) -> usize {
        self.dense.len()
    }

    /// Returns true if the set is empty.
    pub fn is_empty(&self) -> bool {
        self.dense.is_empty()
    }

    /// Clears all entries from the set.
    pub fn clear(&mut self) {
        self.dense.clear();
        self.sparse.clear();
    }

    /// Retains only the entries whose keys are in the provided set.
    pub fn retain_keys(&mut self, valid_keys: &HashSet<K>)
    where
        K: std::hash::Hash + Eq,
    {
        let mut i = 0;
        while i < self.dense.len() {
            let (key, _) = self.dense[i];
            if !valid_keys.contains(&key) {
                self.remove(key);
            } else {
                i += 1;
            }
        }
    }
}

impl<K, V> Default for SparseSet<K, V>
where
    K: SparseKey,
{
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sparse_set_insert() {
        let mut set: SparseSet<usize, i32> = SparseSet::new();
        set.insert(0, 10);
        set.insert(1, 20);
        set.insert(2, 30);

        assert_eq!(set.len(), 3);
        assert_eq!(set.get(0), Some(&10));
        assert_eq!(set.get(1), Some(&20));
        assert_eq!(set.get(2), Some(&30));
    }

    #[test]
    fn test_sparse_set_insert_update() {
        let mut set: SparseSet<usize, i32> = SparseSet::new();
        set.insert(0, 10);
        assert_eq!(set.get(0), Some(&10));

        set.insert(0, 20);
        assert_eq!(set.len(), 1);
        assert_eq!(set.get(0), Some(&20));
    }

    #[test]
    fn test_sparse_set_remove() {
        let mut set: SparseSet<usize, i32> = SparseSet::new();
        set.insert(0, 10);
        set.insert(1, 20);
        set.insert(2, 30);

        assert!(set.remove(1));
        assert_eq!(set.len(), 2);
        assert_eq!(set.get(0), Some(&10));
        assert_eq!(set.get(1), None);
        assert_eq!(set.get(2), Some(&30));

        assert!(!set.remove(1)); // Already removed
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_sparse_set_get_mut() {
        let mut set: SparseSet<usize, i32> = SparseSet::new();
        set.insert(0, 10);

        if let Some(value) = set.get_mut(0) {
            *value = 20;
        }

        assert_eq!(set.get(0), Some(&20));
    }

    #[test]
    fn test_sparse_set_iter() {
        let mut set: SparseSet<usize, i32> = SparseSet::new();
        set.insert(0, 10);
        set.insert(1, 20);
        set.insert(2, 30);

        let items: Vec<(usize, &i32)> = set.iter().collect();
        assert_eq!(items.len(), 3);
        assert!(items.contains(&(0, &10)));
        assert!(items.contains(&(1, &20)));
        assert!(items.contains(&(2, &30)));
    }

    #[test]
    fn test_sparse_set_iter_mut() {
        let mut set: SparseSet<usize, i32> = SparseSet::new();
        set.insert(0, 10);
        set.insert(1, 20);

        for (_, value) in set.iter_mut() {
            *value *= 2;
        }

        assert_eq!(set.get(0), Some(&20));
        assert_eq!(set.get(1), Some(&40));
    }

    #[test]
    fn test_sparse_set_retain_keys() {
        let mut set: SparseSet<usize, i32> = SparseSet::new();
        set.insert(0, 10);
        set.insert(1, 20);
        set.insert(2, 30);
        set.insert(3, 40);

        let mut valid = HashSet::new();
        valid.insert(1);
        valid.insert(3);

        set.retain_keys(&valid);

        assert_eq!(set.len(), 2);
        assert_eq!(set.get(0), None);
        assert_eq!(set.get(1), Some(&20));
        assert_eq!(set.get(2), None);
        assert_eq!(set.get(3), Some(&40));
    }

    #[test]
    fn test_sparse_set_large_key_ids() {
        let mut set: SparseSet<usize, i32> = SparseSet::new();
        set.insert(1000, 100);
        set.insert(5000, 500);
        set.insert(10000, 1000);

        assert_eq!(set.len(), 3);
        assert_eq!(set.get(1000), Some(&100));
        assert_eq!(set.get(5000), Some(&500));
        assert_eq!(set.get(10000), Some(&1000));
    }

    #[test]
    fn test_sparse_set_remove_middle() {
        let mut set: SparseSet<usize, i32> = SparseSet::new();
        set.insert(0, 10);
        set.insert(1, 20);
        set.insert(2, 30);
        set.insert(3, 40);
        set.insert(4, 50);

        set.remove(2);

        assert_eq!(set.len(), 4);
        assert_eq!(set.get(2), None);
        assert_eq!(set.get(0), Some(&10));
        assert_eq!(set.get(1), Some(&20));
        assert_eq!(set.get(3), Some(&40));
        assert_eq!(set.get(4), Some(&50));
    }

    #[test]
    fn test_sparse_set_iteration_order_after_removal() {
        let mut set: SparseSet<usize, i32> = SparseSet::new();
        set.insert(0, 10);
        set.insert(1, 20);
        set.insert(2, 30);

        set.remove(1);

        let items: Vec<(usize, i32)> = set.iter().map(|(k, v)| (k, *v)).collect();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0], (0, 10));
        assert_eq!(items[1], (2, 30));
    }

    #[test]
    fn test_sparse_set_dense_sparse_consistency_after_remove() {
        let mut set: SparseSet<u32, i32> = SparseSet::new();

        for i in 0..5u32 {
            set.insert(i, (i * 10) as i32);
        }

        set.remove(1);
        set.remove(3);

        assert_eq!(set.len(), 3);

        for (key, value) in set.iter() {
            assert!(set.contains(key));
            assert_eq!(*set.get(key).unwrap(), *value);
        }

        assert!(!set.contains(1));
        assert!(!set.contains(3));
        assert_eq!(set.get(1), None);
        assert_eq!(set.get(3), None);

        assert_eq!(set.get(0), Some(&0));
        assert_eq!(set.get(2), Some(&20));
        assert_eq!(set.get(4), Some(&40));
    }

    #[test]
    fn test_sparse_set_remove_all_then_reinsert() {
        let mut set: SparseSet<u32, i32> = SparseSet::new();

        for i in 0..5u32 {
            set.insert(i, i as i32);
        }

        for i in 0..5u32 {
            assert!(set.remove(i));
        }

        assert!(set.is_empty());

        for i in 0..5u32 {
            set.insert(i, (i * 100) as i32);
        }

        assert_eq!(set.len(), 5);
        for i in 0..5u32 {
            assert_eq!(set.get(i), Some(&((i * 100) as i32)));
        }
    }

    #[test]
    fn test_sparse_set_sparse_vec_grows_with_large_indices() {
        let mut set: SparseSet<usize, i32> = SparseSet::new();

        // Insert a key with a large index — sparse vec should grow
        set.insert(50000, 42);
        assert_eq!(set.get(50000), Some(&42));
        assert_eq!(set.len(), 1);

        // Sparse vec should have at least 50001 entries
        assert!(set.sparse.len() > 50000);

        // Small index should also work after growing
        set.insert(0, 1);
        assert_eq!(set.get(0), Some(&1));
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_sparse_set_contains_after_remove() {
        let mut set: SparseSet<usize, i32> = SparseSet::new();
        set.insert(42, 100);
        assert!(set.contains(42));

        set.remove(42);
        assert!(!set.contains(42));
    }

    #[test]
    fn test_sparse_set_clear_resets_state() {
        let mut set: SparseSet<usize, i32> = SparseSet::new();
        set.insert(0, 10);
        set.insert(100, 20);

        set.clear();

        assert!(set.is_empty());
        assert_eq!(set.len(), 0);
        assert_eq!(set.get(0), None);
        assert_eq!(set.get(100), None);
        assert!(set.sparse.is_empty());
    }

    #[test]
    fn test_sparse_set_values_and_keys() {
        let mut set: SparseSet<usize, i32> = SparseSet::new();
        set.insert(3, 30);
        set.insert(1, 10);
        set.insert(2, 20);

        let mut values: Vec<&i32> = set.values().collect();
        values.sort();
        assert_eq!(values, vec![&10, &20, &30]);

        let keys: std::collections::HashSet<usize> = set.keys().collect();
        assert!(keys.contains(&1));
        assert!(keys.contains(&2));
        assert!(keys.contains(&3));
    }
}
