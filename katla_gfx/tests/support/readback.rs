//! Read one exact committed graph export for native pixel assertions.

use katla_gfx::render_graph::ResourceId;
use katla_gfx::{GpuRenderer, GraphTextureSource, TextureReadbackRegion};
use std::time::{Duration, Instant};

pub fn read_pixels(
    renderer: &mut impl GpuRenderer,
    resource: ResourceId,
) -> (GraphTextureSource, Vec<u8>) {
    let source =
        GpuRenderer::graph_texture_source(renderer, resource).expect("committed graph export");
    let size = renderer.swapchain_extent();
    let ticket = GpuRenderer::queue_texture_readback(
        renderer,
        source,
        TextureReadbackRegion {
            origin: [0, 0],
            size,
            mip_level: 0,
            array_layer: 0,
        },
    )
    .expect("queue exported texture readback");
    assert_eq!(ticket.source, source);
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(data) = GpuRenderer::poll_texture_readback(renderer, ticket)
            .expect("poll exact readback ticket")
        {
            assert_eq!(data.size, size);
            return (source, data.bytes);
        }
        assert!(Instant::now() < deadline, "GPU readback did not complete");
        std::thread::sleep(Duration::from_millis(1));
    }
}
