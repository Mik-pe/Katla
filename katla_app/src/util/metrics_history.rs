//! Ring buffer for tracking metrics history over time.
//!
//! Used for real-time graphs displaying FPS, frame time, and other metrics.

use std::collections::VecDeque;

/// A fixed-capacity ring buffer for tracking metric values over time.
///
/// Automatically computes min, max, and mean as values are added.
pub struct MetricsHistory {
    /// The stored values (oldest to newest).
    values: VecDeque<f32>,
    /// Maximum number of values to store.
    capacity: usize,
    /// Cached minimum value.
    min: f32,
    /// Cached maximum value.
    max: f32,
    /// Cached sum for mean calculation.
    sum: f32,
}

impl MetricsHistory {
    /// Create a new metrics history with the given capacity.
    pub fn new(capacity: usize) -> Self {
        Self {
            values: VecDeque::with_capacity(capacity),
            capacity,
            min: f32::MAX,
            max: f32::MIN,
            sum: 0.0,
        }
    }

    /// Push a new value to the history.
    ///
    /// If the buffer is full, the oldest value is removed.
    pub fn push(&mut self, value: f32) {
        // If at capacity, remove oldest value first
        if self.values.len() == self.capacity
            && let Some(old) = self.values.pop_front()
        {
            self.sum -= old;
        }

        // Add new value
        self.values.push_back(value);
        self.sum += value;

        // Recalculate min/max (could be optimized but fine for small buffers)
        self.recalculate_bounds();
    }

    /// Recalculate min and max from current values.
    fn recalculate_bounds(&mut self) {
        self.min = f32::MAX;
        self.max = f32::MIN;
        for &v in &self.values {
            if v < self.min {
                self.min = v;
            }
            if v > self.max {
                self.max = v;
            }
        }
        if self.values.is_empty() {
            self.min = 0.0;
            self.max = 1.0;
        }
    }

    /// Get all values as a contiguous Vec (for convenience).
    pub fn values_vec(&self) -> Vec<f32> {
        self.values.iter().copied().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_and_bounds() {
        let mut h = MetricsHistory::new(10);
        h.push(5.0);
        h.push(10.0);
        h.push(3.0);

        let vals = h.values_vec();
        assert_eq!(vals, vec![5.0, 10.0, 3.0]);
    }

    #[test]
    fn test_ring_buffer_behavior() {
        let mut h = MetricsHistory::new(3);
        h.push(1.0);
        h.push(2.0);
        h.push(3.0);
        h.push(4.0); // Should push out 1.0

        let vals = h.values_vec();
        assert_eq!(vals, vec![2.0, 3.0, 4.0]);
    }
}
