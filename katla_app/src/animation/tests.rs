#[cfg(test)]
mod tests {
    use crate::animation::components::{AnimationPlayer, JointTransform, MorphTargetWeights};
    use crate::animation::clips::{AnimationClip, AnimationChannel, AnimationSampler, ChannelPath, SampledValue};
    use crate::animation::samplers::Interpolation;
    use katla_math::Quat;
    use std::collections::HashMap;

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
        let player = AnimationPlayer::new("Run")
            .looping()
            .with_speed(2.0);

        assert!(player.loop_animation);
        assert_eq!(player.speed, 2.0);
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
        player.playing = true;

        player.stop();

        assert_eq!(player.time, 0.0);
        assert!(!player.playing);
    }

    #[test]
    fn test_animation_player_seek() {
        let mut player = AnimationPlayer::new("Test");

        player.seek(2.5);

        // Since get_duration() returns 0.0 (no clip data), seek clamps to 0.0
        assert_eq!(player.time, 0.0);
    }

    #[test]
    fn test_animation_player_seek_clamps() {
        let mut player = AnimationPlayer::new("Test");

        player.seek(-5.0);

        // Negative values are clamped to 0.0
        assert_eq!(player.time, 0.0);

        player.seek(10.0);

        // Positive values are clamped to duration (0.0 when no clip data)
        assert_eq!(player.time, 0.0);
    }

    #[test]
    fn test_animation_player_with_clip() {
        let player = AnimationPlayer::stopped()
            .with_clip("Jump");

        assert_eq!(player.current_clip.as_ref().unwrap(), "Jump");
    }

    #[test]
    fn test_joint_transform_identity() {
        let identity = JointTransform::identity();

        assert_eq!(identity.translation, [0.0, 0.0, 0.0]);
        assert_eq!(identity.scale, [1.0, 1.0, 1.0]);
        // Quaternion [0, 0, 0, 1] is identity
        assert_eq!(identity.rotation[3], 1.0); // w component
    }

    #[test]
    fn test_joint_transform_from_translation() {
        let transform = JointTransform::from_translation([1.0, 2.0, 3.0]);

        assert_eq!(transform.translation, [1.0, 2.0, 3.0]);
        assert_eq!(transform.scale, [1.0, 1.0, 1.0]);
        assert_eq!(transform.rotation[3], 1.0); // w component (identity rotation)
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
        // Test SLERP interpolation
        let start = JointTransform {
            translation: [0.0, 0.0, 0.0],
            rotation: Quat::new(), // Identity
            scale: [1.0, 1.0, 1.0],
        };

        // 90 degree rotation around Z axis
        let end_quat = Quat::new_from_xyzw(0.0, 0.0, 0.707, 0.707);
        let end = JointTransform {
            translation: [0.0, 0.0, 0.0],
            rotation: end_quat,
            scale: [1.0, 1.0, 1.0],
        };

        let result = start.lerp(&end, 0.5);

        // At t=0.5, should be approximately 45 degrees
        // w should be > 0.85 for 45 deg (cos(22.5°))
        assert!(result.rotation[3] > 0.85);
    }

    #[test]
    fn test_joint_transform_lerp_full() {
        let start = JointTransform {
            translation: [0.0, 0.0, 0.0],
            rotation: Quat::new(),
            scale: [1.0, 1.0, 1.0],
        };
        let end = JointTransform {
            translation: [10.0, 20.0, 30.0],
            rotation: Quat::new_from_xyzw(0.0, 0.0, 0.707, 0.707),
            scale: [2.0, 2.0, 2.0],
        };

        let result = start.lerp(&end, 0.5);

        assert_eq!(result.translation, [5.0, 10.0, 15.0]);
        assert_eq!(result.scale, [1.5, 1.5, 1.5]);
        // Rotation should be interpolated with SLERP
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

        // Should not panic, just ignore
        weights.set_weight(5, 0.5);

        assert_eq!(weights.get_weight(5), 0.0);
    }

    #[test]
    fn test_morph_target_weights_get_out_of_bounds() {
        let weights = MorphTargetWeights::new(2);

        assert_eq!(weights.get_weight(10), 0.0);
    }

    #[test]
    fn test_animation_sampler_translation() {
        let inputs = vec![0.0, 0.5, 1.0];
        let translations = vec![
            [0.0, 0.0, 0.0],
            [5.0, 5.0, 5.0],
            [10.0, 10.0, 10.0],
        ];

        let sampler = AnimationSampler::new_translation(
            inputs.clone(),
            translations,
            Interpolation::Linear,
        );

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
        let rotations = vec![
            [0.0, 0.0, 0.0, 1.0],
            [0.0, 0.0, 0.707, 0.707],
        ];

        let sampler = AnimationSampler::new_rotation(
            inputs,
            rotations,
            Interpolation::Linear,
        );

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
        let sampler = AnimationSampler::new_translation(
            inputs,
            vec![[0.0; 3]; 5],
            Interpolation::Linear,
        );

        assert_eq!(sampler.keyframe_count(), 5);
    }

    #[test]
    fn test_animation_sampler_duration() {
        let inputs = vec![0.0, 0.5, 1.0, 1.5, 2.0];
        let sampler = AnimationSampler::new_translation(
            inputs,
            vec![[0.0; 3]; 5],
            Interpolation::Linear,
        );

        assert_eq!(sampler.duration(), 2.0);
    }

    #[test]
    fn test_animation_sampler_empty_duration() {
        let sampler = AnimationSampler::new_translation(
            vec![],
            vec![],
            Interpolation::Linear,
        );

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
    }

    #[test]
    fn test_channel_path_display() {
        assert_eq!(format!("{}", ChannelPath::Translation), "translation");
        assert_eq!(format!("{}", ChannelPath::Rotation), "rotation");
        assert_eq!(format!("{}", ChannelPath::Scale), "scale");
        assert_eq!(format!("{}", ChannelPath::Weights), "weights");
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
        use crate::animation::samplers::Interpolation;

        let linear = Interpolation::Linear;
        let step = Interpolation::Step;
        let cubic = Interpolation::CubicSpline;

        // Test that all variants can be created
        assert!(matches!(linear, Interpolation::Linear));
        assert!(matches!(step, Interpolation::Step));
        assert!(matches!(cubic, Interpolation::CubicSpline));
    }
}
