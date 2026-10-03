use std::sync::mpsc;

use rmcp::handler::server::ServerHandler;
use rmcp::handler::server::wrapper::Json;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::Implementation;
use rmcp::model::ServerInfo;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use katla_ecs::scene_tool::{ResourceOp, SceneOp};

#[derive(Clone)]
pub struct KatlaMcpServer {
    request_tx: mpsc::Sender<PendingMcpRequest>,
}

pub struct PendingMcpRequest {
    pub op: McpOp,
    pub response_tx: tokio::sync::oneshot::Sender<McpResponse>,
}

/// Receiver for a committed editor capture, shared by tool and scene-question paths.
pub type McpResponseReceiver = tokio::sync::oneshot::Receiver<McpResponse>;
/// Nonblocking capture receive errors.
pub type McpTryRecvError = tokio::sync::oneshot::error::TryRecvError;

impl PendingMcpRequest {
    /// Request the next committed view without changing camera or selection.
    pub fn observe() -> (Self, McpResponseReceiver) {
        let (response_tx, receiver) = tokio::sync::oneshot::channel();
        (
            Self {
                op: McpOp::Editor(EditorViewOp::Observe { limit: None }),
                response_tx,
            },
            receiver,
        )
    }
}

#[derive(Debug, Clone)]
pub enum McpOpKind {
    SearchAssets(crate::tools::search::AssetSearch),
    Material(crate::material::MaterialOp),
    Prefab(crate::prefab::PrefabOp),
    Behavior(crate::behavior::BehaviorOp),
    Simulation(crate::behavior::SimulationOp),
    Animation(crate::animation::AnimationOp),
    Trigger(crate::events::TriggerOp),
    Editor(EditorViewOp),
    Scene(SceneOp),
    Resource(ResourceOp),
    LoadScene { path: String },
    SaveScene { path: Option<String> },
}

/// Operations on the editor view; these never modify a game camera or save a scene.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum EditorViewOp {
    /// Observe the next committed frame, including its image and geometric candidates.
    Observe {
        #[serde(default)]
        limit: Option<usize>,
    },
    /// Move once to a world-space pose. Manual navigation resumes immediately.
    SetCamera {
        position: [f32; 3],
        target: [f32; 3],
    },
    /// Change only the editor selection. Null clears it.
    Select { entity_id: Option<String> },
    /// Fit the render bounds of an object in the editor view.
    Focus {
        entity_id: String,
        #[serde(default)]
        select: bool,
    },
    /// Undo the last agent scene operation using the editor's existing agent history.
    Undo,
    /// Redo the last undone agent operation.
    Redo,
}

#[derive(Debug, Clone)]
pub enum McpOp {
    SearchAssets(crate::tools::search::AssetSearch),
    Material(crate::material::MaterialOp),
    Prefab(crate::prefab::PrefabOp),
    Behavior(crate::behavior::BehaviorOp),
    Simulation(crate::behavior::SimulationOp),
    Animation(crate::animation::AnimationOp),
    Trigger(crate::events::TriggerOp),
    Editor(EditorViewOp),
    SpawnEntity {
        position: [f32; 3],
        rotation: [f32; 3],
        scale: [f32; 3],
        name: Option<String>,
        shape: Option<String>,
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
        component_filter: Option<String>,
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
            Self::SearchAssets(op) => McpOpKind::SearchAssets(op),
            Self::Material(op) => McpOpKind::Material(op),
            Self::Prefab(op) => McpOpKind::Prefab(op),
            Self::Behavior(op) => McpOpKind::Behavior(op),
            Self::Simulation(op) => McpOpKind::Simulation(op),
            Self::Animation(op) => McpOpKind::Animation(op),
            Self::Trigger(op) => McpOpKind::Trigger(op),
            Self::Editor(op) => McpOpKind::Editor(op),
            Self::SpawnEntity {
                position,
                rotation,
                scale,
                name,
                shape,
            } => McpOpKind::Scene(SceneOp::SpawnEntity {
                position,
                rotation,
                scale,
                name,
                primitive: shape,
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
                component_filter,
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
        let server = KatlaMcpServer { request_tx: tx };
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
        let rt = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(e) => {
                log::error!("Failed to create tokio runtime for MCP server: {}", e);
                return;
            }
        };
        rt.block_on(async {
            #[cfg(unix)]
            if let Some(path) = std::env::var_os("KATLA_MCP_SOCKET") {
                use std::os::unix::fs::PermissionsExt;
                let listener = match tokio::net::UnixListener::bind(&path) {
                    Ok(listener) => listener,
                    Err(e) => { log::error!("MCP socket bind failed: {e}"); return; }
                };
                if let Err(e) = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)) {
                    log::error!("MCP socket permissions failed: {e}");
                    let _ = std::fs::remove_file(&path);
                    return;
                }
                log::info!("MCP editor socket ready: {}", std::path::Path::new(&path).display());
                loop {
                    tokio::select! {
                        connection = listener.accept() => {
                            match connection {
                                Ok((stream, _)) => {
                                    let handler = server.clone();
                                    let mut shutdown = shutdown_rx.clone();
                                    tokio::spawn(async move {
                                        let (read, write) = stream.into_split();
                                        tokio::select! {
                                            _ = async {
                                                match rmcp::serve_server(handler, (read, write)).await {
                                                    Ok(service) => { let _ = service.waiting().await; }
                                                    Err(e) => log::warn!("MCP connection failed: {e}"),
                                                }
                                            } => {},
                                            _ = shutdown.changed() => {},
                                        }
                                    });
                                }
                                Err(e) => log::warn!("MCP accept failed: {e}"),
                            }
                        }
                        _ = shutdown_rx.changed() => break,
                    }
                }
                drop(listener);
                let _ = std::fs::remove_file(&path);
                return;
            }
            tokio::select! {
                _ = async {
                    match rmcp::serve_server(server, (tokio::io::stdin(), tokio::io::stdout())).await {
                        Ok(service) => { let _ = service.waiting().await; }
                        Err(e) => log::error!("MCP server error: {e}"),
                    }
                } => {},
                _ = shutdown_rx.changed() => {},
            }
        });
    });
}

#[derive(Deserialize, JsonSchema)]
#[serde(try_from = "String")]
#[schemars(with = "String")]
struct EntityReference(u64);

impl TryFrom<String> for EntityReference {
    type Error = String;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        value
            .parse::<u64>()
            .map(Self)
            .map_err(|_| "Expected a decimal generational entity_id string".into())
    }
}

#[derive(Deserialize, JsonSchema)]
struct TriggerParams {
    #[serde(flatten)]
    op: crate::events::TriggerOp<String>,
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
    #[serde(default)]
    shape: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct DestroyEntityParams {
    entity_id: EntityReference,
}

#[derive(Deserialize, JsonSchema)]
struct SetFieldParams {
    entity_id: EntityReference,
    component: String,
    field: String,
    value: serde_json::Value,
}

#[derive(Deserialize, JsonSchema)]
struct QueryEntitiesParams {
    #[serde(default)]
    component_filter: Option<String>,
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
    entity_id: EntityReference,
    #[serde(default)]
    position_offset: Option<[f32; 3]>,
}

#[derive(Deserialize, JsonSchema)]
struct AddComponentParams {
    entity_id: EntityReference,
    component: String,
}

#[derive(Deserialize, JsonSchema)]
struct GetComponentAttributesParams {
    entity_id: EntityReference,
    component: String,
}

#[derive(Deserialize, JsonSchema)]
struct SetParentParams {
    entity_id: EntityReference,
    #[serde(default)]
    parent_id: Option<EntityReference>,
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

#[derive(Deserialize, JsonSchema)]
struct EditorViewParams {
    #[serde(flatten)]
    op: EditorViewOp,
}

#[derive(Deserialize)]
struct MaterialParams {
    #[serde(flatten)]
    op: crate::material::MaterialOp,
}

impl JsonSchema for MaterialParams {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "MaterialParams".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        crate::material::MaterialOp::schema_object().into()
    }
}

#[derive(Deserialize, JsonSchema)]
struct AnimationParams {
    #[serde(flatten)]
    op: crate::animation::AnimationOp,
}

#[derive(Deserialize, JsonSchema)]
struct PrefabParams {
    #[serde(flatten)]
    op: crate::prefab::PrefabOp,
}

#[derive(Deserialize, JsonSchema)]
struct BehaviorParams {
    #[serde(flatten)]
    op: crate::behavior::BehaviorOp,
}
#[derive(Deserialize, JsonSchema)]
struct SimulationParams {
    #[serde(flatten)]
    op: crate::behavior::SimulationOp,
}

#[rmcp::tool_router]
impl KatlaMcpServer {
    #[rmcp::tool(
        name = "behavior",
        description = "Attach validated Luau scripts, configure particles and preview bursts. describe gives particle JSON and a script example. inspect returns attachments. set_script uses resource-relative scripts/name.luau, set_particles a full descriptor; null detaches. Authored changes are undoable in edit mode. IDs are decimal strings."
    )]
    async fn behavior(
        &self,
        Parameters(params): Parameters<BehaviorParams>,
    ) -> Json<McpToolResult> {
        self.forward_op(McpOp::Behavior(params.op)).await
    }
    #[rmcp::tool(
        name = "simulation",
        description = "Inspect or explicitly play/pause/resume/stop the editor preview. Stop restores authored state and replaces runtime IDs; query again afterward. Use play to verify scripts and triggers, stop before saving or prefab capture."
    )]
    async fn simulation(
        &self,
        Parameters(params): Parameters<SimulationParams>,
    ) -> Json<McpToolResult> {
        self.forward_op(McpOp::Simulation(params.op)).await
    }

    #[rmcp::tool(
        name = "prefab",
        description = "Author .katmesh and .katprefab assets. describe returns JSON examples; validate/write checks complete recipes; instantiate appends a preview; capture saves a live subtree; remove deletes a preview. Read/edit named parts, then observe editor_view for the rendered result. IDs are decimal strings; paths are project-relative."
    )]
    async fn prefab(&self, Parameters(params): Parameters<PrefabParams>) -> Json<McpToolResult> {
        self.forward_op(McpOp::Prefab(params.op)).await
    }

    #[rmcp::tool(
        name = "search_assets",
        description = "Find project assets recursively by words in their paths and optional extensions. Example query chair, extensions [glb,gltf]. Returns sorted resource-relative paths ready for spawn_model, total and truncation. Empty query lists assets; never invent model filenames."
    )]
    async fn search_assets(
        &self,
        Parameters(op): Parameters<crate::tools::search::AssetSearch>,
    ) -> Json<McpToolResult> {
        self.forward_op(McpOp::SearchAssets(op)).await
    }

    #[rmcp::tool(
        name = "material",
        description = "Discover presets and supported limits, inspect image provenance/UVs/samplers, set_sampling on one role, or set PBR factors on 1..256 mesh objects as one undoable batch. Use action presets first. base_color uses sRGB RGB and linear alpha in 0..1; alpha does not switch render mode. Presets provide isotropic factors without textures or directional brushing. Partial patches preserve other factors; preset supplies defaults, explicit factors override it. set_sampling takes role and patch, with UV rotation in radians. It preserves images and omitted fields and persists through scene reload. Texture assignment is unavailable. Use query_entities material_editable flags to choose targets and editor_view to verify native results."
    )]
    async fn material(
        &self,
        Parameters(params): Parameters<MaterialParams>,
    ) -> rmcp::model::CallToolResult {
        let result = self.forward_op(McpOp::Material(params.op)).await.0;
        if result.success {
            rmcp::model::CallToolResult::structured(
                serde_json::json!({"success":true,"message":result.message,"data":result.data}),
            )
        } else {
            rmcp::model::CallToolResult::structured_error(
                serde_json::json!({"success":false,"message":result.message}),
            )
        }
    }

    #[rmcp::tool(
        name = "trigger",
        description = "Create a sensor box, replace its enter/exit rules, or inspect rules and overlaps. Actions play named animations, burst/toggle particle emitters, or emit Luau events. Entity IDs are decimal generational strings from scene context; use other to act on the visitor. Rules run in play mode."
    )]
    async fn trigger(&self, Parameters(op): Parameters<TriggerParams>) -> Json<McpToolResult> {
        let op = match op.op.resolve_ids() {
            Ok(op) => op,
            Err(message) => {
                return Json(McpToolResult {
                    success: false,
                    message,
                    data: None,
                });
            }
        };
        self.forward_op(McpOp::Trigger(op)).await
    }

    #[rmcp::tool(
        name = "animation",
        description = "Inspect clips and fade progress, or play a named clip. Defaults: fade_seconds 0.25, looping true, speed 1. Positive fades reject an already active fade; zero switches immediately."
    )]
    async fn animation(
        &self,
        Parameters(params): Parameters<AnimationParams>,
    ) -> Json<McpToolResult> {
        self.forward_op(McpOp::Animation(params.op)).await
    }

    #[rmcp::tool(
        name = "editor_view",
        description = "Read the shared editor viewport or move/focus/select once, then read back a fresh committed frame. Returns PNG plus camera, optional selection, projected frustum candidates, and GPU-picked center/pointer. Frustum intersection is NOT occlusion visibility or room membership. Do not edit ambiguous candidates without clarification. entity_id is the stable generational string from context; stale IDs fail. Undo/redo use agent history. Observe is available during Play/Pause; editing the view/history requires edit mode. Camera movement is ephemeral; manual input takes over immediately."
    )]
    async fn editor_view(
        &self,
        Parameters(params): Parameters<EditorViewParams>,
    ) -> rmcp::model::CallToolResult {
        let result = self.forward_op(McpOp::Editor(params.op)).await.0;
        if !result.success {
            return rmcp::model::CallToolResult::error(vec![rmcp::model::ContentBlock::text(
                result.message,
            )]);
        }
        let mut data = result.data.unwrap_or_default();
        let png = data
            .as_object_mut()
            .and_then(|map| map.remove("image_png_base64"));
        let mut content = vec![rmcp::model::ContentBlock::text(data.to_string())];
        if let Some(serde_json::Value::String(png)) = png {
            content.push(rmcp::model::ContentBlock::image(png, "image/png"));
        }
        rmcp::model::CallToolResult::success(content)
    }

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
            shape: params.shape,
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
            entity_id: params.entity_id.0,
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
            entity_id: params.entity_id.0,
            component: params.component,
            field: params.field,
            value: params.value,
        };
        self.forward_op(op).await
    }

    #[rmcp::tool(
        name = "query_entities",
        description = "Find entities by optional component type, name substring, or world position and radius in meters. Results include material_editable flags; choose true rows for material batches. No filter lists the scene. Bounds distance is used when available; this query makes no occlusion claim."
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
            entity_id: params.entity_id.0,
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
            entity_id: params.entity_id.0,
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
            entity_id: params.entity_id.0,
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
            entity_id: params.entity_id.0,
            parent_id: params.parent_id.map(|id| id.0),
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
        description = "Spawn a glTF scene using a resource-relative path from search_assets. Returns root_entity_id, all created entities and material_entity_ids; multi-primitive materials are editable child entities, and animation on the root controls skinned children"
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
        match tokio::time::timeout(std::time::Duration::from_secs(15), rx).await {
            Ok(Ok(response)) => match response.result {
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
            Ok(Err(_)) | Err(_) => Json(McpToolResult {
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
        let mut info = ServerInfo::default();
        info.capabilities = rmcp::model::ServerCapabilities::builder()
            .enable_tools()
            .build();
        info
            .with_instructions("Katla scene authoring: first editor_view observe and query_entities to understand the scene. Search assets with search_assets (model extensions glb/gltf); use returned paths with spawn_model. Spawn named primitives, group them with set_parent, and inspect material presets before applying PBR factors to entity_ids. Y is up, units are meters, rotations are degrees. IDs are decimal generational strings. For reusable assets, prefab describe/read/validate/write builds .katmesh/.katprefab, instantiate returns named nodes; search_assets project_paths are prefab tool paths. behavior describe/set_script/set_particles connects validated Luau and particle descriptors. trigger rules link animations, particle bursts/toggles and script events. simulation play/pause/stop controls preview; stop restores authored state and replaces IDs, so query again before capture/save. Observe after edits, editor_view undo reverses agent edits, save_scene persists authored changes.")
            .with_server_info(Implementation::new("katla-mcp", "0.1.0"))
    }
}

#[cfg(test)]
mod animation_tests {
    use super::*;
    use crate::animation::AnimationOp;

    #[test]
    fn test_material_mcp_schema_has_object_root() {
        let schema = serde_json::to_value(schemars::schema_for!(MaterialParams)).unwrap();
        assert_eq!(schema["type"], "object");
        assert_eq!(schema["additionalProperties"], false);
        assert_eq!(schema["oneOf"].as_array().unwrap().len(), 4);
        for branch in schema["oneOf"].as_array().unwrap() {
            assert_eq!(branch["additionalProperties"], false);
        }
        let params: MaterialParams = serde_json::from_value(
            serde_json::json!({"action":"set","entity_ids":["4294967302"],"roughness":0.3}),
        )
        .unwrap();
        assert!(matches!(
            params.op,
            crate::material::MaterialOp::Set {
                roughness: Some(0.3),
                ..
            }
        ));
    }

    #[test]
    fn test_material_mcp_application_errors_are_structured_tool_errors() {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            use std::future::Future;
            let (server, bridge, _shutdown) = McpBridge::new();
            let mut call = Box::pin(server.material(Parameters(MaterialParams {
                op: crate::material::MaterialOp::Inspect {
                    entity_id: "42".into(),
                },
            })));
            std::future::poll_fn(|cx| {
                assert!(call.as_mut().poll(cx).is_pending());
                std::task::Poll::Ready(())
            })
            .await;
            bridge
                .poll_requests()
                .pop()
                .unwrap()
                .response_tx
                .send(McpResponse {
                    result: Err("Entity 42 has no rendered material".into()),
                })
                .unwrap();
            let response = call.await;
            assert_eq!(response.is_error, Some(true));
            let data = response.structured_content.unwrap();
            assert_eq!(data["success"], false);
            assert_eq!(data["message"], "Entity 42 has no rendered material");
        });
    }

    #[test]
    fn test_animation_tool_forwards_typed_request_and_response() {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let (server, bridge, _shutdown) = McpBridge::new();
            use std::future::Future;
            let mut call = Box::pin(server.animation(Parameters(AnimationParams {
                op: AnimationOp::Play {
                    entity_id: 42,
                    clip: "Run".into(),
                    fade_seconds: 0.25,
                    looping: true,
                    speed: 1.0,
                },
            })));
            std::future::poll_fn(|cx| {
                assert!(call.as_mut().poll(cx).is_pending());
                std::task::Poll::Ready(())
            })
            .await;
            let request = bridge
                .poll_requests()
                .pop()
                .expect("tool reaches application bridge");
            assert!(matches!(
                request.op.into_op(),
                McpOpKind::Animation(AnimationOp::Play {
                    entity_id: 42,
                    fade_seconds: 0.25,
                    ..
                })
            ));
            request
                .response_tx
                .send(McpResponse {
                    result: Ok(serde_json::json!({"playback":{"transition":{"progress":0.5}}})),
                })
                .unwrap();
            let response = call.await.0;
            assert!(response.success);
            assert_eq!(
                response.data.unwrap()["playback"]["transition"]["progress"],
                0.5
            );
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_editor_view_tool_has_object_input_schema() {
        let tools = KatlaMcpServer::tool_router().list_all();
        for name in [
            "editor_view",
            "animation",
            "trigger",
            "prefab",
            "material",
            "search_assets",
            "behavior",
            "simulation",
        ] {
            let tool = tools.iter().find(|t| t.name == name).unwrap();
            assert_eq!(
                tool.input_schema.get("type"),
                Some(&serde_json::json!("object"))
            );
        }
        let animation: AnimationParams = serde_json::from_value(serde_json::json!({
            "action": "play", "entity_id": 42, "clip": "Run"
        }))
        .unwrap();
        assert!(matches!(
            animation.op,
            crate::animation::AnimationOp::Play {
                entity_id: 42,
                fade_seconds: 0.25,
                looping: true,
                speed: 1.0,
                ..
            }
        ));
        let params: EditorViewParams =
            serde_json::from_value(serde_json::json!({"action":"select","entity_id":null}))
                .unwrap();
        assert!(matches!(
            params.op,
            EditorViewOp::Select { entity_id: None }
        ));
    }
}

#[cfg(test)]
mod events_tests {
    use super::*;
    use crate::events::TriggerOp;
    #[test]
    fn test_trigger_tool_rejects_invalid_ids_before_forwarding() {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let (server, bridge, _shutdown) = McpBridge::new();
            let response = server
                .trigger(Parameters(TriggerParams {
                    op: TriggerOp::Inspect {
                        entity_id: "18446744073709551616".into(),
                    },
                }))
                .await
                .0;
            assert!(!response.success);
            assert!(response.message.contains("decimal u64 string"));
            assert!(bridge.poll_requests().is_empty());
        });
    }

    #[test]
    fn test_trigger_tool_forwards_authoring_request_and_response() {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            use std::future::Future;
            let (server, bridge, _shutdown) = McpBridge::new();
            let op = TriggerOp::<String>::CreateBox {
                name: "Entrance".into(),
                position: [0.0; 3],
                half_extents: [1.0; 3],
                rules: vec![],
            };
            let mut call = Box::pin(server.trigger(Parameters(TriggerParams { op })));
            std::future::poll_fn(|cx| {
                assert!(call.as_mut().poll(cx).is_pending());
                std::task::Poll::Ready(())
            })
            .await;
            let request = bridge.poll_requests().pop().unwrap();
            assert!(matches!(
                request.op.into_op(),
                McpOpKind::Trigger(TriggerOp::CreateBox {
                    half_extents: [1.0, 1.0, 1.0],
                    ..
                })
            ));
            request
                .response_tx
                .send(McpResponse {
                    result: Ok(serde_json::json!({"entity_id":"17","rules":[]})),
                })
                .unwrap();
            let response = call.await.0;
            assert!(response.success);
            assert_eq!(response.data.unwrap()["entity_id"], "17");
        });
    }
}
