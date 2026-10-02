//! Reusable graphics descriptor storage owned by one in-flight frame slot.

use std::rc::Rc;

use ash::vk;

use super::context::native_lifetime::NativeDevice;
use crate::RendererError;

#[cfg(test)]
mod tests;

const SETS_PER_POOL: u32 = 128;
const DESCRIPTORS_PER_TYPE: u32 = 256;
const TYPES: [vk::DescriptorType; 5] = [
    vk::DescriptorType::UNIFORM_BUFFER,
    vk::DescriptorType::STORAGE_BUFFER,
    vk::DescriptorType::SAMPLED_IMAGE,
    vk::DescriptorType::STORAGE_IMAGE,
    vk::DescriptorType::SAMPLER,
];

pub(crate) struct DescriptorArena {
    native: Rc<NativeDevice>,
    pools: Vec<Pool>,
    current: usize,
    #[cfg(test)]
    allocation_failure: Option<vk::Result>,
}

struct Pool {
    native: Rc<NativeDevice>,
    handle: vk::DescriptorPool,
    capacity: [u32; TYPES.len()],
    remaining: [u32; TYPES.len()],
    remaining_sets: u32,
    #[cfg(test)]
    allocated_sets: usize,
}

impl DescriptorArena {
    pub(crate) fn new(native: Rc<NativeDevice>) -> Self {
        Self {
            native,
            pools: Vec::new(),
            current: 0,
            #[cfg(test)]
            allocation_failure: None,
        }
    }

    pub(crate) fn allocate(
        &mut self,
        layout: vk::DescriptorSetLayout,
        sizes: &[vk::DescriptorPoolSize],
    ) -> Result<vk::DescriptorSet, RendererError> {
        let mut required = [0u32; TYPES.len()];
        for size in sizes {
            let index = TYPES
                .iter()
                .position(|kind| *kind == size.ty)
                .ok_or_else(|| {
                    RendererError::InvalidOperation(format!(
                        "Unsupported graphics descriptor type: {:?}",
                        size.ty
                    ))
                })?;
            required[index] = required[index]
                .checked_add(size.descriptor_count)
                .ok_or_else(|| {
                    RendererError::InvalidOperation("Descriptor count overflow".into())
                })?;
        }
        loop {
            let fresh = self.current == self.pools.len();
            if fresh {
                self.pools.push(Pool::new(self.native.clone(), required)?);
            }
            let pool = &mut self.pools[self.current];
            if pool.remaining_sets == 0
                || required
                    .iter()
                    .zip(pool.remaining)
                    .any(|(need, left)| *need > left)
            {
                self.current += 1;
                continue;
            }
            let layouts = [layout];
            let info = vk::DescriptorSetAllocateInfo::default()
                .descriptor_pool(pool.handle)
                .set_layouts(&layouts);
            #[cfg(test)]
            let failure = self.allocation_failure.take();
            #[cfg(not(test))]
            let failure: Option<vk::Result> = None;
            let result = match failure {
                Some(error) => Err(error),
                None => unsafe { self.native.device.allocate_descriptor_sets(&info) },
            };
            match result {
                Ok(sets) => {
                    pool.remaining_sets -= 1;
                    #[cfg(test)]
                    {
                        pool.allocated_sets += 1;
                    }
                    for (left, need) in pool.remaining.iter_mut().zip(required) {
                        *left -= need;
                    }
                    return Ok(sets[0]);
                }
                Err(
                    error @ (vk::Result::ERROR_OUT_OF_POOL_MEMORY
                    | vk::Result::ERROR_FRAGMENTED_POOL),
                ) => {
                    pool.remaining_sets = 0;
                    self.current += 1;
                    if fresh {
                        return Err(RendererError::VulkanError(
                            "Failed to allocate graphics descriptors from a new pool".into(),
                            error,
                        ));
                    }
                }
                Err(error) => {
                    return Err(RendererError::VulkanError(
                        "Failed to allocate graphics descriptors".into(),
                        error,
                    ));
                }
            }
        }
    }

    /// Recycle all sets after this slot's GPU work and command recording retire.
    pub(crate) fn reset(&mut self) -> Result<(), RendererError> {
        for pool in &mut self.pools {
            if pool.remaining_sets == SETS_PER_POOL {
                continue;
            }
            unsafe {
                self.native
                    .device
                    .reset_descriptor_pool(pool.handle, vk::DescriptorPoolResetFlags::empty())
            }
            .map_err(|error| {
                RendererError::VulkanError("Failed to reset graphics descriptors".into(), error)
            })?;
            pool.remaining = pool.capacity;
            pool.remaining_sets = SETS_PER_POOL;
            #[cfg(test)]
            {
                pool.allocated_sets = 0;
            }
        }
        self.current = 0;
        Ok(())
    }

    pub(crate) fn clear(&mut self) {
        self.pools.clear();
        self.current = 0;
    }

    #[cfg(test)]
    pub(crate) fn allocated_sets(&self) -> usize {
        self.pools.iter().map(|pool| pool.allocated_sets).sum()
    }

    #[cfg(test)]
    pub(crate) fn pool_count(&self) -> usize {
        self.pools.len()
    }

    #[cfg(test)]
    pub(crate) fn pool_handles(&self) -> Vec<vk::DescriptorPool> {
        self.pools.iter().map(|pool| pool.handle).collect()
    }
}

impl Pool {
    fn new(native: Rc<NativeDevice>, required: [u32; TYPES.len()]) -> Result<Self, RendererError> {
        let capacity = required.map(|count| count.max(DESCRIPTORS_PER_TYPE));
        let sizes: Vec<_> = TYPES
            .into_iter()
            .zip(capacity)
            .map(|(ty, count)| {
                vk::DescriptorPoolSize::default()
                    .ty(ty)
                    .descriptor_count(count)
            })
            .collect();
        let info = vk::DescriptorPoolCreateInfo::default()
            .max_sets(SETS_PER_POOL)
            .pool_sizes(&sizes);
        let handle =
            unsafe { native.device.create_descriptor_pool(&info, None) }.map_err(|error| {
                RendererError::VulkanError(
                    "Failed to create graphics descriptor pool".into(),
                    error,
                )
            })?;
        Ok(Self {
            native,
            handle,
            capacity,
            remaining: capacity,
            remaining_sets: SETS_PER_POOL,
            #[cfg(test)]
            allocated_sets: 0,
        })
    }
}

impl Drop for Pool {
    fn drop(&mut self) {
        unsafe {
            self.native
                .device
                .destroy_descriptor_pool(self.handle, None)
        };
    }
}
