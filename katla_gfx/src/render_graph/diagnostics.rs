//! Deterministic diagnostics for compiled render graphs.
//!
//! Diagnostics intentionally contain only stable graph data: declaration indices,
//! resource names, access hazards, execution order, and parallel levels. Backend
//! pointers, device addresses, and hash-map iteration order never enter the output.
//!
//! Text and DOT access labels include mode, usage, pipeline stage, aspects, and
//! mip/layer ranges as `base+count`. A count of `u32::MAX` means all remaining
//! subresources, matching the typed graph declaration. Read-write accesses have
//! edges in both directions; dotted edges belong to culled passes.
//!
//! Synchronization transitions appear in full in the JSON and text exports:
//! frame-start seeds, per-pass operations in execution order, and frame-end
//! contract operations. The DOT graph renders only the frame-boundary
//! transitions, as dashed edges through `frame start`/`frame end` nodes;
//! pass-to-pass operations ride the dependency edges that already carry
//! their hazards.
//!
//! Transient allocation slots list the physical alias groups from the
//! compiled allocation plan: member resources in first-use (alias
//! predecessor → successor) order, the physical allocation size, the
//! compatibility class every member shares, the execution-position span
//! the slot is live for, and the estimated bytes aliasing saves. The DOT
//! graph renders each slot as a physical storage node wired to its member
//! resources, distinguishing physical allocations from logical graph
//! resources.
//!
//! Each slot also carries its compiled tile-memory verdict: whether every
//! member's typed accesses are whole-resource attachment accesses that the
//! graph never samples, transfers, presents, or exports. The reason string
//! names the first fact that disqualified the slot, and the summary reports
//! the physical bytes held in eligible slots.
//!
//! Checked-in golden snapshots under `tests/goldens/` pin the canonical
//! exports of a representative graph. Rerun the golden tests with
//! `KATLA_BLESS_GOLDENS=1` to regenerate them after an intentional format
//! or compiler change, and review the diff like code.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use super::BACKBUFFER_NAME;
use super::access::{
    BufferAccess, BufferUsage, ImageAccess, ResourceAccessMode, ResourceAccessStage,
    ResourceAccessUsage,
};
use super::allocation_plan::TransientAllocationPlan;
use super::backend::RenderGraphBackend;
use super::compiler::{ExecutionPlan, ResourceLifetime};
use super::error::RenderGraphError;
use super::frame_graph::FrameGraph;
use super::handles::ResourceId;
use super::pass::{PassDesc, PassType};
use super::resource::{
    BufferDesc, BufferMemoryPolicy, BufferUsages, GraphResourceDesc, GraphResourceType,
    ImportedImageContract,
};
use super::sync_plan::BufferSyncState;
use super::{ImageSyncOp, ImageSyncState, ResourceHazardKind};

mod formats;
mod frame_snapshot;
mod model;
mod projection;

#[cfg(test)]
mod tests;

#[cfg(test)]
use projection::BufferDiagnosticResource;

pub use model::*;
