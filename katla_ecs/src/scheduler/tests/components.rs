use super::*;

#[test]
fn test_write_write_same_component_creates_edge() {
    let type_id = TypeId::of::<u32>();
    let systems = make_systems(vec![
        vec![ComponentAccess::Write(type_id)],
        vec![ComponentAccess::Write(type_id)],
    ]);

    let scheduler = SystemScheduler::build(&systems).unwrap();
    let groups = scheduler.groups();

    assert_eq!(groups.len(), 2);
    assert!(groups[0].contains(&0));
    assert!(groups[1].contains(&1));
}

#[test]
fn test_write_different_components_no_conflict() {
    let systems = make_systems(vec![
        vec![ComponentAccess::Write(TypeId::of::<u32>())],
        vec![ComponentAccess::Write(TypeId::of::<u64>())],
    ]);

    let scheduler = SystemScheduler::build(&systems).unwrap();
    let groups = scheduler.groups();

    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].len(), 2);
}

#[test]
fn test_read_read_same_component_no_conflict() {
    let type_id = TypeId::of::<u32>();
    let systems = make_systems(vec![
        vec![ComponentAccess::Read(type_id)],
        vec![ComponentAccess::Read(type_id)],
    ]);

    let scheduler = SystemScheduler::build(&systems).unwrap();
    let groups = scheduler.groups();

    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].len(), 2);
}

#[test]
fn test_read_write_same_component_creates_edge() {
    let type_id = TypeId::of::<u32>();
    let systems = make_systems(vec![
        vec![ComponentAccess::Read(type_id)],
        vec![ComponentAccess::Write(type_id)],
    ]);

    let scheduler = SystemScheduler::build(&systems).unwrap();
    let groups = scheduler.groups();

    assert_eq!(groups.len(), 2);
    assert!(groups[0].contains(&0));
    assert!(groups[1].contains(&1));
}

#[test]
fn test_chain_three_groups() {
    // A writes X, B reads X writes Y, C reads Y -> 3 groups: (A), (B), (C)
    let type_x = TypeId::of::<u32>();
    let type_y = TypeId::of::<u64>();

    let systems = make_systems(vec![
        vec![ComponentAccess::Write(type_x)], // A
        vec![
            ComponentAccess::Read(type_x),
            ComponentAccess::Write(type_y),
        ], // B
        vec![ComponentAccess::Read(type_y)],  // C
    ]);

    let scheduler = SystemScheduler::build(&systems).unwrap();
    let groups = scheduler.groups();

    assert_eq!(groups.len(), 3);
    assert!(groups[0].contains(&0));
    assert!(groups[1].contains(&1));
    assert!(groups[2].contains(&2));
}

#[test]
fn test_diamond_dependency() {
    // A writes X, B reads X writes Y, C reads X writes Z, D reads Y+Z
    let type_x = TypeId::of::<u32>();
    let type_y = TypeId::of::<u64>();
    let type_z = TypeId::of::<i32>();

    let systems = make_systems(vec![
        vec![ComponentAccess::Write(type_x)], // A
        vec![
            ComponentAccess::Read(type_x),
            ComponentAccess::Write(type_y),
        ], // B
        vec![
            ComponentAccess::Read(type_x),
            ComponentAccess::Write(type_z),
        ], // C
        vec![ComponentAccess::Read(type_y), ComponentAccess::Read(type_z)], // D
    ]);

    let scheduler = SystemScheduler::build(&systems).unwrap();
    let groups = scheduler.groups();

    // Group 0: A
    // Group 1: B, C (both read X, write different components)
    // Group 2: D (reads Y and Z)
    assert_eq!(groups.len(), 3);
    assert_eq!(groups[0], vec![0]);
    assert_eq!(groups[1].len(), 2);
    assert!(groups[1].contains(&1));
    assert!(groups[1].contains(&2));
    assert_eq!(groups[2], vec![3]);
}

#[test]
fn test_empty_systems() {
    let scheduler = SystemScheduler::build(&[]).unwrap();
    assert!(scheduler.groups().is_empty());
}

#[test]
fn test_single_system() {
    let systems = make_systems(vec![vec![ComponentAccess::Write(TypeId::of::<u32>())]]);
    let scheduler = SystemScheduler::build(&systems).unwrap();

    assert_eq!(scheduler.groups().len(), 1);
    assert_eq!(scheduler.groups()[0], vec![0]);
}

#[test]
fn test_no_access_no_conflict() {
    let systems = make_systems(vec![Vec::new(), Vec::new()]);
    let scheduler = SystemScheduler::build(&systems).unwrap();

    assert_eq!(scheduler.groups().len(), 1);
    assert_eq!(scheduler.groups()[0].len(), 2);
}

#[test]
fn test_mixed_read_write_multiple_types() {
    // System 0: reads A, writes B
    // System 1: reads B, writes C
    // System 2: reads C
    // All sequential due to write-write conflicts on B and C
    let type_a = TypeId::of::<u8>();
    let type_b = TypeId::of::<u16>();
    let type_c = TypeId::of::<u32>();

    let systems = make_systems(vec![
        vec![
            ComponentAccess::Read(type_a),
            ComponentAccess::Write(type_b),
        ],
        vec![
            ComponentAccess::Read(type_b),
            ComponentAccess::Write(type_c),
        ],
        vec![ComponentAccess::Read(type_c)],
    ]);

    let scheduler = SystemScheduler::build(&systems).unwrap();
    let groups = scheduler.groups();

    assert_eq!(groups.len(), 3);
    assert!(groups[0].contains(&0));
    assert!(groups[1].contains(&1));
    assert!(groups[2].contains(&2));
}

#[test]
fn test_parallel_schedule_ordering() {
    // A writes X, B writes Y, C reads X+Y
    // A and B should run in parallel, C must wait for both
    let type_x = TypeId::of::<u32>();
    let type_y = TypeId::of::<u64>();

    let systems = make_systems(vec![
        vec![ComponentAccess::Write(type_x)], // A (index 0)
        vec![ComponentAccess::Write(type_y)], // B (index 1)
        vec![ComponentAccess::Read(type_x), ComponentAccess::Read(type_y)], // C (index 2)
    ]);

    let scheduler = SystemScheduler::build(&systems).unwrap();
    let groups = scheduler.groups();

    assert_eq!(groups.len(), 2);
    assert!(groups[0].contains(&0)); // A
    assert!(groups[0].contains(&1)); // B
    assert!(groups[1].contains(&2)); // C
}

#[test]
fn test_all_independent_single_group() {
    // A writes X, B writes Y, C writes Z — all in same group
    let type_x = TypeId::of::<u32>();
    let type_y = TypeId::of::<u64>();
    let type_z = TypeId::of::<i32>();

    let systems = make_systems(vec![
        vec![ComponentAccess::Write(type_x)], // A
        vec![ComponentAccess::Write(type_y)], // B
        vec![ComponentAccess::Write(type_z)], // C
    ]);

    let scheduler = SystemScheduler::build(&systems).unwrap();
    let groups = scheduler.groups();

    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].len(), 3);
    assert!(groups[0].contains(&0));
    assert!(groups[0].contains(&1));
    assert!(groups[0].contains(&2));
}

#[test]
fn test_all_conflicting_separate_groups() {
    // A writes X, B writes X, C writes X — all separate groups
    let type_x = TypeId::of::<u32>();

    let systems = make_systems(vec![
        vec![ComponentAccess::Write(type_x)], // A
        vec![ComponentAccess::Write(type_x)], // B
        vec![ComponentAccess::Write(type_x)], // C
    ]);

    let scheduler = SystemScheduler::build(&systems).unwrap();
    let groups = scheduler.groups();

    assert_eq!(groups.len(), 3);
    assert_eq!(groups[0], vec![0]);
    assert_eq!(groups[1], vec![1]);
    assert_eq!(groups[2], vec![2]);
}

#[test]
fn test_preserves_system_indices() {
    let systems: Vec<(usize, Vec<ComponentAccess>, Vec<ResourceAccess>)> = vec![
        (
            5,
            vec![ComponentAccess::Write(TypeId::of::<u32>())],
            Vec::new(),
        ),
        (
            10,
            vec![ComponentAccess::Write(TypeId::of::<u64>())],
            Vec::new(),
        ),
    ];

    let scheduler = SystemScheduler::build(&systems).unwrap();
    let groups = scheduler.groups();

    assert_eq!(groups.len(), 1);
    assert!(groups[0].contains(&5));
    assert!(groups[0].contains(&10));
}

#[test]
fn test_mutual_conflict_resolves_ordering() {
    // A reads X writes Y, B reads Y writes X.
    // Both conflict with each other, but build_edges only creates a
    // one-way edge (later system depends on earlier). This verifies
    // the scheduler doesn't panic and produces a valid ordering.
    let type_x = TypeId::of::<u32>();
    let type_y = TypeId::of::<u64>();

    let systems = make_systems(vec![
        vec![
            ComponentAccess::Read(type_x),
            ComponentAccess::Write(type_y),
        ],
        vec![
            ComponentAccess::Read(type_y),
            ComponentAccess::Write(type_x),
        ],
    ]);

    let scheduler = SystemScheduler::build(&systems).unwrap();
    let groups = scheduler.groups();

    assert_eq!(groups.len(), 2);
    assert!(groups[0].contains(&0));
    assert!(groups[1].contains(&1));
}

#[test]
fn test_large_dag_wide_fan_out() {
    // A writes X. Then 5 independent systems each read X and write unique types.
    // All 5 should be in a single parallel group after A.
    let type_x = TypeId::of::<u32>();
    let type_a = TypeId::of::<u8>();
    let type_b = TypeId::of::<u16>();
    let type_c = TypeId::of::<i8>();
    let type_d = TypeId::of::<i16>();
    let type_e = TypeId::of::<f32>();

    let systems = make_systems(vec![
        vec![ComponentAccess::Write(type_x)], // A (source)
        vec![
            ComponentAccess::Read(type_x),
            ComponentAccess::Write(type_a),
        ], // B
        vec![
            ComponentAccess::Read(type_x),
            ComponentAccess::Write(type_b),
        ], // C
        vec![
            ComponentAccess::Read(type_x),
            ComponentAccess::Write(type_c),
        ], // D
        vec![
            ComponentAccess::Read(type_x),
            ComponentAccess::Write(type_d),
        ], // E
        vec![
            ComponentAccess::Read(type_x),
            ComponentAccess::Write(type_e),
        ], // F
    ]);

    let scheduler = SystemScheduler::build(&systems).unwrap();
    let groups = scheduler.groups();

    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0], vec![0]); // A
    assert_eq!(groups[1].len(), 5); // B, C, D, E, F all parallel
    for i in 1..=5 {
        assert!(groups[1].contains(&i));
    }
}

#[test]
fn test_large_dag_multi_level() {
    // Level 0: A writes X, B writes Y (parallel, no conflict)
    // Level 1: C reads X+Y writes Z (depends on both A and B)
    // Level 2: D reads Z writes W, E reads Z writes V (parallel, both read Z)
    // Level 3: F reads W+V (depends on D and E)
    let type_x = TypeId::of::<u8>();
    let type_y = TypeId::of::<u16>();
    let type_z = TypeId::of::<u32>();
    let type_w = TypeId::of::<u64>();
    let type_v = TypeId::of::<i8>();

    let systems = make_systems(vec![
        vec![ComponentAccess::Write(type_x)], // A
        vec![ComponentAccess::Write(type_y)], // B
        vec![
            ComponentAccess::Read(type_x),
            ComponentAccess::Read(type_y),
            ComponentAccess::Write(type_z),
        ], // C
        vec![
            ComponentAccess::Read(type_z),
            ComponentAccess::Write(type_w),
        ], // D
        vec![
            ComponentAccess::Read(type_z),
            ComponentAccess::Write(type_v),
        ], // E
        vec![ComponentAccess::Read(type_w), ComponentAccess::Read(type_v)], // F
    ]);

    let scheduler = SystemScheduler::build(&systems).unwrap();
    let groups = scheduler.groups();

    assert_eq!(groups.len(), 4);
    assert_eq!(groups[0].len(), 2); // A, B
    assert!(groups[0].contains(&0));
    assert!(groups[0].contains(&1));
    assert_eq!(groups[1], vec![2]); // C (depends on A+B)
    assert_eq!(groups[2].len(), 2); // D, E (both read Z, write different)
    assert!(groups[2].contains(&3));
    assert!(groups[2].contains(&4));
    assert_eq!(groups[3], vec![5]); // F (reads W+V)
}

#[test]
fn test_partial_read_overlap_parallel() {
    // Sys0 reads A + writes B, Sys1 reads A + writes C.
    // Only read overlap on A — no conflict. Should be parallel.
    let type_a = TypeId::of::<u8>();
    let type_b = TypeId::of::<u16>();
    let type_c = TypeId::of::<u32>();

    let systems = make_systems(vec![
        vec![
            ComponentAccess::Read(type_a),
            ComponentAccess::Write(type_b),
        ],
        vec![
            ComponentAccess::Read(type_a),
            ComponentAccess::Write(type_c),
        ],
    ]);

    let scheduler = SystemScheduler::build(&systems).unwrap();
    let groups = scheduler.groups();

    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].len(), 2);
}

#[test]
fn test_multiple_readers_one_writer() {
    // Sys0 writes X. Sys1 reads X writes Y. Sys2 reads X writes Z.
    // Sys0 must run first, then Sys1 and Sys2 in parallel.
    let type_x = TypeId::of::<u32>();
    let type_y = TypeId::of::<u64>();
    let type_z = TypeId::of::<i32>();

    let systems = make_systems(vec![
        vec![ComponentAccess::Write(type_x)], // writer
        vec![
            ComponentAccess::Read(type_x),
            ComponentAccess::Write(type_y),
        ],
        vec![
            ComponentAccess::Read(type_x),
            ComponentAccess::Write(type_z),
        ],
    ]);

    let scheduler = SystemScheduler::build(&systems).unwrap();
    let groups = scheduler.groups();

    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0], vec![0]);
    assert_eq!(groups[1].len(), 2);
    assert!(groups[1].contains(&1));
    assert!(groups[1].contains(&2));
}

#[test]
fn test_system_with_many_components_partial_conflict() {
    // Sys0 writes A B C, Sys1 writes C D E — conflict only on C
    let type_a = TypeId::of::<u8>();
    let type_b = TypeId::of::<u16>();
    let type_c = TypeId::of::<u32>();
    let type_d = TypeId::of::<u64>();
    let type_e = TypeId::of::<i8>();

    let systems = make_systems(vec![
        vec![
            ComponentAccess::Write(type_a),
            ComponentAccess::Write(type_b),
            ComponentAccess::Write(type_c),
        ],
        vec![
            ComponentAccess::Write(type_c),
            ComponentAccess::Write(type_d),
            ComponentAccess::Write(type_e),
        ],
    ]);

    let scheduler = SystemScheduler::build(&systems).unwrap();
    let groups = scheduler.groups();

    assert_eq!(groups.len(), 2); // sequential due to C conflict
}

#[test]
fn test_write_read_read_chain() {
    // Sys0 writes X, Sys1 reads X, Sys2 reads X
    // Only Sys0 conflicts with Sys1 and Sys2. Sys1 and Sys2 have no conflict.
    // Group 0: Sys0, Group 1: Sys1 + Sys2
    let type_x = TypeId::of::<u32>();

    let systems = make_systems(vec![
        vec![ComponentAccess::Write(type_x)],
        vec![ComponentAccess::Read(type_x)],
        vec![ComponentAccess::Read(type_x)],
    ]);

    let scheduler = SystemScheduler::build(&systems).unwrap();
    let groups = scheduler.groups();

    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0], vec![0]);
    assert_eq!(groups[1].len(), 2);
    assert!(groups[1].contains(&1));
    assert!(groups[1].contains(&2));
}
