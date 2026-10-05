use super::*;

/// The native encoder opened for an operation. Metal 4 transfers use compute encoders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapturedEncoderKind {
    Render,
    Compute,
    Blit,
}

/// One actual native encoder opening, including auxiliary encoders without a pass.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CapturedEncoder {
    pub ordinal: usize,
    pub pass_index: Option<usize>,
    pub label: String,
    pub kind: CapturedEncoderKind,
    /// Stable graph resource indices observed at native bindings.
    pub resources: Vec<u32>,
}

/// Feedback visible at capture time; observing it never waits for GPU completion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapturedFeedback {
    Pending,
    Completed,
    Failed,
}

/// Exact reusable frame owner and submission identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CapturedSubmission {
    pub frame_slot: usize,
    pub generation: u64,
    pub command_allocator: usize,
    pub feedback_identity: String,
    pub feedback: CapturedFeedback,
}

/// A native residency member identified by first encounter, never its address.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CapturedResidentResource {
    pub set_identity: usize,
    pub kind: String,
    pub ordinal: usize,
    pub estimated_bytes: u64,
}

/// Reflected native bindings and their submission-owned residency membership.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CapturedBindingSet {
    pub identity: usize,
    pub layout_identity: String,
    pub residency_members: Vec<CapturedResidentResource>,
    pub snapshot_generation: Option<u64>,
}

/// The native byte/subresource range supplied with a synchronization scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CapturedNativeSyncRange {
    Global,
    Image {
        aspects: u32,
        base_mip: u32,
        mips: u32,
        base_layer: u32,
        layers: u32,
    },
    Buffer {
        offset: u64,
        bytes: u64,
    },
}

/// Native synchronization scopes, copied from the actual API arguments.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CapturedNativeSyncScope {
    pub range: CapturedNativeSyncRange,
    pub source_stages: u64,
    pub destination_stages: u64,
    pub source_access: u64,
    pub destination_access: u64,
    pub old_layout: Option<String>,
    pub new_layout: Option<String>,
    pub visibility: u64,
}

/// One range-specific compiler operation consumed at a native boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CapturedSyncOperation {
    pub resource: u32,
    /// Compiler translation or an additional native ownership boundary.
    pub origin: String,
    pub resource_kind: String,
    pub producer: Option<usize>,
    pub consumer: Option<usize>,
    /// The producer access identifies the current range version, including initial imports.
    pub version: String,
    pub range: String,
    pub source: String,
    pub destination: String,
    pub reason: String,
    pub boundary: String,
    /// Actual native stages/access/layout/visibility after backend translation.
    pub native_scope: Vec<CapturedNativeSyncScope>,
    /// Required native scopes from the existing canonical backend translator.
    pub required_native_scope: Vec<CapturedNativeSyncScope>,
    /// False means the backend proved this operation needs no native barrier.
    pub emitted: bool,
    pub omission_reason: Option<String>,
}

struct SyncDescription {
    range: String,
    source: String,
    destination: String,
    reason: String,
}

impl CapturedSyncOperation {
    pub(crate) fn image(op: &ImageSyncOp, boundary: &str) -> Self {
        Self::new(
            op.resource.0,
            "image",
            op.before_pass,
            (!matches!(op.reason, super::super::SyncReason::ImportedFinal)).then_some(op.pass),
            SyncDescription {
                range: format!("{:?}", op.range),
                source: format!("{:?}", op.before),
                destination: format!("{:?}", op.after),
                reason: format!("{:?}", op.reason),
            },
            boundary,
        )
    }
    pub(crate) fn buffer(op: &BufferSyncOp, boundary: &str) -> Self {
        Self::new(
            op.resource.0,
            "buffer",
            op.before_pass,
            Some(op.pass),
            SyncDescription {
                range: format!("{:?}", op.range),
                source: format!("{:?}", op.before),
                destination: format!("{:?}", op.after),
                reason: format!("{:?}", op.reason),
            },
            boundary,
        )
    }
    fn new(
        resource: u32,
        kind: &str,
        producer: Option<usize>,
        consumer: Option<usize>,
        description: SyncDescription,
        boundary: &str,
    ) -> Self {
        Self {
            resource,
            origin: "compiled".into(),
            resource_kind: kind.into(),
            producer,
            consumer,
            version: producer.map_or_else(
                || format!("r{resource}.initial"),
                |pass| format!("r{resource}.access.{pass}"),
            ),
            range: description.range,
            source: description.source,
            destination: description.destination,
            reason: description.reason,
            boundary: boundary.into(),
            native_scope: Vec::new(),
            required_native_scope: Vec::new(),
            emitted: true,
            omission_reason: None,
        }
    }
    /// Records a backend's explicit no-op decision without inventing a native barrier.
    pub fn omitted(mut self, reason: &str) -> Self {
        self.emitted = false;
        self.omission_reason = Some(reason.into());
        self
    }
    pub(super) fn contract(&self) -> impl PartialEq + '_ {
        (
            self.resource,
            &self.resource_kind,
            self.producer,
            self.consumer,
            &self.range,
            &self.source,
            &self.destination,
            &self.reason,
        )
    }
}

/// Records populated at actual native dispatch sites, only when tracing is enabled.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct BackendExecutionTrace {
    pub backend: String,
    pub frame: Option<CapturedSubmission>,
    pub encoders: Vec<CapturedEncoder>,
    pub synchronization: Vec<CapturedSyncOperation>,
    pub bindings: Vec<CapturedBindingSet>,
}

/// Stable pass-level payload accompanying the lower-level native encoder records.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CapturedPass {
    pub pass_index: usize,
    pub label: String,
    pub encode_position: usize,
    pub outcome: String,
    pub skip_reason: Option<String>,
    pub color_targets: Vec<String>,
    pub depth_target: Option<String>,
    pub color_operations: String,
    pub depth_operations: String,
}

/// A complete snapshot. Backend records reflect the observed state, not inferred completion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderGraphCapture {
    pub schema_version: u32,
    pub graph: RenderGraphDiagnostics,
    pub planned_synchronization: Vec<CapturedSyncOperation>,
    pub executed_passes: Vec<CapturedPass>,
    pub backend_execution: BackendExecutionTrace,
    pub comparison: Vec<String>,
}
