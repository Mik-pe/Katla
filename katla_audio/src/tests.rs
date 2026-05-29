use crate::buffer::{AudioBuffer, DecodedAudio, SampleFormat, load_wav};
use crate::command_queue::AudioCategoryValue;
use crate::effect::biquad::{BiquadFilter, FilterKind};
use crate::effect::reverb::ReverbEffect;
use crate::effect::{AudioEffect, AuxBus};
use crate::engine::AudioEngine;
use crate::voice::{VoicePriority, VoiceState};
use std::sync::Arc;

fn make_test_buffer(sample_rate: u32, channels: u16, samples: Vec<f32>) -> AudioBuffer {
    AudioBuffer {
        sample_rate,
        channels,
        samples,
    }
}

#[test]
fn test_buffer_duration() {
    let buf = make_test_buffer(44100, 2, vec![0.0; 44100 * 2]);
    assert!((buf.duration_secs() - 1.0).abs() < 0.001);
}

#[test]
fn test_buffer_sample_count() {
    let buf = make_test_buffer(44100, 2, vec![0.0; 44100 * 2]);
    assert_eq!(buf.sample_count(), 44100);
}

#[test]
fn test_decode_from_i16() {
    let decoded = DecodedAudio {
        sample_rate: 44100,
        channels: 1,
        samples: SampleFormat::I16(vec![0, i16::MAX, i16::MIN]),
    };
    let buf = AudioBuffer::from_decoded(decoded);
    assert!((buf.samples[0]).abs() < 0.001);
    assert!((buf.samples[1] - 1.0).abs() < 0.001);
    assert!((buf.samples[2] - (-1.0)).abs() < 0.001);
}

#[test]
fn test_decode_from_f32() {
    let decoded = DecodedAudio {
        sample_rate: 44100,
        channels: 2,
        samples: SampleFormat::F32(vec![0.5, -0.5, 0.25]),
    };
    let buf = AudioBuffer::from_decoded(decoded);
    assert!((buf.samples[0] - 0.5).abs() < 0.001);
    assert!((buf.samples[1] - (-0.5)).abs() < 0.001);
    assert!((buf.samples[2] - 0.25).abs() < 0.001);
}

#[test]
fn test_mixer_mixes_voices() {
    let mixer = crate::mixer::AudioMixer::new(44100, 2);

    let buf1 = Arc::new(make_test_buffer(44100, 2, vec![0.5; 2048]));
    let buf2 = Arc::new(make_test_buffer(44100, 2, vec![0.3; 2048]));

    mixer.play(buf1, AudioCategoryValue::Sfx, VoicePriority::default());
    mixer.play(buf2, AudioCategoryValue::Sfx, VoicePriority::default());

    let mut fade_buf = vec![0.0f32; 512];
    mixer.render(&mut fade_buf);

    let mut output = vec![0.0f32; 128];
    mixer.render(&mut output);

    for sample in &output {
        let expected = 0.8 * std::f32::consts::FRAC_1_SQRT_2;
        assert!(
            (*sample - expected).abs() < 0.01,
            "Expected ~{expected:.3}, got {sample}"
        );
    }
}

#[test]
fn test_mixer_master_volume() {
    let mixer = crate::mixer::AudioMixer::new(44100, 2);

    let buf = Arc::new(make_test_buffer(44100, 2, vec![1.0; 2048]));
    mixer.play(buf, AudioCategoryValue::Sfx, VoicePriority::default());
    mixer.set_master_volume(0.5);

    let mut fade_buf = vec![0.0f32; 512];
    mixer.render(&mut fade_buf);

    let mut output = vec![0.0f32; 64];
    mixer.render(&mut output);

    for sample in &output {
        let expected = 0.5 * std::f32::consts::FRAC_1_SQRT_2;
        assert!(
            (*sample - expected).abs() < 0.01,
            "Expected ~{expected:.3}, got {sample}"
        );
    }
}

#[test]
fn test_mixer_clamping() {
    let mixer = crate::mixer::AudioMixer::new(44100, 2);

    let buf = Arc::new(make_test_buffer(44100, 2, vec![0.9; 2048]));
    mixer.play(
        buf.clone(),
        AudioCategoryValue::Sfx,
        VoicePriority::default(),
    );
    mixer.play(buf, AudioCategoryValue::Sfx, VoicePriority::default());
    mixer.set_master_volume(1.0);

    let mut fade_buf = vec![0.0f32; 512];
    mixer.render(&mut fade_buf);

    let mut output = vec![0.0f32; 64];
    mixer.render(&mut output);

    for sample in &output {
        assert!(*sample <= 1.0, "Expected clamped to 1.0, got {sample}");
    }
}

#[test]
fn test_voice_stop() {
    let mixer = crate::mixer::AudioMixer::new(44100, 2);

    let buf = Arc::new(make_test_buffer(44100, 2, vec![1.0; 8192]));
    let id = mixer.play(buf, AudioCategoryValue::Sfx, VoicePriority::default());

    let mut fade_buf = vec![0.0f32; 512];
    mixer.render(&mut fade_buf);

    assert_eq!(mixer.active_voice_count(), 1);
    mixer.stop(id);

    let mut fade_out_buf = vec![0.0f32; 512];
    mixer.render(&mut fade_out_buf);

    let mut output = vec![0.0f32; 64];
    mixer.render(&mut output);

    for sample in &output {
        assert!(
            sample.abs() < 0.001,
            "Expected silence after stop + fade-out, got {sample}"
        );
    }
    assert_eq!(mixer.active_voice_count(), 0);
}

#[test]
fn test_voice_finished_cleanup() {
    let mixer = crate::mixer::AudioMixer::new(44100, 2);

    let buf = Arc::new(make_test_buffer(44100, 2, vec![0.5; 64]));
    mixer.play(buf, AudioCategoryValue::Sfx, VoicePriority::default());

    let mut output = vec![0.0f32; 64];
    mixer.render(&mut output);
    assert_eq!(mixer.active_voice_count(), 0);
}

#[test]
fn test_voice_set_volume() {
    let mixer = crate::mixer::AudioMixer::new(44100, 2);

    let buf = Arc::new(make_test_buffer(44100, 2, vec![1.0; 2048]));
    let id = mixer.play(buf, AudioCategoryValue::Sfx, VoicePriority::default());
    mixer.set_voice_volume(id, 0.25);

    let mut fade_buf = vec![0.0f32; 512];
    mixer.render(&mut fade_buf);

    let mut output = vec![0.0f32; 64];
    mixer.render(&mut output);

    for sample in &output {
        let expected = 0.25 * std::f32::consts::FRAC_1_SQRT_2;
        assert!(
            (*sample - expected).abs() < 0.01,
            "Expected ~{expected:.3}, got {sample}"
        );
    }
}

#[test]
fn test_mono_to_stereo_upmix() {
    let mixer = crate::mixer::AudioMixer::new(44100, 2);

    let buf = Arc::new(make_test_buffer(44100, 1, vec![0.7; 1024]));
    mixer.play(buf, AudioCategoryValue::Sfx, VoicePriority::default());

    let mut fade_buf = vec![0.0f32; 512];
    mixer.render(&mut fade_buf);

    let mut output = vec![0.0f32; 64];
    mixer.render(&mut output);

    let (left_gain, _right_gain) = crate::voice::compute_pan_gains(0.0);
    let expected = 0.7 * left_gain;
    for frame in 0..32 {
        let left = output[frame * 2];
        let right = output[frame * 2 + 1];
        assert!(
            (left - expected).abs() < 0.01,
            "Expected left ~{expected:.3}, got {left}"
        );
        assert!(
            (right - expected).abs() < 0.01,
            "Expected right ~{expected:.3}, got {right}"
        );
    }
}

#[test]
fn test_wav_round_trip() {
    let tmp = std::env::temp_dir().join("katla_audio_test.wav");

    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: 44100,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    {
        let mut writer = hound::WavWriter::create(&tmp, spec).unwrap();
        for i in 0..256u32 {
            let sample = (i as f32 / 256.0) * 2.0 - 1.0;
            writer.write_sample(sample).unwrap();
            writer.write_sample(sample).unwrap();
        }
    }

    let buf = load_wav(&tmp).unwrap();
    assert_eq!(buf.sample_rate, 44100);
    assert_eq!(buf.channels, 2);
    assert_eq!(buf.samples.len(), 512);
    assert!((buf.samples[0] - (-1.0)).abs() < 0.001);

    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn test_engine_playback_lifecycle() {
    let engine = match AudioEngine::new() {
        Ok(e) => e,
        Err(_) => {
            eprintln!("Skipping engine test: no audio device available");
            return;
        }
    };

    let buf = Arc::new(make_test_buffer(
        engine.sample_rate(),
        engine.channels(),
        vec![0.5; 4096],
    ));
    let handle = engine.play(&buf);

    assert_eq!(handle.state(), VoiceState::Playing);
    handle.set_volume(0.5);
    assert!((handle.volume() - 0.5).abs() < 0.001);

    engine.resume().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(50));

    handle.stop();
    assert_eq!(handle.state(), VoiceState::Stopped);

    engine.stop_all();
    assert_eq!(engine.active_voice_count(), 0);
}

#[test]
fn test_voice_pan_stereo() {
    let mixer = crate::mixer::AudioMixer::new(44100, 2);

    let buf = Arc::new(make_test_buffer(44100, 2, vec![1.0; 2048]));
    let id = mixer.play(buf, AudioCategoryValue::Sfx, VoicePriority::default());
    mixer.set_voice_pan(id, 1.0);

    let mut fade_buf = vec![0.0f32; 512];
    mixer.render(&mut fade_buf);

    let mut output = vec![0.0f32; 64];
    mixer.render(&mut output);

    for frame in 0..32 {
        let left = output[frame * 2];
        let right = output[frame * 2 + 1];
        assert!(
            right > left,
            "Expected right > left with full-right pan, got left={left}, right={right}"
        );
    }
}

#[test]
fn test_voice_pan_mono_source() {
    let mixer = crate::mixer::AudioMixer::new(44100, 2);

    let buf = Arc::new(make_test_buffer(44100, 1, vec![1.0; 1024]));
    let id = mixer.play(buf, AudioCategoryValue::Sfx, VoicePriority::default());
    mixer.set_voice_pan(id, -1.0);

    let mut fade_buf = vec![0.0f32; 512];
    mixer.render(&mut fade_buf);

    let mut output = vec![0.0f32; 64];
    mixer.render(&mut output);

    for frame in 0..32 {
        let left = output[frame * 2];
        let right = output[frame * 2 + 1];
        assert!(
            left > right,
            "Expected left > right with full-left pan, got left={left}, right={right}"
        );
        assert!(left > 0.0, "Expected left > 0, got {left}");
    }
}

#[test]
fn test_pan_gains_equal_power() {
    let (l, r) = crate::voice::compute_pan_gains(0.0);
    let expected = std::f32::consts::FRAC_1_SQRT_2;
    assert!(
        (l - expected).abs() < 0.001,
        "Center pan left gain should be ~0.707, got {l}"
    );
    assert!(
        (r - expected).abs() < 0.001,
        "Center pan right gain should be ~0.707, got {r}"
    );
}

#[test]
fn test_pan_gains_symmetry() {
    let (l_r, r_r) = crate::voice::compute_pan_gains(1.0);
    let (l_l, r_l) = crate::voice::compute_pan_gains(-1.0);
    assert!((l_r - r_l).abs() < 0.001, "Pan gains should be symmetric");
    assert!((r_r - l_l).abs() < 0.001, "Pan gains should be symmetric");
}

#[test]
fn test_pan_gains_constant_power() {
    for pan_val in [-1.0, -0.5, 0.0, 0.5, 1.0] {
        let (l, r) = crate::voice::compute_pan_gains(pan_val);
        let power = l * l + r * r;
        assert!(
            (power - 1.0).abs() < 0.01,
            "Power should be ~1.0 at pan={pan_val}, got {power}"
        );
    }
}

#[test]
fn test_category_volume_applied() {
    let mixer = crate::mixer::AudioMixer::new(44100, 2);
    mixer.set_category_volume(AudioCategoryValue::Music, 0.5);

    let buf = Arc::new(make_test_buffer(44100, 2, vec![1.0; 2048]));
    mixer.play(buf, AudioCategoryValue::Music, VoicePriority::default());

    let mut fade_buf = vec![0.0f32; 512];
    mixer.render(&mut fade_buf);

    let mut output = vec![0.0f32; 64];
    mixer.render(&mut output);

    let (left_gain, _right_gain) = crate::voice::compute_pan_gains(0.0);
    let expected = 1.0 * 0.5 * left_gain;
    for sample in &output {
        assert!(
            (*sample - expected).abs() < 0.01,
            "Expected ~{expected:.3} with category volume 0.5, got {sample}"
        );
    }
}

#[test]
fn test_lowpass_filter_attenuates_high_frequency() {
    let sample_rate = 44100.0;
    let mut lpf = BiquadFilter::new(FilterKind::LowPass, 1000.0, sample_rate);

    let dc: Vec<f32> = vec![1.0; 512];
    let mut output = dc.clone();
    lpf.process(&mut output, 1);

    let dc_energy: f32 = output[256..].iter().map(|s| s.abs()).sum();

    let mut alternating = vec![0.0f32; 512];
    for i in 0..256 {
        alternating[i * 2] = 1.0;
        alternating[i * 2 + 1] = -1.0;
    }
    let mut high_output = alternating.clone();
    lpf.process(&mut high_output, 1);

    let high_energy: f32 = high_output[256..].iter().map(|s| s.abs()).sum();

    assert!(
        high_energy < dc_energy * 0.1,
        "High frequency should be strongly attenuated: dc_energy={dc_energy}, high_energy={high_energy}"
    );
}

#[test]
fn test_highpass_filter_attenuates_dc() {
    let sample_rate = 44100.0;
    let mut hpf = BiquadFilter::new(FilterKind::HighPass, 1000.0, sample_rate);

    let mut dc = vec![1.0f32; 512];
    hpf.process(&mut dc, 1);

    let dc_energy: f32 = dc[256..].iter().map(|s| s.abs()).sum();
    assert!(
        dc_energy < 1.0,
        "DC should be strongly attenuated by HPF: energy={dc_energy}"
    );
}

#[test]
fn test_filter_stereo() {
    let mut lpf = BiquadFilter::new(FilterKind::LowPass, 2000.0, 44100.0);

    let mut stereo = vec![1.0f32; 128];
    lpf.process(&mut stereo, 2);

    assert!(
        stereo.iter().all(|s| s.is_finite()),
        "Filter output should be finite"
    );
}

#[test]
fn test_reverb_adds_tail() {
    let mut reverb = ReverbEffect::new(44100);
    reverb.set_wet(1.0);

    let mut input = vec![0.0f32; 4096];
    input[0] = 1.0;
    input[1] = 1.0;
    input[2] = 1.0;
    input[3] = 1.0;

    reverb.process(&mut input, 2);

    let tail_energy: f32 = input[64..].iter().map(|s| s * s).sum::<f32>();
    assert!(
        tail_energy > 0.01,
        "Reverb should produce a tail after the impulse: energy={tail_energy}"
    );
}

#[test]
fn test_reverb_wet_dry_mix() {
    let mut reverb = ReverbEffect::new(44100);
    reverb.set_wet(0.0);

    let input = vec![0.5f32; 128];
    let mut output = input.clone();
    reverb.process(&mut output, 1);

    for (i, sample) in output.iter().enumerate() {
        assert!(
            (*sample - input[i]).abs() < 0.001,
            "At wet=0.0 output should be dry: got {sample}, expected {}",
            input[i]
        );
    }
}

#[test]
fn test_effect_chain_on_mixer() {
    let mixer = crate::mixer::AudioMixer::new(44100, 2);

    let lpf = BiquadFilter::new(FilterKind::LowPass, 500.0, 44100.0);
    mixer.add_master_effect(Box::new(lpf));

    let buf = Arc::new(make_test_buffer(44100, 2, vec![1.0; 2048]));
    mixer.play(buf, AudioCategoryValue::Sfx, VoicePriority::default());

    let mut fade_buf = vec![0.0f32; 512];
    mixer.render(&mut fade_buf);

    let mut output = vec![0.0f32; 128];
    mixer.render(&mut output);

    let energy: f32 = output.iter().map(|s| s * s).sum();
    assert!(
        energy > 0.0,
        "Mixer with LPF should still produce output: energy={energy}"
    );
    assert!(
        output.iter().all(|s| s.is_finite()),
        "All output samples should be finite"
    );
}

#[test]
fn test_aux_bus_send_return() {
    let mixer = crate::mixer::AudioMixer::new(44100, 2);

    let mut bus = AuxBus::new(1.0, 1.0);
    bus.add_effect(Box::new(ReverbEffect::new(44100)));
    mixer.add_aux_bus(bus);

    let buf = Arc::new(make_test_buffer(44100, 2, vec![1.0; 8192]));
    mixer.play(buf, AudioCategoryValue::Sfx, VoicePriority::default());

    let mut fade_buf = vec![0.0f32; 512];
    mixer.render(&mut fade_buf);

    let mut output_with_bus = vec![0.0f32; 512];
    mixer.render(&mut output_with_bus);

    let energy_with_bus: f32 = output_with_bus.iter().map(|s| s * s).sum();

    let mixer_no_bus = crate::mixer::AudioMixer::new(44100, 2);
    let buf2 = Arc::new(make_test_buffer(44100, 2, vec![1.0; 8192]));
    mixer_no_bus.play(buf2, AudioCategoryValue::Sfx, VoicePriority::default());
    let mut fade_buf2 = vec![0.0f32; 512];
    mixer_no_bus.render(&mut fade_buf2);
    let mut output_no_bus = vec![0.0f32; 512];
    mixer_no_bus.render(&mut output_no_bus);

    let energy_no_bus: f32 = output_no_bus.iter().map(|s| s * s).sum();

    assert!(
        energy_with_bus > energy_no_bus,
        "Aux bus with reverb should add energy: with={energy_with_bus}, without={energy_no_bus}"
    );
}

#[test]
fn test_aux_bus_zero_send() {
    let mixer = crate::mixer::AudioMixer::new(44100, 2);

    let bus = AuxBus::new(0.0, 1.0);
    mixer.add_aux_bus(bus);

    let buf = Arc::new(make_test_buffer(44100, 2, vec![1.0; 2048]));
    mixer.play(buf, AudioCategoryValue::Sfx, VoicePriority::default());

    let mut fade_buf = vec![0.0f32; 512];
    mixer.render(&mut fade_buf);

    let mut output = vec![0.0f32; 128];
    mixer.render(&mut output);

    let (left_gain, _right_gain) = crate::voice::compute_pan_gains(0.0);
    let expected = 1.0 * left_gain;
    for sample in &output {
        assert!(
            (*sample - expected).abs() < 0.01,
            "Zero send should not affect output: got {sample}, expected {expected:.3}"
        );
    }
}

#[test]
fn test_volume_tween_converges() {
    let mixer = crate::mixer::AudioMixer::new(44100, 2);

    let buf = Arc::new(make_test_buffer(44100, 2, vec![1.0; 8192]));
    let id = mixer.play(buf, AudioCategoryValue::Sfx, VoicePriority::default());

    mixer.set_voice_volume_tweened(id, 0.0);

    for _ in 0..20 {
        let mut output = vec![0.0f32; 64];
        mixer.render(&mut output);
    }

    let mut output = vec![0.0f32; 64];
    mixer.render(&mut output);

    for sample in &output {
        assert!(
            *sample < 0.01,
            "Volume should converge toward 0 after tweening, got {sample}"
        );
    }
}

#[test]
fn test_pan_tween_converges() {
    let mixer = crate::mixer::AudioMixer::new(44100, 2);

    let buf = Arc::new(make_test_buffer(44100, 2, vec![1.0; 16384]));
    let id = mixer.play(buf, AudioCategoryValue::Sfx, VoicePriority::default());

    mixer.set_voice_pan_tweened(id, 1.0);

    for _ in 0..20 {
        let mut output = vec![0.0f32; 64];
        mixer.render(&mut output);
    }

    let mut output = vec![0.0f32; 64];
    mixer.render(&mut output);

    let left_avg: f32 = output.iter().step_by(2).sum::<f32>() / 32.0;
    let right_avg: f32 = output.iter().skip(1).step_by(2).sum::<f32>() / 32.0;

    assert!(
        right_avg > left_avg,
        "After panning right, right should be louder: left={left_avg}, right={right_avg}"
    );
}

#[test]
fn test_sound_cue_plays() {
    let engine = match AudioEngine::new() {
        Ok(e) => e,
        Err(_) => {
            eprintln!("Skipping sound cue test: no audio device available");
            return;
        }
    };

    let buf1 = Arc::new(make_test_buffer(44100, 2, vec![0.5; 1024]));
    let buf2 = Arc::new(make_test_buffer(44100, 2, vec![0.3; 1024]));

    let mut cue = crate::sound_cue::SoundCue::new(crate::engine::AudioCategory::Sfx)
        .with_buffer(buf1)
        .with_buffer(buf2)
        .with_play_mode(crate::sound_cue::CuePlayMode::Random);

    let handle = cue.play(&engine).unwrap();
    assert_eq!(handle.state(), VoiceState::Playing);
    handle.stop();
}

#[test]
fn test_sound_cue_sequential() {
    let engine = match AudioEngine::new() {
        Ok(e) => e,
        Err(_) => {
            eprintln!("Skipping sound cue test: no audio device available");
            return;
        }
    };

    let buf1 = Arc::new(make_test_buffer(44100, 2, vec![0.5; 1024]));
    let buf2 = Arc::new(make_test_buffer(44100, 2, vec![0.3; 1024]));

    let mut cue = crate::sound_cue::SoundCue::new(crate::engine::AudioCategory::Sfx)
        .with_buffers(vec![buf1, buf2])
        .with_play_mode(crate::sound_cue::CuePlayMode::Sequential);

    let h1 = cue.play(&engine).unwrap();
    let h2 = cue.play(&engine).unwrap();
    h1.stop();
    h2.stop();

    assert_ne!(
        h1.id, h2.id,
        "Sequential plays should produce different voices"
    );
}

#[test]
fn test_sound_cue_pitch_variation() {
    let engine = match AudioEngine::new() {
        Ok(e) => e,
        Err(_) => {
            eprintln!("Skipping sound cue test: no audio device available");
            return;
        }
    };

    let buf = Arc::new(make_test_buffer(44100, 2, vec![0.5; 4096]));

    let mut cue = crate::sound_cue::SoundCue::new(crate::engine::AudioCategory::Sfx)
        .with_buffer(buf)
        .with_pitch_variation(6.0);

    for _ in 0..5 {
        if let Some(handle) = cue.play(&engine) {
            let vol = handle.volume();
            assert!(vol > 0.0 && vol <= 1.0, "Volume should be in range: {vol}");
            handle.stop();
        }
    }
}

#[test]
fn test_category_volume_change_affects_playing_voice() {
    let mixer = crate::mixer::AudioMixer::new(44100, 2);

    let buf = Arc::new(make_test_buffer(44100, 2, vec![1.0; 8192]));
    mixer.play(buf, AudioCategoryValue::Sfx, VoicePriority::default());

    let mut fade_buf = vec![0.0f32; 512];
    mixer.render(&mut fade_buf);

    let mut output_full = vec![0.0f32; 64];
    mixer.render(&mut output_full);
    let energy_full: f32 = output_full.iter().map(|s| s * s).sum();

    let buf2 = Arc::new(make_test_buffer(44100, 2, vec![1.0; 8192]));
    mixer.play(buf2, AudioCategoryValue::Sfx, VoicePriority::default());
    mixer.set_category_volume(AudioCategoryValue::Sfx, 0.25);

    let mut output_quiet = vec![0.0f32; 64];
    mixer.render(&mut output_quiet);
    let energy_quiet: f32 = output_quiet.iter().map(|s| s * s).sum();

    assert!(
        energy_quiet < energy_full * 0.3,
        "Category volume change should reduce output energy: full={energy_full}, quiet={energy_quiet}"
    );
}

#[test]
fn test_voice_stealing_higher_priority_takes_slot() {
    let mixer = crate::mixer::AudioMixer::with_pool_size(44100, 2, 1, 1);

    let buf = Arc::new(make_test_buffer(44100, 2, vec![0.5; 8192]));
    let low_id = mixer.play(buf.clone(), AudioCategoryValue::Sfx, VoicePriority::Low);

    let mut fade_buf = vec![0.0f32; 512];
    mixer.render(&mut fade_buf);

    assert_eq!(mixer.active_voice_count(), 1);
    assert_eq!(mixer.voice_state(low_id), VoiceState::Playing);

    let high_id = mixer.play(buf, AudioCategoryValue::Sfx, VoicePriority::High);

    let mut fade_buf2 = vec![0.0f32; 512];
    mixer.render(&mut fade_buf2);

    assert_eq!(mixer.active_voice_count(), 1);
    assert_eq!(mixer.voice_state(high_id), VoiceState::Playing);
}

#[test]
fn test_voice_stealing_same_priority_rejected() {
    let mixer = crate::mixer::AudioMixer::with_pool_size(44100, 2, 1, 1);

    let buf = Arc::new(make_test_buffer(44100, 2, vec![0.5; 8192]));
    let id1 = mixer.play(buf.clone(), AudioCategoryValue::Sfx, VoicePriority::Low);

    let mut fade_buf = vec![0.0f32; 512];
    mixer.render(&mut fade_buf);

    assert_eq!(mixer.active_voice_count(), 1);

    let id2 = mixer.play(buf, AudioCategoryValue::Sfx, VoicePriority::Low);

    let mut fade_buf2 = vec![0.0f32; 512];
    mixer.render(&mut fade_buf2);

    assert_eq!(mixer.active_voice_count(), 1);
    assert_eq!(mixer.voice_state(id1), VoiceState::Playing);
    assert_eq!(mixer.voice_state(id2), VoiceState::Stopped);
}

#[test]
fn test_voice_stealing_lower_priority_cannot_steal() {
    let mixer = crate::mixer::AudioMixer::with_pool_size(44100, 2, 1, 1);

    let buf = Arc::new(make_test_buffer(44100, 2, vec![0.5; 8192]));
    let high_id = mixer.play(buf.clone(), AudioCategoryValue::Sfx, VoicePriority::High);

    let mut fade_buf = vec![0.0f32; 512];
    mixer.render(&mut fade_buf);

    assert_eq!(mixer.active_voice_count(), 1);

    let low_id = mixer.play(buf, AudioCategoryValue::Sfx, VoicePriority::Low);

    let mut fade_buf2 = vec![0.0f32; 512];
    mixer.render(&mut fade_buf2);

    assert_eq!(mixer.active_voice_count(), 1);
    assert_eq!(mixer.voice_state(high_id), VoiceState::Playing);
    assert_eq!(mixer.voice_state(low_id), VoiceState::Stopped);
}

#[test]
fn test_voice_stealing_prefers_lowest_priority() {
    let mixer = crate::mixer::AudioMixer::with_pool_size(44100, 2, 2, 1);

    let buf = Arc::new(make_test_buffer(44100, 2, vec![0.5; 8192]));
    let _low_id = mixer.play(buf.clone(), AudioCategoryValue::Sfx, VoicePriority::Low);

    let mut fade_buf = vec![0.0f32; 512];
    mixer.render(&mut fade_buf);

    let med_id = mixer.play(buf.clone(), AudioCategoryValue::Sfx, VoicePriority::Medium);

    let mut fade_buf2 = vec![0.0f32; 512];
    mixer.render(&mut fade_buf2);

    assert_eq!(mixer.active_voice_count(), 2);

    let high_id = mixer.play(buf, AudioCategoryValue::Sfx, VoicePriority::High);

    let mut fade_buf3 = vec![0.0f32; 512];
    mixer.render(&mut fade_buf3);

    assert_eq!(mixer.active_voice_count(), 2);
    assert_eq!(mixer.voice_state(med_id), VoiceState::Playing);
    assert_eq!(mixer.voice_state(high_id), VoiceState::Playing);
}

#[test]
fn test_voice_stealing_fade_in_prevents_click() {
    let mixer = crate::mixer::AudioMixer::with_pool_size(44100, 2, 1, 1);

    let buf = Arc::new(make_test_buffer(44100, 2, vec![1.0; 8192]));
    let _low_id = mixer.play(buf.clone(), AudioCategoryValue::Sfx, VoicePriority::Low);

    let mut fade_buf = vec![0.0f32; 512];
    mixer.render(&mut fade_buf);

    let _high_id = mixer.play(buf, AudioCategoryValue::Sfx, VoicePriority::High);

    let mut first_frame = vec![0.0f32; 2];
    mixer.render(&mut first_frame);

    assert!(
        first_frame[0] < 0.5,
        "Stolen voice should fade in, first sample should be attenuated: got {}",
        first_frame[0]
    );
}
