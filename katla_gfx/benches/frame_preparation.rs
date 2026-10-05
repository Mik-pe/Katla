//! Frame-preparation benchmarks (issue #101).
//!
//! Measures the CPU cost of preparing one frame's draw submissions for the
//! passes that consume them: per-submission storage, the frame-level object
//! upload walk, and per-pass draw consumption. The legacy pipeline deep-cloned
//! the draw list per submission, cloned every pending draw into a rebuilt
//! upload list, and merged per-pass clones; the prepared pipeline shares one
//! reference-counted submission per list and consumes passes through borrowed
//! `PreparedDraws`. Pure CPU — no GPU device required.
//!
//! Run with: `cargo bench -p katla_gfx --bench frame_preparation`

use std::alloc::{GlobalAlloc, Layout, System};
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use criterion::{Criterion, black_box, criterion_group};
use katla_gfx::renderer::{DrawCall, DrawList, InstanceData, PreparedDraws};
use katla_gfx::{MaterialHandle, MeshHandle};

const PASSES_PER_FRAME: usize = 3; // depth prepass, geometry, picking
const REPORT_ITERATIONS: usize = 100;

static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

struct CountingAllocator;

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL_ALLOCATOR: CountingAllocator = CountingAllocator;

fn draw_list_of(draw_count: usize) -> DrawList {
    let mesh = MeshHandle::from_raw(0, 0);
    let material = MaterialHandle::from_raw(0, 0);
    let mut list = DrawList::new();
    for _ in 0..draw_count {
        list.push(DrawCall::instanced(
            mesh,
            material,
            vec![InstanceData::default()],
        ));
    }
    list
}

/// 64 small draw lists of 8 draws each, as a pass-heavy custom graph submits.
fn many_small_lists() -> Vec<DrawList> {
    (0..64).map(|_| draw_list_of(8)).collect()
}

/// One large scene draw list of 2000 draws, as the editor submits per frame.
fn large_scene() -> Vec<DrawList> {
    vec![draw_list_of(2000)]
}

fn slot_total(draws: &DrawList) -> usize {
    draws
        .draws
        .iter()
        .map(|draw| draw.instance_count().max(1) as usize)
        .sum()
}

/// Legacy pipeline: clone per submission, rebuilt upload list, per-pass merges.
fn legacy_frame(lists: &[DrawList]) -> usize {
    let mut pending: Vec<Vec<Rc<DrawList>>> = vec![Vec::new(); PASSES_PER_FRAME];
    for pass_submissions in &mut pending {
        for list in lists {
            pass_submissions.push(Rc::new(list.clone()));
        }
    }

    let mut upload_draws = Vec::new();
    for pass_submissions in &pending {
        for list in pass_submissions {
            upload_draws.extend(list.draws.iter().cloned());
        }
    }
    let upload_list = DrawList::from_draws(upload_draws);
    let uploaded_slots = slot_total(black_box(&upload_list));

    let mut encoded_slots = 0;
    for pass_submissions in &pending {
        let mut merged = Vec::new();
        for list in pass_submissions {
            merged.extend(list.draws.iter().cloned());
        }
        encoded_slots += slot_total(&DrawList::from_draws(merged));
    }

    uploaded_slots + encoded_slots
}

/// Prepared pipeline: shared submissions, borrowed upload walk, borrowed
/// per-pass consumption.
fn prepared_frame(shared: &[Rc<DrawList>]) -> usize {
    let mut pending: Vec<Vec<Rc<DrawList>>> = vec![Vec::new(); PASSES_PER_FRAME];
    for pass_submissions in &mut pending {
        for list in shared {
            pass_submissions.push(Rc::clone(list));
        }
    }

    // The frame-level upload walk uploads each unique submission once and
    // never rebuilds a merged list.
    let mut uploaded: Vec<*const DrawList> = Vec::new();
    let mut uploaded_slots = 0;
    for pass_submissions in &pending {
        for list in pass_submissions {
            let identity = Rc::as_ptr(list);
            if uploaded.contains(&identity) {
                continue;
            }
            uploaded.push(identity);
            uploaded_slots += slot_total(black_box(list));
        }
    }

    let mut encoded_slots = 0;
    for pass_submissions in &pending {
        let prepared = PreparedDraws::from_lists(pass_submissions);
        encoded_slots += black_box(prepared).counts().instances;
    }

    uploaded_slots + encoded_slots
}

fn bench_frame_preparation(criterion: &mut Criterion) {
    let small = many_small_lists();
    let small_shared: Vec<Rc<DrawList>> = many_small_lists().into_iter().map(Rc::new).collect();
    let large = large_scene();
    let large_shared: Vec<Rc<DrawList>> = large_scene().into_iter().map(Rc::new).collect();

    let mut group = criterion.benchmark_group("frame_preparation");
    group.throughput(criterion::Throughput::Elements(
        (small.len() * 8 + large[0].draws.len()) as u64,
    ));

    group.bench_function("many_small_lists/legacy_clones", |bencher| {
        bencher.iter(|| legacy_frame(black_box(&small)))
    });
    group.bench_function("many_small_lists/prepared", |bencher| {
        bencher.iter(|| prepared_frame(black_box(&small_shared)))
    });
    group.bench_function("large_scene/legacy_clones", |bencher| {
        bencher.iter(|| legacy_frame(black_box(&large)))
    });
    group.bench_function("large_scene/prepared", |bencher| {
        bencher.iter(|| prepared_frame(black_box(&large_shared)))
    });

    group.finish();
}

/// Allocation-count evidence: average heap allocations per frame for each
/// pipeline, printed once outside Criterion's measurement loop.
fn allocation_report() {
    for (name, lists) in [
        ("many small lists (64 x 8 draws)", many_small_lists()),
        ("large scene (1 x 2000 draws)", large_scene()),
    ] {
        let shared: Vec<Rc<DrawList>> = lists.clone().into_iter().map(Rc::new).collect();
        legacy_frame(&lists);
        prepared_frame(&shared);

        let start = ALLOCATIONS.load(Ordering::Relaxed);
        let timer = Instant::now();
        for _ in 0..REPORT_ITERATIONS {
            legacy_frame(&lists);
        }
        let legacy_allocs = (ALLOCATIONS.load(Ordering::Relaxed) - start) / REPORT_ITERATIONS;
        let legacy_time = timer.elapsed() / REPORT_ITERATIONS as u32;

        let start = ALLOCATIONS.load(Ordering::Relaxed);
        let timer = Instant::now();
        for _ in 0..REPORT_ITERATIONS {
            prepared_frame(&shared);
        }
        let prepared_allocs = (ALLOCATIONS.load(Ordering::Relaxed) - start) / REPORT_ITERATIONS;
        let prepared_time = timer.elapsed() / REPORT_ITERATIONS as u32;

        println!(
            "{name}: legacy {legacy_allocs} allocs/frame ({legacy_time:?}), \
             prepared {prepared_allocs} allocs/frame ({prepared_time:?})"
        );
    }
}

criterion_group! {
    name = benches;
    config = Criterion::default();
    targets = bench_frame_preparation
}

fn main() {
    allocation_report();
    benches();
}
