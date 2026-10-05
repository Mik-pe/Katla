//! Opt-in frame-span timestamps backed by one Metal 4 counter heap per slot.

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_foundation::{NSRange, NSString};
use objc2_metal::{
    MTL4CommandBuffer, MTL4CounterHeap, MTL4CounterHeapDescriptor, MTL4CounterHeapType,
    MTL4TimestampHeapEntry, MTLDevice,
};
use std::cell::RefCell;

use super::submission::SubmissionCompletion;
use crate::error::RendererError;
use crate::renderer::types::GpuTimestamp;

const MAX_LABELS: usize = 64;

pub(crate) struct MetalTimestampSlot {
    heap: Retained<ProtocolObject<dyn MTL4CounterHeap>>,
    labels: Vec<String>,
    generation: u64,
    completion: Option<SubmissionCompletion>,
}

impl MetalTimestampSlot {
    pub(crate) fn new(
        device: &ProtocolObject<dyn MTLDevice>,
        slot: usize,
    ) -> Result<Self, RendererError> {
        let descriptor = MTL4CounterHeapDescriptor::new();
        descriptor.setType(MTL4CounterHeapType::Timestamp);
        unsafe {
            descriptor.setCount(2);
        }
        let heap = device
            .newCounterHeapWithDescriptor_error(&descriptor)
            .map_err(|error| {
                RendererError::InitializationFailed(format!(
                    "Metal 4 timestamp heap: {}",
                    error.localizedDescription()
                ))
            })?;
        heap.setLabel(Some(&NSString::from_str(&format!(
            "frame_slot.{slot}.timestamps"
        ))));
        Ok(Self {
            heap,
            labels: Vec::new(),
            generation: 0,
            completion: None,
        })
    }

    pub(crate) fn reset(&mut self) {
        self.labels.clear();
        self.completion = None;
    }

    pub(crate) fn begin(
        &mut self,
        command: &ProtocolObject<dyn MTL4CommandBuffer>,
        labels: &[String],
        generation: u64,
    ) {
        self.labels.clear();
        self.labels.extend_from_slice(labels);
        self.generation = generation;
        if !self.labels.is_empty() {
            unsafe {
                self.heap.invalidateCounterRange(NSRange::new(0, 2));
                command.writeTimestampIntoHeap_atIndex(&self.heap, 0);
            }
        }
    }

    pub(crate) fn end(&self, command: &ProtocolObject<dyn MTL4CommandBuffer>) {
        if !self.labels.is_empty() {
            unsafe {
                command.writeTimestampIntoHeap_atIndex(&self.heap, 1);
            }
        }
    }

    pub(crate) fn submitted(&mut self, completion: SubmissionCompletion) {
        self.completion = Some(completion);
    }

    fn resolve(&self, nanos_per_tick: f64) -> Option<Vec<GpuTimestamp>> {
        if self.labels.is_empty() {
            return None;
        }
        let completion = self.completion.as_ref()?;
        if !completion.is_complete() || completion.result("timestamp heap").is_err() {
            return None;
        }
        let data = unsafe { self.heap.resolveCounterRange(NSRange::new(0, 2)) }?;
        let bytes = unsafe { data.as_bytes_unchecked() };
        let size = std::mem::size_of::<MTL4TimestampHeapEntry>();
        if bytes.len() < size * 2 {
            return None;
        }
        let begin = unsafe {
            bytes
                .as_ptr()
                .cast::<MTL4TimestampHeapEntry>()
                .read_unaligned()
        }
        .timestamp;
        let end = unsafe {
            bytes
                .as_ptr()
                .add(size)
                .cast::<MTL4TimestampHeapEntry>()
                .read_unaligned()
        }
        .timestamp;
        if begin == 0 || end < begin {
            return None;
        }
        let duration_ms = (end - begin) as f64 * nanos_per_tick / 1_000_000.0;
        Some(
            self.labels
                .iter()
                .map(|label| GpuTimestamp {
                    label: label.clone(),
                    duration_ms,
                })
                .collect(),
        )
    }
}

pub(crate) struct MetalTimestampQueries {
    open_labels: Vec<String>,
    nanos_per_tick: f64,
    cached: RefCell<(u64, Vec<GpuTimestamp>)>,
}

impl MetalTimestampQueries {
    pub(crate) fn new() -> Result<Self, RendererError> {
        let mut timebase = mach2::mach_time::mach_timebase_info { numer: 0, denom: 0 };
        if unsafe { mach2::mach_time::mach_timebase_info(&mut timebase) } != 0
            || timebase.denom == 0
        {
            return Err(RendererError::InitializationFailed(
                "Mach timestamp timebase unavailable".into(),
            ));
        }
        Ok(Self {
            open_labels: Vec::new(),
            nanos_per_tick: timebase.numer as f64 / timebase.denom as f64,
            cached: RefCell::new((0, Vec::new())),
        })
    }

    pub(crate) fn begin(&mut self, label: &str) {
        if self.open_labels.len() < MAX_LABELS && !self.open_labels.iter().any(|open| open == label)
        {
            self.open_labels.push(label.to_owned());
        }
    }

    pub(crate) fn end(&mut self, label: &str) {
        self.open_labels.retain(|open| open != label);
    }
    pub(crate) fn labels(&self) -> &[String] {
        &self.open_labels
    }

    pub(crate) fn cache_completed(&self, slot: &MetalTimestampSlot) {
        if slot.generation <= self.cached.borrow().0 {
            return;
        }
        if let Some(results) = slot.resolve(self.nanos_per_tick) {
            *self.cached.borrow_mut() = (slot.generation, results);
        }
    }

    pub(crate) fn cached_results(&self) -> Vec<GpuTimestamp> {
        self.cached.borrow().1.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::super::context::MetalContext;
    use super::*;
    use crate::backend::command::{GpuBlitEncoder, GpuCommandBuffer};
    use crate::backend::resource::GpuBuffer;

    #[test]
    fn test_native_timestamp_heaps_retire_exact_slot() {
        let context = MetalContext::init_headless().unwrap();
        let queries = MetalTimestampQueries::new().unwrap();
        let mut slots = (0..3)
            .map(|slot| MetalTimestampSlot::new(&context.device, slot).unwrap())
            .collect::<Vec<_>>();
        assert_ne!(slots[0].heap, slots[1].heap);
        assert_ne!(slots[1].heap, slots[2].heap);
        let mut commands = Vec::new();
        for (slot, timestamps) in slots.iter_mut().enumerate() {
            let mut command = context.create_command_buffer();
            let buffer = context.create_buffer(1024 * 1024, true).unwrap();
            command.begin();
            timestamps.begin(&command.inner, &[format!("slot.{slot}")], slot as u64 + 1);
            let mut encoder = command.begin_blit_pass();
            encoder.fill_buffer(&buffer, 0, buffer.size(), slot as u8 + 1);
            encoder.end_encoding();
            timestamps.end(&command.inner);
            command.end();
            queries.cache_completed(timestamps);
            assert!(queries.cached_results().is_empty());
            command.submit(&context);
            timestamps.submitted(command.completion.clone());
            commands.push(command);
        }
        for (slot, command) in commands.iter().enumerate() {
            command.wait_until_completed().unwrap();
            queries.cache_completed(&slots[slot]);
            let result = queries.cached_results();
            assert_eq!(result.len(), 1);
            assert_eq!(result[0].label, format!("slot.{slot}"));
            assert!(result[0].duration_ms.is_finite() && result[0].duration_ms > 0.0);
            slots[slot].reset();
            assert_eq!(queries.cached_results()[0].label, format!("slot.{slot}"));
        }
    }

    #[test]
    fn test_timestamp_labels_only_arm_open_render_span() {
        let mut queries = MetalTimestampQueries::new().unwrap();
        assert!(queries.labels().is_empty());
        queries.begin("frame");
        queries.begin("frame");
        assert_eq!(queries.labels(), ["frame"]);
        queries.end("frame");
        assert!(queries.labels().is_empty());
    }
}
