use ash::vk::{self};

#[derive(Clone, Copy, PartialEq, Debug, Hash)]
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
    RGBA8u,   // Four unsigned bytes, not normalized
    RGBA8un,  // Four unsigned bytes, normalized to [0, 1]
    RGBA16un, // Four unsigned shorts, normalized to [0, 1]
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
            VertexFormat::RGBA8u => vk::Format::R8G8B8A8_UINT,
            VertexFormat::RGBA8un => vk::Format::R8G8B8A8_UNORM,
            VertexFormat::RGBA16un => vk::Format::R16G16B16A16_UNORM,
        }
    }

    pub fn get_offset(&self) -> u32 {
        use VertexFormat::*;
        match self {
            R32u | R32i | R32f => 4,
            RG32u | RG32i | RG32f => 8,
            RGB32u | RGB32i | RGB32f => 12,
            RGBA32u | RGBA32i | RGBA32f => 16,
            RGBA16u | RGBA16un => 8, // 4 x u16 = 8 bytes
            RGBA8u | RGBA8un => 4,   // 4 x u8 = 4 bytes
        }
    }
}
#[derive(Clone, Debug, Hash)]
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

    pub fn get_soa_descriptions(
        &self,
    ) -> (
        Vec<vk::VertexInputBindingDescription>,
        Vec<vk::VertexInputAttributeDescription>,
    ) {
        let mut bindings = Vec::new();
        let mut attributes = Vec::new();
        for (location, format) in self.formats.iter().enumerate() {
            let stride = format.get_offset();
            bindings.push(
                vk::VertexInputBindingDescription::default()
                    .binding(location as u32)
                    .stride(stride)
                    .input_rate(vk::VertexInputRate::VERTEX),
            );
            attributes.push(
                vk::VertexInputAttributeDescription::default()
                    .binding(location as u32)
                    .location(location as u32)
                    .format(format.get_vk_format())
                    .offset(0),
            );
        }
        (bindings, attributes)
    }
}
