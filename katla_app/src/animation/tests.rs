#[cfg(test)]
mod tests {
    use crate::animation::clips::{
        AnimationChannel, AnimationClip, AnimationSampler, ChannelPath, SampleBuffer, SampledValue,
    };
    use crate::animation::components::{
        AnimationEvent, AnimationPlayer, JointTransform, MorphTargetWeights,
    };
    use crate::animation::samplers::Interpolation;
    use katla_math::Quat;

    #[test]
    fn test_animation_player_new() {
        let player = AnimationPlayer::new("Walk");

        assert!(player.current_clip.is_some());
        assert_eq!(player.current_clip.as_ref().unwrap(), "Walk");
        assert_eq!(player.time, 0.0);
        assert!(player.playing);
        assert!(!player.loop_animation);
        assert_eq!(player.speed, 1.0);
        assert_eq!(player.blend_weight, 1.0);
        assert!(!player.blending);
        assert!(player.target_clip.is_none());
        assert!(player.events.is_empty());
        assert_eq!(player.loop_count, 0);
    }

    #[test]
    fn test_animation_player_stopped() {
        let player = AnimationPlayer::stopped();

        assert!(player.current_clip.is_none());
        assert_eq!(player.time, 0.0);
        assert!(!player.playing);
        assert!(!player.loop_animation);
        assert_eq!(player.speed, 1.0);
        assert_eq!(player.blend_weight, 1.0);
    }

    #[test]
    fn test_animation_player_builder_pattern() {
        let player = AnimationPlayer::new("Run").looping().with_speed(2.0);

        assert!(player.loop_animation);
        assert_eq!(player.speed, 2.0);
    }

    #[test]
    fn test_animation_player_with_duration() {
        let player = AnimationPlayer::new("Walk").with_duration(5.0);

        assert_eq!(player.duration, 5.0);
    }

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
    fn test_animation_player_seek() {
        let mut player = AnimationPlayer::new("Test").with_duration(10.0);

        player.seek(5.0);
        assert_eq!(player.time, 5.0);
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
    fn test_animation_player_with_clip() {
        let player = AnimationPlayer::stopped().with_clip("Jump");

        assert_eq!(player.current_clip.as_ref().unwrap(), "Jump");
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

    #[test]
    fn test_animation_event_equality() {
        let event1 = AnimationEvent::Completed {
            clip_name: "Walk".to_string(),
        };
        let event2 = AnimationEvent::Completed {
            clip_name: "Walk".to_string(),
        };
        let event3 = AnimationEvent::Completed {
            clip_name: "Run".to_string(),
        };

        assert_eq!(event1, event2);
        assert_ne!(event1, event3);
    }

    #[test]
    fn test_joint_transform_identity() {
        let identity = JointTransform::identity();

        assert_eq!(identity.translation, [0.0, 0.0, 0.0]);
        assert_eq!(identity.scale, [1.0, 1.0, 1.0]);
        assert_eq!(identity.rotation[3], 1.0);
    }

    #[test]
    fn test_joint_transform_from_translation() {
        let transform = JointTransform::from_translation([1.0, 2.0, 3.0]);

        assert_eq!(transform.translation, [1.0, 2.0, 3.0]);
        assert_eq!(transform.scale, [1.0, 1.0, 1.0]);
        assert_eq!(transform.rotation[3], 1.0);
    }

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
    fn test_joint_transform_lerp_arrays() {
        let a = JointTransform::from_translation([0.0, 0.0, 0.0]);
        let b = JointTransform::from_translation([10.0, 20.0, 30.0]);

        let result = JointTransform::lerp_arrays(&a, &b, 0.5);

        assert_eq!(result.translation, [5.0, 10.0, 15.0]);
    }

    #[test]
    fn test_joint_transform_blend() {
        let a = JointTransform::from_translation([0.0, 0.0, 0.0]);
        let b = JointTransform::from_translation([10.0, 10.0, 10.0]);

        let result = JointTransform::blend(&a, &b, 0.25, 0.75);

        assert!(result.translation[0] > 5.0 && result.translation[0] < 10.0);
    }

    #[test]
    fn test_joint_transform_default() {
        let default = JointTransform::default();
        let identity = JointTransform::identity();

        assert_eq!(default.translation, identity.translation);
        assert_eq!(default.scale, identity.scale);
    }

    #[test]
    fn test_morph_target_weights_new() {
        let weights = MorphTargetWeights::new(5);

        assert_eq!(weights.weights.len(), 5);
        assert!(weights.weights.iter().all(|&w| w == 0.0));
    }

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

    #[test]
    fn test_animation_sampler_translation() {
        let inputs = vec![0.0, 0.5, 1.0];
        let translations = vec![[0.0, 0.0, 0.0], [5.0, 5.0, 5.0], [10.0, 10.0, 10.0]];

        let sampler =
            AnimationSampler::new_translation(inputs.clone(), translations, Interpolation::Linear);

        assert_eq!(sampler.inputs, inputs);
        assert!(sampler.translations.is_some());
        assert!(sampler.rotations.is_none());
        assert!(sampler.scales.is_none());
        assert!(sampler.weights.is_none());
        assert_eq!(sampler.interpolation, Interpolation::Linear);
    }

    #[test]
    fn test_animation_sampler_rotation() {
        let inputs = vec![0.0, 1.0];
        let rotations = vec![[0.0, 0.0, 0.0, 1.0], [0.0, 0.0, 0.707, 0.707]];

        let sampler = AnimationSampler::new_rotation(inputs, rotations, Interpolation::Linear);

        assert!(sampler.translations.is_none());
        assert!(sampler.rotations.is_some());
        assert!(sampler.scales.is_none());
        assert!(sampler.weights.is_none());
    }

    #[test]
    fn test_animation_sampler_scale() {
        let inputs = vec![0.0, 1.0];
        let scales = vec![[1.0, 1.0, 1.0], [2.0, 2.0, 2.0]];

        let sampler = AnimationSampler::new_scale(inputs, scales, Interpolation::Linear);

        assert!(sampler.translations.is_none());
        assert!(sampler.rotations.is_none());
        assert!(sampler.scales.is_some());
        assert!(sampler.weights.is_none());
    }

    #[test]
    fn test_animation_sampler_weights() {
        let inputs = vec![0.0, 0.5, 1.0];
        let weights = vec![0.0, 0.5, 1.0];

        let sampler = AnimationSampler::new_weights(inputs, weights, Interpolation::Linear);

        assert!(sampler.translations.is_none());
        assert!(sampler.rotations.is_none());
        assert!(sampler.scales.is_none());
        assert!(sampler.weights.is_some());
    }

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

    #[test]
    fn test_animation_channel() {
        let sampler = AnimationSampler::new_translation(
            vec![0.0, 1.0],
            vec![[0.0, 0.0, 0.0], [1.0, 2.0, 3.0]],
            Interpolation::Linear,
        );

        let channel = AnimationChannel {
            target_node: 5,
            path: ChannelPath::Translation,
            sampler,
        };

        assert_eq!(channel.target_node, 5);
        assert_eq!(channel.path, ChannelPath::Translation);
    }

    #[test]
    fn test_animation_clip() {
        let sampler = AnimationSampler::new_translation(
            vec![0.0, 0.5, 1.0],
            vec![[0.0, 0.0, 0.0], [5.0, 5.0, 5.0], [10.0, 10.0, 10.0]],
            Interpolation::Linear,
        );

        let channel = AnimationChannel {
            target_node: 0,
            path: ChannelPath::Translation,
            sampler,
        };

        let clip = AnimationClip {
            name: "Walk".to_string(),
            duration: 1.0,
            channels: vec![channel],
        };

        assert_eq!(clip.name, "Walk");
        assert_eq!(clip.duration, 1.0);
        assert_eq!(clip.channels.len(), 1);
        assert_eq!(clip.get_duration(), 1.0);
    }

    #[test]
    fn test_channel_path_display() {
        assert_eq!(format!("{}", ChannelPath::Translation), "translation");
        assert_eq!(format!("{}", ChannelPath::Rotation), "rotation");
        assert_eq!(format!("{}", ChannelPath::Scale), "scale");
        assert_eq!(format!("{}", ChannelPath::Weights), "weights");
    }

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
    fn test_sample_buffer_with_capacity() {
        let buffer = SampleBuffer::with_capacity(10);
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

    #[test]
    fn test_animation_clip_with_multiple_channels() {
        let translation_sampler = AnimationSampler::new_translation(
            vec![0.0, 1.0],
            vec![[0.0, 0.0, 0.0], [1.0, 2.0, 3.0]],
            Interpolation::Linear,
        );

        let rotation_sampler = AnimationSampler::new_rotation(
            vec![0.0, 1.0],
            vec![[0.0, 0.0, 0.0, 1.0], [0.0, 0.0, 0.707, 0.707]],
            Interpolation::Linear,
        );

        let clip = AnimationClip {
            name: "Run".to_string(),
            duration: 1.0,
            channels: vec![
                AnimationChannel {
                    target_node: 0,
                    path: ChannelPath::Translation,
                    sampler: translation_sampler,
                },
                AnimationChannel {
                    target_node: 0,
                    path: ChannelPath::Rotation,
                    sampler: rotation_sampler,
                },
            ],
        };

        assert_eq!(clip.channels.len(), 2);
        assert_eq!(clip.channels[0].path, ChannelPath::Translation);
        assert_eq!(clip.channels[1].path, ChannelPath::Rotation);
    }

    #[test]
    fn test_interpolation_variants() {
        let linear = Interpolation::Linear;
        let step = Interpolation::Step;
        let cubic = Interpolation::CubicSpline;

        assert!(matches!(linear, Interpolation::Linear));
        assert!(matches!(step, Interpolation::Step));
        assert!(matches!(cubic, Interpolation::CubicSpline));
    }

    #[test]
    fn test_interpolation_default() {
        let interpolation = Interpolation::default();
        assert!(matches!(interpolation, Interpolation::Linear));
    }

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
