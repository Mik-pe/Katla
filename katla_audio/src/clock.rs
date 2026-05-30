use std::sync::atomic::{AtomicU64, Ordering};

pub struct AudioClock {
    sample_rate: u32,
    total_samples_rendered: AtomicU64,
}

impl AudioClock {
    pub fn new(sample_rate: u32) -> Self {
        AudioClock {
            sample_rate,
            total_samples_rendered: AtomicU64::new(0),
        }
    }

    pub fn advance(&self, samples: u64) {
        self.total_samples_rendered
            .fetch_add(samples, Ordering::Relaxed);
    }

    pub fn sample_position(&self) -> u64 {
        self.total_samples_rendered.load(Ordering::Relaxed)
    }

    pub fn time_secs(&self) -> f64 {
        self.sample_position() as f64 / self.sample_rate as f64
    }

    pub fn time_of_next_sample_count(&self, n: u64) -> f64 {
        (self.sample_position() + n) as f64 / self.sample_rate as f64
    }
}
