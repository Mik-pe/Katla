use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};

/// Synthetic draw command mimicking the data in `ResolvedDrawCommand`.
///
/// Contains the same number of fields as the real struct to accurately
/// represent memory layout and iteration costs.
#[derive(Clone)]
struct MockDrawCommand {
    pipeline: u64,
    layout: u64,
    storage_ds: u64,
    bindless_ds: u64,
    skeleton_ds: u64,
    is_skinned: bool,
    pos_buf: u64,
    norm_buf: u64,
    tang_buf: u64,
    uv_buf: u64,
    index_buf: u64,
    index_count: u32,
    instance_index: u32,
}

impl MockDrawCommand {
    fn new(index: usize) -> Self {
        Self {
            pipeline: (index % 8) as u64,
            layout: (index % 4) as u64,
            storage_ds: 1,
            bindless_ds: 2,
            skeleton_ds: if index % 10 == 0 { 3 } else { 0 },
            is_skinned: index % 10 == 0,
            pos_buf: 100 + index as u64,
            norm_buf: 200 + index as u64,
            tang_buf: 300 + index as u64,
            uv_buf: 400 + index as u64,
            index_buf: 500 + index as u64,
            index_count: 36,
            instance_index: index as u32,
        }
    }
}

/// Simulate the work done in `record_draw_chunk` — sequential Vulkan command recording.
///
/// Each iteration performs the same branching and field accesses as the real
/// `record_draw_chunk` in `parallel_geometry.rs`.
fn record_sequential(commands: &[MockDrawCommand]) -> u64 {
    let mut sum = 0u64;
    for draw in commands {
        sum += draw.pipeline;
        sum += draw.layout;
        sum += draw.storage_ds;
        sum += draw.bindless_ds;
        if draw.is_skinned {
            sum += draw.skeleton_ds;
        }
        sum += draw.pos_buf;
        sum += draw.norm_buf;
        sum += draw.tang_buf;
        sum += draw.uv_buf;
        if draw.index_count > 0 {
            sum += draw.index_buf;
            sum += draw.index_count as u64;
            sum += draw.instance_index as u64;
        }
    }
    sum
}

/// Simulate parallel command buffer recording using the same chunking strategy
/// as `execute_parallel_recording` in `parallel_geometry.rs`.
///
/// Uses `std::thread::scope` with clamped thread count, same as production code.
fn record_parallel(commands: &[MockDrawCommand]) -> u64 {
    if commands.is_empty() {
        return 0;
    }

    let cpu_count = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    let num_threads = cpu_count.clamp(1, 4);
    let chunk_size = commands.len().div_ceil(num_threads);

    let mut results: Vec<u64> = vec![0u64; num_threads];
    let chunks: Vec<&[MockDrawCommand]> = commands.chunks(chunk_size).collect();

    std::thread::scope(|s| {
        for (i, chunk) in chunks.iter().enumerate() {
            let results_ptr = &mut results[..];
            s.spawn(move || {
                results_ptr[i] = record_sequential(chunk);
            });
        }
    });

    results.iter().sum()
}

fn bench_sequential_recording(c: &mut Criterion) {
    let mut group = c.benchmark_group("cb_recording_sequential");
    for size in [100, 500, 1000] {
        let commands: Vec<MockDrawCommand> = (0..size).map(|i| MockDrawCommand::new(i)).collect();

        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, _| {
            b.iter(|| black_box(record_sequential(&commands)));
        });
    }
    group.finish();
}

fn bench_parallel_recording(c: &mut Criterion) {
    let mut group = c.benchmark_group("cb_recording_parallel");
    for size in [100, 500, 1000] {
        let commands: Vec<MockDrawCommand> = (0..size).map(|i| MockDrawCommand::new(i)).collect();

        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, _| {
            b.iter(|| black_box(record_parallel(&commands)));
        });
    }
    group.finish();
}

fn bench_sequential_vs_parallel(c: &mut Criterion) {
    let mut group = c.benchmark_group("cb_recording_comparison");
    for size in [100, 500, 1000] {
        let commands: Vec<MockDrawCommand> = (0..size).map(|i| MockDrawCommand::new(i)).collect();

        group.bench_with_input(BenchmarkId::new("sequential", size), &size, |b, _| {
            b.iter(|| black_box(record_sequential(&commands)));
        });

        group.bench_with_input(BenchmarkId::new("parallel", size), &size, |b, _| {
            b.iter(|| black_box(record_parallel(&commands)));
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_sequential_recording,
    bench_parallel_recording,
    bench_sequential_vs_parallel,
);
criterion_main!(benches);
