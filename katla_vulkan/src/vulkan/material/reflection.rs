//! Shader reflection for parsing WGSL and extracting uniform layouts.
//!
//! This module uses naga to analyze WGSL shaders and extract:
//! - Struct member information (names, offsets, sizes)
//! - Binding information (uniforms, textures, samplers)
//! - Type information for validation

use naga::{Module, Type, TypeInner, Handle};
use std::collections::HashMap;

/// Error types for shader reflection
#[derive(Debug)]
pub enum ReflectionError {
    WgslParseError(String),
    StructNotFound(String),
    MemberNotFound(String),
    InvalidType(String),
    UnsupportedType(String),
}

impl std::fmt::Display for ReflectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReflectionError::WgslParseError(e) => write!(f, "WGSL parse error: {}", e),
            ReflectionError::StructNotFound(name) => write!(f, "Struct '{}' not found", name),
            ReflectionError::MemberNotFound(name) => write!(f, "Member '{}' not found", name),
            ReflectionError::InvalidType(msg) => write!(f, "Invalid type: {}", msg),
            ReflectionError::UnsupportedType(ty) => write!(f, "Unsupported type: {}", ty),
        }
    }
}

impl std::error::Error for ReflectionError {}

/// Information about a single struct member
#[derive(Debug, Clone)]
pub struct StructMember {
    pub name: String,
    pub offset: usize,
    pub size: usize,
    pub ty: MemberType,
}

/// Type of a struct member
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MemberType {
    Mat4,
    Vec4,
    Vec3,
    Vec2,
    Float,
    Color,
}

impl MemberType {
    /// Get the size in bytes for this type
    pub fn size(&self) -> usize {
        match self {
            MemberType::Mat4 => 64,  // 4x4 matrix = 16 floats = 64 bytes
            MemberType::Vec4 => 16,   // 4 floats
            MemberType::Vec3 => 12,   // 3 floats
            MemberType::Vec2 => 8,    // 2 floats
            MemberType::Float => 4,   // 1 float
            MemberType::Color => 16,  // RGBA = 4 floats
        }
    }

    /// Get the alignment requirement for this type
    pub fn alignment(&self) -> usize {
        match self {
            MemberType::Mat4 => 16,  // Mat4 needs 16-byte alignment
            MemberType::Vec4 => 16,
            MemberType::Vec3 => 16,  // Vec3 is treated as Vec4 for alignment
            MemberType::Vec2 => 8,
            MemberType::Float => 4,
            MemberType::Color => 16,
        }
    }
}

/// Layout information for a struct
#[derive(Debug, Clone)]
pub struct StructLayout {
    pub members: Vec<StructMember>,
    pub size: usize,
}

impl StructLayout {
    /// Find a member by name
    pub fn find_member(&self, name: &str) -> Option<&StructMember> {
        self.members.iter().find(|m| m.name == name)
    }

    /// Get the offset of a member by name
    pub fn get_offset(&self, name: &str) -> Option<usize> {
        self.find_member(name).map(|m| m.offset)
    }

    /// Get the size of a member by name
    pub fn get_size(&self, name: &str) -> Option<usize> {
        self.find_member(name).map(|m| m.size)
    }

    /// Get the type of a member by name
    pub fn get_type(&self, name: &str) -> Option<MemberType> {
        self.find_member(name).map(|m| m.ty)
    }
}

/// Information extracted from shader reflection
#[derive(Debug, Clone)]
pub struct ShaderReflection {
    pub structs: HashMap<String, StructLayout>,
    pub has_color_uniform: bool,
    pub needs_separate_bindings: bool,
    pub uniform_buffer_size: usize,
}

impl ShaderReflection {
    /// Parse WGSL shader and extract reflection information
    pub fn from_wgsl(wgsl: &str) -> Result<Self, ReflectionError> {
        // Parse the WGSL shader
        let module = naga::front::wgsl::parse_str(wgsl)
            .map_err(|e| ReflectionError::WgslParseError(format!("{:?}", e)))?;

        // Extract all struct definitions
        let mut structs = HashMap::new();

        for (handle, ty) in module.types.iter() {
            if let TypeInner::Struct { ref members, .. } = ty.inner {
                let layout = Self::analyze_struct(&module, members)?;
                // Try to get the struct name if it exists
                let name = ty.name.clone().unwrap_or_else(|| {
                    format!("struct_{}", handle.index())
                });

                structs.insert(name, layout);
            }
        }

        // Determine if we have a color uniform
        let has_color_uniform = structs.values()
            .any(|layout| layout.members.iter().any(|m| m.name == "color"));

        // For now, assume WGSL shaders need separate bindings
        // In production, we'd analyze the actual bindings
        let needs_separate_bindings = true;

        // Calculate uniform buffer size
        let uniform_buffer_size = structs.values()
            .filter(|layout| layout.members.iter().any(|m| matches!(m.ty, MemberType::Mat4)))
            .map(|layout| layout.size)
            .next()
            .unwrap_or(0);

        Ok(Self {
            structs,
            has_color_uniform,
            needs_separate_bindings,
            uniform_buffer_size,
        })
    }

    /// Analyze a WGSL struct and calculate member offsets and sizes
    fn analyze_struct(
        module: &Module,
        members: &[naga::StructMember],
    ) -> Result<StructLayout, ReflectionError> {
        let mut layout_members = Vec::new();
        let mut offset = 0;

        for member in members {
            let name = member.name.clone().unwrap_or_else(|| {
                format!("member_{}", member.name.as_deref().unwrap_or("unknown"))
            });

            // Get the type information
            let member_ty = Self::get_member_type(module, member.ty)?;

            // Calculate alignment padding
            let alignment = member_ty.alignment();
            if offset % alignment != 0 {
                offset += alignment - (offset % alignment);
            }

            let size = member_ty.size();

            layout_members.push(StructMember {
                name: name.clone(),
                offset,
                size,
                ty: member_ty,
            });

            offset += size;
        }

        // Pad the struct size to 16-byte alignment (std140)
        if offset % 16 != 0 {
            offset += 16 - (offset % 16);
        }

        Ok(StructLayout {
            members: layout_members,
            size: offset,
        })
    }

    /// Determine the type of a struct member
    fn get_member_type(module: &Module, ty: Handle<Type>) -> Result<MemberType, ReflectionError> {
        let type_inner = &module.types[ty].inner;

        match type_inner {
            TypeInner::Matrix { columns, rows, .. } => {
                if columns == &naga::VectorSize::Quad && rows == &naga::VectorSize::Quad {
                    Ok(MemberType::Mat4)
                } else {
                    Err(ReflectionError::UnsupportedType(format!("Matrix {:?}x{:?}", columns, rows)))
                }
            }
            TypeInner::Vector { size, .. } => {
                match size {
                    naga::VectorSize::Quad => Ok(MemberType::Vec4),
                    naga::VectorSize::Tri => Ok(MemberType::Vec3),
                    naga::VectorSize::Bi => Ok(MemberType::Vec2),
                }
            }
            TypeInner::Scalar { .. } => Ok(MemberType::Float),
            _ => Err(ReflectionError::UnsupportedType(format!("{:?}", type_inner))),
        }
    }

    /// Get the struct layout for a given struct name
    pub fn get_struct(&self, name: &str) -> Option<&StructLayout> {
        self.structs.get(name)
    }

    /// Get the "Uniforms" struct (common name for WGSL uniform structs)
    pub fn get_uniforms_struct(&self) -> Option<&StructLayout> {
        // Try common names for uniform structs
        for name in &["Uniforms", "Uniform", "UBO", "Constants"] {
            if let Some(layout) = self.structs.get(*name) {
                return Some(layout);
            }
        }
        // Fall back to first struct with mat4 members (likely the uniform buffer)
        self.structs.values()
            .find(|layout| layout.members.iter().any(|m| matches!(m.ty, MemberType::Mat4)))
    }
}
