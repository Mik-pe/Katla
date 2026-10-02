use ash::{Instance, vk};

use super::QueueFamilyIndices;
use crate::RendererError;

impl QueueFamilyIndices {
    pub(super) fn find_queue_families(
        instance: &Instance,
        surface_loader: &ash::khr::surface::Instance,
        surface: vk::SurfaceKHR,
        physical_device: vk::PhysicalDevice,
    ) -> Result<Self, RendererError> {
        let families =
            unsafe { instance.get_physical_device_queue_family_properties(physical_device) };
        let mut presentation = Vec::with_capacity(families.len());
        for (index, family) in families.iter().enumerate() {
            let supported = if supports_rendering(family) {
                unsafe {
                    surface_loader.get_physical_device_surface_support(
                        physical_device,
                        index as u32,
                        surface,
                    )
                }
                .map_err(|e| {
                    RendererError::VulkanError(
                        "Failed to query queue presentation support".into(),
                        e,
                    )
                })?
            } else {
                false
            };
            presentation.push(supported);
        }
        Ok(select_queue_families(&families, Some(&presentation)))
    }

    pub(super) fn find_queue_families_headless(
        instance: &Instance,
        physical_device: vk::PhysicalDevice,
    ) -> Self {
        let families =
            unsafe { instance.get_physical_device_queue_family_properties(physical_device) };
        select_queue_families(&families, None)
    }
}

fn supports_rendering(family: &vk::QueueFamilyProperties) -> bool {
    family.queue_count > 0
        && family
            .queue_flags
            .contains(vk::QueueFlags::GRAPHICS | vk::QueueFlags::COMPUTE)
}

fn select_queue_families(
    families: &[vk::QueueFamilyProperties],
    presentation: Option<&[bool]>,
) -> QueueFamilyIndices {
    let graphics_idx = families
        .iter()
        .enumerate()
        .find(|(index, family)| {
            supports_rendering(family)
                && presentation.is_none_or(|support| support.get(*index) == Some(&true))
        })
        .map(|(index, _)| index as u32);
    let transfer_idx = families
        .iter()
        .position(|family| {
            family.queue_count > 0
                && family.queue_flags.contains(vk::QueueFlags::TRANSFER)
                && !family
                    .queue_flags
                    .intersects(vk::QueueFlags::GRAPHICS | vk::QueueFlags::COMPUTE)
        })
        .map(|index| index as u32)
        .or(graphics_idx);
    QueueFamilyIndices {
        graphics_idx,
        transfer_idx,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn family(flags: vk::QueueFlags) -> vk::QueueFamilyProperties {
        vk::QueueFamilyProperties {
            queue_flags: flags,
            queue_count: 1,
            ..Default::default()
        }
    }

    #[test]
    fn test_rendering_requires_graphics_and_compute_on_the_same_queue() {
        let families = [
            family(vk::QueueFlags::GRAPHICS),
            family(vk::QueueFlags::COMPUTE),
            family(vk::QueueFlags::GRAPHICS | vk::QueueFlags::COMPUTE),
        ];
        let selected = select_queue_families(&families, None);
        assert_eq!(selected.graphics_idx, Some(2));
        assert_eq!(selected.transfer_idx, Some(2));
        assert_eq!(
            select_queue_families(&families[..2], None).graphics_idx,
            None
        );
    }

    #[test]
    fn test_transfer_queue_does_not_require_presentation() {
        let families = [
            family(vk::QueueFlags::TRANSFER),
            family(vk::QueueFlags::GRAPHICS | vk::QueueFlags::COMPUTE),
        ];
        let selected = select_queue_families(&families, Some(&[false, true]));
        assert_eq!(selected.graphics_idx, Some(1));
        assert_eq!(selected.transfer_idx, Some(0));
        assert_eq!(
            select_queue_families(&families, Some(&[true, false])).graphics_idx,
            None
        );
    }

    #[test]
    fn test_empty_queue_families_are_ignored() {
        let empty = vk::QueueFamilyProperties {
            queue_flags: vk::QueueFlags::GRAPHICS
                | vk::QueueFlags::COMPUTE
                | vk::QueueFlags::TRANSFER,
            ..Default::default()
        };
        let families = [
            empty,
            family(vk::QueueFlags::GRAPHICS | vk::QueueFlags::COMPUTE),
        ];
        let selected = select_queue_families(&families, None);
        assert_eq!(selected.graphics_idx, Some(1));
        assert_eq!(selected.transfer_idx, Some(1));
    }
}
