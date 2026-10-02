use std::ffi::CStr;

use ash::{Instance, vk};

use super::QueueFamilyIndices;
use crate::{RendererError, vulkan::SwapchainInfo};

macro_rules! required_features {
    ($required:ident, $missing:ident, $ty:ty, $($field:ident),+ $(,)?) => {
        pub(super) fn $required() -> $ty {
            <$ty>::default()$(.$field(true))+
        }

        fn $missing(features: &$ty) -> Vec<&'static str> {
            let mut missing = Vec::new();
            $(if features.$field != vk::TRUE { missing.push(stringify!($field)); })+
            missing
        }
    };
}

required_features!(
    required_core_features,
    missing_core_features,
    vk::PhysicalDeviceFeatures,
    sampler_anisotropy
);
required_features!(
    required_vulkan12_features,
    missing_vulkan12_features,
    vk::PhysicalDeviceVulkan12Features<'static>,
    buffer_device_address,
    descriptor_indexing,
    shader_sampled_image_array_non_uniform_indexing,
    descriptor_binding_sampled_image_update_after_bind,
    descriptor_binding_storage_buffer_update_after_bind,
    descriptor_binding_partially_bound,
    descriptor_binding_variable_descriptor_count,
    runtime_descriptor_array
);
required_features!(
    required_vulkan13_features,
    missing_vulkan13_features,
    vk::PhysicalDeviceVulkan13Features<'static>,
    dynamic_rendering,
    synchronization2,
    maintenance4
);

pub(super) fn required_extensions(windowed: bool) -> Vec<&'static CStr> {
    let mut extensions = vec![ash::khr::push_descriptor::NAME];
    if windowed {
        extensions.push(ash::khr::swapchain::NAME);
    }
    extensions
}

fn missing_extensions(available: &[vk::ExtensionProperties], windowed: bool) -> Vec<&'static str> {
    required_extensions(windowed).into_iter().filter(|required| {
        !available.iter().any(|extension| unsafe { CStr::from_ptr(extension.extension_name.as_ptr()) } == *required)
    }).map(|name| name.to_str().unwrap_or("unknown Vulkan extension")).collect()
}

struct DeviceCandidate {
    device: vk::PhysicalDevice,
    name: String,
    score: (u8, u32),
    rejection: Option<String>,
}

fn device_score(properties: &vk::PhysicalDeviceProperties) -> (u8, u32) {
    let rank = match properties.device_type {
        vk::PhysicalDeviceType::DISCRETE_GPU => 4,
        vk::PhysicalDeviceType::INTEGRATED_GPU => 3,
        vk::PhysicalDeviceType::VIRTUAL_GPU => 2,
        vk::PhysicalDeviceType::CPU => 1,
        _ => 0,
    };
    (rank, properties.limits.max_image_dimension2_d)
}

fn choose_device(candidates: &[DeviceCandidate]) -> Result<vk::PhysicalDevice, RendererError> {
    if let Some(best) = candidates
        .iter()
        .filter(|candidate| candidate.rejection.is_none())
        .max_by_key(|candidate| candidate.score)
    {
        log::info!("Picking Vulkan physical device: {}", best.name);
        return Ok(best.device);
    }
    let details = candidates
        .iter()
        .map(|candidate| {
            format!(
                "{}: {}",
                candidate.name,
                candidate.rejection.as_deref().unwrap_or("unsuitable")
            )
        })
        .collect::<Vec<_>>()
        .join("; ");
    Err(RendererError::InitializationFailed(
        if candidates.is_empty() {
            "No Vulkan physical devices found".into()
        } else {
            format!("No suitable Vulkan physical device found: {details}")
        },
    ))
}

pub(super) unsafe fn pick_physical_device(
    instance: &Instance,
    surface: Option<(&ash::khr::surface::Instance, vk::SurfaceKHR)>,
) -> Result<vk::PhysicalDevice, RendererError> {
    let devices = unsafe { instance.enumerate_physical_devices() }.map_err(|e| {
        RendererError::VulkanError("Failed to enumerate Vulkan physical devices".into(), e)
    })?;
    let mut candidates = Vec::with_capacity(devices.len());
    for device in devices {
        let properties = unsafe { instance.get_physical_device_properties(device) };
        let rejection = inspect_device(instance, device, &properties, surface).err();
        let name = unsafe { CStr::from_ptr(properties.device_name.as_ptr()) }
            .to_string_lossy()
            .into_owned();
        if let Some(reason) = &rejection {
            log::debug!("Skipping Vulkan physical device {name}: {reason}");
        }
        candidates.push(DeviceCandidate {
            device,
            name,
            score: device_score(&properties),
            rejection,
        });
    }
    choose_device(&candidates)
}

fn inspect_device(
    instance: &Instance,
    device: vk::PhysicalDevice,
    properties: &vk::PhysicalDeviceProperties,
    surface: Option<(&ash::khr::surface::Instance, vk::SurfaceKHR)>,
) -> Result<(), String> {
    if properties.api_version < vk::API_VERSION_1_3 {
        return Err(format!(
            "Vulkan 1.3 required, reports {}.{}.{}",
            vk::api_version_major(properties.api_version),
            vk::api_version_minor(properties.api_version),
            vk::api_version_patch(properties.api_version)
        ));
    }
    let extensions = unsafe { instance.enumerate_device_extension_properties(device) }
        .map_err(|e| format!("cannot query device extensions: {e:?}"))?;
    let mut missing = missing_extensions(&extensions, surface.is_some());
    let mut vk12 = vk::PhysicalDeviceVulkan12Features::default();
    let mut vk13 = vk::PhysicalDeviceVulkan13Features::default();
    let mut features = vk::PhysicalDeviceFeatures2::default()
        .push_next(&mut vk12)
        .push_next(&mut vk13);
    unsafe {
        instance.get_physical_device_features2(device, &mut features);
    }
    missing.extend(missing_core_features(&features.features));
    missing.extend(missing_vulkan12_features(&vk12));
    missing.extend(missing_vulkan13_features(&vk13));
    if !missing.is_empty() {
        return Err(format!("missing {}", missing.join(", ")));
    }
    let queues = match surface {
        Some((loader, handle)) => {
            let support = SwapchainInfo::query_swapchain_support(loader, device, handle)
                .map_err(|e| format!("cannot query swapchain support: {e}"))?;
            if support.surface_formats.is_empty() || support.present_modes.is_empty() {
                return Err("surface has no formats or presentation modes".into());
            }
            QueueFamilyIndices::find_queue_families(instance, loader, handle, device)
                .map_err(|e| e.to_string())?
        }
        None => QueueFamilyIndices::find_queue_families_headless(instance, device),
    };
    if queues.graphics_idx.is_none() {
        return Err(if surface.is_some() {
            "no graphics+compute queue with presentation support"
        } else {
            "no graphics+compute queue"
        }
        .into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ash::vk::Handle;

    #[test]
    fn test_rejected_high_rank_device_cannot_win_selection() {
        let candidates = [
            DeviceCandidate {
                device: vk::PhysicalDevice::from_raw(1),
                name: "unsupported discrete".into(),
                score: (4, 16384),
                rejection: Some("missing synchronization2".into()),
            },
            DeviceCandidate {
                device: vk::PhysicalDevice::from_raw(2),
                name: "integrated".into(),
                score: (3, 8192),
                rejection: None,
            },
        ];
        assert_eq!(choose_device(&candidates).unwrap(), candidates[1].device);
        let error = choose_device(&candidates[..1]).unwrap_err().to_string();
        assert!(
            error.contains("unsupported discrete: missing synchronization2"),
            "{error}"
        );
        assert!(
            choose_device(&[])
                .unwrap_err()
                .to_string()
                .contains("No Vulkan physical devices")
        );
    }

    #[test]
    fn test_device_type_preference_cannot_be_overridden_by_image_limit() {
        let discrete = vk::PhysicalDeviceProperties {
            device_type: vk::PhysicalDeviceType::DISCRETE_GPU,
            limits: vk::PhysicalDeviceLimits {
                max_image_dimension2_d: 8192,
                ..Default::default()
            },
            ..Default::default()
        };
        let integrated = vk::PhysicalDeviceProperties {
            device_type: vk::PhysicalDeviceType::INTEGRATED_GPU,
            limits: vk::PhysicalDeviceLimits {
                max_image_dimension2_d: 16384,
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(device_score(&discrete) > device_score(&integrated));
    }

    #[test]
    fn test_required_features_match_the_device_creation_request() {
        assert!(missing_core_features(&required_core_features()).is_empty());
        assert!(missing_vulkan12_features(&required_vulkan12_features()).is_empty());
        assert!(missing_vulkan13_features(&required_vulkan13_features()).is_empty());
        let features = required_vulkan13_features().synchronization2(false);
        assert_eq!(missing_vulkan13_features(&features), ["synchronization2"]);
        let features = required_vulkan12_features().runtime_descriptor_array(false);
        assert_eq!(
            missing_vulkan12_features(&features),
            ["runtime_descriptor_array"]
        );
        assert_eq!(
            missing_core_features(&vk::PhysicalDeviceFeatures::default()),
            ["sampler_anisotropy"]
        );
    }

    #[test]
    fn test_swapchain_extension_is_required_only_for_windowed_devices() {
        let mut available = vk::ExtensionProperties::default();
        for (dst, byte) in available
            .extension_name
            .iter_mut()
            .zip(ash::khr::push_descriptor::NAME.to_bytes_with_nul())
        {
            *dst = *byte as _;
        }
        assert!(missing_extensions(&[available], false).is_empty());
        assert_eq!(missing_extensions(&[available], true), ["VK_KHR_swapchain"]);
        assert!(!required_extensions(false).contains(&ash::khr::maintenance4::NAME));
    }
}
