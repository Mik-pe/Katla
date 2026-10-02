//! Application-authored graphics pipelines and per-pass shader data.

use katla_gfx::ShaderStages;
use katla_gfx::render_graph::{ImageSubresourceRange, PassId};
use katla_gfx::renderer::PipelineStages;
use katla_gfx::renderer::frame_bindings::{
    ConstantBinding, ImageBinding, PassBindings, PassDraw, PassDrawPhase, PassPipeline,
    SamplerBinding, SamplingMode,
};
use katla_gfx::renderer::pipeline_descriptor::{
    ColorWriteMask, DepthBias, StencilFaceState, StencilOperation, StencilState,
};
use katla_gfx::shadow::CascadeShadowMap;
use katla_gfx::{
    BlendMode, CascadeParams, CompareOp, CullMode, DepthState, GpuRenderer, ImageFormat,
    MaterialHandle, PipelineDescriptor, Rect, Size2D, VertexLayout,
};

use super::{LightFeatures, ParticleFeatures};
use crate::application::{PassIds, frame_graph_config::FrameGraphBindings};
use crate::{AppResult, FrameGraph, Renderer, resources::ResourceManager};

pub(crate) struct GraphicsFrame<'a> {
    pub(crate) ids: &'a PassIds,
    pub(crate) bindings: &'a FrameGraphBindings,
    pub(crate) uniforms: &'a crate::rendering::FrameUniforms,
    pub(crate) lights: &'a LightFeatures,
    pub(crate) particles: &'a ParticleFeatures,
    pub(crate) selected: Vec<u32>,
    pub(crate) ordinary: Vec<u32>,
    pub(crate) billboards: Vec<u32>,
    pub(crate) frame_slot: usize,
    pub(crate) scene_size: Size2D,
}

pub(crate) struct SceneGraphics {
    depth: Vec<PassPipeline>,
    picking: Vec<PassPipeline>,
    billboard_depth: MaterialHandle,
    billboard_picking: MaterialHandle,
    shadow: Vec<PassPipeline>,
    stencil_mark: Vec<PassPipeline>,
    occlusion_mark: Vec<PassPipeline>,
    outline: Vec<PassPipeline>,
    stencil_indicator: Vec<PassPipeline>,
    sky: MaterialHandle,
    tonemap: MaterialHandle,
    overlay: Option<MaterialHandle>,
    particle: MaterialHandle,
    cascades: CascadeShadowMap,
    flip_y: bool,
}

impl SceneGraphics {
    pub(crate) fn new(
        renderer: &mut Renderer,
        resources: &ResourceManager,
        bindings: &FrameGraphBindings,
    ) -> AppResult<Self> {
        let flip_y = !renderer.capabilities().clip_y_down;
        let cascade_params = CascadeParams {
            shadow_map_size: if flip_y { 2048 } else { 4096 },
            ..CascadeParams::default()
        };
        let depth_state = DepthState {
            test: true,
            write: false,
            compare: CompareOp::GreaterOrEqual,
        };
        let depth = compile_pair(
            renderer,
            resources,
            "depth_prepass",
            ImageFormat::Auto,
            |descriptor| {
                descriptor.color_attachment = false;
                descriptor.depth_format = Some(ImageFormat::D32SfloatS8Uint);
                descriptor.stages = PipelineStages::Graphics {
                    vertex_entry: "vs_main".into(),
                    fragment_entry: None,
                };
            },
        )?;
        let picking = compile_pair(
            renderer,
            resources,
            "picking/object_id",
            ImageFormat::R32Uint,
            |descriptor| descriptor.depth = depth_state,
        )?;
        let billboard_descriptor = PipelineDescriptor::billboard(
            resources
                .shader_path("billboard_depth.wgsl")
                .to_string_lossy()
                .into_owned(),
        )
        .with_blend(BlendMode::Opaque)
        .with_color_attachment(false)
        .with_color_format(ImageFormat::Auto);
        let billboard_depth = renderer.compile_material(&billboard_descriptor)?;
        let billboard_picking = renderer.compile_material(
            &billboard_descriptor
                .with_graphics_entries("vs_main", "fs_object_id")
                .with_color_attachment(true)
                .with_depth(depth_state)
                .with_color_format(ImageFormat::R32Uint),
        )?;
        let shadow = compile_pair(
            renderer,
            resources,
            "shadow/shadow_depth",
            ImageFormat::Auto,
            |descriptor| {
                descriptor.depth = DepthState {
                    test: true,
                    write: true,
                    compare: CompareOp::Less,
                };
                descriptor.stages = PipelineStages::Graphics {
                    vertex_entry: "vs_main".into(),
                    fragment_entry: None,
                };
                descriptor.color_attachment = false;
                descriptor.depth_format = Some(ImageFormat::D32Sfloat);
                descriptor.cull = if flip_y {
                    CullMode::Back
                } else {
                    CullMode::Front
                };
                descriptor.depth_bias = if flip_y {
                    DepthBias::default()
                } else {
                    DepthBias {
                        constant: cascade_params.depth_bias_constant,
                        slope_factor: cascade_params.depth_bias_slope,
                        clamp: 0.0,
                    }
                };
            },
        )?;
        let mark_face = StencilFaceState {
            compare: CompareOp::Always,
            fail: StencilOperation::Keep,
            depth_fail: StencilOperation::Keep,
            pass: StencilOperation::Replace,
        };
        let stencil_mark = compile_pair(
            renderer,
            resources,
            "outline/stencil_mark",
            ImageFormat::R16G16B16A16Sfloat,
            |descriptor| {
                descriptor.depth = depth_state;
                descriptor.color_write_mask = ColorWriteMask::NONE;
                descriptor.cull = CullMode::None;
                descriptor.stencil = Some(StencilState {
                    front: mark_face,
                    back: mark_face,
                    reference: 1,
                    read_mask: 255,
                    write_mask: 1,
                });
            },
        )?;
        let wallhack = bindings.passes.stencil_indicator.is_some();
        let occlusion_mark = if wallhack {
            compile_pair(
                renderer,
                resources,
                "outline/stencil_mark",
                ImageFormat::R16G16B16A16Sfloat,
                |descriptor| {
                    descriptor.depth = depth_state;
                    descriptor.cull = CullMode::Back;
                    descriptor.color_write_mask = ColorWriteMask::NONE;
                    let face = StencilFaceState {
                        compare: CompareOp::Equal,
                        fail: StencilOperation::Keep,
                        depth_fail: StencilOperation::Replace,
                        pass: StencilOperation::Keep,
                    };
                    descriptor.stencil = Some(StencilState {
                        front: face,
                        back: face,
                        reference: 2,
                        read_mask: 1,
                        write_mask: 2,
                    });
                },
            )?
        } else {
            Vec::new()
        };
        let outline_face = StencilFaceState {
            compare: if wallhack {
                CompareOp::Equal
            } else {
                CompareOp::NotEqual
            },
            fail: StencilOperation::Keep,
            depth_fail: StencilOperation::Keep,
            pass: StencilOperation::Keep,
        };
        let outline = compile_pair(
            renderer,
            resources,
            "outline/outline_draw",
            ImageFormat::R16G16B16A16Sfloat,
            |descriptor| {
                descriptor.depth = depth_state;
                descriptor.cull = CullMode::Front;
                descriptor.stencil = Some(StencilState {
                    front: outline_face,
                    back: outline_face,
                    reference: if wallhack { 0 } else { 1 },
                    read_mask: 255,
                    write_mask: 0,
                });
            },
        )?;
        let stencil_indicator = if bindings.passes.stencil_indicator.is_some() {
            compile_pair(
                renderer,
                resources,
                "outline/stencil_indicator",
                ImageFormat::R8Unorm,
                |descriptor| {
                    descriptor.depth = DepthState {
                        test: true,
                        write: false,
                        compare: CompareOp::Always,
                    };
                    descriptor.cull = CullMode::Back;
                    let face = StencilFaceState {
                        compare: CompareOp::Equal,
                        fail: StencilOperation::Keep,
                        depth_fail: StencilOperation::Keep,
                        pass: StencilOperation::Keep,
                    };
                    descriptor.stencil = Some(StencilState {
                        front: face,
                        back: face,
                        reference: 2,
                        read_mask: 255,
                        write_mask: 0,
                    });
                },
            )?
        } else {
            Vec::new()
        };
        let sky = compile_fullscreen(
            renderer,
            resources,
            "sky.wgsl",
            ImageFormat::R16G16B16A16Sfloat,
        )?;
        let tonemap = compile_fullscreen(
            renderer,
            resources,
            "tonemapping.wgsl",
            ImageFormat::B8G8R8A8Srgb,
        )?;
        let overlay = bindings
            .passes
            .wallhack_overlay
            .as_ref()
            .map(|_| {
                compile_fullscreen(
                    renderer,
                    resources,
                    "wallhack_overlay.wgsl",
                    ImageFormat::B8G8R8A8Srgb,
                )
            })
            .transpose()?;
        let particle = renderer.compile_material(
            &PipelineDescriptor::simple(
                resources
                    .shader_path("particles/particle_render.wgsl")
                    .to_string_lossy()
                    .into_owned(),
            )
            .with_depth(depth_state)
            .with_cull(CullMode::None)
            .with_blend(BlendMode::AlphaBlend)
            .with_color_format(ImageFormat::R16G16B16A16Sfloat),
        )?;
        Ok(Self {
            depth,
            picking,
            billboard_depth,
            billboard_picking,
            shadow,
            stencil_mark,
            occlusion_mark,
            outline,
            stencil_indicator,
            sky,
            tonemap,
            overlay,
            particle,
            cascades: CascadeShadowMap::new(cascade_params),
            flip_y,
        })
    }

    pub(crate) fn prepare_frame(
        &mut self,
        graph: &mut FrameGraph,
        frame: GraphicsFrame<'_>,
    ) -> AppResult<()> {
        let GraphicsFrame {
            ids,
            bindings,
            uniforms,
            lights,
            particles,
            selected,
            ordinary,
            billboards,
            frame_slot,
            scene_size,
        } = frame;
        let frame_binding = constant(
            0,
            0,
            ShaderStages::VERTEX_FRAGMENT,
            bytemuck::bytes_of(uniforms),
        );
        self.cascades.update(
            [
                uniforms.light_direction[0],
                uniforms.light_direction[1],
                uniforms.light_direction[2],
            ],
            &uniforms.view_matrix,
            &uniforms.proj_matrix,
        );
        let cascade_data = self.cascades.gpu_data();
        let render_cascades = if self.flip_y {
            katla_gfx::shadow::cascade::flip_projection_y(&cascade_data)
        } else {
            cascade_data
        };
        if let Some(pass) = ids.shadow {
            let mut packet = PassBindings::default();
            packet.constants.push(constant(
                2,
                0,
                ShaderStages::VERTEX,
                bytemuck::bytes_of(&render_cascades),
            ));
            let half = self.cascades.params().shadow_map_size as f32 * 0.5;
            for index in 0..self.cascades.cascade_count() {
                let row = if self.flip_y {
                    1 - index / 2
                } else {
                    index / 2
                };
                let params = [index as u32, 0, 0, 0];
                packet.phases.push(PassDrawPhase {
                    pipelines: self.shadow.clone(),
                    constants: vec![constant(
                        2,
                        1,
                        ShaderStages::VERTEX,
                        bytemuck::cast_slice(&params),
                    )],
                    draw: PassDraw::ObjectIndices(ordinary.clone()),
                    viewport: Some(Rect::new(
                        [index as f32 % 2.0 * half, row as f32 * half],
                        [(index as f32 % 2.0 + 1.0) * half, (row as f32 + 1.0) * half],
                    )),
                });
            }
            graph.set_pass_bindings(pass, packet)?;
        }
        for (pass, pipelines, billboard_material) in [
            (ids.depth_prepass, &self.depth, self.billboard_depth),
            (ids.picking, &self.picking, self.billboard_picking),
        ] {
            if let Some(pass) = pass {
                graph.set_pass_bindings(
                    pass,
                    PassBindings {
                        constants: vec![frame_binding.clone()],
                        phases: vec![
                            PassDrawPhase {
                                pipelines: pipelines.clone(),
                                constants: Vec::new(),
                                draw: PassDraw::ObjectIndices(ordinary.clone()),
                                viewport: None,
                            },
                            PassDrawPhase {
                                pipelines: vec![PassPipeline {
                                    vertex_layout: VertexLayout::pbr(),
                                    material: billboard_material,
                                }],
                                constants: Vec::new(),
                                draw: PassDraw::ObjectIndices(billboards.clone()),
                                viewport: None,
                            },
                        ],
                        ..Default::default()
                    },
                )?;
            }
        }
        if let Some(pass) = ids.geometry {
            let mut packet = PassBindings::default();
            packet.constants.push(frame_binding.clone());
            packet.buffers.extend(lights.graphics_bindings()?);
            packet.constants.push(constant(
                4,
                0,
                ShaderStages::FRAGMENT,
                bytemuck::bytes_of(&cascade_data),
            ));
            if let Some(name) = bindings.resources.shadow_atlas.as_deref()
                && let Some(resource) = graph.resource_id(name)
            {
                packet.images.push(ImageBinding {
                    group: 4,
                    binding: 1,
                    resource,
                    range: ImageSubresourceRange::WHOLE_DEPTH,
                    stages: ShaderStages::FRAGMENT,
                });
                packet.samplers.push(SamplerBinding {
                    group: 4,
                    binding: 2,
                    stages: ShaderStages::FRAGMENT,
                    sampling: SamplingMode::DepthComparison,
                });
            }
            graph.set_pass_bindings(pass, packet)?;
        }
        if let Some(pass) = ids.outline {
            let mut params = [0.0f32; 8];
            params[0] = 0.004 * 1080.0 / scene_size.height.max(1) as f32;
            params[4..].copy_from_slice(&[1.0, 0.55, 0.0, 1.0]);
            let mut phases = vec![PassDrawPhase {
                pipelines: self.stencil_mark.clone(),
                constants: Vec::new(),
                draw: PassDraw::ObjectIndices(selected.clone()),
                viewport: None,
            }];
            if !self.occlusion_mark.is_empty() {
                phases.push(PassDrawPhase {
                    pipelines: self.occlusion_mark.clone(),
                    constants: Vec::new(),
                    draw: PassDraw::ObjectIndices(selected.clone()),
                    viewport: None,
                });
            }
            phases.push(PassDrawPhase {
                pipelines: self.outline.clone(),
                constants: vec![
                    constant(
                        1,
                        0,
                        ShaderStages::VERTEX_FRAGMENT,
                        bytemuck::cast_slice(&params),
                    ),
                    constant(
                        3,
                        0,
                        ShaderStages::VERTEX_FRAGMENT,
                        bytemuck::cast_slice(&params),
                    ),
                ],
                draw: PassDraw::ObjectIndices(selected.clone()),
                viewport: None,
            });
            graph.set_pass_bindings(
                pass,
                PassBindings {
                    phases,
                    constants: vec![frame_binding.clone()],
                    ..Default::default()
                },
            )?;
        }
        if let Some(pass) = ids.stencil_indicator {
            graph.set_pass_bindings(
                pass,
                PassBindings {
                    phases: vec![PassDrawPhase {
                        pipelines: self.stencil_indicator.clone(),
                        constants: Vec::new(),
                        draw: PassDraw::ObjectIndices(selected),
                        viewport: None,
                    }],
                    constants: vec![frame_binding.clone()],
                    ..Default::default()
                },
            )?;
        }
        if let Some(pass) = graph.pass_id("particles") {
            let mut packet = PassBindings::default();
            packet.buffers.extend(particles.graphics_bindings()?);
            let indirect = particles.indirect_resource()?;
            packet.phases.push(PassDrawPhase {
                pipelines: vec![PassPipeline {
                    vertex_layout: VertexLayout::position(),
                    material: self.particle,
                }],
                constants: Vec::new(),
                draw: PassDraw::Indirect {
                    resource: indirect,
                    offset: 0,
                },
                viewport: None,
            });
            graph.set_pass_bindings(pass, packet)?;
        }
        if let Some(pass) = graph.pass_id("sky") {
            set_fullscreen(graph, pass, self.sky, uniforms)?;
        }
        if let Some(pass) = ids.tonemap {
            let mut post = *uniforms;
            if let Some(name) = bindings.resources.hdr_color.as_deref() {
                post.tonemap = [
                    if self.flip_y { 1.0 } else { 0.4 },
                    2.2,
                    0.0,
                    graph
                        .transient_texture_bindless_slot(name, frame_slot)
                        .unwrap_or(0) as f32,
                ];
            }
            set_fullscreen(graph, pass, self.tonemap, &post)?;
        }
        if let (Some(pass), Some(material)) = (ids.wallhack_overlay, self.overlay) {
            let mut post = *uniforms;
            post.overlay = [
                bindings
                    .resources
                    .tonemap_output
                    .as_deref()
                    .and_then(|name| graph.transient_texture_bindless_slot(name, frame_slot))
                    .unwrap_or(0) as f32,
                bindings
                    .resources
                    .stencil_indicator
                    .as_deref()
                    .and_then(|name| graph.transient_texture_bindless_slot(name, frame_slot))
                    .unwrap_or(0) as f32,
                0.0,
                0.0,
            ];
            set_fullscreen(graph, pass, material, &post)?;
        }
        Ok(())
    }
}

fn constant(group: u32, binding: u32, stages: ShaderStages, bytes: &[u8]) -> ConstantBinding {
    ConstantBinding {
        group,
        binding,
        stages,
        bytes: bytes.to_vec(),
    }
}

fn set_fullscreen(
    graph: &mut FrameGraph,
    pass: PassId,
    material: MaterialHandle,
    uniforms: &crate::rendering::FrameUniforms,
) -> AppResult<()> {
    graph.set_pass_bindings(
        pass,
        PassBindings {
            constants: vec![constant(
                0,
                0,
                ShaderStages::VERTEX_FRAGMENT,
                bytemuck::bytes_of(uniforms),
            )],
            phases: vec![PassDrawPhase {
                pipelines: vec![PassPipeline {
                    vertex_layout: VertexLayout::position(),
                    material,
                }],
                constants: Vec::new(),
                draw: PassDraw::Vertices {
                    count: 3,
                    instances: 1,
                },
                viewport: None,
            }],
            ..Default::default()
        },
    )?;
    Ok(())
}

fn compile_fullscreen(
    renderer: &mut Renderer,
    resources: &ResourceManager,
    shader: &str,
    format: ImageFormat,
) -> AppResult<MaterialHandle> {
    Ok(renderer.compile_material(
        &PipelineDescriptor::simple(resources.shader_path(shader).to_string_lossy().into_owned())
            .with_depth(DepthState::disabled())
            .with_depth_format(None)
            .with_cull(CullMode::None)
            .with_color_format(format),
    )?)
}

fn compile_pair(
    renderer: &mut Renderer,
    resources: &ResourceManager,
    shader: &str,
    format: ImageFormat,
    configure: impl Fn(&mut PipelineDescriptor),
) -> AppResult<Vec<PassPipeline>> {
    let mut pipelines = Vec::new();
    for (suffix, layout) in [
        ("", VertexLayout::pbr()),
        ("_skinned", VertexLayout::pbr_skinned()),
    ] {
        let mut descriptor = PipelineDescriptor::pbr(
            resources
                .shader_path(format!("{shader}{suffix}.wgsl"))
                .to_string_lossy()
                .into_owned(),
        )
        .with_vertex_layout(layout.clone())
        .with_color_format(format);
        configure(&mut descriptor);
        let material = renderer.compile_material(&descriptor)?;
        pipelines.push(PassPipeline {
            vertex_layout: layout,
            material,
        });
    }
    let mut descriptor = PipelineDescriptor::simple(
        resources
            .shader_path(format!("{shader}.wgsl"))
            .to_string_lossy()
            .into_owned(),
    )
    .with_color_format(format);
    configure(&mut descriptor);
    let material = renderer.compile_material(&descriptor)?;
    pipelines.push(PassPipeline {
        vertex_layout: VertexLayout::position(),
        material,
    });
    Ok(pipelines)
}
