use erupt::vk1_0::*;

#[derive(Default, Debug, Clone)]
pub struct VertexPosition {
    pub position: [f32; 3],
}
impl VertexPosition {
    pub fn get_binding_desc(binding: u32) -> VertexInputBindingDescription {
        VertexInputBindingDescription {
            binding,
            stride: std::mem::size_of::<Self>() as u32,
            input_rate: VertexInputRate::VERTEX,
        }
    }

    pub fn get_attribute_desc<'a>(binding: u32) -> Vec<VertexInputAttributeDescriptionBuilder<'a>> {
        vec![VertexInputAttributeDescriptionBuilder::new()
            .binding(binding)
            .location(0)
            .format(Format::R32G32B32_SFLOAT)
            .offset(0)]
    }
}
// vulkano::impl_vertex!(VertexPosition, position);

#[derive(Default, Debug, Clone)]
pub struct VertexPos2Color {
    pub position: [f32; 2],
    pub color: [f32; 3],
}
// vulkano::impl_vertex!(VertexPos2Color, position, color);

#[derive(Default, Debug, Clone)]
pub struct VertexNormal {
    pub position: [f32; 3],
    pub normal: [f32; 3],
}
// vulkano::impl_vertex!(VertexNormal, position, normal);

#[derive(Default, Debug, Clone)]
pub struct VertexNormalTangent {
    position: [f32; 3],
    normal: [f32; 3],
    tangent: [f32; 4],
}
// vulkano::impl_vertex!(VertexNormalTangent, position, normal, tangent);

#[derive(Default, Debug, Clone)]
pub struct VertexPBR {
    position: [f32; 3],
    normal: [f32; 3],
    tangent: [f32; 4],
    tex_coord0: [f32; 2],
}
// vulkano::impl_vertex!(VertexPBR, position, normal, tangent, tex_coord0);
