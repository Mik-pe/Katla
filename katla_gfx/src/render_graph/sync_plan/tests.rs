use super::*;
use crate::render_graph::access::{
    ImageAspects, ImageSubresourceRange, ResourceAccessMode, ResourceAccessStage,
    ResourceAccessUsage,
};
use crate::render_graph::compiler::{ExecutionPlan, GraphCompiler, PassInfo};
use crate::render_graph::handles::ResourceId;
use crate::render_graph::pass::{PassDesc, PassType};

fn rid(n: u32) -> ResourceId {
    ResourceId(n)
}

fn buf(resource: ResourceId, mode: ResourceAccessMode, offset: u64, size: u64) -> BufferAccess {
    BufferAccess::new(
        resource,
        mode,
        BufferUsage::Storage,
        ResourceAccessStage::ComputeShader,
        BufferByteRange::new(offset, size),
    )
}

fn buffer_pass(name: &str, accesses: Vec<BufferAccess>) -> PassInfo {
    let (reads, writes) = accesses.iter().fold(
        (Vec::new(), Vec::new()),
        |(mut reads, mut writes), access| {
            if access.mode.reads() {
                reads.push(access.resource);
            }
            if access.mode.writes() {
                writes.push(access.resource);
            }
            (reads, writes)
        },
    );
    PassInfo {
        name: name.to_string(),
        operation: PassType::Graphics,
        reads,
        writes,
        image_accesses: Vec::new(),
        buffer_accesses: accesses,
        attachment_ops: Vec::new(),
        side_effect: false,
    }
}

fn compile(passes: Vec<PassInfo>) -> ExecutionPlan {
    GraphCompiler::new(passes).compile().unwrap()
}

fn compile_with_contracts(
    passes: Vec<PassInfo>,
    contracts: BTreeMap<ResourceId, ImportedImageContract>,
) -> ExecutionPlan {
    GraphCompiler {
        imported_contracts: contracts,
        ..GraphCompiler::new(passes)
    }
    .compile()
    .unwrap()
}

fn access(
    resource: ResourceId,
    mode: ResourceAccessMode,
    usage: ResourceAccessUsage,
    stage: ResourceAccessStage,
    range: ImageSubresourceRange,
) -> ImageAccess {
    ImageAccess::new(resource, mode, usage, stage, range)
}

fn pass(name: &str, accesses: Vec<ImageAccess>) -> PassInfo {
    let (reads, writes) = accesses.iter().fold(
        (Vec::new(), Vec::new()),
        |(mut reads, mut writes), access| {
            if access.mode.reads() && !reads.contains(&access.resource) {
                reads.push(access.resource);
            }
            if access.mode.writes() && !writes.contains(&access.resource) {
                writes.push(access.resource);
            }
            (reads, writes)
        },
    );
    PassInfo {
        name: name.to_string(),
        operation: PassType::Graphics,
        reads,
        writes,
        image_accesses: accesses,
        buffer_accesses: Vec::new(),
        attachment_ops: Vec::new(),
        side_effect: false,
    }
}

fn state(
    usage: ResourceAccessUsage,
    stage: ResourceAccessStage,
    mode: ResourceAccessMode,
) -> ImageSyncState {
    ImageSyncState::Access { usage, stage, mode }
}

fn attachment_write(resource: ResourceId) -> ImageAccess {
    access(
        resource,
        ResourceAccessMode::Write,
        ResourceAccessUsage::ColorAttachment,
        ResourceAccessStage::ColorAttachmentOutput,
        ImageSubresourceRange::WHOLE_COLOR,
    )
}

fn sampled_read(resource: ResourceId) -> ImageAccess {
    ImageAccess::sampled_read(resource)
}

#[test]
fn attachment_to_sampled_orders_the_consuming_pass() {
    let plan = compile(vec![
        pass("shadow", vec![ImageAccess::depth_attachment_write(rid(0))]),
        pass("geometry", vec![sampled_read(rid(0))]),
    ]);

    // The whole-resource read also bootstraps aspect fragments the
    // depth write never covered (dropped at realization by the image's
    // real aspects); the depth-aspect range carries the RAW hazard.
    let ops: Vec<_> = plan.sync.pass_ops[1]
        .iter()
        .filter(|op| op.range == ImageSubresourceRange::WHOLE_DEPTH_STENCIL)
        .collect();
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].before_pass, Some(0));
    assert_eq!(
        ops[0].reason,
        SyncReason::Hazard(ResourceHazardKind::ReadAfterWrite)
    );
    assert_eq!(
        ops[0].before,
        state(
            ResourceAccessUsage::DepthStencilAttachment,
            ResourceAccessStage::DepthStencil,
            ResourceAccessMode::Write,
        )
    );
    assert_eq!(
        ops[0].after,
        state(
            ResourceAccessUsage::Sampled,
            ResourceAccessStage::FragmentShader,
            ResourceAccessMode::Read,
        )
    );
}

#[test]
fn fresh_textures_get_a_bootstrap_operation() {
    // A transient written by exactly one pass needs no steady-state
    // transition, but the first frame after creation must still leave
    // the undefined layout: the plan carries a discard-safe bootstrap
    // operation the backend coalesces once the tracked layout matches.
    let plan = compile(vec![pass("object_id", vec![attachment_write(rid(0))])]);

    let ops = &plan.sync.pass_ops[0];
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].before, ImageSyncState::Undefined);
    assert_eq!(ops[0].before_pass, None);
    assert_eq!(ops[0].reason, SyncReason::InitialUse);
    assert_eq!(
        ops[0].after,
        state(
            ResourceAccessUsage::ColorAttachment,
            ResourceAccessStage::ColorAttachmentOutput,
            ResourceAccessMode::Write,
        )
    );
}

#[test]
fn transfer_write_then_transfer_read_transitions_transfer_states() {
    let plan = compile(vec![
        pass("upload", vec![ImageAccess::transfer_write(rid(0))]),
        pass("readback", vec![ImageAccess::transfer_read(rid(0))]),
    ]);

    let ops = &plan.sync.pass_ops[1];
    assert_eq!(ops.len(), 1);
    assert_eq!(
        ops[0].before,
        state(
            ResourceAccessUsage::TransferDestination,
            ResourceAccessStage::Transfer,
            ResourceAccessMode::Write,
        )
    );
    assert_eq!(
        ops[0].after,
        state(
            ResourceAccessUsage::TransferSource,
            ResourceAccessStage::Transfer,
            ResourceAccessMode::Read,
        )
    );
}

#[test]
fn storage_read_write_after_write_is_a_raw_hazard() {
    let plan = compile(vec![
        pass("writer", vec![ImageAccess::storage_write(rid(0))]),
        pass("blender", vec![ImageAccess::storage_read_write(rid(0))]),
    ]);

    let ops = &plan.sync.pass_ops[1];
    assert_eq!(ops.len(), 1);
    assert_eq!(
        ops[0].reason,
        SyncReason::Hazard(ResourceHazardKind::ReadAfterWrite)
    );
}

#[test]
fn same_state_accesses_across_passes_keep_an_ordering_op() {
    let plan = compile(vec![
        pass("clear_a", vec![attachment_write(rid(0))]),
        pass("clear_b", vec![attachment_write(rid(0))]),
    ]);

    // Same-state WAW still needs an execution/memory barrier between the
    // render-pass instances; the layout does not change.
    let ops = &plan.sync.pass_ops[1];
    assert_eq!(ops.len(), 1);
    assert_eq!(
        ops[0].reason,
        SyncReason::Hazard(ResourceHazardKind::WriteAfterWrite)
    );
    assert_eq!(ops[0].before, ops[0].after);
}

#[test]
fn consecutive_same_state_reads_are_coalesced_away() {
    let plan = compile(vec![
        pass("write", vec![attachment_write(rid(0))]),
        pass("read_a", vec![sampled_read(rid(0))]),
        pass("read_b", vec![sampled_read(rid(0))]),
    ]);

    // read-after-read needs no ordering; only the write→read boundary
    // transitions (plus aspect-fragment bootstraps, realized only when
    // the image carries those aspects).
    assert!(plan.sync.pass_ops[1].iter().all(|op| op.reason
        == SyncReason::Hazard(ResourceHazardKind::ReadAfterWrite)
        || op.reason == SyncReason::InitialUse));
    assert!(plan.sync.pass_ops[2].is_empty());
}

#[test]
fn subresource_ranges_produce_ranged_operations() {
    let mip0 = ImageSubresourceRange::new(ImageAspects::COLOR, 0, 1, 0, 1);
    let mip1 = ImageSubresourceRange::new(ImageAspects::COLOR, 1, 1, 0, 1);

    let plan = compile(vec![
        pass(
            "write_mip0",
            vec![attachment_write(rid(0)).with_range(mip0)],
        ),
        pass(
            "write_mip1",
            vec![attachment_write(rid(0)).with_range(mip1)],
        ),
        pass("sample_all", vec![sampled_read(rid(0))]),
    ]);

    let raws: Vec<_> = plan.sync.pass_ops[2]
        .iter()
        .filter(|op| op.reason == SyncReason::Hazard(ResourceHazardKind::ReadAfterWrite))
        .collect();
    assert_eq!(raws.len(), 2);
    assert_eq!(raws[0].range, mip0);
    assert_eq!(raws[0].before_pass, Some(0));
    assert_eq!(raws[1].range, mip1);
    assert_eq!(raws[1].before_pass, Some(1));
}

#[test]
fn steady_state_cycle_transitions_from_the_previous_frame_end_state() {
    let plan = compile(vec![
        pass("write", vec![attachment_write(rid(0))]),
        pass("read", vec![sampled_read(rid(0))]),
    ]);

    // The write must first undo the previous frame's final sampled state.
    let ops = &plan.sync.pass_ops[0];
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].before_pass, None);
    assert_eq!(ops[0].reason, SyncReason::InitialUse);
    assert_eq!(
        ops[0].before,
        state(
            ResourceAccessUsage::Sampled,
            ResourceAccessStage::FragmentShader,
            ResourceAccessMode::Read,
        )
    );
}

#[test]
fn imported_initial_contract_seeds_the_first_access() {
    let mut contracts = BTreeMap::new();
    contracts.insert(
        rid(0),
        ImportedImageContract::arrives_in(ResourceState::TransferDst),
    );
    let plan = compile_with_contracts(vec![pass("sample", vec![sampled_read(rid(0))])], contracts);

    let ops = &plan.sync.pass_ops[0];
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].before_pass, None);
    assert_eq!(ops[0].reason, SyncReason::InitialUse);
    assert_eq!(
        ops[0].before,
        state(
            ResourceAccessUsage::TransferDestination,
            ResourceAccessStage::Transfer,
            ResourceAccessMode::Write,
        )
    );
}

#[test]
fn imported_final_contract_appends_a_frame_end_operation() {
    let mut contracts = BTreeMap::new();
    contracts.insert(
        rid(0),
        ImportedImageContract::undefined().must_end_in(ResourceState::PresentSrc),
    );
    let plan = compile_with_contracts(vec![pass("ui", vec![attachment_write(rid(0))])], contracts);

    assert_eq!(plan.sync.final_ops.len(), 1);
    let op = plan.sync.final_ops[0];
    assert_eq!(op.before_pass, Some(0));
    assert_eq!(op.reason, SyncReason::ImportedFinal);
    assert_eq!(
        op.after,
        state(
            ResourceAccessUsage::Present,
            ResourceAccessStage::Present,
            ResourceAccessMode::Write
        )
    );
}

#[test]
fn satisfied_final_contract_emits_nothing() {
    let mut contracts = BTreeMap::new();
    contracts.insert(
        rid(0),
        ImportedImageContract::arrives_in(ResourceState::ColorAttachment),
    );
    let plan = compile_with_contracts(vec![pass("ui", vec![attachment_write(rid(0))])], contracts);
    assert!(plan.sync.final_ops.is_empty());
}

#[test]
fn coarse_pass_desc_accesses_drive_the_plan() {
    // PassDesc without explicit accesses infers whole-resource ones; the
    // plan must be derived from the same accesses as the DAG.
    let passes = vec![
        PassDesc::new("write", PassType::Graphics, vec![], vec![rid(0)]),
        PassDesc::new("read", PassType::Graphics, vec![rid(0)], vec![]),
    ];
    let plan = GraphCompiler::from_pass_descs(&passes).compile().unwrap();
    assert_eq!(plan.sync.pass_ops[0].len(), 1);
    assert!(
        plan.sync.pass_ops[1]
            .iter()
            .any(|op| op.reason == SyncReason::Hazard(ResourceHazardKind::ReadAfterWrite))
    );
}
// --- Buffer synchronization ops (#31) ---

#[test]
fn buffer_raw_hazard_emits_one_op_over_the_overlap() {
    let passes = vec![
        buffer_pass("write", vec![buf(rid(0), ResourceAccessMode::Write, 0, 64)]),
        buffer_pass("read", vec![buf(rid(0), ResourceAccessMode::Read, 32, 64)]),
    ];
    let plan = compile(passes);

    let ops = &plan.sync.pass_buffer_ops[1];
    // The untouched tail already has read visibility from the prior
    // frame; only the newly written intersection requires ordering.
    assert_eq!(ops.len(), 1);

    let hazard = ops
        .iter()
        .find(|op| op.reason == SyncReason::Hazard(ResourceHazardKind::ReadAfterWrite))
        .expect("RAW hazard op");
    assert_eq!(hazard.resource, rid(0));
    assert_eq!(hazard.range, BufferByteRange::new(32, 32));
    assert_eq!(hazard.before_pass, Some(0));
}

#[test]
fn disjoint_buffer_ranges_emit_no_ops() {
    let passes = vec![
        buffer_pass("write", vec![buf(rid(0), ResourceAccessMode::Write, 0, 64)]),
        buffer_pass("read", vec![buf(rid(0), ResourceAccessMode::Read, 64, 64)]),
    ];
    let plan = compile(passes);

    // The second pass's range is disjoint from the first's, so it is a
    // first use (`initial_use`), never a hazard against the writer.
    assert!(
        plan.sync.pass_buffer_ops[1]
            .iter()
            .all(|op| op.reason == SyncReason::InitialUse)
    );
}

#[test]
fn buffer_war_and_waw_hazards_are_named() {
    let war = compile(vec![
        buffer_pass("read", vec![buf(rid(0), ResourceAccessMode::Read, 0, 64)]),
        buffer_pass("write", vec![buf(rid(0), ResourceAccessMode::Write, 0, 64)]),
    ]);
    assert_eq!(
        war.sync.pass_buffer_ops[1][0].reason,
        SyncReason::Hazard(ResourceHazardKind::WriteAfterRead)
    );

    let waw = compile(vec![
        buffer_pass("w1", vec![buf(rid(0), ResourceAccessMode::Write, 0, 64)]),
        buffer_pass("w2", vec![buf(rid(0), ResourceAccessMode::Write, 0, 64)]),
    ]);
    assert_eq!(
        waw.sync.pass_buffer_ops[1][0].reason,
        SyncReason::Hazard(ResourceHazardKind::WriteAfterWrite)
    );
}

#[test]
fn test_first_buffer_write_orders_the_prior_frame_state() {
    let passes = vec![buffer_pass(
        "only",
        vec![buf(rid(0), ResourceAccessMode::Write, 0, 64)],
    )];
    let plan = compile(passes);

    let ops = &plan.sync.pass_buffer_ops[0];
    assert_eq!(ops.len(), 1);
    assert_eq!(
        ops[0].before,
        BufferSyncState::of_access(&buf(rid(0), ResourceAccessMode::Write, 0, 64))
    );
    assert_eq!(ops[0].reason, SyncReason::InitialUse);
    assert_eq!(ops[0].before_pass, None);
}

#[test]
fn same_state_buffer_accesses_without_a_hazard_emit_nothing() {
    // Two reads of the same bytes in the same state: no ordering, no op.
    let passes = vec![
        buffer_pass("r1", vec![buf(rid(0), ResourceAccessMode::Read, 0, 64)]),
        buffer_pass("r2", vec![buf(rid(0), ResourceAccessMode::Read, 0, 64)]),
    ];
    let plan = compile(passes);

    assert!(plan.sync.pass_buffer_ops[1].is_empty());
}

#[test]
fn a_partial_rewrite_leaves_the_untouched_bytes_unordered() {
    // w1 writes 0..64; w2 rewrites 0..32 and reads 32..64. Only the
    // overlapping 0..32 is a hazard; the rest is unchanged.
    let passes = vec![
        buffer_pass("w1", vec![buf(rid(0), ResourceAccessMode::Write, 0, 64)]),
        buffer_pass("w2", vec![buf(rid(0), ResourceAccessMode::Write, 0, 32)]),
    ];
    let plan = compile(passes);

    let ops = &plan.sync.pass_buffer_ops[1];
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].range, BufferByteRange::new(0, 32));
}

#[test]
fn buffer_ops_are_per_pass_and_empty_for_untouched_passes() {
    let passes = vec![
        buffer_pass("w", vec![buf(rid(0), ResourceAccessMode::Write, 0, 16)]),
        buffer_pass(
            "unrelated",
            vec![buf(rid(1), ResourceAccessMode::Write, 0, 16)],
        ),
    ];
    let plan = compile(passes);

    assert_eq!(plan.sync.pass_buffer_ops[0].len(), 1);
    assert_eq!(plan.sync.pass_buffer_ops[0][0].resource, rid(0));
    assert_eq!(plan.sync.pass_buffer_ops[1].len(), 1);
    assert_eq!(plan.sync.pass_buffer_ops[1][0].resource, rid(1));
}

#[test]
fn buffer_sync_ops_do_not_disturb_the_image_plan() {
    let image = pass(
        "image",
        vec![access(
            rid(0),
            ResourceAccessMode::Write,
            ResourceAccessUsage::Storage,
            ResourceAccessStage::ComputeShader,
            ImageSubresourceRange::WHOLE_COLOR,
        )],
    );
    let buffer = buffer_pass(
        "buffer",
        vec![buf(rid(0), ResourceAccessMode::Write, 0, 16)],
    );
    let plan = compile(vec![image, buffer]);

    // Pass 0 is the image pass and pass 1 the buffer pass, so each has
    // exactly one operation in its own list and none in the other.
    assert_eq!(plan.sync.pass_ops[0].len(), 1);
    assert!(plan.sync.pass_buffer_ops[0].is_empty());
    assert!(plan.sync.pass_ops[1].is_empty());
    assert_eq!(plan.sync.pass_buffer_ops[1].len(), 1);
}
#[test]
fn test_independent_image_readers_retain_the_writer_scope() {
    let producer = ImageAccess::storage_write(rid(0));
    let fragment = ImageAccess::new(
        rid(0),
        ResourceAccessMode::Read,
        ResourceAccessUsage::Storage,
        ResourceAccessStage::FragmentShader,
        ImageSubresourceRange::WHOLE_COLOR,
    );
    let vertex = ImageAccess::new(
        rid(0),
        ResourceAccessMode::Read,
        ResourceAccessUsage::Storage,
        ResourceAccessStage::VertexShader,
        ImageSubresourceRange::WHOLE_COLOR,
    );
    let plan = compile(vec![
        pass("writer", vec![producer]),
        pass("first", vec![fragment]),
        pass("second", vec![vertex]),
    ]);
    assert!(
        plan.sync.pass_ops[2]
            .iter()
            .any(|op| op.before_pass == Some(0)
                && op.before == ImageSyncState::of_access(&producer)
                && op.after == ImageSyncState::of_access(&vertex))
    );
}

#[test]
fn test_writers_wait_for_every_outstanding_buffer_reader_stage() {
    let fragment =
        BufferAccess::storage_read(rid(0)).with_stage(ResourceAccessStage::FragmentShader);
    let vertex = BufferAccess::storage_read(rid(0)).with_stage(ResourceAccessStage::VertexShader);
    let writer = BufferAccess::storage_write(rid(0));
    let plan = compile(vec![
        buffer_pass("first", vec![fragment]),
        buffer_pass("second", vec![vertex]),
        buffer_pass("write", vec![writer]),
    ]);
    for reader in [fragment, vertex] {
        assert!(
            plan.sync.pass_buffer_ops[2]
                .iter()
                .any(|op| op.before == BufferSyncState::of_access(&reader)
                    && op.reason == SyncReason::Hazard(ResourceHazardKind::WriteAfterRead))
        );
    }
}

#[test]
fn test_independent_buffer_readers_retain_the_writer_scope() {
    let producer = BufferAccess::storage_write(rid(0));
    let fragment =
        BufferAccess::storage_read(rid(0)).with_stage(ResourceAccessStage::FragmentShader);
    let vertex = BufferAccess::storage_read(rid(0)).with_stage(ResourceAccessStage::VertexShader);
    let plan = compile(vec![
        buffer_pass("writer", vec![producer]),
        buffer_pass("first", vec![fragment]),
        buffer_pass("second", vec![vertex]),
    ]);
    assert!(
        plan.sync.pass_buffer_ops[2]
            .iter()
            .any(|op| op.before_pass == Some(0)
                && op.before == BufferSyncState::of_access(&producer)
                && op.after == BufferSyncState::of_access(&vertex))
    );
}

#[test]
fn test_boundaries_follow_operations_and_dag_instead_of_names() {
    let mut graphics = pass(
        "particle_simulate",
        vec![ImageAccess::storage_write(rid(0))],
    );
    graphics.operation = PassType::Graphics;
    let mut transfer = pass("geometry", vec![ImageAccess::transfer_read(rid(0))]);
    transfer.operation = PassType::Transfer;
    let mut compute = buffer_pass("ui", vec![BufferAccess::storage_write(rid(1))]);
    compute.operation = PassType::Compute;
    let plan = compile(vec![graphics, transfer, compute]);
    assert_eq!(
        plan.sync.pass_boundaries[0].as_ref().unwrap().encoder,
        EncoderKind::Render
    );
    let boundary = plan.sync.pass_boundaries[1].as_ref().unwrap();
    assert_eq!(boundary.encoder, EncoderKind::Blit);
    assert_eq!(boundary.queue, QueueClass::Graphics);
    assert_eq!(boundary.predecessors, vec![0]);
    assert_eq!(
        plan.sync.pass_boundaries[2].as_ref().unwrap().encoder,
        EncoderKind::Compute
    );
}

#[test]
fn test_untouched_import_final_transition_preserves_undefined_contents() {
    let mut contracts = BTreeMap::new();
    contracts.insert(
        rid(0),
        ImportedImageContract::undefined().must_end_in(ResourceState::PresentSrc),
    );
    let plan = compile_with_contracts(Vec::new(), contracts);
    assert_eq!(plan.sync.final_ops.len(), 1);
    let op = plan.sync.final_ops[0];
    assert_eq!(op.before, ImageSyncState::Undefined);
    assert_eq!(op.before_pass, None);
    assert_eq!(op.reason, SyncReason::ImportedFinal);
    assert!(plan.sync.pass_boundaries.is_empty());
}
#[test]
fn test_external_upload_ranges_seed_the_exact_consumer_scope() {
    let mip = ImageSubresourceRange::new(ImageAspects::COLOR, 2, 1, 1, 1);
    let producer = ImageAccess::transfer_write(rid(0)).with_range(mip);
    let consumer = ImageAccess::new(
        rid(0),
        ResourceAccessMode::Read,
        ResourceAccessUsage::Sampled,
        ResourceAccessStage::VertexShader,
        mip,
    );
    let mut compiler = GraphCompiler::new(vec![pass("consume", vec![consumer])]);
    compiler.imported_contracts.insert(
        rid(0),
        ImportedImageContract::arrives_in(ResourceState::ShaderRead),
    );
    compiler.external_image_accesses.push(producer);
    let plan = compiler.compile().unwrap();
    assert_eq!(plan.sync.external_image_producers.len(), 1);
    assert_eq!(
        plan.sync.external_image_producers[0].encoder,
        EncoderKind::Blit
    );
    let op = plan.sync.pass_ops[0][0];
    assert_eq!(op.range, mip);
    assert_eq!(op.before, ImageSyncState::of_access(&producer));
    assert_eq!(op.after, ImageSyncState::of_access(&consumer));
    assert_eq!(op.before_pass, None);
}

#[test]
fn test_unchanged_imported_mips_keep_their_declared_initial_scope() {
    let uploaded = ImageSubresourceRange::new(ImageAspects::COLOR, 2, 1, 0, 1);
    let untouched = ImageSubresourceRange::new(ImageAspects::COLOR, 0, 1, 0, 1);
    let consumer = ImageAccess::new(
        rid(0),
        ResourceAccessMode::Read,
        ResourceAccessUsage::TransferSource,
        ResourceAccessStage::Transfer,
        untouched,
    );
    let mut compiler = GraphCompiler::new(vec![pass("consume", vec![consumer])]);
    compiler.imported_contracts.insert(
        rid(0),
        ImportedImageContract::arrives_in(ResourceState::ShaderRead),
    );
    compiler
        .external_image_accesses
        .push(ImageAccess::transfer_write(rid(0)).with_range(uploaded));
    let plan = compiler.compile().unwrap();
    let op = plan.sync.pass_ops[0][0];
    assert_eq!(op.range, untouched);
    assert_eq!(
        op.before,
        ImageSyncState::of_contract_state(ResourceState::ShaderRead)
    );
}
#[test]
fn test_invalid_image_usage_cannot_lower_to_an_empty_native_access_mask() {
    let invalid = ImageAccess::new(
        rid(0),
        ResourceAccessMode::Read,
        ResourceAccessUsage::TransferDestination,
        ResourceAccessStage::Transfer,
        ImageSubresourceRange::WHOLE_COLOR,
    );
    assert!(matches!(
        GraphCompiler::new(vec![pass("invalid", vec![invalid])]).compile(),
        Err(crate::render_graph::RenderGraphError::Validation(
            crate::render_graph::GraphValidationError::InvalidImageAccess { .. }
        ))
    ));
    let invalid = ImageAccess::new(
        rid(0),
        ResourceAccessMode::Write,
        ResourceAccessUsage::Sampled,
        ResourceAccessStage::FragmentShader,
        ImageSubresourceRange::WHOLE_COLOR,
    );
    assert!(
        GraphCompiler::new(vec![pass("invalid", vec![invalid])])
            .compile()
            .is_err()
    );
}
#[test]
fn test_bindless_uploads_require_explicit_shader_encoder_visibility() {
    let mut graphics = pass("scene", Vec::new());
    graphics.operation = PassType::Graphics;
    let mut compute = pass("material_compute", Vec::new());
    compute.operation = PassType::Compute;
    let mut transfer = pass("readback", Vec::new());
    transfer.operation = PassType::Transfer;
    let mut compiler = GraphCompiler::new(vec![graphics, compute, transfer]);
    compiler.external_uploads_pending = true;
    let plan = compiler.compile().unwrap();
    assert!(plan.sync.external_image_producers.is_empty());
    assert!(
        plan.sync.pass_boundaries[0]
            .as_ref()
            .unwrap()
            .external_upload_dependency
    );
    assert!(
        plan.sync.pass_boundaries[1]
            .as_ref()
            .unwrap()
            .external_upload_dependency
    );
    assert!(
        !plan.sync.pass_boundaries[2]
            .as_ref()
            .unwrap()
            .external_upload_dependency
    );
}
#[test]
fn test_read_only_buffer_stage_changes_do_not_create_false_ordering() {
    let fragment =
        BufferAccess::storage_read(rid(0)).with_stage(ResourceAccessStage::FragmentShader);
    let vertex = BufferAccess::storage_read(rid(0)).with_stage(ResourceAccessStage::VertexShader);
    let plan = compile(vec![
        buffer_pass("fragment", vec![fragment]),
        buffer_pass("vertex", vec![vertex]),
    ]);
    assert!(plan.sync.pass_buffer_ops[1].is_empty());
    assert!(
        plan.sync.pass_boundaries[1]
            .as_ref()
            .unwrap()
            .predecessors
            .is_empty()
    );
}
#[test]
fn test_external_upload_visibility_reaches_each_shader_reader_stage() {
    let range = ImageSubresourceRange::WHOLE_COLOR;
    let producer = ImageAccess::transfer_write(rid(0)).with_range(range);
    let fragment = ImageAccess::new(
        rid(0),
        ResourceAccessMode::Read,
        ResourceAccessUsage::Sampled,
        ResourceAccessStage::FragmentShader,
        range,
    );
    let vertex = ImageAccess::new(
        rid(0),
        ResourceAccessMode::Read,
        ResourceAccessUsage::Sampled,
        ResourceAccessStage::VertexShader,
        range,
    );
    let mut compiler = GraphCompiler::new(vec![
        pass("fragment", vec![fragment]),
        pass("vertex", vec![vertex]),
    ]);
    compiler.external_image_accesses.push(producer);
    let plan = compiler.compile().unwrap();
    assert!(
        plan.sync.pass_ops[1]
            .iter()
            .any(|op| op.before == ImageSyncState::of_access(&producer)
                && op.after == ImageSyncState::of_access(&vertex)
                && op.before_pass.is_none())
    );
}
#[test]
fn test_shared_buffer_first_write_orders_all_previous_frame_readers() {
    let write = BufferAccess::storage_write(rid(0));
    let fragment =
        BufferAccess::storage_read(rid(0)).with_stage(ResourceAccessStage::FragmentShader);
    let vertex = BufferAccess::storage_read(rid(0)).with_stage(ResourceAccessStage::VertexShader);
    let plan = compile(vec![
        buffer_pass("write", vec![write]),
        buffer_pass("fragment", vec![fragment]),
        buffer_pass("vertex", vec![vertex]),
    ]);
    for access in [write, fragment, vertex] {
        assert!(
            plan.sync.pass_buffer_ops[0]
                .iter()
                .any(|op| op.before == BufferSyncState::of_access(&access)
                    && op.before_pass.is_none()
                    && op.after == BufferSyncState::of_access(&write))
        );
    }
}
#[test]
fn test_new_topology_orders_the_retained_external_buffer_writer() {
    let writer = BufferAccess::storage_write(rid(9)).with_range(BufferByteRange::new(16, 32));
    let transfer = BufferAccess::new(
        rid(9),
        ResourceAccessMode::Read,
        BufferUsage::TransferSource,
        ResourceAccessStage::Transfer,
        BufferByteRange::new(24, 8),
    );
    let mut compiler = GraphCompiler::new(vec![buffer_pass("new read only graph", vec![transfer])]);
    compiler.external_buffer_accesses.push(writer);
    let plan = compiler.compile().unwrap();
    assert!(plan.sync.pass_buffer_ops[0].iter().any(|op| op.before
        == BufferSyncState::of_access(&writer)
        && op.after == BufferSyncState::of_access(&transfer)
        && op.range == transfer.range
        && op.before_pass.is_none()));
}
