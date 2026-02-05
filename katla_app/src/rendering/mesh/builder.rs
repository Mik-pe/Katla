
use std::rc::Rc;

use katla_ecs::World;
use katla_math::{Transform, Vec3};
use katla_vulkan::{MaterialBuilder, RenderPass, Texture, VulkanContext};

use crate::{
    application::Model,
    entities::ModelEntity,
    rendering::{Material, VertexPBR},
};

pub struct MeshOptions {
    pub size: Option<Vec3>,
    pub radius: Option<f32>,
    pub height: Option<f32>,
    pub segments: Option<u32>,
    pub rings: Option<u32>,
    pub position: Option<Vec3>,
    pub color: Option<[f32; 3]>,
}

impl Default for MeshOptions {
    fn default() -> Self {
        Self {
            size: Some(Vec3::new(10.0, 10.0, 10.0)),
            radius: Some(5.0),
            height: Some(10.0),
            segments: Some(32),
            rings: Some(32),
            position: Some(Vec3::new(0.0, 0.0, 0.0)),
            color: None,
        }
    }
}

pub struct MeshBuilder<'a> {
    options: MeshOptions,
    world: &'a mut World,
    context: std::rc::Rc<VulkanContext>,
    render_pass: &'a RenderPass,
}

impl<'a> MeshBuilder<'a> {
    pub fn new(
        world: &'a mut World,
        context: std::rc::Rc<VulkanContext>,
        render_pass: &'a RenderPass,
    ) -> Self {
        Self {
            options: MeshOptions::default(),
            world,
            context,
            render_pass,
        }
    }

    pub fn size(mut self, size: Vec3) -> Self {
        self.options.size = Some(size);
        self
    }

    pub fn radius(mut self, radius: f32) -> Self {
        self.options.radius = Some(radius);
        self
    }

    pub fn height(mut self, height: f32) -> Self {
        self.options.height = Some(height);
        self
    }

    pub fn segments(mut self, segments: u32) -> Self {
        self.options.segments = Some(segments);
        self
    }

    pub fn rings(mut self, rings: u32) -> Self {
        self.options.rings = Some(rings);
        self
    }

    pub fn position(mut self, position: Vec3) -> Self {
        self.options.position = Some(position);
        self
    }

    pub fn color(mut self, color: [f32; 3]) -> Self {
        self.options.color = Some(color);
        self
    }

    pub fn create_cube(self) -> ModelEntity {
        let size = self.options.size.unwrap_or(Vec3::new(20.0, 20.0, 20.0));
        let mesh = crate::rendering::mesh::create_cube_mesh(self.context.clone(), size);
        let material =
            create_material_with_color(self.context, self.render_pass, self.options.color);
        let position = self.options.position.unwrap_or(Vec3::new(0.0, 0.0, 0.0));
        // Create transform with only position (no scale)
        let transform = Transform {
            position,
            rotation: katla_math::Quat::new(),
            scale: Vec3::new(1.0, 1.0, 1.0),
        };
        let model = Model::new(vec![mesh], material, transform);
        ModelEntity::new(self.world, model)
    }

    pub fn create_sphere(self) -> ModelEntity {
        let radius = self.options.radius.unwrap_or(5.0);
        let segments = self.options.segments.unwrap_or(32);
        let rings = self.options.rings.unwrap_or(32);
        let mesh = crate::rendering::mesh::create_sphere_mesh(self.context.clone(), radius, segments, rings);
        let material =
            create_material_with_color(self.context, self.render_pass, self.options.color);
        let position = self.options.position.unwrap_or(Vec3::new(0.0, 0.0, 0.0));
        let transform = Transform {
            position,
            rotation: katla_math::Quat::new(),
            scale: Vec3::new(1.0, 1.0, 1.0),
        };
        let model = Model::new(vec![mesh], material, transform);
        ModelEntity::new(self.world, model)
    }

    pub fn create_cylinder(self) -> ModelEntity {
        let height = self.options.height.unwrap_or(10.0);
        let radius = self.options.radius.unwrap_or(5.0);
        let segments = self.options.segments.unwrap_or(32);
        let mesh = crate::rendering::mesh::create_cylinder_mesh(self.context.clone(), height, radius, segments);
        let material =
            create_material_with_color(self.context, self.render_pass, self.options.color);
        let position = self.options.position.unwrap_or(Vec3::new(0.0, 0.0, 0.0));
        let transform = Transform {
            position,
            rotation: katla_math::Quat::new(),
            scale: Vec3::new(1.0, 1.0, 1.0),
        };
        let model = Model::new(vec![mesh], material, transform);
        ModelEntity::new(self.world, model)
    }

    pub fn create_plane(self) -> ModelEntity {
        let size = self.options.size.unwrap_or(Vec3::new(100.0, 100.0, 1.0));
        let segments = self.options.segments.unwrap_or(32);
        let mesh = crate::rendering::mesh::create_plane_mesh(self.context.clone(), size.x(), size.y(), segments);
        let material =
            create_material_with_color(self.context, self.render_pass, self.options.color);
        let position = self.options.position.unwrap_or(Vec3::new(0.0, 0.0, 0.0));
        let transform = Transform {
            position,
            rotation: katla_math::Quat::new(),
            scale: Vec3::new(1.0, 1.0, 1.0),
        };
        let model = Model::new(vec![mesh], material, transform);
        ModelEntity::new(self.world, model)
    }

    pub fn create_torus(self) -> ModelEntity {
        let major_radius = self.options.radius.unwrap_or(5.0) * 2.0;
        let minor_radius = self.options.radius.unwrap_or(5.0) * 0.6;
        let segments = self.options.segments.unwrap_or(32);
        let rings = self.options.rings.unwrap_or(32);
        let mesh = crate::rendering::mesh::create_torus_mesh(
            self.context.clone(),
            major_radius,
            minor_radius,
            segments,
            rings,
        );
        let material =
            create_material_with_color(self.context, self.render_pass, self.options.color);
        let position = self.options.position.unwrap_or(Vec3::new(0.0, 0.0, 0.0));
        let transform = Transform {
            position,
            rotation: katla_math::Quat::new(),
            scale: Vec3::new(1.0, 1.0, 1.0),
        };
        let model = Model::new(vec![mesh], material, transform);
        ModelEntity::new(self.world, model)
    }
}

fn create_material_with_color(
    context: Rc<VulkanContext>,
    render_pass: &RenderPass,
    color: Option<[f32; 3]>,
) -> Material {
    // Create a checkerboard texture (64x64)
    let texture_size = 64;
    let checker_size = 8; // 8x8 pixel squares
    let mut pixels = Vec::with_capacity((texture_size * texture_size) as usize);

    for y in 0..texture_size {
        for x in 0..texture_size {
            // Determine which checker square we're in
            let checker_x = x / checker_size;
            let checker_y = y / checker_size;

            // Checkerboard pattern: alternate between two colors
            let is_white = (checker_x + checker_y) % 2 == 0;

            let pixel = if is_white {
                [255, 255, 255, 255] // White
            } else {
                [0, 0, 0, 255] // Black
            };
            pixels.extend_from_slice(&pixel);
        }
    }

    let texture = Rc::new(Texture::create_image(
        context.clone(),
        texture_size,
        texture_size,
        katla_vulkan::ImageFormat::R8G8B8A8Srgb,
        &pixels,
    ));

    let material_pipeline = MaterialBuilder::new(context)
        .with_vertex_binding(VertexPBR::get_vertex_binding())
        .with_vertex_shader(include_bytes!("../../../../resources/shaders/model_pbr.vert.spv"))
        .with_fragment_shader(include_bytes!("../../../../resources/shaders/model.frag.spv"))
        .with_texture(texture.clone())
        .with_depth_test(true)
        .with_depth_write(true)
        .with_backface_culling(true)
        .build(render_pass)
        .expect("Failed to create material pipeline");

    Material {
        material_pipeline,
        texture: Some(texture),
    }
}
