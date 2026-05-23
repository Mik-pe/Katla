use crate::buffer::{AudioBuffer, DecodedAudio, SampleFormat, load_wav};
use crate::engine::AudioEngine;
use crate::voice::VoiceState;
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

    let buf1 = Arc::new(make_test_buffer(44100, 2, vec![0.5; 256]));
    let buf2 = Arc::new(make_test_buffer(44100, 2, vec![0.3; 256]));

    mixer.play(buf1);
    mixer.play(buf2);

    let mut output = vec![0.0f32; 128];
    mixer.render(&mut output);

    for sample in &output {
        assert!((*sample - 0.8).abs() < 0.001, "Expected ~0.8, got {sample}");
    }
}

#[test]
fn test_mixer_master_volume() {
    let mixer = crate::mixer::AudioMixer::new(44100, 2);

    let buf = Arc::new(make_test_buffer(44100, 2, vec![1.0; 256]));
    mixer.play(buf);
    mixer.set_master_volume(0.5);

    let mut output = vec![0.0f32; 64];
    mixer.render(&mut output);

    for sample in &output {
        assert!((*sample - 0.5).abs() < 0.001, "Expected ~0.5, got {sample}");
    }
}

#[test]
fn test_mixer_clamping() {
    let mixer = crate::mixer::AudioMixer::new(44100, 2);

    let buf = Arc::new(make_test_buffer(44100, 2, vec![0.9; 256]));
    mixer.play(buf.clone());
    mixer.play(buf);
    mixer.set_master_volume(1.0);

    let mut output = vec![0.0f32; 64];
    mixer.render(&mut output);

    for sample in &output {
        assert!(*sample <= 1.0, "Expected clamped to 1.0, got {sample}");
    }
}

#[test]
fn test_voice_stop() {
    let mixer = crate::mixer::AudioMixer::new(44100, 2);

    let buf = Arc::new(make_test_buffer(44100, 2, vec![1.0; 512]));
    let id = mixer.play(buf);

    assert_eq!(mixer.active_voice_count(), 1);
    mixer.stop(id);

    let mut output = vec![0.0f32; 64];
    mixer.render(&mut output);

    for sample in &output {
        assert!(
            sample.abs() < 0.001,
            "Expected silence after stop, got {sample}"
        );
    }
    assert_eq!(mixer.active_voice_count(), 0);
}

#[test]
fn test_voice_finished_cleanup() {
    let mixer = crate::mixer::AudioMixer::new(44100, 2);

    let buf = Arc::new(make_test_buffer(44100, 2, vec![0.5; 64]));
    mixer.play(buf);

    let mut output = vec![0.0f32; 64];
    mixer.render(&mut output);
    assert_eq!(mixer.active_voice_count(), 0);
}

#[test]
fn test_voice_set_volume() {
    let mixer = crate::mixer::AudioMixer::new(44100, 2);

    let buf = Arc::new(make_test_buffer(44100, 2, vec![1.0; 256]));
    let id = mixer.play(buf);
    mixer.set_voice_volume(id, 0.25);

    let mut output = vec![0.0f32; 64];
    mixer.render(&mut output);

    for sample in &output {
        assert!(
            (*sample - 0.25).abs() < 0.001,
            "Expected ~0.25, got {sample}"
        );
    }
}

#[test]
fn test_mono_to_stereo_upmix() {
    let mixer = crate::mixer::AudioMixer::new(44100, 2);

    let buf = Arc::new(make_test_buffer(44100, 1, vec![0.7; 128]));
    mixer.play(buf);

    let mut output = vec![0.0f32; 64];
    mixer.render(&mut output);

    for frame in 0..32 {
        let left = output[frame * 2];
        let right = output[frame * 2 + 1];
        assert!((left - 0.7).abs() < 0.001, "Expected left ~0.7, got {left}");
        assert!(
            (right - 0.7).abs() < 0.001,
            "Expected right ~0.7, got {right}"
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

    let buf = Arc::new(make_test_buffer(44100, 2, vec![1.0; 256]));
    let id = mixer.play(buf);
    mixer.set_voice_pan(id, 1.0); // pan fully right

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

    let buf = Arc::new(make_test_buffer(44100, 1, vec![1.0; 128]));
    let id = mixer.play(buf);
    mixer.set_voice_pan(id, -1.0); // pan fully left

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
fn test_pan_gains_center() {
    let (l, r) = crate::voice::compute_pan_gains(0.0);
    assert!(
        (l - 1.0).abs() < 0.001,
        "Center pan left gain should be 1.0"
    );
    assert!(
        (r - 1.0).abs() < 0.001,
        "Center pan right gain should be 1.0"
    );
}

#[test]
fn test_pan_gains_symmetry() {
    let (l_r, r_r) = crate::voice::compute_pan_gains(1.0);
    let (l_l, r_l) = crate::voice::compute_pan_gains(-1.0);
    assert!((l_r - r_l).abs() < 0.001, "Pan gains should be symmetric");
    assert!((r_r - l_l).abs() < 0.001, "Pan gains should be symmetric");
}
