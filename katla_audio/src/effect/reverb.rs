use crate::effect::AudioEffect;

const COMB_COUNT: usize = 4;
const ALLPASS_COUNT: usize = 2;

const COMB_DELAYS_L: [usize; COMB_COUNT] = [1557, 1617, 1491, 1422];
const COMB_DELAYS_R: [usize; COMB_COUNT] = [1666, 1730, 1595, 1522];
const ALLPASS_DELAYS_L: [usize; ALLPASS_COUNT] = [225, 556];
const ALLPASS_DELAYS_R: [usize; ALLPASS_COUNT] = [239, 589];
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

struct ReverbChannel {
    combs: [CombFilter; COMB_COUNT],
    allpasses: [AllPassFilter; ALLPASS_COUNT],
}

impl ReverbChannel {
    fn new(
        comb_delays: &[usize; COMB_COUNT],
        allpass_delays: &[usize; ALLPASS_COUNT],
        sample_rate: u32,
    ) -> Self {
        let scale = sample_rate as f32 / 44100.0;
        let combs = comb_delays.map(|d| CombFilter::new((d as f32 * scale) as usize));
        let allpasses = allpass_delays
            .map(|d| AllPassFilter::new((d as f32 * scale) as usize, ALLPASS_FEEDBACK));

        ReverbChannel { combs, allpasses }
    }

    fn set_decay(&mut self, decay: f32) {
        let fb = decay.clamp(0.0, 0.99);
        for comb in &mut self.combs {
            comb.feedback = fb;
        }
    }

    fn set_dampening(&mut self, dampening: f32) {
        let d = dampening.clamp(0.0, 1.0);
        for comb in &mut self.combs {
            comb.dampening = d;
        }
    }

    fn clear(&mut self) {
        for comb in &mut self.combs {
            comb.clear();
        }
        for allpass in &mut self.allpasses {
            allpass.clear();
        }
    }

    fn process_sample(&mut self, input: f32) -> f32 {
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

pub struct ReverbEffect {
    left: ReverbChannel,
    right: ReverbChannel,
    wet: f32,
}

impl ReverbEffect {
    pub fn new(sample_rate: u32) -> Self {
        let left = ReverbChannel::new(&COMB_DELAYS_L, &ALLPASS_DELAYS_L, sample_rate);
        let right = ReverbChannel::new(&COMB_DELAYS_R, &ALLPASS_DELAYS_R, sample_rate);

        ReverbEffect {
            left,
            right,
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
        self.left.set_decay(decay);
        self.right.set_decay(decay);
    }

    pub fn set_dampening(&mut self, dampening: f32) {
        self.left.set_dampening(dampening);
        self.right.set_dampening(dampening);
    }

    pub fn clear(&mut self) {
        self.left.clear();
        self.right.clear();
    }
}

impl AudioEffect for ReverbEffect {
    fn process(&mut self, input: &mut [f32], channels: usize) {
        match channels {
            1 => {
                for sample in input.iter_mut() {
                    let dry = *sample;
                    let wet = self.left.process_sample(dry);
                    *sample = dry * (1.0 - self.wet) + wet * self.wet;
                }
            }
            2 => {
                for frame in input.chunks_exact_mut(2) {
                    let dry_l = frame[0];
                    let dry_r = frame[1];
                    let wet_l = self.left.process_sample(dry_l);
                    let wet_r = self.right.process_sample(dry_r);
                    frame[0] = dry_l * (1.0 - self.wet) + wet_l * self.wet;
                    frame[1] = dry_r * (1.0 - self.wet) + wet_r * self.wet;
                }
            }
            _ => {
                for frame in input.chunks_exact_mut(channels) {
                    let dry_l = frame[0];
                    let dry_r = *frame.last().unwrap();
                    let wet_l = self.left.process_sample(dry_l);
                    let wet_r = self.right.process_sample(dry_r);
                    let wet_gain = self.wet;
                    for (i, sample) in frame.iter_mut().enumerate() {
                        let dry = *sample;
                        let wet = if i == 0 { wet_l } else { wet_r };
                        *sample = dry * (1.0 - wet_gain) + wet * wet_gain;
                    }
                }
            }
        }
    }
}
