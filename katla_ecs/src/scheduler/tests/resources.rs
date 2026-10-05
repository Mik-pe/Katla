use super::*;

#[test]
fn test_resource_write_write_same_creates_edge() {
    let res_x = TypeId::of::<u32>();
    let systems = make_systems_with_resources(vec![
        (Vec::new(), vec![ResourceAccess::Write(res_x)]),
        (Vec::new(), vec![ResourceAccess::Write(res_x)]),
    ]);

    let scheduler = SystemScheduler::build(&systems).unwrap();
    let groups = scheduler.groups();

    assert_eq!(groups.len(), 2);
    assert!(groups[0].contains(&0));
    assert!(groups[1].contains(&1));
}

#[test]
fn test_resource_read_write_same_creates_edge() {
    let res_x = TypeId::of::<u32>();
    let systems = make_systems_with_resources(vec![
        (Vec::new(), vec![ResourceAccess::Read(res_x)]),
        (Vec::new(), vec![ResourceAccess::Write(res_x)]),
    ]);

    let scheduler = SystemScheduler::build(&systems).unwrap();
    let groups = scheduler.groups();

    assert_eq!(groups.len(), 2);
    assert!(groups[0].contains(&0));
    assert!(groups[1].contains(&1));
}

#[test]
fn test_resource_read_read_no_conflict() {
    let res_x = TypeId::of::<u32>();
    let systems = make_systems_with_resources(vec![
        (Vec::new(), vec![ResourceAccess::Read(res_x)]),
        (Vec::new(), vec![ResourceAccess::Read(res_x)]),
    ]);

    let scheduler = SystemScheduler::build(&systems).unwrap();
    let groups = scheduler.groups();

    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].len(), 2);
}

#[test]
fn test_resource_write_different_no_conflict() {
    let systems = make_systems_with_resources(vec![
        (Vec::new(), vec![ResourceAccess::Write(TypeId::of::<u32>())]),
        (Vec::new(), vec![ResourceAccess::Write(TypeId::of::<u64>())]),
    ]);

    let scheduler = SystemScheduler::build(&systems).unwrap();
    let groups = scheduler.groups();

    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].len(), 2);
}

#[test]
fn test_resource_conflict_independent_of_component_access() {
    // No component conflict, but resource write-write conflict
    let type_a = TypeId::of::<u32>();
    let res_x = TypeId::of::<u64>();

    let systems = make_systems_with_resources(vec![
        (
            vec![ComponentAccess::Write(type_a)],
            vec![ResourceAccess::Write(res_x)],
        ),
        (Vec::new(), vec![ResourceAccess::Write(res_x)]),
    ]);

    let scheduler = SystemScheduler::build(&systems).unwrap();
    let groups = scheduler.groups();

    assert_eq!(groups.len(), 2);
}

#[test]
fn test_component_and_resource_conflict_combined() {
    // Component conflict between sys0 and sys1, resource conflict between sys1 and sys2
    let type_a = TypeId::of::<u32>();
    let res_x = TypeId::of::<u64>();

    let systems = make_systems_with_resources(vec![
        (vec![ComponentAccess::Write(type_a)], Vec::new()),
        (
            vec![ComponentAccess::Write(type_a)],
            vec![ResourceAccess::Write(res_x)],
        ),
        (Vec::new(), vec![ResourceAccess::Write(res_x)]),
    ]);

    let scheduler = SystemScheduler::build(&systems).unwrap();
    let groups = scheduler.groups();

    assert_eq!(groups.len(), 3);
    assert_eq!(groups[0], vec![0]);
    assert_eq!(groups[1], vec![1]);
    assert_eq!(groups[2], vec![2]);
}

#[test]
fn test_resource_only_conflict_creates_groups() {
    let res_x = TypeId::of::<u32>();
    let systems = make_systems_with_resources(vec![
        (Vec::new(), vec![ResourceAccess::Write(res_x)]),
        (Vec::new(), vec![ResourceAccess::Write(res_x)]),
    ]);

    let scheduler = SystemScheduler::build(&systems).unwrap();
    let groups = scheduler.groups();

    assert_eq!(
        groups.len(),
        2,
        "write-write on same resource must split into separate groups"
    );
    assert!(groups[0].contains(&0));
    assert!(groups[1].contains(&1));
}

#[test]
fn test_resource_read_write_creates_dependency() {
    let res_x = TypeId::of::<u32>();
    let systems = make_systems_with_resources(vec![
        (Vec::new(), vec![ResourceAccess::Read(res_x)]),
        (Vec::new(), vec![ResourceAccess::Write(res_x)]),
    ]);

    let scheduler = SystemScheduler::build(&systems).unwrap();
    let groups = scheduler.groups();

    assert_eq!(
        groups.len(),
        2,
        "read-write on same resource must create dependency"
    );
    assert!(groups[0].contains(&0));
    assert!(groups[1].contains(&1));
}

#[test]
fn test_independent_resources_parallel_group() {
    let res_r1 = TypeId::of::<u8>();
    let res_r2 = TypeId::of::<u16>();
    let res_r3 = TypeId::of::<u32>();

    let systems = make_systems_with_resources(vec![
        (Vec::new(), vec![ResourceAccess::Write(res_r1)]),
        (Vec::new(), vec![ResourceAccess::Write(res_r2)]),
        (Vec::new(), vec![ResourceAccess::Write(res_r3)]),
    ]);

    let scheduler = SystemScheduler::build(&systems).unwrap();
    let groups = scheduler.groups();

    assert_eq!(
        groups.len(),
        1,
        "independent resource writes should be in one parallel group"
    );
    assert_eq!(groups[0].len(), 3);
    assert!(groups[0].contains(&0));
    assert!(groups[0].contains(&1));
    assert!(groups[0].contains(&2));
}

#[test]
fn test_mixed_component_resource_conflict() {
    let comp_a = TypeId::of::<u8>();
    let comp_b = TypeId::of::<u16>();
    let res_r = TypeId::of::<u32>();

    let systems = make_systems_with_resources(vec![
        (
            vec![ComponentAccess::Write(comp_a)],
            vec![ResourceAccess::Read(res_r)],
        ),
        (
            vec![ComponentAccess::Write(comp_b)],
            vec![ResourceAccess::Write(res_r)],
        ),
    ]);

    let scheduler = SystemScheduler::build(&systems).unwrap();
    let groups = scheduler.groups();

    assert_eq!(
        groups.len(),
        2,
        "no component conflict (A != B), but resource read-write conflict forces separate groups"
    );
    assert!(groups[0].contains(&0));
    assert!(groups[1].contains(&1));
}

#[test]
fn test_component_and_resource_conflict_adds_single_edge() {
    let comp_a = TypeId::of::<u8>();
    let res_r = TypeId::of::<u32>();

    let systems = make_systems_with_resources(vec![
        (
            vec![ComponentAccess::Write(comp_a)],
            vec![ResourceAccess::Write(res_r)],
        ),
        (
            vec![ComponentAccess::Write(comp_a)],
            vec![ResourceAccess::Write(res_r)],
        ),
    ]);

    let scheduler = SystemScheduler::build(&systems).unwrap();
    let groups = scheduler.groups();

    assert_eq!(
        groups.len(),
        2,
        "conflicts on both component and resource should produce exactly one edge, not two"
    );
    assert!(groups[0].contains(&0));
    assert!(groups[1].contains(&1));
}

#[test]
fn test_resource_conflict_ready_systems() {
    let res_r = TypeId::of::<u32>();

    let systems = make_systems_with_resources(vec![
        (Vec::new(), vec![ResourceAccess::Write(res_r)]),
        (Vec::new(), vec![ResourceAccess::Write(res_r)]),
        (Vec::new(), vec![ResourceAccess::Write(res_r)]),
    ]);

    let scheduler = SystemScheduler::build(&systems).unwrap();
    let groups = scheduler.groups();

    // All three conflict with each other on the same resource,
    // so they must be in separate sequential groups: [0], [1], [2].
    // Group ordering maps directly to ready-system behavior:
    //   After group 0 (sys0) completes, group 1 (sys1) becomes ready.
    //   After group 1 (sys1) completes, group 2 (sys2) becomes ready.
    assert_eq!(groups.len(), 3);
    assert_eq!(groups[0], vec![0], "sys0 runs first");
    assert_eq!(groups[1], vec![1], "sys1 ready after sys0 completes");
    assert_eq!(groups[2], vec![2], "sys2 ready after sys1 completes");
}
