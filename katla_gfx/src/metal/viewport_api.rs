use super::metal_renderer::MetalRenderer;

impl MetalRenderer {
    pub(crate) fn create_viewport_impl(&mut self) -> crate::viewport::ViewportBuilder {
        crate::viewport::ViewportBuilder::new()
    }

    pub(crate) fn viewport_count_impl(&self) -> usize {
        self.viewports.len()
    }

    pub(crate) fn get_viewport_impl(
        &self,
        handle: crate::viewport::ViewportHandle,
    ) -> Option<&crate::viewport::Viewport> {
        self.viewports.get(handle.0)
    }

    pub(crate) fn viewport_extent_impl(
        &self,
        handle: crate::viewport::ViewportHandle,
    ) -> Option<crate::size::Size2D> {
        self.viewports.get(handle.0).map(|v| v.extent)
    }

    pub(crate) fn destroy_viewport_impl(&mut self, handle: crate::viewport::ViewportHandle) {
        if handle.0 < self.viewports.len() {
            // Viewports don't have GPU resources to free, just leave the slot
        }
    }
}
