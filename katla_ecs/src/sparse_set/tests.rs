use super::*;

#[test]
fn test_sparse_set_remove_repairs_moved_key_across_pages() {
    let mut set = SparseSet::new();
    let first = crate::EntityId::new(1, 4);
    let middle = crate::EntityId::new(PAGE_SIZE as u32 + 1, 5);
    let last = crate::EntityId::new(2 * PAGE_SIZE as u32 + 3, 6);
    set.insert(first, 10);
    set.insert(middle, 20);
    set.insert(last, 30);

    assert!(set.remove(first));
    assert_eq!(set.dense(), &[(last, 30), (middle, 20)]);
    assert_eq!(set.get(last), Some(&30));
    assert_eq!(set.get(middle), Some(&20));
    assert!(set.get(first).is_none());

    assert!(set.remove(last));
    assert_eq!(set.get(middle), Some(&20));
    assert!(set.remove(middle));
    assert!(set.is_empty());
    set.insert(first, 40);
    assert_eq!(set.get(first), Some(&40));
}

#[test]
fn test_generational_keys_reject_stale_access() {
    let mut set = SparseSet::new();
    let old = crate::EntityId::new(7, 0);
    let live = crate::EntityId::new(7, 1);
    set.insert(old, 10);
    set.insert(live, 20);
    assert_eq!(set.len(), 1);
    assert_eq!(set.get(old), None);
    assert!(set.get_mut(old).is_none());
    assert!(!set.contains(old));
    assert!(!set.remove(old));
    assert_eq!(set.get(live), Some(&20));
    assert_eq!(set.keys().collect::<Vec<_>>(), vec![live]);
    assert!(set.remove(live));
}

#[test]
fn test_generational_operations_match_reference_model() {
    use crate::EntityId;
    use std::collections::HashMap;

    let mut set = SparseSet::new();
    let mut model = HashMap::new();
    let mut generations = [0u32; 64];
    let mut seed = 0x527a_64e1u64;
    for step in 0..10_000 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let slot = ((seed >> 32) as usize) % generations.len();
        let key = EntityId::new(slot as u32, generations[slot]);
        match seed % 4 {
            0 => {
                set.insert(key, step);
                model.insert(slot, (key, step));
            }
            1 => {
                assert_eq!(set.remove(key), model.remove(&slot).is_some());
                generations[slot] += 1;
            }
            2 => {
                if let Some(value) = set.get_mut(key) {
                    *value += 1;
                    model.get_mut(&slot).unwrap().1 += 1;
                }
            }
            _ => assert_eq!(set.get(key).copied(), model.get(&slot).map(|x| x.1)),
        }
        assert_eq!(set.len(), model.len());
        for (&index, &(live, value)) in &model {
            assert_eq!(set.get(live), Some(&value));
            let stale = EntityId::new(index as u32, live.generation().wrapping_sub(1));
            assert!(set.get(stale).is_none());
            assert!(set.get_mut(stale).is_none());
            assert!(!set.remove(stale));
        }
    }
}

#[test]
fn test_sparse_set_insert() {
    let mut set: SparseSet<usize, i32> = SparseSet::new();
    set.insert(0, 10);
    set.insert(1, 20);
    set.insert(2, 30);

    assert_eq!(set.len(), 3);
    assert_eq!(set.get(0), Some(&10));
    assert_eq!(set.get(1), Some(&20));
    assert_eq!(set.get(2), Some(&30));
}

#[test]
fn test_sparse_set_insert_update() {
    let mut set: SparseSet<usize, i32> = SparseSet::new();
    set.insert(0, 10);
    assert_eq!(set.get(0), Some(&10));

    set.insert(0, 20);
    assert_eq!(set.len(), 1);
    assert_eq!(set.get(0), Some(&20));
}

#[test]
fn test_sparse_set_remove() {
    let mut set: SparseSet<usize, i32> = SparseSet::new();
    set.insert(0, 10);
    set.insert(1, 20);
    set.insert(2, 30);

    assert!(set.remove(1));
    assert_eq!(set.len(), 2);
    assert_eq!(set.get(0), Some(&10));
    assert_eq!(set.get(1), None);
    assert_eq!(set.get(2), Some(&30));

    assert!(!set.remove(1)); // Already removed
    assert_eq!(set.len(), 2);
}

#[test]
fn test_sparse_set_get_mut() {
    let mut set: SparseSet<usize, i32> = SparseSet::new();
    set.insert(0, 10);

    if let Some(value) = set.get_mut(0) {
        *value = 20;
    }

    assert_eq!(set.get(0), Some(&20));
}

#[test]
fn test_sparse_set_iter() {
    let mut set: SparseSet<usize, i32> = SparseSet::new();
    set.insert(0, 10);
    set.insert(1, 20);
    set.insert(2, 30);

    let items: Vec<(usize, &i32)> = set.iter().collect();
    assert_eq!(items.len(), 3);
    assert!(items.contains(&(0, &10)));
    assert!(items.contains(&(1, &20)));
    assert!(items.contains(&(2, &30)));
}

#[test]
fn test_sparse_set_iter_mut() {
    let mut set: SparseSet<usize, i32> = SparseSet::new();
    set.insert(0, 10);
    set.insert(1, 20);

    for (_, value) in set.iter_mut() {
        *value *= 2;
    }

    assert_eq!(set.get(0), Some(&20));
    assert_eq!(set.get(1), Some(&40));
}

#[test]
fn test_sparse_set_retain_keys() {
    let mut set: SparseSet<usize, i32> = SparseSet::new();
    set.insert(0, 10);
    set.insert(1, 20);
    set.insert(2, 30);
    set.insert(3, 40);

    let mut valid = HashSet::new();
    valid.insert(1);
    valid.insert(3);

    set.retain_keys(&valid);

    assert_eq!(set.len(), 2);
    assert_eq!(set.get(0), None);
    assert_eq!(set.get(1), Some(&20));
    assert_eq!(set.get(2), None);
    assert_eq!(set.get(3), Some(&40));
}

#[test]
fn test_sparse_set_large_key_ids() {
    let mut set: SparseSet<usize, i32> = SparseSet::new();
    set.insert(1000, 100);
    set.insert(5000, 500);
    set.insert(10000, 1000);

    assert_eq!(set.len(), 3);
    assert_eq!(set.get(1000), Some(&100));
    assert_eq!(set.get(5000), Some(&500));
    assert_eq!(set.get(10000), Some(&1000));
}

#[test]
fn test_sparse_set_remove_middle() {
    let mut set: SparseSet<usize, i32> = SparseSet::new();
    set.insert(0, 10);
    set.insert(1, 20);
    set.insert(2, 30);
    set.insert(3, 40);
    set.insert(4, 50);

    set.remove(2);

    assert_eq!(set.len(), 4);
    assert_eq!(set.get(2), None);
    assert_eq!(set.get(0), Some(&10));
    assert_eq!(set.get(1), Some(&20));
    assert_eq!(set.get(3), Some(&40));
    assert_eq!(set.get(4), Some(&50));
}

#[test]
fn test_sparse_set_iteration_order_after_removal() {
    let mut set: SparseSet<usize, i32> = SparseSet::new();
    set.insert(0, 10);
    set.insert(1, 20);
    set.insert(2, 30);

    set.remove(1);

    let items: Vec<(usize, i32)> = set.iter().map(|(k, v)| (k, *v)).collect();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0], (0, 10));
    assert_eq!(items[1], (2, 30));
}

#[test]
fn test_sparse_set_dense_sparse_consistency_after_remove() {
    let mut set: SparseSet<u32, i32> = SparseSet::new();

    for i in 0..5u32 {
        set.insert(i, (i * 10) as i32);
    }

    set.remove(1);
    set.remove(3);

    assert_eq!(set.len(), 3);

    for (key, value) in set.iter() {
        assert!(set.contains(key));
        assert_eq!(*set.get(key).unwrap(), *value);
    }

    assert!(!set.contains(1));
    assert!(!set.contains(3));
    assert_eq!(set.get(1), None);
    assert_eq!(set.get(3), None);

    assert_eq!(set.get(0), Some(&0));
    assert_eq!(set.get(2), Some(&20));
    assert_eq!(set.get(4), Some(&40));
}

#[test]
fn test_sparse_set_remove_all_then_reinsert() {
    let mut set: SparseSet<u32, i32> = SparseSet::new();

    for i in 0..5u32 {
        set.insert(i, i as i32);
    }

    for i in 0..5u32 {
        assert!(set.remove(i));
    }

    assert!(set.is_empty());

    for i in 0..5u32 {
        set.insert(i, (i * 100) as i32);
    }

    assert_eq!(set.len(), 5);
    for i in 0..5u32 {
        assert_eq!(set.get(i), Some(&((i * 100) as i32)));
    }
}

#[test]
fn test_sparse_set_large_index_only_allocates_needed_pages() {
    let mut set: SparseSet<usize, i32> = SparseSet::new();

    set.insert(50000, 42);
    assert_eq!(set.get(50000), Some(&42));
    assert_eq!(set.len(), 1);

    // Only 1 page should be allocated for index 50000 (page 48)
    assert_eq!(set.pages.len(), 49); // pages 0..=48
    let allocated_pages: Vec<_> = set.pages.iter().filter(|p| p.is_some()).collect();
    assert_eq!(allocated_pages.len(), 1);

    // Small index should also work — allocates a second page
    set.insert(0, 1);
    assert_eq!(set.get(0), Some(&1));
    assert_eq!(set.len(), 2);

    let allocated_pages: Vec<_> = set.pages.iter().filter(|p| p.is_some()).collect();
    assert_eq!(allocated_pages.len(), 2);
}

#[test]
fn test_sparse_set_contains_after_remove() {
    let mut set: SparseSet<usize, i32> = SparseSet::new();
    set.insert(42, 100);
    assert!(set.contains(42));

    set.remove(42);
    assert!(!set.contains(42));
}

#[test]
fn test_sparse_set_clear_resets_state() {
    let mut set: SparseSet<usize, i32> = SparseSet::new();
    set.insert(0, 10);
    set.insert(100, 20);

    set.clear();

    assert!(set.is_empty());
    assert_eq!(set.len(), 0);
    assert_eq!(set.get(0), None);
    assert_eq!(set.get(100), None);
    assert!(set.pages.is_empty());
}

#[test]
fn test_sparse_set_values_and_keys() {
    let mut set: SparseSet<usize, i32> = SparseSet::new();
    set.insert(3, 30);
    set.insert(1, 10);
    set.insert(2, 20);

    let mut values: Vec<&i32> = set.values().collect();
    values.sort();
    assert_eq!(values, vec![&10, &20, &30]);

    let keys: std::collections::HashSet<usize> = set.keys().collect();
    assert!(keys.contains(&1));
    assert!(keys.contains(&2));
    assert!(keys.contains(&3));
}

#[test]
fn test_sparse_set_memory_efficiency_with_sparse_indices() {
    let mut set: SparseSet<usize, i32> = SparseSet::new();

    // Insert 3 entries spread far apart
    set.insert(0, 1);
    set.insert(100_000, 2);
    set.insert(1_000_000, 3);

    assert_eq!(set.len(), 3);

    // Only 3 pages should be allocated (one per distant index)
    let allocated_pages: Vec<_> = set.pages.iter().filter(|p| p.is_some()).collect();
    assert_eq!(allocated_pages.len(), 3);

    // Flat vec would have required 1_000_001 entries (8MB+).
    // Paged approach uses 3 pages × 1024 × 8 bytes = ~24KB + page vec overhead.
    let total_entries: usize = set.pages.iter().filter(|p| p.is_some()).count() * PAGE_SIZE;
    assert!(
        total_entries < 10_000,
        "paged allocation should be far smaller than flat 1M entries"
    );
}
