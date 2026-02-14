pub mod builder;
pub mod cube;
pub mod cylinder;
pub mod plane;
pub mod sphere;
pub mod torus;

use crate::rendering::{VertexPBR, VertexSkinned};
use crate::util::GLTFModel;
pub use builder::*;
pub use cube::*;
pub use cylinder::*;
pub use plane::*;
pub use sphere::*;
pub use torus::*;

use katla_vulkan::context::VulkanContext;
use katla_vulkan::vulkan::{
    vertex_attr_set::VertexAttributeSet,
    vertex_attribute::{AttributeBinding, AttributeType},
    VertexFormat,
};
use katla_vulkan::{self, IndexBuffer, IndexType, MeshHandle, VertexBuffer};

use std::rc::Rc;

/// Mesh represents geometry with GPU-side vertex and index buffers.
///
/// DESIGN NOTE: Currently `Mesh` owns GPU buffer handles directly. A future
/// improvement could split this into separate CPU/GPU representations, where
/// the CPU side holds the raw data and the GPU side manages Vulkan resources.
/// This would enable features like CPU-side mesh processing and dynamic updates.
///
/// SOA Support: The mesh can use SoA (Structure of Arrays) layout via `attributes`
/// for flexible rendering pipelines (depth-only, shadow mapping, etc.).
pub struct Mesh {
    /// Legacy AoS (Array of Structures) vertex buffer
    pub vertex_buffer: Option<VertexBuffer>,
    /// Index buffer (shared by both AoS and SoA layouts)
    pub index_buffer: Option<IndexBuffer>,
    /// SoA (Structure of Arrays) vertex attribute set for flexible rendering
    pub attributes: Option<VertexAttributeSet>,
    /// Handle after registration with renderer (None until registered)
    pub handle: Option<MeshHandle>,
}

impl Mesh {
    pub fn new(context: Rc<VulkanContext>, vertices: Vec<VertexPBR>, indices: Vec<u32>) -> Self {
        let index_buffer = Self::create_index_buffer(&context, indices, IndexType::Uint32);
        let vertex_buffer = Self::create_vertex_buffer(&context, vertices);

        Self {
            vertex_buffer,
            index_buffer,
            attributes: None,
            handle: None,
        }
    }

    pub fn new_from_model(model: Rc<GLTFModel>, context: Rc<VulkanContext>) -> Self {
        let index_type = match model.index_stride {
            1 => IndexType::Uint8,
            2 => IndexType::Uint16,
            4 => IndexType::Uint32,
            _ => IndexType::None,
        };
        let index_buffer = Self::create_index_buffer(&context, model.index_data(), index_type);
        let vertex_buffer = Self::create_vertex_buffer(&context, model.vertpbr());

        Self {
            vertex_buffer,
            index_buffer,
            attributes: None,
            handle: None,
        }
    }

    /// Create a skinned mesh from a GLTF model with skeletal animation data.
    pub fn new_skinned_from_model(model: Rc<GLTFModel>, context: Rc<VulkanContext>) -> Self {
        let index_type = match model.index_stride {
            1 => IndexType::Uint8,
            2 => IndexType::Uint16,
            4 => IndexType::Uint32,
            _ => IndexType::None,
        };
        let index_buffer = Self::create_index_buffer(&context, model.index_data(), index_type);
        let vertex_buffer = Self::create_vertex_buffer(&context, model.vertskinned());

        Self {
            vertex_buffer,
            index_buffer,
            attributes: None,
            handle: None,
        }
    }

    /// Create a mesh from a GLTF model using SoA (Structure of Arrays) vertex layout.
    ///
    /// This constructor uses ParsedAttributes from the model to create
    /// separate buffers for each attribute type.
    ///
    /// # Arguments
    /// * `model` - GLTF model with parsed attributes
    /// * `context` - Vulkan context
    pub fn new_from_model_soa(model: Rc<GLTFModel>, context: Rc<VulkanContext>) -> Self {
        use crate::util::gltf_parser::ParsedAttributes;

        let index_type = match model.index_stride {
            1 => IndexType::Uint8,
            2 => IndexType::Uint16,
            4 => IndexType::Uint32,
            _ => IndexType::None,
        };

        let index_buffer = Self::create_index_buffer(&context, model.index_data(), index_type);

        // Try to get parsed SoA attributes
        let mut attributes = None;
        if let Some(parsed) = model.parsed_attributes() {
            let vertex_count = parsed.positions.len() as u32;
            let mut attr_set = VertexAttributeSet::new(vertex_count);

            // Create separate buffers for each attribute
            if !parsed.positions.is_empty() {
                if let Some(buf) = Self::create_attribute_buffer(
                    context.clone(),
                    &parsed.positions,
                    AttributeType::Position,
                    VertexFormat::RGB32f,
                ) {
                    attr_set.add_attribute(buf);
                }
            }

            if !parsed.normals.is_empty() {
                if let Some(buf) = Self::create_attribute_buffer(
                    context.clone(),
                    &parsed.normals,
                    AttributeType::Normal,
                    VertexFormat::RGB32f,
                ) {
                    attr_set.add_attribute(buf);
                }
            }

            if !parsed.tangents.is_empty() {
                if let Some(buf) = Self::create_attribute_buffer(
                    context.clone(),
                    &parsed.tangents,
                    AttributeType::Tangent,
                    VertexFormat::RGBA32f,
                ) {
                    attr_set.add_attribute(buf);
                }
            }

            if !parsed.tex_coords0.is_empty() {
                if let Some(buf) = Self::create_attribute_buffer(
                    context.clone(),
                    &parsed.tex_coords0,
                    AttributeType::TexCoord0,
                    VertexFormat::RG32f,
                ) {
                    attr_set.add_attribute(buf);
                }
            }

            if !parsed.joint_indices.is_empty() {
                if let Some(buf) = Self::create_attribute_buffer(
                    context.clone(),
                    &parsed.joint_indices,
                    AttributeType::JointIndices,
                    VertexFormat::RGBA16u,
                ) {
                    attr_set.add_attribute(buf);
                }
            }

            if !parsed.joint_weights.is_empty() {
                if let Some(buf) = Self::create_attribute_buffer(
                    context.clone(),
                    &parsed.joint_weights,
                    AttributeType::JointWeights,
                    VertexFormat::RGBA32f,
                ) {
                    attr_set.add_attribute(buf);
                }
            }

            attributes = Some(attr_set);
        }

        Self {
            vertex_buffer: None,
            index_buffer,
            attributes,
            handle: None,
        }
    }

    /// Create a mesh using SoA (Structure of Arrays) vertex layout.
    ///
    /// This constructor creates separate buffers for each attribute type,
    /// enabling efficient rendering passes that only need a subset of attributes
    /// (e.g., depth-only, shadow mapping, deferred G-buffer fills).
    ///
    /// # Arguments
    /// * `positions` - Vertex positions (vec3<f32>)
    /// * `normals` - Vertex normals (vec3<f32>)
    /// * `tangents` - Vertex tangents (vec4<f32>)
    /// * `tex_coords0` - Primary texture coordinates (vec2<f32>)
    /// * `indices` - Index data
    ///
    /// # Example
    /// ```no_run
    /// # use katla_app::rendering::Mesh;
    /// # use std::rc::Rc;
    /// # let context: Rc<katla_vulkan::VulkanContext> = unsafe { std::mem::zeroed() };
    /// let positions = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]];
    /// let normals = vec![[0.0, 1.0, 0.0], [0.0, 1.0, 0.0]];
    /// let tangents = vec![[1.0, 0.0, 0.0, 1.0], [1.0, 0.0, 0.0, 1.0]];
    /// let tex_coords0 = vec![[0.0, 0.0], [1.0, 0.0]];
    /// let indices = vec![0u32, 1u32];
    ///
    /// let mesh = Mesh::new_soa(
    ///     context,
    ///     positions,
    ///     normals,
    ///     tangents,
    ///     tex_coords0,
    ///     indices,
    /// );
    /// ```
    pub fn new_soa(
        context: Rc<VulkanContext>,
        positions: Vec<[f32; 3]>,
        normals: Vec<[f32; 3]>,
        tangents: Vec<[f32; 4]>,
        tex_coords0: Vec<[f32; 2]>,
        indices: Vec<u32>,
    ) -> Self {
        let vertex_count = positions.len() as u32;
        let mut attributes = VertexAttributeSet::new(vertex_count);

        // Create separate buffers for each attribute
        if !positions.is_empty() {
            let buffer = Self::create_attribute_buffer(
                context.clone(),
                &positions,
                AttributeType::Position,
                VertexFormat::RGB32f,
            );
            if let Some(buf) = buffer {
                attributes.add_attribute(buf);
            }
        }

        if !normals.is_empty() {
            let buffer = Self::create_attribute_buffer(
                context.clone(),
                &normals,
                AttributeType::Normal,
                VertexFormat::RGB32f,
            );
            if let Some(buf) = buffer {
                attributes.add_attribute(buf);
            }
        }

        if !tangents.is_empty() {
            let buffer = Self::create_attribute_buffer(
                context.clone(),
                &tangents,
                AttributeType::Tangent,
                VertexFormat::RGBA32f,
            );
            if let Some(buf) = buffer {
                attributes.add_attribute(buf);
            }
        }

        if !tex_coords0.is_empty() {
            let buffer = Self::create_attribute_buffer(
                context.clone(),
                &tex_coords0,
                AttributeType::TexCoord0,
                VertexFormat::RG32f,
            );
            if let Some(buf) = buffer {
                attributes.add_attribute(buf);
            }
        }

        let index_buffer = Self::create_index_buffer(&context, indices, IndexType::Uint32);

        Self {
            vertex_buffer: None,
            index_buffer,
            attributes: Some(attributes),
            handle: None,
        }
    }

    /// Create a skinned mesh using SoA (Structure of Arrays) vertex layout.
    ///
    /// Includes skeletal animation attributes (joint indices and weights).
    ///
    /// # Arguments
    /// * `positions` - Vertex positions (vec3<f32>)
    /// * `normals` - Vertex normals (vec3<f32>)
    /// * `tangents` - Vertex tangents (vec4<f32>)
    /// * `tex_coords0` - Primary texture coordinates (vec2<f32>)
    /// * `joint_indices` - Joint indices for skinning (uvec4, u16x4)
    /// * `joint_weights` - Joint weights for skinning (vec4<f32>)
    /// * `indices` - Index data
    pub fn new_skinned_soa(
        context: Rc<VulkanContext>,
        positions: Vec<[f32; 3]>,
        normals: Vec<[f32; 3]>,
        tangents: Vec<[f32; 4]>,
        tex_coords0: Vec<[f32; 2]>,
        joint_indices: Vec<[u16; 4]>,
        joint_weights: Vec<[f32; 4]>,
        indices: Vec<u32>,
    ) -> Self {
        let vertex_count = positions.len() as u32;
        let mut attributes = VertexAttributeSet::new(vertex_count);

        // Create separate buffers for each attribute
        if !positions.is_empty() {
            let buffer = Self::create_attribute_buffer(
                context.clone(),
                &positions,
                AttributeType::Position,
                VertexFormat::RGB32f,
            );
            if let Some(buf) = buffer {
                attributes.add_attribute(buf);
            }
        }

        if !normals.is_empty() {
            let buffer = Self::create_attribute_buffer(
                context.clone(),
                &normals,
                AttributeType::Normal,
                VertexFormat::RGB32f,
            );
            if let Some(buf) = buffer {
                attributes.add_attribute(buf);
            }
        }

        if !tangents.is_empty() {
            let buffer = Self::create_attribute_buffer(
                context.clone(),
                &tangents,
                AttributeType::Tangent,
                VertexFormat::RGBA32f,
            );
            if let Some(buf) = buffer {
                attributes.add_attribute(buf);
            }
        }

        if !tex_coords0.is_empty() {
            let buffer = Self::create_attribute_buffer(
                context.clone(),
                &tex_coords0,
                AttributeType::TexCoord0,
                VertexFormat::RG32f,
            );
            if let Some(buf) = buffer {
                attributes.add_attribute(buf);
            }
        }

        if !joint_indices.is_empty() {
            let buffer = Self::create_attribute_buffer(
                context.clone(),
                &joint_indices,
                AttributeType::JointIndices,
                VertexFormat::RGBA16u,
            );
            if let Some(buf) = buffer {
                attributes.add_attribute(buf);
            }
        }

        if !joint_weights.is_empty() {
            let buffer = Self::create_attribute_buffer(
                context.clone(),
                &joint_weights,
                AttributeType::JointWeights,
                VertexFormat::RGBA32f,
            );
            if let Some(buf) = buffer {
                attributes.add_attribute(buf);
            }
        }

        let index_buffer = Self::create_index_buffer(&context, indices, IndexType::Uint32);

        Self {
            vertex_buffer: None,
            index_buffer,
            attributes: Some(attributes),
            handle: None,
        }
    }

    /// Create an attribute buffer from raw data.
    ///
    /// # Type Parameters
    /// * `T` - The data type (e.g., `[f32; 3]`, `[u16; 4]`)
    ///
    /// # Arguments
    /// * `context` - Vulkan context
    /// * `data` - Attribute data
    /// * `attr_type` - Semantic attribute type
    /// * `format` - Vertex format
    fn create_attribute_buffer<T>(
        context: Rc<VulkanContext>,
        data: &[T],
        attr_type: AttributeType,
        format: VertexFormat,
    ) -> Option<AttributeBinding> {
        if data.is_empty() {
            return None;
        }

        let data_slice = unsafe {
            std::slice::from_raw_parts(
                data.as_ptr() as *const u8,
                data.len() * std::mem::size_of::<T>(),
            )
        };

        let mut vertex_buffer = VertexBuffer::new(
            context,
            data_slice.len() as u64,
            data.len() as u32,
        );
        vertex_buffer.upload_data(data_slice);

        Some(AttributeBinding::new(attr_type, format, vertex_buffer.object()))
    }

    fn create_index_buffer<DataType>(
        context: &Rc<VulkanContext>,
        data: Vec<DataType>,
        index_type: IndexType,
    ) -> Option<IndexBuffer> {
        if data.is_empty() {
            None
        } else {
            let data_slice = unsafe {
                std::slice::from_raw_parts(
                    data.as_ptr() as *const u8,
                    data.len() * std::mem::size_of::<DataType>(),
                )
            };
            let count = match index_type {
                IndexType::Uint8 => data_slice.len() as u32,
                IndexType::Uint16 => (data_slice.len() as u32) / 2,
                IndexType::Uint32 => (data_slice.len() as u32) / 4,
                IndexType::None => 0_u32,
            };
            let mut index_buffer =
                IndexBuffer::new(context.clone(), data_slice.len() as u64, index_type, count);
            index_buffer.upload_data(data_slice);
            Some(index_buffer)
        }
    }

    fn create_vertex_buffer<DataType>(
        context: &Rc<VulkanContext>,
        data: Vec<DataType>,
    ) -> Option<VertexBuffer> {
        if data.is_empty() {
            None
        } else {
            let data_slice = unsafe {
                std::slice::from_raw_parts(
                    data.as_ptr() as *const u8,
                    data.len() * std::mem::size_of::<DataType>(),
                )
            };
            let mut vertex_buffer =
                VertexBuffer::new(context.clone(), data_slice.len() as u64, data.len() as u32);
            vertex_buffer.upload_data(data_slice);
            Some(vertex_buffer)
        }
    }

    pub fn draw(&self, command_buffer: &katla_vulkan::CommandBuffer) {
        if let Some(index_buffer) = &self.index_buffer {
            command_buffer.bind_index_buffer(index_buffer.object(), 0, index_buffer.index_type);

            if let Some(vertex_buffer) = &self.vertex_buffer {
                command_buffer.bind_vertex_buffers(0, &[vertex_buffer.object()], &[0]);
                command_buffer.draw_indexed(index_buffer.count(), 1, 0, 0, 0);
            }
        } else if let Some(vertex_buffer) = &self.vertex_buffer {
            command_buffer.bind_vertex_buffers(0, &[vertex_buffer.object()], &[0]);
            command_buffer.draw_array(vertex_buffer.count(), 1, 0, 0);
        }
    }

    /// Draw mesh using SoA (Structure of Arrays) vertex attributes.
    ///
    /// This method binds all vertex attributes from the SoA attribute set,
    /// enabling flexible rendering with full attribute set.
    ///
    /// # Arguments
    /// * `command_buffer` - Command buffer to record draw commands into
    ///
    /// # Example
    /// ```no_run
    /// # use katla_app::rendering::Mesh;
    /// # let mesh: Mesh = unsafe { std::mem::zeroed() };
    /// # let command_buffer: katla_vulkan::CommandBuffer = unsafe { std::mem::zeroed() };
    /// mesh.draw_soa(&command_buffer);
    /// ```
    pub fn draw_soa(&self, command_buffer: &katla_vulkan::CommandBuffer) {
        if let Some(attributes) = &self.attributes {
            if let Some(index_buffer) = &self.index_buffer {
                command_buffer.bind_index_buffer(index_buffer.object(), 0, index_buffer.index_type);
                command_buffer.bind_vertex_attributes(attributes);
                command_buffer.draw_indexed(index_buffer.count(), 1, 0, 0, 0);
            }
        }
    }

    /// Draw mesh using a subset of SoA attributes.
    ///
    /// This method binds only the specified attributes, enabling efficient
    /// depth-only prepasses, shadow mapping, or deferred G-buffer fills.
    ///
    /// # Arguments
    /// * `command_buffer` - Command buffer to record draw commands into
    /// * `attr_types` - Slice of attribute types to bind (order determines binding locations)
    ///
    /// # Example
    /// ```no_run
    /// # use katla_app::rendering::Mesh;
    /// # use katla_vulkan::vulkan::vertex_attribute::AttributeType;
    /// # let mesh: Mesh = unsafe { std::mem::zeroed() };
    /// # let command_buffer: katla_vulkan::CommandBuffer = unsafe { std::mem::zeroed() };
    /// // Depth-only pass: only position needed
    /// mesh.draw_soa_subset(&command_buffer, &[AttributeType::Position]);
    ///
    /// // Shadow mapping: position only
    /// mesh.draw_soa_subset(&command_buffer, &[AttributeType::Position]);
    ///
    /// // Deferred G-buffer fill: position + normal
    /// mesh.draw_soa_subset(
    ///     &command_buffer,
    ///     &[AttributeType::Position, AttributeType::Normal],
    /// );
    /// ```
    pub fn draw_soa_subset(
        &self,
        command_buffer: &katla_vulkan::CommandBuffer,
        attr_types: &[AttributeType],
    ) {
        if let Some(attributes) = &self.attributes {
            if let Some(index_buffer) = &self.index_buffer {
                command_buffer.bind_index_buffer(index_buffer.object(), 0, index_buffer.index_type);
                command_buffer.bind_attributes_subset(attributes, attr_types);
                command_buffer.draw_indexed(index_buffer.count(), 1, 0, 0, 0);
            }
        }
    }

    /// Get the handle (returns None if not yet registered with renderer)
    pub fn handle(&self) -> Option<MeshHandle> {
        self.handle
    }
}
