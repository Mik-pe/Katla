//! Enum-based frame dispatch for dynamic backend selection.

use super::handles::PassId;
use crate::renderer::types::{DrawList, UIDrawList};

#[cfg(target_os = "macos")]
use crate::metal::metal_renderer::MetalRenderer;
use crate::renderer::VulkanRenderer;

/// Frame context that wraps both Vulkan and Metal behind a single type.
///
/// Passed to the closure in `AnyRenderer::render()`. Provides the same
/// submit/submit_ui/dispatch API as the backend-specific Frame types.
pub enum AnyFrame<'a, 'b> {
    Vulkan(&'a mut super::frame::Frame<'b, VulkanRenderer>),
    #[cfg(target_os = "macos")]
    Metal(&'a mut super::frame::Frame<'b, MetalRenderer>),
}

impl<'a, 'b> AnyFrame<'a, 'b> {
    /// Submit a draw list to a pass.
    pub fn submit(&mut self, pass_id: PassId, draw_list: &DrawList) -> &mut Self {
        match self {
            AnyFrame::Vulkan(f) => {
                f.submit(pass_id, draw_list);
            }
            #[cfg(target_os = "macos")]
            AnyFrame::Metal(f) => {
                f.submit(pass_id, draw_list);
            }
        }
        self
    }

    /// Submit a UI draw list to a pass.
    pub fn submit_ui(&mut self, pass_id: PassId, ui_draw_list: &UIDrawList) -> &mut Self {
        match self {
            AnyFrame::Vulkan(f) => {
                f.submit_ui(pass_id, ui_draw_list);
            }
            #[cfg(target_os = "macos")]
            AnyFrame::Metal(f) => {
                f.submit_ui(pass_id, ui_draw_list);
            }
        }
        self
    }

    /// Dispatch compute workgroups for a pass.
    pub fn dispatch(&mut self, pass_id: PassId, x: u32, y: u32, z: u32) -> &mut Self {
        match self {
            AnyFrame::Vulkan(f) => {
                f.dispatch(pass_id, x, y, z);
            }
            #[cfg(target_os = "macos")]
            AnyFrame::Metal(f) => {
                f.dispatch(pass_id, x, y, z);
            }
        }
        self
    }
}
