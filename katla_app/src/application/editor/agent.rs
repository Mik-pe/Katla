use katla_agent::ToolCall;
use katla_ecs::EntityId;
use katla_ecs::scene_tool::{ResourceOp, SceneOp, SceneToolExecutor};
use katla_gfx::primitives;

fn build_hierarchy_json(app: &super::super::Application) -> serde_json::Value {
    use crate::components::{Children, NameComponent, Parent};

    let entities: Vec<EntityId> = app.world.entity_ids().collect();

    let mut parent_map = std::collections::HashMap::new();
    for &entity in &entities {
        if let Some(parent_comp) = app.world.get_component::<Parent>(entity) {
            parent_map.insert(entity, parent_comp.parent);
        }
    }

    let mut children_map: std::collections::HashMap<EntityId, Vec<EntityId>> =
        std::collections::HashMap::new();
    for &entity in &entities {
        if let Some(children_comp) = app.world.get_component::<Children>(entity) {
            children_map.insert(entity, children_comp.children.clone());
        }
    }

    let mut name_map = std::collections::HashMap::new();
    for &entity in &entities {
        if let Some(name_comp) = app.world.get_component::<NameComponent>(entity) {
            name_map.insert(entity, name_comp.name.clone());
        }
    }

    let roots: Vec<EntityId> = entities
        .iter()
        .filter(|&&e| !parent_map.contains_key(&e))
        .copied()
        .collect();

    fn build_tree(
        entity: EntityId,
        children_map: &std::collections::HashMap<EntityId, Vec<EntityId>>,
        name_map: &std::collections::HashMap<EntityId, String>,
        depth: usize,
    ) -> serde_json::Value {
        let name = name_map
            .get(&entity)
            .cloned()
            .unwrap_or_else(|| entity.to_string());
        let children = children_map
            .get(&entity)
            .map(|c| {
                c.iter()
                    .map(|&child| build_tree(child, children_map, name_map, depth + 1))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        serde_json::json!({
            "id": entity.id().to_string(),
            "name": name,
            "depth": depth,
            "children": children,
        })
    }

    let tree: Vec<serde_json::Value> = roots
        .iter()
        .map(|&root| build_tree(root, &children_map, &name_map, 0))
        .collect();

    serde_json::json!({
        "total_count": entities.len(),
        "root_count": roots.len(),
        "tree": tree,
    })
}

pub(crate) fn cleanup_entity_hierarchy(app: &mut super::super::Application, entity: EntityId) {
    use crate::components::{Children, Parent};

    let grandparent = app.world.get_component::<Parent>(entity).map(|p| p.parent);

    if let Some(parent_id) = grandparent
        && let Some(parent_children) = app.world.get_component_mut::<Children>(parent_id)
    {
        parent_children.children.retain(|&c| c != entity);
    }

    if let Some(children_comp) = app.world.get_component::<Children>(entity) {
        let child_ids: Vec<EntityId> = children_comp.children.clone();
        let _ = children_comp;
        for child_id in child_ids {
            app.world.remove_component::<Parent>(child_id);
        }
    }
}

pub(crate) fn set_parent_components(
    app: &mut super::super::Application,
    entity: EntityId,
    new_parent: Option<EntityId>,
) {
    use crate::components::{Children, Parent};

    let old_parent_id = app.world.get_component::<Parent>(entity).map(|p| p.parent);
    if let Some(old_id) = old_parent_id
        && let Some(old_parent_children) = app.world.get_component_mut::<Children>(old_id)
    {
        old_parent_children.children.retain(|&c| c != entity);
    }
    app.world.remove_component::<Parent>(entity);

    if let Some(parent_id) = new_parent {
        let mut has_cycle = false;
        let mut visited = std::collections::HashSet::new();
        let mut current = parent_id;
        while visited.insert(current) {
            if current == entity {
                has_cycle = true;
                break;
            }
            let next = app.world.get_component::<Parent>(current).map(|p| p.parent);
            if let Some(next_id) = next {
                current = next_id;
            } else {
                break;
            }
        }
        if has_cycle {
            log::warn!("SetParent rejected: would create cycle");
            return;
        }

        app.world.add_component(entity, Parent::new(parent_id));
        if let Some(children) = app.world.get_component_mut::<Children>(parent_id) {
            if !children.children.contains(&entity) {
                children.children.push(entity);
            }
        } else {
            app.world
                .add_component(parent_id, Children::new(vec![entity]));
        }
    }
}

const RESOURCE_TOOL_NAMES: &[&str] = &[
    "list_resources",
    "read_resource",
    "write_resource",
    "create_resource",
    "generate_resource",
];

/// Execute a single tool call against the ECS world.
pub(super) fn execute_tool_call(
    app: &mut super::super::Application,
    tool_call: &ToolCall,
) -> String {
    if tool_call.name == "behavior" {
        return match serde_json::from_value(tool_call.arguments.clone())
            .map_err(|e| e.to_string())
            .and_then(|op| super::behavior::execute(app, op))
        {
            Ok(value) => value.to_string(),
            Err(error) => format!("Error: {error}"),
        };
    }
    if tool_call.name == "simulation" {
        return match serde_json::from_value(tool_call.arguments.clone())
            .map_err(|e| e.to_string())
            .and_then(|op| super::simulation::execute(app, op))
        {
            Ok(value) => value.to_string(),
            Err(error) => format!("Error: {error}"),
        };
    }
    if tool_call.name == "prefab" {
        return match serde_json::from_value(tool_call.arguments.clone())
            .map_err(|error| error.to_string())
            .and_then(|op| crate::prefab::control::execute(app, op))
        {
            Ok(result) => result.to_string(),
            Err(error) => format!("Error: {error}"),
        };
    }
    if tool_call.name == "search_assets" {
        return match serde_json::from_value(tool_call.arguments.clone())
            .map_err(|e| e.to_string())
            .and_then(|op| katla_agent::tools::search::search_assets(&app.resources.root, &op))
        {
            Ok(result) => result.to_string(),
            Err(error) => format!("Error: {error}"),
        };
    }
    if tool_call.name == "material" {
        return match serde_json::from_value(tool_call.arguments.clone())
            .map_err(|e| e.to_string())
            .and_then(|op| super::material::execute(app, op, true))
        {
            Ok(result) => result.to_string(),
            Err(error) => format!("Error: {error}"),
        };
    }
    if tool_call.name == "trigger" {
        return match serde_json::from_value::<katla_agent::events::TriggerOp<String>>(
            tool_call.arguments.clone(),
        )
        .map_err(|error| error.to_string())
        .and_then(|op| op.resolve_ids())
        .and_then(|op| crate::events::control::author(app, op))
        {
            Ok(state) => state.to_string(),
            Err(error) => format!("Error: {error}"),
        };
    }
    if tool_call.name == "animation" {
        return match serde_json::from_value(tool_call.arguments.clone())
            .map_err(|error| error.to_string())
            .and_then(|op| crate::animation::control::execute(&mut app.world, op))
        {
            Ok(state) => state.to_string(),
            Err(error) => format!("Error: {error}"),
        };
    }
    if tool_call.name == "spawn_model" {
        return execute_spawn_model(app, tool_call);
    }

    if tool_call.name == "load_scene" {
        return execute_load_scene(app, tool_call);
    }

    if tool_call.name == "save_scene" {
        return execute_save_scene(app, tool_call);
    }

    if RESOURCE_TOOL_NAMES.contains(&tool_call.name.as_str()) {
        let op = match tool_call_to_resource_op(tool_call) {
            Ok(op) => op,
            Err(e) => return format!("Error: {e}"),
        };
        return execute_resource_op(app, op);
    }

    let op = match tool_call_to_scene_op(tool_call) {
        Ok(op) => op,
        Err(e) => return format!("Error: {e}"),
    };

    execute_scene_op(app, op, tool_call)
}

pub(super) fn execute_scene_op(
    app: &mut super::super::Application,
    op: SceneOp,
    tool_call: &ToolCall,
) -> String {
    if let Some(result) = super::scene_query::execute(app, &op) {
        return result;
    }
    if let Err(msg) = check_protected_entity(&op, app) {
        return msg;
    }

    let is_hierarchy_query = matches!(op, SceneOp::GetSceneHierarchy);
    let set_parent_args = match &op {
        SceneOp::SetParent { entity, parent } => Some((*entity, *parent)),
        _ => None,
    };
    let destroy_entity_id = match &op {
        SceneOp::DestroyEntity { entity } => Some(*entity),
        _ => None,
    };
    let source_parent = match &op {
        SceneOp::DuplicateEntity { entity, .. } => app
            .world
            .get_component::<crate::components::Parent>(*entity)
            .map(|p| p.parent),
        _ => None,
    };

    if let Some(entity) = destroy_entity_id {
        cleanup_entity_hierarchy(app, entity);
    }

    let spawn_registry;
    let registry = if matches!(op, SceneOp::SpawnEntity { .. }) {
        spawn_registry = super::component_registry::build_spawn_component_registry();
        &spawn_registry
    } else {
        &app.editor.component_registry
    };
    match SceneToolExecutor::execute(op, &mut app.world, registry) {
        Ok((result, undo_group)) => {
            if !undo_group.commands.is_empty() {
                app.editor.agent_undo_stack.push(undo_group);
                app.editor.agent_redo_stack.clear();
                if app
                    .editor
                    .editor_ui
                    .selected_entity
                    .is_some_and(|id| result.affected_entities.contains(&id))
                {
                    app.editor.editor_ui.inspector_edit_entity = None;
                    app.editor.inspector_slider_was_active = false;
                    app.editor.inspector_drag_snapshot = None;
                }
            }
            if let Some((entity, parent)) = set_parent_args {
                set_parent_components(app, entity, parent);
            }
            if let Some(parent_id) = source_parent
                && let Some(&new_entity) = result.affected_entities.first()
            {
                set_parent_components(app, new_entity, Some(parent_id));
            }
            for &entity in &result.affected_entities {
                attach_spawn_visuals(app, entity, tool_call);
            }
            let mut json = serde_json::json!({
                "success": result.success,
                "message": result.message,
                "entities": result.affected_entities.iter().map(|id| id.id().to_string()).collect::<Vec<_>>(),
            });
            if let Some(data) = result.data
                && let Some(obj) = json.as_object_mut()
            {
                obj.insert("data".to_string(), data);
            }
            if is_hierarchy_query {
                let hierarchy = build_hierarchy_json(app);
                if let Some(obj) = json.as_object_mut() {
                    obj.insert("hierarchy".to_string(), hierarchy);
                }
            }
            serde_json::to_string(&json).unwrap_or(result.message)
        }
        Err(e) => format!("Error: {e}"),
    }
}

/// After a spawn, ensure the entity has a TransformComponent and a default mesh
/// so it appears in both the hierarchy panel and the 3D viewport.
fn attach_spawn_visuals(app: &mut super::super::Application, entity: EntityId, tc: &ToolCall) {
    if tc.name != "spawn_entity" {
        return;
    }

    use crate::components::{DrawableComponent, TransformComponent};
    use crate::scene::entity_source::EntitySource;
    use katla_agent::tool_args::SpawnEntityArgs;
    use katla_math::Vec3;

    let args: SpawnEntityArgs = serde_json::from_value(tc.arguments.clone()).unwrap_or_default();

    if app
        .world
        .get_component::<TransformComponent>(entity)
        .is_none()
    {
        let pos = args
            .position
            .map(|arr| Vec3::new(arr[0], arr[1], arr[2]))
            .unwrap_or(Vec3::new(0.0, 0.0, 0.0));
        app.world
            .add_component(entity, TransformComponent::from_position(pos));
    }

    if app
        .world
        .get_component::<DrawableComponent>(entity)
        .is_none()
    {
        let shape = args.shape.as_deref().unwrap_or("cube");

        let (mesh_result, entity_source) = match shape {
            "sphere" => (
                primitives::create_sphere(&mut app.renderer, 0.5, 32, 16),
                EntitySource::Sphere {
                    radius: 0.5,
                    segments: 32,
                    rings: 16,
                },
            ),
            "plane" => (
                primitives::create_plane(&mut app.renderer, 5.0, 5.0),
                EntitySource::Plane {
                    width: 5.0,
                    height: 5.0,
                },
            ),
            "cylinder" => (
                primitives::create_cylinder(&mut app.renderer, 1.0, 0.5, 32),
                EntitySource::Cylinder {
                    height: 1.0,
                    radius: 0.5,
                    segments: 32,
                },
            ),
            "torus" => (
                primitives::create_torus(&mut app.renderer, 0.7, 0.2, 32, 16),
                EntitySource::Torus {
                    radius: 0.7,
                    tube_radius: 0.2,
                    segments: 32,
                    tube_segments: 16,
                },
            ),
            _ => (
                primitives::create_cube(&mut app.renderer, [1.0; 3]),
                EntitySource::Cube { size: [1.0; 3] },
            ),
        };
        let mesh_handle = match mesh_result {
            Ok(mesh_handle) => mesh_handle,
            Err(error) => {
                log::warn!("Agent spawn visuals: mesh creation failed: {error}");
                return;
            }
        };

        let material_handle = app.default_material();
        let bounds = crate::application::spawning::local_bounds_for_source(&entity_source);
        let drawable = DrawableComponent::with_handles_and_color(
            mesh_handle,
            material_handle,
            katla_math::Color::WHITE.to_linear(),
        )
        .with_bounds(bounds);
        app.gpu_resource_tracker.track_drawable(
            mesh_handle,
            material_handle,
            drawable.skeleton_handle,
        );
        app.world.add_component(entity, drawable);
        app.world.add_component(entity, entity_source);
    }
    crate::application::editor::record_entity_gpu_handles(app, entity);
}

/// Check if the operation targets a protected entity (editor camera, gizmo, etc.).
pub(crate) fn check_protected_entity(
    op: &SceneOp,
    app: &super::super::Application,
) -> Result<(), String> {
    let target = match op {
        SceneOp::DestroyEntity { entity }
        | SceneOp::SetField { entity, .. }
        | SceneOp::DuplicateEntity { entity, .. }
        | SceneOp::AddComponent { entity, .. }
        | SceneOp::RemoveComponent { entity, .. }
        | SceneOp::GetComponentAttributes { entity, .. }
        | SceneOp::SetParent { entity, .. } => Some(*entity),
        _ => None,
    };

    let Some(entity) = target else { return Ok(()) };

    let cam_entity = app.camera.entity;

    if entity == cam_entity {
        return Err(format!(
            "Error: Entity {entity} is the editor camera and cannot be modified"
        ));
    }
    if app
        .world
        .get_component::<crate::components::EditorHidden>(entity)
        .is_some()
    {
        return Err(format!(
            "Error: Entity {entity} is editor-private and cannot be modified"
        ));
    }
    Ok(())
}

/// Convert a ToolCall's arguments into a SceneOp.
fn tool_call_to_scene_op(tool_call: &ToolCall) -> Result<SceneOp, String> {
    use katla_agent::tool_args::{
        AddComponentArgs, DestroyEntityArgs, DuplicateEntityArgs, GetComponentAttributesArgs,
        GetSceneHierarchyArgs, ListAvailableComponentsArgs, QueryEntitiesArgs, SetFieldArgs,
        SetParentArgs, SpawnEntityArgs,
    };

    match tool_call.name.as_str() {
        "spawn_entity" => {
            let args: SpawnEntityArgs = serde_json::from_value(tool_call.arguments.clone())
                .map_err(|e| format!("Invalid spawn_entity args: {e}"))?;
            Ok(SceneOp::SpawnEntity {
                position: args.position.unwrap_or([0.0, 0.0, 0.0]),
                rotation: args.rotation.unwrap_or([0.0, 0.0, 0.0]),
                scale: args.scale.unwrap_or([1.0, 1.0, 1.0]),
                name: args.name,
                primitive: args.shape,
            })
        }
        "destroy_entity" => {
            let args: DestroyEntityArgs = serde_json::from_value(tool_call.arguments.clone())
                .map_err(|e| format!("Invalid destroy_entity args: {e}"))?;
            Ok(SceneOp::DestroyEntity {
                entity: EntityId::from_raw(args.entity_id),
            })
        }
        "set_field" => {
            let args: SetFieldArgs = serde_json::from_value(tool_call.arguments.clone())
                .map_err(|e| format!("Invalid set_field args: {e}"))?;
            Ok(SceneOp::SetField {
                entity: EntityId::from_raw(args.entity_id),
                component: args.component,
                field: args.field,
                value: args.value,
            })
        }
        "query_entities" => {
            let args: QueryEntitiesArgs = serde_json::from_value(tool_call.arguments.clone())
                .map_err(|e| format!("Invalid query_entities args: {e}"))?;
            Ok(SceneOp::QueryEntities {
                component_filter: args.component_filter,
                name_filter: None,
                position: None,
                radius: None,
                limit: args.limit.map(|n| n as usize),
            })
        }
        "get_scene_hierarchy" => {
            let _args: GetSceneHierarchyArgs = serde_json::from_value(tool_call.arguments.clone())
                .map_err(|e| format!("Invalid get_scene_hierarchy args: {e}"))?;
            Ok(SceneOp::GetSceneHierarchy)
        }
        "duplicate_entity" => {
            let args: DuplicateEntityArgs = serde_json::from_value(tool_call.arguments.clone())
                .map_err(|e| format!("Invalid duplicate_entity args: {e}"))?;
            Ok(SceneOp::DuplicateEntity {
                entity: EntityId::from_raw(args.entity_id),
                position_offset: args.position_offset,
            })
        }
        "list_available_components" => {
            let _args: ListAvailableComponentsArgs =
                serde_json::from_value(tool_call.arguments.clone())
                    .map_err(|e| format!("Invalid list_available_components args: {e}"))?;
            Ok(SceneOp::ListAvailableComponents)
        }
        "add_component" => {
            let args: AddComponentArgs = serde_json::from_value(tool_call.arguments.clone())
                .map_err(|e| format!("Invalid add_component args: {e}"))?;
            Ok(SceneOp::AddComponent {
                entity: EntityId::from_raw(args.entity_id),
                component: args.component,
            })
        }
        "get_component_attributes" => {
            let args: GetComponentAttributesArgs =
                serde_json::from_value(tool_call.arguments.clone())
                    .map_err(|e| format!("Invalid get_component_attributes args: {e}"))?;
            Ok(SceneOp::GetComponentAttributes {
                entity: EntityId::from_raw(args.entity_id),
                component: args.component,
            })
        }
        "set_parent" => {
            let args: SetParentArgs = serde_json::from_value(tool_call.arguments.clone())
                .map_err(|e| format!("Invalid set_parent args: {e}"))?;
            let resolve = |value: &str| {
                value
                    .parse::<u64>()
                    .map(EntityId::from_raw)
                    .map_err(|_| "Expected a full decimal generational entity ID string".to_owned())
            };
            Ok(SceneOp::SetParent {
                entity: resolve(&args.entity_id)?,
                parent: args.parent_id.as_deref().map(resolve).transpose()?,
            })
        }
        _ => Err(format!("Unknown tool: {}", tool_call.name)),
    }
}

fn tool_call_to_resource_op(tool_call: &ToolCall) -> Result<ResourceOp, String> {
    use katla_agent::tool_args::{
        CreateResourceArgs, GenerateResourceArgs, ListResourcesArgs, ReadResourceArgs,
        WriteResourceArgs,
    };

    match tool_call.name.as_str() {
        "list_resources" => {
            let args: ListResourcesArgs = serde_json::from_value(tool_call.arguments.clone())
                .map_err(|e| format!("Invalid list_resources args: {e}"))?;
            Ok(ResourceOp::ListResources {
                path: args.path.unwrap_or_else(|| "assets".to_string()),
                filter: args.filter,
            })
        }
        "read_resource" => {
            let args: ReadResourceArgs = serde_json::from_value(tool_call.arguments.clone())
                .map_err(|e| format!("Invalid read_resource args: {e}"))?;
            Ok(ResourceOp::ReadResource { path: args.path })
        }
        "write_resource" => {
            let args: WriteResourceArgs = serde_json::from_value(tool_call.arguments.clone())
                .map_err(|e| format!("Invalid write_resource args: {e}"))?;
            Ok(ResourceOp::WriteResource {
                path: args.path,
                content: args.content,
            })
        }
        "create_resource" => {
            let args: CreateResourceArgs = serde_json::from_value(tool_call.arguments.clone())
                .map_err(|e| format!("Invalid create_resource args: {e}"))?;
            Ok(ResourceOp::CreateResource {
                path: args.path,
                template: args.template,
                content: args.content,
            })
        }
        "generate_resource" => {
            let args: GenerateResourceArgs = serde_json::from_value(tool_call.arguments.clone())
                .map_err(|e| format!("Invalid generate_resource args: {e}"))?;
            Ok(ResourceOp::GenerateResource {
                path: args.path,
                resource_type: args.resource_type,
                description: args.description,
            })
        }
        _ => Err(format!("Unknown resource tool: {}", tool_call.name)),
    }
}

fn execute_spawn_model(app: &mut super::super::Application, tool_call: &ToolCall) -> String {
    use katla_agent::tool_args::SpawnModelArgs;

    let args: SpawnModelArgs = match serde_json::from_value(tool_call.arguments.clone()) {
        Ok(a) => a,
        Err(e) => return format!("Error: invalid spawn_model args: {e}"),
    };

    let position = args.position.unwrap_or([0.0, 0.0, 0.0]);
    let default_animation = args.default_animation.as_deref();

    let path = std::path::Path::new(&args.path);
    if path.is_absolute()
        || path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return "Error: spawn_model expects a resource-relative path from search_assets".into();
    }
    let path = app.resources.root.join(path);
    match app.spawn_gltf_model(&path, position, default_animation) {
        Ok(entity) => {
            let json = serde_json::json!({
                "success": true,
                "message": format!("Model '{}' spawned as entity {}", args.path, entity),
                "entities": [entity.to_string()],
            });
            serde_json::to_string(&json)
                .unwrap_or_else(|_| format!("Model '{}' spawned successfully", args.path))
        }
        Err(e) => format!("Error: failed to spawn model '{}': {}", args.path, e),
    }
}

fn execute_load_scene(app: &mut super::super::Application, tool_call: &ToolCall) -> String {
    use katla_agent::tool_args::LoadSceneArgs;

    let args: LoadSceneArgs = match serde_json::from_value(tool_call.arguments.clone()) {
        Ok(a) => a,
        Err(e) => return format!("Error: invalid load_scene args: {e}"),
    };

    let path = std::path::Path::new(&args.path);
    match crate::scene::SceneManager::load_from_file(app, path) {
        Ok(()) => {
            app.editor.clear_entity_references();
            let json = serde_json::json!({
                "success": true,
                "message": format!("Scene loaded from '{}'", args.path),
            });
            serde_json::to_string(&json)
                .unwrap_or_else(|_| format!("Scene loaded from '{}'", args.path))
        }
        Err(e) => format!("Error: failed to load scene '{}': {}", args.path, e),
    }
}

fn execute_save_scene(app: &mut super::super::Application, tool_call: &ToolCall) -> String {
    use katla_agent::tool_args::SaveSceneArgs;

    let args: SaveSceneArgs = match serde_json::from_value(tool_call.arguments.clone()) {
        Ok(a) => a,
        Err(e) => return format!("Error: invalid save_scene args: {e}"),
    };

    let path_str = args
        .path
        .unwrap_or_else(|| crate::scene::default_scene_path().display().to_string());
    let path = std::path::Path::new(&path_str);
    match crate::scene::SceneManager::save_to_file(app, path) {
        Ok(()) => {
            let json = serde_json::json!({
                "success": true,
                "message": format!("Scene saved to '{}'", path_str),
            });
            serde_json::to_string(&json)
                .unwrap_or_else(|_| format!("Scene saved to '{}'", path_str))
        }
        Err(e) => format!("Error: failed to save scene '{}': {}", path_str, e),
    }
}

pub(super) fn execute_resource_op(app: &super::super::Application, op: ResourceOp) -> String {
    match op {
        ResourceOp::ListResources { path, filter } => {
            execute_list_resources(app, &path, filter.as_deref())
        }
        ResourceOp::ReadResource { path } => execute_read_resource(app, &path),
        ResourceOp::WriteResource { path, content } => execute_write_resource(app, &path, &content),
        ResourceOp::CreateResource {
            path,
            template,
            content,
        } => execute_create_resource(app, &path, template.as_deref(), content.as_deref()),
        ResourceOp::DeleteResource { .. } => {
            "Error: delete_resource not yet implemented".to_string()
        }
        ResourceOp::GenerateResource {
            path,
            resource_type,
            description,
        } => execute_generate_resource(app, &path, &resource_type, &description),
    }
}

fn resolve_project_root(app: &super::super::Application) -> std::path::PathBuf {
    app.resources
        .root
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
}

fn sandbox_path(
    project_root: &std::path::Path,
    relative: &str,
) -> Result<std::path::PathBuf, String> {
    if relative.contains("..") || std::path::Path::new(relative).is_absolute() {
        return Err(format!("Path traversal rejected: {relative}"));
    }
    let resolved = project_root.join(relative);
    Ok(resolved)
}

fn execute_list_resources(
    app: &super::super::Application,
    path: &str,
    filter: Option<&str>,
) -> String {
    let project_root = resolve_project_root(app);
    let dir_path = match sandbox_path(&project_root, path) {
        Ok(p) => p,
        Err(e) => return format!("Error: {e}"),
    };

    if !dir_path.exists() || !dir_path.is_dir() {
        return format!("Error: directory not found: {path}");
    }

    let mut entries = Vec::new();
    if let Err(e) = collect_entries(&dir_path, path, filter, &mut entries) {
        return format!("Error listing directory: {e}");
    }

    let json = serde_json::json!({
        "path": path,
        "count": entries.len(),
        "entries": entries,
    });
    serde_json::to_string(&json)
        .unwrap_or_else(|_| "Error: failed to serialize results".to_string())
}

fn collect_entries(
    dir: &std::path::Path,
    prefix: &str,
    filter: Option<&str>,
    out: &mut Vec<serde_json::Value>,
) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        let relative = if prefix.is_empty() || prefix == "." {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };
        let metadata = entry.metadata()?;
        if metadata.is_dir() {
            collect_entries(&entry.path(), &relative, filter, out)?;
        } else {
            if let Some(ext) = filter {
                let matches = entry.path().extension().map(|e| e == ext).unwrap_or(false);
                if !matches {
                    continue;
                }
            }
            out.push(serde_json::json!({
                "name": name,
                "path": relative,
                "size": metadata.len(),
            }));
        }
    }
    Ok(())
}

fn execute_read_resource(app: &super::super::Application, path: &str) -> String {
    let project_root = resolve_project_root(app);
    let file_path = match sandbox_path(&project_root, path) {
        Ok(p) => p,
        Err(e) => return format!("Error: {e}"),
    };

    if !file_path.exists() {
        return format!("Error: file not found: {path}");
    }

    match std::fs::read_to_string(&file_path) {
        Ok(content) => {
            let json = serde_json::json!({
                "path": path,
                "content": content,
            });
            serde_json::to_string(&json).unwrap_or(content)
        }
        Err(e) => format!("Error reading file: {e}"),
    }
}

fn execute_write_resource(app: &super::super::Application, path: &str, content: &str) -> String {
    let project_root = resolve_project_root(app);
    let file_path = match sandbox_path(&project_root, path) {
        Ok(p) => p,
        Err(e) => return format!("Error: {e}"),
    };

    if !file_path.exists() {
        return format!("Error: file not found: {path} (use create_resource to create new files)");
    }

    match std::fs::write(&file_path, content) {
        Ok(()) => {
            let json = serde_json::json!({
                "success": true,
                "message": format!("Wrote {} bytes to {path}", content.len()),
                "path": path,
            });
            serde_json::to_string(&json)
                .unwrap_or_else(|_| format!("Wrote {} bytes to {path}", content.len()))
        }
        Err(e) => format!("Error writing file: {e}"),
    }
}

fn execute_create_resource(
    app: &super::super::Application,
    path: &str,
    template: Option<&str>,
    content: Option<&str>,
) -> String {
    let project_root = resolve_project_root(app);
    let file_path = match sandbox_path(&project_root, path) {
        Ok(p) => p,
        Err(e) => return format!("Error: {e}"),
    };

    if file_path.exists() {
        return format!("Error: file already exists: {path} (use write_resource to modify)");
    }

    let body = match template {
        Some(tpl) => generate_template_content(tpl),
        None => content.unwrap_or("").to_string(),
    };

    if let Some(parent) = file_path.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        return format!("Error creating parent directory: {e}");
    }

    match std::fs::write(&file_path, &body) {
        Ok(()) => {
            let json = serde_json::json!({
                "success": true,
                "message": format!("Created {path} ({} bytes)", body.len()),
                "path": path,
            });
            serde_json::to_string(&json)
                .unwrap_or_else(|_| format!("Created {path} ({} bytes)", body.len()))
        }
        Err(e) => format!("Error creating file: {e}"),
    }
}

fn generate_template_content(template: &str) -> String {
    match template {
        "scene" => serde_json::json!({
            "version": 1,
            "entities": []
        })
        .to_string(),
        "material" => serde_json::json!({
            "version": 1,
            "shader": "pbr",
            "properties": {}
        })
        .to_string(),
        "particle_system" => serde_json::json!({
            "version": 1,
            "emitter": {
                "rate": 100.0,
                "lifetime": [0.5, 2.0],
                "velocity": [0.0, 1.0, 0.0],
            }
        })
        .to_string(),
        _ => format!("{{ \"template\": \"{template}\" }}"),
    }
}

fn execute_generate_resource(
    app: &super::super::Application,
    path: &str,
    resource_type: &str,
    description: &str,
) -> String {
    let project_root = resolve_project_root(app);
    let file_path = match sandbox_path(&project_root, path) {
        Ok(p) => p,
        Err(e) => return format!("Error: {e}"),
    };

    if file_path.exists() {
        return format!("Error: file already exists: {path} (use write_resource to modify)");
    }

    let body = generate_resource_content(resource_type, description);

    if let Some(parent) = file_path.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        return format!("Error creating parent directory: {e}");
    }

    match std::fs::write(&file_path, &body) {
        Ok(()) => {
            let json = serde_json::json!({
                "success": true,
                "message": format!("Generated {path} as {resource_type} ({} bytes)", body.len()),
                "path": path,
                "resource_type": resource_type,
            });
            serde_json::to_string(&json)
                .unwrap_or_else(|_| format!("Generated {path} as {resource_type}"))
        }
        Err(e) => format!("Error creating file: {e}"),
    }
}

fn generate_resource_content(resource_type: &str, description: &str) -> String {
    match resource_type {
        "particle_system" => generate_particle_system(description),
        "material" => generate_material(description),
        "scene" => generate_scene(description),
        _ => serde_json::json!({
            "version": 1,
            "description": description
        })
        .to_string(),
    }
}

fn generate_particle_system(description: &str) -> String {
    let desc = description.to_lowercase();

    let (
        rate,
        lifetime_min,
        lifetime_max,
        vel_x,
        vel_y,
        vel_z,
        color_start,
        color_end,
        size_start,
        size_end,
    ) = if desc.contains("fire") || desc.contains("flame") || desc.contains("campfire") {
        (
            150.0,
            0.3,
            1.5,
            0.0,
            3.0,
            0.0,
            [1.0, 0.3, 0.0],
            [1.0, 0.8, 0.0],
            0.15,
            0.02,
        )
    } else if desc.contains("rain") {
        (
            500.0,
            0.3,
            0.8,
            0.0,
            -8.0,
            0.0,
            [0.5, 0.6, 0.8],
            [0.3, 0.4, 0.7],
            0.02,
            0.01,
        )
    } else if desc.contains("snow") {
        (
            80.0,
            2.0,
            5.0,
            0.1,
            -1.0,
            0.1,
            [0.95, 0.95, 1.0],
            [0.8, 0.8, 0.9],
            0.05,
            0.03,
        )
    } else if desc.contains("spark") || desc.contains("sparkle") || desc.contains("firework") {
        (
            200.0,
            0.2,
            0.8,
            0.0,
            2.0,
            0.0,
            [1.0, 1.0, 0.5],
            [1.0, 0.5, 0.0],
            0.04,
            0.01,
        )
    } else if desc.contains("smoke") || desc.contains("steam") {
        (
            40.0,
            1.0,
            4.0,
            0.0,
            1.5,
            0.0,
            [0.5, 0.5, 0.5],
            [0.3, 0.3, 0.3],
            0.3,
            0.8,
        )
    } else if desc.contains("dust") || desc.contains("sand") {
        (
            60.0,
            1.0,
            3.0,
            0.2,
            0.3,
            0.2,
            [0.8, 0.7, 0.5],
            [0.6, 0.5, 0.3],
            0.03,
            0.06,
        )
    } else if desc.contains("magic") || desc.contains("enchant") || desc.contains("mystic") {
        (
            120.0,
            0.5,
            2.0,
            0.0,
            2.0,
            0.0,
            [0.5, 0.0, 1.0],
            [0.0, 0.5, 1.0],
            0.08,
            0.02,
        )
    } else if desc.contains("explosion") || desc.contains("burst") {
        (
            300.0,
            0.1,
            0.6,
            0.0,
            0.0,
            0.0,
            [1.0, 0.6, 0.0],
            [0.5, 0.1, 0.0],
            0.2,
            0.02,
        )
    } else {
        (
            100.0,
            0.5,
            2.0,
            0.0,
            1.0,
            0.0,
            [1.0, 1.0, 1.0],
            [0.5, 0.5, 0.5],
            0.1,
            0.02,
        )
    };

    serde_json::json!({
        "version": 1,
        "emitter": {
            "rate": rate,
            "lifetime": [lifetime_min, lifetime_max],
            "velocity": [vel_x, vel_y, vel_z],
        },
        "appearance": {
            "color_start": color_start,
            "color_end": color_end,
            "size_start": size_start,
            "size_end": size_end,
        }
    })
    .to_string()
}

fn generate_material(description: &str) -> String {
    let desc = description.to_lowercase();

    let (base_color, metallic, roughness, emissive) =
        if desc.contains("gold") || desc.contains("brass") {
            ([1.0, 0.84, 0.0], 0.9, 0.2, [0.0, 0.0, 0.0])
        } else if desc.contains("metal") || desc.contains("steel") || desc.contains("iron") {
            ([0.7, 0.7, 0.75], 1.0, 0.3, [0.0, 0.0, 0.0])
        } else if desc.contains("chrome") || desc.contains("mirror") {
            ([0.9, 0.9, 0.9], 1.0, 0.05, [0.0, 0.0, 0.0])
        } else if desc.contains("rubber") || desc.contains("plastic") {
            ([0.3, 0.3, 0.3], 0.0, 0.9, [0.0, 0.0, 0.0])
        } else if desc.contains("wood") {
            ([0.6, 0.4, 0.2], 0.0, 0.8, [0.0, 0.0, 0.0])
        } else if desc.contains("glass") || desc.contains("crystal") {
            ([0.9, 0.95, 1.0], 0.1, 0.1, [0.1, 0.1, 0.15])
        } else if desc.contains("neon")
            || desc.contains("glow")
            || desc.contains("emissive")
            || desc.contains("luminous")
        {
            (
                [0.2, 0.2, 0.2],
                0.0,
                0.5,
                if desc.contains("red") {
                    [2.0, 0.0, 0.0]
                } else if desc.contains("green") {
                    [0.0, 2.0, 0.0]
                } else if desc.contains("blue") {
                    [0.0, 0.0, 2.0]
                } else if desc.contains("pink") || desc.contains("magenta") {
                    [2.0, 0.0, 1.0]
                } else {
                    [0.0, 2.0, 1.0]
                },
            )
        } else if desc.contains("red") {
            ([0.8, 0.1, 0.1], 0.0, 0.5, [0.0, 0.0, 0.0])
        } else if desc.contains("blue") {
            ([0.1, 0.2, 0.8], 0.0, 0.5, [0.0, 0.0, 0.0])
        } else if desc.contains("green") {
            ([0.1, 0.6, 0.1], 0.0, 0.5, [0.0, 0.0, 0.0])
        } else if desc.contains("stone") || desc.contains("concrete") || desc.contains("rock") {
            ([0.5, 0.5, 0.5], 0.0, 0.95, [0.0, 0.0, 0.0])
        } else {
            ([0.8, 0.8, 0.8], 0.0, 0.5, [0.0, 0.0, 0.0])
        };

    serde_json::json!({
        "version": 1,
        "shader": "pbr",
        "properties": {
            "base_color": base_color,
            "metallic": metallic,
            "roughness": roughness,
            "emissive": emissive,
        }
    })
    .to_string()
}

fn generate_scene(description: &str) -> String {
    let desc = description.to_lowercase();

    let (ambient_color, ambient_intensity) = if desc.contains("night") || desc.contains("dark") {
        ([0.05, 0.05, 0.1], 0.1)
    } else if desc.contains("sunset") || desc.contains("dawn") {
        ([0.8, 0.4, 0.2], 0.4)
    } else if desc.contains("indoor") || desc.contains("interior") {
        ([0.9, 0.85, 0.7], 0.3)
    } else {
        ([0.9, 0.95, 1.0], 0.5)
    };

    serde_json::json!({
        "version": 1,
        "settings": {
            "ambient_color": ambient_color,
            "ambient_intensity": ambient_intensity,
        },
        "entities": []
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_teen_room_fixture_placement_clearances_and_scene_tool_undo() {
        use crate::components::{NameComponent, TransformComponent};
        use crate::scene::{EntitySource, Scene};
        use katla_ecs::World;
        let base: Scene =
            ron::from_str(include_str!("../../../../assets/scenes/shared-room.katla")).unwrap();
        let furnished: Scene = ron::from_str(include_str!(
            "../../../../assets/scenes/teen-room-blockout.katla"
        ))
        .unwrap();
        let plan: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../assets/scenes/teen-room-plan.json"
        ))
        .unwrap();
        let placements = plan["placements"].as_array().unwrap();
        assert_eq!(
            furnished.entities.len(),
            base.entities.len() + placements.len()
        );
        assert_eq!(
            serde_json::to_value(&furnished.entities[..base.entities.len()]).unwrap(),
            serde_json::to_value(&base.entities).unwrap()
        );
        let registry = super::super::component_registry::build_spawn_component_registry();
        let mut world = World::new();
        let original = world.spawn((
            NameComponent::new("Existing room"),
            TransformComponent::default(),
        ));
        let mut undo = Vec::new();
        for (placement, descriptor) in placements
            .iter()
            .zip(furnished.entities.iter().skip(base.entities.len()))
        {
            let tool = ToolCall {
                id: "fixture".into(),
                name: "spawn_entity".into(),
                arguments: placement.clone(),
            };
            let op = tool_call_to_scene_op(&tool).unwrap();
            let (result, group) = SceneToolExecutor::execute(op, &mut world, &registry).unwrap();
            let id = result.affected_entities[0];
            let transform = &world
                .get_component::<TransformComponent>(id)
                .unwrap()
                .transform;
            assert_eq!(transform.position.to_array(), descriptor.transform.position);
            assert_eq!(transform.scale.to_array(), descriptor.transform.scale);
            assert_eq!(
                world.get_component::<NameComponent>(id).unwrap().name,
                descriptor.name.as_ref().unwrap().as_str()
            );
            assert_eq!(descriptor.source, EntitySource::Cube { size: [1.0; 3] });
            let bounds = crate::application::spawning::local_bounds_for_source(&descriptor.source)
                .transform(&transform.make_mat4());
            let low = bounds.min();
            let high = bounds.max();
            assert!(low.x() >= -3.9 && high.x() <= 3.9);
            assert!(low.z() >= -6.8 && high.z() <= 2.9);
            assert!(low.y() >= -0.00001 && high.y() <= 3.0);
            if high.y() > 0.2 {
                assert!(
                    high.x() <= -1.0 || low.x() >= 1.0,
                    "Central passage blocked by {}",
                    descriptor.name.as_ref().unwrap()
                );
                for x in [-2.2, 2.2] {
                    assert!(
                        high.x() <= x - 0.8
                            || low.x() >= x + 0.8
                            || low.z() >= -5.0
                            || high.z() <= -7.0,
                        "Door approach blocked"
                    );
                }
            }
            undo.push(group);
        }
        for group in undo.iter_mut().rev() {
            group.undo_all(&mut world).unwrap();
        }
        assert_eq!(world.entity_ids().collect::<Vec<_>>(), vec![original]);
        assert_eq!(
            world.get_component::<NameComponent>(original).unwrap().name,
            "Existing room"
        );
    }

    #[test]
    fn test_tool_call_to_scene_op_spawn() {
        let tc = ToolCall {
            id: "call_1".to_string(),
            name: "spawn_entity".to_string(),
            arguments: serde_json::json!({
                "position": [1.0, 2.0, 3.0],
                "name": "TestCube"
            }),
        };
        let op = tool_call_to_scene_op(&tc).unwrap();
        match op {
            SceneOp::SpawnEntity { position, name, .. } => {
                assert_eq!(position, [1.0, 2.0, 3.0]);
                assert_eq!(name, Some("TestCube".to_string()));
            }
            _ => panic!("Expected SpawnEntity"),
        }
    }

    #[test]
    fn test_tool_call_to_scene_op_unknown() {
        let tc = ToolCall {
            id: "call_x".to_string(),
            name: "unknown_tool".to_string(),
            arguments: serde_json::json!({}),
        };
        let result = tool_call_to_scene_op(&tc);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown tool"));
    }
    #[test]
    fn test_co_creator_reparents_prefab_node_ids_without_losing_generational_bits() {
        let call = ToolCall {
            id: "parent".into(),
            name: "set_parent".into(),
            arguments: serde_json::json!({"entity_id":u64::MAX.to_string(),"parent_id":(u64::MAX-1).to_string()}),
        };
        assert!(
            matches!(tool_call_to_scene_op(&call).unwrap(), SceneOp::SetParent { entity, parent:Some(parent) } if entity.id() == u64::MAX && parent.id() == u64::MAX-1)
        );
        let mut detach = call.clone();
        detach.arguments["parent_id"] = serde_json::Value::Null;
        assert!(matches!(
            tool_call_to_scene_op(&detach).unwrap(),
            SceneOp::SetParent { parent: None, .. }
        ));
        detach.arguments["entity_id"] = serde_json::json!(u64::MAX);
        assert!(tool_call_to_scene_op(&detach).is_err());
    }
}
