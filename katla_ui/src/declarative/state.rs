use std::any::Any;
use std::collections::HashMap;

slotmap::new_key_type! {
    pub struct ViewId;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StateId {
    node: ViewId,
    slot: u32,
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

    pub fn get<T: Clone + 'static>(&self, id: StateId) -> T {
        let cell = self
            .cells
            .get(&id)
            .expect("StateArena::get: invalid StateId");
        cell.value
            .downcast_ref::<T>()
            .cloned()
            .expect("StateArena::get: type mismatch")
    }

    pub fn set<T: PartialEq + 'static>(&mut self, id: StateId, value: T) {
        if let Some(cell) = self.cells.get_mut(&id) {
            let changed = cell
                .value
                .downcast_ref::<T>()
                .is_none_or(|old| *old != value);
            if changed {
                cell.value = Box::new(value);
                cell.dirty = true;
            }
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.cells.values().any(|c| c.dirty)
    }

    pub fn clear_dirty(&mut self) {
        for cell in self.cells.values_mut() {
            cell.dirty = false;
        }
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
