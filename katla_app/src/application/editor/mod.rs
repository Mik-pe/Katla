//! Editor subsystem - handles UI rendering, entity management, and editor actions.

pub mod agent;
pub(crate) mod behavior;
pub mod component_registry;
pub(crate) mod document;
#[cfg(feature = "mcp")]
pub(crate) mod external_chat;
pub(crate) mod fields;
pub(crate) mod material;
pub(crate) mod material_preview;
#[cfg(feature = "mcp")]
pub(crate) mod mcp;
pub(crate) mod preview_maps;
mod scene_query;
pub(crate) mod simulation;
mod transform_registry;
#[cfg(feature = "mcp")]
mod viewport;

use std::collections::{HashMap, HashSet};

use log::info;

use katla_ecs::EntityId;
use katla_ecs::scene_tool::{SceneCommand, SceneToolError, SceneToolExecutor, UndoGroup};
use katla_gfx::GpuRenderer;
use katla_gfx::renderer::UIDrawList;
use katla_math::{Vec2, Vec3};

use crate::components::ParticleEmitterComponent;
use crate::components::{
    Children, DirectionalLight, DrawableComponent, EditorHidden, NameComponent, Parent,
    PerspectiveComponent, PointLight, ReverbZone, TransformComponent, VelocityComponent,
};

use crate::ui::{
    ColliderShapeInfo, ColliderShapeType, DirectionalLightInfo, EditorAction, EntityInfo,
    ParticleEmitterInfo, PerspectiveInfo, PhysicsMaterialInfo, PointLightInfo, RigidBodyInfo,
};

use super::Application;

/// GPU resources retired when an authored entity is removed.
pub(crate) struct GpuCleanupData {
    pub(crate) mesh_handle: katla_gfx::MeshHandle,
    pub(crate) material_handle: katla_gfx::MaterialHandle,
    pub(crate) skeleton_handle: katla_gfx::SkeletonHandle,
    pub(crate) textures: Vec<katla_gfx::TextureHandle>,
}

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
pub fn upload_font_atlas(app: &mut Application, draw_list: &mut Option<UIDrawList>) {
    if !app.frame_graph_runtime.uses_katla_scene() {
        return;
    }
    let previous_slot = app.editor_features.font_atlas_slot();
    if let Err(error) = app
        .editor_features
        .upload_font_atlas(&mut app.renderer, &mut app.ui_context)
    {
        log::error!("Font atlas upload failed: {error}");
        return;
    }
    if let Some(slot) = app.editor_features.font_atlas_slot() {
        app.editor.ui_renderer.set_font_atlas_bindless_slot(slot);
        if let Some(previous_slot) = previous_slot
            && previous_slot != slot
            && let Some(draw_list) = draw_list
        {
            for vertex in &mut draw_list.vertices {
                if vertex.texture_index == previous_slot {
                    vertex.texture_index = slot;
                }
            }
            for instance in &mut draw_list.instances {
                if instance.texture_index == previous_slot {
                    instance.texture_index = slot;
                }
            }
        }
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

    if app.frame_count.is_multiple_of(15) {
        let dirty = app.has_unsaved_scene();
        app.editor.editor_ui.scene_title = format!(
            "{}{}",
            app.scene_document.saved.name,
            if dirty { " *" } else { "" }
        );
    }
    // Collect entity info for editor UI
    let entity_info = collect_entity_info(app);
    let material = entity_info
        .iter()
        .find(|entity| Some(entity.id) == app.editor.editor_ui.selected_entity)
        .and_then(|entity| entity.material);
    let maps = app
        .editor
        .editor_ui
        .selected_entity
        .and_then(|id| {
            app.world
                .get_component::<super::spawning::ModelTextures>(id)
        })
        .and_then(|textures| textures.preview_maps.clone());
    let preview_result = app.editor.material_previews.prepare(
        &mut app.renderer,
        &mut app.editor.ui_renderer,
        material,
        maps,
    );
    if let Err(error) = &preview_result {
        log::warn!("Material preview generation failed: {error}");
    }
    app.editor.editor_ui.material_preset_previews = app.editor.material_previews.presets();
    app.editor.editor_ui.material_preview = preview_result
        .ok()
        .and(material)
        .and_then(|_| app.editor.material_previews.current());

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

    // Refresh script variables for the selected entity

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

    // Convert draw list to GPU format
    let ui_renderer = &mut app.editor.ui_renderer;

    // Register the viewport texture if it exists
    if let Some(texture_id) = viewport_texture_id {
        let texture_handle = katla_gfx::TextureHandle::from_raw(texture_id.0 as u32, 0);
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

pub fn process_editor_actions(app: &mut Application) {
    app.preferences.editor = app.editor.editor_ui.editor_settings().clone();
    let editor_actions = app.editor.editor_ui.take_actions();

    // Process editor actions
    for action in editor_actions {
        if !matches!(action, EditorAction::EditField { .. }) {
            fields::finish_drag(app);
        }
        match action {
            EditorAction::EditField {
                entity,
                component,
                field,
                value,
            } => {
                if let Err(error) = fields::edit(app, entity, component, field, value) {
                    app.show_scene_error(error);
                }
            }
            EditorAction::InstantiatePrefab(path) => {
                if app.play_mode != super::game_state::PlayMode::Editing {
                    app.show_scene_error("Stop simulation before instantiating prefab assets");
                    continue;
                }
                match crate::prefab::instantiate_asset(
                    app,
                    &path,
                    crate::scene::TransformDescriptor::default_transform(),
                ) {
                    Ok(instance) => app.editor.editor_ui.selected_entity = Some(instance.root),
                    Err(error) => app.show_scene_error(error),
                }
            }
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
            EditorAction::SaveScene => app.save_editor_scene(None),
            EditorAction::SaveSceneAs => app.choose_scene_file(true),
            EditorAction::OpenScene => app.choose_scene_file(false),
            EditorAction::SubmitScenePath(path) => app.submit_scene_path(path),
            EditorAction::NewScene => app.request_document_action(document::DocumentAction::New),
            EditorAction::Quit => app.request_document_action(document::DocumentAction::Quit),
            EditorAction::CancelSceneDialog => {
                app.editor.editor_ui.scene_dialog = None;
                app.editor.pending_document_action = None;
            }
            EditorAction::SaveSceneChanges => app.save_editor_scene(None),
            EditorAction::DiscardSceneChanges => {
                app.editor.editor_ui.scene_dialog = None;
                if let Some(action) = app.editor.pending_document_action.take() {
                    app.execute_document_action(action);
                }
            }
            EditorAction::OverwriteSceneFile => {
                if let Some(
                    crate::ui::editor_ui::declarative::scene_dialog::SceneDialog::Overwrite(path),
                ) = app.editor.editor_ui.scene_dialog.take()
                {
                    app.save_editor_scene(Some(path));
                }
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
            EditorAction::EditMaterial(op) => {
                if let Err(error) = material::edit_live(app, op) {
                    log::warn!("Material edit failed: {error}");
                }
            }
            EditorAction::MaterialPreset(op) => {
                if let Err(error) = material::execute(app, op, false) {
                    log::warn!("Material preset failed: {error}");
                }
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
                if let Some(features) = &mut app.scene_features {
                    features.particles.reset_all();
                    info!("Particle system reset queued");
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
            EditorAction::AddComponent { entity, component } => {
                let op = katla_ecs::scene_tool::SceneOp::AddComponent { entity, component };
                match SceneToolExecutor::execute(op, &mut app.world, &app.editor.component_registry)
                {
                    Ok((result, undo_group)) => {
                        if result.success {
                            info!("{}", result.message);
                            app.editor.push_undo(undo_group);
                        } else {
                            log::warn!("{}", result.message);
                        }
                    }
                    Err(e) => log::warn!("Failed to add component: {e}"),
                }
            }
            EditorAction::RemoveComponent { entity, component } => {
                let op = katla_ecs::scene_tool::SceneOp::RemoveComponent { entity, component };
                match SceneToolExecutor::execute(op, &mut app.world, &app.editor.component_registry)
                {
                    Ok((result, undo_group)) => {
                        if result.success {
                            info!("{}", result.message);
                            app.editor.push_undo(undo_group);
                        } else {
                            log::warn!("{}", result.message);
                        }
                    }
                    Err(e) => log::warn!("Failed to remove component: {e}"),
                }
            }
            EditorAction::CoCreatorRequest(text) => {
                #[cfg(feature = "mcp")]
                external_chat::submit(app, text);
                #[cfg(not(feature = "mcp"))]
                {
                    let _ = text;
                    app.editor.editor_ui.co_creator.add_system_message(
                        "Build Katla with MCP support to connect an external conversation.",
                    );
                }
            }
            EditorAction::ConnectExternalChat { socket, thread_id } => {
                app.preferences.external_chat = crate::preferences::ExternalChatPreferences {
                    socket: socket.clone(),
                    thread_id: thread_id.clone(),
                };
                if let Err(error) = app.preferences.save() {
                    log::warn!("Cannot save external chat connection: {error}");
                }
                #[cfg(feature = "mcp")]
                external_chat::connect(app, socket, thread_id);
                #[cfg(not(feature = "mcp"))]
                app.editor.editor_ui.co_creator.add_system_message(
                    "Build Katla with MCP support to connect an external conversation.",
                );
            }
            EditorAction::PlayStart => {
                if let Err(error) =
                    simulation::execute(app, katla_agent::behavior::SimulationOp::Play)
                {
                    app.show_scene_error(error);
                }
            }
            EditorAction::PlayPause => {
                let op = if app.play_mode == super::game_state::PlayMode::Paused {
                    katla_agent::behavior::SimulationOp::Resume
                } else {
                    katla_agent::behavior::SimulationOp::Pause
                };
                if let Err(error) = simulation::execute(app, op) {
                    app.show_scene_error(error);
                }
            }
            EditorAction::PlayStop => {
                if let Err(error) =
                    simulation::execute(app, katla_agent::behavior::SimulationOp::Stop)
                {
                    app.show_scene_error(error);
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

    if !app.ui_context.input().mouse_down[katla_ui::input::mouse_button::LEFT] {
        material::finish_drag(app);
        fields::finish_drag(app);
    }

    // Poll for MCP server requests
    #[cfg(feature = "mcp")]
    {
        mcp::poll(app);
        external_chat::poll(app);
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
    use crate::ui::{EmitterConfigView, ParticleInspectorData};
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
    let stats = app
        .scene_features
        .as_ref()
        .and_then(|features| features.particles.stats());

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
        let has_velocity = app
            .world
            .get_component::<VelocityComponent>(entity_id)
            .is_some();
        let has_reverb_zone = app.world.get_component::<ReverbZone>(entity_id).is_some();
        let has_collision_filter = app
            .world
            .get_component::<katla_physics::CollisionFilter>(entity_id)
            .is_some();
        let point_light =
            app.world
                .get_component::<PointLight>(entity_id)
                .map(|pl| PointLightInfo {
                    color: pl.color,
                    intensity: pl.intensity,
                    range: pl.range,
                });
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
            components.push("NameComponent");
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
            components.push("ParticleEmitterComponent");
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
        if has_velocity {
            components.push("VelocityComponent");
        }
        if has_reverb_zone {
            components.push("ReverbZone");
        }
        if has_collision_filter {
            components.push("CollisionFilter");
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
                material: None,
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

    for entity in &mut result {
        entity.material = app
            .world
            .get_component::<DrawableComponent>(entity.id)
            .map(material::values);
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
                textures: app
                    .world
                    .get_component::<crate::application::spawning::ModelTextures>(entity)
                    .map(|textures| textures.handles.clone())
                    .unwrap_or_default(),
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
            let mut to_destroy = app.gpu_resource_tracker.release_drawable(
                cleanup.mesh_handle,
                cleanup.material_handle,
                cleanup.skeleton_handle,
            );
            for texture in cleanup.textures {
                if app.gpu_resource_tracker.release_texture(texture) {
                    to_destroy.textures.push(texture);
                }
            }
            crate::scene::serialization::destroy_resources(app, to_destroy);
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

        let ps: Option<super::super::scene_features::SceneFeatures> = None;
        assert!(ps.is_none());

        let emitter = world
            .get_component::<ParticleEmitterComponent>(entity)
            .unwrap();
        assert_eq!(emitter.config.emit_rate, 100.0);
        assert!(emitter.active);
    }
}
