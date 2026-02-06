use std::rc::Rc;

use katla_ecs::{EntityId, World};
use katla_math::{Transform, Vec3};
use katla_vulkan::{VulkanContext, VulkanRenderer};

use crate::{
    application::Model,
    entities::create_model_entity,
    rendering::{create_checkerboard_material, Material, MaterialManager},
};

/// Base builder with common options shared across all mesh types.
pub struct MeshBuilder {
    context: Rc<VulkanContext>,
    material_manager: Option<MaterialManager>,
    position: Option<Vec3>,
    color: Option<[f32; 3]>,
    shared_material_name: Option<String>,
}

impl MeshBuilder {
    pub fn new(context: Rc<VulkanContext>) -> Self {
        Self {
            context,
            material_manager: None,
            position: None,
            color: None,
            shared_material_name: None,
        }
    }

    pub fn with_material_manager(mut self, material_manager: MaterialManager) -> Self {
        self.material_manager = Some(material_manager);
        self
    }

    pub fn with_shared_material(mut self, name: impl Into<String>) -> Self {
        self.shared_material_name = Some(name.into());
        self
    }

    pub fn position(mut self, position: Vec3) -> Self {
        self.position = Some(position);
        self
    }

    pub fn color(mut self, color: [f32; 3]) -> Self {
        self.color = Some(color);
        self
    }

    /// Create a cube mesh (default shape).
    /// Returns a CubeBuilder with cube-specific options.
    pub fn cube(self) -> CubeBuilder {
        CubeBuilder {
            base: self,
            size: Some(Vec3::new(10.0, 10.0, 10.0)),
        }
    }

    /// Create a sphere mesh.
    /// Returns a SphereBuilder with sphere-specific options.
    pub fn sphere(self) -> SphereBuilder {
        SphereBuilder {
            base: self,
            radius: Some(5.0),
            segments: Some(32),
            rings: Some(32),
        }
    }

    /// Create a cylinder mesh.
    /// Returns a CylinderBuilder with cylinder-specific options.
    pub fn cylinder(self) -> CylinderBuilder {
        CylinderBuilder {
            base: self,
            height: Some(10.0),
            radius: Some(5.0),
            segments: Some(32),
        }
    }

    /// Create a plane mesh.
    /// Returns a PlaneBuilder with plane-specific options.
    pub fn plane(self) -> PlaneBuilder {
        PlaneBuilder {
            base: self,
            size: Some(Vec3::new(100.0, 100.0, 1.0)),
            segments: Some(32),
        }
    }

    /// Create a torus mesh.
    /// Returns a TorusBuilder with torus-specific options.
    pub fn torus(self) -> TorusBuilder {
        TorusBuilder {
            base: self,
            radius: Some(5.0),
            segments: Some(32),
            rings: Some(32),
        }
    }

    /// Build with default cube (for backwards compatibility).
    pub fn build(self, world: &mut World, renderer: &mut VulkanRenderer) -> EntityId {
        self.cube().build(world, renderer)
    }
}

/// Common functionality shared by all shape-specific builders.
macro_rules! impl_common_builder {
    ($builder:ident) => {
        impl $builder {
            pub fn with_material_manager(mut self, material_manager: MaterialManager) -> Self {
                self.base.material_manager = Some(material_manager);
                self
            }

            pub fn with_shared_material(mut self, name: impl Into<String>) -> Self {
                self.base.shared_material_name = Some(name.into());
                self
            }

            pub fn position(mut self, position: Vec3) -> Self {
                self.base.position = Some(position);
                self
            }

            pub fn color(mut self, color: [f32; 3]) -> Self {
                self.base.color = Some(color);
                self
            }

            fn get_material(&self, renderer: &mut VulkanRenderer) -> Material {
                if let Some(ref manager) = self.base.material_manager {
                    if let Some(ref name) = self.base.shared_material_name {
                        if let Some(material) = manager.clone_material_by_name(name) {
                            return material;
                        }
                    }
                }
                create_checkerboard_material(
                    self.base.context.clone(),
                    &renderer.render_pass,
                )
            }

            fn create_entity(
                self,
                world: &mut World,
                renderer: &mut VulkanRenderer,
                mesh: crate::rendering::mesh::Mesh,
            ) -> EntityId {
                let material = self.get_material(renderer);
                let position = self.base.position.unwrap_or(Vec3::new(0.0, 0.0, 0.0));
                let transform = Transform {
                    position,
                    rotation: katla_math::Quat::new(),
                    scale: Vec3::new(1.0, 1.0, 1.0),
                };
                let model = Model::new(vec![mesh], material);
                create_model_entity(world, model, Some(renderer), transform)
            }
        }
    };
}

/// Builder for cube meshes with cube-specific options.
pub struct CubeBuilder {
    base: MeshBuilder,
    size: Option<Vec3>,
}

impl_common_builder!(CubeBuilder);

impl CubeBuilder {
    pub fn size(mut self, size: Vec3) -> Self {
        self.size = Some(size);
        self
    }

    pub fn build(self, world: &mut World, renderer: &mut VulkanRenderer) -> EntityId {
        let size = self.size.unwrap_or(Vec3::new(10.0, 10.0, 10.0));
        let mesh = crate::rendering::mesh::create_cube_mesh(self.base.context.clone(), size);
        self.create_entity(world, renderer, mesh)
    }
}

/// Builder for sphere meshes with sphere-specific options.
pub struct SphereBuilder {
    base: MeshBuilder,
    radius: Option<f32>,
    segments: Option<u32>,
    rings: Option<u32>,
}

impl_common_builder!(SphereBuilder);

impl SphereBuilder {
    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = Some(radius);
        self
    }

    pub fn segments(mut self, segments: u32) -> Self {
        self.segments = Some(segments);
        self
    }

    pub fn rings(mut self, rings: u32) -> Self {
        self.rings = Some(rings);
        self
    }

    pub fn build(self, world: &mut World, renderer: &mut VulkanRenderer) -> EntityId {
        let radius = self.radius.unwrap_or(5.0);
        let segments = self.segments.unwrap_or(32);
        let rings = self.rings.unwrap_or(32);
        let mesh = crate::rendering::mesh::create_sphere_mesh(
            self.base.context.clone(),
            radius,
            segments,
            rings,
        );
        self.create_entity(world, renderer, mesh)
    }
}

/// Builder for cylinder meshes with cylinder-specific options.
pub struct CylinderBuilder {
    base: MeshBuilder,
    height: Option<f32>,
    radius: Option<f32>,
    segments: Option<u32>,
}

impl_common_builder!(CylinderBuilder);

impl CylinderBuilder {
    pub fn height(mut self, height: f32) -> Self {
        self.height = Some(height);
        self
    }

    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = Some(radius);
        self
    }

    pub fn segments(mut self, segments: u32) -> Self {
        self.segments = Some(segments);
        self
    }

    pub fn build(self, world: &mut World, renderer: &mut VulkanRenderer) -> EntityId {
        let height = self.height.unwrap_or(10.0);
        let radius = self.radius.unwrap_or(5.0);
        let segments = self.segments.unwrap_or(32);
        let mesh = crate::rendering::mesh::create_cylinder_mesh(
            self.base.context.clone(),
            height,
            radius,
            segments,
        );
        self.create_entity(world, renderer, mesh)
    }
}

/// Builder for plane meshes with plane-specific options.
pub struct PlaneBuilder {
    base: MeshBuilder,
    size: Option<Vec3>,
    segments: Option<u32>,
}

impl_common_builder!(PlaneBuilder);

impl PlaneBuilder {
    pub fn size(mut self, size: Vec3) -> Self {
        self.size = Some(size);
        self
    }

    pub fn segments(mut self, segments: u32) -> Self {
        self.segments = Some(segments);
        self
    }

    pub fn build(self, world: &mut World, renderer: &mut VulkanRenderer) -> EntityId {
        let size = self.size.unwrap_or(Vec3::new(100.0, 100.0, 1.0));
        let segments = self.segments.unwrap_or(32);
        let mesh = crate::rendering::mesh::create_plane_mesh(
            self.base.context.clone(),
            size.x(),
            size.y(),
            segments,
        );
        self.create_entity(world, renderer, mesh)
    }
}

/// Builder for torus meshes with torus-specific options.
pub struct TorusBuilder {
    base: MeshBuilder,
    radius: Option<f32>,
    segments: Option<u32>,
    rings: Option<u32>,
}

impl_common_builder!(TorusBuilder);

impl TorusBuilder {
    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = Some(radius);
        self
    }

    pub fn segments(mut self, segments: u32) -> Self {
        self.segments = Some(segments);
        self
    }

    pub fn rings(mut self, rings: u32) -> Self {
        self.rings = Some(rings);
        self
    }

    pub fn build(self, world: &mut World, renderer: &mut VulkanRenderer) -> EntityId {
        let major_radius = self.radius.unwrap_or(5.0) * 2.0;
        let minor_radius = self.radius.unwrap_or(5.0) * 0.6;
        let segments = self.segments.unwrap_or(32);
        let rings = self.rings.unwrap_or(32);
        let mesh = crate::rendering::mesh::create_torus_mesh(
            self.base.context.clone(),
            major_radius,
            minor_radius,
            segments,
            rings,
        );
        self.create_entity(world, renderer, mesh)
    }
}
