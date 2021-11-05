use ash::vk::{self};

pub enum VertexFormat {
    R32_UINT,
    R32_SINT,
    R32_SFLOAT,
    R32G32_UINT,
    R32G32_SINT,
    R32G32_SFLOAT,
    R32G32B32_UINT,
    R32G32B32_SINT,
    R32G32B32_SFLOAT,
    R32G32B32A32_UINT,
    R32G32B32A32_SINT,
    R32G32B32A32_SFLOAT,
}

impl VertexFormat {
    fn get_vk_format(&self) -> vk::Format {
        match self {
            VertexFormat::R32_UINT => vk::Format::R32_UINT,
            VertexFormat::R32_SINT => vk::Format::R32_SINT,
            VertexFormat::R32_SFLOAT => vk::Format::R32_SFLOAT,
            VertexFormat::R32G32_UINT => vk::Format::R32G32_UINT,
            VertexFormat::R32G32_SINT => vk::Format::R32G32_SINT,
            VertexFormat::R32G32_SFLOAT => vk::Format::R32G32_SFLOAT,
            VertexFormat::R32G32B32_UINT => vk::Format::R32G32B32_UINT,
            VertexFormat::R32G32B32_SINT => vk::Format::R32G32B32_SINT,
            VertexFormat::R32G32B32_SFLOAT => vk::Format::R32G32B32_SFLOAT,
            VertexFormat::R32G32B32A32_UINT => vk::Format::R32G32B32A32_UINT,
            VertexFormat::R32G32B32A32_SINT => vk::Format::R32G32B32A32_SINT,
            VertexFormat::R32G32B32A32_SFLOAT => vk::Format::R32G32B32A32_SFLOAT,
        }
    }

    fn get_offset(&self) -> u32 {
        use VertexFormat::*;
        match self {
            R32_UINT | R32_SINT | R32_SFLOAT => 4,
            R32G32_UINT | R32G32_SINT | R32G32_SFLOAT => 8,
            R32G32B32_UINT | R32G32B32_SINT | R32G32B32_SFLOAT => 12,
            R32G32B32A32_UINT | R32G32B32A32_SINT | R32G32B32A32_SFLOAT => 16,
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
        vk::VertexInputBindingDescription::builder()
            .binding(binding)
            .stride(self.get_stride())
            .input_rate(vk::VertexInputRate::VERTEX)
            .build()
    }

    pub fn get_attribute_desc(&self, binding: u32) -> Vec<vk::VertexInputAttributeDescription> {
        let mut current_offset = 0;
        let mut location = 0;
        self.formats
            .iter()
            .map(|format| {
                let out = vk::VertexInputAttributeDescription::builder()
                    .binding(binding)
                    .location(location)
                    .format(format.get_vk_format())
                    .offset(current_offset)
                    .build();
                current_offset += format.get_offset();
                location += 1;
                out
            })
            .collect()
    }
}
