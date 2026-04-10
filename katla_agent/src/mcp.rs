use std::sync::mpsc;

use rmcp::handler::server::ServerHandler;
use rmcp::handler::server::tool::ToolRouter;
use rmcp::handler::server::wrapper::Json;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::Implementation;
use rmcp::model::ServerInfo;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use katla_ecs::scene_tool::SceneOp;

pub struct KatlaMcpServer {
    tool_router: ToolRouter<Self>,
    request_tx: mpsc::Sender<PendingMcpRequest>,
}

pub struct PendingMcpRequest {
    pub op: McpOp,
    pub response_tx: tokio::sync::oneshot::Sender<McpResponse>,
}

#[derive(Debug, Clone)]
pub enum McpOp {
    SpawnEntity {
        position: [f32; 3],
        rotation: [f32; 3],
        scale: [f32; 3],
        name: Option<String>,
    },
    DestroyEntity {
        entity_id: u64,
    },
    SetField {
        entity_id: u64,
        component: String,
        field: String,
        value: serde_json::Value,
    },
    QueryEntities {
        component_filter: String,
        name_filter: Option<String>,
        position: Option<[f32; 3]>,
        radius: Option<f32>,
        limit: Option<usize>,
    },
    GetSceneHierarchy,
    DuplicateEntity {
        entity_id: u64,
        position_offset: Option<[f32; 3]>,
    },
}

impl McpOp {
    pub fn to_scene_op(self) -> SceneOp {
        match self {
            Self::SpawnEntity {
                position,
                rotation,
                scale,
                name,
            } => SceneOp::SpawnEntity {
                position,
                rotation,
                scale,
                name,
            },
            Self::DestroyEntity { entity_id } => SceneOp::DestroyEntity {
                entity: katla_ecs::EntityId::from_raw(entity_id),
            },
            Self::SetField {
                entity_id,
                component,
                field,
                value,
            } => SceneOp::SetField {
                entity: katla_ecs::EntityId::from_raw(entity_id),
                component,
                field,
                value,
            },
            Self::QueryEntities {
                component_filter,
                name_filter,
                position,
                radius,
                limit,
            } => SceneOp::QueryEntities {
                component_filter: Some(component_filter),
                name_filter,
                position,
                radius,
                limit,
            },
            Self::GetSceneHierarchy => SceneOp::GetSceneHierarchy,
            Self::DuplicateEntity {
                entity_id,
                position_offset,
            } => SceneOp::DuplicateEntity {
                entity: katla_ecs::EntityId::from_raw(entity_id),
                position_offset,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct McpToolResult {
    pub success: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

#[derive(Debug)]
pub struct McpResponse {
    pub result: Result<serde_json::Value, String>,
}

pub struct McpBridge {
    receiver: mpsc::Receiver<PendingMcpRequest>,
    shutdown_tx: tokio::sync::watch::Sender<bool>,
}

impl McpBridge {
    pub fn new() -> (KatlaMcpServer, Self, tokio::sync::watch::Receiver<bool>) {
        let (tx, rx) = mpsc::channel();
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        let server = KatlaMcpServer {
            tool_router: KatlaMcpServer::tool_router(),
            request_tx: tx,
        };
        let bridge = McpBridge {
            receiver: rx,
            shutdown_tx,
        };
        (server, bridge, shutdown_rx)
    }

    pub fn poll_requests(&self) -> Vec<PendingMcpRequest> {
        let mut requests = Vec::new();
        while let Ok(req) = self.receiver.try_recv() {
            requests.push(req);
        }
        requests
    }

    pub fn shutdown(&self) {
        let _ = self.shutdown_tx.send(true);
    }
}

/// Start the MCP server on a background thread with stdio transport.
///
/// The server runs until it either completes or a `true` value is sent
/// through the `shutdown_rx` watch channel.
pub fn start_mcp_server_thread(
    server: KatlaMcpServer,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
) {
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let transport = (tokio::io::stdin(), tokio::io::stdout());
            let result = tokio::select! {
                result = rmcp::serve_server(server, transport) => result,
                _ = shutdown_rx.changed() => {
                    log::info!("MCP server shutting down");
                    return;
                }
            };
            if let Err(e) = result {
                log::error!("MCP server error: {}", e);
            }
        });
    });
}

#[derive(Deserialize, JsonSchema, Default)]
struct SpawnEntityParams {
    position: [f32; 3],
    #[serde(default)]
    rotation: Option<[f32; 3]>,
    #[serde(default)]
    scale: Option<[f32; 3]>,
    #[serde(default)]
    name: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct DestroyEntityParams {
    entity_id: u64,
}

#[derive(Deserialize, JsonSchema)]
struct SetFieldParams {
    entity_id: u64,
    component: String,
    field: String,
    value: serde_json::Value,
}

#[derive(Deserialize, JsonSchema)]
struct QueryEntitiesParams {
    component_filter: String,
    #[serde(default)]
    name_filter: Option<String>,
    #[serde(default)]
    position: Option<[f32; 3]>,
    #[serde(default)]
    radius: Option<f32>,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Deserialize, JsonSchema)]
struct DuplicateEntityParams {
    entity_id: u64,
    #[serde(default)]
    position_offset: Option<[f32; 3]>,
}

#[rmcp::tool_router]
impl KatlaMcpServer {
    #[rmcp::tool(
        name = "spawn_entity",
        description = "Spawn a new entity in the scene with a transform"
    )]
    async fn spawn_entity(
        &self,
        Parameters(params): Parameters<SpawnEntityParams>,
    ) -> Json<McpToolResult> {
        let op = McpOp::SpawnEntity {
            position: params.position,
            rotation: params.rotation.unwrap_or([0.0, 0.0, 0.0]),
            scale: params.scale.unwrap_or([1.0, 1.0, 1.0]),
            name: params.name,
        };
        self.forward_op(op).await
    }

    #[rmcp::tool(
        name = "destroy_entity",
        description = "Remove an entity from the scene"
    )]
    async fn destroy_entity(
        &self,
        Parameters(params): Parameters<DestroyEntityParams>,
    ) -> Json<McpToolResult> {
        let op = McpOp::DestroyEntity {
            entity_id: params.entity_id,
        };
        self.forward_op(op).await
    }

    #[rmcp::tool(
        name = "set_field",
        description = "Set a component field value on an entity"
    )]
    async fn set_field(
        &self,
        Parameters(params): Parameters<SetFieldParams>,
    ) -> Json<McpToolResult> {
        let op = McpOp::SetField {
            entity_id: params.entity_id,
            component: params.component,
            field: params.field,
            value: params.value,
        };
        self.forward_op(op).await
    }

    #[rmcp::tool(
        name = "query_entities",
        description = "Query entities by component type"
    )]
    async fn query_entities(
        &self,
        Parameters(params): Parameters<QueryEntitiesParams>,
    ) -> Json<McpToolResult> {
        let op = McpOp::QueryEntities {
            component_filter: params.component_filter,
            name_filter: params.name_filter,
            position: params.position,
            radius: params.radius,
            limit: params.limit,
        };
        self.forward_op(op).await
    }

    #[rmcp::tool(
        name = "get_scene_hierarchy",
        description = "Get the full scene hierarchy as JSON"
    )]
    async fn get_scene_hierarchy(&self) -> Json<McpToolResult> {
        self.forward_op(McpOp::GetSceneHierarchy).await
    }

    #[rmcp::tool(
        name = "duplicate_entity",
        description = "Duplicate an entity with an optional position offset"
    )]
    async fn duplicate_entity(
        &self,
        Parameters(params): Parameters<DuplicateEntityParams>,
    ) -> Json<McpToolResult> {
        let op = McpOp::DuplicateEntity {
            entity_id: params.entity_id,
            position_offset: params.position_offset,
        };
        self.forward_op(op).await
    }
}

impl KatlaMcpServer {
    async fn forward_op(&self, op: McpOp) -> Json<McpToolResult> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let request = PendingMcpRequest {
            op,
            response_tx: tx,
        };
        if self.request_tx.send(request).is_err() {
            return Json(McpToolResult {
                success: false,
                message: "Engine is not running".to_string(),
                data: None,
            });
        }
        match rx.await {
            Ok(response) => match response.result {
                Ok(value) => Json(McpToolResult {
                    success: true,
                    message: "ok".to_string(),
                    data: Some(value),
                }),
                Err(e) => Json(McpToolResult {
                    success: false,
                    message: e,
                    data: None,
                }),
            },
            Err(_) => Json(McpToolResult {
                success: false,
                message: "Engine did not respond".to_string(),
                data: None,
            }),
        }
    }
}

#[rmcp::tool_handler]
impl ServerHandler for KatlaMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::default()
            .with_instructions("Katla 3D engine scene tools. Use these tools to spawn, modify, query, and destroy entities in the live scene.")
            .with_server_info(Implementation::new("katla-mcp", "0.1.0"))
    }
}
