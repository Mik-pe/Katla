#[derive(Default, Debug, Clone)]
pub struct VertexPosition {
    position: [f32; 3],
}
vulkano::impl_vertex!(VertexPosition, position);

#[derive(Default, Debug, Clone)]
pub struct VertexPos2Color {
    pub position: [f32; 2],
    pub color: [f32; 3],
}
vulkano::impl_vertex!(VertexPos2Color, position, color);

#[derive(Default, Debug, Clone)]
pub struct VertexNormal {
    position: [f32; 3],
    normal: [f32; 3],
}
vulkano::impl_vertex!(VertexNormal, position, normal);

#[derive(Default, Debug, Clone)]
pub struct VertexNormalTangent {
    position: [f32; 3],
    normal: [f32; 3],
    tangent: [f32; 4],
}
vulkano::impl_vertex!(VertexNormalTangent, position, normal, tangent);

#[derive(Default, Debug, Clone)]
pub struct VertexPBR {
    position: [f32; 3],
    normal: [f32; 3],
    tangent: [f32; 4],
    tex_coord0: [f32; 2],
}
vulkano::impl_vertex!(VertexPBR, position, normal, tangent, tex_coord0);

pub enum VertexType {
    VertexPosition(VertexPosition),
    VertexPos2Color(VertexPos2Color),
    VertexNormal(VertexNormal),
    VertexNormalTangent(VertexNormalTangent),
    VertexPBR(VertexPBR),
}
