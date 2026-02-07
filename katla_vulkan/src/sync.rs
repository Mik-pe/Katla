//! Wrapper types for Vulkan objects.
//!
//! This module provides wrapper types for Vulkan objects to avoid exposing
//! `ash::vk` types in the public API of katla_vulkan.

use ash::vk;

/// Wrapper around `vk::Semaphore`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VkSemaphore(pub vk::Semaphore);

unsafe impl Send for VkSemaphore {}
unsafe impl Sync for VkSemaphore {}

impl VkSemaphore {
    /// Creates a new Semaphore wrapper.
    pub fn new(semaphore: vk::Semaphore) -> Self {
        Self(semaphore)
    }

    /// Returns the underlying `vk::Semaphore`.
    pub fn vk(&self) -> vk::Semaphore {
        self.0
    }
}

impl From<vk::Semaphore> for VkSemaphore {
    fn from(semaphore: vk::Semaphore) -> Self {
        Self(semaphore)
    }
}

impl From<VkSemaphore> for vk::Semaphore {
    fn from(wrapper: VkSemaphore) -> Self {
        wrapper.0
    }
}

impl AsRef<vk::Semaphore> for VkSemaphore {
    fn as_ref(&self) -> &vk::Semaphore {
        &self.0
    }
}

/// Wrapper around `vk::Fence`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VkFence(pub vk::Fence);

unsafe impl Send for VkFence {}
unsafe impl Sync for VkFence {}

impl VkFence {
    /// Creates a new Fence wrapper.
    pub fn new(fence: vk::Fence) -> Self {
        Self(fence)
    }

    /// Returns the underlying `vk::Fence`.
    pub fn vk(&self) -> vk::Fence {
        self.0
    }
}

impl From<vk::Fence> for VkFence {
    fn from(fence: vk::Fence) -> Self {
        Self(fence)
    }
}

impl From<VkFence> for vk::Fence {
    fn from(wrapper: VkFence) -> Self {
        wrapper.0
    }
}

impl AsRef<vk::Fence> for VkFence {
    fn as_ref(&self) -> &vk::Fence {
        &self.0
    }
}

/// Wrapper around `vk::ImageView`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VkImageView(pub vk::ImageView);

unsafe impl Send for VkImageView {}
unsafe impl Sync for VkImageView {}

impl VkImageView {
    /// Creates a new ImageView wrapper.
    pub fn new(image_view: vk::ImageView) -> Self {
        Self(image_view)
    }

    /// Returns the underlying `vk::ImageView`.
    pub fn vk(&self) -> vk::ImageView {
        self.0
    }
}

impl From<vk::ImageView> for VkImageView {
    fn from(image_view: vk::ImageView) -> Self {
        Self(image_view)
    }
}

impl From<VkImageView> for vk::ImageView {
    fn from(wrapper: VkImageView) -> Self {
        wrapper.0
    }
}

impl AsRef<vk::ImageView> for VkImageView {
    fn as_ref(&self) -> &vk::ImageView {
        &self.0
    }
}

/// Wrapper around `vk::Sampler`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VkSampler(pub vk::Sampler);

unsafe impl Send for VkSampler {}
unsafe impl Sync for VkSampler {}

impl VkSampler {
    /// Creates a new Sampler wrapper.
    pub fn new(sampler: vk::Sampler) -> Self {
        Self(sampler)
    }

    /// Returns the underlying `vk::Sampler`.
    pub fn vk(&self) -> vk::Sampler {
        self.0
    }
}

impl From<vk::Sampler> for VkSampler {
    fn from(sampler: vk::Sampler) -> Self {
        Self(sampler)
    }
}

impl From<VkSampler> for vk::Sampler {
    fn from(wrapper: VkSampler) -> Self {
        wrapper.0
    }
}

impl AsRef<vk::Sampler> for VkSampler {
    fn as_ref(&self) -> &vk::Sampler {
        &self.0
    }
}

/// Wrapper around `vk::Image`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VkImage(pub vk::Image);

unsafe impl Send for VkImage {}
unsafe impl Sync for VkImage {}

impl VkImage {
    /// Creates a new Image wrapper.
    pub fn new(image: vk::Image) -> Self {
        Self(image)
    }

    /// Returns the underlying `vk::Image`.
    pub fn vk(&self) -> vk::Image {
        self.0
    }
}

impl From<vk::Image> for VkImage {
    fn from(image: vk::Image) -> Self {
        Self(image)
    }
}

impl From<VkImage> for vk::Image {
    fn from(wrapper: VkImage) -> Self {
        wrapper.0
    }
}

impl AsRef<vk::Image> for VkImage {
    fn as_ref(&self) -> &vk::Image {
        &self.0
    }
}

/// Wrapper around `vk::RenderPass`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VkRenderPass(pub vk::RenderPass);

unsafe impl Send for VkRenderPass {}
unsafe impl Sync for VkRenderPass {}

impl VkRenderPass {
    /// Creates a new RenderPass wrapper.
    pub fn new(render_pass: vk::RenderPass) -> Self {
        Self(render_pass)
    }

    /// Returns the underlying `vk::RenderPass`.
    pub fn vk(&self) -> vk::RenderPass {
        self.0
    }
}

impl From<vk::RenderPass> for VkRenderPass {
    fn from(render_pass: vk::RenderPass) -> Self {
        Self(render_pass)
    }
}

impl From<VkRenderPass> for vk::RenderPass {
    fn from(wrapper: VkRenderPass) -> Self {
        wrapper.0
    }
}

impl AsRef<vk::RenderPass> for VkRenderPass {
    fn as_ref(&self) -> &vk::RenderPass {
        &self.0
    }
}

/// Wrapper around `vk::Framebuffer`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VkFramebuffer(pub vk::Framebuffer);

unsafe impl Send for VkFramebuffer {}
unsafe impl Sync for VkFramebuffer {}

impl VkFramebuffer {
    /// Creates a new Framebuffer wrapper.
    pub fn new(framebuffer: vk::Framebuffer) -> Self {
        Self(framebuffer)
    }

    /// Returns the underlying `vk::Framebuffer`.
    pub fn vk(&self) -> vk::Framebuffer {
        self.0
    }
}

impl From<vk::Framebuffer> for VkFramebuffer {
    fn from(framebuffer: vk::Framebuffer) -> Self {
        Self(framebuffer)
    }
}

impl From<VkFramebuffer> for vk::Framebuffer {
    fn from(wrapper: VkFramebuffer) -> Self {
        wrapper.0
    }
}

impl AsRef<vk::Framebuffer> for VkFramebuffer {
    fn as_ref(&self) -> &vk::Framebuffer {
        &self.0
    }
}

/// Wrapper around `vk::DescriptorSet`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VkDescriptorSet(pub vk::DescriptorSet);

unsafe impl Send for VkDescriptorSet {}
unsafe impl Sync for VkDescriptorSet {}

impl VkDescriptorSet {
    /// Creates a new DescriptorSet wrapper.
    pub fn new(descriptor_set: vk::DescriptorSet) -> Self {
        Self(descriptor_set)
    }

    /// Returns the underlying `vk::DescriptorSet`.
    pub fn vk(&self) -> vk::DescriptorSet {
        self.0
    }
}

impl From<vk::DescriptorSet> for VkDescriptorSet {
    fn from(descriptor_set: vk::DescriptorSet) -> Self {
        Self(descriptor_set)
    }
}

impl From<VkDescriptorSet> for vk::DescriptorSet {
    fn from(wrapper: VkDescriptorSet) -> Self {
        wrapper.0
    }
}

impl AsRef<vk::DescriptorSet> for VkDescriptorSet {
    fn as_ref(&self) -> &vk::DescriptorSet {
        &self.0
    }
}

/// Wrapper around `vk::DescriptorSetLayout`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VkDescriptorSetLayout(pub vk::DescriptorSetLayout);

unsafe impl Send for VkDescriptorSetLayout {}
unsafe impl Sync for VkDescriptorSetLayout {}

impl VkDescriptorSetLayout {
    /// Creates a new DescriptorSetLayout wrapper.
    pub fn new(descriptor_set_layout: vk::DescriptorSetLayout) -> Self {
        Self(descriptor_set_layout)
    }

    /// Returns the underlying `vk::DescriptorSetLayout`.
    pub fn vk(&self) -> vk::DescriptorSetLayout {
        self.0
    }
}

impl From<vk::DescriptorSetLayout> for VkDescriptorSetLayout {
    fn from(descriptor_set_layout: vk::DescriptorSetLayout) -> Self {
        Self(descriptor_set_layout)
    }
}

impl From<VkDescriptorSetLayout> for vk::DescriptorSetLayout {
    fn from(wrapper: VkDescriptorSetLayout) -> Self {
        wrapper.0
    }
}

impl AsRef<vk::DescriptorSetLayout> for VkDescriptorSetLayout {
    fn as_ref(&self) -> &vk::DescriptorSetLayout {
        &self.0
    }
}

/// Wrapper around `vk::DescriptorPool`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VkDescriptorPool(pub vk::DescriptorPool);

unsafe impl Send for VkDescriptorPool {}
unsafe impl Sync for VkDescriptorPool {}

impl VkDescriptorPool {
    /// Creates a new DescriptorPool wrapper.
    pub fn new(descriptor_pool: vk::DescriptorPool) -> Self {
        Self(descriptor_pool)
    }

    /// Returns the underlying `vk::DescriptorPool`.
    pub fn vk(&self) -> vk::DescriptorPool {
        self.0
    }
}

impl From<vk::DescriptorPool> for VkDescriptorPool {
    fn from(descriptor_pool: vk::DescriptorPool) -> Self {
        Self(descriptor_pool)
    }
}

impl From<VkDescriptorPool> for vk::DescriptorPool {
    fn from(wrapper: VkDescriptorPool) -> Self {
        wrapper.0
    }
}

impl AsRef<vk::DescriptorPool> for VkDescriptorPool {
    fn as_ref(&self) -> &vk::DescriptorPool {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_semaphore_wrapper() {
        let vk_sem = vk::Semaphore::null();
        let sem = VkSemaphore::new(vk_sem);
        assert_eq!(sem.vk(), vk_sem);
    }

    #[test]
    fn test_fence_wrapper() {
        let vk_fence = vk::Fence::null();
        let fence = VkFence::new(vk_fence);
        assert_eq!(fence.vk(), vk_fence);
    }

    #[test]
    fn test_semaphore_conversions() {
        let vk_sem = vk::Semaphore::null();
        let sem: VkSemaphore = vk_sem.into();
        let back: vk::Semaphore = sem.into();
        assert_eq!(vk_sem, back);
    }

    #[test]
    fn test_fence_conversions() {
        let vk_fence = vk::Fence::null();
        let fence: VkFence = vk_fence.into();
        let back: vk::Fence = fence.into();
        assert_eq!(vk_fence, back);
    }
}
