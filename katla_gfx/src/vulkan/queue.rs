use std::rc::Rc;

use super::{CommandBuffer, context::native_lifetime::NativeDevice};
use crate::RendererError;

use ash::{Device, vk};
use smallvec::SmallVec;

type SemaphoreInfos = SmallVec<[vk::SemaphoreSubmitInfo<'static>; 2]>;

pub struct Queue {
    device: Device,
    _native_device: Rc<NativeDevice>,
    queue: vk::Queue,
    #[cfg(test)]
    submission_failure: std::cell::Cell<Option<vk::Result>>,
}

fn semaphore_waits(waits: &[(vk::Semaphore, vk::PipelineStageFlags2)]) -> SemaphoreInfos {
    waits
        .iter()
        .map(|&(semaphore, stage)| {
            vk::SemaphoreSubmitInfo::default()
                .semaphore(semaphore)
                .stage_mask(stage)
        })
        .collect()
}

fn semaphore_signals(signals: &[vk::Semaphore]) -> SemaphoreInfos {
    signals
        .iter()
        .map(|&semaphore| {
            vk::SemaphoreSubmitInfo::default()
                .semaphore(semaphore)
                .stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
        })
        .collect()
}

impl Queue {
    pub(crate) fn new(
        native_device: Rc<NativeDevice>,
        queue_family_index: u32,
        queue_index: u32,
    ) -> Self {
        let device = native_device.device.clone();
        let queue = unsafe { device.get_device_queue(queue_family_index, queue_index) };
        Self {
            device,
            _native_device: native_device,
            queue,
            #[cfg(test)]
            submission_failure: std::cell::Cell::new(None),
        }
    }

    pub fn wait_idle(&self) {
        unsafe {
            let _ = self.device.queue_wait_idle(self.queue);
        }
    }

    /// Get the raw Vulkan queue handle.
    pub fn vk_queue(&self) -> vk::Queue {
        self.queue
    }

    /// Submit through synchronization2. Each binary wait carries its execution
    /// stage; binary signals cover all commands in the submission.
    pub(crate) fn submit(
        &self,
        command_buffers: &[&CommandBuffer],
        waits: &[(vk::Semaphore, vk::PipelineStageFlags2)],
        signals: &[vk::Semaphore],
        fence: vk::Fence,
    ) -> Result<(), RendererError> {
        if command_buffers
            .iter()
            .any(|command| !command.belongs_to(&self._native_device))
        {
            return Err(RendererError::InvalidOperation(
                "Command buffer belongs to another Vulkan device".into(),
            ));
        }
        let commands: SmallVec<[vk::CommandBufferSubmitInfo<'static>; 4]> = command_buffers
            .iter()
            .map(|command| {
                vk::CommandBufferSubmitInfo::default().command_buffer(command.vk_command_buffer())
            })
            .collect();
        let waits = semaphore_waits(waits);
        let signals = semaphore_signals(signals);
        let submit = vk::SubmitInfo2::default()
            .command_buffer_infos(&commands)
            .wait_semaphore_infos(&waits)
            .signal_semaphore_infos(&signals);
        #[cfg(test)]
        if let Some(error) = self.submission_failure.take() {
            return Err(RendererError::VulkanError(
                "Failed to submit Vulkan queue".into(),
                error,
            ));
        }
        unsafe { self.device.queue_submit2(self.queue, &[submit], fence) }
            .map_err(|e| RendererError::VulkanError("Failed to submit Vulkan queue".into(), e))
    }

    #[cfg(test)]
    pub(crate) fn fail_next_submission(&self, error: vk::Result) {
        self.submission_failure.set(Some(error));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ash::vk::Handle;

    #[test]
    fn test_semaphore_scopes_preserve_binary_submission_ordering() {
        let acquire = vk::Semaphore::from_raw(1);
        let previous = vk::Semaphore::from_raw(2);
        let waits = semaphore_waits(&[
            (acquire, vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT),
            (previous, vk::PipelineStageFlags2::ALL_COMMANDS),
        ]);
        assert_eq!(waits[0].semaphore, acquire);
        assert_eq!(
            waits[0].stage_mask,
            vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT
        );
        assert_eq!(waits[1].semaphore, previous);
        assert_eq!(waits[1].stage_mask, vk::PipelineStageFlags2::ALL_COMMANDS);
        let signals = semaphore_signals(&[acquire, previous]);
        assert!(signals.iter().all(|signal| signal.stage_mask
            == vk::PipelineStageFlags2::ALL_COMMANDS
            && signal.value == 0));
    }

    #[test]
    #[ignore = "requires a Vulkan device"]
    fn test_failed_frame_submission_leaves_the_slot_reusable() {
        use crate::render_graph::{FrameGraphBuilder, GeometryPass};
        use crate::renderer::frame_scope::FrameAcquisition;
        use crate::texture::ImageFormat;
        use crate::{GpuRenderer, ValidationMode, VulkanRenderer};
        let mut renderer = VulkanRenderer::init_headless(
            64,
            48,
            ValidationMode::Enabled,
            c"submission failure".into(),
            c"Katla".into(),
        )
        .unwrap();
        let errors = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let captured = errors.clone();
        renderer
            .context
            .set_validation_callback(move |message, level| {
                if level == crate::vulkan::context::ValidationLevel::Error {
                    captured.lock().unwrap().push(message.to_string());
                }
            });
        let mut graph = FrameGraphBuilder::new()
            .add_pass(
                GeometryPass::new("clear")
                    .without_depth()
                    .write_color("backbuffer", ImageFormat::B8G8R8A8Srgb),
            )
            .export_resource("backbuffer")
            .build::<VulkanRenderer>()
            .unwrap();
        let FrameAcquisition::Ready(frame) = renderer.acquire_frame().unwrap() else {
            panic!("headless frame unavailable")
        };
        renderer.render(&frame, &mut graph, |_| {}).unwrap();
        renderer
            .context
            .gfx_queue
            .fail_next_submission(vk::Result::ERROR_OUT_OF_HOST_MEMORY);
        assert!(matches!(
            renderer.present(frame),
            Err(RendererError::VulkanError(
                _,
                vk::Result::ERROR_OUT_OF_HOST_MEMORY
            ))
        ));
        let FrameAcquisition::Ready(retry) = renderer.acquire_frame().unwrap() else {
            panic!("headless retry unavailable")
        };
        assert_eq!(retry.slot(), frame.slot());
        renderer.render(&retry, &mut graph, |_| {}).unwrap();
        renderer.present(retry).unwrap().surface.unwrap();
        let source = renderer
            .graph_texture_source(graph.resource_id("backbuffer").unwrap())
            .unwrap();
        let baseline = renderer.context.allocator.debug_allocation_stats();
        renderer
            .context
            .gfx_queue
            .fail_next_submission(vk::Result::ERROR_OUT_OF_HOST_MEMORY);
        assert!(
            renderer
                .queue_texture_readback(source, crate::TextureReadbackRegion::pixel(0, 0))
                .is_err()
        );
        assert_eq!(
            renderer.context.allocator.debug_allocation_stats(),
            baseline
        );

        let command = renderer.context.begin_single_time_commands().unwrap();
        renderer
            .context
            .gfx_queue
            .fail_next_submission(vk::Result::ERROR_OUT_OF_HOST_MEMORY);
        assert!(renderer.context.end_single_time_commands(command).is_err());
        let ticket = renderer
            .queue_texture_readback(source, crate::TextureReadbackRegion::pixel(0, 0))
            .unwrap();
        renderer.wait_for_device();
        let pixels = renderer.poll_texture_readback(ticket).unwrap().unwrap();
        assert_eq!(pixels.bytes, [0, 0, 0, 255]);
        graph.cleanup();
        renderer.destroy();
        let errors = errors.lock().unwrap();
        assert!(errors.is_empty(), "{errors:?}");
    }
}
