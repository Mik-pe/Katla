use std::{path::PathBuf, rc::Rc};

use katla_ecs::World;
use katla_math::{Transform, Vec3};
use katla_vulkan::{MaterialBuilder, RenderPass, VulkanContext};

use crate::{
    application::Model,
    entities::ModelEntity,
    rendering::{Material, Mesh, VertexPBR},
};

pub fn create_cube_vertices() -> Vec<VertexPBR> {
    let mut vertices = Vec::new();
    for i in 0..=1 {
        let sign = if i % 2 == 0 { 1.0 } else { -1.0 };
        let z = sign * 0.5;
        let lower_left = Vec3::new(-0.5, -0.5, z);
        let upper_left = Vec3::new(-0.5, 0.5, z);
        let upper_right = Vec3::new(0.5, 0.5, z);
        let lower_right = Vec3::new(0.5, -0.5, z);
        vertices.push(VertexPBR::new(
            lower_left.0,
            lower_left.normalize().0,
            [0.5, 0.5, 0.0, 0.0],
            [0.0, 0.0],
        ));
        vertices.push(VertexPBR::new(
            upper_left.0,
            upper_left.normalize().0,
            [0.5, 0.5, 0.0, 0.0],
            [0.0, 0.0],
        ));
        vertices.push(VertexPBR::new(
            upper_right.0,
            upper_right.normalize().0,
            [0.5, 0.5, 0.0, 0.0],
            [0.0, 0.0],
        ));
        vertices.push(VertexPBR::new(
            lower_right.0,
            lower_right.normalize().0,
            [0.5, 0.5, 0.0, 0.0],
            [0.0, 0.0],
        ));
    }
    vertices
}

pub fn create_cube_mesh(context: Rc<VulkanContext>) -> Mesh {
    let vertices = create_cube_vertices();
    let indices = vec![
        0, 2, 1, 3, 2, 0, // Front face
        4, 5, 6, 6, 7, 4, // Back face
        0, 1, 4, 1, 5, 4, // Left face
        2, 3, 6, 3, 7, 6, // Right face
        0, 4, 3, 4, 7, 3, // Top face
        1, 2, 5, 2, 6, 5, // Bottom face
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
