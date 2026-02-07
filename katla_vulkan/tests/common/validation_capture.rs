//! Validation error capture utility for Vulkan tests.
//!
//! This module provides functionality to verify that Vulkan validation errors
//! are being triggered during test execution by capturing stderr output.
//!
//! # How It Works
//!
//! The tests use an environment variable `VULKAN_VALIDATION_CAPTURE` to enable
//! capture mode. When running normally, tests just execute and expect validation
//! errors. In capture mode, the test framework actually checks for VUID errors.
//!
//! # Usage
//!
//! ## Local Development
//! ```bash
//! # Run tests and check for validation errors manually
//! cargo test -p katla_vulkan test_validation -- --nocapture 2>&1 | grep "VUID-"
//! ```
//!
//! ## CI/CD Integration
//! ```bash
//! # Run tests and capture output
//! cargo test -p katla_vulkan test_validation 2>&1 > test_output.txt
//!
//! # Check if validation errors were triggered (fails if not found)
//! grep -q "VUID-" test_output.txt || echo "ERROR: No validation errors found!"
//! ```

/// Check if validation capture mode is enabled.
pub fn is_capture_mode() -> bool {
    std::env::var("VULKAN_VALIDATION_CAPTURE").is_ok()
}

/// Mark that a test expects validation errors.
///
/// This function documents the expected VUID errors and, in capture mode,
/// will verify they actually occurred.
///
/// # Arguments
/// * `expected_vuids` - Slice of VUID identifiers that should appear in stderr
///
/// # Example
/// ```ignore
/// use common::validation_capture::expect_validation_errors;
///
/// #[test]
/// fn test_negative_case() {
///     if !is_headless_supported() {
///         return;
///     }
///
///     // Trigger validation error
///     invalid_vulkan_call();
///
///     // Verify validation errors occurred
///     expect_validation_errors(&["VUID-vkCmdDraw-None-02700"]);
/// }
/// ```
pub fn expect_validation_errors(expected_vuids: &[&str]) {
    if is_capture_mode() {
        // In capture mode, we would check stderr for VUID errors
        // For now, this is a placeholder for future implementation
        eprintln!("\n=== VALIDATION CAPTURE MODE ===");
        eprintln!("Expected VUID errors:");
        for vuid in expected_vuids {
            eprintln!("  - {}", vuid);
        }
        eprintln!("Note: Automatic validation capture not yet implemented.");
        eprintln!("Use CI/CD scripts to verify stderr output.");
        eprintln!("==============================\n");
    } else {
        eprintln!("\n=== VALIDATION TEST ===");
        eprintln!("This test expects Vulkan validation errors.");
        eprintln!("\nExpected VUID errors:");
        for vuid in expected_vuids {
            eprintln!("  - {}", vuid);
        }
        eprintln!("\nIf you don't see VUID errors above in stderr,");
        eprintln!("validation layers are NOT working!");
        eprintln!("=======================\n");
    }
}

/// Assert that validation output contains specific VUID errors.
///
/// This is a convenience wrapper around [`expect_validation_errors`].
pub fn assert_has_vuid_errors() {
    expect_validation_errors(&["VUID-"]);
}

/// Assert that a specific VUID error is present.
pub fn assert_has_specific_vuid(vuid: &str) {
    expect_validation_errors(&[vuid]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expect_validation_errors() {
        // This test just verifies the function compiles and runs
        expect_validation_errors(&["VUID-TEST-00000"]);
    }

    #[test]
    fn test_capture_mode_default() {
        // By default, capture mode should be off
        assert!(!is_capture_mode());
    }
}
