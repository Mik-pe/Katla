//! Editor subsystem - handles UI rendering, entity management, and editor actions.

use std::collections::{HashMap, HashSet};

use log::info;

use katla_ecs::EntityId;
use katla_gfx::renderer::UIDrawList;
use katla_math::{Vec2, Vec3};

use crate::components::{
    Children, DirectionalLight, DrawableComponent, EditorHidden, NameComponent, Parent, PointLight,
    TransformComponent,
};

use crate::ui::{EditorAction, EntityInfo};

use super::Application;

/// Upload font atlas texture to GPU if it has been modified.
///
/// This MUST be called AFTER `generate_ui_draw_list()` (which rasterizes new glyphs
/// into the CPU atlas) and BEFORE `render_frame()` (which samples from the GPU atlas).
/// Calling it after render_frame causes a one-frame lag where the GPU has stale data.
pub fn upload_font_atlas(app: &mut Application) {
    if app.ui_context.fonts.atlas_needs_update() {
        let (width, height) = app.ui_context.fonts.atlas_size();
        let data = app.ui_context.fonts.atlas_data();

        if app.ui_context.fonts.atlas_was_resized() {
            app.renderer.create_ui_font_atlas(width, height, data);

            if let Some(bindless_slot) = app.renderer.ui_renderer.font_atlas_bindless_slot() {
                app.ui_renderer.set_font_atlas_bindless_slot(bindless_slot);
            }

            app.ui_context.fonts.clear_atlas_resized();
        } else {
            app.renderer.update_ui_font_atlas(width, height, data);
        }

        app.ui_context.fonts.mark_atlas_updated();
    }
}

/// Generate UI draw list for the current frame.
///
/// Returns a GPU-ready UIDrawList that can be submitted to the frame graph's UI pass.
/// This should be called BEFORE frame graph execution.
pub fn generate_ui_draw_list(app: &mut Application, dt: f32) -> Option<UIDrawList> {
    let scale_factor = app.scale_factor;

    // Get physical window size and convert to logical for UI layout
    let size = app.window.inner_size();
    let physical_size = Vec2::new(size.width as f32, size.height as f32);

    // UI uses logical coordinates - convert physical to logical
    let screen_size = physical_size / scale_factor;

    // Calculate stats
    let fps = if dt > 0.0 { 1.0 / dt } else { 0.0 };
    let entity_count = app.world.entity_count();

    // Collect entity info for editor UI
    let entity_info = collect_entity_info(app);

    // Set current time for UI animations (cursor blink etc.)
    app.ui_context
        .set_time(app.start_time.elapsed().as_secs_f64());

    // Render UI (editor or debug overlay based on mode)
    let scale_factor = app.scale_factor;
    let use_editor = app.use_editor_ui;

    // Store viewport texture ID before rendering (to avoid borrow issues)
    let viewport_texture_id = app.editor_ui.viewport_texture_ids[0];

    let draw_list = if use_editor {
        // Collect particle inspector data before rendering
        collect_particle_inspector_data(app);

        app.editor_ui
            .render(
                &mut app.ui_context,
                &app.preferences,
                screen_size,
                scale_factor,
                &entity_info,
                fps,
                app.frame_count,
                &mut app.background_loader,
                &app.thumbnail_texture_handles,
            )
            .clone()
    } else {
        app.debug_overlay
            .render(
                &mut app.ui_context,
                screen_size,
                scale_factor,
                fps,
                app.frame_count,
                entity_count,
            )
            .clone()
    };

    // Convert draw list to GPU format
    let ui_renderer = &mut app.ui_renderer;

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

/// Process editor actions after UI rendering.
///
/// Should be called after generate_ui_draw_list to extract any editor actions.
pub fn process_editor_actions(app: &mut Application) {
    let editor_actions = if app.use_editor_ui {
        app.editor_ui.take_actions()
    } else {
        Vec::new()
    };

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
            EditorAction::SpawnModelAtPath {
                path: _,
                position: _,
            } => {
                //TODO: Implement
            }
            EditorAction::DeleteEntity(entity_id) => {
                // Cascade delete: collect all children first, then delete in reverse order
                let mut to_delete = vec![entity_id];
                collect_children_recursive(app, entity_id, &mut to_delete);

                // Delete in reverse order (children before parents)
                for id in to_delete.into_iter().rev() {
                    app.world.destroy_entity(id);
                }
                info!("Deleted entity {:?} and its children", entity_id);
            }
            EditorAction::DuplicateEntity(entity_id) => {
                // TODO: Implement entity duplication with all components
                info!("Duplicate entity {:?} - not yet implemented", entity_id);
            }
            EditorAction::SelectEntity(entity_id) => {
                info!("Selected entity {:?}", entity_id);
            }
            EditorAction::TogglePlay => {
                info!("Toggle play mode");
            }
            EditorAction::SetTheme(theme_key) => {
                if let Some(theme) = crate::ui::Theme::by_name(&theme_key) {
                    app.editor_ui.set_theme(theme);
                    app.preferences.theme = theme_key;
                    info!("Theme changed to: {}", app.editor_ui.theme_name());
                }
            }
            EditorAction::ToggleGrid => {
                app.editor_ui.show_grid = !app.editor_ui.show_grid;
                app.preferences.show_grid = app.editor_ui.show_grid;
                // Grid visibility will be updated below after the match
            }
            EditorAction::ToggleStats => {
                app.editor_ui.show_stats = !app.editor_ui.show_stats;
                app.preferences.show_stats = app.editor_ui.show_stats;
            }
            EditorAction::SetFontScale(scale) => {
                app.editor_ui.set_font_scale(scale);
                app.preferences.font_scale = scale;
                info!("Font scale changed to: {:.0}%", scale * 100.0);
            }
            EditorAction::OpenPanel(panel) => {
                app.editor_ui.open_panel(panel);
            }
            EditorAction::ToggleParticleEmitter => {
                if let Some(entity_id) = app.editor_ui.selected_particle_emitter {
                    if let Some(emitter) = app
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
            }
            EditorAction::ResetParticleSystem => {
                // TODO: implement global particle system reset
                info!("Particle system reset requested - not yet implemented");
            }
        }
    }

    // Update OS cursor based on UI request
    use winit::window::CursorIcon;
    let cursor_icon = match app.ui_context.input.cursor {
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
    app.ui_context.input.clear_frame_state();
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
        if app.editor_ui.selected_particle_emitter == Some(entity_id) {
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
    let stats = app
        .renderer
        .particle_system
        .as_ref()
        .map(|ps| ps.get_stats());

    app.editor_ui.particle_inspector_data = ParticleInspectorData {
        emitter_entities,
        selected_emitter_config: selected_config,
        stats,
    };
}

/// Collect entity information for the editor UI in tree order.
pub fn collect_entity_info(app: &Application) -> Vec<EntityInfo> {
    // First pass: collect all entities with transforms and their relationships
    type EntityData = (String, Vec3, Vec3, Vec3, String, Vec<String>);
    let mut entity_data: HashMap<EntityId, EntityData> = HashMap::new();
    let mut parent_map: HashMap<EntityId, EntityId> = HashMap::new();
    let mut children_map: HashMap<EntityId, Vec<EntityId>> = HashMap::new();
    let mut root_entities: HashSet<EntityId> = HashSet::new();

    for entity_id in app.world.entity_ids() {
        // Skip entities marked as hidden from editor
        if app.world.get_component::<EditorHidden>(entity_id).is_some() {
            continue;
        }

        let transform = match app.world.get_component::<TransformComponent>(entity_id) {
            Some(t) => t,
            None => continue,
        };

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

        // Check for each component type and add friendly names
        if app
            .world
            .get_component::<TransformComponent>(entity_id)
            .is_some()
        {
            components.push("Transform".to_string());
        }
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
        if app.world.get_component::<Parent>(entity_id).is_some() {
            components.push("Parent".to_string());
        }
        if app.world.get_component::<Children>(entity_id).is_some() {
            components.push("Children".to_string());
        }

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

        entity_data.insert(entity_id, (name, pos, rot, scale, entity_type, components));
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
        if let Some((name, pos, rot, scale, entity_type, components)) = entity_data.get(&entity_id)
        {
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
