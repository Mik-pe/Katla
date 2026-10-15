//! Shared CPU benchmark sampling; each operation processes a complete batch.

use std::{
    hint::black_box,
    time::{Duration, Instant},
};

pub fn measure(name: &str, size: usize, mut operation: impl FnMut() -> f64) {
    let budget = Duration::from_millis(
        std::env::var("KATLA_BENCH_SAMPLE_MS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(100),
    );
    for _ in 0..3 {
        black_box(operation());
    }
    for sample in 0..7 {
        let start = Instant::now();
        let mut iterations = 0;
        let mut checksum = 0.0;
        while start.elapsed() < budget {
            checksum += black_box(operation());
            iterations += 1;
        }
        let ns = start.elapsed().as_secs_f64() * 1e9 / iterations as f64;
        println!("{name},{size},1,{size},{sample},{iterations},{ns:.3},{checksum:.6}");
    }
}
