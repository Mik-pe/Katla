//! CPU-side validation for particle system safety (counter corruption, config validity).

use crate::particles::EmitterConfig;

/// Validation errors that can occur in the particle system.
#[derive(Debug, Clone)]
pub enum ValidationError {
    /// Counter corruption detected
    CounterCorruption(String),
    /// Invalid emitter configuration
    InvalidConfig(String),
    /// Multiple validation errors
    MultipleErrors(Vec<String>),
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationError::CounterCorruption(msg) => write!(f, "Counter corruption: {}", msg),
            ValidationError::InvalidConfig(msg) => write!(f, "Invalid config: {}", msg),
            ValidationError::MultipleErrors(errors) => {
                write!(f, "Multiple validation errors:\n{}", errors.join("\n"))
            }
        }
    }
}

impl std::error::Error for ValidationError {}

/// Validate particle counters for consistency.
///
/// # Arguments
/// * `alive_count` - Current alive particle count
/// * `dead_count` - Current dead particle count (stack pointer for free list)
/// * `max_particles` - Maximum particle capacity
///
/// # Returns
/// Ok(()) if counters are valid, Err with descriptive message if invalid
///
/// # Validation Rules
/// - alive_count must not exceed max_particles
/// - dead_count must not exceed max_particles
/// - Note: alive_count + dead_count does NOT need to equal max_particles
///   because dead_count is a stack pointer, not a count of dead particles
pub fn validate_counters(
    alive_count: u32,
    dead_count: u32,
    max_particles: u32,
) -> Result<(), ValidationError> {
    if alive_count > max_particles {
        return Err(ValidationError::CounterCorruption(format!(
            "alive_count ({}) exceeds max_particles ({})",
            alive_count, max_particles
        )));
    }

    if dead_count > max_particles {
        return Err(ValidationError::CounterCorruption(format!(
            "dead_count ({}) exceeds max_particles ({})",
            dead_count, max_particles
        )));
    }

    // NOTE: We do NOT validate that alive_count + dead_count == max_particles
    // because dead_count is a stack pointer (index of next free slot), not
    // a count of dead particles. The invariant that would be useful is:
    // alive_count + (particles in dead list) == max_particles
    // but we don't track "particles in dead list" separately from dead_count.

    Ok(())
}

/// Validate emitter configuration for validity.
///
/// # Arguments
/// * `config` - Emitter configuration to validate
///
/// # Returns
/// Ok(()) if config is valid, Err with descriptive message if invalid
///
/// # Validation Rules
/// - emit_rate must be >= 0
/// - base_lifetime must be > 0 (particles must live for some time)
/// - velocity_magnitude must be >= 0
/// - base_scale must be > 0 (particles must be visible)
pub fn validate_emitter_config(config: &EmitterConfig) -> Result<(), ValidationError> {
    if config.emit_rate < 0.0 {
        return Err(ValidationError::InvalidConfig(format!(
            "emit_rate ({}) must be >= 0",
            config.emit_rate
        )));
    }

    if config.base_lifetime <= 0.0 {
        return Err(ValidationError::InvalidConfig(format!(
            "base_lifetime ({}) must be > 0",
            config.base_lifetime
        )));
    }

    if config.velocity_magnitude < 0.0 {
        return Err(ValidationError::InvalidConfig(format!(
            "velocity_magnitude ({}) must be >= 0",
            config.velocity_magnitude
        )));
    }

    if config.base_scale <= 0.0 {
        return Err(ValidationError::InvalidConfig(format!(
            "base_scale ({}) must be > 0",
            config.base_scale
        )));
    }

    // Check lifetime_variation is reasonable (0-1 range)
    if config.lifetime_variation < 0.0 || config.lifetime_variation > 1.0 {
        return Err(ValidationError::InvalidConfig(format!(
            "lifetime_variation ({}) must be in [0, 1]",
            config.lifetime_variation
        )));
    }

    // Check scale_variation is reasonable (0-1 range)
    if config.scale_variation < 0.0 || config.scale_variation > 1.0 {
        return Err(ValidationError::InvalidConfig(format!(
            "scale_variation ({}) must be in [0, 1]",
            config.scale_variation
        )));
    }

    // Check color_variation is reasonable (0-1 range)
    if config.color_variation < 0.0 || config.color_variation > 1.0 {
        return Err(ValidationError::InvalidConfig(format!(
            "color_variation ({}) must be in [0, 1]",
            config.color_variation
        )));
    }

    // Check velocity_cone_angle is reasonable (0 to PI)
    if config.velocity_cone_angle < 0.0 || config.velocity_cone_angle > std::f32::consts::PI {
        return Err(ValidationError::InvalidConfig(format!(
            "velocity_cone_angle ({}) must be in [0, PI]",
            config.velocity_cone_angle
        )));
    }

    Ok(())
}

/// Validate all emitter configurations in a slice.
///
/// # Arguments
/// * `emitters` - Slice of emitter configurations to validate
///
/// # Returns
/// Vec<String> of all validation errors found (empty if all valid)
///
/// # Use Case
/// Useful for validating all emitters in the system at once.
pub fn validate_all_emitters(emitters: &[EmitterConfig]) -> Vec<String> {
    let mut errors = Vec::new();

    for (index, config) in emitters.iter().enumerate() {
        // Skip default/invalid emitters (emit_rate = 0 means inactive)
        if config.emit_rate == 0.0 && config.base_lifetime == 0.0 {
            continue;
        }

        if let Err(e) = validate_emitter_config(config) {
            errors.push(format!("Emitter {}: {}", index, e));
        }
    }

    errors
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_counters_valid() {
        // Valid case: alive + dead = max
        let result = validate_counters(500, 500, 1000);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_counters_alive_exceeds_max() {
        // Invalid: alive > max
        let result = validate_counters(1500, 0, 1000);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("alive_count"));
        assert!(err.to_string().contains("exceeds max_particles"));
    }

    #[test]
    fn test_validate_counters_dead_exceeds_max() {
        // Invalid: dead > max
        let result = validate_counters(0, 1500, 1000);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("dead_count"));
        assert!(err.to_string().contains("exceeds max_particles"));
    }

    #[test]
    fn test_validate_counters_sum_mismatch() {
        // Note: We no longer validate alive + dead == max because dead_count is
        // a stack pointer, not a count. This test now verifies that we DON'T error.
        let result = validate_counters(300, 400, 1000);
        assert!(result.is_ok(), "Validation should not check sum invariant");
    }

    #[test]
    fn test_validate_counters_corruption_detection() {
        // Note: We no longer validate counter sums. This test verifies we only
        // check individual bounds, not the relationship between counters.
        let result = validate_counters(600, 600, 1000);
        assert!(result.is_ok(), "Validation should not check sum invariant");
    }

    #[test]
    fn test_validate_emitter_config_valid() {
        let config = EmitterConfig {
            emit_rate: 100.0,
            base_lifetime: 5.0,
            velocity_magnitude: 2.0,
            base_scale: 0.1,
            ..Default::default()
        };
        assert!(validate_emitter_config(&config).is_ok());
    }

    #[test]
    fn test_validate_emitter_config_negative_emit_rate() {
        let config = EmitterConfig {
            emit_rate: -10.0,
            ..Default::default()
        };
        let result = validate_emitter_config(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("emit_rate"));
    }

    #[test]
    fn test_validate_emitter_config_zero_lifetime() {
        let config = EmitterConfig {
            base_lifetime: 0.0,
            ..Default::default()
        };
        let result = validate_emitter_config(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("base_lifetime"));
    }

    #[test]
    fn test_validate_emitter_config_negative_lifetime() {
        let config = EmitterConfig {
            base_lifetime: -1.0,
            ..Default::default()
        };
        let result = validate_emitter_config(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("base_lifetime"));
    }

    #[test]
    fn test_validate_emitter_config_negative_velocity() {
        let config = EmitterConfig {
            velocity_magnitude: -1.0,
            ..Default::default()
        };
        let result = validate_emitter_config(&config);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("velocity_magnitude")
        );
    }

    #[test]
    fn test_validate_emitter_config_zero_scale() {
        let config = EmitterConfig {
            base_scale: 0.0,
            ..Default::default()
        };
        let result = validate_emitter_config(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("base_scale"));
    }

    #[test]
    fn test_validate_emitter_config_invalid_lifetime_variation() {
        let config = EmitterConfig {
            lifetime_variation: 1.5,
            ..Default::default()
        };
        let result = validate_emitter_config(&config);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("lifetime_variation")
        );
    }

    #[test]
    fn test_validate_emitter_config_invalid_scale_variation() {
        let config = EmitterConfig {
            scale_variation: -0.1,
            ..Default::default()
        };
        let result = validate_emitter_config(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("scale_variation"));
    }

    #[test]
    fn test_validate_emitter_config_invalid_color_variation() {
        let config = EmitterConfig {
            color_variation: 2.0,
            ..Default::default()
        };
        let result = validate_emitter_config(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("color_variation"));
    }

    #[test]
    fn test_validate_emitter_config_invalid_cone_angle() {
        let config = EmitterConfig {
            velocity_cone_angle: 4.0, // > PI
            ..Default::default()
        };
        let result = validate_emitter_config(&config);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("velocity_cone_angle")
        );
    }

    #[test]
    fn test_validate_all_emitters_all_valid() {
        let emitters = vec![
            EmitterConfig {
                emit_rate: 100.0,
                base_lifetime: 5.0,
                ..Default::default()
            },
            EmitterConfig {
                emit_rate: 50.0,
                base_lifetime: 3.0,
                ..Default::default()
            },
        ];
        let errors = validate_all_emitters(&emitters);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_validate_all_emitters_multiple_errors() {
        let emitters = vec![
            EmitterConfig {
                emit_rate: -10.0, // Invalid
                ..Default::default()
            },
            EmitterConfig {
                base_lifetime: 0.0, // Invalid
                ..Default::default()
            },
            EmitterConfig {
                emit_rate: 100.0,
                base_lifetime: 5.0,
                ..Default::default()
            },
        ];
        let errors = validate_all_emitters(&emitters);
        assert_eq!(errors.len(), 2);
        assert!(errors[0].contains("Emitter 0"));
        assert!(errors[1].contains("Emitter 1"));
    }

    #[test]
    fn test_validate_all_emitters_skips_inactive() {
        let emitters = vec![
            EmitterConfig::default(), // Inactive (all zeros)
            EmitterConfig {
                emit_rate: 100.0,
                base_lifetime: 5.0,
                ..Default::default()
            },
        ];
        let errors = validate_all_emitters(&emitters);
        assert!(errors.is_empty());
    }
}
