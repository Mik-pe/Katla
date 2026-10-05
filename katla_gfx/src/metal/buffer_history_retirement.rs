//! Destroyed allocation scopes retire after all exact consuming submissions.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Weak};

use super::buffer::{BufferLifetime, BufferRetirementQueue, MetalBuffer};
use super::metal_renderer::FRAMES_IN_FLIGHT;
use super::submission::SubmissionCompletion;

struct AllocationLeases {
    owners: Vec<Weak<BufferLifetime>>,
    completions: [Option<SubmissionCompletion>; FRAMES_IN_FLIGHT],
}

#[derive(Default)]
pub(crate) struct BufferHistoryRetirement {
    queue: BufferRetirementQueue,
    allocations: HashMap<u64, AllocationLeases>,
    candidates: HashSet<u64>,
}

impl BufferHistoryRetirement {
    pub(crate) fn record(
        &mut self,
        identity: u64,
        buffer: &MetalBuffer,
        slot: usize,
        completion: &SubmissionCompletion,
    ) {
        buffer.lifetime.watch(identity, &self.queue);
        let allocation = self
            .allocations
            .entry(identity)
            .or_insert_with(|| AllocationLeases {
                owners: Vec::new(),
                completions: Default::default(),
            });
        let owner = Arc::downgrade(&buffer.lifetime);
        if !allocation
            .owners
            .iter()
            .any(|existing| Weak::ptr_eq(existing, &owner))
        {
            allocation.owners.push(owner);
        }
        allocation.completions[slot] = Some(completion.clone());
    }

    pub(crate) fn retire_completed(
        &mut self,
        history: &mut crate::render_graph::BufferExecutionHistory,
    ) {
        self.candidates.extend(std::mem::take(
            &mut *self.queue.lock().unwrap_or_else(|error| error.into_inner()),
        ));
        self.candidates.retain(|identity| {
            let Some(allocation) = self.allocations.get(identity) else {
                return false;
            };
            let destroyed = allocation
                .owners
                .iter()
                .all(|owner| owner.strong_count() == 0);
            let completed = allocation
                .completions
                .iter()
                .flatten()
                .all(SubmissionCompletion::is_complete);
            if destroyed && completed {
                history.retire(*identity);
                self.allocations.remove(identity);
                false
            } else {
                true
            }
        });
    }

    pub(crate) fn clear(&mut self) {
        self.allocations.clear();
        self.candidates.clear();
        self.queue
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render_graph::{
        BufferAccess, BufferByteRange, BufferUsage, ResourceAccessMode, ResourceAccessStage,
        ResourceId,
    };
    use objc2_metal::MTLBuffer;

    #[test]
    fn test_destroyed_history_waits_for_every_consuming_slot() {
        let context = super::super::context::MetalContext::init_headless().unwrap();
        let buffer = context.create_buffer(32, true).unwrap();
        let identity = buffer.inner.gpuAddress();
        let access = BufferAccess::new(
            ResourceId(0),
            ResourceAccessMode::Write,
            BufferUsage::Storage,
            ResourceAccessStage::ComputeShader,
            BufferByteRange::new(0, 32),
        );
        let mut history = crate::render_graph::BufferExecutionHistory::default();
        history.record(identity, 0, &[access]);
        let first = SubmissionCompletion::default();
        let second = SubmissionCompletion::default();
        assert!(first.mark_submitted());
        assert!(second.mark_submitted());
        let mut retirements = BufferHistoryRetirement::default();
        retirements.record(identity, &buffer, 0, &first);
        retirements.record(identity, &buffer, 1, &second);
        drop(buffer);
        retirements.retire_completed(&mut history);
        assert_eq!(history.previous(identity, 0, 32).len(), 1);
        first.finish(super::super::submission::CommitFeedback {
            gpu_start: 0.0,
            gpu_end: 1.0,
            error: None,
        });
        retirements.retire_completed(&mut history);
        assert_eq!(history.previous(identity, 0, 32).len(), 1);
        second.finish(super::super::submission::CommitFeedback {
            gpu_start: 0.0,
            gpu_end: 1.0,
            error: None,
        });
        retirements.retire_completed(&mut history);
        assert!(history.previous(identity, 0, 32).is_empty());
        assert!(retirements.allocations.is_empty());
        assert!(retirements.candidates.is_empty());
    }
}
