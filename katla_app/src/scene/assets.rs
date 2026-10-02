//! Explicit asset roots, portable references and scene-origin resolution.

use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};

/// File references state their root rather than depending on the process cwd.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssetRef {
    /// Relative to the application's ResourceManager root.
    Resource(String),
    /// Relative to the scene file's directory.
    Scene(String),
    /// An intentionally nonportable absolute file reference.
    File(PathBuf),
}

impl AssetRef {
    /// Display label without interpreting the path's root.
    pub fn path(&self) -> &Path {
        match self {
            Self::Resource(path) | Self::Scene(path) => Path::new(path),
            Self::File(path) => path,
        }
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        let path = self.path();
        if path.as_os_str().is_empty() || path.to_string_lossy().contains('\0') {
            return Err("an asset path is required".into());
        }
        match self {
            Self::File(path) if !path.is_absolute() => {
                Err("File references require an absolute path".into())
            }
            Self::File(_) => Ok(()),
            Self::Resource(value) | Self::Scene(value) => {
                if value.contains('\\') || value.contains(':') || value.contains('\0') {
                    return Err(
                        "relative asset paths use forward slashes and no drive prefix".into(),
                    );
                }
                if value
                    .split('/')
                    .any(|part| part.is_empty() || part == "." || part == "..")
                    || path
                        .components()
                        .any(|part| !matches!(part, Component::Normal(_)))
                {
                    return Err("relative asset paths cannot contain roots, '.' or '..'".into());
                }
                Ok(())
            }
        }
    }
}

/// Resolution context for a document or an in-memory scene.
#[derive(Debug, Clone)]
pub struct SceneAssetContext {
    resources: PathBuf,
    directory: Option<PathBuf>,
}

impl SceneAssetContext {
    /// Capture absolute roots once; asset resolution itself never reads cwd.
    pub fn new(resources: impl AsRef<Path>, scene_file: Option<&Path>) -> std::io::Result<Self> {
        let cwd = std::env::current_dir()?;
        let absolute = |path: &Path| {
            if path.is_absolute() {
                path.to_path_buf()
            } else {
                cwd.join(path)
            }
        };
        Ok(Self {
            resources: absolute(resources.as_ref()),
            directory: scene_file.and_then(|file| absolute(file).parent().map(Path::to_path_buf)),
        })
    }

    /// Rebase a document origin while retaining its captured resource root.
    pub fn with_origin(&self, scene_file: &Path) -> std::io::Result<Self> {
        Self::new(&self.resources, Some(scene_file))
    }

    /// Resolve an explicit reference; caller decides whether existence is required.
    pub fn resolve(&self, reference: &AssetRef) -> Result<PathBuf, String> {
        reference.validate()?;
        match reference {
            AssetRef::Resource(path) => Ok(self.resources.join(path)),
            AssetRef::Scene(path) => self
                .directory
                .as_ref()
                .map(|base| base.join(path))
                .ok_or_else(|| "Scene references require a scene file origin".into()),
            AssetRef::File(path) => Ok(path.clone()),
        }
    }

    /// Prefer portable roots when a runtime component contains an absolute path.
    pub fn identify(&self, path: &Path) -> Result<AssetRef, String> {
        if !path.is_absolute() {
            return Err("runtime asset paths must be absolute".into());
        }
        let relative = |base: &Path| {
            path.strip_prefix(base)
                .ok()
                .and_then(|value| value.to_str())
                .map(|value| value.replace('\\', "/"))
        };
        if let Some(path) = relative(&self.resources) {
            return Ok(AssetRef::Resource(path));
        }
        if let Some(path) = self.directory.as_ref().and_then(|base| relative(base)) {
            return Ok(AssetRef::Scene(path));
        }
        Ok(AssetRef::File(path.to_path_buf()))
    }
}
