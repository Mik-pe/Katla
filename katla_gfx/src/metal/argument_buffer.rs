use std::collections::HashSet;
use std::panic::AssertUnwindSafe;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::{
    MTLArgumentEncoder, MTLBuffer, MTLDevice, MTLFunction, MTLResourceOptions, MTLTexture,
};

use crate::error::RendererError;

/// Maximum number of textures in the bindless array.
const MAX_BINDLESS_TEXTURES: u32 = 4096;

/// Naga maps Katla's bindless texture array to `[[buffer(9)]]` in MSL.
const BINDLESS_ARGUMENT_BUFFER_INDEX: usize = 9;

/// Metal bindless texture manager using an argument buffer.
///
/// This is the Metal equivalent of Vulkan's descriptor indexing
/// (`binding_array<texture2d>`). Textures are stored in a Vec indexed
/// by slot and encoded into a Metal argument buffer that is bound
/// at buffer index 9 for both vertex and fragment stages.
///
/// The argument-buffer layout is created lazily from an actual compiled
/// `MTLFunction`. Creating an arbitrary layout through
/// `MTLDevice::newArgumentEncoderWithArguments` is not supported by every Metal
/// implementation (notably Apple's virtualized `AppleParavirtDevice`) and may
/// raise an Objective-C exception instead of returning an error.
pub(crate) struct MetalBindlessTextureManager {
    textures: Vec<Option<Retained<ProtocolObject<dyn MTLTexture>>>>,
    /// Stack of free slot indices for O(1) allocation.
    free_slots: Vec<u32>,
    /// Argument encoder derived from the compiled shader's buffer-9 layout.
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
        if capacity == 0 {
            return Err(RendererError::InitializationFailed(
                "Metal bindless texture capacity must be greater than zero".into(),
            ));
        }

        Ok(Self {
            textures: vec![None; capacity as usize],
            free_slots: (0..capacity).rev().collect(),
            encoder: None,
            argument_buffer: None,
            default_texture: None,
            dirty_slots: HashSet::new(),
        })
    }

    /// Set the texture used to populate unoccupied bindless slots.
    ///
    /// Texture registration is allowed before the shader layout is available.
    /// Once [`Self::initialize_from_function`] succeeds, every existing slot is
    /// encoded in one deterministic pass.
    pub(crate) fn set_default_texture(
        &mut self,
        texture: &ProtocolObject<dyn MTLTexture>,
    ) {
        self.default_texture = Some(texture.retain());
    }

    pub(crate) fn is_initialized(&self) -> bool {
        self.encoder.is_some() && self.argument_buffer.is_some()
    }

    /// Initialize the bindless argument buffer from the actual shader layout.
    ///
    /// `newArgumentEncoderWithBufferIndex` is the shader-reflection path: Metal
    /// owns the concrete ABI for the generated argument-buffer struct and returns
    /// an encoder with exactly the layout expected at buffer index 9.
    ///
    /// The single risky Objective-C message is wrapped in a scoped exception
    /// boundary. Unsupported devices therefore produce a normal `RendererError`
    /// rather than unwinding a foreign exception through Rust and aborting the
    /// process.
    pub(crate) fn initialize_from_function(
        &mut self,
        function: &ProtocolObject<dyn MTLFunction>,
    ) -> Result<(), RendererError> {
        if self.is_initialized() {
            return Ok(());
        }

        let default_texture = self.default_texture.clone().ok_or_else(|| {
            RendererError::InitializationFailed(
                "Metal bindless argument buffer has no default texture".into(),
            )
        })?;

        let encoder = objc2::exception::catch(AssertUnwindSafe(|| unsafe {
            function.newArgumentEncoderWithBufferIndex(BINDLESS_ARGUMENT_BUFFER_INDEX)
        }))
        .map_err(|exception| {
            RendererError::InitializationFailed(format!(
                "Metal device cannot create the bindless shader argument layout at buffer {}: {:?}",
                BINDLESS_ARGUMENT_BUFFER_INDEX, exception
            ))
        })?;

        let encoded_length = encoder.encodedLength();
        if encoded_length == 0 {
            return Err(RendererError::InitializationFailed(format!(
                "Metal shader reported an empty bindless argument layout at buffer {}",
                BINDLESS_ARGUMENT_BUFFER_INDEX
            )));
        }

        let device = function.device();
        let buffer = device
            .newBufferWithLength_options(encoded_length, MTLResourceOptions::StorageModeShared)
            .ok_or_else(|| {
                RendererError::ResourceCreationFailed(format!(
                    "Failed to allocate {} bytes for the Metal bindless argument buffer",
                    encoded_length
                ))
            })?;

        unsafe {
            encoder.setArgumentBuffer_offset(Some(&buffer), 0);
            for (index, slot) in self.textures.iter().enumerate() {
                let texture = slot
                    .as_deref()
                    .unwrap_or(default_texture.as_ref());
                encoder.setTexture_atIndex(Some(texture), index);
            }
        }

        self.encoder = Some(encoder);
        self.argument_buffer = Some(buffer);
        self.dirty_slots.clear();
        Ok(())
    }

    /// Allocate a free slot and store the texture.
    ///
    /// Returns the assigned slot index. Registration may happen before the
    /// argument-buffer layout is initialized; the slot is encoded during lazy
    /// initialization or the next flush.
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
    /// when the texture set is stable. Before lazy initialization this is an
    /// intentional no-op; all registered slots are encoded by
    /// [`Self::initialize_from_function`].
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
                let texture = self.textures[slot as usize]
                    .as_deref()
                    .unwrap_or(default);
                encoder.setTexture_atIndex(Some(texture), slot as usize);
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
            .filter_map(|texture| texture.as_deref())
            .chain(self.default_texture.as_deref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_zero_capacity() {
        let result = MetalBindlessTextureManager::new(0);
        assert!(matches!(
            result,
            Err(RendererError::InitializationFailed(message))
                if message.contains("greater than zero")
        ));
    }
}
