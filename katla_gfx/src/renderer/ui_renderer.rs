//! Optional native scratch storage for UI draw encoding.

use crate::renderer::UiFrameResources;
use crate::vulkan::context::VulkanContext;
use std::rc::Rc;

#[derive(Default)]
pub(crate) struct UIRenderer {
    ui_resources: Option<UiFrameResources>,
}

impl UIRenderer {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn ui_resources_mut(
        &mut self,
        context: &Rc<VulkanContext>,
    ) -> &mut UiFrameResources {
        self.ui_resources
            .get_or_insert_with(|| UiFrameResources::new(context))
    }

    #[cfg(test)]
    pub(crate) fn is_installed(&self) -> bool {
        self.ui_resources.is_some()
    }

    pub(crate) fn destroy(&mut self, context: &Rc<VulkanContext>) {
        if let Some(resources) = self.ui_resources.take() {
            for (buffer, allocation) in resources.uniform_buffers {
                context.free_buffer(buffer, allocation);
            }
        }
    }
}
