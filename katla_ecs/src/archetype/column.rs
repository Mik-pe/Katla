use std::any::TypeId;
use std::ptr::NonNull;

struct ColumnVTable {
    drop_fn: unsafe fn(NonNull<u8>, usize),
    clone_fn: unsafe fn(NonNull<u8>, NonNull<u8>),
    size: usize,
    align: usize,
}

pub struct ComponentColumn {
    data: Vec<u8>,
    len: usize,
    type_id: TypeId,
    vtable: ColumnVTable,
}

impl ComponentColumn {
    pub fn new<T: Clone + 'static>() -> Self {
        let size = std::mem::size_of::<T>();
        let align = std::mem::align_of::<T>();
        Self {
            data: Vec::new(),
            len: 0,
            type_id: TypeId::of::<T>(),
            vtable: ColumnVTable {
                drop_fn: drop_in_place::<T>,
                clone_fn: clone_value::<T>,
                size,
                align,
            },
        }
    }

    pub fn push<T: 'static>(&mut self, value: T) {
        assert_eq!(
            TypeId::of::<T>(),
            self.type_id,
            "ComponentColumn type mismatch"
        );
        let size = self.vtable.size;
        let align = self.vtable.align;

        let target_offset = self.len * size;
        let needed = target_offset + size;

        if self.data.is_empty() {
            self.data = aligned_vec(align, needed);
        } else if self.data.capacity() < needed {
            let new_cap = needed.next_power_of_two().max(needed);
            let old_data = std::mem::take(&mut self.data);
            let mut new_data = aligned_vec(align, new_cap);
            new_data.extend_from_slice(&old_data);
            self.data = new_data;
        }

        // SAFETY: We've ensured `data` has sufficient capacity and proper alignment.
        // `target_offset` points to the next slot, and `size` bytes are available.
        unsafe {
            let ptr = self.data.as_mut_ptr().add(target_offset);
            std::ptr::write(ptr.cast::<T>(), value);
            self.data.set_len(needed);
        }
        self.len += 1;
    }

    pub fn remove_swap(&mut self, index: usize) {
        assert!(index < self.len, "remove_swap index out of bounds");
        let size = self.vtable.size;

        // SAFETY: element_ptr points to a valid, aligned T within our buffer
        unsafe {
            let base = NonNull::new_unchecked(self.data.as_mut_ptr());
            let removed_ptr = NonNull::new_unchecked(base.as_ptr().add(index * size));
            (self.vtable.drop_fn)(removed_ptr, size);

            if index != self.len - 1 {
                let last_ptr = NonNull::new_unchecked(base.as_ptr().add((self.len - 1) * size));
                (self.vtable.clone_fn)(last_ptr, removed_ptr);
                (self.vtable.drop_fn)(last_ptr, size);
            }
        }

        self.len -= 1;
        self.data.truncate(self.len * size);
    }

    pub fn get<T: 'static>(&self, index: usize) -> Option<&T> {
        if TypeId::of::<T>() != self.type_id || index >= self.len {
            return None;
        }
        // SAFETY: index is in bounds, data is properly aligned, type matches
        unsafe {
            let ptr = self.data.as_ptr().add(index * self.vtable.size).cast::<T>();
            Some(&*ptr)
        }
    }

    pub fn get_mut<T: 'static>(&mut self, index: usize) -> Option<&mut T> {
        if TypeId::of::<T>() != self.type_id || index >= self.len {
            return None;
        }
        // SAFETY: index is in bounds, data is properly aligned, type matches
        unsafe {
            let ptr = self
                .data
                .as_mut_ptr()
                .add(index * self.vtable.size)
                .cast::<T>();
            Some(&mut *ptr)
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn type_id(&self) -> TypeId {
        self.type_id
    }
}

impl Drop for ComponentColumn {
    fn drop(&mut self) {
        if self.vtable.size == 0 || self.len == 0 {
            return;
        }
        // SAFETY: All elements are properly initialized within the buffer
        unsafe {
            let base = NonNull::new_unchecked(self.data.as_mut_ptr());
            for i in 0..self.len {
                let ptr = NonNull::new_unchecked(base.as_ptr().add(i * self.vtable.size));
                (self.vtable.drop_fn)(ptr, self.vtable.size);
            }
        }
    }
}

// SAFETY: The caller must ensure `ptr` refers to a valid, aligned, initialized T.
unsafe fn drop_in_place<T>(ptr: NonNull<u8>, _size: usize) {
    // SAFETY: ptr refers to a valid, aligned, initialized T per caller contract
    unsafe {
        ptr.as_ptr().cast::<T>().drop_in_place();
    }
}

// SAFETY: `src` must point to a valid, aligned T. `dst` must point to sufficient
// properly aligned memory for a T.
unsafe fn clone_value<T: Clone>(src: NonNull<u8>, dst: NonNull<u8>) {
    // SAFETY: src points to a valid, aligned T per caller contract
    let src_ref = unsafe { &*src.as_ptr().cast::<T>() };
    let dst_ptr = dst.as_ptr().cast::<T>();
    // SAFETY: dst points to valid, aligned memory for a T per caller contract
    unsafe {
        std::ptr::write(dst_ptr, src_ref.clone());
    }
}

/// Creates a `Vec<u8>` with its allocation aligned to `align`.
/// The returned vector has capacity for at least `capacity` bytes.
fn aligned_vec(align: usize, capacity: usize) -> Vec<u8> {
    if align <= std::mem::align_of::<u8>() {
        let mut v = Vec::with_capacity(capacity);
        // SAFETY: length is already 0, no initialized elements
        unsafe {
            v.set_len(0);
        }
        return v;
    }

    let layout =
        std::alloc::Layout::from_size_align(capacity.max(1), align).expect("invalid layout");
    // SAFETY: layout size > 0, layout is valid
    let ptr = unsafe { std::alloc::alloc(layout) };
    if ptr.is_null() {
        std::alloc::handle_alloc_error(layout);
    }

    // SAFETY: ptr is non-null, aligned, and valid for `capacity` bytes.
    // Length is 0 because no elements are initialized yet.
    unsafe { Vec::from_raw_parts(ptr, 0, capacity) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_column_push_and_get() {
        let mut col = ComponentColumn::new::<i32>();
        col.push(10i32);
        col.push(20i32);
        col.push(30i32);

        assert_eq!(col.len(), 3);
        assert_eq!(*col.get::<i32>(0).unwrap(), 10);
        assert_eq!(*col.get::<i32>(1).unwrap(), 20);
        assert_eq!(*col.get::<i32>(2).unwrap(), 30);
        assert!(col.get::<i32>(3).is_none());
    }

    #[test]
    fn test_column_remove_swap() {
        let mut col = ComponentColumn::new::<i32>();
        col.push(10i32);
        col.push(20i32);
        col.push(30i32);

        col.remove_swap(1);

        assert_eq!(col.len(), 2);
        assert_eq!(*col.get::<i32>(0).unwrap(), 10);
        assert_eq!(*col.get::<i32>(1).unwrap(), 30);
    }

    #[test]
    fn test_column_different_types() {
        let mut col = ComponentColumn::new::<f32>();
        col.push(1.5f32);
        col.push(2.5f32);

        assert_eq!(col.len(), 2);
        assert_eq!(*col.get::<f32>(0).unwrap(), 1.5);
        assert_eq!(*col.get::<f32>(1).unwrap(), 2.5);

        assert!(col.get::<i32>(0).is_none());
    }

    #[test]
    fn test_column_get_mut() {
        let mut col = ComponentColumn::new::<i32>();
        col.push(10i32);
        col.push(20i32);

        *col.get_mut::<i32>(0).unwrap() = 99;
        assert_eq!(*col.get::<i32>(0).unwrap(), 99);
    }

    #[test]
    fn test_column_is_empty() {
        let col = ComponentColumn::new::<i32>();
        assert!(col.is_empty());
        assert_eq!(col.len(), 0);
    }

    #[test]
    fn test_column_type_id() {
        let col = ComponentColumn::new::<i32>();
        assert_eq!(col.type_id(), TypeId::of::<i32>());
    }

    #[test]
    fn test_column_drop_with_non_trivial_type() {
        #[derive(Clone)]
        struct DropTracker {
            inner: std::rc::Rc<()>,
        }
        let tracker = std::rc::Rc::new(());
        {
            let mut col = ComponentColumn::new::<DropTracker>();
            col.push(DropTracker {
                inner: tracker.clone(),
            });
            col.push(DropTracker {
                inner: tracker.clone(),
            });
            assert_eq!(std::rc::Rc::strong_count(&tracker), 3);
        }
        assert_eq!(std::rc::Rc::strong_count(&tracker), 1);
    }
}
