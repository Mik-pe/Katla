use std::sync::Arc;

use katla_agent::serialize_scene_context;
use katla_agent::{LocalAction, OpenAiProvider, StreamEvent, ToolCall};
use katla_ecs::EntityId;
use katla_ecs::agent::{Agent, AgentAction, AgentSession, Observation};
use katla_ecs::scene_tool::{ComponentRegistry, ResourceOp, SceneOp, SceneToolExecutor};
use katla_math::Vec3;
use log::warn;

pub(crate) fn run_scripted_agent(
    ops: Vec<SceneOp>,
    world: &mut katla_ecs::World,
    registry: &ComponentRegistry,
) -> Result<AgentSession, katla_ecs::scene_tool::SceneToolError> {
    let mut harness = katla_ecs::agent::AgentHarness::new();
    let mut agent = ScriptedAgent { ops, step: 0 };
    harness.run_sync_agent(&mut agent, world, registry)?;
    Ok(std::mem::take(harness.session_mut()))
}

struct ScriptedAgent {
    ops: Vec<SceneOp>,
    step: usize,
}

impl Agent for ScriptedAgent {
    fn observe(&mut self, _observation: &Observation) {}
    fn decide(&mut self) -> Option<SceneOp> {
        if self.step < self.ops.len() {
            let op = self.ops[self.step].clone();
            self.step += 1;
            Some(op)
        } else {
            None
        }
    }
    fn on_result(&mut self, _action: &AgentAction) {}
    fn name(&self) -> &str {
        "ScriptedAgent"
    }
}

pub(crate) fn get_scene_context_json(
    world: &mut katla_ecs::World,
    registry: &ComponentRegistry,
    selected_entity: Option<EntityId>,
) -> String {
    let ctx = serialize_scene_context(world, registry, selected_entity);
    serde_json::to_string_pretty(&ctx).unwrap_or_default()
}

/// Process a co-creator chat request from the user.
///
/// If the LLM is configured and enabled, delegates to the CoCreatorAgent
/// for streaming. Falls back to local pattern matching when disabled.
pub(crate) fn process_co_creator_request(app: &mut super::super::Application, text: &str) {
    if app.editor.llm_config.is_enabled() {
        submit_llm_stream_request(app, text);
    } else {
        process_local_request(app, text);
    }
}

/// Submit a user message to the LLM via the CoCreatorAgent for streaming.
fn submit_llm_stream_request(app: &mut super::super::Application, text: &str) {
    let Some(ref bridge) = app.editor.async_bridge else {
        app.editor
            .editor_ui
            .co_creator
            .add_system_message("LLM runtime is not available.");
        return;
    };

    let scene_context_json = get_scene_context_json(
        &mut app.world,
        &app.editor.component_registry,
        app.editor.editor_ui.selected_entity,
    );

    match OpenAiProvider::from_config(&app.editor.llm_config) {
        Ok(provider) => {
            app.editor.co_creator_agent.submit_request(
                bridge,
                Arc::new(provider),
                &scene_context_json,
                text,
            );
        }
        Err(e) => {
            warn!("Failed to create LLM provider: {}", e);
            app.editor
                .editor_ui
                .co_creator
                .add_system_message(&format!("LLM configuration error: {}", e));
        }
    }
}

/// Poll for streaming LLM chunks from the CoCreatorAgent. Called each frame.
pub(crate) fn poll_llm_stream(app: &mut super::super::Application) {
    if !app.editor.co_creator_agent.is_streaming() {
        return;
    }

    let events = app.editor.co_creator_agent.poll_stream();
    for event in events {
        match event {
            StreamEvent::TextDelta(delta) => {
                app.editor
                    .editor_ui
                    .co_creator
                    .append_streaming_text(&delta);
            }
            StreamEvent::Truncated => {
                app.editor
                    .editor_ui
                    .co_creator
                    .add_system_message("Response was truncated due to token limit.");
            }
            StreamEvent::ToolCall(tool_calls) => {
                let mut summaries = Vec::new();
                for tc in &tool_calls {
                    summaries.push(format_tool_call_summary(tc));
                }
                app.editor
                    .editor_ui
                    .co_creator
                    .add_system_message(&format!("Calling: {}", summaries.join(", ")));
            }
            StreamEvent::Error(msg) => {
                app.editor
                    .editor_ui
                    .co_creator
                    .add_system_message(&format!("LLM error: {}", msg));
            }
        }
    }

    // If streaming just finished, finalize and handle tool calls
    if !app.editor.co_creator_agent.is_streaming() {
        let full_text = app
            .editor
            .editor_ui
            .co_creator
            .messages
            .last()
            .map(|m| m.text.clone())
            .unwrap_or_default();
        app.editor.co_creator_agent.finalize_response(&full_text);
        app.editor.editor_ui.co_creator.finalize_streaming();

        // Execute pending tool calls and continue the conversation
        if app.editor.co_creator_agent.has_pending_tool_calls() {
            execute_and_continue_tool_calls(app);
        }
    }
}

/// Execute pending tool calls and submit results back to the LLM.
fn execute_and_continue_tool_calls(app: &mut super::super::Application) {
    let tool_calls = app.editor.co_creator_agent.take_pending_tool_calls();

    for tc in &tool_calls {
        let result = execute_tool_call(app, tc);
        let display = format_tool_call_result(tc, &result);
        app.editor.editor_ui.co_creator.add_system_message(&display);
        app.editor
            .co_creator_agent
            .add_tool_result(tc.id.clone(), result);
    }

    submit_continuation(app);
}

/// Submit a continuation request to the LLM after tool results.
fn submit_continuation(app: &mut super::super::Application) {
    let Some(ref bridge) = app.editor.async_bridge else {
        return;
    };

    let scene_context_json = get_scene_context_json(
        &mut app.world,
        &app.editor.component_registry,
        app.editor.editor_ui.selected_entity,
    );

    match OpenAiProvider::from_config(&app.editor.llm_config) {
        Ok(provider) => {
            app.editor.co_creator_agent.submit_continuation(
                bridge,
                Arc::new(provider),
                &scene_context_json,
            );
        }
        Err(e) => {
            warn!("Failed to create LLM provider for continuation: {}", e);
        }
    }
}

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
            "id": entity.to_string(),
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
];

/// Execute a single tool call against the ECS world.
fn execute_tool_call(app: &mut super::super::Application, tool_call: &ToolCall) -> String {
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

    match SceneToolExecutor::execute(op, &mut app.world, &app.editor.component_registry) {
        Ok((result, undo_group)) => {
            app.editor.agent_undo_stack.push(undo_group);
            app.editor.agent_redo_stack.clear();
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
                "entities": result.affected_entities.iter().map(|id| id.to_string()).collect::<Vec<_>>(),
            });
            if let Some(data) = result.data {
                json.as_object_mut()
                    .unwrap()
                    .insert("data".to_string(), data);
            }
            if is_hierarchy_query {
                let hierarchy = build_hierarchy_json(app);
                json.as_object_mut()
                    .unwrap()
                    .insert("hierarchy".to_string(), hierarchy);
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
    use katla_agent::co_creator::SpawnEntityArgs;
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
        let scale = args.scale.unwrap_or([1.0, 1.0, 1.0]);

        let (mesh_handle, entity_source) = match shape {
            "sphere" => (
                app.renderer.create_sphere_mesh(0.5, 32, 16),
                EntitySource::Sphere {
                    radius: 0.5,
                    segments: 32,
                    rings: 16,
                },
            ),
            "plane" => (
                app.renderer.create_plane_mesh(5.0, 5.0),
                EntitySource::Plane {
                    width: 5.0,
                    height: 5.0,
                },
            ),
            "cylinder" => (
                app.renderer.create_cylinder_mesh(1.0, 0.5, 32),
                EntitySource::Cylinder {
                    height: 1.0,
                    radius: 0.5,
                    segments: 32,
                },
            ),
            "torus" => (
                app.renderer.create_torus_mesh(0.7, 0.2, 32, 16),
                EntitySource::Torus {
                    radius: 0.7,
                    tube_radius: 0.2,
                    segments: 32,
                    tube_segments: 16,
                },
            ),
            _ => (
                app.renderer.create_cube_mesh(scale),
                EntitySource::Cube { size: scale },
            ),
        };

        let material_handle = app.default_material();
        let drawable = DrawableComponent::with_handles_and_color(
            mesh_handle,
            material_handle,
            katla_math::Color::WHITE.to_linear(),
        );
        app.gpu_resource_tracker.track_drawable(
            mesh_handle,
            material_handle,
            drawable.skeleton_handle,
        );
        app.world.add_component(entity, drawable);
        app.world.add_component(entity, entity_source);
    }
}

/// Check if the operation targets a protected entity (editor camera, gizmo, etc.).
fn check_protected_entity(op: &SceneOp, app: &super::super::Application) -> Result<(), String> {
    let target = match op {
        SceneOp::DestroyEntity { entity }
        | SceneOp::SetField { entity, .. }
        | SceneOp::DuplicateEntity { entity, .. }
        | SceneOp::AddComponent { entity, .. }
        | SceneOp::GetComponentAttributes { entity, .. }
        | SceneOp::SetParent { entity, .. } => Some(*entity),
        _ => None,
    };

    let Some(entity) = target else { return Ok(()) };

    let cam_entity = app.camera.borrow().entity;
    let gizmo_entity = app.editor.gizmo_state.entity;

    if entity == cam_entity {
        return Err(format!(
            "Error: Entity {entity} is the editor camera and cannot be modified"
        ));
    }
    if gizmo_entity == Some(entity) {
        return Err(format!(
            "Error: Entity {entity} is the editor gizmo and cannot be modified"
        ));
    }
    Ok(())
}

/// Convert a ToolCall's arguments into a SceneOp.
fn tool_call_to_scene_op(tool_call: &ToolCall) -> Result<SceneOp, String> {
    use katla_agent::co_creator::{
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
            Ok(SceneOp::SetParent {
                entity: EntityId::from_raw(args.entity_id),
                parent: args.parent_id.map(EntityId::from_raw),
            })
        }
        _ => Err(format!("Unknown tool: {}", tool_call.name)),
    }
}

fn tool_call_to_resource_op(tool_call: &ToolCall) -> Result<ResourceOp, String> {
    use katla_agent::co_creator::{
        CreateResourceArgs, ListResourcesArgs, ReadResourceArgs, WriteResourceArgs,
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
        _ => Err(format!("Unknown resource tool: {}", tool_call.name)),
    }
}

fn execute_resource_op(app: &super::super::Application, op: ResourceOp) -> String {
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
            serde_json::to_string(&json).unwrap()
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

    if let Some(parent) = file_path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            return format!("Error creating parent directory: {e}");
        }
    }

    match std::fs::write(&file_path, &body) {
        Ok(()) => {
            let json = serde_json::json!({
                "success": true,
                "message": format!("Created {path} ({} bytes)", body.len()),
                "path": path,
            });
            serde_json::to_string(&json).unwrap()
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

fn format_tool_call_summary(tc: &ToolCall) -> String {
    use katla_agent::co_creator::{DestroyEntityArgs, DuplicateEntityArgs, SpawnEntityArgs};

    match tc.name.as_str() {
        "spawn_entity" => {
            let args: SpawnEntityArgs =
                serde_json::from_value(tc.arguments.clone()).unwrap_or_default();
            match args.position {
                Some(pos) => {
                    let coords = format!("{:.1}, {:.1}, {:.1}", pos[0], pos[1], pos[2]);
                    match args.name {
                        Some(n) => format!("Spawn \"{n}\" at ({coords})"),
                        None => format!("Spawn entity at ({coords})"),
                    }
                }
                None => "Spawn entity".to_string(),
            }
        }
        "destroy_entity" => {
            let args: DestroyEntityArgs =
                serde_json::from_value(tc.arguments.clone()).unwrap_or_default();
            format!("Destroy entity {}", args.entity_id)
        }
        "set_field" => {
            let comp = tc
                .arguments
                .get("component")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let field = tc
                .arguments
                .get("field")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            format!("Set {comp}.{field}")
        }
        "query_entities" => {
            let filter = tc
                .arguments
                .get("component_filter")
                .and_then(|v| v.as_str())
                .unwrap_or("*");
            format!("Query {filter}")
        }
        "get_scene_hierarchy" => "Get scene hierarchy".to_string(),
        "duplicate_entity" => {
            let args: DuplicateEntityArgs =
                serde_json::from_value(tc.arguments.clone()).unwrap_or_default();
            format!("Duplicate entity {}", args.entity_id)
        }
        "list_available_components" => "List available components".to_string(),
        "add_component" => {
            let comp = tc
                .arguments
                .get("component")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let eid = tc
                .arguments
                .get("entity_id")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            format!("Add {comp} to entity {eid}")
        }
        "get_component_attributes" => {
            let comp = tc
                .arguments
                .get("component")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let eid = tc
                .arguments
                .get("entity_id")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            format!("Get attributes of {comp} on entity {eid}")
        }
        "set_parent" => {
            let eid = tc
                .arguments
                .get("entity_id")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let pid = tc.arguments.get("parent_id").and_then(|v| v.as_u64());
            match pid {
                Some(p) => format!("Set parent of entity {eid} to {p}"),
                None => format!("Unparent entity {eid}"),
            }
        }
        "list_resources" => {
            let path = tc
                .arguments
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("assets");
            format!("List resources in {path}")
        }
        "read_resource" => {
            let path = tc
                .arguments
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            format!("Read {path}")
        }
        "write_resource" => {
            let path = tc
                .arguments
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            format!("Write {path}")
        }
        "create_resource" => {
            let path = tc
                .arguments
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            format!("Create {path}")
        }
        _ => tc.name.clone(),
    }
}

fn format_tool_call_result(tc: &ToolCall, result: &str) -> String {
    let summary = format_tool_call_summary(tc);
    // Truncate very long results for display
    let display_result = if result.len() > 200 {
        format!("{}...", &result[..200])
    } else {
        result.to_string()
    };
    format!("{summary} -> {display_result}")
}

/// Execute local pattern-matching fallback via the CoCreatorAgent.
fn process_local_request(app: &mut super::super::Application, text: &str) {
    let response = app.editor.co_creator_agent.handle_local_request(text);
    execute_local_actions(app, &response.actions);
    app.editor
        .editor_ui
        .co_creator
        .add_assistant_message(&response.text);
}

/// Execute local actions returned by the pattern-matching handler.
fn execute_local_actions(app: &mut super::super::Application, actions: &[LocalAction]) {
    for action in actions {
        match action {
            LocalAction::SpawnCube { position, size } => {
                app.spawn_test_cube(*position, *size);
            }
            LocalAction::SpawnSphere { position, radius } => {
                app.spawn_sphere(*position, *radius, 32, 16);
            }
            LocalAction::SpawnLight { position } => {
                use crate::components::{PointLight, TransformComponent};
                let entity = app.world.create_entity();
                app.world.add_component(
                    entity,
                    TransformComponent::from_position(Vec3::new(
                        position[0],
                        position[1],
                        position[2],
                    )),
                );
                app.world
                    .add_component(entity, PointLight::new([1.0, 1.0, 0.9], 10.0, 20.0));
                app.attach_billboard_icon(
                    entity,
                    crate::components::billboard::BillboardIcon::Lightbulb,
                );
            }
            LocalAction::SpawnCubeRing { count } => {
                let n = (*count).min(10);
                for i in 0..n {
                    let angle = (i as f32 / n as f32) * std::f32::consts::TAU;
                    let x = angle.cos() * 3.0;
                    let z = angle.sin() * 3.0;
                    app.spawn_test_cube([x, 0.5, z], [1.0, 1.0, 1.0]);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::{MassComponent, NameComponent, PointLight};

    fn test_world_and_registry() -> (katla_ecs::World, ComponentRegistry) {
        (
            katla_ecs::World::new(),
            super::super::component_registry::build_editor_component_registry(),
        )
    }

    #[test]
    fn test_scripted_agent_spawn() {
        let (mut world, registry) = test_world_and_registry();
        let ops = vec![
            SceneOp::SpawnEntity {
                position: [0.0, 0.0, 0.0],
                rotation: [0.0, 0.0, 0.0],
                scale: [1.0, 1.0, 1.0],
                name: Some("Entity A".to_string()),
                primitive: None,
            },
            SceneOp::SpawnEntity {
                position: [1.0, 0.0, 0.0],
                rotation: [0.0, 0.0, 0.0],
                scale: [1.0, 1.0, 1.0],
                name: Some("Entity B".to_string()),
                primitive: None,
            },
        ];

        let session = run_scripted_agent(ops, &mut world, &registry).unwrap();
        assert_eq!(session.action_count(), 2);
        assert!(world.entity_count() >= 2);
    }

    #[test]
    fn test_scripted_agent_set_field() {
        let (mut world, registry) = test_world_and_registry();
        let entity = world.create_entity();
        world.add_component(entity, NameComponent::new("Original"));

        let ops = vec![SceneOp::SetField {
            entity,
            component: "NameComponent".to_string(),
            field: "name".to_string(),
            value: serde_json::json!("Updated"),
        }];

        let session = run_scripted_agent(ops, &mut world, &registry).unwrap();
        assert_eq!(session.action_count(), 1);

        let name = world.get_component::<NameComponent>(entity).unwrap();
        assert_eq!(name.name, "Updated");
    }

    #[test]
    fn test_scripted_agent_undo() {
        let (mut world, registry) = test_world_and_registry();

        let ops = vec![SceneOp::SpawnEntity {
            position: [0.0, 0.0, 0.0],
            rotation: [0.0, 0.0, 0.0],
            scale: [1.0, 1.0, 1.0],
            name: Some("TempEntity".to_string()),
            primitive: None,
        }];

        let mut session = run_scripted_agent(ops, &mut world, &registry).unwrap();
        assert!(world.entity_count() > 0);

        session.undo_all(&mut world).unwrap();
        assert_eq!(world.entity_count(), 0);
    }

    #[test]
    fn test_scene_context_json() {
        let (mut world, registry) = test_world_and_registry();
        let entity = world.create_entity();
        world.add_component(entity, NameComponent::new("TestEntity"));
        world.add_component(entity, PointLight::new([1.0, 0.0, 0.0], 5.0, 20.0));
        world.add_component(entity, MassComponent { mass: 2.5 });

        let json = get_scene_context_json(&mut world, &registry, Some(entity));
        assert!(json.contains("TestEntity"));
        assert!(json.contains("entity_count"));
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
}
