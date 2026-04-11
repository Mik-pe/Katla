use katla_agent::{McpBridge, McpResponse, start_mcp_server_thread};
use katla_ecs::EntityId;
use katla_ecs::scene_tool::{ComponentRegistry, SceneOp, SceneToolExecutor};
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
        world: &mut katla_ecs::World,
        registry: &ComponentRegistry,
        protected: &ProtectedEntities,
    ) {
        let requests = self.bridge.poll_requests();
        for req in requests {
            let scene_op = req.op.to_scene_op();

            if let Err(msg) = check_protected_entity(&scene_op, protected) {
                let _ = req.response_tx.send(McpResponse { result: Err(msg) });
                continue;
            }

            if let SceneOp::DestroyEntity { entity } = &scene_op {
                cleanup_entity_hierarchy_world(world, *entity);
            }

            let result = SceneToolExecutor::execute(scene_op, world, registry);
            let response = match result {
                Ok((tool_result, _undo_group)) => {
                    let mut json = serde_json::json!({
                        "success": tool_result.success,
                        "message": tool_result.message,
                        "affected_entities": tool_result.affected_entities.iter().map(|id| id.id()).collect::<Vec<u64>>(),
                    });
                    if let Some(data) = tool_result.data {
                        json.as_object_mut()
                            .unwrap()
                            .insert("data".to_string(), data);
                    }
                    McpResponse { result: Ok(json) }
                }
                Err(e) => McpResponse {
                    result: Err(format!("{:?}", e)),
                },
            };
            let _ = req.response_tx.send(response);
        }
    }
}

fn check_protected_entity(op: &SceneOp, protected: &ProtectedEntities) -> Result<(), String> {
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
