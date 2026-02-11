//! Validation error detection utilities for Vulkan tests.
//!
//! This module provides utilities to verify that Vulkan validation layers
//! are working correctly during tests.
//!
//! # How Validation Detection Works
//!
//! Vulkan validation layers write error messages to stderr with VUID identifiers.
//! These tests are designed to trigger specific validation errors.
//!
//! To verify validation is working, run tests with stderr output:
//!
//! ```bash
//! # Run with stderr visible
//! cargo test -p katla_vulkan test_validation -- --nocapture 2>&1 | tee test_output.txt
//!
//! # Check for VUID errors
//! grep "VUID-" test_output.txt
//! ```
//!
//! If no VUID errors appear, validation layers are not working.
//!
//! # CI/CD Integration
//!
//! For CI/CD pipelines, capture stderr and check for VUID patterns:
//!
//! ```bash
//! # Run tests and capture output
//! cargo test -p katla_vulkan test_validation 2>&1 > output.txt
//!
//! # Fail if no validation errors found
//! if ! grep -q "VUID-" output.txt; then
//!     echo "ERROR: Validation layers not working!"
//!     exit 1
//! fi
//! ```

use std::io::{self, Write};

/// Expected validation error identifier.
///
/// Use this type to document which VUID errors a test should trigger.
#[derive(Debug, Clone, PartialEq)]
pub struct ExpectedVuid(pub &'static str);

impl ExpectedVuid {
    /// Create a new expected VUID.
    pub const fn new(vuid: &'static str) -> Self {
        Self(vuid)
    }

    /// Get the VUID string.
    pub fn as_str(&self) -> &'static str {
        self.0
    }
}

/// Common VUID errors that tests might trigger.
pub mod vuids {
    use super::ExpectedVuid;

    /// Drawing without a graphics pipeline bound.
    pub const DRAW_NO_PIPELINE: ExpectedVuid = ExpectedVuid::new("VUID-vkCmdDraw-None-02700");

    /// Ending a render pass without beginning one.
    pub const END_RENDER_PASS_MISMATCH: ExpectedVuid =
        ExpectedVuid::new("VUID-vkCmdEndRenderPass-None-00679");

    /// Image layout mismatch.
    pub const IMAGE_LAYOUT_MISMATCH: ExpectedVuid =
        ExpectedVuid::new("VUID-VkImageMemoryBarrier-oldLayout-01199");

    /// Command buffer in invalid state.
    pub const CMD_BUFFER_STATE: ExpectedVuid =
        ExpectedVuid::new("VUID-vkCmdDraw-commandBuffer-00027");
}

/// Print validation test instructions to stderr.
///
/// This function outputs clear instructions about what validation errors
/// should appear in stderr. Run this at the end of negative tests.
pub fn print_validation_instructions(expected: &[ExpectedVuid]) {
    eprintln!("\n=== VALIDATION TEST INSTRUCTIONS ===");
    eprintln!("This test should trigger Vulkan validation errors.");
    eprintln!("\nExpected validation errors:");
    for vuid in expected {
        eprintln!("  - {}", vuid.as_str());
    }
    eprintln!("\nIf you DON'T see these VUID errors above in stderr,");
    eprintln!("validation layers are NOT properly configured!");
    eprintln!("===================================\n");
}

/// Print a warning if validation layers might not be enabled.
///
/// Call this at the start of tests that require validation layers.
pub fn warn_if_validation_disabled() {
    // Note: We can't programmatically check if validation is enabled
    // without accessing internal VulkanContext fields.
    // This function serves as documentation and a reminder.
    eprintln!("NOTE: This test requires Vulkan validation layers enabled.");
    eprintln!("If validation errors don't appear, check your Vulkan SDK installation.\n");
}

/// Check stderr for VUID errors (external helper).
///
/// This is meant to be used by external scripts or CI/CD pipelines.
/// Returns true if VUID errors are found in the provided string.
pub fn has_vuid_in_output(output: &str) -> bool {
    output.contains("VUID-")
}

/// Check stderr for a specific VUID error (external helper).
pub fn has_specific_vuid_in_output(output: &str, vuid: &str) -> bool {
    output.contains(vuid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vuid_detection() {
        let output = "Some error: VUID-vkCmdDraw-None-02700";
        assert!(has_vuid_in_output(output));
        assert!(has_specific_vuid_in_output(
            output,
            "VUID-vkCmdDraw-None-02700"
        ));
    }

    #[test]
    fn test_vuid_not_found() {
        let output = "Some regular output without VUID errors";
        assert!(!has_vuid_in_output(output));
        assert!(!has_specific_vuid_in_output(
            output,
            "VUID-vkCmdDraw-None-02700"
        ));
    }

    #[test]
    fn test_expected_vuid() {
        assert_eq!(
            vuids::DRAW_NO_PIPELINE.as_str(),
            "VUID-vkCmdDraw-None-02700"
        );
    }
}
