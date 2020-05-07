// use std::collections::VecDeque;

// pub struct Timer {
//     all_timestamps: VecDeque<f64>,
//     current_mean: f64,
//     current_min: f64,
//     current_max: f64,
//     max_num_timestamps: usize,
// }

// impl Timer {
//     pub fn new(max_num_timestamps: usize) -> Self {
//         Self {
//             all_timestamps: VecDeque::new(),
//             current_mean: 0.0,
//             current_min: 0.0,
//             current_max: 0.0,
//             max_num_timestamps: max_num_timestamps,
//         }
//     }

//     pub fn add_timestamp(&mut self, in_timestamp: f64) {
//         self.all_timestamps.push_back(in_timestamp);
//         let mut sum_timestamps = 0.0;
//         self.current_max = std::f64::MIN;
//         self.current_min = std::f64::MAX;
//         for timestamp in &self.all_timestamps {
//             sum_timestamps += timestamp;
//             self.current_max = f64::max(self.current_max, *timestamp);
//             self.current_min = f64::min(self.current_min, *timestamp);
//         }
//         if self.all_timestamps.len() > self.max_num_timestamps {
//             self.all_timestamps.pop_front();
//         }
//         self.current_mean = sum_timestamps / self.all_timestamps.len() as f64;
//     }

//     pub fn current_mean(&self) -> f64 {
//         self.current_mean
//     }

//     pub fn current_max(&self) -> f64 {
//         self.current_max
//     }

//     pub fn current_min(&self) -> f64 {
//         self.current_min
//     }
// }
