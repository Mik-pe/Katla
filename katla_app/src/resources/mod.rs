//! Resource path management.
//!
//! This module provides centralized resource path discovery and management.

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
}

impl ResourceManager {
    /// Discover resource paths by searching common locations.
    ///
    /// Searches in order:
    /// 1. Current directory (workspace root)
    /// 2. Parent directory (crate root)
    /// 3. Grandparent directory (target/debug)
    /// 4. CARGO_MANIFEST_DIR absolute path
    ///
    /// # Returns
    /// `Ok(ResourceManager)` if a valid resources directory was found
    /// `Err(AppError::ResourcesNotFound)` if no valid directory was found
    pub fn discover() -> AppResult<Self> {
        // List of possible root paths to check, in order of preference
        let possible_roots = vec![
            // Current directory (for running from workspace root)
            PathBuf::from("resources"),
            // Parent directory (for running from katla_app)
            PathBuf::from("../resources"),
            // Grandparent directory (for running from target/debug)
            PathBuf::from("../../resources"),
            // Absolute path using CARGO_MANIFEST_DIR (for tests)
            {
                let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
                path.pop(); // Go up from katla_app to workspace root
                path.push("resources");
                path
            },
        ];

        for root in possible_roots {
            if root.exists() {
                info!("Found resources at: {}", root.display());
                return Ok(Self::from_root(root));
            }
        }

        Err(AppError::ResourcesNotFound {
            path: "resources/".to_string(),
        })
    }

    /// Create ResourceManager from an explicit root path.
    pub fn from_root(root: PathBuf) -> Self {
        let models = root.join("models");
        let materials = root.join("materials");
        let shaders = root.join("shaders");
        let fonts = root.join("fonts");

        Self {
            root,
            models,
            materials,
            shaders,
            fonts,
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_root() {
        let root = PathBuf::from("/test/resources");
        let manager = ResourceManager::from_root(root.clone());

        assert_eq!(manager.root, root);
        assert_eq!(manager.models, root.join("models"));
        assert_eq!(manager.materials, root.join("materials"));
        assert_eq!(manager.shaders, root.join("shaders"));
        assert_eq!(manager.fonts, root.join("fonts"));
    }

    #[test]
    fn test_model_path() {
        let manager = ResourceManager::from_root(PathBuf::from("/test/resources"));
        let path = manager.model_path("Fox.glb");

        assert_eq!(path, PathBuf::from("/test/resources/models/Fox.glb"));
    }

    #[test]
    fn test_material_path() {
        let manager = ResourceManager::from_root(PathBuf::from("/test/resources"));
        let path = manager.material_path("test.toml");

        assert_eq!(path, PathBuf::from("/test/resources/materials/test.toml"));
    }

    #[test]
    fn test_shader_path() {
        let manager = ResourceManager::from_root(PathBuf::from("/test/resources"));
        let path = manager.shader_path("test.wgsl");

        assert_eq!(path, PathBuf::from("/test/resources/shaders/test.wgsl"));
    }

    #[test]
    fn test_font_path() {
        let manager = ResourceManager::from_root(PathBuf::from("/test/resources"));
        let path = manager.font_path("roboto.ttf");

        assert_eq!(path, PathBuf::from("/test/resources/fonts/roboto.ttf"));
    }
}
