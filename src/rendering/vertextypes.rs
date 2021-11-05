use katla_vulkan::vertexbinding::{VertexBinding, VertexFormat};

#[repr(C)]
#[derive(Default, Debug, Clone)]
pub struct VertexPosition {
    pub position: [f32; 3],
}
// descriptors!(VertexPosition, position);

impl VertexPosition {
    pub fn get_vertex_binding() -> VertexBinding {
        VertexBinding {
            formats: vec![VertexFormat::R32G32B32_SFLOAT],
        }
    }
}
// vulkano::impl_vertex!(VertexPosition, position);

#[repr(C)]
#[derive(Default, Debug, Clone)]
pub struct VertexPos2Color {
    pub position: [f32; 2],
    pub color: [f32; 3],
}
// descriptors!(VertexPos2Color, position, color);

// vulkano::impl_vertex!(VertexPos2Color, position, color);
#[repr(C)]
#[derive(Default, Debug, Clone)]
pub struct VertexNormal {
    pub position: [f32; 3],
    pub normal: [f32; 3],
}

impl VertexNormal {
    pub fn get_vertex_binding() -> VertexBinding {
        VertexBinding {
            formats: vec![
                VertexFormat::R32G32B32_SFLOAT,
                VertexFormat::R32G32B32_SFLOAT,
            ],
        }
    }
}

#[repr(C)]
#[derive(Default, Debug, Clone)]
pub struct VertexNormalTangent {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub tangent: [f32; 4],
}
// vulkano::impl_vertex!(VertexNormalTangent, position, normal, tangent);

#[repr(C)]
#[derive(Default, Debug, Clone)]
pub struct VertexPBR {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub tangent: [f32; 4],
    pub tex_coord0: [f32; 2],
}

impl VertexPBR {
    pub fn get_vertex_binding() -> VertexBinding {
        VertexBinding {
            formats: vec![
                VertexFormat::R32G32B32_SFLOAT,
                VertexFormat::R32G32B32_SFLOAT,
                VertexFormat::R32G32B32A32_SFLOAT,
                VertexFormat::R32G32_SFLOAT,
            ],
        }
    }
}
// impl VertexBinding for VertexPBR {
//     fn get_binding_desc<'a>(binding: u32) -> vk::VertexInputBindingDescriptionBuilder<'a> {
//         vk::VertexInputBindingDescription::builder()
//             .binding(binding)
//             .stride(std::mem::size_of::<Self>() as u32)
//             .input_rate(vk::VertexInputRate::VERTEX)
//     }

//     fn get_attribute_desc(binding: u32) -> Vec<vk::VertexInputAttributeDescription> {
//         vec![
//             vk::VertexInputAttributeDescription::builder()
//                 .binding(binding)
//                 .location(0)
//                 .format(vk::Format::R32G32B32_SFLOAT)
//                 .offset(0)
//                 .build(),
//             vk::VertexInputAttributeDescription::builder()
//                 .binding(binding)
//                 .location(1)
//                 .format(vk::Format::R32G32B32_SFLOAT)
//                 .offset(12)
//                 .build(),
//             vk::VertexInputAttributeDescription::builder()
//                 .binding(binding)
//                 .location(2)
//                 .format(vk::Format::R32G32B32A32_SFLOAT)
//                 .offset(24)
//                 .build(),
//             vk::VertexInputAttributeDescription::builder()
//                 .binding(binding)
//                 .location(3)
//                 .format(vk::Format::R32G32_SFLOAT)
//                 .offset(40)
//                 .build(),
//         ]
//     }
// }
