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
        if self.values.len() == self.capacity {
            if let Some(old) = self.values.pop_front() {
                self.sum -= old;
            }
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

    /// Get the values as a slice (oldest to newest).
    pub fn values(&self) -> (&[f32], &[f32]) {
        let (a, b) = self.values.as_slices();
        (a, b)
    }

    /// Get all values as a contiguous Vec (for convenience).
    pub fn values_vec(&self) -> Vec<f32> {
        self.values.iter().copied().collect()
    }

    /// Get the number of values currently stored.
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Check if the history is empty.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Get the minimum value in the history.
    pub fn min(&self) -> f32 {
        if self.values.is_empty() {
            0.0
        } else {
            self.min
        }
    }

    /// Get the maximum value in the history.
    pub fn max(&self) -> f32 {
        if self.values.is_empty() {
            1.0
        } else {
            self.max
        }
    }

    /// Get the mean (average) of all values.
    pub fn mean(&self) -> f32 {
        if self.values.is_empty() {
            0.0
        } else {
            self.sum / self.values.len() as f32
        }
    }

    /// Get the most recent value.
    pub fn last(&self) -> Option<f32> {
        self.values.back().copied()
    }

    /// Clear all values.
    pub fn clear(&mut self) {
        self.values.clear();
        self.min = f32::MAX;
        self.max = f32::MIN;
        self.sum = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_history() {
        let h = MetricsHistory::new(10);
        assert!(h.is_empty());
        assert_eq!(h.len(), 0);
        assert_eq!(h.min(), 0.0);
        assert_eq!(h.max(), 1.0);
        assert_eq!(h.mean(), 0.0);
    }

    #[test]
    fn test_push_and_bounds() {
        let mut h = MetricsHistory::new(10);
        h.push(5.0);
        h.push(10.0);
        h.push(3.0);

        assert_eq!(h.len(), 3);
        assert_eq!(h.min(), 3.0);
        assert_eq!(h.max(), 10.0);
        assert!((h.mean() - 6.0).abs() < 0.001);
    }

    #[test]
    fn test_ring_buffer_behavior() {
        let mut h = MetricsHistory::new(3);
        h.push(1.0);
        h.push(2.0);
        h.push(3.0);
        h.push(4.0); // Should push out 1.0

        assert_eq!(h.len(), 3);
        let vals = h.values_vec();
        assert_eq!(vals, vec![2.0, 3.0, 4.0]);
        assert_eq!(h.min(), 2.0);
        assert_eq!(h.max(), 4.0);
    }

    #[test]
    fn test_clear() {
        let mut h = MetricsHistory::new(10);
        h.push(1.0);
        h.push(2.0);
        h.clear();
        assert!(h.is_empty());
    }
}
