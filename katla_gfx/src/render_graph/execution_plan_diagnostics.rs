//! Human-readable diagnostics for compiled render graph execution plans.

use std::fmt;

use super::compiler::ExecutionPlan;

impl fmt::Display for ExecutionPlan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let dependency_edges = self
            .dag
            .iter()
            .map(|node| node.successors.len())
            .sum::<usize>();

        write!(
            f,
            "{} live passes ({} culled), {} dependency edges, {} parallel levels",
            self.sorted_passes.len(),
            self.culled_passes.len(),
            dependency_edges,
            self.parallel_groups.len()
        )
    }
}
