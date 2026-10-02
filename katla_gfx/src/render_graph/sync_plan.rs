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
mod tests {
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
        let plan =
            compile_with_contracts(vec![pass("sample", vec![sampled_read(rid(0))])], contracts);

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
        let plan =
            compile_with_contracts(vec![pass("ui", vec![attachment_write(rid(0))])], contracts);

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
        let vertex =
            BufferAccess::storage_read(rid(0)).with_stage(ResourceAccessStage::VertexShader);
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
        let vertex =
            BufferAccess::storage_read(rid(0)).with_stage(ResourceAccessStage::VertexShader);
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
        let vertex =
            BufferAccess::storage_read(rid(0)).with_stage(ResourceAccessStage::VertexShader);
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
        let vertex =
            BufferAccess::storage_read(rid(0)).with_stage(ResourceAccessStage::VertexShader);
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
        let mut compiler =
            GraphCompiler::new(vec![buffer_pass("new read only graph", vec![transfer])]);
        compiler.external_buffer_accesses.push(writer);
        let plan = compiler.compile().unwrap();
        assert!(plan.sync.pass_buffer_ops[0].iter().any(|op| op.before
            == BufferSyncState::of_access(&writer)
            && op.after == BufferSyncState::of_access(&transfer)
            && op.range == transfer.range
            && op.before_pass.is_none()));
    }
}
