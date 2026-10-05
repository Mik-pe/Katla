//! Passive, pointer-free records joining compiled graphs to native execution.

use super::{BufferSyncOp, ImageSyncOp, RenderGraphDiagnostics, ResourceExecutionTrace};
use serde::Serialize;

mod comparison;
mod formats;
mod model;

pub use model::*;

impl RenderGraphCapture {
    pub(crate) fn join(
        graph: RenderGraphDiagnostics,
        planned_synchronization: Vec<CapturedSyncOperation>,
        trace: &ResourceExecutionTrace,
        comparison: Vec<String>,
    ) -> Self {
        let mut capture = Self {
            schema_version: graph.schema_version,
            graph,
            planned_synchronization,
            executed_passes: trace
                .entries()
                .iter()
                .map(|entry| CapturedPass {
                    pass_index: entry.pass_index,
                    label: entry.name.clone(),
                    encode_position: entry.encode_position,
                    outcome: entry.outcome.to_string(),
                    skip_reason: (entry.outcome == super::EmittedPassOutcome::SkippedNoWork)
                        .then(|| "no native workload for this frame".into()),
                    color_targets: entry.color_targets.clone(),
                    depth_target: entry.depth_target.clone(),
                    color_operations: format!("{:?}", entry.color_attachment_ops),
                    depth_operations: format!("{:?}", entry.depth_attachment_ops),
                })
                .collect(),
            backend_execution: trace.backend.clone(),
            comparison,
        };
        capture.comparison.extend(capture.compare_native());
        capture
    }
}
