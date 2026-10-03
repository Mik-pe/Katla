//! Native material registration, reload latency and steady-frame Rust allocations.
//!
//! Run with `cargo bench -p katla_gfx --bench material_instances` on Vulkan or Metal 4.
//! The first registration uses a fresh renderer cache, not a cleared driver cache.
//! This isolates material/pipeline cost; it does not time model decoding or uploads.

use std::alloc::{GlobalAlloc, Layout, System};
use std::ffi::CString;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use katla_gfx::render_graph::{
    FrameGraph, FrameGraphBuilder, GeometryPass, GraphResourceDesc, GraphResourceType,
};
use katla_gfx::renderer::frame_scope::FrameAcquisition;
use katla_gfx::{
    CullMode, DepthState, GpuRenderer, ImageFormat, MaterialHandle, PassBindings, PassDraw,
    PassDrawPhase, PassPipeline, PipelineDescriptor, TextureReadbackRegion, ValidationMode,
    VertexLayout,
};

#[cfg(target_os = "macos")]
use katla_gfx::MetalRenderer as NativeRenderer;
#[cfg(not(target_os = "macos"))]
use katla_gfx::VulkanRenderer as NativeRenderer;

static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

struct CountingAllocator;

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.realloc(ptr, layout, new_size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL_ALLOCATOR: CountingAllocator = CountingAllocator;

const SOURCE: &str = r#"
@vertex fn vs_main(@builtin(vertex_index) index:u32)->@builtin(position) vec4f {
    let p=array<vec2f,3>(vec2f(-1.,-1.),vec2f(3.,-1.),vec2f(-1.,3.));
    return vec4f(p[index],0.,1.);
}
@fragment fn fs_main()->@location(0) vec4f { return vec4f(1.,0.,0.,1.); }
"#;

fn measure<T>(label: &str, operation: impl FnOnce() -> T) -> T {
    let allocations = ALLOCATIONS.load(Ordering::Relaxed);
    let start = Instant::now();
    let result = operation();
    let elapsed_us = start.elapsed().as_micros();
    let allocations = ALLOCATIONS.load(Ordering::Relaxed) - allocations;
    println!("MATERIAL_BENCH phase={label} elapsed_us={elapsed_us} rust_allocations={allocations}");
    result
}

fn frame(renderer: &mut NativeRenderer, graph: &mut FrameGraph<NativeRenderer>) {
    let FrameAcquisition::Ready(frame) = renderer.acquire_frame().expect("headless acquire") else {
        panic!("headless frame unavailable");
    };
    renderer
        .render(&frame, graph, |_| {})
        .expect("native frame");
    renderer.present(frame).expect("native submit");
}

fn pixel(renderer: &mut NativeRenderer, graph: &FrameGraph<NativeRenderer>) -> Vec<u8> {
    let source = renderer
        .graph_texture_source(
            graph
                .resource_id("material_color")
                .expect("export resource"),
        )
        .expect("committed export");
    let ticket = renderer
        .queue_texture_readback(source, TextureReadbackRegion::pixel(8, 8))
        .expect("readback enqueue");
    renderer.wait_for_device();
    renderer
        .poll_texture_readback(ticket)
        .expect("readback poll")
        .expect("completed readback")
        .bytes
}

fn main() {
    let path =
        std::env::temp_dir().join(format!("katla-material-bench-{}.wgsl", std::process::id()));
    std::fs::write(&path, SOURCE).expect("benchmark source");
    let mut renderer = NativeRenderer::init_headless(
        16,
        16,
        ValidationMode::Disabled,
        CString::new("material instance benchmark").expect("application name"),
        CString::new("Katla").expect("engine name"),
    )
    .expect("native device");
    #[cfg(target_os = "macos")]
    {
        use objc2_metal::{
            MTLCreateSystemDefaultDevice, MTLDevice, MTLPixelFormat, MTLTextureDescriptor,
            MTLTextureUsage,
        };
        let device = MTLCreateSystemDefaultDevice().expect("Metal drawable device");
        let descriptor = unsafe {
            MTLTextureDescriptor::texture2DDescriptorWithPixelFormat_width_height_mipmapped(
                MTLPixelFormat::BGRA8Unorm_sRGB,
                16,
                16,
                false,
            )
        };
        descriptor.setUsage(MTLTextureUsage::RenderTarget);
        renderer.set_headless_drawable(
            device
                .newTextureWithDescriptor(&descriptor)
                .expect("headless drawable"),
        );
    }
    let descriptor = PipelineDescriptor::simple(path.to_string_lossy())
        .with_vertex_layout(VertexLayout::empty())
        .with_color_format(ImageFormat::B8G8R8A8Srgb)
        .with_depth(DepthState::disabled())
        .with_depth_format(None)
        .with_cull(CullMode::None);
    let first = measure("first_registration", || {
        renderer
            .compile_material(&descriptor)
            .expect("first material")
    });
    let mut materials = measure("warm_registration_31", || {
        (0..31)
            .map(|_| {
                renderer
                    .compile_material(&descriptor)
                    .expect("warm material")
            })
            .collect::<Vec<MaterialHandle>>()
    });
    materials.push(first);
    let mut graph = FrameGraphBuilder::new()
        .create_resource(GraphResourceDesc {
            name: "material_color".into(),
            resource_type: GraphResourceType::ColorAttachment {
                clear_value: Some([0.0; 4]),
            },
            format: ImageFormat::B8G8R8A8Srgb,
            width: 16,
            height: 16,
            tracks_swapchain_size: false,
        })
        .add_pass(
            GeometryPass::new("materials")
                .without_depth()
                .write_color("material_color", ImageFormat::B8G8R8A8Srgb),
        )
        .export_resource("material_color")
        .build::<NativeRenderer>()
        .expect("benchmark graph");
    let pass = graph.pass_id("materials").expect("material pass");
    graph
        .set_pass_bindings(
            pass,
            PassBindings {
                phases: materials
                    .iter()
                    .map(|&material| PassDrawPhase {
                        pipelines: vec![PassPipeline {
                            material,
                            vertex_layout: VertexLayout::empty(),
                        }],
                        constants: vec![],
                        draw: PassDraw::Vertices {
                            count: 3,
                            instances: 1,
                        },
                        viewport: None,
                    })
                    .collect(),
                ..Default::default()
            },
        )
        .expect("material packets");
    for _ in 0..6 {
        frame(&mut renderer, &mut graph);
    }
    assert_eq!(pixel(&mut renderer, &graph), [0, 0, 255, 255]);
    measure("steady_frames_100", || {
        for _ in 0..100 {
            frame(&mut renderer, &mut graph);
        }
        renderer.wait_for_device();
    });
    std::fs::write(
        &path,
        SOURCE.replace("vec4f(1.,0.,0.,1.)", "vec4f(0.,1.,0.,1.)"),
    )
    .expect("changed source");
    measure("reload_32_to_green_pixels", || {
        assert_eq!(renderer.recompile_materials_for_shader(&path), 32);
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            frame(&mut renderer, &mut graph);
            if pixel(&mut renderer, &graph) == [0, 255, 0, 255] {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "replacement did not reach native output"
            );
        }
    });
    for material in materials {
        renderer.destroy_material(material);
    }
    graph.cleanup();
    renderer.destroy();
    std::fs::remove_file(path).expect("benchmark cleanup");
}
