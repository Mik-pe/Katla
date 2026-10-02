use serde_json::Value;

use super::formats::escape_dot;

mod fixtures;
use super::*;
use crate::render_graph::compiler::GraphCompiler;
use crate::render_graph::resource::ResourceState;
use crate::texture::ImageFormat;
use fixtures::*;

#[test]
fn pass_traces_expose_declared_attachment_ops() {
    let resources = vec![
        namespace_resource(BACKBUFFER_NAME),
        namespace_resource("color"),
    ];
    let transient_resources = vec![transient_resource("color")];
    let mut geometry = pass("geometry", Vec::new(), vec![ResourceId(1)]);
    geometry.color_attachments.push((
        ResourceId(1),
        crate::render_pass::AttachmentOps {
            load: crate::render_pass::LoadOp::Clear,
            store: crate::render_pass::StoreOp::Store,
            clear_value: crate::render_pass::ClearValue::OPAQUE_BLACK,
        },
    ));
    geometry.depth_attachment =
        Some(crate::render_pass::DepthStencilAttachmentOps::reverse_z_default());
    let mut present = pass("present", vec![ResourceId(1)], vec![ResourceId(0)]);
    present
        .color_attachments
        .push((ResourceId(0), crate::render_pass::AttachmentOps::load()));
    let passes = vec![geometry, present];

    let exported_resources = BTreeSet::from([ResourceId(0)]);
    let plan = GraphCompiler::from_pass_descs_with_exports(
        &passes,
        exported_resources.iter().copied(),
        BTreeMap::new(),
    )
    .compile()
    .unwrap();
    let diagnostics = RenderGraphDiagnostics::from_parts(
        &passes,
        &resources,
        &transient_resources,
        &exported_resources,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &plan,
    );

    let trace = diagnostics.to_string();
    assert!(
        trace.contains("colors [r1 Clear->Store]"),
        "geometry trace missing declared color ops: {trace}"
    );
    assert!(
        trace.contains("depth [Clear->Store, stencil Clear->DontCare]"),
        "geometry trace missing declared depth ops: {trace}"
    );
    assert!(
        trace.contains("colors [r0 Load->Store]"),
        "present trace missing declared color ops: {trace}"
    );
}

#[test]
fn diagnostics_are_deterministic_and_machine_readable() {
    let first = diagnostics();
    let expected_json = first.to_json_pretty().unwrap();
    let expected_dot = first.to_dot();

    for _ in 0..32 {
        let current = diagnostics();
        assert_eq!(current, first);
        assert_eq!(current.to_json_pretty().unwrap(), expected_json);
        assert_eq!(current.to_dot(), expected_dot);
    }

    let json: Value = serde_json::from_str(&expected_json).unwrap();
    assert_eq!(
        json["schema_version"],
        RENDER_GRAPH_DIAGNOSTICS_SCHEMA_VERSION
    );
    assert_eq!(json["execution_order"], serde_json::json!([0, 1, 2, 3]));
    assert_eq!(json["passes"][0]["image_accesses"][0]["mode"], "write");
    assert_eq!(json["passes"][1]["image_accesses"][0]["usage"], "sampled");
    assert_eq!(json["summary"]["dependency_edges"], 4);
    assert_eq!(json["summary"]["synchronization_transitions"], 10);
    assert_eq!(json["summary"]["physical_transient_allocations"], 2);
    assert_eq!(json["summary"]["logical_transient_bytes"], 65536);
    assert_eq!(json["summary"]["physical_transient_bytes"], 65536);
    assert_eq!(json["summary"]["transient_alias_savings_bytes"], 0);
    assert_eq!(json["summary"]["parallel_levels"], 4);
}

#[test]
fn diagnostics_name_every_raw_war_and_waw_hazard() {
    let diagnostics = diagnostics();
    let feedback = diagnostics
        .dependencies
        .iter()
        .find(|dependency| dependency.from_pass == 1 && dependency.to_pass == 2)
        .unwrap();

    assert_eq!(
        feedback.hazards,
        vec![
            RenderGraphDiagnosticHazard {
                kind: RenderGraphHazardKind::War,
                resource: RenderGraphDiagnosticResourceRef {
                    id: 1,
                    name: "color".to_string(),
                },
            },
            RenderGraphDiagnosticHazard {
                kind: RenderGraphHazardKind::Raw,
                resource: RenderGraphDiagnosticResourceRef {
                    id: 2,
                    name: "post".to_string(),
                },
            },
        ]
    );

    let geometry_to_feedback = diagnostics
        .dependencies
        .iter()
        .find(|dependency| dependency.from_pass == 0 && dependency.to_pass == 2)
        .unwrap();
    assert_eq!(
        geometry_to_feedback.hazards[0].kind,
        RenderGraphHazardKind::Waw
    );
}

#[test]
fn diagnostics_expose_stable_physical_allocation_ids_and_memory_totals() {
    let diagnostics = early_late_diagnostics();

    assert_eq!(diagnostics.resources[1].physical_allocation_id, Some(0));
    assert_eq!(diagnostics.resources[2].physical_allocation_id, Some(0));
    assert_eq!(diagnostics.summary.physical_transient_allocations, 1);
    assert_eq!(diagnostics.summary.logical_transient_bytes, 65536);
    assert_eq!(diagnostics.summary.physical_transient_bytes, 32768);
    assert_eq!(diagnostics.summary.transient_alias_savings_bytes, 32768);

    assert_eq!(
        diagnostics.transient_slots,
        vec![RenderGraphDiagnosticAllocationSlot {
            id: 0,
            resources: vec![
                RenderGraphDiagnosticResourceRef {
                    id: 1,
                    name: "early".to_string(),
                },
                RenderGraphDiagnosticResourceRef {
                    id: 2,
                    name: "late".to_string(),
                },
            ],
            bytes: 32768,
            logical_bytes: 65536,
            saved_bytes: 32768,
            compatibility: RenderGraphDiagnosticCompatibilityClass {
                kind: "color_attachment".to_string(),
                format: "R8G8B8A8Unorm".to_string(),
                width: 128,
                height: 64,
                tracks_swapchain_size: true,
            },
            tile_memory: RenderGraphDiagnosticTileMemory {
                eligible: false,
                reason:
                    "resource is sampled, stored, transferred, or presented outside an attachment"
                        .to_string(),
            },
            first_execution_position: 0,
            last_execution_position: 3,
        }]
    );
}

/// An early and a late transient with disjoint lifetimes sharing one
/// physical allocation slot.

#[test]
fn transient_slot_exports_render_compatibility_alias_order_and_savings() {
    let diagnostics = early_late_diagnostics();

    let text = diagnostics.to_string();
    assert!(
        text.contains(
            "slot 0 (32768 bytes, saves 32768, positions 0-3, color_attachment \
             R8G8B8A8Unorm 128x64, swapchain-tracked, tile memory: no (resource is \
             sampled, stored, transferred, or presented outside an attachment)): \
             r1 (early) -> r2 (late)"
        ),
        "text export missing aliasing slot line: {text}"
    );

    let dot = diagnostics.to_dot();
    assert!(
        dot.contains(
            "a0 [shape=cylinder,style=dashed,label=\"slot 0: 32768 bytes\\nsaves \
             32768 bytes\\ncolor_attachment R8G8B8A8Unorm 128x64 \
             swapchain-tracked\\ntile memory: no (resource is sampled, stored, \
             transferred, or presented outside an attachment)\\npositions 0-3\"];"
        ),
        "dot export missing physical allocation node: {dot}"
    );
    for member in ["r1", "r2"] {
        assert!(
            dot.contains(&format!(
                "a0 -> {member} [arrowhead=none,style=dotted,color=gray50,label=\"alias\"];"
            )),
            "dot export missing alias edge to {member}: {dot}"
        );
    }

    let json = serde_json::to_value(&diagnostics).unwrap();
    assert_eq!(
        json["transient_slots"][0]["compatibility"],
        serde_json::json!({
            "kind": "color_attachment",
            "format": "R8G8B8A8Unorm",
            "width": 128,
            "height": 64,
            "tracks_swapchain_size": true,
        })
    );
    assert_eq!(json["transient_slots"][0]["bytes"], 32768);
    assert_eq!(json["transient_slots"][0]["logical_bytes"], 65536);
    assert_eq!(json["transient_slots"][0]["saved_bytes"], 32768);
    assert_eq!(json["transient_slots"][0]["resources"][1]["name"], "late");
    assert_eq!(
        json["transient_slots"][0]["tile_memory"],
        serde_json::json!({
            "eligible": false,
            "reason": "resource is sampled, stored, transferred, or presented outside an attachment",
        })
    );
}

#[test]
fn diagnostics_expose_compiler_owned_synchronization_transitions() {
    let diagnostics = diagnostics();
    let raw = diagnostics
        .synchronization
        .iter()
        .find(|transition| {
            transition.before_pass == Some(0)
                && transition.to_pass == Some(1)
                && transition.resource.id == 1
        })
        .unwrap();

    assert_eq!(raw.hazard, Some(RenderGraphHazardKind::Raw));
    assert_eq!(raw.reason, RenderGraphDiagnosticSyncReason::Hazard);
    assert_eq!(
        raw.range,
        RenderGraphDiagnosticImageSubresourceRange {
            aspects: vec!["color".to_string()],
            base_mip_level: 0,
            mip_level_count: u32::MAX,
            base_array_layer: 0,
            array_layer_count: u32::MAX,
        }
    );
    // The fixture's coarse write infers a whole-resource storage access.
    assert_eq!(
        raw.before_state,
        RenderGraphDiagnosticSyncState::Access {
            usage: RenderGraphDiagnosticResourceAccessUsage::Storage,
            stage: RenderGraphDiagnosticImageStage::AllGraphics,
            mode: RenderGraphDiagnosticResourceAccessMode::Write,
        }
    );
    assert_eq!(
        raw.after_state,
        RenderGraphDiagnosticSyncState::Access {
            usage: RenderGraphDiagnosticResourceAccessUsage::Sampled,
            stage: RenderGraphDiagnosticImageStage::FragmentShader,
            mode: RenderGraphDiagnosticResourceAccessMode::Read,
        }
    );
    assert_eq!(raw.before_name.as_deref(), Some("geometry"));
    assert_eq!(raw.to_name.as_deref(), Some("post"));
}

#[test]
fn diagnostics_include_resource_lifetimes_and_origins() {
    let diagnostics = diagnostics();
    assert_eq!(
        diagnostics.resources[0].origin,
        RenderGraphDiagnosticResourceOrigin::BuiltIn
    );
    assert_eq!(
        diagnostics.resources[1].origin,
        RenderGraphDiagnosticResourceOrigin::Transient
    );
    assert_eq!(
        diagnostics.resources[1].lifetime,
        Some(RenderGraphDiagnosticResourceLifetime {
            first_execution_position: 0,
            first_pass: 0,
            last_execution_position: 3,
            last_pass: 3,
        })
    );
    assert_eq!(diagnostics.resources[1].width, Some(128));
    assert!(diagnostics.resources[0].exported);
    assert!(!diagnostics.resources[1].exported);
}

#[test]
fn dot_output_distinguishes_passes_resources_and_hazards() {
    let dot = diagnostics().to_dot();
    assert!(dot.starts_with("digraph render_graph"));
    assert!(dot.contains("r1 [shape=ellipse"));
    assert!(dot.contains("p0 [shape=box"));
    assert!(dot.contains("p0 -> r1 [label=\"write Storage @ AllGraphics"));
    assert!(dot.contains("r1 -> p1 [label=\"read Sampled @ FragmentShader"));
    assert!(dot.contains("Waw color"));
    assert!(dot.contains("Raw post"));
}

#[test]
fn diagnostics_expose_imported_state_contracts() {
    let resources = vec![
        namespace_resource(BACKBUFFER_NAME),
        namespace_resource("external"),
        namespace_resource("color"),
    ];
    let transient_resources = vec![transient_resource("color")];
    let passes = vec![
        pass("paint", Vec::new(), vec![ResourceId(2)]),
        pass("present", Vec::new(), vec![ResourceId(0)]),
    ];
    let exported_resources = BTreeSet::from([ResourceId(0)]);
    let imported_contracts = BTreeMap::from([
        (
            ResourceId(0),
            ImportedImageContract::arrives_in(ResourceState::ColorAttachment)
                .must_end_in(ResourceState::PresentSrc),
        ),
        (
            ResourceId(1),
            ImportedImageContract::arrives_in(ResourceState::ShaderRead),
        ),
    ]);
    let plan = GraphCompiler::from_pass_descs_with_exports(
        &passes,
        exported_resources.iter().copied(),
        BTreeMap::new(),
    )
    .compile()
    .unwrap();
    let diagnostics = RenderGraphDiagnostics::from_parts(
        &passes,
        &resources,
        &transient_resources,
        &exported_resources,
        &imported_contracts,
        &BTreeMap::new(),
        &plan,
    );

    assert_eq!(
        diagnostics.resources[0].imported_contract,
        Some(RenderGraphDiagnosticImportedContract {
            initial: "ColorAttachment".to_string(),
            required_final: Some("PresentSrc".to_string()),
        })
    );
    assert_eq!(
        diagnostics.resources[1].imported_contract,
        Some(RenderGraphDiagnosticImportedContract {
            initial: "ShaderRead".to_string(),
            required_final: None,
        })
    );
    assert_eq!(diagnostics.resources[2].imported_contract, None);

    let json = serde_json::to_value(&diagnostics).unwrap();
    assert_eq!(
        json["resources"][0]["imported_contract"]["required_final"],
        "PresentSrc"
    );

    let text = diagnostics.to_string();
    assert!(text.contains("r0 (backbuffer) imported, initial ColorAttachment"));
    assert!(text.contains("required final PresentSrc"));
    assert!(text.contains("r1 (external) imported, initial ShaderRead"));
    assert!(text.contains("required final unconstrained"));

    let dot = diagnostics.to_dot();
    assert!(dot.contains("initial ColorAttachment, final PresentSrc"));
}

#[test]
fn diagnostics_expose_live_culled_exported_and_side_effect_state() {
    let resources = vec![
        namespace_resource(BACKBUFFER_NAME),
        namespace_resource("dead"),
        namespace_resource("telemetry_input"),
    ];
    let transient_resources = vec![
        transient_resource("dead"),
        transient_resource("telemetry_input"),
    ];
    let mut telemetry = pass("telemetry", vec![ResourceId(2)], Vec::new());
    telemetry.side_effect = true;
    let passes = vec![
        pass("present", Vec::new(), vec![ResourceId(0)]),
        pass("dead_branch", Vec::new(), vec![ResourceId(1)]),
        pass("telemetry_source", Vec::new(), vec![ResourceId(2)]),
        telemetry,
    ];
    let exported_resources = BTreeSet::from([ResourceId(0)]);
    let plan = GraphCompiler::from_pass_descs_with_exports(
        &passes,
        exported_resources.iter().copied(),
        BTreeMap::new(),
    )
    .compile()
    .unwrap();
    let diagnostics = RenderGraphDiagnostics::from_parts(
        &passes,
        &resources,
        &transient_resources,
        &exported_resources,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &plan,
    );

    assert_eq!(diagnostics.summary.declared_passes, 4);
    assert_eq!(diagnostics.summary.live_passes, 3);
    assert_eq!(diagnostics.summary.culled_passes, 1);
    assert!(diagnostics.passes[0].live);
    assert!(diagnostics.passes[1].culled);
    assert_eq!(diagnostics.passes[1].execution_position, None);
    assert_eq!(diagnostics.passes[1].parallel_level, None);
    assert!(diagnostics.passes[3].side_effect);
    assert!(diagnostics.resources[0].exported);

    let dot = diagnostics.to_dot();
    assert!(dot.contains("exported"));
    assert!(dot.contains("culled"));
    assert!(dot.contains("side-effect"));
    assert!(diagnostics.to_string().contains("3 live, 1 culled"));
}

#[test]
fn test_typed_access_exports_preserve_ranges_and_culled_passes() {
    use crate::render_graph::access::{ImageAspects, ImageSubresourceRange};

    let resources = vec![namespace_resource("depth\"atlas")];
    let access = ImageAccess::storage_read_write(ResourceId(0)).with_range(
        ImageSubresourceRange::new(ImageAspects::DEPTH | ImageAspects::STENCIL, 2, 3, 4, 5),
    );
    let mut live = pass("live", vec![], vec![]).with_image_accesses([access]);
    live.side_effect = true;
    let passes = vec![
        live,
        pass("unused", vec![], vec![]).with_image_accesses([access]),
    ];
    let plan = GraphCompiler::from_pass_descs_with_exports(&passes, [], BTreeMap::new())
        .compile()
        .unwrap();
    let diagnostics = RenderGraphDiagnostics::from_parts(
        &passes,
        &resources,
        &[],
        &BTreeSet::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &plan,
    );
    let label = "read_write Storage @ AllGraphics, depth|stencil, mips 2+3, layers 4+5";
    let text = diagnostics.to_string();
    assert_eq!(text.matches(label).count(), 2);
    assert!(text.contains("[1] unused (culled)"));
    let dot = diagnostics.to_dot();
    for edge in ["r0 -> p0", "p0 -> r0"] {
        assert!(dot.contains(&format!("{edge} [label=\"{label}\"];")));
    }
    for edge in ["r0 -> p1", "p1 -> r0"] {
        assert!(dot.contains(&format!(
            "{edge} [label=\"{label}\",style=\"dotted\",color=\"gray60\"];"
        )));
    }
    assert!(dot.contains("depth\\\"atlas"));
    let json: Value = serde_json::from_str(&diagnostics.to_json_pretty().unwrap()).unwrap();
    assert_eq!(
        json["passes"][0]["image_accesses"][0]["range"],
        serde_json::json!({
            "aspects": ["depth", "stencil"],
            "base_mip_level": 2, "mip_level_count": 3,
            "base_array_layer": 4, "array_layer_count": 5,
        })
    );
    assert_eq!(diagnostics.execution_order, vec![0]);
}

#[test]
fn test_buffer_access_exports_preserve_byte_ranges() {
    use crate::render_graph::access::{
        BufferAccess, BufferByteRange, BufferUsage, ResourceAccessStage,
    };

    let resources = vec![namespace_resource("tiles")];
    let bounded = BufferAccess::new(
        ResourceId(0),
        ResourceAccessMode::ReadWrite,
        BufferUsage::Storage,
        ResourceAccessStage::ComputeShader,
        BufferByteRange::new(4096, 256),
    );
    let unbounded = BufferAccess::new(
        ResourceId(0),
        ResourceAccessMode::Read,
        BufferUsage::Storage,
        ResourceAccessStage::ComputeShader,
        BufferByteRange::from(512),
    );
    let mut live = pass("cull", vec![], vec![]).with_buffer_accesses([bounded]);
    live.side_effect = true;
    let passes = vec![
        live,
        pass("trace", vec![], vec![]).with_buffer_accesses([unbounded]),
    ];
    let plan = GraphCompiler::from_pass_descs_with_exports(&passes, [], BTreeMap::new())
        .compile()
        .unwrap();
    let diagnostics = RenderGraphDiagnostics::from_parts(
        &passes,
        &resources,
        &[],
        &BTreeSet::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &plan,
    );

    let text = diagnostics.to_string();
    assert!(
        text.contains("read_write Storage @ ComputeShader, bytes 4096+256"),
        "text export missing bounded buffer range: {text}"
    );
    assert!(
        text.contains("read Storage @ ComputeShader, bytes 512+unbounded"),
        "text export missing unbounded buffer range: {text}"
    );

    let dot = diagnostics.to_dot();
    for edge in ["r0 -> p0", "p0 -> r0"] {
        assert!(
            dot.contains(&format!(
                "{edge} [label=\"read_write Storage @ ComputeShader, bytes 4096+256\"];"
            )),
            "dot export missing buffer edge {edge}: {dot}"
        );
    }

    let json: Value = serde_json::from_str(&diagnostics.to_json_pretty().unwrap()).unwrap();
    assert_eq!(
        json["passes"][0]["buffer_accesses"][0]["range"],
        serde_json::json!({ "offset": 4096, "size": 256 })
    );
    assert_eq!(
        json["passes"][0]["buffer_accesses"][0]["usage"],
        serde_json::json!("storage")
    );
    assert_eq!(
        json["passes"][1]["buffer_accesses"][0]["range"],
        serde_json::json!({ "offset": 512, "size": u64::MAX })
    );
}

#[test]
fn test_buffer_synchronization_exports_byte_ranges_and_hazards() {
    use crate::render_graph::access::{
        BufferAccess, BufferByteRange, BufferUsage, ResourceAccessStage,
    };

    let resources = vec![namespace_resource("particles")];
    let write = BufferAccess::new(
        ResourceId(0),
        ResourceAccessMode::Write,
        BufferUsage::Storage,
        ResourceAccessStage::ComputeShader,
        BufferByteRange::new(0, 64),
    );
    let read = BufferAccess::new(
        ResourceId(0),
        ResourceAccessMode::Read,
        BufferUsage::Storage,
        ResourceAccessStage::ComputeShader,
        BufferByteRange::new(0, 64),
    );
    // Both passes are liveness roots: a buffer-only pass has no exported
    // resource to anchor it, so without a side effect culling removes it.
    let mut simulate = pass("simulate", vec![], vec![]).with_buffer_accesses([write]);
    simulate.side_effect = true;
    let mut draw = pass("draw", vec![], vec![]).with_buffer_accesses([read]);
    draw.side_effect = true;
    let passes = vec![simulate, draw];
    let plan = GraphCompiler::from_pass_descs_with_exports(&passes, [], BTreeMap::new())
        .compile()
        .unwrap();
    let diagnostics = RenderGraphDiagnostics::from_parts(
        &passes,
        &resources,
        &[],
        &BTreeSet::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &plan,
    );

    // Frame-start scopes retain the previous writer and reader, then
    // the current reader depends on this frame's writer.
    assert_eq!(diagnostics.summary.buffer_synchronization_ops, 3);
    let op = diagnostics
        .buffer_synchronization
        .iter()
        .find(|op| op.hazard.is_some())
        .expect("hazard op");
    assert_eq!(op.pass, 1);
    assert_eq!(op.pass_name, "draw");
    assert_eq!(op.before_pass, Some(0));
    assert_eq!(op.before_name.as_deref(), Some("simulate"));
    assert_eq!(op.hazard, Some(RenderGraphHazardKind::Raw));
    assert_eq!(op.range.offset, 0);
    assert_eq!(op.range.size, 64);

    let text = diagnostics.to_string();
    assert!(
        text.contains("buffer synchronization operations:"),
        "text export missing the buffer section: {text}"
    );
    assert!(
        text.contains(
            "[pass 0 (simulate) -> pass 1 (draw)] r0 (particles), bytes 0+64: write \
             Storage @ ComputeShader -> read Storage @ ComputeShader (hazard Raw)"
        ),
        "text export missing the buffer op line: {text}"
    );

    let json: Value = serde_json::from_str(&diagnostics.to_json_pretty().unwrap()).unwrap();
    let hazard_index = diagnostics
        .buffer_synchronization
        .iter()
        .position(|op| op.hazard.is_some())
        .expect("hazard op index");
    assert_eq!(
        json["buffer_synchronization"][hazard_index]["range"],
        serde_json::json!({ "offset": 0, "size": 64 })
    );
    assert_eq!(
        json["buffer_synchronization"][hazard_index]["reason"],
        "hazard"
    );
    assert_eq!(
        json["buffer_synchronization"][0]["reason"], "initial_use",
        "the writer's first use is recorded before the reader's hazard"
    );
    assert_eq!(
        json["summary"]["buffer_synchronization_ops"],
        serde_json::json!(3)
    );
}

#[test]
fn test_dot_output_escapes_unstable_user_names() {
    assert_eq!(escape_dot("a\\b\"c\nd"), "a\\\\b\\\"c\\nd");
}

/// The canonical graph the golden snapshots pin: a shadow/geometry/
/// lighting/present chain over transients, an imported backbuffer with an
/// arrival/final contract, RAW hazards, steady-state frame-start seeds,
/// and a frame-end contract operation.

#[test]
fn test_liveness_reasons_distinguish_final_writers_and_disabled_culling() {
    let resources = vec![
        namespace_resource("output"),
        namespace_resource("intermediate"),
        namespace_resource("other_output"),
    ];
    let passes = vec![
        pass("earlier", vec![], vec![ResourceId(0), ResourceId(1)]),
        pass("consumer", vec![ResourceId(1)], vec![ResourceId(2)]),
        pass("replacement", vec![], vec![ResourceId(0)]),
    ];
    let exports = BTreeSet::from([ResourceId(0), ResourceId(2)]);
    let plan = GraphCompiler::from_pass_descs_with_exports(
        &passes,
        exports.iter().copied(),
        BTreeMap::new(),
    )
    .compile()
    .unwrap();
    let diagnostics = RenderGraphDiagnostics::from_parts(
        &passes,
        &resources,
        &[],
        &exports,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &plan,
    );
    assert_eq!(diagnostics.liveness_roots, vec![1, 2]);
    assert_eq!(
        diagnostics.passes[0].liveness_reason,
        "required producer of a live pass"
    );
    assert_eq!(
        diagnostics.passes[2].liveness_reason,
        "final exported resource producer"
    );
    let plan = GraphCompiler::from_pass_descs(&passes).compile().unwrap();
    let diagnostics = RenderGraphDiagnostics::from_parts(
        &passes,
        &resources,
        &[],
        &exports,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &plan,
    );
    assert!(!diagnostics.culling_enabled);
    assert!(
        diagnostics
            .passes
            .iter()
            .all(|pass| pass.liveness_reason == "culling disabled")
    );
}

#[test]
fn test_joined_capture_goldens_and_pointer_free_identity() {
    let capture = golden_capture();
    assert!(capture.comparison.is_empty(), "{:?}", capture.comparison);
    let json = capture.to_json_pretty().unwrap();
    for forbidden in ["0x", "/Users/", "/home/", "MTLTexture", "VkImage"] {
        assert!(!json.contains(forbidden), "unstable field {forbidden}");
    }
    assert_eq!(capture, golden_capture());
    bless_or_compare("render_graph_capture.json", &format!("{json}\n"));
    bless_or_compare("render_graph_capture.text", &capture.to_string());
    bless_or_compare("render_graph_capture.dot", &capture.to_dot());
}

#[test]
fn test_capture_reports_missing_extra_and_changed_synchronization() {
    let mut capture = golden_capture();
    capture.backend_execution.synchronization.clear();
    assert!(
        capture
            .compare_native()
            .iter()
            .any(|failure| failure.contains("missing native synchronization"))
    );
    capture = golden_capture();
    capture.backend_execution.synchronization[0].destination = "different access".into();
    let report = capture.compare_native();
    assert!(
        report
            .iter()
            .any(|failure| failure.contains("missing native synchronization"))
    );
    assert!(
        report
            .iter()
            .any(|failure| failure.contains("unexpected native synchronization"))
    );
    capture = golden_capture();
    capture
        .backend_execution
        .synchronization
        .push(capture.backend_execution.synchronization[0].clone());
    assert!(
        capture
            .compare_native()
            .iter()
            .any(|failure| failure.contains("unexpected native synchronization"))
    );
}

#[test]
fn test_capture_reports_order_resources_and_unexplained_omissions() {
    let mut capture = golden_capture();
    capture.backend_execution.encoders.swap(1, 2);
    assert!(
        capture
            .compare_native()
            .iter()
            .any(|failure| failure.contains("native pass order differs"))
    );
    capture = golden_capture();
    capture.backend_execution.encoders[1].resources.push(999);
    assert!(
        capture
            .compare_native()
            .iter()
            .any(|failure| failure.contains("undeclared resource r999"))
    );
    capture = golden_capture();
    capture.backend_execution.synchronization[0].emitted = false;
    assert!(
        capture
            .compare_native()
            .iter()
            .any(|failure| failure.contains("omitted without a reason"))
    );
    capture.backend_execution.synchronization[0].omission_reason =
        Some("same-encoder order is sufficient".into());
    assert!(capture.compare_native().is_empty());
}

#[test]
fn test_capture_reports_missing_native_attachments_and_frame_owner() {
    let mut capture = golden_capture();
    capture.backend_execution.encoders[1].resources.clear();
    assert!(
        capture
            .compare_native()
            .iter()
            .any(|failure| failure.contains("did not bind attachment"))
    );
    capture = golden_capture();
    capture.backend_execution.encoders[1].kind =
        super::super::capture::CapturedEncoderKind::Compute;
    assert!(
        capture
            .compare_native()
            .iter()
            .any(|failure| failure.contains("uses compute, compiled"))
    );
    capture = golden_capture();
    capture.backend_execution.frame = None;
    assert!(
        capture
            .compare_native()
            .iter()
            .any(|failure| failure.contains("no frame/submission owner"))
    );
}

#[test]
fn test_capture_reports_wrong_native_synchronization_scope() {
    let mut capture = golden_capture();
    capture.backend_execution.synchronization[0].native_scope[0].destination_stages = 0;
    assert!(
        capture
            .compare_native()
            .iter()
            .any(|failure| failure.contains("SyncScopeMismatch"))
    );
    capture = golden_capture();
    capture.backend_execution.synchronization[0].native_scope[0].source_access = 0;
    assert!(
        capture
            .compare_native()
            .iter()
            .any(|failure| failure.contains("SyncScopeMismatch"))
    );
}

#[test]
fn test_capture_feedback_preserves_pending_completed_and_failed_identity() {
    let mut capture = golden_capture();
    for feedback in [
        super::super::capture::CapturedFeedback::Pending,
        super::super::capture::CapturedFeedback::Completed,
        super::super::capture::CapturedFeedback::Failed,
    ] {
        capture.backend_execution.frame.as_mut().unwrap().feedback = feedback;
        let json = capture.to_json_pretty().unwrap();
        assert!(json.contains("slot.2.generation.9"));
        assert!(json.contains(&format!("{:?}", feedback).to_lowercase()));
    }
}

/// Directory failed render-graph tests leave their actual exports in, so CI can
/// upload them as an artifact. Honors `CARGO_TARGET_DIR` like cargo itself.

#[test]
fn test_golden_snapshots_pin_the_canonical_exports() {
    let diagnostics = golden_diagnostics();

    assert!(!diagnostics.synchronization.is_empty());
    assert!(
        diagnostics
            .synchronization
            .iter()
            .any(|transition| transition.reason == RenderGraphDiagnosticSyncReason::ImportedFinal)
    );
    assert!(
        diagnostics
            .synchronization
            .iter()
            .any(|transition| transition.before_pass.is_none())
    );

    let json = diagnostics.to_json_pretty().unwrap();
    let text = diagnostics.to_string();
    let dot = diagnostics.to_dot();

    bless_or_compare("render_graph_diagnostics.json", &json);
    bless_or_compare("render_graph_diagnostics.text", &text);
    bless_or_compare("render_graph_diagnostics.dot", &dot);
}
