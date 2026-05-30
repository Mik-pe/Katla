use katla_gfx::GpuRenderer;
use katla_gfx::primitives;
use log::{debug, info};

use crate::scene::entity_source::EntitySource;

struct GltfTextureUpload {
    indices: [u32; 5],
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

    /// Spawn a test cube entity with a specific color.
    /// Color is expected in sRGB (perceptual) space and converted to linear for PBR.
    pub fn spawn_test_cube_with_color(
        &mut self,
        position: [f32; 3],
        size: [f32; 3],
        color: katla_math::Color,
    ) -> katla_ecs::EntityId {
        let mesh_handle = primitives::create_cube(&mut self.renderer, size);
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
        let mesh_handle = primitives::create_sphere(&mut self.renderer, radius, segments, rings);
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

        let mesh_handle = primitives::create_sphere(&mut self.renderer, radius, segments, rings);
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
        let mesh_handle = primitives::create_cylinder(&mut self.renderer, height, radius, segments);
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
        let mesh_handle = primitives::create_plane(&mut self.renderer, width, height);
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
        let mesh_handle = primitives::create_torus(
            &mut self.renderer,
            radius,
            tube_radius,
            segments,
            tube_segments,
        );
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

    /// Spawn a GLTF model from file. Handles both static and skinned meshes.
    ///
    /// # Arguments
    /// * `path` - Path to the GLTF file
    /// * `position` - World position to spawn at
    /// * `default_animation` - Optional animation name to play automatically
    ///
    /// # Returns
    /// The entity ID of the spawned model.
    ///
    /// # Errors
    /// Returns `AppError::ShaderCompileFailed` if the PBR shader fails to compile.
    /// Returns `AppError::SkeletonCreateFailed` if GPU skeleton creation fails for skinned models.
    pub fn spawn_gltf_model(
        &mut self,
        path: impl AsRef<std::path::Path>,
        position: [f32; 3],
        default_animation: Option<&str>,
    ) -> crate::error::AppResult<katla_ecs::EntityId> {
        use crate::components::{DrawableComponent, TransformComponent};
        use crate::error::AppError;
        use katla_math::Vec3;

        // 1. Load model from cache
        let path_buf = path.as_ref().to_path_buf();
        let path_display = path.as_ref().to_string_lossy().to_string();
        let model = self.gltf_cache.read(path_buf)?;

        // 2. Convert indices to u32 (generate sequential indices for non-indexed geometry)
        let vertex_count = if model.has_skinning {
            model.skinned_vertex_data.len()
        } else {
            model.vertex_data.len()
        };
        let indices = Self::convert_indices_to_u32_with_vertex_count(
            &model.index_data,
            model.index_stride,
            vertex_count,
        );

        debug!(
            "Model '{}' index conversion: {} bytes input (stride {}), {} indices output, {} vertices",
            path.as_ref().display(),
            model.index_data.len(),
            model.index_stride,
            indices.len(),
            vertex_count
        );

        // 3. Create mesh using interleaved vertex data via create_mesh_dynamic
        let vertex_bytes: &[u8] = if model.has_skinning {
            bytemuck::cast_slice(&model.skinned_vertex_data)
        } else {
            bytemuck::cast_slice(&model.vertex_data)
        };
        let mesh_handle =
            self.renderer
                .create_mesh_dynamic(vertex_bytes, vertex_count as u32, &indices);

        let positions: Vec<[f32; 3]> = if model.has_skinning {
            model
                .skinned_vertex_data
                .iter()
                .map(|v| v.position)
                .collect()
        } else {
            model.vertex_data.iter().map(|v| v.position).collect()
        };
        let triangles = indices
            .chunks_exact(3)
            .map(|c| [c[0], c[1], c[2]])
            .collect();
        self.geometry_cache.insert(
            mesh_handle,
            crate::geometry_cache::MeshGeometryData {
                positions,
                triangles,
            },
        );

        // 4. Create material (skinned or regular)
        let shader_path = if model.has_skinning {
            self.resources.shader_path("model_pbr_skinned.wgsl")
        } else {
            self.resources.shader_path("model_pbr.wgsl")
        };
        let vertex_type_str = if model.has_skinning { "skinned" } else { "pbr" };
        let shader_str = shader_path.to_string_lossy();
        let material_handle = self
            .renderer
            .compile_material(&shader_str, vertex_type_str)
            .map_err(|e| AppError::ShaderCompileFailed {
                path: path_display.clone(),
                reason: format!("{e}"),
            })?;

        // 5. Upload textures and set texture indices
        let texture_upload = self.upload_gltf_textures(&model);

        // Track texture handles for GPU cleanup on scene load/entity destruction
        for handle in &texture_upload.handles {
            self.gpu_resource_tracker.track_texture(*handle);
        }

        // Set texture indices on material (only first 4: albedo, normal, mr, ao)
        self.renderer.set_material_texture_indices(
            material_handle,
            [
                texture_upload.indices[0],
                texture_upload.indices[1],
                texture_upload.indices[2],
                texture_upload.indices[3],
            ],
        );

        // 6. Spawn entity with emission texture index
        let entity = self.world.spawn((
            TransformComponent::from_position(Vec3::new(position[0], position[1], position[2])),
            DrawableComponent::with_handles(mesh_handle, material_handle),
        ));

        self.world.add_component(
            entity,
            EntitySource::GltfModel {
                path: path.as_ref().to_string_lossy().to_string(),
            },
        );
        self.world.add_component(
            entity,
            crate::components::NameComponent::new(
                std::path::Path::new(path.as_ref())
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Model"),
            ),
        );

        // Set emission texture index on drawable component
        if let Some(drawable) = self.world.get_component_mut::<DrawableComponent>(entity) {
            drawable.emission = texture_upload.indices[4] as f32;

            if texture_upload.indices[4] > 0 {
                info!(
                    "Model '{}' has emission texture at bindless index {}",
                    path.as_ref().display(),
                    texture_upload.indices[4]
                );
            }
        }

        // 7. If skinned, set up animation
        if model.has_skinning {
            // Get joint count from skin
            let joint_count = model
                .document
                .skins()
                .next()
                .map(|s| s.joints().count())
                .unwrap_or(0);

            if joint_count > 0 {
                // Create GPU skeleton
                let skeleton_handle = self.renderer.create_skeleton(joint_count).map_err(|e| {
                    AppError::SkeletonCreateFailed {
                        path: path_display.clone(),
                        reason: format!("{e}"),
                    }
                })?;

                // Add skeleton handle to drawable
                if let Some(drawable) = self.world.get_component_mut::<DrawableComponent>(entity) {
                    drawable.skeleton_handle = skeleton_handle;
                }

                // Set up CPU animation components
                crate::animation::AnimationManager::setup_animated_model(
                    &mut self.world,
                    entity,
                    &model,
                    default_animation,
                );

                info!(
                    "Spawned animated model '{}' with {} joints",
                    path.as_ref().display(),
                    joint_count
                );
            }
        } else {
            info!("Spawned static model '{}'", path.as_ref().display());
        }

        // Track drawable GPU resources for cleanup on entity destruction
        if let Some(drawable) = self.world.get_component::<DrawableComponent>(entity) {
            self.gpu_resource_tracker.track_drawable(
                drawable.mesh_handle,
                drawable.material_handle,
                drawable.skeleton_handle,
            );
        }

        Ok(entity)
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
                path: path.as_ref().to_string_lossy().to_string(),
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

    /// Upload textures from a GLTF model and return bindless texture indices.
    ///
    /// Returns [albedo, normal, metallic_roughness, ao, emission] indices.
    fn upload_gltf_textures(&mut self, model: &crate::util::GLTFModel) -> GltfTextureUpload {
        let default_index = 0u32;
        let mut albedo_index = default_index;
        let mut normal_index = default_index;
        let mut mr_index = default_index;
        let mut ao_index = default_index;
        let mut emission_index = default_index;
        let mut handles = Vec::new();

        let material_info = model.materials.first();

        if let Some(mat) = material_info {
            if let Some(tex_idx) = mat.base_color_texture
                && let Some(image) = model.images.get(tex_idx)
            {
                let handle = self.upload_gltf_image(image, true);
                albedo_index = self.get_bindless_index(handle);
                handles.push(handle);
                debug!(
                    "Uploaded albedo texture {} -> bindless {}",
                    tex_idx, albedo_index
                );
            }

            if let Some(tex_idx) = mat.normal_texture
                && let Some(image) = model.images.get(tex_idx)
            {
                let handle = self.upload_gltf_image(image, false);
                normal_index = self.get_bindless_index(handle);
                handles.push(handle);
                debug!(
                    "Uploaded normal texture {} -> bindless {}",
                    tex_idx, normal_index
                );
            }

            if let Some(tex_idx) = mat.metallic_roughness_texture
                && let Some(image) = model.images.get(tex_idx)
            {
                let handle = self.upload_gltf_image(image, false);
                mr_index = self.get_bindless_index(handle);
                handles.push(handle);
                debug!("Uploaded MR texture {} -> bindless {}", tex_idx, mr_index);
            }

            if let Some(tex_idx) = mat.occlusion_texture
                && let Some(image) = model.images.get(tex_idx)
            {
                let handle = self.upload_gltf_image(image, false);
                ao_index = self.get_bindless_index(handle);
                handles.push(handle);
                debug!("Uploaded AO texture {} -> bindless {}", tex_idx, ao_index);
            }

            if let Some(tex_idx) = mat.emission_texture
                && let Some(image) = model.images.get(tex_idx)
            {
                let handle = self.upload_gltf_image(image, false);
                emission_index = self.get_bindless_index(handle);
                handles.push(handle);
                debug!(
                    "Uploaded emissive texture {} -> bindless {}",
                    tex_idx, emission_index
                );
            }
        }

        GltfTextureUpload {
            indices: [
                albedo_index,
                normal_index,
                mr_index,
                ao_index,
                emission_index,
            ],
            handles,
        }
    }

    /// Upload a single GLTF image to the GPU.
    fn upload_gltf_image(
        &mut self,
        image: &gltf::image::Data,
        srgb: bool,
    ) -> katla_gfx::TextureHandle {
        let pixels = if image.format == gltf::image::Format::R8G8B8 {
            let mut rgba = Vec::with_capacity(image.pixels.len() / 3 * 4);
            for chunk in image.pixels.chunks(3) {
                rgba.extend_from_slice(&[chunk[0], chunk[1], chunk[2], 255]);
            }
            rgba
        } else {
            image.pixels.clone()
        };

        if srgb {
            let desc = katla_gfx::TextureDescriptor::rgba8_srgb(image.width, image.height);
            self.renderer.create_texture(&desc, &pixels)
        } else {
            let desc = katla_gfx::TextureDescriptor::rgba8_unorm(image.width, image.height);
            self.renderer.create_texture(&desc, &pixels)
        }
    }

    /// Get the bindless texture index for a texture handle.
    fn get_bindless_index(&self, handle: katla_gfx::TextureHandle) -> u32 {
        // The texture manager assigns bindless indices during texture creation
        // We need to query the texture manager for the bindless slot
        self.renderer.get_texture_bindless_index(handle)
    }

    /// Convert index data from bytes to u32 based on stride.
    ///
    /// For non-indexed geometry (empty index_data), generates sequential indices
    /// [0, 1, 2, ... vertex_count-1] for the given vertex count.
    pub(crate) fn convert_indices_to_u32_with_vertex_count(
        index_data: &[u8],
        index_stride: u8,
        vertex_count: usize,
    ) -> Vec<u32> {
        if index_data.is_empty() || index_stride == 0 {
            // Generate sequential indices for non-indexed geometry
            return (0..vertex_count as u32).collect();
        }

        match index_stride {
            1 => index_data.iter().map(|&b| b as u32).collect(),
            2 => index_data
                .chunks(2)
                .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]) as u32)
                .collect(),
            4 => index_data
                .chunks(4)
                .map(|chunk| u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                .collect(),
            _ => Vec::new(),
        }
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
