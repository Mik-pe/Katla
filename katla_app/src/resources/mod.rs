//! Resource management.
//!
//! This module provides centralized resource path discovery and management,
//! as well as ECS resources for the editor.

pub mod ambient_light;
pub mod viewport_state;

pub use ambient_light::*;

// Resource path management (legacy)

use crate::{AppError, AppResult};
use log::info;
use std::path::{Path, PathBuf};

/// Centralized resource path manager.
///
/// Handles discovery of resource directories (models, materials, shaders, etc.)
/// from various runtime contexts (workspace root, crate root, target/debug, etc.).
#[derive(Debug, Clone)]
pub struct ResourceManager {
    /// Root resources directory (parent of models/, materials/, shaders/)
    pub root: PathBuf,
    /// Path to models directory
    pub models: PathBuf,
    /// Path to materials directory
    pub materials: PathBuf,
    /// Path to shaders directory
    pub shaders: PathBuf,
    /// Path to fonts directory
    pub fonts: PathBuf,
    /// Path to scripts directory
    pub scripts: PathBuf,
}

impl ResourceManager {
    /// Discover resource paths by searching common locations.
    ///
    /// Searches in order:
    /// 1. `KATLA_RESOURCES_PATH` environment variable (explicit override)
    /// 2. Absolute path derived from `CARGO_MANIFEST_DIR` (most reliable)
    /// 3. Current directory (workspace root)
    /// 4. Parent directory (crate root)
    /// 5. Grandparent directory (target/debug)
    ///
    /// # Returns
    /// `Ok(ResourceManager)` if a valid resources directory was found
    /// `Err(AppError::ResourcesNotFound)` if no valid directory was found
    pub fn discover() -> AppResult<Self> {
        let possible_roots = Self::discover_paths();

        for root in &possible_roots {
            if root.exists() {
                info!("Found resources at: {}", root.display());
                return Ok(Self::from_root(root.clone()));
            }
        }

        Err(AppError::ResourcesNotFound {
            path: possible_roots
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", "),
        })
    }

    /// Build the ordered list of candidate resource paths.
    fn discover_paths() -> Vec<PathBuf> {
        let mut paths = Vec::new();

        // 1. Explicit override via environment variable
        if let Ok(env_path) = std::env::var("KATLA_RESOURCES_PATH") {
            paths.push(PathBuf::from(env_path));
        }

        // 2. Absolute path from CARGO_MANIFEST_DIR (most reliable, works from any cwd)
        {
            let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            path.pop();
            path.push("resources");
            paths.push(path);
        }

        // 3-5. Relative fallbacks (depend on working directory)
        paths.push(PathBuf::from("resources"));
        paths.push(PathBuf::from("../resources"));
        paths.push(PathBuf::from("../../resources"));

        paths
    }

    /// Create ResourceManager from an explicit root path.
    pub fn from_root(root: PathBuf) -> Self {
        let models = root.join("models");
        let materials = root.join("materials");
        let shaders = root.join("shaders");
        let fonts = root.join("fonts");
        let scripts = root.join("scripts");

        Self {
            root,
            models,
            materials,
            shaders,
            fonts,
            scripts,
        }
    }

    /// Get path to a model file by name.
    pub fn model_path(&self, name: impl AsRef<Path>) -> PathBuf {
        self.models.join(name)
    }

    /// Get path to a material file by name.
    pub fn material_path(&self, name: impl AsRef<Path>) -> PathBuf {
        self.materials.join(name)
    }

    /// Get path to a shader file by name.
    pub fn shader_path(&self, name: impl AsRef<Path>) -> PathBuf {
        self.shaders.join(name)
    }

    /// Get path to a font file by name.
    pub fn font_path(&self, name: impl AsRef<Path>) -> PathBuf {
        self.fonts.join(name)
    }

    /// Get path to a script file by name (without extension).
    pub fn script_path(&self, name: impl AsRef<Path>) -> PathBuf {
        self.scripts.join(name).with_extension("luau")
    }
}
