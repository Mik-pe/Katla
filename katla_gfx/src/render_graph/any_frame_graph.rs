//! Enum-based frame graph dispatch for dynamic backend selection.

use super::backend::RenderGraphBackend;
use super::error::RenderGraphError;
use super::frame_graph::FrameGraph;
use super::handles::PassId;
use super::pass::PassDesc;

use crate::metal::metal_renderer::MetalRenderer;
use crate::renderer::VulkanRenderer;

/// Frame graph that wraps both Vulkan and Metal backends behind a single type.
pub enum AnyFrameGraph {
    Vulkan(FrameGraph<VulkanRenderer>),
    #[cfg(target_os = "macos")]
    Metal(FrameGraph<MetalRenderer>),
}

impl AnyFrameGraph {
    pub fn from_vulkan(fg: FrameGraph<VulkanRenderer>) -> Self {
        AnyFrameGraph::Vulkan(fg)
    }

    #[cfg(target_os = "macos")]
    pub fn from_metal(fg: FrameGraph<MetalRenderer>) -> Self {
        AnyFrameGraph::Metal(fg)
    }

    pub fn new() -> Self {
        AnyFrameGraph::Vulkan(FrameGraph::new())
    }

    pub fn add_pass(&mut self, pass: PassDesc) -> PassId {
        match self {
            AnyFrameGraph::Vulkan(fg) => fg.add_pass(pass),
            #[cfg(target_os = "macos")]
            AnyFrameGraph::Metal(fg) => fg.add_pass(pass),
        }
    }

    pub fn insert_pass(&mut self, index: usize, pass: PassDesc) {
        match self {
            AnyFrameGraph::Vulkan(fg) => fg.insert_pass(index, pass),
            #[cfg(target_os = "macos")]
            AnyFrameGraph::Metal(fg) => fg.insert_pass(index, pass),
        }
    }

    pub fn pass_id(&self, name: &str) -> Option<PassId> {
        match self {
            AnyFrameGraph::Vulkan(fg) => fg.pass_id(name),
            #[cfg(target_os = "macos")]
            AnyFrameGraph::Metal(fg) => fg.pass_id(name),
        }
    }

    pub fn set_delta_time(&mut self, delta_time: f32) {
        match self {
            AnyFrameGraph::Vulkan(fg) => fg.set_delta_time(delta_time),
            #[cfg(target_os = "macos")]
            AnyFrameGraph::Metal(fg) => fg.set_delta_time(delta_time),
        }
    }

    pub fn set_frame_count(&mut self, frame_count: usize) {
        match self {
            AnyFrameGraph::Vulkan(fg) => fg.set_frame_count(frame_count),
            #[cfg(target_os = "macos")]
            AnyFrameGraph::Metal(fg) => fg.set_frame_count(frame_count),
        }
    }

    pub fn set_particle_emit_workgroup_count(&mut self, count: u32) {
        match self {
            AnyFrameGraph::Vulkan(fg) => fg.set_particle_emit_workgroup_count(count),
            #[cfg(target_os = "macos")]
            AnyFrameGraph::Metal(fg) => fg.set_particle_emit_workgroup_count(count),
        }
    }

    pub fn set_particle_simulate_workgroup_count(&mut self, count: u32) {
        match self {
            AnyFrameGraph::Vulkan(fg) => fg.set_particle_simulate_workgroup_count(count),
            #[cfg(target_os = "macos")]
            AnyFrameGraph::Metal(fg) => fg.set_particle_simulate_workgroup_count(count),
        }
    }

    pub fn set_skeleton_copy_commands(&mut self, commands: Vec<(u32, u32, u32)>) {
        match self {
            AnyFrameGraph::Vulkan(fg) => fg.set_skeleton_copy_commands(commands),
            #[cfg(target_os = "macos")]
            AnyFrameGraph::Metal(fg) => fg.set_skeleton_copy_commands(commands),
        }
    }

    pub fn set_tonemap_texture_index(
        &mut self,
        pass_id: PassId,
        texture_index: u32,
    ) -> Result<(), RenderGraphError> {
        match self {
            AnyFrameGraph::Vulkan(fg) => fg.set_tonemap_texture_index(pass_id, texture_index),
            #[cfg(target_os = "macos")]
            AnyFrameGraph::Metal(fg) => fg.set_tonemap_texture_index(pass_id, texture_index),
        }
    }

    pub fn get_ldr_texture_base_index(&self) -> Option<u32> {
        match self {
            AnyFrameGraph::Vulkan(fg) => fg.get_ldr_texture_base_index(),
            #[cfg(target_os = "macos")]
            AnyFrameGraph::Metal(fg) => fg.get_ldr_texture_base_index(),
        }
    }

    pub fn cleanup(&mut self) {
        match self {
            AnyFrameGraph::Vulkan(fg) => fg.cleanup(),
            #[cfg(target_os = "macos")]
            AnyFrameGraph::Metal(fg) => fg.cleanup(),
        }
    }

    // --- Backend-specific accessors ---

    /// Access the Vulkan frame graph mutably.
    pub fn as_vulkan_mut(&mut self) -> &mut FrameGraph<VulkanRenderer> {
        match self {
            AnyFrameGraph::Vulkan(fg) => fg,
            #[cfg(target_os = "macos")]
            AnyFrameGraph::Metal(_) => panic!("Expected Vulkan frame graph"),
        }
    }

    /// Access the Vulkan frame graph (const).
    pub fn as_vulkan(&self) -> &FrameGraph<VulkanRenderer> {
        match self {
            AnyFrameGraph::Vulkan(fg) => fg,
            #[cfg(target_os = "macos")]
            AnyFrameGraph::Metal(_) => panic!("Expected Vulkan frame graph"),
        }
    }

    /// Access the Metal frame graph mutably.
    #[cfg(target_os = "macos")]
    pub fn as_metal_mut(&mut self) -> &mut FrameGraph<MetalRenderer> {
        match self {
            AnyFrameGraph::Vulkan(_) => panic!("Expected Metal frame graph"),
            AnyFrameGraph::Metal(fg) => fg,
        }
    }

    /// Access the Metal frame graph (const).
    #[cfg(target_os = "macos")]
    pub fn as_metal(&self) -> &FrameGraph<MetalRenderer> {
        match self {
            AnyFrameGraph::Vulkan(_) => panic!("Expected Metal frame graph"),
            AnyFrameGraph::Metal(fg) => fg,
        }
    }

    /// Get the transient texture bindless slot for a named texture.
    /// Returns None if the texture doesn't exist or has no bindless slot.
    pub fn transient_texture_bindless_slot(&self, name: &str, frame_idx: usize) -> Option<u32> {
        match self {
            AnyFrameGraph::Vulkan(fg) => fg.transient_texture(name, frame_idx).and_then(|t| {
                <VulkanRenderer as RenderGraphBackend>::transient_texture_bindless_slot(t)
            }),
            #[cfg(target_os = "macos")]
            AnyFrameGraph::Metal(fg) => fg.transient_texture(name, frame_idx).and_then(|t| {
                <MetalRenderer as RenderGraphBackend>::transient_texture_bindless_slot(t)
            }),
        }
    }

    /// Recreate transient textures with new dimensions.
    /// Returns (texture_name, bindless_slot) pairs for all recreated textures.
    pub fn recreate_transient_textures(
        &mut self,
        renderer: &mut crate::renderer::any_renderer::AnyRenderer,
        width: u32,
        height: u32,
    ) -> Result<Vec<(String, u32)>, RenderGraphError> {
        match (self, renderer) {
            (AnyFrameGraph::Vulkan(fg), crate::renderer::any_renderer::AnyRenderer::Vulkan(r)) => {
                fg.recreate_transient_textures(r, width, height)
            }
            #[cfg(target_os = "macos")]
            (AnyFrameGraph::Metal(fg), crate::renderer::any_renderer::AnyRenderer::Metal(r)) => {
                fg.recreate_transient_textures(r, width, height)
            }
            _ => Err(RenderGraphError::BackendError(
                "Backend mismatch between frame graph and renderer".into(),
            )),
        }
    }

    /// Initialize transient textures using the renderer.
    pub fn initialize_transient_textures(
        &mut self,
        renderer: &mut crate::renderer::any_renderer::AnyRenderer,
    ) -> Result<(), RenderGraphError> {
        match (self, renderer) {
            (AnyFrameGraph::Vulkan(fg), crate::renderer::any_renderer::AnyRenderer::Vulkan(r)) => {
                fg.initialize_transient_textures(r)
            }
            #[cfg(target_os = "macos")]
            (AnyFrameGraph::Metal(fg), crate::renderer::any_renderer::AnyRenderer::Metal(r)) => {
                fg.initialize_transient_textures(r)
            }
            _ => Err(RenderGraphError::BackendError(
                "Backend mismatch between frame graph and renderer".into(),
            )),
        }
    }

    /// Register a transient texture with the bindless system.
    pub fn register_transient_texture_bindless(
        &mut self,
        renderer: &mut crate::renderer::any_renderer::AnyRenderer,
        name: &str,
    ) -> Result<u32, RenderGraphError> {
        match (self, renderer) {
            (AnyFrameGraph::Vulkan(fg), crate::renderer::any_renderer::AnyRenderer::Vulkan(r)) => {
                fg.register_transient_texture_bindless(r, name)
            }
            #[cfg(target_os = "macos")]
            (AnyFrameGraph::Metal(fg), crate::renderer::any_renderer::AnyRenderer::Metal(r)) => {
                fg.register_transient_texture_bindless(r, name)
            }
            _ => Err(RenderGraphError::BackendError(
                "Backend mismatch between frame graph and renderer".into(),
            )),
        }
    }

    // --- Metal-specific methods ---

    #[cfg(target_os = "macos")]
    pub fn transient_image_view_metal(
        &self,
        name: &str,
        frame_idx: usize,
    ) -> Option<crate::metal::texture::MetalTextureView> {
        match self {
            AnyFrameGraph::Vulkan(_) => None,
            AnyFrameGraph::Metal(fg) => fg.transient_image_view(name, frame_idx),
        }
    }

    #[cfg(target_os = "macos")]
    pub fn transient_texture_metal(
        &self,
        name: &str,
        frame_idx: usize,
    ) -> Option<&<MetalRenderer as RenderGraphBackend>::TransientTexture> {
        match self {
            AnyFrameGraph::Vulkan(_) => None,
            AnyFrameGraph::Metal(fg) => fg.transient_texture(name, frame_idx),
        }
    }
}

impl Default for AnyFrameGraph {
    fn default() -> Self {
        Self::new()
    }
}
