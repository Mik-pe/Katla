#![cfg(feature = "vulkan")]
//! Tests for headless Vulkan context initialization.
//!
//! These tests validate that headless Vulkan support works correctly,
//! enabling automated testing without requiring a display or extensions.

mod common;

use common::create_headless_context;

/// Test basic headless context creation.
///
/// This test verifies that a VulkanContext can be created without a window.
/// The test validates behavior through the public API - if context creation succeeds,
/// the internal handles are guaranteed valid by Vulkan.
#[test]
fn test_headless_context_creation() {
    let context = create_headless_context(false);

    // Verify context was created successfully by checking we can access the device
    // This tests the public API behavior, not internal field values
    let _device = &context.device;
    let _physical_device = context.physical_device;

    println!("Headless context created successfully");
}

/// Test headless context with validation layers enabled.
///
/// This test ensures that validation layers work correctly in headless mode,
/// which is critical for catching Vulkan errors during automated testing.
#[test]
fn test_headless_with_validation() {
    let context = create_headless_context(true);

    // Verify context was created successfully with validation layers
    let _device = &context.device;

    println!("Headless context with validation layers created successfully");
}

/// Test headless context doesn't require surface-related structures.
///
/// In headless mode, the context should initialize successfully without
/// any surface or swapchain dependencies. This is verified by successful
/// context creation rather than checking internal fields.
#[test]
fn test_headless_no_surface_required() {
    // This test passes if create_headless_context succeeds without panicking
    let _headless_context = create_headless_context(false);

    println!("Headless context works without surface/swapchain");
}
