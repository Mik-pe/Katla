use crate::effect::AudioEffect;

const COMB_COUNT: usize = 4;
const ALLPASS_COUNT: usize = 2;

const COMB_DELAYS: [usize; COMB_COUNT] = [1557, 1617, 1491, 1422];
const ALLPASS_DELAYS: [usize; ALLPASS_COUNT] = [225, 556];
const ALLPASS_FEEDBACK: f32 = 0.5;

struct CombFilter {
    buffer: Vec<f32>,
    index: usize,
    feedback: f32,
    filter_store: f32,
    dampening: f32,
}

impl CombFilter {
    fn new(delay: usize) -> Self {
        CombFilter {
            buffer: vec![0.0; delay],
            index: 0,
            feedback: 0.84,
            filter_store: 0.0,
            dampening: 0.2,
        }
    }

    fn process(&mut self, input: f32) -> f32 {
        let output = self.buffer[self.index];
        self.filter_store = output * (1.0 - self.dampening) + self.filter_store * self.dampening;
        self.buffer[self.index] = input + self.filter_store * self.feedback;
        self.index = (self.index + 1) % self.buffer.len();
        output
    }

    fn clear(&mut self) {
        self.buffer.fill(0.0);
        self.index = 0;
        self.filter_store = 0.0;
    }
}

struct AllPassFilter {
    buffer: Vec<f32>,
    index: usize,
    feedback: f32,
}

impl AllPassFilter {
    fn new(delay: usize, feedback: f32) -> Self {
        AllPassFilter {
            buffer: vec![0.0; delay],
            index: 0,
            feedback,
        }
    }

    fn process(&mut self, input: f32) -> f32 {
        let buffered = self.buffer[self.index];
        let output = -input + buffered;
        self.buffer[self.index] = input + buffered * self.feedback;
        self.index = (self.index + 1) % self.buffer.len();
        output
    }

    fn clear(&mut self) {
        self.buffer.fill(0.0);
        self.index = 0;
    }
}

pub struct ReverbEffect {
    combs: [CombFilter; COMB_COUNT],
    allpasses: [AllPassFilter; ALLPASS_COUNT],
    wet: f32,
}

impl ReverbEffect {
    pub fn new(sample_rate: u32) -> Self {
        let scale = sample_rate as f32 / 44100.0;
        let combs = COMB_DELAYS.map(|d| CombFilter::new((d as f32 * scale) as usize));
        let allpasses = ALLPASS_DELAYS
            .map(|d| AllPassFilter::new((d as f32 * scale) as usize, ALLPASS_FEEDBACK));

        ReverbEffect {
            combs,
            allpasses,
            wet: 0.3,
        }
    }

    pub fn set_wet(&mut self, wet: f32) {
        self.wet = wet.clamp(0.0, 1.0);
    }

    pub fn wet(&self) -> f32 {
        self.wet
    }

    pub fn set_decay(&mut self, decay: f32) {
        let fb = decay.clamp(0.0, 0.99);
        for comb in &mut self.combs {
            comb.feedback = fb;
        }
    }

    pub fn set_dampening(&mut self, dampening: f32) {
        let d = dampening.clamp(0.0, 1.0);
        for comb in &mut self.combs {
            comb.dampening = d;
        }
    }

    pub fn clear(&mut self) {
        for comb in &mut self.combs {
            comb.clear();
        }
        for allpass in &mut self.allpasses {
            allpass.clear();
        }
    }

    fn process_sample_reverb(&mut self, input: f32) -> f32 {
        let mut reverb = 0.0;
        for comb in &mut self.combs {
            reverb += comb.process(input);
        }

        for allpass in &mut self.allpasses {
            reverb = allpass.process(reverb);
        }

        reverb
    }
}

impl AudioEffect for ReverbEffect {
    fn process(&mut self, input: &mut [f32], channels: usize) {
        match channels {
            1 => {
                for sample in input.iter_mut() {
                    let dry = *sample;
                    let wet = self.process_sample_reverb(dry);
                    *sample = dry * (1.0 - self.wet) + wet * self.wet;
                }
            }
            2 => {
                for frame in input.chunks_exact_mut(2) {
                    let mono_in = (frame[0] + frame[1]) * 0.5;
                    let dry_l = frame[0];
                    let dry_r = frame[1];
                    let wet = self.process_sample_reverb(mono_in);
                    frame[0] = dry_l * (1.0 - self.wet) + wet * self.wet;
                    frame[1] = dry_r * (1.0 - self.wet) + wet * self.wet;
                }
            }
            _ => {
                for frame in input.chunks_exact_mut(channels) {
                    let mut mono_in = 0.0;
                    for s in &*frame {
                        mono_in += *s;
                    }
                    mono_in /= channels as f32;
                    let wet = self.process_sample_reverb(mono_in);
                    let wet_gain = self.wet;
                    for sample in frame.iter_mut() {
                        let dry = *sample;
                        *sample = dry * (1.0 - wet_gain) + wet * wet_gain;
                    }
                }
            }
        }
    }
}
