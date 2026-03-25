use katla_gfx::TextureHandle;
use log::{debug, info};

use crate::components::TransformComponent;
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

        let entity = self.world.spawn((
            TransformComponent {
                transform: katla_math::Transform::new_from_position(Vec3::new(
                    position[0],
                    position[1],
                    position[2],
                )),
            },
            DrawableComponent::with_handles_and_color(mesh_handle, material_handle, linear_color),
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

        let entity = self.world.spawn((
            TransformComponent {
                transform: katla_math::Transform::new_from_position(Vec3::new(
                    position[0],
                    position[1],
                    position[2],
                )),
            },
            DrawableComponent::with_handles_and_material(
                mesh_handle,
                material_handle,
                Some(linear_color),
                metallic,
                roughness,
                1.0,
            ),
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

    /// Spawn a grid of spheres showcasing PBR material properties.
    pub fn spawn_pbr_material_grid(
        &mut self,
        center: [f32; 3],
        grid_size: usize,
        sphere_radius: f32,
        spacing: f32,
    ) {
        use katla_math::Color;

        let half_grid = (grid_size - 1) as f32 / 2.0;

        for y in 0..grid_size {
            for x in 0..grid_size {
                let metallic = y as f32 / (grid_size - 1).max(1) as f32;
                let roughness = x as f32 / (grid_size - 1).max(1) as f32;

                let pos_x = center[0] + (x as f32 - half_grid) * spacing;
                let pos_y = center[1] + (y as f32 - half_grid) * spacing;
                let pos_z = center[2];

                let base_color = Color::rgb(0.4 + metallic * 0.2, 0.6 + metallic * 0.2, 1.0);

                self.spawn_sphere_with_material(
                    [pos_x, pos_y, pos_z],
                    sphere_radius,
                    32,
                    16,
                    base_color,
                    metallic,
                    roughness,
                );
            }
        }

        info!(
            "Spawned PBR material grid ({}x{}) at {:?}",
            grid_size, grid_size, center
        );
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

    /// Set up a default test scene with various primitives.
    ///
    /// Creates a visually interesting scene with multiple objects for testing.
    pub fn setup_default_scene(&mut self) {
        use katla_math::Color;

        info!("Setting up default scene...");

        // Ground plane - nice dark gray
        self.spawn_plane_with_color([0.0, -1.0, 0.0], 20.0, 20.0, Color::from_u8(40, 44, 52));

        // PBR material grid - metallic (Y) x roughness (X)
        self.spawn_pbr_material_grid([0.0, 2.0, -6.0], 5, 0.4, 1.2);

        // Center cube - vibrant coral/orange
        self.spawn_test_cube_with_color(
            [-5.0, 0.0, -5.0],
            [1.0, 1.0, 1.0],
            Color::from_u8(255, 120, 80),
        );

        // Sphere to the left - bright cyan
        self.spawn_sphere_with_color([-7.0, 0.0, -5.0], 0.7, 32, 16, Color::from_u8(80, 220, 255));

        // Cylinder to the right - magenta/pink
        self.spawn_cylinder_with_color(
            [5.0, 0.0, -5.0],
            1.5,
            0.5,
            32,
            Color::from_u8(255, 80, 200),
        );

        // Torus in front - lime green
        self.spawn_torus_with_color(
            [7.0, 0.5, -3.0],
            0.8,
            0.2,
            32,
            16,
            Color::from_u8(150, 255, 100),
        );

        // Distant plane as backdrop - deep purple/blue
        self.spawn_plane_with_color([0.0, 2.0, -10.0], 15.0, 8.0, Color::from_u8(60, 40, 100));

        // Add animated Fox - scale down and position
        if let Some(fox) =
            self.spawn_gltf_model("resources/models/Fox.glb", [3.0, 0.0, 0.0], Some("Run"))
        {
            // Scale down the fox (it's huge by default)
            if let Some(transform) = self.world.get_component_mut::<TransformComponent>(fox) {
                transform.transform.scale = katla_math::Vec3::new(0.01, 0.01, 0.01);
            }
            info!("Spawned animated Fox with Run animation");
        }

        // Add DamagedHelmet - position for viewing
        if let Some(helmet) =
            self.spawn_gltf_model("resources/models/DamagedHelmet.glb", [0.0, 1.5, -5.0], None)
        {
            // Scale the helmet appropriately
            if let Some(transform) = self.world.get_component_mut::<TransformComponent>(helmet) {
                transform.transform.scale = katla_math::Vec3::new(1.0, 1.0, 1.0);
            }
            info!("Spawned DamagedHelmet model");
        }

        // Add particle emitters
        self.setup_particle_emitters();

        // Add point lights for Forward+ dynamic lighting
        self.setup_point_lights();

        info!(
            "Default scene setup complete - {} entities spawned with particle effects",
            self.world.entity_ids().count()
        );
    }

    /// Set up particle emitters for the default scene.
    fn setup_particle_emitters(&mut self) {
        use crate::components::ParticleEmitterComponent;
        use katla_gfx::particles::{EmitterConfig, EmitterShape};

        info!("Setting up particle emitters via ECS...");

        // Fire emitter near the center cube
        let fire_emitter = ParticleEmitterComponent::with_config(EmitterConfig {
            position: [-3.0, 1.0, -3.0],
            emit_rate: 400.0,
            base_lifetime: 2.5,
            lifetime_variation: 0.3,
            velocity_direction: [0.0, 1.0, 0.0],
            velocity_magnitude: 3.0,
            velocity_cone_angle: 0.05,
            base_scale: 0.08,
            scale_variation: 0.2,
            color: [1.0, 0.5, 0.0, 1.0],
            color_variation: 0.1,
            gravity: 0.0,
            ..Default::default()
        });
        let fire_entity = self.world.spawn((
            TransformComponent {
                transform: katla_math::Transform::new_from_position(katla_math::Vec3::new(
                    -3.0, 1.0, -3.0,
                )),
            },
            fire_emitter,
        ));
        self.world
            .add_component(fire_entity, EntitySource::ParticleEmitter);

        // Ethereal/spiritual rising particles
        let mut ethereal_emitter = ParticleEmitterComponent::with_config(EmitterConfig {
            position: [3.0, 0.5, 0.0],
            emit_rate: 200.0,
            base_lifetime: 4.0,
            lifetime_variation: 0.5,
            velocity_direction: [0.0, 1.0, 0.0],
            velocity_magnitude: 1.5,
            velocity_cone_angle: 0.1,
            base_scale: 0.12,
            scale_variation: 0.4,
            color: [0.6, 0.8, 1.0, 0.8],
            color_variation: 0.2,
            gravity: -0.5,
            turbulence_strength: 4.0,
            turbulence_frequency: 3.0,
            ..Default::default()
        });
        ethereal_emitter.config.set_shape(EmitterShape::Circle);
        ethereal_emitter.config.shape_params = [2.0, 0.0, 0.0, 0.0];
        let ethereal_entity = self.world.spawn((
            TransformComponent {
                transform: katla_math::Transform::new_from_position(katla_math::Vec3::new(
                    3.0, 0.5, 0.0,
                )),
            },
            ethereal_emitter,
        ));
        self.world
            .add_component(ethereal_entity, EntitySource::ParticleEmitter);

        // Magic sparkles emitter
        let sparkle_emitter = ParticleEmitterComponent::with_config(EmitterConfig {
            position: [0.0, 3.0, 0.0],
            emit_rate: 250.0,
            base_lifetime: 3.0,
            lifetime_variation: 1.0,
            velocity_direction: [0.0, -1.0, 0.0],
            velocity_magnitude: 0.5,
            velocity_cone_angle: 0.1,
            base_scale: 0.1,
            scale_variation: 0.5,
            color: [0.8, 0.9, 1.0, 1.0],
            color_variation: 0.3,
            ..Default::default()
        });
        let sparkle_entity = self.world.spawn((
            TransformComponent {
                transform: katla_math::Transform::new_from_position(katla_math::Vec3::new(
                    0.0, 3.0, 0.0,
                )),
            },
            sparkle_emitter,
        ));
        self.world
            .add_component(sparkle_entity, EntitySource::ParticleEmitter);

        info!("Particle emitters setup complete - emitters will be initialized by ParticleSystem");
    }

    /// Set up point lights for Forward+ dynamic lighting demonstration.
    fn setup_point_lights(&mut self) {
        use crate::components::{DrawableComponent, PointLight, TransformComponent};

        info!("Setting up point lights...");

        let mesh_handle = self.renderer.create_sphere_mesh(0.2, 16, 12);
        let material_handle = self.default_material();

        let make_indicator = |color: katla_math::Color| {
            DrawableComponent::with_handles_and_material(
                mesh_handle,
                material_handle,
                Some(color),
                0.0, // fully dielectric
                1.0, // fully rough (matte light bulb look)
                1.0,
            )
        };

        // Warm light near the coral cube
        let warm_light = self.world.spawn((
            PointLight::new([1.0, 0.6, 0.2], 15.0, 12.0),
            TransformComponent {
                transform: katla_math::Transform::new_from_position(katla_math::Vec3::new(
                    -5.0, 3.0, -3.0,
                )),
            },
            make_indicator(katla_math::Color::rgb(1.0, 0.6, 0.2)),
        ));
        self.world.add_component(warm_light, EntitySource::Light);

        // Cool blue light near the sphere
        let cool_light = self.world.spawn((
            PointLight::new([0.3, 0.5, 1.0], 12.0, 10.0),
            TransformComponent {
                transform: katla_math::Transform::new_from_position(katla_math::Vec3::new(
                    -7.0, 2.0, -4.0,
                )),
            },
            make_indicator(katla_math::Color::rgb(0.3, 0.5, 1.0)),
        ));
        self.world.add_component(cool_light, EntitySource::Light);

        // Magenta light near the cylinder
        let magenta_light = self.world.spawn((
            PointLight::new([1.0, 0.2, 0.8], 14.0, 10.0),
            TransformComponent {
                transform: katla_math::Transform::new_from_position(katla_math::Vec3::new(
                    5.0, 2.5, -3.0,
                )),
            },
            make_indicator(katla_math::Color::rgb(1.0, 0.2, 0.8)),
        ));
        self.world.add_component(magenta_light, EntitySource::Light);

        // Green light near the torus
        let green_light = self.world.spawn((
            PointLight::new([0.3, 1.0, 0.4], 10.0, 8.0),
            TransformComponent {
                transform: katla_math::Transform::new_from_position(katla_math::Vec3::new(
                    7.0, 1.5, -1.0,
                )),
            },
            make_indicator(katla_math::Color::rgb(0.3, 1.0, 0.4)),
        ));
        self.world.add_component(green_light, EntitySource::Light);

        // White overhead light for general illumination
        let overhead_light = self.world.spawn((
            PointLight::new([0.9, 0.85, 0.8], 8.0, 15.0),
            TransformComponent {
                transform: katla_math::Transform::new_from_position(katla_math::Vec3::new(
                    0.0, 6.0, -3.0,
                )),
            },
            make_indicator(katla_math::Color::rgb(0.9, 0.85, 0.8)),
        ));
        self.world
            .add_component(overhead_light, EntitySource::Light);

        // Name them for the editor hierarchy
        use crate::components::NameComponent;
        if let Some(name) = self.world.get_component_mut::<NameComponent>(warm_light) {
            name.name = "Warm Point Light".to_string();
        }
        if let Some(name) = self.world.get_component_mut::<NameComponent>(cool_light) {
            name.name = "Cool Point Light".to_string();
        }
        if let Some(name) = self.world.get_component_mut::<NameComponent>(magenta_light) {
            name.name = "Magenta Point Light".to_string();
        }
        if let Some(name) = self.world.get_component_mut::<NameComponent>(green_light) {
            name.name = "Green Point Light".to_string();
        }
        if let Some(name) = self
            .world
            .get_component_mut::<NameComponent>(overhead_light)
        {
            name.name = "Overhead Point Light".to_string();
        }

        info!("5 point lights created for Forward+ dynamic lighting");
    }
}
