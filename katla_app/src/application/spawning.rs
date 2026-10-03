//! Entity spawning and owned model resource preparation.

mod gltf_model;
mod texture_cache;
pub(crate) use texture_cache::GltfTextureCache;

#[derive(katla_ecs::Component)]
pub(crate) struct ModelTextures {
    #[inspect(skip)]
    pub(crate) handles: Vec<katla_gfx::TextureHandle>,
}

/// Geometry retained independently of a rendered surface for model-group colliders.
#[derive(katla_ecs::Component)]
pub(crate) struct CollisionMesh {
    #[inspect(skip)]
    pub(crate) handle: katla_gfx::MeshHandle,
}

use katla_gfx::GpuRenderer;
use katla_gfx::primitives;
use log::{debug, info};

use crate::scene::entity_source::EntitySource;

struct GltfTextureUpload {
    /// Typed handles for the four material PBR roles.
    textures: katla_gfx::MaterialTextures,
    /// Emission texture for the draw path (a draw field, not material
    /// state; the backend resolves the handle each frame).
    emission: katla_gfx::TextureHandle,
    handles: Vec<katla_gfx::TextureHandle>,
}

impl super::Application {
    /// Spawn a primitive entity with a specific color using the default material.
    ///
    /// Color is expected in sRGB (perceptual) space and converted to linear for PBR.
    fn spawn_primitive_with_color(
        &mut self,
        position: [f32; 3],
        color: katla_math::Color,
        mesh_handle: katla_gfx::MeshHandle,
        source: EntitySource,
    ) -> katla_ecs::EntityId {
        use crate::components::{DrawableComponent, TransformComponent};
        use katla_math::Vec3;

        let material_handle = self.default_material();
        let linear_color = color.to_linear();

        let bounds = local_bounds_for_source(&source);

        let drawable =
            DrawableComponent::with_handles_and_color(mesh_handle, material_handle, linear_color)
                .with_bounds(bounds);
        self.gpu_resource_tracker.track_drawable(
            mesh_handle,
            material_handle,
            drawable.skeleton_handle,
        );

        let entity = self.world.spawn((
            TransformComponent::from_position(Vec3::new(position[0], position[1], position[2])),
            drawable,
        ));

        self.world.add_component(entity, source.clone());
        self.world.add_component(
            entity,
            crate::components::NameComponent::new(source.display_name()),
        );
        entity
    }

    /// Spawn a test cube entity with the default material.
    pub fn spawn_test_cube(&mut self, position: [f32; 3], size: [f32; 3]) -> katla_ecs::EntityId {
        self.spawn_test_cube_with_color(position, size, katla_math::Color::WHITE)
    }

    /// Spawn a bare transform entity when GPU mesh creation fails.
    ///
    /// Spawn helpers must return an entity; a failed mesh upload logs loudly
    /// and yields a transform-only entity instead of aliasing a wrong mesh.
    fn spawn_empty_at(&mut self, position: [f32; 3]) -> katla_ecs::EntityId {
        use crate::components::TransformComponent;
        use katla_math::Vec3;
        self.world
            .spawn((TransformComponent::from_position(Vec3::new(
                position[0],
                position[1],
                position[2],
            )),))
    }

    /// Spawn a test cube entity with a specific color.
    /// Color is expected in sRGB (perceptual) space and converted to linear for PBR.
    pub fn spawn_test_cube_with_color(
        &mut self,
        position: [f32; 3],
        size: [f32; 3],
        color: katla_math::Color,
    ) -> katla_ecs::EntityId {
        let mesh_handle = match primitives::create_cube(&mut self.renderer, size) {
            Ok(handle) => handle,
            Err(error) => {
                log::error!("Spawn mesh creation failed: {error}");
                return self.spawn_empty_at(position);
            }
        };
        info!("Spawned test cube at {:?} with size {:?}", position, size);
        self.spawn_primitive_with_color(position, color, mesh_handle, EntitySource::Cube { size })
    }

    /// Spawn a sphere entity with the default material.
    pub fn spawn_sphere(
        &mut self,
        position: [f32; 3],
        radius: f32,
        segments: u32,
        rings: u32,
    ) -> katla_ecs::EntityId {
        self.spawn_sphere_with_color(position, radius, segments, rings, katla_math::Color::WHITE)
    }

    /// Spawn a sphere entity with a specific color.
    /// Color is expected in sRGB (perceptual) space and converted to linear for PBR.
    pub fn spawn_sphere_with_color(
        &mut self,
        position: [f32; 3],
        radius: f32,
        segments: u32,
        rings: u32,
        color: katla_math::Color,
    ) -> katla_ecs::EntityId {
        let mesh_handle =
            match primitives::create_sphere(&mut self.renderer, radius, segments, rings) {
                Ok(handle) => handle,
                Err(error) => {
                    log::error!("Spawn mesh creation failed: {error}");
                    return self.spawn_empty_at(position);
                }
            };
        info!("Spawned sphere at {:?} with radius {}", position, radius);
        self.spawn_primitive_with_color(
            position,
            color,
            mesh_handle,
            EntitySource::Sphere {
                radius,
                segments,
                rings,
            },
        )
    }

    /// Spawn a sphere entity with PBR material properties.
    /// Color is expected in sRGB (perceptual) space and converted to linear for PBR.
    pub fn spawn_sphere_with_material(
        &mut self,
        position: [f32; 3],
        radius: f32,
        segments: u32,
        rings: u32,
        material: &crate::spawner::PrimitiveMaterialParams,
    ) -> katla_ecs::EntityId {
        use crate::components::{DrawableComponent, TransformComponent};
        use katla_math::Vec3;

        let mesh_handle =
            match primitives::create_sphere(&mut self.renderer, radius, segments, rings) {
                Ok(handle) => handle,
                Err(error) => {
                    log::error!("Spawn mesh creation failed: {error}");
                    return self.spawn_empty_at(position);
                }
            };
        let material_handle = self.default_material();
        let linear_color = material.color.map(|c| c.to_linear()).unwrap_or_default();

        let bounds = katla_math::AABB::from_min_max(
            katla_math::Vec3::new(-radius, -radius, -radius),
            katla_math::Vec3::new(radius, radius, radius),
        );

        let drawable = DrawableComponent::with_handles_and_material(
            mesh_handle,
            material_handle,
            Some(linear_color),
            material.metallic,
            material.roughness,
            1.0,
        )
        .with_bounds(bounds);
        self.gpu_resource_tracker.track_drawable(
            mesh_handle,
            material_handle,
            drawable.skeleton_handle,
        );

        let entity = self.world.spawn((
            TransformComponent::from_position(Vec3::new(position[0], position[1], position[2])),
            drawable,
        ));

        self.world.add_component(
            entity,
            EntitySource::Sphere {
                radius,
                segments,
                rings,
            },
        );

        entity
    }

    /// Spawn a cylinder entity with the default material.
    pub fn spawn_cylinder(
        &mut self,
        position: [f32; 3],
        height: f32,
        radius: f32,
        segments: u32,
    ) -> katla_ecs::EntityId {
        self.spawn_cylinder_with_color(position, height, radius, segments, katla_math::Color::WHITE)
    }

    /// Spawn a cylinder entity with a specific color.
    /// Color is expected in sRGB (perceptual) space and converted to linear for PBR.
    pub fn spawn_cylinder_with_color(
        &mut self,
        position: [f32; 3],
        height: f32,
        radius: f32,
        segments: u32,
        color: katla_math::Color,
    ) -> katla_ecs::EntityId {
        let mesh_handle =
            match primitives::create_cylinder(&mut self.renderer, height, radius, segments) {
                Ok(handle) => handle,
                Err(error) => {
                    log::error!("Spawn mesh creation failed: {error}");
                    return self.spawn_empty_at(position);
                }
            };
        info!("Spawned cylinder at {:?}", position);
        self.spawn_primitive_with_color(
            position,
            color,
            mesh_handle,
            EntitySource::Cylinder {
                height,
                radius,
                segments,
            },
        )
    }

    /// Spawn a plane entity with the default material.
    pub fn spawn_plane(
        &mut self,
        position: [f32; 3],
        width: f32,
        height: f32,
    ) -> katla_ecs::EntityId {
        self.spawn_plane_with_color(position, width, height, katla_math::Color::WHITE)
    }

    /// Spawn a plane entity with a specific color.
    /// Color is expected in sRGB (perceptual) space and converted to linear for PBR.
    pub fn spawn_plane_with_color(
        &mut self,
        position: [f32; 3],
        width: f32,
        height: f32,
        color: katla_math::Color,
    ) -> katla_ecs::EntityId {
        let mesh_handle = match primitives::create_plane(&mut self.renderer, width, height) {
            Ok(handle) => handle,
            Err(error) => {
                log::error!("Spawn mesh creation failed: {error}");
                return self.spawn_empty_at(position);
            }
        };
        info!("Spawned plane at {:?}", position);
        self.spawn_primitive_with_color(
            position,
            color,
            mesh_handle,
            EntitySource::Plane { width, height },
        )
    }

    /// Spawn a torus entity with the default material.
    pub fn spawn_torus(
        &mut self,
        position: [f32; 3],
        radius: f32,
        tube_radius: f32,
        segments: u32,
        tube_segments: u32,
    ) -> katla_ecs::EntityId {
        self.spawn_torus_with_color(
            position,
            radius,
            tube_radius,
            segments,
            tube_segments,
            katla_math::Color::WHITE,
        )
    }

    /// Spawn a torus entity with a specific color.
    /// Color is expected in sRGB (perceptual) space and converted to linear for PBR.
    pub fn spawn_torus_with_color(
        &mut self,
        position: [f32; 3],
        radius: f32,
        tube_radius: f32,
        segments: u32,
        tube_segments: u32,
        color: katla_math::Color,
    ) -> katla_ecs::EntityId {
        let mesh_handle = match primitives::create_torus(
            &mut self.renderer,
            radius,
            tube_radius,
            segments,
            tube_segments,
        ) {
            Ok(handle) => handle,
            Err(error) => {
                log::error!("Spawn mesh creation failed: {error}");
                return self.spawn_empty_at(position);
            }
        };
        info!("Spawned torus at {:?}", position);
        self.spawn_primitive_with_color(
            position,
            color,
            mesh_handle,
            EntitySource::Torus {
                radius,
                tube_radius,
                segments,
                tube_segments,
            },
        )
    }

    /// Spawn an STL model from file.
    ///
    /// STL files contain only triangle geometry. They are spawned with the default PBR
    /// material and no textures. The entity gets an [`EntitySource::StlModel`] for round-tripping.
    pub fn spawn_stl_model(
        &mut self,
        path: impl AsRef<std::path::Path>,
        position: [f32; 3],
    ) -> crate::error::AppResult<katla_ecs::EntityId> {
        use crate::components::{DrawableComponent, TransformComponent};
        use katla_math::{AABB, Vec3};

        let source_path = std::fs::canonicalize(path.as_ref())?;
        let (mesh_handle, bounds) = self.load_stl_mesh(path.as_ref())?;

        let material_handle = self.default_material();

        let local_bounds = AABB::from_min_max(
            bounds.center - Vec3::new(bounds.radius, bounds.radius, bounds.radius),
            bounds.center + Vec3::new(bounds.radius, bounds.radius, bounds.radius),
        );

        let entity = self.world.spawn((
            TransformComponent::from_position(Vec3::new(position[0], position[1], position[2])),
            DrawableComponent::with_handles(mesh_handle, material_handle).with_bounds(local_bounds),
        ));

        self.world.add_component(
            entity,
            EntitySource::StlModel {
                path: crate::scene::AssetRef::File(source_path),
            },
        );
        self.world.add_component(
            entity,
            crate::components::NameComponent::new(
                std::path::Path::new(path.as_ref())
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("STL Model"),
            ),
        );

        if let Some(drawable) = self.world.get_component::<DrawableComponent>(entity) {
            self.gpu_resource_tracker.track_drawable(
                drawable.mesh_handle,
                drawable.material_handle,
                drawable.skeleton_handle,
            );
        }

        info!("Spawned STL model '{}'", path.as_ref().to_string_lossy());

        Ok(entity)
    }

    /// Upload textures from a GLTF model and return typed texture handles.
    ///
    /// Material roles go through [`katla_gfx::MaterialTextures`]; the
    /// emission texture is returned separately because it rides on the
    /// draw call, not on material state.
    fn upload_gltf_textures(
        &mut self,
        asset: &std::path::Path,
        images: &[gltf::image::Data],
        mat: &crate::util::gltf_material::GltfMaterialInfo,
    ) -> GltfTextureUpload {
        let mut textures = self.scene_features.as_ref().map_or_else(
            katla_gfx::MaterialTextures::default,
            super::scene_features::SceneFeatures::material_textures,
        );
        let mut emission = katla_gfx::TextureHandle::NONE;
        let mut handles = Vec::new();
        self.gltf_texture_cache.prune(&self.renderer);

        for (image_index, srgb, role) in [
            (mat.base_color_texture, true, &mut textures.albedo),
            (mat.normal_texture, false, &mut textures.normal),
            (
                mat.metallic_roughness_texture,
                false,
                &mut textures.metallic_roughness,
            ),
            (mat.occlusion_texture, false, &mut textures.occlusion),
            (mat.emission_texture, true, &mut emission),
        ] {
            let Some(image_index) = image_index else {
                continue;
            };
            let Some(image) = images.get(image_index) else {
                log::warn!("GLTF image {image_index} is missing; retaining material fallback");
                continue;
            };
            let srgb = srgb
                && !matches!(
                    image.format,
                    gltf::image::Format::R32G32B32FLOAT | gltf::image::Format::R32G32B32A32FLOAT
                );
            let cached = self.gltf_texture_cache.get(asset, image_index, srgb);
            match cached.map_or_else(|| self.upload_gltf_image(image, srgb), Ok) {
                Ok(handle) => {
                    *role = handle;
                    self.gltf_texture_cache
                        .insert(asset, image_index, srgb, handle);
                    if !handles.contains(&handle) {
                        handles.push(handle);
                    }
                    debug!("Uploaded GLTF image {image_index} -> {handle:?} (srgb={srgb})");
                }
                Err(error) => log::warn!(
                    "GLTF image {image_index} upload failed: {error}; retaining material fallback"
                ),
            }
        }

        GltfTextureUpload {
            textures,
            emission,
            handles,
        }
    }

    /// Upload a single GLTF image, preserving the caller's role fallback on error.
    fn upload_gltf_image(
        &mut self,
        image: &gltf::image::Data,
        srgb: bool,
    ) -> Result<katla_gfx::TextureHandle, String> {
        let (descriptor, pixels) = crate::util::gltf_image::texture_upload(image, srgb)?;
        self.renderer
            .create_texture(&descriptor, &pixels)
            .map_err(|e| e.to_string())
    }
}

pub(crate) fn local_bounds_for_source(source: &EntitySource) -> katla_math::AABB {
    use katla_math::{AABB, Vec3};

    match source {
        EntitySource::Cube { size } => AABB::from_min_max(
            Vec3::new(-size[0] / 2.0, -size[1] / 2.0, -size[2] / 2.0),
            Vec3::new(size[0] / 2.0, size[1] / 2.0, size[2] / 2.0),
        ),
        EntitySource::Sphere { radius, .. } => AABB::from_min_max(
            Vec3::new(-radius, -radius, -radius),
            Vec3::new(*radius, *radius, *radius),
        ),
        EntitySource::Plane { width, height } => AABB::from_min_max(
            Vec3::new(-width / 2.0, 0.0, -height / 2.0),
            Vec3::new(*width / 2.0, 0.0, *height / 2.0),
        ),
        EntitySource::Cylinder { height, radius, .. } => AABB::from_min_max(
            Vec3::new(-radius, -height / 2.0, -radius),
            Vec3::new(*radius, *height / 2.0, *radius),
        ),
        EntitySource::Torus {
            radius,
            tube_radius,
            ..
        } => AABB::from_min_max(
            Vec3::new(-radius - tube_radius, -tube_radius, -radius - tube_radius),
            Vec3::new(radius + tube_radius, *tube_radius, radius + tube_radius),
        ),
        _ => AABB::from_min_max(Vec3::new(-0.5, -0.5, -0.5), Vec3::new(0.5, 0.5, 0.5)),
    }
}

#[cfg(test)]
mod material_tests;
