use log::{error, info, warn};

use katla_gfx::GpuRenderer;
#[cfg(feature = "editor")]
use katla_gfx::primitives;

use crate::application::Application;
use crate::error::AppError;
impl Application {
    pub fn init(&mut self) -> Result<(), AppError> {
        info!("Application::init() called");

        let uses_katla_scene = self.frame_graph_runtime.uses_katla_scene();
        if uses_katla_scene {
            self.world
                .insert_resource(crate::resources::AmbientLight::default());

            self.init_scene_features()?;
        } else {
            info!("Skipping Katla scene renderer initialization for graph-only runtime");
        }

        match crate::systems::AudioSystem::new() {
            Ok(audio) => {
                info!("Audio system initialized");
                let engine = audio.engine();
                engine.set_master_volume(self.preferences.audio.master_volume);
                engine.set_category_volume(
                    katla_audio::AudioCategory::Sfx,
                    self.preferences.audio.sfx_volume,
                );
                engine.set_category_volume(
                    katla_audio::AudioCategory::Music,
                    self.preferences.audio.music_volume,
                );
                engine.set_category_volume(
                    katla_audio::AudioCategory::Ambient,
                    self.preferences.audio.ambient_volume,
                );
                self.audio_system = Some(audio);
            }
            Err(katla_audio::AudioError::DeviceAccessDenied(ref msg)) => {
                warn!("Audio initialization failed (access denied): {msg}");
                warn!("Audio will be unavailable. Grant audio permissions and restart.");
            }
            Err(katla_audio::AudioError::DeviceNotFound(ref msg)) => {
                warn!("Audio initialization failed (no device): {msg}");
                warn!("Audio will be unavailable until an output device is connected.");
            }
            Err(e) => {
                warn!("Audio initialization failed: {e}");
                warn!("Audio will be unavailable.");
            }
        }

        if uses_katla_scene {
            let scene_path_str = self
                .info
                .scene_path
                .clone()
                .unwrap_or_else(|| crate::scene::default_scene_path().display().to_string());
            let scene_path = std::path::Path::new(&scene_path_str);
            match crate::scene::SceneManager::load_from_file(self, scene_path) {
                Ok(()) => info!("Loaded scene from {}", scene_path_str),
                Err(e) => error!("Failed to load scene from {}: {}", scene_path_str, e),
            }
        } else {
            info!("Skipping implicit scene loading for graph-only runtime");
        }

        info!("Application::init() completed");
        Ok(())
    }
}

impl Application {
    fn init_scene_features(&mut self) -> Result<(), AppError> {
        let shader_path = self.resources.shader_path("model_pbr.wgsl");
        self.default_material_handle = self.renderer.compile_material(
            &katla_gfx::PipelineDescriptor::pbr(shader_path.to_string_lossy().into_owned())
                .with_color_format(katla_gfx::ImageFormat::R16G16B16A16Sfloat),
        )?;
        if let Some(features) = &mut self.scene_features {
            self.renderer
                .set_material_textures(self.default_material_handle, features.material_textures());
        }
        self.gpu_resource_tracker
            .set_protected_material(self.default_material_handle);
        self.gpu_animation_system =
            Some(crate::systems::gpu_animation_system::GpuAnimationSystem::new());
        #[cfg(feature = "editor")]
        {
            self.init_gizmo_resources();
            self.init_billboard_resources();
        }
        Ok(())
    }
}

impl Application {
    /// Initialize GPU resources for billboard icons (mesh + material + icon textures).
    #[cfg(feature = "editor")]
    pub(crate) fn init_billboard_resources(&mut self) {
        use crate::billboard::BillboardResources;
        use crate::components::BillboardIcon;

        let mesh = match primitives::create_plane_xy(&mut self.renderer, 1.0, 1.0, 1) {
            Ok(mesh) => mesh,
            Err(error) => {
                log::error!("Failed to create billboard quad mesh: {error}");
                return;
            }
        };

        let shader_path = self.resources.shader_path("billboard.wgsl");
        let descriptor =
            katla_gfx::PipelineDescriptor::billboard(shader_path.to_string_lossy().into_owned())
                .with_color_format(katla_gfx::ImageFormat::R16G16B16A16Sfloat);
        let material = match self.renderer.compile_material(&descriptor) {
            Ok(m) => m,
            Err(e) => {
                log::error!("Failed to create billboard material: {e}");
                return;
            }
        };

        let mut icon_textures = std::collections::HashMap::new();
        for icon in [BillboardIcon::Lightbulb, BillboardIcon::Fire] {
            let rasterized = crate::rendering::rasterize_billboard_icon(icon, 64);
            let desc =
                katla_gfx::TextureDescriptor::rgba8_srgb(rasterized.width, rasterized.height);
            // Icons are optional chrome: a failed upload skips the icon
            // (consumers skip missing entries) instead of failing init.
            match self.renderer.create_texture(&desc, &rasterized.pixels) {
                Ok(texture_handle) => {
                    icon_textures.insert(icon, texture_handle);
                }
                Err(error) => {
                    warn!("Billboard icon {:?} upload failed: {}", icon, error);
                }
            }
        }

        self.editor.billboard_resources = BillboardResources {
            mesh,
            material,
            icon_textures,
            initialized: true,
        };

        info!("Billboard GPU resources initialized");
    }

    #[cfg(feature = "editor")]
    pub(crate) fn focus_camera_on_entity(&mut self, entity_id: katla_ecs::EntityId) {
        use crate::components::OrbitCameraControllerComponent;
        let poses = crate::systems::resolve_world_transforms(&self.world);
        let (center, radius) =
            if let Some(bounds) = crate::systems::subtree_render_bounds(&self.world, entity_id) {
                (bounds.center, bounds.extent.length().max(0.5))
            } else if let Some(pose) = poses.get(&entity_id) {
                (pose.transform.position, 0.5)
            } else {
                return;
            };

        // Distance to fit the object so it covers ~50% of the smaller viewport dimension.
        let camera_entity = self.camera.entity;
        let fov_rad = self
            .world
            .get_component::<crate::components::PerspectiveComponent>(camera_entity)
            .map(|p| p.fov.to_radians())
            .unwrap_or_else(|| 60.0_f32.to_radians());
        let aspect = self
            .world
            .get_component::<crate::components::PerspectiveComponent>(camera_entity)
            .map(|p| p.aspect_ratio)
            .unwrap_or(16.0 / 9.0);
        let target_fraction = 0.5;
        // Vertical FOV covers half_height = distance * tan(fov/2)
        // Horizontal FOV covers half_width = half_height * aspect
        // We want the object to fill target_fraction of the smaller visible extent
        let half_height = 1.0 / fov_rad.tan(); // at distance=1
        let half_width = half_height * aspect;
        let smaller_half = half_height.min(half_width);
        let distance = radius / (target_fraction * smaller_half);

        if let Some(orbit) = self
            .world
            .get_component_mut::<OrbitCameraControllerComponent>(camera_entity)
        {
            orbit.focus = Some(crate::components::camera::orbit_camera::FocusTarget {
                target: center,
                distance: distance.clamp(orbit.min_distance, orbit.max_distance),
                duration: 0.35,
                elapsed: 0.0,
                start_target: orbit.target,
                start_distance: orbit.distance,
                start_yaw: orbit.yaw,
                start_pitch: orbit.pitch,
                target_yaw: orbit.yaw,
                target_pitch: orbit.pitch,
            });
        }
    }
}
