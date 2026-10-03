//! CPU reference only: no device, stream, codec or sound file is opened.
use katla_audio::{AudioEffect, AuxBus, BiquadFilter, EffectChain, FilterKind, ReverbEffect};

fn input(block: usize, channels: usize) -> Vec<f32> {
    (0..64 * channels)
        .map(|i| {
            let frame = block * 64 + i / channels;
            let ch = i % channels;
            if frame == 0 {
                if ch == 0 { 1.0 } else { 0.5 }
            } else if frame < 512 {
                (frame as f32 * 0.071).sin() * if ch == 0 { 0.2 } else { -0.1 }
            } else {
                0.0
            }
        })
        .collect()
}
fn emit(name: &str, rate: u32, channels: usize, block: usize, values: &[f32]) {
    print!("{name}_{rate}_{channels}_{block}");
    for value in values {
        print!(" {value:.9e}");
    }
    println!();
}
fn main() {
    for rate in [44100, 48000, 96000] {
        for channels in [1, 2] {
            for (kind, name) in [(FilterKind::LowPass, "low"), (FilterKind::HighPass, "high")] {
                let mut filter = BiquadFilter::new(kind, 800.0, rate as f32);
                for block in 0..32 {
                    if block == 8 {
                        filter.set_cutoff(3200.0);
                    }
                    let mut signal = input(block, channels);
                    filter.process(&mut signal, channels);
                    emit(name, rate, channels, block, &signal);
                }
            }
            let mut reverb = ReverbEffect::new(rate);
            for block in 0..32 {
                if block == 16 {
                    reverb.set_wet(0.6);
                    reverb.set_decay(0.65);
                    reverb.set_dampening(0.3);
                }
                let mut signal = input(block, channels);
                reverb.process(&mut signal, channels);
                emit("reverb", rate, channels, block, &signal);
            }
            let mut chain = EffectChain::new();
            chain.add_effect(Box::new(BiquadFilter::new(
                FilterKind::LowPass,
                1000.0,
                rate as f32,
            )));
            chain.add_effect(Box::new(ReverbEffect::new(rate)));
            for block in 0..32 {
                let mut signal = input(block, channels);
                chain.process(&mut signal, channels);
                emit("chain", rate, channels, block, &signal);
            }
            let mut bus = AuxBus::new(0.4, 0.7);
            bus.add_effect(Box::new(BiquadFilter::new(
                FilterKind::HighPass,
                300.0,
                rate as f32,
            )));
            bus.add_effect(Box::new(ReverbEffect::new(rate)));
            for block in 0..32 {
                let mut signal = input(block, channels);
                bus.prepare(signal.len());
                bus.accumulate_voice(&signal, bus.send_level);
                let second: Vec<f32> = signal.iter().map(|v| -*v * 0.3).collect();
                bus.accumulate_voice(&second, 0.2);
                bus.process_effects(channels);
                bus.mix_into(&mut signal);
                emit("aux", rate, channels, block, &signal);
            }
        }
    }
    for i in 0..41 {
        let pan = i as f32 / 20.0 - 1.0;
        let (l, r) = katla_audio::compute_pan_gains(pan);
        let gain = katla_audio::db_to_linear(i as f32 - 20.0);
        emit("gain", 0, 0, i, &[l, r, gain]);
    }
}
