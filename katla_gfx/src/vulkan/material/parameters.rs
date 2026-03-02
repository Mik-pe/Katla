//! Type-safe parameter management for materials.
//!
//! This module provides a container for material parameters with type-safe setters
//! and automatic buffer generation.

use super::{
    reflection::{MemberType, ShaderReflection, StructLayout},
    MaterialDescriptor, MaterialValue,
};
use std::collections::HashMap;

/// Error types for parameter operations
#[derive(Debug)]
pub enum ParameterError {
    StructNotFound(String),
    MemberNotFound(String),
    TypeMismatch {
        member: String,
        expected: String,
        got: String,
    },
    ReflectionError(String),
}

impl std::fmt::Display for ParameterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParameterError::StructNotFound(name) => {
                write!(f, "Struct '{}' not found in shader reflection", name)
            }
            ParameterError::MemberNotFound(name) => {
                write!(f, "Parameter '{}' not found in uniform struct", name)
            }
            ParameterError::TypeMismatch {
                member,
                expected,
                got,
            } => {
                write!(
                    f,
                    "Type mismatch for parameter '{}': expected {}, got {}",
                    member, expected, got
                )
            }
            ParameterError::ReflectionError(msg) => {
                write!(f, "Reflection error: {}", msg)
            }
        }
    }
}

impl std::error::Error for ParameterError {}

/// Container for material parameters with type-safe access
#[derive(Clone)]
pub struct MaterialParameters {
    descriptor: MaterialDescriptor,
    reflection: ShaderReflection,
    values: HashMap<String, MaterialValue>,
}

impl MaterialParameters {
    /// Create a new parameter container from a descriptor and reflection
    pub fn new(descriptor: MaterialDescriptor, reflection: ShaderReflection) -> Self {
        let values = descriptor.parameters.clone();

        Self {
            descriptor,
            reflection,
            values,
        }
    }

    /// Get the uniforms struct layout
    fn get_uniforms_layout(&self) -> Result<&StructLayout, ParameterError> {
        self.reflection
            .get_uniforms_struct()
            .ok_or_else(|| ParameterError::StructNotFound("Uniforms".to_string()))
    }

    /// Set a float parameter
    pub fn set_float(&mut self, name: &str, value: f32) -> Result<(), ParameterError> {
        let layout = self.get_uniforms_layout()?;
        let member = layout
            .find_member(name)
            .ok_or_else(|| ParameterError::MemberNotFound(name.to_string()))?;

        if member.ty != MemberType::Float {
            return Err(ParameterError::TypeMismatch {
                member: name.to_string(),
                expected: format!("{:?}", member.ty),
                got: "Float".to_string(),
            });
        }

        self.values
            .insert(name.to_string(), MaterialValue::Float(value));
        Ok(())
    }

    /// Set a Vec2 parameter
    pub fn set_vec2(&mut self, name: &str, value: [f32; 2]) -> Result<(), ParameterError> {
        let layout = self.get_uniforms_layout()?;
        let member = layout
            .find_member(name)
            .ok_or_else(|| ParameterError::MemberNotFound(name.to_string()))?;

        if member.ty != MemberType::Vec2 {
            return Err(ParameterError::TypeMismatch {
                member: name.to_string(),
                expected: format!("{:?}", member.ty),
                got: "Vec2".to_string(),
            });
        }

        self.values
            .insert(name.to_string(), MaterialValue::Vec2(value));
        Ok(())
    }

    /// Set a Vec3 parameter
    pub fn set_vec3(&mut self, name: &str, value: [f32; 3]) -> Result<(), ParameterError> {
        let layout = self.get_uniforms_layout()?;
        let member = layout
            .find_member(name)
            .ok_or_else(|| ParameterError::MemberNotFound(name.to_string()))?;

        if member.ty != MemberType::Vec3 {
            return Err(ParameterError::TypeMismatch {
                member: name.to_string(),
                expected: format!("{:?}", member.ty),
                got: "Vec3".to_string(),
            });
        }

        self.values
            .insert(name.to_string(), MaterialValue::Vec3(value));
        Ok(())
    }

    /// Set a Vec4 parameter
    pub fn set_vec4(&mut self, name: &str, value: [f32; 4]) -> Result<(), ParameterError> {
        let layout = self.get_uniforms_layout()?;
        let member = layout
            .find_member(name)
            .ok_or_else(|| ParameterError::MemberNotFound(name.to_string()))?;

        if member.ty != MemberType::Vec4 {
            return Err(ParameterError::TypeMismatch {
                member: name.to_string(),
                expected: format!("{:?}", member.ty),
                got: "Vec4".to_string(),
            });
        }

        self.values
            .insert(name.to_string(), MaterialValue::Vec4(value));
        Ok(())
    }

    /// Set a Color parameter
    pub fn set_color(&mut self, name: &str, value: [f32; 4]) -> Result<(), ParameterError> {
        let layout = self.get_uniforms_layout()?;
        let member = layout
            .find_member(name)
            .ok_or_else(|| ParameterError::MemberNotFound(name.to_string()))?;

        if member.ty != MemberType::Color {
            return Err(ParameterError::TypeMismatch {
                member: name.to_string(),
                expected: format!("{:?}", member.ty),
                got: "Color".to_string(),
            });
        }

        self.values
            .insert(name.to_string(), MaterialValue::Color(value));
        Ok(())
    }

    /// Get a parameter value
    pub fn get(&self, name: &str) -> Option<&MaterialValue> {
        self.values.get(name)
    }

    /// Get the reflection information
    pub fn reflection(&self) -> &ShaderReflection {
        &self.reflection
    }

    /// Get the descriptor
    pub fn descriptor(&self) -> &MaterialDescriptor {
        &self.descriptor
    }

    /// Generate the uniform buffer data from current parameter values
    ///
    /// This creates a properly aligned std140 buffer with all parameter values
    /// at their correct offsets.
    pub fn to_bytes(&self) -> Result<Vec<u8>, ParameterError> {
        let layout = self.get_uniforms_layout()?;
        let mut buffer = vec![0u8; layout.size];

        for member in &layout.members {
            if let Some(value) = self.values.get(&member.name) {
                let value_bytes = value.to_bytes();
                let offset = member.offset;

                // Ensure we don't write past the buffer
                if offset + value_bytes.len() <= buffer.len() {
                    buffer[offset..offset + value_bytes.len()].copy_from_slice(&value_bytes);
                }
            }
        }

        Ok(buffer)
    }

    /// Get the total size of the uniform buffer
    pub fn buffer_size(&self) -> Result<usize, ParameterError> {
        let layout = self.get_uniforms_layout()?;
        Ok(layout.size)
    }
}

// Note: Tests for parameter validation are tested through the example
// programs and integration tests with actual WGSL shaders.
