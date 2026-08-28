use std::sync::atomic::{AtomicBool, Ordering};

use crate::backend::resource::{GpuEvent, GpuFence};

pub(crate) struct MetalFence {
    signaled: AtomicBool,
}

#[cfg(test)]
impl MetalFence {
    fn new() -> Self {
        Self {
            signaled: AtomicBool::new(false),
        }
    }

    fn signal(&self) {
        self.signaled.store(true, Ordering::Release);
    }

    fn reset(&self) {
        self.signaled.store(false, Ordering::Release);
    }
}

impl GpuFence for MetalFence {
    fn is_signaled(&self) -> bool {
        self.signaled.load(Ordering::Acquire)
    }
}

pub(crate) struct MetalEvent {}

impl GpuEvent for MetalEvent {}

unsafe impl Send for MetalEvent {}
unsafe impl Sync for MetalEvent {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    #[test]
    fn test_fence_initial_state_unsignaled() {
        let fence = MetalFence::new();
        assert!(!fence.is_signaled());
    }

    #[test]
    fn test_fence_signal() {
        let fence = MetalFence::new();
        fence.signal();
        assert!(fence.is_signaled());
    }

    #[test]
    fn test_fence_reset() {
        let fence = MetalFence::new();
        fence.signal();
        assert!(fence.is_signaled());
        fence.reset();
        assert!(!fence.is_signaled());
    }

    #[test]
    fn test_fence_signal_idempotent() {
        let fence = MetalFence::new();
        fence.signal();
        fence.signal();
        assert!(fence.is_signaled());
    }

    #[test]
    fn test_fence_reset_idempotent() {
        let fence = MetalFence::new();
        fence.reset();
        assert!(!fence.is_signaled());
    }

    #[test]
    fn test_fence_signal_reset_cycle() {
        let fence = MetalFence::new();

        fence.signal();
        assert!(fence.is_signaled());

        fence.reset();
        assert!(!fence.is_signaled());

        fence.signal();
        assert!(fence.is_signaled());
    }

    #[test]
    fn test_fence_ordering_semantics() {
        let fence = MetalFence::new();
        fence.signaled.store(true, Ordering::Release);
        assert!(fence.signaled.load(Ordering::Acquire));
    }
}
