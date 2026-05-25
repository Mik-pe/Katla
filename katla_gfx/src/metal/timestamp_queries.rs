use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_foundation::{NSRange, NSString, NSUInteger};
use objc2_metal::{
    MTLCommonCounterSetTimestamp, MTLCounterResultTimestamp, MTLCounterSampleBuffer,
    MTLCounterSampleBufferDescriptor, MTLCounterSet, MTLDevice, MTLStorageMode,
};

use crate::renderer::types::GpuTimestamp;

const MAX_SAMPLES: usize = 128;

pub(crate) struct MetalTimestampQueries {
    sample_buffer: Option<Retained<ProtocolObject<dyn MTLCounterSampleBuffer>>>,
    pending_labels: Vec<String>,
    results: Vec<GpuTimestamp>,
}

impl MetalTimestampQueries {
    pub fn new(device: &ProtocolObject<dyn MTLDevice>) -> Option<Self> {
        let counter_sets = device.counterSets()?;
        let timestamp_set = counter_sets.iter().find(|cs| {
            let name: &NSString = &cs.name();
            unsafe { name == MTLCommonCounterSetTimestamp }
        })?;

        let desc = MTLCounterSampleBufferDescriptor::new();
        desc.setCounterSet(Some(timestamp_set.as_ref()));
        unsafe { desc.setSampleCount(MAX_SAMPLES as NSUInteger) };
        desc.setStorageMode(MTLStorageMode::Shared);
        desc.setLabel(&NSString::from_str("Katla Timestamp Queries"));

        let sample_buffer = unsafe {
            device
                .newCounterSampleBufferWithDescriptor_error(&desc)
                .ok()
        };

        if sample_buffer.is_none() {
            log::warn!("Failed to create Metal counter sample buffer for timestamp queries");
            return None;
        }

        Some(Self {
            sample_buffer,
            pending_labels: Vec::new(),
            results: Vec::new(),
        })
    }

    pub fn begin(&mut self, label: &str) {
        if self.pending_labels.len() * 2 + 2 <= MAX_SAMPLES {
            self.pending_labels.push(label.to_string());
        }
    }

    pub fn end(&mut self, _label: &str) {}

    pub fn sample_buffer(&self) -> Option<&Retained<ProtocolObject<dyn MTLCounterSampleBuffer>>> {
        self.sample_buffer.as_ref()
    }

    pub fn pending_count(&self) -> usize {
        self.pending_labels.len()
    }

    pub fn cached_results(&self) -> Vec<GpuTimestamp> {
        self.results.clone()
    }

    pub fn resolve_and_cache(&mut self) {
        let Some(ref sb) = self.sample_buffer else {
            self.pending_labels.clear();
            return;
        };

        let num_pairs = self.pending_labels.len();
        if num_pairs == 0 {
            return;
        }

        let total_samples = num_pairs * 2;
        let data = unsafe { sb.resolveCounterRange(NSRange::new(0, total_samples as NSUInteger)) };

        if let Some(data) = data {
            let byte_slice = unsafe { data.as_bytes_unchecked() };
            let sample_size = std::mem::size_of::<MTLCounterResultTimestamp>();
            let actual_samples = byte_slice.len() / sample_size;

            self.results.clear();
            for (i, label) in self.pending_labels.drain(..).enumerate() {
                if i * 2 + 1 < actual_samples {
                    let begin_offset = i * 2 * sample_size;
                    let end_offset = (i * 2 + 1) * sample_size;
                    let begin_ts = unsafe {
                        byte_slice
                            .as_ptr()
                            .add(begin_offset)
                            .cast::<MTLCounterResultTimestamp>()
                            .read_unaligned()
                    }
                    .timestamp;
                    let end_ts = unsafe {
                        byte_slice
                            .as_ptr()
                            .add(end_offset)
                            .cast::<MTLCounterResultTimestamp>()
                            .read_unaligned()
                    }
                    .timestamp;
                    if begin_ts != 0 && end_ts != 0 && end_ts >= begin_ts {
                        let elapsed = end_ts - begin_ts;
                        let duration_ns = elapsed as f64;
                        let duration_ms = duration_ns / 1_000_000.0;
                        self.results.push(GpuTimestamp { label, duration_ms });
                    }
                }
            }
        } else {
            self.pending_labels.clear();
        }
    }
}
