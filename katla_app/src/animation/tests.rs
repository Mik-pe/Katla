use crate::animation::clips::{
    AnimationChannel, AnimationClip, AnimationSampler, ChannelPath, SampleBuffer,
};
use crate::animation::components::{AnimationPlayer, JointTransform};
use crate::animation::samplers::Interpolation;
use katla_math::Quat;

// Tests for actual behavior - seek/crossfade
#[test]
fn test_animation_player_seek_clamps() {
    let mut player = AnimationPlayer::new("Test").with_duration(10.0);

    player.seek(-5.0);
    assert_eq!(player.time, 0.0);

    player.seek(20.0);
    assert_eq!(player.time, 10.0);
}

#[test]
fn test_animation_player_set_clip() {
    let mut player = AnimationPlayer::stopped();
    player.loop_count = 5;

    player.set_clip("Run", 3.5);

    assert_eq!(player.current_clip.as_ref().unwrap(), "Run");
    assert_eq!(player.duration, 3.5);
    assert_eq!(player.time, 0.0);
    assert_eq!(player.loop_count, 0);
    assert!(!player.blending);
}

#[test]
fn test_animation_player_crossfade() {
    let mut player = AnimationPlayer::new("Walk").with_duration(5.0);

    player.crossfade_to("Run", 3.0, 0.5).unwrap();

    assert!(player.blending);
    assert_eq!(player.target_clip.as_ref().unwrap(), "Run");
    assert_eq!(player.target_duration, 3.0);
    assert_eq!(player.blend_duration, 0.5);
    assert_eq!(player.blend_time, 0.0);
}

#[test]
fn test_animation_crossfade_zero_duration() {
    let mut player = AnimationPlayer::new("Walk").with_duration(5.0);

    player.crossfade_to("Run", 3.0, 0.0).unwrap();

    assert!(!player.blending);
    assert_eq!(player.current_clip.as_deref(), Some("Run"));
    assert_eq!(player.blend_duration, 0.0);
    assert_eq!(player.blend_time, 0.0);
    assert_eq!(player.blend_weight, 1.0); // Should still be valid, no NaN

    // The blend_weight should remain a valid f32 (not NaN)
    assert!(!player.blend_weight.is_nan());
}

// Tests for lerp/blending behavior (actual interpolation logic)
#[test]
fn test_joint_transform_lerp_translation() {
    let start = JointTransform::from_translation([0.0, 0.0, 0.0]);
    let end = JointTransform::from_translation([10.0, 20.0, 30.0]);

    let result = start.lerp(&end, 0.5);

    assert_eq!(result.translation, [5.0, 10.0, 15.0]);
}

#[test]
fn test_joint_transform_lerp_scale() {
    let start = JointTransform {
        translation: [0.0, 0.0, 0.0],
        rotation: Quat::identity(),
        scale: [1.0, 1.0, 1.0],
    };
    let end = JointTransform {
        translation: [0.0, 0.0, 0.0],
        rotation: Quat::identity(),
        scale: [2.0, 2.0, 2.0],
    };

    let result = start.lerp(&end, 0.5);

    assert_eq!(result.scale, [1.5, 1.5, 1.5]);
}

#[test]
fn test_joint_transform_lerp_rotation() {
    let start = JointTransform {
        translation: [0.0, 0.0, 0.0],
        rotation: Quat::identity(),
        scale: [1.0, 1.0, 1.0],
    };

    let end_quat = Quat::new(0.0, 0.0, 0.707, 0.707);
    let end = JointTransform {
        translation: [0.0, 0.0, 0.0],
        rotation: end_quat,
        scale: [1.0, 1.0, 1.0],
    };

    let result = start.lerp(&end, 0.5);

    assert!(result.rotation[3] > 0.85);
}

#[test]
fn test_joint_transform_blend() {
    let a = JointTransform::from_translation([0.0, 0.0, 0.0]);
    let b = JointTransform::from_translation([10.0, 10.0, 10.0]);

    let result = JointTransform::blend(&a, &b, 0.25, 0.75);

    assert!(result.translation[0] > 5.0 && result.translation[0] < 10.0);
}

// Tests for animation sampler duration calculations (actual logic)
#[test]
fn test_animation_sampler_duration() {
    let inputs = vec![0.0, 0.5, 1.0, 1.5, 2.0];
    let sampler =
        AnimationSampler::new_translation(inputs, vec![[0.0; 3]; 5], Interpolation::Linear);

    assert_eq!(sampler.duration(), 2.0);
}

// Tests for sample buffer behavior (actual functionality)
#[test]
fn test_animation_clip_sample_into() {
    let sampler = AnimationSampler::new_translation(
        vec![0.0, 1.0],
        vec![[0.0, 0.0, 0.0], [10.0, 10.0, 10.0]],
        Interpolation::Linear,
    );

    let channel = AnimationChannel {
        target_node: 0,
        path: ChannelPath::Translation,
        sampler,
    };

    let clip = AnimationClip {
        name: "Test".to_string(),
        duration: 1.0,
        channels: vec![channel],
    };

    let mut buffer = SampleBuffer::new();
    clip.sample_into(0.5, &mut buffer);

    assert_eq!(buffer.samples().len(), 1);
}

#[test]
fn test_sampler_clamps_to_channel_interval() {
    for interpolation in [
        Interpolation::Linear,
        Interpolation::Step,
        Interpolation::CubicSpline,
    ] {
        let values = if interpolation == Interpolation::CubicSpline {
            vec![
                [90.0; 3], [2.0; 3], [0.0; 3], [0.0; 3], [12.0; 3], [80.0; 3],
            ]
        } else {
            vec![[2.0; 3], [12.0; 3]]
        };
        let sampler = AnimationSampler::new_translation(vec![0.5, 1.0], values, interpolation);
        for (time, expected) in [(-1.0, 2.0), (0.0, 2.0), (1.0, 12.0), (2.0, 12.0)] {
            let crate::animation::SampledValue::Vec3(value) = sampler.sample(time) else {
                panic!("expected translation")
            };
            assert_eq!(value, [expected; 3], "{interpolation:?} at {time}");
        }
    }
}

#[test]
fn test_cubic_single_keyframe_returns_value_slot() {
    let sampler = AnimationSampler::new_scale(
        vec![0.5],
        vec![[90.0; 3], [2.0; 3], [80.0; 3]],
        Interpolation::CubicSpline,
    );
    for time in [-1.0, 0.5, 2.0] {
        let crate::animation::SampledValue::Vec3(value) = sampler.sample(time) else {
            panic!("expected scale")
        };
        assert_eq!(value, [2.0; 3]);
    }
    let sampler =
        AnimationSampler::new_weights(vec![0.5], vec![90.0, 2.0, 80.0], Interpolation::CubicSpline);
    let crate::animation::SampledValue::Float(value) = sampler.sample(2.0) else {
        panic!("expected weight")
    };
    assert_eq!(value, 2.0);
}

#[test]
fn test_cubic_rotation_is_normalized() {
    let sampler = AnimationSampler::new_rotation(
        vec![0.0, 1.0],
        vec![
            [0.0; 4],
            [0.0, 0.0, 0.0, 1.0],
            [0.0; 4],
            [0.0; 4],
            [0.0, 0.0, 1.0, 0.0],
            [0.0; 4],
        ],
        Interpolation::CubicSpline,
    );
    for time in [0.0, 0.25, 0.5, 0.75, 1.0, 2.0] {
        let crate::animation::SampledValue::Quat(value) = sampler.sample(time) else {
            panic!("expected rotation")
        };
        let length = value.iter().map(|v| v * v).sum::<f32>().sqrt();
        assert!((length - 1.0).abs() < 1e-5, "at {time}: {value:?}");
        if time >= 1.0 {
            assert_eq!(value, [0.0, 0.0, 1.0, 0.0]);
        }
    }
}

#[test]
fn test_cached_sampler_matches_seek_and_loop_samples() {
    let sampler = AnimationSampler::new_translation(
        vec![0.5, 1.0, 2.0],
        vec![[2.0; 3], [12.0; 3], [22.0; 3]],
        Interpolation::Linear,
    );
    let mut cached = crate::animation::samplers::CachedSampler::new(&sampler);
    for time in [0.0, 0.75, 1.5, 2.5, 0.0, 1.0, 0.5] {
        let crate::animation::SampledValue::Vec3(value) = cached.sample(time) else {
            panic!("expected translation")
        };
        let crate::animation::SampledValue::Vec3(expected) = sampler.sample(time) else {
            panic!("expected translation")
        };
        assert_eq!(value, expected);
    }
}
