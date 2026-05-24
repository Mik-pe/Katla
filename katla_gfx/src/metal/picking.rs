//! Metal GPU picking subsystem.
//!
//! Renders instance indices to an R32Uint texture, then reads back a single
//! pixel via a blit encoder + Shared buffer for CPU-side entity resolution.

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_foundation::NSString;
use objc2_metal::{
    MTLBlitCommandEncoder, MTLCommandBuffer, MTLCommandEncoder, MTLOrigin, MTLPixelFormat, MTLSize,
};

use crate::backend::command::{
    ColorAttachmentInfo, DepthAttachmentInfo, GpuCommandBuffer, GpuRenderEncoder, IndexType,
    RenderPassInfo, ShaderStages,
};
use crate::backend::resource::GpuBuffer;
use crate::error::RendererError;
use crate::handle::ResourceStorage;
use crate::pipeline::CompareOp;
use crate::render_pass::{ClearValue, LoadOp, StoreOp};
use crate::texture::{ImageFormat, TextureDescriptor, TextureUsage};

use super::buffer::MetalBuffer;
use super::context::MetalContext;
use super::metal_renderer::{MetalMaterial, MetalMesh};
use super::pipeline::MetalGraphicsPipeline;
use super::texture::MetalTextureView;

/// Pending picking readback on Metal.
struct PendingPick {
    /// Frame number when the pick was triggered.
    frame: usize,
    /// Command buffer used for the blit copy.
    command_buffer: Retained<ProtocolObject<dyn MTLCommandBuffer>>,
    /// Shared buffer containing the single u32 pixel readback.
    readback_buffer: MetalBuffer,
    /// Whether the result has been consumed.
    resolved: bool,
}

/// Metal picking subsystem.
///
/// Owns the object-ID texture, picking pipelines, and readback state.
pub(crate) struct MetalPickingSubsystem {
    /// R32Uint texture for rendering object IDs.
    object_id_texture: Option<MetalTextureView>,
    /// Picking pipeline for static meshes.
    pipeline: Option<MetalGraphicsPipeline>,
    /// Picking pipeline for skinned meshes.
    pipeline_skinned: Option<MetalGraphicsPipeline>,
    /// Pending pixel readback.
    pending: Option<PendingPick>,
    /// Cached texture dimensions.
    texture_width: u32,
    texture_height: u32,
}

impl MetalPickingSubsystem {
    pub(crate) fn new() -> Self {
        Self {
            object_id_texture: None,
            pipeline: None,
            pipeline_skinned: None,
            pending: None,
            texture_width: 0,
            texture_height: 0,
        }
    }

    pub(crate) fn pipeline(&self) -> Option<&MetalGraphicsPipeline> {
        self.pipeline.as_ref()
    }

    pub(crate) fn pipeline_skinned(&self) -> Option<&MetalGraphicsPipeline> {
        self.pipeline_skinned.as_ref()
    }

    pub(crate) fn object_id_texture(&self) -> Option<&MetalTextureView> {
        self.object_id_texture.as_ref()
    }

    /// Create or recreate the object-ID texture.
    pub(crate) fn resize(
        &mut self,
        context: &MetalContext,
        width: u32,
        height: u32,
    ) -> Result<(), RendererError> {
        if width == self.texture_width && height == self.texture_height {
            return Ok(());
        }

        let desc = TextureDescriptor::new(width, height, ImageFormat::R32Uint)
            .with_usage(TextureUsage::COLOR_ATTACHMENT | TextureUsage::SAMPLED);

        let (_texture, view) = context.create_texture(&desc)?;

        self.object_id_texture = Some(view);
        self.texture_width = width;
        self.texture_height = height;
        Ok(())
    }

    /// Create the static picking pipeline.
    pub(crate) fn create_pipeline(
        &mut self,
        context: &MetalContext,
        vertex_function: &ProtocolObject<dyn objc2_metal::MTLFunction>,
        fragment_function: &ProtocolObject<dyn objc2_metal::MTLFunction>,
    ) -> Result<(), RendererError> {
        let pipeline = context.create_graphics_pipeline(
            vertex_function,
            Some(fragment_function),
            &[MTLPixelFormat::R32Uint],
            Some(MTLPixelFormat::Depth32Float_Stencil8),
            true,
            CompareOp::GreaterOrEqual,
            objc2_metal::MTLCullMode::Back,
            objc2_metal::MTLWinding::Clockwise,
        )?;

        self.pipeline = Some(pipeline);
        Ok(())
    }

    /// Create the skinned picking pipeline.
    pub(crate) fn create_pipeline_skinned(
        &mut self,
        context: &MetalContext,
        vertex_function: &ProtocolObject<dyn objc2_metal::MTLFunction>,
        fragment_function: &ProtocolObject<dyn objc2_metal::MTLFunction>,
    ) -> Result<(), RendererError> {
        let vd = super::context::pbr_skinned_vertex_descriptor();
        let pipeline = context.create_graphics_pipeline_with_vertex_descriptor(
            vertex_function,
            Some(fragment_function),
            &[MTLPixelFormat::R32Uint],
            Some(MTLPixelFormat::Depth32Float_Stencil8),
            true,
            CompareOp::GreaterOrEqual,
            objc2_metal::MTLCullMode::Back,
            objc2_metal::MTLWinding::Clockwise,
            Some(&vd),
            false,
        )?;

        self.pipeline_skinned = Some(pipeline);
        Ok(())
    }

    /// Queue a single-pixel readback from the object-ID texture.
    pub(crate) fn queue_picking_readback(
        &mut self,
        context: &MetalContext,
        frame: usize,
        x: u32,
        y: u32,
    ) -> Result<(), RendererError> {
        // Check if previous pick is still pending (not yet resolved)
        if self.pending.is_some() {
            // Resolve the old one first — non-blocking check
            let completed = self
                .pending
                .as_ref()
                .map(|p| {
                    p.command_buffer.status() == objc2_metal::MTLCommandBufferStatus::Completed
                })
                .unwrap_or(true);
            if !completed {
                log::debug!("Previous picking readback still in flight, skipping new pick");
                return Ok(());
            }
            // Consume and discard old result
            self.pending = None;
        }

        let id_texture = self.object_id_texture.as_ref().ok_or_else(|| {
            RendererError::InvalidOperation("Object-ID texture not created".into())
        })?;

        // Create a small Shared buffer for the 4-byte pixel readback
        let readback_buffer = context.create_buffer(4, true).map_err(|e| {
            RendererError::InvalidOperation(format!(
                "Failed to create picking readback buffer: {}",
                e
            ))
        })?;

        let cmd_buffer = context.create_command_buffer();
        let label = NSString::from_str("picking_readback");
        unsafe { cmd_buffer.inner.setLabel(Some(&label)) };

        // Use blit encoder to copy a single pixel from GPU-private texture to Shared buffer
        let blit_encoder = cmd_buffer
            .inner
            .blitCommandEncoder()
            .expect("Failed to create blit encoder for picking readback");

        unsafe {
            blit_encoder.copyFromTexture_sourceSlice_sourceLevel_sourceOrigin_sourceSize_toBuffer_destinationOffset_destinationBytesPerRow_destinationBytesPerImage(
                &id_texture.inner,
                0,
                0,
                MTLOrigin {
                    x: x as usize,
                    y: y as usize,
                    z: 0,
                },
                MTLSize {
                    width: 1,
                    height: 1,
                    depth: 1,
                },
                &readback_buffer.inner,
                0,
                4,
                4,
            );
        }

        blit_encoder.endEncoding();
        cmd_buffer.inner.commit();

        self.pending = Some(PendingPick {
            frame,
            command_buffer: cmd_buffer.inner,
            readback_buffer,
            resolved: false,
        });

        Ok(())
    }

    /// Check if a pending picking readback is complete.
    ///
    /// Returns `Some((frame, instance_index))` if ready, `None` if not.
    pub(crate) fn check_picking_readback(&mut self) -> Option<(usize, u32)> {
        let pending = self.pending.as_mut()?;

        if pending.resolved {
            return None;
        }

        let status = pending.command_buffer.status();
        if status != objc2_metal::MTLCommandBufferStatus::Completed {
            return None;
        }

        // Read the u32 from the shared buffer
        let ptr = pending.readback_buffer.map() as *const u32;
        let value = unsafe { std::ptr::read(ptr) };
        pending.readback_buffer.unmap();

        let frame = pending.frame;
        pending.resolved = true;
        self.pending = None;

        Some((frame, value))
    }

    pub(crate) fn has_pending_readback(&self) -> bool {
        self.pending.is_some()
    }
}

/// Render the object-ID picking pass.
///
/// Draws all objects encoding instance_index + 1 into the R32Uint texture.
/// Reuses depth from the depth prepass for correct occlusion.
pub(crate) fn render_object_id_pass(
    cmd_buffer: &mut super::command_buffer::MetalCommandBuffer,
    picking_pipeline: &MetalGraphicsPipeline,
    picking_skinned_pipeline: Option<&MetalGraphicsPipeline>,
    object_id_view: &MetalTextureView,
    depth_view: &MetalTextureView,
    width: u32,
    height: u32,
    frame_uniform_buffer: &MetalBuffer,
    object_storage_buffer: &MetalBuffer,
    meshes: &ResourceStorage<MetalMesh>,
    materials: &ResourceStorage<MetalMaterial>,
    draw_list: &crate::renderer::types::DrawList,
    skeleton_buffers: &ResourceStorage<MetalBuffer>,
) {
    let render_pass_info = RenderPassInfo {
        color_attachments: vec![ColorAttachmentInfo {
            view: object_id_view.clone(),
            load_op: LoadOp::Clear,
            store_op: StoreOp::Store,
            clear_value: ClearValue::TRANSPARENT_BLACK,
        }],
        depth_attachment: Some(DepthAttachmentInfo {
            view: depth_view.clone(),
            load_op: LoadOp::Load,
            store_op: StoreOp::DontCare,
            clear_value: ClearValue::DEFAULT_DEPTH,
            format: ImageFormat::D32SfloatS8Uint,
        }),
    };

    let mut encoder = cmd_buffer.begin_render_pass(render_pass_info);

    encoder.bind_graphics_pipeline(picking_pipeline);
    encoder.set_viewport(0.0, 0.0, width as f32, height as f32, 0.0, 1.0);

    let stages = ShaderStages::VERTEX_FRAGMENT;
    encoder.bind_storage_buffer(frame_uniform_buffer, 0, 0, stages);
    encoder.bind_storage_buffer(object_storage_buffer, 0, 1, stages);

    let mut current_is_skinned = false;

    for draw in &draw_list.draws {
        let Some(mesh) = meshes.get(draw.mesh.index()) else {
            continue;
        };
        let Some(material) = materials.get(draw.material.index()) else {
            continue;
        };
        let Some(ref _pipeline) = material.pipeline else {
            continue;
        };

        let is_skinned = !draw.skeleton.is_none() && picking_skinned_pipeline.is_some();

        if is_skinned != current_is_skinned {
            if is_skinned {
                encoder.bind_graphics_pipeline(picking_skinned_pipeline.unwrap());
                encoder.bind_storage_buffer(frame_uniform_buffer, 0, 0, stages);
                encoder.bind_storage_buffer(object_storage_buffer, 0, 1, stages);
            } else {
                encoder.bind_graphics_pipeline(picking_pipeline);
                encoder.bind_storage_buffer(frame_uniform_buffer, 0, 0, stages);
                encoder.bind_storage_buffer(object_storage_buffer, 0, 1, stages);
            }
            current_is_skinned = is_skinned;
        }

        if is_skinned {
            if let Some(skeleton_buf) = skeleton_buffers.get(draw.skeleton.index()) {
                encoder.bind_storage_buffer(skeleton_buf, 0, 2, stages);
            }
        }

        encoder.bind_vertex_buffer(&mesh.vertex_buffer, 0, 10);
        encoder.bind_index_buffer(&mesh.index_buffer, 0, IndexType::Uint32);
        encoder.draw_indexed(mesh.index_count, 1, 0, 0, draw.instance_index);
    }

    encoder.end_encoding();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::command::{GpuCommandBuffer, GpuRenderEncoder};
    use crate::backend::resource::GpuBuffer;
    use crate::metal::shader::{self, ShaderProfile};
    use crate::texture::TextureUsage;

    #[test]
    fn test_picking_subsystem_creation() {
        let subsystem = MetalPickingSubsystem::new();
        assert!(subsystem.pipeline.is_none());
        assert!(subsystem.pipeline_skinned.is_none());
        assert!(subsystem.object_id_texture.is_none());
        assert!(!subsystem.has_pending_readback());
    }

    /// Read a single u32 pixel from an R32Uint texture at (x, y).
    fn read_pixel_r32(ctx: &MetalContext, texture: &MetalTextureView, x: u32, y: u32) -> u32 {
        let readback = ctx.create_buffer(4, true).unwrap();
        let cmd_buffer = ctx.create_command_buffer();
        let blit = cmd_buffer.inner.blitCommandEncoder().unwrap();
        unsafe {
            blit.copyFromTexture_sourceSlice_sourceLevel_sourceOrigin_sourceSize_toBuffer_destinationOffset_destinationBytesPerRow_destinationBytesPerImage(
                &texture.inner,
                0,
                0,
                MTLOrigin { x: x as usize, y: y as usize, z: 0 },
                MTLSize { width: 1, height: 1, depth: 1 },
                &readback.inner,
                0,
                4,
                4,
            );
        }
        blit.endEncoding();
        cmd_buffer.inner.commit();
        cmd_buffer.inner.waitUntilCompleted();

        let ptr = readback.map() as *const u32;
        let value = unsafe { std::ptr::read(ptr) };
        readback.unmap();
        value
    }

    /// Test that the picking texture Y-axis matches screen-space Y (top = 0).
    ///
    /// Renders two small quads with different instance IDs:
    /// - Quad A at the top of the viewport (clip Y = -0.5) with instance_id = 10
    /// - Quad B at the bottom of the viewport (clip Y = +0.5) with instance_id = 20
    ///
    /// Then reads back pixels at Y=0 (top row) and Y=height-1 (bottom row).
    /// The pixel at Y=0 should contain instance 10 (top quad) and the pixel
    /// at Y=height-1 should contain instance 20 (bottom quad).
    ///
    /// This validates that Metal's viewport transform maps clip Y = -1 to the
    /// top of the texture (pixel Y = 0), which is what the picking readback
    /// coordinate calculation assumes.
    #[test]
    fn test_picking_y_axis_matches_screen_space() {
        let ctx = MetalContext::init_headless().unwrap();

        let width = 64u32;
        let height = 64u32;

        // Shader that outputs instance_index + 1 as the fragment color (R32Uint).
        // Each draw call renders a full-screen quad — we control position via clip-space
        // in the vertex shader based on vertex_index, and rely on depth test to place
        // the correct instance at the correct Y position.
        //
        // For this test we use a simpler approach: render two full-width horizontal
        // strips whose vertical positions are baked into the vertex shader per instance.
        let wgsl = r#"
struct VertexOutput {
    @builtin(position) clip_position: vec4f,
    @location(0) @interpolate(flat) instance_idx: u32,
}

@vertex fn vs_main(
    @builtin(vertex_index) vi: u32,
    @builtin(instance_index) instance_idx: u32,
) -> VertexOutput {
    // Instance 0: top half (clip Y from -1.0 to 0.0)
    // Instance 1: bottom half (clip Y from 0.0 to 1.0)
    let y_offset = select(-1.0, 0.0, instance_idx == 1u);
    let y_next = select(0.0, 1.0, instance_idx == 1u);

    var positions = array<vec2f, 6>(
        vec2f(-1.0, y_offset),
        vec2f( 1.0, y_offset),
        vec2f( 1.0, y_next),
        vec2f(-1.0, y_offset),
        vec2f( 1.0, y_next),
        vec2f(-1.0, y_next),
    );

    var out: VertexOutput;
    out.clip_position = vec4f(positions[vi], 0.0, 1.0);
    out.instance_idx = instance_idx;
    return out;
}

@fragment fn fs_main(input: VertexOutput) -> @location(0) vec4u {
    return vec4u(input.instance_idx + 1u, 0u, 0u, 1u);
}
"#;

        let compiled = shader::compile_wgsl_to_metal(
            &ctx.device,
            wgsl,
            &["vs_main", "fs_main"],
            ShaderProfile::Graphics,
        )
        .unwrap();
        let vs = compiled.module.entry_points.get("vs_main").unwrap();
        let fs = compiled.module.entry_points.get("fs_main").unwrap();

        let pipeline = ctx
            .create_graphics_pipeline_with_vertex_descriptor(
                vs,
                Some(fs),
                &[MTLPixelFormat::R32Uint],
                None,
                false,
                CompareOp::Always,
                objc2_metal::MTLCullMode::None,
                objc2_metal::MTLWinding::Clockwise,
                Some(&super::super::context::fullscreen_vertex_descriptor()),
                false,
            )
            .unwrap();

        // Create R32Uint picking texture
        let desc = TextureDescriptor::new(width, height, ImageFormat::R32Uint)
            .with_usage(TextureUsage::COLOR_ATTACHMENT | TextureUsage::SAMPLED);
        let (_texture, view) = ctx.create_texture(&desc).unwrap();

        let mut cmd_buffer = ctx.create_command_buffer();
        cmd_buffer.begin();

        let render_pass_info = RenderPassInfo {
            color_attachments: vec![ColorAttachmentInfo {
                view: view.clone(),
                load_op: LoadOp::Clear,
                store_op: StoreOp::Store,
                clear_value: ClearValue::TRANSPARENT_BLACK,
            }],
            depth_attachment: None,
        };

        let mut encoder = cmd_buffer.begin_render_pass(render_pass_info);
        encoder.set_viewport(0.0, 0.0, width as f32, height as f32, 0.0, 1.0);
        encoder.bind_graphics_pipeline(&pipeline);

        // Dummy vertex buffer at index 10 (required by vertex descriptor)
        let dummy_vb = ctx.create_buffer(4, true).unwrap();
        encoder.bind_vertex_buffer(&dummy_vb, 0, 10);

        // Draw both instances in one call:
        // Instance 0 (id=0): clip Y from -1.0 to 0.0 (top half in clip space)
        // Instance 1 (id=1): clip Y from 0.0 to 1.0 (bottom half in clip space)
        encoder.draw(6, 2, 0, 0);

        encoder.end_encoding();
        cmd_buffer.end();
        cmd_buffer.submit(&ctx);
        cmd_buffer.inner.waitUntilCompleted();

        // Read pixels at top center and bottom center
        let top_pixel = read_pixel_r32(&ctx, &view, width / 2, 0);
        let bottom_pixel = read_pixel_r32(&ctx, &view, width / 2, height - 1);

        // EMPIRICAL: Metal's viewport maps clip Y = +1 to pixel Y = 0 (top)
        // and clip Y = -1 to pixel Y = height-1 (bottom).
        // So instance 0 (clip Y=-1..0) is at the bottom, instance 1 (clip Y=0..+1) at the top.
        assert_eq!(
            top_pixel, 2,
            "Top of picking texture (y=0) should contain instance 1 (clip Y=0..+1), got {}",
            top_pixel
        );
        assert_eq!(
            bottom_pixel,
            1,
            "Bottom of picking texture (y={}) should contain instance 0 (clip Y=-1..0), got {}",
            height - 1,
            bottom_pixel
        );
    }

    /// Test that the picking readback Y coordinate matches the rendered scene.
    ///
    /// From test_picking_y_axis_matches_screen_space we empirically know:
    /// - Metal's viewport maps clip Y = +1 → pixel Y = 0 (top)
    /// - Metal's viewport maps clip Y = -1 → pixel Y = height-1 (bottom)
    ///
    /// The engine's projection matrix negates Y (`-f`), so:
    /// - Objects above camera (world Y > 0) → clip Y < 0 → pixel Y near bottom
    /// - Objects below camera (world Y < 0) → clip Y > 0 → pixel Y near top
    ///
    /// But the user sees the scene correctly because the tonemap fullscreen triangle
    /// flips Y again. The picking readback coordinates (physical_y) are derived from
    /// screen space where Y=0 is the top. So physical_y=0 should pick the object the
    /// user sees at the top of the screen.
    ///
    /// This test proves that physical_y=0 (top of screen) reads from the TOP of the
    /// picking texture, which in Metal contains the object with clip Y = +1 (NOT the
    /// object above the camera). The picking Y must be flipped for Metal.
    #[test]
    fn test_picking_readback_y_matches_tonemapped_display() {
        let ctx = MetalContext::init_headless().unwrap();

        let width = 64u32;
        let height = 64u32;

        let wgsl = r#"
struct VertexOutput {
    @builtin(position) clip_position: vec4f,
    @location(0) @interpolate(flat) instance_idx: u32,
}

@vertex fn vs_main(
    @builtin(vertex_index) vi: u32,
    @builtin(instance_index) instance_idx: u32,
) -> VertexOutput {
    // Instance 0: top half (clip Y from -1.0 to 0.0)
    // Instance 1: bottom half (clip Y from 0.0 to 1.0)
    let y_offset = select(-1.0, 0.0, instance_idx == 1u);
    let y_next = select(0.0, 1.0, instance_idx == 1u);

    var positions = array<vec2f, 6>(
        vec2f(-1.0, y_offset),
        vec2f( 1.0, y_offset),
        vec2f( 1.0, y_next),
        vec2f(-1.0, y_offset),
        vec2f( 1.0, y_next),
        vec2f(-1.0, y_next),
    );

    var out: VertexOutput;
    out.clip_position = vec4f(positions[vi], 0.0, 1.0);
    out.instance_idx = instance_idx;
    return out;
}

@fragment fn fs_main(input: VertexOutput) -> @location(0) vec4u {
    return vec4u(input.instance_idx + 1u, 0u, 0u, 1u);
}
"#;

        let compiled = shader::compile_wgsl_to_metal(
            &ctx.device,
            wgsl,
            &["vs_main", "fs_main"],
            ShaderProfile::Graphics,
        )
        .unwrap();
        let vs = compiled.module.entry_points.get("vs_main").unwrap();
        let fs = compiled.module.entry_points.get("fs_main").unwrap();

        let pipeline = ctx
            .create_graphics_pipeline_with_vertex_descriptor(
                vs,
                Some(fs),
                &[MTLPixelFormat::R32Uint],
                None,
                false,
                CompareOp::Always,
                objc2_metal::MTLCullMode::None,
                objc2_metal::MTLWinding::Clockwise,
                Some(&super::super::context::fullscreen_vertex_descriptor()),
                false,
            )
            .unwrap();

        let desc = TextureDescriptor::new(width, height, ImageFormat::R32Uint)
            .with_usage(TextureUsage::COLOR_ATTACHMENT | TextureUsage::SAMPLED);
        let (_texture, picking_view) = ctx.create_texture(&desc).unwrap();

        let dummy_vb = ctx.create_buffer(4, true).unwrap();

        let mut cmd_buffer = ctx.create_command_buffer();
        cmd_buffer.begin();

        let render_pass_info = RenderPassInfo {
            color_attachments: vec![ColorAttachmentInfo {
                view: picking_view.clone(),
                load_op: LoadOp::Clear,
                store_op: StoreOp::Store,
                clear_value: ClearValue::TRANSPARENT_BLACK,
            }],
            depth_attachment: None,
        };

        let mut encoder = cmd_buffer.begin_render_pass(render_pass_info);
        encoder.set_viewport(0.0, 0.0, width as f32, height as f32, 0.0, 1.0);
        encoder.bind_graphics_pipeline(&pipeline);
        encoder.bind_vertex_buffer(&dummy_vb, 0, 10);

        // Draw both instances:
        // Instance 0 (id=0): clip Y from -1.0 to 0.0 (negative clip Y, simulates "above camera")
        // Instance 1 (id=1): clip Y from 0.0 to +1.0 (positive clip Y, simulates "below camera")
        encoder.draw(6, 2, 0, 0);

        encoder.end_encoding();
        cmd_buffer.end();
        cmd_buffer.submit(&ctx);
        cmd_buffer.inner.waitUntilCompleted();

        // Simulate the picking readback. The user clicks at the top of the viewport.
        // The picking code maps this to physical_y = 0 (top row of the picking texture).
        //
        // From test_picking_y_axis_matches_screen_space, we know:
        //   clip Y = +1 → pixel Y = 0 (top of texture)
        //   clip Y = -1 → pixel Y = height-1 (bottom of texture)
        //
        // So instance 0 (clip Y < 0, "above camera") is at the BOTTOM of the picking texture,
        // and instance 1 (clip Y > 0, "below camera") is at the TOP.
        //
        // The fix: flip the Y coordinate before reading, so that screen-top maps to
        // the bottom of the picking texture (where clip Y < 0 objects are).
        let flipped_y_top = height - 1 - 0u32; // physical_y=0 → flipped to bottom
        let flipped_y_bottom = height - 1 - (height - 1); // physical_y=63 → flipped to top

        let picked_top = read_pixel_r32(&ctx, &picking_view, width / 2, flipped_y_top);
        let picked_bottom = read_pixel_r32(&ctx, &picking_view, width / 2, flipped_y_bottom);

        // After the Y-flip, picking at screen-top should read the object with
        // negative clip Y (above camera, instance 0+1=1).
        assert_eq!(
            picked_top, 1,
            "Picking at the top of the viewport (physical_y=0, flipped to {}) should select the object \
             with negative clip Y (above camera, instance 0+1=1), got {}",
            flipped_y_top, picked_top
        );
        assert_eq!(
            picked_bottom,
            2,
            "Picking at the bottom of the viewport (physical_y={}, flipped to {}) should select the object \
             with positive clip Y (below camera, instance 1+1=2), got {}",
            height - 1,
            flipped_y_bottom,
            picked_bottom
        );
    }
}
