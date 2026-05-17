use super::assets;
use std::fs;
use std::path::PathBuf;

fn create_temp_resources() -> (PathBuf, tempfile::TempDir) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().to_path_buf();
    fs::create_dir_all(root.join("scripts")).unwrap();
    fs::create_dir_all(root.join("models")).unwrap();
    fs::create_dir_all(root.join("shaders")).unwrap();
    fs::write(root.join("scripts/hello.luau"), "print('hello')").unwrap();
    fs::write(root.join("scripts/utils.luau"), "return 42").unwrap();
    fs::write(root.join("models/cube.gltf"), "<gltf>").unwrap();
    fs::write(root.join("shaders/vert.spv"), b"\x03\x02\x23\x07").unwrap();
    fs::write(root.join("shaders/frag.spv"), b"\x03\x02\x23\x07").unwrap();
    fs::write(root.join("readme.txt"), "asset readme").unwrap();
    (root, temp)
}

#[test]
fn test_list_assets_no_filter() {
    let (root, _temp) = create_temp_resources();
    let all = assets::list_assets_with_root(None, None, &root).unwrap();
    assert!(!all.is_empty());
    assert!(all.iter().any(|p| p.contains("hello.luau")));
    assert!(all.iter().any(|p| p.contains("cube.gltf")));
    assert!(all.iter().any(|p| p.contains("vert.spv")));
}

#[test]
fn test_list_assets_by_extension() {
    let (root, _temp) = create_temp_resources();
    let luau_files = assets::list_assets_with_root(Some("luau"), None, &root).unwrap();
    assert_eq!(luau_files.len(), 2);
    for path in &luau_files {
        assert!(
            path.to_lowercase().ends_with(".luau"),
            "Expected .luau extension, got: {path}"
        );
    }
}

#[test]
fn test_list_assets_by_subdir() {
    let (root, _temp) = create_temp_resources();
    let scripts = assets::list_assets_with_root(None, Some("scripts"), &root).unwrap();
    assert_eq!(scripts.len(), 2);
    for path in &scripts {
        assert!(
            path.starts_with("scripts"),
            "Expected path under scripts/, got: {path}"
        );
    }
}

#[test]
fn test_list_assets_extension_and_subdir() {
    let (root, _temp) = create_temp_resources();
    let shaders = assets::list_assets_with_root(Some("spv"), Some("shaders"), &root).unwrap();
    assert_eq!(shaders.len(), 2);
    for path in &shaders {
        assert!(path.starts_with("shaders"));
        assert!(path.to_lowercase().ends_with(".spv"));
    }
}

#[test]
fn test_list_assets_nonexistent_subdir() {
    let (root, _temp) = create_temp_resources();
    let result = assets::list_assets_with_root(None, Some("nonexistent_dir_xyz"), &root);
    assert!(result.is_err());
}

#[test]
fn test_list_assets_traversal_subdir() {
    let (root, _temp) = create_temp_resources();
    let result = assets::list_assets_with_root(None, Some("../secret"), &root);
    assert!(result.is_err());
}

#[test]
fn test_read_asset_existing() {
    let (root, _temp) = create_temp_resources();
    let content = assets::read_asset_with_root("scripts/hello.luau", &root).unwrap();
    assert_eq!(content, "print('hello')");
}

#[test]
fn test_read_asset_nonexistent() {
    let (root, _temp) = create_temp_resources();
    let result = assets::read_asset_with_root("nonexistent_file_xyz.txt", &root);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("not found"));
}

#[test]
fn test_read_asset_traversal() {
    let (root, _temp) = create_temp_resources();
    let result = assets::read_asset_with_root("../Cargo.toml", &root);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("traversal"));
}

#[test]
fn test_write_and_read_roundtrip() {
    let (root, _temp) = create_temp_resources();
    let content = "hello from test\nline two";
    let result = assets::write_asset_with_root("test_roundtrip.txt", content, &root);
    assert!(result.is_ok(), "write_asset failed: {:?}", result);

    let read_back = assets::read_asset_with_root("test_roundtrip.txt", &root).unwrap();
    assert_eq!(read_back, content);
}

#[test]
fn test_write_creates_parent_dirs() {
    let (root, _temp) = create_temp_resources();
    let result = assets::write_asset_with_root("a/b/c/deep.txt", "deeply nested", &root);
    assert!(result.is_ok(), "write_asset failed: {:?}", result);

    let read_back = assets::read_asset_with_root("a/b/c/deep.txt", &root).unwrap();
    assert_eq!(read_back, "deeply nested");
}

#[test]
fn test_write_traversal() {
    let (root, _temp) = create_temp_resources();
    let result = assets::write_asset_with_root("../evil.txt", "bad", &root);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("traversal"));
}

#[test]
fn test_write_absolute_path() {
    let (root, _temp) = create_temp_resources();
    let result = assets::write_asset_with_root("/tmp/evil.txt", "bad", &root);
    assert!(result.is_err());
}

#[test]
fn test_path_validation_root_dir() {
    let (root, _temp) = create_temp_resources();
    let result = assets::read_asset_with_root("/etc/passwd", &root);
    assert!(result.is_err());
}
