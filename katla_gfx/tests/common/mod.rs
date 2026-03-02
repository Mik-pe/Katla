//! Common test utilities for headless Vulkan testing.

pub mod validation;

use katla_gfx::VulkanContext;
use std::ffi::CString;

/// Create a headless Vulkan context for testing.
///
/// This function initializes a VulkanContext without requiring a window.
/// Tests can create their own VkImage render targets for validation testing.
///
/// # Panics
/// - If Vulkan is not available
/// - If Vulkan initialization fails
///
/// # Example
/// ```no_run
/// use katla_gfx::tests::common::create_headless_context;
///
/// let context = create_headless_context(true);
/// println!("Headless context created successfully");
/// ```
pub fn create_headless_context(with_validation_layers: bool) -> VulkanContext {
    let app_name = CString::new("Katla Headless Tests").unwrap();
    let engine_name = CString::new("Katla Engine").unwrap();

    VulkanContext::init_headless(with_validation_layers, app_name, engine_name)
}
