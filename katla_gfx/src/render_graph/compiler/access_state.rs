//! Declaration-order versions and range-scoped dependency hazards.

use std::collections::BTreeSet;

use super::{DependencyNode, add_dependency};
use crate::render_graph::access::{BufferByteRange, ImageSubresourceRange, ResourceAccessMode};

/// A resource range the dependency analysis can compare, subtract, and test
/// for emptiness.
///
/// Images and buffers both version their accesses by declaration order, and
/// both only order accesses that touch overlapping ranges. Implementing this
/// trait for a range type is what lets one state machine serve both.
pub(super) trait AccessRange: Copy {
    fn overlaps_with(self, other: Self) -> bool;
    /// The pieces of `self` outside `other`, for version replacement.
    fn subtract_range(self, other: Self) -> Vec<Self>;
    fn is_empty_range(self) -> bool;
}

impl AccessRange for ImageSubresourceRange {
    fn overlaps_with(self, other: Self) -> bool {
        self.overlaps(other)
    }

    fn subtract_range(self, other: Self) -> Vec<Self> {
        self.subtract(other)
    }

    fn is_empty_range(self) -> bool {
        self.is_empty()
    }
}

impl AccessRange for BufferByteRange {
    fn overlaps_with(self, other: Self) -> bool {
        self.overlaps(other)
    }

    fn subtract_range(self, other: Self) -> Vec<Self> {
        self.subtract(other)
    }

    fn is_empty_range(self) -> bool {
        self.is_empty()
    }
}

/// One outstanding typed access on a resource version, with its range.
#[derive(Debug, Clone, Copy)]
struct OutstandingAccess<R> {
    pass: usize,
    range: R,
}

/// Outstanding resource versions seen so far in declaration order.
///
/// Writers hold the current version of the ranges they wrote; readers hold the
/// ranges they consumed. A later writer only replaces the ranges it actually
/// covers, so accesses to disjoint ranges of one resource stay independent
/// while overlapping ranges keep producing the minimal RAW, WAR, and WAW
/// ordering constraints.
pub(super) struct ResourceAccessState<R> {
    writers: Vec<OutstandingAccess<R>>,
    readers: Vec<OutstandingAccess<R>>,
}

impl<R: AccessRange> Default for ResourceAccessState<R> {
    fn default() -> Self {
        Self {
            writers: Vec::new(),
            readers: Vec::new(),
        }
    }
}

impl<R: AccessRange> ResourceAccessState<R> {
    pub(super) fn final_writers(self) -> impl Iterator<Item = usize> {
        self.writers.into_iter().map(|writer| writer.pass)
    }

    /// RAW: earlier writers of overlapping ranges produce this read.
    fn raw_edges(
        &self,
        pass: usize,
        range: R,
        graph: &mut [DependencyNode],
        data_predecessors: &mut [BTreeSet<usize>],
    ) {
        for writer in &self.writers {
            if writer.range.overlaps_with(range) {
                add_dependency(graph, writer.pass, pass);
                data_predecessors[pass].insert(writer.pass);
            }
        }
    }

    /// WAR: earlier readers of overlapping ranges must finish first.
    fn war_edges(&self, pass: usize, range: R, graph: &mut [DependencyNode]) {
        for reader in &self.readers {
            if reader.pass != pass && reader.range.overlaps_with(range) {
                add_dependency(graph, reader.pass, pass);
            }
        }
    }

    /// WAW: earlier writers of overlapping ranges must finish first.
    fn waw_edges(&self, pass: usize, range: R, graph: &mut [DependencyNode]) {
        for writer in &self.writers {
            if writer.pass != pass && writer.range.overlaps_with(range) {
                add_dependency(graph, writer.pass, pass);
            }
        }
    }

    /// Replace the versions a write covers; partially covered versions
    /// survive on their remaining ranges.
    fn replace_version(&mut self, pass: usize, range: R) {
        for versions in [&mut self.writers, &mut self.readers] {
            let mut remaining = Vec::new();
            for access in versions.drain(..) {
                for piece in access.range.subtract_range(range) {
                    if !piece.is_empty_range() {
                        remaining.push(OutstandingAccess {
                            pass: access.pass,
                            range: piece,
                        });
                    }
                }
            }
            *versions = remaining;
        }
        self.writers.push(OutstandingAccess { pass, range });
    }

    fn add_reader(&mut self, pass: usize, range: R) {
        self.readers.push(OutstandingAccess { pass, range });
    }
}

/// Record one typed access against a resource's version state.
///
/// The same hazard rules apply to image subresource ranges and buffer byte
/// ranges: RAW orders readers after overlapping writers, WAR and WAW order a
/// writer after overlapping readers and writers, and a write replaces only the
/// range it covers.
pub(super) fn apply_access<R: AccessRange>(
    state: &mut ResourceAccessState<R>,
    pass: usize,
    mode: ResourceAccessMode,
    range: R,
    graph: &mut [DependencyNode],
    data_predecessors: &mut [BTreeSet<usize>],
) {
    if mode.reads() {
        state.raw_edges(pass, range, graph, data_predecessors);
    }
    if mode.writes() {
        state.waw_edges(pass, range, graph);
        state.war_edges(pass, range, graph);
        state.replace_version(pass, range);
    } else if mode.reads() {
        state.add_reader(pass, range);
    }
}
