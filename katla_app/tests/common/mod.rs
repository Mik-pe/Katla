//! Common test utilities for katla_app testing.

use katla_gfx::tests::common::create_headless_context;
use katla_gfx::ValidationMode;
use katla_gfx::VulkanContext;
use std::ffi::CString;
use std::rc::Rc;

/// Create a headless Vulkan context for testing with GlobalParticleSystem.
///
/// This is a convenience wrapper around the katla_gfx test utilities
/// specifically for katla_app tests that need both Vulkan context and particle system.
pub fn create_particle_test_context() -> Rc<VulkanContext> {
    Rc::new(create_headless_context(false))
}
