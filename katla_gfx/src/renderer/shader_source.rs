//! Canonical shader sources and transitive include identities for both backends.

use std::collections::BTreeSet;
use std::io;
use std::path::{Component, Path, PathBuf};

pub(crate) struct ShaderSource {
    pub(crate) code: String,
    pub(crate) dependencies: BTreeSet<PathBuf>,
}

impl ShaderSource {
    pub(crate) fn load(path: &Path) -> io::Result<Self> {
        let path = if path.exists() {
            path.to_path_buf()
        } else {
            [
                "resources/shaders",
                "../resources/shaders",
                "../../resources/shaders",
            ]
            .into_iter()
            .map(|root| Path::new(root).join(path))
            .find(|candidate| candidate.exists())
            .unwrap_or_else(|| path.to_path_buf())
        };
        let mut source = Self {
            code: String::new(),
            dependencies: BTreeSet::new(),
        };
        source.expand(&path, &mut BTreeSet::new())?;
        Ok(source)
    }

    fn expand(&mut self, path: &Path, active: &mut BTreeSet<PathBuf>) -> io::Result<()> {
        let path = path.canonicalize()?;
        if active.contains(&path) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Cyclic shader include: {}", path.display()),
            ));
        }
        if !self.dependencies.insert(path.clone()) {
            return Ok(());
        }
        active.insert(path.clone());
        let raw = std::fs::read_to_string(&path)?;
        let parent = path.parent().unwrap_or(Path::new("."));
        for line in raw.lines() {
            let trimmed = line.trim();
            if let Some(include) = trimmed
                .strip_prefix("//include ")
                .or_else(|| trimmed.strip_prefix("#include "))
            {
                let include = include.trim();
                let target = if let Some(relative) = include
                    .strip_prefix('"')
                    .and_then(|value| value.strip_suffix('"'))
                {
                    parent.join(relative)
                } else if let Some(common) = include
                    .strip_prefix('<')
                    .and_then(|value| value.strip_suffix('>'))
                {
                    let root = parent
                        .ancestors()
                        .find(|dir| dir.join("common").is_dir())
                        .ok_or_else(|| {
                            io::Error::new(
                                io::ErrorKind::NotFound,
                                format!("Shader common directory missing for {}", path.display()),
                            )
                        })?;
                    root.join("common").join(common)
                } else {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("Invalid shader include in {}: {include}", path.display()),
                    ));
                };
                self.expand(&target, active)?;
            } else {
                self.code.push_str(line);
                self.code.push('\n');
            }
        }
        active.remove(&path);
        Ok(())
    }
}

pub(crate) fn path_identity(path: &Path) -> PathBuf {
    if let Ok(canonical) = path.canonicalize() {
        return canonical;
    }
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(path)
    };
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shader_dependencies_are_transitive_and_distinguish_duplicate_names() {
        let root = std::env::temp_dir().join(format!(
            "katla-shader-dependencies-{}",
            crate::renderer::texture_readback::fresh_readback_id()
        ));
        std::fs::create_dir_all(root.join("common")).unwrap();
        std::fs::create_dir_all(root.join("other")).unwrap();
        std::fs::write(root.join("common/leaf.wgsl"), "const LEAF:u32=7u;\n").unwrap();
        std::fs::write(
            root.join("common/shared.wgsl"),
            "#include \"leaf.wgsl\"\nconst SHARED:u32=LEAF;\n",
        )
        .unwrap();
        std::fs::write(
            root.join("model.wgsl"),
            "#include <shared.wgsl>\n#include <leaf.wgsl>\nconst MODEL:u32=SHARED;\n",
        )
        .unwrap();
        std::fs::write(root.join("other/model.wgsl"), "const OTHER:u32=0u;\n").unwrap();
        let source = ShaderSource::load(&root.join("model.wgsl")).unwrap();
        assert_eq!(source.dependencies.len(), 3);
        assert!(
            source
                .dependencies
                .contains(&path_identity(&root.join("common/leaf.wgsl")))
        );
        assert!(
            !source
                .dependencies
                .contains(&path_identity(&root.join("other/model.wgsl")))
        );
        assert_eq!(source.code.matches("const LEAF").count(), 1);
        std::fs::remove_file(root.join("common/leaf.wgsl")).unwrap();
        assert!(
            source
                .dependencies
                .contains(&path_identity(&root.join("common/../common/leaf.wgsl")))
        );
        std::fs::write(root.join("common/leaf.wgsl"), "#include <shared.wgsl>\n").unwrap();
        assert_eq!(
            ShaderSource::load(&root.join("model.wgsl"))
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::InvalidData
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
