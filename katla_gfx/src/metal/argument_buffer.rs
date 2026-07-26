use std::collections::HashSet;
use std::mem::size_of;
use std::panic::AssertUnwindSafe;

use objc2::Message;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::{
    MTLArgumentBuffersTier, MTLArgumentEncoder, MTLBuffer, MTLDevice, MTLFunction, MTLResourceID,
    MTLResourceOptions, MTLTexture,
};

use crate::error::RendererError;

/// Maximum number of textures in the bindless array.
const MAX_BINDLESS_TEXTURES: u32 = 4096;

/// Naga maps Katla's bindless texture array to `[[buffer(9)]]` in MSL.
const BINDLESS_ARGUMENT_BUFFER_INDEX: usize = 9;

/// How texture references are written into the argument buffer.
enum ArgumentBufferEncoding {
    /// Tier 2 argument buffers on macOS 13+ use the equivalent C structure
    /// layout, so texture `MTLResourceID` values can be written directly.
    DirectResourceIds,
    /// Compatibility path for Tier 1 devices with a private argument-buffer ABI.
    Encoder(Retained<ProtocolObject<dyn MTLArgumentEncoder>>),
}

/// Metal bindless texture manager using an argument buffer.
///
/// Texture slots may be registered before a shader is compiled. The backing
/// argument buffer is initialized lazily from the first fragment function so
/// the renderer can choose the encoding path supported by the actual device.
pub(crate) struct MetalBindlessTextureManager {
    textures: Vec<Option<Retained<ProtocolObject<dyn MTLTexture>>>>,
    free_slots: Vec<u32>,
    encoding: Option<ArgumentBufferEncoding>,
    argument_buffer: Option<Retained<ProtocolObject<dyn MTLBuffer>>>,
    default_texture: Option<Retained<ProtocolObject<dyn MTLTexture>>>,
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
            encoding: None,
            argument_buffer: None,
            default_texture: None,
            dirty_slots: HashSet::new(),
        })
    }

    pub(crate) fn set_default_texture(&mut self, texture: &ProtocolObject<dyn MTLTexture>) {
        self.default_texture = Some(texture.retain());
    }

    pub(crate) fn is_initialized(&self) -> bool {
        self.encoding.is_some() && self.argument_buffer.is_some()
    }

    /// Initialize the bindless argument buffer for the device used by `function`.
    ///
    /// Modern Tier 2 devices are encoded directly with `MTLResourceID`. This is
    /// both Metal's documented low-overhead path and the only compatible path on
    /// Apple's virtualized `AppleParavirtDevice`, whose driver raises an
    /// Objective-C exception from both argument-encoder factory APIs.
    ///
    /// Tier 1 keeps the shader-reflection encoder path because its layout is
    /// private. The risky Objective-C message is scoped inside `exception::catch`
    /// so an unsupported implementation returns a normal renderer error.
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
        let device = function.device();

        if device.argumentBuffersSupport() == MTLArgumentBuffersTier::Tier2 {
            let encoded_length = self
                .textures
                .len()
                .checked_mul(size_of::<MTLResourceID>())
                .ok_or_else(|| {
                    RendererError::InitializationFailed(
                        "Metal bindless argument-buffer size overflow".into(),
                    )
                })?;
            let buffer = Self::allocate_buffer(&device, encoded_length)?;
            Self::write_all_resource_ids(
                &buffer,
                &self.textures,
                default_texture.as_ref(),
            )?;

            log::info!(
                "Initialized Metal bindless argument buffer with {} direct resource IDs",
                self.textures.len()
            );
            self.encoding = Some(ArgumentBufferEncoding::DirectResourceIds);
            self.argument_buffer = Some(buffer);
            self.dirty_slots.clear();
            return Ok(());
        }

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

        let buffer = Self::allocate_buffer(&device, encoded_length)?;
        unsafe {
            encoder.setArgumentBuffer_offset(Some(&buffer), 0);
            for (index, slot) in self.textures.iter().enumerate() {
                let texture = slot.as_deref().unwrap_or(default_texture.as_ref());
                encoder.setTexture_atIndex(Some(texture), index);
            }
        }

        log::info!(
            "Initialized Metal Tier 1 bindless argument buffer with reflected shader layout"
        );
        self.encoding = Some(ArgumentBufferEncoding::Encoder(encoder));
        self.argument_buffer = Some(buffer);
        self.dirty_slots.clear();
        Ok(())
    }

    fn allocate_buffer(
        device: &ProtocolObject<dyn MTLDevice>,
        encoded_length: usize,
    ) -> Result<Retained<ProtocolObject<dyn MTLBuffer>>, RendererError> {
        unsafe {
            device.newBufferWithLength_options(
                encoded_length,
                MTLResourceOptions::StorageModeShared,
            )
        }
        .ok_or_else(|| {
            RendererError::ResourceCreationFailed(format!(
                "Failed to allocate {} bytes for the Metal bindless argument buffer",
                encoded_length
            ))
        })
    }

    fn texture_resource_id(
        texture: &ProtocolObject<dyn MTLTexture>,
    ) -> Result<u64, RendererError> {
        objc2::exception::catch(AssertUnwindSafe(|| texture.gpuResourceID().to_raw())).map_err(
            |exception| {
                RendererError::InitializationFailed(format!(
                    "Metal texture does not expose a GPU resource ID for direct argument-buffer encoding: {:?}",
                    exception
                ))
            },
        )
    }

    fn write_all_resource_ids(
        buffer: &ProtocolObject<dyn MTLBuffer>,
        textures: &[Option<Retained<ProtocolObject<dyn MTLTexture>>>],
        default_texture: &ProtocolObject<dyn MTLTexture>,
    ) -> Result<(), RendererError> {
        let required_length = textures
            .len()
            .checked_mul(size_of::<MTLResourceID>())
            .ok_or_else(|| {
                RendererError::InitializationFailed(
                    "Metal bindless argument-buffer size overflow".into(),
                )
            })?;
        if buffer.length() < required_length {
            return Err(RendererError::InitializationFailed(format!(
                "Metal bindless argument buffer is {} bytes but requires {} bytes",
                buffer.length(),
                required_length
            )));
        }

        let default_id = Self::texture_resource_id(default_texture)?;
        let destination = buffer.contents().as_ptr().cast::<u64>();
        for (index, texture) in textures.iter().enumerate() {
            let id = match texture.as_deref() {
                Some(texture) => Self::texture_resource_id(texture)?,
                None => default_id,
            };
            unsafe {
                destination.add(index).write(id);
            }
        }
        Ok(())
    }

    fn write_resource_id(
        buffer: &ProtocolObject<dyn MTLBuffer>,
        slot: u32,
        texture: &ProtocolObject<dyn MTLTexture>,
    ) -> Result<(), RendererError> {
        let offset = slot as usize * size_of::<MTLResourceID>();
        if offset + size_of::<MTLResourceID>() > buffer.length() {
            return Err(RendererError::InvalidOperation(format!(
                "Bindless slot {} exceeds the Metal argument buffer",
                slot
            )));
        }
        let id = Self::texture_resource_id(texture)?;
        unsafe {
            buffer.contents().as_ptr().cast::<u64>().add(slot as usize).write(id);
        }
        Ok(())
    }

    pub(crate) fn register_texture(
        &mut self,
        texture: &ProtocolObject<dyn MTLTexture>,
    ) -> Result<u32, RendererError> {
        let slot = self.free_slots.pop().ok_or_else(|| {
            RendererError::InvalidOperation("No free bindless texture slots available".into())
        })?;
        self.textures[slot as usize] = Some(texture.retain());
        self.dirty_slots.insert(slot);
        Ok(slot)
    }

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
        self.dirty_slots.insert(slot);
        Ok(())
    }

    pub(crate) fn release_slot(&mut self, slot: u32) -> bool {
        if slot as usize >= self.textures.len() || self.textures[slot as usize].is_none() {
            return false;
        }
        self.textures[slot as usize] = None;
        self.dirty_slots.insert(slot);
        self.free_slots.push(slot);
        true
    }

    /// Re-encode only slots changed since the previous frame.
    pub(crate) fn flush_argument_buffer(&mut self) {
        if self.dirty_slots.is_empty() {
            return;
        }
        let Some(encoding) = self.encoding.as_ref() else {
            return;
        };
        let Some(buffer) = self.argument_buffer.as_deref() else {
            return;
        };
        let Some(default) = self.default_texture.as_deref() else {
            return;
        };

        let result = match encoding {
            ArgumentBufferEncoding::DirectResourceIds => {
                for &slot in &self.dirty_slots {
                    let texture = self.textures[slot as usize].as_deref().unwrap_or(default);
                    if let Err(error) = Self::write_resource_id(buffer, slot, texture) {
                        log::error!("Failed to update Metal bindless slot {}: {}", slot, error);
                        return;
                    }
                }
                Ok(())
            }
            ArgumentBufferEncoding::Encoder(encoder) => {
                unsafe {
                    encoder.setArgumentBuffer_offset(Some(buffer), 0);
                    for &slot in &self.dirty_slots {
                        let texture = self.textures[slot as usize].as_deref().unwrap_or(default);
                        encoder.setTexture_atIndex(Some(texture), slot as usize);
                    }
                }
                Ok(())
            }
        };

        if result.is_ok() {
            self.dirty_slots.clear();
        }
    }

    pub(crate) fn argument_buffer(&self) -> Option<&ProtocolObject<dyn MTLBuffer>> {
        self.argument_buffer.as_deref()
    }

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

    #[test]
    fn direct_layout_matches_resource_id_array_size() {
        assert_eq!(size_of::<MTLResourceID>(), size_of::<u64>());
        assert_eq!(
            MAX_BINDLESS_TEXTURES as usize * size_of::<MTLResourceID>(),
            32 * 1024
        );
    }
}
