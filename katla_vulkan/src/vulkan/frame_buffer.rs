//! Frame buffer abstraction for double/triple-buffered resources.
//!
//! Provides a generic container for per-frame resources that need to be
//! duplicated to avoid synchronization issues when frames overlap.

use std::cell::{Ref, RefCell, RefMut};
use std::ops::Index;

/// A container for double/triple-buffered resources.
///
/// Uses `crate::renderer::FRAMES_IN_FLIGHT` for size - no configuration needed.
/// This ensures consistent frame buffering across the entire engine.
///
/// # Example
///
/// ```ignore
/// let buffers = FrameBuffer::new(|frame_idx| {
///     VertexBuffer::new(context.clone(), capacity, frame_idx as u32)
/// });
///
/// // Get current frame's buffer (frame_index from PassExecutionContext)
/// let mut buffer = buffers.current_mut(ctx.frame_index());
/// buffer.upload_data(&data);
/// ```
pub struct FrameBuffer<T> {
    buffers: Vec<RefCell<T>>,
}

impl<T> FrameBuffer<T> {
    /// Create a new FrameBuffer with one instance per frame-in-flight.
    ///
    /// The factory function receives the frame index (0, 1, ...) if needed
    /// for resource initialization.
    pub fn new<F>(factory: F) -> Self
    where
        F: Fn(usize) -> T,
    {
        let buffers = (0..crate::renderer::FRAMES_IN_FLIGHT)
            .map(|i| RefCell::new(factory(i)))
            .collect();
        Self { buffers }
    }

    /// Get the current frame's data (immutable).
    ///
    /// # Arguments
    /// * `frame_index` - The current frame index (from PassExecutionContext or SwapData)
    ///
    /// # Panics
    /// Panics if the borrow is already held (should not happen in single-threaded use).
    pub fn current(&self, frame_index: usize) -> Ref<'_, T> {
        self.buffers[frame_index % self.buffers.len()].borrow()
    }

    /// Get the current frame's data (mutable).
    ///
    /// # Arguments
    /// * `frame_index` - The current frame index (from PassExecutionContext or SwapData)
    ///
    /// # Panics
    /// Panics if the borrow is already held (should not happen in single-threaded use).
    pub fn current_mut(&self, frame_index: usize) -> RefMut<'_, T> {
        self.buffers[frame_index % self.buffers.len()].borrow_mut()
    }

    /// Number of buffers (equals FRAMES_IN_FLIGHT).
    pub fn len(&self) -> usize {
        self.buffers.len()
    }

    /// Check if the buffer is empty (always false for FRAMES_IN_FLIGHT >= 1).
    pub fn is_empty(&self) -> bool {
        self.buffers.is_empty()
    }
}

impl<T> Index<usize> for FrameBuffer<T> {
    type Output = RefCell<T>;

    fn index(&self, index: usize) -> &Self::Output {
        &self.buffers[index % self.buffers.len()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_buffer_creation() {
        let buffer: FrameBuffer<i32> = FrameBuffer::new(|i| i as i32 * 10);
        assert_eq!(buffer.len(), crate::renderer::FRAMES_IN_FLIGHT);
    }

    #[test]
    fn test_frame_buffer_access() {
        let buffer: FrameBuffer<String> = FrameBuffer::new(|i| format!("frame_{}", i));

        // Frame 0
        assert_eq!(*buffer.current(0), "frame_0");
        *buffer.current_mut(0) = "modified".to_string();
        assert_eq!(*buffer.current(0), "modified");

        // Frame 1
        assert_eq!(*buffer.current(1), "frame_1");

        // Wrap around (frame 2 -> index 0 with FRAMES_IN_FLIGHT = 2)
        assert_eq!(*buffer.current(2), "modified");
    }

    #[test]
    fn test_frame_buffer_index() {
        let buffer: FrameBuffer<i32> = FrameBuffer::new(|i| i as i32 * 10);

        // Direct index access
        assert_eq!(*buffer[0].borrow(), 0);
        assert_eq!(*buffer[1].borrow(), 10);

        // Wrap around
        assert_eq!(*buffer[2].borrow(), 0); // 2 % 2 = 0
        assert_eq!(*buffer[3].borrow(), 10); // 3 % 2 = 1
    }
}
