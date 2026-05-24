//! Enum-based renderer dispatch for dynamic backend selection.
//!
//! `AnyRenderer` wraps `VulkanRenderer` and `MetalRenderer` behind a single
//! enum that implements `GpuRenderer`. This allows both backends to compile
//! side-by-side and be selected at runtime.

use crate::error::RendererError;
use crate::handle::{MaterialHandle, MeshHandle, SkeletonHandle, TextureHandle};
use crate::renderer::gpu_renderer::GpuRenderer;
use crate::renderer::types::{DrawCall, DrawList, FrameUniforms, PointLightGPU, UIDrawList};
use crate::texture::TextureDescriptor;
use crate::viewport::{Viewport, ViewportBuilder, ViewportHandle};

use crate::metal::metal_renderer::MetalRenderer;
use crate::renderer::VulkanRenderer;

/// Renderer backend that wraps both Vulkan and Metal behind a single type.
///
/// Implements `GpuRenderer` by delegating to the active variant.
/// Backend-specific methods are available via `as_vulkan()` / `as_metal()`.
pub enum AnyRenderer {
    Vulkan(VulkanRenderer),
    #[cfg(target_os = "macos")]
    Metal(MetalRenderer),
}

impl AnyRenderer {
    /// Which backend is active.
    pub fn backend_name(&self) -> &'static str {
        match self {
            AnyRenderer::Vulkan(_) => "vulkan",
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(_) => "metal",
        }
    }

    /// Access the Vulkan renderer, if active.
    pub fn as_vulkan(&mut self) -> Option<&mut VulkanRenderer> {
        match self {
            AnyRenderer::Vulkan(r) => Some(r),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(_) => None,
        }
    }

    /// Access the Vulkan renderer (panics if not Vulkan).
    pub fn unwrap_vulkan(&mut self) -> &mut VulkanRenderer {
        self.as_vulkan().expect("Expected Vulkan backend")
    }

    /// Access the Metal renderer, if active.
    #[cfg(target_os = "macos")]
    pub fn as_metal(&mut self) -> Option<&mut MetalRenderer> {
        match self {
            AnyRenderer::Vulkan(_) => None,
            AnyRenderer::Metal(r) => Some(r),
        }
    }

    /// Access the Metal renderer (panics if not Metal).
    #[cfg(target_os = "macos")]
    pub fn unwrap_metal(&mut self) -> &mut MetalRenderer {
        self.as_metal().expect("Expected Metal backend")
    }

    /// Create a new Vulkan renderer.
    pub fn new_vulkan(
        display: &dyn raw_window_handle::HasDisplayHandle,
        window: &dyn raw_window_handle::HasWindowHandle,
        validation_mode: crate::error::ValidationMode,
        app_name: std::ffi::CString,
        engine_name: std::ffi::CString,
    ) -> Result<Self, RendererError> {
        Ok(AnyRenderer::Vulkan(VulkanRenderer::init(
            display,
            window,
            validation_mode,
            app_name,
            engine_name,
        )?))
    }

    /// Create a new Metal renderer.
    #[cfg(target_os = "macos")]
    pub fn new_metal(
        display: &dyn raw_window_handle::HasDisplayHandle,
        window: &dyn raw_window_handle::HasWindowHandle,
        validation_mode: crate::error::ValidationMode,
        app_name: std::ffi::CString,
        engine_name: std::ffi::CString,
    ) -> Result<Self, RendererError> {
        Ok(AnyRenderer::Metal(MetalRenderer::init(
            display,
            window,
            validation_mode,
            app_name,
            engine_name,
        )?))
    }
}

impl GpuRenderer for AnyRenderer {
    fn swapchain_extent(&self) -> crate::Size2D {
        match self {
            AnyRenderer::Vulkan(r) => r.swapchain_extent(),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.swapchain_extent(),
        }
    }

    fn current_frame(&self) -> usize {
        match self {
            AnyRenderer::Vulkan(r) => r.current_frame(),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.current_frame(),
        }
    }

    fn num_images(&self) -> usize {
        match self {
            AnyRenderer::Vulkan(r) => r.num_images(),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.num_images(),
        }
    }

    fn wait_for_device(&self) {
        match self {
            AnyRenderer::Vulkan(r) => r.wait_for_device(),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.wait_for_device(),
        }
    }

    fn destroy(&mut self) {
        match self {
            AnyRenderer::Vulkan(r) => r.destroy(),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.destroy(),
        }
    }

    fn capabilities(&self) -> &crate::renderer::types::GpuCapabilities {
        match self {
            AnyRenderer::Vulkan(r) => r.capabilities(),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.capabilities(),
        }
    }

    fn wait_for_frame(&mut self) -> Result<(), RendererError> {
        match self {
            AnyRenderer::Vulkan(r) => r.wait_for_frame(),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.wait_for_frame(),
        }
    }

    fn set_frame_uniforms(&mut self, uniforms: FrameUniforms) {
        match self {
            AnyRenderer::Vulkan(r) => r.set_frame_uniforms(uniforms),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.set_frame_uniforms(uniforms),
        }
    }

    fn execute_draw_calls(&mut self, draw_list: &DrawList) -> Result<(), RendererError> {
        match self {
            AnyRenderer::Vulkan(r) => r.execute_draw_calls(draw_list),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.execute_draw_calls(draw_list),
        }
    }

    fn draw(
        &mut self,
        uniforms: &FrameUniforms,
        draw_calls: &[DrawCall],
    ) -> Result<DrawList, RendererError> {
        match self {
            AnyRenderer::Vulkan(r) => r.draw(uniforms, draw_calls),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.draw(uniforms, draw_calls),
        }
    }

    fn frame_uniforms(&self) -> &FrameUniforms {
        match self {
            AnyRenderer::Vulkan(r) => r.frame_uniforms(),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.frame_uniforms(),
        }
    }

    fn render_frame(&mut self) -> Result<(), RendererError> {
        match self {
            AnyRenderer::Vulkan(r) => r.render_frame(),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.render_frame(),
        }
    }

    fn begin_frame(&mut self) -> Result<u32, RendererError> {
        match self {
            AnyRenderer::Vulkan(r) => r.begin_frame(),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.begin_frame(),
        }
    }

    fn end_frame(&mut self) -> Result<(), RendererError> {
        match self {
            AnyRenderer::Vulkan(r) => r.end_frame(),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.end_frame(),
        }
    }

    fn create_mesh<T, U>(&mut self, vertices: &[T], indices: &[U]) -> MeshHandle
    where
        T: bytemuck::Pod,
        U: bytemuck::Pod,
    {
        match self {
            AnyRenderer::Vulkan(r) => r.create_mesh(vertices, indices),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.create_mesh(vertices, indices),
        }
    }

    fn create_mesh_dynamic(
        &mut self,
        vertex_data: &[u8],
        vertex_count: u32,
        indices: &[u32],
    ) -> MeshHandle {
        match self {
            AnyRenderer::Vulkan(r) => r.create_mesh_dynamic(vertex_data, vertex_count, indices),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.create_mesh_dynamic(vertex_data, vertex_count, indices),
        }
    }

    fn update_mesh_dynamic(
        &mut self,
        mesh: MeshHandle,
        vertex_data: &[u8],
        vertex_count: u32,
        indices: &[u32],
    ) -> Result<(), RendererError> {
        match self {
            AnyRenderer::Vulkan(r) => {
                r.update_mesh_dynamic(mesh, vertex_data, vertex_count, indices)
            }
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => {
                r.update_mesh_dynamic(mesh, vertex_data, vertex_count, indices)
            }
        }
    }

    fn create_texture(&mut self, desc: &TextureDescriptor, data: &[u8]) -> TextureHandle {
        match self {
            AnyRenderer::Vulkan(r) => r.create_texture(desc, data),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.create_texture(desc, data),
        }
    }

    fn create_texture_solid(&mut self, color: [u8; 4]) -> TextureHandle {
        match self {
            AnyRenderer::Vulkan(r) => r.create_texture_solid(color),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.create_texture_solid(color),
        }
    }

    fn update_texture(&mut self, handle: TextureHandle, data: &[u8]) -> Result<(), RendererError> {
        match self {
            AnyRenderer::Vulkan(r) => r.update_texture(handle, data),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.update_texture(handle, data),
        }
    }

    fn get_bindless_slot(&self, handle: TextureHandle) -> Option<u32> {
        match self {
            AnyRenderer::Vulkan(r) => r.get_bindless_slot(handle),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.get_bindless_slot(handle),
        }
    }

    fn get_texture_at_slot(&self, slot: u32) -> Option<TextureHandle> {
        match self {
            AnyRenderer::Vulkan(r) => r.get_texture_at_slot(slot),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.get_texture_at_slot(slot),
        }
    }

    fn get_texture_bindless_index(&self, handle: TextureHandle) -> u32 {
        match self {
            AnyRenderer::Vulkan(r) => r.get_texture_bindless_index(handle),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.get_texture_bindless_index(handle),
        }
    }

    fn default_texture(&self) -> TextureHandle {
        match self {
            AnyRenderer::Vulkan(r) => r.default_texture(),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.default_texture(),
        }
    }

    fn compile_material(
        &mut self,
        shader_path: &str,
        vertex_type: &str,
    ) -> Result<MaterialHandle, RendererError> {
        match self {
            AnyRenderer::Vulkan(r) => GpuRenderer::compile_material(r, shader_path, vertex_type),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => GpuRenderer::compile_material(r, shader_path, vertex_type),
        }
    }

    fn set_material_texture_indices(&mut self, material: MaterialHandle, indices: [u32; 4]) {
        match self {
            AnyRenderer::Vulkan(r) => r.set_material_texture_indices(material, indices),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.set_material_texture_indices(material, indices),
        }
    }

    fn default_material(&self) -> MaterialHandle {
        match self {
            AnyRenderer::Vulkan(r) => r.default_material(),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.default_material(),
        }
    }

    fn destroy_mesh(&mut self, handle: MeshHandle) {
        match self {
            AnyRenderer::Vulkan(r) => r.destroy_mesh(handle),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.destroy_mesh(handle),
        }
    }

    fn destroy_material(&mut self, handle: MaterialHandle) {
        match self {
            AnyRenderer::Vulkan(r) => r.destroy_material(handle),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.destroy_material(handle),
        }
    }

    fn destroy_texture(&mut self, handle: TextureHandle) {
        match self {
            AnyRenderer::Vulkan(r) => r.destroy_texture(handle),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.destroy_texture(handle),
        }
    }

    fn destroy_skeleton(&mut self, handle: SkeletonHandle) {
        match self {
            AnyRenderer::Vulkan(r) => r.destroy_skeleton(handle),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.destroy_skeleton(handle),
        }
    }

    fn create_viewport(&mut self) -> ViewportBuilder {
        match self {
            AnyRenderer::Vulkan(r) => r.create_viewport(),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.create_viewport(),
        }
    }

    fn viewport_count(&self) -> usize {
        match self {
            AnyRenderer::Vulkan(r) => r.viewport_count(),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.viewport_count(),
        }
    }

    fn get_viewport(&self, handle: ViewportHandle) -> Option<&Viewport> {
        match self {
            AnyRenderer::Vulkan(r) => r.get_viewport(handle),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.get_viewport(handle),
        }
    }

    fn viewport_extent(&self, handle: ViewportHandle) -> Option<crate::Size2D> {
        match self {
            AnyRenderer::Vulkan(r) => r.viewport_extent(handle),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.viewport_extent(handle),
        }
    }

    fn destroy_viewport(&mut self, handle: ViewportHandle) {
        match self {
            AnyRenderer::Vulkan(r) => r.destroy_viewport(handle),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.destroy_viewport(handle),
        }
    }

    fn resize(&mut self, width: u32, height: u32) -> Result<(), RendererError> {
        match self {
            AnyRenderer::Vulkan(r) => r.resize(width, height),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.resize(width, height),
        }
    }

    fn upload_lights(&mut self, lights: &[PointLightGPU]) {
        match self {
            AnyRenderer::Vulkan(r) => r.upload_lights(lights),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.upload_lights(lights),
        }
    }

    fn update_shadows(&mut self, light_direction: [f32; 3]) {
        match self {
            AnyRenderer::Vulkan(r) => r.update_shadows(light_direction),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.update_shadows(light_direction),
        }
    }

    fn upload_shadow_cascades(&mut self) {
        match self {
            AnyRenderer::Vulkan(r) => r.upload_shadow_cascades(),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.upload_shadow_cascades(),
        }
    }

    fn depth_texture_base_index(&self) -> Option<u32> {
        match self {
            AnyRenderer::Vulkan(r) => r.depth_texture_base_index(),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.depth_texture_base_index(),
        }
    }

    fn viewport_bindless_index(&self) -> Option<u32> {
        match self {
            AnyRenderer::Vulkan(r) => r.viewport_bindless_index(),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.viewport_bindless_index(),
        }
    }

    fn register_depth_textures_bindless(&mut self) -> Result<u32, RendererError> {
        match self {
            AnyRenderer::Vulkan(r) => r.register_depth_textures_bindless(),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.register_depth_textures_bindless(),
        }
    }

    fn geometry_hdr_bindless_index(&self) -> Option<u32> {
        match self {
            AnyRenderer::Vulkan(r) => r.geometry_hdr_bindless_index(),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.geometry_hdr_bindless_index(),
        }
    }

    fn init_animation_pipeline(
        &mut self,
        shader_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        match self {
            AnyRenderer::Vulkan(r) => r.init_animation_pipeline(shader_path),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.init_animation_pipeline(shader_path),
        }
    }

    fn set_ui_material(&mut self, material: MaterialHandle) {
        match self {
            AnyRenderer::Vulkan(r) => r.set_ui_material(material),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.set_ui_material(material),
        }
    }

    fn render_ui_pass(&mut self, draw_list: UIDrawList) {
        match self {
            AnyRenderer::Vulkan(r) => r.render_ui_pass(draw_list),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.render_ui_pass(draw_list),
        }
    }

    fn create_skeleton(&mut self, joint_count: usize) -> Result<SkeletonHandle, RendererError> {
        match self {
            AnyRenderer::Vulkan(r) => r.create_skeleton(joint_count),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.create_skeleton(joint_count),
        }
    }

    fn update_skeleton(&mut self, handle: SkeletonHandle, matrices: &[[f32; 16]]) {
        match self {
            AnyRenderer::Vulkan(r) => r.update_skeleton(handle, matrices),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.update_skeleton(handle, matrices),
        }
    }

    fn init_particle_system(&mut self) -> Result<(), RendererError> {
        match self {
            AnyRenderer::Vulkan(r) => r.init_particle_system(),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.init_particle_system(),
        }
    }

    fn create_ui_font_atlas(&mut self, width: u32, height: u32, data: &[u8]) -> TextureHandle {
        match self {
            AnyRenderer::Vulkan(r) => r.create_ui_font_atlas(width, height, data),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.create_ui_font_atlas(width, height, data),
        }
    }

    fn update_ui_font_atlas(&mut self, width: u32, height: u32, data: &[u8]) {
        match self {
            AnyRenderer::Vulkan(r) => r.update_ui_font_atlas(width, height, data),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.update_ui_font_atlas(width, height, data),
        }
    }

    fn ui_font_atlas_handle(&self) -> Option<TextureHandle> {
        match self {
            AnyRenderer::Vulkan(r) => r.ui_font_atlas_handle(),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.ui_font_atlas_handle(),
        }
    }

    fn begin_timestamp(&mut self, label: &str) {
        match self {
            AnyRenderer::Vulkan(r) => r.begin_timestamp(label),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.begin_timestamp(label),
        }
    }

    fn end_timestamp(&mut self, label: &str) {
        match self {
            AnyRenderer::Vulkan(r) => r.end_timestamp(label),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.end_timestamp(label),
        }
    }

    fn read_timestamps(&self) -> Vec<crate::renderer::types::GpuTimestamp> {
        match self {
            AnyRenderer::Vulkan(r) => r.read_timestamps(),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.read_timestamps(),
        }
    }
}

// --- Non-trait methods that both backends implement ---

impl AnyRenderer {
    /// Execute the frame graph and present the frame.
    /// The closure receives an `AnyFrame` for submitting draw lists to passes.
    pub fn render<F>(
        &mut self,
        frame_graph: &mut crate::render_graph::any_frame_graph::AnyFrameGraph,
        f: F,
    ) -> Result<(), RendererError>
    where
        F: FnOnce(&mut crate::render_graph::any_frame::AnyFrame<'_, '_>),
    {
        match self {
            AnyRenderer::Vulkan(r) => {
                let fg = frame_graph.as_vulkan_mut();
                r.render(fg, |frame| {
                    let mut any_frame = crate::render_graph::any_frame::AnyFrame::Vulkan(frame);
                    f(&mut any_frame);
                })
            }
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => {
                let fg = frame_graph.as_metal_mut();
                r.render(fg, |frame| {
                    let mut any_frame = crate::render_graph::any_frame::AnyFrame::Metal(frame);
                    f(&mut any_frame);
                })
            }
        }
    }

    /// Recreate the swapchain (Vulkan) or resize (Metal).
    /// Returns updated transient texture names and bindless slots.
    pub fn recreate_swapchain(
        &mut self,
        frame_graph: &mut crate::render_graph::any_frame_graph::AnyFrameGraph,
    ) -> Result<Vec<(String, u32)>, RendererError> {
        match self {
            AnyRenderer::Vulkan(r) => {
                let fg = frame_graph.as_vulkan_mut();
                r.recreate_swapchain(fg)
            }
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(_r) => {
                // MetalRenderer doesn't have recreate_swapchain with frame graph.
                // Metal swapchain recreation happens via resize() + recreate_transient_textures.
                Err(RendererError::InvalidOperation(
                    "Metal backend uses resize(), not recreate_swapchain()".into(),
                ))
            }
        }
    }

    // --- Metal-specific methods ---

    // --- Pipeline init methods (delegated to GpuRenderer trait) ---

    pub fn init_light_culling(
        &mut self,
        width: u32,
        height: u32,
        shader_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        match self {
            AnyRenderer::Vulkan(r) => r.init_light_culling(width, height, shader_path),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.init_light_culling(width, height, shader_path),
        }
    }

    pub fn init_shadow_resources(&mut self) -> Result<(), RendererError> {
        GpuRenderer::init_shadow_resources(self)
    }

    pub fn init_shadow_pipeline(
        &mut self,
        shader_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        match self {
            AnyRenderer::Vulkan(r) => r.init_shadow_pipeline(shader_path),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.init_shadow_pipeline(shader_path),
        }
    }

    pub fn init_shadow_pipeline_skinned(
        &mut self,
        shader_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        match self {
            AnyRenderer::Vulkan(r) => r.init_shadow_pipeline_skinned(shader_path),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.init_shadow_pipeline_skinned(shader_path),
        }
    }

    pub fn init_sky_pipeline(
        &mut self,
        shader_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        match self {
            AnyRenderer::Vulkan(r) => r.init_sky_pipeline(shader_path),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.init_sky_pipeline(shader_path),
        }
    }

    pub fn init_tonemap_pipeline(
        &mut self,
        shader_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        match self {
            AnyRenderer::Vulkan(r) => r.init_tonemap_pipeline(shader_path),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.init_tonemap_pipeline(shader_path),
        }
    }

    pub fn set_viewport_bindless_slot(&mut self, slot: u32) {
        match self {
            AnyRenderer::Vulkan(r) => r.set_viewport_bindless_slot(slot),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.set_viewport_bindless_slot(slot),
        }
    }

    pub fn init_depth_prepass_pipeline(
        &mut self,
        shader_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        match self {
            AnyRenderer::Vulkan(r) => r.init_depth_prepass_pipeline(shader_path),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.init_depth_prepass_pipeline(shader_path),
        }
    }

    pub fn init_depth_prepass_skinned_pipeline(
        &mut self,
        shader_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        match self {
            AnyRenderer::Vulkan(r) => r.init_depth_prepass_skinned_pipeline(shader_path),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.init_depth_prepass_skinned_pipeline(shader_path),
        }
    }

    pub fn init_outline_pipelines(
        &mut self,
        stencil_mark_path: &std::path::Path,
        stencil_mark_skinned_path: &std::path::Path,
        outline_draw_path: &std::path::Path,
        outline_draw_skinned_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        match self {
            AnyRenderer::Vulkan(r) => r.init_outline_pipelines(
                stencil_mark_path,
                stencil_mark_skinned_path,
                outline_draw_path,
                outline_draw_skinned_path,
            ),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.init_outline_pipelines(
                stencil_mark_path,
                stencil_mark_skinned_path,
                outline_draw_path,
                outline_draw_skinned_path,
            ),
        }
    }

    pub fn init_picking_pipeline(
        &mut self,
        shader_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        match self {
            AnyRenderer::Vulkan(r) => r.init_picking_pipeline(shader_path),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.init_picking_pipeline(shader_path),
        }
    }

    pub fn init_picking_skinned_pipeline(
        &mut self,
        shader_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        match self {
            AnyRenderer::Vulkan(r) => r.init_picking_skinned_pipeline(shader_path),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.init_picking_skinned_pipeline(shader_path),
        }
    }

    // --- Metal-specific methods (take Metal types, not in trait) ---

    #[cfg(target_os = "macos")]
    pub fn set_geometry_hdr_view(
        &mut self,
        view: crate::metal::texture::MetalTextureView,
        bindless_slot: u32,
    ) {
        match self {
            AnyRenderer::Vulkan(_) => {}
            AnyRenderer::Metal(r) => r.set_geometry_hdr_view(view, bindless_slot),
        }
    }

    #[cfg(target_os = "macos")]
    pub fn set_tonemap_output_view(&mut self, view: crate::metal::texture::MetalTextureView) {
        match self {
            AnyRenderer::Vulkan(_) => {}
            AnyRenderer::Metal(r) => r.set_tonemap_output_view(view),
        }
    }

    pub fn queue_metal_picking_readback(
        &mut self,
        frame: usize,
        x: u32,
        y: u32,
    ) -> Result<(), RendererError> {
        match self {
            AnyRenderer::Vulkan(_) => {
                Err(RendererError::InvalidOperation("Not Metal backend".into()))
            }
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.queue_picking_readback(frame, x, y),
        }
    }

    pub fn check_metal_picking_readback(&mut self) -> Option<(usize, u32)> {
        match self {
            AnyRenderer::Vulkan(_) => None,
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.check_picking_readback(),
        }
    }

    pub fn has_pending_metal_picking_readback(&self) -> bool {
        match self {
            AnyRenderer::Vulkan(_) => false,
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.has_pending_picking_readback(),
        }
    }
}
