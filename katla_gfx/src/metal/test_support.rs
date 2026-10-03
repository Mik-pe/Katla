//! Small ordinary-resource fixtures for native Metal contract tests.
use super::metal_renderer::MetalRenderer;
use crate::handle::MaterialHandle;
use crate::renderer::frame_bindings::{PassBindings, PassDraw, PassDrawPhase, PassPipeline};
use crate::renderer::frame_scope::{FrameAcquisition, FrameToken};
use crate::renderer::texture_readback::{TextureReadbackData, TextureReadbackTicket};
use crate::texture::{ImageFormat, TextureDescriptor, TextureUsage};
use crate::vertex::VertexLayout;
use crate::{GpuRenderer, PipelineDescriptor};

pub(super) fn material(
    renderer: &mut MetalRenderer,
    source: &str,
    mut descriptor: PipelineDescriptor,
) -> MaterialHandle {
    let path = std::env::temp_dir().join(format!(
        "katla-metal-fixture-{}.wgsl",
        crate::renderer::texture_readback::fresh_readback_id()
    ));
    std::fs::write(&path, source).unwrap();
    descriptor.shader_path = path.to_string_lossy().into_owned();
    let result = renderer.compile_material(&descriptor);
    std::fs::remove_file(path).unwrap();
    result.unwrap()
}
pub(super) fn fullscreen_descriptor(format: ImageFormat) -> PipelineDescriptor {
    PipelineDescriptor::pbr("")
        .with_vertex_layout(VertexLayout::new(vec![]))
        .with_depth_format(None)
        .with_depth(crate::DepthState::disabled())
        .with_cull(crate::CullMode::None)
        .with_color_format(format)
}
pub(super) fn vertices(material: MaterialHandle, layout: VertexLayout, count: u32) -> PassBindings {
    PassBindings {
        phases: vec![PassDrawPhase {
            samplers: Vec::new(),
            pipelines: vec![PassPipeline {
                material,
                vertex_layout: layout,
            }],
            constants: vec![],
            draw: PassDraw::Vertices {
                count,
                instances: 1,
            },
            viewport: None,
        }],
        ..Default::default()
    }
}
pub(super) fn acquire(renderer: &mut MetalRenderer, extent: u32) -> FrameToken {
    let desc = TextureDescriptor::new(extent, extent, ImageFormat::B8G8R8A8Srgb)
        .with_usage(TextureUsage::COLOR_ATTACHMENT);
    let (_, view) = renderer.context.create_texture_shared(&desc).unwrap();
    renderer.set_headless_drawable(view.inner);
    let FrameAcquisition::Ready(frame) = renderer.acquire_frame().unwrap() else {
        panic!("headless frame")
    };
    frame
}
pub(super) fn readback(
    renderer: &mut MetalRenderer,
    ticket: TextureReadbackTicket,
) -> TextureReadbackData {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        if let Some(data) = renderer.poll_texture_readback(ticket).unwrap() {
            return data;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "native readback timed out"
        );
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}
pub(super) const FULLSCREEN_VERTEX: &str = "@vertex fn vs_main(@builtin(vertex_index) i:u32)->@builtin(position) vec4<f32>{let p=array<vec2<f32>,3>(vec2<f32>(-1.,-1.),vec2<f32>(3.,-1.),vec2<f32>(-1.,3.));return vec4<f32>(p[i],0.,1.);}";
