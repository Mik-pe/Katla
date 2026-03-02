//! Material asset loading from TOML files.
//!
//! This module provides functionality to load material definitions from TOML files,
//! allowing declarative material creation without code changes.

use super::{
    DescriptorBinding, MaterialDescriptor, MaterialValue, RenderState, ShaderSource, UniformType,
};
use serde::Deserialize;
use std::{collections::HashMap, path::Path};

/// Errors that can occur during material asset loading
#[derive(Debug)]
pub enum AssetError {
    IoError(std::io::Error),
    TomlParseError(toml::de::Error),
    InvalidMaterial(String),
    InvalidShaderPath(String),
    MissingField(String),
}

impl std::fmt::Display for AssetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AssetError::IoError(e) => write!(f, "IO error: {}", e),
            AssetError::TomlParseError(e) => write!(f, "TOML parse error: {}", e),
            AssetError::InvalidMaterial(msg) => write!(f, "Invalid material: {}", msg),
            AssetError::InvalidShaderPath(path) => write!(f, "Invalid shader path: {}", path),
            AssetError::MissingField(field) => write!(f, "Missing required field: {}", field),
        }
    }
}

impl std::error::Error for AssetError {}

impl From<std::io::Error> for AssetError {
    fn from(e: std::io::Error) -> Self {
        AssetError::IoError(e)
    }
}

impl From<toml::de::Error> for AssetError {
    fn from(e: toml::de::Error) -> Self {
        AssetError::TomlParseError(e)
    }
}

/// TOML representation of a material definition
#[derive(Debug, Deserialize)]
struct MaterialToml {
    name: String,
    shaders: ShaderToml,
    bindings: BindingsToml,
    #[serde(default)]
    parameters: HashMap<String, ValueToml>,
    #[serde(default)]
    render_state: RenderStateToml,
}

/// TOML representation of shader definitions
#[derive(Debug, Deserialize)]
struct ShaderToml {
    vertex: ShaderPathToml,
    fragment: ShaderPathToml,
}

/// TOML representation of a shader path (file or string)
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ShaderPathToml {
    File(String),
    Inline { code: String },
}

/// TOML representation of descriptor bindings
#[derive(Debug, Deserialize)]
struct BindingsToml {
    #[serde(default)]
    uniforms: HashMap<String, UniformToml>,
    #[serde(default)]
    textures: HashMap<String, TextureToml>,
}

/// TOML representation of a uniform binding
#[derive(Debug, Deserialize)]
struct UniformToml {
    #[serde(rename = "type")]
    ty: String,
    #[serde(default)]
    count: Option<usize>,
}

/// TOML representation of a texture binding
#[derive(Debug, Deserialize)]
struct TextureToml {
    binding: u32,
    #[serde(default)]
    name: Option<String>,
}

/// TOML representation of a parameter value
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ValueToml {
    Float(f32),
    Vec2([f32; 2]),
    Vec3([f32; 3]),
    Vec4([f32; 4]),
}

/// TOML representation of render state
#[derive(Debug, Deserialize, Default)]
struct RenderStateToml {
    #[serde(default = "default_true")]
    depth_test: bool,

    #[serde(default = "default_true")]
    depth_write: bool,

    #[serde(default = "default_true")]
    cull_backfaces: bool,

    #[serde(default)]
    alpha_blending: bool,
}

fn default_true() -> bool {
    true
}

impl MaterialToml {
    /// Convert TOML representation to MaterialDescriptor
    pub fn into_descriptor(self, base_dir: &Path) -> Result<MaterialDescriptor, AssetError> {
        // Parse vertex shader
        let vertex_shader = match self.shaders.vertex {
            ShaderPathToml::File(path) => {
                let full_path = base_dir.join(&path);
                if !full_path.exists() {
                    return Err(AssetError::InvalidShaderPath(format!(
                        "Vertex shader not found: {}",
                        full_path.display()
                    )));
                }
                ShaderSource::WgslFile(full_path)
            }
            ShaderPathToml::Inline { code } => ShaderSource::WgslString(code),
        };

        // Parse fragment shader
        let fragment_shader = match self.shaders.fragment {
            ShaderPathToml::File(path) => {
                let full_path = base_dir.join(&path);
                if !full_path.exists() {
                    return Err(AssetError::InvalidShaderPath(format!(
                        "Fragment shader not found: {}",
                        full_path.display()
                    )));
                }
                ShaderSource::WgslFile(full_path)
            }
            ShaderPathToml::Inline { code } => ShaderSource::WgslString(code),
        };

        // Build descriptor
        let mut descriptor = MaterialDescriptor::new(self.name, vertex_shader, fragment_shader);

        // Add uniform bindings
        for (name, uniform) in self.bindings.uniforms {
            let ty = parse_uniform_type(&uniform.ty, uniform.count.unwrap_or(1))?;
            descriptor = descriptor.with_binding(DescriptorBinding::Uniform { name, ty });
        }

        // Add texture bindings
        for (slot, texture) in self.bindings.textures {
            let name = texture.name.unwrap_or_else(|| slot.clone());
            // Determine if this is a separate binding or combined
            // For now, we'll use CombinedImageSampler as default
            descriptor = descriptor.with_binding(DescriptorBinding::CombinedImageSampler {
                binding: texture.binding,
                name,
            });
        }

        // Add parameters
        for (name, value) in self.parameters {
            let material_value = match value {
                ValueToml::Float(v) => MaterialValue::Float(v),
                ValueToml::Vec2(v) => MaterialValue::Vec2(v),
                ValueToml::Vec3(v) => MaterialValue::Vec3(v),
                ValueToml::Vec4(v) => MaterialValue::Vec4(v),
            };
            descriptor.parameters.insert(name, material_value);
        }

        // Set render state
        descriptor.render_state = RenderState {
            depth_test: self.render_state.depth_test,
            depth_write: self.render_state.depth_write,
            cull_backfaces: self.render_state.cull_backfaces,
            alpha_blending: self.render_state.alpha_blending,
        };

        Ok(descriptor)
    }
}

/// Parse uniform type from string
fn parse_uniform_type(ty: &str, count: usize) -> Result<UniformType, AssetError> {
    match ty.to_lowercase().as_str() {
        "mat4" | "mat4x4" => Ok(UniformType::Mat4 { count }),
        "vec4" => Ok(UniformType::Vec4),
        "vec3" => Ok(UniformType::Vec3),
        "vec2" => Ok(UniformType::Vec2),
        "float" | "f32" => Ok(UniformType::Float),
        "color" => Ok(UniformType::Color),
        _ => Err(AssetError::InvalidMaterial(format!(
            "Unknown uniform type: {}",
            ty
        ))),
    }
}

/// Load a material descriptor from a TOML file
pub fn load_material_from_file(path: &Path) -> Result<MaterialDescriptor, AssetError> {
    let content = std::fs::read_to_string(path)?;
    let toml: MaterialToml = toml::from_str(&content)?;

    // Get the base directory for resolving relative paths
    let base_dir = path.parent().unwrap_or(Path::new("."));

    toml.into_descriptor(base_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_material() {
        let toml_str = r#"
name = "Test Material"

[shaders]
vertex = "test.vert.wgsl"
fragment = "test.frag.wgsl"

[bindings.uniforms.transform]
type = "mat4"
count = 3

[bindings.uniforms.color]
type = "color"

[parameters]
color = [1.0, 0.5, 0.0, 1.0]

[render_state]
depth_test = true
depth_write = true
cull_backfaces = true
"#;

        let toml: MaterialToml = toml::from_str(toml_str).unwrap();
        assert_eq!(toml.name, "Test Material");
        assert_eq!(toml.bindings.uniforms.len(), 2);
        assert_eq!(toml.parameters.len(), 1);
    }

    #[test]
    fn test_parse_inline_shader() {
        let toml_str = r#"
name = "Inline Shader Material"

[shaders.vertex]
code = "@vertex fn main() -> @builtin(position) vec4f { return vec4f(0.0); }"

[shaders.fragment]
code = "@fragment fn main() -> @location(0) vec4f { return vec4f(1.0); }"

[bindings.uniforms]
transform = { type = "mat4", count = 3 }

[render_state]
"#;

        let toml: MaterialToml = toml::from_str(toml_str).unwrap();
        assert_eq!(toml.name, "Inline Shader Material");
    }
}
