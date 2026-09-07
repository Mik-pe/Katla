use std::cell::{RefCell, RefMut};
use std::mem::ManuallyDrop;
#[cfg(test)]
use std::sync::atomic::AtomicU32;
use std::sync::atomic::{AtomicUsize, Ordering};

use ash::vk;
use gpu_allocator::{
    MemoryLocation,
    vulkan::{Allocation, AllocationCreateDesc, AllocationScheme, Allocator},
};

use crate::error::RendererError;

use super::VulkanContext;

/// Temporary owner of a partially-constructed buffer.
///
/// Destroys the native buffer and releases its allocation (if already made)
/// when dropped without `commit`, so a failed constructor step leaves no
/// half-owned GPU resource behind.
struct OwnedBuffer<'a> {
    context: &'a VulkanContext,
    buffer: vk::Buffer,
    allocation: Option<Allocation>,
    committed: bool,
}

impl<'a> OwnedBuffer<'a> {
    fn new(context: &'a VulkanContext, buffer: vk::Buffer) -> Self {
        Self {
            context,
            buffer,
            allocation: None,
            committed: false,
        }
    }

    /// Hand the fully-constructed buffer and its allocation to the caller.
    fn commit(mut self) -> (vk::Buffer, Allocation) {
        self.committed = true;
        let allocation = self
            .allocation
            .take()
            .expect("committed buffer holds an allocation");
        (self.buffer, allocation)
    }
}

impl Drop for OwnedBuffer<'_> {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        if let Some(allocation) = self.allocation.take() {
            self.context.allocator.free(allocation, "buffer");
        }
        unsafe {
            self.context.device.destroy_buffer(self.buffer, None);
        }
    }
}

/// Temporary owner of a partially-constructed image.
///
/// Destroys the native image and releases its allocation (if already made)
/// when dropped without `commit`, so a failed constructor step leaves no
/// half-owned GPU resource behind.
struct OwnedImage<'a> {
    context: &'a VulkanContext,
    image: vk::Image,
    allocation: Option<Allocation>,
    committed: bool,
}

impl OwnedImage<'_> {
    /// Hand the fully-constructed image and its allocation to the caller.
    fn commit(mut self) -> (vk::Image, Allocation) {
        self.committed = true;
        let allocation = self
            .allocation
            .take()
            .expect("committed image holds an allocation");
        (self.image, allocation)
    }
}

impl Drop for OwnedImage<'_> {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        if let Some(allocation) = self.allocation.take() {
            self.context.allocator.free(allocation, "image");
        }
        unsafe {
            self.context.device.destroy_image(self.image, None);
        }
    }
}

/// Wrapper around `ManuallyDrop<RefCell<Allocator>>` that provides safe
/// allocation/deallocation methods which log warnings on borrow conflicts
/// instead of panicking or silently leaking memory.
///
/// A borrow conflict during `free` cannot happen while another allocation is
/// in progress (the allocator is borrowed for its whole duration), but it can
/// happen when a resource is dropped re-entrantly. Freed allocations in that
/// situation are queued and drained deterministically on the next `allocate`
/// or `drain_pending_frees` call instead of being abandoned.
pub struct GpuAllocator {
    inner: ManuallyDrop<RefCell<Allocator>>,
    /// Live allocations handed out by `allocate` and released by `free`.
    live_allocations: AtomicUsize,
    /// Allocations that could not be released due to a borrow conflict.
    pending_frees: RefCell<Vec<(Allocation, String)>>,
    /// Test-only countdown: the next N allocations fail before touching memory.
    #[cfg(test)]
    injected_allocation_failures: AtomicU32,
}

impl GpuAllocator {
    pub(crate) fn new(allocator: Allocator) -> Self {
        Self {
            inner: ManuallyDrop::new(RefCell::new(allocator)),
            live_allocations: AtomicUsize::new(0),
            pending_frees: RefCell::new(Vec::new()),
            #[cfg(test)]
            injected_allocation_failures: AtomicU32::new(0),
        }
    }

    /// Snapshot of the debug allocation accounting: (live, pending free).
    ///
    /// `live` counts allocations handed out by `allocate` that have not been
    /// released yet; `pending` counts allocations queued for release because
    /// of a borrow conflict. Both must return to their baseline after every
    /// create/destroy cycle.
    pub fn debug_allocation_stats(&self) -> (usize, usize) {
        (
            self.live_allocations.load(Ordering::Relaxed),
            self.pending_frees.borrow().len(),
        )
    }

    /// Make the next `count` allocations fail without touching memory.
    ///
    /// Test-only hook for exercising partial-failure cleanup paths.
    #[cfg(test)]
    pub(crate) fn inject_allocation_failures(&self, count: u32) {
        self.injected_allocation_failures
            .store(count, Ordering::Relaxed);
    }

    /// Allocate GPU memory via the inner allocator.
    ///
    /// Returns `Err` on allocation failure, injected test failure, and borrow
    /// conflict. Borrow conflicts are logged as warnings — they indicate a
    /// re-entrant call (e.g. freeing memory during a Drop while the allocator
    /// is borrowed for an allocation).
    pub fn allocate(
        &self,
        desc: &AllocationCreateDesc,
        context: &str,
    ) -> Result<Allocation, RendererError> {
        self.drain_pending_frees();

        #[cfg(test)]
        {
            let remaining = self.injected_allocation_failures.load(Ordering::Relaxed);
            if remaining > 0 {
                self.injected_allocation_failures
                    .store(remaining - 1, Ordering::Relaxed);
                return Err(RendererError::InvalidOperation(format!(
                    "Injected allocation failure for {context}"
                )));
            }
        }

        let mut allocator = self.try_borrow(context)?;
        let allocation = allocator
            .allocate(desc)
            .map_err(|e| RendererError::from_allocation_error(context, e))?;
        self.live_allocations.fetch_add(1, Ordering::Relaxed);
        Ok(allocation)
    }

    /// Free a GPU memory allocation.
    ///
    /// If the allocator is currently borrowed (re-entrant release), the
    /// allocation is queued and released on the next `allocate` or
    /// `drain_pending_frees` call instead of being abandoned.
    pub fn free(&self, allocation: Allocation, context: &str) {
        let offset = allocation.offset();
        match self.inner.try_borrow_mut() {
            Ok(mut allocator) => {
                if let Err(e) = allocator.free(allocation) {
                    log::warn!(
                        "Failed to free {} allocation at offset {:?}: {:?}",
                        context,
                        offset,
                        e
                    );
                } else {
                    self.live_allocations.fetch_sub(1, Ordering::Relaxed);
                }
            }
            Err(_) => {
                log::debug!(
                    "GpuAllocator borrow conflict during {} free — \
                     allocation at offset {:?} queued for deferred release",
                    context,
                    offset
                );
                self.pending_frees
                    .borrow_mut()
                    .push((allocation, context.to_string()));
            }
        }
    }

    /// Release every allocation queued by a conflicting `free`.
    ///
    /// Called at the start of `allocate` and during teardown so queued
    /// releases cannot be starved.
    pub fn drain_pending_frees(&self) {
        let queued: Vec<(Allocation, String)> = {
            let mut pending = self.pending_frees.borrow_mut();
            std::mem::take(&mut *pending)
        };
        if queued.is_empty() {
            return;
        }
        let mut requeued = Vec::new();
        for (allocation, context) in queued {
            let offset = allocation.offset();
            match self.inner.try_borrow_mut() {
                Ok(mut allocator) => {
                    if let Err(e) = allocator.free(allocation) {
                        log::warn!(
                            "Failed to free deferred {} allocation at offset {:?}: {:?}",
                            context,
                            offset,
                            e
                        );
                    } else {
                        self.live_allocations.fetch_sub(1, Ordering::Relaxed);
                    }
                }
                Err(_) => {
                    // The allocator is borrowed again — requeue for the next
                    // drain instead of leaking silently.
                    log::debug!(
                        "GpuAllocator borrow conflict during deferred {} free — requeued",
                        context
                    );
                    requeued.push((allocation, context));
                }
            }
        }
        if !requeued.is_empty() {
            *self.pending_frees.borrow_mut() = requeued;
        }
    }

    /// Borrow the allocator mutably for direct operations.
    ///
    /// Logs a warning on borrow conflict and returns a `RendererError`.
    pub fn try_borrow(&self, context: &str) -> Result<RefMut<'_, Allocator>, RendererError> {
        self.inner.try_borrow_mut().map_err(|_| {
            log::warn!(
                "GpuAllocator borrow conflict during {} — \
                 allocator is already borrowed",
                context
            );
            RendererError::InvalidOperation(format!(
                "GpuAllocator borrow conflict during {}",
                context
            ))
        })
    }

    /// Drop the inner allocator. Called during `VulkanContext::drop`.
    pub(crate) unsafe fn destroy(&mut self) {
        self.drain_pending_frees();
        unsafe {
            ManuallyDrop::drop(&mut self.inner);
        }
    }
}

impl VulkanContext {
    pub fn allocate_buffer(
        &self,
        buffer_info: &vk::BufferCreateInfo,
        location: MemoryLocation,
    ) -> Result<(vk::Buffer, Allocation), RendererError> {
        self.allocate_buffer_named(buffer_info, location, "Buffer Allocation")
    }

    /// Allocate a buffer under a specific debug name.
    ///
    /// The native buffer, its memory, and the bind step form one transaction:
    /// any failing step releases everything already created.
    pub fn allocate_buffer_named(
        &self,
        buffer_info: &vk::BufferCreateInfo,
        location: MemoryLocation,
        name: &str,
    ) -> Result<(vk::Buffer, Allocation), RendererError> {
        let buffer = unsafe { self.device.create_buffer(buffer_info, None) }
            .map_err(|e| RendererError::VulkanError("Failed to create buffer".into(), e))?;
        let mut guard = OwnedBuffer::new(self, buffer);

        let requirements = unsafe { self.device.get_buffer_memory_requirements(guard.buffer) };
        let allocation_info = AllocationCreateDesc {
            name,
            requirements,
            location,
            linear: true,
            allocation_scheme: AllocationScheme::GpuAllocatorManaged,
        };

        let allocation = self.allocator.allocate(&allocation_info, name)?;
        guard.allocation = Some(allocation);

        let allocation = guard.allocation.as_ref().expect("guard holds allocation");
        unsafe {
            self.device
                .bind_buffer_memory(guard.buffer, allocation.memory(), allocation.offset())
                .map_err(|e| {
                    RendererError::VulkanError("Failed to bind buffer memory".into(), e)
                })?;
        }
        Ok(guard.commit())
    }

    /// Free a buffer and its allocation.
    pub(crate) fn free_buffer(&self, buffer: vk::Buffer, allocation: Allocation) {
        self.allocator.free(allocation, "buffer");
        unsafe { self.device.destroy_buffer(buffer, None) };
    }

    /// Map a buffer allocation to host memory.
    /// Currently maps the entire buffer; partial mapping could be added as an optimization.
    pub fn map_buffer(&self, allocation: &Allocation) -> Result<*mut u8, RendererError> {
        allocation
            .mapped_ptr()
            .map(|ptr| ptr.cast().as_ptr())
            .ok_or_else(|| RendererError::InvalidOperation("Buffer is not mapped".to_string()))
    }

    /// Flush mapped memory ranges to make CPU writes visible to the GPU.
    ///
    /// This is required for non-coherent memory types. For coherent memory, this is a no-op.
    ///
    /// The offset and size are automatically aligned to `non_coherent_atom_size` as required
    /// by the Vulkan specification. The actual flushed range may be slightly larger than
    /// requested to ensure proper alignment.
    pub fn flush_mapped_memory(
        &self,
        allocation: &Allocation,
        offset: vk::DeviceSize,
        size: vk::DeviceSize,
    ) -> Result<(), RendererError> {
        let base_memory_offset = allocation.offset() + offset;

        let aligned_memory_offset = base_memory_offset & !(self.non_coherent_atom_size - 1);

        let end = base_memory_offset + size;

        let aligned_size = if size == vk::WHOLE_SIZE {
            vk::WHOLE_SIZE
        } else {
            let size_needed = end - aligned_memory_offset;
            (size_needed + self.non_coherent_atom_size - 1) & !(self.non_coherent_atom_size - 1)
        };

        unsafe {
            let memory = allocation.memory();
            let flush_range = vk::MappedMemoryRange::default()
                .memory(memory)
                .offset(aligned_memory_offset)
                .size(aligned_size);

            self.device
                .flush_mapped_memory_ranges(&[flush_range])
                .map_err(|e| {
                    RendererError::VulkanError("Failed to flush mapped memory".into(), e)
                })?;
        }
        Ok(())
    }

    /// Invalidate mapped memory ranges to make GPU writes visible to CPU reads.
    ///
    /// Must be called after a GPU write (e.g., compute shader atomic operations,
    /// vkCmdFillBuffer, vkCmdCopyBuffer) before reading the mapped memory on CPU.
    ///
    /// The offset and size are automatically aligned to `non_coherent_atom_size` as required
    /// by the Vulkan specification.
    pub fn invalidate_mapped_memory(
        &self,
        allocation: &Allocation,
        offset: vk::DeviceSize,
        size: vk::DeviceSize,
    ) -> Result<(), RendererError> {
        let base_memory_offset = allocation.offset() + offset;
        let aligned_memory_offset = base_memory_offset & !(self.non_coherent_atom_size - 1);

        let aligned_size = if size == vk::WHOLE_SIZE {
            vk::WHOLE_SIZE
        } else {
            let end = base_memory_offset + size;
            let size_needed = end - aligned_memory_offset;
            (size_needed + self.non_coherent_atom_size - 1) & !(self.non_coherent_atom_size - 1)
        };

        unsafe {
            let memory = allocation.memory();
            let range = vk::MappedMemoryRange::default()
                .memory(memory)
                .offset(aligned_memory_offset)
                .size(aligned_size);

            self.device
                .invalidate_mapped_memory_ranges(&[range])
                .map_err(|e| {
                    RendererError::VulkanError("Failed to invalidate mapped memory".into(), e)
                })?;
        }
        Ok(())
    }

    pub fn create_image(
        &self,
        image_create_info: vk::ImageCreateInfo,
        location: MemoryLocation,
    ) -> Result<(vk::Image, Allocation), RendererError> {
        self.create_image_named(image_create_info, location, "Image Allocation")
    }

    /// Allocate an image under a specific debug name.
    ///
    /// The native image, its memory, and the bind step form one transaction:
    /// any failing step releases everything already created.
    pub fn create_image_named(
        &self,
        image_create_info: vk::ImageCreateInfo,
        location: MemoryLocation,
        name: &str,
    ) -> Result<(vk::Image, Allocation), RendererError> {
        let image = unsafe { self.device.create_image(&image_create_info, None) }
            .map_err(|e| RendererError::VulkanError("Failed to create image".into(), e))?;
        let mut guard = OwnedImage {
            context: self,
            image,
            allocation: None,
            committed: false,
        };

        let requirements = unsafe { self.device.get_image_memory_requirements(guard.image) };
        let allocation_info = AllocationCreateDesc {
            name,
            requirements,
            location,
            linear: true,
            allocation_scheme: AllocationScheme::GpuAllocatorManaged,
        };

        let allocation = self.allocator.allocate(&allocation_info, name)?;
        guard.allocation = Some(allocation);

        let allocation = guard.allocation.as_ref().expect("guard holds allocation");
        unsafe {
            self.device
                .bind_image_memory(guard.image, allocation.memory(), allocation.offset())
                .map_err(|e| RendererError::VulkanError("Failed to bind image memory".into(), e))?;
        }
        Ok(guard.commit())
    }

    /// Free an image and its allocation.
    /// Uses wrapper type to avoid exposing vk::Image in public API.
    pub(crate) fn free_image(&self, image: crate::sync::VkImage, allocation: Allocation) {
        self.allocator.free(allocation, "image");
        unsafe {
            self.device.destroy_image(image.vk(), None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ValidationMode;
    use std::ffi::CString;
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::rc::Rc;

    fn headless_context() -> Rc<VulkanContext> {
        Rc::new(
            VulkanContext::init_headless(
                ValidationMode::Disabled,
                CString::new("Katla resource transaction tests").unwrap(),
                CString::new("Katla").unwrap(),
            )
            .expect("headless Vulkan context"),
        )
    }

    fn buffer_create_info(size: vk::DeviceSize) -> vk::BufferCreateInfo<'static> {
        vk::BufferCreateInfo::default()
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .usage(vk::BufferUsageFlags::VERTEX_BUFFER)
            .size(size)
    }

    fn image_create_info() -> vk::ImageCreateInfo<'static> {
        vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(vk::Format::R8G8B8A8_UNORM)
            .extent(vk::Extent3D {
                width: 4,
                height: 4,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
    }

    #[test]
    #[ignore = "requires a Vulkan device"]
    fn test_buffer_allocation_failure_leaks_nothing() {
        let context = headless_context();
        let baseline = context.allocator.debug_allocation_stats();

        context.allocator.inject_allocation_failures(1);
        let result = context.allocate_buffer(&buffer_create_info(1024), MemoryLocation::CpuToGpu);
        assert!(result.is_err());
        assert_eq!(context.allocator.debug_allocation_stats(), baseline);

        // The injected failure is consumed; normal allocation still works.
        let (buffer, allocation) = context
            .allocate_buffer(&buffer_create_info(1024), MemoryLocation::CpuToGpu)
            .unwrap();
        context.free_buffer(buffer, allocation);
        assert_eq!(context.allocator.debug_allocation_stats(), baseline);
    }

    #[test]
    #[ignore = "requires a Vulkan device"]
    fn test_image_allocation_failure_leaks_nothing() {
        let context = headless_context();
        let baseline = context.allocator.debug_allocation_stats();

        context.allocator.inject_allocation_failures(1);
        let result = context.create_image(image_create_info(), MemoryLocation::GpuOnly);
        assert!(result.is_err());
        assert_eq!(context.allocator.debug_allocation_stats(), baseline);

        let (image, allocation) = context
            .create_image(image_create_info(), MemoryLocation::GpuOnly)
            .unwrap();
        context.free_image(crate::sync::VkImage::new(image), allocation);
        assert_eq!(context.allocator.debug_allocation_stats(), baseline);
    }

    #[test]
    #[ignore = "requires a Vulkan device"]
    fn test_create_destroy_cycles_balance_accounting() {
        let context = headless_context();
        let baseline = context.allocator.debug_allocation_stats();

        for _ in 0..10 {
            let (buffer, allocation) = context
                .allocate_buffer(&buffer_create_info(4096), MemoryLocation::CpuToGpu)
                .unwrap();
            context.free_buffer(buffer, allocation);

            let (image, allocation) = context
                .create_image(image_create_info(), MemoryLocation::GpuOnly)
                .unwrap();
            context.free_image(crate::sync::VkImage::new(image), allocation);
        }

        assert_eq!(context.allocator.debug_allocation_stats(), baseline);
    }

    #[test]
    #[ignore = "requires a Vulkan device"]
    fn test_buffer_resize_failure_keeps_buffer_usable() {
        let context = headless_context();
        let mut vertex_buffer = crate::vulkan::VertexBuffer::new(context.clone(), 1024, 0);
        vertex_buffer.upload_data(&[7u8; 64]);
        let baseline = context.allocator.debug_allocation_stats();

        // Growth triggers a replacement allocation; the injected failure makes
        // it panic, but the old buffer must survive and stay usable.
        context.allocator.inject_allocation_failures(1);
        let result = catch_unwind(AssertUnwindSafe(|| {
            vertex_buffer.upload_data(&[7u8; 4096]);
        }));
        assert!(result.is_err(), "resize under injected failure must panic");
        assert_eq!(context.allocator.debug_allocation_stats(), baseline);

        vertex_buffer.upload_data(&[7u8; 64]);
    }

    #[test]
    #[ignore = "requires a Vulkan device"]
    fn test_borrow_conflict_defers_free_instead_of_leaking() {
        let context = headless_context();
        let baseline = context.allocator.debug_allocation_stats();

        let (buffer, allocation) = context
            .allocate_buffer(&buffer_create_info(1024), MemoryLocation::CpuToGpu)
            .unwrap();
        assert_eq!(
            context.allocator.debug_allocation_stats(),
            (baseline.0 + 1, 0)
        );

        // Re-entrant release while the allocator is borrowed: the allocation
        // must be queued, not abandoned.
        {
            let _borrow = context.allocator.try_borrow("test").unwrap();
            context.allocator.free(allocation, "buffer");
            assert_eq!(
                context.allocator.debug_allocation_stats(),
                (baseline.0 + 1, 1)
            );
        }

        // Draining (also implicit in the next allocate) releases the queue.
        context.allocator.drain_pending_frees();
        assert_eq!(context.allocator.debug_allocation_stats(), (baseline.0, 0));

        // The allocation is gone; only the native buffer handle needs cleanup.
        unsafe { context.device.destroy_buffer(buffer, None) };
    }

    #[test]
    #[ignore = "requires a Vulkan device"]
    fn test_dropped_partial_image_releases_image_and_allocation() {
        // A constructor that fails after create_image returned must release
        // both objects: dropping the temporary owner (without commit) is the
        // cleanup path such a constructor relies on.
        let context = headless_context();
        let baseline = context.allocator.debug_allocation_stats();

        let (image, allocation) = context
            .create_image(image_create_info(), MemoryLocation::GpuOnly)
            .unwrap();
        assert_eq!(
            context.allocator.debug_allocation_stats(),
            (baseline.0 + 1, 0)
        );

        let owned = OwnedImage {
            context: &context,
            image,
            allocation: Some(allocation),
            committed: false,
        };
        drop(owned);

        assert_eq!(context.allocator.debug_allocation_stats(), baseline);
    }
}
