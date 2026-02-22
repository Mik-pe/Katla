//! Editor subsystem - handles UI rendering, entity management, and editor actions.

use std::collections::{HashMap, HashSet};

use log::info;

use katla_ecs::EntityId;
use katla_math::{Vec2, Vec3};

use crate::components::{
    Children, DirectionalLight, DrawableComponent, EditorHidden, NameComponent, Parent,
    ParticleEmitter, PointLight, TransformComponent,
};
use crate::rendering::MeshBuilder;
use crate::ui::{EditorAction, EntityInfo, SpawnableModel};

use super::Application;

/// Render debug UI overlay with stats and controls.
pub fn render_debug_ui(app: &mut Application, dt: f32) {
    let scale_factor = app.scale_factor;

    // Get physical window size and convert to logical for UI layout
    let physical_size = if let Some(ref window) = app.window {
        let size = window.inner_size();
        Vec2::new(size.width as f32, size.height as f32)
    } else {
        Vec2::new(1920.0, 1080.0)
    };

    // UI uses logical coordinates - convert physical to logical
    let screen_size = physical_size / scale_factor;

    // Calculate stats
    let fps = if dt > 0.0 { 1.0 / dt } else { 0.0 };
    let entity_count = app.world.entity_count();

    // Collect entity info for editor UI
    let entity_info = collect_entity_info(app);

    // Render UI (editor or debug overlay based on mode)
    // We extract the vertices immediately to release the borrow on editor_ui
    let scale_factor = app.scale_factor;
    let (vertices, indices, commands, use_editor) = if app.use_editor_ui {
        let draw_list = app.editor_ui.render(
            &mut app.ui_context,
            screen_size,
            scale_factor,
            &entity_info,
            fps,
            app.frame_count,
            &mut app.background_loader,
            &app.thumbnail_texture_ids,
        );
        (
            draw_list.vertices.clone(),
            draw_list.indices.clone(),
            draw_list.commands.clone(),
            true,
        )
    } else {
        let draw_list = app.debug_overlay.render(
            &mut app.ui_context,
            screen_size,
            scale_factor,
            fps,
            app.frame_count,
            entity_count,
        );
        (
            draw_list.vertices.clone(),
            draw_list.indices.clone(),
            draw_list.commands.clone(),
            false,
        )
    };

    // Extract editor actions (safe now since editor_ui borrow is released)
    let editor_actions = if use_editor {
        app.editor_ui.take_actions()
    } else {
        Vec::new()
    };

    // Process editor actions
    for action in editor_actions {
        match action {
            EditorAction::SpawnModel(model_type, position) => {
                spawn_model(app, model_type, position);
            }
            EditorAction::SpawnModelAtPath { path, position } => {
                // Load model from file path
                spawn_model_from_path(app, path, position);
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
            EditorAction::MoveEntity(_entity_id, _position) => {
                // TODO: Implement entity moving
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
        }

        // Update grid visibility by rebuilding render graph
        // Grid toggle requires render graph rebuild since passes own their pipelines
        if let (Some(ref mut renderer), Some(ref sky_pipeline), Some(ref grid_pipeline), Some(ref ui_pipeline)) =
            (&mut app.renderer, &app.sky_pipeline, &app.grid_pipeline, &app.ui_pipeline)
        {
            let grid_to_use = if app.editor_ui.show_grid {
                Some(grid_pipeline.clone())
            } else {
                None
            };
            super::renderer::render_graph::build_render_graph(
                renderer,
                Some(sky_pipeline.clone()),
                grid_to_use,
            );
        }
    }

    // Pass UI data to renderer if we have data and a renderer
    if !vertices.is_empty() {
        use crate::rendering::ui_material::UiShaderVertex;

        // Convert vertices to shader format (logical coordinates)
        // NDC transform happens in the shader using uniform buffer with logical screen size
        let shader_vertices: Vec<UiShaderVertex> = vertices
            .iter()
            .map(|v| {
                UiShaderVertex::new(
                    [v.position.x(), v.position.y()],  // Logical coordinates
                    [v.uv.x(), v.uv.y()],
                    [v.color.r, v.color.g, v.color.b, v.color.a],
                )
            })
            .collect();

        // Convert vertices to raw bytes
        let vertex_bytes = unsafe {
            std::slice::from_raw_parts(
                shader_vertices.as_ptr() as *const u8,
                shader_vertices.len() * std::mem::size_of::<UiShaderVertex>(),
            )
        }
        .to_vec();

        // Convert indices to raw bytes
        let index_bytes = unsafe {
            std::slice::from_raw_parts(
                indices.as_ptr() as *const u8,
                indices.len() * std::mem::size_of::<u32>(),
            )
        }
        .to_vec();

        // Convert commands to renderer format
        // Scale clip rects from logical to physical for Vulkan scissor testing
        let ui_commands: Vec<crate::rendering::UiDrawCommand> = commands
            .iter()
            .map(|cmd| crate::rendering::UiDrawCommand {
                index_offset: cmd.index_offset,
                index_count: cmd.index_count,
                clip_rect: [
                    cmd.clip_rect.min.x() * scale_factor,
                    cmd.clip_rect.min.y() * scale_factor,
                    cmd.clip_rect.width() * scale_factor,
                    cmd.clip_rect.height() * scale_factor,
                ],
                texture_id: cmd.texture.0, // Pass texture ID for dynamic binding
            })
            .collect();

        // Pass to renderer
        // Use physical size for viewport/scissor (Vulkan operates in physical pixels)
        // But use logical size for UI uniform (vertices are in logical coords)
        if let Some(ref ui_renderer) = app.ui_renderer {
            // Update screen size uniform for shader NDC transform (logical size!)
            ui_renderer.update_screen_size(screen_size.x(), screen_size.y());

            // Store UI data for render graph to pick up
            *app.ui_draw_data.borrow_mut() = Some(crate::rendering::UiDrawData {
                vertex_data: vertex_bytes,
                index_data: index_bytes,
                screen_size: [physical_size.x(), physical_size.y()],
                commands: ui_commands,
            });
        }
    }

    // Update font atlas texture if needed (render may have added new glyphs)
    if app.ui_context.fonts.atlas_needs_update() {
        if let Some(ref mut ui_renderer) = app.ui_renderer {
            // Check if atlas was resized
            if app.ui_context.fonts.atlas_was_resized() {
                let (new_width, new_height) = app.ui_context.fonts.atlas_size();
                let atlas_data = app.ui_context.fonts.atlas_data().to_vec();
                ui_renderer.resize_font_atlas(new_width, new_height, &atlas_data);
                app.ui_context.fonts.clear_atlas_resized();
            } else {
                let atlas_data = app.ui_context.fonts.atlas_data().to_vec();
                ui_renderer.update_font_atlas(&atlas_data);
            }
        }
        app.ui_context.fonts.mark_atlas_updated();
    }

    // Update OS cursor based on UI request
    if let Some(ref window) = app.window {
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
        window.set_cursor(cursor_icon);
    }

    // Clear input state for next frame
    app.ui_context.input.clear_frame_state();
}

/// Collect entity information for the editor UI in tree order.
pub fn collect_entity_info(app: &Application) -> Vec<EntityInfo> {
    use crate::animation::Skeleton;
    

    // First pass: collect all entities with transforms and their relationships
    // EntityData: (name, position, rotation, scale, entity_type, components)
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
            .get_component::<ParticleEmitter>(entity_id)
            .is_some()
        {
            components.push("ParticleEmitter".to_string());
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
        if app.world.get_component::<Skeleton>(entity_id).is_some() {
            components.push("Skeleton".to_string());
        }

        // Determine entity type based on primary component
        let entity_type = if app
            .world
            .get_component::<ParticleEmitter>(entity_id)
            .is_some()
        {
            "Particle Emitter".to_string()
        } else if app
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

/// Spawn a model from the editor UI.
pub fn spawn_model(app: &mut Application, model_type: SpawnableModel, position: Vec3) {
    let context = match &app.renderer {
        Some(r) => r.context.clone(),
        None => return,
    };

    // Create mesh using MeshBuilder (creates entity internally)
    let builder = MeshBuilder::new(context.clone()).position(position);

    let spawned_id = match model_type {
        SpawnableModel::Fox => {
            info!("Spawning Fox at {:?} (using cube placeholder)", position);
            builder
                .cube()
                .build(&mut app.world, app.renderer.as_mut().unwrap())
        }
        SpawnableModel::Cube => builder
            .cube()
            .build(&mut app.world, app.renderer.as_mut().unwrap()),
        SpawnableModel::Sphere => builder
            .sphere()
            .build(&mut app.world, app.renderer.as_mut().unwrap()),
        SpawnableModel::Cylinder => builder
            .cylinder()
            .build(&mut app.world, app.renderer.as_mut().unwrap()),
        SpawnableModel::Plane => builder
            .plane()
            .build(&mut app.world, app.renderer.as_mut().unwrap()),
        SpawnableModel::Torus => builder
            .torus()
            .build(&mut app.world, app.renderer.as_mut().unwrap()),
    };

    // Update the name component with a more descriptive name
    let name = format!("{}_{}", model_type.name(), spawned_id.id());
    if let Some(name_comp) = app.world.get_component_mut::<NameComponent>(spawned_id) {
        name_comp.name = name;
    }

    info!(
        "Spawned {} (entity {}) at {:?}",
        model_type.name(),
        spawned_id.id(),
        position
    );
}

/// Spawn a model from a file path (e.g., .glb file).
pub fn spawn_model_from_path(app: &mut Application, path: std::path::PathBuf, position: Vec3) {
    use crate::entities::Model;
    use std::rc::Rc;

    let context = match &app.renderer {
        Some(r) => r.context.clone(),
        None => return,
    };

    // Clone material registry Rc before mutable borrow
    let material_registry = Rc::clone(&app.renderer.as_ref().unwrap().material_registry);

    // Load the GLTF model using the file cache
    let model = app.gltf_cache.read(path.clone());

    // Create transform for the model
    let transform = katla_math::Transform::new_from_position(position);

    // Create entity with the loaded model using the smart unified importer
    let entity = Model::from_gltf(
        &mut app.world,
        model.clone(),
        context,
        app.renderer.as_mut(),
        transform,
        &material_registry,
    );

    // Update name with filename
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Model")
        .to_string();
    if let Some(name_comp) = app.world.get_component_mut::<NameComponent>(entity.entity) {
        name_comp.name = format!("{}_{}", name, entity.entity.id());
    }

    info!(
        "Spawned model from {:?} (entity {}) at {:?}",
        path,
        entity.entity.id(),
        position
    );
}
