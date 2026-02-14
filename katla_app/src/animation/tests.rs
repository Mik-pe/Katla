#[cfg(test)]
mod tests {
    use crate::animation::clips::{
        AnimationChannel, AnimationClip, AnimationSampler, ChannelPath, SampleBuffer,
    };
    use crate::animation::components::{
        AnimationEvent, AnimationPlayer, JointTransform, MorphTargetWeights,
    };
    use crate::animation::samplers::Interpolation;
    use katla_math::Quat;

    // Tests for actual behavior - play/pause/stop/seek/crossfade
    #[test]
    fn test_animation_player_play_pause() {
        let mut player = AnimationPlayer::stopped();

        assert!(!player.playing);
        player.play();
        assert!(player.playing);
        player.pause();
        assert!(!player.playing);
    }

    #[test]
    fn test_animation_player_stop() {
        let mut player = AnimationPlayer::new("Test");
        player.time = 5.0;
        player.loop_count = 3;
        player.playing = true;

        player.stop();

        assert_eq!(player.time, 0.0);
        assert!(!player.playing);
        assert_eq!(player.loop_count, 0);
    }

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

        player.crossfade_to("Run", 3.0, 0.5);

        assert!(player.blending);
        assert_eq!(player.target_clip.as_ref().unwrap(), "Run");
        assert_eq!(player.target_duration, 3.0);
        assert_eq!(player.blend_duration, 0.5);
        assert_eq!(player.blend_time, 0.0);
    }

    #[test]
    fn test_animation_player_take_events() {
        let mut player = AnimationPlayer::new("Test");
        player.events.push(AnimationEvent::Completed {
            clip_name: "Test".to_string(),
        });
        player.events.push(AnimationEvent::Looped {
            clip_name: "Test".to_string(),
            loop_count: 1,
        });

        let events = player.take_events();

        assert_eq!(events.len(), 2);
        assert!(player.events.is_empty());
    }

    #[test]
    fn test_animation_player_is_complete() {
        let mut player = AnimationPlayer::new("Test").with_duration(5.0);
        player.loop_animation = false;

        assert!(!player.is_complete());

        player.time = 5.0;
        player.playing = false;

        assert!(player.is_complete());
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
            rotation: Quat::new(),
            scale: [1.0, 1.0, 1.0],
        };
        let end = JointTransform {
            translation: [0.0, 0.0, 0.0],
            rotation: Quat::new(),
            scale: [2.0, 2.0, 2.0],
        };

        let result = start.lerp(&end, 0.5);

        assert_eq!(result.scale, [1.5, 1.5, 1.5]);
    }

    #[test]
    fn test_joint_transform_lerp_rotation() {
        let start = JointTransform {
            translation: [0.0, 0.0, 0.0],
            rotation: Quat::new(),
            scale: [1.0, 1.0, 1.0],
        };

        let end_quat = Quat::new_from_xyzw(0.0, 0.0, 0.707, 0.707);
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

    // Tests for morph target weights (actual behavior: clamping, bounds checking)
    #[test]
    fn test_morph_target_weights_set() {
        let mut weights = MorphTargetWeights::new(3);

        weights.set_weight(0, 0.5);
        weights.set_weight(1, 1.0);
        weights.set_weight(2, 0.75);

        assert_eq!(weights.get_weight(0), 0.5);
        assert_eq!(weights.get_weight(1), 1.0);
        assert_eq!(weights.get_weight(2), 0.75);
    }

    #[test]
    fn test_morph_target_weights_set_clamps() {
        let mut weights = MorphTargetWeights::new(2);

        weights.set_weight(0, 1.5);
        weights.set_weight(1, -0.5);

        assert_eq!(weights.get_weight(0), 1.0);
        assert_eq!(weights.get_weight(1), 0.0);
    }

    #[test]
    fn test_morph_target_weights_set_out_of_bounds() {
        let mut weights = MorphTargetWeights::new(2);

        weights.set_weight(5, 0.5);

        assert_eq!(weights.get_weight(5), 0.0);
    }

    // Tests for animation sampler duration calculations (actual logic)
    #[test]
    fn test_animation_sampler_keyframe_count() {
        let inputs = vec![0.0, 0.25, 0.5, 0.75, 1.0];
        let sampler =
            AnimationSampler::new_translation(inputs, vec![[0.0; 3]; 5], Interpolation::Linear);

        assert_eq!(sampler.keyframe_count(), 5);
    }

    #[test]
    fn test_animation_sampler_duration() {
        let inputs = vec![0.0, 0.5, 1.0, 1.5, 2.0];
        let sampler =
            AnimationSampler::new_translation(inputs, vec![[0.0; 3]; 5], Interpolation::Linear);

        assert_eq!(sampler.duration(), 2.0);
    }

    #[test]
    fn test_animation_sampler_empty_duration() {
        let sampler = AnimationSampler::new_translation(vec![], vec![], Interpolation::Linear);

        assert_eq!(sampler.duration(), 0.0);
    }

    // Tests for sample buffer behavior (actual functionality)
    #[test]
    fn test_sample_buffer_clear() {
        use crate::animation::clips::AnimationSampler;

        let sampler = AnimationSampler::new_translation(
            vec![0.0, 1.0],
            vec![[0.0, 0.0, 0.0], [1.0, 1.0, 1.0]],
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
        assert!(!buffer.samples().is_empty());

        buffer.clear();

        assert!(buffer.samples().is_empty());
    }

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

    // Tests for interpolation conversion to/from GLTF (actual serialization logic)
    #[test]
    fn test_interpolation_to_gltf() {
        assert_eq!(Interpolation::Linear.to_gltf(), "LINEAR");
        assert_eq!(Interpolation::Step.to_gltf(), "STEP");
        assert_eq!(Interpolation::CubicSpline.to_gltf(), "CUBICSPLINE");
    }

    #[test]
    fn test_interpolation_from_gltf() {
        assert!(matches!(
            Interpolation::from_gltf("LINEAR"),
            Interpolation::Linear
        ));
        assert!(matches!(
            Interpolation::from_gltf("STEP"),
            Interpolation::Step
        ));
        assert!(matches!(
            Interpolation::from_gltf("CUBICSPLINE"),
            Interpolation::CubicSpline
        ));
        assert!(matches!(
            Interpolation::from_gltf("UNKNOWN"),
            Interpolation::Linear
        ));
    }
}
