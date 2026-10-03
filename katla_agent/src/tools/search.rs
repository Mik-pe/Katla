//! Deterministic asset discovery rooted in the application's resource directory.

use serde::Deserialize;
use std::path::Path;

/// Search paths using all whitespace-separated words, without guessing asset names.
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(feature = "mcp-server", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct AssetSearch {
    /// Case-insensitive words to match anywhere in the relative asset path.
    #[serde(default)]
    pub query: String,
    /// File extensions without dots, e.g. ["glb", "gltf"] for models.
    #[serde(default)]
    pub extensions: Vec<String>,
    /// Maximum results, default 64 and capped at 256.
    #[serde(default)]
    pub limit: Option<usize>,
}

/// Return relative paths suitable for spawn_model and explicit truncation metadata.
pub fn search_assets(root: &Path, request: &AssetSearch) -> Result<serde_json::Value, String> {
    let words: Vec<_> = request
        .query
        .split_whitespace()
        .map(str::to_lowercase)
        .collect();
    let extensions: Vec<_> = request
        .extensions
        .iter()
        .map(|e| e.trim_start_matches('.').to_lowercase())
        .collect();
    let mut matches = Vec::new();
    let mut remaining = 20_000usize;
    visit(
        root,
        root,
        &words,
        &extensions,
        &mut matches,
        &mut remaining,
    )?;
    matches.sort();
    let total = matches.len();
    let limit = request.limit.unwrap_or(64).clamp(1, 256);
    matches.truncate(limit);
    let prefix = root
        .file_name()
        .ok_or("Resource root needs a project-relative directory name")?
        .to_string_lossy();
    let project_paths: Vec<_> = matches
        .iter()
        .map(|path| format!("{prefix}/{path}"))
        .collect();
    Ok(
        serde_json::json!({"assets":matches,"project_paths":project_paths,"total":total,"truncated":total>limit,"root":root,"path_contract":"Paths are relative to the discovered resources root; pass assets to spawn_model/behavior set_script, project_paths to prefab read/instantiate or material_asset read/apply."}),
    )
}

fn visit(
    dir: &Path,
    root: &Path,
    words: &[String],
    extensions: &[String],
    matches: &mut Vec<String>,
    remaining: &mut usize,
) -> Result<(), String> {
    let mut entries = std::fs::read_dir(dir)
        .map_err(|e| format!("Cannot search {}: {e}", dir.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        if *remaining == 0 {
            return Err("Asset search exceeded 20000 entries; narrow the resource root".into());
        }
        *remaining -= 1;
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        let path = entry.path();
        if kind.is_symlink() {
            continue;
        }
        if kind.is_dir() {
            visit(&path, root, words, extensions, matches, remaining)?;
        } else if kind.is_file() {
            let relative = path
                .strip_prefix(root)
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .replace('\\', "/");
            let lower = relative.to_lowercase();
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or_default()
                .to_lowercase();
            if words.iter().all(|w| lower.contains(w))
                && (extensions.is_empty() || extensions.contains(&ext))
            {
                matches.push(relative);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_search_words_extensions_and_truncation() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("models")).unwrap();
        for name in ["Oak Chair.glb", "Oak Table.glb", "Oak Chair.png"] {
            std::fs::write(root.path().join("models").join(name), []).unwrap();
        }
        let request = AssetSearch {
            query: "MODELS oak".into(),
            extensions: vec!["GLB".into()],
            limit: Some(1),
        };
        let result = search_assets(root.path(), &request).unwrap();
        assert_eq!(result["total"], 2);
        assert_eq!(result["truncated"], true);
        assert_eq!(result["assets"][0], "models/Oak Chair.glb");
    }
    #[cfg(unix)]
    #[test]
    fn test_search_does_not_follow_symlinks() {
        let root = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(root.path(), root.path().join("loop")).unwrap();
        let request = AssetSearch {
            query: String::new(),
            extensions: vec![],
            limit: None,
        };
        assert_eq!(search_assets(root.path(), &request).unwrap()["total"], 0);
    }
}
