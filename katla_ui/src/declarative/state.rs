use std::any::Any;
use std::collections::{HashMap, HashSet};

slotmap::new_key_type! {
    pub struct ViewId;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StateId {
    node: ViewId,
    slot: u32,
}

impl StateId {
    #[cfg(test)]
    pub fn test_id() -> Self {
        Self {
            node: ViewId::from(slotmap::KeyData::from_ffi(0)),
            slot: 0,
        }
    }
}

#[derive(Default)]
pub struct StateArena {
    cells: HashMap<StateId, StateCell>,
    slot_counters: HashMap<ViewId, u32>,
}

struct StateCell {
    value: Box<dyn Any>,
    dirty: bool,
}

impl StateArena {
    pub fn new() -> Self {
        Self::default()
    }

    /// Reset all slot counters. Call at the start of each build frame so that
    /// `get_or_create` reuses the same slots across frames.
    pub fn reset_slots(&mut self) {
        for slot in self.slot_counters.values_mut() {
            *slot = 0;
        }
    }

    pub fn get_or_create<T: Clone + PartialEq + 'static>(
        &mut self,
        node_id: ViewId,
        initial: T,
    ) -> StateId {
        let slot = self.slot_counters.entry(node_id).or_insert(0);
        let id = StateId {
            node: node_id,
            slot: *slot,
        };
        *slot += 1;

        self.cells.entry(id).or_insert_with(|| StateCell {
            value: Box::new(initial),
            dirty: false,
        });

        id
    }

    pub fn get<T: Clone + 'static>(&self, id: StateId) -> Option<T> {
        let cell = self.cells.get(&id)?;
        cell.value.downcast_ref::<T>().cloned()
    }

    pub fn set<T: PartialEq + 'static>(&mut self, id: StateId, value: T) -> bool {
        let Some(cell) = self.cells.get_mut(&id) else {
            return false;
        };
        let changed = cell
            .value
            .downcast_ref::<T>()
            .is_none_or(|old| *old != value);
        if changed {
            cell.value = Box::new(value);
            cell.dirty = true;
        }
        changed
    }

    pub fn is_dirty(&self) -> bool {
        self.cells.values().any(|c| c.dirty)
    }

    pub fn clear_dirty(&mut self) {
        for cell in self.cells.values_mut() {
            cell.dirty = false;
        }
    }

    /// Garbage collect orphaned state entries.
    ///
    /// Removes all `StateCell` entries whose `StateId.node` references a
    /// `ViewId` not present in `live_view_ids`. Also removes stale slot
    /// counters.
    pub fn gc(&mut self, live_view_ids: &HashSet<ViewId>) {
        self.cells.retain(|id, _| live_view_ids.contains(&id.node));
        self.slot_counters
            .retain(|view_id, _| live_view_ids.contains(view_id));
    }

    /// Returns the number of state cells currently stored.
    pub fn cell_count(&self) -> usize {
        self.cells.len()
    }
}

pub struct Binding<T> {
    get: Box<dyn Fn() -> T>,
    set: Box<dyn Fn(T)>,
}

impl<T: Clone> Binding<T> {
    pub fn new(get: impl Fn() -> T + 'static, set: impl Fn(T) + 'static) -> Self {
        Self {
            get: Box::new(get),
            set: Box::new(set),
        }
    }

    pub fn get(&self) -> T {
        (self.get)()
    }

    pub fn set(&self, value: T) {
        (self.set)(value)
    }
}

impl<T> Binding<T> {
    pub fn from_ref(value: &mut T) -> BindingRef<'_, T> {
        BindingRef { value }
    }
}

pub struct BindingRef<'a, T> {
    value: &'a mut T,
}

impl<'a, T: Clone> BindingRef<'a, T> {
    pub fn get(&self) -> T {
        self.value.clone()
    }

    pub fn set(&mut self, val: T) {
        *self.value = val;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_view_id(ffi: u64) -> ViewId {
        ViewId::from(slotmap::KeyData::from_ffi(ffi))
    }

    #[test]
    fn test_gc_removes_orphaned_state() {
        let mut arena = StateArena::new();
        let id1 = make_view_id(1);
        let id2 = make_view_id(2);
        let id3 = make_view_id(3);

        let sid1 = arena.get_or_create(id1, 10_i32);
        let sid2 = arena.get_or_create(id2, 20_i32);
        let sid3 = arena.get_or_create(id3, 30_i32);

        assert_eq!(arena.cell_count(), 3);

        let live: HashSet<ViewId> = [id1, id3].into_iter().collect();
        arena.gc(&live);

        assert_eq!(arena.cell_count(), 2);
        assert!(arena.get::<i32>(sid1).is_some());
        assert!(arena.get::<i32>(sid2).is_none());
        assert!(arena.get::<i32>(sid3).is_some());
    }

    #[test]
    fn test_gc_removes_stale_slot_counters() {
        let mut arena = StateArena::new();
        let id1 = make_view_id(1);
        let id2 = make_view_id(2);

        arena.get_or_create(id1, 0_i32);
        arena.get_or_create(id2, 0_i32);

        assert_eq!(arena.slot_counters.len(), 2);

        let live: HashSet<ViewId> = [id1].into_iter().collect();
        arena.gc(&live);

        assert_eq!(arena.slot_counters.len(), 1);
        assert!(arena.slot_counters.contains_key(&id1));
        assert!(!arena.slot_counters.contains_key(&id2));
    }

    #[test]
    fn test_gc_preserves_state_for_live_nodes() {
        let mut arena = StateArena::new();
        let id = make_view_id(1);
        let sid = arena.get_or_create(id, 42_i32);
        arena.set(sid, 100_i32);

        let live: HashSet<ViewId> = [id].into_iter().collect();
        arena.gc(&live);

        assert_eq!(arena.get::<i32>(sid).unwrap(), 100);
    }

    #[test]
    fn test_gc_with_empty_live_set_removes_all() {
        let mut arena = StateArena::new();
        arena.get_or_create(make_view_id(1), 1_i32);
        arena.get_or_create(make_view_id(2), 2_i32);

        let live: HashSet<ViewId> = HashSet::new();
        arena.gc(&live);

        assert_eq!(arena.cell_count(), 0);
        assert!(arena.slot_counters.is_empty());
    }

    #[test]
    fn test_gc_no_leak_over_1000_iterations() {
        let mut arena = StateArena::new();

        for i in 0..1000 {
            arena.reset_slots();

            let live_id = make_view_id(0);
            let dead_id_a = make_view_id(i * 2 + 1);
            let dead_id_b = make_view_id(i * 2 + 2);

            arena.get_or_create(live_id, 0_i32);
            arena.get_or_create(dead_id_a, i);
            arena.get_or_create(dead_id_b, i);

            let live: HashSet<ViewId> = [live_id].into_iter().collect();
            arena.gc(&live);
        }

        assert!(
            arena.cell_count() <= 2,
            "arena should stay bounded, got {} cells",
            arena.cell_count()
        );
    }
}
