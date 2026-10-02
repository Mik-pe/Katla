//! Bounded text assets and atomic publication after semantic validation.

use std::{io::Read, path::Path};

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
