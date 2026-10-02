//! Submission-scoped native residency with retained allocation ownership.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use objc2::Message;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_foundation::NSString;
use objc2_metal::{
    MTLAllocation, MTLBuffer, MTLDevice, MTLHeap, MTLResidencySet, MTLResidencySetDescriptor,
    MTLResource, MTLStorageMode, MTLTexture,
};

use crate::error::RendererError;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) struct ResidencyDiagnostics {
    pub(crate) label: String,
    pub(crate) allocation_count: usize,
    pub(crate) estimated_resident_bytes: u64,
    pub(crate) sealed: bool,
    pub(crate) members: Vec<ResidencyMember>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) struct ResidencyMember {
    pub(crate) id: usize,
    pub(crate) kind: &'static str,
    pub(crate) estimated_bytes: u64,
}

struct RetainedAllocation {
    _allocation: Retained<ProtocolObject<dyn MTLAllocation>>,
    member: ResidencyMember,
}

/// An immutable-after-commit residency set. Its owner must retain it until the
/// exact submission completes; neither queue-global lifetime nor frame counts
/// substitute for command completion.
pub(crate) struct MetalResidency {
    native: Retained<ProtocolObject<dyn MTLResidencySet>>,
    allocations: RefCell<HashMap<usize, RetainedAllocation>>,
    bytes: Cell<u64>,
    sealed: Cell<bool>,
    label: String,
}

impl MetalResidency {
    pub(crate) fn new(
        device: &ProtocolObject<dyn MTLDevice>,
        label: &str,
    ) -> Result<Self, RendererError> {
        let descriptor = MTLResidencySetDescriptor::new();
        descriptor.setLabel(Some(&NSString::from_str(label)));
        let native = device
            .newResidencySetWithDescriptor_error(&descriptor)
            .map_err(|error| {
                RendererError::ResourceCreationFailed(format!(
                    "Metal residency set '{label}': {}",
                    error.localizedDescription()
                ))
            })?;
        Ok(Self {
            native,
            allocations: RefCell::new(HashMap::new()),
            bytes: Cell::new(0),
            sealed: Cell::new(false),
            label: label.into(),
        })
    }

    fn key(allocation: &ProtocolObject<dyn MTLAllocation>) -> usize {
        allocation as *const _ as usize
    }

    fn add_allocation(
        &self,
        allocation: &ProtocolObject<dyn MTLAllocation>,
        bytes: u64,
        kind: &'static str,
    ) -> Result<(), RendererError> {
        let mut allocations = self.allocations.borrow_mut();
        let key = Self::key(allocation);
        if allocations.contains_key(&key) {
            return Ok(());
        }
        if self.sealed.get() {
            return Err(RendererError::InvalidOperation(format!(
                "Cannot modify committed residency set '{}'",
                self.label
            )));
        }
        self.native.addAllocation(allocation);
        let id = allocations.len();
        allocations.insert(
            key,
            RetainedAllocation {
                _allocation: allocation.retain(),
                member: ResidencyMember {
                    id,
                    kind,
                    estimated_bytes: bytes,
                },
            },
        );
        self.bytes.set(self.bytes.get().saturating_add(bytes));
        Ok(())
    }

    pub(crate) fn add_heap(&self, heap: &ProtocolObject<dyn MTLHeap>) -> Result<(), RendererError> {
        self.add_allocation(ProtocolObject::from_ref(heap), heap.size() as u64, "heap")
    }

    pub(crate) fn add_buffer(
        &self,
        buffer: &ProtocolObject<dyn MTLBuffer>,
    ) -> Result<(), RendererError> {
        let heap = buffer.heap();
        if let Some(heap) = heap.as_deref() {
            self.add_heap(heap)?;
        }
        self.add_allocation(
            ProtocolObject::from_ref(buffer),
            if heap.is_some() {
                0
            } else {
                MTLResource::allocatedSize(buffer) as u64
            },
            "buffer",
        )?;
        self.validate_buffer(buffer)
    }

    pub(crate) fn add_texture(
        &self,
        texture: &ProtocolObject<dyn MTLTexture>,
    ) -> Result<(), RendererError> {
        if texture.storageMode() == MTLStorageMode::Memoryless {
            return Ok(());
        }
        let heap = texture.heap();
        if let Some(heap) = heap.as_deref() {
            self.add_heap(heap)?;
        }
        self.add_allocation(
            ProtocolObject::from_ref(texture),
            if heap.is_some() {
                0
            } else {
                MTLResource::allocatedSize(texture) as u64
            },
            "texture",
        )?;
        self.validate_texture(texture)
    }

    pub(crate) fn validate_buffer(
        &self,
        buffer: &ProtocolObject<dyn MTLBuffer>,
    ) -> Result<(), RendererError> {
        self.validate_allocation(ProtocolObject::from_ref(buffer))
    }

    pub(crate) fn validate_texture(
        &self,
        texture: &ProtocolObject<dyn MTLTexture>,
    ) -> Result<(), RendererError> {
        if texture.storageMode() == MTLStorageMode::Memoryless {
            return Ok(());
        }
        self.validate_allocation(ProtocolObject::from_ref(texture))
    }

    fn validate_allocation(
        &self,
        allocation: &ProtocolObject<dyn MTLAllocation>,
    ) -> Result<(), RendererError> {
        if !self
            .allocations
            .borrow()
            .contains_key(&Self::key(allocation))
            || !self.native.containsAllocation(allocation)
        {
            return Err(RendererError::InvalidOperation(format!(
                "Command references an allocation absent from residency set '{}'",
                self.label
            )));
        }
        Ok(())
    }

    pub(crate) fn commit(&self) {
        if !self.sealed.replace(true) {
            self.native.commit();
        }
    }

    pub(crate) fn native(&self) -> &ProtocolObject<dyn MTLResidencySet> {
        &self.native
    }

    pub(crate) fn diagnostics(&self) -> ResidencyDiagnostics {
        let mut members: Vec<_> = self
            .allocations
            .borrow()
            .values()
            .map(|allocation| allocation.member.clone())
            .collect();
        members.sort_by_key(|member| member.id);
        ResidencyDiagnostics {
            label: self.label.clone(),
            allocation_count: self.allocations.borrow().len(),
            estimated_resident_bytes: self.bytes.get(),
            sealed: self.sealed.get(),
            members,
        }
    }
}

/// Stable resource-manager residency, republished only at allocation mutations.
#[derive(Clone)]
struct PersistentBufferEntry {
    id: u64,
    buffer: Retained<ProtocolObject<dyn MTLBuffer>>,
}

pub(crate) struct PersistentBufferResidency {
    device: Retained<ProtocolObject<dyn MTLDevice>>,
    label: String,
    buffers: HashMap<usize, PersistentBufferEntry>,
    next_id: u64,
    snapshot: std::rc::Rc<MetalResidency>,
}

impl PersistentBufferResidency {
    pub(crate) fn new(
        device: &ProtocolObject<dyn MTLDevice>,
        label: &str,
    ) -> Result<Self, RendererError> {
        let snapshot = std::rc::Rc::new(MetalResidency::new(device, label)?);
        snapshot.commit();
        Ok(Self {
            device: device.retain(),
            label: label.into(),
            buffers: HashMap::new(),
            next_id: 0,
            snapshot,
        })
    }

    pub(crate) fn replace(
        &mut self,
        removed: &[&ProtocolObject<dyn MTLBuffer>],
        added: &[&ProtocolObject<dyn MTLBuffer>],
    ) -> Result<(), RendererError> {
        let mut staged = self.buffers.clone();
        for buffer in removed {
            staged.remove(&(*buffer as *const _ as usize));
        }
        let mut next_id = self.next_id;
        for buffer in added {
            let key = *buffer as *const _ as usize;
            if let std::collections::hash_map::Entry::Vacant(entry) = staged.entry(key) {
                entry.insert(PersistentBufferEntry {
                    id: next_id,
                    buffer: buffer.retain(),
                });
                next_id = next_id.checked_add(1).ok_or_else(|| {
                    RendererError::InvalidOperation(
                        "Persistent residency identity exhausted".into(),
                    )
                })?;
            }
        }
        let snapshot = std::rc::Rc::new(MetalResidency::new(&self.device, &self.label)?);
        let mut ordered: Vec<_> = staged.values().collect();
        ordered.sort_by_key(|entry| entry.id);
        for entry in ordered {
            snapshot.add_buffer(&entry.buffer)?;
        }
        snapshot.commit();
        self.buffers = staged;
        self.next_id = next_id;
        self.snapshot = snapshot;
        Ok(())
    }

    pub(crate) fn clear(&mut self) -> Result<(), RendererError> {
        let snapshot = std::rc::Rc::new(MetalResidency::new(&self.device, &self.label)?);
        snapshot.commit();
        self.buffers.clear();
        self.snapshot = snapshot;
        Ok(())
    }

    pub(crate) fn snapshot(&self) -> std::rc::Rc<MetalResidency> {
        self.snapshot.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use objc2_metal::{MTLCreateSystemDefaultDevice, MTLResourceOptions};

    #[test]
    fn test_native_residency_deduplicates_and_rejects_missing() {
        let device = MTLCreateSystemDefaultDevice().unwrap();
        let set = MetalResidency::new(&device, "residency_test").unwrap();
        let buffer = device
            .newBufferWithLength_options(256, MTLResourceOptions::StorageModeShared)
            .unwrap();
        let missing = device
            .newBufferWithLength_options(256, MTLResourceOptions::StorageModeShared)
            .unwrap();
        assert!(set.validate_buffer(&buffer).is_err());
        set.add_buffer(&buffer).unwrap();
        set.add_buffer(&buffer).unwrap();
        set.validate_buffer(&buffer).unwrap();
        assert!(set.validate_buffer(&missing).is_err());
        assert_eq!(set.diagnostics().allocation_count, 1);
        assert_eq!(set.native().allocationCount(), 1);
        set.commit();
        assert!(set.add_buffer(&missing).is_err());
        set.add_buffer(&buffer).unwrap();
        assert!(set.diagnostics().sealed);
    }
    #[test]
    fn test_native_heap_residency_retains_owner_and_counts_physical_bytes_once() {
        use objc2_metal::{MTLHeapDescriptor, MTLStorageMode};
        let device = MTLCreateSystemDefaultDevice().unwrap();
        let descriptor = MTLHeapDescriptor::new();
        descriptor.setStorageMode(MTLStorageMode::Private);
        descriptor.setSize(65536);
        let heap = device.newHeapWithDescriptor(&descriptor).unwrap();
        let first = heap
            .newBufferWithLength_options(256, MTLResourceOptions::StorageModePrivate)
            .unwrap();
        let second = heap
            .newBufferWithLength_options(256, MTLResourceOptions::StorageModePrivate)
            .unwrap();
        let set = MetalResidency::new(&device, "heap_test").unwrap();
        set.add_buffer(&first).unwrap();
        set.add_buffer(&second).unwrap();
        set.commit();
        assert_eq!(set.diagnostics().allocation_count, 3);
        assert_eq!(
            set.diagnostics().estimated_resident_bytes,
            heap.size() as u64
        );
        assert!(
            set.native()
                .containsAllocation(ProtocolObject::from_ref(&*heap))
        );
        drop(heap);
        set.validate_buffer(&first).unwrap();
        set.validate_buffer(&second).unwrap();
    }
    #[test]
    fn test_slot_residency_sets_have_independent_native_membership() {
        let device = MTLCreateSystemDefaultDevice().unwrap();
        let buffers: Vec<_> = (0..3)
            .map(|_| {
                device
                    .newBufferWithLength_options(256, MTLResourceOptions::StorageModeShared)
                    .unwrap()
            })
            .collect();
        let sets: Vec<_> = (0..3)
            .map(|slot| MetalResidency::new(&device, &format!("slot_{slot}")).unwrap())
            .collect();
        for (set, buffer) in sets.iter().zip(&buffers) {
            set.add_buffer(buffer).unwrap();
            set.commit();
        }
        for (slot, set) in sets.iter().enumerate() {
            for (other, buffer) in buffers.iter().enumerate() {
                assert_eq!(set.validate_buffer(buffer).is_ok(), slot == other);
            }
            assert_eq!(set.diagnostics().members[0].id, 0);
            assert_eq!(set.diagnostics().members[0].kind, "buffer");
        }
    }
    #[test]
    fn test_persistent_buffer_replacement_preserves_previous_native_set() {
        let device = MTLCreateSystemDefaultDevice().unwrap();
        let original = device
            .newBufferWithLength_options(256, MTLResourceOptions::StorageModeShared)
            .unwrap();
        let replacement = device
            .newBufferWithLength_options(512, MTLResourceOptions::StorageModeShared)
            .unwrap();
        let mut manager = PersistentBufferResidency::new(&device, "persistent_test").unwrap();
        manager.replace(&[], &[&original]).unwrap();
        let submitted = manager.snapshot();
        assert!(std::rc::Rc::ptr_eq(&submitted, &manager.snapshot()));
        manager.replace(&[&original], &[&replacement]).unwrap();
        let next = manager.snapshot();
        assert!(!std::rc::Rc::ptr_eq(&submitted, &next));
        submitted.validate_buffer(&original).unwrap();
        assert!(submitted.validate_buffer(&replacement).is_err());
        next.validate_buffer(&replacement).unwrap();
        assert!(next.validate_buffer(&original).is_err());
        manager.clear().unwrap();
        assert_eq!(manager.snapshot().diagnostics().allocation_count, 0);
        submitted.validate_buffer(&original).unwrap();
        next.validate_buffer(&replacement).unwrap();
    }

    #[test]
    fn test_memoryless_attachment_never_enters_native_residency_set() {
        use objc2_metal::{MTLPixelFormat, MTLTextureDescriptor, MTLTextureUsage};
        let device = MTLCreateSystemDefaultDevice().unwrap();
        let descriptor = MTLTextureDescriptor::new();
        unsafe {
            descriptor.setWidth(1);
            descriptor.setHeight(1);
        }
        descriptor.setPixelFormat(MTLPixelFormat::RGBA8Unorm);
        descriptor.setStorageMode(MTLStorageMode::Memoryless);
        descriptor.setUsage(MTLTextureUsage::RenderTarget);
        let texture = device.newTextureWithDescriptor(&descriptor).unwrap();
        let set = MetalResidency::new(&device, "memoryless_test").unwrap();
        set.add_texture(&texture).unwrap();
        set.validate_texture(&texture).unwrap();
        set.commit();
        assert_eq!(set.native().allocationCount(), 0);
        assert_eq!(set.diagnostics().estimated_resident_bytes, 0);
    }
}
