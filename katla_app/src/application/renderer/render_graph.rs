//! Render graph setup for the application layer.
//!
//! This module provides the render graph building API for the application.
//! Currently uses the legacy VulkanRenderer.setup_render_graph() method.
//!
//! ## Future Work
//!
//! The goal is to have the application own the render graph configuration:
//! - Application owns pipelines (sky, grid, ui)
//! - Application defines passes via builder API
//! - VulkanRenderer is just infrastructure
//!
//! For now, we use the legacy API and will migrate incrementally.

use std::cell::RefCell;
use std::rc::Rc;

use katla_vulkan::{MaterialPipeline, VulkanRenderer};

/// Build the render graph with all application passes.
///
/// This function configures the render graph with sky, grid, geometry, UI, and present passes.
/// Currently uses the legacy VulkanRenderer.setup_render_graph() method.
///
/// # Arguments
///
/// * `renderer` - The Vulkan renderer
/// * `sky_pipeline` - Sky rendering pipeline (optional)
/// * `grid_pipeline` - Grid rendering pipeline (optional)
/// * `ui_pipeline` - UI rendering pipeline (optional)
pub fn build_render_graph(
    renderer: &mut VulkanRenderer,
    sky_pipeline: Option<Rc<RefCell<MaterialPipeline>>>,
    grid_pipeline: Option<Rc<RefCell<MaterialPipeline>>>,
    ui_pipeline: Option<Rc<RefCell<MaterialPipeline>>>,
) {
    // Use the legacy API for now
    // TODO: Migrate to application-owned render graph using new API
    renderer.setup_render_graph(sky_pipeline, grid_pipeline, ui_pipeline);
}
