pub mod biquad;
pub mod reverb;
pub mod zone_reverb;

pub trait AudioEffect {
    fn process(&mut self, input: &mut [f32], channels: usize);
}

pub struct EffectChain {
    effects: Vec<Box<dyn AudioEffect + Send>>,
}

impl Default for EffectChain {
    fn default() -> Self {
        Self::new()
    }
}

impl EffectChain {
    pub fn new() -> Self {
        EffectChain {
            effects: Vec::new(),
        }
    }

    pub fn add_effect(&mut self, effect: Box<dyn AudioEffect + Send>) {
        self.effects.push(effect);
    }

    pub fn process(&mut self, buffer: &mut [f32], channels: usize) {
        for effect in &mut self.effects {
            effect.process(buffer, channels);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.effects.is_empty()
    }
}

pub struct AuxBus {
    pub send_level: f32,
    pub return_level: f32,
    pub effects: EffectChain,
    buffer: Vec<f32>,
}

impl AuxBus {
    pub fn new(send_level: f32, return_level: f32) -> Self {
        AuxBus {
            send_level,
            return_level,
            effects: EffectChain::new(),
            buffer: Vec::new(),
        }
    }

    pub fn add_effect(&mut self, effect: Box<dyn AudioEffect + Send>) {
        self.effects.add_effect(effect);
    }

    pub fn accumulate(&mut self, main_buffer: &[f32]) {
        if self.send_level == 0.0 {
            return;
        }
        if self.buffer.len() != main_buffer.len() {
            self.buffer.resize(main_buffer.len(), 0.0);
        }
        self.buffer.fill(0.0);
        for (dst, src) in self.buffer.iter_mut().zip(main_buffer.iter()) {
            *dst = src * self.send_level;
        }
    }

    pub fn process_effects(&mut self, channels: usize) {
        self.effects.process(&mut self.buffer, channels);
    }

    pub fn mix_into(&self, output: &mut [f32]) {
        if self.return_level == 0.0 {
            return;
        }
        for (dst, src) in output.iter_mut().zip(self.buffer.iter()) {
            *dst += src * self.return_level;
        }
    }
}
