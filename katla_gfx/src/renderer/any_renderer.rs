//! Enum-based renderer dispatch for dynamic backend selection.
//!
//! `AnyRenderer` wraps `VulkanRenderer` and `MetalRenderer` behind a single
//! enum that implements `GpuRenderer`. This allows both backends to compile
//! side-by-side and be selected at runtime.

use crate::error::RendererError;
use crate::handle::{BufferHandle, MaterialHandle, MeshHandle, SkeletonHandle, TextureHandle};
use crate::render_graph::BufferDesc;
use crate::renderer::gpu_renderer::GpuRenderer;
use crate::renderer::pipeline_descriptor::PipelineDescriptor;
use crate::renderer::registry::PrimitiveTopology;
use crate::renderer::types::DrawList;
use crate::texture::TextureDescriptor;

#[cfg(target_os = "macos")]
use crate::metal::metal_renderer::MetalRenderer;
use crate::renderer::VulkanRenderer;

/// Renderer backend that wraps both Vulkan and Metal behind a single type.
///
/// Implements `GpuRenderer` by delegating to the active variant.
/// Backend-specific methods are available via `as_vulkan()` / `as_metal()`.
#[allow(clippy::large_enum_variant)]
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
        size: crate::Size2D,
        validation_mode: crate::error::ValidationMode,
        app_name: std::ffi::CString,
        engine_name: std::ffi::CString,
    ) -> Result<Self, RendererError> {
        Ok(AnyRenderer::Vulkan(VulkanRenderer::init(
            display,
            window,
            size,
            validation_mode,
            app_name,
            engine_name,
        )?))
    }

    /// Create a Vulkan renderer with offscreen targets and no window surface.
    pub fn new_vulkan_headless(
        width: u32,
        height: u32,
        validation_mode: crate::error::ValidationMode,
        app_name: std::ffi::CString,
        engine_name: std::ffi::CString,
    ) -> Result<Self, RendererError> {
        Ok(Self::Vulkan(VulkanRenderer::init_headless(
            width,
            height,
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

    /// Create a new Metal renderer for headless (offscreen) rendering.
    #[cfg(target_os = "macos")]
    pub fn new_metal_headless(
        width: u32,
        height: u32,
        validation_mode: crate::error::ValidationMode,
        app_name: std::ffi::CString,
        engine_name: std::ffi::CString,
    ) -> Result<Self, RendererError> {
        Ok(AnyRenderer::Metal(MetalRenderer::init_headless(
            width,
            height,
            validation_mode,
            app_name,
            engine_name,
        )?))
    }

    /// Get the Metal device (macOS only).
    #[cfg(target_os = "macos")]
    pub fn metal_device(&self) -> &objc2::runtime::ProtocolObject<dyn objc2_metal::MTLDevice> {
        match self {
            AnyRenderer::Vulkan(_) => panic!("metal_device called on Vulkan backend"),
            AnyRenderer::Metal(r) => &r.context.device,
        }
    }

    /// Set the headless offscreen texture as the drawable (macOS only).
    #[cfg(target_os = "macos")]
    pub fn set_headless_drawable(
        &mut self,
        texture: objc2::rc::Retained<objc2::runtime::ProtocolObject<dyn objc2_metal::MTLTexture>>,
    ) {
        match self {
            AnyRenderer::Vulkan(_) => panic!("set_headless_drawable called on Vulkan backend"),
            AnyRenderer::Metal(r) => r.set_headless_drawable(texture),
        }
    }

    /// Take back the headless texture for readback (macOS only).
    #[cfg(target_os = "macos")]
    pub fn take_headless_texture(
        &mut self,
    ) -> Option<objc2::rc::Retained<objc2::runtime::ProtocolObject<dyn objc2_metal::MTLTexture>>>
    {
        match self {
            AnyRenderer::Vulkan(_) => None,
            AnyRenderer::Metal(r) => r.take_headless_texture(),
        }
    }
}

impl GpuRenderer for AnyRenderer {
    fn acquire_frame(
        &mut self,
    ) -> Result<crate::renderer::frame_scope::FrameAcquisition, RendererError> {
        match self {
            AnyRenderer::Vulkan(r) => crate::renderer::frame_lifecycle::acquire_frame(r),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => crate::metal::frame_lifecycle::acquire_frame(r),
        }
    }

    fn execute_draw_calls(
        &mut self,
        frame: &crate::renderer::frame_scope::FrameToken,
        draw_list: &DrawList,
    ) -> Result<(), RendererError> {
        match self {
            AnyRenderer::Vulkan(r) => GpuRenderer::execute_draw_calls(r, frame, draw_list),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => GpuRenderer::execute_draw_calls(r, frame, draw_list),
        }
    }

    fn present(
        &mut self,
        frame: crate::renderer::frame_scope::FrameToken,
    ) -> Result<crate::renderer::frame_scope::PresentOutcome, RendererError> {
        match self {
            AnyRenderer::Vulkan(r) => GpuRenderer::present(r, frame),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => GpuRenderer::present(r, frame),
        }
    }

    fn abort(
        &mut self,
        frame: crate::renderer::frame_scope::FrameToken,
    ) -> Result<(), RendererError> {
        match self {
            AnyRenderer::Vulkan(r) => GpuRenderer::abort(r, frame),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => GpuRenderer::abort(r, frame),
        }
    }

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

    fn create_buffer(&mut self, desc: BufferDesc) -> Result<BufferHandle, RendererError> {
        match self {
            AnyRenderer::Vulkan(r) => GpuRenderer::create_buffer(r, desc),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => GpuRenderer::create_buffer(r, desc),
        }
    }

    fn buffer_descriptor(&self, handle: BufferHandle) -> Option<BufferDesc> {
        match self {
            Self::Vulkan(renderer) => GpuRenderer::buffer_descriptor(renderer, handle),
            #[cfg(target_os = "macos")]
            Self::Metal(renderer) => GpuRenderer::buffer_descriptor(renderer, handle),
        }
    }

    fn capture_submission_snapshot(
        &self,
    ) -> Option<crate::render_graph::capture::CapturedSubmission> {
        match self {
            Self::Vulkan(renderer) => GpuRenderer::capture_submission_snapshot(renderer),
            #[cfg(target_os = "macos")]
            Self::Metal(renderer) => GpuRenderer::capture_submission_snapshot(renderer),
        }
    }

    fn create_buffer_with_data(
        &mut self,
        desc: BufferDesc,
        data: &[u8],
    ) -> Result<BufferHandle, RendererError> {
        match self {
            Self::Vulkan(renderer) => GpuRenderer::create_buffer_with_data(renderer, desc, data),
            #[cfg(target_os = "macos")]
            Self::Metal(renderer) => GpuRenderer::create_buffer_with_data(renderer, desc, data),
        }
    }

    fn write_buffer(
        &mut self,
        frame: &crate::renderer::frame_scope::FrameToken,
        handle: BufferHandle,
        offset: u64,
        data: &[u8],
    ) -> Result<(), RendererError> {
        match self {
            Self::Vulkan(renderer) => {
                GpuRenderer::write_buffer(renderer, frame, handle, offset, data)
            }
            #[cfg(target_os = "macos")]
            Self::Metal(renderer) => {
                GpuRenderer::write_buffer(renderer, frame, handle, offset, data)
            }
        }
    }

    fn read_buffer_completed(
        &mut self,
        handle: BufferHandle,
        range: crate::render_graph::BufferByteRange,
    ) -> Result<Option<Vec<u8>>, RendererError> {
        match self {
            Self::Vulkan(renderer) => GpuRenderer::read_buffer_completed(renderer, handle, range),
            #[cfg(target_os = "macos")]
            Self::Metal(renderer) => GpuRenderer::read_buffer_completed(renderer, handle, range),
        }
    }

    fn graph_texture_source(
        &self,
        resource: crate::render_graph::ResourceId,
    ) -> Option<super::texture_readback::GraphTextureSource> {
        match self {
            Self::Vulkan(renderer) => GpuRenderer::graph_texture_source(renderer, resource),
            #[cfg(target_os = "macos")]
            Self::Metal(renderer) => GpuRenderer::graph_texture_source(renderer, resource),
        }
    }

    fn queue_texture_readback(
        &mut self,
        source: super::texture_readback::GraphTextureSource,
        region: super::texture_readback::TextureReadbackRegion,
    ) -> Result<super::texture_readback::TextureReadbackTicket, RendererError> {
        match self {
            Self::Vulkan(renderer) => GpuRenderer::queue_texture_readback(renderer, source, region),
            #[cfg(target_os = "macos")]
            Self::Metal(renderer) => GpuRenderer::queue_texture_readback(renderer, source, region),
        }
    }

    fn poll_texture_readback(
        &mut self,
        ticket: super::texture_readback::TextureReadbackTicket,
    ) -> Result<Option<super::texture_readback::TextureReadbackData>, RendererError> {
        match self {
            Self::Vulkan(renderer) => GpuRenderer::poll_texture_readback(renderer, ticket),
            #[cfg(target_os = "macos")]
            Self::Metal(renderer) => GpuRenderer::poll_texture_readback(renderer, ticket),
        }
    }

    fn frame_slot_count(&self) -> usize {
        match self {
            Self::Vulkan(renderer) => GpuRenderer::frame_slot_count(renderer),
            #[cfg(target_os = "macos")]
            Self::Metal(renderer) => GpuRenderer::frame_slot_count(renderer),
        }
    }

    fn skeleton_buffer_handle(
        &mut self,
        frame: &crate::renderer::frame_scope::FrameToken,
        skeleton: SkeletonHandle,
    ) -> Result<BufferHandle, RendererError> {
        match self {
            Self::Vulkan(renderer) => {
                GpuRenderer::skeleton_buffer_handle(renderer, frame, skeleton)
            }
            #[cfg(target_os = "macos")]
            Self::Metal(renderer) => GpuRenderer::skeleton_buffer_handle(renderer, frame, skeleton),
        }
    }

    fn destroy_buffer(&mut self, handle: BufferHandle) -> Result<(), RendererError> {
        match self {
            AnyRenderer::Vulkan(r) => GpuRenderer::destroy_buffer(r, handle),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => GpuRenderer::destroy_buffer(r, handle),
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

    fn supports_feature(&self, feature: crate::renderer::features::RendererFeature) -> bool {
        match self {
            AnyRenderer::Vulkan(r) => r.supports_feature(feature),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.supports_feature(feature),
        }
    }

    fn create_mesh<T, U>(
        &mut self,
        vertices: &[T],
        indices: &[U],
        topology: PrimitiveTopology,
    ) -> Result<MeshHandle, RendererError>
    where
        T: crate::vertex::Vertex,
        U: crate::renderer::registry::MeshIndexElement,
    {
        match self {
            AnyRenderer::Vulkan(r) => r.create_mesh(vertices, indices, topology),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.create_mesh(vertices, indices, topology),
        }
    }

    fn mesh_index_format(&self, mesh: MeshHandle) -> Option<crate::backend::command::IndexType> {
        match self {
            AnyRenderer::Vulkan(r) => r.mesh_index_format(mesh),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.mesh_index_format(mesh),
        }
    }

    fn mesh_vertex_count(&self, mesh: MeshHandle) -> Option<u32> {
        match self {
            AnyRenderer::Vulkan(r) => r.mesh_vertex_count(mesh),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.mesh_vertex_count(mesh),
        }
    }

    fn mesh_index_count(&self, mesh: MeshHandle) -> Option<u32> {
        match self {
            AnyRenderer::Vulkan(r) => r.mesh_index_count(mesh),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.mesh_index_count(mesh),
        }
    }

    fn create_mesh_dynamic(
        &mut self,
        descriptor: &crate::renderer::registry::MeshDescriptor,
        vertex_data: &[u8],
        indices: &[u32],
    ) -> Result<MeshHandle, RendererError> {
        match self {
            AnyRenderer::Vulkan(r) => r.create_mesh_dynamic(descriptor, vertex_data, indices),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.create_mesh_dynamic(descriptor, vertex_data, indices),
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

    fn create_texture(
        &mut self,
        desc: &TextureDescriptor,
        data: &[u8],
    ) -> Result<TextureHandle, RendererError> {
        match self {
            AnyRenderer::Vulkan(r) => r.create_texture(desc, data),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.create_texture(desc, data),
        }
    }

    fn create_texture_solid(&mut self, color: [u8; 4]) -> Result<TextureHandle, RendererError> {
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

    fn update_texture_region(
        &mut self,
        handle: TextureHandle,
        region: crate::texture::TextureUploadRegion,
        data: &[u8],
    ) -> Result<(), RendererError> {
        match self {
            AnyRenderer::Vulkan(r) => r.update_texture_region(handle, region, data),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.update_texture_region(handle, region, data),
        }
    }

    fn pending_texture_uploads(&self) -> Vec<(TextureHandle, crate::texture::TextureUploadRegion)> {
        match self {
            AnyRenderer::Vulkan(r) => r.pending_texture_uploads(),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.pending_texture_uploads(),
        }
    }

    fn texture_upload_metrics(&self) -> Option<crate::texture::TextureUploadMetrics> {
        match self {
            AnyRenderer::Vulkan(r) => r.texture_upload_metrics(),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.texture_upload_metrics(),
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
        descriptor: &PipelineDescriptor,
    ) -> Result<MaterialHandle, RendererError> {
        match self {
            AnyRenderer::Vulkan(r) => GpuRenderer::compile_material(r, descriptor),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => GpuRenderer::compile_material(r, descriptor),
        }
    }

    fn set_material_textures(
        &mut self,
        material: MaterialHandle,
        textures: crate::renderer::registry::MaterialTextures,
    ) {
        match self {
            AnyRenderer::Vulkan(r) => r.set_material_textures(material, textures),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.set_material_textures(material, textures),
        }
    }

    fn recompile_materials_for_shader(&mut self, shader_path: &std::path::Path) -> usize {
        match self {
            AnyRenderer::Vulkan(r) => r.recompile_materials_for_shader(shader_path),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.recompile_materials_for_shader(shader_path),
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

    fn resize(&mut self, width: u32, height: u32) -> Result<(), RendererError> {
        match self {
            AnyRenderer::Vulkan(r) => r.resize(width, height),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.resize(width, height),
        }
    }

    fn create_skeleton(&mut self, joint_count: usize) -> Result<SkeletonHandle, RendererError> {
        match self {
            AnyRenderer::Vulkan(r) => r.create_skeleton(joint_count),
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => r.create_skeleton(joint_count),
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
    /// Execute the frame graph for an open frame. The closure receives an
    /// `AnyFrame` for submitting draw lists to passes. A failure poisons the
    /// frame so `present` cannot submit half-encoded work.
    pub fn render<F>(
        &mut self,
        frame: &crate::renderer::frame_scope::FrameToken,
        frame_graph: &mut crate::render_graph::any_frame_graph::AnyFrameGraph,
        f: F,
    ) -> Result<(), RendererError>
    where
        F: FnOnce(&mut crate::render_graph::any_frame::AnyFrame<'_, '_>),
    {
        match self {
            AnyRenderer::Vulkan(r) => {
                let fg = frame_graph.as_vulkan_mut();
                r.render(frame, fg, |graph_frame| {
                    let mut any_frame =
                        crate::render_graph::any_frame::AnyFrame::Vulkan(graph_frame);
                    f(&mut any_frame);
                })
            }
            #[cfg(target_os = "macos")]
            AnyRenderer::Metal(r) => {
                let fg = frame_graph.as_metal_mut();
                r.render(frame, fg, |graph_frame| {
                    let mut any_frame =
                        crate::render_graph::any_frame::AnyFrame::Metal(graph_frame);
                    f(&mut any_frame);
                })
            }
        }
    }

    // --- Metal-specific methods ---

    // --- Pipeline init methods (delegated to GpuRenderer trait) ---

    // --- Metal-specific methods (take Metal types, not in trait) ---

    /// Create an offscreen BGRA8 texture suitable for headless rendering and CPU readback.
    ///
    /// Returns an opaque Metal texture handle that can be passed to `set_headless_drawable`.
    #[cfg(target_os = "macos")]
    pub fn create_offscreen_texture(
        &self,
        width: u32,
        height: u32,
    ) -> objc2::rc::Retained<objc2::runtime::ProtocolObject<dyn objc2_metal::MTLTexture>> {
        use crate::texture::{ImageFormat, TextureDescriptor, TextureUsage};

        match self {
            AnyRenderer::Vulkan(_) => panic!("create_offscreen_texture called on Vulkan backend"),
            AnyRenderer::Metal(r) => {
                let desc = TextureDescriptor::new(width, height, ImageFormat::B8G8R8A8Srgb)
                    .with_usage(TextureUsage::COLOR_ATTACHMENT | TextureUsage::SAMPLED);
                let (tex, _view) = r
                    .context
                    .create_texture_shared(&desc)
                    .expect("Failed to create offscreen texture");
                tex.inner
            }
        }
    }

    /// Read back pixels from a Shared-storage Metal texture as BGRA8.
    ///
    /// Returns raw BGRA pixel data.
    #[cfg(target_os = "macos")]
    pub fn readback_bgra_texture(
        texture: &objc2::runtime::ProtocolObject<dyn objc2_metal::MTLTexture>,
        width: u32,
        height: u32,
    ) -> Vec<u8> {
        use objc2_metal::MTLTexture;
        let bytes_per_row = width as usize * 4;
        let mut data = vec![0u8; bytes_per_row * height as usize];
        let region = objc2_metal::MTLRegion {
            origin: objc2_metal::MTLOrigin { x: 0, y: 0, z: 0 },
            size: objc2_metal::MTLSize {
                width: width as usize,
                height: height as usize,
                depth: 1,
            },
        };
        unsafe {
            texture.getBytes_bytesPerRow_fromRegion_mipmapLevel(
                std::ptr::NonNull::new(data.as_mut_ptr() as *mut std::ffi::c_void).unwrap(),
                bytes_per_row,
                region,
                0,
            );
        }
        data
    }
}
