use ash::vk;
use std::{collections::HashMap, path::PathBuf};

/// Source for shader code
#[derive(Clone, Debug)]
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

/// Descriptor binding types
#[derive(Clone, Debug)]
pub enum DescriptorBinding {
    /// Uniform buffer binding
    Uniform { name: String, ty: UniformType },

    /// Sampled image binding
    SampledImage { binding: u32, name: String },

    /// Sampler binding
    Sampler { binding: u32, name: String },

    /// Combined image sampler binding
    CombinedImageSampler { binding: u32, name: String },
}

impl DescriptorBinding {
    pub fn binding(&self) -> Option<u32> {
        match self {
            DescriptorBinding::SampledImage { binding, .. } => Some(*binding),
            DescriptorBinding::Sampler { binding, .. } => Some(*binding),
            DescriptorBinding::CombinedImageSampler { binding, .. } => Some(*binding),
            DescriptorBinding::Uniform { .. } => None,
        }
    }

    pub fn descriptor_type(&self) -> Option<vk::DescriptorType> {
        match self {
            DescriptorBinding::SampledImage { .. } => Some(vk::DescriptorType::SAMPLED_IMAGE),
            DescriptorBinding::Sampler { .. } => Some(vk::DescriptorType::SAMPLER),
            DescriptorBinding::CombinedImageSampler { .. } => {
                Some(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            }
            DescriptorBinding::Uniform { .. } => None,
        }
    }

    pub fn name(&self) -> String {
        match self {
            DescriptorBinding::Uniform { name, .. } => name.clone(),
            DescriptorBinding::SampledImage { name, .. } => name.clone(),
            DescriptorBinding::Sampler { name, .. } => name.clone(),
            DescriptorBinding::CombinedImageSampler { name, .. } => name.clone(),
        }
    }
}

/// Uniform buffer data types
#[derive(Clone, Debug)]
pub enum UniformType {
    /// 4x4 matrix (64 bytes)
    Mat4 { count: usize },

    /// 4-component float vector (16 bytes)
    Vec4,

    /// 3-component float vector (12 bytes)
    Vec3,

    /// 2-component float vector (8 bytes)
    Vec2,

    /// Single float (4 bytes)
    Float,

    /// 4-component color (16 bytes)
    Color,
}

impl UniformType {
    pub fn size(&self) -> usize {
        match self {
            UniformType::Mat4 { count } => 64 * count,
            UniformType::Vec4 => 16,
            UniformType::Vec3 => 12,
            UniformType::Vec2 => 8,
            UniformType::Float => 4,
            UniformType::Color => 16,
        }
    }
}

/// Render state configuration
#[derive(Clone, Debug, Default)]
pub struct RenderState {
    pub depth_test: bool,
    pub depth_write: bool,
    pub cull_backfaces: bool,
    pub alpha_blending: bool,
}

/// Material value for parameters
#[derive(Clone, Debug)]
pub enum MaterialValue {
    Float(f32),
    Vec2([f32; 2]),
    Vec3([f32; 3]),
    Vec4([f32; 4]),
    Color([f32; 4]), // RGBA color values
}

impl MaterialValue {
    pub fn size(&self) -> usize {
        match self {
            MaterialValue::Float(_) => 4,
            MaterialValue::Vec2(_) => 8,
            MaterialValue::Vec3(_) => 12,
            MaterialValue::Vec4(_) => 16,
            MaterialValue::Color(_) => 16,
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        match self {
            MaterialValue::Float(v) => bytemuck::cast_slice(&v.to_le_bytes()).to_vec(),
            MaterialValue::Vec2(v) => bytemuck::cast_slice(v).to_vec(),
            MaterialValue::Vec3(v) => bytemuck::cast_slice(v).to_vec(),
            MaterialValue::Vec4(v) => bytemuck::cast_slice(v).to_vec(),
            MaterialValue::Color(c) => bytemuck::cast_slice(c).to_vec(),
        }
    }
}

/// Complete material descriptor
#[derive(Clone)]
pub struct MaterialDescriptor {
    pub name: String,
    pub vertex_shader: ShaderSource,
    pub fragment_shader: ShaderSource,
    pub bindings: Vec<DescriptorBinding>,
    pub parameters: HashMap<String, MaterialValue>,
    pub render_state: RenderState,
}

impl MaterialDescriptor {
    /// Create a new material descriptor
    pub fn new(
        name: impl Into<String>,
        vertex_shader: ShaderSource,
        fragment_shader: ShaderSource,
    ) -> Self {
        Self {
            name: name.into(),
            vertex_shader,
            fragment_shader,
            bindings: Vec::new(),
            parameters: HashMap::new(),
            render_state: RenderState::default(),
        }
    }

    /// Add a binding to the descriptor
    pub fn with_binding(mut self, binding: DescriptorBinding) -> Self {
        self.bindings.push(binding);
        self
    }

    /// Add multiple bindings
    pub fn with_bindings(mut self, bindings: Vec<DescriptorBinding>) -> Self {
        self.bindings = bindings;
        self
    }

    /// Set a parameter value
    pub fn with_parameter(mut self, name: impl Into<String>, value: MaterialValue) -> Self {
        self.parameters.insert(name.into(), value);
        self
    }

    /// Set render state
    pub fn with_render_state(mut self, state: RenderState) -> Self {
        self.render_state = state;
        self
    }

    /// Calculate the total uniform buffer size required
    pub fn uniform_buffer_size(&self) -> usize {
        self.bindings
            .iter()
            .filter_map(|b| {
                if let DescriptorBinding::Uniform { ty, .. } = b {
                    Some(ty.size())
                } else {
                    None
                }
            })
            .sum()
    }

    /// Check if this material needs separate texture/sampler bindings (WGSL)
    pub fn needs_separate_bindings(&self) -> bool {
        self.bindings.iter().any(|b| {
            matches!(
                b,
                DescriptorBinding::SampledImage { .. } | DescriptorBinding::Sampler { .. }
            )
        })
    }

    /// Check if this material needs a color uniform
    pub fn has_color_uniform(&self) -> bool {
        self.bindings
            .iter()
            .any(|b| matches!(b, DescriptorBinding::Uniform { name, .. } if name == "color"))
    }
}

/// Errors that can occur during material descriptor operations
#[derive(Debug)]
pub enum MaterialError {
    ShaderLoadFailed(PathBuf, std::io::Error),
    InvalidDescriptor(String),
    MissingBinding(String),
    BindingConflict {
        binding: u32,
        existing: String,
        attempted: String,
    },
    InvalidParameter(String),
    ShaderCompilationFailed {
        stage: ShaderStage,
        error: String,
    },
}

impl std::fmt::Display for MaterialError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MaterialError::ShaderLoadFailed(path, e) => {
                write!(f, "Failed to load shader from {}: {}", path.display(), e)
            }
            MaterialError::InvalidDescriptor(msg) => {
                write!(f, "Invalid material descriptor: {}", msg)
            }
            MaterialError::MissingBinding(name) => {
                write!(f, "Missing required binding: {}", name)
            }
            MaterialError::BindingConflict {
                binding,
                existing,
                attempted,
            } => {
                write!(
                    f,
                    "Binding {} conflict: '{}' already bound, attempted '{}'",
                    binding, existing, attempted
                )
            }
            MaterialError::InvalidParameter(name) => {
                write!(f, "Invalid parameter '{}'", name)
            }
            MaterialError::ShaderCompilationFailed { stage, error } => {
                write!(
                    f,
                    "Shader compilation failed in {:?} stage: {}",
                    stage, error
                )
            }
        }
    }
}

impl std::error::Error for MaterialError {}

#[derive(Debug, Clone, Copy)]
pub enum ShaderStage {
    Vertex,
    Fragment,
    Compute,
}
