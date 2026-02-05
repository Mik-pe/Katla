
use std::rc::Rc;

use katla_ecs::World;
use katla_math::{Transform, Vec3};
use katla_vulkan::{MaterialBuilder, RenderPass, Texture, VulkanContext, VulkanRenderer};

use crate::{
    application::Model,
    entities::ModelEntity,
    rendering::{Material, ShaderRegistry, VertexPBR},
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

pub struct MeshBuilder {
    options: MeshOptions,
    context: Rc<VulkanContext>,
    shader_registry: ShaderRegistry,
}

impl MeshBuilder {
    pub fn new(
        context: Rc<VulkanContext>,
    ) -> Self {
        Self {
            options: MeshOptions::default(),
            context,
            shader_registry: ShaderRegistry::new(),
        }
    }

    pub fn size(mut self, size: Vec3) -> Self {
        self.options.size = Some(size);
        self
    }

    #[allow(dead_code)]
    pub fn radius(mut self, radius: f32) -> Self {
        self.options.radius = Some(radius);
        self
    }

    #[allow(dead_code)]
    pub fn height(mut self, height: f32) -> Self {
        self.options.height = Some(height);
        self
    }

    #[allow(dead_code)]
    pub fn segments(mut self, segments: u32) -> Self {
        self.options.segments = Some(segments);
        self
    }

    #[allow(dead_code)]
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

    pub fn create_cube(self, world: &mut World, renderer: &mut VulkanRenderer) -> ModelEntity {
        let size = self.options.size.unwrap_or(Vec3::new(20.0, 20.0, 20.0));
        let mesh = crate::rendering::mesh::create_cube_mesh(self.context.clone(), size);
        let material =
            create_material_with_color(self.context.clone(), &renderer.render_pass, self.options.color, &self.shader_registry);
        let position = self.options.position.unwrap_or(Vec3::new(0.0, 0.0, 0.0));
        // Create transform with only position (no scale)
        let transform = Transform {
            position,
            rotation: katla_math::Quat::new(),
            scale: Vec3::new(1.0, 1.0, 1.0),
        };
        let model = Model::new(vec![mesh], material);
        ModelEntity::new_with_renderer(world, model, Some(renderer), transform)
    }

    pub fn create_sphere(self, world: &mut World, renderer: &mut VulkanRenderer) -> ModelEntity {
        let radius = self.options.radius.unwrap_or(5.0);
        let segments = self.options.segments.unwrap_or(32);
        let rings = self.options.rings.unwrap_or(32);
        let mesh = crate::rendering::mesh::create_sphere_mesh(self.context.clone(), radius, segments, rings);
        let material =
            create_material_with_color(self.context.clone(), &renderer.render_pass, self.options.color, &self.shader_registry);
        let position = self.options.position.unwrap_or(Vec3::new(0.0, 0.0, 0.0));
        let transform = Transform {
            position,
            rotation: katla_math::Quat::new(),
            scale: Vec3::new(1.0, 1.0, 1.0),
        };
        let model = Model::new(vec![mesh], material);
        ModelEntity::new_with_renderer(world, model, Some(renderer), transform)
    }

    pub fn create_cylinder(self, world: &mut World, renderer: &mut VulkanRenderer) -> ModelEntity {
        let height = self.options.height.unwrap_or(10.0);
        let radius = self.options.radius.unwrap_or(5.0);
        let segments = self.options.segments.unwrap_or(32);
        let mesh = crate::rendering::mesh::create_cylinder_mesh(self.context.clone(), height, radius, segments);
        let material =
            create_material_with_color(self.context.clone(), &renderer.render_pass, self.options.color, &self.shader_registry);
        let position = self.options.position.unwrap_or(Vec3::new(0.0, 0.0, 0.0));
        let transform = Transform {
            position,
            rotation: katla_math::Quat::new(),
            scale: Vec3::new(1.0, 1.0, 1.0),
        };
        let model = Model::new(vec![mesh], material);
        ModelEntity::new_with_renderer(world, model, Some(renderer), transform)
    }

    pub fn create_plane(self, world: &mut World, renderer: &mut VulkanRenderer) -> ModelEntity {
        let size = self.options.size.unwrap_or(Vec3::new(100.0, 100.0, 1.0));
        let segments = self.options.segments.unwrap_or(32);
        let mesh = crate::rendering::mesh::create_plane_mesh(self.context.clone(), size.x(), size.y(), segments);
        let material =
            create_material_with_color(self.context.clone(), &renderer.render_pass, self.options.color, &self.shader_registry);
        let position = self.options.position.unwrap_or(Vec3::new(0.0, 0.0, 0.0));
        let transform = Transform {
            position,
            rotation: katla_math::Quat::new(),
            scale: Vec3::new(1.0, 1.0, 1.0),
        };
        let model = Model::new(vec![mesh], material);
        ModelEntity::new_with_renderer(world, model, Some(renderer), transform)
    }

    pub fn create_torus(self, world: &mut World, renderer: &mut VulkanRenderer) -> ModelEntity {
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
            create_material_with_color(self.context.clone(), &renderer.render_pass, self.options.color, &self.shader_registry);
        let position = self.options.position.unwrap_or(Vec3::new(0.0, 0.0, 0.0));
        let transform = Transform {
            position,
            rotation: katla_math::Quat::new(),
            scale: Vec3::new(1.0, 1.0, 1.0),
        };
        let model = Model::new(vec![mesh], material);
        ModelEntity::new_with_renderer(world, model, Some(renderer), transform)
    }
}

fn create_material_with_color(
    context: std::rc::Rc<VulkanContext>,
    render_pass: &RenderPass,
    _color: Option<[f32; 3]>,
    shader_registry: &ShaderRegistry,
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

    let vertex_binding = VertexPBR::get_vertex_binding();
    let material_pipeline = MaterialBuilder::new(context.clone())
        .with_vertex_binding(vertex_binding.clone())
        .with_vertex_shader(shader_registry.get_vertex_shader("model_pbr.vert"))
        .with_fragment_shader(shader_registry.get_fragment_shader("model.frag"))
        .with_texture(texture.clone())
        .with_depth_test(true)
        .with_depth_write(true)
        .with_backface_culling(true)
        .build(render_pass)
        .expect("Failed to create material pipeline");

    Material {
        material_pipeline: std::rc::Rc::new(std::cell::RefCell::new(material_pipeline)),
        texture: Some(texture),
        vertex_binding,
        handle: None,
    }
}
