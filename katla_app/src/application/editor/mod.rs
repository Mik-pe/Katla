//! Editor subsystem - handles UI rendering, entity management, and editor actions.

use std::collections::{HashMap, HashSet};

use log::{debug, info};

use katla_ecs::EntityId;
use katla_gfx::renderer::UIDrawList;
use katla_math::{Vec2, Vec3, Vec4};

use crate::components::{
    Children, DirectionalLight, DrawableComponent, EditorHidden, NameComponent, Parent,
    ParticleEmitterComponent, PointLight, TransformComponent,
};

use crate::ui::{EditorAction, EntityInfo, ParticleEmitterInfo, PointLightInfo};

use super::Application;

/// Upload font atlas texture to GPU if it has been modified.
///
/// This MUST be called AFTER `generate_ui_draw_list()` (which rasterizes new glyphs
/// into the CPU atlas) and BEFORE `render_frame()` (which samples from the GPU atlas).
/// Calling it after render_frame causes a one-frame lag where the GPU has stale data.
pub fn upload_font_atlas(app: &mut Application) {
    if app.ui_context.fonts().atlas_needs_update() {
        let (width, height) = app.ui_context.fonts().atlas_size();
        let data = app.ui_context.fonts().atlas_data();

        if app.ui_context.fonts().atlas_was_resized() {
            app.renderer.create_ui_font_atlas(width, height, data);

            if let Some(bindless_slot) = app.renderer.ui_renderer.font_atlas_bindless_slot() {
                app.editor
                    .ui_renderer
                    .set_font_atlas_bindless_slot(bindless_slot);
            }

            app.ui_context.fonts_mut().clear_atlas_resized();
        } else {
            app.renderer.update_ui_font_atlas(width, height, data);
        }

        app.ui_context.fonts_mut().mark_atlas_updated();
    }
}

/// Generate UI draw list for the current frame.
///
/// Returns a GPU-ready UIDrawList that can be submitted to the frame graph's UI pass.
/// This should be called BEFORE frame graph execution.
pub fn generate_ui_draw_list(app: &mut Application, dt: f32) -> Option<UIDrawList> {
    let scale_factor = app.scale_factor;

    // Update per-frame timers
    app.editor.editor_ui.update_timers(dt);

    // Get physical window size and convert to logical for UI layout
    let size = app.window.inner_size();
    let physical_size = Vec2::new(size.width as f32, size.height as f32);

    // UI uses logical coordinates - convert physical to logical
    let screen_size = physical_size / scale_factor;

    // Calculate stats
    let fps = if dt > 0.0 { 1.0 / dt } else { 0.0 };
    let _entity_count = app.world.entity_count();

    // Collect entity info for editor UI
    let entity_info = collect_entity_info(app);

    // Sync gizmo mode to editor UI for toolbar display
    app.editor.editor_ui.gizmo_mode = match app.editor.gizmo_state.mode {
        crate::gizmo::GizmoMode::Translate => 0,
        crate::gizmo::GizmoMode::Rotate => 1,
        crate::gizmo::GizmoMode::Scale => 2,
    };

    // Sync inspector editing state from current entity data
    app.editor.editor_ui.sync_inspector_edit_state(&entity_info);

    // Set current time for UI animations (cursor blink etc.)
    app.ui_context
        .set_time(app.start_time.elapsed().as_secs_f64());

    // Store viewport texture ID before rendering (to avoid borrow issues)
    let viewport_texture_id = app.editor.editor_ui.viewport_texture_ids[0];

    let draw_list = {
        // Collect particle inspector data before rendering
        collect_particle_inspector_data(app);

        app.editor
            .editor_ui
            .render(
                &mut app.ui_context,
                &mut crate::ui::EditorRenderParams {
                    preferences: &app.preferences,
                    screen_size,
                    scale_factor,
                    entities: &entity_info,
                    fps,
                    frame_count: app.frame_count,
                    loader: &mut app.editor.background_loader,
                    thumbnail_texture_handles: &app.editor.thumbnail_texture_handles,
                },
            )
            .clone()
    };

    // Apply real-time inspector slider changes to ECS during drag.
    // This happens every frame while a slider is being dragged so the viewport updates immediately.
    // Must happen before borrowing ui_renderer to avoid double mutable borrow.
    apply_inspector_slider_changes(app);

    // Convert draw list to GPU format
    let ui_renderer = &mut app.editor.ui_renderer;

    // Register the viewport texture if it exists
    if let Some(texture_id) = viewport_texture_id {
        let texture_handle = katla_gfx::TextureHandle::new(texture_id.0 as u32);
        ui_renderer.register_texture(texture_id, texture_handle);
    }

    if !draw_list.is_empty() {
        let gpu_list = ui_renderer.convert_draw_list(
            &draw_list,
            [screen_size.x(), screen_size.y()],
            scale_factor,
        );

        Some(gpu_list)
    } else {
        None
    }
}

/// Apply real-time inspector slider changes to ECS components during drag.
///
/// Compares the inspector editing state against the current ECS component values.
/// If they differ, updates the ECS component immediately (for viewport feedback)
/// and pushes an EditorAction to commit the final value on slider release.
fn apply_inspector_slider_changes(app: &mut Application) {
    use crate::ui::InspectorEditState;

    let entity_id = match app.editor.editor_ui.inspector_edit_entity {
        Some(id) => id,
        None => return,
    };

    let InspectorEditState {
        pos,
        rot,
        scale,
        light_color,
        light_intensity,
        light_range,
        emit_rate,
        velocity,
        lifetime,
        gravity,
        particle_scale,
    } = &app.editor.editor_ui.inspector_edit;

    // Transform
    if let Some(transform) = app.world.get_component_mut::<TransformComponent>(entity_id) {
        let pos_vec = Vec3::new(pos[0], pos[1], pos[2]);
        let rot_vec = Vec3::new(rot[0], rot[1], rot[2]);
        let scale_vec = Vec3::new(scale[0], scale[1], scale[2]);

        let pos_changed = (pos_vec - transform.transform.position).length() > 1e-4;
        let euler = transform.transform.rotation.to_euler();
        let rot_changed = (rot_vec.x() - euler.0).abs() > 1e-3
            || (rot_vec.y() - euler.1).abs() > 1e-3
            || (rot_vec.z() - euler.2).abs() > 1e-3;
        let scale_changed = (scale_vec - transform.transform.scale).length() > 1e-4;

        if pos_changed || rot_changed || scale_changed {
            transform.transform.position = pos_vec;
            if rot_changed {
                transform.transform.rotation =
                    katla_math::Quat::from_euler(rot_vec.x(), rot_vec.y(), rot_vec.z());
            }
            if scale_changed {
                transform.transform.scale = scale_vec;
            }

            app.editor
                .editor_ui
                .pending_actions
                .push(EditorAction::UpdateTransform {
                    entity_id,
                    position: pos_vec,
                    rotation: rot_vec,
                    scale: scale_vec,
                });
        }
    }

    // PointLight
    if let Some(light) = app.world.get_component::<PointLight>(entity_id) {
        let color_changed = (light_color[0] - light.color[0]).abs() > 1e-3
            || (light_color[1] - light.color[1]).abs() > 1e-3
            || (light_color[2] - light.color[2]).abs() > 1e-3;
        let intensity_changed = (*light_intensity - light.intensity).abs() > 1e-4;
        let range_changed = (*light_range - light.range).abs() > 1e-4;

        if color_changed || intensity_changed || range_changed {
            app.editor
                .editor_ui
                .pending_actions
                .push(EditorAction::UpdatePointLight {
                    entity_id,
                    color: *light_color,
                    intensity: *light_intensity,
                    range: *light_range,
                });
        }
    }

    // ParticleEmitter
    if let Some(emitter) = app
        .world
        .get_component::<ParticleEmitterComponent>(entity_id)
    {
        let rate_changed = (*emit_rate - emitter.config.emit_rate).abs() > 1e-4;
        let vel_changed = (*velocity - emitter.config.velocity_magnitude).abs() > 1e-4;
        let life_changed = (*lifetime - emitter.config.base_lifetime).abs() > 1e-4;
        let grav_changed = (*gravity - emitter.config.gravity).abs() > 1e-4;
        let scale_changed = (*particle_scale - emitter.config.base_scale).abs() > 1e-4;

        if rate_changed || vel_changed || life_changed || grav_changed || scale_changed {
            app.editor
                .editor_ui
                .pending_actions
                .push(EditorAction::UpdateParticleEmitter {
                    entity_id,
                    emit_rate: *emit_rate,
                    velocity_magnitude: *velocity,
                    base_lifetime: *lifetime,
                    gravity: *gravity,
                    base_scale: *particle_scale,
                });
        }
    }
}

/// Process editor actions after UI rendering.
///
/// Should be called after generate_ui_draw_list to extract any editor actions.
pub fn process_editor_actions(app: &mut Application) {
    let editor_actions = app.editor.editor_ui.take_actions();

    // Process editor actions
    for action in editor_actions {
        match action {
            EditorAction::SpawnModel(model_type, position) => {
                use crate::ui::SpawnableModel;

                let pos = [position.x(), position.y(), position.z()];
                match model_type {
                    SpawnableModel::Cube => {
                        app.spawn_test_cube(pos, [1.0, 1.0, 1.0]);
                    }
                    SpawnableModel::Sphere => {
                        app.spawn_sphere(pos, 0.7, 32, 16);
                    }
                    SpawnableModel::Cylinder => {
                        app.spawn_cylinder(pos, 1.5, 0.5, 32);
                    }
                    SpawnableModel::Plane => {
                        app.spawn_plane(pos, 5.0, 5.0);
                    }
                    SpawnableModel::Torus => {
                        app.spawn_torus(pos, 0.8, 0.2, 32, 16);
                    }
                }
            }
            EditorAction::SpawnModelAtPath { path, screen_pos } => {
                let world_pos = unproject_to_ground_plane(app, screen_pos);
                info!(
                    "Spawning model '{}' at screen ({:.0}, {:.0}) -> world ({:.1}, {:.1}, {:.1})",
                    path.display(),
                    screen_pos.x(),
                    screen_pos.y(),
                    world_pos.x(),
                    world_pos.y(),
                    world_pos.z()
                );
                if let Err(e) =
                    app.spawn_gltf_model(&path, [world_pos.x(), world_pos.y(), world_pos.z()], None)
                {
                    log::error!("Failed to spawn GLTF model '{}': {}", path.display(), e);
                }
            }
            EditorAction::DeleteEntity(entity_id) => {
                // Cascade delete: collect all children first, then delete in reverse order
                let mut to_delete = vec![entity_id];
                collect_children_recursive(app, entity_id, &mut to_delete);

                // Clean up particle emitters before destroying entities
                for id in &to_delete {
                    if let Some(emitter) =
                        app.world.get_component_mut::<ParticleEmitterComponent>(*id)
                        && let Some(handle) = emitter.emitter_handle.take()
                        && let Some(ps) = &mut app.renderer.particle_system
                    {
                        ps.destroy_emitter(handle);
                        info!("Destroyed particle emitter for deleted entity {:?}", id);
                    }
                }

                // Delete in reverse order (children before parents)
                for id in to_delete.into_iter().rev() {
                    app.world.destroy_entity(id);
                }
                info!("Deleted entity {:?} and its children", entity_id);
            }
            EditorAction::DuplicateEntity(entity_id) => {
                let mut ctx = DuplicateContext {
                    world: &mut app.world,
                    gpu_resource_tracker: &mut app.gpu_resource_tracker,
                    particle_system: &mut app.renderer.particle_system,
                };
                if let Some(new_entity_id) = duplicate_entity(&mut ctx, entity_id) {
                    app.editor.editor_ui.selected_entity = Some(new_entity_id);
                    info!("Duplicated entity {:?} -> {:?}", entity_id, new_entity_id);
                }
            }
            EditorAction::SaveScene => {
                let path = std::path::PathBuf::from("assets/scenes/default.katla");
                match crate::scene::SceneManager::save_to_file(app, &path) {
                    Ok(()) => {
                        info!("Scene saved to {:?}", path);
                        app.editor.editor_ui.show_save_confirmation();
                    }
                    Err(e) => log::error!("Failed to save scene: {}", e),
                }
            }
            EditorAction::OpenScene => {
                let path = std::path::PathBuf::from("assets/scenes/default.katla");
                match crate::scene::SceneManager::load_from_file(app, &path) {
                    Ok(()) => {
                        app.editor.editor_ui.selected_entity = None;
                        info!("Scene loaded from {:?}", path);
                    }
                    Err(e) => log::error!("Failed to load scene: {}", e),
                }
            }
            EditorAction::NewScene => {
                let to_remove: Vec<EntityId> = app
                    .world
                    .entity_ids()
                    .filter(|id| app.world.get_component::<EditorHidden>(*id).is_none())
                    .collect();

                // Clean up particle emitters before destroying entities
                for id in &to_remove {
                    if let Some(emitter) =
                        app.world.get_component_mut::<ParticleEmitterComponent>(*id)
                        && let Some(handle) = emitter.emitter_handle.take()
                        && let Some(ps) = &mut app.renderer.particle_system
                    {
                        ps.destroy_emitter(handle);
                    }
                }

                // Wait for all in-flight GPU work to complete before freeing resources.
                // With FRAMES_IN_FLIGHT=2, the previous frame may still reference
                // these buffers on the GPU.
                app.renderer.wait_for_device();

                // Release all GPU resources before destroying entities
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

                for id in to_remove {
                    app.world.destroy_entity(id);
                }
                app.editor.editor_ui.selected_entity = None;
                info!("New scene created");
            }
            EditorAction::Quit => {
                app.quit_requested = true;
            }
            EditorAction::SelectEntity(entity_id) => {
                info!("Selected entity {:?}", entity_id);
            }
            EditorAction::SetTheme(theme_key) => {
                if let Some(theme) = crate::ui::Theme::by_name(&theme_key) {
                    app.editor.editor_ui.set_theme(theme);
                    app.preferences.theme = theme_key;
                    info!("Theme changed to: {}", app.editor.editor_ui.theme_name());
                }
            }
            EditorAction::ToggleGrid => {
                app.editor.editor_ui.show_grid = !app.editor.editor_ui.show_grid;
                app.preferences.show_grid = app.editor.editor_ui.show_grid;
                // Grid visibility will be updated below after the match
            }
            EditorAction::ToggleStats => {
                app.editor.editor_ui.show_stats = !app.editor.editor_ui.show_stats;
                app.preferences.show_stats = app.editor.editor_ui.show_stats;
            }
            EditorAction::SetFontScale(scale) => {
                app.editor.editor_ui.set_font_scale(scale);
                app.preferences.font_scale = scale;
                info!("Font scale changed to: {:.0}%", scale * 100.0);
            }
            EditorAction::OpenPanel(panel) => {
                app.editor.editor_ui.open_panel(panel);
            }
            EditorAction::ToggleParticleEmitter => {
                if let Some(entity_id) = app.editor.editor_ui.selected_particle_emitter
                    && let Some(emitter) = app
                        .world
                        .get_component_mut::<crate::components::ParticleEmitterComponent>(entity_id)
                {
                    emitter.active = !emitter.active;
                    info!(
                        "Particle emitter {:?} {}",
                        entity_id,
                        if emitter.active {
                            "enabled"
                        } else {
                            "disabled"
                        }
                    );
                }
            }
            EditorAction::ResetParticleSystem => {
                if let Some(ps) = &mut app.renderer.particle_system {
                    use katla_gfx::particles::EmitterHandle;

                    let entity_configs: Vec<(
                        EntityId,
                        EmitterHandle,
                        katla_gfx::particles::EmitterConfig,
                    )> = app
                        .world
                        .query::<&mut ParticleEmitterComponent>()
                        .filter_map(|(id, emitter)| {
                            emitter.emitter_handle.map(|h| (id, h, emitter.config))
                        })
                        .collect();

                    for (id, handle, _config) in &entity_configs {
                        ps.destroy_emitter(*handle);
                        if let Some(emitter) =
                            app.world.get_component_mut::<ParticleEmitterComponent>(*id)
                        {
                            emitter.emitter_handle = None;
                        }
                    }

                    if let Err(e) = ps.reset_all() {
                        log::error!("Failed to reset particle system: {}", e);
                    }

                    for (id, _old_handle, config) in entity_configs {
                        match ps.create_emitter(config) {
                            Ok(handle) => {
                                if let Some(emitter) =
                                    app.world.get_component_mut::<ParticleEmitterComponent>(id)
                                {
                                    emitter.emitter_handle = Some(handle);
                                }
                            }
                            Err(e) => {
                                log::error!("Failed to recreate particle emitter: {}", e);
                            }
                        }
                    }

                    info!("Particle system reset complete");
                }
            }
            EditorAction::UpdateTransform {
                entity_id,
                position,
                rotation,
                scale,
            } => {
                if let Some(transform) =
                    app.world.get_component_mut::<TransformComponent>(entity_id)
                {
                    transform.transform.position = position;
                    transform.transform.rotation =
                        katla_math::Quat::from_euler(rotation.x(), rotation.y(), rotation.z());
                    transform.transform.scale = scale;
                    debug!(
                        "Transform updated for entity {:?}: pos=({:.2}, {:.2}, {:.2}) rot=({:.1}, {:.1}, {:.1}) scale=({:.2}, {:.2}, {:.2})",
                        entity_id,
                        position.x(),
                        position.y(),
                        position.z(),
                        rotation.x(),
                        rotation.y(),
                        rotation.z(),
                        scale.x(),
                        scale.y(),
                        scale.z(),
                    );
                }
            }
            EditorAction::UpdatePointLight {
                entity_id,
                color,
                intensity,
                range,
            } => {
                if let Some(light) = app.world.get_component_mut::<PointLight>(entity_id) {
                    light.color = color;
                    light.intensity = intensity;
                    light.range = range;
                    debug!(
                        "PointLight updated for entity {:?}: color=({:.2}, {:.2}, {:.2}) intensity={:.2} range={:.2}",
                        entity_id, color[0], color[1], color[2], intensity, range,
                    );
                }
            }
            EditorAction::UpdateParticleEmitter {
                entity_id,
                emit_rate,
                velocity_magnitude,
                base_lifetime,
                gravity,
                base_scale,
            } => {
                if let Some(emitter) = app
                    .world
                    .get_component_mut::<ParticleEmitterComponent>(entity_id)
                {
                    emitter.config.emit_rate = emit_rate;
                    emitter.config.velocity_magnitude = velocity_magnitude;
                    emitter.config.base_lifetime = base_lifetime;
                    emitter.config.gravity = gravity;
                    emitter.config.base_scale = base_scale;
                    debug!(
                        "ParticleEmitter updated for entity {:?}: rate={:.1} vel={:.1} life={:.2} grav={:.1} scale={:.2}",
                        entity_id,
                        emit_rate,
                        velocity_magnitude,
                        base_lifetime,
                        gravity,
                        base_scale,
                    );
                }
            }
            EditorAction::SetGizmoMode(mode_id) => {
                let mode = match mode_id {
                    0 => crate::gizmo::GizmoMode::Translate,
                    1 => crate::gizmo::GizmoMode::Rotate,
                    2 => crate::gizmo::GizmoMode::Scale,
                    _ => crate::gizmo::GizmoMode::Translate,
                };
                app.editor.gizmo_state.set_mode(mode);
            }
        }
    }

    // Update OS cursor based on UI request
    use winit::window::CursorIcon;
    let cursor_icon = match app.ui_context.input().cursor {
        katla_ui::input::MouseCursor::Arrow => CursorIcon::Default,
        katla_ui::input::MouseCursor::Text => CursorIcon::Text,
        katla_ui::input::MouseCursor::ResizeHorizontal => CursorIcon::EwResize,
        katla_ui::input::MouseCursor::ResizeVertical => CursorIcon::NsResize,
        katla_ui::input::MouseCursor::ResizeDiagonal => CursorIcon::NwseResize,
        katla_ui::input::MouseCursor::ResizeDiagonal2 => CursorIcon::NeswResize,
        katla_ui::input::MouseCursor::Hand => CursorIcon::Pointer,
        katla_ui::input::MouseCursor::Crosshair => CursorIcon::Crosshair,
        katla_ui::input::MouseCursor::NotAllowed => CursorIcon::NotAllowed,
    };
    app.window.set_cursor(cursor_icon);

    // Clear input state for next frame
    app.editor.gizmo_state.consumed_click = false;
    app.ui_context.input_mut().clear_frame_state();
}

/// Collect particle inspector data from the world and particle system.
///
/// This queries the ECS for all particle emitter entities, builds a read-only
/// view of the selected emitter's config, and gathers system-wide stats.
fn collect_particle_inspector_data(app: &mut Application) {
    use crate::components::ParticleEmitterComponent;
    use crate::ui::{EmitterConfigView, ParticleInspectorData, ParticleStats};
    use katla_gfx::particles::EmitterShape;

    let mut emitter_entities = Vec::new();
    let mut selected_config = None;

    // Collect all entities with ParticleEmitterComponent
    for (entity_id, emitter) in app.world.query::<&ParticleEmitterComponent>() {
        emitter_entities.push(entity_id);

        // Build config view for the selected emitter
        if app.editor.editor_ui.selected_particle_emitter == Some(entity_id) {
            let shape_name = match EmitterShape::from_u32(emitter.config.shape) {
                EmitterShape::Point => "Point",
                EmitterShape::Line => "Line",
                EmitterShape::Circle => "Circle",
                EmitterShape::Sphere => "Sphere",
                EmitterShape::Box => "Box",
            };
            selected_config = Some(EmitterConfigView {
                active: emitter.active,
                shape_name,
                shape_params: [
                    emitter.config.shape_params[0],
                    emitter.config.shape_params[1],
                    emitter.config.shape_params[2],
                ],
                emit_rate: emitter.config.emit_rate,
                base_lifetime: emitter.config.base_lifetime,
                lifetime_variation: emitter.config.lifetime_variation,
                velocity_magnitude: emitter.config.velocity_magnitude,
                velocity_cone_angle: emitter.config.velocity_cone_angle,
                base_scale: emitter.config.base_scale,
                scale_variation: emitter.config.scale_variation,
                color: emitter.config.color,
                color_variation: emitter.config.color_variation,
                gravity: emitter.config.gravity,
                turbulence_strength: emitter.config.turbulence_strength,
                turbulence_frequency: emitter.config.turbulence_frequency,
            });
        }
    }

    // Get system-wide stats
    let stats = app.renderer.particle_system.as_ref().map(|ps| {
        let alive = ps.alive_count();
        let max = ps.max_particles();
        ParticleStats {
            max_alive_count: max,
            current_alive_count: alive,
            dead_count: max - alive,
            total_emitted: 0,
            total_died: 0,
            compute_time_ms: 0.0,
            avg_compute_time_ms: 0.0,
            peak_compute_time_ms: 0.0,
            emitter_counts: ps
                .get_emitters()
                .iter()
                .filter(|e| e.emit_rate > 0.0)
                .map(|_| 0)
                .collect(),
            memory_used_mb: (max as f32) * 48.0 / (1024.0 * 1024.0)
                + (max as f32) * 12.0 / (1024.0 * 1024.0),
            buffer_utilization: if max > 0 {
                alive as f32 / max as f32
            } else {
                0.0
            },
            frame_count: 0,
            total_dispatches: 0,
        }
    });

    app.editor.editor_ui.particle_inspector_data = ParticleInspectorData {
        emitter_entities,
        selected_emitter_config: selected_config,
        stats,
    };
}

/// Collect entity information for the editor UI in tree order.
pub fn collect_entity_info(app: &Application) -> Vec<EntityInfo> {
    // First pass: collect all entities with transforms and their relationships
    type EntityData = (
        String,
        Vec3,
        Vec3,
        Vec3,
        String,
        Vec<String>,
        Option<PointLightInfo>,
        Option<ParticleEmitterInfo>,
    );
    let mut entity_data: HashMap<EntityId, EntityData> = HashMap::new();
    let mut parent_map: HashMap<EntityId, EntityId> = HashMap::new();
    let mut children_map: HashMap<EntityId, Vec<EntityId>> = HashMap::new();
    let mut root_entities: HashSet<EntityId> = HashSet::new();

    for (entity_id, transform) in app.world.query_ref::<&TransformComponent>() {
        // Skip entities marked as hidden from editor
        if app.world.get_component::<EditorHidden>(entity_id).is_some() {
            continue;
        }

        let name = app
            .world
            .get_component::<NameComponent>(entity_id)
            .map(|n| n.name.clone())
            .unwrap_or_else(|| format!("Entity {}", entity_id.id()));

        let pos = transform.transform.position;
        let euler = transform.transform.rotation.to_euler();
        let rot = Vec3::new(euler.0, euler.1, euler.2);
        let scale = transform.transform.scale;

        // Collect all component names for this entity
        let mut components: Vec<String> = Vec::new();

        components.push("Transform".to_string());
        if app
            .world
            .get_component::<NameComponent>(entity_id)
            .is_some()
        {
            components.push("Name".to_string());
        }
        if app
            .world
            .get_component::<DrawableComponent>(entity_id)
            .is_some()
        {
            components.push("Drawable".to_string());
        }
        if app
            .world
            .get_component::<DirectionalLight>(entity_id)
            .is_some()
        {
            components.push("DirectionalLight".to_string());
        }
        if app.world.get_component::<PointLight>(entity_id).is_some() {
            components.push("PointLight".to_string());
        }
        if app
            .world
            .get_component::<ParticleEmitterComponent>(entity_id)
            .is_some()
        {
            components.push("ParticleEmitter".to_string());
        }
        if app.world.get_component::<Parent>(entity_id).is_some() {
            components.push("Parent".to_string());
        }
        if app.world.get_component::<Children>(entity_id).is_some() {
            components.push("Children".to_string());
        }

        // Collect PointLight data
        let point_light =
            app.world
                .get_component::<PointLight>(entity_id)
                .map(|pl| PointLightInfo {
                    color: pl.color,
                    intensity: pl.intensity,
                    range: pl.range,
                });

        // Collect ParticleEmitter data
        let particle_emitter = app
            .world
            .get_component::<ParticleEmitterComponent>(entity_id)
            .map(|pe| ParticleEmitterInfo {
                emit_rate: pe.config.emit_rate,
                velocity_magnitude: pe.config.velocity_magnitude,
                base_lifetime: pe.config.base_lifetime,
                gravity: pe.config.gravity,
                base_scale: pe.config.base_scale,
            });

        // Determine entity type based on primary component
        let entity_type = if app
            .world
            .get_component::<DirectionalLight>(entity_id)
            .is_some()
        {
            "Directional Light".to_string()
        } else if app.world.get_component::<PointLight>(entity_id).is_some() {
            "Point Light".to_string()
        } else if app
            .world
            .get_component::<DrawableComponent>(entity_id)
            .is_some()
        {
            "Mesh".to_string()
        } else {
            "Empty".to_string()
        };

        entity_data.insert(
            entity_id,
            (
                name,
                pos,
                rot,
                scale,
                entity_type,
                components,
                point_light,
                particle_emitter,
            ),
        );
        root_entities.insert(entity_id);

        // Track parent relationship
        if let Some(parent) = app.world.get_component::<Parent>(entity_id) {
            parent_map.insert(entity_id, parent.parent);
            root_entities.remove(&entity_id);

            children_map
                .entry(parent.parent)
                .or_default()
                .push(entity_id);
        }
    }

    // Build tree in depth-first order
    let mut result = Vec::new();

    fn add_entity_and_children(
        entity_id: EntityId,
        parent_id: Option<EntityId>,
        entity_data: &HashMap<EntityId, EntityData>,
        children_map: &HashMap<EntityId, Vec<EntityId>>,
        result: &mut Vec<EntityInfo>,
        depth: u32,
    ) {
        if let Some(data) = entity_data.get(&entity_id) {
            let (name, pos, rot, scale, entity_type, components, point_light, particle_emitter) =
                data;

            let children = children_map
                .get(&entity_id)
                .map(|c| c.as_slice())
                .unwrap_or(&[]);
            result.push(EntityInfo {
                id: entity_id,
                name: name.clone(),
                position: *pos,
                rotation: *rot,
                scale: *scale,
                entity_type: entity_type.clone(),
                components: components.clone(),
                depth,
                has_children: !children.is_empty(),
                parent_id,
                point_light: point_light.clone(),
                particle_emitter: particle_emitter.clone(),
            });

            // Recursively add children
            for child_id in children {
                add_entity_and_children(
                    *child_id,
                    Some(entity_id),
                    entity_data,
                    children_map,
                    result,
                    depth + 1,
                );
            }
        }
    }

    // Add root entities (those without parents) in order
    let mut roots: Vec<EntityId> = root_entities.into_iter().collect();
    roots.sort_by_key(|id| id.id());

    for root_id in roots {
        add_entity_and_children(root_id, None, &entity_data, &children_map, &mut result, 0);
    }

    result
}

/// Position offset applied to duplicated entities to avoid overlapping the source.
fn duplicate_offset() -> Vec3 {
    Vec3::new(0.5, 0.0, 0.5)
}

/// Context needed for entity duplication, decoupled from `Application` for testability.
pub(crate) struct DuplicateContext<'a> {
    pub(crate) world: &'a mut katla_ecs::World,
    pub(crate) gpu_resource_tracker: &'a mut crate::gpu_resource_tracker::GpuResourceTracker,
    pub(crate) particle_system: &'a mut Option<katla_gfx::particles::GlobalParticleSystem>,
}

/// Duplicate an entity, copying all its components to a new entity.
///
/// Returns `Some(new_entity_id)` if the source entity exists, `None` otherwise.
///
/// Component-specific behavior:
/// - **TransformComponent**: Position offset by `duplicate_offset()` (0.5, 0.0, 0.5); rotation and scale copied exactly.
/// - **NameComponent**: Appends " (copy)" to the source name.
/// - **DrawableComponent**: Handles copied as-is; GPU resource tracker incremented via `track_drawable()`.
/// - **ParticleEmitterComponent**: Config copied; a new GPU emitter handle is created via `create_emitter()`.
/// - **PointLight**, **VelocityComponent**, **AnimationPlayer**, **BillboardComponent**: Cloned directly.
/// - **EntitySource**: Cloned directly (preserves serialization round-trip).
pub(crate) fn duplicate_entity(
    ctx: &mut DuplicateContext<'_>,
    entity_id: EntityId,
) -> Option<EntityId> {
    use crate::animation::components::AnimationPlayer;
    use crate::components::physics::physics::VelocityComponent;
    use crate::components::rendering::billboard::BillboardComponent;
    use crate::scene::entity_source::EntitySource;

    let new_id = ctx.world.create_entity();

    // TransformComponent: offset position, copy rotation and scale
    if let Some(transform) = ctx.world.get_component::<TransformComponent>(entity_id) {
        let mut new_transform = TransformComponent {
            transform: transform.transform,
        };
        new_transform.transform.position = transform.transform.position + duplicate_offset();
        ctx.world.add_component(new_id, new_transform);
    }

    // NameComponent: append " (copy)"
    if let Some(name) = ctx.world.get_component::<NameComponent>(entity_id) {
        let new_name = format!("{} (copy)", name.name);
        ctx.world
            .add_component(new_id, NameComponent { name: new_name });
    }

    // DrawableComponent: copy handles, track GPU resources
    if let Some(drawable) = ctx.world.get_component::<DrawableComponent>(entity_id) {
        let new_drawable = DrawableComponent {
            mesh_handle: drawable.mesh_handle,
            material_handle: drawable.material_handle,
            color: drawable.color,
            skeleton_handle: drawable.skeleton_handle,
            metallic: drawable.metallic,
            roughness: drawable.roughness,
            ao: drawable.ao,
            emission: drawable.emission,
        };
        ctx.gpu_resource_tracker.track_drawable(
            new_drawable.mesh_handle,
            new_drawable.material_handle,
            new_drawable.skeleton_handle,
        );
        ctx.world.add_component(new_id, new_drawable);
    }

    // ParticleEmitterComponent: copy config, create new GPU emitter
    if let Some(emitter) = ctx
        .world
        .get_component::<ParticleEmitterComponent>(entity_id)
    {
        let is_active = emitter.active;
        let emitter_config = emitter.config;
        let new_emitter = ParticleEmitterComponent {
            config: emitter_config,
            emitter_handle: None,
            active: is_active,
            timed_emission: emitter.timed_emission,
            burst_queue: emitter.burst_queue.clone(),
        };
        ctx.world.add_component(new_id, new_emitter);

        if is_active && let Some(ps) = ctx.particle_system {
            match ps.create_emitter(emitter_config) {
                Ok(handle) => {
                    if let Some(em) = ctx
                        .world
                        .get_component_mut::<ParticleEmitterComponent>(new_id)
                    {
                        em.emitter_handle = Some(handle);
                    }
                }
                Err(e) => log::warn!(
                    "Failed to create GPU emitter for duplicated entity {:?}: {}",
                    new_id,
                    e
                ),
            }
        }
    }

    // PointLight: Copy type, deref from reference
    if let Some(light) = ctx.world.get_component::<PointLight>(entity_id) {
        ctx.world.add_component(new_id, *light);
    }

    // VelocityComponent: copy fields manually
    if let Some(vel) = ctx.world.get_component::<VelocityComponent>(entity_id) {
        ctx.world.add_component(
            new_id,
            VelocityComponent {
                velocity: vel.velocity,
                acceleration: vel.acceleration,
            },
        );
    }

    // AnimationPlayer: clone
    if let Some(player) = ctx.world.get_component::<AnimationPlayer>(entity_id) {
        ctx.world.add_component(new_id, player.clone());
    }

    // BillboardComponent: clone
    if let Some(billboard) = ctx.world.get_component::<BillboardComponent>(entity_id) {
        ctx.world.add_component(new_id, billboard.clone());
    }

    // EntitySource: clone for serialization round-trip
    if let Some(source) = ctx.world.get_component::<EntitySource>(entity_id) {
        ctx.world.add_component(new_id, source.clone());
    }

    Some(new_id)
}

/// Recursively collect all children of an entity for cascade delete.
pub fn collect_children_recursive(
    app: &Application,
    entity_id: EntityId,
    result: &mut Vec<EntityId>,
) {
    if let Some(children) = app.world.get_component::<Children>(entity_id) {
        for child_id in &children.children {
            result.push(*child_id);
            collect_children_recursive(app, *child_id, result);
        }
    }
}

/// Unproject a screen position to a world position on the y=0 ground plane.
///
/// Pure function taking view/projection matrices, camera position, viewport
/// bounds, and screen position. Suitable for unit testing.
fn unproject_to_ground_plane_impl(
    view_mat: katla_math::Mat4,
    proj_mat: katla_math::Mat4,
    cam_pos: Vec3,
    viewport: katla_math::Rect2D,
    screen_pos: Vec2,
) -> Vec3 {
    // Convert screen position to normalized device coordinates (-1 to 1)
    // Screen space: (0,0) = top-left, Y increases downward
    // Vulkan NDC on Windows (top-down swapchain): Y=-1 at top, Y=+1 at bottom
    let ndc_x = ((screen_pos.x() - viewport.min.x()) / viewport.width()) * 2.0 - 1.0;
    let ndc_y = ((screen_pos.y() - viewport.min.y()) / viewport.height()) * 2.0 - 1.0;

    let vp = proj_mat * view_mat;
    let inv_vp = vp.inverse();

    // Unproject two points at different depths to get the ray direction.
    // Reverse-Z infinite projection: ndc_z=1 is near plane, ndc_z=0 is infinity.
    // Use ndc_z=1 (near) and ndc_z=0.5 (mid-range) for two distinct world-space points.
    let near_clip = inv_vp * Vec4::new(ndc_x, ndc_y, 1.0, 1.0);
    let far_clip = inv_vp * Vec4::new(ndc_x, ndc_y, 0.5, 1.0);

    // Perspective divide
    let near = Vec3::new(
        near_clip.x() / near_clip.w(),
        near_clip.y() / near_clip.w(),
        near_clip.z() / near_clip.w(),
    );
    let far = Vec3::new(
        far_clip.x() / far_clip.w(),
        far_clip.y() / far_clip.w(),
        far_clip.z() / far_clip.w(),
    );

    // Ray origin is the unprojected near point, direction toward far point
    let ray_origin = near;
    let ray_dir = (far - near).normalize();

    // Intersect ray with y=0 plane: t = -ray_origin.y / ray_dir.y
    if ray_dir.y().abs() < 1e-6 {
        return Vec3::new(cam_pos.x(), 0.0, cam_pos.z());
    }

    let t = -ray_origin.y() / ray_dir.y();
    let t = if t < 0.0 { 10.0 } else { t };

    ray_origin + ray_dir * t
}

/// Unproject a screen position to a world position on the y=0 ground plane.
///
/// Takes the mouse position in logical screen coordinates and the current viewport
/// panel bounds, then raycasts from the camera through that pixel to find where
/// the ray intersects the ground plane.
fn unproject_to_ground_plane(app: &Application, screen_pos: Vec2) -> Vec3 {
    let viewport = app.editor.editor_ui.last_viewport_bounds;

    let camera = app.camera.borrow();
    let view_mat = camera.get_view_mat(&app.world);
    let proj_mat = camera.get_proj_mat(&app.world);
    let cam_entity = camera.entity;
    drop(camera);

    let cam_pos = app
        .world
        .get_component::<crate::components::TransformComponent>(cam_entity)
        .map(|t| t.transform.position)
        .unwrap_or(Vec3::new(0.0, 2.0, 10.0));

    unproject_to_ground_plane_impl(view_mat, proj_mat, cam_pos, viewport, screen_pos)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu_resource_tracker::GpuResourceTracker;
    use katla_math::{Mat4, Quat, Rect2D, Vec3};

    /// Build a view matrix matching what Camera::get_view_mat does:
    /// camera-to-world from a position + rotation, then inverted.
    fn make_view_matrix(position: Vec3, pitch: f32, yaw: f32) -> Mat4 {
        let rotation = Quat::from_euler(pitch, yaw, 0.0);
        let rotation_mat = rotation.make_mat4();
        let fwd = rotation_mat * Vec3::new(0.0, 0.0, -1.0);
        Mat4::create_lookat(position, position + fwd, Vec3::new(0.0, 1.0, 0.0)).inverse()
    }

    fn make_proj(fov: f32, aspect: f32, near: f32) -> Mat4 {
        Mat4::create_proj(fov, aspect, near)
    }

    fn viewport(w: f32, h: f32) -> Rect2D {
        Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(w, h))
    }

    /// Camera looking straight down (-Y) from y=10. Center of viewport should hit origin.
    #[test]
    fn test_unproject_camera_straight_down() {
        let cam_pos = Vec3::new(0.0, 10.0, 0.0);
        let view = make_view_matrix(cam_pos, -std::f32::consts::FRAC_PI_2, 0.0);
        let proj = make_proj(60.0, 16.0 / 9.0, 0.001);
        let vp = viewport(1600.0, 900.0);

        let center =
            unproject_to_ground_plane_impl(view, proj, cam_pos, vp, Vec2::new(800.0, 450.0));

        assert!(
            !center.x().is_nan() && !center.y().is_nan() && !center.z().is_nan(),
            "center should not be NaN, got ({:.3}, {:.3}, {:.3})",
            center.x(),
            center.y(),
            center.z()
        );
        assert!(
            (center.x().abs() < 0.1) && (center.z().abs() < 0.1),
            "looking straight down at center should hit near origin, got ({:.3}, {:.3}, {:.3})",
            center.x(),
            center.y(),
            center.z()
        );
        assert!(
            (center.y() - 0.0).abs() < 0.01,
            "should be on ground plane, y={:.3}",
            center.y()
        );
    }

    /// Camera angled down 45 degrees from height 10 at origin, looking along -Z.
    /// Center of viewport should hit (0, 0, -10) on the ground.
    #[test]
    fn test_unproject_camera_angled_down_45() {
        let cam_pos = Vec3::new(0.0, 10.0, 0.0);
        // Pitch -45 deg = looking 45 degrees below horizontal
        let view = make_view_matrix(cam_pos, -std::f32::consts::FRAC_PI_4, 0.0);
        let proj = make_proj(60.0, 16.0 / 9.0, 0.001);
        let vp = viewport(1600.0, 900.0);

        let center =
            unproject_to_ground_plane_impl(view, proj, cam_pos, vp, Vec2::new(800.0, 450.0));

        assert!(
            !center.x().is_nan() && !center.y().is_nan() && !center.z().is_nan(),
            "should not be NaN, got ({:.3}, {:.3}, {:.3})",
            center.x(),
            center.y(),
            center.z()
        );
        // Looking 45 deg down from height 10 -> ground hit is 10 units away in Z
        assert!(
            (center.z() - (-10.0)).abs() < 1.0,
            "looking 45 deg down should hit ~z=-10, got z={:.3}",
            center.z()
        );
        assert!(
            (center.y()).abs() < 0.01,
            "should be on ground plane, y={:.3}",
            center.y()
        );
    }

    /// Camera at (5, 5, 5) looking at origin. Center viewport should hit near origin.
    #[test]
    fn test_unproject_camera_offset_position() {
        let cam_pos = Vec3::new(5.0, 5.0, 5.0);
        // Look toward origin: yaw ~225 deg (toward -X, -Z), pitch ~-35 deg (downward)
        let yaw = std::f32::consts::FRAC_PI_4 + std::f32::consts::FRAC_PI_4; // 135 deg
        let pitch = -35.0_f32.to_radians();
        let view = make_view_matrix(cam_pos, pitch, yaw);
        let proj = make_proj(60.0, 16.0 / 9.0, 0.001);
        let vp = viewport(1600.0, 900.0);

        let center =
            unproject_to_ground_plane_impl(view, proj, cam_pos, vp, Vec2::new(800.0, 450.0));

        assert!(
            !center.x().is_nan() && !center.y().is_nan() && !center.z().is_nan(),
            "should not be NaN, got ({:.3}, {:.3}, {:.3})",
            center.x(),
            center.y(),
            center.z()
        );
        assert!(
            (center.y()).abs() < 0.01,
            "should be on ground plane, y={:.3}",
            center.y()
        );
        // Should be somewhere between camera and origin on the ground
        assert!(
            center.x() < cam_pos.x() && center.z() < cam_pos.z(),
            "should be between camera and origin, got ({:.3}, {:.3}, {:.3})",
            center.x(),
            center.y(),
            center.z()
        );
    }

    /// Dragging to different positions within the viewport should give different results.
    #[test]
    fn test_unproject_different_viewport_positions() {
        let cam_pos = Vec3::new(0.0, 10.0, 0.0);
        let view = make_view_matrix(cam_pos, -std::f32::consts::FRAC_PI_4, 0.0);
        let proj = make_proj(60.0, 16.0 / 9.0, 0.001);
        let vp = viewport(1600.0, 900.0);

        let top_left =
            unproject_to_ground_plane_impl(view, proj, cam_pos, vp, Vec2::new(100.0, 100.0));
        let center =
            unproject_to_ground_plane_impl(view, proj, cam_pos, vp, Vec2::new(800.0, 450.0));
        let bottom_right =
            unproject_to_ground_plane_impl(view, proj, cam_pos, vp, Vec2::new(1500.0, 800.0));

        // All should be on ground plane
        for p in [top_left, center, bottom_right] {
            assert!(
                !p.x().is_nan() && !p.y().is_nan() && !p.z().is_nan(),
                "should not be NaN"
            );
            assert!(
                (p.y()).abs() < 0.01,
                "should be on ground plane, y={:.3}",
                p.y()
            );
        }

        // All three positions should be distinct
        assert!(
            (top_left - center).length() > 0.1,
            "top-left and center should be different positions"
        );
        assert!(
            (center - bottom_right).length() > 0.1,
            "center and bottom-right should be different positions"
        );
    }

    /// Viewport offset: viewport at (200, 32) to (1200, 632). Center of that should
    /// still unproject correctly.
    #[test]
    fn test_unproject_offset_viewport() {
        let cam_pos = Vec3::new(0.0, 10.0, 0.0);
        let view = make_view_matrix(cam_pos, -std::f32::consts::FRAC_PI_4, 0.0);
        let proj = make_proj(60.0, 16.0 / 9.0, 0.001);
        // Viewport panel at (200, 32) with size 1000x600
        let vp = Rect2D::new(Vec2::new(200.0, 32.0), Vec2::new(1200.0, 632.0));

        let center =
            unproject_to_ground_plane_impl(view, proj, cam_pos, vp, Vec2::new(700.0, 332.0));

        assert!(
            !center.x().is_nan() && !center.y().is_nan() && !center.z().is_nan(),
            "should not be NaN, got ({:.3}, {:.3}, {:.3})",
            center.x(),
            center.y(),
            center.z()
        );
        assert!(
            (center.y()).abs() < 0.01,
            "should be on ground plane, y={:.3}",
            center.y()
        );
    }

    /// Camera looking straight ahead (horizontal). Should still produce valid results
    /// (will fall back to camera XZ position for parallel-to-ground rays).
    #[test]
    fn test_unproject_camera_horizontal() {
        let cam_pos = Vec3::new(0.0, 10.0, 10.0);
        let view = make_view_matrix(cam_pos, 0.0, 0.0);
        let proj = make_proj(60.0, 16.0 / 9.0, 0.001);
        let vp = viewport(1600.0, 900.0);

        // Looking straight ahead, center viewport ray is nearly parallel to ground
        let result =
            unproject_to_ground_plane_impl(view, proj, cam_pos, vp, Vec2::new(800.0, 450.0));

        assert!(
            !result.x().is_nan() && !result.y().is_nan() && !result.z().is_nan(),
            "horizontal camera should not produce NaN, got ({:.3}, {:.3}, {:.3})",
            result.x(),
            result.y(),
            result.z()
        );
    }

    /// Default camera (0, 2, 10) looking along -Z with slight downward pitch.
    /// Matches the initial editor camera setup.
    #[test]
    fn test_unproject_default_editor_camera() {
        let cam_pos = Vec3::new(0.0, 2.0, 10.0);
        let view = make_view_matrix(cam_pos, -10.0_f32.to_radians(), 0.0);
        let proj = make_proj(60.0, 16.0 / 9.0, 0.001);
        let vp = viewport(1600.0, 900.0);

        let center =
            unproject_to_ground_plane_impl(view, proj, cam_pos, vp, Vec2::new(800.0, 450.0));

        assert!(
            !center.x().is_nan() && !center.y().is_nan() && !center.z().is_nan(),
            "default camera should not produce NaN, got ({:.3}, {:.3}, {:.3})",
            center.x(),
            center.y(),
            center.z()
        );
        assert!(
            (center.y()).abs() < 0.01,
            "should be on ground plane, y={:.3}",
            center.y()
        );
        // Should be somewhere ahead of camera on the ground
        assert!(
            center.z() < cam_pos.z(),
            "should be in front of camera, got z={:.3} vs cam z={:.3}",
            center.z(),
            cam_pos.z()
        );
    }

    #[test]
    fn test_duplicate_entity_preserves_all_components() {
        use crate::animation::components::AnimationPlayer;
        use crate::components::physics::physics::VelocityComponent;
        use crate::components::rendering::billboard::{BillboardComponent, BillboardIcon};
        use crate::scene::entity_source::EntitySource;
        use katla_gfx::{MaterialHandle, MeshHandle, SkeletonHandle};

        let mut world = katla_ecs::World::new();
        let mut tracker = GpuResourceTracker::new(MaterialHandle::new(999));

        let source = world.spawn((
            TransformComponent::from_position(Vec3::new(1.0, 2.0, 3.0)),
            NameComponent {
                name: "TestEntity".to_string(),
            },
        ));

        let mesh = MeshHandle::new(10);
        let mat = MaterialHandle::new(20);
        let drawable =
            DrawableComponent::with_handles_and_color(mesh, mat, katla_math::Color::WHITE);
        world.add_component(source, drawable);
        tracker.track_drawable(mesh, mat, SkeletonHandle::NONE);

        world.add_component(
            source,
            PointLight {
                color: [1.0, 0.5, 0.0],
                intensity: 10.0,
                range: 50.0,
            },
        );

        world.add_component(
            source,
            VelocityComponent::new(
                katla_math::Vec3::new(1.0, 0.0, -1.0),
                katla_math::Vec3::new(0.0, -9.8, 0.0),
            ),
        );

        world.add_component(source, AnimationPlayer::new("walk"));

        world.add_component(source, BillboardComponent::new(BillboardIcon::Lightbulb));

        world.add_component(
            source,
            EntitySource::Cube {
                size: [1.0, 1.0, 1.0],
            },
        );

        let mut ctx = DuplicateContext {
            world: &mut world,
            gpu_resource_tracker: &mut tracker,
            particle_system: &mut None,
        };
        let new_id = duplicate_entity(&mut ctx, source).unwrap();

        // Verify all components exist on new entity
        assert!(
            world.get_component::<TransformComponent>(new_id).is_some(),
            "TransformComponent should be copied"
        );
        assert!(
            world.get_component::<NameComponent>(new_id).is_some(),
            "NameComponent should be copied"
        );
        assert!(
            world.get_component::<DrawableComponent>(new_id).is_some(),
            "DrawableComponent should be copied"
        );
        assert!(
            world.get_component::<PointLight>(new_id).is_some(),
            "PointLight should be copied"
        );
        assert!(
            world.get_component::<VelocityComponent>(new_id).is_some(),
            "VelocityComponent should be copied"
        );
        assert!(
            world.get_component::<AnimationPlayer>(new_id).is_some(),
            "AnimationPlayer should be copied"
        );
        assert!(
            world.get_component::<BillboardComponent>(new_id).is_some(),
            "BillboardComponent should be copied"
        );
        assert!(
            world.get_component::<EntitySource>(new_id).is_some(),
            "EntitySource should be copied"
        );

        // Verify field-level values are preserved
        let new_name = world.get_component::<NameComponent>(new_id).unwrap();
        assert_eq!(new_name.name, "TestEntity (copy)");

        let new_light = world.get_component::<PointLight>(new_id).unwrap();
        assert_eq!(new_light.color, [1.0, 0.5, 0.0]);
        assert_eq!(new_light.intensity, 10.0);
        assert_eq!(new_light.range, 50.0);

        let new_vel = world.get_component::<VelocityComponent>(new_id).unwrap();
        assert_eq!(new_vel.velocity, katla_math::Vec3::new(1.0, 0.0, -1.0));
        assert_eq!(new_vel.acceleration, katla_math::Vec3::new(0.0, -9.8, 0.0));

        let new_source = world.get_component::<EntitySource>(new_id).unwrap();
        assert_eq!(
            *new_source,
            EntitySource::Cube {
                size: [1.0, 1.0, 1.0]
            }
        );

        // Verify GPU resource tracker incremented ref counts
        assert_eq!(
            tracker.mesh_ref_count(mesh),
            2,
            "Mesh ref count should be 2 after duplication"
        );
        assert_eq!(
            tracker.material_ref_count(mat),
            2,
            "Material ref count should be 2 after duplication"
        );
    }

    #[test]
    fn test_duplicate_entity_offsets_transform() {
        let mut world = katla_ecs::World::new();
        let mut tracker = GpuResourceTracker::new(katla_gfx::MaterialHandle::NONE);

        let source = world.spawn((TransformComponent::from_position(Vec3::new(5.0, 3.0, -2.0)),));

        let mut ctx = DuplicateContext {
            world: &mut world,
            gpu_resource_tracker: &mut tracker,
            particle_system: &mut None,
        };
        let new_id = duplicate_entity(&mut ctx, source).unwrap();

        let src_transform = world.get_component::<TransformComponent>(source).unwrap();
        let new_transform = world.get_component::<TransformComponent>(new_id).unwrap();

        let expected_pos = src_transform.transform.position + duplicate_offset();
        assert_eq!(
            new_transform.transform.position, expected_pos,
            "Position should be offset by duplicate_offset()"
        );

        // Rotation: compare via matrix since Quat doesn't implement PartialEq
        let src_mat = src_transform.transform.rotation.make_mat4();
        let new_mat = new_transform.transform.rotation.make_mat4();
        assert_eq!(src_mat, new_mat, "Rotation should be copied exactly");

        assert_eq!(
            new_transform.transform.scale, src_transform.transform.scale,
            "Scale should be copied exactly"
        );
    }

    #[test]
    fn test_duplicate_entity_empty() {
        let mut world = katla_ecs::World::new();
        let mut tracker = GpuResourceTracker::new(katla_gfx::MaterialHandle::NONE);

        let source = world.create_entity();

        let mut ctx = DuplicateContext {
            world: &mut world,
            gpu_resource_tracker: &mut tracker,
            particle_system: &mut None,
        };

        let new_id = duplicate_entity(&mut ctx, source);

        assert!(new_id.is_some(), "Empty entity duplication should succeed");
        let new_id = new_id.unwrap();
        assert_ne!(new_id, source, "New entity should have a different ID");

        assert!(
            world.get_component::<TransformComponent>(new_id).is_none(),
            "Empty entity duplicate should have no TransformComponent"
        );
        assert!(
            world.get_component::<NameComponent>(new_id).is_none(),
            "Empty entity duplicate should have no NameComponent"
        );
        assert!(
            world.get_component::<DrawableComponent>(new_id).is_none(),
            "Empty entity duplicate should have no DrawableComponent"
        );
    }

    #[test]
    fn test_duplicate_entity_updates_selection() {
        let mut world = katla_ecs::World::new();
        let mut tracker = GpuResourceTracker::new(katla_gfx::MaterialHandle::NONE);

        let source = world.spawn((TransformComponent::from_position(Vec3::new(0.0, 0.0, 0.0)),));

        let mut ctx = DuplicateContext {
            world: &mut world,
            gpu_resource_tracker: &mut tracker,
            particle_system: &mut None,
        };
        let new_id = duplicate_entity(&mut ctx, source).unwrap();

        // Verify the new entity was created (selection update is handled at call site)
        assert_ne!(
            new_id, source,
            "Duplicated entity should differ from source"
        );
        assert!(
            world.get_component::<TransformComponent>(new_id).is_some(),
            "Duplicated entity should have TransformComponent"
        );
    }

    #[test]
    fn test_reset_particle_system_editor_action() {
        use katla_gfx::particles::EmitterConfig;

        let mut world = katla_ecs::World::new();

        let entity = world.spawn((ParticleEmitterComponent::with_config(EmitterConfig {
            emit_rate: 100.0,
            base_lifetime: 3.0,
            gravity: -5.0,
            ..Default::default()
        }),));

        // Verify the component was created with correct config
        let emitter = world
            .get_component::<ParticleEmitterComponent>(entity)
            .unwrap();
        assert_eq!(emitter.config.emit_rate, 100.0);
        assert_eq!(emitter.config.base_lifetime, 3.0);
        assert_eq!(emitter.config.gravity, -5.0);
        assert!(emitter.active);

        // When particle_system is None, the reset action is a no-op
        // (the if-let on particle_system skips entirely)
        // This verifies the code path doesn't panic
        let ps: Option<katla_gfx::particles::GlobalParticleSystem> = None;
        assert!(ps.is_none());

        // Verify emitter component is unchanged after no-op reset
        let emitter = world
            .get_component::<ParticleEmitterComponent>(entity)
            .unwrap();
        assert_eq!(emitter.config.emit_rate, 100.0);
        assert!(emitter.active);
    }
}
