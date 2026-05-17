use std::fs;
use std::path::Path;
use std::path::PathBuf;

const RESOURCES_DIR: &str = "resources";

fn resources_root() -> PathBuf {
    PathBuf::from(RESOURCES_DIR)
}

fn validate_relative_path_with_root(path: &str, root: &Path) -> Result<PathBuf, String> {
    let relative = Path::new(path);
    for component in relative.components() {
        match component {
            std::path::Component::ParentDir => {
                return Err(format!("Path traversal denied: '{path}'"));
            }
            std::path::Component::RootDir => {
                return Err(format!("Absolute path denied: '{path}'"));
            }
            std::path::Component::Prefix(_) => {
                return Err(format!("Windows prefix denied: '{path}'"));
            }
            _ => {}
        }
    }
    Ok(root.join(relative))
}

/// List files in the resources directory recursively.
///
/// - `extension`: optional file extension filter (e.g., "luau", "gltf") — matched case-insensitively
/// - `subdir`: optional subdirectory to scope the listing (e.g., "scripts", "models")
///
/// Returns paths relative to `resources/`.
pub fn list_assets(extension: Option<&str>, subdir: Option<&str>) -> Result<Vec<String>, String> {
    list_assets_with_root(extension, subdir, &resources_root())
}

pub(crate) fn list_assets_with_root(
    extension: Option<&str>,
    subdir: Option<&str>,
    root: &Path,
) -> Result<Vec<String>, String> {
    let base = match subdir {
        Some(dir) => validate_relative_path_with_root(dir, root)?,
        None => root.to_path_buf(),
    };

    if !base.exists() {
        return Err(format!("Directory not found: {}", base.display()));
    }
    if !base.is_dir() {
        return Err(format!("Not a directory: {}", base.display()));
    }

    let mut results = Vec::new();

    visit_dir(&base, root, extension, &mut results)?;

    results.sort();
    Ok(results)
}

fn visit_dir(
    dir: &Path,
    root: &Path,
    extension: Option<&str>,
    results: &mut Vec<String>,
) -> Result<(), String> {
    let entries =
        fs::read_dir(dir).map_err(|e| format!("Failed to read {}: {e}", dir.display()))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read entry: {e}"))?;
        let path = entry.path();

        if path.is_dir() {
            visit_dir(&path, root, extension, results)?;
        } else if path.is_file() {
            let matches_ext = match extension {
                Some(ext) => path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case(ext)),
                None => true,
            };

            if matches_ext && let Ok(rel) = path.strip_prefix(root) {
                results.push(rel.to_string_lossy().to_string());
            }
        }
    }

    Ok(())
}

/// Read file contents from the resources directory by relative path.
///
/// Returns the file contents as a String.
pub fn read_asset(path: &str) -> Result<String, String> {
    read_asset_with_root(path, &resources_root())
}

pub(crate) fn read_asset_with_root(path: &str, root: &Path) -> Result<String, String> {
    let full_path = validate_relative_path_with_root(path, root)?;

    if !full_path.exists() {
        return Err(format!("File not found: {path}"));
    }
    if !full_path.is_file() {
        return Err(format!("Not a file: {path}"));
    }

    fs::read_to_string(&full_path).map_err(|e| format!("Failed to read {path}: {e}"))
}

/// Write content to a file in the resources directory, creating parent directories as needed.
///
/// The path is relative to `resources/`. Path traversal (e.g., `..`) is rejected.
pub fn write_asset(path: &str, content: &str) -> Result<String, String> {
    write_asset_with_root(path, content, &resources_root())
}

pub(crate) fn write_asset_with_root(
    path: &str,
    content: &str,
    root: &Path,
) -> Result<String, String> {
    let full_path = validate_relative_path_with_root(path, root)?;

    if let Some(parent) = full_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create directories: {e}"))?;
    }

    fs::write(&full_path, content).map_err(|e| format!("Failed to write {path}: {e}"))?;

    Ok(format!("Written {} bytes to {path}", content.len()))
}
