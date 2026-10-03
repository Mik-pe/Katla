//! Bounded text assets and atomic publication after semantic validation.

use std::{
    io::Read,
    path::{Path, PathBuf},
};

pub(crate) const MAX_ASSET_BYTES: usize = 64 * 1024 * 1024;

pub(crate) fn read_text(path: &Path) -> Result<String, String> {
    let file = std::fs::File::open(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let mut text = String::new();
    file.take(MAX_ASSET_BYTES as u64 + 1)
        .read_to_string(&mut text)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    if text.len() > MAX_ASSET_BYTES {
        return Err("Asset file exceeds the 64 MiB limit".into());
    }
    Ok(text)
}

pub(crate) fn write_text(path: &Path, text: &str) -> Result<(), String> {
    if text.len() > MAX_ASSET_BYTES {
        return Err("Asset file exceeds the 64 MiB limit".into());
    }
    super::config::write_atomic(path, text.as_bytes())
        .map_err(|error| format!("{}: {error}", path.display()))
}

/// Resolve a project-relative publication path without escaping through symlinks.
pub(crate) fn project_file(resources: &Path, relative: &str) -> Result<PathBuf, String> {
    crate::scene::AssetRef::Scene(relative.into()).validate()?;
    let root = resources
        .parent()
        .ok_or("Resource root has no project directory")?
        .canonicalize()
        .map_err(|error| error.to_string())?;
    let file = root.join(relative);
    let mut ancestor = file.as_path();
    while !ancestor.exists() {
        ancestor = ancestor.parent().ok_or("No existing asset directory")?;
    }
    if !ancestor
        .canonicalize()
        .map_err(|error| error.to_string())?
        .starts_with(&root)
    {
        return Err("Asset path escapes the project through a symbolic link".into());
    }
    Ok(file)
}
