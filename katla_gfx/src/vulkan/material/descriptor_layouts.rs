//! Descriptor layouts retained by the pipelines that allocate and bind them.

use ash::vk;

pub(crate) struct DescriptorLayouts {
    device: ash::Device,
    owned: Vec<vk::DescriptorSetLayout>,
    all: Vec<vk::DescriptorSetLayout>,
}

impl DescriptorLayouts {
    pub(crate) fn new(device: ash::Device) -> Self {
        Self {
            device,
            owned: Vec::new(),
            all: Vec::new(),
        }
    }

    pub(crate) fn push_owned(&mut self, layout: vk::DescriptorSetLayout) {
        self.owned.push(layout);
        self.all.push(layout);
    }

    pub(crate) fn push_borrowed(&mut self, layout: vk::DescriptorSetLayout) {
        self.all.push(layout);
    }

    pub(crate) fn as_slice(&self) -> &[vk::DescriptorSetLayout] {
        &self.all
    }
}

impl Drop for DescriptorLayouts {
    fn drop(&mut self) {
        for layout in self.owned.drain(..) {
            unsafe {
                self.device.destroy_descriptor_set_layout(layout, None);
            }
        }
    }
}
