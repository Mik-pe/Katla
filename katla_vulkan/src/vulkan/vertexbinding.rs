use ash::vk::{self};

/// Standard PBR vertex format with position, normal, tangent, and UV
pub fn get_pbr_vertex_binding() -> VertexBinding {
    VertexBinding {
        formats: vec![
            VertexFormat::RGB32f,  // position
            VertexFormat::RGB32f,  // normal
            VertexFormat::RGBA32f, // tangent
            VertexFormat::RG32f,   // uv
        ],
    }
}

/// Skinned PBR vertex format with skeletal animation support
///
/// Adds joint indices (u16x4) and weights (f32x4) for GPU skinning.
/// Each vertex can be influenced by up to 4 joints.
pub fn get_skinned_vertex_binding() -> VertexBinding {
    VertexBinding {
        formats: vec![
            VertexFormat::RGB32f,   // position (location 0)
            VertexFormat::RGB32f,   // normal (location 1)
            VertexFormat::RGBA32f,  // tangent (location 2)
            VertexFormat::RG32f,    // uv (location 3)
            VertexFormat::RGBA16u,  // joint_indices (location 4) - u16x4, 65k joints max
            VertexFormat::RGBA32f,  // joint_weights (location 5)
        ],
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum VertexFormat {
    R32u,
    R32i,
    R32f,
    RG32u,
    RG32i,
    RG32f,
    RGB32u,
    RGB32i,
    RGB32f,
    RGBA32u,
    RGBA32i,
    RGBA32f,
    RGBA16u,  // For joint indices (u16 x 4)
}

impl VertexFormat {
    pub fn get_vk_format(&self) -> vk::Format {
        match self {
            VertexFormat::R32u => vk::Format::R32_UINT,
            VertexFormat::R32i => vk::Format::R32_SINT,
            VertexFormat::R32f => vk::Format::R32_SFLOAT,
            VertexFormat::RG32u => vk::Format::R32G32_UINT,
            VertexFormat::RG32i => vk::Format::R32G32_SINT,
            VertexFormat::RG32f => vk::Format::R32G32_SFLOAT,
            VertexFormat::RGB32u => vk::Format::R32G32B32_UINT,
            VertexFormat::RGB32i => vk::Format::R32G32B32_SINT,
            VertexFormat::RGB32f => vk::Format::R32G32B32_SFLOAT,
            VertexFormat::RGBA32u => vk::Format::R32G32B32A32_UINT,
            VertexFormat::RGBA32i => vk::Format::R32G32B32A32_SINT,
            VertexFormat::RGBA32f => vk::Format::R32G32B32A32_SFLOAT,
            VertexFormat::RGBA16u => vk::Format::R16G16B16A16_UINT,
        }
    }

    pub fn get_offset(&self) -> u32 {
        use VertexFormat::*;
        match self {
            R32u | R32i | R32f => 4,
            RG32u | RG32i | RG32f => 8,
            RGB32u | RGB32i | RGB32f => 12,
            RGBA32u | RGBA32i | RGBA32f => 16,
            RGBA16u => 8,  // 4 x u16 = 8 bytes
        }
    }
}
pub struct VertexBinding {
    pub formats: Vec<VertexFormat>,
}

impl Clone for VertexBinding {
    fn clone(&self) -> Self {
        Self {
            formats: self.formats.clone(),
        }
    }
}

impl VertexBinding {
    fn get_stride(&self) -> u32 {
        self.formats.iter().map(|f| f.get_offset()).sum()
    }

    pub fn get_binding_desc(&self, binding: u32) -> vk::VertexInputBindingDescription {
        vk::VertexInputBindingDescription::default()
            .binding(binding)
            .stride(self.get_stride())
            .input_rate(vk::VertexInputRate::VERTEX)
    }

    pub fn get_attribute_desc(&self, binding: u32) -> Vec<vk::VertexInputAttributeDescription> {
        let mut current_offset = 0;
        let mut location = 0;
        self.formats
            .iter()
            .map(|format| {
                let out = vk::VertexInputAttributeDescription::default()
                    .binding(binding)
                    .location(location)
                    .format(format.get_vk_format())
                    .offset(current_offset);
                current_offset += format.get_offset();
                location += 1;
                out
            })
            .collect()
    }
}
