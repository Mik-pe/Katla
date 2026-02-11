use std::rc::Rc;

use katla_ecs::{EntityId, World};
use katla_math::{Transform, Vec3};
use katla_vulkan::{MaterialRegistry, Texture, VulkanContext, VulkanRenderer};

use crate::{
    entities::Model,
    rendering::{create_checkerboard_material, Material},
};

/// Base builder with common options shared across all mesh types.
pub struct MeshBuilder {
    context: Rc<VulkanContext>,
    /// Raw pointer to MaterialRegistry for template-based material creation.
    /// This is safe because the registry outlives the builder and we only use it during build().
    material_registry: Option<*const std::cell::RefCell<MaterialRegistry>>,
    position: Option<Vec3>,
    color: Option<[f32; 3]>,
    shared_material_name: Option<String>,
    /// Cached checkerboard texture (created once and reused for all checkerboard materials)
    checkerboard_texture: Option<Rc<Texture>>,
}

impl MeshBuilder {
    pub fn new(context: Rc<VulkanContext>) -> Self {
        Self {
            context,
            material_registry: None,
            position: None,
            color: None,
            shared_material_name: None,
            checkerboard_texture: None,
        }
    }

    pub fn with_material_registry_ptr(
        mut self,
        registry_ptr: *const std::cell::RefCell<MaterialRegistry>,
    ) -> Self {
        self.material_registry = Some(registry_ptr);
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

    /// Get the material from the renderer's AssetRegistry by name.
    /// This is used internally by the shape-specific builders.
    #[allow(dead_code)]
    fn get_material_from_renderer(
        &self,
        _renderer: &VulkanRenderer,
        _name: &str,
    ) -> Option<Material> {
        // Try to get the material from the renderer's AssetRegistry
        // For now, this always returns None - the MaterialManager handles materials
        // TODO: Integrate with renderer's AssetRegistry for handle-based materials
        None
    }
}

/// Common functionality shared by all shape-specific builders.
macro_rules! impl_common_builder {
    ($builder:ident) => {
        impl $builder {
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

            fn get_material(&mut self, _renderer: &mut VulkanRenderer) -> Material {
                // Try to get material from template in the registry
                if let (Some(registry_ptr), Some(ref name)) =
                    (self.base.material_registry, &self.base.shared_material_name)
                {
                    // SAFETY: The raw pointer points to the MaterialRegistry in VulkanRenderer
                    // which is guaranteed to be valid for the lifetime of the application.
                    // We only access it during the build() call, and the renderer always outlives the builder.
                    unsafe {
                        let registry = &*registry_ptr;
                        if let Some(template) = registry.borrow().get_template(name) {
                            println!("  MeshBuilder: Using material from template '{}'", name);

                            // Check if this template needs a texture (Checkerboard uses procedural texture)
                            // Create and cache the texture on first use
                            let texture = if name == "Checkerboard" {
                                if self.base.checkerboard_texture.is_none() {
                                    self.base.checkerboard_texture = Some(std::rc::Rc::new(
                                        crate::rendering::create_checkerboard_texture(
                                            self.base.context.clone(),
                                        ),
                                    ));
                                }
                                self.base.checkerboard_texture.clone()
                            } else {
                                None
                            };

                            return Material::from_template(template, texture, None);
                        }
                        println!(
                            "  MeshBuilder: Template '{}' not found, creating directly",
                            name
                        );
                    }
                }

                // Fallback to creating material directly
                create_checkerboard_material(
                    self.base.context.clone(),
                )
            }

            fn create_entity(
                mut self,
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

                // Convert color from [f32; 3] to Color if specified
                let color = self
                    .base
                    .color
                    .map(|c| katla_math::Color::rgb(c[0], c[1], c[2]));

                Model::new(
                    world,
                    vec![mesh],
                    material,
                    Some(renderer),
                    transform,
                    color,
                )
                .entity
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
