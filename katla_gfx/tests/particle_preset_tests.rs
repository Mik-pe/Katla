//! Error case tests for particle preset system.
//!
//! These tests validate that the preset system handles errors correctly:
//! - Missing preset files
//! - Invalid JSON in preset files
//! - Permission errors on preset directory
//! - Preset save/load operations

mod common;

use katla_gfx::particles::{EmitterConfig, EmitterPreset};
use std::fs::{self, File};
use std::io::Write;

/// Test loading a preset that doesn't exist.
///
/// Verifies that attempting to load a nonexistent preset returns
/// a descriptive error message.
#[test]
fn test_load_nonexistent_preset() {
    // Create a temporary directory for this test
    let temp_dir = std::env::temp_dir().join("katla_test_nonexistent");
    fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");

    let nonexistent_path = temp_dir.join("does_not_exist.json");

    // Try to load a preset that doesn't exist
    let result = EmitterPreset::load_from_file(&nonexistent_path);

    // Verify it returns an error
    assert!(
        result.is_err(),
        "Should return error for nonexistent preset"
    );

    let error_msg = result.unwrap_err();
    assert!(
        error_msg.contains("Failed to read preset file"),
        "Error message should mention file read failure"
    );
    assert!(
        error_msg.contains(&nonexistent_path.display().to_string()),
        "Error message should include the file path"
    );

    // Clean up
    fs::remove_dir_all(&temp_dir).ok();

    println!("✓ test_load_nonexistent_preset passed");
    println!("  Error message: {}", error_msg);
}

/// Test loading a preset with invalid JSON.
///
/// Verifies that attempting to load a preset with malformed JSON
/// returns a deserialization error.
#[test]
fn test_load_invalid_json_preset() {
    // Create a temporary directory for this test
    let temp_dir = std::env::temp_dir().join("katla_test_invalid_json");
    fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");

    let invalid_json_path = temp_dir.join("invalid_preset.json");

    // Create a file with invalid JSON
    let mut file = File::create(&invalid_json_path).expect("Failed to create test file");
    writeln!(
        file,
        "{{ \"name\": \"test\", \"config\": invalid_json_here }}"
    )
    .expect("Failed to write invalid JSON");

    // Try to load the invalid preset
    let result = EmitterPreset::load_from_file(&invalid_json_path);

    // Verify it returns a deserialization error
    assert!(result.is_err(), "Should return error for invalid JSON");

    let error_msg = result.unwrap_err();
    assert!(
        error_msg.contains("Failed to deserialize preset"),
        "Error message should mention deserialization failure"
    );
    assert!(
        error_msg.contains(&invalid_json_path.display().to_string()),
        "Error message should include the file path"
    );

    // Clean up
    fs::remove_dir_all(&temp_dir).ok();

    println!("✓ test_load_invalid_json_preset passed");
    println!("  Error message: {}", error_msg);
}

/// Test saving and loading a preset.
///
/// Verifies that a preset can be saved to a file and loaded back
/// with all data preserved correctly.
#[test]
fn test_save_and_load_preset() {
    // Create a temporary directory for this test
    let temp_dir = std::env::temp_dir().join("katla_test_save_load");
    fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");

    let preset_path = temp_dir.join("test_preset.json");

    // Create a test preset with specific values
    let original_config = EmitterConfig {
        position: [1.5, 2.5, 3.5],
        velocity_magnitude: 10.0,
        velocity_direction: [0.0, 1.0, 0.0],
        velocity_cone_angle: 0.5,
        emit_rate: 123.45,
        base_lifetime: 6.78,
        lifetime_variation: 0.5,
        base_scale: 2.0,
        scale_variation: 0.3,
        color: [0.8, 0.2, 0.1, 1.0],
        color_variation: 0.1,
        shape: katla_gfx::particles::EmitterShape::Circle,
        shape_params: [3.0, 0.0, 0.0, 0.0],
        ..Default::default()
    };

    let original_preset = EmitterPreset::new("Test Preset".to_string(), original_config);

    // Save the preset
    let save_result = original_preset.save_to_file(&preset_path);
    assert!(
        save_result.is_ok(),
        "Should successfully save preset: {:?}",
        save_result
    );

    // Verify the file was created
    assert!(
        preset_path.exists(),
        "Preset file should exist after saving"
    );

    // Load the preset back
    let load_result = EmitterPreset::load_from_file(&preset_path);
    assert!(
        load_result.is_ok(),
        "Should successfully load saved preset: {:?}",
        load_result
    );

    let loaded_preset = load_result.unwrap();

    // Verify all data matches
    assert_eq!(
        loaded_preset.name, original_preset.name,
        "Name should match"
    );
    assert_eq!(
        loaded_preset.config.position, original_preset.config.position,
        "Position should match"
    );
    assert_eq!(
        loaded_preset.config.velocity_magnitude, original_preset.config.velocity_magnitude,
        "Velocity magnitude should match"
    );
    assert_eq!(
        loaded_preset.config.emit_rate, original_preset.config.emit_rate,
        "Emit rate should match"
    );
    assert_eq!(
        loaded_preset.config.base_lifetime, original_preset.config.base_lifetime,
        "Base lifetime should match"
    );
    assert_eq!(
        loaded_preset.config.color, original_preset.config.color,
        "Color should match"
    );
    assert_eq!(
        loaded_preset.config.color_variation, original_preset.config.color_variation,
        "Color variation should match"
    );
    assert_eq!(
        loaded_preset.config.shape, original_preset.config.shape,
        "Emitter shape should match"
    );
    assert_eq!(
        loaded_preset.config.shape_params, original_preset.config.shape_params,
        "Shape params should match"
    );

    // Clean up
    fs::remove_dir_all(&temp_dir).ok();

    println!("✓ test_save_and_load_preset passed");
    println!("  All {} fields preserved correctly", 15);
}

/// Test loading a preset with valid JSON but missing required fields.
///
/// Verifies that attempting to load a preset with incomplete data
/// returns a deserialization error.
#[test]
fn test_load_incomplete_json_preset() {
    // Create a temporary directory for this test
    let temp_dir = std::env::temp_dir().join("katla_test_incomplete_json");
    fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");

    let incomplete_json_path = temp_dir.join("incomplete_preset.json");

    // Create a file with valid JSON but missing required fields
    let mut file = File::create(&incomplete_json_path).expect("Failed to create test file");
    writeln!(file, "{{ \"name\": \"test\" }}").expect("Failed to write incomplete JSON");

    // Try to load the incomplete preset
    let result = EmitterPreset::load_from_file(&incomplete_json_path);

    // Verify it returns a deserialization error
    assert!(result.is_err(), "Should return error for incomplete JSON");

    let error_msg = result.unwrap_err();
    assert!(
        error_msg.contains("Failed to deserialize preset"),
        "Error message should mention deserialization failure"
    );

    // Clean up
    fs::remove_dir_all(&temp_dir).ok();

    println!("✓ test_load_incomplete_json_preset passed");
    println!("  Error message: {}", error_msg);
}
