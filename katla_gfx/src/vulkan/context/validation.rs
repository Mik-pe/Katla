use std::{ffi::CStr, sync::Arc};

use ash::{Entry, ext::debug_utils::Instance as DebugInstance, vk};
use log::debug;

use super::LAYER_KHRONOS_VALIDATION;

/// Validation mode controls the level of Vulkan validation enabled.
///
/// Each mode includes all features from previous modes plus additional checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ValidationMode {
    /// No validation layers enabled.
    #[default]
    Disabled,
    /// Standard validation with synchronization checks.
    /// Catches common API usage errors and sync hazards.
    Enabled,
    /// GPU-assisted validation in addition to standard validation.
    /// Uses the GPU to detect additional issues like out-of-bounds descriptor access,
    /// uninitialized descriptors, and more. Requires additional descriptor bindings.
    GpuAssisted,
}

impl ValidationMode {
    /// Returns true if any validation is enabled.
    pub fn is_enabled(&self) -> bool {
        matches!(self, Self::Enabled | Self::GpuAssisted)
    }

    /// Returns true if GPU-assisted validation is enabled.
    pub fn is_gpu_assisted(&self) -> bool {
        matches!(self, Self::GpuAssisted)
    }
}

/// Validation message severity level (internal).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ValidationSeverity {
    Verbose,
    Info,
    Warning,
    Error,
}

impl std::fmt::Display for ValidationSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationSeverity::Verbose => write!(f, "VERBOSE"),
            ValidationSeverity::Info => write!(f, "INFO"),
            ValidationSeverity::Warning => write!(f, "WARNING"),
            ValidationSeverity::Error => write!(f, "ERROR"),
        }
    }
}

impl From<vk::DebugUtilsMessageSeverityFlagsEXT> for ValidationSeverity {
    fn from(severity: vk::DebugUtilsMessageSeverityFlagsEXT) -> Self {
        if severity.contains(vk::DebugUtilsMessageSeverityFlagsEXT::ERROR) {
            ValidationSeverity::Error
        } else if severity.contains(vk::DebugUtilsMessageSeverityFlagsEXT::WARNING) {
            ValidationSeverity::Warning
        } else if severity.contains(vk::DebugUtilsMessageSeverityFlagsEXT::INFO) {
            ValidationSeverity::Info
        } else {
            ValidationSeverity::Verbose
        }
    }
}

/// A validation message from Vulkan validation layers (internal).
#[derive(Debug, Clone)]
pub(crate) struct ValidationMessage {
    pub severity: ValidationSeverity,
    pub message: String,
    /// VUID (Vulkan Unique ID) if present, e.g., "VUID-vkCmdDraw-None-02700"
    pub vuid: Option<String>,
}

/// Type for validation callbacks (internal).
///
/// The callback receives a reference to the validation message and should return
/// `false` to continue execution or `true` to trigger a breakpoint.
pub(crate) type ValidationCallback = dyn FnMut(&ValidationMessage) -> bool + Send + Sync;

/// Validation message level for user callbacks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationLevel {
    Error,
    Warning,
    Info,
    Debug,
}

impl From<ValidationSeverity> for ValidationLevel {
    fn from(severity: ValidationSeverity) -> Self {
        match severity {
            ValidationSeverity::Error => ValidationLevel::Error,
            ValidationSeverity::Warning => ValidationLevel::Warning,
            ValidationSeverity::Info => ValidationLevel::Info,
            ValidationSeverity::Verbose => ValidationLevel::Debug,
        }
    }
}

/// Internal storage for validation callbacks.
pub(crate) struct ValidationCallbackStorage {
    pub(crate) callback: Option<Box<ValidationCallback>>,
    #[allow(clippy::type_complexity)]
    pub(crate) simplified_callback: Option<Box<dyn FnMut(&str, ValidationLevel) + Send + Sync>>,
    pub(crate) messages: Vec<ValidationMessage>,
}

impl ValidationCallbackStorage {
    pub(crate) fn new() -> Self {
        Self {
            callback: None,
            simplified_callback: None,
            messages: Vec::new(),
        }
    }

    pub(crate) fn call(&mut self, msg: &ValidationMessage) -> bool {
        self.messages.push(msg.clone());

        if let Some(ref mut cb) = self.simplified_callback {
            cb(&msg.message, ValidationLevel::from(msg.severity));
        }

        if let Some(ref mut cb) = self.callback {
            cb(msg)
        } else {
            false
        }
    }

    pub(crate) fn set_callback(&mut self, callback: Box<ValidationCallback>) {
        self.callback = Some(callback);
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn set_simplified_callback(
        &mut self,
        callback: Box<dyn FnMut(&str, ValidationLevel) + Send + Sync>,
    ) {
        self.simplified_callback = Some(callback);
    }
}

pub(super) fn create_debug_messenger(
    debug_utils_loader: &DebugInstance,
    with_validation_layers: bool,
    user_data: *mut std::ffi::c_void,
) -> Option<vk::DebugUtilsMessengerEXT> {
    if with_validation_layers {
        let create_info = vk::DebugUtilsMessengerCreateInfoEXT::default()
            .message_severity(
                vk::DebugUtilsMessageSeverityFlagsEXT::VERBOSE
                    | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
                    | vk::DebugUtilsMessageSeverityFlagsEXT::ERROR,
            )
            .message_type(
                vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                    | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
            )
            .pfn_user_callback(Some(debug_callback))
            .user_data(user_data);

        Some(
            unsafe { debug_utils_loader.create_debug_utils_messenger(&create_info, None) }.unwrap(),
        )
    } else {
        None
    }
}

unsafe extern "system" fn debug_callback(
    message_severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    _message_types: vk::DebugUtilsMessageTypeFlagsEXT,
    p_callback_data: *const vk::DebugUtilsMessengerCallbackDataEXT,
    p_user_data: *mut std::ffi::c_void,
) -> vk::Bool32 {
    unsafe {
        let callback_data = &*p_callback_data;

        let severity = ValidationSeverity::from(message_severity);
        let message = CStr::from_ptr(callback_data.p_message)
            .to_string_lossy()
            .to_string();

        let vuid = if !callback_data.p_message_id_name.is_null() {
            let id_name = CStr::from_ptr(callback_data.p_message_id_name);
            let id_str = id_name.to_string_lossy();
            if id_str.starts_with("VUID-") {
                Some(id_str.to_string())
            } else {
                None
            }
        } else {
            message
                .split_whitespace()
                .find(|s| s.starts_with("VUID-"))
                .map(|s| s.to_string())
        };

        let validation_msg = ValidationMessage {
            severity,
            message,
            vuid,
        };

        let storage =
            Arc::from_raw(p_user_data as *const std::sync::Mutex<ValidationCallbackStorage>);
        let mut storage_guard = storage.lock().unwrap();
        let should_break = storage_guard.call(&validation_msg);
        drop(storage_guard);
        let _ = Arc::into_raw(storage);

        debug!(
            "{}",
            CStr::from_ptr(callback_data.p_message).to_string_lossy()
        );

        if should_break { vk::TRUE } else { vk::FALSE }
    }
}

pub(super) fn check_validation_support(entry: &Entry) -> bool {
    unsafe {
        let available_layers = entry.enumerate_instance_layer_properties().unwrap();
        let validation_name = CStr::from_ptr(LAYER_KHRONOS_VALIDATION.as_ptr() as *const i8);
        for layer in available_layers {
            let layer_name = std::ffi::CStr::from_ptr(layer.layer_name.as_ptr() as _);
            if layer_name == validation_name {
                return true;
            }
        }
    }

    false
}

impl super::VulkanContext {
    /// Set a callback for validation messages.
    ///
    /// The callback receives the message text and severity level.
    ///
    /// # Example
    /// ```no_run
    /// use katla_gfx::{VulkanContext, ValidationLevel};
    ///
    /// # let context: VulkanContext = unsafe { std::mem::zeroed() };
    /// context.set_validation_callback(|message: &str, level: ValidationLevel| {
    ///     println!("[{:?}] {}", level, message);
    /// });
    /// ```
    pub fn set_validation_callback<F>(&self, callback: F)
    where
        F: FnMut(&str, ValidationLevel) + Send + Sync + 'static,
    {
        let mut storage = self.validation_callback.lock().unwrap();
        storage.set_simplified_callback(Box::new(callback));
    }

    /// Set a detailed validation callback (internal use).
    ///
    /// This allows tests to receive full validation message details including VUIDs.
    pub(crate) fn set_validation_callback_detailed(&self, callback: Box<ValidationCallback>) {
        let mut storage = self.validation_callback.lock().unwrap();
        storage.set_callback(callback);
    }

    /// Set up default logging callback that logs validation messages at appropriate levels.
    ///
    /// - Error messages → `error!`
    /// - Warning messages → `warn!`
    /// - Info messages → `info!`
    /// - Verbose messages → `debug!`
    pub fn setup_validation_logging(&self) {
        let is_gpu_av = self.gpu_assisted_validation;

        self.set_validation_callback_detailed(Box::new(move |msg| {
            // GPU-AV false positive on Intel: when multiple passes with different pipelines
            // share a command buffer, GPU-AV can report wrong descriptor bound ranges for
            // storage buffer access. The reported range matches a push descriptor buffer size
            // rather than the actual descriptor range. Core validation catches real OOB errors.
            // This is confirmed absent in single-pass GPU-AV (e.g. particle_validation example).
            // Ref: https://github.com/KhronosGroup/Vulkan-ValidationLayers/issues/7737
            if is_gpu_av
                && let Some(ref vuid) = msg.vuid
                && vuid.contains("storageBuffers-06936")
            {
                return false;
            }

            let prefix = if let Some(ref vuid) = msg.vuid {
                format!("[{}]", vuid)
            } else {
                String::new()
            };

            match msg.severity {
                ValidationSeverity::Error => {
                    log::error!("{} {}", prefix, msg.message);
                }
                ValidationSeverity::Warning => {
                    log::warn!("{} {}", prefix, msg.message);
                }
                ValidationSeverity::Info => {
                    log::info!("{} {}", prefix, msg.message);
                }
                ValidationSeverity::Verbose => {
                    log::debug!("{} {}", prefix, msg.message);
                }
            }
            false
        }));
    }
}
