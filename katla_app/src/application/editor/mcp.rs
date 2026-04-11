use katla_agent::{McpBridge, McpResponse, start_mcp_server_thread};
use katla_ecs::EntityId;
use katla_ecs::scene_tool::{ComponentRegistry, SceneOp, SceneToolExecutor};
use log::info;

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

            let result = SceneToolExecutor::execute(scene_op, world, registry);
            let response = match result {
                Ok((tool_result, _undo_group)) => McpResponse {
                    result: Ok(serde_json::json!({
                        "success": tool_result.success,
                        "message": tool_result.message,
                        "affected_entities": tool_result.affected_entities.iter().map(|id| id.id()).collect::<Vec<u64>>(),
                    })),
                },
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
        | SceneOp::DuplicateEntity { entity, .. } => Some(*entity),
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
