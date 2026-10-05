//! Forward+ resources and compute work owned by the scene composition.

use bytemuck::{Pod, Zeroable};
use katla_gfx::ShaderStages;
use katla_gfx::render_graph::{
    BufferAccess, BufferByteRange, BufferDesc, BufferMemoryPolicy, BufferUsages, ComputeBinding,
    ComputeCommand, ComputeDispatch, ComputeDispatchSize, ComputePipelineDesc, PassDesc, PassType,
    ResourceId,
};
use katla_gfx::renderer::frame_bindings::BufferBinding;
use katla_gfx::renderer::frame_scope::FrameToken;
use katla_gfx::{BufferHandle, GpuRenderer, PointLightGPU, Size2D};

use crate::rendering::FrameUniforms;
use crate::resources::ResourceManager;
use crate::{AppError, AppResult, FrameGraph, Renderer};

const FRAME_SLOTS: usize = 3;
const MAX_LIGHTS: usize = 256;
const TILE_SIZE: u32 = 16;
const LIGHTS_PER_TILE: u64 = 128;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct LightFrame {
    view: [f32; 16],
    projection: [f32; 16],
    light_count: u32,
    tiles_x: u32,
    tiles_y: u32,
    width: u32,
    height: u32,
    padding: [u32; 3],
}

#[derive(Clone, Copy)]
struct LightSlot {
    data: BufferHandle,
    indices: BufferHandle,
    headers: BufferHandle,
    frame: BufferHandle,
}

#[derive(Clone, Copy)]
struct LightResources {
    data: ResourceId,
    indices: ResourceId,
    headers: ResourceId,
    frame: ResourceId,
}

pub(crate) struct LightFeatures {
    slots: Vec<LightSlot>,
    resources: Option<LightResources>,
    pipeline: ComputePipelineDesc,
    size: Size2D,
}

impl LightFeatures {
    pub(crate) fn new(renderer: &mut Renderer, resources: &ResourceManager) -> AppResult<Self> {
        let source = std::fs::read_to_string(resources.shader_path("lighting/light_cull.wgsl"))?
            .replace(
                "#include \"../common/lighting_types.wgsl\"",
                &std::fs::read_to_string(resources.shader_path("common/lighting_types.wgsl"))?,
            );
        let pipeline = ComputePipelineDesc {
            wgsl: source,
            entry: "cs_main".into(),
        };
        pipeline.interface().map_err(other_error)?;
        let size = renderer.swapchain_extent();
        let descs = descriptors(size);
        let mut allocated = Vec::new();
        let result = (|| {
            let mut slots = Vec::with_capacity(FRAME_SLOTS);
            for _ in 0..FRAME_SLOTS {
                let mut create = |desc| {
                    let handle = renderer.create_buffer(desc).map_err(graphics_error)?;
                    allocated.push(handle);
                    Ok::<_, AppError>(handle)
                };
                slots.push(LightSlot {
                    data: create(descs[0])?,
                    indices: create(descs[1])?,
                    headers: create(descs[2])?,
                    frame: create(descs[3])?,
                });
            }
            Ok(Self {
                slots,
                resources: None,
                pipeline,
                size,
            })
        })();
        if result.is_err() {
            for handle in allocated {
                let _ = renderer.destroy_buffer(handle);
            }
        }
        result
    }

    pub(crate) fn install_graph(&mut self, graph: &mut FrameGraph) -> AppResult<()> {
        let slot = self.slots[0];
        let descs = descriptors(self.size);
        self.resources = Some(LightResources {
            data: graph
                .import_buffer("scene_light_data", slot.data, descs[0])
                .map_err(other_error)?,
            indices: graph
                .import_buffer("scene_light_indices", slot.indices, descs[1])
                .map_err(other_error)?,
            headers: graph
                .import_buffer("scene_light_headers", slot.headers, descs[2])
                .map_err(other_error)?,
            frame: graph
                .import_buffer("scene_light_frame", slot.frame, descs[3])
                .map_err(other_error)?,
        });
        let dispatch = self.dispatch()?;
        let accesses = dispatch.accesses().map_err(other_error)?;
        graph
            .insert_pass(
                0,
                PassDesc::new("light_culling", PassType::Compute, vec![], vec![])
                    .with_buffer_accesses(accesses)
                    .with_commands([ComputeCommand::Dispatch(dispatch)]),
            )
            .map_err(other_error)?;
        let (commands, accesses) = self.clear_commands()?;
        graph
            .insert_pass(
                0,
                PassDesc::new("light_tiles_clear", PassType::Transfer, vec![], vec![])
                    .with_buffer_accesses(accesses)
                    .with_commands(commands),
            )
            .map_err(other_error)?;
        if graph.pass_id("geometry").is_some() {
            graph
                .extend_pass_buffer_accesses("geometry", self.graphics_accesses()?)
                .map_err(other_error)?;
        }
        Ok(())
    }

    pub(crate) fn resize(
        &mut self,
        renderer: &mut Renderer,
        graph: &mut FrameGraph,
        size: Size2D,
    ) -> AppResult<()> {
        if size == self.size {
            return Ok(());
        }
        let resources = self
            .resources
            .ok_or_else(|| other_error("Light resources are not installed"))?;
        let descs = descriptors(size);
        let mut replacements = Vec::with_capacity(FRAME_SLOTS);
        let mut allocated = Vec::new();
        for slot in &self.slots {
            let result = (|| {
                let indices = renderer.create_buffer(descs[1]).map_err(graphics_error)?;
                allocated.push(indices);
                let headers = renderer.create_buffer(descs[2]).map_err(graphics_error)?;
                allocated.push(headers);
                Ok::<_, AppError>(LightSlot {
                    indices,
                    headers,
                    ..*slot
                })
            })();
            match result {
                Ok(slot) => replacements.push(slot),
                Err(error) => {
                    for handle in allocated {
                        let _ = renderer.destroy_buffer(handle);
                    }
                    return Err(error);
                }
            }
        }
        graph
            .redefine_imported_buffer(resources.indices, replacements[0].indices, descs[1])
            .map_err(other_error)?;
        graph
            .redefine_imported_buffer(resources.headers, replacements[0].headers, descs[2])
            .map_err(other_error)?;
        let old = std::mem::replace(&mut self.slots, replacements);
        self.size = size;
        for slot in old {
            renderer
                .destroy_buffer(slot.indices)
                .map_err(graphics_error)?;
            renderer
                .destroy_buffer(slot.headers)
                .map_err(graphics_error)?;
        }
        Ok(())
    }

    pub(crate) fn prepare_frame(
        &mut self,
        renderer: &mut Renderer,
        graph: &mut FrameGraph,
        token: &FrameToken,
        lights: &[PointLightGPU],
        uniforms: &FrameUniforms,
        size: Size2D,
    ) -> AppResult<()> {
        if size != self.size {
            return Err(other_error(
                "Resize scene lights before acquiring the frame",
            ));
        }
        let resources = self
            .resources
            .ok_or_else(|| other_error("Light resources are not installed"))?;
        let slot = self
            .slots
            .get(token.slot())
            .ok_or_else(|| other_error("Invalid light frame slot"))?;
        for (id, handle) in [
            (resources.data, slot.data),
            (resources.indices, slot.indices),
            (resources.headers, slot.headers),
            (resources.frame, slot.frame),
        ] {
            graph
                .rebind_imported_buffer(id, handle)
                .map_err(other_error)?;
        }
        let count = lights.len().min(MAX_LIGHTS);
        let mut data = vec![0u8; MAX_LIGHTS * std::mem::size_of::<PointLightGPU>()];
        data[..std::mem::size_of_val(&lights[..count])]
            .copy_from_slice(bytemuck::cast_slice(&lights[..count]));
        renderer
            .write_buffer(token, slot.data, 0, &data)
            .map_err(graphics_error)?;
        let frame = LightFrame {
            view: uniforms.view_matrix,
            projection: uniforms.proj_matrix,
            light_count: count as u32,
            tiles_x: size.width.max(1).div_ceil(TILE_SIZE),
            tiles_y: size.height.max(1).div_ceil(TILE_SIZE),
            width: size.width.max(1),
            height: size.height.max(1),
            padding: [0; 3],
        };
        renderer
            .write_buffer(token, slot.frame, 0, bytemuck::bytes_of(&frame))
            .map_err(graphics_error)?;
        let dispatch = self.dispatch()?;
        let (commands, accesses) = self.clear_commands()?;
        graph
            .set_pass_commands(
                graph
                    .pass_id("light_tiles_clear")
                    .ok_or_else(|| other_error("Light clear pass is missing"))?,
                commands,
                accesses,
            )
            .map_err(other_error)?;
        graph
            .set_pass_commands(
                graph
                    .pass_id("light_culling")
                    .ok_or_else(|| other_error("Light pass is missing"))?,
                vec![ComputeCommand::Dispatch(dispatch.clone())],
                dispatch.accesses().map_err(other_error)?,
            )
            .map_err(other_error)
    }

    pub(crate) fn graphics_bindings(&self) -> AppResult<Vec<BufferBinding>> {
        let resources = self
            .resources
            .ok_or_else(|| other_error("Light resources are not installed"))?;
        Ok([resources.data, resources.indices, resources.headers]
            .into_iter()
            .enumerate()
            .map(|(binding, resource)| BufferBinding {
                group: 3,
                binding: binding as u32,
                resource,
                range: BufferByteRange::new(0, descriptors(self.size)[binding].size),
                stages: ShaderStages::FRAGMENT,
            })
            .collect())
    }

    pub(crate) fn graphics_accesses(&self) -> AppResult<Vec<BufferAccess>> {
        Ok(self
            .graphics_bindings()?
            .into_iter()
            .map(|binding| {
                BufferAccess::new(
                    binding.resource,
                    katla_gfx::render_graph::ResourceAccessMode::Read,
                    katla_gfx::render_graph::BufferUsage::Storage,
                    katla_gfx::render_graph::ResourceAccessStage::FragmentShader,
                    binding.range,
                )
            })
            .collect())
    }

    fn clear_commands(&self) -> AppResult<(Vec<ComputeCommand>, Vec<BufferAccess>)> {
        let resources = self
            .resources
            .ok_or_else(|| other_error("Light resources are not installed"))?;
        let range = BufferByteRange::new(0, descriptors(self.size)[2].size);
        Ok((
            vec![ComputeCommand::FillBuffer {
                resource: resources.headers,
                range,
                value: 0,
            }],
            vec![BufferAccess::transfer_write(resources.headers).with_range(range)],
        ))
    }

    fn dispatch(&self) -> AppResult<ComputeDispatch> {
        let resources = self
            .resources
            .ok_or_else(|| other_error("Light resources are not installed"))?;
        Ok(ComputeDispatch {
            pipeline: self.pipeline.clone(),
            bindings: [
                resources.data,
                resources.indices,
                resources.headers,
                resources.frame,
            ]
            .into_iter()
            .enumerate()
            .map(|(binding, resource)| ComputeBinding {
                group: 0,
                binding: binding as u32,
                resource,
                range: BufferByteRange::new(0, descriptors(self.size)[binding].size),
            })
            .collect(),
            constants: vec![],
            size: ComputeDispatchSize::Direct([
                self.size.width.max(1).div_ceil(TILE_SIZE),
                self.size.height.max(1).div_ceil(TILE_SIZE),
                1,
            ]),
        })
    }
}

fn descriptors(size: Size2D) -> [BufferDesc; 4] {
    let tiles = u64::from(size.width.max(1).div_ceil(TILE_SIZE))
        * u64::from(size.height.max(1).div_ceil(TILE_SIZE));
    let sizes = [
        MAX_LIGHTS as u64 * std::mem::size_of::<PointLightGPU>() as u64,
        tiles * LIGHTS_PER_TILE * 4,
        tiles * 4,
        std::mem::size_of::<LightFrame>() as u64,
    ];
    std::array::from_fn(|index| {
        BufferDesc::new(
            sizes[index],
            BufferUsages::STORAGE
                | BufferUsages::UNIFORM
                | BufferUsages::TRANSFER_SOURCE
                | BufferUsages::TRANSFER_DESTINATION,
            if index == 1 || index == 2 {
                BufferMemoryPolicy::DeviceLocal
            } else {
                BufferMemoryPolicy::CpuVisible
            },
        )
    })
}

fn graphics_error(source: katla_gfx::RendererError) -> AppError {
    AppError::Graphics { source }
}
fn other_error(error: impl std::fmt::Display) -> AppError {
    AppError::RendererInitFailed {
        reason: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_light_buffer_abi_and_partial_tiles() {
        assert_eq!(std::mem::size_of::<LightFrame>(), 160);
        assert_eq!(std::mem::size_of::<PointLightGPU>(), 32);
        let descs = descriptors(Size2D::new(17, 31));
        assert_eq!(descs[0].size, 8192);
        assert_eq!(descs[1].size, 4 * 128 * 4);
        assert_eq!(descs[2].size, 16);
        assert_eq!(descs[0].memory, BufferMemoryPolicy::CpuVisible);
        assert_eq!(descs[1].memory, BufferMemoryPolicy::DeviceLocal);
        assert_eq!(descs[2].memory, BufferMemoryPolicy::DeviceLocal);
        assert_eq!(descs[3].memory, BufferMemoryPolicy::CpuVisible);
    }
}
