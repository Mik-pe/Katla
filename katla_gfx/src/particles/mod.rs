//! Shared particle shader data and scene emitter contracts.

#[cfg(test)]
mod config_tests;
mod data;
pub mod particle_drive;
mod presets;
pub(crate) mod types;
mod validation;

pub use crate::handle::EmitterHandle;
pub use data::{FrameData, IndirectDrawCommandData, ParticleCounters, ParticleData};
pub use presets::EmitterPreset;
pub use types::{Align16Vec4, EmitterConfig, EmitterConfigBuilder, EmitterShape};
pub use validation::{
    ValidationError, validate_all_emitters, validate_counters, validate_emitter_config,
};

/// Default capacity of the scene's shared particle storage.
pub const DEFAULT_MAX_PARTICLES: u32 = 1_048_576;
/// Maximum active scene emitters supported by the shader interface.
pub const MAX_EMITTERS: u32 = 1024;
/// Workgroup width of the particle emission shader.
pub const PARTICLE_EMIT_WORKGROUP_SIZE: u32 = 256;
/// Workgroup width of the particle simulation shader.
pub const PARTICLE_SIMULATE_WORKGROUP_SIZE: u32 = 64;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_emitter_config_size() {
        assert_eq!(std::mem::size_of::<EmitterConfig>(), 160);
    }

    #[test]
    fn test_emitter_handle() {
        let handle = EmitterHandle::from_raw(42, 0);
        assert_eq!(handle.index(), 42);
        assert_ne!(handle, EmitterHandle::NONE);
    }

    #[test]
    fn test_emitter_shape_default() {
        let config = EmitterConfig::default();
        assert_eq!(config.shape, EmitterShape::Point);
        assert_eq!(config.shape_params, [0.0; 4]);
    }

    #[test]
    fn test_emitter_shape_point() {
        let config = EmitterConfig {
            shape: EmitterShape::Point,
            ..Default::default()
        };
        assert_eq!(config.shape, EmitterShape::Point);
    }

    #[test]
    fn test_emitter_shape_line() {
        let config = EmitterConfig {
            shape: EmitterShape::Line,
            shape_params: [10.0, 0.0, 0.0, 0.0],
            ..Default::default()
        };
        assert_eq!(config.shape, EmitterShape::Line);
        assert_eq!(config.shape_params[0], 10.0);
    }

    #[test]
    fn test_emitter_shape_circle() {
        let config = EmitterConfig {
            shape: EmitterShape::Circle,
            shape_params: [5.0, 0.0, 0.0, 0.0],
            ..Default::default()
        };
        assert_eq!(config.shape, EmitterShape::Circle);
        assert_eq!(config.shape_params[0], 5.0);
    }

    #[test]
    fn test_emitter_shape_sphere() {
        let config = EmitterConfig {
            shape: EmitterShape::Sphere,
            shape_params: [3.0, 0.0, 0.0, 0.0],
            ..Default::default()
        };
        assert_eq!(config.shape, EmitterShape::Sphere);
        assert_eq!(config.shape_params[0], 3.0);
    }

    #[test]
    fn test_emitter_shape_box() {
        let config = EmitterConfig {
            shape: EmitterShape::Box,
            shape_params: [4.0, 3.0, 2.0, 0.0],
            ..Default::default()
        };
        assert_eq!(config.shape, EmitterShape::Box);
        assert_eq!(config.shape_params[0], 4.0);
        assert_eq!(config.shape_params[1], 3.0);
        assert_eq!(config.shape_params[2], 2.0);
    }

    #[test]
    fn test_emitter_shape_serialization() {
        let config = EmitterConfig {
            position: [1.0, 2.0, 3.0],
            _pad_position: 0.0,
            shape: EmitterShape::Sphere,
            emit_rate: 100.0,
            base_lifetime: 2.0,
            lifetime_variation: 0.5,
            velocity_direction: [0.0, 1.0, 0.0],
            _pad_velocity: 0.0,
            velocity_magnitude: 5.0,
            velocity_cone_angle: 0.3,
            base_scale: 0.2,
            scale_variation: 0.3,
            color: [1.0, 0.5, 0.0, 1.0],
            color_variation: 0.2,
            color_end: Align16Vec4([0.0; 4]),
            shape_params: [2.5, 0.0, 0.0, 0.0],
            gravity: -9.8,
            turbulence_strength: 0.0,
            turbulence_frequency: 3.0,
            kill_all: 0,
            scale_end: 1.0,
            _pad2: [0.0; 3],
        };

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: EmitterConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.shape, EmitterShape::Sphere);
        assert_eq!(deserialized.shape_params[0], 2.5);
        assert_eq!(deserialized.position, [1.0, 2.0, 3.0]);
    }

    #[test]
    fn test_emitter_config_field_offsets() {
        assert_eq!(std::mem::offset_of!(EmitterConfig, position), 0);
        assert_eq!(std::mem::offset_of!(EmitterConfig, shape), 16);
        assert_eq!(std::mem::offset_of!(EmitterConfig, emit_rate), 20);
        assert_eq!(std::mem::offset_of!(EmitterConfig, base_lifetime), 24);
        assert_eq!(std::mem::offset_of!(EmitterConfig, lifetime_variation), 28);
        assert_eq!(std::mem::offset_of!(EmitterConfig, velocity_direction), 32);
        assert_eq!(std::mem::offset_of!(EmitterConfig, velocity_magnitude), 48);
        assert_eq!(std::mem::offset_of!(EmitterConfig, velocity_cone_angle), 52);
        assert_eq!(std::mem::offset_of!(EmitterConfig, base_scale), 56);
        assert_eq!(std::mem::offset_of!(EmitterConfig, scale_variation), 60);
        assert_eq!(std::mem::offset_of!(EmitterConfig, color), 64);
        assert_eq!(std::mem::offset_of!(EmitterConfig, color_variation), 80);
        assert_eq!(std::mem::offset_of!(EmitterConfig, color_end), 96);
        assert_eq!(std::mem::offset_of!(EmitterConfig, shape_params), 112);
        assert_eq!(std::mem::offset_of!(EmitterConfig, gravity), 128);
        assert_eq!(std::mem::offset_of!(EmitterConfig, kill_all), 140);
        assert_eq!(std::mem::offset_of!(EmitterConfig, scale_end), 144);
        assert_eq!(std::mem::offset_of!(EmitterConfig, _pad2), 148);
    }

    #[test]
    fn test_all_emitter_shapes() {
        let shapes = [
            EmitterShape::Point,
            EmitterShape::Line,
            EmitterShape::Circle,
            EmitterShape::Sphere,
            EmitterShape::Box,
        ];

        for shape in shapes {
            let config = EmitterConfig {
                shape,
                ..Default::default()
            };
            assert_eq!(config.shape, shape);
        }
    }
}
