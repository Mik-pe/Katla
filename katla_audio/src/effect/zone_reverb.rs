use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use super::AudioEffect;
use crate::effect::reverb::ReverbEffect;

fn bits_to_f32(v: u32) -> f32 {
    f32::from_bits(v)
}

/// Reverb effect whose parameters are controlled via shared atomics.
///
/// Target parameters (decay, wet, dampening) are written from the main thread
/// and smoothly interpolated toward each render frame. When all targets are
/// zero, the effect outputs silence (no zone active).
pub struct ZoneReverbEffect {
    inner: ReverbEffect,
    target_decay: Arc<AtomicU32>,
    target_wet: Arc<AtomicU32>,
    target_dampening: Arc<AtomicU32>,
    current_decay: f32,
    current_wet: f32,
    current_dampening: f32,
}

impl ZoneReverbEffect {
    pub fn new(
        sample_rate: u32,
        target_decay: Arc<AtomicU32>,
        target_wet: Arc<AtomicU32>,
        target_dampening: Arc<AtomicU32>,
    ) -> Self {
        let mut inner = ReverbEffect::new(sample_rate);
        inner.set_wet(0.0);
        ZoneReverbEffect {
            inner,
            target_decay,
            target_wet,
            target_dampening,
            current_decay: 0.0,
            current_wet: 0.0,
            current_dampening: 0.2,
        }
    }
}

impl AudioEffect for ZoneReverbEffect {
    fn process(&mut self, input: &mut [f32], channels: usize) {
        let target_decay = bits_to_f32(self.target_decay.load(Ordering::Relaxed));
        let target_wet = bits_to_f32(self.target_wet.load(Ordering::Relaxed));
        let target_dampening = bits_to_f32(self.target_dampening.load(Ordering::Relaxed));

        // Smooth toward targets (exponential smoothing)
        let smoothing = 0.08;
        self.current_decay += (target_decay - self.current_decay) * smoothing;
        self.current_wet += (target_wet - self.current_wet) * smoothing;
        self.current_dampening += (target_dampening - self.current_dampening) * smoothing;

        // If effectively no zone active, zero output and skip processing
        if self.current_wet < 0.001 {
            input.fill(0.0);
            return;
        }

        self.inner.set_decay(self.current_decay);
        self.inner.set_wet(self.current_wet);
        self.inner.set_dampening(self.current_dampening);

        self.inner.process(input, channels);
    }
}
