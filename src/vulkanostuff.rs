use std::sync::Arc;
use vulkano::device::{Device, DeviceExtensions, Features};
use vulkano::instance::{Instance, InstanceExtensions, PhysicalDevice};

pub struct VulkanoCtx {
    id: i32,
    instance: Arc<Instance>,
}

impl VulkanoCtx {
    pub fn init() -> Self {
        let instance = Instance::new(None, &InstanceExtensions::none(), None)
            .expect("failed to create instance");
        let physical = PhysicalDevice::enumerate(&instance)
            .next()
            .expect("no device available");
        for family in physical.queue_families() {
            println!(
                "Found a queue family with {:?} queue(s)",
                family.queues_count()
            );
        }

        println!("Physical device: {:?}", physical.name());
        let queue_family = physical
            .queue_families()
            .find(|&q| q.supports_graphics())
            .expect("couldn't find a graphical queue family");

        let (device, mut queues) = {
            Device::new(
                physical,
                &Features::none(),
                &DeviceExtensions::none(),
                [(queue_family, 0.5)].iter().cloned(),
            )
            .expect("failed to create device")
        };
        let queue = queues.next().unwrap();
        Self { id: 0, instance }
    }
}
