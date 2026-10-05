//! Frame-owned transient placement heaps and tile-local attachment storage.

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::{
    MTLDevice, MTLGPUFamily, MTLHazardTrackingMode, MTLHeap, MTLHeapDescriptor, MTLHeapType,
    MTLStorageMode, MTLTextureDescriptor, MTLTextureUsage,
};

use super::format::to_mtl_pixel_format;
use super::metal_transient_texture::{MetalTransientAllocation, MetalTransientTexture};
use super::texture::{MetalTexture, MetalTextureView};
use crate::render_graph::{
    GraphResourceDesc, GraphResourceType, RenderGraphError, TransientSlotPolicy,
};

fn descriptor(
    desc: &GraphResourceDesc,
    memoryless: bool,
    storage: bool,
) -> Retained<MTLTextureDescriptor> {
    let native = unsafe {
        MTLTextureDescriptor::texture2DDescriptorWithPixelFormat_width_height_mipmapped(
            to_mtl_pixel_format(desc.format),
            desc.width as usize,
            desc.height as usize,
            false,
        )
    };
    let usage = if memoryless {
        MTLTextureUsage::RenderTarget
    } else {
        match desc.resource_type {
            GraphResourceType::ColorAttachment { .. } => {
                MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead
            }
            GraphResourceType::DepthAttachment { sampled, .. } => {
                if sampled {
                    MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead
                } else {
                    MTLTextureUsage::RenderTarget
                }
            }
            GraphResourceType::SampledImage => MTLTextureUsage::ShaderRead,
        }
    };
    native.setUsage(if storage {
        usage | MTLTextureUsage::ShaderWrite
    } else {
        usage
    });
    native.setStorageMode(if memoryless {
        MTLStorageMode::Memoryless
    } else {
        MTLStorageMode::Private
    });
    native.setHazardTrackingMode(MTLHazardTrackingMode::Untracked);
    native
}

pub(crate) fn create_slot(
    device: &ProtocolObject<dyn MTLDevice>,
    members: &[GraphResourceDesc],
    policy: TransientSlotPolicy,
) -> Result<Vec<MetalTransientTexture>, RenderGraphError> {
    let memoryless = policy.optimize
        && policy.memoryless
        && !policy.storage
        && !policy.transfer_destination
        && device.supportsFamily(MTLGPUFamily::Apple7);
    let descriptors = members
        .iter()
        .map(|desc| descriptor(desc, memoryless, policy.storage))
        .collect::<Vec<_>>();
    let sizes = descriptors
        .iter()
        .map(|desc| {
            if memoryless {
                desc.setStorageMode(MTLStorageMode::Private);
                let bytes = device.heapTextureSizeAndAlignWithDescriptor(desc).size;
                desc.setStorageMode(MTLStorageMode::Memoryless);
                bytes
            } else {
                device.heapTextureSizeAndAlignWithDescriptor(desc).size
            }
        })
        .collect::<Vec<_>>();
    let bytes = sizes.iter().copied().max().unwrap_or(0);
    let heap = if policy.optimize && !memoryless {
        let desc = MTLHeapDescriptor::new();
        desc.setType(MTLHeapType::Placement);
        desc.setStorageMode(MTLStorageMode::Private);
        desc.setHazardTrackingMode(MTLHazardTrackingMode::Untracked);
        desc.setSize(bytes);
        Some(device.newHeapWithDescriptor(&desc).ok_or_else(|| {
            RenderGraphError::BackendError(
                "Metal transient placement heap allocation failed".into(),
            )
        })?)
    } else {
        None
    };
    members
        .iter()
        .zip(descriptors)
        .zip(sizes)
        .map(|((desc, native), size)| {
            // Placement heaps permit simultaneous texture objects at an identical
            // range; the compiled execution intervals govern when each is used.
            let texture = if let Some(heap) = &heap {
                unsafe { heap.newTextureWithDescriptor_offset(&native, 0) }
            } else {
                device.newTextureWithDescriptor(&native)
            }
            .ok_or_else(|| {
                RenderGraphError::BackendError(format!(
                    "Metal transient '{}' allocation failed",
                    desc.name
                ))
            })?;
            let allocation = MetalTransientAllocation {
                frame_slot: policy.frame_slot,
                slot: policy.allocation_slot,
                offset: 0,
                bytes: if memoryless { 0 } else { size as u64 },
                logical_bytes: size as u64,
                memoryless,
                aliased: heap.is_some() && members.len() > 1,
                heap: heap.clone(),
            };
            let image = MetalTexture::new(texture.clone(), desc.format);
            let view = MetalTextureView::new(texture, image.clone());
            Ok(MetalTransientTexture::new(
                image,
                view,
                desc.format,
                desc.width,
                desc.height,
                allocation,
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::command::{
        ColorAttachmentInfo, GpuBlitEncoder, GpuCommandBuffer, GpuRenderEncoder, RenderPassInfo,
    };
    use crate::backend::resource::GpuBuffer;
    use crate::render_pass::{ClearValue, LoadOp, StoreOp};
    use crate::texture::ImageFormat;
    use objc2_metal::{MTL4CommandEncoder, MTL4VisibilityOptions, MTLResource, MTLStages};

    fn desc(name: &str, width: u32) -> GraphResourceDesc {
        GraphResourceDesc {
            name: name.into(),
            resource_type: GraphResourceType::ColorAttachment { clear_value: None },
            format: ImageFormat::R8G8B8A8Unorm,
            width,
            height: 16,
            tracks_swapchain_size: false,
        }
    }
    fn policy(frame_slot: usize) -> TransientSlotPolicy {
        TransientSlotPolicy {
            frame_slot,
            allocation_slot: 0,
            optimize: true,
            memoryless: false,
            storage: false,
            transfer_destination: false,
        }
    }

    fn encode_alias_probe(
        context: &super::super::context::MetalContext,
        textures: &[MetalTransientTexture],
    ) -> (
        super::super::command_buffer::MetalCommandBuffer,
        super::super::buffer::MetalBuffer,
    ) {
        let mut command = context.create_command_buffer();
        command.begin();
        for (texture, color) in textures
            .iter()
            .zip([[1.0, 0.0, 0.0, 1.0], [0.0, 1.0, 0.0, 1.0]])
        {
            let render = command.begin_render_pass(RenderPassInfo::unlabeled(
                vec![ColorAttachmentInfo {
                    view: texture.view.clone(),
                    load_op: LoadOp::Clear,
                    store_op: StoreOp::Store,
                    clear_value: ClearValue::Color(color),
                }],
                None,
            ));
            render
                .inner
                .barrierAfterQueueStages_beforeStages_visibilityOptions(
                    MTLStages::All,
                    MTLStages::All,
                    MTL4VisibilityOptions::Device | MTL4VisibilityOptions::ResourceAlias,
                );
            render.end_encoding();
        }
        let buffer = context.create_buffer(4, true).unwrap();
        let mut blit = command.begin_blit_pass();
        blit.inner
            .barrierAfterQueueStages_beforeStages_visibilityOptions(
                MTLStages::All,
                MTLStages::Blit,
                MTL4VisibilityOptions::Device | MTL4VisibilityOptions::ResourceAlias,
            );
        blit.copy_texture_pixel_to_buffer(&textures[0].texture, 8, 8, &buffer);
        blit.end_encoding();
        command.end();
        command.submit(context);
        (command, buffer)
    }

    #[test]
    fn test_native_metal4_placement_aliases_ranges_and_stresses_three_frame_slots() {
        let context = super::super::context::MetalContext::init_headless().unwrap();
        for rebuild in 0..8 {
            let members = [
                desc("earlier", 16 + rebuild * 4),
                desc("later", 16 + rebuild * 4),
            ];
            let frames = (0..3)
                .map(|slot| create_slot(&context.device, &members, policy(slot)).unwrap())
                .collect::<Vec<_>>();
            for frame in &frames {
                assert_eq!(frame[0].texture.inner.heap(), frame[1].texture.inner.heap());
                assert_eq!(
                    frame[0].texture.inner.heapOffset(),
                    frame[1].texture.inner.heapOffset()
                );
                assert!(frame[0].allocation.aliased);
            }
            assert_ne!(
                frames[0][0].texture.inner.heap(),
                frames[1][0].texture.inner.heap()
            );
            assert_ne!(
                frames[1][0].texture.inner.heap(),
                frames[2][0].texture.inner.heap()
            );
            let submissions = frames
                .iter()
                .map(|textures| encode_alias_probe(&context, textures))
                .collect::<Vec<_>>();
            for (slot, (command, buffer)) in submissions.iter().enumerate() {
                command.wait_until_completed().unwrap();
                let pixel = unsafe { std::ptr::read(buffer.map().cast::<[u8; 4]>()) };
                buffer.unmap();
                assert_eq!(pixel, [0, 255, 0, 255], "rebuild {rebuild}, frame {slot}");
                assert_eq!(frames[slot][0].allocation.frame_slot, slot);
            }
        }
    }

    #[test]
    fn test_debug_switch_preserves_independent_native_contents() {
        let context = super::super::context::MetalContext::init_headless().unwrap();
        let mut standalone = policy(0);
        standalone.optimize = false;
        let textures =
            create_slot(&context.device, &[desc("a", 16), desc("b", 16)], standalone).unwrap();
        assert!(
            textures.iter().all(
                |texture| texture.texture.inner.heap().is_none() && !texture.allocation.aliased
            )
        );
        let (command, buffer) = encode_alias_probe(&context, &textures);
        command.wait_until_completed().unwrap();
        let pixel = unsafe { std::ptr::read(buffer.map().cast::<[u8; 4]>()) };
        buffer.unmap();
        assert_eq!(pixel, [255, 0, 0, 255]);
    }

    #[test]
    fn test_memoryless_native_attachment_executes_discard_store() {
        let context = super::super::context::MetalContext::init_headless().unwrap();
        if !context.device.supportsFamily(MTLGPUFamily::Apple7) {
            return;
        }
        let mut tile = policy(0);
        tile.memoryless = true;
        let textures = create_slot(&context.device, &[desc("tile", 16)], tile).unwrap();
        assert_eq!(
            textures[0].texture.inner.storageMode(),
            MTLStorageMode::Memoryless
        );
        assert_eq!(textures[0].allocation.bytes, 0);
        let mut command = context.create_command_buffer();
        command.begin();
        let render = command.begin_render_pass(RenderPassInfo::unlabeled(
            vec![ColorAttachmentInfo {
                view: textures[0].view.clone(),
                load_op: LoadOp::Clear,
                store_op: StoreOp::DontCare,
                clear_value: ClearValue::Color([0.0, 1.0, 0.0, 1.0]),
            }],
            None,
        ));
        render.end_encoding();
        command.end();
        command.submit(&context);
        command.wait_until_completed().unwrap();
    }
}
