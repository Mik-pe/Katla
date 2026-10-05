use super::*;

fn rid(n: u32) -> ResourceId {
    ResourceId(n)
}

fn make_pass(name: &str, reads: Vec<ResourceId>, writes: Vec<ResourceId>) -> PassInfo {
    // Mirror the default coarse-to-typed derivation: one whole-resource
    // access per touched resource, matching PassDesc's inference.
    let mut resources = BTreeSet::new();
    resources.extend(reads.iter().copied());
    resources.extend(writes.iter().copied());
    let image_accesses = resources
        .into_iter()
        .map(|resource| {
            let read = reads.contains(&resource);
            let write = writes.contains(&resource);
            match (read, write) {
                (true, true) => ImageAccess::storage_read_write(resource),
                (true, false) => ImageAccess::sampled_read(resource),
                (false, true) => ImageAccess::storage_write(resource),
                (false, false) => unreachable!("resource came from the read/write union"),
            }
        })
        .collect();
    PassInfo {
        name: name.to_string(),
        operation: PassType::Graphics,
        reads,
        writes,
        image_accesses,
        buffer_accesses: Vec::new(),
        attachment_ops: Vec::new(),
        side_effect: false,
    }
}

// --- Range-aware dependency analysis (#30) ---

use super::super::access::{
    BufferByteRange, BufferUsage, ImageAspects, ImageSubresourceRange, ResourceAccessMode,
    ResourceAccessStage, ResourceAccessUsage,
};
use super::super::sync_plan::ResourceHazardKind;
use super::super::sync_plan::{ImageSyncOp, ImageSyncState, SyncReason};

fn access(
    resource: ResourceId,
    mode: ResourceAccessMode,
    range: ImageSubresourceRange,
) -> ImageAccess {
    ImageAccess::new(
        resource,
        mode,
        ResourceAccessUsage::Storage,
        ResourceAccessStage::FragmentShader,
        range,
    )
}

fn typed_pass(name: &str, accesses: Vec<ImageAccess>) -> PassInfo {
    PassInfo {
        name: name.to_string(),
        operation: PassType::Graphics,
        reads: accesses
            .iter()
            .filter(|a| a.mode.reads())
            .map(|a| a.resource)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
        writes: accesses
            .iter()
            .filter(|a| a.mode.writes())
            .map(|a| a.resource)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
        image_accesses: accesses,
        buffer_accesses: Vec::new(),
        attachment_ops: Vec::new(),
        side_effect: false,
    }
}

fn mips(base: u32, count: u32) -> ImageSubresourceRange {
    ImageSubresourceRange::new(ImageAspects::COLOR, base, count, 0, 1)
}

// --- Range-aware buffer dependencies (#31) ---

fn buffer_access(
    resource: ResourceId,
    mode: ResourceAccessMode,
    offset: u64,
    size: u64,
) -> BufferAccess {
    BufferAccess::new(
        resource,
        mode,
        BufferUsage::Storage,
        ResourceAccessStage::ComputeShader,
        BufferByteRange::new(offset, size),
    )
}

fn buffer_pass(name: &str, accesses: Vec<BufferAccess>) -> PassInfo {
    PassInfo {
        name: name.to_string(),
        operation: PassType::Graphics,
        reads: accesses
            .iter()
            .filter(|a| a.mode.reads())
            .map(|a| a.resource)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
        writes: accesses
            .iter()
            .filter(|a| a.mode.writes())
            .map(|a| a.resource)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
        image_accesses: Vec::new(),
        buffer_accesses: accesses,
        attachment_ops: Vec::new(),
        side_effect: false,
    }
}

#[test]
fn disjoint_buffer_writes_are_independent() {
    let a = buffer_pass(
        "a",
        vec![buffer_access(rid(1), ResourceAccessMode::Write, 0, 64)],
    );
    let b = buffer_pass(
        "b",
        vec![buffer_access(rid(1), ResourceAccessMode::Write, 64, 64)],
    );

    let plan = GraphCompiler::new(vec![a, b]).compile().unwrap();

    assert!(plan.dag[0].successors.is_empty());
    assert!(plan.dag[1].predecessors.is_empty());
    assert_eq!(plan.dag[1].level, 0);
}

#[test]
fn overlapping_buffer_writes_stay_waw_ordered() {
    let a = buffer_pass(
        "a",
        vec![buffer_access(rid(1), ResourceAccessMode::Write, 0, 128)],
    );
    let b = buffer_pass(
        "b",
        vec![buffer_access(rid(1), ResourceAccessMode::Write, 64, 64)],
    );

    let plan = GraphCompiler::new(vec![a, b]).compile().unwrap();

    assert!(plan.dag[1].predecessors.contains(&0));
}

#[test]
fn buffer_read_of_a_written_range_is_raw_ordered() {
    let writer = buffer_pass(
        "writer",
        vec![buffer_access(rid(1), ResourceAccessMode::Write, 0, 64)],
    );
    let reader = buffer_pass(
        "reader",
        vec![buffer_access(rid(1), ResourceAccessMode::Read, 32, 32)],
    );

    let plan = GraphCompiler::new(vec![writer, reader]).compile().unwrap();

    assert!(plan.dag[1].predecessors.contains(&0));
}

#[test]
fn buffer_read_of_a_disjoint_range_needs_no_ordering() {
    let writer = buffer_pass(
        "writer",
        vec![buffer_access(rid(1), ResourceAccessMode::Write, 0, 64)],
    );
    let reader = buffer_pass(
        "reader",
        vec![buffer_access(rid(1), ResourceAccessMode::Read, 128, 64)],
    );

    let plan = GraphCompiler::new(vec![writer, reader]).compile().unwrap();

    assert!(plan.dag[1].predecessors.is_empty());
}

#[test]
fn a_later_buffer_write_waits_for_an_overlapping_reader() {
    let reader = buffer_pass(
        "reader",
        vec![buffer_access(rid(1), ResourceAccessMode::Read, 0, 64)],
    );
    let writer = buffer_pass(
        "writer",
        vec![buffer_access(rid(1), ResourceAccessMode::Write, 32, 64)],
    );

    let plan = GraphCompiler::new(vec![reader, writer]).compile().unwrap();

    assert!(plan.dag[1].predecessors.contains(&0));
}

#[test]
fn a_partial_buffer_write_leaves_the_untouched_range_reusable() {
    // Pass A writes bytes 0..64, pass B writes 64..128, then pass C reads
    // 0..64. Only A must precede C; B covers a disjoint range.
    let a = buffer_pass(
        "a",
        vec![buffer_access(rid(1), ResourceAccessMode::Write, 0, 64)],
    );
    let b = buffer_pass(
        "b",
        vec![buffer_access(rid(1), ResourceAccessMode::Write, 64, 64)],
    );
    let c = buffer_pass(
        "c",
        vec![buffer_access(rid(1), ResourceAccessMode::Read, 0, 64)],
    );

    let plan = GraphCompiler::new(vec![a, b, c]).compile().unwrap();

    assert!(plan.dag[2].predecessors.contains(&0));
    assert!(!plan.dag[2].predecessors.contains(&1));
}

#[test]
fn buffer_read_modify_write_orders_both_directions_without_a_self_edge() {
    let rmw = buffer_pass(
        "rmw",
        vec![buffer_access(rid(1), ResourceAccessMode::ReadWrite, 0, 64)],
    );

    let plan = GraphCompiler::new(vec![rmw]).compile().unwrap();

    assert!(plan.dag[0].predecessors.is_empty());
    assert!(plan.dag[0].successors.is_empty());
}

#[test]
fn an_unbounded_buffer_range_covers_every_later_access() {
    let whole = buffer_pass(
        "whole",
        vec![buffer_access(
            rid(1),
            ResourceAccessMode::Write,
            8,
            u64::MAX,
        )],
    );
    let tail = buffer_pass(
        "tail",
        vec![buffer_access(rid(1), ResourceAccessMode::Read, 4096, 16)],
    );

    let plan = GraphCompiler::new(vec![whole, tail]).compile().unwrap();

    assert!(plan.dag[1].predecessors.contains(&0));
}

#[test]
fn buffer_and_image_accesses_to_one_id_do_not_order_each_other() {
    // The resource-id namespace is shared, but byte ranges and subresource
    // ranges are different axes: a buffer access cannot order an image
    // access to the same id.
    let buffer_write = buffer_pass(
        "buffer_write",
        vec![buffer_access(rid(1), ResourceAccessMode::Write, 0, 64)],
    );
    let image_read = typed_pass(
        "image_read",
        vec![access(rid(1), ResourceAccessMode::Read, mips(0, 1))],
    );

    let plan = GraphCompiler::new(vec![buffer_write, image_read])
        .compile()
        .unwrap();

    assert!(plan.dag[1].predecessors.is_empty());
}

#[test]
fn disjoint_mip_writes_are_independent() {
    let a = typed_pass(
        "a",
        vec![access(rid(1), ResourceAccessMode::Write, mips(0, 1))],
    );
    let b = typed_pass(
        "b",
        vec![access(rid(1), ResourceAccessMode::Write, mips(1, 1))],
    );
    let plan = GraphCompiler::new(vec![a, b]).compile().unwrap();
    assert!(plan.dag[0].successors.is_empty());
    assert!(plan.dag[1].predecessors.is_empty());
    // No ordering constraint: both passes sit at parallel level 0.
    assert_eq!(plan.dag[0].level, 0);
    assert_eq!(plan.dag[1].level, 0);
}

#[test]
fn overlapping_mip_writes_stay_waw_ordered() {
    let a = typed_pass(
        "a",
        vec![access(rid(1), ResourceAccessMode::Write, mips(0, 2))],
    );
    let b = typed_pass(
        "b",
        vec![access(rid(1), ResourceAccessMode::Write, mips(1, 1))],
    );
    let plan = GraphCompiler::new(vec![a, b]).compile().unwrap();
    assert_eq!(plan.dag[0].successors, vec![1]);
    assert_eq!(plan.dag[1].predecessors, vec![0]);
}

#[test]
fn raw_reaches_only_overlapping_subresources() {
    let writer = typed_pass(
        "writer",
        vec![access(rid(1), ResourceAccessMode::Write, mips(0, 2))],
    );
    let reader_low = typed_pass(
        "reader_low",
        vec![access(rid(1), ResourceAccessMode::Read, mips(0, 1))],
    );
    let reader_high = typed_pass(
        "reader_high",
        vec![access(rid(1), ResourceAccessMode::Read, mips(2, 1))],
    );
    let plan = GraphCompiler::new(vec![writer, reader_low, reader_high])
        .compile()
        .unwrap();
    assert!(plan.dag[0].successors.contains(&1));
    assert!(!plan.dag[0].successors.contains(&2));
    assert!(plan.dag[1].predecessors.contains(&0));
    assert!(plan.dag[2].predecessors.is_empty());
}

#[test]
fn aspect_disjoint_accesses_are_independent() {
    let color = typed_pass(
        "color",
        vec![access(
            rid(1),
            ResourceAccessMode::Write,
            ImageSubresourceRange::WHOLE_COLOR,
        )],
    );
    let depth = typed_pass(
        "depth",
        vec![access(
            rid(1),
            ResourceAccessMode::Read,
            ImageSubresourceRange::WHOLE_DEPTH,
        )],
    );
    let plan = GraphCompiler::new(vec![color, depth]).compile().unwrap();
    assert!(plan.dag[0].successors.is_empty());
    assert!(plan.dag[1].predecessors.is_empty());
}

#[test]
fn read_modify_write_never_self_depends() {
    let start = typed_pass(
        "start",
        vec![access(rid(1), ResourceAccessMode::Write, mips(0, 2))],
    );
    let rmw = typed_pass(
        "rmw",
        vec![access(rid(1), ResourceAccessMode::ReadWrite, mips(0, 2))],
    );
    let plan = GraphCompiler::new(vec![start, rmw]).compile().unwrap();
    // RAW from the producer, no self edge, ordered chain.
    assert_eq!(plan.dag[0].successors, vec![1]);
    assert_eq!(plan.dag[1].predecessors, vec![0]);
    assert!(!plan.dag[1].successors.contains(&1));
}

#[test]
fn partial_overwrite_keeps_versions_for_the_remaining_range() {
    let reader = typed_pass(
        "reader",
        vec![access(rid(1), ResourceAccessMode::Read, mips(0, 2))],
    );
    let overwriter = typed_pass(
        "overwriter",
        vec![access(rid(1), ResourceAccessMode::Write, mips(0, 1))],
    );
    let late_reader = typed_pass(
        "late_reader",
        vec![access(rid(1), ResourceAccessMode::Read, mips(1, 1))],
    );
    let plan = GraphCompiler::new(vec![reader, overwriter, late_reader])
        .compile()
        .unwrap();
    // WAR: the overwriter must wait for the reader it partially replaces.
    assert!(plan.dag[0].successors.contains(&1));
    // The late reader of the untouched mip still reads the ORIGINAL
    // version: no producer exists for it, and it must not read from the
    // overwriter.
    assert!(plan.dag[2].predecessors.is_empty());
    assert!(!plan.dag[1].successors.contains(&2));
}

#[test]
fn disjoint_array_layers_are_independent() {
    let layer0 = typed_pass(
        "layer0",
        vec![access(
            rid(1),
            ResourceAccessMode::Write,
            ImageSubresourceRange::new(ImageAspects::COLOR, 0, 1, 0, 1),
        )],
    );
    let layer1 = typed_pass(
        "layer1",
        vec![access(
            rid(1),
            ResourceAccessMode::Write,
            ImageSubresourceRange::new(ImageAspects::COLOR, 0, 1, 1, 1),
        )],
    );
    let plan = GraphCompiler::new(vec![layer0, layer1]).compile().unwrap();
    assert!(plan.dag[0].successors.is_empty());
    assert!(plan.dag[1].predecessors.is_empty());
    assert_eq!(plan.dag[1].level, 0);
}

#[test]
fn explicit_subrange_reads_order_after_the_writing_pass() {
    let writer = typed_pass(
        "writer",
        vec![access(rid(1), ResourceAccessMode::Write, mips(0, 4))],
    );
    let mip_reader = typed_pass(
        "mip_reader",
        vec![access(rid(1), ResourceAccessMode::Read, mips(3, 1))],
    );
    let plan = GraphCompiler::new(vec![writer, mip_reader])
        .compile()
        .unwrap();
    assert_eq!(plan.dag[0].successors, vec![1]);
}

fn compile(passes: Vec<PassInfo>) -> ExecutionPlan {
    GraphCompiler::new(passes).compile().unwrap()
}

#[test]
fn topological_sort_preserves_dependency_chain() {
    let plan = compile(vec![
        make_pass("A", vec![], vec![rid(0)]),
        make_pass("B", vec![rid(0)], vec![rid(1)]),
        make_pass("C", vec![rid(1)], vec![]),
    ]);

    assert_eq!(plan.sorted_passes, vec![0, 1, 2]);
}

#[test]
fn independent_passes_keep_stable_declaration_order() {
    let passes = vec![
        make_pass("A", vec![], vec![rid(0)]),
        make_pass("B", vec![], vec![rid(1)]),
        make_pass("C", vec![], vec![]),
    ];

    for _ in 0..32 {
        assert_eq!(compile(passes.clone()).sorted_passes, vec![0, 1, 2]);
    }
}

#[test]
fn read_before_later_write_is_war_not_a_false_cycle() {
    let plan = compile(vec![
        make_pass("ReadImported", vec![rid(0)], vec![]),
        make_pass("Replace", vec![], vec![rid(0)]),
        make_pass("ReadReplacement", vec![rid(0)], vec![]),
    ]);

    assert_eq!(plan.sorted_passes, vec![0, 1, 2]);
    assert_eq!(plan.dag[0].successors, vec![1]);
    assert_eq!(plan.dag[1].predecessors, vec![0]);
    assert_eq!(plan.dag[1].successors, vec![2]);
    assert_eq!(plan.dag[2].predecessors, vec![1]);
}

#[test]
fn resource_feedback_names_are_versioned_by_declaration_order() {
    let plan = compile(vec![
        make_pass("A", vec![rid(2)], vec![rid(0)]),
        make_pass("B", vec![rid(0)], vec![rid(1)]),
        make_pass("C", vec![rid(1)], vec![rid(2)]),
    ]);

    assert_eq!(plan.sorted_passes, vec![0, 1, 2]);
    assert_eq!(plan.dag[0].successors, vec![1, 2]);
    assert_eq!(plan.dag[1].successors, vec![2]);
}

#[test]
fn cycle_diagnostics_report_a_closed_stable_path() {
    let mut compiler = GraphCompiler::new(vec![
        make_pass("A", vec![], vec![]),
        make_pass("B", vec![], vec![]),
    ]);
    compiler.dependency_graph = vec![DependencyNode::default(); 2];
    add_dependency(&mut compiler.dependency_graph, 0, 1);
    add_dependency(&mut compiler.dependency_graph, 1, 0);

    assert_eq!(compiler.detect_cycle(), Some(vec![0, 1, 0]));
    assert_eq!(
        compiler.topological_sort().unwrap_err(),
        "Cycle detected involving passes: A -> B -> A"
    );
}

#[test]
fn raw_dependency_links_latest_writer_to_reader() {
    let plan = compile(vec![
        make_pass("Writer", vec![], vec![rid(0)]),
        make_pass("Reader", vec![rid(0)], vec![]),
    ]);

    assert_eq!(plan.dag[0].successors, vec![1]);
    assert_eq!(plan.dag[1].predecessors, vec![0]);
}

#[test]
fn waw_dependency_links_consecutive_writers() {
    let plan = compile(vec![
        make_pass("WriterA", vec![], vec![rid(0)]),
        make_pass("WriterB", vec![], vec![rid(0)]),
    ]);

    assert_eq!(plan.dag[0].successors, vec![1]);
    assert_eq!(plan.dag[1].predecessors, vec![0]);
}

#[test]
fn war_dependency_waits_for_every_reader_of_replaced_version() {
    let plan = compile(vec![
        make_pass("ReaderA", vec![rid(0)], vec![]),
        make_pass("ReaderB", vec![rid(0)], vec![]),
        make_pass("Writer", vec![], vec![rid(0)]),
    ]);

    assert_eq!(plan.dag[2].predecessors, vec![0, 1]);
    assert_eq!(plan.parallel_groups, vec![vec![0, 1], vec![2]]);
}

#[test]
fn writer_chain_uses_minimal_transitive_edges() {
    let plan = compile(vec![
        make_pass("WriterA", vec![], vec![rid(0)]),
        make_pass("WriterB", vec![], vec![rid(0)]),
        make_pass("Reader", vec![rid(0)], vec![]),
    ]);

    assert_eq!(plan.dag[0].successors, vec![1]);
    assert_eq!(plan.dag[1].successors, vec![2]);
    assert_eq!(plan.dag[2].predecessors, vec![1]);
    assert_eq!(plan.sorted_passes, vec![0, 1, 2]);
}

#[test]
fn read_modify_write_pass_does_not_depend_on_itself() {
    let plan = compile(vec![
        make_pass("Writer", vec![], vec![rid(0)]),
        make_pass("ReadModifyWrite", vec![rid(0)], vec![rid(0)]),
        make_pass("Reader", vec![rid(0)], vec![]),
    ]);

    assert_eq!(plan.dag[1].predecessors, vec![0]);
    assert_eq!(plan.dag[1].successors, vec![2]);
}

#[test]
fn diamond_dependencies_create_expected_parallel_groups() {
    let plan = compile(vec![
        make_pass("A", vec![], vec![rid(0)]),
        make_pass("B", vec![rid(0)], vec![rid(1)]),
        make_pass("C", vec![rid(0)], vec![rid(2)]),
        make_pass("D", vec![rid(1), rid(2)], vec![]),
    ]);

    assert_eq!(plan.sorted_passes, vec![0, 1, 2, 3]);
    assert_eq!(plan.parallel_groups, vec![vec![0], vec![1, 2], vec![3]]);
    assert_eq!(plan.dag[0].level, 0);
    assert_eq!(plan.dag[1].level, 1);
    assert_eq!(plan.dag[2].level, 1);
    assert_eq!(plan.dag[3].level, 2);
}

#[test]
fn execution_plan_views_share_the_same_edges_and_levels() {
    let plan = compile(vec![
        make_pass("A", vec![], vec![rid(0)]),
        make_pass("B", vec![], vec![rid(1)]),
        make_pass("C", vec![rid(0), rid(1)], vec![rid(2)]),
        make_pass("D", vec![rid(2)], vec![]),
    ]);
    let positions: HashMap<usize, usize> = plan
        .sorted_passes
        .iter()
        .enumerate()
        .map(|(position, &pass)| (pass, position))
        .collect();

    for node in &plan.dag {
        for &predecessor in &node.predecessors {
            assert!(positions[&predecessor] < positions[&node.pass_index]);
            assert!(plan.dag[predecessor].successors.contains(&node.pass_index));
            assert!(plan.dag[predecessor].level < node.level);
        }
    }

    for (level, group) in plan.parallel_groups.iter().enumerate() {
        assert!(group.iter().all(|&pass| plan.dag[pass].level == level));
    }
}

#[test]
fn empty_and_single_pass_graphs_compile() {
    assert!(compile(Vec::new()).sorted_passes.is_empty());

    let plan = compile(vec![make_pass("Solo", vec![], vec![])]);
    assert_eq!(plan.sorted_passes, vec![0]);
    assert_eq!(plan.parallel_groups, vec![vec![0]]);
}

fn compile_with_exports(passes: Vec<PassInfo>, exports: &[ResourceId]) -> ExecutionPlan {
    GraphCompiler::with_exports(passes, exports.iter().copied())
        .compile()
        .unwrap()
}

#[test]
fn test_export_retains_latest_disjoint_buffer_versions() {
    let plan = compile_with_exports(
        vec![
            buffer_pass(
                "initial",
                vec![buffer_access(rid(1), ResourceAccessMode::Write, 0, 128)],
            ),
            buffer_pass(
                "first half",
                vec![buffer_access(rid(1), ResourceAccessMode::Write, 0, 64)],
            ),
            buffer_pass(
                "second half",
                vec![buffer_access(rid(1), ResourceAccessMode::Write, 64, 64)],
            ),
        ],
        &[rid(1)],
    );
    assert_eq!(plan.sorted_passes, vec![1, 2]);
    assert_eq!(plan.culled_passes, vec![0]);
    assert_eq!(plan.liveness_roots, vec![1, 2]);
}

#[test]
fn test_export_retains_partially_overwritten_buffer_producer() {
    let plan = compile_with_exports(
        vec![
            buffer_pass(
                "initial",
                vec![buffer_access(rid(1), ResourceAccessMode::Write, 0, 128)],
            ),
            buffer_pass(
                "middle",
                vec![buffer_access(rid(1), ResourceAccessMode::Write, 32, 64)],
            ),
        ],
        &[rid(1)],
    );
    assert_eq!(plan.sorted_passes, vec![0, 1]);
    assert_eq!(plan.liveness_roots, vec![0, 1]);
}

#[test]
fn test_export_retains_latest_mips_layers_and_aspects() {
    let ranges = [
        ImageSubresourceRange::new(ImageAspects::COLOR, 0, 1, 0, 1),
        ImageSubresourceRange::new(ImageAspects::COLOR, 1, 1, 0, 1),
        ImageSubresourceRange::new(ImageAspects::COLOR, 0, 1, 1, 1),
        ImageSubresourceRange::new(ImageAspects::DEPTH, 0, 1, 0, 1),
        ImageSubresourceRange::new(ImageAspects::STENCIL, 0, 1, 0, 1),
    ];
    let mut passes = vec![typed_pass(
        "overwritten",
        vec![access(rid(1), ResourceAccessMode::Write, ranges[0])],
    )];
    passes.extend(ranges.map(|range| {
        typed_pass(
            "latest",
            vec![access(rid(1), ResourceAccessMode::Write, range)],
        )
    }));
    let plan = compile_with_exports(passes, &[rid(1)]);
    assert_eq!(plan.sorted_passes, vec![1, 2, 3, 4, 5]);
    assert_eq!(plan.culled_passes, vec![0]);
    assert_eq!(plan.liveness_roots, vec![1, 2, 3, 4, 5]);
}

#[test]
fn culls_dead_branches_from_explicit_exports() {
    let plan = compile_with_exports(
        vec![
            make_pass("live_source", vec![], vec![rid(0)]),
            make_pass("live_present", vec![rid(0)], vec![rid(1)]),
            make_pass("dead_source", vec![], vec![rid(2)]),
            make_pass("dead_consumer", vec![rid(2)], vec![rid(3)]),
        ],
        &[rid(1)],
    );
    assert_eq!(plan.sorted_passes, vec![0, 1]);
    assert_eq!(plan.culled_passes, vec![2, 3]);
    assert_eq!(plan.live_passes, vec![true, true, false, false]);
}

#[test]
fn keeps_shared_producers_for_multiple_live_consumers() {
    let plan = compile_with_exports(
        vec![
            make_pass("shared", vec![], vec![rid(0)]),
            make_pass("left", vec![rid(0)], vec![rid(1)]),
            make_pass("right", vec![rid(0)], vec![rid(2)]),
        ],
        &[rid(1), rid(2)],
    );
    assert_eq!(plan.sorted_passes, vec![0, 1, 2]);
    assert!(plan.culled_passes.is_empty());
}

#[test]
fn side_effect_passes_are_liveness_roots() {
    let mut side_effect = make_pass("timestamp", vec![rid(0)], vec![]);
    side_effect.side_effect = true;
    let plan = compile_with_exports(
        vec![
            make_pass("producer", vec![], vec![rid(0)]),
            side_effect,
            make_pass("dead", vec![], vec![rid(1)]),
        ],
        &[],
    );
    assert_eq!(plan.sorted_passes, vec![0, 1]);
    assert_eq!(plan.culled_passes, vec![2]);
}

#[test]
fn imported_writes_are_not_implicit_side_effects() {
    let plan = compile_with_exports(vec![make_pass("write_imported", vec![], vec![rid(7)])], &[]);
    assert!(plan.sorted_passes.is_empty());
    assert_eq!(plan.culled_passes, vec![0]);
}

#[test]
fn fully_culled_graph_has_no_execution_or_parallel_work() {
    let plan = compile_with_exports(
        vec![
            make_pass("a", vec![], vec![rid(0)]),
            make_pass("b", vec![rid(0)], vec![rid(1)]),
        ],
        &[],
    );
    assert!(plan.sorted_passes.is_empty());
    assert!(plan.parallel_groups.is_empty());
    assert_eq!(plan.culled_passes, vec![0, 1]);
}

#[test]
fn compiles_sync_ops_from_the_live_dependency_dag() {
    let plan = compile(vec![
        make_pass("write", vec![], vec![rid(0)]),
        make_pass("read", vec![rid(0)], vec![]),
        make_pass("replace", vec![], vec![rid(0)]),
        make_pass("read_replacement", vec![rid(0)], vec![]),
    ]);

    // The steady-state cycle: the write replaces the previous frame's
    // final sampled state, then each hazard boundary transitions r0.
    let storage_write = ImageSyncState::Access {
        usage: ResourceAccessUsage::Storage,
        stage: ResourceAccessStage::AllGraphics,
        mode: ResourceAccessMode::Write,
    };
    let sampled_read = ImageSyncState::Access {
        usage: ResourceAccessUsage::Sampled,
        stage: ResourceAccessStage::FragmentShader,
        mode: ResourceAccessMode::Read,
    };
    let op = |pass: usize| plan.sync.pass_ops[pass].as_slice();
    assert_eq!(
        op(0),
        &[ImageSyncOp {
            resource: rid(0),
            range: ImageSubresourceRange::WHOLE_COLOR,
            before: sampled_read,
            after: storage_write,
            before_pass: None,
            pass: 0,
            reason: SyncReason::InitialUse,
        }]
    );
    // The whole-resource read also carries an aspect-fragment bootstrap;
    // the color range itself is the RAW hazard.
    let raw_at_1: Vec<ImageSyncOp> = op(1)
        .iter()
        .copied()
        .filter(|op| op.range == ImageSubresourceRange::WHOLE_COLOR)
        .collect();
    assert_eq!(
        raw_at_1,
        vec![ImageSyncOp {
            resource: rid(0),
            range: ImageSubresourceRange::WHOLE_COLOR,
            before: storage_write,
            after: sampled_read,
            before_pass: Some(0),
            pass: 1,
            reason: SyncReason::Hazard(ResourceHazardKind::ReadAfterWrite),
        }]
    );
    assert_eq!(
        op(2),
        &[
            ImageSyncOp {
                resource: rid(0),
                range: ImageSubresourceRange::WHOLE_COLOR,
                before: sampled_read,
                after: storage_write,
                before_pass: Some(1),
                pass: 2,
                reason: SyncReason::Hazard(ResourceHazardKind::WriteAfterRead),
            },
            ImageSyncOp {
                resource: rid(0),
                range: ImageSubresourceRange::WHOLE_COLOR,
                before: storage_write,
                after: storage_write,
                before_pass: Some(0),
                pass: 2,
                reason: SyncReason::Hazard(ResourceHazardKind::WriteAfterWrite),
            },
        ]
    );
    assert_eq!(
        op(3),
        &[ImageSyncOp {
            resource: rid(0),
            range: ImageSubresourceRange::WHOLE_COLOR,
            before: storage_write,
            after: sampled_read,
            before_pass: Some(2),
            pass: 3,
            reason: SyncReason::Hazard(ResourceHazardKind::ReadAfterWrite),
        }]
    );
    assert!(plan.sync.final_ops.is_empty());
}

#[test]
fn culled_passes_emit_no_sync_ops() {
    let plan = compile_with_exports(
        vec![
            make_pass("live", vec![], vec![rid(0)]),
            make_pass("dead_writer", vec![], vec![rid(1)]),
            make_pass("dead_reader", vec![rid(1)], vec![rid(2)]),
        ],
        &[rid(0)],
    );

    assert_eq!(plan.culled_passes, vec![1, 2]);
    assert!(plan.sync.pass_ops[1].is_empty());
    assert!(plan.sync.pass_ops[2].is_empty());
    // The live pass keeps only its fresh-texture bootstrap operation.
    assert!(
        plan.sync.pass_ops[0]
            .iter()
            .all(|op| op.reason == SyncReason::InitialUse)
    );
    assert!(plan.sync.final_ops.is_empty());
}

#[test]
fn compiles_live_resource_lifetimes_in_execution_coordinates() {
    let plan = compile(vec![
        make_pass("write", vec![], vec![rid(0)]),
        make_pass("unrelated", vec![], vec![rid(1)]),
        make_pass("read", vec![rid(0)], vec![]),
    ]);

    assert_eq!(
        plan.resource_lifetimes.get(&rid(0)),
        Some(&ResourceLifetime {
            first_execution_position: 0,
            first_pass: 0,
            last_execution_position: 2,
            last_pass: 2,
        })
    );
    assert_eq!(
        plan.resource_lifetimes.get(&rid(1)),
        Some(&ResourceLifetime {
            first_execution_position: 1,
            first_pass: 1,
            last_execution_position: 1,
            last_pass: 1,
        })
    );
}

#[test]
fn culled_resource_accesses_do_not_extend_live_lifetimes() {
    let plan = compile_with_exports(
        vec![
            make_pass("live_writer", vec![], vec![rid(0)]),
            make_pass("live_present", vec![rid(0)], vec![rid(1)]),
            make_pass("dead_reader", vec![rid(0)], vec![rid(2)]),
        ],
        &[rid(1)],
    );

    assert_eq!(plan.culled_passes, vec![2]);
    assert_eq!(
        plan.resource_lifetimes.get(&rid(0)),
        Some(&ResourceLifetime {
            first_execution_position: 0,
            first_pass: 0,
            last_execution_position: 1,
            last_pass: 1,
        })
    );
    assert!(!plan.resource_lifetimes.contains_key(&rid(2)));
}

#[test]
fn replacing_an_export_does_not_keep_dead_previous_version_readers() {
    let plan = compile_with_exports(
        vec![
            make_pass("old_writer", vec![], vec![rid(0)]),
            make_pass("dead_reader", vec![rid(0)], vec![rid(1)]),
            make_pass("final_writer", vec![], vec![rid(0)]),
        ],
        &[rid(0)],
    );
    assert_eq!(plan.sorted_passes, vec![2]);
    assert_eq!(plan.culled_passes, vec![0, 1]);
}
#[test]
fn test_undefined_import_load_requires_an_authored_producer_for_each_aspect() {
    use crate::render_pass::{AttachmentOps, ClearValue};
    for aspects in [
        ImageAspects::COLOR,
        ImageAspects::DEPTH,
        ImageAspects::STENCIL,
    ] {
        let range = ImageSubresourceRange::whole(aspects);
        let (usage, stage) = if aspects == ImageAspects::COLOR {
            (
                ResourceAccessUsage::ColorAttachment,
                ResourceAccessStage::ColorAttachmentOutput,
            )
        } else {
            (
                ResourceAccessUsage::DepthStencilAttachment,
                ResourceAccessStage::DepthStencil,
            )
        };
        let mut load = typed_pass(
            "load",
            vec![ImageAccess::new(
                rid(0),
                ResourceAccessMode::ReadWrite,
                usage,
                stage,
                range,
            )],
        );
        load.attachment_ops
            .push((rid(0), aspects, AttachmentOps::load()));
        let mut compiler = GraphCompiler::new(vec![load.clone()]);
        compiler
            .imported_contracts
            .insert(rid(0), ImportedImageContract::undefined());
        assert!(matches!(
            compiler.compile(),
            Err(RenderGraphError::Validation(
                super::super::GraphValidationError::LoadingUninitializedImport { resource: 0, .. }
            ))
        ));
        let mut clear = typed_pass(
            "clear",
            vec![ImageAccess::new(
                rid(0),
                ResourceAccessMode::Write,
                usage,
                stage,
                range,
            )],
        );
        clear.attachment_ops.push((
            rid(0),
            aspects,
            AttachmentOps::clear(if aspects == ImageAspects::COLOR {
                ClearValue::OPAQUE_BLACK
            } else {
                ClearValue::DepthStencil {
                    depth: 0.0,
                    stencil: 0,
                }
            }),
        ));
        let mut compiler = GraphCompiler::new(vec![clear, load.clone()]);
        compiler
            .imported_contracts
            .insert(rid(0), ImportedImageContract::undefined());
        assert!(compiler.compile().is_ok());
        let mut compiler = GraphCompiler::new(vec![load]);
        compiler.imported_contracts.insert(
            rid(0),
            ImportedImageContract::arrives_in(super::super::ResourceState::ShaderRead),
        );
        assert!(compiler.compile().is_ok());
    }
}

#[test]
fn test_discarded_import_attachment_cannot_supply_a_later_load() {
    use crate::render_pass::{AttachmentOps, ClearValue, StoreOp};
    let mut clear = typed_pass(
        "clear discarded",
        vec![ImageAccess::color_attachment_write(rid(0))],
    );
    clear.attachment_ops.push((
        rid(0),
        ImageAspects::COLOR,
        AttachmentOps {
            store: StoreOp::DontCare,
            ..AttachmentOps::clear(ClearValue::OPAQUE_BLACK)
        },
    ));
    let mut load = typed_pass(
        "load discarded",
        vec![ImageAccess::color_attachment_read_write(rid(0))],
    );
    load.attachment_ops
        .push((rid(0), ImageAspects::COLOR, AttachmentOps::load()));
    let mut compiler = GraphCompiler::new(vec![clear, load]);
    compiler.imported_contracts.insert(
        rid(0),
        ImportedImageContract::arrives_in(super::super::ResourceState::ShaderRead),
    );
    assert!(matches!(
        compiler.compile(),
        Err(RenderGraphError::Validation(
            super::super::GraphValidationError::LoadingUninitializedImport { .. }
        ))
    ));
}
