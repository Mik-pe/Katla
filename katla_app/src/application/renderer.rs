//! Frame rendering implementation.
//!
//! This module delegates frame rendering to VulkanRenderer.

use super::Application;

impl Application {
    /// Render a single frame.
    ///
    /// Delegates to VulkanRenderer::render_frame() which handles:
    /// - Acquiring swapchain images
    /// - Recording command buffers
    /// - Executing render passes
    /// - Submitting to GPU
    /// - Presenting to swapchain
    pub fn render_frame(&mut self) {
        self.renderer.render_frame();
    }
}
