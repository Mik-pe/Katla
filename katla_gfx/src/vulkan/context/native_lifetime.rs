//! Native parents retained by command pools and their allocated buffers.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use ash::{Device, Entry, Instance, vk};

use super::validation::{self, ValidationCallbackStorage};
use crate::RendererError;

pub(crate) struct NativeInstance {
    _entry: Entry,
    instance: Instance,
    surface_loader: ash::khr::surface::Instance,
    debug_loader: ash::ext::debug_utils::Instance,
    pub(crate) surface: Cell<Option<vk::SurfaceKHR>>,
    pub(crate) debug_callback: Cell<Option<vk::DebugUtilsMessengerEXT>>,
    pub(crate) validation_callback: Arc<Mutex<ValidationCallbackStorage>>,
}

impl NativeInstance {
    pub(crate) fn new(
        entry: Entry,
        instance: Instance,
        validation_active: bool,
    ) -> Result<Rc<Self>, RendererError> {
        let owner = Rc::new(Self {
            surface_loader: ash::khr::surface::Instance::new(&entry, &instance),
            debug_loader: ash::ext::debug_utils::Instance::new(&entry, &instance),
            _entry: entry,
            instance,
            surface: Cell::new(None),
            debug_callback: Cell::new(None),
            validation_callback: Arc::new(Mutex::new(ValidationCallbackStorage::new())),
        });
        let user_data = Arc::as_ptr(&owner.validation_callback).cast_mut().cast();
        let messenger =
            validation::create_debug_messenger(&owner.debug_loader, validation_active, user_data)?;
        owner.debug_callback.set(messenger);
        Ok(owner)
    }

    pub(crate) fn destroy_surface(&self) {
        if let Some(surface) = self.surface.take() {
            unsafe { self.surface_loader.destroy_surface(surface, None) };
        }
    }
}

impl Drop for NativeInstance {
    fn drop(&mut self) {
        self.destroy_surface();
        unsafe {
            if let Some(messenger) = self.debug_callback.take() {
                self.debug_loader
                    .destroy_debug_utils_messenger(messenger, None);
            }
            self.instance.destroy_instance(None);
        }
    }
}

pub(crate) struct NativeDevice {
    pub(crate) device: Device,
    _instance: Rc<NativeInstance>,
}

impl NativeDevice {
    pub(crate) fn new(device: Device, instance: Rc<NativeInstance>) -> Rc<Self> {
        Rc::new(Self {
            device,
            _instance: instance,
        })
    }
}

impl Drop for NativeDevice {
    fn drop(&mut self) {
        unsafe {
            let _ = self.device.device_wait_idle();
            self.device.destroy_device(None);
        }
    }
}
