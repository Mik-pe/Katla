use katla_agent::{McpBridge, McpResponse, start_mcp_server_thread};
use katla_ecs::scene_tool::{ComponentRegistry, SceneToolExecutor};
use log::info;

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

    pub(crate) fn poll(&mut self, world: &mut katla_ecs::World, registry: &ComponentRegistry) {
        let requests = self.bridge.poll_requests();
        for req in requests {
            let scene_op = req.op.to_scene_op();
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
