use super::FRAMES_IN_FLIGHT;
use crate::RendererError;
use crate::handle::PipelineHandle;
use crate::pipeline::{CompareOp, CullMode, FrontFace};
use crate::renderer::registry::AssetRegistry;
use crate::texture::ImageFormat;
use crate::vulkan::context::VulkanContext;
use crate::vulkan::material::builder::PipelineBuilder;
use crate::vulkan::material::compiler::MaterialCompiler;
use crate::vulkan::material::storage_uniform::StorageDescriptorSet;
use crate::vulkan::vertexbinding::VertexFormat;
use ash::vk;
use log::info;
use std::rc::Rc;

/// Push constants for outline draw pipelines.
///
/// Layout must match `OutlinePushConstants` in outline_draw.wgsl:
/// - offset 0: outline_width (f32) + 3 x padding (f32) = 16 bytes
/// - offset 16: outline_color (vec4) = 16 bytes
///
/// Total: 32 bytes
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct OutlinePushConstants {
    pub outline_width: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
    pub outline_color: [f32; 4],
}

impl Default for OutlinePushConstants {
    fn default() -> Self {
        Self {
            outline_width: 0.004,
            _pad0: 0.0,
            _pad1: 0.0,
            _pad2: 0.0,
            outline_color: [1.0, 0.55, 0.0, 1.0],
        }
    }
}

/// Compute a viewport-aware outline width in NDC.
/// The base width (0.004) targets ~1080p; scale inversely with viewport height
/// so outlines remain a consistent pixel width across resolutions.
pub(crate) fn compute_outline_width(viewport_height: f32) -> f32 {
    const BASE_HEIGHT: f32 = 1080.0;
    const BASE_WIDTH: f32 = 0.004;
    BASE_WIDTH * (BASE_HEIGHT / viewport_height)
}

#[derive(Default)]
/// Owns all outline highlight GPU state for stencil-based selection.
///
/// Lifecycle:
/// - `init_outline_pipelines()` — creates params resources + 6 outline pipelines
/// - `init_stencil_indicator_pipelines()` — creates 2 stencil indicator pipelines
/// - `destroy()` — tears down params pool, buffers, descriptor layout
pub(crate) struct OutlineSubsystem {
    /// Pipeline for stencil mark pass (writes ref=1 to stencil for visible selected objects).
    pub stencil_mark_pipeline: PipelineHandle,
    /// Skinned stencil mark pipeline.
    pub stencil_mark_skinned_pipeline: PipelineHandle,
    /// Pipeline for occlusion mark pass (promotes stencil 1→2 where selected objects are occluded).
    pub occlusion_mark_pipeline: PipelineHandle,
    /// Skinned occlusion mark pipeline.
    pub occlusion_mark_skinned_pipeline: PipelineHandle,
    /// Pipeline for outline draw pass (inverted culling, stencil != 1).
    pub outline_draw_pipeline: PipelineHandle,
    /// Skinned outline draw pipeline.
    pub outline_draw_skinned_pipeline: PipelineHandle,
    /// Pipeline for stencil indicator pass (writes R8 where stencil == 2).
    pub stencil_indicator_pipeline: PipelineHandle,
    /// Skinned stencil indicator pipeline.
    pub stencil_indicator_skinned_pipeline: PipelineHandle,
    /// Descriptor set layout for outline params uniform buffer.
    pub params_descriptor_layout: vk::DescriptorSetLayout,
    /// Per-frame descriptor sets for outline params (set 1 non-skinned, set 3 skinned).
    pub params_descriptor_sets: Vec<vk::DescriptorSet>,
    /// Per-frame uniform buffers for outline params.
    pub params_buffers: Vec<vk::Buffer>,
    /// Per-frame buffer allocations for outline params.
    pub params_allocations: Vec<gpu_allocator::vulkan::Allocation>,
    /// Descriptor pool for outline params.
    pub params_descriptor_pool: vk::DescriptorPool,
}

/// Dependencies needed from VulkanRenderer for outline subsystem initialization.
pub(crate) struct OutlineInitContext<'a> {
    pub context: &'a Rc<VulkanContext>,
    pub material_compiler: &'a mut MaterialCompiler,
    pub storage_descriptor_set: &'a StorageDescriptorSet,
    pub shared_empty_descriptor_layout: vk::DescriptorSetLayout,
    pub asset_registry: &'a mut AssetRegistry,
}

/// Parameters for building an outline pipeline.
struct OutlinePipelineParams {
    storage_layout: vk::DescriptorSetLayout,
    stencil_state: vk::StencilOpState,
    depth_compare: CompareOp,
    cull_mode: CullMode,
    color_format: ImageFormat,
    color_write_mask: vk::ColorComponentFlags,
    empty_layout: Option<vk::DescriptorSetLayout>,
    params_layout: Option<vk::DescriptorSetLayout>,
}

impl OutlineSubsystem {
    fn build_outline_pipeline(
        ctx: &mut OutlineInitContext,
        vert: vk::ShaderModule,
        frag: vk::ShaderModule,
        params: &OutlinePipelineParams,
    ) -> Result<PipelineHandle, RendererError> {
        let mut builder = PipelineBuilder::new(ctx.context.clone())
            .with_shaders(vert, frag)
            .with_soa_attribute(0, VertexFormat::RGB32f)
            .with_depth_test(true, false, params.depth_compare)
            .with_cull_mode(params.cull_mode, FrontFace::CounterClockwise)
            .with_stencil_test(params.stencil_state, params.stencil_state)
            .with_color_write_mask(params.color_write_mask)
            .with_rendering_formats(
                Some(params.color_format),
                Some(ImageFormat::D32SfloatS8Uint),
            );

        if let Some(empty_layout) = params.empty_layout {
            let skeleton_layout = ctx.material_compiler.skeleton_descriptor_layout();
            if let Some(params_layout) = params.params_layout {
                builder = builder.with_descriptor_layouts(vec![
                    params.storage_layout,
                    empty_layout,
                    skeleton_layout,
                    params_layout,
                ]);
            } else {
                builder = builder.with_descriptor_layouts(vec![
                    params.storage_layout,
                    empty_layout,
                    skeleton_layout,
                ]);
            }
            builder = builder
                .with_soa_attribute(4, VertexFormat::RGBA16u)
                .with_soa_attribute(5, VertexFormat::RGBA32f);
        } else {
            if let Some(params_layout) = params.params_layout {
                builder =
                    builder.with_descriptor_layouts(vec![params.storage_layout, params_layout]);
            } else {
                builder = builder.with_descriptor_layouts(vec![params.storage_layout]);
            }
        }

        let pipeline = builder.build_dynamic().map_err(|e| {
            RendererError::InitializationFailed(format!(
                "Failed to build outline pipeline: {:?}",
                e
            ))
        })?;

        Ok(ctx.asset_registry.register_pipeline(pipeline))
    }

    fn load_outline_shaders(
        material_compiler: &MaterialCompiler,
        path: &std::path::Path,
        name: &str,
    ) -> Result<(vk::ShaderModule, vk::ShaderModule), RendererError> {
        let mut cache = material_compiler.shader_cache.borrow_mut();
        let vert = cache
            .load_shader(path, vk::ShaderStageFlags::VERTEX)
            .map_err(|e| {
                RendererError::InitializationFailed(format!(
                    "Failed to load {} vertex shader: {:?}",
                    name, e
                ))
            })?;
        let frag = cache
            .load_shader(path, vk::ShaderStageFlags::FRAGMENT)
            .map_err(|e| {
                RendererError::InitializationFailed(format!(
                    "Failed to load {} fragment shader: {:?}",
                    name, e
                ))
            })?;
        Ok((vert, frag))
    }

    /// Initialize outline params resources and all outline pipelines.
    ///
    /// Creates the outline params uniform buffer (per-frame) and builds 6 pipelines:
    /// stencil mark, skinned stencil mark, occlusion mark, skinned occlusion mark,
    /// outline draw, skinned outline draw.
    pub fn init_outline_pipelines(
        &mut self,
        ctx: &mut OutlineInitContext,
        stencil_mark_path: &std::path::Path,
        stencil_mark_skinned_path: &std::path::Path,
        outline_draw_path: &std::path::Path,
        outline_draw_skinned_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        let storage_layout = ctx.storage_descriptor_set.layout();
        let device = &ctx.context.device;

        // Create outline params uniform buffer resources.
        // Used by the outline draw pipelines (non-skinned: set 1, skinned: set 3).
        let params_size = std::mem::size_of::<OutlinePushConstants>() as u64;

        let params_descriptor_layout = unsafe {
            let bindings = [vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)];
            device
                .create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
                    None,
                )
                .map_err(|e| {
                    RendererError::InitializationFailed(format!(
                        "Failed to create outline params descriptor layout: {:?}",
                        e
                    ))
                })?
        };

        let pool_sizes = [vk::DescriptorPoolSize::default()
            .ty(vk::DescriptorType::UNIFORM_BUFFER)
            .descriptor_count(FRAMES_IN_FLIGHT as u32)];
        let params_descriptor_pool = unsafe {
            device
                .create_descriptor_pool(
                    &vk::DescriptorPoolCreateInfo::default()
                        .pool_sizes(&pool_sizes)
                        .max_sets(FRAMES_IN_FLIGHT as u32),
                    None,
                )
                .map_err(|e| {
                    RendererError::InitializationFailed(format!(
                        "Failed to create outline params descriptor pool: {:?}",
                        e
                    ))
                })?
        };

        let layouts = (0..FRAMES_IN_FLIGHT)
            .map(|_| params_descriptor_layout)
            .collect::<Vec<_>>();
        let params_descriptor_sets = unsafe {
            device
                .allocate_descriptor_sets(
                    &vk::DescriptorSetAllocateInfo::default()
                        .descriptor_pool(params_descriptor_pool)
                        .set_layouts(&layouts),
                )
                .map_err(|e| {
                    RendererError::InitializationFailed(format!(
                        "Failed to allocate outline params descriptor sets: {:?}",
                        e
                    ))
                })?
        };

        let mut params_buffers = Vec::with_capacity(FRAMES_IN_FLIGHT);
        let mut params_allocations = Vec::with_capacity(FRAMES_IN_FLIGHT);

        for &params_ds in params_descriptor_sets.iter() {
            let buffer_info = vk::BufferCreateInfo::default()
                .size(params_size)
                .usage(vk::BufferUsageFlags::UNIFORM_BUFFER)
                .sharing_mode(vk::SharingMode::EXCLUSIVE);

            let (buffer, allocation) = ctx
                .context
                .allocate_buffer(&buffer_info, gpu_allocator::MemoryLocation::CpuToGpu)
                .expect("Failed to allocate outline params buffer");

            unsafe {
                let ptr = ctx
                    .context
                    .map_buffer(&allocation)
                    .expect("Failed to map buffer");
                let defaults = OutlinePushConstants::default();
                std::ptr::copy_nonoverlapping(
                    &defaults as *const _ as *const u8,
                    ptr,
                    params_size as usize,
                );
            }
            let _ = ctx.context.flush_mapped_memory(&allocation, 0, params_size);

            let buffer_info = [vk::DescriptorBufferInfo::default()
                .buffer(buffer)
                .offset(0)
                .range(params_size)];

            unsafe {
                device.update_descriptor_sets(
                    &[vk::WriteDescriptorSet::default()
                        .dst_set(params_ds)
                        .dst_binding(0)
                        .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                        .descriptor_count(1)
                        .buffer_info(&buffer_info)],
                    &[],
                );
            }

            params_buffers.push(buffer);
            params_allocations.push(allocation);
        }

        self.params_descriptor_layout = params_descriptor_layout;
        self.params_descriptor_sets = params_descriptor_sets;
        self.params_buffers = params_buffers;
        self.params_allocations = params_allocations;
        self.params_descriptor_pool = params_descriptor_pool;

        // === Stencil Mark Pipeline ===
        {
            let (vert, frag) = Self::load_outline_shaders(
                ctx.material_compiler,
                stencil_mark_path,
                "stencil mark",
            )?;
            let stencil_state = vk::StencilOpState {
                fail_op: vk::StencilOp::KEEP,
                pass_op: vk::StencilOp::REPLACE,
                depth_fail_op: vk::StencilOp::KEEP,
                compare_op: vk::CompareOp::ALWAYS,
                compare_mask: 0xFF,
                write_mask: 0x01,
                reference: 1,
            };
            let handle = Self::build_outline_pipeline(
                ctx,
                vert,
                frag,
                &OutlinePipelineParams {
                    storage_layout,
                    stencil_state,
                    depth_compare: CompareOp::GreaterOrEqual,
                    cull_mode: CullMode::Back,
                    color_format: ImageFormat::R16G16B16A16Sfloat,
                    color_write_mask: vk::ColorComponentFlags::empty(),
                    empty_layout: None,
                    params_layout: None,
                },
            )?;
            self.stencil_mark_pipeline = handle;
        }

        // === Skinned Stencil Mark Pipeline ===
        {
            let (vert, frag) = Self::load_outline_shaders(
                ctx.material_compiler,
                stencil_mark_skinned_path,
                "skinned stencil mark",
            )?;

            let stencil_state = vk::StencilOpState {
                fail_op: vk::StencilOp::KEEP,
                pass_op: vk::StencilOp::REPLACE,
                depth_fail_op: vk::StencilOp::KEEP,
                compare_op: vk::CompareOp::ALWAYS,
                compare_mask: 0xFF,
                write_mask: 0x01,
                reference: 1,
            };
            let handle = Self::build_outline_pipeline(
                ctx,
                vert,
                frag,
                &OutlinePipelineParams {
                    storage_layout,
                    stencil_state,
                    depth_compare: CompareOp::GreaterOrEqual,
                    cull_mode: CullMode::Back,
                    color_format: ImageFormat::R16G16B16A16Sfloat,
                    color_write_mask: vk::ColorComponentFlags::empty(),
                    empty_layout: Some(ctx.shared_empty_descriptor_layout),
                    params_layout: None,
                },
            )?;
            self.stencil_mark_skinned_pipeline = handle;
        }

        // === Occlusion Mark Pipeline ===
        {
            let (vert, frag) = Self::load_outline_shaders(
                ctx.material_compiler,
                stencil_mark_path,
                "occlusion mark",
            )?;
            let stencil_state = vk::StencilOpState {
                fail_op: vk::StencilOp::KEEP,
                pass_op: vk::StencilOp::KEEP,
                depth_fail_op: vk::StencilOp::REPLACE,
                compare_op: vk::CompareOp::EQUAL,
                compare_mask: 0x01,
                write_mask: 0x02,
                reference: 2,
            };
            let handle = Self::build_outline_pipeline(
                ctx,
                vert,
                frag,
                &OutlinePipelineParams {
                    storage_layout,
                    stencil_state,
                    depth_compare: CompareOp::GreaterOrEqual,
                    cull_mode: CullMode::Back,
                    color_format: ImageFormat::R16G16B16A16Sfloat,
                    color_write_mask: vk::ColorComponentFlags::empty(),
                    empty_layout: None,
                    params_layout: None,
                },
            )?;
            self.occlusion_mark_pipeline = handle;
        }

        // === Skinned Occlusion Mark Pipeline ===
        {
            let (vert, frag) = Self::load_outline_shaders(
                ctx.material_compiler,
                stencil_mark_skinned_path,
                "skinned occlusion mark",
            )?;

            let stencil_state = vk::StencilOpState {
                fail_op: vk::StencilOp::KEEP,
                pass_op: vk::StencilOp::KEEP,
                depth_fail_op: vk::StencilOp::REPLACE,
                compare_op: vk::CompareOp::EQUAL,
                compare_mask: 0x01,
                write_mask: 0x02,
                reference: 2,
            };
            let handle = Self::build_outline_pipeline(
                ctx,
                vert,
                frag,
                &OutlinePipelineParams {
                    storage_layout,
                    stencil_state,
                    depth_compare: CompareOp::GreaterOrEqual,
                    cull_mode: CullMode::Back,
                    color_format: ImageFormat::R16G16B16A16Sfloat,
                    color_write_mask: vk::ColorComponentFlags::empty(),
                    empty_layout: Some(ctx.shared_empty_descriptor_layout),
                    params_layout: None,
                },
            )?;
            self.occlusion_mark_skinned_pipeline = handle;
        }

        // === Outline Draw Pipeline ===
        {
            let (vert, frag) = Self::load_outline_shaders(
                ctx.material_compiler,
                outline_draw_path,
                "outline draw",
            )?;
            let stencil_state = vk::StencilOpState {
                fail_op: vk::StencilOp::KEEP,
                pass_op: vk::StencilOp::KEEP,
                depth_fail_op: vk::StencilOp::KEEP,
                compare_op: vk::CompareOp::EQUAL,
                compare_mask: 0xFF,
                write_mask: 0x00,
                reference: 0,
            };
            let handle = Self::build_outline_pipeline(
                ctx,
                vert,
                frag,
                &OutlinePipelineParams {
                    storage_layout,
                    stencil_state,
                    depth_compare: CompareOp::GreaterOrEqual,
                    cull_mode: CullMode::Front,
                    color_format: ImageFormat::R16G16B16A16Sfloat,
                    color_write_mask: vk::ColorComponentFlags::R
                        | vk::ColorComponentFlags::G
                        | vk::ColorComponentFlags::B
                        | vk::ColorComponentFlags::A,
                    empty_layout: None,
                    params_layout: Some(self.params_descriptor_layout),
                },
            )?;
            self.outline_draw_pipeline = handle;
        }

        // === Skinned Outline Draw Pipeline ===
        {
            let (vert, frag) = Self::load_outline_shaders(
                ctx.material_compiler,
                outline_draw_skinned_path,
                "skinned outline draw",
            )?;

            let stencil_state = vk::StencilOpState {
                fail_op: vk::StencilOp::KEEP,
                pass_op: vk::StencilOp::KEEP,
                depth_fail_op: vk::StencilOp::KEEP,
                compare_op: vk::CompareOp::EQUAL,
                compare_mask: 0xFF,
                write_mask: 0x00,
                reference: 0,
            };
            let handle = Self::build_outline_pipeline(
                ctx,
                vert,
                frag,
                &OutlinePipelineParams {
                    storage_layout,
                    stencil_state,
                    depth_compare: CompareOp::GreaterOrEqual,
                    cull_mode: CullMode::Front,
                    color_format: ImageFormat::R16G16B16A16Sfloat,
                    color_write_mask: vk::ColorComponentFlags::R
                        | vk::ColorComponentFlags::G
                        | vk::ColorComponentFlags::B
                        | vk::ColorComponentFlags::A,
                    empty_layout: Some(ctx.shared_empty_descriptor_layout),
                    params_layout: Some(self.params_descriptor_layout),
                },
            )?;
            self.outline_draw_skinned_pipeline = handle;
        }

        info!("Outline pipelines initialized (stencil-based selection highlight)");

        Ok(())
    }

    /// Initialize stencil indicator pipelines for wallhack overlay.
    ///
    /// Creates 2 pipelines: stencil indicator (non-skinned) and skinned stencil indicator.
    pub fn init_stencil_indicator_pipelines(
        &mut self,
        ctx: &mut OutlineInitContext,
        shader_path: &std::path::Path,
        skinned_shader_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        let storage_layout = ctx.storage_descriptor_set.layout();

        {
            let (vert, frag) = Self::load_outline_shaders(
                ctx.material_compiler,
                shader_path,
                "stencil indicator",
            )?;
            let stencil_state = vk::StencilOpState {
                fail_op: vk::StencilOp::KEEP,
                pass_op: vk::StencilOp::KEEP,
                depth_fail_op: vk::StencilOp::KEEP,
                compare_op: vk::CompareOp::EQUAL,
                compare_mask: 0xFF,
                write_mask: 0x00,
                reference: 2,
            };
            let handle = Self::build_outline_pipeline(
                ctx,
                vert,
                frag,
                &OutlinePipelineParams {
                    storage_layout,
                    stencil_state,
                    depth_compare: CompareOp::Always,
                    cull_mode: CullMode::Back,
                    color_format: ImageFormat::R8Unorm,
                    color_write_mask: vk::ColorComponentFlags::R
                        | vk::ColorComponentFlags::G
                        | vk::ColorComponentFlags::B
                        | vk::ColorComponentFlags::A,
                    empty_layout: None,
                    params_layout: None,
                },
            )?;
            self.stencil_indicator_pipeline = handle;
        }

        {
            let (vert, frag) = Self::load_outline_shaders(
                ctx.material_compiler,
                skinned_shader_path,
                "skinned stencil indicator",
            )?;

            let stencil_state = vk::StencilOpState {
                fail_op: vk::StencilOp::KEEP,
                pass_op: vk::StencilOp::KEEP,
                depth_fail_op: vk::StencilOp::KEEP,
                compare_op: vk::CompareOp::EQUAL,
                compare_mask: 0xFF,
                write_mask: 0x00,
                reference: 2,
            };
            let handle = Self::build_outline_pipeline(
                ctx,
                vert,
                frag,
                &OutlinePipelineParams {
                    storage_layout,
                    stencil_state,
                    depth_compare: CompareOp::Always,
                    cull_mode: CullMode::Back,
                    color_format: ImageFormat::R8Unorm,
                    color_write_mask: vk::ColorComponentFlags::R
                        | vk::ColorComponentFlags::G
                        | vk::ColorComponentFlags::B
                        | vk::ColorComponentFlags::A,
                    empty_layout: Some(ctx.shared_empty_descriptor_layout),
                    params_layout: None,
                },
            )?;
            self.stencil_indicator_skinned_pipeline = handle;
        }

        info!("Stencil indicator pipelines initialized");

        Ok(())
    }

    /// Destroy all outline GPU resources.
    ///
    /// Frees the descriptor pool, per-frame buffers, and descriptor layout.
    pub fn destroy(&mut self, context: &Rc<VulkanContext>) {
        if self.params_descriptor_pool != vk::DescriptorPool::null() {
            unsafe {
                context
                    .device
                    .destroy_descriptor_pool(self.params_descriptor_pool, None);
            }
            self.params_descriptor_pool = vk::DescriptorPool::null();
        }
        for (buffer, allocation) in self
            .params_buffers
            .drain(..)
            .zip(self.params_allocations.drain(..))
        {
            context.free_buffer(buffer, allocation);
        }
        if self.params_descriptor_layout != vk::DescriptorSetLayout::null() {
            unsafe {
                context
                    .device
                    .destroy_descriptor_set_layout(self.params_descriptor_layout, None);
            }
            self.params_descriptor_layout = vk::DescriptorSetLayout::null();
        }
    }
}

impl super::VulkanRenderer {
    /// Initialize outline params resources and all outline pipelines.
    ///
    /// Delegates to [`OutlineSubsystem::init_outline_pipelines`].
    pub fn init_outline_pipelines(
        &mut self,
        stencil_mark_path: &std::path::Path,
        stencil_mark_skinned_path: &std::path::Path,
        outline_draw_path: &std::path::Path,
        outline_draw_skinned_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        let mut ctx = super::outline::OutlineInitContext {
            context: &self.context,
            material_compiler: &mut self.material_compiler,
            storage_descriptor_set: &self.storage_descriptor_sets[0],
            shared_empty_descriptor_layout: self.shared_empty_descriptor_layout,
            asset_registry: &mut self.asset_registry,
        };
        self.outline.init_outline_pipelines(
            &mut ctx,
            stencil_mark_path,
            stencil_mark_skinned_path,
            outline_draw_path,
            outline_draw_skinned_path,
        )
    }

    /// Initialize stencil indicator pipelines for wallhack overlay.
    ///
    /// Delegates to [`OutlineSubsystem::init_stencil_indicator_pipelines`].
    pub fn init_stencil_indicator_pipelines(
        &mut self,
        shader_path: &std::path::Path,
        skinned_shader_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        let mut ctx = super::outline::OutlineInitContext {
            context: &self.context,
            material_compiler: &mut self.material_compiler,
            storage_descriptor_set: &self.storage_descriptor_sets[0],
            shared_empty_descriptor_layout: self.shared_empty_descriptor_layout,
            asset_registry: &mut self.asset_registry,
        };
        self.outline
            .init_stencil_indicator_pipelines(&mut ctx, shader_path, skinned_shader_path)
    }
}
