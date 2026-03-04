//! Editor subsystem - handles UI rendering, entity management, and editor actions.

use std::collections::{HashMap, HashSet};

use log::info;

use katla_ecs::EntityId;
use katla_gfx::renderer::UiDrawCommand;
use katla_math::{Vec2, Vec3};

use crate::components::{
    Children, DirectionalLight, DrawableComponent, EditorHidden, NameComponent, Parent, PointLight,
    TransformComponent,
};

use crate::ui::{EditorAction, EntityInfo};

use super::Application;

/// Render debug UI overlay with stats and controls.
pub fn render_debug_ui(app: &mut Application, dt: f32) {
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

    // Render UI (editor or debug overlay based on mode)
    let scale_factor = app.scale_factor;
    let use_editor = app.use_editor_ui;

    let draw_list = if use_editor {
        app.editor_ui.render(
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
    } else {
        app.debug_overlay.render(
            &mut app.ui_context,
            screen_size,
            scale_factor,
            fps,
            app.frame_count,
            entity_count,
        )
    };

    // Use UIRenderer to convert and draw list
    if !draw_list.is_empty() {
        // Convert the UI draw list to GPU format using UIRenderer
        // Use the existing UIRenderer from the UI module
        let ui_renderer = crate::ui::UIRenderer::new();
        let gpu_draw_list = ui_renderer.convert_draw_list(&draw_list);

        app.renderer.render_ui(
            &gpu_draw_list.vertex_bytes(),
            gpu_draw_list.vertex_count() as u32,
            &gpu_draw_list.indices,
            &gpu_draw_list.commands,
            [screen_size.x(), screen_size.y()],
        );
    }

    // Extract editor actions (safe now since editor_ui borrow is released)
    let editor_actions = if use_editor {
        app.editor_ui.take_actions()
    } else {
        Vec::new()
    };

    // Process editor actions
    for action in editor_actions {
        match action {
            EditorAction::SpawnModel(_model_type, _position) => {
                //TODO: Implement
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
        }
    }

    // Update font atlas texture if needed (render may have added new glyphs)
    if app.ui_context.fonts.atlas_needs_update() {
        app.ui_context.fonts.mark_atlas_updated();
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
