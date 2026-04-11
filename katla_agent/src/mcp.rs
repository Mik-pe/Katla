use std::sync::mpsc;

use rmcp::handler::server::ServerHandler;
use rmcp::handler::server::tool::ToolRouter;
use rmcp::handler::server::wrapper::Json;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::Implementation;
use rmcp::model::ServerInfo;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use katla_ecs::scene_tool::{ResourceOp, SceneOp};

pub struct KatlaMcpServer {
    tool_router: ToolRouter<Self>,
    request_tx: mpsc::Sender<PendingMcpRequest>,
}

pub struct PendingMcpRequest {
    pub op: McpOp,
    pub response_tx: tokio::sync::oneshot::Sender<McpResponse>,
}

#[derive(Debug, Clone)]
pub enum McpOpKind {
    Scene(SceneOp),
    Resource(ResourceOp),
    LoadScene { path: String },
    SaveScene { path: Option<String> },
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
    ListAvailableComponents,
    AddComponent {
        entity_id: u64,
        component: String,
    },
    GetComponentAttributes {
        entity_id: u64,
        component: String,
    },
    SetParent {
        entity_id: u64,
        parent_id: Option<u64>,
    },
    ListResources {
        path: Option<String>,
        filter: Option<String>,
    },
    ReadResource {
        path: String,
    },
    WriteResource {
        path: String,
        content: String,
    },
    CreateResource {
        path: String,
        template: Option<String>,
        content: Option<String>,
    },
    SpawnModel {
        path: String,
        position: [f32; 3],
        default_animation: Option<String>,
    },
    LoadScene {
        path: String,
    },
    SaveScene {
        path: Option<String>,
    },
}

impl McpOp {
    pub fn into_op(self) -> McpOpKind {
        match self {
            Self::SpawnEntity {
                position,
                rotation,
                scale,
                name,
            } => McpOpKind::Scene(SceneOp::SpawnEntity {
                position,
                rotation,
                scale,
                name,
                primitive: None,
            }),
            Self::DestroyEntity { entity_id } => McpOpKind::Scene(SceneOp::DestroyEntity {
                entity: katla_ecs::EntityId::from_raw(entity_id),
            }),
            Self::SetField {
                entity_id,
                component,
                field,
                value,
            } => McpOpKind::Scene(SceneOp::SetField {
                entity: katla_ecs::EntityId::from_raw(entity_id),
                component,
                field,
                value,
            }),
            Self::QueryEntities {
                component_filter,
                name_filter,
                position,
                radius,
                limit,
            } => McpOpKind::Scene(SceneOp::QueryEntities {
                component_filter: Some(component_filter),
                name_filter,
                position,
                radius,
                limit,
            }),
            Self::GetSceneHierarchy => McpOpKind::Scene(SceneOp::GetSceneHierarchy),
            Self::DuplicateEntity {
                entity_id,
                position_offset,
            } => McpOpKind::Scene(SceneOp::DuplicateEntity {
                entity: katla_ecs::EntityId::from_raw(entity_id),
                position_offset,
            }),
            Self::ListAvailableComponents => McpOpKind::Scene(SceneOp::ListAvailableComponents),
            Self::AddComponent {
                entity_id,
                component,
            } => McpOpKind::Scene(SceneOp::AddComponent {
                entity: katla_ecs::EntityId::from_raw(entity_id),
                component,
            }),
            Self::GetComponentAttributes {
                entity_id,
                component,
            } => McpOpKind::Scene(SceneOp::GetComponentAttributes {
                entity: katla_ecs::EntityId::from_raw(entity_id),
                component,
            }),
            Self::SetParent {
                entity_id,
                parent_id,
            } => McpOpKind::Scene(SceneOp::SetParent {
                entity: katla_ecs::EntityId::from_raw(entity_id),
                parent: parent_id.map(katla_ecs::EntityId::from_raw),
            }),
            Self::ListResources { path, filter } => {
                McpOpKind::Resource(ResourceOp::ListResources {
                    path: path.unwrap_or_default(),
                    filter,
                })
            }
            Self::ReadResource { path } => McpOpKind::Resource(ResourceOp::ReadResource { path }),
            Self::WriteResource { path, content } => {
                McpOpKind::Resource(ResourceOp::WriteResource { path, content })
            }
            Self::CreateResource {
                path,
                template,
                content,
            } => McpOpKind::Resource(ResourceOp::CreateResource {
                path,
                template,
                content,
            }),
            Self::SpawnModel {
                path,
                position,
                default_animation,
            } => McpOpKind::Scene(SceneOp::SpawnModel {
                path,
                position,
                default_animation,
            }),
            Self::LoadScene { path } => McpOpKind::LoadScene { path },
            Self::SaveScene { path } => McpOpKind::SaveScene { path },
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

#[derive(Deserialize, JsonSchema, Default)]
struct AddComponentParams {
    entity_id: u64,
    component: String,
}

#[derive(Deserialize, JsonSchema)]
struct GetComponentAttributesParams {
    entity_id: u64,
    component: String,
}

#[derive(Deserialize, JsonSchema, Default)]
struct SetParentParams {
    entity_id: u64,
    #[serde(default)]
    parent_id: Option<u64>,
}

#[derive(Deserialize, JsonSchema, Default)]
struct ListResourcesParams {
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    filter: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct ReadResourceParams {
    path: String,
}

#[derive(Deserialize, JsonSchema)]
struct WriteResourceParams {
    path: String,
    content: String,
}

#[derive(Deserialize, JsonSchema)]
struct CreateResourceParams {
    path: String,
    #[serde(default)]
    template: Option<String>,
    #[serde(default)]
    content: Option<String>,
}

#[derive(Deserialize, JsonSchema, Default)]
struct SpawnModelParams {
    path: String,
    #[serde(default)]
    position: Option<[f32; 3]>,
    #[serde(default)]
    default_animation: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct LoadSceneParams {
    path: String,
}

#[derive(Deserialize, JsonSchema, Default)]
struct SaveSceneParams {
    #[serde(default)]
    path: Option<String>,
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

    #[rmcp::tool(
        name = "list_available_components",
        description = "List all registered component types with their settable fields and types"
    )]
    async fn list_available_components(&self) -> Json<McpToolResult> {
        self.forward_op(McpOp::ListAvailableComponents).await
    }

    #[rmcp::tool(
        name = "add_component",
        description = "Add a component with default values to an existing entity"
    )]
    async fn add_component(
        &self,
        Parameters(params): Parameters<AddComponentParams>,
    ) -> Json<McpToolResult> {
        let op = McpOp::AddComponent {
            entity_id: params.entity_id,
            component: params.component,
        };
        self.forward_op(op).await
    }

    #[rmcp::tool(
        name = "get_component_attributes",
        description = "Get settable fields, types, and current values for a component on an entity"
    )]
    async fn get_component_attributes(
        &self,
        Parameters(params): Parameters<GetComponentAttributesParams>,
    ) -> Json<McpToolResult> {
        let op = McpOp::GetComponentAttributes {
            entity_id: params.entity_id,
            component: params.component,
        };
        self.forward_op(op).await
    }

    #[rmcp::tool(
        name = "set_parent",
        description = "Set or clear the parent of an entity"
    )]
    async fn set_parent(
        &self,
        Parameters(params): Parameters<SetParentParams>,
    ) -> Json<McpToolResult> {
        let op = McpOp::SetParent {
            entity_id: params.entity_id,
            parent_id: params.parent_id,
        };
        self.forward_op(op).await
    }

    #[rmcp::tool(
        name = "list_resources",
        description = "List resource files in a project directory"
    )]
    async fn list_resources(
        &self,
        Parameters(params): Parameters<ListResourcesParams>,
    ) -> Json<McpToolResult> {
        let op = McpOp::ListResources {
            path: params.path,
            filter: params.filter,
        };
        self.forward_op(op).await
    }

    #[rmcp::tool(
        name = "read_resource",
        description = "Read a resource file's content as text"
    )]
    async fn read_resource(
        &self,
        Parameters(params): Parameters<ReadResourceParams>,
    ) -> Json<McpToolResult> {
        let op = McpOp::ReadResource { path: params.path };
        self.forward_op(op).await
    }

    #[rmcp::tool(
        name = "write_resource",
        description = "Write content to an existing resource file"
    )]
    async fn write_resource(
        &self,
        Parameters(params): Parameters<WriteResourceParams>,
    ) -> Json<McpToolResult> {
        let op = McpOp::WriteResource {
            path: params.path,
            content: params.content,
        };
        self.forward_op(op).await
    }

    #[rmcp::tool(
        name = "create_resource",
        description = "Create a new resource file with optional template"
    )]
    async fn create_resource(
        &self,
        Parameters(params): Parameters<CreateResourceParams>,
    ) -> Json<McpToolResult> {
        let op = McpOp::CreateResource {
            path: params.path,
            template: params.template,
            content: params.content,
        };
        self.forward_op(op).await
    }

    #[rmcp::tool(
        name = "spawn_model",
        description = "Spawn a GLTF model from the project's assets directory"
    )]
    async fn spawn_model(
        &self,
        Parameters(params): Parameters<SpawnModelParams>,
    ) -> Json<McpToolResult> {
        let op = McpOp::SpawnModel {
            path: params.path,
            position: params.position.unwrap_or([0.0, 0.0, 0.0]),
            default_animation: params.default_animation,
        };
        self.forward_op(op).await
    }

    #[rmcp::tool(
        name = "load_scene",
        description = "Load a scene from a .katla file, replacing all entities in the current scene"
    )]
    async fn load_scene(
        &self,
        Parameters(params): Parameters<LoadSceneParams>,
    ) -> Json<McpToolResult> {
        let op = McpOp::LoadScene { path: params.path };
        self.forward_op(op).await
    }

    #[rmcp::tool(
        name = "save_scene",
        description = "Save the current scene to a .katla file"
    )]
    async fn save_scene(
        &self,
        Parameters(params): Parameters<SaveSceneParams>,
    ) -> Json<McpToolResult> {
        let op = McpOp::SaveScene { path: params.path };
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
