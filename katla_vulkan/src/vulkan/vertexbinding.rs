use ash::vk::{self};

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
}

impl VertexFormat {
    fn get_vk_format(&self) -> vk::Format {
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
        }
    }

    fn get_offset(&self) -> u32 {
        use VertexFormat::*;
        match self {
            R32u | R32i | R32f => 4,
            RG32u | RG32i | RG32f => 8,
            RGB32u | RGB32i | RGB32f => 12,
            RGBA32u | RGBA32i | RGBA32f => 16,
        }
    }
}
pub struct VertexBinding {
    pub formats: Vec<VertexFormat>,
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
