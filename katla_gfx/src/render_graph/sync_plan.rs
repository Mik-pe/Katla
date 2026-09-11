//! Backend-neutral synchronization plan compiled from typed image accesses.
//!
//! The dependency DAG orders passes; this plan says what each pass boundary
//! must synchronize. A single forward scan over the sorted live passes tracks
//! the state every subresource range is in (usage, pipeline stage, access
//! mode) and emits one operation whenever the next access needs a different
//! state — or a same-state execution/memory barrier across a RAW, WAR, or WAW
//! hazard. The plan is derived from exactly the accesses that built the DAG,
//! so scheduling and synchronization can no longer disagree.
//!
//! Frame-periodic steady state: the graph repeats identically every frame, so
//! the state a resource ends one frame in is the state the next frame's first
//! access finds. Backends bootstrap freshly created textures by substituting
//! an `UNDEFINED` source layout for the compiled `before` state, which is
//! always legal because their contents are garbage anyway.

use std::collections::BTreeMap;

use super::access::{
    ImageAccess, ImageAccessMode, ImagePipelineStage, ImageSubresourceRange, ImageUsage,
};
use super::handles::ResourceId;
use super::resource::{ImportedImageContract, ResourceState};

/// Backend-neutral hazard kind derived from the canonical access DAG.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ResourceHazardKind {
    ReadAfterWrite,
    WriteAfterRead,
    WriteAfterWrite,
}

/// The synchronization state one access (or import contract) puts a
/// resource's subresources into.
///
/// State equality decides whether a state transition is needed: two accesses
/// in the same state need no layout or mask change (a cross-pass hazard may
/// still order them with a same-state barrier).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageSyncState {
    /// Subresources no live pass has written this frame: layout undefined,
    /// contents discardable.
    Undefined,
    /// The state a typed access leaves the subresources in.
    Access {
        usage: ImageUsage,
        stage: ImagePipelineStage,
        mode: ImageAccessMode,
    },
}

impl ImageSyncState {
    fn of_access(access: &ImageAccess) -> Self {
        Self::Access {
            usage: access.usage,
            stage: access.stage,
            mode: access.mode,
        }
    }

    /// Map a coarse contract state to its synchronization state. Contract
    /// states describe whole images; read-write is the conservative mode for
    /// observable contents.
    fn of_contract_state(state: ResourceState) -> Self {
        use super::resource::ResourceState as S;
        match state {
            S::Undefined => Self::Undefined,
            S::ColorAttachment => Self::Access {
                usage: ImageUsage::ColorAttachment,
                stage: ImagePipelineStage::ColorAttachmentOutput,
                mode: ImageAccessMode::ReadWrite,
            },
            S::DepthStencilAttachment => Self::Access {
                usage: ImageUsage::DepthStencilAttachment,
                stage: ImagePipelineStage::DepthStencil,
                mode: ImageAccessMode::ReadWrite,
            },
            S::ShaderRead => Self::Access {
                usage: ImageUsage::Sampled,
                stage: ImagePipelineStage::FragmentShader,
                mode: ImageAccessMode::Read,
            },
            S::ShaderWrite => Self::Access {
                usage: ImageUsage::Storage,
                stage: ImagePipelineStage::AllGraphics,
                mode: ImageAccessMode::Write,
            },
            S::TransferSrc => Self::Access {
                usage: ImageUsage::TransferSource,
                stage: ImagePipelineStage::Transfer,
                mode: ImageAccessMode::Read,
            },
            S::TransferDst => Self::Access {
                usage: ImageUsage::TransferDestination,
                stage: ImagePipelineStage::Transfer,
                mode: ImageAccessMode::Write,
            },
            S::PresentSrc => Self::Access {
                usage: ImageUsage::Present,
                stage: ImagePipelineStage::Present,
                mode: ImageAccessMode::Write,
            },
        }
    }

    fn writes(self) -> bool {
        match self {
            Self::Undefined => false,
            Self::Access { mode, .. } => mode.writes(),
        }
    }

    fn reads(self) -> bool {
        match self {
            Self::Undefined => false,
            Self::Access { mode, .. } => mode.reads(),
        }
    }
}

/// Why one synchronization operation exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncReason {
    /// First use of the subresources this frame: the resource arrives
    /// undefined (transient) or in its imported initial contract state.
    InitialUse,
    /// The operation orders a hazard between two passes.
    Hazard(ResourceHazardKind),
    /// A state change between accesses with no cross-pass hazard.
    StateChange,
    /// Frame-end transition satisfying an imported image's final contract.
    ImportedFinal,
}

/// One compiled synchronization operation on a subresource range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageSyncOp {
    pub resource: ResourceId,
    /// Subresources the operation covers (intersection of the previous
    /// access's range and the next access's range).
    pub range: ImageSubresourceRange,
    /// State the subresources are in before the operation.
    pub before: ImageSyncState,
    /// State the access requires.
    pub after: ImageSyncState,
    /// Pass that established `before`; `None` at frame start (undefined or
    /// imported initial state).
    pub before_pass: Option<usize>,
    /// Pass the operation precedes.
    pub pass: usize,
    pub reason: SyncReason,
}

/// Compiled synchronization plan.
///
/// `pass_ops` holds the operations to execute before each pass (indexed by
/// declared pass index; culled passes have none). `final_ops` runs after the
/// last live pass to satisfy imported final-state contracts.
#[derive(Debug, Clone, Default)]
pub struct SyncPlan {
    pub pass_ops: Vec<Vec<ImageSyncOp>>,
    pub final_ops: Vec<ImageSyncOp>,
}

/// One tracked state piece of a resource.
#[derive(Debug, Clone, Copy)]
struct StatePiece {
    range: ImageSubresourceRange,
    state: ImageSyncState,
    /// Pass that established the state; `None` for frame-start states.
    pass: Option<usize>,
}

fn hazard_between(before: ImageSyncState, after: ImageSyncState) -> Option<ResourceHazardKind> {
    if before.writes() && after.reads() {
        Some(ResourceHazardKind::ReadAfterWrite)
    } else if before.reads() && after.writes() {
        Some(ResourceHazardKind::WriteAfterRead)
    } else if before.writes() && after.writes() {
        Some(ResourceHazardKind::WriteAfterWrite)
    } else {
        // Read-after-read needs no ordering.
        None
    }
}

/// Build the synchronization plan.
///
/// The graph executes identically every frame, so a transient's frame-start
/// state is the state the previous frame left it in: the scan runs twice, the
/// first pass discovering the frame-end states and the second compiling the
/// operations against those steady-state seeds. Imports do not cycle — the
/// importer hands the image over in its contract's initial state every frame
/// — so they always seed from the contract, never from the frame-end states.
pub(crate) fn build_sync_plan(
    passes: &[super::compiler::PassInfo],
    sorted_passes: &[usize],
    imported_contracts: &BTreeMap<ResourceId, ImportedImageContract>,
) -> SyncPlan {
    let (_, first_cycle_end_states) = scan_passes(passes, sorted_passes, imported_contracts, None);
    let (pass_ops, end_states) = scan_passes(
        passes,
        sorted_passes,
        imported_contracts,
        Some(&first_cycle_end_states),
    );
    let final_ops = build_final_ops(&end_states, imported_contracts);

    SyncPlan {
        pass_ops,
        final_ops,
    }
}

/// One forward scan over the sorted live passes, tracking subresource states.
/// Returns the per-pass operations and the frame-end states.
///
/// `steady_state_seeds` supplies each transient's frame-start pieces; the
/// first (discovery) scan runs without them.
fn scan_passes(
    passes: &[super::compiler::PassInfo],
    sorted_passes: &[usize],
    imported_contracts: &BTreeMap<ResourceId, ImportedImageContract>,
    steady_state_seeds: Option<&BTreeMap<ResourceId, Vec<StatePiece>>>,
) -> (Vec<Vec<ImageSyncOp>>, BTreeMap<ResourceId, Vec<StatePiece>>) {
    let mut pass_ops: Vec<Vec<ImageSyncOp>> = vec![Vec::new(); passes.len()];
    let mut states: BTreeMap<ResourceId, Vec<StatePiece>> = BTreeMap::new();

    for &pass_index in sorted_passes {
        for access in &passes[pass_index].image_accesses {
            let target = ImageSyncState::of_access(access);
            let pieces = states.entry(access.resource).or_default();
            if pieces.is_empty() {
                // Frame-start state: imports arrive in their contract's
                // initial state; transients in their previous-frame end state.
                if let Some(contract) = imported_contracts.get(&access.resource) {
                    let initial = ImageSyncState::of_contract_state(contract.initial);
                    if initial != ImageSyncState::Undefined {
                        pieces.push(StatePiece {
                            range: ImageSubresourceRange::whole(super::access::ImageAspects::ALL),
                            state: initial,
                            pass: None,
                        });
                    }
                } else if let Some(seeds) = steady_state_seeds
                    && let Some(seed_pieces) = seeds.get(&access.resource)
                {
                    pieces.extend(seed_pieces.iter().copied().map(|piece| StatePiece {
                        pass: None,
                        ..piece
                    }));
                }
            }

            let mut access_ops = Vec::new();
            let mut covered = Vec::new();
            for piece in pieces.iter().copied() {
                let Some(range) = piece.range.intersection(access.range) else {
                    continue;
                };
                covered.push(piece.range);
                let hazard = match piece.pass {
                    Some(before_pass) if before_pass != pass_index => {
                        hazard_between(piece.state, target)
                    }
                    _ => None,
                };
                if piece.state == target && hazard.is_none() {
                    // A frame-start state that already matches needs no
                    // steady-state transition, but freshly created textures
                    // still arrive undefined: emit a discard-safe bootstrap
                    // operation the backend coalesces away once the tracked
                    // layout matches.
                    if piece.pass.is_none() {
                        access_ops.push(ImageSyncOp {
                            resource: access.resource,
                            range,
                            before: ImageSyncState::Undefined,
                            after: target,
                            before_pass: None,
                            pass: pass_index,
                            reason: SyncReason::InitialUse,
                        });
                    }
                    continue;
                }
                let reason = if let Some(kind) = hazard {
                    SyncReason::Hazard(kind)
                } else if piece.pass.is_none() {
                    SyncReason::InitialUse
                } else {
                    SyncReason::StateChange
                };
                access_ops.push(ImageSyncOp {
                    resource: access.resource,
                    range,
                    before: piece.state,
                    after: target,
                    before_pass: piece.pass,
                    pass: pass_index,
                    reason,
                });
            }

            // Subresources no previous access or frame-start state covers
            // arrive undefined; contents may be discarded.
            let mut remainder = vec![access.range];
            for covered_range in covered {
                remainder = remainder
                    .iter()
                    .flat_map(|range| range.subtract(covered_range))
                    .collect();
            }
            for range in remainder {
                access_ops.push(ImageSyncOp {
                    resource: access.resource,
                    range,
                    before: ImageSyncState::Undefined,
                    after: target,
                    before_pass: None,
                    pass: pass_index,
                    reason: SyncReason::InitialUse,
                });
            }

            pass_ops[pass_index].extend(access_ops);

            // Replace the subresources this access covers.
            let mut remaining_pieces = Vec::with_capacity(pieces.len() + 1);
            for piece in pieces.iter().copied() {
                for fragment in piece.range.subtract(access.range) {
                    if !fragment.is_empty() {
                        remaining_pieces.push(StatePiece {
                            range: fragment,
                            ..piece
                        });
                    }
                }
            }
            remaining_pieces.push(StatePiece {
                range: access.range,
                state: target,
                pass: Some(pass_index),
            });
            *pieces = remaining_pieces;
        }
    }

    (pass_ops, states)
}

/// Emit frame-end operations for imports whose required final state differs
/// from the state the last live access left their subresources in.
fn build_final_ops(
    states: &BTreeMap<ResourceId, Vec<StatePiece>>,
    imported_contracts: &BTreeMap<ResourceId, ImportedImageContract>,
) -> Vec<ImageSyncOp> {
    let mut ops = Vec::new();

    for (resource, contract) in imported_contracts {
        let Some(required) = contract.required_final else {
            continue;
        };
        let final_state = ImageSyncState::of_contract_state(required);
        let Some(pieces) = states.get(resource) else {
            // No live pass touched the image; compile-time validation already
            // rejected unreachable required-final states.
            continue;
        };

        for piece in pieces.iter().copied() {
            if piece.state == final_state {
                continue;
            }
            ops.push(ImageSyncOp {
                resource: *resource,
                range: piece.range,
                before: piece.state,
                after: final_state,
                before_pass: piece.pass,
                pass: usize::MAX,
                reason: SyncReason::ImportedFinal,
            });
        }
    }

    ops
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render_graph::access::{
        ImageAccessMode, ImageAspects, ImagePipelineStage, ImageSubresourceRange, ImageUsage,
    };
    use crate::render_graph::compiler::{ExecutionPlan, GraphCompiler, PassInfo};
    use crate::render_graph::handles::ResourceId;
    use crate::render_graph::pass::{PassDesc, PassType};

    fn rid(n: u32) -> ResourceId {
        ResourceId(n)
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
        mode: ImageAccessMode,
        usage: ImageUsage,
        stage: ImagePipelineStage,
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
            reads,
            writes,
            image_accesses: accesses,
            side_effect: false,
        }
    }

    fn state(
        usage: ImageUsage,
        stage: ImagePipelineStage,
        mode: ImageAccessMode,
    ) -> ImageSyncState {
        ImageSyncState::Access { usage, stage, mode }
    }

    fn attachment_write(resource: ResourceId) -> ImageAccess {
        access(
            resource,
            ImageAccessMode::Write,
            ImageUsage::ColorAttachment,
            ImagePipelineStage::ColorAttachmentOutput,
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
                ImageUsage::DepthStencilAttachment,
                ImagePipelineStage::DepthStencil,
                ImageAccessMode::Write,
            )
        );
        assert_eq!(
            ops[0].after,
            state(
                ImageUsage::Sampled,
                ImagePipelineStage::FragmentShader,
                ImageAccessMode::Read,
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
                ImageUsage::ColorAttachment,
                ImagePipelineStage::ColorAttachmentOutput,
                ImageAccessMode::Write,
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
                ImageUsage::TransferDestination,
                ImagePipelineStage::Transfer,
                ImageAccessMode::Write,
            )
        );
        assert_eq!(
            ops[0].after,
            state(
                ImageUsage::TransferSource,
                ImagePipelineStage::Transfer,
                ImageAccessMode::Read,
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
                ImageUsage::Sampled,
                ImagePipelineStage::FragmentShader,
                ImageAccessMode::Read,
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
        let plan =
            compile_with_contracts(vec![pass("sample", vec![sampled_read(rid(0))])], contracts);

        let ops = &plan.sync.pass_ops[0];
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0].before_pass, None);
        assert_eq!(ops[0].reason, SyncReason::InitialUse);
        assert_eq!(
            ops[0].before,
            state(
                ImageUsage::TransferDestination,
                ImagePipelineStage::Transfer,
                ImageAccessMode::Write,
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
        let plan =
            compile_with_contracts(vec![pass("ui", vec![attachment_write(rid(0))])], contracts);

        assert_eq!(plan.sync.final_ops.len(), 1);
        let op = plan.sync.final_ops[0];
        assert_eq!(op.before_pass, Some(0));
        assert_eq!(op.reason, SyncReason::ImportedFinal);
        assert_eq!(
            op.after,
            state(
                ImageUsage::Present,
                ImagePipelineStage::Present,
                ImageAccessMode::Write
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
        let plan =
            compile_with_contracts(vec![pass("ui", vec![attachment_write(rid(0))])], contracts);
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
}
