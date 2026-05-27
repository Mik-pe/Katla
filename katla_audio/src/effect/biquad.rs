use crate::effect::AudioEffect;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterKind {
    LowPass,
    HighPass,
}

pub struct BiquadFilter {
    kind: FilterKind,
    cutoff: f32,
    sample_rate: f32,
    q: f32,
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    x1: [f32; 2],
    x2: [f32; 2],
    y1: [f32; 2],
    y2: [f32; 2],
}

impl BiquadFilter {
    pub fn new(kind: FilterKind, cutoff: f32, sample_rate: f32) -> Self {
        let mut filter = BiquadFilter {
            kind,
            cutoff,
            sample_rate,
            q: 0.707,
            b0: 0.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            x1: [0.0; 2],
            x2: [0.0; 2],
            y1: [0.0; 2],
            y2: [0.0; 2],
        };
        filter.recalculate();
        filter
    }

    pub fn set_cutoff(&mut self, cutoff: f32) {
        self.cutoff = cutoff;
        self.recalculate();
    }

    pub fn cutoff(&self) -> f32 {
        self.cutoff
    }

    fn recalculate(&mut self) {
        let w0 = 2.0 * std::f32::consts::PI * self.cutoff / self.sample_rate;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let alpha = sin_w0 / (2.0 * self.q);

        let (b0, b1, b2, a0, a1, a2) = match self.kind {
            FilterKind::LowPass => {
                let b1 = 1.0 - cos_w0;
                let b0 = b1 * 0.5;
                let b2 = b0;
                let a0 = 1.0 + alpha;
                (b0, b1, b2, a0, -2.0 * cos_w0, 1.0 - alpha)
            }
            FilterKind::HighPass => {
                let b0 = (1.0 + cos_w0) * 0.5;
                let b1 = -(1.0 + cos_w0);
                let b2 = b0;
                let a0 = 1.0 + alpha;
                (b0, b1, b2, a0, -2.0 * cos_w0, 1.0 - alpha)
            }
        };

        self.b0 = b0 / a0;
        self.b1 = b1 / a0;
        self.b2 = b2 / a0;
        self.a1 = a1 / a0;
        self.a2 = a2 / a0;
    }

    fn process_sample(&mut self, ch: usize, x0: f32) -> f32 {
        let y0 = self.b0 * x0 + self.b1 * self.x1[ch] + self.b2 * self.x2[ch]
            - self.a1 * self.y1[ch]
            - self.a2 * self.y2[ch];
        self.x2[ch] = self.x1[ch];
        self.x1[ch] = x0;
        self.y2[ch] = self.y1[ch];
        self.y1[ch] = y0;
        y0
    }
}

impl AudioEffect for BiquadFilter {
    fn process(&mut self, input: &mut [f32], channels: usize) {
        match channels {
            1 => {
                for sample in input.iter_mut() {
                    *sample = self.process_sample(0, *sample);
                }
            }
            2 => {
                for frame in input.chunks_exact_mut(2) {
                    frame[0] = self.process_sample(0, frame[0]);
                    frame[1] = self.process_sample(1, frame[1]);
                }
            }
            _ => {
                for (i, sample) in input.iter_mut().enumerate() {
                    let ch = i % channels;
                    *sample = self.process_sample(ch.min(1), *sample);
                }
            }
        }
    }
}
