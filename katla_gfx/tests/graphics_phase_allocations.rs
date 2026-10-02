//! CPU allocation growth of the actual Vulkan graphics encoding path.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use katla_gfx::render_graph::{FrameGraphBuilder, GeometryPass};
use katla_gfx::renderer::frame_scope::FrameAcquisition;
use katla_gfx::{
    ConstantBinding, GpuRenderer, ImageFormat, PassBindings, PassDraw, PassDrawPhase, PassPipeline,
    ShaderStages, ValidationMode, VertexLayout, VulkanRenderer,
};

static COUNTING: AtomicBool = AtomicBool::new(false);
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

struct CountingAllocator;

fn count() {
    if COUNTING.load(Ordering::Relaxed) {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
    }
}

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count();
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count();
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        count();
        unsafe { System.realloc(ptr, layout, size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

#[test]
#[ignore = "requires a Vulkan device"]
fn test_native_graphics_phase_encoding_allocations_grow_linearly() {
    let mut renderer = VulkanRenderer::init_headless(
        4,
        4,
        ValidationMode::Enabled,
        c"phase allocation growth".into(),
        c"Katla".into(),
    )
    .unwrap();
    assert!(renderer.context().validation_active());
    let errors = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let captured = errors.clone();
    renderer
        .context()
        .set_validation_callback(move |message, level| {
            if level == katla_gfx::ValidationLevel::Error {
                captured.lock().unwrap().push(message.to_owned());
            }
        });
    let path = std::env::temp_dir().join(format!(
        "katla-phase-allocations-{}.wgsl",
        std::process::id()
    ));
    std::fs::write(
        &path,
        r#"
@group(0) @binding(0) var<uniform> tint: vec4f;
@vertex fn vs_main(@builtin(vertex_index) i:u32)->@builtin(position) vec4f {
    let p=array<vec2f,3>(vec2f(-1.,-1.),vec2f(3.,-1.),vec2f(-1.,3.));
    return vec4f(p[i],0.,1.);
}
@fragment fn fs_main()->@location(0) vec4f { return tint; }
"#,
    )
    .unwrap();
    let descriptor = katla_gfx::PipelineDescriptor::simple(path.to_string_lossy())
        .with_vertex_layout(VertexLayout::empty())
        .with_color_format(ImageFormat::B8G8R8A8Srgb)
        .with_depth(katla_gfx::DepthState::disabled())
        .with_depth_format(None)
        .with_cull(katla_gfx::CullMode::None);
    let material = renderer.compile_material(&descriptor).unwrap();
    std::fs::remove_file(path).unwrap();
    let mut graph = FrameGraphBuilder::new()
        .add_pass(
            GeometryPass::new("draw")
                .without_depth()
                .write_color("backbuffer", ImageFormat::B8G8R8A8Srgb),
        )
        .export_resource("backbuffer")
        .build::<VulkanRenderer>()
        .unwrap();
    let pass = graph.pass_id("draw").unwrap();
    let mut measurements = Vec::new();
    for phase_count in [64, 256] {
        let packet = PassBindings {
            phases: (0..phase_count)
                .map(|_| PassDrawPhase {
                    pipelines: vec![PassPipeline {
                        material,
                        vertex_layout: VertexLayout::empty(),
                    }],
                    constants: vec![ConstantBinding {
                        group: 0,
                        binding: 0,
                        stages: ShaderStages::FRAGMENT,
                        bytes: [0.0f32, 1., 0., 1.]
                            .into_iter()
                            .flat_map(f32::to_ne_bytes)
                            .collect(),
                    }],
                    draw: PassDraw::Vertices {
                        count: 3,
                        instances: 1,
                    },
                    viewport: None,
                })
                .collect(),
            ..Default::default()
        };
        graph.set_pass_bindings(pass, packet).unwrap();
        for iteration in 0..3 {
            let FrameAcquisition::Ready(frame) = renderer.acquire_frame().unwrap() else {
                panic!("headless acquisition");
            };
            ALLOCATIONS.store(0, Ordering::Relaxed);
            COUNTING.store(iteration == 2, Ordering::Relaxed);
            let result = renderer.render(&frame, &mut graph, |_| {});
            COUNTING.store(false, Ordering::Relaxed);
            let allocations = ALLOCATIONS.load(Ordering::Relaxed);
            result.unwrap();
            renderer.present(frame).unwrap().surface.unwrap();
            if iteration == 2 {
                measurements.push(allocations);
            }
        }
    }
    let source = renderer
        .graph_texture_source(graph.resource_id("backbuffer").unwrap())
        .unwrap();
    let ticket = renderer
        .queue_texture_readback(source, katla_gfx::TextureReadbackRegion::pixel(1, 1))
        .unwrap();
    renderer.wait_for_device();
    assert_eq!(
        renderer
            .poll_texture_readback(ticket)
            .unwrap()
            .unwrap()
            .bytes,
        [0, 255, 0, 255]
    );
    graph.cleanup();
    renderer.destroy();
    drop(renderer);
    let errors = errors.lock().unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    assert!(measurements[0] > 0);
    eprintln!(
        "Graphics encoding allocations: 64 phases = {}, 256 phases = {}",
        measurements[0], measurements[1]
    );
    assert!(
        measurements[1] < measurements[0] * 6,
        "Quadrupling phases must keep allocation growth below 6x: {measurements:?}"
    );
}
