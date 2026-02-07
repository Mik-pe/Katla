//! Tests for headless Vulkan context initialization.
//!
//! These tests validate that headless Vulkan support works correctly,
//! enabling automated testing without requiring a display or extensions.

mod common;

use common::create_headless_context;

/// Test basic headless context creation.
///
/// This test verifies that a VulkanContext can be created without a window.
#[test]
fn test_headless_context_creation() {
    let context = create_headless_context(false);

    // Verify context was created successfully
    assert_ne!(context.physical_device, ash::vk::PhysicalDevice::null());

    // Verify graphics queue is available
    assert_ne!(context.graphics_queue, ash::vk::Queue::null());

    // Verify device was created
    assert_ne!(context.device.handle(), ash::vk::Device::null());

    // Verify no surface loader in headless mode
    assert!(context.surface_loader.is_none());

    // Verify no swapchain loader in headless mode
    assert!(context.swapchain_loader.is_none());

    // Verify no surface in headless mode (tests create their own render targets)
    assert!(context.surface.is_none());

    println!("Headless context created successfully");
}

/// Test headless context with validation layers enabled.
///
/// This test ensures that validation layers work correctly in headless mode,
/// which is critical for catching Vulkan errors during automated testing.
#[test]
fn test_headless_with_validation() {
    let context = create_headless_context(true);

    // Verify context was created successfully
    assert_ne!(context.physical_device, ash::vk::PhysicalDevice::null());

    println!("Headless context with validation layers created successfully");
}

/// Test headless context structure.
///
/// This verifies that headless mode doesn't create swapchain-related structures.
#[test]
fn test_headless_context_structure() {
    let headless_context = create_headless_context(false);

    // Verify headless context structure
    assert!(headless_context.surface_loader.is_none());
    assert!(headless_context.swapchain_loader.is_none());
    assert!(headless_context.surface.is_none()); // No surface
    assert_ne!(headless_context.graphics_queue, ash::vk::Queue::null());
    assert_ne!(headless_context.transfer_queue, ash::vk::Queue::null());

    println!("Headless context structure verified");
}
