use ash::{vk, Device};

pub struct Framebuffer {
    vk_framebuffer: vk::Framebuffer,
    device: Device,
}

impl Framebuffer {
    /// Create a new framebuffer from the given parameters.
    ///
    /// # Arguments
    /// * `device` - The Vulkan device
    /// * `render_pass` - The render pass this framebuffer is compatible with
    /// * `attachments` - Array of image views to attach to the framebuffer
    /// * `extent` - Width and height of the framebuffer
    /// * `layers` - Number of layers in image views (usually 1)
    pub fn create(
        device: &Device,
        render_pass: vk::RenderPass,
        attachments: &[vk::ImageView],
        extent: vk::Extent2D,
        layers: u32,
    ) -> Result<Self, vk::Result> {
        let create_info = vk::FramebufferCreateInfo::default()
            .render_pass(render_pass)
            .attachments(attachments)
            .width(extent.width)
            .height(extent.height)
            .layers(layers);

        let vk_framebuffer = unsafe { device.create_framebuffer(&create_info, None)? };

        Ok(Self {
            vk_framebuffer,
            device: device.clone(),
        })
    }

    /// Get the underlying Vulkan framebuffer handle.
    pub fn vk_framebuffer(&self) -> vk::Framebuffer {
        self.vk_framebuffer
    }

    /// Destroy the framebuffer and release Vulkan resources.
    pub fn destroy(&self) {
        unsafe {
            self.device.destroy_framebuffer(self.vk_framebuffer, None);
        }
    }
}

impl Drop for Framebuffer {
    fn drop(&mut self) {
        self.destroy();
    }
}

#[cfg(test)]
mod tests {
    // Note: These tests require a valid Vulkan context
    // They are meant to demonstrate the API and should be run in integration tests
}
