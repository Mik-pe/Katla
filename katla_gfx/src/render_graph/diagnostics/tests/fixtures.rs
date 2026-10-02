use super::*;

pub(super) fn namespace_resource(name: &str) -> GraphResourceDesc {
    GraphResourceDesc {
        name: name.to_string(),
        resource_type: GraphResourceType::SampledImage,
        format: ImageFormat::R8G8B8A8Unorm,
        width: 0,
        height: 0,
        tracks_swapchain_size: false,
    }
}

pub(super) fn transient_resource(name: &str) -> GraphResourceDesc {
    GraphResourceDesc {
        name: name.to_string(),
        resource_type: GraphResourceType::ColorAttachment { clear_value: None },
        format: ImageFormat::R8G8B8A8Unorm,
        width: 128,
        height: 64,
        tracks_swapchain_size: true,
    }
}

pub(super) fn pass(name: &str, reads: Vec<ResourceId>, writes: Vec<ResourceId>) -> PassDesc {
    PassDesc::new(name, PassType::Graphics, reads, writes)
}

pub(super) fn diagnostics() -> RenderGraphDiagnostics {
    let resources = vec![
        namespace_resource(BACKBUFFER_NAME),
        namespace_resource("color"),
        namespace_resource("post"),
    ];
    let transient_resources = vec![transient_resource("color"), transient_resource("post")];
    let passes = vec![
        pass("geometry", Vec::new(), vec![ResourceId(1)]),
        pass("post", vec![ResourceId(1)], vec![ResourceId(2)]),
        pass("feedback", vec![ResourceId(2)], vec![ResourceId(1)]),
        pass("present", vec![ResourceId(1)], vec![ResourceId(0)]),
    ];
    let exported_resources = BTreeSet::from([ResourceId(0)]);
    let plan = GraphCompiler::from_pass_descs_with_exports(
        &passes,
        exported_resources.iter().copied(),
        BTreeMap::new(),
    )
    .compile()
    .unwrap();
    RenderGraphDiagnostics::from_parts(
        &passes,
        &resources,
        &transient_resources,
        &exported_resources,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &plan,
    )
}

pub(super) fn early_late_diagnostics() -> RenderGraphDiagnostics {
    let resources = vec![
        namespace_resource(BACKBUFFER_NAME),
        transient_resource("early"),
        transient_resource("late"),
    ];
    let transient_resources = vec![transient_resource("early"), transient_resource("late")];
    let passes = vec![
        pass("write_early", Vec::new(), vec![ResourceId(1)]),
        pass("consume_early", vec![ResourceId(1)], vec![ResourceId(0)]),
        pass("write_late", vec![ResourceId(0)], vec![ResourceId(2)]),
        pass("consume_late", vec![ResourceId(2)], vec![ResourceId(0)]),
    ];
    let exported_resources = BTreeSet::from([ResourceId(0)]);
    let plan = GraphCompiler::from_pass_descs_with_exports(
        &passes,
        exported_resources.iter().copied(),
        BTreeMap::new(),
    )
    .compile()
    .unwrap();
    RenderGraphDiagnostics::from_parts(
        &passes,
        &resources,
        &transient_resources,
        &exported_resources,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &plan,
    )
}

pub(super) fn golden_graph_parts() -> (RenderGraphDiagnostics, ExecutionPlan) {
    let resources = vec![
        namespace_resource(BACKBUFFER_NAME),
        namespace_resource("shadow_atlas"),
        namespace_resource("gbuffer"),
        namespace_resource("hdr_color"),
        namespace_resource("indirect_commands"),
        namespace_resource("frame_constants"),
        namespace_resource("unused_scratch"),
    ];
    let transient_resources = vec![
        transient_resource("shadow_atlas"),
        transient_resource("gbuffer"),
        transient_resource("hdr_color"),
    ];
    let mut passes = vec![
        pass("shadow", Vec::new(), vec![ResourceId(1)]),
        pass("geometry", vec![ResourceId(1)], vec![ResourceId(2)])
            .with_buffer_accesses([BufferAccess::uniform_read(ResourceId(5))]),
        pass("lighting", vec![ResourceId(2)], vec![ResourceId(3)])
            .with_buffer_accesses([BufferAccess::storage_write(ResourceId(4))
                .with_range(crate::render_graph::BufferByteRange::new(0, 64))]),
        pass("present", vec![ResourceId(3)], vec![ResourceId(0)])
            .with_buffer_accesses([BufferAccess::indirect_read(ResourceId(4))
                .with_range(crate::render_graph::BufferByteRange::new(0, 64))]),
        pass("unused_compute", vec![], vec![])
            .with_buffer_accesses([BufferAccess::storage_write(ResourceId(6))]),
    ];
    for pass in passes.iter_mut().take(4) {
        let target = pass
            .image_accesses
            .iter()
            .find(|access| access.mode.writes())
            .unwrap()
            .resource;
        pass.color_attachments.push((
            target,
            crate::render_pass::AttachmentOps::clear(crate::render_pass::ClearValue::OPAQUE_BLACK),
        ));
        pass.refine_inferred_image_accesses();
    }
    let buffers = BTreeMap::from([
        (
            ResourceId(4),
            BufferDiagnosticResource {
                descriptor: BufferDesc::new(
                    128,
                    BufferUsages::STORAGE | BufferUsages::INDIRECT,
                    BufferMemoryPolicy::DeviceLocal,
                ),
                origin: RenderGraphDiagnosticResourceOrigin::Transient,
            },
        ),
        (
            ResourceId(5),
            BufferDiagnosticResource {
                descriptor: BufferDesc::new(
                    256,
                    BufferUsages::UNIFORM,
                    BufferMemoryPolicy::CpuVisible,
                ),
                origin: RenderGraphDiagnosticResourceOrigin::Imported,
            },
        ),
        (
            ResourceId(6),
            BufferDiagnosticResource {
                descriptor: BufferDesc::new(
                    512,
                    BufferUsages::STORAGE,
                    BufferMemoryPolicy::DeviceLocal,
                ),
                origin: RenderGraphDiagnosticResourceOrigin::Transient,
            },
        ),
    ]);
    let exported_resources = BTreeSet::from([ResourceId(0)]);
    let imported_contracts = BTreeMap::from([(
        ResourceId(0),
        ImportedImageContract::arrives_in(ResourceState::ColorAttachment)
            .must_end_in(ResourceState::PresentSrc),
    )]);
    let plan = GraphCompiler::from_pass_descs_with_exports(
        &passes,
        exported_resources.iter().copied(),
        imported_contracts.clone(),
    )
    .compile()
    .unwrap();
    let diagnostics = RenderGraphDiagnostics::from_parts(
        &passes,
        &resources,
        &transient_resources,
        &exported_resources,
        &imported_contracts,
        &buffers,
        &plan,
    );
    (diagnostics, plan)
}

pub(super) fn golden_diagnostics() -> RenderGraphDiagnostics {
    golden_graph_parts().0
}

pub(super) fn golden_capture() -> crate::render_graph::capture::RenderGraphCapture {
    use crate::render_graph::capture::*;
    use crate::render_graph::trace::{
        EmittedPassOutcome, ResourceExecutionTrace, ResourceExecutionTraceEntry,
    };
    let (graph, plan) = golden_graph_parts();
    let mut trace = ResourceExecutionTrace::new();
    trace.backend = BackendExecutionTrace {
        backend: "fixture".into(),
        frame: Some(CapturedSubmission {
            frame_slot: 2,
            generation: 9,
            command_allocator: 2,
            feedback_identity: "slot.2.generation.9".into(),
            feedback: CapturedFeedback::Pending,
        }),
        bindings: vec![CapturedBindingSet {
            identity: 0,
            layout_identity: "vertex:buffer[0,1];fragment:texture[0];sampler[0]".into(),
            residency_members: vec![
                CapturedResidentResource {
                    set_identity: 0,
                    estimated_bytes: 256,
                    kind: "buffer".into(),
                    ordinal: 0,
                },
                CapturedResidentResource {
                    set_identity: 0,
                    estimated_bytes: 256,
                    kind: "texture".into(),
                    ordinal: 1,
                },
            ],
            snapshot_generation: Some(4),
        }],
        ..Default::default()
    };
    trace.backend.encoders.push(CapturedEncoder {
        ordinal: 0,
        pass_index: None,
        label: "upload batch".into(),
        kind: CapturedEncoderKind::Blit,
        resources: vec![],
    });
    for pass in graph.passes.iter().filter(|pass| pass.live) {
        trace.push(ResourceExecutionTraceEntry {
            pass_index: pass.index,
            name: pass.name.clone(),
            pass_type: PassType::Graphics,
            encode_position: pass.execution_position.unwrap(),
            outcome: EmittedPassOutcome::Encoded,
            draw_calls: 1,
            instances: 1,
            color_targets: pass
                .color_attachments
                .iter()
                .map(|attachment| graph.resources[attachment.resource as usize].name.clone())
                .collect(),
            depth_target: None,
            color_attachment_ops: vec![crate::render_pass::AttachmentOps::clear(
                crate::render_pass::ClearValue::OPAQUE_BLACK,
            )],
            depth_attachment_ops: None,
        });
        trace.backend.encoders.push(CapturedEncoder {
            ordinal: trace.backend.encoders.len(),
            pass_index: Some(pass.index),
            label: pass.name.clone(),
            kind: CapturedEncoderKind::Render,
            resources: pass
                .image_accesses
                .iter()
                .map(|access| access.resource.id)
                .chain(pass.buffer_accesses.iter().map(|access| access.resource.id))
                .collect(),
        });
    }
    trace.backend.encoders.push(CapturedEncoder {
        ordinal: trace.backend.encoders.len(),
        pass_index: None,
        label: "auxiliary compute".into(),
        kind: CapturedEncoderKind::Compute,
        resources: vec![],
    });
    let mut planned = Vec::new();
    for &pass in &plan.sorted_passes {
        for operation in &plan.sync.pass_ops[pass] {
            planned.push(CapturedSyncOperation::image(
                operation,
                "compiled pass boundary",
            ));
        }
        for operation in &plan.sync.pass_buffer_ops[pass] {
            planned.push(CapturedSyncOperation::buffer(
                operation,
                "compiled pass boundary",
            ));
        }
    }
    for operation in &plan.sync.final_ops {
        planned.push(CapturedSyncOperation::image(operation, "frame_end"));
    }
    trace.backend.synchronization = planned
        .iter()
        .cloned()
        .map(|mut operation| {
            operation.boundary = "fixture native boundary".into();
            let scope = CapturedNativeSyncScope {
                range: CapturedNativeSyncRange::Global,
                source_stages: 4,
                destination_stages: 8,
                source_access: 2,
                destination_access: 1,
                old_layout: None,
                new_layout: None,
                visibility: 0,
            };
            operation.native_scope = vec![scope.clone()];
            operation.required_native_scope = vec![scope];
            operation
        })
        .collect();
    RenderGraphCapture::join(graph, planned, &trace, vec![])
}

pub(super) fn diagnostics_artifact_dir() -> std::path::PathBuf {
    let target_dir = std::env::var_os("CARGO_TARGET_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join("target")
        });
    target_dir.join("render-graph-diagnostics")
}

pub(super) fn bless_or_compare(name: &str, actual: &str) {
    let golden_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/goldens");
    let path = format!("{golden_dir}/{name}");
    if std::env::var_os("KATLA_BLESS_GOLDENS").is_some() {
        std::fs::create_dir_all(golden_dir).expect("create goldens directory");
        std::fs::write(&path, actual).expect("write golden snapshot");
        println!("blessed golden snapshot {path}");
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!("missing golden snapshot {path} ({error}); run the test suite once with KATLA_BLESS_GOLDENS=1 to write it")
    });
    if expected != actual {
        let artifact_dir = diagnostics_artifact_dir();
        if std::fs::create_dir_all(&artifact_dir).is_ok() {
            let _ = std::fs::write(artifact_dir.join(name), format!("{actual}\n"));
            println!(
                "wrote actual export for comparison to {}",
                artifact_dir.join(name).display()
            );
        }
    }
    assert_eq!(
        expected,
        actual,
        "golden snapshot {name} drifted; the actual export is under {}/ and CI uploads it on failure; rerun with KATLA_BLESS_GOLDENS=1 only if the change is intentional",
        diagnostics_artifact_dir().display()
    );
}
