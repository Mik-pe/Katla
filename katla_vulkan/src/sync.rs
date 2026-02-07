//! Wrapper types for Vulkan objects.
//!
//! This module provides wrapper types for Vulkan objects to avoid exposing
//! `ash::vk` types in the public API of katla_vulkan.

use ash::vk;

/// Wrapper around `vk::Semaphore`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Semaphore(pub vk::Semaphore);

unsafe impl Send for Semaphore {}
unsafe impl Sync for Semaphore {}

impl Semaphore {
    /// Creates a new Semaphore wrapper.
    pub fn new(semaphore: vk::Semaphore) -> Self {
        Self(semaphore)
    }

    /// Returns the underlying `vk::Semaphore`.
    pub fn vk(&self) -> vk::Semaphore {
        self.0
    }
}

impl From<vk::Semaphore> for Semaphore {
    fn from(semaphore: vk::Semaphore) -> Self {
        Self(semaphore)
    }
}

impl From<Semaphore> for vk::Semaphore {
    fn from(wrapper: Semaphore) -> Self {
        wrapper.0
    }
}

impl AsRef<vk::Semaphore> for Semaphore {
    fn as_ref(&self) -> &vk::Semaphore {
        &self.0
    }
}

/// Wrapper around `vk::Fence`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Fence(pub vk::Fence);

unsafe impl Send for Fence {}
unsafe impl Sync for Fence {}

impl Fence {
    /// Creates a new Fence wrapper.
    pub fn new(fence: vk::Fence) -> Self {
        Self(fence)
    }

    /// Returns the underlying `vk::Fence`.
    pub fn vk(&self) -> vk::Fence {
        self.0
    }
}

impl From<vk::Fence> for Fence {
    fn from(fence: vk::Fence) -> Self {
        Self(fence)
    }
}

impl From<Fence> for vk::Fence {
    fn from(wrapper: Fence) -> Self {
        wrapper.0
    }
}

impl AsRef<vk::Fence> for Fence {
    fn as_ref(&self) -> &vk::Fence {
        &self.0
    }
}

/// Wrapper around `vk::CommandBuffer`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CommandBuffer(pub vk::CommandBuffer);

unsafe impl Send for CommandBuffer {}
unsafe impl Sync for CommandBuffer {}

impl CommandBuffer {
    /// Creates a new CommandBuffer wrapper.
    pub fn new(command_buffer: vk::CommandBuffer) -> Self {
        Self(command_buffer)
    }

    /// Returns the underlying `vk::CommandBuffer`.
    pub fn vk(&self) -> vk::CommandBuffer {
        self.0
    }
}

impl From<vk::CommandBuffer> for CommandBuffer {
    fn from(command_buffer: vk::CommandBuffer) -> Self {
        Self(command_buffer)
    }
}

impl From<CommandBuffer> for vk::CommandBuffer {
    fn from(wrapper: CommandBuffer) -> Self {
        wrapper.0
    }
}

impl AsRef<vk::CommandBuffer> for CommandBuffer {
    fn as_ref(&self) -> &vk::CommandBuffer {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_semaphore_wrapper() {
        let vk_sem = vk::Semaphore::null();
        let sem = Semaphore::new(vk_sem);
        assert_eq!(sem.vk(), vk_sem);
    }

    #[test]
    fn test_fence_wrapper() {
        let vk_fence = vk::Fence::null();
        let fence = Fence::new(vk_fence);
        assert_eq!(fence.vk(), vk_fence);
    }

    #[test]
    fn test_command_buffer_wrapper() {
        let vk_cmd = vk::CommandBuffer::null();
        let cmd = CommandBuffer::new(vk_cmd);
        assert_eq!(cmd.vk(), vk_cmd);
    }

    #[test]
    fn test_semaphore_conversions() {
        let vk_sem = vk::Semaphore::null();
        let sem: Semaphore = vk_sem.into();
        let back: vk::Semaphore = sem.into();
        assert_eq!(vk_sem, back);
    }

    #[test]
    fn test_fence_conversions() {
        let vk_fence = vk::Fence::null();
        let fence: Fence = vk_fence.into();
        let back: vk::Fence = fence.into();
        assert_eq!(vk_fence, back);
    }
}
