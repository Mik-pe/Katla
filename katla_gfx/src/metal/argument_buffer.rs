use std::collections::HashSet;

use objc2::Message;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_foundation::NSArray;
use objc2_metal::{
    MTLArgumentDescriptor, MTLArgumentEncoder, MTLBindingAccess, MTLBuffer, MTLDataType, MTLDevice,
    MTLResourceOptions, MTLTexture, MTLTextureType,
};

use crate::error::RendererError;

/// Maximum number of textures in the bindless array.
const MAX_BINDLESS_TEXTURES: u32 = 4096;

/// Metal bindless texture manager using an argument buffer.
///
/// This is the Metal equivalent of Vulkan's descriptor indexing
/// (binding_array<texture2d>). Textures are stored in a Vec indexed
/// by slot and encoded into a Metal argument buffer that is bound
/// at buffer index 9 for both vertex and fragment stages.
///
/// The generated MSL expects:
/// `constant NagaArgumentBufferWrapper<texture2d<float>>* bindless_textures [[buffer(9)]]`
/// and reads textures as `bindless_textures[idx].inner.sample(...)`.
pub(crate) struct MetalBindlessTextureManager {
    textures: Vec<Option<Retained<ProtocolObject<dyn MTLTexture>>>>,
    /// Stack of free slot indices for O(1) allocation.
    free_slots: Vec<u32>,
    /// Argument encoder for the texture array.
    encoder: Option<Retained<ProtocolObject<dyn MTLArgumentEncoder>>>,
    /// The argument buffer containing all bindless textures.
    argument_buffer: Option<Retained<ProtocolObject<dyn MTLBuffer>>>,
    /// Default (white 1x1) texture used for unused slots.
    default_texture: Option<Retained<ProtocolObject<dyn MTLTexture>>>,
    /// Slots that have been modified since the last flush and need re-encoding.
    dirty_slots: HashSet<u32>,
}

impl MetalBindlessTextureManager {
    pub(crate) fn new(capacity: u32) -> Result<Self, RendererError> {
        let capacity = capacity.min(MAX_BINDLESS_TEXTURES);
        Ok(Self {
            textures: vec![None; capacity as usize],
            free_slots: (0..capacity).rev().collect(),
            encoder: None,
            argument_buffer: None,
            default_texture: None,
            dirty_slots: HashSet::new(),
        })
    }

    /// Initialize the argument buffer with a device and default texture.
    ///
    /// Must be called after `new()`, once a default texture is available.
    /// Encodes all currently registered textures into the argument buffer.
    pub(crate) fn init_argument_buffer(
        &mut self,
        device: &ProtocolObject<dyn MTLDevice>,
        default_texture: &ProtocolObject<dyn MTLTexture>,
    ) {
        self.default_texture = Some(default_texture.retain());

        let capacity = self.textures.len() as u32;

        let arg_desc = {
            let desc = MTLArgumentDescriptor::argumentDescriptor();
            desc.setDataType(MTLDataType::Texture);
            desc.setIndex(0);
            desc.setArrayLength(capacity as usize);
            desc.setTextureType(MTLTextureType::Type2D);
            desc.setAccess(MTLBindingAccess::ReadOnly);
            desc
        };

        let descriptors = NSArray::from_slice(&[&*arg_desc]);

        unsafe {
            let encoder = device
                .newArgumentEncoderWithArguments(&*descriptors)
                .expect("Failed to create argument encoder for bindless textures");

            let encoded_length = encoder.encodedLength();
            let buffer = device
                .newBufferWithLength_options(encoded_length, MTLResourceOptions::StorageModeShared)
                .expect("Failed to create argument buffer for bindless textures");

            encoder.setArgumentBuffer_offset(Some(&buffer), 0);

            for (i, slot) in self.textures.iter().enumerate() {
                let tex = slot.as_ref().map(|t| t.as_ref()).unwrap_or(default_texture);
                encoder.setTexture_atIndex(Some(tex), i);
            }

            self.encoder = Some(encoder);
            self.argument_buffer = Some(buffer);
        }
    }

    /// Allocate a free slot and store the texture.
    ///
    /// Returns the assigned slot index.
    pub(crate) fn register_texture(
        &mut self,
        texture: &ProtocolObject<dyn MTLTexture>,
    ) -> Result<u32, RendererError> {
        let slot = self.free_slots.pop().ok_or_else(|| {
            RendererError::InvalidOperation("No free bindless texture slots available".into())
        })?;
        self.textures[slot as usize] = Some(texture.retain());
        self.mark_dirty(slot);
        Ok(slot)
    }

    /// Update an existing slot with a new texture (keeps the same slot index).
    pub(crate) fn update_texture(
        &mut self,
        slot: u32,
        texture: &ProtocolObject<dyn MTLTexture>,
    ) -> Result<(), RendererError> {
        if slot as usize >= self.textures.len() {
            return Err(RendererError::InvalidOperation(format!(
                "Bindless slot {} out of bounds",
                slot
            )));
        }
        self.textures[slot as usize] = Some(texture.retain());
        self.mark_dirty(slot);
        Ok(())
    }

    /// Release a slot, making it available for reuse.
    ///
    /// Returns `true` if the slot was occupied and released.
    pub(crate) fn release_slot(&mut self, slot: u32) -> bool {
        if slot as usize >= self.textures.len() {
            return false;
        }
        if self.textures[slot as usize].is_none() {
            return false;
        }
        self.textures[slot as usize] = None;
        self.mark_dirty(slot);
        self.free_slots.push(slot);
        true
    }

    /// Mark a slot as needing re-encoding in the next flush.
    fn mark_dirty(&mut self, slot: u32) {
        self.dirty_slots.insert(slot);
    }

    /// Re-encode only the dirty slots into the argument buffer.
    ///
    /// Call once per frame after the CPU-GPU sync point. Only slots that
    /// changed since the last flush are re-encoded, avoiding O(N) work
    /// when the texture set is stable.
    pub(crate) fn flush_argument_buffer(&mut self) {
        if self.dirty_slots.is_empty() {
            return;
        }
        let Some(ref encoder) = self.encoder else {
            return;
        };
        let Some(ref buffer) = self.argument_buffer else {
            return;
        };
        let Some(default) = self.default_texture.as_deref() else {
            return;
        };
        unsafe {
            encoder.setArgumentBuffer_offset(Some(buffer), 0);
            for &slot in &self.dirty_slots {
                let tex = self.textures[slot as usize]
                    .as_ref()
                    .map(|t| t.as_ref())
                    .unwrap_or(default);
                encoder.setTexture_atIndex(Some(tex), slot as usize);
            }
        }
        self.dirty_slots.clear();
    }

    /// Get the argument buffer for binding at buffer index 9.
    pub(crate) fn argument_buffer(&self) -> Option<&ProtocolObject<dyn MTLBuffer>> {
        self.argument_buffer.as_deref()
    }

    /// Iterate over all registered textures for `useResource` calls.
    pub(crate) fn registered_textures(
        &self,
    ) -> impl Iterator<Item = &ProtocolObject<dyn MTLTexture>> {
        self.textures
            .iter()
            .filter_map(|opt| opt.as_deref())
            .chain(self.default_texture.as_deref().into_iter())
    }
}
