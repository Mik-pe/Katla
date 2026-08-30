//! Editor subsystem - handles UI rendering, entity management, and editor actions.

pub mod agent;
pub mod component_registry;
#[cfg(feature = "mcp")]
pub(crate) mod mcp;

use std::collections::{HashMap, HashSet};

use log::info;

use katla_ecs::EntityId;
use katla_ecs::scene_tool::{SceneCommand, SceneToolError, UndoGroup};
use katla_gfx::GpuRenderer;
use katla_gfx::renderer::UIDrawList;
use katla_math::{Vec2, Vec3};

use crate::components::ParticleEmitterComponent;
use crate::components::{
    Children, DirectionalLight, DrawableComponent, EditorHidden, NameComponent, Parent,
    PerspectiveComponent, PointLight, TransformComponent,
};

use crate::ui::{
    ColliderShapeInfo, ColliderShapeType, DirectionalLightInfo, EditorAction, EntityInfo,
    ParticleEmitterInfo, PerspectiveInfo, PhysicsMaterialInfo, PointLightInfo, RigidBodyInfo,
};

use super::Application;

/// Snapshot of ECS component values before an inspector slider drag.
pub(crate) struct InspectorDragSnapshot {
    entity: EntityId,
    position: Vec3,
    rotation_euler: (f32, f32, f32),
    scale: Vec3,
    light_color: Option<[f32; 3]>,
    light_intensity: Option<f32>,
    light_range: Option<f32>,
    emit_rate: Option<f32>,
    velocity: Option<f32>,
    lifetime: Option<f32>,
    gravity: Option<f32>,
    particle_scale: Option<f32>,
    fov: Option<f32>,
    near: Option<f32>,
    aspect_ratio: Option<f32>,
    directional_direction: Option<[f32; 3]>,
    directional_color: Option<[f32; 3]>,
    directional_intensity: Option<f32>,
}

/// Command that restores inspector properties to pre-drag values.
struct InspectorDragUndo {
    entity: EntityId,
    snapshot: InspectorDragSnapshot,
    executed: bool,
}

impl InspectorDragUndo {
    fn new(snapshot: InspectorDragSnapshot) -> Self {
        Self {
            entity: snapshot.entity,
            snapshot,
            executed: true,
        }
    }
}

impl SceneCommand for InspectorDragUndo {
    fn execute(&mut self, world: &mut katla_ecs::World) -> Result<(), SceneToolError> {
        if !world.entity_exists(self.entity) {
            return Err(SceneToolError::EntityNotFound(self.entity));
        }
        apply_inspector_snapshot(world, self.entity, &self.snapshot);
        self.executed = true;
        Ok(())
    }

    fn undo(&mut self, world: &mut katla_ecs::World) -> Result<(), SceneToolError> {
        if !world.entity_exists(self.entity) {
            return Err(SceneToolError::EntityNotFound(self.entity));
        }
        apply_inspector_snapshot(world, self.entity, &self.snapshot);
        self.executed = false;
        Ok(())
    }

    fn description(&self) -> String {
        format!("Inspector drag on entity {}", self.entity)
    }

    fn affected_entities(&self) -> Vec<EntityId> {
        vec![self.entity]
    }
}

fn apply_inspector_snapshot(
    world: &mut katla_ecs::World,
    entity: EntityId,
    snapshot: &InspectorDragSnapshot,
) {
    if let Some(transform) = world.get_component_mut::<TransformComponent>(entity) {
        transform.transform.position = snapshot.position;
        transform.transform.rotation = katla_math::Quat::from_euler(
            snapshot.rotation_euler.0,
            snapshot.rotation_euler.1,
            snapshot.rotation_euler.2,
        );
        transform.transform.scale = snapshot.scale;
    }
    if let Some(light) = world.get_component_mut::<PointLight>(entity) {
        if let Some(color) = snapshot.light_color {
            light.color = color;
        }
        if let Some(intensity) = snapshot.light_intensity {
            light.intensity = intensity;
        }
        if let Some(range) = snapshot.light_range {
            light.range = range;
        }
    }
    if let Some(emitter) = world.get_component_mut::<ParticleEmitterComponent>(entity) {
        if let Some(rate) = snapshot.emit_rate {
            emitter.config.emit_rate = rate;
        }
        if let Some(vel) = snapshot.velocity {
            emitter.config.velocity_magnitude = vel;
        }
        if let Some(life) = snapshot.lifetime {
            emitter.config.base_lifetime = life;
        }
        if let Some(grav) = snapshot.gravity {
            emitter.config.gravity = grav;
        }
        if let Some(sc) = snapshot.particle_scale {
            emitter.config.base_scale = sc;
        }
    }
    if let Some(persp) = world.get_component_mut::<PerspectiveComponent>(entity) {
        if let Some(fov) = snapshot.fov {
            persp.fov = fov;
        }
        if let Some(near) = snapshot.near {
            persp.near = near;
        }
        if let Some(ar) = snapshot.aspect_ratio {
            persp.aspect_ratio = ar;
        }
    }
    if let Some(dl) = world.get_component_mut::<DirectionalLight>(entity) {
        if let Some(dir) = snapshot.directional_direction {
            dl.direction = Vec3::new(dir[0], dir[1], dir[2]);
        }
        if let Some(color) = snapshot.directional_color {
            dl.color = color;
        }
        if let Some(intensity) = snapshot.directional_intensity {
            dl.intensity = intensity;
        }
    }
}

/// Snapshot current ECS component values for the inspector drag undo.
fn snapshot_inspector_state(app: &Application, entity: EntityId) -> InspectorDragSnapshot {
    let (position, rotation_euler, scale) =
        if let Some(transform) = app.world.get_component::<TransformComponent>(entity) {
            let euler = transform.transform.rotation.to_euler();
            (
                transform.transform.position,
                (euler.0, euler.1, euler.2),
                transform.transform.scale,
            )
        } else {
            (
                Vec3::new(0.0, 0.0, 0.0),
                (0.0, 0.0, 0.0),
                Vec3::new(1.0, 1.0, 1.0),
            )
        };

    let (light_color, light_intensity, light_range) =
        if let Some(light) = app.world.get_component::<PointLight>(entity) {
            (Some(light.color), Some(light.intensity), Some(light.range))
        } else {
            (None, None, None)
        };

    let (emit_rate, velocity, lifetime, gravity, particle_scale) =
        if let Some(emitter) = app.world.get_component::<ParticleEmitterComponent>(entity) {
            (
                Some(emitter.config.emit_rate),
                Some(emitter.config.velocity_magnitude),
                Some(emitter.config.base_lifetime),
                Some(emitter.config.gravity),
                Some(emitter.config.base_scale),
            )
        } else {
            (None, None, None, None, None)
        };

    let (fov, near, aspect_ratio) = app
        .world
        .get_component::<PerspectiveComponent>(entity)
        .map(|p| (Some(p.fov), Some(p.near), Some(p.aspect_ratio)))
        .unwrap_or((None, None, None));
    let (directional_direction, directional_color, directional_intensity) = app
        .world
        .get_component::<DirectionalLight>(entity)
        .map(|dl| {
            (
                Some([dl.direction.x(), dl.direction.y(), dl.direction.z()]),
                Some(dl.color),
                Some(dl.intensity),
            )
        })
        .unwrap_or((None, None, None));

    InspectorDragSnapshot {
        entity,
        position,
        rotation_euler,
        scale,
        light_color,
        light_intensity,
        light_range,
        emit_rate,
        velocity,
        lifetime,
        gravity,
        particle_scale,
        fov,
        near,
        aspect_ratio,
        directional_direction,
        directional_color,
        directional_intensity,
    }
}

/// GPU handles associated with a spawned entity, used for cleanup on undo/redo.
pub(crate) struct GpuCleanupData {
    pub(crate) mesh_handle: katla_gfx::MeshHandle,
    pub(crate) material_handle: katla_gfx::MaterialHandle,
    pub(crate) skeleton_handle: katla_gfx::SkeletonHandle,
}

/// Command that reverses a spawn by destroying the entity.
/// Undo re-creates the entity (no component restoration for editor spawns).
struct EditorSpawnCommand {
    entity: EntityId,
}

impl EditorSpawnCommand {
    fn new(entity: EntityId) -> Self {
        Self { entity }
    }
}

impl SceneCommand for EditorSpawnCommand {
    fn execute(&mut self, world: &mut katla_ecs::World) -> Result<(), SceneToolError> {
        if world.entity_exists(self.entity) {
            world.destroy_entity(self.entity);
        }
        Ok(())
    }

    fn undo(&mut self, world: &mut katla_ecs::World) -> Result<(), SceneToolError> {
        if world.entity_exists(self.entity) {
            world.destroy_entity(self.entity);
        }
        Ok(())
    }

    fn description(&self) -> String {
        format!("Destroy spawned entity {}", self.entity)
    }

    fn affected_entities(&self) -> Vec<EntityId> {
        vec![self.entity]
    }
}

/// Upload font atlas texture to GPU if it has been modified.
///
/// This MUST be called AFTER `generate_ui_draw_list()` (which rasterizes new glyphs
/// into the CPU atlas) and BEFORE `render_frame()` (which samples from the GPU atlas).
/// Calling it after render_frame causes a one-frame lag where the GPU has stale data.
pub fn upload_font_atlas(app: &mut Application) {
    let (needs_update, width, height, was_resized) = {
        let fonts = app.ui_context.fonts();
        let needs_update = fonts.atlas_needs_update();
        if !needs_update {
            (false, 0, 0, false)
        } else {
            let (w, h) = fonts.atlas_size();
            let resized = fonts.atlas_was_resized();
            (true, w, h, resized)
        }
    };

    if !needs_update {
        return;
    }

    let data = app.ui_context.fonts().atlas_data_rgba();

    if was_resized {
        let _atlas_handle = app.renderer.create_ui_font_atlas(width, height, &data);

        if let Some(bindless_slot) = match &mut app.renderer {
            katla_gfx::AnyRenderer::Vulkan(r) => r.ui_renderer.font_atlas_bindless_slot(),
            #[cfg(target_os = "macos")]
            katla_gfx::AnyRenderer::Metal(_) => app.renderer.get_bindless_slot(_atlas_handle),
        } {
            app.editor
                .ui_renderer
                .set_font_atlas_bindless_slot(bindless_slot);
        }

        app.ui_context.fonts_mut().clear_atlas_resized();
    } else {
        app.renderer.update_ui_font_atlas(width, height, &data);
    }

    app.ui_context.fonts_mut().mark_atlas_updated();
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
    let physical_size = if let Some(ref window) = app.window {
        let size = window.inner_size();
        Vec2::new(size.width as f32, size.height as f32)
    } else {
        // Headless mode: use renderer swapchain extent
        let extent = app.renderer.swapchain_extent();
        Vec2::new(extent.width as f32, extent.height as f32)
    };

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

    app.editor.editor_ui.is_playing = app.play_mode == super::game_state::PlayMode::Playing
        || app.play_mode == super::game_state::PlayMode::Paused;
    app.editor.editor_ui.is_paused = app.play_mode == super::game_state::PlayMode::Paused;

    // Sync inspector editing state from current entity data
    app.editor.editor_ui.sync_inspector_edit_state(&entity_info);
    // Refresh script variables for the selected entity
    app.editor.editor_ui.refresh_script_vars(&app.world);

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
                    frame_time_ms: dt * 1000.0,
                    loader: &mut app.editor.background_loader,
                    thumbnail_texture_handles: &app.editor.thumbnail_texture_handles,
                    llm_config: &app.editor.llm_config,
                    undo_count: app.editor.undo_stack.len(),
                    redo_count: app.editor.redo_stack.len(),
                    agent_undo_count: app.editor.agent_undo_stack.len(),
                    audio_levels: app
                        .audio_system
                        .as_ref()
                        .map_or(katla_audio::LevelsSnapshot::default(), |a| {
                            a.engine().read_levels()
                        }),
                    audio_active_voices: app
                        .audio_system
                        .as_ref()
                        .map_or(0, |a| a.engine().active_voice_count()),
                    audio_peak_voices: app
                        .audio_system
                        .as_ref()
                        .map_or(0, |a| a.engine().peak_voice_count()),
                },
            )
            .clone()
    };

    // Apply real-time inspector slider changes to ECS during drag.
    // This happens every frame while a slider is being dragged so the viewport updates immediately.
    // Must happen before borrowing ui_renderer to avoid double mutable borrow.
    handle_inspector_drag_undo(app);
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

/// Detect inspector slider drag start/end and manage undo snapshots.
///
/// Compares inspector edit state before and after UI render to detect active slider dragging.
/// On drag start, snapshots pre-drag ECS values. On drag end, pushes an UndoGroup.
fn handle_inspector_drag_undo(app: &mut Application) {
    let entity_id = match app.editor.editor_ui.inspector_edit_entity {
        Some(id) => id,
        None => {
            app.editor.inspector_slider_was_active = false;
            app.editor.inspector_drag_snapshot = None;
            return;
        }
    };

    let edit = &app.editor.editor_ui.inspector_edit;
    let slider_active = inspector_values_differ_from_ecs(entity_id, edit, &app.world);

    let was_active = app.editor.inspector_slider_was_active;

    if slider_active && !was_active {
        app.editor.inspector_drag_snapshot = Some(snapshot_inspector_state(app, entity_id));
    }

    if !slider_active
        && was_active
        && let Some(snapshot) = app.editor.inspector_drag_snapshot.take()
        && inspector_snapshot_differs_from_ecs(entity_id, &snapshot, &app.world)
    {
        let mut undo_group = UndoGroup::new("Inspector slider drag");
        undo_group
            .commands
            .push(Box::new(InspectorDragUndo::new(snapshot)));
        app.editor.push_undo(undo_group);
    }

    app.editor.inspector_slider_was_active = slider_active;
    if !slider_active {
        app.editor.inspector_drag_snapshot = None;
    }
}

/// Check if the current inspector edit state differs from ECS component values.
fn inspector_values_differ_from_ecs(
    entity: EntityId,
    edit: &crate::ui::InspectorEditState,
    world: &katla_ecs::World,
) -> bool {
    if let Some(transform) = world.get_component::<TransformComponent>(entity) {
        let pos_vec = Vec3::new(edit.pos[0], edit.pos[1], edit.pos[2]);
        let rot_vec = Vec3::new(edit.rot[0], edit.rot[1], edit.rot[2]);
        let scale_vec = Vec3::new(edit.scale[0], edit.scale[1], edit.scale[2]);

        if (pos_vec - transform.transform.position).length() > 1e-4 {
            return true;
        }
        let euler = transform.transform.rotation.to_euler();
        if (rot_vec.x() - euler.0).abs() > 1e-3
            || (rot_vec.y() - euler.1).abs() > 1e-3
            || (rot_vec.z() - euler.2).abs() > 1e-3
        {
            return true;
        }
        if (scale_vec - transform.transform.scale).length() > 1e-4 {
            return true;
        }
    }

    if let Some(light) = world.get_component::<PointLight>(entity) {
        if (edit.light_color[0] - light.color[0]).abs() > 1e-3
            || (edit.light_color[1] - light.color[1]).abs() > 1e-3
            || (edit.light_color[2] - light.color[2]).abs() > 1e-3
        {
            return true;
        }
        if (edit.light_intensity - light.intensity).abs() > 1e-4 {
            return true;
        }
        if (edit.light_range - light.range).abs() > 1e-4 {
            return true;
        }
    }

    if let Some(emitter) = world.get_component::<ParticleEmitterComponent>(entity) {
        if (edit.emit_rate - emitter.config.emit_rate).abs() > 1e-4 {
            return true;
        }
        if (edit.velocity - emitter.config.velocity_magnitude).abs() > 1e-4 {
            return true;
        }
        if (edit.lifetime - emitter.config.base_lifetime).abs() > 1e-4 {
            return true;
        }
        if (edit.gravity - emitter.config.gravity).abs() > 1e-4 {
            return true;
        }
        if (edit.particle_scale - emitter.config.base_scale).abs() > 1e-4 {
            return true;
        }
    }

    if let Some(persp) = world.get_component::<PerspectiveComponent>(entity) {
        if (edit.fov - persp.fov).abs() > 1e-4 {
            return true;
        }
        if (edit.near - persp.near).abs() > 1e-4 {
            return true;
        }
        if (edit.aspect_ratio - persp.aspect_ratio).abs() > 1e-4 {
            return true;
        }
    }

    if let Some(dl) = world.get_component::<DirectionalLight>(entity) {
        if (edit.directional_direction[0] - dl.direction.x()).abs() > 1e-3
            || (edit.directional_direction[1] - dl.direction.y()).abs() > 1e-3
            || (edit.directional_direction[2] - dl.direction.z()).abs() > 1e-3
        {
            return true;
        }
        if (edit.directional_color[0] - dl.color[0]).abs() > 1e-3
            || (edit.directional_color[1] - dl.color[1]).abs() > 1e-3
            || (edit.directional_color[2] - dl.color[2]).abs() > 1e-3
        {
            return true;
        }
        if (edit.directional_intensity - dl.intensity).abs() > 1e-4 {
            return true;
        }
    }

    if let Some(ae) = world.get_component::<crate::components::AudioEmitter>(entity) {
        if (edit.audio_volume - ae.volume).abs() > 1e-4 {
            return true;
        }
        if (edit.audio_min_distance - ae.min_distance).abs() > 1e-4 {
            return true;
        }
        if (edit.audio_max_distance - ae.max_distance).abs() > 1e-4 {
            return true;
        }
        if (edit.audio_rolloff_factor - ae.rolloff_factor).abs() > 1e-4 {
            return true;
        }
    }

    false
}

/// Check if a pre-drag snapshot differs from current ECS values.
fn inspector_snapshot_differs_from_ecs(
    entity: EntityId,
    snapshot: &InspectorDragSnapshot,
    world: &katla_ecs::World,
) -> bool {
    if let Some(transform) = world.get_component::<TransformComponent>(entity) {
        if (snapshot.position - transform.transform.position).length() > 1e-4 {
            return true;
        }
        if (snapshot.scale - transform.transform.scale).length() > 1e-4 {
            return true;
        }
    }
    if let Some(light) = world.get_component::<PointLight>(entity) {
        if let Some(color) = snapshot.light_color
            && ((color[0] - light.color[0]).abs() > 1e-3
                || (color[1] - light.color[1]).abs() > 1e-3
                || (color[2] - light.color[2]).abs() > 1e-3)
        {
            return true;
        }
        if let Some(intensity) = snapshot.light_intensity
            && (intensity - light.intensity).abs() > 1e-4
        {
            return true;
        }
        if let Some(range) = snapshot.light_range
            && (range - light.range).abs() > 1e-4
        {
            return true;
        }
    }
    if let Some(emitter) = world.get_component::<ParticleEmitterComponent>(entity) {
        if let Some(rate) = snapshot.emit_rate
            && (rate - emitter.config.emit_rate).abs() > 1e-4
        {
            return true;
        }
        if let Some(vel) = snapshot.velocity
            && (vel - emitter.config.velocity_magnitude).abs() > 1e-4
        {
            return true;
        }
        if let Some(life) = snapshot.lifetime
            && (life - emitter.config.base_lifetime).abs() > 1e-4
        {
            return true;
        }
        if let Some(grav) = snapshot.gravity
            && (grav - emitter.config.gravity).abs() > 1e-4
        {
            return true;
        }
        if let Some(sc) = snapshot.particle_scale
            && (sc - emitter.config.base_scale).abs() > 1e-4
        {
            return true;
        }
    }
    false
}

/// Apply real-time inspector slider changes to ECS components during drag.
///
/// Compares the inspector editing state against the current ECS component values.
/// If they differ, updates the ECS component immediately (for viewport feedback).
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
        script_path: _,
        fov,
        near,
        aspect_ratio,
        directional_direction,
        directional_color,
        directional_intensity,
        audio_source_path: _,
        audio_volume,
        audio_looping: _,
        audio_spatial: _,
        audio_min_distance,
        audio_max_distance,
        audio_rolloff_factor,
        collider_shape_type: _,
        collider_sphere_radius,
        collider_box_half_extents,
        collider_capsule_half_height,
        collider_capsule_radius,
        rigid_body_type: _,
        rigid_body_gravity_scale,
        rigid_body_velocity: _,
        physics_friction,
        physics_restitution,
        physics_density,
        script_vars: _,
    } = &app.editor.editor_ui.inspector_edit;

    let _ = (emit_rate, velocity, lifetime, gravity, particle_scale);

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
        }
    }

    // PointLight
    if let Some(light) = app.world.get_component_mut::<PointLight>(entity_id) {
        let color_changed = (light_color[0] - light.color[0]).abs() > 1e-3
            || (light_color[1] - light.color[1]).abs() > 1e-3
            || (light_color[2] - light.color[2]).abs() > 1e-3;
        let intensity_changed = (*light_intensity - light.intensity).abs() > 1e-4;
        let range_changed = (*light_range - light.range).abs() > 1e-4;

        if color_changed || intensity_changed || range_changed {
            light.color = *light_color;
            light.intensity = *light_intensity;
            light.range = *light_range;
        }
    }

    // ParticleEmitter
    if let Some(emitter) = app
        .world
        .get_component_mut::<ParticleEmitterComponent>(entity_id)
    {
        let rate_changed = (*emit_rate - emitter.config.emit_rate).abs() > 1e-4;
        let vel_changed = (*velocity - emitter.config.velocity_magnitude).abs() > 1e-4;
        let life_changed = (*lifetime - emitter.config.base_lifetime).abs() > 1e-4;
        let grav_changed = (*gravity - emitter.config.gravity).abs() > 1e-4;
        let scale_changed = (*particle_scale - emitter.config.base_scale).abs() > 1e-4;

        if rate_changed || vel_changed || life_changed || grav_changed || scale_changed {
            emitter.config.emit_rate = *emit_rate;
            emitter.config.velocity_magnitude = *velocity;
            emitter.config.base_lifetime = *lifetime;
            emitter.config.gravity = *gravity;
            emitter.config.base_scale = *particle_scale;
        }
    }

    // PerspectiveComponent
    if let Some(persp) = app
        .world
        .get_component_mut::<PerspectiveComponent>(entity_id)
    {
        let fov_changed = (*fov - persp.fov).abs() > 1e-4;
        let near_changed = (*near - persp.near).abs() > 1e-4;
        let aspect_changed = (*aspect_ratio - persp.aspect_ratio).abs() > 1e-4;
        if fov_changed || near_changed || aspect_changed {
            persp.fov = *fov;
            persp.near = *near;
            persp.aspect_ratio = *aspect_ratio;
        }
    }

    // DirectionalLight
    if let Some(dl) = app.world.get_component_mut::<DirectionalLight>(entity_id) {
        let dir_changed = (directional_direction[0] - dl.direction.x()).abs() > 1e-3
            || (directional_direction[1] - dl.direction.y()).abs() > 1e-3
            || (directional_direction[2] - dl.direction.z()).abs() > 1e-3;
        let color_changed = (directional_color[0] - dl.color[0]).abs() > 1e-3
            || (directional_color[1] - dl.color[1]).abs() > 1e-3
            || (directional_color[2] - dl.color[2]).abs() > 1e-3;
        let intensity_changed = (*directional_intensity - dl.intensity).abs() > 1e-4;
        if dir_changed || color_changed || intensity_changed {
            dl.direction = Vec3::new(
                directional_direction[0],
                directional_direction[1],
                directional_direction[2],
            );
            dl.color = *directional_color;
            dl.intensity = *directional_intensity;
        }
    }

    // AudioEmitter
    if let Some(ae) = app
        .world
        .get_component_mut::<crate::components::AudioEmitter>(entity_id)
    {
        let vol_changed = (*audio_volume - ae.volume).abs() > 1e-4;
        let min_changed = (*audio_min_distance - ae.min_distance).abs() > 1e-4;
        let max_changed = (*audio_max_distance - ae.max_distance).abs() > 1e-4;
        let roll_changed = (*audio_rolloff_factor - ae.rolloff_factor).abs() > 1e-4;

        if vol_changed || min_changed || max_changed || roll_changed {
            ae.volume = *audio_volume;
            ae.min_distance = *audio_min_distance;
            ae.max_distance = *audio_max_distance;
            ae.rolloff_factor = *audio_rolloff_factor;
        }
    }

    // ColliderShape
    if let Some(cs) = app
        .world
        .get_component_mut::<katla_physics::ColliderShape>(entity_id)
    {
        let changed = match cs {
            katla_physics::ColliderShape::Sphere(s) => {
                (*collider_sphere_radius - s.radius).abs() > 1e-4
            }
            katla_physics::ColliderShape::Box(b) => {
                (collider_box_half_extents[0] - b.half_extents[0]).abs() > 1e-4
                    || (collider_box_half_extents[1] - b.half_extents[1]).abs() > 1e-4
                    || (collider_box_half_extents[2] - b.half_extents[2]).abs() > 1e-4
            }
            katla_physics::ColliderShape::Capsule(c) => {
                (*collider_capsule_half_height - c.half_height).abs() > 1e-4
                    || (*collider_capsule_radius - c.radius).abs() > 1e-4
            }
            katla_physics::ColliderShape::Trimesh(_)
            | katla_physics::ColliderShape::ConvexHull(_)
            | katla_physics::ColliderShape::Heightfield(_) => false,
        };
        if changed {
            match cs {
                katla_physics::ColliderShape::Sphere(s) => s.radius = *collider_sphere_radius,
                katla_physics::ColliderShape::Box(b) => {
                    b.half_extents = *collider_box_half_extents;
                }
                katla_physics::ColliderShape::Capsule(c) => {
                    c.half_height = *collider_capsule_half_height;
                    c.radius = *collider_capsule_radius;
                }
                katla_physics::ColliderShape::Trimesh(_)
                | katla_physics::ColliderShape::ConvexHull(_)
                | katla_physics::ColliderShape::Heightfield(_) => {}
            }
            if let Some(rb) = app
                .world
                .get_component_mut::<katla_physics::RigidBody>(entity_id)
            {
                rb.body_handle = None;
                rb.collider_handle = None;
            }
        }
    }

    // RigidBody gravity scale
    if let Some(rb) = app
        .world
        .get_component_mut::<katla_physics::RigidBody>(entity_id)
        && (*rigid_body_gravity_scale - rb.gravity_scale).abs() > 1e-4
    {
        rb.gravity_scale = *rigid_body_gravity_scale;
    }

    // PhysicsMaterial
    if let Some(pm) = app
        .world
        .get_component_mut::<katla_physics::PhysicsMaterial>(entity_id)
    {
        let friction_changed = (*physics_friction - pm.friction).abs() > 1e-4;
        let restitution_changed = (*physics_restitution - pm.restitution).abs() > 1e-4;
        let density_changed = (*physics_density - pm.density).abs() > 1e-4;
        if friction_changed || restitution_changed || density_changed {
            pm.friction = *physics_friction;
            pm.restitution = *physics_restitution;
            pm.density = *physics_density;
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
                let spawned_entity = match model_type {
                    SpawnableModel::Cube => app.spawn_test_cube(pos, [1.0, 1.0, 1.0]),
                    SpawnableModel::Sphere => app.spawn_sphere(pos, 0.7, 32, 16),
                    SpawnableModel::Cylinder => app.spawn_cylinder(pos, 1.5, 0.5, 32),
                    SpawnableModel::Plane => app.spawn_plane(pos, 5.0, 5.0),
                    SpawnableModel::Torus => app.spawn_torus(pos, 0.8, 0.2, 32, 16),
                };
                let mut undo_group = UndoGroup::new("Spawn model");
                undo_group
                    .commands
                    .push(Box::new(EditorSpawnCommand::new(spawned_entity)));
                record_entity_gpu_handles(app, spawned_entity);
                app.editor.push_undo(undo_group);
            }
            EditorAction::SaveScene => {
                let path = crate::scene::default_scene_path();
                match crate::scene::SceneManager::save_to_file(app, &path) {
                    Ok(()) => {
                        info!("Scene saved to {:?}", path);
                        app.editor.editor_ui.show_save_confirmation();
                    }
                    Err(e) => log::error!("Failed to save scene: {}", e),
                }
            }
            EditorAction::OpenScene => {
                let path = crate::scene::default_scene_path();
                match crate::scene::SceneManager::load_from_file(app, &path) {
                    Ok(()) => {
                        app.editor.clear_entity_references();
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
                if let Some(vulkan_renderer) = app.renderer.as_vulkan() {
                    for id in &to_remove {
                        if let Some(emitter) =
                            app.world.get_component_mut::<ParticleEmitterComponent>(*id)
                            && let Some(handle) = emitter.emitter_handle.take()
                            && let Some(ps) = &mut vulkan_renderer.particle_system
                        {
                            ps.destroy_emitter(handle, emitter.kill_on_destroy);
                        }
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
                app.editor.clear_entity_references();
                info!("New scene created");
            }
            EditorAction::Quit => {
                app.quit_requested = true;
            }
            EditorAction::Undo => {
                app.editor.perform_undo(&mut app.world);
                process_gpu_cleanup_for_destroyed_entities(app);
            }
            EditorAction::Redo => {
                app.editor.perform_redo(&mut app.world);
                process_gpu_cleanup_for_destroyed_entities(app);
            }
            EditorAction::AgentUndo => {
                app.editor.perform_agent_undo(&mut app.world);
                process_gpu_cleanup_for_destroyed_entities(app);
            }
            EditorAction::SelectEntity(entity_id) => {
                info!("Selected entity {:?}", entity_id);
            }
            EditorAction::SetTheme(theme_key) => {
                if let Some(theme) = crate::ui::ColorScheme::by_name(&theme_key) {
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
            EditorAction::TogglePhysicsDebug => {
                app.editor.editor_ui.show_physics_debug = !app.editor.editor_ui.show_physics_debug;
                app.preferences.show_physics_debug = app.editor.editor_ui.show_physics_debug;
            }
            EditorAction::ToggleReverbDebug => {
                app.editor.editor_ui.show_reverb_debug = !app.editor.editor_ui.show_reverb_debug;
                app.preferences.show_reverb_debug = app.editor.editor_ui.show_reverb_debug;
            }
            EditorAction::SetFontScale(scale) => {
                app.editor.editor_ui.set_font_scale(scale);
                app.preferences.font_scale = scale;
                info!("Font scale changed to: {:.0}%", scale * 100.0);
            }
            EditorAction::SetMasterVolume(vol) => {
                app.preferences.audio.master_volume = vol;
                if let Some(ref audio) = app.audio_system {
                    audio.engine().set_master_volume(vol);
                }
            }
            EditorAction::SetSfxVolume(vol) => {
                app.preferences.audio.sfx_volume = vol;
                if let Some(ref audio) = app.audio_system {
                    audio
                        .engine()
                        .set_category_volume(katla_audio::AudioCategory::Sfx, vol);
                }
            }
            EditorAction::SetMusicVolume(vol) => {
                app.preferences.audio.music_volume = vol;
                if let Some(ref audio) = app.audio_system {
                    audio
                        .engine()
                        .set_category_volume(katla_audio::AudioCategory::Music, vol);
                }
            }
            EditorAction::SetAmbientVolume(vol) => {
                app.preferences.audio.ambient_volume = vol;
                if let Some(ref audio) = app.audio_system {
                    audio
                        .engine()
                        .set_category_volume(katla_audio::AudioCategory::Ambient, vol);
                }
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
                if let katla_gfx::AnyRenderer::Vulkan(vulkan_renderer) = &mut app.renderer
                    && let Some(ps) = &mut vulkan_renderer.particle_system
                {
                    use katla_gfx::particles::EmitterHandle;

                    let entity_configs: Vec<(
                        EntityId,
                        EmitterHandle,
                        katla_gfx::particles::EmitterConfig,
                        bool,
                    )> = app
                        .world
                        .query::<&mut ParticleEmitterComponent>()
                        .filter_map(|(id, emitter)| {
                            emitter
                                .emitter_handle
                                .map(|h| (id, h, emitter.config, emitter.kill_on_destroy))
                        })
                        .collect();

                    for (id, handle, _config, kill_on_destroy) in &entity_configs {
                        ps.destroy_emitter(*handle, *kill_on_destroy);
                        if let Some(emitter) =
                            app.world.get_component_mut::<ParticleEmitterComponent>(*id)
                        {
                            emitter.emitter_handle = None;
                        }
                    }

                    if let Err(e) = ps.reset_all() {
                        log::error!("Failed to reset particle system: {}", e);
                    }

                    for (id, _old_handle, config, _kill_on_destroy) in entity_configs {
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
            EditorAction::SetGizmoMode(mode_id) => {
                let mode = match mode_id {
                    0 => crate::gizmo::GizmoMode::Translate,
                    1 => crate::gizmo::GizmoMode::Rotate,
                    2 => crate::gizmo::GizmoMode::Scale,
                    _ => crate::gizmo::GizmoMode::Translate,
                };
                app.editor.gizmo_state.set_mode(mode);
            }
            EditorAction::CoCreatorRequest(text) => {
                agent::process_co_creator_request(app, &text);
            }
            EditorAction::SetLlmProvider(key) => {
                use katla_agent::config::LlmProviderKind;
                app.editor.llm_config.provider = match key.as_str() {
                    "open_ai" => LlmProviderKind::OpenAi,
                    "open_ai_compatible" => LlmProviderKind::OpenAiCompatible,
                    _ => LlmProviderKind::Disabled,
                };
            }
            EditorAction::SetLlmApiKey(key) => {
                app.editor.llm_config.api_key = key;
            }
            EditorAction::SetLlmBaseUrl(url) => {
                app.editor.llm_config.base_url = if url.is_empty() { None } else { Some(url) };
            }
            EditorAction::SetLlmModel(model) => {
                app.editor.llm_config.model = model;
            }
            EditorAction::SetLlmMaxTokens(tokens) => {
                app.editor.llm_config.max_tokens = tokens;
            }
            EditorAction::SetLlmTemperature(temp) => {
                app.editor.llm_config.temperature = temp.clamp(0.0, 2.0);
            }
            EditorAction::SaveLlmConfig => {
                if let Err(e) = app.editor.llm_config.save() {
                    log::error!("Failed to save LLM config: {}", e);
                } else {
                    info!("LLM configuration saved");
                }
            }
            EditorAction::PlayStart => {
                if app.play_mode == super::game_state::PlayMode::Editing {
                    app.scene_snapshot = Some(super::game_state::SceneSnapshot::capture(app));
                    app.play_mode = super::game_state::PlayMode::Playing;
                    if let Some(active) =
                        app.world.get_resource_mut::<katla_script::ScriptsActive>()
                    {
                        active.0 = true;
                    }
                    if let Some(physics) =
                        app.world.get_resource_mut::<katla_physics::PhysicsActive>()
                    {
                        physics.0 = true;
                    }
                    info!("Entered play mode");
                }
            }
            EditorAction::PlayPause => match app.play_mode {
                super::game_state::PlayMode::Playing => {
                    app.play_mode = super::game_state::PlayMode::Paused;
                    if let Some(active) =
                        app.world.get_resource_mut::<katla_script::ScriptsActive>()
                    {
                        active.0 = false;
                    }
                    if let Some(physics) =
                        app.world.get_resource_mut::<katla_physics::PhysicsActive>()
                    {
                        physics.0 = false;
                    }
                    info!("Play mode paused");
                }
                super::game_state::PlayMode::Paused => {
                    app.play_mode = super::game_state::PlayMode::Playing;
                    if let Some(active) =
                        app.world.get_resource_mut::<katla_script::ScriptsActive>()
                    {
                        active.0 = true;
                    }
                    if let Some(physics) =
                        app.world.get_resource_mut::<katla_physics::PhysicsActive>()
                    {
                        physics.0 = true;
                    }
                    info!("Play mode resumed");
                }
                super::game_state::PlayMode::Editing => {}
            },
            EditorAction::PlayStop => {
                if app.play_mode != super::game_state::PlayMode::Editing {
                    if let Some(snapshot) = app.scene_snapshot.take() {
                        snapshot.restore(app);
                    }
                    app.editor.clear_entity_references();
                    app.play_mode = super::game_state::PlayMode::Editing;
                    if let Some(active) =
                        app.world.get_resource_mut::<katla_script::ScriptsActive>()
                    {
                        active.0 = false;
                    }
                    if let Some(physics) =
                        app.world.get_resource_mut::<katla_physics::PhysicsActive>()
                    {
                        physics.0 = false;
                    }
                    info!("Stopped play mode, scene restored");
                }
            }
            EditorAction::SetEmitterField { entity, field } => {
                let _ = entity;
                if let Some(emitter) = app
                    .world
                    .get_component_mut::<ParticleEmitterComponent>(entity)
                {
                    use crate::ui::EmitterField;
                    match field {
                        EmitterField::EmitRate(v) => emitter.config.emit_rate = v,
                        EmitterField::BaseLifetime(v) => emitter.config.base_lifetime = v,
                        EmitterField::LifetimeVariation(v) => emitter.config.lifetime_variation = v,
                        EmitterField::VelocityMagnitude(v) => emitter.config.velocity_magnitude = v,
                        EmitterField::VelocityConeAngle(v) => {
                            emitter.config.velocity_cone_angle = v
                        }
                        EmitterField::BaseScale(v) => emitter.config.base_scale = v,
                        EmitterField::ScaleVariation(v) => emitter.config.scale_variation = v,
                        EmitterField::Gravity(v) => emitter.config.gravity = v,
                        EmitterField::TurbulenceStrength(v) => {
                            emitter.config.turbulence_strength = v
                        }
                        EmitterField::TurbulenceFrequency(v) => {
                            emitter.config.turbulence_frequency = v
                        }
                        EmitterField::Color(v) => emitter.config.color = v,
                        EmitterField::ColorVariation(v) => emitter.config.color_variation = v,
                        EmitterField::ColorEnd(v) => {
                            use katla_gfx::particles::Align16Vec4;
                            emitter.config.color_end = Align16Vec4(v)
                        }
                        EmitterField::ScaleEnd(v) => emitter.config.scale_end = v,
                        EmitterField::ShapePoint => {
                            emitter.config.shape = katla_gfx::particles::EmitterShape::Point;
                            emitter.config.shape_params = [0.0; 4];
                        }
                        EmitterField::ShapeLine => {
                            emitter.config.shape = katla_gfx::particles::EmitterShape::Line;
                        }
                        EmitterField::ShapeCircle => {
                            emitter.config.shape = katla_gfx::particles::EmitterShape::Circle;
                        }
                        EmitterField::ShapeSphere => {
                            emitter.config.shape = katla_gfx::particles::EmitterShape::Sphere;
                        }
                        EmitterField::ShapeBox => {
                            emitter.config.shape = katla_gfx::particles::EmitterShape::Box;
                        }
                        EmitterField::ShapeParam0(v) => emitter.config.shape_params[0] = v,
                        EmitterField::ShapeParam1(v) => emitter.config.shape_params[1] = v,
                        EmitterField::ShapeParam2(v) => emitter.config.shape_params[2] = v,
                    }
                }
            }
            EditorAction::AudioPreviewToggle { path } => {
                if let Some(ref mut audio_sys) = app.audio_system {
                    let key = path.to_string_lossy().to_string();
                    if let Some(handle) = app.editor.preview_voice.take() {
                        handle.stop();
                    } else {
                        let buffer = audio_sys.get_or_load_buffer(&key);
                        if let Some(buf) = buffer {
                            let handle = audio_sys.engine().play(&buf);
                            app.editor.preview_voice = Some(handle);
                        }
                    }
                }
            }
        }
    }

    // Poll for pending LLM stream chunks each frame
    agent::poll_llm_stream(app);

    // Poll for MCP server requests
    #[cfg(feature = "mcp")]
    {
        let registry = &app.editor.component_registry;
        let protected = mcp::ProtectedEntities {
            camera_entity: app.camera.entity,
            gizmo_entity: app.editor.gizmo_state.entity,
        };
        app.editor.mcp_state.poll(app, registry, &protected);
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
    if let Some(ref window) = app.window {
        window.set_cursor(cursor_icon);
    }

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
            let shape_name = match emitter.config.shape {
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
                color_end: emitter.config.color_end.0,
                scale_end: emitter.config.scale_end,
                gravity: emitter.config.gravity,
                turbulence_strength: emitter.config.turbulence_strength,
                turbulence_frequency: emitter.config.turbulence_frequency,
            });
        }
    }

    // Get system-wide stats
    let stats = match &app.renderer {
        katla_gfx::AnyRenderer::Vulkan(vulkan_renderer) => {
            vulkan_renderer.particle_system.as_ref().map(|ps| {
                let s = ps.get_stats();
                ParticleStats {
                    max_alive_count: s.max_alive_count,
                    current_alive_count: s.current_alive_count,
                    dead_count: s.dead_count,
                    total_emitted: s.total_emitted,
                    total_died: s.total_died,
                    compute_time_ms: s.compute_time_ms,
                    avg_compute_time_ms: s.avg_compute_time_ms,
                    peak_compute_time_ms: s.peak_compute_time_ms,
                    emitter_counts: s.emitter_counts,
                    memory_used_mb: s.memory_used_mb,
                    buffer_utilization: s.buffer_utilization,
                    frame_count: s.frame_count,
                    total_dispatches: s.total_dispatches,
                }
            })
        }
        #[cfg(target_os = "macos")]
        katla_gfx::AnyRenderer::Metal(_) => None,
    };

    app.editor.editor_ui.particle_inspector_data = ParticleInspectorData {
        emitter_entities,
        selected_emitter_entity: app.editor.editor_ui.selected_particle_emitter,
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
        Option<String>,
        Option<PerspectiveInfo>,
        Option<DirectionalLightInfo>,
        Option<crate::ui::AudioEmitterInfo>,
        Option<crate::ui::AudioSourceInfo>,
        bool,
        Option<ColliderShapeInfo>,
        Option<RigidBodyInfo>,
        Option<PhysicsMaterialInfo>,
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

        // Query each component type once and reuse results
        let has_name = app
            .world
            .get_component::<NameComponent>(entity_id)
            .is_some();
        let has_drawable = app
            .world
            .get_component::<DrawableComponent>(entity_id)
            .is_some();
        let point_light =
            app.world
                .get_component::<PointLight>(entity_id)
                .map(|pl| PointLightInfo {
                    color: pl.color,
                    intensity: pl.intensity,
                    range: pl.range,
                });
        let _particle_emitter = app
            .world
            .get_component::<ParticleEmitterComponent>(entity_id)
            .map(|pe| ParticleEmitterInfo {
                emit_rate: pe.config.emit_rate,
                velocity_magnitude: pe.config.velocity_magnitude,
                base_lifetime: pe.config.base_lifetime,
                gravity: pe.config.gravity,
                base_scale: pe.config.base_scale,
            });
        let particle_emitter: Option<ParticleEmitterInfo> = None;
        let has_parent = app.world.get_component::<Parent>(entity_id).is_some();
        let has_children = app.world.get_component::<Children>(entity_id).is_some();

        let perspective_info = app
            .world
            .get_component::<PerspectiveComponent>(entity_id)
            .map(|p| PerspectiveInfo {
                fov: p.fov,
                near: p.near,
                aspect_ratio: p.aspect_ratio,
            });
        let directional_info = app
            .world
            .get_component::<DirectionalLight>(entity_id)
            .map(|dl| DirectionalLightInfo {
                direction: [dl.direction.x(), dl.direction.y(), dl.direction.z()],
                color: dl.color,
                intensity: dl.intensity,
            });

        let audio_emitter_info = app
            .world
            .get_component::<crate::components::AudioEmitter>(entity_id)
            .map(|ae| crate::ui::AudioEmitterInfo {
                source_path: ae.source_path.clone(),
                volume: ae.volume,
                looping: ae.looping,
                playing: ae.playing,
                spatial: ae.spatial,
                min_distance: ae.min_distance,
                max_distance: ae.max_distance,
                rolloff_factor: ae.rolloff_factor,
            });

        // Build component list from cached query results
        let mut components: Vec<&'static str> = Vec::with_capacity(12);
        components.push("Transform");
        if has_name {
            components.push("Name");
        }
        if has_drawable {
            components.push("Drawable");
        }
        if directional_info.is_some() {
            components.push("DirectionalLight");
        }
        if point_light.is_some() {
            components.push("PointLight");
        }
        if particle_emitter.is_some() {
            components.push("ParticleEmitter");
        }
        if perspective_info.is_some() {
            components.push("PerspectiveComponent");
        }
        if app
            .world
            .get_component::<katla_script::ScriptComponent>(entity_id)
            .is_some()
        {
            components.push("ScriptComponent");
        }
        if audio_emitter_info.is_some() {
            components.push("AudioEmitter");
        }

        let audio_source_info = app
            .world
            .get_component::<crate::components::AudioSource>(entity_id)
            .map(|src| {
                let (sample_rate, channels, duration_secs) =
                    katla_audio::audio_metadata(std::path::Path::new(&src.path))
                        .map(|m| (Some(m.sample_rate), Some(m.channels), Some(m.duration_secs)))
                        .unwrap_or((None, None, None));
                crate::ui::AudioSourceInfo {
                    path: src.path.clone(),
                    sample_rate,
                    channels,
                    duration_secs,
                }
            });

        let has_audio_listener = app
            .world
            .get_component::<crate::components::AudioListener>(entity_id)
            .is_some();

        if audio_source_info.is_some() {
            components.push("AudioSource");
        }
        if has_audio_listener {
            components.push("AudioListener");
        }

        let collider_shape_info = app
            .world
            .get_component::<katla_physics::ColliderShape>(entity_id)
            .map(|cs| {
                let (shape_type, sphere_radius, box_he, capsule_hh, capsule_r) = match cs {
                    katla_physics::ColliderShape::Sphere(s) => (
                        ColliderShapeType::Sphere,
                        s.radius,
                        [0.5, 0.5, 0.5],
                        0.5,
                        0.25,
                    ),
                    katla_physics::ColliderShape::Box(b) => {
                        (ColliderShapeType::Box, 0.5, b.half_extents, 0.5, 0.25)
                    }
                    katla_physics::ColliderShape::Capsule(c) => (
                        ColliderShapeType::Capsule,
                        0.5,
                        [0.5, 0.5, 0.5],
                        c.half_height,
                        c.radius,
                    ),
                    katla_physics::ColliderShape::Trimesh(_)
                    | katla_physics::ColliderShape::ConvexHull(_)
                    | katla_physics::ColliderShape::Heightfield(_) => {
                        (ColliderShapeType::Sphere, 0.5, [0.5, 0.5, 0.5], 0.5, 0.25)
                    }
                };
                ColliderShapeInfo {
                    shape_type,
                    sphere_radius,
                    box_half_extents: box_he,
                    capsule_half_height: capsule_hh,
                    capsule_radius: capsule_r,
                }
            });

        let rigid_body_info = app
            .world
            .get_component::<katla_physics::RigidBody>(entity_id)
            .map(|rb| RigidBodyInfo {
                body_type: rb.body_type.into(),
                gravity_scale: rb.gravity_scale,
                linear_velocity: [
                    rb.linear_velocity.x(),
                    rb.linear_velocity.y(),
                    rb.linear_velocity.z(),
                ],
            });

        let physics_material_info = app
            .world
            .get_component::<katla_physics::PhysicsMaterial>(entity_id)
            .map(|pm| PhysicsMaterialInfo {
                friction: pm.friction,
                restitution: pm.restitution,
                density: pm.density,
            });

        if collider_shape_info.is_some() {
            components.push("ColliderShape");
        }
        if rigid_body_info.is_some() {
            components.push("RigidBody");
        }
        if physics_material_info.is_some() {
            components.push("PhysicsMaterial");
        }
        if has_parent {
            components.push("Parent");
        }
        if has_children {
            components.push("Children");
        }

        // Determine entity type from cached query results
        let entity_type = if directional_info.is_some() {
            "Directional Light"
        } else if point_light.is_some() {
            "Point Light"
        } else if has_drawable {
            "Mesh"
        } else {
            "Empty"
        };

        let script_path = app
            .world
            .get_component::<katla_script::ScriptComponent>(entity_id)
            .map(|s| s.script_path.clone());

        entity_data.insert(
            entity_id,
            (
                name,
                pos,
                rot,
                scale,
                entity_type.to_string(),
                components.into_iter().map(String::from).collect(),
                point_light,
                particle_emitter,
                script_path,
                perspective_info,
                directional_info,
                audio_emitter_info,
                audio_source_info,
                has_audio_listener,
                collider_shape_info,
                rigid_body_info,
                physics_material_info,
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
            let (
                name,
                pos,
                rot,
                scale,
                entity_type,
                components,
                point_light,
                particle_emitter,
                script_path,
                perspective,
                directional_light,
                audio_emitter,
                audio_source,
                has_audio_listener,
                collider_shape,
                rigid_body,
                physics_material,
            ) = data;

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
                script_path: script_path.clone(),
                perspective: perspective.clone(),
                directional_light: directional_light.clone(),
                audio_emitter: audio_emitter.clone(),
                audio_source: audio_source.clone(),
                has_audio_listener: *has_audio_listener,
                collider_shape: collider_shape.clone(),
                rigid_body: rigid_body.clone(),
                physics_material: physics_material.clone(),
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

/// Record GPU handles for a spawned entity so they can be released on undo.
pub fn record_entity_gpu_handles(app: &mut Application, entity: EntityId) {
    if let Some(drawable) = app.world.get_component::<DrawableComponent>(entity) {
        app.editor.entity_gpu_handles.insert(
            entity,
            GpuCleanupData {
                mesh_handle: drawable.mesh_handle,
                material_handle: drawable.material_handle,
                skeleton_handle: drawable.skeleton_handle,
            },
        );
    }
}

/// Release GPU resources for entities that have been destroyed via undo/redo.
///
/// Checks `EditorState::entity_gpu_handles` for entries whose entity no longer
/// exists in the world, releases those handles via the GPU resource tracker,
/// and destroys the underlying GPU objects.
pub fn process_gpu_cleanup_for_destroyed_entities(app: &mut Application) {
    let destroyed_entities: Vec<EntityId> = app
        .editor
        .entity_gpu_handles
        .keys()
        .filter(|id| !app.world.entity_exists(**id))
        .copied()
        .collect();

    for entity in destroyed_entities {
        if let Some(cleanup) = app.editor.entity_gpu_handles.remove(&entity) {
            let to_destroy = app.gpu_resource_tracker.release_drawable(
                cleanup.mesh_handle,
                cleanup.material_handle,
                cleanup.skeleton_handle,
            );
            for handle in &to_destroy.meshes {
                app.renderer.destroy_mesh(*handle);
            }
            for handle in &to_destroy.materials {
                app.renderer.destroy_material(*handle);
            }
            for handle in &to_destroy.skeletons {
                app.renderer.destroy_skeleton(*handle);
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

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

        let emitter = world
            .get_component::<ParticleEmitterComponent>(entity)
            .unwrap();
        assert_eq!(emitter.config.emit_rate, 100.0);
        assert_eq!(emitter.config.base_lifetime, 3.0);
        assert_eq!(emitter.config.gravity, -5.0);
        assert!(emitter.active);

        let ps: Option<katla_gfx::particles::GlobalParticleSystem> = None;
        assert!(ps.is_none());

        let emitter = world
            .get_component::<ParticleEmitterComponent>(entity)
            .unwrap();
        assert_eq!(emitter.config.emit_rate, 100.0);
        assert!(emitter.active);
    }
}
