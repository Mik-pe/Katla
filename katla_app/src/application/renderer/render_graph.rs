//! Render graph setup for the application layer.
//!
//! This module defines the render graph passes that the application needs.
//! The application just says "draw stuff" and katla_vulkan handles the complexity.

use std::cell::RefCell;
use std::rc::Rc;

use katla_vulkan::{MaterialPipeline, UiDrawData, UIRenderer, VulkanRenderer};

/// Build the render graph with all application passes.
///
/// This function creates the render graph with sky, grid, geometry, UI, and present passes.
/// The API is simple - just tell each pass what to draw.
pub fn build_render_graph(
    renderer: &mut VulkanRenderer,
    sky_pipeline: Option<Rc<RefCell<MaterialPipeline>>>,
    grid_pipeline: Option<Rc<RefCell<MaterialPipeline>>>,
    ui_renderer: Option<&UIRenderer>,
    ui_draw_data: Rc<RefCell<Option<UiDrawData>>>,
) {
    // Get builder with resources pre-registered
    let (mut builder, resources) = renderer.create_render_graph_with_resources();

    // === SKY PASS ===
    // Draw a fullscreen sky using the sky material
    if let Some(sky_pipeline) = sky_pipeline {
        builder.add_pass("sky_pass", move |pass| {
            let sky_pipeline = sky_pipeline.clone();
            pass.write_color(&resources.viewport_color)
                .write_depth(&resources.viewport_depth)
                .clear_color_target(&resources.viewport_color, [0.4, 0.6, 0.9, 1.0])
                .clear_depth_target(&resources.viewport_depth, 1.0)
                .execute("sky_pass", move |ctx| {
                    ctx.draw_fullscreen_with_material(&sky_pipeline);
                });
        });
    }

    // === GRID PASS ===
    // Draw a fullscreen grid using the grid material
    if let Some(grid_pipeline) = grid_pipeline {
        builder.add_pass("grid_pass", move |pass| {
            let grid_pipeline = grid_pipeline.clone();
            pass.write_color(&resources.viewport_color)
                .write_depth(&resources.viewport_depth)
                .execute("grid_pass", move |ctx| {
                    ctx.draw_fullscreen_with_material(&grid_pipeline);
                });
        });
    }

    // === GEOMETRY PASS ===
    // Draw all meshes from the draw list
    builder.add_pass("geometry_pass", move |pass| {
        pass.write_color(&resources.viewport_color)
            .write_depth(&resources.viewport_depth)
            .execute("geometry_pass", move |ctx| {
                ctx.draw_draw_list();
            });
    });

    // === UI PASS ===
    // Draw the UI overlay using the UIRenderer
    if let Some(ui_renderer) = ui_renderer {
        // Note: We can't pass &UIRenderer to the closure because it's not Clone.
        // The UIRenderer needs to be accessible from the render graph pass.
        // For now, we'll skip the UI pass in the static render graph.
        // The application will need to render UI dynamically each frame.
        // TODO: Refactor to pass UIRenderer reference through RendererContext
    }

    // === PRESENT PASS ===
    // Copy the output to the swapchain
    builder.add_pass("present_pass", move |pass| {
        pass.blit(&resources.output_color, &resources.swapchain)
            .execute("present_pass", move |ctx| {
                // Perform the blit
                if let (Some((src_img, _)), Some((dst_img, _))) = (
                    ctx.get_image(resources.output_color.resource_id()),
                    ctx.get_image(resources.swapchain.resource_id()),
                ) {
                    let (width, height) = ctx.extent();
                    ctx.blit_images(src_img, dst_img, width, height);
                }
            });
    });

    // Compile the render graph with swapchain resource ID for proper layout transitions
    if let Err(e) = renderer.compile_render_graph(builder, Some(resources.swapchain.resource_id())) {
        log::error!("Failed to compile render graph: {:?}", e);
    }
}
