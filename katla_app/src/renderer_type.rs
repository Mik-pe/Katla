//! cfg-based Renderer type alias.
//!
//! Switches between `VulkanRenderer` and `MetalRenderer` depending on
//! enabled Cargo features. When both are enabled, Vulkan takes priority.

#[cfg(feature = "vulkan")]
pub type Renderer = katla_gfx::VulkanRenderer;

#[cfg(all(feature = "metal", not(feature = "vulkan")))]
pub type Renderer = katla_gfx::MetalRenderer;
