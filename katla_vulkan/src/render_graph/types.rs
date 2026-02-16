#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    R8G8B8A8Srgb,
    B8G8R8A8Srgb,
    D32Sfloat,
    D32SfloatS8Uint,
    D24UnormS8Uint,
    D16Unorm,
    R32Sfloat,
}

impl From<ImageFormat> for ash::vk::Format {
    fn from(format: ImageFormat) -> Self {
        match format {
            ImageFormat::R8G8B8A8Srgb => ash::vk::Format::R8G8B8A8_SRGB,
            ImageFormat::B8G8R8A8Srgb => ash::vk::Format::B8G8R8A8_SRGB,
            ImageFormat::D32Sfloat => ash::vk::Format::D32_SFLOAT,
            ImageFormat::D32SfloatS8Uint => ash::vk::Format::D32_SFLOAT_S8_UINT,
            ImageFormat::D24UnormS8Uint => ash::vk::Format::D24_UNORM_S8_UINT,
            ImageFormat::D16Unorm => ash::vk::Format::D16_UNORM,
            ImageFormat::R32Sfloat => ash::vk::Format::R32_SFLOAT,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageLayout {
    Undefined,
    General,
    ColorAttachmentOptimal,
    DepthStencilAttachmentOptimal,
    DepthStencilReadOnlyOptimal,
    ShaderReadOnlyOptimal,
    TransferSrcOptimal,
    TransferDstOptimal,
    Preinitialized,
}

impl ImageLayout {
    /// Convert from a raw vk::ImageLayout (for backward compatibility).
    pub fn from_vk(layout: ash::vk::ImageLayout) -> Self {
        match layout {
            ash::vk::ImageLayout::UNDEFINED => ImageLayout::Undefined,
            ash::vk::ImageLayout::GENERAL => ImageLayout::General,
            ash::vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL => ImageLayout::ColorAttachmentOptimal,
            ash::vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL => {
                ImageLayout::DepthStencilAttachmentOptimal
            }
            ash::vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL => {
                ImageLayout::DepthStencilReadOnlyOptimal
            }
            ash::vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL => ImageLayout::ShaderReadOnlyOptimal,
            ash::vk::ImageLayout::TRANSFER_SRC_OPTIMAL => ImageLayout::TransferSrcOptimal,
            ash::vk::ImageLayout::TRANSFER_DST_OPTIMAL => ImageLayout::TransferDstOptimal,
            ash::vk::ImageLayout::PREINITIALIZED => ImageLayout::Preinitialized,
            _ => ImageLayout::Undefined, // Fallback for unknown layouts
        }
    }
}

impl From<ImageLayout> for ash::vk::ImageLayout {
    fn from(layout: ImageLayout) -> Self {
        match layout {
            ImageLayout::Undefined => ash::vk::ImageLayout::UNDEFINED,
            ImageLayout::General => ash::vk::ImageLayout::GENERAL,
            ImageLayout::ColorAttachmentOptimal => ash::vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            ImageLayout::DepthStencilAttachmentOptimal => {
                ash::vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL
            }
            ImageLayout::DepthStencilReadOnlyOptimal => {
                ash::vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL
            }
            ImageLayout::ShaderReadOnlyOptimal => ash::vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            ImageLayout::TransferSrcOptimal => ash::vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            ImageLayout::TransferDstOptimal => ash::vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            ImageLayout::Preinitialized => ash::vk::ImageLayout::PREINITIALIZED,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachmentLoadOp {
    Load,
    Clear,
    DontCare,
}

impl From<AttachmentLoadOp> for ash::vk::AttachmentLoadOp {
    fn from(op: AttachmentLoadOp) -> Self {
        match op {
            AttachmentLoadOp::Load => ash::vk::AttachmentLoadOp::LOAD,
            AttachmentLoadOp::Clear => ash::vk::AttachmentLoadOp::CLEAR,
            AttachmentLoadOp::DontCare => ash::vk::AttachmentLoadOp::DONT_CARE,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachmentStoreOp {
    Store,
    DontCare,
}

impl From<AttachmentStoreOp> for ash::vk::AttachmentStoreOp {
    fn from(op: AttachmentStoreOp) -> Self {
        match op {
            AttachmentStoreOp::Store => ash::vk::AttachmentStoreOp::STORE,
            AttachmentStoreOp::DontCare => ash::vk::AttachmentStoreOp::DONT_CARE,
        }
    }
}

//=============================================================================
// Vulkan Handle Wrappers (for type safety in render graph API)
//=============================================================================

/// Wrapper for vk::ImageView to avoid exposing ash types in public API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VkImageView(pub(crate) ash::vk::ImageView);

impl VkImageView {
    /// Create a new VkImageView wrapper.
    pub fn new(view: ash::vk::ImageView) -> Self {
        Self(view)
    }

    /// Get the underlying Vulkan image view handle.
    pub fn vk(&self) -> ash::vk::ImageView {
        self.0
    }
}

impl From<ash::vk::ImageView> for VkImageView {
    fn from(view: ash::vk::ImageView) -> Self {
        Self(view)
    }
}

impl From<VkImageView> for ash::vk::ImageView {
    fn from(view: VkImageView) -> Self {
        view.0
    }
}

//=============================================================================
// Extent Types
//=============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Extent2D {
    pub width: u32,
    pub height: u32,
}

impl Extent2D {
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }
}

impl From<Extent2D> for ash::vk::Extent2D {
    fn from(extent: Extent2D) -> Self {
        ash::vk::Extent2D {
            width: extent.width,
            height: extent.height,
        }
    }
}

impl From<ash::vk::Extent2D> for Extent2D {
    fn from(extent: ash::vk::Extent2D) -> Self {
        Self {
            width: extent.width,
            height: extent.height,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Extent3D {
    pub width: u32,
    pub height: u32,
    pub depth: u32,
}

impl Extent3D {
    pub fn new(width: u32, height: u32, depth: u32) -> Self {
        Self {
            width,
            height,
            depth,
        }
    }

    pub fn from_2d(extent: Extent2D) -> Self {
        Self {
            width: extent.width,
            height: extent.height,
            depth: 1,
        }
    }
}

impl From<Extent3D> for ash::vk::Extent3D {
    fn from(extent: Extent3D) -> Self {
        ash::vk::Extent3D {
            width: extent.width,
            height: extent.height,
            depth: extent.depth,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageUsage {
    TransferSrc,
    TransferDst,
    Sampled,
    Storage,
    ColorAttachment,
    DepthStencilAttachment,
    InputAttachment,
}

impl ImageUsage {
    pub fn to_vk_flags(self) -> ash::vk::ImageUsageFlags {
        match self {
            ImageUsage::TransferSrc => ash::vk::ImageUsageFlags::TRANSFER_SRC,
            ImageUsage::TransferDst => ash::vk::ImageUsageFlags::TRANSFER_DST,
            ImageUsage::Sampled => ash::vk::ImageUsageFlags::SAMPLED,
            ImageUsage::Storage => ash::vk::ImageUsageFlags::STORAGE,
            ImageUsage::ColorAttachment => ash::vk::ImageUsageFlags::COLOR_ATTACHMENT,
            ImageUsage::DepthStencilAttachment => {
                ash::vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT
            }
            ImageUsage::InputAttachment => ash::vk::ImageUsageFlags::INPUT_ATTACHMENT,
        }
    }

    pub fn all(usages: Vec<ImageUsage>) -> ash::vk::ImageUsageFlags {
        usages
            .iter()
            .fold(ash::vk::ImageUsageFlags::empty(), |acc, u| {
                acc | u.to_vk_flags()
            })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageTiling {
    Optimal,
    Linear,
}

impl From<ImageTiling> for ash::vk::ImageTiling {
    fn from(tiling: ImageTiling) -> Self {
        match tiling {
            ImageTiling::Optimal => ash::vk::ImageTiling::OPTIMAL,
            ImageTiling::Linear => ash::vk::ImageTiling::LINEAR,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleCount {
    Sample1,
    Sample2,
    Sample4,
    Sample8,
    Sample16,
    Sample32,
    Sample64,
}

impl From<SampleCount> for ash::vk::SampleCountFlags {
    fn from(count: SampleCount) -> Self {
        match count {
            SampleCount::Sample1 => ash::vk::SampleCountFlags::TYPE_1,
            SampleCount::Sample2 => ash::vk::SampleCountFlags::TYPE_2,
            SampleCount::Sample4 => ash::vk::SampleCountFlags::TYPE_4,
            SampleCount::Sample8 => ash::vk::SampleCountFlags::TYPE_8,
            SampleCount::Sample16 => ash::vk::SampleCountFlags::TYPE_16,
            SampleCount::Sample32 => ash::vk::SampleCountFlags::TYPE_32,
            SampleCount::Sample64 => ash::vk::SampleCountFlags::TYPE_64,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ClearColor {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl ClearColor {
    pub fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub fn solid(color: [f32; 3]) -> Self {
        Self {
            r: color[0],
            g: color[1],
            b: color[2],
            a: 1.0,
        }
    }
}

impl From<ClearColor> for ash::vk::ClearValue {
    fn from(color: ClearColor) -> Self {
        ash::vk::ClearValue {
            color: ash::vk::ClearColorValue {
                float32: [color.r, color.g, color.b, color.a],
            },
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ClearDepthStencil {
    pub depth: f32,
    pub stencil: u32,
}

impl ClearDepthStencil {
    pub fn new(depth: f32, stencil: u32) -> Self {
        Self { depth, stencil }
    }
}

impl From<ClearDepthStencil> for ash::vk::ClearValue {
    fn from(ds: ClearDepthStencil) -> Self {
        ash::vk::ClearValue {
            depth_stencil: ash::vk::ClearDepthStencilValue {
                depth: ds.depth,
                stencil: ds.stencil,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineBindPoint {
    Graphics,
    Compute,
}

impl From<PipelineBindPoint> for ash::vk::PipelineBindPoint {
    fn from(bind_point: PipelineBindPoint) -> Self {
        match bind_point {
            PipelineBindPoint::Graphics => ash::vk::PipelineBindPoint::GRAPHICS,
            PipelineBindPoint::Compute => ash::vk::PipelineBindPoint::COMPUTE,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferUsage {
    TransferSrc,
    TransferDst,
    UniformTexelBuffer,
    StorageTexelBuffer,
    UniformBuffer,
    StorageBuffer,
    IndexBuffer,
    VertexBuffer,
    IndirectBuffer,
}

impl BufferUsage {
    pub fn to_vk_flags(self) -> ash::vk::BufferUsageFlags {
        match self {
            BufferUsage::TransferSrc => ash::vk::BufferUsageFlags::TRANSFER_SRC,
            BufferUsage::TransferDst => ash::vk::BufferUsageFlags::TRANSFER_DST,
            BufferUsage::UniformTexelBuffer => ash::vk::BufferUsageFlags::UNIFORM_TEXEL_BUFFER,
            BufferUsage::StorageTexelBuffer => ash::vk::BufferUsageFlags::STORAGE_TEXEL_BUFFER,
            BufferUsage::UniformBuffer => ash::vk::BufferUsageFlags::UNIFORM_BUFFER,
            BufferUsage::StorageBuffer => ash::vk::BufferUsageFlags::STORAGE_BUFFER,
            BufferUsage::IndexBuffer => ash::vk::BufferUsageFlags::INDEX_BUFFER,
            BufferUsage::VertexBuffer => ash::vk::BufferUsageFlags::VERTEX_BUFFER,
            BufferUsage::IndirectBuffer => ash::vk::BufferUsageFlags::INDIRECT_BUFFER,
        }
    }

    pub fn all(usages: Vec<BufferUsage>) -> ash::vk::BufferUsageFlags {
        usages
            .iter()
            .fold(ash::vk::BufferUsageFlags::empty(), |acc, u| {
                acc | u.to_vk_flags()
            })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryProperty {
    DeviceLocal,
    HostVisible,
    HostCoherent,
    HostCached,
    LazilyAllocated,
}

impl MemoryProperty {
    pub fn to_vk_flags(self) -> ash::vk::MemoryPropertyFlags {
        match self {
            MemoryProperty::DeviceLocal => ash::vk::MemoryPropertyFlags::DEVICE_LOCAL,
            MemoryProperty::HostVisible => ash::vk::MemoryPropertyFlags::HOST_VISIBLE,
            MemoryProperty::HostCoherent => ash::vk::MemoryPropertyFlags::HOST_COHERENT,
            MemoryProperty::HostCached => ash::vk::MemoryPropertyFlags::HOST_CACHED,
            MemoryProperty::LazilyAllocated => ash::vk::MemoryPropertyFlags::LAZILY_ALLOCATED,
        }
    }

    pub fn all(properties: Vec<MemoryProperty>) -> ash::vk::MemoryPropertyFlags {
        properties
            .iter()
            .fold(ash::vk::MemoryPropertyFlags::empty(), |acc, p| {
                acc | p.to_vk_flags()
            })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    IndirectCommandRead,
    IndexRead,
    VertexAttributeRead,
    UniformRead,
    InputAttachmentRead,
    ShaderRead,
    ShaderWrite,
    ColorAttachmentRead,
    ColorAttachmentWrite,
    DepthStencilAttachmentRead,
    DepthStencilAttachmentWrite,
    TransferRead,
    TransferWrite,
    HostRead,
    HostWrite,
    MemoryRead,
    MemoryWrite,
}

impl Access {
    pub fn to_vk_flags(self) -> ash::vk::AccessFlags {
        match self {
            Access::IndirectCommandRead => ash::vk::AccessFlags::INDIRECT_COMMAND_READ,
            Access::IndexRead => ash::vk::AccessFlags::INDEX_READ,
            Access::VertexAttributeRead => ash::vk::AccessFlags::VERTEX_ATTRIBUTE_READ,
            Access::UniformRead => ash::vk::AccessFlags::UNIFORM_READ,
            Access::InputAttachmentRead => ash::vk::AccessFlags::INPUT_ATTACHMENT_READ,
            Access::ShaderRead => ash::vk::AccessFlags::SHADER_READ,
            Access::ShaderWrite => ash::vk::AccessFlags::SHADER_WRITE,
            Access::ColorAttachmentRead => ash::vk::AccessFlags::COLOR_ATTACHMENT_READ,
            Access::ColorAttachmentWrite => ash::vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
            Access::DepthStencilAttachmentRead => {
                ash::vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ
            }
            Access::DepthStencilAttachmentWrite => {
                ash::vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE
            }
            Access::TransferRead => ash::vk::AccessFlags::TRANSFER_READ,
            Access::TransferWrite => ash::vk::AccessFlags::TRANSFER_WRITE,
            Access::HostRead => ash::vk::AccessFlags::HOST_READ,
            Access::HostWrite => ash::vk::AccessFlags::HOST_WRITE,
            Access::MemoryRead => ash::vk::AccessFlags::MEMORY_READ,
            Access::MemoryWrite => ash::vk::AccessFlags::MEMORY_WRITE,
        }
    }

    pub fn all(accesses: &[Access]) -> ash::vk::AccessFlags {
        accesses
            .iter()
            .fold(ash::vk::AccessFlags::empty(), |acc, a| {
                acc | a.to_vk_flags()
            })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineStage {
    TopOfPipe,
    DrawIndirect,
    VertexInput,
    VertexShader,
    FragmentShader,
    EarlyFragmentTests,
    LateFragmentTests,
    ColorAttachmentOutput,
    ComputeShader,
    Transfer,
    BottomOfPipe,
    Host,
}

impl PipelineStage {
    pub fn to_vk_flags(self) -> ash::vk::PipelineStageFlags {
        match self {
            PipelineStage::TopOfPipe => ash::vk::PipelineStageFlags::TOP_OF_PIPE,
            PipelineStage::DrawIndirect => ash::vk::PipelineStageFlags::DRAW_INDIRECT,
            PipelineStage::VertexInput => ash::vk::PipelineStageFlags::VERTEX_INPUT,
            PipelineStage::VertexShader => ash::vk::PipelineStageFlags::VERTEX_SHADER,
            PipelineStage::FragmentShader => ash::vk::PipelineStageFlags::FRAGMENT_SHADER,
            PipelineStage::EarlyFragmentTests => ash::vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS,
            PipelineStage::LateFragmentTests => ash::vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
            PipelineStage::ColorAttachmentOutput => {
                ash::vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
            }
            PipelineStage::ComputeShader => ash::vk::PipelineStageFlags::COMPUTE_SHADER,
            PipelineStage::Transfer => ash::vk::PipelineStageFlags::TRANSFER,
            PipelineStage::BottomOfPipe => ash::vk::PipelineStageFlags::BOTTOM_OF_PIPE,
            PipelineStage::Host => ash::vk::PipelineStageFlags::HOST,
        }
    }

    pub fn all(stages: &[PipelineStage]) -> ash::vk::PipelineStageFlags {
        stages
            .iter()
            .fold(ash::vk::PipelineStageFlags::empty(), |acc, s| {
                acc | s.to_vk_flags()
            })
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ClearValue {
    Color(ClearColor),
    DepthStencil(ClearDepthStencil),
}

impl ClearValue {
    pub fn color(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self::Color(ClearColor::new(r, g, b, a))
    }

    pub fn solid(color: [f32; 3]) -> Self {
        Self::Color(ClearColor::solid(color))
    }

    pub fn depth(depth: f32, stencil: u32) -> Self {
        Self::DepthStencil(ClearDepthStencil::new(depth, stencil))
    }
}

impl From<ClearValue> for ash::vk::ClearValue {
    fn from(val: ClearValue) -> Self {
        match val {
            ClearValue::Color(c) => c.into(),
            ClearValue::DepthStencil(ds) => ds.into(),
        }
    }
}

//=============================================================================
// Dynamic Rendering Types (Vulkan 1.3)
//=============================================================================

/// Rendering attachment info for Dynamic Rendering.
/// Describes a color or depth-stencil attachment for dynamic rendering.
#[derive(Debug, Clone, Copy)]
pub struct RenderingAttachmentInfo {
    /// Image view to use as attachment.
    pub image_view: VkImageView,
    /// Image layout expected during rendering.
    pub image_layout: ImageLayout,
    /// Load operation for this attachment.
    pub load_op: AttachmentLoadOp,
    /// Store operation for this attachment.
    pub store_op: AttachmentStoreOp,
    /// Clear value (only used if load_op is Clear).
    pub clear_value: Option<ClearValue>,
}

impl RenderingAttachmentInfo {
    /// Create a new rendering attachment info from a VkImageView wrapper.
    pub fn new(image_view: VkImageView) -> Self {
        Self {
            image_view,
            image_layout: ImageLayout::ColorAttachmentOptimal,
            load_op: AttachmentLoadOp::Load,
            store_op: AttachmentStoreOp::Store,
            clear_value: None,
        }
    }

    /// Create from a raw vk::ImageView (convenience for internal use).
    pub fn from_vk(image_view: ash::vk::ImageView) -> Self {
        Self::new(VkImageView::new(image_view))
    }

    /// Set the image layout.
    pub fn layout(mut self, layout: ImageLayout) -> Self {
        self.image_layout = layout;
        self
    }

    /// Set the image layout from a raw vk::ImageLayout (convenience).
    pub fn layout_vk(mut self, layout: ash::vk::ImageLayout) -> Self {
        self.image_layout = ImageLayout::from_vk(layout);
        self
    }

    /// Set the load operation.
    pub fn load_op(mut self, op: AttachmentLoadOp) -> Self {
        self.load_op = op;
        self
    }

    /// Set the store operation.
    pub fn store_op(mut self, op: AttachmentStoreOp) -> Self {
        self.store_op = op;
        self
    }

    /// Set the clear value and set load_op to Clear.
    pub fn clear(mut self, value: ClearValue) -> Self {
        self.clear_value = Some(value);
        self.load_op = AttachmentLoadOp::Clear;
        self
    }

    /// Convert to Vulkan vk::RenderingAttachmentInfoKHR.
    pub fn into_vk(self) -> ash::vk::RenderingAttachmentInfoKHR<'static> {
        let clear_value = self
            .clear_value
            .map_or_else(ash::vk::ClearValue::default, |cv| cv.into());

        ash::vk::RenderingAttachmentInfoKHR::default()
            .image_view(self.image_view.vk())
            .image_layout(self.image_layout.into())
            .load_op(self.load_op.into())
            .store_op(self.store_op.into())
            .clear_value(clear_value)
    }
}

/// Rendering info for Dynamic Rendering.
/// Describes a rendering pass using VK_KHR_dynamic_rendering.
#[derive(Debug, Clone)]
pub struct RenderingInfo {
    /// Color attachments.
    pub color_attachments: Vec<RenderingAttachmentInfo>,
    /// Depth-stencil attachment (optional).
    pub depth_attachment: Option<RenderingAttachmentInfo>,
    /// Stencil attachment (optional, for separate stencil).
    pub stencil_attachment: Option<RenderingAttachmentInfo>,
    /// Render area (offset and extent).
    pub render_area: ash::vk::Rect2D,
    /// Layer count.
    pub layer_count: u32,
    /// View mask for multiview (0 for non-multiview).
    pub view_mask: u32,
}

impl RenderingInfo {
    /// Create a new rendering info.
    pub fn new() -> Self {
        Self {
            color_attachments: Vec::new(),
            depth_attachment: None,
            stencil_attachment: None,
            render_area: ash::vk::Rect2D::default(),
            layer_count: 1,
            view_mask: 0,
        }
    }

    /// Add a color attachment.
    pub fn add_color_attachment(mut self, attachment: RenderingAttachmentInfo) -> Self {
        self.color_attachments.push(attachment);
        self
    }

    /// Set the depth attachment.
    pub fn depth_attachment(mut self, attachment: RenderingAttachmentInfo) -> Self {
        self.depth_attachment = Some(attachment);
        self
    }

    /// Set the stencil attachment (separate from depth).
    pub fn stencil_attachment(mut self, attachment: RenderingAttachmentInfo) -> Self {
        self.stencil_attachment = Some(attachment);
        self
    }

    /// Set the render area.
    pub fn render_area(mut self, area: ash::vk::Rect2D) -> Self {
        self.render_area = area;
        self
    }

    /// Set the layer count.
    pub fn layer_count(mut self, count: u32) -> Self {
        self.layer_count = count;
        self
    }

    /// Set the view mask for multiview.
    pub fn view_mask(mut self, mask: u32) -> Self {
        self.view_mask = mask;
        self
    }

    /// Build and execute with the given callback.
    /// This handles the lifetime issues by keeping the Vulkan structs alive during the callback.
    pub fn build<F, R>(self, f: F) -> R
    where
        F: FnOnce(&ash::vk::RenderingInfoKHR) -> R,
    {
        let color_attachments_vk: Vec<ash::vk::RenderingAttachmentInfoKHR> = self
            .color_attachments
            .iter()
            .map(|a| (*a).into_vk())
            .collect();

        let depth_vk = self.depth_attachment.as_ref().map(|d| (*d).into_vk());
        let stencil_vk = self
            .stencil_attachment
            .as_ref()
            .map(|s| (*s).into_vk());

        let mut builder = ash::vk::RenderingInfoKHR::default()
            .render_area(self.render_area)
            .layer_count(self.layer_count)
            .color_attachments(&color_attachments_vk);

        if self.view_mask != 0 {
            builder = builder.view_mask(self.view_mask);
        }

        if let Some(depth) = &depth_vk {
            builder = builder.depth_attachment(depth);
        }

        if let Some(stencil) = &stencil_vk {
            builder = builder.stencil_attachment(stencil);
        }

        f(&builder)
    }
}

impl Default for RenderingInfo {
    fn default() -> Self {
        Self::new()
    }
}

/// Rect2D wrapper for render area specification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect2D {
    pub offset: Offset2D,
    pub extent: Extent2D,
}

/// Offset2D wrapper for 2D offset specification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Offset2D {
    pub x: i32,
    pub y: i32,
}

impl Offset2D {
    pub fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

impl Rect2D {
    pub fn new(offset: Offset2D, extent: Extent2D) -> Self {
        Self { offset, extent }
    }

    pub fn from_extents(width: u32, height: u32) -> Self {
        Self {
            offset: Offset2D::new(0, 0),
            extent: Extent2D::new(width, height),
        }
    }
}

impl From<Rect2D> for ash::vk::Rect2D {
    fn from(rect: Rect2D) -> Self {
        ash::vk::Rect2D {
            offset: ash::vk::Offset2D {
                x: rect.offset.x,
                y: rect.offset.y,
            },
            extent: rect.extent.into(),
        }
    }
}

/// Shader stage flags for pipeline creation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ShaderStages {
    pub vertex: bool,
    pub fragment: bool,
    pub compute: bool,
    pub geometry: bool,
    pub tessellation_control: bool,
    pub tessellation_evaluation: bool,
}

impl ShaderStages {
    pub const VERTEX: Self = Self { vertex: true, fragment: false, compute: false, geometry: false, tessellation_control: false, tessellation_evaluation: false };
    pub const FRAGMENT: Self = Self { vertex: false, fragment: true, compute: false, geometry: false, tessellation_control: false, tessellation_evaluation: false };
    pub const VERTEX_FRAGMENT: Self = Self { vertex: true, fragment: true, compute: false, geometry: false, tessellation_control: false, tessellation_evaluation: false };
    pub const ALL_GRAPHICS: Self = Self { vertex: true, fragment: true, compute: false, geometry: true, tessellation_control: true, tessellation_evaluation: true };
}

impl From<ShaderStages> for ash::vk::ShaderStageFlags {
    fn from(stages: ShaderStages) -> Self {
        let mut flags = ash::vk::ShaderStageFlags::empty();
        if stages.vertex { flags |= ash::vk::ShaderStageFlags::VERTEX; }
        if stages.fragment { flags |= ash::vk::ShaderStageFlags::FRAGMENT; }
        if stages.compute { flags |= ash::vk::ShaderStageFlags::COMPUTE; }
        if stages.geometry { flags |= ash::vk::ShaderStageFlags::GEOMETRY; }
        if stages.tessellation_control { flags |= ash::vk::ShaderStageFlags::TESSELLATION_CONTROL; }
        if stages.tessellation_evaluation { flags |= ash::vk::ShaderStageFlags::TESSELLATION_EVALUATION; }
        flags
    }
}
