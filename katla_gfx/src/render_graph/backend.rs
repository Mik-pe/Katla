//! Render graph backend trait.
//!
//! Defines the interface that GPU backends must implement to execute
//! a render graph. The render graph core (Layer 1) uses this trait
//! to delegate all GPU-specific work to the backend.

use super::error::RenderGraphError;
use super::resource::{BufferDesc, GraphResourceDesc};
use crate::texture::ImageFormat;

/// Compiled storage policy for one frame-owned physical texture range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransientSlotPolicy {
    /// Frame slot that owns this allocation until GPU completion.
    pub frame_slot: usize,
    /// Stable compiled physical allocation index.
    pub allocation_slot: u32,
    /// Enable physical aliasing and tile-memory selection.
    pub optimize: bool,
    /// Every member is a single-pass attachment with discarded contents.
    pub memoryless: bool,
    /// Native shader write capability required by live accesses.
    pub storage: bool,
    /// Native transfer destination capability required by live accesses.
    pub transfer_destination: bool,
}

/// Backend-observed storage for a logical transient texture.
#[derive(Debug, Clone)]
pub struct NativeTransientAllocation {
    /// Process-local identity used only to deduplicate physical ranges.
    pub identity: u64,
    /// Byte offset of the texture range in its native backing allocation.
    pub offset: u64,
    /// Native reserved byte count for this physical range.
    pub bytes: u64,
    /// Equivalent standalone native allocation requirement for this texture.
    pub logical_bytes: u64,
    /// Backend storage strategy selected for the observed allocation.
    pub strategy: &'static str,
}

/// Backend interface for render graph execution.
///
/// Each GPU backend (Vulkan, Metal) implements this trait to provide
/// concrete resource creation, barrier insertion, and pass execution.
///
/// The trait is designed so that the render graph core can drive execution
/// without knowing about GPU-specific types. Backend implementations handle
/// all GPU-specific details internally.
pub trait RenderGraphBackend: Sized + 'static {
    /// Pending uploads that precede this frame's graph encoding.
    fn graph_texture_upload_producers(
        &self,
    ) -> Vec<(
        crate::handle::TextureHandle,
        crate::texture::TextureUploadRegion,
    )> {
        Vec::new()
    }

    /// Backend-specific transient texture type.
    type TransientTexture;

    /// Backend-specific image view type for render pass attachments.
    type ImageView: Clone + Send + Sync;

    /// Backend-specific buffer allocation type.
    type TransientBuffer;

    /// Create transient textures for one physical allocation slot.
    ///
    /// The graph compiler assigns compatible, non-overlapping transient
    /// resources to the same slot; `members` holds their descriptors in
    /// declaration order. Backends with memory aliasing back every member
    /// with shared physical storage sized for the largest member; backends
    /// without aliasing (or single-member slots) create standalone
    /// resources. The returned textures correspond one-to-one with
    /// `members`.
    fn create_transient_slot(
        &self,
        members: &[GraphResourceDesc],
        policy: TransientSlotPolicy,
    ) -> Result<Vec<Self::TransientTexture>, RenderGraphError>;

    /// Inspect actual storage after native texture allocation.
    fn transient_allocation_info(
        _texture: &Self::TransientTexture,
    ) -> Option<NativeTransientAllocation> {
        None
    }

    /// Inspect actual backing storage of one graph-owned buffer.
    fn transient_buffer_allocation_info(
        _buffer: &Self::TransientBuffer,
    ) -> Option<NativeTransientAllocation> {
        None
    }

    /// Create one graph-owned buffer allocation.
    fn create_transient_buffer(
        &self,
        desc: BufferDesc,
    ) -> Result<Self::TransientBuffer, RenderGraphError>;

    /// Destroy a transient texture.
    fn destroy_transient_texture(texture: Self::TransientTexture);

    /// Destroy a graph-owned buffer allocation.
    fn destroy_transient_buffer(buffer: Self::TransientBuffer);

    /// Return the byte capacity of a backend buffer.
    fn transient_buffer_size(buffer: &Self::TransientBuffer) -> u64;

    /// Return the declared allocation properties of a backend buffer.
    fn buffer_desc(buffer: &Self::TransientBuffer) -> BufferDesc;

    /// Resolve an externally created buffer handle for graph imports.
    fn buffer_by_handle(
        &self,
        handle: crate::handle::BufferHandle,
    ) -> Option<&Self::TransientBuffer>;

    /// Byte offset of a graph-visible slice within its native allocation.
    fn buffer_offset(_buffer: &Self::TransientBuffer) -> u64 {
        0
    }

    /// Prior canonical access scopes of this resolved native buffer slice.
    fn graph_buffer_previous_accesses(
        &self,
        _buffer: &Self::TransientBuffer,
    ) -> Vec<super::BufferAccess> {
        Vec::new()
    }

    /// Retain scopes for subsequent submissions, including other frame graphs.
    fn record_graph_buffer_accesses(
        &self,
        _buffer: &Self::TransientBuffer,
        _accesses: &[super::BufferAccess],
    ) {
    }

    /// Warm a reflected pipeline before any frame command encoding begins.
    fn prepare_compute_pipeline(
        &mut self,
        descriptor: &super::compute::ComputePipelineDesc,
    ) -> Result<(), RenderGraphError> {
        Err(super::error::GraphValidationError::InvalidComputeCommand {
            pass: descriptor.entry.clone(),
            reason: "Compute shader pipelines are unsupported by this backend".into(),
        }
        .into())
    }

    /// Current frame index (for double-buffered resources).
    fn current_frame(&self) -> usize;

    /// Number of transient texture sets to create (one per frame in flight).
    fn transient_texture_frames() -> usize;

    /// Register a texture with the bindless system, return slot.
    fn register_bindless_texture(
        &mut self,
        texture: &Self::TransientTexture,
    ) -> Result<u32, RenderGraphError>;

    /// Update an existing bindless texture slot with a new texture.
    fn update_bindless_texture(
        &mut self,
        slot: u32,
        texture: &Self::TransientTexture,
    ) -> Result<(), RenderGraphError>;

    /// Get the image format of a transient texture.
    fn transient_texture_format(texture: &Self::TransientTexture) -> ImageFormat;

    /// Get the width and height of a transient texture.
    fn transient_texture_extent(texture: &Self::TransientTexture) -> (u32, u32);

    /// Whether the transient texture is a depth format.
    fn transient_texture_is_depth(texture: &Self::TransientTexture) -> bool;

    /// Get or set the bindless slot stored on a transient texture.
    fn transient_texture_bindless_slot(texture: &Self::TransientTexture) -> Option<u32>;
    fn set_transient_texture_bindless_slot(texture: &mut Self::TransientTexture, slot: u32);

    /// Extract the image view from a transient texture for render pass attachment.
    fn transient_texture_view(texture: &Self::TransientTexture) -> Self::ImageView;

    /// Get the swapchain image view for the current frame's image index.
    fn swapchain_image_view(&self, image_index: u32) -> Self::ImageView;
}

/// Borrowed graph allocation or application-owned imported buffer.
pub enum ResolvedGraphBuffer<'a, B: RenderGraphBackend> {
    Borrowed(&'a B::TransientBuffer),
}

impl<B: RenderGraphBackend> std::ops::Deref for ResolvedGraphBuffer<'_, B> {
    type Target = B::TransientBuffer;
    fn deref(&self) -> &Self::Target {
        match self {
            Self::Borrowed(buffer) => buffer,
        }
    }
}
