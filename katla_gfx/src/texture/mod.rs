//! Texture management module.
//!
//! This module provides a clean public API for texture creation and management
//! that doesn't expose Vulkan types directly.
//!
//! # Overview
//!
//! - [`TextureDescriptor`] - Describes texture properties for creation
//! - [`TextureUsage`] - Usage flags for textures
//! - [`TextureManager`] - Centralized texture creation and storage
//!
//! # Example
//!
//! ```ignore
//! use katla_gfx::{TextureDescriptor, TextureManager, TextureHandle};
//!
//! // Create texture manager
//! let texture_manager = TextureManager::new(context)?;
//!
//! // Create a texture from pixel data
//! let desc = TextureDescriptor::rgba8_srgb(512, 512);
//! let handle = texture_manager.create(&desc, &pixel_data);
//!
//! // Get the Vulkan image view for rendering
//! let view = texture_manager.get_view(handle);
//! ```

mod descriptor;
mod format;
mod manager;

pub use descriptor::{TextureDescriptor, TextureUsage};
pub use format::*;
pub use manager::TextureManager;
