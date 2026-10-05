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
    BufferAccess, BufferByteRange, BufferUsage, ImageAccess, ImageSubresourceRange,
    ResourceAccessMode, ResourceAccessStage, ResourceAccessUsage,
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
        usage: ResourceAccessUsage,
        stage: ResourceAccessStage,
        mode: ResourceAccessMode,
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
                usage: ResourceAccessUsage::ColorAttachment,
                stage: ResourceAccessStage::ColorAttachmentOutput,
                mode: ResourceAccessMode::ReadWrite,
            },
            S::DepthStencilAttachment => Self::Access {
                usage: ResourceAccessUsage::DepthStencilAttachment,
                stage: ResourceAccessStage::DepthStencil,
                mode: ResourceAccessMode::ReadWrite,
            },
            S::ShaderRead => Self::Access {
                usage: ResourceAccessUsage::Sampled,
                stage: ResourceAccessStage::FragmentShader,
                mode: ResourceAccessMode::Read,
            },
            S::ShaderWrite => Self::Access {
                usage: ResourceAccessUsage::Storage,
                stage: ResourceAccessStage::AllGraphics,
                mode: ResourceAccessMode::Write,
            },
            S::TransferSrc => Self::Access {
                usage: ResourceAccessUsage::TransferSource,
                stage: ResourceAccessStage::Transfer,
                mode: ResourceAccessMode::Read,
            },
            S::TransferDst => Self::Access {
                usage: ResourceAccessUsage::TransferDestination,
                stage: ResourceAccessStage::Transfer,
                mode: ResourceAccessMode::Write,
            },
            S::PresentSrc => Self::Access {
                usage: ResourceAccessUsage::Present,
                stage: ResourceAccessStage::Present,
                mode: ResourceAccessMode::Write,
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

/// The synchronization state one buffer access leaves a byte range in.
///
/// Buffers have no layouts, so a state is just the usage, stage, and mode the
/// access needs. Equal states need no barrier unless a hazard orders them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferSyncState {
    /// No prior graph access establishes the byte range's synchronization state.
    Undefined,
    /// The state a typed buffer access leaves the bytes in.
    Access {
        usage: BufferUsage,
        stage: ResourceAccessStage,
        mode: ResourceAccessMode,
    },
}

impl BufferSyncState {
    fn of_access(access: &BufferAccess) -> Self {
        Self::Access {
            usage: access.usage,
            stage: access.stage,
            mode: access.mode,
        }
    }

    fn reads(self) -> bool {
        matches!(self, Self::Access { mode, .. } if mode.reads())
    }

    fn writes(self) -> bool {
        matches!(self, Self::Access { mode, .. } if mode.writes())
    }
}

/// One compiled synchronization operation on a byte range of a buffer.
///
/// A buffer state change is purely an execution/memory dependency: there is no
/// layout to transition, so a backend realizes every operation as a memory
/// barrier over `range` (or skips it when the hazard cannot reach the GPU).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BufferSyncOp {
    pub resource: ResourceId,
    /// Bytes the operation covers (intersection of the previous access's range
    /// and the next access's range).
    pub range: BufferByteRange,
    /// State the bytes are in before the operation.
    pub before: BufferSyncState,
    /// State the access requires.
    pub after: BufferSyncState,
    /// Pass that established `before`; `None` at frame start.
    pub before_pass: Option<usize>,
    /// Pass the operation precedes.
    pub pass: usize,
    pub reason: SyncReason,
}

/// Compiled synchronization plan.
///
/// `pass_ops` holds the operations to execute before each pass (indexed by
/// declared pass index; culled passes have none). `final_ops` runs after the
/// last live pass to satisfy imported final-state contracts. Buffer operations
/// are held in the parallel `pass_buffer_ops` list.
#[derive(Debug, Clone, Default)]
pub struct SyncPlan {
    /// Physical-range handoffs before each pass, filled by allocation compilation.
    pub alias_handoffs: Vec<Vec<ResourceId>>,
    /// Transfer producers encoded before graph passes on the declared queue.
    pub external_image_producers: Vec<ExternalImageProducer>,
    /// Queue and encoder requirements indexed by declared pass.
    pub pass_boundaries: Vec<Option<PassBoundary>>,
    pub pass_ops: Vec<Vec<ImageSyncOp>>,
    pub final_ops: Vec<ImageSyncOp>,
    /// Buffer operations to execute before each pass, indexed by declared pass
    /// index, matching `pass_ops`.
    pub pass_buffer_ops: Vec<Vec<BufferSyncOp>>,
}

/// Queue on which graph operations execute. No implicit asynchronous work is permitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueClass {
    Graphics,
}

/// Native encoding domain selected by the declared operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncoderKind {
    Render,
    Compute,
    Blit,
}

/// One external upload's typed producer scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExternalImageProducer {
    pub access: ImageAccess,
    pub queue: QueueClass,
    pub encoder: EncoderKind,
}

/// An encoder boundary and its canonical ordering requirements.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PassBoundary {
    pub queue: QueueClass,
    pub encoder: EncoderKind,
    pub predecessors: Vec<usize>,
    /// The pass may sample bindless textures produced by the external upload batch.
    pub external_upload_dependency: bool,
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
    dag: &[super::compiler::PassDagNode],
    imported_contracts: &BTreeMap<ResourceId, ImportedImageContract>,
    external_image_accesses: &[ImageAccess],
    external_buffer_accesses: &[BufferAccess],
    external_uploads_pending: bool,
) -> SyncPlan {
    let (_, first_cycle_end_states) = scan_passes(
        passes,
        sorted_passes,
        imported_contracts,
        external_image_accesses,
        None,
    );
    let (mut pass_ops, end_states) = scan_passes(
        passes,
        sorted_passes,
        imported_contracts,
        external_image_accesses,
        Some(&first_cycle_end_states),
    );
    let final_ops = build_final_ops(&end_states, imported_contracts);
    let mut buffer_seeds = buffer_cycle_seeds(passes, sorted_passes);
    for access in external_buffer_accesses {
        let pieces = buffer_seeds.entry(access.resource).or_default();
        let state = BufferSyncState::of_access(access);
        if !pieces
            .iter()
            .any(|piece| piece.range == access.range && piece.state == state)
        {
            pieces.push(BufferStatePiece {
                range: access.range,
                state,
                pass: None,
            });
        }
    }
    let mut pass_buffer_ops = scan_buffer_passes(passes, sorted_passes, &buffer_seeds);
    complete_hazard_scopes(
        passes,
        sorted_passes,
        dag,
        external_image_accesses,
        &buffer_seeds,
        &mut pass_ops,
        &mut pass_buffer_ops,
    );
    let mut pass_boundaries = vec![None; passes.len()];
    for &pass in sorted_passes {
        pass_boundaries[pass] = Some(PassBoundary {
            queue: QueueClass::Graphics,
            encoder: match passes[pass].operation {
                super::pass::PassType::Graphics => EncoderKind::Render,
                super::pass::PassType::Compute => EncoderKind::Compute,
                super::pass::PassType::Transfer => EncoderKind::Blit,
            },
            predecessors: dag[pass].predecessors.clone(),
            external_upload_dependency: external_uploads_pending
                && matches!(
                    passes[pass].operation,
                    super::pass::PassType::Graphics | super::pass::PassType::Compute
                ),
        });
    }

    SyncPlan {
        alias_handoffs: vec![Vec::new(); passes.len()],
        external_image_producers: external_image_accesses
            .iter()
            .copied()
            .map(|access| ExternalImageProducer {
                access,
                queue: QueueClass::Graphics,
                encoder: EncoderKind::Blit,
            })
            .collect(),
        pass_boundaries,
        pass_ops,
        final_ops,
        pass_buffer_ops,
    }
}

/// Complete execution scopes from the canonical DAG. The layout scan tracks
/// the latest access, while independent readers can leave several outstanding
/// stages. Each edge retains its producer scope until a barrier at that scope
/// has made the same destination access visible.
fn complete_hazard_scopes(
    passes: &[super::compiler::PassInfo],
    sorted: &[usize],
    dag: &[super::compiler::PassDagNode],
    external_images: &[ImageAccess],
    buffer_seeds: &BTreeMap<ResourceId, Vec<BufferStatePiece>>,
    images: &mut [Vec<ImageSyncOp>],
    buffers: &mut [Vec<BufferSyncOp>],
) {
    let mut image_frontiers: BTreeMap<ResourceId, Vec<(usize, ImageAccess)>> = BTreeMap::new();
    for &access in external_images {
        image_frontiers
            .entry(access.resource)
            .or_default()
            .push((usize::MAX, access));
    }
    let mut buffer_frontiers: BTreeMap<ResourceId, Vec<(usize, BufferAccess)>> = BTreeMap::new();
    for (&resource, pieces) in buffer_seeds {
        for piece in pieces {
            if let BufferSyncState::Access { usage, stage, mode } = piece.state {
                buffer_frontiers.entry(resource).or_default().push((
                    usize::MAX,
                    BufferAccess::new(resource, mode, usage, stage, piece.range),
                ));
            }
        }
    }

    let mut image_visibility: BTreeMap<(usize, ResourceId), Vec<ImageSyncOp>> = BTreeMap::new();
    let mut buffer_visibility: BTreeMap<(usize, ResourceId), Vec<BufferSyncOp>> = BTreeMap::new();
    for &consumer in sorted {
        for after in &passes[consumer].image_accesses {
            let frontier = image_frontiers.entry(after.resource).or_default();
            for &(producer, before) in frontier.iter() {
                if producer != usize::MAX && !dag[consumer].predecessors.contains(&producer) {
                    continue;
                }
                let Some(range) = before.range.intersection(after.range) else {
                    continue;
                };
                let before_state = ImageSyncState::of_access(&before);
                let after_state = ImageSyncState::of_access(after);
                let Some(hazard) = hazard_between(before_state, after_state) else {
                    continue;
                };
                let covered = images[consumer].iter().any(|op| {
                    op.resource == after.resource
                        && op.range == range
                        && op.before == before_state
                        && op.after == after_state
                });
                let visible = hazard == ResourceHazardKind::ReadAfterWrite
                    && image_visibility
                        .get(&(producer, after.resource))
                        .is_some_and(|ops| {
                            ops.iter()
                                .any(|op| op.range == range && op.after == after_state)
                        });
                if !covered && !visible {
                    images[consumer].push(ImageSyncOp {
                        resource: after.resource,
                        range,
                        before: before_state,
                        after: after_state,
                        before_pass: (producer != usize::MAX).then_some(producer),
                        pass: consumer,
                        reason: if producer == usize::MAX {
                            SyncReason::InitialUse
                        } else {
                            SyncReason::Hazard(hazard)
                        },
                    });
                }
            }
            if after.mode.writes() {
                *frontier = frontier
                    .iter()
                    .flat_map(|&(pass, access)| {
                        access
                            .range
                            .subtract(after.range)
                            .into_iter()
                            .map(move |range| (pass, ImageAccess { range, ..access }))
                    })
                    .collect();
            }
            frontier.push((consumer, *after));
        }
        for after in &passes[consumer].buffer_accesses {
            let frontier = buffer_frontiers.entry(after.resource).or_default();
            for &(producer, before) in frontier.iter() {
                if producer != usize::MAX && !dag[consumer].predecessors.contains(&producer) {
                    continue;
                }
                let Some(range) = before.range.intersection(after.range) else {
                    continue;
                };
                let before_state = BufferSyncState::of_access(&before);
                let after_state = BufferSyncState::of_access(after);
                let Some(hazard) = buffer_hazard_between(before_state, after_state) else {
                    continue;
                };
                let covered = buffers[consumer].iter().any(|op| {
                    op.resource == after.resource
                        && op.range == range
                        && op.before == before_state
                        && op.after == after_state
                });
                let visible = hazard == ResourceHazardKind::ReadAfterWrite
                    && buffer_visibility
                        .get(&(producer, after.resource))
                        .is_some_and(|ops| {
                            ops.iter()
                                .any(|op| op.range == range && op.after == after_state)
                        });
                if !covered && !visible {
                    buffers[consumer].push(BufferSyncOp {
                        resource: after.resource,
                        range,
                        before: before_state,
                        after: after_state,
                        before_pass: (producer != usize::MAX).then_some(producer),
                        pass: consumer,
                        reason: if producer == usize::MAX {
                            SyncReason::InitialUse
                        } else {
                            SyncReason::Hazard(hazard)
                        },
                    });
                }
            }
            if after.mode.writes() {
                *frontier = frontier
                    .iter()
                    .flat_map(|&(pass, access)| {
                        access
                            .range
                            .subtract(after.range)
                            .into_iter()
                            .map(move |range| (pass, BufferAccess { range, ..access }))
                    })
                    .collect();
            }
            frontier.push((consumer, *after));
        }
        for op in &images[consumer] {
            if let Some(producer) = op.before_pass.or_else(|| {
                external_images
                    .iter()
                    .any(|access| access.resource == op.resource)
                    .then_some(usize::MAX)
            }) {
                image_visibility
                    .entry((producer, op.resource))
                    .or_default()
                    .push(*op);
            }
        }
        for op in &buffers[consumer] {
            if let Some(producer) = op.before_pass.or_else(|| {
                buffer_seeds
                    .contains_key(&op.resource)
                    .then_some(usize::MAX)
            }) {
                buffer_visibility
                    .entry((producer, op.resource))
                    .or_default()
                    .push(*op);
            }
        }
    }
}

/// One tracked byte range of a buffer.
#[derive(Debug, Clone, Copy)]
struct BufferStatePiece {
    range: BufferByteRange,
    state: BufferSyncState,
    /// Pass that established the state; `None` for frame-start states.
    pass: Option<usize>,
}

/// Retain all scopes that can be outstanding when the next frame starts.
fn buffer_cycle_seeds(
    passes: &[super::compiler::PassInfo],
    sorted: &[usize],
) -> BTreeMap<ResourceId, Vec<BufferStatePiece>> {
    let mut states: BTreeMap<ResourceId, Vec<BufferStatePiece>> = BTreeMap::new();
    for &pass in sorted {
        for access in &passes[pass].buffer_accesses {
            let pieces = states.entry(access.resource).or_default();
            if access.mode.writes() {
                *pieces = pieces
                    .iter()
                    .flat_map(|piece| {
                        piece
                            .range
                            .subtract(access.range)
                            .into_iter()
                            .map(|range| BufferStatePiece { range, ..*piece })
                    })
                    .collect();
            }
            pieces.push(BufferStatePiece {
                range: access.range,
                state: BufferSyncState::of_access(access),
                pass: None,
            });
        }
    }
    states
}

/// One forward scan over the sorted live passes, tracking buffer byte ranges.
///
/// Buffers have no layout, so the scan emits an operation only when a hazard
/// orders two accesses (RAW/WAR/WAW). Seeds include repeated graph scopes and
/// retained physical scopes from earlier submissions, including other graphs.
fn scan_buffer_passes(
    passes: &[super::compiler::PassInfo],
    sorted_passes: &[usize],
    seeds: &BTreeMap<ResourceId, Vec<BufferStatePiece>>,
) -> Vec<Vec<BufferSyncOp>> {
    let mut pass_ops: Vec<Vec<BufferSyncOp>> = vec![Vec::new(); passes.len()];
    let mut states = seeds.clone();

    for &pass_index in sorted_passes {
        for access in &passes[pass_index].buffer_accesses {
            let target = BufferSyncState::of_access(access);
            let pieces = states.entry(access.resource).or_default();

            let mut access_ops = Vec::new();
            let mut covered = Vec::new();
            for piece in pieces.iter().copied() {
                let Some(range) = piece.range.intersection(access.range) else {
                    continue;
                };
                covered.push(piece.range);

                // Same bytes, same state: nothing to do without a hazard.
                let hazard = match piece.pass {
                    Some(before_pass) if before_pass == pass_index => None,
                    _ => buffer_hazard_between(piece.state, target),
                };
                if hazard.is_none()
                    && (piece.state == target
                        || (piece.state.reads()
                            && !piece.state.writes()
                            && target.reads()
                            && !target.writes()))
                {
                    continue;
                }

                access_ops.push(BufferSyncOp {
                    resource: access.resource,
                    range,
                    before: piece.state,
                    after: target,
                    before_pass: piece.pass,
                    pass: pass_index,
                    reason: match hazard {
                        _ if piece.pass.is_none() => SyncReason::InitialUse,
                        Some(kind) => SyncReason::Hazard(kind),
                        None => SyncReason::StateChange,
                    },
                });
            }

            // First graph accesses have no prior GPU scope to synchronize.
            let mut remainder = vec![access.range];
            for covered_range in covered {
                remainder = remainder
                    .iter()
                    .flat_map(|range| range.subtract(covered_range))
                    .collect();
            }
            for range in remainder {
                if range.is_empty() {
                    continue;
                }
                access_ops.push(BufferSyncOp {
                    resource: access.resource,
                    range,
                    before: BufferSyncState::Undefined,
                    after: target,
                    before_pass: None,
                    pass: pass_index,
                    reason: SyncReason::InitialUse,
                });
            }

            pass_ops[pass_index].extend(access_ops);

            // Replace the bytes this access covers.
            let mut remaining_pieces = Vec::with_capacity(pieces.len() + 1);
            for piece in pieces.iter().copied() {
                for fragment in piece.range.subtract(access.range) {
                    if !fragment.is_empty() {
                        remaining_pieces.push(BufferStatePiece {
                            range: fragment,
                            ..piece
                        });
                    }
                }
            }
            remaining_pieces.push(BufferStatePiece {
                range: access.range,
                state: target,
                pass: Some(pass_index),
            });
            *pieces = remaining_pieces;
        }
    }

    pass_ops
}

fn buffer_hazard_between(
    before: BufferSyncState,
    after: BufferSyncState,
) -> Option<ResourceHazardKind> {
    if before.writes() && after.reads() {
        Some(ResourceHazardKind::ReadAfterWrite)
    } else if before.reads() && after.writes() {
        Some(ResourceHazardKind::WriteAfterRead)
    } else if before.writes() && after.writes() {
        Some(ResourceHazardKind::WriteAfterWrite)
    } else {
        None
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
    external_image_accesses: &[ImageAccess],
    steady_state_seeds: Option<&BTreeMap<ResourceId, Vec<StatePiece>>>,
) -> (Vec<Vec<ImageSyncOp>>, BTreeMap<ResourceId, Vec<StatePiece>>) {
    let mut pass_ops: Vec<Vec<ImageSyncOp>> = vec![Vec::new(); passes.len()];
    let mut states: BTreeMap<ResourceId, Vec<StatePiece>> = BTreeMap::new();

    for access in external_image_accesses {
        let pieces = states.entry(access.resource).or_default();
        if pieces.is_empty()
            && let Some(contract) = imported_contracts.get(&access.resource)
        {
            pieces.push(StatePiece {
                range: ImageSubresourceRange::whole(super::ImageAspects::ALL),
                state: ImageSyncState::of_contract_state(contract.initial),
                pass: None,
            });
        }
        *pieces = pieces
            .iter()
            .flat_map(|piece| {
                piece
                    .range
                    .subtract(access.range)
                    .into_iter()
                    .map(|range| StatePiece { range, ..*piece })
            })
            .collect();
        pieces.push(StatePiece {
            range: access.range,
            state: ImageSyncState::of_access(access),
            pass: None,
        });
    }

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
                let same_read_layout = matches!((piece.state, target),
                    (ImageSyncState::Access { usage: before, mode: ResourceAccessMode::Read, .. },
                     ImageSyncState::Access { usage: after, mode: ResourceAccessMode::Read, .. }) if before == after);
                if same_read_layout && piece.state != target && hazard.is_none() {
                    continue;
                }
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
            let initial = ImageSyncState::of_contract_state(contract.initial);
            if initial != final_state {
                ops.push(ImageSyncOp {
                    resource: *resource,
                    range: ImageSubresourceRange::whole(super::access::ImageAspects::ALL),
                    before: initial,
                    after: final_state,
                    before_pass: None,
                    pass: usize::MAX,
                    reason: SyncReason::ImportedFinal,
                });
            }
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
mod tests;
