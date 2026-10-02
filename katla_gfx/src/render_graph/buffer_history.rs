//! Resolved physical buffer access frontiers between graph submissions.

use std::collections::BTreeMap;

use super::{BufferAccess, BufferByteRange};

/// Retains conservative access scopes of each native range until GPU drain.
#[derive(Debug, Default)]
pub(crate) struct BufferExecutionHistory {
    accesses: BTreeMap<u64, Vec<BufferAccess>>,
}

impl BufferExecutionHistory {
    /// Project native byte ranges into the graph-visible slice.
    pub(crate) fn previous(&self, identity: u64, offset: u64, size: u64) -> Vec<BufferAccess> {
        let slice = BufferByteRange::new(offset, size);
        self.accesses
            .get(&identity)
            .into_iter()
            .flatten()
            .filter_map(|access| {
                let overlap = access.range.intersection(slice)?;
                Some(BufferAccess {
                    range: BufferByteRange::new(overlap.offset - offset, overlap.size),
                    ..*access
                })
            })
            .collect()
    }

    /// Record canonical accesses in execution order after successful encoding.
    pub(crate) fn record(&mut self, identity: u64, offset: u64, accesses: &[BufferAccess]) {
        let frontier = self.accesses.entry(identity).or_default();
        for access in accesses {
            let mut access = BufferAccess {
                resource: super::ResourceId(0),
                range: BufferByteRange::new(
                    offset.saturating_add(access.range.offset),
                    access.range.size,
                ),
                ..*access
            };
            loop {
                let previous_count = frontier.len();
                frontier.retain(|previous| {
                    let same_scope = previous.mode == access.mode
                        && previous.usage == access.usage
                        && previous.stage == access.stage;
                    if same_scope
                        && previous.range.offset <= access.range.end()
                        && access.range.offset <= previous.range.end()
                    {
                        let offset = previous.range.offset.min(access.range.offset);
                        let end = previous.range.end().max(access.range.end());
                        access.range = BufferByteRange::new(offset, end - offset);
                        false
                    } else {
                        true
                    }
                });
                if frontier.len() == previous_count {
                    break;
                }
            }
            frontier.push(access);
        }
    }

    /// Remove a native allocation after its final referencing submission completes.
    pub(crate) fn retire(&mut self, identity: u64) {
        self.accesses.remove(&identity);
    }

    /// Drop completed scopes when the backend has drained all submissions.
    pub(crate) fn clear(&mut self) {
        self.accesses.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render_graph::{BufferUsage, ResourceAccessMode, ResourceAccessStage, ResourceId};

    fn access(resource: u32, mode: ResourceAccessMode, stage: ResourceAccessStage) -> BufferAccess {
        BufferAccess::new(
            ResourceId(resource),
            mode,
            BufferUsage::Storage,
            stage,
            BufferByteRange::new(0, 32),
        )
    }

    #[test]
    fn test_alternating_roles_resolve_the_previous_physical_writer() {
        let mut history = BufferExecutionHistory::default();
        history.record(
            11,
            64,
            &[access(
                1,
                ResourceAccessMode::Write,
                ResourceAccessStage::ComputeShader,
            )],
        );
        history.record(
            11,
            96,
            &[access(
                2,
                ResourceAccessMode::Read,
                ResourceAccessStage::Transfer,
            )],
        );
        let current = history.previous(11, 64, 32);
        assert_eq!(current.len(), 1);
        assert_eq!(current[0].stage, ResourceAccessStage::ComputeShader);
        assert_eq!(current[0].range, BufferByteRange::new(0, 32));
        assert!(history.previous(12, 64, 32).is_empty());
    }

    #[test]
    fn test_completed_allocation_retirement_preserves_other_in_flight_scopes() {
        let mut history = BufferExecutionHistory::default();
        let scope = access(
            1,
            ResourceAccessMode::Write,
            ResourceAccessStage::ComputeShader,
        );
        history.record(1, 0, &[scope]);
        history.record(2, 0, &[scope]);
        history.retire(1);
        assert!(history.previous(1, 0, 32).is_empty());
        assert_eq!(history.previous(2, 0, 32).len(), 1);
        history.record(2, 64, &[scope]);
        history.record(2, 32, &[scope]);
        assert_eq!(history.previous(2, 0, 96).len(), 1);
    }

    #[test]
    fn test_aborted_or_zero_dispatch_write_cannot_erase_real_reader_scopes() {
        let mut history = BufferExecutionHistory::default();
        history.record(
            7,
            0,
            &[access(
                1,
                ResourceAccessMode::Read,
                ResourceAccessStage::FragmentShader,
            )],
        );
        history.record(
            7,
            0,
            &[access(
                99,
                ResourceAccessMode::Write,
                ResourceAccessStage::ComputeShader,
            )],
        );
        let previous = history.previous(7, 0, 32);
        assert!(
            previous
                .iter()
                .any(|access| access.stage == ResourceAccessStage::FragmentShader)
        );
        assert!(
            previous
                .iter()
                .any(|access| access.stage == ResourceAccessStage::ComputeShader)
        );
        history.record(
            7,
            0,
            &[access(
                100,
                ResourceAccessMode::Write,
                ResourceAccessStage::ComputeShader,
            )],
        );
        assert_eq!(history.previous(7, 0, 32).len(), 2);
    }

    #[test]
    fn test_new_graph_retains_all_readers_and_culled_writer_scopes() {
        let mut history = BufferExecutionHistory::default();
        history.record(
            7,
            0,
            &[access(
                1,
                ResourceAccessMode::Write,
                ResourceAccessStage::ComputeShader,
            )],
        );
        history.record(
            7,
            0,
            &[access(
                99,
                ResourceAccessMode::Read,
                ResourceAccessStage::VertexShader,
            )],
        );
        history.record(
            7,
            0,
            &[access(
                100,
                ResourceAccessMode::Read,
                ResourceAccessStage::Transfer,
            )],
        );
        assert_eq!(history.previous(7, 0, 32).len(), 3);
        history.record(
            7,
            8,
            &[BufferAccess {
                range: BufferByteRange::new(0, 8),
                ..access(
                    101,
                    ResourceAccessMode::Write,
                    ResourceAccessStage::Transfer,
                )
            }],
        );
        assert_eq!(history.previous(7, 8, 8).len(), 4);
        assert_eq!(history.previous(7, 16, 16).len(), 3);
        history.retire(7);
        assert!(history.previous(7, 0, 32).is_empty());
        history.clear();
    }
}
