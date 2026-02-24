//! Fullscreen renderer for sky and grid rendering.
//!
//! Encapsulates fullscreen pipeline creation and management.
//! Materials are pure config; this renderer owns the pipelines internally.

use std::cell::RefCell;
use std::rc::Rc;

use katla_vulkan::{MaterialPipeline, MaterialPipelineCache};

use super::{GridMaterial, SkyMaterial};

/// Renderer for fullscreen effects (sky, grid).
///
/// Owns pipelines internally. Created from pure material configs
/// using the shared material cache.
pub struct FullscreenRenderer {
    sky_pipeline: Option<Rc<RefCell<MaterialPipeline>>>,
    grid_pipeline: Option<Rc<RefCell<MaterialPipeline>>>,
}

impl FullscreenRenderer {
    /// Create a new fullscreen renderer.
    ///
    /// Creates pipelines internally from SkyMaterial and GridMaterial configs.
    pub fn new(cache: &mut MaterialPipelineCache) -> Self {
        // Create sky pipeline from pure config
        let sky_material = SkyMaterial::default();
        let sky_pipeline = cache.get_or_create(&sky_material).ok();
        if sky_pipeline.is_none() {
            log::error!("Failed to create sky pipeline!");
        } else {
            log::debug!("Sky pipeline created successfully");
        }

        // Create grid pipeline from pure config
        let grid_material = GridMaterial::default();
        let grid_pipeline = cache.get_or_create(&grid_material).ok();
        if grid_pipeline.is_none() {
            log::error!("Failed to create grid pipeline!");
        } else {
            log::debug!("Grid pipeline created successfully");
        }

        Self {
            sky_pipeline,
            grid_pipeline,
        }
    }

    /// Get the sky pipeline.
    pub fn sky_pipeline(&self) -> Option<Rc<RefCell<MaterialPipeline>>> {
        self.sky_pipeline.clone()
    }

    /// Get the grid pipeline (only if grid should be visible).
    pub fn grid_pipeline(&self, visible: bool) -> Option<Rc<RefCell<MaterialPipeline>>> {
        if visible {
            self.grid_pipeline.clone()
        } else {
            None
        }
    }

    /// Check if sky pipeline is available.
    pub fn has_sky(&self) -> bool {
        self.sky_pipeline.is_some()
    }

    /// Check if grid pipeline is available.
    pub fn has_grid(&self) -> bool {
        self.grid_pipeline.is_some()
    }
}
