use std::ffi::CStr;

use ash::{Device, Entry, Instance};
use log::info;

use crate::error::RendererError;

use super::*;

impl QueueFamilyIndices {
    pub fn find_queue_families(
        instance: &Instance,
        surface_loader: &ash::khr::surface::Instance,
        surface: vk::SurfaceKHR,
        physical_device: vk::PhysicalDevice,
    ) -> Result<Self, RendererError> {
        let mut queue_family_indices = Self {
            graphics_idx: None,
            transfer_idx: None,
        };
        unsafe {
            let family_props =
                instance.get_physical_device_queue_family_properties(physical_device);
            info!("Num family indices: {}", family_props.len());
            for (idx, properties) in family_props.iter().enumerate() {
                let surface_support = surface_loader
                    .get_physical_device_surface_support(physical_device, idx as u32, surface)
                    .map_err(|e| {
                        RendererError::VulkanError(format!(
                            "Failed to query surface support for queue family {}: {:?}",
                            idx, e
                        ))
                    })?;

                if properties.queue_flags.contains(vk::QueueFlags::GRAPHICS)
                    && surface_support
                    && queue_family_indices.graphics_idx.is_none()
                {
                    queue_family_indices.graphics_idx = Some(idx as u32);
                    continue;
                }

                if properties.queue_flags.contains(vk::QueueFlags::TRANSFER)
                    && surface_support
                    && queue_family_indices.transfer_idx.is_none()
                {
                    queue_family_indices.transfer_idx = Some(idx as u32);
                    continue;
                }
            }
        };

        Ok(queue_family_indices)
    }

    /// Find queue families for headless rendering (without surface support check).
    /// This is used when VK_EXT_headless_surface is available and we don't need
    /// presentation capabilities.
    pub fn find_queue_families_headless(
        instance: &Instance,
        physical_device: vk::PhysicalDevice,
    ) -> Self {
        let mut queue_family_indices = Self {
            graphics_idx: None,
            transfer_idx: None,
        };
        unsafe {
            let family_props =
                instance.get_physical_device_queue_family_properties(physical_device);
            info!("Num family indices (headless): {}", family_props.len());
            for (idx, properties) in family_props.iter().enumerate() {
                if properties.queue_flags.contains(vk::QueueFlags::GRAPHICS)
                    && queue_family_indices.graphics_idx.is_none()
                {
                    queue_family_indices.graphics_idx = Some(idx as u32);
                    continue;
                }

                if properties.queue_flags.contains(vk::QueueFlags::TRANSFER)
                    && !properties.queue_flags.contains(vk::QueueFlags::GRAPHICS)
                    && queue_family_indices.transfer_idx.is_none()
                {
                    queue_family_indices.transfer_idx = Some(idx as u32);
                    continue;
                }
            }

            if queue_family_indices.transfer_idx.is_none() {
                queue_family_indices.transfer_idx = queue_family_indices.graphics_idx;
            }
        };

        queue_family_indices
    }
}

pub(super) fn create_device(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    queue_create_infos: Vec<vk::DeviceQueueCreateInfo>,
    with_validation_layers: bool,
    enable_swapchain: bool,
) -> Result<Device, RendererError> {
    let device_extensions = if enable_swapchain {
        vec![
            ash::khr::swapchain::NAME.as_ptr(),
            ash::khr::push_descriptor::NAME.as_ptr(),
            ash::khr::maintenance4::NAME.as_ptr(),
        ]
    } else {
        vec![
            ash::khr::push_descriptor::NAME.as_ptr(),
            ash::khr::maintenance4::NAME.as_ptr(),
        ]
    };

    let mut device_layers = vec![];
    if with_validation_layers {
        device_layers.push(LAYER_KHRONOS_VALIDATION.as_ptr() as *const i8);
    }

    let vk13_features = vk::PhysicalDeviceVulkan13Features {
        s_type: vk::StructureType::PHYSICAL_DEVICE_VULKAN_1_3_FEATURES,
        p_next: std::ptr::null_mut(),
        dynamic_rendering: vk::TRUE,
        synchronization2: vk::TRUE,
        ..Default::default()
    };

    let mut vk12_features = vk::PhysicalDeviceVulkan12Features {
        s_type: vk::StructureType::PHYSICAL_DEVICE_VULKAN_1_2_FEATURES,
        p_next: &vk13_features as *const _ as *mut _,
        buffer_device_address: vk::TRUE,
        descriptor_indexing: vk::TRUE,
        shader_sampled_image_array_non_uniform_indexing: vk::TRUE,
        descriptor_binding_sampled_image_update_after_bind: vk::TRUE,
        descriptor_binding_storage_buffer_update_after_bind: vk::TRUE,
        descriptor_binding_partially_bound: vk::TRUE,
        descriptor_binding_variable_descriptor_count: vk::TRUE,
        runtime_descriptor_array: vk::TRUE,
        ..Default::default()
    };

    let features = vk::PhysicalDeviceFeatures {
        sampler_anisotropy: 1,
        ..Default::default()
    };

    let create_info = vk::DeviceCreateInfo::default()
        .enabled_extension_names(&device_extensions)
        .queue_create_infos(&queue_create_infos)
        .enabled_features(&features)
        .push_next(&mut vk12_features);

    unsafe {
        instance
            .create_device(physical_device, &create_info, None)
            .map_err(|e| {
                RendererError::InitializationFailed(format!(
                    "Failed to create Vulkan device: {:?}",
                    e
                ))
            })
    }
}

pub(super) unsafe fn pick_physical_device(
    instance: &Instance,
    surface_loader: &ash::khr::surface::Instance,
    surface: vk::SurfaceKHR,
) -> Result<vk::PhysicalDevice, RendererError> {
    let physical_devices = unsafe { instance.enumerate_physical_devices() }.map_err(|e| {
        RendererError::InitializationFailed(format!(
            "Failed to enumerate physical devices: {:?}",
            e
        ))
    })?;

    let physical_device = physical_devices.into_iter().max_by_key(|pd| unsafe {
        is_physical_device_suitable(instance, surface_loader, *pd, surface)
    });

    let device = physical_device.ok_or_else(|| {
        RendererError::InitializationFailed("No suitable physical device found".to_string())
    })?;

    unsafe {
        let properties = instance.get_physical_device_properties(device);
        info!(
            "Picking physical device: {:?}",
            CStr::from_ptr(properties.device_name.as_ptr())
        );
    }

    Ok(device)
}

pub(super) unsafe fn is_physical_device_suitable(
    instance: &Instance,
    surface_loader: &ash::khr::surface::Instance,
    physical_device: vk::PhysicalDevice,
    surface: vk::SurfaceKHR,
) -> u32 {
    unsafe {
        let properties = instance.get_physical_device_properties(physical_device);
        let mut score = 0;

        match properties.device_type {
            vk::PhysicalDeviceType::DISCRETE_GPU => score += 1000,
            vk::PhysicalDeviceType::INTEGRATED_GPU => score += 100,
            vk::PhysicalDeviceType::CPU => score += 10,
            _ => {}
        }

        score += properties.limits.max_image_dimension2_d;

        let swapchain_support = match SwapchainInfo::query_swapchain_support(
            surface_loader,
            physical_device,
            surface,
        ) {
            Ok(support) => support,
            Err(_) => return 0,
        };

        if swapchain_support.surface_formats.is_empty()
            && swapchain_support.present_modes.is_empty()
        {
            score = 0;
        }

        score
    }
}

/// Pick a physical device for headless rendering.
/// Simplified version that doesn't require swapchain support.
pub(super) unsafe fn pick_physical_device_headless(
    instance: &Instance,
) -> Result<vk::PhysicalDevice, RendererError> {
    let physical_devices = unsafe { instance.enumerate_physical_devices() }.map_err(|e| {
        RendererError::InitializationFailed(format!(
            "Failed to enumerate physical devices: {:?}",
            e
        ))
    })?;

    let physical_device = physical_devices.into_iter().max_by_key(|physical_device| {
        let mut score = 0u32;
        unsafe {
            let properties = instance.get_physical_device_properties(*physical_device);
            match properties.device_type {
                vk::PhysicalDeviceType::DISCRETE_GPU => score += 1000,
                vk::PhysicalDeviceType::INTEGRATED_GPU => score += 100,
                vk::PhysicalDeviceType::CPU => score += 10,
                _ => {}
            }
            score += properties.limits.max_image_dimension2_d;
        }
        score
    });

    let device = physical_device.ok_or_else(|| {
        RendererError::InitializationFailed(
            "No suitable physical device found for headless rendering".to_string(),
        )
    })?;

    unsafe {
        let properties = instance.get_physical_device_properties(device);
        info!(
            "Picking physical device (headless): {:?}",
            CStr::from_ptr(properties.device_name.as_ptr())
        );
    }

    Ok(device)
}

impl VulkanContext {
    pub(super) fn create_instance(
        validation_mode: ValidationMode,
        app_name: &CStr,
        engine_name: &CStr,
        display: Option<&dyn raw_window_handle::HasDisplayHandle>,
        entry: &Entry,
    ) -> Result<Instance, RendererError> {
        use ash::vk::{self, ValidationFeatureEnableEXT, ValidationFeaturesEXT};

        if validation_mode.is_enabled() && !validation::check_validation_support(entry) {
            return Err(RendererError::InitializationFailed(
                "Validation layers requested, but unavailable".to_string(),
            ));
        }

        let mut extension_names_raw = if let Some(d) = display {
            let display_handle = d.display_handle().map_err(|e| {
                RendererError::InitializationFailed(format!(
                    "Failed to get display handle: {:?}",
                    e
                ))
            })?;
            ash_window::enumerate_required_extensions(display_handle.as_raw())
                .map_err(|e| {
                    RendererError::InitializationFailed(format!(
                        "Failed to enumerate required extensions: {:?}",
                        e
                    ))
                })?
                .to_vec()
        } else {
            vec![]
        };

        let mut instance_layers = vec![];
        if validation_mode.is_enabled() {
            extension_names_raw.push(ash::ext::debug_utils::NAME.as_ptr());
            instance_layers.push(LAYER_KHRONOS_VALIDATION.as_ptr() as *const i8);
        }

        let app_info = vk::ApplicationInfo::default()
            .application_name(app_name)
            .application_version(0)
            .engine_name(engine_name)
            .engine_version(0)
            .api_version(vk::make_api_version(0, 1, 3, 0));

        let gpu_assisted_features = [
            ValidationFeatureEnableEXT::SYNCHRONIZATION_VALIDATION,
            ValidationFeatureEnableEXT::GPU_ASSISTED,
            ValidationFeatureEnableEXT::GPU_ASSISTED_RESERVE_BINDING_SLOT,
        ];
        let standard_features = [ValidationFeatureEnableEXT::SYNCHRONIZATION_VALIDATION];

        let mut validation_features = match validation_mode {
            ValidationMode::GpuAssisted => Some(
                ValidationFeaturesEXT::default()
                    .enabled_validation_features(&gpu_assisted_features),
            ),
            ValidationMode::Enabled => Some(
                ValidationFeaturesEXT::default().enabled_validation_features(&standard_features),
            ),
            ValidationMode::Disabled => None,
        };

        let mut create_info = vk::InstanceCreateInfo::default()
            .application_info(&app_info)
            .enabled_extension_names(&extension_names_raw)
            .enabled_layer_names(&instance_layers);

        if let Some(ref mut features) = validation_features {
            create_info = create_info.push_next(features);
        }

        unsafe {
            entry.create_instance(&create_info, None).map_err(|e| {
                RendererError::InitializationFailed(format!(
                    "Failed to create Vulkan instance: {:?}",
                    e
                ))
            })
        }
    }

    pub fn find_supported_format(
        &self,
        candidates: Vec<vk::Format>,
        tiling: vk::ImageTiling,
        features: vk::FormatFeatureFlags,
    ) -> Result<vk::Format, RendererError> {
        for candidate in candidates {
            let format_props = unsafe {
                self.instance
                    .get_physical_device_format_properties(self.physical_device, candidate)
            };

            let has_features = format_props.optimal_tiling_features & features == features;

            if has_features
                && (tiling == vk::ImageTiling::LINEAR || tiling == vk::ImageTiling::OPTIMAL)
            {
                return Ok(candidate);
            }
        }

        Err(RendererError::NotFound(
            "No acceptable format found".to_string(),
        ))
    }

    pub fn find_depth_format(&self) -> Result<vk::Format, RendererError> {
        let candidates = vec![
            vk::Format::D32_SFLOAT_S8_UINT,
            vk::Format::D32_SFLOAT,
            vk::Format::D24_UNORM_S8_UINT,
        ];
        let tiling = vk::ImageTiling::OPTIMAL;
        let features = vk::FormatFeatureFlags::DEPTH_STENCIL_ATTACHMENT;
        self.find_supported_format(candidates, tiling, features)
    }
}
