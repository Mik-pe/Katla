use katla_gfx::TextureHandle;
use log::{debug, info};

use crate::scene::entity_source::EntitySource;

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

        let drawable =
            DrawableComponent::with_handles_and_color(mesh_handle, material_handle, linear_color);
        self.gpu_resource_tracker.track_drawable(
            mesh_handle,
            material_handle,
            drawable.skeleton_handle,
        );

        let entity = self.world.spawn((
            TransformComponent {
                transform: katla_math::Transform::new_from_position(Vec3::new(
                    position[0],
                    position[1],
                    position[2],
                )),
            },
            drawable,
        ));

        self.world.add_component(entity, source);
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
        let mesh_handle = self.renderer.create_cube_mesh(size);
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
        let mesh_handle = self.renderer.create_sphere_mesh(radius, segments, rings);
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
    #[allow(clippy::too_many_arguments)]
    pub fn spawn_sphere_with_material(
        &mut self,
        position: [f32; 3],
        radius: f32,
        segments: u32,
        rings: u32,
        color: katla_math::Color,
        metallic: f32,
        roughness: f32,
    ) -> katla_ecs::EntityId {
        use crate::components::{DrawableComponent, TransformComponent};
        use katla_math::Vec3;

        let mesh_handle = self.renderer.create_sphere_mesh(radius, segments, rings);
        let material_handle = self.default_material();
        let linear_color = color.to_linear();

        let drawable = DrawableComponent::with_handles_and_material(
            mesh_handle,
            material_handle,
            Some(linear_color),
            metallic,
            roughness,
            1.0,
        );
        self.gpu_resource_tracker.track_drawable(
            mesh_handle,
            material_handle,
            drawable.skeleton_handle,
        );

        let entity = self.world.spawn((
            TransformComponent {
                transform: katla_math::Transform::new_from_position(Vec3::new(
                    position[0],
                    position[1],
                    position[2],
                )),
            },
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
        let mesh_handle = self.renderer.create_cylinder_mesh(height, radius, segments);
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
        let mesh_handle = self.renderer.create_plane_mesh(width, height);
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
        let mesh_handle =
            self.renderer
                .create_torus_mesh(radius, tube_radius, segments, tube_segments);
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
    /// The entity ID of the spawned model, or None if loading failed
    pub fn spawn_gltf_model(
        &mut self,
        path: impl AsRef<std::path::Path>,
        position: [f32; 3],
        default_animation: Option<&str>,
    ) -> Option<katla_ecs::EntityId> {
        use crate::components::{DrawableComponent, TransformComponent};
        use katla_math::Vec3;

        // 1. Load model from cache
        let path_buf = path.as_ref().to_path_buf();
        let model = self.gltf_cache.read(path_buf);

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

        // 3. Create mesh (skinned or regular) using SOA attribute buffers
        let mesh_handle = if model.has_skinning {
            self.renderer.create_mesh_soa(
                &model.skinned_vertex_attributes,
                model.skinned_vertex_data.len() as u32,
                &indices,
            )
        } else {
            self.renderer.create_mesh_soa(
                &model.vertex_attributes,
                model.vertex_data.len() as u32,
                &indices,
            )
        };

        // 4. Create material (skinned or regular)
        let shader_path = if model.has_skinning {
            self.resources.shader_path("model_pbr_skinned.wgsl")
        } else {
            self.resources.shader_path("model_pbr.wgsl")
        };

        let material_handle = self
            .renderer
            .compile_material(
                &shader_path,
                katla_gfx::MaterialOptions {
                    vertex_type: if model.has_skinning {
                        katla_gfx::VertexType::Skinned
                    } else {
                        katla_gfx::VertexType::Pbr
                    },
                    color_format: katla_gfx::ImageFormat::R16G16B16A16Sfloat,
                    ..Default::default()
                },
            )
            .ok()?;

        // 5. Upload textures and set texture indices
        let texture_indices = self.upload_gltf_textures(&model);

        // Set texture indices on material (only first 4: albedo, normal, mr, ao)
        self.renderer.set_material_texture_indices(
            material_handle,
            [
                texture_indices[0],
                texture_indices[1],
                texture_indices[2],
                texture_indices[3],
            ],
        );

        // 6. Spawn entity with emission texture index
        let entity = self.world.spawn((
            TransformComponent {
                transform: katla_math::Transform::new_from_position(Vec3::new(
                    position[0],
                    position[1],
                    position[2],
                )),
            },
            DrawableComponent::with_handles(mesh_handle, material_handle),
        ));

        self.world.add_component(
            entity,
            EntitySource::GltfModel {
                path: path.as_ref().to_string_lossy().to_string(),
            },
        );

        // Set emission texture index on drawable component
        if let Some(drawable) = self.world.get_component_mut::<DrawableComponent>(entity) {
            drawable.emission = texture_indices[4] as f32;

            // Log if we have an emission texture
            if texture_indices[4] > 0 {
                info!(
                    "Model '{}' has emission texture at bindless index {}",
                    path.as_ref().display(),
                    texture_indices[4]
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
                let skeleton_handle = self.renderer.create_skeleton(joint_count).ok()?;

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

        Some(entity)
    }

    /// Upload textures from a GLTF model and return bindless texture indices.
    ///
    /// Returns [albedo, normal, metallic_roughness, ao, emission] indices.
    fn upload_gltf_textures(&mut self, model: &crate::util::GLTFModel) -> [u32; 5] {
        let default_index = 0u32; // Default white texture
        let mut albedo_index = default_index;
        let mut normal_index = default_index;
        let mut mr_index = default_index;
        let mut ao_index = default_index;
        let mut emission_index = default_index;

        // Get first material if available
        let material_info = model.materials.first();

        if let Some(mat) = material_info {
            // Upload albedo texture
            if let Some(tex_idx) = mat.base_color_texture
                && let Some(image) = model.images.get(tex_idx)
            {
                let handle = self.upload_gltf_image(image, true);
                albedo_index = self.get_bindless_index(handle);
                debug!(
                    "Uploaded albedo texture {} -> bindless {}",
                    tex_idx, albedo_index
                );
            }

            // Upload normal texture
            if let Some(tex_idx) = mat.normal_texture
                && let Some(image) = model.images.get(tex_idx)
            {
                let handle = self.upload_gltf_image(image, false);
                normal_index = self.get_bindless_index(handle);
                debug!(
                    "Uploaded normal texture {} -> bindless {}",
                    tex_idx, normal_index
                );
            }

            // Upload metallic/roughness texture
            if let Some(tex_idx) = mat.metallic_roughness_texture
                && let Some(image) = model.images.get(tex_idx)
            {
                let handle = self.upload_gltf_image(image, false);
                mr_index = self.get_bindless_index(handle);
                debug!("Uploaded MR texture {} -> bindless {}", tex_idx, mr_index);
            }

            // Upload AO texture
            if let Some(tex_idx) = mat.occlusion_texture
                && let Some(image) = model.images.get(tex_idx)
            {
                let handle = self.upload_gltf_image(image, false);
                ao_index = self.get_bindless_index(handle);
                debug!("Uploaded AO texture {} -> bindless {}", tex_idx, ao_index);
            }

            // Upload emissive texture
            if let Some(tex_idx) = mat.emission_texture
                && let Some(image) = model.images.get(tex_idx)
            {
                let handle = self.upload_gltf_image(image, false);
                emission_index = self.get_bindless_index(handle);
                debug!(
                    "Uploaded emissive texture {} -> bindless {}",
                    tex_idx, emission_index
                );
            }
        }

        [
            albedo_index,
            normal_index,
            mr_index,
            ao_index,
            emission_index,
        ]
    }

    /// Upload a single GLTF image to the GPU.
    fn upload_gltf_image(&mut self, image: &gltf::image::Data, srgb: bool) -> TextureHandle {
        // Convert RGB to RGBA if needed (Vulkan requires 4-channel alignment)
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
    fn convert_indices_to_u32_with_vertex_count(
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
