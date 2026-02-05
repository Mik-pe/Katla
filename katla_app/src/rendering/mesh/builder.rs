
use std::rc::Rc;

use katla_ecs::World;
use katla_math::{Transform, Vec3};
use katla_vulkan::{VulkanContext, VulkanRenderer};

use crate::{
    application::Model,
    entities::ModelEntity,
    rendering::{create_checkerboard_material, Material, MaterialManager, ShaderRegistry},
};

pub struct MeshOptions {
    pub size: Option<Vec3>,
    pub radius: Option<f32>,
    pub height: Option<f32>,
    pub segments: Option<u32>,
    pub rings: Option<u32>,
    pub position: Option<Vec3>,
    pub color: Option<[f32; 3]>,
    pub shared_material_name: Option<String>,
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
            shared_material_name: None,
        }
    }
}

pub struct MeshBuilder {
    options: MeshOptions,
    context: Rc<VulkanContext>,
    shader_registry: ShaderRegistry,
    material_manager: Option<MaterialManager>,
}

impl MeshBuilder {
    pub fn new(
        context: Rc<VulkanContext>,
    ) -> Self {
        Self {
            options: MeshOptions::default(),
            context,
            shader_registry: ShaderRegistry::new(),
            material_manager: None,
        }
    }

    pub fn with_material_manager(mut self, material_manager: MaterialManager) -> Self {
        self.material_manager = Some(material_manager);
        self
    }

    pub fn with_shared_material(mut self, name: impl Into<String>) -> Self {
        self.options.shared_material_name = Some(name.into());
        self
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

    fn get_material(&self, renderer: &mut VulkanRenderer) -> Material {
        // Use shared material if specified
        if let Some(ref manager) = self.material_manager {
            if let Some(ref name) = self.options.shared_material_name {
                if let Some(material) = manager.clone_material_by_name(name) {
                    return material;
                }
            }
        }
        // Otherwise create a new checkerboard material
        create_checkerboard_material(self.context.clone(), &renderer.render_pass, &self.shader_registry)
    }

    pub fn create_cube(self, world: &mut World, renderer: &mut VulkanRenderer) -> ModelEntity {
        let size = self.options.size.unwrap_or(Vec3::new(20.0, 20.0, 20.0));
        let mesh = crate::rendering::mesh::create_cube_mesh(self.context.clone(), size);
        let material = self.get_material(renderer);
        let position = self.options.position.unwrap_or(Vec3::new(0.0, 0.0, 0.0));
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
        let material = self.get_material(renderer);
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
        let material = self.get_material(renderer);
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
        let material = self.get_material(renderer);
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
        let material = self.get_material(renderer);
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
