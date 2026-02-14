//! Sparse set implementation for O(1) lookup, insert, remove operations
//! while maintaining contiguous storage for fast iteration.
//!
//! # TODO: Benchmarks
//!
//! Implement comprehensive benchmarks to compare performance:
//! - O(1) lookups vs O(n) linear search
//! - Insert/remove operations
//! - Iteration performance
//! - Memory overhead comparison
//! - Large dataset performance (10k+ entities)
//! - Sparse key handling
//!
//! Note: Benchmarks require either nightly Rust (for `#[bench]` attribute)
//! or the `criterion` library for more advanced benchmarking.

use std::collections::{HashMap, HashSet};

/// A sparse set data structure that provides O(1) operations while
/// maintaining contiguous storage for iteration.
///
/// Internally uses:
/// - `dense`: Stores (K, V) pairs contiguously for iteration
/// - `sparse`: Maps keys to indices in the dense vector via HashMap
///
/// # Type Parameters
/// - `K`: Key type (must be Hash + Eq)
/// - `V`: Value type
///
/// # Performance
/// - Insert: O(1) amortized
/// - Remove: O(1)
/// - Get/Contains: O(1)
/// - Iterate: O(n) with excellent cache locality
///
/// # Example
/// ```rust
/// use katla_ecs::sparse_set::SparseSet;
///
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
    K: std::hash::Hash + Eq + Copy + Clone,
{
    /// Dense array storing (Key, Value) pairs contiguously
    dense: Vec<(K, V)>,

    /// Sparse HashMap mapping Key → index in dense array
    sparse: HashMap<K, usize>,
}

impl<K, V> SparseSet<K, V>
where
    K: std::hash::Hash + Eq + Copy + Clone,
{
    /// Creates a new empty SparseSet.
    pub fn new() -> Self {
        Self {
            dense: Vec::new(),
            sparse: HashMap::new(),
        }
    }

    /// Creates a new SparseSet with the specified capacity.
    pub fn with_capacity(dense_capacity: usize, sparse_capacity: usize) -> Self {
        Self {
            dense: Vec::with_capacity(dense_capacity),
            sparse: HashMap::with_capacity(sparse_capacity),
        }
    }

    /// Inserts or updates a key-value pair.
    ///
    /// If the key already exists, the value is updated.
    /// If the key doesn't exist, a new entry is created.
    pub fn insert(&mut self, key: K, value: V) {
        if let Some(&dense_idx) = self.sparse.get(&key) {
            // Key exists, update value
            self.dense[dense_idx].1 = value;
        } else {
            // New key
            let dense_idx = self.dense.len();
            self.dense.push((key, value));
            self.sparse.insert(key, dense_idx);
        }
    }

    /// Removes the value for the given key.
    ///
    /// Returns true if the key existed and was removed, false otherwise.
    pub fn remove(&mut self, key: K) -> bool {
        if let Some(&dense_idx) = self.sparse.get(&key) {
            // Remove from dense using swap_remove for O(1)
            self.dense.swap_remove(dense_idx);

            // Update sparse mapping for the element that was swapped
            if let Some((moved_key, _)) = self.dense.get(dense_idx) {
                self.sparse.insert(*moved_key, dense_idx);
            }

            // Remove the sparse entry for the removed key
            self.sparse.remove(&key);

            true
        } else {
            false
        }
    }

    /// Gets a reference to the value for the given key.
    pub fn get(&self, key: K) -> Option<&V> {
        self.sparse
            .get(&key)
            .and_then(|&dense_idx| self.dense.get(dense_idx))
            .map(|(_, value)| value)
    }

    /// Gets a mutable reference to the value for the given key.
    pub fn get_mut(&mut self, key: K) -> Option<&mut V> {
        if let Some(&dense_idx) = self.sparse.get(&key) {
            self.dense.get_mut(dense_idx).map(|(_, value)| value)
        } else {
            None
        }
    }

    /// Returns true if the key exists in the set.
    pub fn contains(&self, key: K) -> bool {
        self.sparse.contains_key(&key)
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
    pub fn retain_keys(&mut self, valid_keys: &HashSet<K>) {
        let mut i = 0;
        while i < self.dense.len() {
            let (key, _) = self.dense[i];
            if !valid_keys.contains(&key) {
                self.remove(key);
                // Don't increment i, as a new element is now at position i
            } else {
                i += 1;
            }
        }
    }

    /// Returns the capacity of the dense array.
    pub fn dense_capacity(&self) -> usize {
        self.dense.capacity()
    }

    /// Returns the capacity of the sparse HashMap.
    pub fn sparse_capacity(&self) -> usize {
        self.sparse.capacity()
    }
}

impl<K, V> Default for SparseSet<K, V>
where
    K: std::hash::Hash + Eq + Copy + Clone,
{
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sparse_set_new() {
        let set: SparseSet<usize, i32> = SparseSet::new();
        assert!(set.is_empty());
        assert_eq!(set.len(), 0);
    }

    #[test]
    fn test_sparse_set_insert() {
        let mut set = SparseSet::new();
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
        let mut set = SparseSet::new();
        set.insert(0, 10);
        assert_eq!(set.get(0), Some(&10));

        set.insert(0, 20);
        assert_eq!(set.len(), 1);
        assert_eq!(set.get(0), Some(&20));
    }

    #[test]
    fn test_sparse_set_remove() {
        let mut set = SparseSet::new();
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
    fn test_sparse_set_contains() {
        let mut set = SparseSet::new();
        set.insert(0, 10);
        set.insert(2, 30);

        assert!(set.contains(0));
        assert!(!set.contains(1));
        assert!(set.contains(2));
    }

    #[test]
    fn test_sparse_set_get() {
        let mut set = SparseSet::new();
        set.insert(0, 10);
        set.insert(1, 20);

        assert_eq!(set.get(0), Some(&10));
        assert_eq!(set.get(1), Some(&20));
        assert_eq!(set.get(2), None);
    }

    #[test]
    fn test_sparse_set_get_mut() {
        let mut set = SparseSet::new();
        set.insert(0, 10);

        if let Some(value) = set.get_mut(0) {
            *value = 20;
        }

        assert_eq!(set.get(0), Some(&20));
    }

    #[test]
    fn test_sparse_set_iter() {
        let mut set = SparseSet::new();
        set.insert(0, 10);
        set.insert(1, 20);
        set.insert(2, 30);

        let items: Vec<(usize, &i32)> = set.iter().collect();
        assert_eq!(items.len(), 3);
        // Note: order should be insertion order due to push-based insertion
        assert!(items.contains(&(0, &10)));
        assert!(items.contains(&(1, &20)));
        assert!(items.contains(&(2, &30)));
    }

    #[test]
    fn test_sparse_set_iter_mut() {
        let mut set = SparseSet::new();
        set.insert(0, 10);
        set.insert(1, 20);

        for (_, value) in set.iter_mut() {
            *value *= 2;
        }

        assert_eq!(set.get(0), Some(&20));
        assert_eq!(set.get(1), Some(&40));
    }

    #[test]
    fn test_sparse_values() {
        let mut set = SparseSet::new();
        set.insert(0, 10);
        set.insert(1, 20);
        set.insert(2, 30);

        let values: Vec<&i32> = set.values().collect();
        assert_eq!(values.len(), 3);
        assert!(values.contains(&&10));
        assert!(values.contains(&&20));
        assert!(values.contains(&&30));
    }

    #[test]
    fn test_sparse_keys() {
        let mut set = SparseSet::new();
        set.insert(0, 10);
        set.insert(1, 20);
        set.insert(2, 30);

        let keys: Vec<usize> = set.keys().collect();
        assert_eq!(keys.len(), 3);
        assert!(keys.contains(&0));
        assert!(keys.contains(&1));
        assert!(keys.contains(&2));
    }

    #[test]
    fn test_sparse_set_clear() {
        let mut set = SparseSet::new();
        set.insert(0, 10);
        set.insert(1, 20);

        set.clear();

        assert!(set.is_empty());
        assert_eq!(set.len(), 0);
        assert_eq!(set.get(0), None);
    }

    #[test]
    fn test_sparse_set_retain_keys() {
        let mut set = SparseSet::new();
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
        let mut set = SparseSet::new();
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
        let mut set = SparseSet::new();
        set.insert(0, 10);
        set.insert(1, 20);
        set.insert(2, 30);
        set.insert(3, 40);
        set.insert(4, 50);

        set.remove(2);

        assert_eq!(set.len(), 4);
        assert_eq!(set.get(2), None);
        // Verify other elements are still accessible
        assert_eq!(set.get(0), Some(&10));
        assert_eq!(set.get(1), Some(&20));
        assert_eq!(set.get(3), Some(&40));
        assert_eq!(set.get(4), Some(&50));
    }

    #[test]
    fn test_sparse_set_with_capacity() {
        let set: SparseSet<usize, i32> = SparseSet::with_capacity(10, 10);
        assert!(set.is_empty());
        assert!(set.dense_capacity() >= 10);
        assert!(set.sparse_capacity() >= 10);
    }

    #[test]
    fn test_sparse_set_iteration_order_after_removal() {
        let mut set = SparseSet::new();
        set.insert(0, 10);
        set.insert(1, 20);
        set.insert(2, 30);

        set.remove(1);

        let items: Vec<(usize, i32)> = set.iter().map(|(k, v)| (k, *v)).collect();
        assert_eq!(items.len(), 2);
        // Last element was moved to position 1
        assert_eq!(items[0], (0, 10));
        assert_eq!(items[1], (2, 30));
    }

    #[test]
    fn test_sparse_set_capacity_methods() {
        let mut set = SparseSet::new();
        set.insert(0, 10);
        set.insert(1, 20);

        assert!(set.dense_capacity() >= 2);
        assert!(set.sparse_capacity() >= 2);
    }

    #[test]
    fn test_sparse_set_empty_operations() {
        let mut set: SparseSet<usize, i32> = SparseSet::new();

        assert!(!set.remove(0));
        assert_eq!(set.get(0), None);
        assert!(!set.contains(0));
        assert_eq!(set.values().count(), 0);
        assert_eq!(set.keys().count(), 0);
        assert_eq!(set.iter().count(), 0);
    }
}
