//! Complete scene PBR shaders compared with independent double-precision lighting.

use super::*;
use crate::rendering::{FrameContext, FrameUniforms, MaterialSurface};
use bytemuck::Zeroable;
use katla_gfx::render_graph::{
    GraphResourceDesc, GraphResourceType, ImageSubresourceRange, PassBuilder, PassType, SimplePass,
};
use katla_gfx::{
    AttachmentOps, ClearValue, ImageBinding, MaterialTextures, PointLightGPU, SamplerBinding,
    SamplerDescriptor, TextureDescriptor, Vertex, VertexPBR, VertexPBRSkinned,
};
use katla_math::{Mat3, Mat4, Vec3};

#[test]
#[ignore = "requires native Vulkan or Metal complete PBR shader execution"]
fn test_native_complete_pbr_lighting_and_transformed_static_skinned_frames() {
    #[cfg(target_os = "macos")]
    {
        assert_eq!(std::env::var("MTL_DEBUG_LAYER").as_deref(), Ok("1"));
        assert_eq!(
            std::env::var("METAL_DEVICE_WRAPPER_TYPE").as_deref(),
            Ok("1")
        );
    }
    // Full scene compilation triggers an Intel driver crash with Vulkan validation.
    let mut app = ApplicationBuilder::new()
        .validation_layer(false)
        .with_frame_graph(|renderer, _| Ok(ApplicationFrameGraph::new(empty_frame_graph(renderer))))
        .build_headless(1, String::new())
        .unwrap();
    let shaders = app.resources.shaders.clone();
    let static_material = app
        .renderer
        .compile_material(
            &PipelineDescriptor::pbr(shaders.join("model_pbr.wgsl").to_string_lossy())
                .with_depth(DepthState::disabled())
                .with_depth_format(None)
                .with_cull(CullMode::None),
        )
        .unwrap();
    let skin_material = app
        .renderer
        .compile_material(
            &PipelineDescriptor::pbr(shaders.join("model_pbr_skinned.wgsl").to_string_lossy())
                .with_vertex_layout(VertexPBRSkinned::layout())
                .with_depth(DepthState::disabled())
                .with_depth_format(None)
                .with_cull(CullMode::None),
        )
        .unwrap();
    let defaults =
        crate::application::scene_features::create_material_textures(&mut app.renderer).unwrap();
    let normal_map = app
        .renderer
        .create_texture(
            &TextureDescriptor::rgba16_float(1, 1),
            bytemuck::cast_slice(&[
                half::f16::from_f32(0.75).to_bits(),
                half::f16::from_f32(0.5).to_bits(),
                half::f16::ONE.to_bits(),
                half::f16::ONE.to_bits(),
            ]),
        )
        .unwrap();
    let skeleton = app.renderer.create_skeleton(1).unwrap();
    let normal = Vec3::new(1., 1., 1.).normalize();
    let tangent = Vec3::new(1., -1., 0.).normalize();
    let vertices = [[-4., -4., 0.], [4., -4., 0.], [0., 4., 0.]].map(|position| {
        VertexPBR::new(
            position,
            normal.to_array(),
            [tangent.x(), tangent.y(), tangent.z(), -1.],
            [0.25; 2],
        )
    });
    let transforms = [
        Mat4::identity(),
        matrix([
            2., 0., 0., 0., 0.4, 0.5, 0., 0., 0.3, -0.2, 1.5, 0., 0., 0., 0., 1.,
        ]),
        matrix([
            -2., 0., 0., 0., 0.4, 0.5, 0., 0., 0.3, -0.2, 1.5, 0., 0., 0., 0., 1.,
        ]),
    ];
    let mut point_lights = [PointLightGPU::zeroed(); 256];
    point_lights[0] = PointLightGPU {
        position: [2., 1., 4.5],
        range: 8.,
        color: [0.8, 0.3, 0.1],
        intensity: 2.,
    };
    point_lights[1] = PointLightGPU {
        position: [-1., 2., 3.5],
        range: 6.,
        color: [0.1, 0.4, 0.9],
        intensity: 1.5,
    };
    let mut tile_indices = [0u32; 9 * 128];
    for tile in tile_indices.as_chunks_mut::<128>().0 {
        tile[1] = 1;
    }
    let identity = Mat4::identity().to_array();
    let mut object = identity;
    object[14] = 0.5;
    let sun = Vec3::new(0.5, 0.25, 1.).normalize().to_array();
    let uniforms = FrameUniforms {
        view_matrix: identity,
        proj_matrix: identity,
        camera_position: [0., 0., 5., 1.],
        light_direction: [sun[0], sun[1], sun[2], 0.],
        light_color: [1., 0.8, 0.6, 0.],
        light_intensity: [1.7, 0., 0., 0.],
        tiles: [3, 3, 0, 0],
        ..Default::default()
    };
    let mut lit_graph = graph(1.);
    let mut shadowed_graph = graph(0.);
    let mut accepted_samples = 0;
    let mut maximum_error = 0.0f64;
    for (transform_index, transform) in transforms.into_iter().enumerate() {
        for geometry in 0..4 {
            let mut base = vertices;
            let mut object_matrix = object;
            let mut joints = identity;
            let mut indices = [0u32, 1, 2];
            let skinned = geometry >= 2;
            if geometry == 0 {
                crate::util::GLTFModel::transform_vertex_data(&mut base, &transform);
                if transform_index == 2 {
                    indices.swap(1, 2);
                }
            } else if geometry == 1 {
                object_matrix = (matrix(object) * transform).to_array();
            } else if geometry == 2 {
                joints = transform.to_array();
            } else {
                let model_scale = matrix([
                    0.5, 0., 0., 0., 0., 2., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
                ]);
                object_matrix = (matrix(object) * model_scale).to_array();
                joints = (model_scale.inverse().unwrap() * transform).to_array();
            }
            let mesh = if skinned {
                app.renderer
                    .create_mesh(
                        &base.map(|vertex| {
                            VertexPBRSkinned::from_pbr(vertex, [0; 4], [1., 0., 0., 0.])
                        }),
                        &indices,
                        katla_gfx::PrimitiveTopology::TriangleList,
                    )
                    .unwrap()
            } else {
                app.renderer
                    .create_mesh(&base, &indices, katla_gfx::PrimitiveTopology::TriangleList)
                    .unwrap()
            };
            let material = if skinned {
                skin_material
            } else {
                static_material
            };
            let mut neutral_pixel = None;
            for (
                case,
                metallic,
                roughness,
                normal_scale,
                ao,
                emission,
                points,
                shadowed,
                textured,
            ) in [
                (0, 0., 0.1, 1., 1., [0.; 3], false, false, false),
                (1, 0., 0.4, 1., 1., [0.; 3], false, false, false),
                (2, 1., 1., 1., 1., [0.; 3], false, false, false),
                (3, 0.5, 0.4, 1., 0.3, [0.; 3], true, false, false),
                (4, 0.5, 0.4, 1., 0.3, [0.; 3], true, true, false),
                (5, 0.2, 0.7, 0., 1., [0.; 3], false, false, true),
                (6, 0.2, 0.7, 2., 1., [0.; 3], true, false, true),
                (7, 0.2, 0.7, -1., 1., [0.; 3], false, false, true),
                (8, 0., 1., 1., 0., [4., 0.5, 2.], false, true, false),
                (9, 0., 0.4, 0., 1., [0.; 3], false, false, false),
            ] {
                let graph = if shadowed {
                    &mut shadowed_graph
                } else {
                    &mut lit_graph
                };
                app.renderer.set_material_textures(
                    material,
                    MaterialTextures {
                        normal: if textured {
                            normal_map
                        } else {
                            defaults.normal
                        },
                        ..defaults
                    },
                );
                let mut context = FrameContext::new();
                context
                    .draw(mesh, material)
                    .with_transform(object_matrix)
                    .with_color([0.6, 0.3, 0.15, 1.])
                    .with_pbr(metallic, roughness, ao)
                    .with_skeleton(if skinned {
                        skeleton
                    } else {
                        katla_gfx::SkeletonHandle::NONE
                    })
                    .with_surface(MaterialSurface {
                        normal_scale,
                        emissive_factor: emission,
                        ..Default::default()
                    })
                    .submit();
                let submission = context.take_submission();
                let mut shadow = katla_gfx::shadow::cascade::ShadowFrameData::zeroed();
                shadow.light_direction[3] = 1.;
                shadow.cascades[0].view_proj = identity;
                shadow.cascades[0].split_distance = 100.;
                let mut constants = vec![
                    constant(
                        0,
                        0,
                        ShaderStages::VERTEX_FRAGMENT,
                        bytemuck::bytes_of(&uniforms),
                    ),
                    constant(
                        0,
                        2,
                        ShaderStages::FRAGMENT,
                        bytemuck::cast_slice(&submission.surfaces),
                    ),
                    constant(
                        3,
                        0,
                        ShaderStages::FRAGMENT,
                        bytemuck::cast_slice(&point_lights),
                    ),
                    constant(
                        3,
                        1,
                        ShaderStages::FRAGMENT,
                        bytemuck::cast_slice(&tile_indices),
                    ),
                    constant(
                        3,
                        2,
                        ShaderStages::FRAGMENT,
                        bytemuck::cast_slice(&[if points { 2u32 } else { 0 }; 9]),
                    ),
                    constant(4, 0, ShaderStages::FRAGMENT, bytemuck::bytes_of(&shadow)),
                ];
                if skinned {
                    constants.push(constant(
                        2,
                        0,
                        ShaderStages::VERTEX,
                        bytemuck::cast_slice(&joints),
                    ));
                }
                graph.set_pass_bindings(graph.pass_id("pbr").unwrap(), PassBindings {
                    constants,
                    phases: crate::application::scene_features::material_pipelines::geometry_phases(&[1], &submission.samplers),
                    images: vec![ImageBinding { group: 4, binding: 1, resource: graph.resource_id("shadow").unwrap(), range: ImageSubresourceRange::WHOLE_DEPTH, stages: ShaderStages::FRAGMENT }],
                    samplers: vec![SamplerBinding { group: 4,binding: 2, sampling: SamplerDescriptor::depth_comparison(katla_gfx::CompareOp::LessOrEqual), stages: ShaderStages::FRAGMENT }],
                    ..Default::default()
                }).unwrap();
                let pixel = draw(&mut app, graph, submission.draw_list);
                if case == 1 {
                    neutral_pixel = Some(pixel);
                }
                if case == 9 {
                    assert_eq!(
                        Some(pixel),
                        neutral_pixel,
                        "A missing normal map must remain exactly flat at every scale"
                    );
                }
                let expected = reference(
                    Mat3::from(transform),
                    normal.to_array(),
                    [tangent.x(), tangent.y(), tangent.z(), -1.],
                    ReferenceSurface {
                        metallic,
                        roughness,
                        scale: normal_scale,
                        ao,
                        emission,
                        points,
                        shadowed,
                        textured,
                    },
                    &point_lights,
                    sun,
                );
                for channel in 0..3 {
                    let tolerance = 0.00005 + expected[channel].abs() * 0.0006;
                    maximum_error =
                        maximum_error.max((f64::from(pixel[channel]) - expected[channel]).abs());
                    assert!(
                        (f64::from(pixel[channel]) - expected[channel]).abs() <= tolerance,
                        "transform {transform_index} geometry {geometry} case {case}: {pixel:?}, expected {expected:?}"
                    );
                }
                assert_eq!(pixel[3], 1.);
                accepted_samples += 1;
            }
            app.renderer.destroy_mesh(mesh);
        }
    }
    println!(
        "material_lighting_acceptance: samples={accepted_samples}, maximum_absolute_rgb_error={maximum_error:.9}"
    );
    lit_graph.cleanup();
    shadowed_graph.cleanup();
    app.renderer.destroy();
}

fn constant(group: u32, binding: u32, stages: ShaderStages, bytes: &[u8]) -> ConstantBinding {
    ConstantBinding {
        group,
        binding,
        stages,
        bytes: bytes.to_vec(),
    }
}

fn graph(shadow: f32) -> crate::FrameGraph {
    let builder = FrameGraphBuilder::new()
        .create_resource(GraphResourceDesc {
            name: "shadow".into(),
            resource_type: GraphResourceType::DepthAttachment {
                clear_value: shadow,
                sampled: true,
            },
            format: ImageFormat::D32Sfloat,
            width: 4,
            height: 4,
            tracks_swapchain_size: false,
        })
        .create_resource(GraphResourceDesc {
            name: "result".into(),
            resource_type: GraphResourceType::ColorAttachment {
                clear_value: Some([0.; 4]),
            },
            format: ImageFormat::R16G16B16A16Sfloat,
            width: 33,
            height: 33,
            tracks_swapchain_size: false,
        })
        .add_pass(
            SimplePass::new("shadow clear", PassType::Graphics)
                .depth_ops(
                    AttachmentOps::clear(ClearValue::DepthStencil {
                        depth: shadow,
                        stencil: 0,
                    }),
                    AttachmentOps::dont_care(),
                )
                .depth_target("shadow"),
        )
        .add_pass(
            SimplePass::new("pbr", PassType::Graphics)
                .without_depth()
                .read("shadow")
                .write("result")
                .attachment(
                    "result",
                    AttachmentOps::clear(ClearValue::TRANSPARENT_BLACK),
                ),
        )
        .export_resource("result");
    #[cfg(not(target_os = "macos"))]
    {
        katla_gfx::render_graph::any_frame_graph::AnyFrameGraph::from_vulkan(
            builder.build::<katla_gfx::VulkanRenderer>().unwrap(),
        )
    }
    #[cfg(target_os = "macos")]
    {
        katla_gfx::render_graph::any_frame_graph::AnyFrameGraph::from_metal(
            builder.build::<katla_gfx::MetalRenderer>().unwrap(),
        )
    }
}

fn draw(
    app: &mut crate::application::Application,
    graph: &mut crate::FrameGraph,
    list: katla_gfx::renderer::DrawList,
) -> [f32; 4] {
    #[cfg(target_os = "macos")]
    {
        let image = app.renderer.create_offscreen_texture(
            crate::application::headless::HEADLESS_WIDTH,
            crate::application::headless::HEADLESS_HEIGHT,
        );
        app.renderer.set_headless_drawable(image);
    }
    let katla_gfx::FrameAcquisition::Ready(frame) = app.renderer.acquire_frame().unwrap() else {
        panic!("native frame");
    };
    app.renderer.execute_draw_calls(&frame, &list).unwrap();
    let pass = graph.pass_id("pbr").unwrap();
    app.renderer
        .render(&frame, graph, |submission| {
            submission.submit(pass, std::rc::Rc::new(list));
        })
        .unwrap();
    app.renderer.present(frame).unwrap();
    let source = app
        .renderer
        .graph_texture_source(graph.resource_id("result").unwrap())
        .unwrap();
    let ticket = app
        .renderer
        .queue_texture_readback(source, TextureReadbackRegion::pixel(16, 16))
        .unwrap();
    app.renderer.wait_for_device();
    let result = app.renderer.poll_texture_readback(ticket).unwrap().unwrap();
    result
        .bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|v| half::f16::from_bits(u16::from_ne_bytes(*v)).to_f32())
        .collect::<Vec<_>>()
        .try_into()
        .unwrap()
}

struct ReferenceSurface {
    metallic: f32,
    roughness: f32,
    scale: f32,
    ao: f32,
    emission: [f32; 3],
    points: bool,
    shadowed: bool,
    textured: bool,
}

fn reference(
    transform: Mat3,
    normal: [f32; 3],
    tangent: [f32; 4],
    surface: ReferenceSurface,
    lights: &[PointLightGPU; 256],
    sun: [f32; 3],
) -> [f64; 3] {
    let ReferenceSurface {
        metallic,
        roughness,
        scale,
        ao,
        emission,
        points,
        shadowed,
        textured,
    } = surface;
    let matrix: [[f64; 3]; 3] = std::array::from_fn(|col| transform[col].to_array().map(f64::from));
    let multiply = |value: [f64; 3]| {
        std::array::from_fn(|row| (0..3).map(|col| matrix[col][row] * value[col]).sum::<f64>())
    };
    let cofactors = [
        cross(matrix[1], matrix[2]),
        cross(matrix[2], matrix[0]),
        cross(matrix[0], matrix[1]),
    ];
    let sign = if dot(matrix[0], cofactors[0]) < 0. {
        -1.
    } else {
        1.
    };
    let n = normalized(std::array::from_fn(|row| {
        (0..3)
            .map(|col| cofactors[col][row] * f64::from(normal[col]) * sign)
            .sum::<f64>()
    }));
    let t = multiply([
        f64::from(tangent[0]),
        f64::from(tangent[1]),
        f64::from(tangent[2]),
    ]);
    let t = normalized(std::array::from_fn(|i| t[i] - n[i] * dot(n, t)));
    let b = cross(n, t).map(|value| value * f64::from(tangent[3]) * sign);
    let map = if textured {
        normalized([0.5 * f64::from(scale), 0., 1.])
    } else {
        [0., 0., 1.]
    };
    let n = normalized(std::array::from_fn(|i| {
        t[i] * map[0] + b[i] * map[1] + n[i] * map[2]
    }));
    let albedo = [0.6, 0.3, 0.15];
    let mut result =
        std::array::from_fn(|i| 0.15 * albedo[i] * f64::from(ao) + f64::from(emission[i]));
    if !shadowed {
        let direct = brdf(
            n,
            [0., 0., 1.],
            sun.map(f64::from),
            albedo,
            f64::from(metallic),
            f64::from(roughness),
            [1.7, 1.36, 1.02],
        );
        for i in 0..3 {
            result[i] += direct[i];
        }
    }
    if points {
        for light in &lights[..2] {
            let to_light = [
                f64::from(light.position[0]),
                f64::from(light.position[1]),
                f64::from(light.position[2]) - 0.5,
            ];
            let distance = dot(to_light, to_light).sqrt();
            let attenuation = (1. - distance / f64::from(light.range)).powi(2);
            let radiance = light
                .color
                .map(|v| f64::from(v) * f64::from(light.intensity) * attenuation);
            let direct = brdf(
                n,
                [0., 0., 1.],
                normalized(to_light),
                albedo,
                f64::from(metallic),
                f64::from(roughness),
                radiance,
            );
            for i in 0..3 {
                result[i] += direct[i];
            }
        }
    }
    result
}

fn brdf(
    n: [f64; 3],
    v: [f64; 3],
    l: [f64; 3],
    albedo: [f64; 3],
    metallic: f64,
    roughness: f64,
    radiance: [f64; 3],
) -> [f64; 3] {
    let no_v = dot(n, v).clamp(0., 1.);
    let no_l = dot(n, l).clamp(0., 1.);
    if no_v <= 0. || no_l <= 0. {
        return [0.; 3];
    }
    let h = normalized(std::array::from_fn(|i| v[i] + l[i]));
    let alpha_squared = roughness.clamp(0.04, 1.).powi(4);
    let no_h = dot(n, h).clamp(0., 1.);
    let distribution =
        alpha_squared / (std::f64::consts::PI * (1. + no_h.powi(2) * (alpha_squared - 1.)).powi(2));
    let visibility = 0.5
        / (no_l * (no_v.powi(2) * (1. - alpha_squared) + alpha_squared).sqrt()
            + no_v * (no_l.powi(2) * (1. - alpha_squared) + alpha_squared).sqrt());
    let fresnel_weight = (1. - dot(h, v).clamp(0., 1.)).powi(5);
    std::array::from_fn(|i| {
        let f0 = 0.04 * (1. - metallic) + albedo[i] * metallic;
        let f = f0 + (1. - f0) * fresnel_weight;
        ((1. - f) * (1. - metallic) * albedo[i] / std::f64::consts::PI
            + distribution * visibility * f)
            * radiance[i]
            * no_l
    })
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a.into_iter().zip(b).map(|(a, b)| a * b).sum()
}
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn normalized(v: [f64; 3]) -> [f64; 3] {
    let length = dot(v, v).sqrt();
    v.map(|v| v / length)
}

fn matrix(flat: [f32; 16]) -> Mat4 {
    Mat4(std::array::from_fn(|col| {
        katla_math::Vec4::new(
            flat[col * 4],
            flat[col * 4 + 1],
            flat[col * 4 + 2],
            flat[col * 4 + 3],
        )
    }))
}
