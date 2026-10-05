use super::*;

/// Schema version for serialized render-graph diagnostics.
pub const RENDER_GRAPH_DIAGNOSTICS_SCHEMA_VERSION: u32 = 13;

/// Stable, backend-neutral snapshot of a render graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderGraphDiagnostics {
    pub schema_version: u32,
    pub summary: RenderGraphDiagnosticSummary,
    pub culling_enabled: bool,
    pub liveness_roots: Vec<usize>,
    pub resources: Vec<RenderGraphDiagnosticResource>,
    pub passes: Vec<RenderGraphDiagnosticPass>,
    pub external_image_producers: Vec<RenderGraphDiagnosticExternalProducer>,
    pub dependencies: Vec<RenderGraphDiagnosticDependency>,
    pub synchronization: Vec<RenderGraphDiagnosticTransition>,
    /// Compiled buffer synchronization operations, in execution order. Buffer
    /// ops carry byte ranges rather than layouts, so they are a separate list.
    pub buffer_synchronization: Vec<RenderGraphDiagnosticBufferSyncOp>,
    pub execution_order: Vec<usize>,
    pub parallel_groups: Vec<Vec<usize>>,
    pub transient_slots: Vec<RenderGraphDiagnosticAllocationSlot>,
    /// Compiler projection before allocation; backend-observed facts afterwards.
    pub allocation_source: String,
    pub native_allocations: Vec<RenderGraphDiagnosticNativeAllocation>,
}

/// Backend-observed physical range owned by one frame slot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderGraphDiagnosticNativeAllocation {
    pub id: u32,
    pub frame_slot: usize,
    pub first_execution_position: Option<usize>,
    pub last_execution_position: Option<usize>,
    pub compatibility_class: String,
    pub resources: Vec<RenderGraphDiagnosticResourceRef>,
    pub offset: u64,
    pub bytes: u64,
    pub logical_bytes: u64,
    pub strategy: String,
    pub alias_savings_bytes: u64,
    pub tile_storage_savings_bytes: u64,
    /// Unknown without hardware counters; discard stores alone do not imply traffic.
    pub bandwidth_savings_estimate_bytes: Option<u64>,
}

/// Content persistence required by the live resource accesses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderGraphDiagnosticPersistence {
    pub exported: bool,
    pub crosses_render_pass: bool,
    pub sampled: bool,
    pub storage: bool,
    pub transfer_or_readback: bool,
    pub tile_memory_eligible: bool,
}

/// One compiled buffer synchronization operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderGraphDiagnosticBufferSyncOp {
    pub version: String,
    pub source_boundary: String,
    pub destination_boundary: String,
    pub resource: RenderGraphDiagnosticResourceRef,
    pub range: RenderGraphDiagnosticBufferByteRange,
    pub before: String,
    pub after: String,
    /// Pass that established `before`; absent at frame start.
    pub before_pass: Option<usize>,
    pub before_name: Option<String>,
    /// Pass the operation precedes.
    pub pass: usize,
    pub pass_name: String,
    pub hazard: Option<RenderGraphHazardKind>,
    pub reason: String,
}

/// Aggregate counts for a diagnostics snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderGraphDiagnosticSummary {
    pub declared_passes: usize,
    pub live_passes: usize,
    pub culled_passes: usize,
    pub resources: usize,
    pub dependency_edges: usize,
    pub synchronization_transitions: usize,
    /// Compiled buffer synchronization operations.
    pub buffer_synchronization_ops: usize,
    pub physical_transient_allocations: usize,
    pub logical_transient_bytes: u64,
    pub physical_transient_bytes: u64,
    pub transient_alias_savings_bytes: u64,
    /// Physical bytes in slots whose compiled accesses allow tile-resident
    /// storage, i.e. bytes that could avoid main-memory storage entirely.
    pub tile_memory_eligible_bytes: u64,
    pub parallel_levels: usize,
}

/// Stable resource origin classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderGraphDiagnosticResourceOrigin {
    BuiltIn,
    Imported,
    Transient,
}

/// First and last scheduled access to a resource.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderGraphDiagnosticResourceLifetime {
    pub first_execution_position: usize,
    pub first_pass: usize,
    pub last_execution_position: usize,
    pub last_pass: usize,
}

/// Imported-image initial/final state contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderGraphDiagnosticImportedContract {
    pub initial: String,
    pub required_final: Option<String>,
}

/// One physical allocation slot and the transient resources aliased into it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderGraphDiagnosticAllocationSlot {
    pub id: u32,
    /// Slot members in first-use order: each member's alias predecessor
    /// precedes it and its alias successor follows it in this list.
    pub resources: Vec<RenderGraphDiagnosticResourceRef>,
    /// Byte size of the physical allocation (largest member).
    pub bytes: u64,
    /// Standalone allocation bytes of the members added up.
    pub logical_bytes: u64,
    /// Estimated memory aliasing saves over standalone member allocations.
    pub saved_bytes: u64,
    /// Physical memory compatibility class every member shares.
    pub compatibility: RenderGraphDiagnosticCompatibilityClass,
    /// Whether every member's compiled accesses allow tile-resident storage,
    /// with the reason for a negative verdict.
    pub tile_memory: RenderGraphDiagnosticTileMemory,
    /// Inclusive span of execution positions the slot is live for.
    pub first_execution_position: usize,
    pub last_execution_position: usize,
}

/// Compiled tile-memory verdict for one physical allocation slot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderGraphDiagnosticTileMemory {
    pub eligible: bool,
    pub reason: String,
}

/// Physical memory compatibility class shared by one allocation slot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderGraphDiagnosticCompatibilityClass {
    pub kind: String,
    pub format: String,
    pub width: u32,
    pub height: u32,
    pub tracks_swapchain_size: bool,
}

/// Resource information resolved from the graph namespace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderGraphDiagnosticResource {
    pub id: u32,
    pub name: String,
    pub origin: RenderGraphDiagnosticResourceOrigin,
    pub kind: Option<String>,
    pub format: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub tracks_swapchain_size: Option<bool>,
    /// Allocation requirements for a buffer; absent for image resources.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub buffer: Option<RenderGraphDiagnosticBufferDescriptor>,
    pub exported: bool,
    pub lifetime: Option<RenderGraphDiagnosticResourceLifetime>,
    pub live: bool,
    pub cull_reason: Option<String>,
    /// Neighbors sharing the physical range, ordered by compiled lifetime.
    pub alias_predecessor: Option<u32>,
    pub alias_successor: Option<u32>,
    /// Stable backend-neutral physical allocation slot assigned by the alias planner.
    pub physical_allocation_id: Option<u32>,
    pub persistence: Option<RenderGraphDiagnosticPersistence>,
    /// Declared initial/final state contract, present for imported images.
    pub imported_contract: Option<RenderGraphDiagnosticImportedContract>,
}

/// Declared buffer allocation requirements, independent of native allocation identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderGraphDiagnosticBufferDescriptor {
    pub size: u64,
    pub usages: Vec<RenderGraphDiagnosticBufferUsage>,
    pub memory: RenderGraphDiagnosticBufferMemory,
}

/// Requested buffer memory placement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderGraphDiagnosticBufferMemory {
    DeviceLocal,
    CpuVisible,
    Readback,
}

impl From<BufferDesc> for RenderGraphDiagnosticBufferDescriptor {
    fn from(desc: BufferDesc) -> Self {
        let usages = [
            (
                BufferUsages::UNIFORM,
                RenderGraphDiagnosticBufferUsage::Uniform,
            ),
            (
                BufferUsages::STORAGE,
                RenderGraphDiagnosticBufferUsage::Storage,
            ),
            (
                BufferUsages::VERTEX,
                RenderGraphDiagnosticBufferUsage::Vertex,
            ),
            (BufferUsages::INDEX, RenderGraphDiagnosticBufferUsage::Index),
            (
                BufferUsages::INDIRECT,
                RenderGraphDiagnosticBufferUsage::Indirect,
            ),
            (
                BufferUsages::TRANSFER_SOURCE,
                RenderGraphDiagnosticBufferUsage::TransferSource,
            ),
            (
                BufferUsages::TRANSFER_DESTINATION,
                RenderGraphDiagnosticBufferUsage::TransferDestination,
            ),
            (
                BufferUsages::READBACK,
                RenderGraphDiagnosticBufferUsage::Readback,
            ),
        ]
        .into_iter()
        .filter_map(|(flag, usage)| desc.usages.contains(flag).then_some(usage))
        .collect();
        Self {
            size: desc.size,
            usages,
            memory: match desc.memory {
                BufferMemoryPolicy::DeviceLocal => RenderGraphDiagnosticBufferMemory::DeviceLocal,
                BufferMemoryPolicy::CpuVisible => RenderGraphDiagnosticBufferMemory::CpuVisible,
                BufferMemoryPolicy::Readback => RenderGraphDiagnosticBufferMemory::Readback,
            },
        }
    }
}

/// Pass type without backend-specific command data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderGraphDiagnosticPassType {
    Graphics,
    Compute,
    Transfer,
}

/// Stable resource reference used by pass diagnostics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderGraphDiagnosticResourceRef {
    pub id: u32,
    pub name: String,
}

/// Stable typed image access mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderGraphDiagnosticResourceAccessMode {
    Read,
    Write,
    ReadWrite,
}

/// Stable typed image usage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderGraphDiagnosticResourceAccessUsage {
    Sampled,
    ColorAttachment,
    DepthStencilAttachment,
    Storage,
    TransferSource,
    TransferDestination,
    Present,
}

/// Stable typed pipeline visibility for an image access.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderGraphDiagnosticImageStage {
    VertexInput,
    DrawIndirect,
    Host,
    VertexShader,
    FragmentShader,
    ComputeShader,
    ColorAttachmentOutput,
    DepthStencil,
    Transfer,
    Present,
    AllGraphics,
}

/// Stable image subresource range.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderGraphDiagnosticImageSubresourceRange {
    pub aspects: Vec<String>,
    pub base_mip_level: u32,
    pub mip_level_count: u32,
    pub base_array_layer: u32,
    pub array_layer_count: u32,
}

/// One typed image access declared by a pass.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderGraphDiagnosticImageAccess {
    pub resource: RenderGraphDiagnosticResourceRef,
    pub mode: RenderGraphDiagnosticResourceAccessMode,
    pub usage: RenderGraphDiagnosticResourceAccessUsage,
    pub stage: RenderGraphDiagnosticImageStage,
    pub range: RenderGraphDiagnosticImageSubresourceRange,
}

/// Byte range of one buffer access.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderGraphDiagnosticBufferByteRange {
    pub offset: u64,
    pub size: u64,
}

/// One typed buffer access declared by a pass.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderGraphDiagnosticBufferAccess {
    pub resource: RenderGraphDiagnosticResourceRef,
    pub mode: RenderGraphDiagnosticResourceAccessMode,
    pub usage: RenderGraphDiagnosticBufferUsage,
    pub stage: RenderGraphDiagnosticImageStage,
    pub range: RenderGraphDiagnosticBufferByteRange,
}

/// Backend-neutral buffer usage of one declared access.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderGraphDiagnosticBufferUsage {
    Uniform,
    Storage,
    Vertex,
    Index,
    Indirect,
    TransferSource,
    TransferDestination,
    Readback,
}

/// Pass information with canonical DAG metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderGraphDiagnosticPass {
    pub index: usize,
    pub name: String,
    pub pass_type: RenderGraphDiagnosticPassType,
    pub queue: Option<String>,
    pub encoder: Option<String>,
    pub alias_handoffs: Vec<RenderGraphDiagnosticResourceRef>,
    pub external_upload_dependency: bool,
    pub kind: Option<String>,
    pub reads: Vec<RenderGraphDiagnosticResourceRef>,
    pub writes: Vec<RenderGraphDiagnosticResourceRef>,
    pub image_accesses: Vec<RenderGraphDiagnosticImageAccess>,
    /// Typed buffer accesses with their byte ranges.
    pub buffer_accesses: Vec<RenderGraphDiagnosticBufferAccess>,
    /// Declared load/store operations per color target, in declaration order.
    pub color_attachments: Vec<RenderGraphDiagnosticAttachmentOps>,
    /// Declared depth/stencil operations, when the pass has a depth contract.
    pub depth_attachment: Option<RenderGraphDiagnosticDepthAttachmentOps>,
    pub predecessors: Vec<usize>,
    pub successors: Vec<usize>,
    pub execution_position: Option<usize>,
    pub parallel_level: Option<usize>,
    pub side_effect: bool,
    pub live: bool,
    pub culled: bool,
    /// Stable liveness decision from the compiler roots and producer closure.
    pub liveness_reason: String,
}

/// Explicit upload producer encoded before graph consumers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderGraphDiagnosticExternalProducer {
    pub queue: String,
    pub encoder: String,
    pub access: RenderGraphDiagnosticImageAccess,
}

/// Declared load/store/clear operations for one color target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderGraphDiagnosticAttachmentOps {
    pub resource: u32,
    pub load: String,
    pub store: String,
    pub clear_value: String,
}

/// Declared per-aspect operations for a depth-stencil target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderGraphDiagnosticDepthAttachmentOps {
    pub depth_load: String,
    pub depth_store: String,
    pub stencil_load: String,
    pub stencil_store: String,
}

/// Resource hazard represented by a dependency edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum RenderGraphHazardKind {
    Raw,
    War,
    Waw,
}

/// One concrete hazard carried by a dependency edge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderGraphDiagnosticHazard {
    pub kind: RenderGraphHazardKind,
    pub resource: RenderGraphDiagnosticResourceRef,
}

/// Dependency between two passes, including every resource hazard that created it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderGraphDiagnosticDependency {
    pub from_pass: usize,
    pub from_name: String,
    pub to_pass: usize,
    pub to_name: String,
    pub hazards: Vec<RenderGraphDiagnosticHazard>,
}

/// Synchronization state one side of a transition is in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum RenderGraphDiagnosticSyncState {
    Undefined,
    Access {
        usage: RenderGraphDiagnosticResourceAccessUsage,
        stage: RenderGraphDiagnosticImageStage,
        mode: RenderGraphDiagnosticResourceAccessMode,
    },
}

/// Why one compiled synchronization operation exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderGraphDiagnosticSyncReason {
    InitialUse,
    Hazard,
    StateChange,
    ImportedFinal,
}

/// One compiled synchronization operation between frame start, live passes,
/// and frame end.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderGraphDiagnosticTransition {
    pub version: String,
    pub source_boundary: String,
    pub destination_boundary: String,
    pub resource: RenderGraphDiagnosticResourceRef,
    /// Subresources the operation covers (intersection of the producing and
    /// consuming accesses).
    pub range: RenderGraphDiagnosticImageSubresourceRange,
    /// Pass that established the `before` state; `None` at frame start.
    pub before_pass: Option<usize>,
    pub before_name: Option<String>,
    /// Pass the operation precedes; `None` at frame end.
    pub to_pass: Option<usize>,
    pub to_name: Option<String>,
    pub before_state: RenderGraphDiagnosticSyncState,
    pub after_state: RenderGraphDiagnosticSyncState,
    pub hazard: Option<RenderGraphHazardKind>,
    pub reason: RenderGraphDiagnosticSyncReason,
}
