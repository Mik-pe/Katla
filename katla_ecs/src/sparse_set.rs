//! Sparse set implementation for O(1) lookup, insert, remove operations
//! while maintaining contiguous storage for fast iteration.
//!
//! Uses a paged sparse array where the index space is divided into fixed-size
//! pages. Only pages that are actually used are allocated, avoiding memory
//! waste when entity indices have large gaps.

use std::collections::HashSet;
use std::marker::PhantomData;

const PAGE_SIZE: usize = 1024;

/// Trait for keys that can be used as indices into the sparse array.
///
/// Keys sharing a sparse index replace one another; lookups also verify equality.
#[doc(hidden)]
pub trait SparseKey: Copy + Eq {
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

type Page = Box<[Option<usize>; PAGE_SIZE]>;

fn new_page() -> Page {
    Box::new([None; PAGE_SIZE])
}

/// A sparse set data structure that provides O(1) operations while
/// maintaining contiguous storage for iteration.
///
/// Internally uses:
/// - `dense`: Stores (K, V) pairs contiguously for iteration
/// - `pages`: Paged sparse array mapping key index → index in dense array.
///   Only pages that contain at least one entry are allocated.
///
/// # Type Parameters
/// - `K`: Key type (must implement `SparseKey`)
/// - `V`: Value type
///
/// # Performance
/// - Insert: O(1) amortized
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

    /// Paged sparse array mapping key index → index in dense array.
    /// Each page covers `PAGE_SIZE` indices. Only allocated pages exist.
    pages: Vec<Option<Page>>,
}

/// Independently borrowed sparse indices and a frozen dense allocation.
#[doc(hidden)]
pub struct SparseView<'a, K: SparseKey, V> {
    pages: &'a [Option<Page>],
    dense: *mut (K, V),
    len: usize,
    borrow: PhantomData<&'a V>,
}

impl<'a, K: SparseKey, V> SparseView<'a, K, V> {
    #[inline]
    pub(crate) fn get_ptr(&self, key: K) -> Option<*mut V> {
        let (page, offset) = SparseSet::<K, V>::page_coords(key.sparse_index());
        let index = self.pages.get(page)?.as_ref()?[offset]?;
        if index >= self.len {
            return None;
        }
        // SAFETY: The view freezes both the sparse indices and dense allocation.
        // Keys and component values are disjoint fields; no full-row borrow is made.
        unsafe {
            let row = self.dense.add(index);
            if std::ptr::addr_of!((*row).0).read() != key {
                return None;
            }
            Some(std::ptr::addr_of_mut!((*row).1))
        }
    }

    #[inline]
    pub(crate) fn cursor(&self) -> KeyCursor<'a, K> {
        let key = if self.len == 0 {
            std::ptr::null()
        } else {
            // SAFETY: A nonempty view owns a valid first dense row.
            unsafe { std::ptr::addr_of!((*self.dense).0).cast::<u8>() }
        };
        KeyCursor {
            key,
            stride: std::mem::size_of::<(K, V)>(),
            remaining: self.len,
            borrow: PhantomData,
        }
    }
}

/// Iterates immutable key fields without borrowing neighboring component values.
#[doc(hidden)]
pub struct KeyCursor<'a, K> {
    key: *const u8,
    stride: usize,
    remaining: usize,
    borrow: PhantomData<&'a K>,
}

impl<K: SparseKey> Iterator for KeyCursor<'_, K> {
    type Item = K;
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
    #[inline]
    fn next(&mut self) -> Option<K> {
        if self.remaining == 0 {
            return None;
        }
        // SAFETY: The cursor retains the view's storage lifetime and advances by
        // the exact dense-row stride, reading only the immutable key field.
        let key = unsafe { self.key.cast::<K>().read() };
        self.remaining -= 1;
        if self.remaining != 0 {
            // SAFETY: Another dense row remains in the frozen allocation.
            self.key = unsafe { self.key.add(self.stride) };
        }
        Some(key)
    }
}

impl<K: SparseKey> ExactSizeIterator for KeyCursor<'_, K> {}

impl<K, V> SparseSet<K, V>
where
    K: SparseKey,
{
    /// Creates a new empty SparseSet.
    pub fn new() -> Self {
        Self {
            dense: Vec::new(),
            pages: Vec::new(),
        }
    }

    pub(crate) fn view(&self) -> SparseView<'_, K, V> {
        SparseView {
            pages: &self.pages,
            dense: self.dense.as_ptr() as *mut _,
            len: self.dense.len(),
            borrow: PhantomData,
        }
    }

    pub(crate) fn view_mut(&mut self) -> SparseView<'_, K, V> {
        SparseView {
            pages: &self.pages,
            dense: self.dense.as_mut_ptr(),
            len: self.dense.len(),
            borrow: PhantomData,
        }
    }

    /// Returns the page index and offset for a given sparse index.
    #[inline]
    fn page_coords(index: usize) -> (usize, usize) {
        (index / PAGE_SIZE, index % PAGE_SIZE)
    }

    /// Inserts or updates a key-value pair.
    ///
    /// If the key already exists, the value is updated.
    /// If the key doesn't exist, a new entry is created.
    #[inline]
    pub fn insert(&mut self, key: K, value: V) {
        let idx = key.sparse_index();
        let (page_idx, offset) = Self::page_coords(idx);

        // Check if the key already exists before potential page allocation
        let existing = self
            .pages
            .get(page_idx)
            .and_then(|p| p.as_ref())
            .and_then(|page| page[offset]);

        if let Some(dense_idx) = existing {
            self.dense[dense_idx] = (key, value);
        } else {
            let dense_idx = self.dense.len();
            self.dense.push((key, value));

            if page_idx >= self.pages.len() {
                self.pages.resize_with(page_idx + 1, || None);
            }
            let page = self.pages[page_idx].get_or_insert_with(new_page);
            page[offset] = Some(dense_idx);
        }
    }

    /// Removes the value for the given key.
    ///
    /// Returns true if the key existed and was removed, false otherwise.
    #[inline]
    pub fn remove(&mut self, key: K) -> bool {
        let idx = key.sparse_index();
        let (page_idx, offset) = Self::page_coords(idx);

        let page = match self.pages.get_mut(page_idx) {
            Some(Some(page)) => page,
            _ => return false,
        };

        let Some(dense_idx) = page[offset] else {
            return false;
        };
        if self.dense[dense_idx].0 != key {
            return false;
        }
        page[offset] = None;
        self.dense.swap_remove(dense_idx);

        if let Some((moved_key, _)) = self.dense.get(dense_idx) {
            let moved_idx = moved_key.sparse_index();
            let (moved_page, moved_offset) = Self::page_coords(moved_idx);
            let page = self.pages[moved_page]
                .as_mut()
                .expect("dense key has an allocated sparse page");
            page[moved_offset] = Some(dense_idx);
        }

        true
    }

    /// Gets a reference to the value for the given key.
    #[inline]
    pub fn get(&self, key: K) -> Option<&V> {
        let idx = key.sparse_index();
        let (page_idx, offset) = Self::page_coords(idx);
        self.pages
            .get(page_idx)
            .and_then(|opt| opt.as_ref())
            .and_then(|page| page[offset])
            .and_then(|dense_idx| self.dense.get(dense_idx))
            .filter(|(stored_key, _)| *stored_key == key)
            .map(|(_, value)| value)
    }

    /// Gets a mutable reference to the value for the given key.
    #[inline]
    pub fn get_mut(&mut self, key: K) -> Option<&mut V> {
        let idx = key.sparse_index();
        let (page_idx, offset) = Self::page_coords(idx);
        let dense_idx = self
            .pages
            .get(page_idx)
            .and_then(|opt| opt.as_ref())
            .and_then(|page| page[offset])?;
        self.dense
            .get_mut(dense_idx)
            .filter(|(stored_key, _)| *stored_key == key)
            .map(|(_, value)| value)
    }

    pub(crate) fn selected_indices(&self, keys: &[K]) -> Vec<usize> {
        keys.iter()
            .map(|&key| {
                let (page, offset) = Self::page_coords(key.sparse_index());
                self.pages
                    .get(page)
                    .and_then(|page| page.as_ref())
                    .and_then(|page| page[offset])
                    .filter(|&index| self.dense[index].0 == key)
                    .expect("selected query key exists")
            })
            .collect()
    }

    pub(crate) fn dense_base(&self) -> *const (K, V) {
        self.dense.as_ptr()
    }

    pub(crate) fn dense_base_mut(&mut self) -> *mut (K, V) {
        self.dense.as_mut_ptr()
    }

    /// Returns true if the key exists in the set.
    #[inline]
    pub fn contains(&self, key: K) -> bool {
        self.get(key).is_some()
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
        self.pages.clear();
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
mod tests;
