//! Validation error detection utilities for Vulkan tests.
//!
//! This module provides utilities to verify that Vulkan validation layers
//! are working correctly during tests.

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
}
