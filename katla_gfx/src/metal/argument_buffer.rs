use std::collections::{BTreeMap, VecDeque};
use std::rc::Rc;

use super::residency::MetalResidency;
use std::mem::size_of;
use std::panic::AssertUnwindSafe;

use objc2::Message;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::{
    MTLArgumentBuffersTier, MTLBuffer, MTLDevice, MTLResource, MTLResourceID, MTLResourceOptions,
    MTLStorageMode, MTLTexture,
};

use crate::error::RendererError;

/// Maximum number of textures in the bindless array.
const MAX_BINDLESS_TEXTURES: u32 = 4096;

/// Immutable bindless data and native residency retained by each submission.
pub(crate) struct BindlessSnapshot {
    pub(crate) buffer: Retained<ProtocolObject<dyn MTLBuffer>>,
    pub(crate) residency: Rc<MetalResidency>,
    pub(crate) generation: u64,
}

struct DirtySlots {
    slots: Vec<u32>,
    marked: Vec<bool>,
}

impl DirtySlots {
    fn new(capacity: usize) -> Self {
        Self {
            slots: Vec::new(),
            marked: vec![false; capacity],
        }
    }
    fn insert(&mut self, slot: u32) {
        if !self.marked[slot as usize] {
            self.marked[slot as usize] = true;
            self.slots.push(slot);
        }
    }
    fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }
    fn clear(&mut self) {
        for slot in self.slots.drain(..) {
            self.marked[slot as usize] = false;
        }
    }
}

struct CachedSnapshot {
    transient_bindings: Vec<(u32, u64)>,
    snapshot: Rc<BindlessSnapshot>,
}

/// Owns immutable bindless snapshots consumed through Metal 4 argument tables.
pub(crate) struct MetalBindlessTextureManager {
    textures: Vec<Option<Retained<ProtocolObject<dyn MTLTexture>>>>,
    free_slots: Vec<u32>,
    snapshot: Option<Rc<BindlessSnapshot>>,
    device: Option<Retained<ProtocolObject<dyn MTLDevice>>>,
    generation: u64,
    default_texture: Option<Retained<ProtocolObject<dyn MTLTexture>>>,
    dirty_slots: DirtySlots,
    transient_slots: BTreeMap<u32, u64>,
    slot_snapshots: VecDeque<CachedSnapshot>,
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
            snapshot: None,
            device: None,
            generation: 0,
            default_texture: None,
            dirty_slots: DirtySlots::new(capacity as usize),
            transient_slots: BTreeMap::new(),
            slot_snapshots: VecDeque::new(),
        })
    }

    pub(crate) fn set_default_texture(&mut self, texture: &ProtocolObject<dyn MTLTexture>) {
        self.slot_snapshots.clear();
        self.default_texture = Some(texture.retain());
        for (slot, texture) in self.textures.iter().enumerate() {
            if texture.is_none() {
                self.dirty_slots.insert(slot as u32);
            }
        }
    }

    pub(crate) fn is_initialized(&self) -> bool {
        self.snapshot.is_some()
    }

    pub(crate) fn unsupported_device_reason(
        device: &ProtocolObject<dyn MTLDevice>,
    ) -> Option<String> {
        (device.argumentBuffersSupport() != MTLArgumentBuffersTier::Tier2).then(|| {
            format!(
                "Metal device '{}' cannot execute Katla's Metal 4 bindless ABI",
                device.name()
            )
        })
    }

    pub(crate) fn initialize(
        &mut self,
        device: &ProtocolObject<dyn MTLDevice>,
    ) -> Result<(), RendererError> {
        if self.is_initialized() {
            return Ok(());
        }
        if let Some(reason) = Self::unsupported_device_reason(device) {
            return Err(RendererError::UnsupportedFeature(reason));
        }
        self.device = Some(device.retain());
        self.rebuild_snapshot()
    }

    fn rebuild_snapshot(&mut self) -> Result<(), RendererError> {
        let key: Vec<_> = self
            .transient_slots
            .iter()
            .map(|(&slot, &id)| (slot, id))
            .collect();
        if let Some(cached) = self
            .slot_snapshots
            .iter()
            .find(|candidate| candidate.transient_bindings == key)
        {
            self.snapshot = Some(cached.snapshot.clone());
            self.dirty_slots.clear();
            return Ok(());
        }
        let device = self.device.as_deref().ok_or_else(|| {
            RendererError::InitializationFailed("Bindless device is absent".into())
        })?;
        let default = self.default_texture.as_deref().ok_or_else(|| {
            RendererError::InitializationFailed("Bindless default texture is absent".into())
        })?;
        let buffer =
            Self::allocate_buffer(device, self.textures.len() * size_of::<MTLResourceID>())?;
        if let Some(previous) = self.snapshot.as_ref() {
            unsafe {
                std::ptr::copy_nonoverlapping(
                    previous.buffer.contents().as_ptr().cast::<u8>(),
                    buffer.contents().as_ptr().cast::<u8>(),
                    buffer.length(),
                );
            }
            for &slot in &self.dirty_slots.slots {
                Self::write_resource_id(
                    &buffer,
                    slot,
                    self.textures[slot as usize].as_deref().unwrap_or(default),
                )?;
            }
        } else {
            Self::write_all_resource_ids(&buffer, &self.textures, default)?;
        }
        let residency = Rc::new(MetalResidency::new(device, "persistent_bindless")?);
        residency.add_buffer(&buffer)?;
        residency.add_texture(default)?;
        for texture in self.textures.iter().flatten() {
            residency.add_texture(texture)?;
        }
        residency.commit();
        self.generation = self.generation.checked_add(1).ok_or_else(|| {
            RendererError::InvalidOperation("Bindless snapshot generation exhausted".into())
        })?;
        self.snapshot = Some(Rc::new(BindlessSnapshot {
            buffer,
            residency,
            generation: self.generation,
        }));
        if log::log_enabled!(log::Level::Debug)
            && let Some(snapshot) = &self.snapshot
        {
            let diagnostics = snapshot.residency.diagnostics();
            log::debug!(
                "Published bindless snapshot {}: {} allocations, {} estimated resident bytes",
                snapshot.generation,
                diagnostics.allocation_count,
                diagnostics.estimated_resident_bytes
            );
        }
        if !key.is_empty() {
            if let Some(snapshot) = self.snapshot.clone() {
                self.slot_snapshots.push_back(CachedSnapshot {
                    transient_bindings: key,
                    snapshot,
                });
            }
            while self.slot_snapshots.len() > 3 {
                self.slot_snapshots.pop_front();
            }
        }
        self.dirty_slots.clear();
        Ok(())
    }

    fn allocate_buffer(
        device: &ProtocolObject<dyn MTLDevice>,
        encoded_length: usize,
    ) -> Result<Retained<ProtocolObject<dyn MTLBuffer>>, RendererError> {
        device
            .newBufferWithLength_options(encoded_length, MTLResourceOptions::StorageModeShared)
            .ok_or_else(|| {
                RendererError::ResourceCreationFailed(format!(
                    "Failed to allocate {} bytes for the Metal bindless argument buffer",
                    encoded_length
                ))
            })
    }

    fn texture_resource_id(texture: &ProtocolObject<dyn MTLTexture>) -> Result<u64, RendererError> {
        objc2::exception::catch(AssertUnwindSafe(|| texture.gpuResourceID().to_raw())).map_err(
            |exception| {
                RendererError::UnsupportedFeature(format!(
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
            buffer
                .contents()
                .as_ptr()
                .cast::<u64>()
                .add(slot as usize)
                .write(id);
        }
        Ok(())
    }

    pub(crate) fn register_texture(
        &mut self,
        texture: &ProtocolObject<dyn MTLTexture>,
    ) -> Result<u32, RendererError> {
        if texture.storageMode() == MTLStorageMode::Memoryless {
            return Err(RendererError::InvalidOperation(
                "Tile-local memoryless attachments cannot enter shader-visible bindless tables"
                    .into(),
            ));
        }
        let slot = self.free_slots.pop().ok_or_else(|| {
            RendererError::InvalidOperation("No free bindless texture slots available".into())
        })?;
        self.slot_snapshots.clear();
        self.textures[slot as usize] = Some(texture.retain());
        self.dirty_slots.insert(slot);
        Ok(slot)
    }

    /// Allocate one graph-visible bindless address shared by its frame slots.
    pub(crate) fn register_transient_texture(
        &mut self,
        texture: &ProtocolObject<dyn MTLTexture>,
    ) -> Result<u32, RendererError> {
        let id = Self::texture_resource_id(texture)?;
        let slot = self.register_texture(texture)?;
        self.transient_slots.insert(slot, id);
        Ok(slot)
    }

    /// Select the active graph allocation without invalidating stable slot snapshots.
    pub(crate) fn update_transient_texture(
        &mut self,
        slot: u32,
        texture: &ProtocolObject<dyn MTLTexture>,
    ) -> Result<(), RendererError> {
        if texture.storageMode() == MTLStorageMode::Memoryless {
            return Err(RendererError::InvalidOperation(
                "Tile-local memoryless attachments cannot enter shader-visible bindless tables"
                    .into(),
            ));
        }
        if !self.transient_slots.contains_key(&slot) {
            return Err(RendererError::InvalidOperation(format!(
                "Bindless slot {slot} is not a graph resource"
            )));
        }
        let id = Self::texture_resource_id(texture)?;
        if self.transient_slots.get(&slot) == Some(&id) {
            return Ok(());
        }
        self.transient_slots.insert(slot, id);
        self.textures[slot as usize] = Some(texture.retain());
        self.dirty_slots.insert(slot);
        Ok(())
    }

    pub(crate) fn release_slot(&mut self, slot: u32) -> bool {
        if slot as usize >= self.textures.len() || self.textures[slot as usize].is_none() {
            return false;
        }
        self.slot_snapshots.clear();
        self.transient_slots.remove(&slot);
        self.textures[slot as usize] = None;
        self.dirty_slots.insert(slot);
        self.free_slots.push(slot);
        true
    }

    /// Publish a replacement snapshot without modifying any in-flight table.
    pub(crate) fn publish_snapshot(&mut self) -> Result<(), RendererError> {
        if self.dirty_slots.is_empty() || self.device.is_none() {
            return Ok(());
        }
        self.rebuild_snapshot()
    }

    pub(crate) fn snapshot(&self) -> Option<Rc<BindlessSnapshot>> {
        self.snapshot.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rejects_zero_capacity() {
        let result = MetalBindlessTextureManager::new(0);
        assert!(matches!(
            result,
            Err(RendererError::InitializationFailed(message))
                if message.contains("greater than zero")
        ));
    }

    #[test]
    fn test_direct_layout_matches_resource_id_array_size() {
        assert_eq!(size_of::<MTLResourceID>(), size_of::<u64>());
        assert_eq!(
            MAX_BINDLESS_TEXTURES as usize * size_of::<MTLResourceID>(),
            32 * 1024
        );
    }
    fn texture(device: &ProtocolObject<dyn MTLDevice>) -> Retained<ProtocolObject<dyn MTLTexture>> {
        let descriptor = objc2_metal::MTLTextureDescriptor::new();
        descriptor.setPixelFormat(objc2_metal::MTLPixelFormat::RGBA8Unorm);
        unsafe {
            descriptor.setWidth(1);
            descriptor.setHeight(1);
        }
        descriptor.setUsage(objc2_metal::MTLTextureUsage::ShaderRead);
        device.newTextureWithDescriptor(&descriptor).unwrap()
    }

    fn snapshot_id(snapshot: &BindlessSnapshot, slot: u32) -> u64 {
        unsafe {
            snapshot
                .buffer
                .contents()
                .as_ptr()
                .cast::<u64>()
                .add(slot as usize)
                .read()
        }
    }

    #[test]
    fn test_streamed_replacement_and_slot_reuse_preserve_inflight_snapshot() {
        let device = objc2_metal::MTLCreateSystemDefaultDevice().unwrap();
        let default = texture(&device);
        let first = texture(&device);
        let replacement = texture(&device);
        let mut manager = MetalBindlessTextureManager::new(4).unwrap();
        manager.set_default_texture(&default);
        manager.device = Some(device);
        let slot = manager.register_texture(&first).unwrap();
        manager.rebuild_snapshot().unwrap();
        let inflight = manager.snapshot().unwrap();
        assert!(manager.release_slot(slot));
        assert_eq!(manager.register_texture(&replacement).unwrap(), slot);
        manager.rebuild_snapshot().unwrap();
        let updated = manager.snapshot().unwrap();
        assert_eq!(snapshot_id(&inflight, slot), first.gpuResourceID().to_raw());
        assert_eq!(
            snapshot_id(&updated, slot),
            replacement.gpuResourceID().to_raw()
        );
        assert_ne!(inflight.generation, updated.generation);
        inflight.residency.validate_texture(&first).unwrap();
        assert!(inflight.residency.validate_texture(&replacement).is_err());
        updated.residency.validate_texture(&replacement).unwrap();
        assert!(updated.residency.validate_texture(&first).is_err());
        assert!(manager.release_slot(slot));
        assert!(!manager.release_slot(slot));
        manager.rebuild_snapshot().unwrap();
        assert_eq!(
            snapshot_id(&manager.snapshot().unwrap(), slot),
            default.gpuResourceID().to_raw()
        );
        assert_eq!(manager.register_texture(&first).unwrap(), slot);
        manager.rebuild_snapshot().unwrap();
        assert_eq!(
            snapshot_id(&updated, slot),
            replacement.gpuResourceID().to_raw()
        );
        assert_eq!(snapshot_id(&inflight, slot), first.gpuResourceID().to_raw());
    }

    #[test]
    fn test_non_graph_slot_update_is_rejected() {
        let device = objc2_metal::MTLCreateSystemDefaultDevice().unwrap();
        let mut manager = MetalBindlessTextureManager::new(2).unwrap();
        assert!(
            manager
                .update_transient_texture(0, &texture(&device))
                .is_err()
        );
    }

    #[test]
    fn test_graph_slot_snapshot_reuse_and_streaming_invalidation() {
        let device = objc2_metal::MTLCreateSystemDefaultDevice().unwrap();
        let textures: Vec<_> = (0..3).map(|_| texture(&device)).collect();
        let mut manager = MetalBindlessTextureManager::new(8).unwrap();
        manager.set_default_texture(&texture(&device));
        manager.device = Some(device.clone());
        let slot = manager.register_transient_texture(&textures[0]).unwrap();
        let mut snapshots = Vec::new();
        for texture in &textures {
            manager.update_transient_texture(slot, texture).unwrap();
            manager.publish_snapshot().unwrap();
            snapshots.push(manager.snapshot().unwrap());
        }
        for (index, texture) in textures.iter().enumerate() {
            manager.update_transient_texture(slot, texture).unwrap();
            manager.publish_snapshot().unwrap();
            assert!(Rc::ptr_eq(&manager.snapshot().unwrap(), &snapshots[index]));
            assert_eq!(
                snapshot_id(&snapshots[index], slot),
                texture.gpuResourceID().to_raw()
            );
        }
        let streamed = texture(&device);
        let persistent_slot = manager.register_texture(&streamed).unwrap();
        manager.publish_snapshot().unwrap();
        let current = manager.snapshot().unwrap();
        assert!(
            !snapshots
                .iter()
                .any(|previous| Rc::ptr_eq(previous, &current))
        );
        assert_eq!(
            snapshot_id(&current, persistent_slot),
            streamed.gpuResourceID().to_raw()
        );
        current.residency.validate_texture(&streamed).unwrap();
        for previous in &snapshots {
            assert!(previous.residency.validate_texture(&streamed).is_err());
        }
    }

    #[test]
    #[ignore = "native encoder binding setup benchmark"]
    fn test_benchmark_encoder_setup_registered_texture_scaling() {
        use objc2_metal::{MTL4ArgumentTable, MTL4ArgumentTableDescriptor};
        let device = objc2_metal::MTLCreateSystemDefaultDevice().unwrap();
        for count in [1, 64, 512, 4096] {
            let mut manager = MetalBindlessTextureManager::new(count).unwrap();
            manager.set_default_texture(&texture(&device));
            manager.device = Some(device.clone());
            for _ in 0..count {
                manager.register_texture(&texture(&device)).unwrap();
            }
            manager.rebuild_snapshot().unwrap();
            let descriptor = MTL4ArgumentTableDescriptor::new();
            descriptor.setMaxBufferBindCount(10);
            descriptor.setInitializeBindings(true);
            let table = device
                .newArgumentTableWithDescriptor_error(&descriptor)
                .unwrap();
            let mut measurements = Vec::new();
            for _ in 0..15 {
                let started = std::time::Instant::now();
                for _ in 0..100_000 {
                    let snapshot = std::hint::black_box(manager.snapshot().unwrap());
                    snapshot
                        .residency
                        .validate_buffer(&snapshot.buffer)
                        .unwrap();
                    unsafe {
                        table.setAddress_atIndex(snapshot.buffer.gpuAddress(), 9);
                    }
                    std::hint::black_box(snapshot.residency.native());
                }
                measurements.push(started.elapsed().as_nanos() as f64 / 100_000.0);
            }
            measurements.sort_by(f64::total_cmp);
            println!(
                "registered_textures={count} native_binding_setup_ns median={:.1} p95={:.1}",
                measurements[7], measurements[14]
            );
        }
    }
    #[test]
    #[ignore = "native frame-slot snapshot publication benchmark"]
    fn test_benchmark_slot_publication_registered_texture_scaling() {
        let device = objc2_metal::MTLCreateSystemDefaultDevice().unwrap();
        for count in [1, 64, 512, 4096] {
            let mut manager = MetalBindlessTextureManager::new(count).unwrap();
            manager.set_default_texture(&texture(&device));
            manager.device = Some(device.clone());
            let graph_textures: Vec<_> = (0..3).map(|_| texture(&device)).collect();
            let slot = manager
                .register_transient_texture(&graph_textures[0])
                .unwrap();
            for _ in 1..count {
                manager.register_texture(&texture(&device)).unwrap();
            }
            for texture in &graph_textures {
                manager.update_transient_texture(slot, texture).unwrap();
                manager.publish_snapshot().unwrap();
            }
            let mut measurements = Vec::new();
            for _ in 0..15 {
                let started = std::time::Instant::now();
                for iteration in 0..100_000 {
                    manager
                        .update_transient_texture(slot, &graph_textures[iteration % 3])
                        .unwrap();
                    manager.publish_snapshot().unwrap();
                    std::hint::black_box(manager.snapshot().unwrap());
                }
                measurements.push(started.elapsed().as_nanos() as f64 / 100_000.0);
            }
            measurements.sort_by(f64::total_cmp);
            println!(
                "registered_textures={count} frame_slot_publication_ns median={:.1} p95={:.1}",
                measurements[7], measurements[14]
            );
        }
    }
}
