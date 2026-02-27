//! SOA (Structure of Arrays) vertex attribute types and bindings.
//!
//! This module provides the foundation for separate attribute buffers,
//! enabling depth-only passes, shadow mapping, and flexible rendering pipelines.

use crate::vulkan::vertexbinding::VertexFormat;
use ash::vk;

/// Semantic attribute types for vertex data.
///
/// Each attribute has a default Vulkan location that follows
/// the standard PBR layout convention.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AttributeType {
    /// Position attribute (vec3<f32>) - Location 0
    Position,
    /// Normal attribute (vec3<f32>) - Location 1
    Normal,
    /// Tangent attribute (vec4<f32>) - Location 2
    Tangent,
    /// Texture coordinate 0 (vec2<f32>) - Location 3
    TexCoord0,
    /// Texture coordinate 1 (vec2<f32>) - Location 4
    TexCoord1,
    /// Color attribute 0 (vec4<f32>) - Location 5
    Color0,
    /// Joint indices for skeletal animation (uvec4) - Location 6
    /// Uses RGBA16u format (u16 x 4)
    JointIndices,
    /// Joint weights for skeletal animation (vec4<f32>) - Location 7
    JointWeights,
}

impl AttributeType {
    /// Get the default Vulkan location for this attribute type.
    ///
    /// Follows the standard PBR vertex layout convention:
    /// - Position: 0
    /// - Normal: 1
    /// - Tangent: 2
    /// - TexCoord0: 3
    /// - TexCoord1: 4
    /// - Color0: 5
    /// - JointIndices: 6
    /// - JointWeights: 7
    pub fn default_location(&self) -> u32 {
        match self {
            AttributeType::Position => 0,
            AttributeType::Normal => 1,
            AttributeType::Tangent => 2,
            AttributeType::TexCoord0 => 3,
            AttributeType::JointIndices => 4, // Match skinned shader location
            AttributeType::JointWeights => 5, // Match skinned shader location
            AttributeType::TexCoord1 => 6,
            AttributeType::Color0 => 7,
        }
    }

    /// Get the recommended VertexFormat for this attribute type.
    pub fn default_format(&self) -> VertexFormat {
        match self {
            AttributeType::Position => VertexFormat::RGB32f,
            AttributeType::Normal => VertexFormat::RGB32f,
            AttributeType::Tangent => VertexFormat::RGBA32f,
            AttributeType::TexCoord0 => VertexFormat::RG32f,
            AttributeType::TexCoord1 => VertexFormat::RG32f,
            AttributeType::Color0 => VertexFormat::RGBA32f,
            AttributeType::JointIndices => VertexFormat::RGBA16u,
            AttributeType::JointWeights => VertexFormat::RGBA32f,
        }
    }
}

/// Single attribute buffer with format and Vulkan buffer binding.
///
/// In SOA layout, each attribute type has its own buffer.
/// This struct holds the binding information for one attribute type.
pub struct AttributeBinding {
    pub attr_type: AttributeType,
    pub format: VertexFormat,
    pub(crate) buffer: vk::Buffer,
}

impl AttributeBinding {
    /// Create a new attribute binding.
    pub fn new(attr_type: AttributeType, format: VertexFormat, buffer: vk::Buffer) -> Self {
        Self {
            attr_type,
            format,
            buffer,
        }
    }

    /// Return a wrapper around the underlying Vulkan buffer.
    pub fn wrapped_buffer(&self) -> crate::sync::VkBuffer {
        crate::sync::VkBuffer::new(self.buffer)
    }

    /// Get the Vulkan vertex attribute description for pipeline creation.
    pub(crate) fn get_attribute_desc(&self, binding: u32) -> vk::VertexInputAttributeDescription {
        vk::VertexInputAttributeDescription::default()
            .binding(binding)
            .location(self.attr_type.default_location())
            .format(self.format.get_vk_format())
            .offset(0) // Always 0 for SOA - each buffer starts at beginning
    }

    /// Get the Vulkan binding description for pipeline creation.
    pub(crate) fn get_binding_desc(&self, binding: u32) -> vk::VertexInputBindingDescription {
        vk::VertexInputBindingDescription::default()
            .binding(binding)
            .stride(self.format.get_offset()) // Single element stride
            .input_rate(vk::VertexInputRate::VERTEX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_attribute_type_locations() {
        assert_eq!(AttributeType::Position.default_location(), 0);
        assert_eq!(AttributeType::Normal.default_location(), 1);
        assert_eq!(AttributeType::Tangent.default_location(), 2);
        assert_eq!(AttributeType::TexCoord0.default_location(), 3);
        assert_eq!(AttributeType::JointIndices.default_location(), 4); // Match skinned shader
        assert_eq!(AttributeType::JointWeights.default_location(), 5); // Match skinned shader
        assert_eq!(AttributeType::TexCoord1.default_location(), 6);
        assert_eq!(AttributeType::Color0.default_location(), 7);
    }

    #[test]
    fn test_attribute_type_formats() {
        assert_eq!(
            AttributeType::Position.default_format(),
            VertexFormat::RGB32f
        );
        assert_eq!(AttributeType::Normal.default_format(), VertexFormat::RGB32f);
        assert_eq!(
            AttributeType::JointIndices.default_format(),
            VertexFormat::RGBA16u
        );
        assert_eq!(
            AttributeType::JointWeights.default_format(),
            VertexFormat::RGBA32f
        );
    }
}
