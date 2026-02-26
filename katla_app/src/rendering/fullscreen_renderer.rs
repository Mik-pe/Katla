//! Fullscreen renderer for sky and grid rendering.
//!
//! Encapsulates fullscreen pipeline creation and management.
//! Materials are pure config; this renderer owns the pipeline handles internally.

use katla_vulkan::{MaterialPipelineCache, PipelineHandle};

use super::{GridMaterial, SkyMaterial};

/// Renderer for fullscreen effects (sky, grid).
///
/// Owns pipeline handles internally. Created from pure material configs
/// using the shared material cache.
pub struct FullscreenRenderer {
    sky_pipeline: Option<PipelineHandle>,
    grid_pipeline: Option<PipelineHandle>,
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

    /// Get the sky pipeline handle.
    pub fn sky_pipeline(&self) -> Option<PipelineHandle> {
        self.sky_pipeline
    }

    /// Get the grid pipeline handle (only if grid should be visible).
    pub fn grid_pipeline(&self, visible: bool) -> Option<PipelineHandle> {
        if visible {
            self.grid_pipeline
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
