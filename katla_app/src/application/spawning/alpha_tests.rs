//! Native coverage, compositing, depth, picking and face-orientation acceptance.

use super::*;
use crate::application::scene_features::material_pipelines::CoveragePass;
use crate::rendering::{AlphaMode, FrameContext, MaterialSurface};
use katla_gfx::render_graph::{
    GraphResourceDesc, GraphResourceType, ImageSubresourceRange, PassBuilder, PassType, SimplePass,
};
use katla_gfx::{
    AttachmentOps, ClearValue, ImageBinding, MaterialTextures, TextureDescriptor, Vertex,
};

#[test]
#[ignore = "requires native Vulkan or Metal surface rasterization"]
fn test_native_static_alpha_coverage_and_sorted_compositing() {
    native_alpha(false);
}

#[test]
#[ignore = "requires native Vulkan or Metal skinned surface rasterization"]
fn test_native_skinned_alpha_coverage_and_sorted_compositing() {
    native_alpha(true);
}

fn native_alpha(skinned: bool) {
    let fixture = Fixture::new();
    let common =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../resources/shaders/common");
    let mut source = format!(
        "#include \"{}\"\n#include \"{}\"\n#include \"{}\"\n",
        common.join("frame_uniforms.wgsl").display(),
        common.join("bindless.wgsl").display(),
        common.join("material_surface.wgsl").display()
    );
    source.push_str(r#"
@group(0) @binding(0) var<storage,read> frame_data: FrameUniforms;
@group(0) @binding(1) var<storage,read> objects: array<ObjectUniforms>;
@group(0) @binding(2) var<storage,read> surfaces: array<SurfaceParameters>;
@group(0) @binding(3) var<uniform> probe: vec4u;
@group(5) @binding(0) var albedo_sampler: sampler;
struct Input { @location(0) position: vec3f, @location(3) uv: vec2f, @location(6) uv1: vec2f, }
struct Output { @builtin(position) position: vec4f, @location(0) uv: vec2f, @location(2) uv1: vec2f, @location(1) @interpolate(flat) slot: u32, }
@vertex fn vs_main(input: Input, @builtin(instance_index) slot: u32) -> Output {
 var out: Output; out.position = frame_data.proj * frame_data.view * objects[slot].model * vec4f(input.position,1); out.uv=input.uv; out.uv1=input.uv1; out.slot=slot; return out;
}
@fragment fn fs_main(input: Output, @builtin(front_facing) front: bool) -> @location(0) vec4f {
 let obj = objects[input.slot]; let surface=surfaces[input.slot]; let texel=textureSample(bindless_textures[obj.texture_indices.x],albedo_sampler,material_uv(surface,0u,input.uv,input.uv1));
 let alpha=surface_alpha(texel.a * obj.base_color.a, surface);
 if (probe.x == 1u && obj.base_color.r > 0.5) { return vec4f(surface_face_normal(vec3f(0,0,1),front,surface)*0.5+0.5,alpha); }
 return vec4f(obj.base_color.rgb * texel.rgb,alpha);
}
"#);
    if skinned {
        source=source.replace("@location(6) uv1: vec2f, }", "@location(6) uv1: vec2f, @location(4) joints: vec4u, @location(5) weights: vec4f, }")
            .replace("@vertex fn vs_main", "@group(2) @binding(0) var<storage,read> joints: array<mat4x4f>; @vertex fn vs_main")
            .replace("objects[slot].model * vec4f(input.position,1)", "objects[slot].model * (joints[input.joints.x]*input.weights.x+joints[input.joints.y]*input.weights.y+joints[input.joints.z]*input.weights.z+joints[input.joints.w]*input.weights.w) * vec4f(input.position,1)");
    }
    std::fs::write(fixture.0.join("color.wgsl"), source).unwrap();
    std::fs::write(fixture.0.join("depth_view.wgsl"),r#"
@group(0) @binding(0) var image: texture_depth_2d;
@vertex fn vs_main(@builtin(vertex_index) i: u32) -> @builtin(position) vec4f {
 let positions=array<vec2f,3>(vec2f(-1,-1),vec2f(3,-1),vec2f(-1,3)); return vec4f(positions[i],0,1);
}
@fragment fn fs_main(@builtin(position) p: vec4f) -> @location(0) vec4f { return vec4f(textureLoad(image,vec2i(p.xy),0),0,0,1); }
"#).unwrap();
    let mut app = ApplicationBuilder::new()
        .validation_layer(true)
        .with_frame_graph(|renderer, _| Ok(ApplicationFrameGraph::new(empty_frame_graph(renderer))))
        .build_headless(1, String::new())
        .unwrap();
    let validation_errors = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    match &app.renderer {
        crate::Renderer::Vulkan(renderer) => {
            assert!(
                renderer.context().validation_active(),
                "native acceptance requires active Vulkan validation"
            );
            let errors = validation_errors.clone();
            renderer
                .context()
                .set_validation_callback(move |message, level| {
                    if level == katla_gfx::ValidationLevel::Error {
                        errors.lock().unwrap().push(message.into());
                    }
                });
        }
        #[cfg(target_os = "macos")]
        crate::Renderer::Metal(_) => {}
    }
    #[cfg(target_os = "macos")]
    {
        assert_eq!(std::env::var("MTL_DEBUG_LAYER").as_deref(), Ok("1"));
        assert_eq!(
            std::env::var("METAL_DEVICE_WRAPPER_TYPE").as_deref(),
            Ok("1")
        );
    }
    let layout = if skinned {
        katla_gfx::VertexPBRSkinned::layout()
    } else {
        katla_gfx::VertexPBR::layout()
    };
    let base = [
        ([-1., -1., 0.], [0., 0.]),
        ([1., -1., 0.], [1., 0.]),
        ([1., 1., 0.], [1., 1.]),
        ([-1., 1., 0.], [0., 1.]),
    ]
    .map(|(position, uv)| katla_gfx::VertexPBR::new(position, [0., 0., 1.], [1., 0., 0., 1.], uv));
    let create_mesh = |renderer: &mut crate::Renderer, indices: &[u32]| {
        if skinned {
            renderer
                .create_mesh(
                    &base.map(|vertex| {
                        katla_gfx::VertexPBRSkinned::from_pbr(vertex, [0; 4], [1., 0., 0., 0.])
                    }),
                    indices,
                    katla_gfx::PrimitiveTopology::TriangleList,
                )
                .unwrap()
        } else {
            renderer
                .create_mesh(&base, indices, katla_gfx::PrimitiveTopology::TriangleList)
                .unwrap()
        }
    };
    let mesh = create_mesh(&mut app.renderer, &[0, 1, 2, 0, 2, 3]);
    let back_mesh = create_mesh(&mut app.renderer, &[0, 2, 1, 0, 3, 2]);
    let skeleton = if skinned {
        app.renderer.create_skeleton(1).unwrap()
    } else {
        katla_gfx::SkeletonHandle::NONE
    };
    let color = app
        .renderer
        .compile_material(
            &PipelineDescriptor::pbr(fixture.0.join("color.wgsl").to_string_lossy())
                .with_vertex_layout(layout.clone())
                .with_color_format(ImageFormat::R8G8B8A8Unorm),
        )
        .unwrap();
    let texture = app
        .renderer
        .create_texture(
            &TextureDescriptor::rgba8_unorm(2, 1),
            &[255, 255, 255, 0, 255, 255, 255, 255],
        )
        .unwrap();
    let masked = app
        .renderer
        .compile_material(
            &PipelineDescriptor::pbr(fixture.0.join("color.wgsl").to_string_lossy())
                .with_vertex_layout(layout.clone())
                .with_color_format(ImageFormat::R8G8B8A8Unorm),
        )
        .unwrap();
    app.renderer.set_material_textures(
        masked,
        MaterialTextures {
            albedo: texture,
            ..Default::default()
        },
    );
    let real = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../resources/shaders");
    let suffix = if skinned { "_skinned" } else { "" };
    let depth = app
        .renderer
        .compile_material(
            &PipelineDescriptor::pbr(
                real.join(format!("depth_prepass{suffix}.wgsl"))
                    .to_string_lossy(),
            )
            .with_vertex_layout(layout.clone())
            .with_color_attachment(false)
            .with_graphics_entries("vs_main", "fs_depth"),
        )
        .unwrap();
    let picking = app
        .renderer
        .compile_material(
            &PipelineDescriptor::pbr(
                real.join(format!("depth_prepass{suffix}.wgsl"))
                    .to_string_lossy(),
            )
            .with_vertex_layout(layout.clone())
            .with_color_format(ImageFormat::R32Uint),
        )
        .unwrap();
    let shadow = app
        .renderer
        .compile_material(
            &PipelineDescriptor::pbr(
                real.join(format!("shadow/shadow_depth{suffix}.wgsl"))
                    .to_string_lossy(),
            )
            .with_vertex_layout(layout.clone())
            .with_color_attachment(false)
            .with_graphics_entries("vs_main", "fs_depth")
            .with_depth_format(Some(ImageFormat::D32Sfloat))
            .with_depth(DepthState {
                test: true,
                write: true,
                compare: katla_gfx::CompareOp::Less,
            }),
        )
        .unwrap();
    let visualize = app
        .renderer
        .compile_material(
            &PipelineDescriptor::pbr(fixture.0.join("depth_view.wgsl").to_string_lossy())
                .with_vertex_layout(VertexLayout::empty())
                .with_cull(CullMode::None)
                .with_depth(DepthState::disabled())
                .with_depth_format(None)
                .with_color_format(ImageFormat::R8G8B8A8Unorm),
        )
        .unwrap();
    let mut builder = FrameGraphBuilder::new();
    for name in ["color", "depth_view", "shadow_view", "id"] {
        builder = builder
            .create_resource(GraphResourceDesc {
                name: name.into(),
                resource_type: GraphResourceType::ColorAttachment {
                    clear_value: Some([0.; 4]),
                },
                format: if name == "id" {
                    ImageFormat::R32Uint
                } else {
                    ImageFormat::R8G8B8A8Unorm
                },
                width: 8,
                height: 4,
                tracks_swapchain_size: false,
            })
            .export_resource(name);
    }
    for (name, clear, format) in [
        ("depth", 0., ImageFormat::D32SfloatS8Uint),
        ("pick_depth", 0., ImageFormat::D32SfloatS8Uint),
        ("shadow", 1., ImageFormat::D32Sfloat),
    ] {
        builder = builder.create_resource(GraphResourceDesc {
            name: name.into(),
            resource_type: GraphResourceType::DepthAttachment {
                clear_value: clear,
                sampled: name != "pick_depth",
            },
            format,
            width: 8,
            height: 4,
            tracks_swapchain_size: false,
        });
    }
    let clear = AttachmentOps::clear(ClearValue::DepthStencil {
        depth: 0.0,
        stencil: 0,
    });
    let load = clear.with_load(katla_gfx::LoadOp::Load);
    let shadow_clear = AttachmentOps::clear(ClearValue::DepthStencil {
        depth: 1.0,
        stencil: 0,
    });
    builder = builder
        .add_pass(
            SimplePass::new("depth", PassType::Graphics)
                .depth_ops(clear, clear)
                .depth_target("depth"),
        )
        .add_pass(
            SimplePass::new("color", PassType::Graphics)
                .write("color")
                .attachment("color", AttachmentOps::clear(ClearValue::TRANSPARENT_BLACK))
                .depth_ops(load, load)
                .depth_target("depth"),
        )
        .add_pass(
            SimplePass::new("picking", PassType::Graphics)
                .write("id")
                .attachment("id", AttachmentOps::clear(ClearValue::TRANSPARENT_BLACK))
                .depth_ops(clear, clear)
                .depth_target("pick_depth"),
        )
        .add_pass(
            SimplePass::new("shadow", PassType::Graphics)
                .depth_ops(shadow_clear, shadow_clear)
                .depth_target("shadow"),
        );
    for (name, source) in [("depth_view", "depth"), ("shadow_view", "shadow")] {
        builder = builder.add_pass(
            SimplePass::new(name, PassType::Graphics)
                .without_depth()
                .read(source)
                .write(name)
                .attachment(name, AttachmentOps::clear(ClearValue::TRANSPARENT_BLACK)),
        );
    }
    #[cfg(not(target_os = "macos"))]
    let mut graph = katla_gfx::render_graph::any_frame_graph::AnyFrameGraph::from_vulkan(
        builder.build::<katla_gfx::VulkanRenderer>().unwrap(),
    );
    #[cfg(target_os = "macos")]
    let mut graph = katla_gfx::render_graph::any_frame_graph::AnyFrameGraph::from_metal(
        builder.build::<katla_gfx::MetalRenderer>().unwrap(),
    );
    graph
        .recreate_transient_textures(&mut app.renderer, 8, 4)
        .unwrap();
    assert!(
        graph
            .register_transient_texture_bindless(&mut app.renderer, "pick_depth")
            .is_err()
    );
    assert!(
        graph
            .transient_texture_bindless_slot("pick_depth", 0)
            .is_none()
    );
    let identity = katla_math::Mat4::identity().to_array();
    let mut projection = identity;
    projection[14] = 1.;
    if app.renderer.capabilities().clip_y_down {
        projection[5] = -1.;
    }
    let uniforms = crate::rendering::FrameUniforms {
        view_matrix: identity,
        proj_matrix: projection,
        ..Default::default()
    };
    let mut shadow_projection = identity;
    shadow_projection[14] = 1.0;
    if !app.renderer.capabilities().clip_y_down {
        shadow_projection[5] = -1.0;
    }
    let mut variants =
        crate::application::scene_features::material_pipelines::MaterialPipelines::default();
    for case in 0..13 {
        let mut context = FrameContext::new();
        let mut transform = identity;
        transform[14] = -0.9;
        let bg = context
            .draw(mesh, color)
            .with_transform(transform)
            .with_color([0., 1., 0., 1.])
            .with_skeleton(skeleton)
            .submit();
        transform[14] = -0.25;
        if case == 7 {
            transform[0] = -1.;
        }
        let mode = match case {
            1 | 2 | 8 | 10 | 11 | 12 => AlphaMode::Mask,
            3 | 9 => AlphaMode::Blend,
            _ => AlphaMode::Opaque,
        };
        let fg = context
            .draw(
                if (5..=7).contains(&case) {
                    back_mesh
                } else {
                    mesh
                },
                if case <= 2 || case >= 11 {
                    masked
                } else {
                    color
                },
            )
            .with_transform(transform)
            .with_skeleton(skeleton)
            .with_color([
                1.,
                0.,
                0.,
                if case == 2 {
                    0.4
                } else if case == 1 || case == 8 || case >= 11 {
                    1.0
                } else if case == 3 || case == 10 {
                    0.5
                } else {
                    0.
                },
            ])
            .with_sampling(crate::rendering::MaterialSampling {
                albedo: crate::rendering::TextureSampling {
                    uv: if case == 11 {
                        crate::rendering::UvTransform {
                            tex_coord: 1,
                            scale: [-1., 1.],
                            offset: [1., 0.],
                            ..Default::default()
                        }
                    } else if case == 12 {
                        crate::rendering::UvTransform {
                            offset: [0.5, 0.],
                            ..Default::default()
                        }
                    } else {
                        Default::default()
                    },
                    sampler: if case == 12 {
                        katla_gfx::SamplerDescriptor {
                            address_u: katla_gfx::AddressMode::Repeat,
                            ..katla_gfx::SamplerDescriptor::nearest_clamp()
                        }
                    } else {
                        katla_gfx::SamplerDescriptor::nearest_clamp()
                    },
                },
                ..Default::default()
            })
            .with_surface(MaterialSurface {
                alpha_mode: mode,
                double_sided: case == 6 || case == 7,
                alpha_cutoff: if case == 8 { 1.25 } else { 0.5 },
                ..Default::default()
            })
            .submit();
        if case == 3 {
            transform[14] = -0.5;
            context
                .draw(mesh, color)
                .with_transform(transform)
                .with_color([0., 0., 1., 0.5])
                .with_skeleton(skeleton)
                .with_surface(MaterialSurface {
                    alpha_mode: AlphaMode::Blend,
                    ..Default::default()
                })
                .submit();
        }
        let mut submission = context.take_submission();
        variants
            .prepare_draws(
                &mut app.renderer,
                &mut submission.draw_list,
                &submission.surfaces,
            )
            .unwrap();
        submission.draw_list.sort_for_view(&identity);
        let all = submission
            .draw_list
            .iter()
            .map(|draw| draw.base_object_slot())
            .collect::<Vec<_>>();
        let constants = vec![
            ConstantBinding {
                group: 0,
                binding: 0,
                stages: ShaderStages::VERTEX,
                bytes: bytemuck::bytes_of(&uniforms).to_vec(),
            },
            ConstantBinding {
                group: 0,
                binding: 2,
                stages: ShaderStages::FRAGMENT,
                bytes: bytemuck::cast_slice(&submission.surfaces).to_vec(),
            },
            ConstantBinding {
                group: 0,
                binding: 3,
                stages: ShaderStages::FRAGMENT,
                bytes: bytemuck::cast_slice(&[u32::from((4..=7).contains(&case)), 0, 0, 0])
                    .to_vec(),
            },
        ];
        let skin = ConstantBinding {
            group: 2,
            binding: 0,
            stages: ShaderStages::VERTEX,
            bytes: bytemuck::cast_slice(&identity).to_vec(),
        };
        for (name, material, indices, policy) in [
            ("depth", depth, all.clone(), CoveragePass::Depth),
            ("picking", picking, all.clone(), CoveragePass::Picking),
            (
                "shadow",
                shadow,
                vec![fg],
                CoveragePass::Shadow {
                    reverse_winding: app.renderer.capabilities().clip_y_down,
                },
            ),
        ] {
            let phases = variants
                .auxiliary_phases(
                    &mut app.renderer,
                    &[PassPipeline {
                        vertex_layout: layout.clone(),
                        material,
                    }],
                    &indices,
                    crate::rendering::frame_context::SurfaceRows {
                        parameters: &submission.surfaces,
                        samplers: &submission.samplers,
                    },
                    policy,
                )
                .unwrap();
            let mut bindings = PassBindings {
                constants: constants.clone(),
                phases,
                ..Default::default()
            };
            if skinned {
                bindings.constants.push(skin.clone());
            }
            if name == "shadow" {
                let cascade = katla_gfx::shadow::cascade::ShadowCascadeGPU {
                    view_proj: shadow_projection,
                    split_distance: 1.,
                    texel_size: 0.,
                    _pad: [0.; 2],
                };
                bindings.constants.push(ConstantBinding {
                    group: 3,
                    binding: 0,
                    stages: ShaderStages::VERTEX,
                    bytes: bytemuck::cast_slice(&[cascade; 4]).to_vec(),
                });
                bindings.constants.push(ConstantBinding {
                    group: 3,
                    binding: 1,
                    stages: ShaderStages::VERTEX,
                    bytes: bytemuck::cast_slice(&[0u32; 4]).to_vec(),
                });
            }
            graph
                .set_pass_bindings(graph.pass_id(name).unwrap(), bindings)
                .unwrap();
        }
        let mut bindings = PassBindings {
            phases: crate::application::scene_features::material_pipelines::geometry_phases(
                &all,
                &submission.samplers,
            ),
            constants,
            ..Default::default()
        };
        if skinned {
            bindings.constants.push(skin);
        }
        graph
            .set_pass_bindings(graph.pass_id("color").unwrap(), bindings)
            .unwrap();
        for (name, source) in [("depth_view", "depth"), ("shadow_view", "shadow")] {
            graph
                .set_pass_bindings(
                    graph.pass_id(name).unwrap(),
                    PassBindings {
                        images: vec![ImageBinding {
                            group: 0,
                            binding: 0,
                            resource: graph.resource_id(source).unwrap(),
                            range: ImageSubresourceRange::WHOLE_DEPTH,
                            stages: ShaderStages::FRAGMENT,
                        }],
                        phases: vec![PassDrawPhase {
                            samplers: Vec::new(),
                            pipelines: vec![PassPipeline {
                                vertex_layout: VertexLayout::empty(),
                                material: visualize,
                            }],
                            constants: vec![],
                            draw: PassDraw::Vertices {
                                count: 3,
                                instances: 1,
                            },
                            viewport: None,
                        }],
                        ..Default::default()
                    },
                )
                .unwrap();
        }
        #[cfg(target_os = "macos")]
        {
            let offscreen = app.renderer.create_offscreen_texture(
                crate::application::headless::HEADLESS_WIDTH,
                crate::application::headless::HEADLESS_HEIGHT,
            );
            app.renderer.set_headless_drawable(offscreen);
        }
        let katla_gfx::FrameAcquisition::Ready(frame) = app.renderer.acquire_frame().unwrap()
        else {
            panic!("frame")
        };
        app.renderer
            .execute_draw_calls(&frame, &submission.draw_list)
            .unwrap();
        let draws = std::rc::Rc::new(submission.draw_list);
        let passes =
            ["depth", "color", "picking", "shadow"].map(|name| graph.pass_id(name).unwrap());
        app.renderer
            .render(&frame, &mut graph, |frame| {
                for pass in passes {
                    frame.submit(pass, std::rc::Rc::clone(&draws));
                }
            })
            .unwrap();
        app.renderer.present(frame).unwrap();
        for (x, left) in [(1, true), (6, false)] {
            let pixel = read(&mut app, &graph, "color", x);
            let holes = case == 2
                || (case == 1 && left)
                || (case >= 11 && !left)
                || matches!(case, 5 | 8 | 9);
            let expected = if holes {
                [0, 255, 0, 255]
            } else if case == 3 {
                [128, 64, 64, 255]
            } else if (4..=7).contains(&case) {
                [128, 128, if case == 4 { 255 } else { 0 }, 255]
            } else {
                [255, 0, 0, 255]
            };
            assert_pixel(&pixel, expected, case, "color");
            let id = u32::from_ne_bytes(read(&mut app, &graph, "id", x).try_into().unwrap());
            assert_eq!(
                id,
                if holes { bg + 1 } else { fg + 1 },
                "case {case} picking"
            );
            let depth = read(&mut app, &graph, "depth_view", x)[0];
            assert!(
                depth.abs_diff(if holes || case == 3 { 26 } else { 191 }) <= 1,
                "case {case} depth {depth}"
            );
            let shadow_depth = read(&mut app, &graph, "shadow_view", x)[0];
            assert!(
                shadow_depth.abs_diff(if holes || case == 3 { 255 } else { 191 }) <= 1,
                "case {case} shadow {shadow_depth}"
            );
        }
    }
    app.renderer.destroy();
    let errors = validation_errors.lock().unwrap();
    assert!(errors.is_empty(), "native validation errors: {errors:?}");
}

fn read(
    app: &mut crate::application::Application,
    graph: &crate::FrameGraph,
    name: &str,
    x: u32,
) -> Vec<u8> {
    let source = app
        .renderer
        .graph_texture_source(graph.resource_id(name).unwrap())
        .unwrap();
    let ticket = app
        .renderer
        .queue_texture_readback(source, TextureReadbackRegion::pixel(x, 2))
        .unwrap();
    app.renderer.wait_for_device();
    app.renderer
        .poll_texture_readback(ticket)
        .unwrap()
        .unwrap()
        .bytes
}
fn assert_pixel(actual: &[u8], expected: [u8; 4], case: u32, name: &str) {
    assert!(
        actual.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 1),
        "case {case} {name}: {actual:?} expected {expected:?}"
    );
}
