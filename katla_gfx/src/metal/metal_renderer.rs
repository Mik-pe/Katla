//! Metal backend implementation of the GpuRenderer trait.
//!
//! MetalRenderer wraps MetalContext and provides the same rendering API as
//! VulkanRenderer, allowing katla_app to be generic over the graphics backend.

use std::mem;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::{MTLCommandBuffer, MTLDevice, MTLTexture};

use crate::backend::command::GpuCommandBuffer;
use crate::backend::resource::GpuBuffer;
use crate::error::RendererError;
use crate::handle::{MaterialHandle, MeshHandle, ResourceStorage, SkeletonHandle, TextureHandle};

use crate::renderer::MAX_OBJECTS_PER_FRAME;
use crate::renderer::gpu_renderer::GpuRenderer;
use crate::renderer::pipeline_kind::PipelineKind;
use crate::renderer::types::{DrawList, FrameUniforms};
use crate::size::Size2D;
use crate::texture::{ImageFormat, TextureDescriptor};
use crate::viewport::Viewport;

use super::animation::MetalAnimationSystem;
use super::argument_buffer::MetalBindlessTextureManager;
use super::buffer::MetalBuffer;
use super::context::MetalContext;
use super::depth_prepass::MetalDepthPrepass;
use super::light_culling::MetalLightCulling;
use super::outline::MetalOutlineSubsystem;
use super::particle::MetalParticleSubsystem;
use super::picking::MetalPickingSubsystem;
use super::shadow::MetalShadowSubsystem;
use super::texture::MetalTextureView;
use super::ui_renderer::MetalUIRenderer;

pub(crate) const OBJECT_UNIFORM_SIZE: u64 = 16 * 4 + 4 * 4 + 4 * 4 + 4 * 4;
pub(crate) const FRAMES_IN_FLIGHT: usize = 2;

/// Map the monotonically increasing frame index to the slot that owns mutable GPU data.
#[inline]
pub(crate) const fn frame_slot(frame_index: u32) -> usize {
    (frame_index as usize) % FRAMES_IN_FLIGHT
}

fn validate_object_buffer_capacity(
    draw_list: &DrawList,
    buffer_size: usize,
) -> Result<(), RendererError> {
    let Some(max_instance_index) = draw_list.draws.iter().map(|draw| draw.instance_index).max()
    else {
        return Ok(());
    };

    let required_size = (max_instance_index as usize)
        .checked_add(1)
        .and_then(|count| count.checked_mul(OBJECT_UNIFORM_SIZE as usize))
        .ok_or_else(|| {
            RendererError::InvalidOperation(format!(
                "Object uniform size overflow for instance index {max_instance_index}"
            ))
        })?;

    if required_size > buffer_size {
        let capacity = buffer_size / OBJECT_UNIFORM_SIZE as usize;
        return Err(RendererError::InvalidOperation(format!(
            "Draw list requires object instance index {max_instance_index}, but the Metal object buffer only has {capacity} slots"
        )));
    }

    Ok(())
}

#[cfg(test)]
mod object_buffer_capacity_tests {
    use super::*;
    use crate::renderer::types::DrawCall;

    fn draw_list(indices: &[u32]) -> DrawList {
        let mut draws = DrawList::new();
        for &index in indices {
            draws.push(
                DrawCall::new(MeshHandle::NONE, MaterialHandle::NONE).with_instance_index(index),
            );
        }
        draws
    }

    #[test]
    fn empty_draw_list_needs_no_object_storage() {
        assert!(validate_object_buffer_capacity(&DrawList::new(), 0).is_ok());
    }

    #[test]
    fn highest_instance_index_must_fit_the_uploaded_buffer() {
        let two_slots = OBJECT_UNIFORM_SIZE as usize * 2;
        assert!(validate_object_buffer_capacity(&draw_list(&[0, 1]), two_slots).is_ok());

        let error = validate_object_buffer_capacity(&draw_list(&[0, 2]), two_slots)
            .expect_err("instance index 2 must not fit a two-slot object buffer");
        assert!(error.to_string().contains("instance index 2"));
        assert!(error.to_string().contains("2 slots"));
    }
}

/// A mesh stored in Metal GPU buffers.
pub(crate) struct MetalMesh {
    pub(crate) vertex_buffer: MetalBuffer,
    pub(crate) index_buffer: MetalBuffer,
    pub(crate) index_count: u32,
}

/// A material (pipeline state + texture indices).
pub(crate) struct MetalMaterial {
    pub(crate) pipeline: Option<super::pipeline::MetalGraphicsPipeline>,
    pub(crate) texture_indices: [u32; 4],
    pub(crate) shader_path: Option<String>,
    pub(crate) vertex_type: Option<String>,
}

/// A texture stored with its bindless slot.
pub(crate) struct MetalTextureEntry {
    pub(crate) texture: super::texture::MetalTexture,
    pub(crate) _view: MetalTextureView,
    pub(crate) bindless_slot: Option<u32>,
}

pub(crate) fn read_shader(path: &str) -> Result<String, RendererError> {
    let resolved_path = if std::path::Path::new(path).exists() {
        std::path::PathBuf::from(path)
    } else {
        let mut found = None;
        for candidate in [
            format!("resources/shaders/{path}"),
            format!("../resources/shaders/{path}"),
            format!("../../resources/shaders/{path}"),
        ] {
            if std::path::Path::new(&candidate).exists() {
                found = Some(std::path::PathBuf::from(candidate));
                break;
            }
        }
        found
            .ok_or_else(|| RendererError::InvalidOperation(format!("Shader not found: {}", path)))?
    };

    let raw = std::fs::read_to_string(&resolved_path).map_err(|e| {
        RendererError::InvalidOperation(format!(
            "Failed to read shader '{}': {}",
            resolved_path.display(),
            e
        ))
    })?;
    resolve_wgsl_includes(&raw, &resolved_path)
}

pub(crate) fn resolve_wgsl_includes(
    source: &str,
    file_path: &std::path::Path,
) -> Result<String, RendererError> {
    let mut result = String::new();
    let base_dir = file_path.parent().unwrap_or(std::path::Path::new("."));
    let shader_root = {
        let mut p = base_dir;
        while !p.join("common").exists() && p.parent().is_some() {
            p = p.parent().unwrap();
        }
        p.to_path_buf()
    };

    for line in source.lines() {
        let trimmed = line.trim();
        if let Some(path_str) = trimmed
            .strip_prefix("//include ")
            .or_else(|| trimmed.strip_prefix("#include "))
        {
            let path_str = path_str.trim();
            let include_rel = path_str
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix('"'))
                .or_else(|| path_str.strip_prefix('<').and_then(|s| s.strip_suffix('>')))
                .unwrap_or(path_str);
            let include_path = base_dir
                .join(include_rel)
                .exists()
                .then(|| base_dir.join(include_rel))
                .or_else(|| {
                    let p = shader_root.join(include_rel);
                    p.exists().then_some(p)
                })
                .or_else(|| {
                    let p = shader_root.join("common").join(include_rel);
                    p.exists().then_some(p)
                })
                .ok_or_else(|| {
                    RendererError::InvalidOperation(format!("Include not found: {}", include_rel))
                })?;
            let include_source = std::fs::read_to_string(&include_path).map_err(|e| {
                RendererError::InvalidOperation(format!(
                    "Failed to read include '{}': {}",
                    include_path.display(),
                    e
                ))
            })?;
            let expanded = resolve_wgsl_includes(&include_source, &include_path)?;
            result.push_str(&expanded);
        } else {
            result.push_str(line);
        }
        result.push('\n');
    }
    Ok(result)
}

pub struct MetalRenderer {
    pub(crate) context: MetalContext,
    pub(crate) frame_uniforms: FrameUniforms,
    pub(crate) frame_uniform_buffers: [Option<MetalBuffer>; FRAMES_IN_FLIGHT],
    pub(crate) object_storage_buffers: [Option<MetalBuffer>; FRAMES_IN_FLIGHT],
    pub(crate) current_drawable_texture: Option<Retained<ProtocolObject<dyn MTLTexture>>>,
    pub(crate) drawable_texture_view: Option<MetalTextureView>,
    pub(crate) frame_index: u32,
    pub(crate) meshes: ResourceStorage<MetalMesh>,
    pub(crate) materials: ResourceStorage<MetalMaterial>,
    pub(crate) textures: ResourceStorage<MetalTextureEntry>,
    pub(crate) skeletons: ResourceStorage<MetalBuffer>,
    pub(crate) viewports: Vec<Viewport>,
    pub(crate) bindless_manager: MetalBindlessTextureManager,
    pub(crate) default_texture: Option<TextureHandle>,
    pub(crate) default_normal_texture: Option<TextureHandle>,
    pub(crate) default_mr_texture: Option<TextureHandle>,
    pub(crate) default_material: Option<MaterialHandle>,
    pub(crate) size: Size2D,
    pub(crate) drawable_size: Size2D,
    pub(crate) ui_font_atlas: Option<TextureHandle>,
    pub(crate) last_command_buffer: Option<Retained<ProtocolObject<dyn MTLCommandBuffer>>>,
    pub(crate) texture_uploads: super::texture_upload::TextureUploadQueue,
    pub(crate) pending_draw_list: Option<DrawList>,
    pub(crate) light_culling: Option<MetalLightCulling>,
    pub(crate) ui_renderer: MetalUIRenderer,
    pub(crate) animation_system: Option<MetalAnimationSystem>,
    pub(crate) particle_system: Option<MetalParticleSubsystem>,
    pub(crate) pending_ui_draw_list: Option<crate::renderer::types::UIDrawList>,
    pub(crate) shadow: MetalShadowSubsystem,
    pub(crate) depth_prepass: MetalDepthPrepass,
    pub(crate) outline: MetalOutlineSubsystem,
    pub(crate) picking: MetalPickingSubsystem,
    pub(crate) depth_texture_view: Option<MetalTextureView>,
    pub(crate) hdr_color_view: Option<MetalTextureView>,
    pub(crate) depth_stencil_view: Option<MetalTextureView>,
    pub(crate) shared_sampler: Option<super::sampler::MetalSamplerState>,
    pub(crate) shadow_cascade_buffer: Option<MetalBuffer>,
    pub(crate) shadow_sampler: Option<super::sampler::MetalSamplerState>,
    pub(crate) buffer_sizes_buffer: Option<MetalBuffer>,
    #[expect(dead_code)]
    pub(crate) scene_color_view: Option<MetalTextureView>,
    pub(crate) viewport_bindless_slot: Option<u32>,
    pub(crate) tonemap_output_view: Option<MetalTextureView>,
    pub(crate) geometry_hdr_view: Option<MetalTextureView>,
    pub(crate) geometry_hdr_bindless_slot: Option<u32>,
    pub(crate) tonemap_pipeline: Option<super::pipeline::MetalGraphicsPipeline>,
    pub(crate) sky_pipeline: Option<super::pipeline::MetalGraphicsPipeline>,
    pub(crate) dummy_vertex_buffer: Option<MetalBuffer>,
    pub(crate) tonemap_fence: Option<Retained<ProtocolObject<dyn objc2_metal::MTLFence>>>,
    pub(crate) capabilities: crate::renderer::types::GpuCapabilities,
    pub(crate) timestamp_queries: Option<super::timestamp_queries::MetalTimestampQueries>,
    pub(crate) viewport_panel_rect: Option<crate::rect::Rect>,
}

impl MetalRenderer {
    pub fn init(
        display: &dyn raw_window_handle::HasDisplayHandle,
        window: &dyn raw_window_handle::HasWindowHandle,
        validation_mode: crate::error::ValidationMode,
        _app_name: std::ffi::CString,
        _engine_name: std::ffi::CString,
    ) -> Result<Self, RendererError> {
        if validation_mode.is_enabled() {
            log::info!("Metal validation requested");
            let already_set = std::env::var("METAL_DEVICE_WRAPPER_TYPE").is_ok();
            if !already_set {
                log::warn!(
                    "METAL_DEVICE_WRAPPER_TYPE not set before process launch. \
                     Metal validation requires env vars to be set externally. \
                     Run: METAL_DEVICE_WRAPPER_TYPE=1 cargo run -- -s"
                );
            }
        }

        let context = MetalContext::init(window, display)?;
        let mut renderer = Self::new(context)?;

        let ds = renderer.context.surface.layer.drawableSize();
        let dw = ds.width as u32;
        let dh = ds.height as u32;
        if dw > 0 && dh > 0 {
            renderer.drawable_size = Size2D::new(dw, dh);
            renderer.size = Size2D::new(dw, dh);
            renderer.recreate_render_targets(dw, dh);
            renderer.resize_light_culling(dw, dh);
        }

        Ok(renderer)
    }

    /// Create a headless Metal renderer without a window.
    ///
    /// Uses an offscreen CAMetalLayer at the specified resolution.
    /// Suitable for automated rendering and screenshot capture.
    pub fn init_headless(
        width: u32,
        height: u32,
        _validation_mode: crate::error::ValidationMode,
        _app_name: std::ffi::CString,
        _engine_name: std::ffi::CString,
    ) -> Result<Self, RendererError> {
        let context = MetalContext::init_headless_with_size(width, height)?;
        let mut renderer = Self::new(context)?;

        renderer.drawable_size = Size2D::new(width, height);
        renderer.size = Size2D::new(width, height);
        renderer.recreate_render_targets(width, height);
        renderer.resize_light_culling(width, height);

        Ok(renderer)
    }

    /// Set the offscreen texture as the current drawable for headless rendering.
    ///
    /// This replaces the normal `acquire_next_drawable()` from CAMetalLayer.
    /// The texture must have Shared storage mode for CPU readback.
    pub fn set_headless_drawable(&mut self, texture: Retained<ProtocolObject<dyn MTLTexture>>) {
        self.current_drawable_texture = Some(texture.clone());
        self.drawable_texture_view = Some(super::texture::MetalTextureView::new(
            texture,
            super::texture::MetalTexture::new(
                self.current_drawable_texture.clone().unwrap(),
                ImageFormat::B8G8R8A8Srgb,
            ),
        ));
    }

    /// Take back the offscreen texture after rendering (for readback).
    pub fn take_headless_texture(&mut self) -> Option<Retained<ProtocolObject<dyn MTLTexture>>> {
        self.current_drawable_texture.take()
    }

    pub(crate) fn new(context: MetalContext) -> Result<Self, RendererError> {
        let features = context.detect_features();
        let bindless_manager = MetalBindlessTextureManager::new(features.max_bindless_textures)?;

        let mut renderer = Self {
            context,
            frame_uniforms: FrameUniforms::default(),
            frame_uniform_buffers: [const { None }; FRAMES_IN_FLIGHT],
            object_storage_buffers: [const { None }; FRAMES_IN_FLIGHT],
            current_drawable_texture: None,
            drawable_texture_view: None,
            frame_index: 0,
            meshes: ResourceStorage::new(),
            materials: ResourceStorage::new(),
            textures: ResourceStorage::new(),
            skeletons: ResourceStorage::new(),
            viewports: Vec::new(),
            bindless_manager,
            default_texture: None,
            default_normal_texture: None,
            default_mr_texture: None,
            default_material: None,
            size: Size2D::default(),
            drawable_size: Size2D::default(),
            ui_font_atlas: None,
            last_command_buffer: None,
            texture_uploads: super::texture_upload::TextureUploadQueue::default(),
            pending_draw_list: None,
            light_culling: None,
            ui_renderer: MetalUIRenderer::new(),
            animation_system: None,
            particle_system: None,
            pending_ui_draw_list: None,
            shadow: MetalShadowSubsystem::new(),
            depth_prepass: MetalDepthPrepass::new(),
            outline: MetalOutlineSubsystem::new(),
            picking: MetalPickingSubsystem::new(),
            depth_texture_view: None,
            hdr_color_view: None,
            depth_stencil_view: None,
            shared_sampler: None,
            shadow_cascade_buffer: None,
            shadow_sampler: None,
            buffer_sizes_buffer: None,
            scene_color_view: None,
            viewport_bindless_slot: None,
            tonemap_output_view: None,
            geometry_hdr_view: None,
            geometry_hdr_bindless_slot: None,
            tonemap_pipeline: None,
            sky_pipeline: None,
            dummy_vertex_buffer: None,
            tonemap_fence: None,
            capabilities: {
                use crate::renderer::types::{GpuCapabilities, GpuVendor};
                GpuCapabilities {
                    max_texture_size: 16384,
                    max_bindless_textures: features.max_bindless_textures,
                    supports_compute: true,
                    max_frames_in_flight: FRAMES_IN_FLIGHT,
                    vendor: GpuVendor::Apple,
                    supports_light_culling: false,
                }
            },
            timestamp_queries: None,
            viewport_panel_rect: None,
        };

        let default_tex = renderer.create_texture_solid([255, 255, 255, 255]);
        renderer.default_texture = Some(default_tex);

        // Flat normal: [128,128,255,255] in UNORM = neutral tangent-space normal (0.5,0.5,1.0)
        let normal_desc = TextureDescriptor::new(1, 1, ImageFormat::R8G8B8A8Unorm);
        let default_normal = renderer.create_texture(&normal_desc, &[128, 128, 255, 255]);
        renderer.default_normal_texture = Some(default_normal);

        // Metallic-Roughness default: [255,128,0,255] in UNORM = roughness=0.5, metallic=0.0
        let mr_desc = TextureDescriptor::new(1, 1, ImageFormat::R8G8B8A8Unorm);
        let default_mr = renderer.create_texture(&mr_desc, &[255, 128, 0, 255]);
        renderer.default_mr_texture = Some(default_mr);

        // Texture registration is valid before a shader layout exists. The argument
        // buffer itself is initialized lazily from the first compiled fragment
        // function so Metal, rather than Katla, owns the concrete layout ABI.
        if let Some(entry) = renderer.textures.get(default_tex.index()) {
            renderer
                .bindless_manager
                .set_default_texture(&entry._view.inner);
        }

        renderer.tonemap_fence = renderer.context.device.newFence();

        let default_mat = MetalMaterial {
            pipeline: None,
            texture_indices: [0, 1, 2, 0],
            shader_path: None,
            vertex_type: None,
        };
        let id = renderer.materials.insert(default_mat);
        renderer.default_material = Some(MaterialHandle::new(id));

        renderer.recreate_render_targets(renderer.size.width, renderer.size.height);

        // Create shared sampler for texture sampling
        renderer.shared_sampler = Some(renderer.context.create_sampler()?);

        // Shadow cascade data buffer (ShadowFrameData for Set 4, binding 0)
        let shadow_cascade_buf = renderer.context.create_buffer(512, true)?;
        {
            let ptr = shadow_cascade_buf.map();
            unsafe {
                std::ptr::write_bytes(ptr, 0, 512);
            }
            shadow_cascade_buf.unmap();
        }
        renderer.shadow_cascade_buffer = Some(shadow_cascade_buf);

        // Shadow comparison sampler (Set 4, binding 2)
        {
            let desc = objc2_metal::MTLSamplerDescriptor::new();
            desc.setMinFilter(objc2_metal::MTLSamplerMinMagFilter::Linear);
            desc.setMagFilter(objc2_metal::MTLSamplerMinMagFilter::Linear);
            desc.setMipFilter(objc2_metal::MTLSamplerMipFilter::NotMipmapped);
            desc.setSAddressMode(objc2_metal::MTLSamplerAddressMode::ClampToEdge);
            desc.setTAddressMode(objc2_metal::MTLSamplerAddressMode::ClampToEdge);
            desc.setCompareFunction(objc2_metal::MTLCompareFunction::LessEqual);
            let sampler = renderer.context.create_sampler_with_descriptor(&desc)?;
            renderer.shadow_sampler = Some(sampler);
        }

        // Create buffer for naga's _mslBufferSizes struct at [[buffer(8)]].
        // Contains packed u32 sizes for each storage buffer with runtime arrays.
        // Layout: size1(frame_uniforms), size2(objects), size9(argument_buffer), ...
        // We use a fixed 256-byte buffer filled with large values so bounds checks pass.
        let buffer_sizes = renderer.context.create_buffer(256, true)?;
        {
            let ptr = buffer_sizes.map();
            unsafe {
                std::ptr::write_bytes(ptr, 0xFF, 256);
            }
            buffer_sizes.unmap();
        }
        renderer.buffer_sizes_buffer = Some(buffer_sizes);

        // Small dummy vertex buffer for fullscreen passes that have a vertex descriptor
        // referencing buffer index 10 but don't actually read vertex data.
        let dummy_vb = renderer.context.create_buffer(4, true)?;
        {
            let ptr = dummy_vb.map();
            unsafe {
                std::ptr::write_bytes(ptr, 0, 4);
            }
            dummy_vb.unmap();
        }
        renderer.dummy_vertex_buffer = Some(dummy_vb);

        renderer.timestamp_queries =
            super::timestamp_queries::MetalTimestampQueries::new(&renderer.context.device);
        if renderer.timestamp_queries.is_some() {
            log::info!("Metal timestamp queries initialized");
        }

        Ok(renderer)
    }

    fn ensure_uniform_buffers(&mut self) -> Result<(), RendererError> {
        let frame_idx = frame_slot(self.frame_index);
        if self.frame_uniform_buffers[frame_idx].is_none() {
            let frame_size = mem::size_of::<FrameUniforms>() as u64;
            self.frame_uniform_buffers[frame_idx] =
                Some(self.context.create_buffer(frame_size, true)?);
        }
        if self.object_storage_buffers[frame_idx].is_none() {
            let object_size = MAX_OBJECTS_PER_FRAME as u64 * OBJECT_UNIFORM_SIZE;
            self.object_storage_buffers[frame_idx] =
                Some(self.context.create_buffer(object_size, true)?);
        }
        Ok(())
    }

    pub(crate) fn current_frame_uniform_buffer(&self) -> Option<&MetalBuffer> {
        let idx = frame_slot(self.frame_index);
        self.frame_uniform_buffers[idx].as_ref()
    }

    pub(crate) fn current_object_storage_buffer(&self) -> Option<&MetalBuffer> {
        let idx = frame_slot(self.frame_index);
        self.object_storage_buffers[idx].as_ref()
    }

    /// Set the geometry HDR view and bindless slot from an external source (frame graph).
    pub fn set_geometry_hdr_view(&mut self, view: MetalTextureView, bindless_slot: u32) {
        self.geometry_hdr_view = Some(view);
        self.geometry_hdr_bindless_slot = Some(bindless_slot);
    }

    /// Set the viewport bindless slot from the frame graph.
    pub fn set_viewport_bindless_slot(&mut self, slot: u32) {
        self.viewport_bindless_slot = Some(slot);
    }

    /// Set the tonemap output target (viewport_0 LDR texture view).
    pub fn set_tonemap_output_view(&mut self, view: MetalTextureView) {
        self.tonemap_output_view = Some(view);
    }

    /// Execute the frame graph and present the frame.
    ///
    /// Acquires the drawable, dispatches light culling, collects draw lists
    /// via the frame graph closure, then renders using Metal's internal pipeline.
    pub fn render<F>(
        &mut self,
        frame_graph: &mut crate::render_graph::FrameGraph<Self>,
        f: F,
    ) -> Result<(), RendererError>
    where
        F: FnOnce(&mut crate::render_graph::Frame<'_, Self>),
    {
        self.wait_for_frame()?;

        self.begin_frame()?;

        let pending = frame_graph
            .collect_draw_lists(self, f)
            .map_err(|e| RendererError::InvalidOperation(e.to_string()))?;

        let frame_idx = self.frame_index();

        // Flush only the argument buffer slots that changed since last frame.
        self.bindless_manager.flush_argument_buffer();

        let view_matrix = self.frame_uniforms.view_matrix;
        let proj_matrix = self.frame_uniforms.proj_matrix;
        if let Some(ref lc) = self.light_culling
            && lc.light_count() > 0
        {
            self.dispatch_light_culling(&(), &view_matrix, &proj_matrix);
        }

        self.execute_metal_passes(pending, frame_graph, frame_idx)?;

        self.end_frame()?;

        Ok(())
    }

    fn execute_metal_passes(
        &mut self,
        pending: std::collections::HashMap<usize, crate::render_graph::PassExecutionData>,
        frame_graph: &crate::render_graph::FrameGraph<Self>,
        _frame_idx: usize,
    ) -> Result<(), RendererError> {
        let plan = super::execution_plan::MetalExecutionPlan::compile(frame_graph)
            .map_err(|error| RendererError::InvalidOperation(error.to_string()))?;

        self.render_frame(&plan, pending)
    }

    /// Initialize the Forward+ light culling system.
    pub fn init_light_culling(
        &mut self,
        screen_width: u32,
        screen_height: u32,
        _shader_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        let lc = MetalLightCulling::new(&self.context, screen_width, screen_height)?;
        self.light_culling = Some(lc);
        self.capabilities.supports_light_culling = true;
        Ok(())
    }

    /// Upload point light data for the current frame.
    pub fn upload_lights(&mut self, lights: &[super::light_culling::PointLightGPU]) {
        if let Some(ref mut lc) = self.light_culling {
            lc.upload_lights(lights);
        }
    }

    /// Dispatch the light culling compute pass.
    ///
    /// Call after uploading lights and before the geometry pass.
    pub fn dispatch_light_culling(
        &mut self,
        _cmd: &(), // Metal doesn't use external command buffers for this
        view_matrix: &[f32; 16],
        proj_matrix: &[f32; 16],
    ) {
        if let Some(ref mut lc) = self.light_culling {
            lc.dispatch_light_culling(&self.context, view_matrix, proj_matrix);
        }
    }

    /// Whether the light culling system is active.
    pub fn has_light_culling(&self) -> bool {
        self.light_culling.is_some()
    }

    /// Recreate light culling buffers for new screen dimensions.
    pub fn resize_light_culling(&mut self, screen_width: u32, screen_height: u32) {
        if let Some(ref mut lc) = self.light_culling
            && let Err(e) = lc.resize(&self.context, screen_width, screen_height)
        {
            log::error!("Failed to resize Metal light culling: {}", e);
        }
    }

    /// Initialize the GPU animation compute pipeline.
    pub fn init_animation_pipeline(
        &mut self,
        shader_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        let mut system = MetalAnimationSystem::new();
        system.init_pipeline_with_source(&self.context, shader_path)?;
        self.animation_system = Some(system);
        Ok(())
    }

    /// Queue a UI draw list for rendering in the next frame.
    ///
    /// The UI is rendered after geometry passes, directly to the swapchain image.
    pub fn render_ui_pass(&mut self, draw_list: crate::renderer::types::UIDrawList) {
        self.pending_ui_draw_list = Some(draw_list);
    }

    /// Create the shadow map depth texture.
    pub fn init_shadow_resources(
        &mut self,
        _shadow_atlas_view: Option<()>,
    ) -> Result<(), RendererError> {
        self.shadow.create_shadow_map(&self.context)
    }

    pub fn queue_picking_readback(
        &mut self,
        frame: usize,
        x: u32,
        y: u32,
    ) -> Result<(), RendererError> {
        self.picking
            .queue_picking_readback(&self.context, frame, x, y)
    }

    pub fn check_picking_readback(&mut self) -> Option<(usize, u32)> {
        self.picking.check_picking_readback()
    }

    pub fn has_pending_picking_readback(&self) -> bool {
        self.picking.has_pending_readback()
    }

    /// Update shadow cascade view-projection matrices.
    pub fn update_shadows(&mut self, light_direction: [f32; 3]) {
        self.shadow.update_cascades(
            &self.frame_uniforms.view_matrix,
            &self.frame_uniforms.proj_matrix,
            light_direction,
        );
    }

    /// Render the shadow pass for all cascades.
    pub fn render_shadow_pass(&mut self) -> Result<(), RendererError> {
        let Some(shadow_map) = self.shadow.shadow_map_view() else {
            return Ok(());
        };
        let Some(pipeline) = self.shadow.pipeline() else {
            return Ok(());
        };
        let Some(ref draw_list) = self.pending_draw_list else {
            return Ok(());
        };
        let Some(frame_buf) = self.current_frame_uniform_buffer() else {
            return Ok(());
        };
        let Some(object_buf) = self.current_object_storage_buffer() else {
            return Ok(());
        };
        let Some(ref shadow_buf) = self.shadow_cascade_buffer else {
            return Ok(());
        };

        let mut cmd_buffer = self.context.create_command_buffer();
        cmd_buffer.begin();
        {
            let label = objc2_foundation::NSString::from_str("shadow_pass");
            cmd_buffer.inner.setLabel(Some(&label));
        }

        super::shadow::render_cascades(
            &mut cmd_buffer,
            pipeline,
            self.shadow.pipeline_skinned(),
            Some(&self.skeletons),
            shadow_map,
            self.shadow.shadow_resolution(),
            frame_buf,
            object_buf,
            shadow_buf,
            self.buffer_sizes_buffer.as_ref(),
            self.shadow.cascade_count(),
            &self.meshes,
            &self.materials,
            draw_list,
        );

        cmd_buffer.end();
        cmd_buffer.submit(&self.context);

        Ok(())
    }

    /// Render the depth prepass.
    pub fn render_depth_prepass(&mut self) -> Result<(), RendererError> {
        let Some(pipeline) = self.depth_prepass.pipeline() else {
            return Ok(());
        };
        let Some(ref draw_list) = self.pending_draw_list else {
            return Ok(());
        };
        let Some(frame_buf) = self.current_frame_uniform_buffer() else {
            return Ok(());
        };
        let Some(object_buf) = self.current_object_storage_buffer() else {
            return Ok(());
        };
        let Some(ref depth_view) = self.depth_stencil_view else {
            return Ok(());
        };

        let width = self.size.width;
        let height = self.size.height;

        let mut cmd_buffer = self.context.create_command_buffer();
        cmd_buffer.begin();
        {
            let label = objc2_foundation::NSString::from_str("depth_prepass");
            cmd_buffer.inner.setLabel(Some(&label));
        }

        super::depth_prepass::render_depth_prepass(
            &mut cmd_buffer,
            pipeline,
            self.depth_prepass.pipeline_skinned(),
            self.depth_prepass.pipeline_billboard(),
            depth_view,
            width,
            height,
            frame_buf,
            object_buf,
            &self.meshes,
            &self.materials,
            draw_list,
            &self.skeletons,
            self.bindless_manager.argument_buffer(),
            self.shared_sampler.as_ref(),
        );

        cmd_buffer.end();
        cmd_buffer.submit(&self.context);

        Ok(())
    }

    /// Render the outline pass for selected objects.
    pub fn render_outline_pass(&mut self) -> Result<(), RendererError> {
        let Some(stencil_pipeline) = self.outline.stencil_mark_pipeline() else {
            return Ok(());
        };
        let Some(outline_pipeline) = self.outline.outline_draw_pipeline() else {
            return Ok(());
        };
        let Some(ref draw_list) = self.pending_draw_list else {
            return Ok(());
        };
        let Some(frame_buf) = self.current_frame_uniform_buffer() else {
            return Ok(());
        };
        let Some(object_buf) = self.current_object_storage_buffer() else {
            return Ok(());
        };
        let Some(ref color_view) = self.hdr_color_view else {
            return Ok(());
        };
        let Some(ref depth_view) = self.depth_stencil_view else {
            return Ok(());
        };

        let width = self.size.width;
        let height = self.size.height;

        let mut cmd_buffer = self.context.create_command_buffer();
        cmd_buffer.begin();

        super::outline::render_stencil_mark(
            &mut cmd_buffer,
            stencil_pipeline,
            self.outline.stencil_mark_skinned_pipeline(),
            color_view,
            depth_view,
            width,
            height,
            frame_buf,
            object_buf,
            &self.meshes,
            &self.materials,
            draw_list,
            &self.skeletons,
        );

        super::outline::render_outline(
            &mut cmd_buffer,
            outline_pipeline,
            self.outline.outline_draw_skinned_pipeline(),
            color_view,
            depth_view,
            width,
            height,
            frame_buf,
            object_buf,
            &self.meshes,
            &self.materials,
            draw_list,
            &self.skeletons,
        );

        cmd_buffer.end();
        cmd_buffer.submit(&self.context);

        Ok(())
    }

    /// Register a Metal texture with the bindless system (render graph backend).
    pub(crate) fn register_metal_bindless_texture(
        &mut self,
        texture: &objc2::rc::Retained<objc2::runtime::ProtocolObject<dyn objc2_metal::MTLTexture>>,
    ) -> Result<u32, RendererError> {
        self.bindless_manager
            .register_texture(texture)
            .map_err(|e| {
                RendererError::InvalidOperation(format!(
                    "Failed to register bindless texture: {}",
                    e
                ))
            })
    }

    pub(crate) fn update_metal_bindless_texture(
        &mut self,
        slot: u32,
        texture: &objc2::rc::Retained<objc2::runtime::ProtocolObject<dyn objc2_metal::MTLTexture>>,
    ) -> Result<(), RendererError> {
        self.bindless_manager
            .update_texture(slot, texture)
            .map_err(|e| {
                RendererError::InvalidOperation(format!(
                    "Failed to update bindless texture slot {}: {}",
                    slot, e
                ))
            })
    }

    /// Get the active ownership slot for per-frame resources.
    pub(crate) fn frame_index(&self) -> usize {
        frame_slot(self.frame_index)
    }
}

impl GpuRenderer for MetalRenderer {
    fn swapchain_extent(&self) -> Size2D {
        self.drawable_size
    }

    fn current_frame(&self) -> usize {
        self.frame_index()
    }

    fn num_images(&self) -> usize {
        1
    }

    fn wait_for_device(&self) {
        // Metal doesn't have a global device wait.
        // Per-frame sync is handled via wait_for_frame.
    }

    fn capabilities(&self) -> &crate::renderer::types::GpuCapabilities {
        &self.capabilities
    }

    fn destroy(&mut self) {
        self.particle_system = None;
        self.meshes = ResourceStorage::new();
        self.materials = ResourceStorage::new();
        self.textures = ResourceStorage::new();
        self.skeletons = ResourceStorage::new();
        self.viewports.clear();
    }

    fn wait_for_frame(&mut self) -> Result<(), RendererError> {
        self.wait_for_frame_impl()
    }

    fn set_frame_uniforms(&mut self, uniforms: FrameUniforms) {
        self.frame_uniforms = uniforms;
    }

    fn execute_draw_calls(&mut self, draw_list: &DrawList) -> Result<(), RendererError> {
        self.ensure_uniform_buffers()?;

        {
            let frame_buf = self.current_frame_uniform_buffer().unwrap();
            let ptr = frame_buf.map();
            unsafe {
                std::ptr::copy_nonoverlapping(
                    &self.frame_uniforms as *const FrameUniforms as *const u8,
                    ptr,
                    mem::size_of::<FrameUniforms>(),
                );
            }
            frame_buf.unmap();
        }

        let object_buf = self.current_object_storage_buffer().unwrap();
        let buf_size = object_buf.size() as usize;
        validate_object_buffer_capacity(draw_list, buf_size)?;
        let ptr = object_buf.map();

        for draw in &draw_list.draws {
            let inst = match draw.instances.first() {
                Some(i) => i,
                None => continue,
            };

            let offset = draw.instance_index as usize * OBJECT_UNIFORM_SIZE as usize;
            debug_assert!(offset + OBJECT_UNIFORM_SIZE as usize <= buf_size);
            let dst = unsafe { ptr.add(offset) };

            let material_params = draw.material_params();

            unsafe {
                std::ptr::copy_nonoverlapping(inst.model_matrix.as_ptr(), dst as *mut f32, 16);
                std::ptr::copy_nonoverlapping(inst.color.as_ptr(), dst.add(64) as *mut f32, 4);
                std::ptr::copy_nonoverlapping(material_params.as_ptr(), dst.add(80) as *mut f32, 4);

                let tex_indices: [u32; 4] =
                    if let Some(mat) = self.materials.get(draw.material.index()) {
                        mat.texture_indices
                    } else {
                        [0, 1, 2, 0]
                    };
                std::ptr::copy_nonoverlapping(tex_indices.as_ptr(), dst.add(96) as *mut u32, 4);
            }
        }

        object_buf.unmap();

        self.pending_draw_list = Some(draw_list.clone());

        Ok(())
    }

    fn draw(
        &mut self,
        uniforms: &FrameUniforms,
        draw_calls: &[crate::renderer::types::DrawCall],
    ) -> Result<DrawList, RendererError> {
        self.set_frame_uniforms(uniforms.clone());
        let draw_list = DrawList {
            draws: draw_calls.to_vec(),
        };
        self.execute_draw_calls(&draw_list)?;
        Ok(draw_list)
    }

    fn frame_uniforms(&self) -> &FrameUniforms {
        &self.frame_uniforms
    }

    fn begin_frame(&mut self) -> Result<u32, RendererError> {
        self.begin_frame_impl()
    }

    fn end_frame(&mut self) -> Result<(), RendererError> {
        self.end_frame_impl()
    }

    fn create_mesh<T, U>(&mut self, vertices: &[T], indices: &[U]) -> MeshHandle
    where
        T: bytemuck::Pod,
        U: bytemuck::Pod,
    {
        self.create_mesh_from_vertices(vertices, indices)
    }

    fn create_mesh_dynamic(
        &mut self,
        vertex_data: &[u8],
        _vertex_count: u32,
        indices: &[u32],
    ) -> MeshHandle {
        self.register_mesh_raw_impl(vertex_data, indices)
    }

    fn update_mesh_dynamic(
        &mut self,
        mesh: MeshHandle,
        vertex_data: &[u8],
        _vertex_count: u32,
        indices: &[u32],
    ) -> Result<(), RendererError> {
        self.update_mesh_dynamic_impl(mesh, vertex_data, indices)
    }

    fn create_texture(&mut self, desc: &TextureDescriptor, data: &[u8]) -> TextureHandle {
        self.create_texture_impl(desc, data)
    }

    fn create_texture_solid(&mut self, color: [u8; 4]) -> TextureHandle {
        self.create_texture_solid_impl(color)
    }

    fn update_texture(&mut self, handle: TextureHandle, data: &[u8]) -> Result<(), RendererError> {
        self.update_texture_impl(handle, data)
    }

    fn get_bindless_slot(&self, handle: TextureHandle) -> Option<u32> {
        self.get_bindless_slot_impl(handle)
    }

    fn get_texture_at_slot(&self, slot: u32) -> Option<TextureHandle> {
        self.get_texture_at_slot_impl(slot)
    }

    fn get_texture_bindless_index(&self, handle: TextureHandle) -> u32 {
        self.get_bindless_slot(handle).unwrap_or(0)
    }

    fn default_texture(&self) -> TextureHandle {
        self.default_texture_impl()
    }

    fn destroy_mesh(&mut self, handle: MeshHandle) {
        self.meshes.remove(handle.index());
    }

    fn destroy_texture(&mut self, handle: TextureHandle) {
        self.destroy_texture_impl(handle)
    }

    fn create_viewport(&mut self) -> crate::viewport::ViewportBuilder {
        self.create_viewport_impl()
    }

    fn viewport_count(&self) -> usize {
        self.viewport_count_impl()
    }

    fn get_viewport(
        &self,
        handle: crate::viewport::ViewportHandle,
    ) -> Option<&crate::viewport::Viewport> {
        self.get_viewport_impl(handle)
    }

    fn viewport_extent(
        &self,
        handle: crate::viewport::ViewportHandle,
    ) -> Option<crate::size::Size2D> {
        self.viewport_extent_impl(handle)
    }

    fn destroy_viewport(&mut self, handle: crate::viewport::ViewportHandle) {
        self.destroy_viewport_impl(handle)
    }

    fn compile_material(
        &mut self,
        shader_path: &str,
        vertex_type: &str,
    ) -> Result<MaterialHandle, RendererError> {
        self.compile_material_impl(shader_path, vertex_type)
    }

    fn set_material_texture_indices(&mut self, material: MaterialHandle, indices: [u32; 4]) {
        self.set_material_texture_indices_impl(material, indices)
    }

    fn set_default_material(&mut self, material: MaterialHandle) {
        self.default_material = Some(material);
    }

    fn default_material(&self) -> MaterialHandle {
        self.default_material_impl()
    }

    fn recompile_materials_for_shader(&mut self, shader_path: &std::path::Path) -> usize {
        self.recompile_materials_for_shader_impl(shader_path)
    }

    fn destroy_material(&mut self, handle: MaterialHandle) {
        self.destroy_material_impl(handle)
    }

    fn destroy_skeleton(&mut self, handle: SkeletonHandle) {
        self.destroy_skeleton_impl(handle)
    }

    fn resize(&mut self, width: u32, height: u32) -> Result<(), RendererError> {
        self.size = Size2D::new(width, height);
        self.context.surface.resize(width, height);
        let ds = self.context.surface.layer.drawableSize();
        let dw = ds.width as u32;
        let dh = ds.height as u32;
        if dw > 0 && dh > 0 {
            self.drawable_size = Size2D::new(dw, dh);
        }
        self.resize_light_culling(dw, dh);
        self.recreate_render_targets(dw, dh);
        Ok(())
    }

    fn recreate_scene_render_targets(&mut self, width: u32, height: u32) {
        self.recreate_render_targets(width, height);
        // The Forward+ light culling grid must match the scene render target
        // size: the PBR shader looks up tiles by framebuffer pixel coordinate,
        // and the HDR attachment is now panel-sized.
        self.resize_light_culling(width, height);
    }

    fn create_skeleton(&mut self, joint_count: usize) -> Result<SkeletonHandle, RendererError> {
        self.create_skeleton_impl(joint_count)
    }

    fn update_skeleton(&mut self, handle: SkeletonHandle, matrices: &[[f32; 16]]) {
        self.update_skeleton_impl(handle, matrices)
    }

    fn init_particle_system(&mut self) -> Result<(), RendererError> {
        const MAX_PARTICLES: u32 = 1_048_576; // Must match WGSL MAX_PARTICLES
        let subsystem = MetalParticleSubsystem::new(&self.context, MAX_PARTICLES)?;
        self.particle_system = Some(subsystem);
        Ok(())
    }

    fn create_ui_font_atlas(&mut self, width: u32, height: u32, data: &[u8]) -> TextureHandle {
        self.create_ui_font_atlas_impl(width, height, data)
    }

    fn update_ui_font_atlas(&mut self, width: u32, height: u32, data: &[u8]) {
        self.update_ui_font_atlas_impl(width, height, data)
    }

    fn ui_font_atlas_handle(&self) -> Option<TextureHandle> {
        self.ui_font_atlas_handle_impl()
    }

    // -- Lighting --

    fn upload_lights(&mut self, lights: &[crate::renderer::types::PointLightGPU]) {
        MetalRenderer::upload_lights(self, lights);
    }

    // -- Shadows --

    fn update_shadows(&mut self, light_direction: [f32; 3]) {
        MetalRenderer::update_shadows(self, light_direction);
    }

    fn upload_shadow_cascades(&mut self) {
        let Some(ref shadow_buf) = self.shadow_cascade_buffer else {
            return;
        };
        let data = self.shadow.gpu_data();
        let bytes = bytemuck::bytes_of(&data);
        let ptr = shadow_buf.map();
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr.cast(), bytes.len());
        }
        shadow_buf.unmap();
    }

    fn depth_texture_base_index(&self) -> Option<u32> {
        // Metal does not use bindless depth textures in the same way
        None
    }

    fn viewport_bindless_index(&self) -> Option<u32> {
        self.viewport_bindless_slot
    }

    fn geometry_hdr_bindless_index(&self) -> Option<u32> {
        self.geometry_hdr_bindless_slot
    }

    fn register_depth_textures_bindless(&mut self) -> Result<u32, RendererError> {
        Err(RendererError::InvalidOperation(
            "register_depth_textures_bindless not supported on Metal".into(),
        ))
    }

    // -- Animation --

    fn init_animation_pipeline(
        &mut self,
        shader_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        MetalRenderer::init_animation_pipeline(self, shader_path)
    }

    // -- Pipeline Initialization --

    fn init_light_culling(
        &mut self,
        width: u32,
        height: u32,
        shader_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        MetalRenderer::init_light_culling(self, width, height, shader_path)
    }

    fn init_shadow_resources(&mut self) -> Result<(), RendererError> {
        self.shadow.create_shadow_map(&self.context)
    }

    fn init_pass_pipeline(
        &mut self,
        kind: crate::renderer::pipeline_kind::PipelineKind,
        shader_paths: &[&std::path::Path],
    ) -> Result<(), RendererError> {
        match kind {
            PipelineKind::Shadow => MetalRenderer::init_shadow_pipeline(self, shader_paths[0]),
            PipelineKind::ShadowSkinned => {
                MetalRenderer::init_shadow_pipeline_skinned(self, shader_paths[0])
            }
            PipelineKind::DepthPrepass => {
                MetalRenderer::init_depth_prepass_pipeline(self, shader_paths[0])
            }
            PipelineKind::DepthPrepassSkinned => {
                MetalRenderer::init_depth_prepass_skinned_pipeline(self, shader_paths[0])
            }
            PipelineKind::DepthPrepassBillboard => {
                MetalRenderer::init_depth_prepass_billboard_pipeline(self, shader_paths[0])
            }
            PipelineKind::Outline => MetalRenderer::init_outline_pipelines(
                self,
                shader_paths[0],
                shader_paths[1],
                shader_paths[2],
                shader_paths[3],
            ),
            PipelineKind::StencilIndicator => MetalRenderer::init_stencil_indicator_pipelines(
                self,
                shader_paths[0],
                shader_paths[1],
            ),
            PipelineKind::Picking => MetalRenderer::init_picking_pipeline(self, shader_paths[0]),
            PipelineKind::PickingSkinned => {
                MetalRenderer::init_picking_skinned_pipeline(self, shader_paths[0])
            }
            PipelineKind::Sky => MetalRenderer::init_sky_pipeline(self, shader_paths[0]),
            PipelineKind::Tonemap => MetalRenderer::init_tonemap_pipeline(self, shader_paths[0]),
        }
    }

    fn set_viewport_bindless_slot(&mut self, slot: u32) {
        self.viewport_bindless_slot = Some(slot);
    }

    // -- UI Rendering --

    fn set_ui_material(&mut self, material: MaterialHandle) {
        self.ui_renderer.set_ui_material(material);
    }

    fn render_ui_pass(&mut self, draw_list: crate::renderer::types::UIDrawList) {
        MetalRenderer::render_ui_pass(self, draw_list);
    }

    fn begin_timestamp(&mut self, label: &str) {
        if let Some(ref mut tq) = self.timestamp_queries {
            tq.begin(label);
        }
    }

    fn end_timestamp(&mut self, label: &str) {
        if let Some(ref mut tq) = self.timestamp_queries {
            tq.end(label);
        }
    }

    fn read_timestamps(&self) -> Vec<crate::renderer::types::GpuTimestamp> {
        if let Some(ref tq) = self.timestamp_queries {
            tq.cached_results()
        } else {
            Vec::new()
        }
    }

    fn set_viewport_panel_rect(&mut self, rect: Option<crate::rect::Rect>) {
        self.viewport_panel_rect = rect;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::gpu_renderer::GpuRenderer;
    use crate::renderer::types::{DrawCall, DrawList, FrameUniforms};
    use crate::texture::TextureUsage;

    fn create_renderer() -> MetalRenderer {
        let context = MetalContext::init_headless().expect("Failed to create headless context");
        MetalRenderer::new(context).expect("Failed to create MetalRenderer")
    }

    #[test]
    fn frame_slots_wrap_at_the_renderer_slot_count() {
        assert_eq!(frame_slot(0), 0);
        assert_eq!(frame_slot(1), 1);
        assert_eq!(frame_slot(2), 0);
        assert_eq!(frame_slot(3), 1);
    }

    #[test]
    fn test_metal_renderer_creation() {
        let renderer = create_renderer();

        assert!(
            renderer.default_texture.is_some(),
            "default_texture should be set"
        );
        assert!(
            renderer.default_material.is_some(),
            "default_material should be set"
        );

        let default_tex = renderer.default_texture();
        assert!(
            default_tex.is_some(),
            "default texture handle should be valid"
        );

        let default_mat = renderer.default_material();
        assert!(
            default_mat.is_some(),
            "default material handle should be valid"
        );
    }

    #[test]
    fn test_metal_skeleton_create_update() {
        let mut renderer = create_renderer();

        let skeleton = renderer.create_skeleton(4).expect("create_skeleton failed");
        assert!(skeleton.is_some(), "skeleton handle should be valid");

        let identity = [[
            1.0f32, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ]; 4];
        renderer.update_skeleton(skeleton, &identity);
    }

    #[test]
    fn test_metal_primitive_meshes() {
        let mut renderer = create_renderer();

        let cube = crate::primitives::create_cube(&mut renderer, [1.0, 1.0, 1.0]);
        assert!(cube.is_some(), "cube handle should be valid");

        let sphere = crate::primitives::create_sphere(&mut renderer, 1.0, 16, 16);
        assert!(sphere.is_some(), "sphere handle should be valid");

        let plane = crate::primitives::create_plane(&mut renderer, 2.0, 2.0);
        assert!(plane.is_some(), "plane handle should be valid");

        assert_ne!(
            cube, sphere,
            "different meshes should have different handles"
        );
        assert_ne!(
            sphere, plane,
            "different meshes should have different handles"
        );
    }

    #[test]
    fn test_metal_texture_creation() {
        let mut renderer = create_renderer();

        let red_tex = renderer.create_texture_solid([255, 0, 0, 255]);
        assert!(red_tex.is_some(), "texture handle should be valid");

        let bindless_index = renderer.get_texture_bindless_index(red_tex);
        assert_ne!(
            bindless_index, 0,
            "custom texture should have a non-zero bindless slot"
        );

        let slot = renderer.get_bindless_slot(red_tex);
        assert!(
            slot.is_some(),
            "bindless slot should be allocated for custom texture"
        );
    }

    #[test]
    fn test_metal_execute_draw_calls() {
        let mut renderer = create_renderer();

        let default_mesh = crate::primitives::create_cube(&mut renderer, [1.0, 1.0, 1.0]);
        let default_mat = renderer.default_material();

        let draw = DrawCall::new(default_mesh, default_mat);
        let draw_list = DrawList { draws: vec![draw] };

        let result = renderer.execute_draw_calls(&draw_list);
        assert!(
            result.is_ok(),
            "execute_draw_calls should succeed: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_metal_mesh_dynamic_update() {
        let mut renderer = create_renderer();

        let vertex_data: [f32; 12] = [
            -0.5, -0.5, 0.0, 1.0, 0.5, -0.5, 0.0, 1.0, 0.0, 0.5, 0.0, 1.0,
        ];
        let indices: [u32; 3] = [0, 1, 2];
        let vertex_bytes = bytemuck::cast_slice(&vertex_data);

        let mesh = renderer.create_mesh_dynamic(vertex_bytes, 3, &indices);
        assert!(mesh.is_some(), "dynamic mesh handle should be valid");

        let updated_verts: [f32; 12] = [
            -1.0, -1.0, 0.0, 1.0, 1.0, -1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0,
        ];
        let updated_bytes = bytemuck::cast_slice(&updated_verts);
        let result = renderer.update_mesh_dynamic(mesh, updated_bytes, 3, &indices);
        assert!(
            result.is_ok(),
            "update_mesh_dynamic should succeed: {:?}",
            result.err()
        );
    }

    // --- Headless render test helpers ---

    fn sub3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
        [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
    }

    fn cross3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    }

    fn dot3(a: [f32; 3], b: [f32; 3]) -> f32 {
        a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
    }

    fn normalize3(v: [f32; 3]) -> [f32; 3] {
        let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        if len > 0.0 {
            [v[0] / len, v[1] / len, v[2] / len]
        } else {
            [0.0, 0.0, 0.0]
        }
    }

    fn look_at(eye: [f32; 3], target: [f32; 3], up: [f32; 3]) -> [f32; 16] {
        let fwd = normalize3(sub3(target, eye));
        let right = normalize3(cross3(fwd, up));
        let real_up = cross3(right, fwd);
        [
            right[0],
            real_up[0],
            -fwd[0],
            0.0,
            right[1],
            real_up[1],
            -fwd[1],
            0.0,
            right[2],
            real_up[2],
            -fwd[2],
            0.0,
            -dot3(right, eye),
            -dot3(real_up, eye),
            dot3(fwd, eye),
            1.0,
        ]
    }

    fn perspective(fov_deg: f32, aspect: f32, near: f32, far: f32) -> [f32; 16] {
        let f = 1.0 / (fov_deg * std::f32::consts::PI / 360.0).tan();
        [
            f / aspect,
            0.0,
            0.0,
            0.0,
            0.0,
            -f,
            0.0,
            0.0,
            0.0,
            0.0,
            far / (near - far),
            -1.0,
            0.0,
            0.0,
            near * far / (near - far),
            0.0,
        ]
    }

    fn mat4_mul(a: &[f32; 16], b: &[f32; 16]) -> [f32; 16] {
        let mut r = [0.0f32; 16];
        for col in 0..4 {
            for row in 0..4 {
                let mut sum = 0.0f32;
                for k in 0..4 {
                    sum += a[k * 4 + row] * b[col * 4 + k];
                }
                r[col * 4 + row] = sum;
            }
        }
        r
    }

    fn mat4_inverse(m: &[f32; 16]) -> [f32; 16] {
        let m00 = m[0];
        let m10 = m[1];
        let m20 = m[2];
        let m30 = m[3];
        let m01 = m[4];
        let m11 = m[5];
        let m21 = m[6];
        let m31 = m[7];
        let m02 = m[8];
        let m12 = m[9];
        let m22 = m[10];
        let m32 = m[11];
        let m03 = m[12];
        let m13 = m[13];
        let m23 = m[14];
        let m33 = m[15];

        let coef00 = m22 * m33 - m32 * m23;
        let coef02 = m12 * m33 - m32 * m13;
        let coef03 = m12 * m23 - m22 * m13;
        let coef04 = m21 * m33 - m31 * m23;
        let coef06 = m11 * m33 - m31 * m13;
        let coef07 = m11 * m23 - m21 * m13;
        let coef08 = m21 * m32 - m31 * m22;
        let coef10 = m11 * m32 - m31 * m12;
        let coef11 = m11 * m22 - m21 * m12;
        let coef12 = m20 * m33 - m30 * m23;
        let coef14 = m10 * m33 - m30 * m13;
        let coef15 = m10 * m23 - m20 * m13;
        let coef16 = m20 * m32 - m30 * m22;
        let coef18 = m10 * m32 - m30 * m12;
        let coef19 = m10 * m22 - m20 * m12;
        let coef20 = m20 * m31 - m30 * m21;
        let coef22 = m10 * m31 - m30 * m11;
        let coef23 = m10 * m21 - m20 * m11;

        let fac0 = [coef00, coef00, coef02, coef03];
        let fac1 = [coef04, coef04, coef06, coef07];
        let fac2 = [coef08, coef08, coef10, coef11];
        let fac3 = [coef12, coef12, coef14, coef15];
        let fac4 = [coef16, coef16, coef18, coef19];
        let fac5 = [coef20, coef20, coef22, coef23];

        let sign_a: [f32; 4] = [1.0, -1.0, 1.0, -1.0];
        let sign_b: [f32; 4] = [-1.0, 1.0, -1.0, 1.0];

        let det = m00 * (m11 * coef00 - m12 * coef04 + m13 * coef08)
            + m01 * (m10 * coef00 - m12 * coef12 + m13 * coef16)
            + m02 * (m10 * coef04 - m11 * coef12 + m13 * coef20)
            + m03 * (m10 * coef08 - m11 * coef16 + m12 * coef20);
        let inv_det = 1.0 / det;

        let row0 = [
            m11 * fac0[0] - m12 * fac1[0] + m13 * fac2[0],
            m10 * fac0[1] - m12 * fac3[1] + m13 * fac4[1],
            m10 * fac1[2] - m11 * fac3[2] + m13 * fac5[2],
            m10 * fac2[3] - m11 * fac4[3] + m12 * fac5[3],
        ];
        let row1 = [
            m01 * fac0[0] - m02 * fac1[0] + m03 * fac2[0],
            m00 * fac0[1] - m02 * fac3[1] + m03 * fac4[1],
            m00 * fac1[2] - m01 * fac3[2] + m03 * fac5[2],
            m00 * fac2[3] - m01 * fac4[3] + m02 * fac5[3],
        ];
        let row2 = [
            m31 * fac0[0] - m32 * fac1[0] + m33 * fac2[0],
            m30 * fac0[1] - m32 * fac3[1] + m33 * fac4[1],
            m30 * fac1[2] - m31 * fac3[2] + m33 * fac5[2],
            m30 * fac2[3] - m31 * fac4[3] + m32 * fac5[3],
        ];
        let row3 = [
            m21 * fac0[0] - m22 * fac1[0] + m23 * fac2[0],
            m20 * fac0[1] - m22 * fac3[1] + m23 * fac4[1],
            m20 * fac1[2] - m21 * fac3[2] + m23 * fac5[2],
            m20 * fac2[3] - m21 * fac4[3] + m22 * fac5[3],
        ];

        [
            row0[0] * sign_a[0] * inv_det,
            row0[1] * sign_b[0] * inv_det,
            row0[2] * sign_a[1] * inv_det,
            row0[3] * sign_b[1] * inv_det,
            row1[0] * sign_b[0] * inv_det,
            row1[1] * sign_a[1] * inv_det,
            row1[2] * sign_b[1] * inv_det,
            row1[3] * sign_a[2] * inv_det,
            row2[0] * sign_a[1] * inv_det,
            row2[1] * sign_b[1] * inv_det,
            row2[2] * sign_a[2] * inv_det,
            row2[3] * sign_b[2] * inv_det,
            row3[0] * sign_b[1] * inv_det,
            row3[1] * sign_a[2] * inv_det,
            row3[2] * sign_b[2] * inv_det,
            row3[3] * sign_a[3] * inv_det,
        ]
    }

    fn readback_texture_bgra8(
        texture: &ProtocolObject<dyn MTLTexture>,
        width: u32,
        height: u32,
    ) -> Vec<u8> {
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

    /// Regression test for white flicker bugs: render a basic scene with sky + PBR + tonemap
    /// and verify the output isn't predominantly white.
    #[test]
    fn test_headless_render_not_white() {
        let _ = env_logger::builder().is_test(true).try_init();

        const W: u32 = 256;
        const H: u32 = 256;

        // Create headless context with drawable support
        let context = MetalContext::init_headless_with_size(W, H)
            .expect("Failed to create headless context with size");
        let mut renderer = MetalRenderer::new(context).expect("Failed to create MetalRenderer");

        if let Some(reason) =
            super::super::argument_buffer::MetalBindlessTextureManager::unsupported_device_reason(
                &renderer.context.device,
            )
        {
            eprintln!("SKIP test_headless_render_not_white: {reason}");
            return;
        }

        // Resize to set up render targets (depth, HDR, depth-stencil)
        renderer.resize(W, H).expect("resize failed");

        // Compile pipelines the same way the app does
        let pbr_material = renderer
            .compile_material("model_pbr.wgsl", "pbr")
            .expect("Failed to compile PBR material");
        renderer
            .init_sky_pipeline(std::path::Path::new("sky.wgsl"))
            .expect("Failed to init sky pipeline");
        renderer
            .init_tonemap_pipeline(std::path::Path::new("tonemapping.wgsl"))
            .expect("Failed to init tonemap pipeline");

        // Initialize light culling so PBR shader's Forward+ bindings are satisfied
        renderer
            .init_light_culling(W, H, std::path::Path::new("light_culling.wgsl"))
            .expect("Failed to init light culling");
        renderer.upload_lights(&[]);

        // Create meshes
        let cube = crate::primitives::create_cube(&mut renderer, [1.0, 1.0, 1.0]);
        let plane = crate::primitives::create_plane(&mut renderer, 10.0, 10.0);

        // Create Shared BGRA8 texture as tonemap output (CPU-readable via getBytes)
        let readback_desc = TextureDescriptor::new(W, H, ImageFormat::B8G8R8A8Srgb)
            .with_usage(TextureUsage::COLOR_ATTACHMENT | TextureUsage::SAMPLED);
        let (readback_tex, readback_view) = renderer
            .context
            .create_texture_shared(&readback_desc)
            .expect("Failed to create readback texture");
        // The no-UI headless schedule tonemaps directly to the current drawable.
        // Use the CPU-readable texture as that drawable so the test reads the
        // attachment that was actually rendered.
        renderer.set_headless_drawable(readback_tex.inner.clone());

        // Create HDR texture for geometry pass, register with bindless
        let hdr_desc = TextureDescriptor::new(W, H, ImageFormat::R16G16B16A16Sfloat)
            .with_usage(TextureUsage::COLOR_ATTACHMENT | TextureUsage::SAMPLED);
        let (hdr_tex, hdr_view) = renderer
            .context
            .create_texture(&hdr_desc)
            .expect("Failed to create HDR texture");
        let hdr_slot = renderer
            .bindless_manager
            .register_texture(&hdr_tex.inner)
            .expect("Failed to register HDR texture in bindless");

        // Wire up the transient textures the same way the frame graph does
        renderer.set_geometry_hdr_view(hdr_view, hdr_slot);
        renderer.set_tonemap_output_view(readback_view);

        // Set up camera and frame uniforms
        let view = look_at([3.0, 3.0, 3.0], [0.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
        let proj = perspective(60.0, 1.0, 0.1, 100.0);
        let inv_view_proj = mat4_inverse(&mat4_mul(&proj, &view));

        let uniforms = FrameUniforms {
            view_matrix: view,
            proj_matrix: proj,
            inv_view_proj_matrix: inv_view_proj,
            camera_position: [3.0, 3.0, 3.0, 1.0],
            light_direction: [0.3, 1.0, 0.2, 0.0],
            light_color: [1.0, 0.98, 0.95, 0.0],
            light_intensity: [3.0, 0.0, 0.0, 0.0],
            tiles: [W / 16, H / 16, 0, 0],
            tonemap: [1.0, 2.2, 0.0, hdr_slot as f32],
            overlay: [0.0, 0.0, 0.0, 0.0],
            compositing: [0.0, 0.0, 0.0, 0.0],
        };
        renderer.set_frame_uniforms(uniforms);

        // Build draw list: red cube at origin, green ground plane below
        let plane_transform: [f32; 16] = [
            10.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 10.0, 0.0, 0.0, -0.5, 0.0, 1.0,
        ];

        let plane_draw = DrawCall::new(plane, pbr_material)
            .with_transform(plane_transform)
            .with_color([0.5, 0.7, 0.5, 1.0])
            .with_pbr(0.0, 0.8, 1.0);

        let cube_draw = DrawCall::new(cube, pbr_material)
            .with_color([0.8, 0.2, 0.2, 1.0])
            .with_pbr(0.0, 0.5, 1.0);

        let draw_list = DrawList {
            draws: vec![plane_draw, cube_draw],
        };

        // Upload uniforms and object data to GPU
        renderer
            .execute_draw_calls(&draw_list)
            .expect("execute_draw_calls failed");

        // Flush bindless argument buffer (same as MetalRenderer::render())
        renderer.bindless_manager.flush_argument_buffer();

        // Run the frame lifecycle
        renderer.begin_frame().expect("begin_frame failed");
        let plan = crate::metal::execution_plan::MetalExecutionPlan::for_test(&[
            crate::render_graph::PassKind::Geometry,
            crate::render_graph::PassKind::Fullscreen,
        ]);
        let mut pending = std::collections::HashMap::new();
        pending.insert(
            0,
            crate::render_graph::PassExecutionData {
                draw_lists: vec![std::rc::Rc::new(draw_list)],
                ..Default::default()
            },
        );
        renderer
            .render_frame(&plan, pending)
            .expect("render_frame failed");
        renderer.end_frame().expect("end_frame failed");

        // Wait for GPU to finish writing to the tonemap output texture
        renderer.wait_for_frame().expect("wait_for_frame failed");

        // Read back pixels from Shared storage texture
        let pixels = readback_texture_bgra8(&readback_tex.inner, W, H);

        // Analyze: count pixels that are nearly white (all channels > 240)
        let total_pixels = W as usize * H as usize;
        let white_count = pixels
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[0] > 240 && p[1] > 240 && p[2] > 240)
            .count();
        let white_ratio = white_count as f32 / total_pixels as f32;

        assert!(
            white_ratio < 0.5,
            "Image is {:.0}% white pixels (threshold 50%). \
             Sky + PBR + tonemap produced a blown-out white frame — \
             white flicker regression likely.",
            white_ratio * 100.0
        );
    }
}

impl Drop for MetalRenderer {
    fn drop(&mut self) {
        if let Some(archive) = self.context.pipeline_archive.as_ref() {
            log::info!(
                "Flushing pipeline archive ({} pipelines registered this session, {})",
                archive.registered_pipelines.get(),
                if archive.loaded_from_disk {
                    "opened from disk"
                } else {
                    "rebuilt"
                }
            );
            archive.flush();
        }
    }
}
