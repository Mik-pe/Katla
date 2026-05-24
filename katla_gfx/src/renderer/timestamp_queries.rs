use ash::vk;

use super::types::GpuTimestamp;

const MAX_TIMESTAMP_QUERIES: usize = 128;

pub(crate) struct TimestampQueries {
    pool: vk::QueryPool,
    pending_labels: Vec<String>,
    results: Vec<GpuTimestamp>,
    query_count: u32,
    timestamp_period: f32,
}

impl TimestampQueries {
    pub fn new(device: &ash::Device, timestamp_period: f32) -> Result<Self, vk::Result> {
        let create_info = vk::QueryPoolCreateInfo::default()
            .query_type(vk::QueryType::TIMESTAMP)
            .query_count(MAX_TIMESTAMP_QUERIES as u32);

        let pool = unsafe { device.create_query_pool(&create_info, None)? };

        Ok(TimestampQueries {
            pool,
            pending_labels: Vec::new(),
            results: Vec::new(),
            query_count: 0,
            timestamp_period,
        })
    }

    pub fn begin(&mut self, label: &str) {
        if self.query_count as usize + 2 <= MAX_TIMESTAMP_QUERIES {
            self.pending_labels.push(label.to_string());
        }
    }

    pub fn end(&mut self, _label: &str) {
        // Each begin/end pair uses 2 query indices; the label is recorded at begin
    }

    pub fn record_pending(&mut self, device: &ash::Device, command_buffer: vk::CommandBuffer) {
        let total_queries = self.pending_labels.len() * 2;
        if total_queries == 0 || total_queries > MAX_TIMESTAMP_QUERIES {
            return;
        }

        unsafe {
            device.cmd_reset_query_pool(command_buffer, self.pool, 0, total_queries as u32);
        }

        for (i, _) in self.pending_labels.iter().enumerate() {
            let begin_query = i as u32 * 2;
            let end_query = i as u32 * 2 + 1;
            unsafe {
                device.cmd_write_timestamp(
                    command_buffer,
                    vk::PipelineStageFlags::TOP_OF_PIPE,
                    self.pool,
                    begin_query,
                );
                device.cmd_write_timestamp(
                    command_buffer,
                    vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                    self.pool,
                    end_query,
                );
            }
        }

        self.query_count = total_queries as u32;
    }

    pub fn cached_results(&self) -> Vec<GpuTimestamp> {
        self.results.clone()
    }

    /// Read GPU results and update the cache. Call this once per frame after GPU work completes.
    pub fn read_and_cache(&mut self, device: &ash::Device) {
        self.results = self.read_results_inner(device);
    }

    fn read_results_inner(&mut self, device: &ash::Device) -> Vec<GpuTimestamp> {
        if self.query_count == 0 {
            self.pending_labels.clear();
            return Vec::new();
        }

        let num_pairs = self.query_count as usize / 2;
        let mut timestamps = vec![0u64; self.query_count as usize];

        let result = unsafe {
            device.get_query_pool_results(
                self.pool,
                0,
                &mut timestamps,
                vk::QueryResultFlags::TYPE_64 | vk::QueryResultFlags::WAIT,
            )
        };

        let mut results = Vec::with_capacity(num_pairs);
        if result.is_ok() {
            for (i, label) in self.pending_labels.drain(..).enumerate() {
                if i * 2 + 1 < timestamps.len() && timestamps[i * 2 + 1] != 0 {
                    let begin_ts = timestamps[i * 2];
                    let end_ts = timestamps[i * 2 + 1];
                    let duration_ns =
                        end_ts.wrapping_sub(begin_ts) as f64 * self.timestamp_period as f64;
                    let duration_ms = duration_ns / 1_000_000.0;
                    results.push(GpuTimestamp { label, duration_ms });
                }
            }
        } else {
            self.pending_labels.clear();
        }

        self.query_count = 0;
        results
    }

    pub fn destroy(&self, device: &ash::Device) {
        unsafe {
            device.destroy_query_pool(self.pool, None);
        }
    }
}
