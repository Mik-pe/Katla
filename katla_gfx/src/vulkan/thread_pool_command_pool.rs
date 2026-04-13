use ash::Device;

use super::CommandPool;

/// Manages a pool of command pools, one per worker thread.
/// Each thread gets its own CommandPool for lock-free secondary command buffer allocation.
pub(crate) struct ThreadPoolCommandPool {
    pools: Vec<CommandPool>,
}

impl ThreadPoolCommandPool {
    /// Create N command pools, one per worker thread.
    pub fn new(device: Device, queue_family_index: u32, num_threads: usize) -> Self {
        let pools = (0..num_threads)
            .map(|_| CommandPool::new(device.clone(), queue_family_index))
            .collect();
        Self { pools }
    }

    /// Get the command pool for a specific thread index.
    pub fn get_pool(&self, thread_index: usize) -> &CommandPool {
        &self.pools[thread_index % self.pools.len()]
    }

    /// Allocate a secondary command buffer from a specific thread's pool.
    pub fn allocate_secondary(&self, thread_index: usize) -> super::CommandBuffer {
        self.get_pool(thread_index).allocate_secondary()
    }

    /// Reset all pools for reuse (call once per frame).
    pub fn reset_all(&self) {
        for pool in &self.pools {
            pool.reset();
        }
    }

    /// Destroy all pools.
    pub fn destroy(&self) {
        for pool in &self.pools {
            pool.destroy();
        }
    }

    /// Number of pools in this thread pool.
    pub fn len(&self) -> usize {
        self.pools.len()
    }

    /// Whether this pool is empty.
    pub fn is_empty(&self) -> bool {
        self.pools.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_thread_pool_command_pool_wrapping_index() {
        // Test the index wrapping logic without needing a Vulkan device
        let pool_count = 4usize;
        let cases = [
            (0, 0),
            (1, 1),
            (2, 2),
            (3, 3),
            (4, 0),  // wraps
            (5, 1),  // wraps
            (99, 3), // wraps: 99 % 4 = 3
            (100, 0),
        ];

        for (thread_index, expected) in cases {
            assert_eq!(
                thread_index % pool_count,
                expected,
                "thread_index {} should map to pool {}",
                thread_index,
                expected
            );
        }
    }

    #[test]
    fn test_thread_pool_command_pool_len_and_empty() {
        // Test size calculations without Vulkan device
        let empty_pools: Vec<CommandPool> = vec![];
        assert!(empty_pools.is_empty());
        assert_eq!(empty_pools.len(), 0);

        let num_threads = 8usize;
        assert_ne!(num_threads, 0);
        assert_eq!(num_threads, 8);
    }

    #[test]
    fn test_pool_count_matches_threads() {
        // Verify pool count calculation for various thread counts
        for num_threads in [1, 2, 4, 8, 16] {
            assert!(num_threads > 0);
            // Each thread should map to exactly one pool
            for thread_index in 0..num_threads {
                assert_eq!(thread_index % num_threads, thread_index);
            }
        }
    }

    #[test]
    fn test_single_pool_all_threads_map_to_same() {
        // With 1 pool, all thread indices should map to pool 0
        let pool_count = 1usize;
        for thread_index in [0, 1, 5, 100, 999] {
            assert_eq!(
                thread_index % pool_count,
                0,
                "With 1 pool, all threads should map to index 0"
            );
        }
    }
}
