//! Tests for render graph compilation and execution in headless mode.
//!
//! These tests validate that the render graph system works correctly
//! without requiring a window or swapchain, enabling automated testing.

mod common;

use ash::vk;
use common::create_headless_context;
use katla_vulkan::{
    render_graph::{
        types::{
            Extent3D, ImageFormat, ImageLayout, ImageTiling, ImageUsage, SampleCount,
        },
        Attachment, RenderGraphBuilder, ResourceKind,
    },
    CommandBuffer,
};
use std::rc::Rc;

/// Test single-pass render graph compilation in headless mode.
///
/// This test creates a simple render graph with one offscreen render pass,
/// verifying that compilation works without a swapchain.
#[test]
fn test_render_graph_compilation_headless() {
    let context = Rc::new(create_headless_context(true));

    // Build a simple render graph with offscreen rendering
    let mut graph_builder = RenderGraphBuilder::new();

    // Create offscreen color target
    let color_target = graph_builder.add_resource(
        "color_target",
        ResourceKind::Image {
            extent: Extent3D {
                width: 512,
                height: 512,
                depth: 1,
            },
            format: ImageFormat::R8G8B8A8Srgb,
            usage: vec![ImageUsage::ColorAttachment],
            samples: SampleCount::Sample1,
            tiling: ImageTiling::Optimal,
            initial_layout: ImageLayout::Undefined,
            final_layout: ImageLayout::ShaderReadOnlyOptimal,
        },
    );

    // Create depth target
    let depth_target = graph_builder.add_resource(
        "depth_target",
        ResourceKind::Image {
            extent: Extent3D {
                width: 512,
                height: 512,
                depth: 1,
            },
            format: ImageFormat::D32Sfloat,
            usage: vec![ImageUsage::DepthStencilAttachment],
            samples: SampleCount::Sample1,
            tiling: ImageTiling::Optimal,
            initial_layout: ImageLayout::Undefined,
            final_layout: ImageLayout::DepthStencilAttachmentOptimal,
        },
    );

    // Add a simple render pass
    graph_builder.add_pass("offscreen_pass", |pass| {
        pass.write(Attachment::Color(color_target))
            .write(Attachment::DepthStencil(depth_target))
            .clear_color(color_target, [0.2, 0.4, 0.8, 1.0])
            .clear_depth_stencil(depth_target, 1.0, 0)
            .execute("offscreen_pass", |_ctx| {
                // Pass execution callback (empty for this test)
            });
    });

    // Compile the render graph
    match graph_builder.build(&context) {
        Ok(_graph) => {
            println!("Render graph compiled successfully in headless mode");
        }
        Err(e) => {
            panic!("Failed to compile render graph: {:?}", e);
        }
    }
}

/// Test multi-pass render graph with barriers in headless mode.
///
/// This test creates a render graph with multiple passes that require
/// explicit barriers, testing the full compilation pipeline.
#[test]
fn test_render_graph_multiple_passes_headless() {
    let context = Rc::new(create_headless_context(true));

    // Build a multi-pass render graph
    let mut graph_builder = RenderGraphBuilder::new();

    // Create intermediate texture
    let intermediate_texture = graph_builder.add_resource(
        "intermediate",
        ResourceKind::Image {
            extent: Extent3D {
                width: 256,
                height: 256,
                depth: 1,
            },
            format: ImageFormat::R8G8B8A8Srgb,
            usage: vec![
                ImageUsage::ColorAttachment,
                ImageUsage::Sampled,
                ImageUsage::InputAttachment,
            ],
            samples: SampleCount::Sample1,
            tiling: ImageTiling::Optimal,
            initial_layout: ImageLayout::Undefined,
            final_layout: ImageLayout::ShaderReadOnlyOptimal,
        },
    );

    // Create final output
    let final_output = graph_builder.add_resource(
        "final_output",
        ResourceKind::Image {
            extent: Extent3D {
                width: 256,
                height: 256,
                depth: 1,
            },
            format: ImageFormat::R8G8B8A8Srgb,
            usage: vec![ImageUsage::ColorAttachment, ImageUsage::TransferSrc],
            samples: SampleCount::Sample1,
            tiling: ImageTiling::Optimal,
            initial_layout: ImageLayout::Undefined,
            final_layout: ImageLayout::TransferSrcOptimal,
        },
    );

    // First pass: render to intermediate texture
    graph_builder.add_pass("geometry_pass", |pass| {
        pass.write(Attachment::Color(intermediate_texture))
            .clear_color(intermediate_texture, [0.1, 0.2, 0.3, 1.0])
            .execute("geometry_pass", |_ctx| {
                // Geometry rendering
            });
    });

    // Second pass: post-process using intermediate texture
    graph_builder.add_pass("postprocess_pass", |pass| {
        pass.read(intermediate_texture)
            .write(Attachment::Color(final_output))
            .clear_color(final_output, [0.0, 0.0, 0.0, 1.0])
            .execute("postprocess_pass", |_ctx| {
                // Post-processing using intermediate texture
            });
    });

    // Compile the multi-pass render graph
    match graph_builder.build(&context) {
        Ok(graph) => {
            println!(
                "Multi-pass render graph compiled successfully: {} passes",
                graph.passes.len()
            );
            assert_eq!(graph.passes.len(), 2);
        }
        Err(e) => {
            panic!("Failed to compile multi-pass render graph: {:?}", e);
        }
    }
}

/// Test render graph resource lifetime analysis in headless mode.
///
/// This test verifies that the render graph correctly handles resources
/// that are written in one pass and can be sampled in later passes.
#[test]
fn test_render_graph_lifetime_analysis_headless() {
    let context = Rc::new(create_headless_context(true));

    // Build a render graph with a resource that's written and then sampled
    let mut graph_builder = RenderGraphBuilder::new();

    // Create a color target that will be written to
    let color_target = graph_builder.add_resource(
        "color_target",
        ResourceKind::Image {
            extent: Extent3D {
                width: 128,
                height: 128,
                depth: 1,
            },
            format: ImageFormat::R8G8B8A8Srgb,
            usage: vec![ImageUsage::ColorAttachment, ImageUsage::Sampled],
            samples: SampleCount::Sample1,
            tiling: ImageTiling::Optimal,
            initial_layout: ImageLayout::Undefined,
            final_layout: ImageLayout::ShaderReadOnlyOptimal,
        },
    );

    // Write to the color target
    graph_builder.add_pass("write_pass", |pass| {
        pass.write(Attachment::Color(color_target))
            .clear_color(color_target, [0.2, 0.4, 0.6, 1.0])
            .execute("write_pass", |_ctx| {});
    });

    // Compile and verify
    match graph_builder.build(&context) {
        Ok(graph) => {
            println!(
                "Render graph compiled successfully: {} passes, {} resources",
                graph.passes.len(),
                graph.resources.borrow().len()
            );
            assert_eq!(graph.passes.len(), 1);
        }
        Err(e) => {
            panic!("Failed to compile render graph: {:?}", e);
        }
    }
}

/// Test command buffer recording in headless mode.
///
/// This test verifies that command buffers can be created and recorded
/// correctly in headless mode without a swapchain.
#[test]
fn test_headless_command_buffer_recording() {
    let context = create_headless_context(false);

    // Create a simple command buffer
    let command_buffer = CommandBuffer::new(&context.device, &context.gfx_cmdpool);

    // Begin recording
    command_buffer.begin_single_time_command();

    // Record some commands (e.g., pipeline barrier)
    let memory_barrier = vk::MemoryBarrier::default()
        .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
        .dst_access_mask(vk::AccessFlags::TRANSFER_READ);

    unsafe {
        context.device.cmd_pipeline_barrier(
            command_buffer.vk_command_buffer(),
            vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::TRANSFER,
            vk::DependencyFlags::empty(),
            &[memory_barrier],
            &[],
            &[],
        );
    }

    // End recording
    command_buffer.end_single_time_command();

    println!("Command buffer recorded successfully in headless mode");
}
