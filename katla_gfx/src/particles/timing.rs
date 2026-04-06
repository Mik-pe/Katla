//! GPU timing queries for particle system compute shader execution.

use std::rc::Rc;

use ash::vk;
use log::debug;

use crate::vulkan::context::VulkanContext;

/// GPU timestamp query pools for measuring compute shader execution time.
#[allow(dead_code)]
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

#[allow(dead_code)]
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
    /// Does NOT use PARTIAL_BIT as it's not allowed for timestamp queries per Vulkan spec.
    /// If timing data isn't ready yet (query not reset on GPU), returns cached value.
    pub fn get_compute_time_ms(&mut self) -> Result<f32, String> {
        if self.destroyed {
            return Err("Timestamp query destroyed".to_string());
        }

        // If no timing has been recorded yet, return 0
        if !self.timing_available {
            return Ok(0.0);
        }

        // Get timestamp period (nanoseconds per timestamp value)
        let device_properties = unsafe {
            self.context
                .instance
                .get_physical_device_properties(self.context.physical_device)
        };

        let timestamp_period = device_properties.limits.timestamp_period;

        // Read start timestamp - don't use WAIT to avoid blocking
        // If query hasn't been reset/executed yet, we'll get an error
        let mut start_data = [0u64; 1];
        let start_result = unsafe {
            self.context.device.get_query_pool_results(
                self.start_pool,
                0, // First query
                &mut start_data,
                vk::QueryResultFlags::TYPE_64,
            )
        };

        if let Err(e) = start_result {
            debug!("Start timing data not ready: {:?}", e);
            return Ok(self.cached_time_ms);
        }

        // Read end timestamp
        let mut end_data = [0u64; 1];
        let end_result = unsafe {
            self.context.device.get_query_pool_results(
                self.end_pool,
                0, // First query
                &mut end_data,
                vk::QueryResultFlags::TYPE_64,
            )
        };

        if let Err(e) = end_result {
            debug!("End timing data not ready: {:?}", e);
            return Ok(self.cached_time_ms);
        }

        // Check for zero values (query not ready or not executed)
        if start_data[0] == 0 || end_data[0] == 0 {
            return Ok(self.cached_time_ms);
        }

        let start_ns = start_data[0] as f64 * timestamp_period as f64;
        let end_ns = end_data[0] as f64 * timestamp_period as f64;
        let elapsed_ns = end_ns - start_ns;
        let elapsed_ms = elapsed_ns / 1_000_000.0; // Convert to milliseconds

        // Cache the result
        self.cached_time_ms = elapsed_ms as f32;

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
