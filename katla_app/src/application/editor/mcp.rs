use katla_agent::{McpBridge, McpOpKind, McpResponse, start_mcp_server_thread};
use katla_ecs::EntityId;
use katla_ecs::scene_tool::{ComponentRegistry, ResourceOp, SceneOp, SceneToolExecutor};
use log::info;

use crate::components::{Children, Parent};

pub(crate) struct ProtectedEntities {
    pub(crate) camera_entity: EntityId,
    pub(crate) gizmo_entity: Option<EntityId>,
}

pub(crate) struct McpState {
    bridge: McpBridge,
}

impl McpState {
    pub(crate) fn new() -> Self {
        let (server, bridge, shutdown_rx) = McpBridge::new();
        start_mcp_server_thread(server, shutdown_rx);
        info!("MCP server bridge initialized");
        Self { bridge }
    }

    pub(crate) fn poll(
        &mut self,
        app: &mut crate::application::Application,
        registry: &ComponentRegistry,
        protected: &ProtectedEntities,
    ) {
        let requests = self.bridge.poll_requests();
        for req in requests {
            let response = match req.op.into_op() {
                McpOpKind::Scene(scene_op) => {
                    if let Err(msg) = check_protected_entity(&scene_op, protected) {
                        McpResponse { result: Err(msg) }
                    } else {
                        if let SceneOp::DestroyEntity { entity } = &scene_op {
                            cleanup_entity_hierarchy_world(&mut app.world, *entity);
                        }
                        execute_scene_op(scene_op, &mut app.world, registry)
                    }
                }
                McpOpKind::Resource(resource_op) => execute_resource_op(resource_op),
                McpOpKind::LoadScene { path } => execute_load_scene(app, &path),
                McpOpKind::SaveScene { path } => execute_save_scene(app, path.as_deref()),
            };
            let _ = req.response_tx.send(response);
        }
    }
}

fn execute_load_scene(app: &mut crate::application::Application, path: &str) -> McpResponse {
    let file_path = std::path::Path::new(path);
    match crate::scene::SceneManager::load_from_file(app, file_path) {
        Ok(()) => {
            app.editor.clear_entity_references();
            McpResponse {
                result: Ok(serde_json::json!({
                    "success": true,
                    "message": format!("Scene loaded from '{path}'"),
                })),
            }
        }
        Err(e) => McpResponse {
            result: Err(format!("Failed to load scene '{path}': {e}")),
        },
    }
}

fn execute_save_scene(
    app: &mut crate::application::Application,
    path: Option<&str>,
) -> McpResponse {
    let path_str = path
        .map(String::from)
        .unwrap_or_else(|| crate::scene::default_scene_path().display().to_string());
    let file_path = std::path::Path::new(&path_str);
    match crate::scene::SceneManager::save_to_file(app, file_path) {
        Ok(()) => McpResponse {
            result: Ok(serde_json::json!({
                "success": true,
                "message": format!("Scene saved to '{path_str}'"),
            })),
        },
        Err(e) => McpResponse {
            result: Err(format!("Failed to save scene '{path_str}': {e}")),
        },
    }
}

fn execute_scene_op(
    scene_op: SceneOp,
    world: &mut katla_ecs::World,
    registry: &ComponentRegistry,
) -> McpResponse {
    let result = SceneToolExecutor::execute(scene_op, world, registry);
    match result {
        Ok((tool_result, _undo_group)) => {
            let mut json = serde_json::json!({
                "success": tool_result.success,
                "message": tool_result.message,
                "affected_entities": tool_result.affected_entities.iter().map(|id| id.id()).collect::<Vec<u64>>(),
            });
            if let Some(data) = tool_result.data {
                if let Some(map) = json.as_object_mut() {
                    map.insert("data".to_string(), data);
                }
            }
            McpResponse { result: Ok(json) }
        }
        Err(e) => McpResponse {
            result: Err(format!("{:?}", e)),
        },
    }
}

fn execute_resource_op(op: ResourceOp) -> McpResponse {
    McpResponse {
        result: Err(format!("Resource operation not yet implemented: {:?}", op)),
    }
}

fn check_protected_entity(op: &SceneOp, protected: &ProtectedEntities) -> Result<(), String> {
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

    if entity == protected.camera_entity {
        return Err(format!(
            "Entity {entity} is the editor camera and cannot be modified"
        ));
    }
    if protected.gizmo_entity == Some(entity) {
        return Err(format!(
            "Entity {entity} is the editor gizmo and cannot be modified"
        ));
    }
    Ok(())
}

fn cleanup_entity_hierarchy_world(world: &mut katla_ecs::World, entity: EntityId) {
    let grandparent = world.get_component::<Parent>(entity).map(|p| p.parent);

    if let Some(parent_id) = grandparent
        && let Some(parent_children) = world.get_component_mut::<Children>(parent_id)
    {
        parent_children.children.retain(|&c| c != entity);
    }

    if let Some(children_comp) = world.get_component::<Children>(entity) {
        let child_ids: Vec<EntityId> = children_comp.children.clone();
        let _ = children_comp;
        for child_id in child_ids {
            world.remove_component::<Parent>(child_id);
        }
    }
}
