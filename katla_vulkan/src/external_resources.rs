//! External resource registry for render graph imports.
//!
//! This module provides a registry for external resources (images and buffers)
//! that can be imported into a render graph. External resources are created
//! outside the render graph and registered here for lookup during graph building.

use std::collections::HashMap;

use ash::vk;

use crate::{BufferHandle, ImageHandle};

//=============================================================================
// External Image Descriptor
//=============================================================================

/// Describes an external image that can be imported into the render graph.
///
/// Contains all metadata needed for the render graph to properly use
/// an image that was created outside the graph.
#[derive(Clone, Debug)]
pub struct ExternalImageDescriptor {
    /// Unique name for lookup during render graph building.
    pub name: String,
    /// Handle to the image resource.
    pub handle: ImageHandle,
    /// Image format (e.g., R8G8B8A8_UNORM).
    pub format: vk::Format,
    /// Image dimensions.
    pub extent: vk::Extent3D,
    /// Usage flags indicating how the image can be used.
    pub usage: vk::ImageUsageFlags,
}

//=============================================================================
// External Buffer Descriptor
//=============================================================================

/// Describes an external buffer that can be imported into the render graph.
///
/// Contains all metadata needed for the render graph to properly use
/// a buffer that was created outside the graph.
#[derive(Clone, Debug)]
pub struct ExternalBufferDescriptor {
    /// Unique name for lookup during render graph building.
    pub name: String,
    /// Handle to the buffer resource.
    pub handle: BufferHandle,
    /// Size of the buffer in bytes.
    pub size: vk::DeviceSize,
    /// Usage flags indicating how the buffer can be used.
    pub usage: vk::BufferUsageFlags,
}

//=============================================================================
// External Resource Registry
//=============================================================================

/// Registry for external resources usable by render graph.
///
/// Stores descriptors for images and buffers created outside the render graph.
/// Resources are registered by name and can be looked up during graph building.
///
/// # Example
///
/// ```ignore
/// use katla_vulkan::{ExternalResourceRegistry, ExternalImageDescriptor, ImageHandle};
///
/// let mut registry = ExternalResourceRegistry::new();
///
/// let descriptor = ExternalImageDescriptor {
///     name: "backbuffer".to_string(),
///     handle: image_handle,
///     format: ash::vk::Format::R8G8B8A8_UNORM,
///     extent: ash::vk::Extent3D { width: 1920, height: 1080, depth: 1 },
///     usage: ash::vk::ImageUsageFlags::COLOR_ATTACHMENT,
/// };
///
/// registry.register_image(descriptor);
///
/// // Later, during render graph building:
/// if let Some(img) = registry.get_image("backbuffer") {
///     // Use the image in the graph
/// }
/// ```
pub struct ExternalResourceRegistry {
    images: HashMap<String, ExternalImageDescriptor>,
    buffers: HashMap<String, ExternalBufferDescriptor>,
}

impl ExternalResourceRegistry {
    /// Create a new empty registry.
    #[inline]
    pub fn new() -> Self {
        Self {
            images: HashMap::new(),
            buffers: HashMap::new(),
        }
    }

    /// Create a new registry with pre-allocated capacity.
    #[inline]
    pub fn with_capacity(image_capacity: usize, buffer_capacity: usize) -> Self {
        Self {
            images: HashMap::with_capacity(image_capacity),
            buffers: HashMap::with_capacity(buffer_capacity),
        }
    }

    /// Register an external image for render graph import.
    ///
    /// If an image with the same name already exists, it will be replaced.
    pub fn register_image(&mut self, descriptor: ExternalImageDescriptor) {
        self.images.insert(descriptor.name.clone(), descriptor);
    }

    /// Register an external buffer for render graph import.
    ///
    /// If a buffer with the same name already exists, it will be replaced.
    pub fn register_buffer(&mut self, descriptor: ExternalBufferDescriptor) {
        self.buffers.insert(descriptor.name.clone(), descriptor);
    }

    /// Get image descriptor by name.
    #[inline]
    pub fn get_image(&self, name: &str) -> Option<&ExternalImageDescriptor> {
        self.images.get(name)
    }

    /// Get buffer descriptor by name.
    #[inline]
    pub fn get_buffer(&self, name: &str) -> Option<&ExternalBufferDescriptor> {
        self.buffers.get(name)
    }

    /// Check if an image with the given name is registered.
    #[inline]
    pub fn contains_image(&self, name: &str) -> bool {
        self.images.contains_key(name)
    }

    /// Check if a buffer with the given name is registered.
    #[inline]
    pub fn contains_buffer(&self, name: &str) -> bool {
        self.buffers.contains_key(name)
    }

    /// Remove an image from the registry by name.
    ///
    /// Returns the removed descriptor if it existed.
    #[inline]
    pub fn remove_image(&mut self, name: &str) -> Option<ExternalImageDescriptor> {
        self.images.remove(name)
    }

    /// Remove a buffer from the registry by name.
    ///
    /// Returns the removed descriptor if it existed.
    #[inline]
    pub fn remove_buffer(&mut self, name: &str) -> Option<ExternalBufferDescriptor> {
        self.buffers.remove(name)
    }

    /// Get the number of registered images.
    #[inline]
    pub fn image_count(&self) -> usize {
        self.images.len()
    }

    /// Get the number of registered buffers.
    #[inline]
    pub fn buffer_count(&self) -> usize {
        self.buffers.len()
    }

    /// Check if the registry is empty (no images or buffers).
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.images.is_empty() && self.buffers.is_empty()
    }

    /// Remove all registered resources.
    pub fn clear(&mut self) {
        self.images.clear();
        self.buffers.clear();
    }

    /// Iterate over all registered image descriptors.
    #[inline]
    pub fn iter_images(&self) -> impl Iterator<Item = &ExternalImageDescriptor> {
        self.images.values()
    }

    /// Iterate over all registered buffer descriptors.
    #[inline]
    pub fn iter_buffers(&self) -> impl Iterator<Item = &ExternalBufferDescriptor> {
        self.buffers.values()
    }
}

impl Default for ExternalResourceRegistry {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

//=============================================================================
// Tests
//=============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_image_descriptor(name: &str) -> ExternalImageDescriptor {
        ExternalImageDescriptor {
            name: name.to_string(),
            handle: ImageHandle::new(1),
            format: vk::Format::R8G8B8A8_UNORM,
            extent: vk::Extent3D {
                width: 1920,
                height: 1080,
                depth: 1,
            },
            usage: vk::ImageUsageFlags::COLOR_ATTACHMENT,
        }
    }

    fn create_test_buffer_descriptor(name: &str) -> ExternalBufferDescriptor {
        ExternalBufferDescriptor {
            name: name.to_string(),
            handle: BufferHandle::new(1),
            size: 1024,
            usage: vk::BufferUsageFlags::STORAGE_BUFFER,
        }
    }

    #[test]
    fn test_registry_new() {
        let registry = ExternalResourceRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.image_count(), 0);
        assert_eq!(registry.buffer_count(), 0);
    }

    #[test]
    fn test_registry_default() {
        let registry = ExternalResourceRegistry::default();
        assert!(registry.is_empty());
    }

    #[test]
    fn test_register_image() {
        let mut registry = ExternalResourceRegistry::new();
        let descriptor = create_test_image_descriptor("backbuffer");

        registry.register_image(descriptor);

        assert!(!registry.is_empty());
        assert_eq!(registry.image_count(), 1);
        assert!(registry.contains_image("backbuffer"));
    }

    #[test]
    fn test_register_buffer() {
        let mut registry = ExternalResourceRegistry::new();
        let descriptor = create_test_buffer_descriptor("uniform_buffer");

        registry.register_buffer(descriptor);

        assert!(!registry.is_empty());
        assert_eq!(registry.buffer_count(), 1);
        assert!(registry.contains_buffer("uniform_buffer"));
    }

    #[test]
    fn test_get_image() {
        let mut registry = ExternalResourceRegistry::new();
        let descriptor = create_test_image_descriptor("backbuffer");

        registry.register_image(descriptor);

        let retrieved = registry.get_image("backbuffer");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "backbuffer");
        assert_eq!(retrieved.unwrap().format, vk::Format::R8G8B8A8_UNORM);
    }

    #[test]
    fn test_get_buffer() {
        let mut registry = ExternalResourceRegistry::new();
        let descriptor = create_test_buffer_descriptor("storage_buffer");

        registry.register_buffer(descriptor);

        let retrieved = registry.get_buffer("storage_buffer");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "storage_buffer");
        assert_eq!(retrieved.unwrap().size, 1024);
    }

    #[test]
    fn test_get_nonexistent() {
        let registry = ExternalResourceRegistry::new();

        assert!(registry.get_image("nonexistent").is_none());
        assert!(registry.get_buffer("nonexistent").is_none());
    }

    #[test]
    fn test_remove_image() {
        let mut registry = ExternalResourceRegistry::new();
        let descriptor = create_test_image_descriptor("backbuffer");

        registry.register_image(descriptor);
        assert_eq!(registry.image_count(), 1);

        let removed = registry.remove_image("backbuffer");
        assert!(removed.is_some());
        assert_eq!(registry.image_count(), 0);
        assert!(!registry.contains_image("backbuffer"));
    }

    #[test]
    fn test_remove_buffer() {
        let mut registry = ExternalResourceRegistry::new();
        let descriptor = create_test_buffer_descriptor("uniform_buffer");

        registry.register_buffer(descriptor);
        assert_eq!(registry.buffer_count(), 1);

        let removed = registry.remove_buffer("uniform_buffer");
        assert!(removed.is_some());
        assert_eq!(registry.buffer_count(), 0);
        assert!(!registry.contains_buffer("uniform_buffer"));
    }

    #[test]
    fn test_replace_image() {
        let mut registry = ExternalResourceRegistry::new();

        let descriptor1 = ExternalImageDescriptor {
            name: "backbuffer".to_string(),
            handle: ImageHandle::new(1),
            format: vk::Format::R8G8B8A8_UNORM,
            extent: vk::Extent3D {
                width: 1920,
                height: 1080,
                depth: 1,
            },
            usage: vk::ImageUsageFlags::COLOR_ATTACHMENT,
        };

        let descriptor2 = ExternalImageDescriptor {
            name: "backbuffer".to_string(),
            handle: ImageHandle::new(2),
            format: vk::Format::B8G8R8A8_UNORM,
            extent: vk::Extent3D {
                width: 1280,
                height: 720,
                depth: 1,
            },
            usage: vk::ImageUsageFlags::COLOR_ATTACHMENT,
        };

        registry.register_image(descriptor1);
        registry.register_image(descriptor2);

        assert_eq!(registry.image_count(), 1);
        let retrieved = registry.get_image("backbuffer").unwrap();
        assert_eq!(retrieved.handle.index(), 2);
        assert_eq!(retrieved.format, vk::Format::B8G8R8A8_UNORM);
    }

    #[test]
    fn test_clear() {
        let mut registry = ExternalResourceRegistry::new();

        registry.register_image(create_test_image_descriptor("img1"));
        registry.register_image(create_test_image_descriptor("img2"));
        registry.register_buffer(create_test_buffer_descriptor("buf1"));

        assert_eq!(registry.image_count(), 2);
        assert_eq!(registry.buffer_count(), 1);

        registry.clear();

        assert!(registry.is_empty());
        assert_eq!(registry.image_count(), 0);
        assert_eq!(registry.buffer_count(), 0);
    }

    #[test]
    fn test_with_capacity() {
        let registry = ExternalResourceRegistry::with_capacity(10, 5);
        assert!(registry.is_empty());
    }

    #[test]
    fn test_iter_images() {
        let mut registry = ExternalResourceRegistry::new();

        registry.register_image(create_test_image_descriptor("img1"));
        registry.register_image(create_test_image_descriptor("img2"));
        registry.register_image(create_test_image_descriptor("img3"));

        let names: Vec<&str> = registry.iter_images().map(|d| d.name.as_str()).collect();
        assert_eq!(names.len(), 3);
    }

    #[test]
    fn test_iter_buffers() {
        let mut registry = ExternalResourceRegistry::new();

        registry.register_buffer(create_test_buffer_descriptor("buf1"));
        registry.register_buffer(create_test_buffer_descriptor("buf2"));

        let names: Vec<&str> = registry.iter_buffers().map(|d| d.name.as_str()).collect();
        assert_eq!(names.len(), 2);
    }
}
