//! SOA (Structure of Arrays) vertex attribute types and bindings.
//!
//! This module provides the foundation for separate attribute buffers,
//! enabling depth-only passes, shadow mapping, and flexible rendering pipelines.

pub use crate::vertex::AttributeType;
use crate::vulkan::vertexbinding::VertexFormat;
use ash::vk;

/// Default Vulkan location for each attribute type.
impl AttributeType {
    pub fn default_location(&self) -> u32 {
        match self {
            AttributeType::Position => 0,
            AttributeType::Normal => 1,
            AttributeType::Tangent => 2,
            AttributeType::TexCoord0 => 3,
            AttributeType::JointIndices => 4,
            AttributeType::JointWeights => 5,
            AttributeType::TexCoord1 => 6,
            AttributeType::Color0 => 7,
        }
    }

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
