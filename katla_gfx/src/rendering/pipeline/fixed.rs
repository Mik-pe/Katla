//! Fixed render pipeline implementation.
//!
//! This module provides a fixed render pipeline that that executes render passes in order
//! all is a pipeline builder pattern for is.

//!
//! `render()` method executes all passes.
//! `resize()` method handles window resize.
//!
//! `attachment_configs` field holds attachment configurations by name
pub struct attachmentConfig {
    pub format: ImageFormat,
    pub size: AttachmentSize,
    pub usage: vk::ImageUsageFlags,
}
