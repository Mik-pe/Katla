#[cfg(test)]
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(test)]
use objc2::rc::Retained;
#[cfg(test)]
use objc2::runtime::ProtocolObject;
#[cfg(test)]
use objc2_metal::MTLSharedEvent;

use crate::backend::resource::{GpuEvent, GpuFence};

pub(crate) struct MetalFence {
    #[cfg(test)]
    signaled: AtomicBool,
}

impl MetalFence {
    #[cfg(test)]
    pub(crate) fn new() -> Self {
        Self {
            signaled: AtomicBool::new(false),
        }
    }

    #[cfg(test)]
    pub(crate) fn signal(&self) {
        self.signaled.store(true, Ordering::Release);
    }

    #[cfg(test)]
    pub(crate) fn reset(&self) {
        self.signaled.store(false, Ordering::Release);
    }
}

impl GpuFence for MetalFence {
    fn is_signaled(&self) -> bool {
        #[cfg(test)]
        {
            self.signaled.load(Ordering::Acquire)
        }
        #[cfg(not(test))]
        {
            true
        }
    }
}

pub(crate) struct MetalEvent {
    #[cfg(test)]
    pub(crate) inner: Retained<ProtocolObject<dyn MTLSharedEvent>>,
    #[cfg(test)]
    pub(crate) value: u64,
}

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
