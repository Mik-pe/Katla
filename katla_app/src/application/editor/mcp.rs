use super::{agent, viewport};
use katla_agent::PendingMcpRequest;
use katla_agent::mcp::EditorViewOp;
use katla_agent::{McpBridge, McpOpKind, McpResponse, start_mcp_server_thread};
use katla_ecs::EntityId;
use katla_ecs::scene_tool::SceneOp;
use katla_gfx::GpuRenderer;
use log::info;

pub(crate) struct McpState {
    bridge: McpBridge,
    awaiting_view: Vec<(PendingMcpRequest, usize)>,
    captures: Vec<ViewCapture>,
}

impl McpState {
    pub(super) fn observe(&mut self) -> katla_agent::mcp::McpResponseReceiver {
        let (request, receiver) = PendingMcpRequest::observe();
        self.awaiting_view.push((request, 64));
        receiver
    }

    pub(crate) fn new() -> Self {
        let (server, bridge, shutdown_rx) = McpBridge::new();
        start_mcp_server_thread(server, shutdown_rx);
        info!("MCP server bridge initialized");
        Self {
            bridge,
            awaiting_view: Vec::new(),
            captures: Vec::new(),
        }
    }
}

pub(crate) fn poll(app: &mut crate::application::Application) {
    poll_captures(app);
    let requests = app.editor.mcp_state.bridge.poll_requests();
    for req in requests {
        let response = match req.op.clone().into_op() {
            McpOpKind::Behavior(op) => McpResponse {
                result: super::behavior::execute(app, op),
            },
            McpOpKind::Simulation(op) => McpResponse {
                result: super::simulation::execute(app, op),
            },
            McpOpKind::Prefab(op) => McpResponse {
                result: crate::prefab::control::execute(app, op),
            },
            McpOpKind::SearchAssets(op) => McpResponse {
                result: katla_agent::tools::search::search_assets(&app.resources.root, &op),
            },
            McpOpKind::Material(op) => McpResponse {
                result: super::material::execute(app, op, true),
            },
            McpOpKind::Trigger(op) => McpResponse {
                result: crate::events::control::author(app, op),
            },
            McpOpKind::Animation(op) => McpResponse {
                result: crate::animation::control::execute(&mut app.world, op),
            },
            McpOpKind::Editor(op) => match viewport::apply(app, &op) {
                Ok(()) => {
                    let limit = if let EditorViewOp::Observe { limit } = op {
                        limit.unwrap_or(64)
                    } else {
                        64
                    };
                    app.editor
                        .mcp_state
                        .awaiting_view
                        .push((req, limit.clamp(1, 256)));
                    continue;
                }
                Err(error) => McpResponse { result: Err(error) },
            },
            McpOpKind::Scene(scene_op) => {
                if let Err(msg) = agent::check_protected_entity(&scene_op, app) {
                    McpResponse { result: Err(msg) }
                } else {
                    let call = match &scene_op {
                        SceneOp::SpawnEntity {
                            position,
                            rotation,
                            scale,
                            name,
                            primitive,
                        } => katla_agent::ToolCall {
                            id: String::new(),
                            name: "spawn_entity".into(),
                            arguments: serde_json::json!({"position": position, "rotation": rotation, "scale": scale, "name": name, "shape": primitive}),
                        },
                        SceneOp::SpawnModel {
                            path,
                            position,
                            default_animation,
                        } => katla_agent::ToolCall {
                            id: String::new(),
                            name: "spawn_model".into(),
                            arguments: serde_json::json!({"path":path,"position":position,"default_animation":default_animation}),
                        },
                        _ => katla_agent::ToolCall {
                            id: String::new(),
                            name: String::new(),
                            arguments: serde_json::Value::Null,
                        },
                    };
                    let result = if call.name == "spawn_model" {
                        agent::execute_tool_call(app, &call)
                    } else {
                        agent::execute_scene_op(app, scene_op, &call)
                    };
                    response_from_text(result)
                }
            }
            McpOpKind::Resource(resource_op) => {
                response_from_text(agent::execute_resource_op(app, resource_op))
            }
            McpOpKind::LoadScene { path } => execute_load_scene(app, &path),
            McpOpKind::SaveScene { path } => execute_save_scene(app, path.as_deref()),
        };
        let _ = req.response_tx.send(response);
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

fn response_from_text(result: String) -> McpResponse {
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(&result);
    McpResponse {
        result: parsed.map_err(|_| result).and_then(|value| {
            if value.get("success") == Some(&serde_json::Value::Bool(false)) {
                Err(value
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Scene operation failed")
                    .to_string())
            } else {
                Ok(value)
            }
        }),
    }
}

struct ViewCapture {
    request: PendingMcpRequest,
    metadata: serde_json::Value,
    image: Option<katla_gfx::TextureReadbackTicket>,
    pixels: Option<katla_gfx::TextureReadbackData>,
    picks: Vec<(String, katla_gfx::TextureReadbackTicket)>,
    entity_map: std::collections::HashMap<u32, EntityId>,
    error: Option<String>,
}

/// Queue copies before another frame can overwrite these committed graph sources.
pub(crate) fn capture_requested_view(app: &mut crate::application::Application) {
    let requests = std::mem::take(&mut app.editor.mcp_state.awaiting_view);
    for (request, limit) in requests {
        if request.response_tx.is_closed() {
            continue;
        }
        let result = queue_capture(app, request, limit);
        match result {
            Ok(capture) => app.editor.mcp_state.captures.push(capture),
            Err(failure) => {
                let (request, error) = *failure;
                let _ = request.response_tx.send(McpResponse { result: Err(error) });
            }
        }
    }
}

fn queue_capture(
    app: &mut crate::application::Application,
    request: PendingMcpRequest,
    limit: usize,
) -> Result<ViewCapture, Box<(PendingMcpRequest, String)>> {
    let Some(committed) = &app.editor_features.committed_pick else {
        return Err(Box::new((
            request,
            "No committed editor picking frame".into(),
        )));
    };
    let size = committed.size;
    let pick_source = committed.source;
    let entity_map = committed.entity_map.clone();
    let Some(source) = app
        .frame_graph_bindings
        .resources
        .viewport
        .as_deref()
        .and_then(|name| app.frame_graph.resource_id(name))
        .and_then(|id| app.renderer.graph_texture_source(id))
    else {
        return Err(Box::new((
            request,
            "Graph must export its viewport image".into(),
        )));
    };
    if source.submission != pick_source.submission {
        return Err(Box::new((
            request,
            "Viewport and picking do not belong to the same submission".into(),
        )));
    }
    let mut metadata = viewport::snapshot(app, limit);
    metadata["submission"] = serde_json::json!(source.submission);
    metadata["image_size"] = serde_json::json!([size.width, size.height]);
    let image = match app.renderer.queue_texture_readback(
        source,
        katla_gfx::TextureReadbackRegion {
            origin: [0, 0],
            size,
            mip_level: 0,
            array_layer: 0,
        },
    ) {
        Ok(ticket) => ticket,
        Err(error) => return Err(Box::new((request, error.to_string()))),
    };
    let mut capture = ViewCapture {
        request,
        metadata,
        image: Some(image),
        pixels: None,
        picks: Vec::new(),
        entity_map,
        error: None,
    };
    let bounds = app.editor.editor_ui.last_viewport_bounds;
    let mouse = app.ui_context.input().mouse_pos;
    let pointer = if bounds.contains(mouse) {
        super::super::picking::pick_pixel(
            [mouse.x() - bounds.min.x(), mouse.y() - bounds.min.y()],
            [bounds.width(), bounds.height()],
            size,
            app.renderer.capabilities().clip_y_down,
        )
    } else {
        None
    };
    capture.metadata["pointer_pixel"] = serde_json::json!(pointer);
    for (name, pixel) in [
        ("center_pick", Some([size.width / 2, size.height / 2])),
        ("pointer_pick", pointer),
    ] {
        if let Some([x, y]) = pixel {
            match app
                .renderer
                .queue_texture_readback(pick_source, katla_gfx::TextureReadbackRegion::pixel(x, y))
            {
                Ok(ticket) => capture.picks.push((name.into(), ticket)),
                Err(error) => capture.error = Some(error.to_string()),
            }
        } else {
            capture.metadata[name] = serde_json::Value::Null;
        }
    }
    Ok(capture)
}

fn poll_captures(app: &mut crate::application::Application) {
    let captures = std::mem::take(&mut app.editor.mcp_state.captures);
    for mut capture in captures {
        if let Some(ticket) = capture.image {
            match app.renderer.poll_texture_readback(ticket) {
                Ok(Some(data)) => {
                    capture.pixels = Some(data);
                    capture.image = None;
                }
                Ok(None) => {}
                Err(error) => {
                    capture.error = Some(error.to_string());
                    capture.image = None;
                }
            }
        }
        capture.picks.retain(
            |(name, ticket)| match app.renderer.poll_texture_readback(*ticket) {
                Ok(Some(data)) => {
                    let raw_id = data.single_u32();
                    capture.metadata[format!("{name}_sample")] = serde_json::json!({"encoded_object_id":raw_id,"meaning": "A mapped ID identifies a scene draw. Zero is background; an unmapped nonzero ID may be editor overlay geometry. This is a pixel helper, not user intent."});
                    capture.metadata[name] = serde_json::json!(
                        data.single_u32()
                            .and_then(|n| n.checked_sub(1))
                            .and_then(|n| capture.entity_map.get(&n))
                            .map(|id| id.id().to_string())
                    );
                    false
                }
                Ok(None) => true,
                Err(error) => {
                    capture.error = Some(error.to_string());
                    false
                }
            },
        );
        if capture.image.is_some() || !capture.picks.is_empty() {
            app.editor.mcp_state.captures.push(capture);
            continue;
        }
        let result = if let Some(error) = capture.error {
            Err(error)
        } else if let Some(data) = capture.pixels {
            encode_image(data).map(|png| {
                capture.metadata["image_png_base64"] = serde_json::json!(png);
                capture.metadata["returned_at_frame"] = serde_json::json!(app.frame_count);
                capture.metadata
            })
        } else {
            Err("Viewport readback did not return pixels".into())
        };
        let _ = capture.request.response_tx.send(McpResponse { result });
    }
}

fn encode_image(data: katla_gfx::TextureReadbackData) -> Result<String, String> {
    use base64::Engine;
    let mut pixels = data.bytes;
    match data.format {
        katla_gfx::ImageFormat::B8G8R8A8Srgb => {
            for pixel in pixels.as_chunks_mut::<4>().0 {
                pixel.swap(0, 2);
            }
        }
        katla_gfx::ImageFormat::R8G8B8A8Srgb | katla_gfx::ImageFormat::R8G8B8A8Unorm => {}
        other => return Err(format!("Unsupported viewport readback format: {other:?}")),
    }
    let mut png = Vec::new();
    let mut encoder = png::Encoder::new(&mut png, data.size.width, data.size.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    {
        let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
        writer
            .write_image_data(&pixels)
            .map_err(|e| e.to_string())?;
    }
    Ok(base64::engine::general_purpose::STANDARD.encode(png))
}
