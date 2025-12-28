use std::{path::PathBuf, rc::Rc};

use katla_ecs::World;
use katla_math::{Transform, Vec3};
use katla_vulkan::{MaterialBuilder, RenderPass, VulkanContext};

use crate::{
    application::Model,
    entities::ModelEntity,
    rendering::{Material, Mesh, VertexPBR},
};
const FRAG_SHADER: &[u8] = include_bytes!("../../../../resources/shaders/model_no_tex.frag.spv");

pub fn create_cube_vertices() -> Vec<VertexPBR> {
    let mut vertices = Vec::new();
    for i in 0..=1 {
        let z = if i % 2 == 0 { 0.5 } else { -0.5 };
        vertices.push(VertexPBR::new(
            [-0.5, -0.5, z],
            [0.0, 0.0, 1.0],
            [0.5, 0.5, 0.0, 0.0],
            [0.0, 0.0],
        ));
        vertices.push(VertexPBR::new(
            [-0.5, 0.5, z],
            [0.0, 0.0, 1.0],
            [0.5, 0.5, 0.0, 0.0],
            [0.0, 0.0],
        ));
        vertices.push(VertexPBR::new(
            [0.5, 0.5, z],
            [0.0, 0.0, 1.0],
            [0.5, 0.5, 0.0, 0.0],
            [0.0, 0.0],
        ));
        vertices.push(VertexPBR::new(
            [0.5, -0.5, z],
            [0.0, 0.0, 1.0],
            [0.5, 0.5, 0.0, 0.0],
            [0.0, 0.0],
        ));
    }
    vertices
}

pub fn create_cube_mesh(context: Rc<VulkanContext>) -> Mesh {
    let vertices = create_cube_vertices();
    let indices = vec![
        0, 1, 2, 2, 3, 0, // Front face
        4, 6, 5, 6, 4, 7, // Back face
        0, 4, 1, 1, 4, 5, // Left face
        2, 6, 3, 3, 6, 7, // Right face
        0, 3, 4, 4, 3, 7, // Top face
        1, 5, 2, 2, 5, 6, // Bottom face
    ];
    Mesh::new(context, vertices, indices)
}

pub fn create_cube_material(context: Rc<VulkanContext>, render_pass: &RenderPass) -> Material {
    let builder = MaterialBuilder::new(context)
        .with_depth_test(true)
        .with_wgsl_shader(&PathBuf::from("./resources/shaders/model.wgsl"))
        .with_vertex_binding(VertexPBR::get_vertex_binding());
    let pipeline = builder
        .build(render_pass)
        .expect("Failed to create material pipeline");
    Material {
        material_pipeline: pipeline,
        texture: None,
    }
}

pub fn create_cube(
    world: &mut World,
    context: Rc<VulkanContext>,
    render_pass: &RenderPass,
) -> ModelEntity {
    let mesh = create_cube_mesh(context.clone());
    let material = create_cube_material(context, render_pass);
    let mut transform = Transform::default();
    transform.scale = Vec3::new(5.0, 5.0, 5.0);
    let model = Model::new(vec![mesh], material, transform);
    ModelEntity::new(world, model)
}
