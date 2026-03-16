//! GPU timing queries for particle system compute shader.
//!
//! This module implements Vulkan timestamp queries to measure compute shader
//! execution time. It uses query pools to record timestamps before and after
//! compute dispatch operations.

use std::rc::Rc;

use ash::vk;
use log::debug;

use crate::vulkan::context::VulkanContext;

/// GPU timing queries for compute shader execution.
///
/// Records timestamps before and after compute operations to measure
/// GPU execution time in milliseconds.
pub struct TimestampQuery {
    /// Query pool for start timestamp
    start_pool: vk::QueryPool,

    /// Query pool for end timestamp
    end_pool: vk::QueryPool,

    /// Vulkan context for device access
    context: Rc<VulkanContext>,

    /// Cached timing result (milliseconds)
    cached_time_ms: f32,

    /// Flag indicating if timing data is available
    timing_available: bool,

    /// Flag to prevent double destruction
    destroyed: bool,
}

impl TimestampQuery {
    /// Create a new timestamp query pool.
    ///
    /// # Arguments
    /// * `context` - Vulkan context for device access
    ///
    /// # Returns
    /// Initialized timestamp query ready for recording
    pub fn new(context: &Rc<VulkanContext>) -> Result<Self, String> {
        // Check if timestamp queries are supported
        let device_properties = unsafe {
            context
                .instance
                .get_physical_device_properties(context.physical_device)
        };

        // Check timestamp period (nanoseconds per timestamp value)
        let timestamp_period = device_properties.limits.timestamp_period;

        if timestamp_period == 0.0 {
            return Err("Timestamp queries not supported on this device".to_string());
        }

        debug!("Timestamp query period: {} ns", timestamp_period);

        // Create query pool for start timestamps (single query)
        let start_pool_info = vk::QueryPoolCreateInfo::default()
            .query_type(vk::QueryType::TIMESTAMP)
            .query_count(1) // Single query for start time
            .flags(vk::QueryPoolCreateFlags::empty());

        let start_pool = unsafe {
            context
                .device
                .create_query_pool(&start_pool_info, None)
                .map_err(|e| format!("Failed to create start query pool: {:?}", e))?
        };

        // Create query pool for end timestamps (single query)
        let end_pool_info = vk::QueryPoolCreateInfo::default()
            .query_type(vk::QueryType::TIMESTAMP)
            .query_count(1) // Single query for end time
            .flags(vk::QueryPoolCreateFlags::empty());

        let end_pool = unsafe {
            context
                .device
                .create_query_pool(&end_pool_info, None)
                .map_err(|e| format!("Failed to create end query pool: {:?}", e))?
        };

        debug!("Created timestamp query pools");

        Ok(Self {
            start_pool,
            end_pool,
            context: context.clone(),
            cached_time_ms: 0.0,
            timing_available: false,
            destroyed: false,
        })
    }

    /// Reset query pools for a new frame.
    ///
    /// Must be called before recording timestamps to reset the query state.
    ///
    /// # Arguments
    /// * `command_buffer` - Command buffer to record reset into
    pub fn reset(&self, command_buffer: vk::CommandBuffer) {
        if self.destroyed {
            return;
        }

        unsafe {
            // Reset both query pools
            self.context.device.cmd_reset_query_pool(
                command_buffer,
                self.start_pool,
                0, // First query
                1, // Query count
            );
            self.context.device.cmd_reset_query_pool(
                command_buffer,
                self.end_pool,
                0, // First query
                1, // Query count
            );
        }
    }

    /// Record start timestamp.
    ///
    /// Records a timestamp at the top of the compute shader pipeline.
    ///
    /// # Arguments
    /// * `command_buffer` - Command buffer to record timestamp into
    pub fn write_start(&self, command_buffer: vk::CommandBuffer) {
        if self.destroyed {
            return;
        }

        unsafe {
            self.context.device.cmd_write_timestamp(
                command_buffer,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                self.start_pool,
                0, // Query index
            );
        }
    }

    /// Record end timestamp.
    ///
    /// Records a timestamp at the bottom of the compute shader pipeline.
    ///
    /// # Arguments
    /// * `command_buffer` - Command buffer to record timestamp into
    pub fn write_end(&self, command_buffer: vk::CommandBuffer) {
        if self.destroyed {
            return;
        }

        unsafe {
            self.context.device.cmd_write_timestamp(
                command_buffer,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                self.end_pool,
                0, // Query index
            );
        }
    }

    /// Read back timestamp results and calculate compute time.
    ///
    /// # Returns
    /// Execution time in milliseconds, or error if readback failed
    ///
    /// # Notes
    /// Uses PARTIAL flag to avoid blocking. If timing data isn't ready yet,
    /// returns the cached value from the previous successful read.
    pub fn get_compute_time_ms(&mut self) -> Result<f32, String> {
        if self.destroyed {
            return Err("Timestamp query destroyed".to_string());
        }

        // Get timestamp period (nanoseconds per timestamp value)
        let device_properties = unsafe {
            self.context
                .instance
                .get_physical_device_properties(self.context.physical_device)
        };

        let timestamp_period = device_properties.limits.timestamp_period;

        // Read start timestamp with PARTIAL flag to avoid blocking
        // PARTIAL flag means: return available data, don't wait if not ready
        let mut start_data = [0u64; 1];
        let start_result = unsafe {
            self.context.device.get_query_pool_results(
                self.start_pool,
                0, // First query
                &mut start_data,
                vk::QueryResultFlags::TYPE_64 | vk::QueryResultFlags::PARTIAL,
            )
        };

        if let Err(e) = start_result {
            debug!("Timing data not ready yet: {:?}", e);
            return Ok(self.cached_time_ms); // Return previous value if not ready
        }

        // Read end timestamp with PARTIAL flag to avoid blocking
        let mut end_data = [0u64; 1];
        let end_result = unsafe {
            self.context.device.get_query_pool_results(
                self.end_pool,
                0, // First query
                &mut end_data,
                vk::QueryResultFlags::TYPE_64 | vk::QueryResultFlags::PARTIAL,
            )
        };

        if let Err(e) = end_result {
            debug!("Timing data not ready yet: {:?}", e);
            return Ok(self.cached_time_ms); // Return previous value if not ready
        }

        let start_ns = start_data[0] as f64 * timestamp_period as f64;
        let end_ns = end_data[0] as f64 * timestamp_period as f64;
        let elapsed_ns = end_ns - start_ns;
        let elapsed_ms = elapsed_ns / 1_000_000.0; // Convert to milliseconds

        // Cache the result
        self.cached_time_ms = elapsed_ms as f32;
        self.timing_available = true;

        debug!("Compute time: {:.3} ms ({} ns)", elapsed_ms, elapsed_ns);

        Ok(elapsed_ms as f32)
    }

    /// Get cached timing result without readback.
    ///
    /// Returns the last successfully read timing value.
    /// Returns 0.0 if no timing data is available yet.
    pub fn cached_time_ms(&self) -> f32 {
        self.cached_time_ms
    }

    /// Check if timing data is available.
    pub fn is_timing_available(&self) -> bool {
        self.timing_available
    }

    /// Destroy query pools and release resources.
    pub fn destroy(&mut self) {
        if self.destroyed {
            return;
        }
        self.destroyed = true;

        unsafe {
            if self.start_pool != vk::QueryPool::null() {
                self.context
                    .device
                    .destroy_query_pool(self.start_pool, None);
            }
            if self.end_pool != vk::QueryPool::null() {
                self.context.device.destroy_query_pool(self.end_pool, None);
            }
        }

        debug!("Destroyed timestamp query pools");
    }
}

impl Drop for TimestampQuery {
    fn drop(&mut self) {
        self.destroy();
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_timestamp_query_size() {
        // Ensure timestamp data type is correct size
        assert_eq!(std::mem::size_of::<u64>(), 8);
    }
}
