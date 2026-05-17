//! Editor play mode state machine and scene snapshot for play/stop cycle.

use log::info;

use crate::scene::SceneManager;

/// Editor play mode state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayMode {
    Editing,
    Playing,
    Paused,
}

/// Serialized snapshot of all non-hidden entities in the world.
///
/// Captured before entering play mode and restored when play mode stops,
/// so any runtime mutations during gameplay are discarded.
pub(crate) struct SceneSnapshot {
    ron_data: String,
}

impl SceneSnapshot {
    /// Serialize all non-hidden entities into an in-memory RON snapshot.
    pub fn capture(app: &crate::application::Application) -> Self {
        let scene = SceneManager::save_scene(app);
        let ron_data = ron::ser::to_string_pretty(&scene, crate::scene::ron_pretty_config())
            .expect("Scene serialization should not fail for in-memory snapshot");
        info!(
            "Captured scene snapshot ({} bytes, {} entities)",
            ron_data.len(),
            scene.entities.len()
        );
        Self { ron_data }
    }

    /// Restore the world to the snapshotted state.
    ///
    /// Destroys all non-hidden entities, releases GPU resources,
    /// then re-spawns from the snapshot.
    pub fn restore(self, app: &mut crate::application::Application) {
        use crate::components::{EditorHidden, ParticleEmitterComponent};
        use katla_ecs::EntityId;

        let scene: crate::scene::Scene = ron::from_str(&self.ron_data)
            .expect("Scene deserialization should not fail for in-memory snapshot");
        info!(
            "Restoring scene snapshot ({} entities)",
            scene.entities.len()
        );

        // Collect non-hidden entities to destroy
        let to_remove: Vec<EntityId> = app
            .world
            .entity_ids()
            .filter(|id| app.world.get_component::<EditorHidden>(*id).is_none())
            .collect();

        // Clean up particle emitters
        for id in &to_remove {
            if let Some(emitter) = app.world.get_component_mut::<ParticleEmitterComponent>(*id)
                && let Some(handle) = emitter.emitter_handle.take()
                && let Some(ps) = &mut app.renderer.particle_system
            {
                ps.destroy_emitter(handle);
            }
        }

        // Release GPU resources
        let to_destroy = app.gpu_resource_tracker.release_all();
        for handle in &to_destroy.meshes {
            app.renderer.destroy_mesh(*handle);
        }
        for handle in &to_destroy.materials {
            app.renderer.destroy_material(*handle);
        }
        for handle in &to_destroy.textures {
            app.renderer.destroy_texture(*handle);
        }
        for handle in &to_destroy.skeletons {
            app.renderer.destroy_skeleton(*handle);
        }

        // Destroy entities
        for id in to_remove {
            app.world.destroy_entity(id);
        }

        // Re-create camera (destroyed above)
        app.camera = crate::application::camera::Camera::new(&mut app.world);

        // Re-spawn entities from snapshot using a minimal path that
        // avoids wait_for_device (we already cleaned up above)
        let mut name_to_entity: std::collections::HashMap<String, katla_ecs::EntityId> =
            std::collections::HashMap::new();
        let mut spawned_ids: Vec<katla_ecs::EntityId> = Vec::with_capacity(scene.entities.len());

        for desc in &scene.entities {
            let entity_id = Self::spawn_from_descriptor(app, desc);
            spawned_ids.push(entity_id);
            if let Some(ref name) = desc.name {
                name_to_entity.entry(name.clone()).or_insert(entity_id);
            }
        }

        // Resolve parent relationships
        for (idx, desc) in scene.entities.iter().enumerate() {
            let child_id = spawned_ids[idx];
            if let Some(ref parent_name) = desc.parent {
                if let Some(&parent_id) = name_to_entity.get(parent_name) {
                    app.world
                        .add_component(child_id, crate::components::Parent::new(parent_id));
                    if let Some(children) = app
                        .world
                        .get_component_mut::<crate::components::Children>(parent_id)
                    {
                        children.children.push(child_id);
                    } else {
                        app.world.add_component(
                            parent_id,
                            crate::components::Children::new(vec![child_id]),
                        );
                    }
                }
            }
        }

        info!(
            "Scene restored from snapshot ({} entities)",
            spawned_ids.len()
        );
    }

    /// Spawn a single entity from a descriptor (mirrors SceneManager::spawn_entity).
    fn spawn_from_descriptor(
        app: &mut crate::application::Application,
        desc: &crate::scene::EntityDescriptor,
    ) -> katla_ecs::EntityId {
        use crate::components::{
            DrawableComponent, NameComponent, ParticleEmitterComponent, PointLight,
            TransformComponent,
        };
        use crate::scene::entity_source::EntitySource;

        let pos = desc.transform.position;
        let entity_id = if desc.source.is_mesh_primitive() {
            let mesh_handle = match &desc.source {
                EntitySource::Cube { size } => app.renderer.create_cube_mesh(*size),
                EntitySource::Sphere {
                    radius,
                    segments,
                    rings,
                    ..
                } => app.renderer.create_sphere_mesh(*radius, *segments, *rings),
                EntitySource::Plane { width, height } => {
                    app.renderer.create_plane_mesh(*width, *height)
                }
                EntitySource::Cylinder {
                    height,
                    radius,
                    segments,
                    ..
                } => app
                    .renderer
                    .create_cylinder_mesh(*height, *radius, *segments),
                EntitySource::Torus {
                    radius,
                    tube_radius,
                    segments,
                    tube_segments,
                    ..
                } => {
                    app.renderer
                        .create_torus_mesh(*radius, *tube_radius, *segments, *tube_segments)
                }
                _ => unreachable!(),
            };

            let material_handle = app.default_material();
            let srgb_color = desc
                .drawable
                .as_ref()
                .and_then(|d| d.color)
                .map(|c| katla_math::Color::new(c[0], c[1], c[2], c[3]))
                .unwrap_or(katla_math::Color::WHITE);
            let linear_color = srgb_color.to_linear();

            let drawable = DrawableComponent::with_handles_and_color(
                mesh_handle,
                material_handle,
                linear_color,
            );
            app.gpu_resource_tracker.track_drawable(
                mesh_handle,
                material_handle,
                drawable.skeleton_handle,
            );

            app.world.spawn((
                TransformComponent::from_position(katla_math::Vec3::new(pos[0], pos[1], pos[2])),
                drawable,
            ))
        } else {
            match &desc.source {
                EntitySource::Light => {
                    let point_light = desc
                        .point_light
                        .as_ref()
                        .map(|pl| PointLight::new(pl.color, pl.intensity, pl.range))
                        .unwrap_or_default();
                    let transform = TransformComponent::from_position(katla_math::Vec3::new(
                        pos[0], pos[1], pos[2],
                    ));
                    let entity_id = app.world.spawn((transform, point_light));
                    app.attach_billboard_icon(
                        entity_id,
                        crate::components::billboard::BillboardIcon::Lightbulb,
                    );
                    entity_id
                }
                EntitySource::ParticleEmitter => {
                    let config = katla_gfx::particles::EmitterConfig {
                        position: desc
                            .particle_emitter
                            .as_ref()
                            .map(|p| p.position)
                            .unwrap_or(pos),
                        ..Default::default()
                    };
                    let mut emitter = ParticleEmitterComponent::with_config(config);
                    if let Some(ref pe) = desc.particle_emitter {
                        emitter.active = pe.active;
                    }
                    let transform = TransformComponent::from_position(katla_math::Vec3::new(
                        pos[0], pos[1], pos[2],
                    ));
                    let entity_id = app.world.spawn((transform, emitter));
                    app.attach_billboard_icon(
                        entity_id,
                        crate::components::billboard::BillboardIcon::Fire,
                    );
                    entity_id
                }
                _ => {
                    // Fallback for GLTF models and other sources during restore
                    let transform = TransformComponent::from_position(katla_math::Vec3::new(
                        pos[0], pos[1], pos[2],
                    ));
                    app.world.spawn((transform,))
                }
            }
        };

        // Apply rotation and scale
        let (qx, qy, qz, qw) = (
            desc.transform.rotation[0],
            desc.transform.rotation[1],
            desc.transform.rotation[2],
            desc.transform.rotation[3],
        );
        if let Some(transform) = app.world.get_component_mut::<TransformComponent>(entity_id) {
            transform.transform.rotation = katla_math::Quat::new(qx, qy, qz, qw);
            transform.transform.scale = katla_math::Vec3::new(
                desc.transform.scale[0],
                desc.transform.scale[1],
                desc.transform.scale[2],
            );
        }

        // Attach script
        if let Some(ref script_desc) = desc.script {
            app.world.add_component(
                entity_id,
                katla_script::ScriptComponent::new(&script_desc.script_path),
            );
        }

        // Attach EntitySource
        app.world.add_component(entity_id, desc.source.clone());

        // Attach name
        if let Some(ref name) = desc.name {
            app.world
                .add_component(entity_id, NameComponent::new(name.clone()));
        }

        entity_id
    }
}
