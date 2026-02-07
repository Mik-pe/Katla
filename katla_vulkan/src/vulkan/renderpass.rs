use ash::{vk, Device};

pub struct RenderPass {
    vk_renderpass: vk::RenderPass,
    device: Device,
}

impl Clone for RenderPass {
    fn clone(&self) -> Self {
        Self {
            vk_renderpass: self.vk_renderpass,
            device: self.device.clone(),
        }
    }
}

impl RenderPass {
    /// Create an opaque render pass with default formats.
    ///
    /// This is a convenience method that uses common default formats:
    /// - Color: R8G8B8A8_SRGB
    /// - Depth: D32_SFLOAT
    ///
    /// This method does not expose ash types and is suitable for use
    /// in integration tests where ash should not be a dependency.
    pub fn create_default_opaque(device: Device) -> Self {
        Self::create_opaque(
            device,
            vk::Format::R8G8B8A8_SRGB,
            vk::Format::D32_SFLOAT,
        )
    }

    pub fn create_opaque(
        device: Device,
        color_format: vk::Format,
        depth_format: vk::Format,
    ) -> Self {
        let color_attachment = vk::AttachmentDescription::default()
            .format(color_format)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
            .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .final_layout(vk::ImageLayout::PRESENT_SRC_KHR);

        let depth_attachment = vk::AttachmentDescription::default()
            .format(depth_format)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::DONT_CARE)
            .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
            .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .final_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL);

        let attachments = [color_attachment, depth_attachment];

        let color_attachment_refs = [vk::AttachmentReference::default()
            .attachment(0)
            .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)];
        let depth_attachment_ref = vk::AttachmentReference::default()
            .attachment(1)
            .layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL);
        let subpasses = [vk::SubpassDescription::default()
            .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
            .color_attachments(&color_attachment_refs)
            .depth_stencil_attachment(&depth_attachment_ref)];
        let dependencies = [vk::SubpassDependency::default()
            .src_subpass(vk::SUBPASS_EXTERNAL)
            .dst_subpass(0)
            .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
            .src_access_mask(vk::AccessFlags::empty())
            .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
            .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)];

        let create_info = vk::RenderPassCreateInfo::default()
            .attachments(&attachments)
            .subpasses(&subpasses)
            .dependencies(&dependencies);

        let vk_renderpass = unsafe { device.create_render_pass(&create_info, None) }.unwrap();

        Self {
            vk_renderpass,
            device,
        }
    }

    pub fn get_vk_renderpass(&self) -> vk::RenderPass {
        self.vk_renderpass
    }

    /// Create a render pass from custom attachments, subpasses, and dependencies.
    /// This provides full flexibility for creating any render pass structure.
    ///
    /// # Arguments
    /// * `device` - The Vulkan device
    /// * `attachments` - Array of attachment descriptions
    /// * `subpasses` - Array of subpass descriptions
    /// * `dependencies` - Array of subpass dependencies (can be empty)
    pub fn create_from_config(
        device: Device,
        attachments: &[vk::AttachmentDescription],
        subpasses: &[vk::SubpassDescription],
        dependencies: &[vk::SubpassDependency],
    ) -> Result<Self, vk::Result> {
        let create_info = vk::RenderPassCreateInfo::default()
            .attachments(attachments)
            .subpasses(subpasses)
            .dependencies(dependencies);

        let vk_renderpass = unsafe { device.create_render_pass(&create_info, None)? };

        Ok(Self {
            vk_renderpass,
            device,
        })
    }

    pub fn destroy(&self) {
        unsafe {
            self.device.destroy_render_pass(self.vk_renderpass, None);
        }
    }
}
