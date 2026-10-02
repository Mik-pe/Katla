//! Command-pool ownership shared only with its allocated command buffers.

use std::rc::Rc;

use ash::vk;

use super::CommandBuffer;
use super::context::native_lifetime::NativeDevice;
use crate::RendererError;

pub(crate) struct CommandPool {
    pub(crate) owner: Rc<CommandPoolOwner>,
}

pub(crate) struct CommandPoolOwner {
    pub(crate) native: Rc<NativeDevice>,
    pub(crate) pool: vk::CommandPool,
    #[cfg(test)]
    pub(crate) live_buffers: std::cell::Cell<usize>,
    #[cfg(test)]
    allocation_failure: std::cell::Cell<Option<vk::Result>>,
}

impl CommandPool {
    pub(crate) fn new(native: Rc<NativeDevice>, queue_family: u32) -> Result<Self, RendererError> {
        let info = vk::CommandPoolCreateInfo::default()
            .queue_family_index(queue_family)
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
        let pool = unsafe { native.device.create_command_pool(&info, None) }.map_err(|error| {
            RendererError::VulkanError("Failed to create command pool".into(), error)
        })?;
        Ok(Self {
            owner: Rc::new(CommandPoolOwner {
                native,
                pool,
                #[cfg(test)]
                live_buffers: std::cell::Cell::new(0),
                #[cfg(test)]
                allocation_failure: std::cell::Cell::new(None),
            }),
        })
    }

    pub(crate) fn create_command_buffers(
        &self,
        count: u32,
    ) -> Result<Vec<CommandBuffer>, RendererError> {
        if count == 0 {
            return Ok(Vec::new());
        }
        #[cfg(test)]
        if let Some(error) = self.owner.allocation_failure.take() {
            return Err(RendererError::VulkanError(
                "Failed to allocate command buffers".into(),
                error,
            ));
        }
        let info = vk::CommandBufferAllocateInfo::default()
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_pool(self.owner.pool)
            .command_buffer_count(count);
        let commands = unsafe { self.owner.native.device.allocate_command_buffers(&info) }
            .map_err(|error| {
                RendererError::VulkanError("Failed to allocate command buffers".into(), error)
            })?;
        Ok(commands
            .into_iter()
            .map(|command| CommandBuffer::from_raw(self.owner.clone(), command))
            .collect())
    }

    #[cfg(test)]
    fn fail_next_allocation(&self, error: vk::Result) {
        self.owner.allocation_failure.set(Some(error));
    }
}

impl Drop for CommandPoolOwner {
    fn drop(&mut self) {
        unsafe { self.native.device.destroy_command_pool(self.pool, None) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ValidationMode, VulkanContext};

    fn context() -> VulkanContext {
        VulkanContext::init_headless(
            ValidationMode::Enabled,
            c"command ownership".into(),
            c"Katla".into(),
        )
        .unwrap()
    }

    #[test]
    #[ignore = "requires a Vulkan device"]
    fn test_command_buffer_keeps_native_parents_and_validation_storage_alive() {
        let context = context();
        assert!(context.validation_active());
        let errors = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let captured = errors.clone();
        context.set_validation_callback(move |message, level| {
            if level == crate::ValidationLevel::Error {
                captured.lock().unwrap().push(message.to_owned());
            }
        });
        let device = Rc::downgrade(&context.gfx_cmdpool.owner.native);
        let validation = std::sync::Arc::downgrade(&context.validation_callback);
        let pool = Rc::downgrade(&context.gfx_cmdpool.owner);
        let command = context.begin_single_time_commands().unwrap();
        command.end_single_time_command().unwrap();
        drop(context);
        assert!(device.upgrade().is_some());
        assert!(validation.upgrade().is_some());
        assert_eq!(pool.upgrade().unwrap().live_buffers.get(), 1);
        drop(command);
        assert!(pool.upgrade().is_none());
        assert!(device.upgrade().is_none());
        assert!(validation.upgrade().is_none());
        let errors = errors.lock().unwrap();
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    #[ignore = "requires a Vulkan device"]
    fn test_command_allocation_failure_is_typed_and_retry_releases_the_batch() {
        let context = context();
        context
            .gfx_cmdpool
            .fail_next_allocation(vk::Result::ERROR_OUT_OF_HOST_MEMORY);
        assert!(matches!(
            context.begin_single_time_commands(),
            Err(RendererError::VulkanError(
                _,
                vk::Result::ERROR_OUT_OF_HOST_MEMORY
            ))
        ));
        assert_eq!(context.gfx_cmdpool.owner.live_buffers.get(), 0);
        let baseline = context.allocator.debug_allocation_stats();
        context
            .gfx_cmdpool
            .fail_next_allocation(vk::Result::ERROR_OUT_OF_HOST_MEMORY);
        let context = Rc::new(context);
        assert!(matches!(
            super::super::context::VulkanFrameCtx::init_headless(
                &context,
                vk::Extent2D {
                    width: 8,
                    height: 8
                }
            ),
            Err(RendererError::VulkanError(
                _,
                vk::Result::ERROR_OUT_OF_HOST_MEMORY
            ))
        ));
        assert_eq!(context.allocator.debug_allocation_stats(), baseline);
        assert_eq!(context.gfx_cmdpool.owner.live_buffers.get(), 0);
        let commands = context.gfx_cmdpool.create_command_buffers(3).unwrap();
        assert_eq!(context.gfx_cmdpool.owner.live_buffers.get(), 3);
        drop(commands);
        assert_eq!(context.gfx_cmdpool.owner.live_buffers.get(), 0);
    }
    #[test]
    #[ignore = "requires a Vulkan device"]
    fn test_frame_context_drop_retires_commands_and_targets() {
        let context = Rc::new(context());
        let errors = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let captured = errors.clone();
        context.set_validation_callback(move |message, level| {
            if level == crate::ValidationLevel::Error {
                captured.lock().unwrap().push(message.to_owned());
            }
        });
        let baseline = context.allocator.debug_allocation_stats();
        let frame = super::super::context::VulkanFrameCtx::init_headless(
            &context,
            vk::Extent2D {
                width: 8,
                height: 8,
            },
        )
        .unwrap();
        assert_eq!(
            context.gfx_cmdpool.owner.live_buffers.get(),
            crate::renderer::FRAMES_IN_FLIGHT
        );
        assert_ne!(context.allocator.debug_allocation_stats(), baseline);
        drop(frame);
        assert_eq!(context.gfx_cmdpool.owner.live_buffers.get(), 0);
        assert_eq!(context.allocator.debug_allocation_stats(), baseline);
        drop(context);
        let errors = errors.lock().unwrap();
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    #[ignore = "requires a Vulkan device"]
    fn test_foreign_command_buffer_is_rejected_before_native_recording_or_submission() {
        let first = context();
        let second = context();
        let errors = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        for context in [&first, &second] {
            let captured = errors.clone();
            context.set_validation_callback(move |message, level| {
                if level == crate::ValidationLevel::Error {
                    captured.lock().unwrap().push(message.to_owned());
                }
            });
        }
        let command = first.begin_single_time_commands().unwrap();
        assert!(matches!(
            second.end_single_time_commands(command),
            Err(RendererError::InvalidOperation(_))
        ));
        assert_eq!(first.gfx_cmdpool.owner.live_buffers.get(), 0);
        let command = first.begin_single_time_commands().unwrap();
        command.end_single_time_command().unwrap();
        assert!(matches!(
            second
                .gfx_queue
                .submit(&[&command], &[], &[], vk::Fence::null()),
            Err(RendererError::InvalidOperation(_))
        ));
        drop(command);
        let command = first.begin_single_time_commands().unwrap();
        first.end_single_time_commands(command).unwrap();
        assert_eq!(first.gfx_cmdpool.owner.live_buffers.get(), 0);
        drop(first);
        drop(second);
        let errors = errors.lock().unwrap();
        assert!(errors.is_empty(), "{errors:?}");
    }
}
