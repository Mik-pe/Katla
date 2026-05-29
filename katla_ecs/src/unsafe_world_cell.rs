use crate::world::World;

/// Thin wrapper around `*mut World` for scoped unsafe access to World data.
///
/// This is similar to Bevy's `UnsafeWorldCell` — it provides methods for
/// reading and writing component storage through a raw pointer. The caller
/// is responsible for ensuring no aliasing violations (e.g., only one mutable
/// reference per component type at a time).
///
/// This is the building block for parallel system execution: the scheduler
/// can hand out `UnsafeWorldCell` references to systems that access disjoint
/// component types.
#[derive(Copy, Clone)]
pub(crate) struct UnsafeWorldCell(*mut World);

impl UnsafeWorldCell {
    /// Create from a raw World pointer.
    ///
    /// # Safety
    /// Caller must ensure `world` is valid, properly aligned, and no other
    /// `&mut World` reference exists for the duration of the returned cell's use.
    #[inline]
    pub unsafe fn new(world: *mut World) -> Self {
        Self(world)
    }

    /// Get the raw `*mut World` pointer.
    #[inline]
    pub fn as_ptr(&self) -> *mut World {
        self.0
    }
}

// SAFETY: UnsafeWorldCell is explicitly designed for concurrent read access
// from multiple threads (each accessing different component types). The caller
// is responsible for ensuring no two threads mutate the same data simultaneously.
unsafe impl Send for UnsafeWorldCell {}
unsafe impl Sync for UnsafeWorldCell {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::Component;

    #[derive(Component, Default, PartialEq, Debug)]
    struct Position {
        x: f32,
        y: f32,
    }

    #[derive(Component, Default, PartialEq, Debug)]
    struct Velocity {
        dx: f32,
        dy: f32,
    }

    #[test]
    fn test_create_from_world() {
        let mut world = World::new();
        let cell = unsafe { world.as_unsafe_world_cell() };
        let _ = cell;
    }
}
