//! Native sampler filtering, coordinate wrapping and phase isolation.

use super::*;
use crate::{
    AddressMode, ConstantBinding, CullMode, DepthState, FilterMode, ImageBinding, ImageFormat,
    MipFilter, PassBindings, PassDraw, PassDrawPhase, PassPipeline, PipelineDescriptor,
    SamplerBinding, SamplerDescriptor, ShaderStages, TextureDescriptor, TextureReadbackRegion,
    VertexLayout,
};

const SHADER: &str = r#"
@group(0) @binding(0) var<uniform> params:vec4f;
@group(5) @binding(0) var image:texture_2d<f32>;
@group(5) @binding(1) var image_sampler:sampler;
@vertex fn vs_main(@builtin(vertex_index) i:u32)->@builtin(position) vec4f {
 let p=array<vec2f,3>(vec2f(-1,-1),vec2f(3,-1),vec2f(-1,3));return vec4f(p[i],0,1);
}
@fragment fn fs_main()->@location(0) vec4f {
 if (params.w>0.) {return textureSampleGrad(image,image_sampler,params.xy,vec2f(params.w,0),vec2f(0,params.w));}
 return textureSampleLevel(image,image_sampler,params.xy,params.z);
}
"#;

fn sampler(descriptor: SamplerDescriptor) -> SamplerBinding {
    SamplerBinding {
        group: 5,
        binding: 1,
        stages: ShaderStages::FRAGMENT,
        sampling: descriptor,
    }
}

fn draw_pixels(
    renderer: &mut NativeRenderer,
    graph: &mut FrameGraph<NativeRenderer>,
    xs: &[u32],
) -> Vec<Vec<u8>> {
    let frame = acquire(renderer);
    renderer.render(&frame, graph, |_| {}).unwrap();
    renderer.present(frame).unwrap();
    let source = renderer
        .graph_texture_source(graph.resource_id("result").unwrap())
        .unwrap();
    let tickets: Vec<_> = xs
        .iter()
        .map(|x| {
            renderer
                .queue_texture_readback(source, TextureReadbackRegion::pixel(*x, 8))
                .unwrap()
        })
        .collect();
    renderer.wait_for_device();
    tickets
        .into_iter()
        .map(|ticket| {
            renderer
                .poll_texture_readback(ticket)
                .unwrap()
                .unwrap()
                .bytes
        })
        .collect()
}

#[test]
fn test_native_depth_comparison_samplers_override_by_phase() {
    #[cfg(target_os = "macos")]
    {
        assert_eq!(std::env::var("MTL_DEBUG_LAYER").as_deref(), Ok("1"));
        assert_eq!(
            std::env::var("METAL_DEVICE_WRAPPER_TYPE").as_deref(),
            Ok("1")
        );
    }
    let mut renderer = renderer();
    let errors = capture_validation_errors(&renderer);
    let path = std::env::temp_dir().join(format!(
        "katla-depth-sampler-{}.wgsl",
        crate::renderer::texture_readback::fresh_readback_id()
    ));
    let source = SHADER
        .replace("texture_2d<f32>", "texture_depth_2d")
        .replace("image_sampler:sampler", "image_sampler:sampler_comparison");
    let source = format!(
        "{}\n@fragment fn fs_main()->@location(0) vec4f {{let value=textureSampleCompare(image,image_sampler,params.xy,params.z);return vec4f(vec3f(value),1);}}",
        source.split("@fragment").next().unwrap()
    );
    std::fs::write(&path, source).unwrap();
    let material = renderer
        .compile_material(
            &PipelineDescriptor::simple(path.to_string_lossy())
                .with_vertex_layout(VertexLayout::empty())
                .with_color_format(ImageFormat::R8G8B8A8Unorm)
                .with_depth(DepthState::disabled())
                .with_depth_format(None)
                .with_cull(CullMode::None),
        )
        .unwrap();
    std::fs::remove_file(path).unwrap();
    let mut graph = FrameGraphBuilder::new()
        .create_resource(GraphResourceDesc {
            name: "depth".into(),
            resource_type: GraphResourceType::DepthAttachment {
                clear_value: 0.5,
                sampled: true,
            },
            format: ImageFormat::D32Sfloat,
            width: 16,
            height: 16,
            tracks_swapchain_size: false,
        })
        .create_resource(GraphResourceDesc {
            name: "result".into(),
            resource_type: GraphResourceType::ColorAttachment { clear_value: None },
            format: ImageFormat::R8G8B8A8Unorm,
            width: 16,
            height: 16,
            tracks_swapchain_size: false,
        })
        .add_pass(
            SimplePass::new("clear", PassType::Graphics)
                .depth_ops(
                    crate::render_pass::AttachmentOps::clear(
                        crate::render_pass::ClearValue::DepthStencil {
                            depth: 0.5,
                            stencil: 0,
                        },
                    ),
                    crate::render_pass::AttachmentOps::dont_care(),
                )
                .depth_target("depth"),
        )
        .add_pass(
            GeometryPass::new("probe")
                .without_depth()
                .read("depth")
                .write_color("result", ImageFormat::R8G8B8A8Unorm),
        )
        .export_resource("result")
        .build::<NativeRenderer>()
        .unwrap();
    let pass = graph.pass_id("probe").unwrap();
    let comparisons = [
        crate::CompareOp::Never,
        crate::CompareOp::Less,
        crate::CompareOp::Equal,
        crate::CompareOp::LessOrEqual,
        crate::CompareOp::Greater,
        crate::CompareOp::NotEqual,
        crate::CompareOp::GreaterOrEqual,
        crate::CompareOp::Always,
    ];
    for (reference, expected) in [
        (0.5, [0u8, 0, 255, 255, 0, 0, 255, 255]),
        (0.25, [0, 255, 0, 255, 0, 255, 0, 255]),
        (0.75, [0, 0, 0, 0, 255, 255, 255, 255]),
    ] {
        graph
            .set_pass_bindings(
                pass,
                PassBindings {
                    images: vec![ImageBinding {
                        group: 5,
                        binding: 0,
                        resource: graph.resource_id("depth").unwrap(),
                        range: ImageSubresourceRange::WHOLE_DEPTH,
                        stages: ShaderStages::FRAGMENT,
                    }],
                    samplers: vec![sampler(SamplerDescriptor::depth_comparison(
                        crate::CompareOp::LessOrEqual,
                    ))],
                    phases: comparisons
                        .iter()
                        .enumerate()
                        .map(|(i, comparison)| PassDrawPhase {
                            pipelines: vec![PassPipeline {
                                material,
                                vertex_layout: VertexLayout::empty(),
                            }],
                            constants: vec![ConstantBinding {
                                group: 0,
                                binding: 0,
                                stages: ShaderStages::FRAGMENT,
                                bytes: [0.5f32, 0.5, reference, 0.]
                                    .into_iter()
                                    .flat_map(f32::to_ne_bytes)
                                    .collect(),
                            }],
                            samplers: vec![sampler(SamplerDescriptor::depth_comparison(
                                *comparison,
                            ))],
                            draw: PassDraw::Vertices {
                                count: 3,
                                instances: 1,
                            },
                            viewport: Some(crate::Rect::new(
                                [(i * 2) as f32, 0.],
                                [(i * 2 + 2) as f32, 16.],
                            )),
                        })
                        .collect(),
                    ..Default::default()
                },
            )
            .unwrap();
        let pixels = draw_pixels(&mut renderer, &mut graph, &[1, 3, 5, 7, 9, 11, 13, 15]);
        for (index, (pixel, expected)) in pixels.iter().zip(expected).enumerate() {
            assert_eq!(
                pixel.as_slice(),
                [expected, expected, expected, 255],
                "{:?}, reference {reference}",
                comparisons[index]
            );
        }
    }
    graph.cleanup();
    renderer.destroy();
    let errors = errors.lock().unwrap();
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn test_native_sampler_filters_wraps_mips_and_phase_overrides() {
    #[cfg(target_os = "macos")]
    {
        assert_eq!(std::env::var("MTL_DEBUG_LAYER").as_deref(), Ok("1"));
        assert_eq!(
            std::env::var("METAL_DEVICE_WRAPPER_TYPE").as_deref(),
            Ok("1")
        );
    }
    let mut renderer = renderer();
    let errors = capture_validation_errors(&renderer);
    let path = std::env::temp_dir().join(format!(
        "katla-sampler-{}.wgsl",
        crate::renderer::texture_readback::fresh_readback_id()
    ));
    std::fs::write(&path, SHADER).unwrap();
    let material = renderer
        .compile_material(
            &PipelineDescriptor::simple(path.to_string_lossy())
                .with_vertex_layout(VertexLayout::empty())
                .with_color_format(ImageFormat::R8G8B8A8Unorm)
                .with_depth(DepthState::disabled())
                .with_depth_format(None)
                .with_cull(CullMode::None),
        )
        .unwrap();
    std::fs::remove_file(path).unwrap();
    let pixels: Vec<_> = (0..4)
        .flat_map(|y| {
            (0..4).flat_map(move |x| {
                [
                    if x < 2 { 0 } else { 255 },
                    if y < 2 { 0 } else { 255 },
                    0u8,
                    255,
                ]
            })
        })
        .collect();
    let mut descriptor = TextureDescriptor::new(4, 4, ImageFormat::R8G8B8A8Unorm);
    descriptor.mip_levels = 3;
    descriptor.generate_mips = true;
    let texture = renderer.create_texture(&descriptor, &pixels).unwrap();
    let mut graph = FrameGraphBuilder::new()
        .import_resource(
            "image",
            texture,
            ImportedImageContract::arrives_in(ResourceState::ShaderRead),
        )
        .create_resource(GraphResourceDesc {
            name: "result".into(),
            resource_type: GraphResourceType::ColorAttachment { clear_value: None },
            format: ImageFormat::R8G8B8A8Unorm,
            width: 16,
            height: 16,
            tracks_swapchain_size: false,
        })
        .add_pass(
            GeometryPass::new("probe")
                .without_depth()
                .read("image")
                .write_color("result", ImageFormat::R8G8B8A8Unorm),
        )
        .export_resource("result")
        .build::<NativeRenderer>()
        .unwrap();
    let image = graph.resource_id("image").unwrap();
    let pass = graph.pass_id("probe").unwrap();
    let nearest = SamplerDescriptor::nearest_clamp();
    let repeated = SamplerDescriptor {
        address_u: AddressMode::Repeat,
        address_v: AddressMode::Repeat,
        ..nearest
    };
    let mirrored = SamplerDescriptor {
        address_u: AddressMode::MirroredRepeat,
        address_v: AddressMode::MirroredRepeat,
        ..nearest
    };
    let mipped = SamplerDescriptor {
        mip_filter: MipFilter::Nearest,
        ..nearest
    };
    let cases: [(Option<SamplerDescriptor>, [f32; 4], [u8; 4]); 14] = [
        (Some(repeated), [1.125, 1.125, 0., 0.], [0, 0, 0, 255]),
        (Some(mirrored), [1.125, 1.125, 0., 0.], [255, 255, 0, 255]),
        (Some(mirrored), [1.875, 1.875, 0., 0.], [0, 0, 0, 255]),
        (None, [1.125, 1.125, 0., 0.], [255, 255, 0, 255]),
        (Some(repeated), [-0.125, -0.125, 0., 0.], [255, 255, 0, 255]),
        (Some(mirrored), [-0.125, -0.125, 0., 0.], [0, 0, 0, 255]),
        (
            Some(SamplerDescriptor::linear_clamp()),
            [0.5, 0.125, 0., 0.001],
            [128, 0, 0, 255],
        ),
        (
            Some(SamplerDescriptor {
                min_filter: FilterMode::Linear,
                ..mipped
            }),
            [0.5, 0.125, 0., 0.001],
            [255, 0, 0, 255],
        ),
        (
            Some(SamplerDescriptor {
                min_filter: FilterMode::Linear,
                ..mipped
            }),
            [0.5, 0.125, 0., 0.5],
            [128, 0, 0, 255],
        ),
        (
            Some(SamplerDescriptor {
                mag_filter: FilterMode::Linear,
                ..mipped
            }),
            [0.5, 0.125, 0., 0.5],
            [255, 0, 0, 255],
        ),
        (Some(mipped), [0.125, 0.125, 1.5, 0.], [128, 128, 0, 255]),
        (
            Some(SamplerDescriptor {
                mip_filter: MipFilter::Linear,
                ..nearest
            }),
            [0.125, 0.125, 1.5, 0.],
            [64, 64, 0, 255],
        ),
        (None, [0.125, 0.125, 2., 0.], [0, 0, 0, 255]),
        (
            Some(SamplerDescriptor {
                anisotropy: 16,
                ..SamplerDescriptor::linear_repeat()
            }),
            [0.5, 0.5, 2., 0.],
            [128, 128, 0, 255],
        ),
    ];
    for (batch_index, batch) in cases.chunks(4).enumerate() {
        let phases = batch
            .iter()
            .enumerate()
            .map(|(index, (sampling, params, _))| PassDrawPhase {
                samplers: sampling.map(sampler).into_iter().collect(),
                pipelines: vec![PassPipeline {
                    material,
                    vertex_layout: VertexLayout::empty(),
                }],
                constants: vec![ConstantBinding {
                    group: 0,
                    binding: 0,
                    stages: ShaderStages::FRAGMENT,
                    bytes: params
                        .iter()
                        .flat_map(|value: &f32| value.to_ne_bytes())
                        .collect(),
                }],
                draw: PassDraw::Vertices {
                    count: 3,
                    instances: 1,
                },
                viewport: Some(crate::Rect::new(
                    [(index * 4) as f32, 0.],
                    [(index * 4 + 4) as f32, 16.],
                )),
            })
            .collect();
        graph
            .set_pass_bindings(
                pass,
                PassBindings {
                    images: vec![ImageBinding {
                        group: 5,
                        binding: 0,
                        resource: image,
                        range: ImageSubresourceRange::whole(ImageAspects::COLOR),
                        stages: ShaderStages::FRAGMENT,
                    }],
                    samplers: vec![sampler(nearest)],
                    phases,
                    ..Default::default()
                },
            )
            .unwrap();
        let xs: Vec<_> = (0..batch.len())
            .map(|index| (index * 4 + 2) as u32)
            .collect();
        let pixels = draw_pixels(&mut renderer, &mut graph, &xs);
        for (index, (actual, (_, _, expected))) in pixels.iter().zip(batch).enumerate() {
            assert!(
                actual
                    .iter()
                    .zip(expected)
                    .all(|(a, b)| i16::from(*a).abs_diff(i16::from(*b)) <= 2),
                "case {}: {actual:?} != {expected:?}",
                batch_index * 4 + index
            );
        }
    }
    let mut packet = graph
        .pass(graph.pass_position(pass).unwrap())
        .unwrap()
        .bindings
        .clone();
    packet.images[0].range = ImageSubresourceRange::new(ImageAspects::COLOR, 2, 1, 0, 1);
    packet.phases.truncate(1);
    packet.phases[0].samplers.clear();
    packet.phases[0].constants[0].bytes = [0.125f32, 0.125, 0., 0.]
        .into_iter()
        .flat_map(f32::to_ne_bytes)
        .collect();
    graph.set_pass_bindings(pass, packet).unwrap();
    let actual = draw_pixels(&mut renderer, &mut graph, &[2]).pop().unwrap();
    assert!(
        actual[0].abs_diff(128) <= 1 && actual[1].abs_diff(128) <= 1,
        "{actual:?}"
    );
    graph.cleanup();
    renderer.destroy_texture(texture);
    renderer.destroy();
    let errors = errors.lock().unwrap();
    assert!(errors.is_empty(), "{errors:?}");
}
