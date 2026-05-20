//! Metal backend implementation of the GpuRenderer trait.
//!
//! MetalRenderer wraps MetalContext and provides the same rendering API as
//! VulkanRenderer, allowing katla_app to be generic over the graphics backend.

use std::collections::HashMap;
use std::mem;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::{MTLCommandBuffer, MTLPixelFormat, MTLRenderCommandEncoder, MTLTexture};

use crate::backend::command::{
    ColorAttachmentInfo, DepthAttachmentInfo, GpuCommandBuffer, GpuRenderEncoder, IndexType,
    RenderPassInfo, ShaderStages,
};
use crate::backend::resource::GpuBuffer;
use crate::error::RendererError;
use crate::handle::{MaterialHandle, MeshHandle, ResourceStorage, SkeletonHandle, TextureHandle};

use crate::primitives;
use crate::render_pass::{ClearValue, LoadOp, StoreOp};
use crate::renderer::MAX_OBJECTS_PER_FRAME;
use crate::renderer::gpu_renderer::GpuRenderer;
use crate::renderer::types::{DrawList, FrameUniforms};
use crate::size::Size2D;
use crate::texture::{ImageFormat, TextureDescriptor, TextureUsage};
use crate::vertex::VertexPBR;
use crate::viewport::{Viewport, ViewportBuilder, ViewportHandle};

use super::animation::MetalAnimationSystem;
use super::argument_buffer::MetalBindlessTextureManager;
use super::buffer::MetalBuffer;
use super::context::MetalContext;
use super::depth_prepass::MetalDepthPrepass;
use super::light_culling::MetalLightCulling;
use super::outline::MetalOutlineSubsystem;
use super::particle::MetalParticleSubsystem;
use super::shader;
use super::shadow::MetalShadowSubsystem;
use super::texture::{MetalTexture, MetalTextureView};
use super::ui_renderer::MetalUIRenderer;

const OBJECT_UNIFORM_SIZE: u64 = 16 * 4 + 4 * 4 + 4 * 4 + 4 * 4;

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
}

/// A texture stored with its bindless slot.
struct MetalTextureEntry {
    _view: MetalTextureView,
    bindless_slot: Option<u32>,
}

fn read_shader(path: &str) -> Result<String, RendererError> {
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

fn resolve_wgsl_includes(
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
    frame_uniforms: FrameUniforms,
    frame_uniform_buffer: Option<MetalBuffer>,
    object_storage_buffer: Option<MetalBuffer>,
    current_drawable_texture: Option<Retained<ProtocolObject<dyn MTLTexture>>>,
    frame_index: u32,
    meshes: ResourceStorage<MetalMesh>,
    materials: ResourceStorage<MetalMaterial>,
    textures: ResourceStorage<MetalTextureEntry>,
    skeletons: ResourceStorage<MetalBuffer>,
    viewports: Vec<Viewport>,
    bindless_manager: MetalBindlessTextureManager,
    default_texture: Option<TextureHandle>,
    default_material: Option<MaterialHandle>,
    size: Size2D,
    drawable_size: Size2D,
    ui_font_atlas: Option<TextureHandle>,
    last_command_buffer: Option<Retained<ProtocolObject<dyn MTLCommandBuffer>>>,
    pending_draw_list: Option<DrawList>,
    light_culling: Option<MetalLightCulling>,
    ui_renderer: MetalUIRenderer,
    animation_system: Option<MetalAnimationSystem>,
    particle_system: Option<MetalParticleSubsystem>,
    pending_ui_draw_list: Option<crate::renderer::types::UIDrawList>,
    shadow: MetalShadowSubsystem,
    depth_prepass: MetalDepthPrepass,
    outline: MetalOutlineSubsystem,
    depth_texture_view: Option<MetalTextureView>,
    hdr_color_view: Option<MetalTextureView>,
    depth_stencil_view: Option<MetalTextureView>,
    shared_sampler: Option<super::sampler::MetalSamplerState>,
    shadow_cascade_buffer: Option<MetalBuffer>,
    shadow_sampler: Option<super::sampler::MetalSamplerState>,
    buffer_sizes_buffer: Option<MetalBuffer>,
    scene_color_view: Option<MetalTextureView>,
    viewport_bindless_slot: Option<u32>,
    sky_pipeline: Option<super::pipeline::MetalGraphicsPipeline>,
    dummy_vertex_buffer: Option<MetalBuffer>,
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
            unsafe {
                std::env::set_var("METAL_DEVICE_WRAPPER_TYPE", "1");
                std::env::set_var("MTL_DEBUG_LAYER", "1");
            }
            if validation_mode.is_gpu_assisted() {
                unsafe {
                    std::env::set_var("METAL_ERROR_MODE", "5");
                }
            }
            log::info!("Metal validation enabled");
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

    pub(crate) fn new(context: MetalContext) -> Result<Self, RendererError> {
        let features = context.detect_features();
        let bindless_manager = MetalBindlessTextureManager::new(features.max_bindless_textures)?;

        let mut renderer = Self {
            context,
            frame_uniforms: FrameUniforms::default(),
            frame_uniform_buffer: None,
            object_storage_buffer: None,
            current_drawable_texture: None,
            frame_index: 0,
            meshes: ResourceStorage::new(),
            materials: ResourceStorage::new(),
            textures: ResourceStorage::new(),
            skeletons: ResourceStorage::new(),
            viewports: Vec::new(),
            bindless_manager,
            default_texture: None,
            default_material: None,
            size: Size2D::default(),
            drawable_size: Size2D::default(),
            ui_font_atlas: None,
            last_command_buffer: None,
            pending_draw_list: None,
            light_culling: None,
            ui_renderer: MetalUIRenderer::new(),
            animation_system: None,
            particle_system: None,
            pending_ui_draw_list: None,
            shadow: MetalShadowSubsystem::new(),
            depth_prepass: MetalDepthPrepass::new(),
            outline: MetalOutlineSubsystem::new(),
            depth_texture_view: None,
            hdr_color_view: None,
            depth_stencil_view: None,
            shared_sampler: None,
            shadow_cascade_buffer: None,
            shadow_sampler: None,
            buffer_sizes_buffer: None,
            scene_color_view: None,
            viewport_bindless_slot: None,
            sky_pipeline: None,
            dummy_vertex_buffer: None,
        };

        let default_tex = renderer.create_texture_solid([255, 255, 255, 255]);
        renderer.default_texture = Some(default_tex);

        // Initialize the argument buffer now that the default texture exists.
        if let Some(entry) = renderer.textures.get(default_tex.index()) {
            renderer
                .bindless_manager
                .init_argument_buffer(&renderer.context.device, &entry._view.inner);
        }

        let default_mat = MetalMaterial {
            pipeline: None,
            texture_indices: [0; 4],
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

        Ok(renderer)
    }

    fn upload_vertex_index_data(
        &mut self,
        vertex_data: &[u8],
        index_data: &[u32],
    ) -> Result<(MetalBuffer, MetalBuffer, u32), RendererError> {
        let vertex_buffer = self.context.create_buffer(vertex_data.len() as u64, true)?;
        let index_buffer = self
            .context
            .create_buffer((index_data.len() * 4) as u64, true)?;

        {
            let ptr = vertex_buffer.map();
            unsafe {
                std::ptr::copy_nonoverlapping(vertex_data.as_ptr(), ptr, vertex_data.len());
            }
            vertex_buffer.unmap();
        }
        {
            let ptr = index_buffer.map();
            let index_bytes = unsafe {
                std::slice::from_raw_parts(index_data.as_ptr() as *const u8, index_data.len() * 4)
            };
            unsafe {
                std::ptr::copy_nonoverlapping(index_bytes.as_ptr(), ptr, index_bytes.len());
            }
            index_buffer.unmap();
        }

        Ok((vertex_buffer, index_buffer, index_data.len() as u32))
    }

    fn create_primitive_mesh(&mut self, vertices: Vec<VertexPBR>, indices: Vec<u32>) -> MeshHandle {
        let vertex_bytes = bytemuck::cast_slice(&vertices);
        let (vertex_buffer, index_buffer, index_count) = self
            .upload_vertex_index_data(vertex_bytes, &indices)
            .expect("Failed to create primitive mesh buffers");

        let mesh = MetalMesh {
            vertex_buffer,
            index_buffer,
            index_count,
        };
        let id = self.meshes.insert(mesh);
        MeshHandle::new(id)
    }

    fn ensure_uniform_buffers(&mut self) -> Result<(), RendererError> {
        if self.frame_uniform_buffer.is_none() {
            let frame_size = mem::size_of::<FrameUniforms>() as u64;
            self.frame_uniform_buffer = Some(self.context.create_buffer(frame_size, true)?);
        }
        if self.object_storage_buffer.is_none() {
            let object_size = MAX_OBJECTS_PER_FRAME as u64 * OBJECT_UNIFORM_SIZE;
            self.object_storage_buffer = Some(self.context.create_buffer(object_size, true)?);
        }
        Ok(())
    }

    /// Recreate render targets (depth, HDR color, scene color, depth-stencil) for the given size.
    fn recreate_render_targets(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }

        // Depth texture for depth prepass and main pass
        {
            let desc = TextureDescriptor::new(width, height, ImageFormat::D32Sfloat)
                .with_usage(TextureUsage::DEPTH_STENCIL_ATTACHMENT | TextureUsage::SAMPLED);
            if let Ok((_tex, view)) = self.context.create_texture(&desc) {
                self.depth_texture_view = Some(view);
            }
        }

        // HDR color texture for outline pass
        {
            let desc = TextureDescriptor::new(width, height, ImageFormat::R16G16B16A16Sfloat)
                .with_usage(TextureUsage::COLOR_ATTACHMENT | TextureUsage::SAMPLED);
            if let Ok((_tex, view)) = self.context.create_texture(&desc) {
                self.hdr_color_view = Some(view);
            }
        }

        // Scene color texture: blitted from drawable after rendering for UI viewport sampling
        {
            if let Some(old_slot) = self.viewport_bindless_slot.take() {
                self.bindless_manager.release_slot(old_slot);
            }
            let desc = TextureDescriptor::new(width, height, ImageFormat::B8G8R8A8Srgb)
                .with_usage(TextureUsage::COLOR_ATTACHMENT | TextureUsage::SAMPLED);
            if let Ok((_tex, view)) = self.context.create_texture(&desc) {
                if let Ok(slot) = self.bindless_manager.register_texture(&view.inner) {
                    self.viewport_bindless_slot = Some(slot);
                }
                self.scene_color_view = Some(view);
            }
        }

        // Depth-stencil texture for outline pass (needs stencil)
        {
            let desc = TextureDescriptor::new(width, height, ImageFormat::D32SfloatS8Uint)
                .with_usage(TextureUsage::DEPTH_STENCIL_ATTACHMENT);
            if let Ok((_tex, view)) = self.context.create_texture(&desc) {
                self.depth_stencil_view = Some(view);
            }
        }
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
        if let Some(ref mut lc) = self.light_culling {
            if let Err(e) = lc.resize(&self.context, screen_width, screen_height) {
                log::error!("Failed to resize Metal light culling: {}", e);
            }
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

    /// Create the shadow depth-only pipeline.
    pub fn init_shadow_pipeline(
        &mut self,
        shader_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        let wgsl_source = read_shader(&shader_path.to_string_lossy())?;

        let compiled =
            shader::compile_wgsl_to_metal(&self.context.device, &wgsl_source, &["vs_main"], false)?;

        let vertex_fn = compiled.module.entry_points.get("vs_main").ok_or_else(|| {
            RendererError::InvalidOperation("Shadow vertex entry point not found".into())
        })?;

        self.shadow.create_pipeline(&self.context, vertex_fn)
    }

    /// Create the skinned shadow pipeline.
    pub fn init_shadow_pipeline_skinned(
        &mut self,
        shader_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        let wgsl_source = read_shader(&shader_path.to_string_lossy())?;

        let compiled =
            shader::compile_wgsl_to_metal(&self.context.device, &wgsl_source, &["vs_main"], false)?;

        let vertex_fn = compiled.module.entry_points.get("vs_main").ok_or_else(|| {
            RendererError::InvalidOperation("Skinned shadow vertex entry point not found".into())
        })?;

        self.shadow.create_pipeline(&self.context, vertex_fn)
    }

    /// Create the depth prepass pipeline.
    pub fn init_depth_prepass_pipeline(
        &mut self,
        shader_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        let wgsl_source = std::fs::read_to_string(shader_path).map_err(|e| {
            RendererError::InvalidOperation(format!(
                "Failed to read depth prepass shader '{}': {}",
                shader_path.display(),
                e
            ))
        })?;

        let compiled =
            shader::compile_wgsl_to_metal(&self.context.device, &wgsl_source, &["vs_main"], false)?;

        let vertex_fn = compiled.module.entry_points.get("vs_main").ok_or_else(|| {
            RendererError::InvalidOperation("Depth prepass vertex entry point not found".into())
        })?;

        self.depth_prepass.create_pipeline(&self.context, vertex_fn)
    }

    /// Create the skinned depth prepass pipeline.
    pub fn init_depth_prepass_skinned_pipeline(
        &mut self,
        shader_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        let wgsl_source = std::fs::read_to_string(shader_path).map_err(|e| {
            RendererError::InvalidOperation(format!(
                "Failed to read skinned depth prepass shader '{}': {}",
                shader_path.display(),
                e
            ))
        })?;

        let compiled =
            shader::compile_wgsl_to_metal(&self.context.device, &wgsl_source, &["vs_main"], false)?;

        let vertex_fn = compiled.module.entry_points.get("vs_main").ok_or_else(|| {
            RendererError::InvalidOperation(
                "Skinned depth prepass vertex entry point not found".into(),
            )
        })?;

        self.depth_prepass
            .create_pipeline_skinned(&self.context, vertex_fn)
    }

    /// Create the billboard depth prepass pipeline.
    pub fn init_depth_prepass_billboard_pipeline(
        &mut self,
        shader_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        let wgsl_source = std::fs::read_to_string(shader_path).map_err(|e| {
            RendererError::InvalidOperation(format!(
                "Failed to read billboard depth prepass shader '{}': {}",
                shader_path.display(),
                e
            ))
        })?;

        let compiled =
            shader::compile_wgsl_to_metal(&self.context.device, &wgsl_source, &["vs_main"], false)?;

        let vertex_fn = compiled.module.entry_points.get("vs_main").ok_or_else(|| {
            RendererError::InvalidOperation(
                "Billboard depth prepass vertex entry point not found".into(),
            )
        })?;

        self.depth_prepass.create_pipeline(&self.context, vertex_fn)
    }

    /// Create outline (stencil mark + draw) pipelines.
    pub fn init_outline_pipelines(
        &mut self,
        stencil_mark_path: &std::path::Path,
        stencil_mark_skinned_path: &std::path::Path,
        outline_draw_path: &std::path::Path,
        outline_draw_skinned_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        // Stencil mark (non-skinned)
        {
            let wgsl = std::fs::read_to_string(stencil_mark_path).map_err(|e| {
                RendererError::InvalidOperation(format!(
                    "Failed to read stencil mark shader '{}': {}",
                    stencil_mark_path.display(),
                    e
                ))
            })?;
            let compiled =
                shader::compile_wgsl_to_metal(&self.context.device, &wgsl, &["vs_main"], false)?;
            let vs = compiled.module.entry_points.get("vs_main").ok_or_else(|| {
                RendererError::InvalidOperation("Stencil mark vertex entry point not found".into())
            })?;
            self.outline
                .create_stencil_mark_pipeline(&self.context, vs)?;
        }

        // Stencil mark (skinned)
        {
            let wgsl = std::fs::read_to_string(stencil_mark_skinned_path).map_err(|e| {
                RendererError::InvalidOperation(format!(
                    "Failed to read skinned stencil mark shader '{}': {}",
                    stencil_mark_skinned_path.display(),
                    e
                ))
            })?;
            let compiled =
                shader::compile_wgsl_to_metal(&self.context.device, &wgsl, &["vs_main"], false)?;
            let vs = compiled.module.entry_points.get("vs_main").ok_or_else(|| {
                RendererError::InvalidOperation(
                    "Skinned stencil mark vertex entry point not found".into(),
                )
            })?;
            self.outline
                .create_stencil_mark_skinned_pipeline(&self.context, vs)?;
        }

        // Outline draw (non-skinned)
        {
            let wgsl = std::fs::read_to_string(outline_draw_path).map_err(|e| {
                RendererError::InvalidOperation(format!(
                    "Failed to read outline draw shader '{}': {}",
                    outline_draw_path.display(),
                    e
                ))
            })?;
            let compiled = shader::compile_wgsl_to_metal(
                &self.context.device,
                &wgsl,
                &["vs_main", "fs_main"],
                false,
            )?;
            let vs = compiled.module.entry_points.get("vs_main").ok_or_else(|| {
                RendererError::InvalidOperation("Outline draw vertex entry point not found".into())
            })?;
            let fs = compiled.module.entry_points.get("fs_main").ok_or_else(|| {
                RendererError::InvalidOperation(
                    "Outline draw fragment entry point not found".into(),
                )
            })?;
            self.outline
                .create_outline_draw_pipeline(&self.context, vs, fs)?;
        }

        // Outline draw (skinned)
        {
            let wgsl = std::fs::read_to_string(outline_draw_skinned_path).map_err(|e| {
                RendererError::InvalidOperation(format!(
                    "Failed to read skinned outline draw shader '{}': {}",
                    outline_draw_skinned_path.display(),
                    e
                ))
            })?;
            let compiled = shader::compile_wgsl_to_metal(
                &self.context.device,
                &wgsl,
                &["vs_main", "fs_main"],
                false,
            )?;
            let vs = compiled.module.entry_points.get("vs_main").ok_or_else(|| {
                RendererError::InvalidOperation(
                    "Skinned outline draw vertex entry point not found".into(),
                )
            })?;
            let fs = compiled.module.entry_points.get("fs_main").ok_or_else(|| {
                RendererError::InvalidOperation(
                    "Skinned outline draw fragment entry point not found".into(),
                )
            })?;
            self.outline
                .create_outline_draw_skinned_pipeline(&self.context, vs, fs)?;
        }

        Ok(())
    }

    /// Create stencil indicator pipelines (no-op for Metal, not yet needed).
    pub fn init_stencil_indicator_pipelines(
        &mut self,
        _shader_path: &std::path::Path,
        _skinned_shader_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        Ok(())
    }

    /// Initialize the sky pipeline for procedural atmosphere rendering.
    ///
    /// Compiles the sky WGSL shader into a Metal fullscreen pipeline that uses
    /// `@builtin(vertex_index)` to generate a fullscreen triangle.
    pub fn init_sky_pipeline(
        &mut self,
        shader_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        let wgsl_source = read_shader(&shader_path.to_string_lossy())?;

        let compiled = shader::compile_wgsl_to_metal(
            &self.context.device,
            &wgsl_source,
            &["vs_main", "fs_main"],
            false,
        )?;

        let vertex_fn = compiled.module.entry_points.get("vs_main").ok_or_else(|| {
            RendererError::InvalidOperation("Sky vertex entry point not found".into())
        })?;
        let fragment_fn = compiled.module.entry_points.get("fs_main").ok_or_else(|| {
            RendererError::InvalidOperation("Sky fragment entry point not found".into())
        })?;

        let pipeline = self
            .context
            .create_graphics_pipeline_with_vertex_descriptor(
                vertex_fn,
                Some(fragment_fn),
                &[MTLPixelFormat::BGRA8Unorm_sRGB],
                None,
                false,
                crate::pipeline::CompareOp::Always,
                objc2_metal::MTLCullMode::None,
                objc2_metal::MTLWinding::CounterClockwise,
                Some(&super::context::fullscreen_vertex_descriptor()),
                false,
            )?;

        self.sky_pipeline = Some(pipeline);
        Ok(())
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
        let Some(ref shadow_map) = self.shadow.shadow_map_view() else {
            return Ok(());
        };
        let Some(ref pipeline) = self.shadow.pipeline() else {
            return Ok(());
        };
        let Some(ref draw_list) = self.pending_draw_list else {
            return Ok(());
        };
        let Some(ref frame_buf) = self.frame_uniform_buffer else {
            return Ok(());
        };
        let Some(ref object_buf) = self.object_storage_buffer else {
            return Ok(());
        };

        let mut cmd_buffer = self.context.create_command_buffer();
        cmd_buffer.begin();
        unsafe {
            let label = objc2_foundation::NSString::from_str("shadow_pass");
            cmd_buffer.inner.setLabel(Some(&label));
        }

        for cascade_idx in 0..self.shadow.cascade_count() as usize {
            let cascade_view_proj = self.shadow.cascade_view_proj(cascade_idx);
            super::shadow::render_cascade(
                &mut cmd_buffer,
                pipeline,
                shadow_map,
                self.shadow.shadow_resolution(),
                frame_buf,
                object_buf,
                &cascade_view_proj,
                &self.meshes,
                &self.materials,
                draw_list,
            );
        }

        cmd_buffer.end();
        cmd_buffer.submit(&self.context);
        cmd_buffer.inner.waitUntilCompleted();

        Ok(())
    }

    /// Render the depth prepass.
    pub fn render_depth_prepass(&mut self) -> Result<(), RendererError> {
        let Some(ref pipeline) = self.depth_prepass.pipeline() else {
            return Ok(());
        };
        let Some(ref draw_list) = self.pending_draw_list else {
            return Ok(());
        };
        let Some(ref frame_buf) = self.frame_uniform_buffer else {
            return Ok(());
        };
        let Some(ref object_buf) = self.object_storage_buffer else {
            return Ok(());
        };
        let Some(ref depth_view) = self.depth_texture_view else {
            return Ok(());
        };

        let width = self.size.width;
        let height = self.size.height;

        let mut cmd_buffer = self.context.create_command_buffer();
        cmd_buffer.begin();
        unsafe {
            let label = objc2_foundation::NSString::from_str("depth_prepass");
            cmd_buffer.inner.setLabel(Some(&label));
        }

        super::depth_prepass::render_depth_prepass(
            &mut cmd_buffer,
            pipeline,
            depth_view,
            width,
            height,
            frame_buf,
            object_buf,
            &self.meshes,
            &self.materials,
            draw_list,
        );

        cmd_buffer.end();
        cmd_buffer.submit(&self.context);
        cmd_buffer.inner.waitUntilCompleted();

        Ok(())
    }

    /// Render the outline pass for selected objects.
    pub fn render_outline_pass(&mut self) -> Result<(), RendererError> {
        let Some(ref stencil_pipeline) = self.outline.stencil_mark_pipeline() else {
            return Ok(());
        };
        let Some(ref outline_pipeline) = self.outline.outline_draw_pipeline() else {
            return Ok(());
        };
        let Some(ref draw_list) = self.pending_draw_list else {
            return Ok(());
        };
        let Some(ref frame_buf) = self.frame_uniform_buffer else {
            return Ok(());
        };
        let Some(ref object_buf) = self.object_storage_buffer else {
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
            color_view,
            depth_view,
            width,
            height,
            frame_buf,
            object_buf,
            &self.meshes,
            &self.materials,
            draw_list,
        );

        super::outline::render_outline(
            &mut cmd_buffer,
            outline_pipeline,
            color_view,
            depth_view,
            width,
            height,
            frame_buf,
            object_buf,
            &self.meshes,
            &self.materials,
            draw_list,
        );

        cmd_buffer.end();
        cmd_buffer.submit(&self.context);
        cmd_buffer.inner.waitUntilCompleted();

        Ok(())
    }
}

impl GpuRenderer for MetalRenderer {
    fn swapchain_extent(&self) -> Size2D {
        self.size
    }

    fn current_frame(&self) -> usize {
        (self.frame_index % 3) as usize
    }

    fn num_images(&self) -> usize {
        3
    }

    fn wait_for_device(&self) {
        // Metal doesn't have a global device wait.
        // Per-frame sync is handled via wait_for_frame.
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
        if let Some(cmd_buffer) = self.last_command_buffer.take() {
            cmd_buffer.waitUntilCompleted();
        }
        Ok(())
    }

    fn set_frame_uniforms(&mut self, mut uniforms: FrameUniforms) {
        // Metal uses Y-up NDC while the projection matrix is built for Vulkan's Y-down NDC.
        // Negate column 1 (Y column) of the projection matrix to correct the flip.
        // Column-major flat array: column 1 is at indices [4, 5, 6, 7].
        for i in [4, 5, 6, 7] {
            uniforms.proj_matrix[i] = -uniforms.proj_matrix[i];
        }
        // inv_view_proj: new_P = diag(1,-1,1,1)*old_P, so new_inv = old_inv * diag(1,-1,1,1)
        // which negates column 1 of the inverse.
        for i in [4, 5, 6, 7] {
            uniforms.inv_view_proj_matrix[i] = -uniforms.inv_view_proj_matrix[i];
        }
        self.frame_uniforms = uniforms;
    }

    fn execute_draw_calls(&mut self, draw_list: &DrawList) -> Result<(), RendererError> {
        self.ensure_uniform_buffers()?;

        {
            let frame_buf = self.frame_uniform_buffer.as_ref().unwrap();
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

        let object_buf = self.object_storage_buffer.as_ref().unwrap();
        let ptr = object_buf.map();

        for draw in &draw_list.draws {
            let inst = match draw.instances.first() {
                Some(i) => i,
                None => continue,
            };

            let offset = draw.instance_index as u64 * OBJECT_UNIFORM_SIZE;
            let dst = unsafe { ptr.add(offset as usize) };

            let material_params = draw.material_params();

            unsafe {
                std::ptr::copy_nonoverlapping(inst.model_matrix.as_ptr(), dst as *mut f32, 16);
                std::ptr::copy_nonoverlapping(inst.color.as_ptr(), dst.add(64) as *mut f32, 4);
                std::ptr::copy_nonoverlapping(material_params.as_ptr(), dst.add(80) as *mut f32, 4);

                let tex_indices: [u32; 4] =
                    if let Some(mat) = self.materials.get(draw.material.index()) {
                        mat.texture_indices
                    } else {
                        [0; 4]
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

    fn render_frame(&mut self) -> Result<(), RendererError> {
        if self.depth_texture_view.is_none() {
            self.current_drawable_texture = None;
            return Ok(());
        }

        let drawable_texture = self
            .current_drawable_texture
            .take()
            .ok_or_else(|| RendererError::InvalidOperation("No drawable texture".into()))?;

        let drawable_view = MetalTextureView::new(
            drawable_texture.clone(),
            MetalTexture::new(drawable_texture, ImageFormat::B8G8R8A8Srgb),
        );

        let mut cmd_buffer = self.context.create_command_buffer();
        cmd_buffer.begin();
        unsafe {
            let label = objc2_foundation::NSString::from_str("main_render");
            cmd_buffer.inner.setLabel(Some(&label));
        }

        let depth_attachment = self
            .depth_texture_view
            .as_ref()
            .map(|view| DepthAttachmentInfo {
                view: view.clone(),
                load_op: LoadOp::Clear,
                store_op: StoreOp::DontCare,
                clear_value: ClearValue::depth_stencil(0.0, 0),
                format: ImageFormat::D32Sfloat,
            });

        let render_pass_info = RenderPassInfo {
            color_attachments: vec![ColorAttachmentInfo {
                view: drawable_view,
                load_op: LoadOp::Clear,
                store_op: StoreOp::Store,
                clear_value: ClearValue::OPAQUE_BLACK,
            }],
            depth_attachment,
        };

        let mut encoder = cmd_buffer.begin_render_pass(render_pass_info);

        let width = self.drawable_size.width as f32;
        let height = self.drawable_size.height as f32;
        if width > 0.0 && height > 0.0 {
            encoder.set_viewport(0.0, 0.0, width, height, 0.0, 1.0);
        }

        if let (Some(frame_buf), Some(object_buf)) =
            (&self.frame_uniform_buffer, &self.object_storage_buffer)
        {
            let stages = ShaderStages::VERTEX_FRAGMENT;
            encoder.bind_storage_buffer(frame_buf, 0, 0, stages);
            encoder.bind_storage_buffer(object_buf, 0, 1, stages);
        }

        // Bind the bindless texture argument buffer at index 9.
        if let Some(arg_buffer) = self.bindless_manager.argument_buffer() {
            let stages = ShaderStages::VERTEX_FRAGMENT;
            unsafe {
                if stages.vertex {
                    encoder
                        .inner
                        .setVertexBuffer_offset_atIndex(Some(arg_buffer), 0, 9);
                }
                if stages.fragment {
                    encoder
                        .inner
                        .setFragmentBuffer_offset_atIndex(Some(arg_buffer), 0, 9);
                }
            }
        }

        // Bind the shared sampler at index 0
        if let Some(ref sampler) = self.shared_sampler {
            unsafe {
                encoder
                    .inner
                    .setVertexSamplerState_atIndex(Some(&sampler.inner), 0);
                encoder
                    .inner
                    .setFragmentSamplerState_atIndex(Some(&sampler.inner), 0);
            }
        }

        // Bind the buffer sizes buffer at index 8 (naga's _mslBufferSizes)
        if let Some(ref buf_sizes) = self.buffer_sizes_buffer {
            let stages = ShaderStages::VERTEX_FRAGMENT;
            encoder.bind_storage_buffer(buf_sizes, 0, 8, stages);
        }

        // Bind light culling buffers (Set 3: point_lights, tile_indices, tile_counts)
        if let Some(ref lc) = self.light_culling {
            let stages = ShaderStages::FRAGMENT;
            encoder.bind_storage_buffer(lc.light_buffer(), 0, 3, stages);
            encoder.bind_storage_buffer(lc.tile_index_buffer(), 0, 4, stages);
            encoder.bind_storage_buffer(lc.tile_count_buffer(), 0, 5, stages);
        }

        // Bind shadow cascade data at buffer 7 (Set 4, binding 0)
        if let Some(ref shadow_buf) = self.shadow_cascade_buffer {
            let stages = ShaderStages::FRAGMENT;
            encoder.bind_storage_buffer(shadow_buf, 0, 7, stages);
        }

        // Bind shadow atlas texture at texture 1 (Set 4, binding 1)
        if let Some(ref shadow_view) = self.shadow.shadow_map_view() {
            unsafe {
                encoder
                    .inner
                    .setFragmentTexture_atIndex(Some(&shadow_view.inner), 1);
            }
        }

        // Bind shadow comparison sampler at sampler 1 (Set 4, binding 2)
        if let Some(ref sampler) = self.shadow_sampler {
            unsafe {
                encoder
                    .inner
                    .setFragmentSamplerState_atIndex(Some(&sampler.inner), 1);
            }
        }

        // Make all bindless textures and the argument buffer resident for the GPU.
        if let Some(arg_buffer) = self.bindless_manager.argument_buffer() {
            unsafe {
                let resource: &ProtocolObject<dyn objc2_metal::MTLResource> =
                    std::mem::transmute(arg_buffer);
                encoder.inner.useResource_usage_stages(
                    resource,
                    objc2_metal::MTLResourceUsage::Read,
                    objc2_metal::MTLRenderStages::Fragment,
                );
            }
        }
        for texture in self.bindless_manager.registered_textures() {
            unsafe {
                let resource: &ProtocolObject<dyn objc2_metal::MTLResource> =
                    std::mem::transmute(texture);
                encoder.inner.useResource_usage_stages(
                    resource,
                    objc2_metal::MTLResourceUsage::Read,
                    objc2_metal::MTLRenderStages::Fragment,
                );
            }
        }

        // Draw sky fullscreen triangle before geometry (writes to background)
        if let Some(ref sky_pipeline) = self.sky_pipeline {
            if let (Some(frame_buf), Some(object_buf)) =
                (&self.frame_uniform_buffer, &self.object_storage_buffer)
            {
                let stages = ShaderStages::VERTEX_FRAGMENT;
                encoder.bind_storage_buffer(frame_buf, 0, 0, stages);
                encoder.bind_storage_buffer(object_buf, 0, 1, stages);
            }
            if let Some(ref buf_sizes) = self.buffer_sizes_buffer {
                encoder.bind_storage_buffer(buf_sizes, 0, 8, ShaderStages::VERTEX_FRAGMENT);
            }
            if let Some(ref dummy_vb) = self.dummy_vertex_buffer {
                encoder.bind_vertex_buffer(dummy_vb, 0, 10);
            }
            encoder.bind_graphics_pipeline(sky_pipeline);
            encoder.draw(3, 1, 0, 0);
        }

        if let Some(draw_list) = self.pending_draw_list.take() {
            for (i, draw) in draw_list.draws.iter().enumerate() {
                let Some(mesh) = self.meshes.get(draw.mesh.index()) else {
                    log::warn!("Draw {}: mesh index {} not found", i, draw.mesh.index());
                    continue;
                };
                let Some(material) = self.materials.get(draw.material.index()) else {
                    log::warn!(
                        "Draw {}: material index {} not found",
                        i,
                        draw.material.index()
                    );
                    continue;
                };
                let Some(ref pipeline) = material.pipeline else {
                    log::warn!("Draw {}: no pipeline", i);
                    continue;
                };

                encoder.bind_graphics_pipeline(pipeline);
                encoder.bind_vertex_buffer(&mesh.vertex_buffer, 0, 10);
                encoder.bind_index_buffer(&mesh.index_buffer, 0, IndexType::Uint32);
                encoder.draw_indexed(mesh.index_count, 1, 0, 0, draw.instance_index);
            }
        }

        // UI rendering pass (after geometry, before present)
        if let Some(ui_draw_list) = self.pending_ui_draw_list.take() {
            if !ui_draw_list.is_empty() {
                if self
                    .ui_renderer
                    .upload_draw_list(&self.context, &ui_draw_list)
                    .is_ok()
                {
                    let ui_material_handle = self.ui_renderer.ui_material();
                    if let Some(ui_mat_handle) = ui_material_handle {
                        if let Some(ui_material) = self.materials.get(ui_mat_handle.index()) {
                            if let Some(ref ui_pipeline) = ui_material.pipeline {
                                encoder.bind_graphics_pipeline(ui_pipeline);

                                let dw = self.drawable_size.width as f32;
                                let dh = self.drawable_size.height as f32;
                                encoder.set_viewport(0.0, 0.0, dw, dh, 0.0, 1.0);

                                // UI vertices are in logical pixels; screen_size must match.
                                let [screen_w, screen_h] = ui_draw_list.screen_size;
                                let uniform_data: [f32; 4] = [screen_w, screen_h, -1.0, 0.0];
                                encoder.set_push_constants(
                                    bytemuck::cast_slice(&uniform_data),
                                    3,
                                    ShaderStages::VERTEX_FRAGMENT,
                                );

                                // Bind bindless texture argument buffer at index 9
                                if let Some(arg_buffer) = self.bindless_manager.argument_buffer() {
                                    unsafe {
                                        encoder.inner.setVertexBuffer_offset_atIndex(
                                            Some(arg_buffer),
                                            0,
                                            9,
                                        );
                                        encoder.inner.setFragmentBuffer_offset_atIndex(
                                            Some(arg_buffer),
                                            0,
                                            9,
                                        );
                                    }
                                }

                                // Bind shared sampler at index 0 for font sampling
                                if let Some(ref sampler) = self.shared_sampler {
                                    unsafe {
                                        encoder.inner.setFragmentSamplerState_atIndex(
                                            Some(&sampler.inner),
                                            0,
                                        );
                                    }
                                }

                                if let Some(ref vb) = self.ui_renderer.vertex_buffer() {
                                    encoder.bind_vertex_buffer(vb, 0, 10);
                                }
                                if let Some(ref ib) = self.ui_renderer.index_buffer() {
                                    encoder.bind_index_buffer(ib, 0, IndexType::Uint32);
                                }
                                self.ui_renderer.render_ui_commands(
                                    &mut encoder,
                                    &ui_draw_list,
                                    self.size.width,
                                    self.size.height,
                                );
                            }
                        }
                    }
                }
            }
        }

        encoder.end_encoding();

        cmd_buffer.end();
        self.context.surface.present(&cmd_buffer.inner);
        self.last_command_buffer = Some(cmd_buffer.inner.clone());
        cmd_buffer.submit(&self.context);

        Ok(())
    }

    fn begin_frame(&mut self) -> Result<u32, RendererError> {
        let texture = self.context.surface.acquire_next_drawable()?;
        self.current_drawable_texture = Some(texture);
        self.frame_index += 1;
        Ok(self.frame_index)
    }

    fn end_frame(&mut self) -> Result<(), RendererError> {
        self.current_drawable_texture = None;
        Ok(())
    }

    fn create_mesh<T, U>(&mut self, vertices: &[T], indices: &[U]) -> MeshHandle
    where
        T: bytemuck::Pod,
        U: bytemuck::Pod,
    {
        let vertex_bytes = bytemuck::cast_slice(vertices);
        let index_u32: Vec<u32> = indices
            .iter()
            .map(|v| {
                let bytes = bytemuck::bytes_of(v);
                match bytes.len() {
                    1 => bytes[0] as u32,
                    2 => u16::from_ne_bytes([bytes[0], bytes[1]]) as u32,
                    4 => u32::from_ne_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
                    _ => 0,
                }
            })
            .collect();

        let (vertex_buffer, index_buffer, index_count) = self
            .upload_vertex_index_data(vertex_bytes, &index_u32)
            .expect("Failed to create mesh buffers");

        let mesh = MetalMesh {
            vertex_buffer,
            index_buffer,
            index_count,
        };
        let id = self.meshes.insert(mesh);
        MeshHandle::new(id)
    }

    fn create_mesh_soa(
        &mut self,
        _attributes: &HashMap<u32, Vec<u8>>,
        _vertex_count: u32,
        _indices: &[u32],
    ) -> MeshHandle {
        MeshHandle::default()
    }

    fn register_mesh_raw(
        &mut self,
        vertex_data: &[u8],
        _vertex_count: u32,
        index_data: &[u32],
    ) -> MeshHandle {
        let (vertex_buffer, index_buffer, index_count) = self
            .upload_vertex_index_data(vertex_data, index_data)
            .expect("Failed to create mesh buffers");

        let mesh = MetalMesh {
            vertex_buffer,
            index_buffer,
            index_count,
        };
        let id = self.meshes.insert(mesh);
        MeshHandle::new(id)
    }

    fn create_cube_mesh(&mut self, size: [f32; 3]) -> MeshHandle {
        let (vertices, indices) = primitives::generate_cube(size);
        self.create_primitive_mesh(vertices, indices)
    }

    fn create_sphere_mesh(&mut self, radius: f32, segments: u32, rings: u32) -> MeshHandle {
        let (vertices, indices) = primitives::generate_sphere(radius, segments, rings);
        self.create_primitive_mesh(vertices, indices)
    }

    fn create_plane_mesh(&mut self, width: f32, height: f32) -> MeshHandle {
        let (vertices, indices) = primitives::generate_plane(width, height);
        self.create_primitive_mesh(vertices, indices)
    }

    fn create_cone_mesh(&mut self, height: f32, base_radius: f32, segments: u32) -> MeshHandle {
        let (vertices, indices) = primitives::generate_cone(height, base_radius, segments);
        self.create_primitive_mesh(vertices, indices)
    }

    fn create_cylinder_mesh(&mut self, height: f32, radius: f32, segments: u32) -> MeshHandle {
        let (vertices, indices) = primitives::generate_cylinder(height, radius, segments);
        self.create_primitive_mesh(vertices, indices)
    }

    fn create_torus_mesh(
        &mut self,
        major_radius: f32,
        minor_radius: f32,
        segments: u32,
        rings: u32,
    ) -> MeshHandle {
        let (vertices, indices) =
            primitives::generate_torus(major_radius, minor_radius, segments, rings);
        self.create_primitive_mesh(vertices, indices)
    }

    fn create_plane_xy_mesh(&mut self, width: f32, height: f32, segments: u32) -> MeshHandle {
        let (vertices, indices) = primitives::generate_plane_xy(width, height, segments);
        self.create_primitive_mesh(vertices, indices)
    }

    fn create_mesh_dynamic(
        &mut self,
        vertex_data: &[u8],
        _vertex_count: u32,
        indices: &[u32],
    ) -> MeshHandle {
        self.register_mesh_raw(vertex_data, _vertex_count, indices)
    }

    fn update_mesh_dynamic(
        &mut self,
        mesh: MeshHandle,
        vertex_data: &[u8],
        _vertex_count: u32,
        indices: &[u32],
    ) -> Result<(), RendererError> {
        let Some(m) = self.meshes.get_mut(mesh.index()) else {
            return Err(RendererError::NotFound("Mesh not found".into()));
        };
        {
            let ptr = m.vertex_buffer.map();
            unsafe {
                std::ptr::copy_nonoverlapping(
                    vertex_data.as_ptr(),
                    ptr,
                    vertex_data.len().min(m.vertex_buffer.size() as usize),
                );
            }
            m.vertex_buffer.unmap();
        }
        {
            let index_bytes = unsafe {
                std::slice::from_raw_parts(indices.as_ptr() as *const u8, indices.len() * 4)
            };
            let ptr = m.index_buffer.map();
            unsafe {
                std::ptr::copy_nonoverlapping(
                    index_bytes.as_ptr(),
                    ptr,
                    index_bytes.len().min(m.index_buffer.size() as usize),
                );
            }
            m.index_buffer.unmap();
        }
        m.index_count = indices.len() as u32;
        Ok(())
    }

    fn create_texture(&mut self, desc: &TextureDescriptor, data: &[u8]) -> TextureHandle {
        let result = if data.is_empty() {
            self.context.create_texture(desc)
        } else {
            self.context.create_texture_with_data(desc)
        };
        match result {
            Ok((texture, view)) => {
                if !data.is_empty() {
                    let region = objc2_metal::MTLRegion {
                        origin: objc2_metal::MTLOrigin { x: 0, y: 0, z: 0 },
                        size: objc2_metal::MTLSize {
                            width: desc.width as usize,
                            height: desc.height as usize,
                            depth: 1,
                        },
                    };
                    let bytes_per_row = desc.width as usize * 4;
                    unsafe {
                        texture
                            .inner
                            .replaceRegion_mipmapLevel_withBytes_bytesPerRow(
                                region,
                                0,
                                std::ptr::NonNull::new(data.as_ptr() as *mut std::ffi::c_void)
                                    .unwrap(),
                                bytes_per_row,
                            );
                    }
                }

                let bindless_slot = self.bindless_manager.register_texture(&texture.inner).ok();

                let entry = MetalTextureEntry {
                    _view: view,
                    bindless_slot,
                };
                let id = self.textures.insert(entry);
                TextureHandle::new(id)
            }
            Err(_) => self.default_texture(),
        }
    }

    fn create_texture_solid(&mut self, color: [u8; 4]) -> TextureHandle {
        let desc = TextureDescriptor::new(1, 1, ImageFormat::R8G8B8A8Srgb);
        self.create_texture(&desc, &color)
    }

    fn get_bindless_slot(&self, handle: TextureHandle) -> Option<u32> {
        self.textures
            .get(handle.index())
            .and_then(|entry| entry.bindless_slot)
    }

    fn get_texture_at_slot(&self, slot: u32) -> Option<TextureHandle> {
        for (idx, entry) in self.textures.iter().enumerate() {
            if entry.bindless_slot == Some(slot) {
                return Some(TextureHandle::new(idx as u32));
            }
        }
        None
    }

    fn get_texture_bindless_index(&self, handle: TextureHandle) -> u32 {
        self.get_bindless_slot(handle).unwrap_or(0)
    }

    fn default_texture(&self) -> TextureHandle {
        self.default_texture.unwrap_or(TextureHandle::default())
    }

    fn compile_material(
        &mut self,
        shader_path: &str,
        vertex_type: &str,
    ) -> Result<MaterialHandle, RendererError> {
        let wgsl_source = read_shader(shader_path)?;

        log::debug!(
            "compile_material: shader_path={}, wgsl_size={} bytes",
            shader_path,
            wgsl_source.len()
        );
        if wgsl_source.contains("pbr_lighting") {
            log::debug!("compile_material: WGSL contains PBR lighting code");
        }

        let entry_points = match vertex_type {
            "compute" => vec!["cs_main"],
            _ => vec!["vs_main", "fs_main"],
        };

        let is_ui = vertex_type == "ui";

        let compiled = shader::compile_wgsl_to_metal(
            &self.context.device,
            &wgsl_source,
            &entry_points,
            is_ui,
        )?;

        let vertex_fn = compiled
            .module
            .entry_points
            .get("vs_main")
            .or_else(|| compiled.module.entry_points.get("cs_main"))
            .ok_or_else(|| {
                RendererError::InvalidOperation("Vertex entry point not found".into())
            })?;

        let fragment_fn = compiled.module.entry_points.get("fs_main");

        let color_formats = &[MTLPixelFormat::BGRA8Unorm_sRGB];
        let depth_format = if is_ui {
            Some(MTLPixelFormat::Depth32Float)
        } else {
            Some(MTLPixelFormat::Depth32Float)
        };

        let pipeline = if is_ui {
            let vd = super::context::ui_vertex_descriptor();
            self.context
                .create_graphics_pipeline_with_vertex_descriptor(
                    vertex_fn,
                    fragment_fn
                        .as_ref()
                        .map(|f| f.as_ref() as &ProtocolObject<dyn objc2_metal::MTLFunction>),
                    color_formats,
                    depth_format,
                    false,
                    crate::pipeline::CompareOp::Always,
                    objc2_metal::MTLCullMode::None,
                    objc2_metal::MTLWinding::CounterClockwise,
                    Some(&vd),
                    true,
                )?
        } else {
            self.context.create_graphics_pipeline(
                vertex_fn,
                fragment_fn
                    .as_ref()
                    .map(|f| f.as_ref() as &ProtocolObject<dyn objc2_metal::MTLFunction>),
                color_formats,
                depth_format,
                true,
                crate::pipeline::CompareOp::GreaterOrEqual,
                objc2_metal::MTLCullMode::Back,
                objc2_metal::MTLWinding::CounterClockwise,
            )?
        };

        let material = MetalMaterial {
            pipeline: Some(pipeline),
            texture_indices: [0; 4],
        };
        let id = self.materials.insert(material);
        Ok(MaterialHandle::new(id))
    }

    fn set_material_texture_indices(&mut self, material: MaterialHandle, indices: [u32; 4]) {
        if let Some(mat) = self.materials.get_mut(material.index()) {
            mat.texture_indices = indices;
        }
    }

    fn default_material(&self) -> MaterialHandle {
        self.default_material.unwrap_or(MaterialHandle::default())
    }

    fn destroy_mesh(&mut self, handle: MeshHandle) {
        self.meshes.remove(handle.index());
    }

    fn destroy_material(&mut self, handle: MaterialHandle) {
        self.materials.remove(handle.index());
    }

    fn destroy_texture(&mut self, handle: TextureHandle) {
        if let Some(entry) = self.textures.remove(handle.index()) {
            if let Some(slot) = entry.bindless_slot {
                self.bindless_manager.release_slot(slot);
            }
        }
    }

    fn destroy_skeleton(&mut self, handle: SkeletonHandle) {
        self.skeletons.remove(handle.index());
    }

    fn create_viewport(&mut self) -> ViewportBuilder {
        ViewportBuilder::new()
    }

    fn viewport_count(&self) -> usize {
        self.viewports.len()
    }

    fn get_viewport(&self, handle: ViewportHandle) -> Option<&Viewport> {
        self.viewports.get(handle.0)
    }

    fn viewport_extent(&self, handle: ViewportHandle) -> Option<Size2D> {
        self.viewports.get(handle.0).map(|v| v.extent)
    }

    fn destroy_viewport(&mut self, handle: ViewportHandle) {
        if handle.0 < self.viewports.len() {
            // Viewports don't have GPU resources to free, just leave the slot
        }
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

    fn create_skeleton(&mut self, joint_count: usize) -> Result<SkeletonHandle, RendererError> {
        let buffer_size = (joint_count * 64) as u64;
        let buffer = self.context.create_buffer(buffer_size, true)?;
        let id = self.skeletons.insert(buffer);
        Ok(SkeletonHandle::new(id))
    }

    fn update_skeleton(&mut self, handle: SkeletonHandle, matrices: &[[f32; 16]]) {
        let Some(buffer) = self.skeletons.get_mut(handle.index()) else {
            return;
        };
        let ptr = buffer.map();
        let matrices_bytes = unsafe {
            std::slice::from_raw_parts(matrices.as_ptr() as *const u8, matrices.len() * 64)
        };
        unsafe {
            std::ptr::copy_nonoverlapping(matrices_bytes.as_ptr(), ptr, matrices_bytes.len());
        }
        buffer.unmap();
    }

    fn init_particle_system(&mut self) -> Result<(), RendererError> {
        const MAX_PARTICLES: u32 = 1_048_576; // Must match WGSL MAX_PARTICLES
        let subsystem = MetalParticleSubsystem::new(&self.context, MAX_PARTICLES)?;
        self.particle_system = Some(subsystem);
        Ok(())
    }

    fn create_ui_font_atlas(&mut self, width: u32, height: u32, data: &[u8]) -> TextureHandle {
        let desc = TextureDescriptor::new(width, height, ImageFormat::R8G8B8A8Srgb);
        let handle = self.create_texture(&desc, data);
        self.ui_font_atlas = Some(handle);
        handle
    }

    fn update_ui_font_atlas(&mut self, width: u32, height: u32, data: &[u8]) {
        if let Some(atlas_handle) = self.ui_font_atlas {
            self.destroy_texture(atlas_handle);
        }
        let desc = TextureDescriptor::new(width, height, ImageFormat::R8G8B8A8Srgb);
        let handle = self.create_texture(&desc, data);
        self.ui_font_atlas = Some(handle);
    }

    fn ui_font_atlas_handle(&self) -> Option<TextureHandle> {
        self.ui_font_atlas
    }

    // -- Lighting --

    fn upload_lights(&mut self, lights: &[crate::renderer::types::PointLightGPU]) {
        MetalRenderer::upload_lights(self, lights);
    }

    fn has_light_culling(&self) -> bool {
        MetalRenderer::has_light_culling(self)
    }

    // -- Shadows --

    fn update_shadows(&mut self, light_direction: [f32; 3]) {
        MetalRenderer::update_shadows(self, light_direction);
    }

    fn upload_shadow_cascades(&mut self) {
        let Some(ref shadow_buf) = self.shadow_cascade_buffer else {
            return;
        };
        // ShadowFrameData layout (must match WGSL):
        //   cascades[4]: each { view_proj: mat4x4f(64B), split_distance: f32, texel_size: f32, pad: vec2f } = 80B
        //   light_direction: vec4f(16B)
        //   shadow_bias: vec4f(16B)
        let cascade_stride: usize = 80;
        let ptr = shadow_buf.map();
        unsafe {
            std::ptr::write_bytes(ptr, 0, cascade_stride * 4 + 16 + 16);
            for i in 0..self.shadow.cascade_count() as usize {
                let vp = self.shadow.cascade_view_proj(i);
                let sd = self.shadow.cascade_split_depth(i);
                let base = ptr.add(i * cascade_stride);
                std::ptr::copy_nonoverlapping(vp.as_ptr(), base as *mut f32, 16);
                std::ptr::write(base.add(64) as *mut f32, sd);
            }
            let light_dir = self.frame_uniforms.light_direction;
            std::ptr::copy_nonoverlapping(
                light_dir.as_ptr(),
                ptr.add(cascade_stride * 4) as *mut f32,
                4,
            );
            // Set num_cascades in light_direction.w so sample_shadow returns early
            *(ptr.add(cascade_stride * 4) as *mut f32).add(3) = self.shadow.cascade_count() as f32;
            let bias: [f32; 4] = [0.005, 0.0, 0.0, 0.0];
            std::ptr::copy_nonoverlapping(
                bias.as_ptr(),
                ptr.add(cascade_stride * 4 + 16) as *mut f32,
                4,
            );
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

    // -- UI Rendering --

    fn set_ui_material(&mut self, material: MaterialHandle) {
        self.ui_renderer.set_ui_material(material);
    }

    fn render_ui_pass(&mut self, draw_list: crate::renderer::types::UIDrawList) {
        MetalRenderer::render_ui_pass(self, draw_list);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::gpu_renderer::GpuRenderer;
    use crate::renderer::types::{DrawCall, DrawList};

    fn create_renderer() -> MetalRenderer {
        let context = MetalContext::init_headless().expect("Failed to create headless context");
        MetalRenderer::new(context).expect("Failed to create MetalRenderer")
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

        let cube = renderer.create_cube_mesh([1.0, 1.0, 1.0]);
        assert!(cube.is_some(), "cube handle should be valid");

        let sphere = renderer.create_sphere_mesh(1.0, 16, 16);
        assert!(sphere.is_some(), "sphere handle should be valid");

        let plane = renderer.create_plane_mesh(2.0, 2.0);
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

        let default_mesh = renderer.create_cube_mesh([1.0, 1.0, 1.0]);
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
}
