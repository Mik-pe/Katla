//! CPU-only descriptor construction and reconciliation through the public API.
//! No layout, text shaping, drawing, window, or GPU device is involved.

#[path = "../../scripts/cpu_bench_support.rs"]
mod support;

use katla_ui::declarative::{KeyedChild, ViewTree, WidgetBox, text, vstack_keyed};
use std::hint::black_box;

#[derive(Clone, Copy)]
enum Scenario {
    Stable,
    Reverse,
    Rotate,
    InsertRemove,
    Mixed,
    Duplicates,
    Unkeyed,
}

fn rebuild(tree: &mut ViewTree, size: usize, frame: usize, scenario: Scenario) -> f64 {
    let rows = size + usize::from(matches!(scenario, Scenario::InsertRemove) && frame % 2 == 1);
    let children = (0..rows)
        .map(|index| {
            let key = match scenario {
                Scenario::Reverse if frame % 2 == 1 => Some((size - index - 1) as u64),
                Scenario::Rotate => Some(((index + frame) % size) as u64),
                Scenario::Mixed if index % 2 == 0 => None,
                Scenario::Duplicates => Some((index % 16) as u64),
                Scenario::Unkeyed => None,
                _ => Some(index as u64),
            };
            KeyedChild {
                key,
                widget: text("row").boxed(),
            }
        })
        .collect();
    tree.set_root(black_box(vstack_keyed(children).boxed()));
    let root = tree.get(tree.root().expect("rebuild mounts a root"));
    let children = &root.expect("root remains mounted").children;
    assert_eq!(children.len(), rows);
    black_box(children);
    rows as f64
}

fn main() {
    println!("scenario,entities,threads,matches,sample,iterations,ns_per_iteration,checksum");
    for size in [8, 128, 1024, 4096] {
        for (name, scenario) in [
            ("ui_keyed_stable", Scenario::Stable),
            ("ui_keyed_reverse", Scenario::Reverse),
            ("ui_keyed_rotate", Scenario::Rotate),
            ("ui_keyed_insert_remove", Scenario::InsertRemove),
            ("ui_keyed_mixed", Scenario::Mixed),
            ("ui_keyed_duplicates", Scenario::Duplicates),
            ("ui_unkeyed_stable", Scenario::Unkeyed),
        ] {
            let mut tree = ViewTree::new();
            rebuild(&mut tree, size, 0, scenario);
            let mut frame = 0_usize;
            support::measure(name, size, || {
                frame = frame.wrapping_add(1);
                rebuild(&mut tree, size, frame, scenario)
            });
        }
    }
}
