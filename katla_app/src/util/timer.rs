use std::{collections::VecDeque, time::Instant};

pub struct Timer {
    timestamps: VecDeque<f64>,
    current_mean: f64,
    current_min: f64,
    current_max: f64,
    delta: f64,
    last_frame: Instant,
    max_num_timestamps: usize,
}

impl Timer {
    pub fn new(max_num_timestamps: usize) -> Self {
        Self {
            timestamps: VecDeque::new(),
            current_mean: 0.0,
            current_min: 0.0,
            current_max: 0.0,
            delta: 0.0,
            last_frame: Instant::now(),
            max_num_timestamps,
        }
    }

    pub fn get_delta(&self) -> f64 {
        self.delta
    }

    pub fn add_timestamp(&mut self) {
        let in_timestamp = self.last_frame.elapsed().as_micros() as f64 / 1000.0;
        self.delta = self.last_frame.elapsed().as_micros() as f64 / 1_000_000.0;

        self.timestamps.push_back(in_timestamp);
        let mut sum_timestamps = 0.0;
        self.current_max = std::f64::MIN;
        self.current_min = std::f64::MAX;
        if self.timestamps.len() > self.max_num_timestamps {
            self.timestamps.pop_front();
        }
        for timestamp in &self.timestamps {
            sum_timestamps += timestamp;
            self.current_max = f64::max(self.current_max, *timestamp);
            self.current_min = f64::min(self.current_min, *timestamp);
        }
        self.current_mean = sum_timestamps / self.timestamps.len() as f64;
        self.last_frame = Instant::now();
    }
}
