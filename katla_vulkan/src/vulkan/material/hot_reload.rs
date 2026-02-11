//! Hot reload functionality for materials.
//!
//! This module provides runtime shader reloading, allowing shader changes
//! to be reflected immediately without restarting the application.

use super::{MaterialBuilder, MaterialDescriptor, MaterialError, MaterialPipeline, ShaderSource};
use crate::{RenderPass, Texture, VertexBinding, VulkanContext};
use std::{rc::Rc, time::SystemTime};

/// Errors that can occur during hot reload
#[derive(Debug)]
pub enum HotReloadError {
    ShaderNotReloadable(String),
    RecompilationFailed(String),
    PipelineCreationFailed(MaterialError),
    WatchNotEnabled,
}

impl std::fmt::Display for HotReloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HotReloadError::ShaderNotReloadable(msg) => {
                write!(f, "Shader not reloadable: {}", msg)
            }
            HotReloadError::RecompilationFailed(msg) => {
                write!(f, "Shader recompilation failed: {}", msg)
            }
            HotReloadError::PipelineCreationFailed(e) => {
                write!(f, "Pipeline creation failed: {}", e)
            }
            HotReloadError::WatchNotEnabled => {
                write!(f, "Hot reload watch is not enabled")
            }
        }
    }
}

impl std::error::Error for HotReloadError {}

/// Material with hot reload support
pub struct HotReloadMaterial {
    material: MaterialPipeline,
    descriptor: MaterialDescriptor,
    context: Rc<VulkanContext>,
    render_pass: RenderPass,
    watch_enabled: bool,
    last_modified: SystemTime,
    vertex_binding: Option<VertexBinding>,
    texture: Option<Rc<Texture>>,
}

impl HotReloadMaterial {
    /// Create a new hot-reloadable material
    pub fn new(
        descriptor: MaterialDescriptor,
        context: Rc<VulkanContext>,
        render_pass: RenderPass,
    ) -> Result<Self, MaterialError> {
        // Build the initial material
        let builder = MaterialBuilder::from_descriptor(descriptor.clone(), context.clone())?;

        // Store vertex binding and texture for rebuild
        let vertex_binding = None; // Will be set by caller
        let texture = None; // Will be set by caller

        let material = builder.build(Some(&render_pass)).map_err(|e| {
            MaterialError::InvalidDescriptor(format!("Pipeline creation failed: {:?}", e))
        })?;

        // Get initial modification time
        let last_modified =
            Self::get_shader_modification_time(&descriptor).unwrap_or_else(SystemTime::now);

        Ok(Self {
            material,
            descriptor,
            context,
            render_pass,
            watch_enabled: false,
            last_modified,
            vertex_binding,
            texture,
        })
    }

    /// Enable hot reload watching for this material
    pub fn enable_watch(&mut self) {
        self.watch_enabled = true;
    }

    /// Disable hot reload watching
    pub fn disable_watch(&mut self) {
        self.watch_enabled = false;
    }

    /// Check if hot reload is enabled
    pub fn is_watching(&self) -> bool {
        self.watch_enabled
    }

    /// Set the vertex binding (required for rebuild)
    pub fn set_vertex_binding(&mut self, binding: VertexBinding) {
        self.vertex_binding = Some(binding);
    }

    /// Set the texture (required for rebuild)
    pub fn set_texture(&mut self, texture: Rc<Texture>) {
        self.texture = Some(texture);
    }

    /// Check for shader modifications and reload if necessary
    ///
    /// Returns true if the material was reloaded
    pub fn check_reload(&mut self) -> Result<bool, HotReloadError> {
        if !self.watch_enabled {
            return Ok(false);
        }

        // Check if shaders are reloadable
        if !self.are_shaders_reloadable() {
            return Err(HotReloadError::ShaderNotReloadable(
                "Shaders must be file-based WGSL to support hot reload".to_string(),
            ));
        }

        // Check modification times
        let current_time =
            Self::get_shader_modification_time(&self.descriptor).ok_or_else(|| {
                HotReloadError::ShaderNotReloadable(
                    "Could not read shader modification time".to_string(),
                )
            })?;

        if current_time > self.last_modified {
            // Shaders have been modified, trigger reload
            self.reload()?;
            self.last_modified = current_time;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Force a reload of the material
    pub fn reload(&mut self) -> Result<(), HotReloadError> {
        // Destroy old material
        self.material.destroy();

        // Rebuild the material
        let mut builder =
            MaterialBuilder::from_descriptor(self.descriptor.clone(), self.context.clone())
                .map_err(HotReloadError::PipelineCreationFailed)?;

        // Re-apply vertex binding and texture if present
        if let Some(ref binding) = self.vertex_binding {
            builder = builder.with_vertex_binding(binding.clone());
        }

        if let Some(ref texture) = self.texture {
            builder = builder.with_texture(texture.clone());
        }

        // Build new material
        self.material = builder.build(Some(&self.render_pass)).map_err(|e| {
            HotReloadError::PipelineCreationFailed(MaterialError::InvalidDescriptor(format!(
                "Pipeline creation failed: {:?}",
                e
            )))
        })?;

        Ok(())
    }

    /// Get the underlying material pipeline
    pub fn material(&self) -> &MaterialPipeline {
        &self.material
    }

    /// Get mutable access to the underlying material
    pub fn material_mut(&mut self) -> &mut MaterialPipeline {
        &mut self.material
    }

    /// Get the material descriptor
    pub fn descriptor(&self) -> &MaterialDescriptor {
        &self.descriptor
    }

    /// Check if the shaders are file-based (and thus reloadable)
    fn are_shaders_reloadable(&self) -> bool {
        matches!(self.descriptor.vertex_shader, ShaderSource::WgslFile(_))
            && matches!(self.descriptor.fragment_shader, ShaderSource::WgslFile(_))
    }

    /// Get the latest modification time from shader files
    fn get_shader_modification_time(descriptor: &MaterialDescriptor) -> Option<SystemTime> {
        let mut latest_time = SystemTime::UNIX_EPOCH;

        // Check vertex shader
        if let ShaderSource::WgslFile(path) = &descriptor.vertex_shader {
            if let Ok(metadata) = std::fs::metadata(path) {
                if let Ok(modified) = metadata.modified() {
                    latest_time = latest_time.max(modified);
                }
            }
        }

        // Check fragment shader
        if let ShaderSource::WgslFile(path) = &descriptor.fragment_shader {
            if let Ok(metadata) = std::fs::metadata(path) {
                if let Ok(modified) = metadata.modified() {
                    latest_time = latest_time.max(modified);
                }
            }
        }

        if latest_time == SystemTime::UNIX_EPOCH {
            None
        } else {
            Some(latest_time)
        }
    }

    /// Destroy the material and release resources
    pub fn destroy(&mut self) {
        self.material.destroy();
    }
}

impl Drop for HotReloadMaterial {
    fn drop(&mut self) {
        self.material.destroy();
    }
}

// Note: Tests for hot reload require file system setup and are tested
// in the example programs (hot_reload_code.rs, etc.)
