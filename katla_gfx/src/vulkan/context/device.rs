use std::ffi::CStr;

use ash::{Device, Entry, Instance};

use crate::error::RendererError;

use super::*;

pub(super) fn create_device(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    queue_create_infos: &[vk::DeviceQueueCreateInfo<'_>],
    enable_swapchain: bool,
) -> Result<Device, RendererError> {
    let device_extensions: Vec<_> = super::physical_device::required_extensions(enable_swapchain)
        .iter()
        .map(|name| name.as_ptr())
        .collect();
    let features = super::physical_device::required_core_features();
    let mut vk12_features = super::physical_device::required_vulkan12_features();
    let mut vk13_features = super::physical_device::required_vulkan13_features();
    let create_info = vk::DeviceCreateInfo::default()
        .enabled_extension_names(&device_extensions)
        .queue_create_infos(queue_create_infos)
        .enabled_features(&features)
        .push_next(&mut vk12_features)
        .push_next(&mut vk13_features);
    unsafe { instance.create_device(physical_device, &create_info, None) }
        .map_err(|e| RendererError::VulkanError("Failed to create Vulkan device".into(), e))
}

impl VulkanContext {
    pub(super) fn create_instance(
        validation_mode: ValidationMode,
        app_name: &CStr,
        engine_name: &CStr,
        display: Option<&dyn raw_window_handle::HasDisplayHandle>,
        entry: &Entry,
    ) -> Result<(Instance, bool), RendererError> {
        match create_instance_inner(validation_mode, app_name, engine_name, display, entry) {
            Ok(result) => Ok(result),
            Err(e) if validation_mode.is_enabled() => {
                log::warn!(
                    "Vulkan instance creation with validation layers failed: {}",
                    e
                );
                log::warn!("Retrying without validation layers");
                create_instance_inner(
                    ValidationMode::Disabled,
                    app_name,
                    engine_name,
                    display,
                    entry,
                )
            }
            Err(e) => Err(e),
        }
    }
}

fn create_instance_inner(
    validation_mode: ValidationMode,
    app_name: &CStr,
    engine_name: &CStr,
    display: Option<&dyn raw_window_handle::HasDisplayHandle>,
    entry: &Entry,
) -> Result<(Instance, bool), RendererError> {
    use ash::vk::{self, ValidationFeatureEnableEXT, ValidationFeaturesEXT};

    let mut extension_names_raw = if let Some(d) = display {
        let display_handle = d.display_handle().map_err(|e| {
            RendererError::InitializationFailed(format!("Failed to get display handle: {:?}", e))
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
        // Log all available instance layers for diagnostics
        unsafe {
            match entry.enumerate_instance_layer_properties() {
                Ok(layers) => {
                    log::info!("Available Vulkan instance layers ({} total):", layers.len());
                    for layer in &layers {
                        let name = std::ffi::CStr::from_ptr(layer.layer_name.as_ptr() as _);
                        let desc = std::ffi::CStr::from_ptr(layer.description.as_ptr() as _);
                        log::info!(
                            "  {} (v{}.{}.{}) - {}",
                            name.to_string_lossy(),
                            vk::api_version_major(layer.spec_version),
                            vk::api_version_minor(layer.spec_version),
                            vk::api_version_patch(layer.spec_version),
                            desc.to_string_lossy()
                        );
                    }
                }
                Err(e) => {
                    log::warn!("Failed to enumerate instance layers: {:?}", e);
                }
            }
        }

        if !validation::check_validation_support(entry) {
            log::error!("VK_LAYER_KHRONOS_validation NOT FOUND - validation layers are NOT active");
            log::error!("On macOS, install the Vulkan SDK from https://vulkan.lunarg.com/sdk/home");
            log::error!("And ensure VK_ICD_FILENAMES and VK_LAYER_PATH are set correctly");
            log::warn!("Falling back to no validation layers");
            return create_instance_inner(
                ValidationMode::Disabled,
                app_name,
                engine_name,
                display,
                entry,
            );
        }
        log::info!("VK_LAYER_KHRONOS_validation found, enabling validation layers");
        extension_names_raw.push(ash::ext::debug_utils::NAME.as_ptr());
        instance_layers.push(LAYER_KHRONOS_VALIDATION.as_ptr().cast::<std::ffi::c_char>());
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
            ValidationFeaturesEXT::default().enabled_validation_features(&gpu_assisted_features),
        ),
        ValidationMode::Enabled => {
            Some(ValidationFeaturesEXT::default().enabled_validation_features(&standard_features))
        }
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
        entry
            .create_instance(&create_info, None)
            .map(|instance| (instance, !instance_layers.is_empty()))
            .map_err(|e| {
                RendererError::InitializationFailed(format!(
                    "Failed to create Vulkan instance: {:?}",
                    e
                ))
            })
    }
}

impl VulkanContext {
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

            let has_features = if tiling == vk::ImageTiling::LINEAR {
                format_props.linear_tiling_features & features == features
            } else {
                format_props.optimal_tiling_features & features == features
            };

            if has_features {
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
