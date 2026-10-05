//! Native completion and storage isolation across the three renderer frame slots.

use super::context::MetalContext;
use super::metal_renderer::MetalRenderer;
use crate::GpuRenderer;
use crate::backend::command::{GpuBlitEncoder, GpuCommandBuffer};
use crate::backend::resource::{GpuBuffer, GpuImageView};
use crate::render_graph::{
    FrameGraphBuilder, GraphResourceDesc, GraphResourceType, PassKind, PassType, SimplePass,
};
use crate::render_pass::{AttachmentOps, ClearValue};
use crate::renderer::frame_scope::FrameAcquisition;
use crate::renderer::types::UIDrawList;
use crate::texture::{ImageFormat, TextureDescriptor, TextureUsage};
use crate::vertex::VertexUI;
use objc2_metal::MTLBuffer;

#[test]
fn test_native_three_frame_slots_preserve_uploads_through_resize() {
    let mut renderer =
        MetalRenderer::new(MetalContext::init_headless_with_size(16, 16).unwrap()).unwrap();
    let stream_desc =
        TextureDescriptor::new(1, 1, ImageFormat::R8G8B8A8Unorm).with_usage(TextureUsage::SAMPLED);
    let stream = renderer
        .create_texture_impl(&stream_desc, &[0, 0, 0, 255])
        .unwrap();
    let buffers = (0..3)
        .map(|_| {
            renderer
                .create_buffer(crate::render_graph::BufferDesc::new(
                    16,
                    crate::render_graph::BufferUsages::STORAGE
                        | crate::render_graph::BufferUsages::TRANSFER_SOURCE,
                    crate::render_graph::BufferMemoryPolicy::CpuVisible,
                ))
                .unwrap()
        })
        .collect::<Vec<_>>();
    for cycle in 0..8u32 {
        let extent = 16 + cycle * 2;
        renderer.resize(extent, extent).unwrap();
        let mut graph = FrameGraphBuilder::new()
            .create_resource(GraphResourceDesc {
                name: "slot color".into(),
                resource_type: GraphResourceType::ColorAttachment { clear_value: None },
                format: ImageFormat::R8G8B8A8Unorm,
                width: extent,
                height: extent,
                tracks_swapchain_size: true,
            })
            .export_resource("slot color")
            .add_pass(
                SimplePass::new("slot clear", PassType::Graphics)
                    .without_depth()
                    .write("slot color")
                    .attachment(
                        "slot color",
                        AttachmentOps::clear(ClearValue::color(1.0, 0.0, 0.0, 1.0)),
                    )
                    .with_kind(PassKind::Geometry),
            )
            .build::<MetalRenderer>()
            .unwrap();
        graph.initialize_transient_textures(&renderer).unwrap();
        let mut probes = Vec::new();
        let mut addresses = Vec::new();
        for expected_slot in 0..3usize {
            let drawable = TextureDescriptor::new(extent, extent, ImageFormat::B8G8R8A8Srgb)
                .with_usage(TextureUsage::COLOR_ATTACHMENT);
            let (_, drawable) = renderer.context.create_texture_shared(&drawable).unwrap();
            renderer.set_headless_drawable(drawable.inner);
            let FrameAcquisition::Ready(frame) = renderer.acquire_frame().unwrap() else {
                panic!("headless frame")
            };
            assert_eq!(frame.slot(), expected_slot);
            let marker = (cycle * 3 + expected_slot as u32 + 1) as f32;
            let marker_bytes = [marker, marker + 1., marker + 2., 1.];
            renderer
                .write_buffer(
                    &frame,
                    buffers[frame.slot()],
                    0,
                    bytemuck::cast_slice(&marker_bytes),
                )
                .unwrap();
            let vertex = VertexUI::new(
                [marker, marker + 1.0],
                [0.0, 1.0],
                [expected_slot as u8, cycle as u8, 100, 255],
                0,
            );
            let ui = UIDrawList {
                vertices: vec![vertex],
                indices: vec![0],
                ..Default::default()
            };
            renderer.ui_renderers[frame.slot()]
                .upload_draw_list(&renderer.context, &ui)
                .unwrap();
            let stream_pixel = [expected_slot as u8 + 10, cycle as u8 + 20, 30, 255];
            renderer.update_texture_impl(stream, &stream_pixel).unwrap();
            renderer.render(&frame, &mut graph, |_| {}).unwrap();
            renderer.present(frame).unwrap();
            assert!(renderer.frame_slots[frame.slot()].submission.is_some());
            let uniform_buffer = &renderer
                .graph_buffers
                .get(buffers[frame.slot()])
                .unwrap()
                .buffer;
            let ui_buffer = renderer.ui_renderers[frame.slot()].vertex_buffer().unwrap();
            addresses.push((
                uniform_buffer.inner.gpuAddress(),
                ui_buffer.inner.gpuAddress(),
                graph
                    .transient_texture("slot color", frame.slot())
                    .unwrap()
                    .view
                    .inner
                    .clone(),
            ));
            let uniform_readback = renderer.context.create_buffer(16, true).unwrap();
            let ui_readback = renderer
                .context
                .create_buffer(std::mem::size_of::<VertexUI>() as u64, true)
                .unwrap();
            let color_readback = renderer.context.create_buffer(4, true).unwrap();
            let stream_readback = renderer.context.create_buffer(4, true).unwrap();
            let mut command = renderer.context.create_command_buffer();
            command.begin();
            let mut encoder = command.begin_blit_pass_with_label("frame slot probes");
            encoder.copy_buffer_to_buffer(uniform_buffer, 0, &uniform_readback, 0, 16);
            encoder.copy_buffer_to_buffer(
                ui_buffer,
                0,
                &ui_readback,
                0,
                std::mem::size_of::<VertexUI>() as u64,
            );
            encoder.copy_texture_pixel_to_buffer(
                graph
                    .transient_texture("slot color", frame.slot())
                    .unwrap()
                    .view
                    .image(),
                0,
                0,
                &color_readback,
            );
            encoder.copy_texture_pixel_to_buffer(
                &renderer.textures.get(stream).unwrap().texture,
                0,
                0,
                &stream_readback,
            );
            encoder.end_encoding();
            command.end();
            command.submit(&renderer.context);
            probes.push((
                frame.slot(),
                command,
                uniform_readback,
                ui_readback,
                color_readback,
                stream_readback,
                marker_bytes,
                vertex,
                stream_pixel,
            ));
        }
        for a in 0..3 {
            for b in a + 1..3 {
                assert_ne!(addresses[a].0, addresses[b].0);
                assert_ne!(addresses[a].1, addresses[b].1);
                assert_ne!(addresses[a].2, addresses[b].2);
            }
        }
        for (slot, command, uniform, ui, color, stream, camera, vertex, stream_pixel) in probes {
            command.wait_until_completed().unwrap();
            renderer.wait_for_slot(slot).unwrap();
            let actual = unsafe { std::ptr::read_unaligned(uniform.map().cast::<[f32; 4]>()) };
            assert_eq!(actual, camera);
            uniform.unmap();
            let actual = unsafe { std::ptr::read_unaligned(ui.map().cast::<VertexUI>()) };
            assert_eq!(actual, vertex);
            ui.unmap();
            assert_eq!(
                unsafe { std::ptr::read(color.map().cast::<[u8; 4]>()) },
                [255, 0, 0, 255]
            );
            color.unmap();
            assert_eq!(
                unsafe { std::ptr::read(stream.map().cast::<[u8; 4]>()) },
                stream_pixel
            );
            stream.unmap();
        }
    }
}
