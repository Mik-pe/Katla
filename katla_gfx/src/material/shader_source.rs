//! Shader source types for materials.

use std::path::PathBuf;

/// Source for shader code
#[derive(Clone, Debug, Hash)]
pub enum ShaderSource {
    /// WGSL shader loaded from a file
    WgslFile(PathBuf),

    /// WGSL shader provided as a string
    WgslString(String),

    /// Pre-compiled SPIR-V shader
    PreCompiled(&'static [u8]),
}

impl ShaderSource {
    /// Get the shader source code as a string
    pub fn load(&self) -> Result<String, std::io::Error> {
        match self {
            ShaderSource::WgslFile(path) => std::fs::read_to_string(path),
            ShaderSource::WgslString(s) => Ok(s.clone()),
            ShaderSource::PreCompiled(_) => Ok(String::new()),
        }
    }
}
