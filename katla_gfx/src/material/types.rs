//! Core material types - minimal, handle-based.
//!
//! These types provide the foundation for the materials API with:
//! - Handle-based references (no Rc/RefCell)
//! - Bindless-first design
//! - Simple builder pattern

use crate::handle::{MaterialHandle, PipelineHandle, TextureHandle};

/// Maximum textures per material instance.
pub const MAX_MATERIAL_TEXTURES: usize = 8;

/// Minimal material instance with handle-based references.
///
/// Textures are stored as a fixed array for bindless access.
/// Use the builder methods to configure.
#[derive(Clone, Debug)]
pub struct Material {
    /// Reference to the material template (pipeline + descriptor layouts)
    template: MaterialHandle,
    /// Texture handles for bindless access
    textures: [TextureHandle; MAX_MATERIAL_TEXTURES],
    /// Number of active textures in the array
    texture_count: u32,
    /// Optional push constant data
    push_constants: Vec<u8>,
    /// Optional pipeline override (for materials without templates)
    pipeline: PipelineHandle,
    /// Vertex binding for pipeline override case
    vertex_binding: Option<crate::vulkan::vertexbinding::VertexBinding>,
    /// Whether this material uses bindless textures
    is_bindless: bool,
}

impl Material {
    /// Create a new material from a template handle.
    pub fn new(template: MaterialHandle) -> Self {
        Self {
            template,
            textures: [TextureHandle::NONE; MAX_MATERIAL_TEXTURES],
            texture_count: 0,
            push_constants: Vec::new(),
            pipeline: PipelineHandle::NONE,
            vertex_binding: None,
            is_bindless: true, // Default to bindless
        }
    }

    /// Create a material from a pipeline handle (no template).
    pub fn from_pipeline(pipeline: PipelineHandle, is_bindless: bool) -> Self {
        Self {
            template: MaterialHandle::NONE,
            textures: [TextureHandle::NONE; MAX_MATERIAL_TEXTURES],
            texture_count: 0,
            push_constants: Vec::new(),
            pipeline,
            vertex_binding: None,
            is_bindless,
        }
    }

    /// Set a texture at the given slot.
    pub fn with_texture(mut self, slot: u32, handle: TextureHandle) -> Self {
        if (slot as usize) < MAX_MATERIAL_TEXTURES {
            self.textures[slot as usize] = handle;
            self.texture_count = self.texture_count.max(slot + 1);
        }
        self
    }

    /// Set push constant data.
    pub fn with_push_constants(mut self, data: Vec<u8>) -> Self {
        self.push_constants = data;
        self
    }

    /// Set the vertex binding (for pipeline override case).
    pub fn with_vertex_binding(
        mut self,
        binding: crate::vulkan::vertexbinding::VertexBinding,
    ) -> Self {
        self.vertex_binding = Some(binding);
        self
    }

    /// Get the template handle.
    pub fn template(&self) -> MaterialHandle {
        self.template
    }

    /// Get the pipeline handle (from template or override).
    pub fn pipeline(&self) -> PipelineHandle {
        self.pipeline
    }

    /// Get texture handles.
    pub fn textures(&self) -> &[TextureHandle] {
        &self.textures[..self.texture_count as usize]
    }

    /// Get all texture slots (including empty ones).
    pub fn texture_slots(&self) -> &[TextureHandle; MAX_MATERIAL_TEXTURES] {
        &self.textures
    }

    /// Get push constant data.
    pub fn push_constants(&self) -> &[u8] {
        &self.push_constants
    }

    /// Check if this material uses bindless textures.
    pub fn is_bindless(&self) -> bool {
        self.is_bindless
    }

    /// Get the vertex binding (if set).
    pub fn vertex_binding(&self) -> Option<&crate::vulkan::vertexbinding::VertexBinding> {
        self.vertex_binding.as_ref()
    }

    /// Set a texture at the given slot (mutable).
    pub fn set_texture(&mut self, slot: u32, handle: TextureHandle) {
        if (slot as usize) < MAX_MATERIAL_TEXTURES {
            self.textures[slot as usize] = handle;
            self.texture_count = self.texture_count.max(slot + 1);
        }
    }

    /// Set push constant data (mutable).
    pub fn set_push_constants(&mut self, data: Vec<u8>) {
        self.push_constants = data;
    }
}

impl Default for Material {
    fn default() -> Self {
        Self::new(MaterialHandle::NONE)
    }
}
