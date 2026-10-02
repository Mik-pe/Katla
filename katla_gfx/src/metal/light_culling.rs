//! Metal Forward+ tile-based light culling subsystem.
//!
//! Manages GPU buffers and compute pipeline for building per-tile light lists
//! from dynamic point lights. Each tile covers a 16×16 pixel region of the screen.
//!
//! The compute shader projects light spheres into screen space and tests tile AABB
//! overlap to determine which lights affect each tile.

use bytemuck::{Pod, Zeroable};
use log::info;

use crate::backend::resource::GpuBuffer;
use crate::error::RendererError;
use crate::metal::buffer::MetalBuffer;

use super::context::MetalContext;
use super::metal_renderer::FRAMES_IN_FLIGHT;

/// Maximum number of point lights supported.
const MAX_POINT_LIGHTS: u32 = 256;

/// Tile size in pixels (width and height).
const TILE_SIZE: u32 = 16;

/// Maximum number of lights per tile.
const MAX_LIGHTS_PER_TILE: u32 = 128;

/// GPU representation of a point light (32 bytes).
///
/// Must match WGSL `PointLightGPU` exactly.
pub type PointLightGPU = crate::renderer::types::PointLightGPU;

/// Frame data for the light culling compute shader.
///
/// Must match WGSL `LightCullFrameData` exactly.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct LightCullFrameData {
    view_matrix: [f32; 16],
    proj_matrix: [f32; 16],
    light_count: u32,
    tiles_x: u32,
    tiles_y: u32,
    screen_width: u32,
    screen_height: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
}

/// Metal-native Forward+ light culling subsystem.
///
/// Owns all GPU state for tile-based light culling:
/// - Light buffer (point light data uploaded from CPU)
/// - Tile light index buffer (per-tile light lists, written by compute)
/// - Tile light count buffer (per-tile atomic counters, written by compute)
/// - Frame data buffer (view/proj matrices and tile params)
/// - Compute pipeline compiled from `light_culling.wgsl`
pub(crate) struct MetalLightCulling {
    /// Storage buffer: point light data array.
    light_buffer: [super::buffer::MetalBuffer; FRAMES_IN_FLIGHT],
    /// Storage buffer: per-tile visible light indices (u32 array).
    tile_index_buffer: [super::buffer::MetalBuffer; FRAMES_IN_FLIGHT],
    /// Storage buffer: per-tile light counts (u32 array, used atomically by shader).
    tile_count_buffer: [super::buffer::MetalBuffer; FRAMES_IN_FLIGHT],
    /// Uniform buffer: frame data (view/proj matrices, tile params).
    frame_data_buffer: [super::buffer::MetalBuffer; FRAMES_IN_FLIGHT],
    /// Compute pipeline for light culling.
    active_slot: usize,
    /// Number of tiles in X and Y.
    tiles_x: u32,
    tiles_y: u32,
    /// Screen dimensions.
    screen_width: u32,
    screen_height: u32,
    /// Number of lights uploaded in the current frame.
    light_count: u32,
    /// Number of lights uploaded in the previous frame (for stale-entry cleanup).
    prev_light_count: u32,
}

impl MetalLightCulling {
    /// Initialize the light culling subsystem.
    ///
    /// Creates GPU buffers for the given screen dimensions and compiles
    /// the light culling compute shader.
    pub fn new(
        context: &MetalContext,
        screen_width: u32,
        screen_height: u32,
    ) -> Result<Self, RendererError> {
        let tiles_x = screen_width.div_ceil(TILE_SIZE);
        let tiles_y = screen_height.div_ceil(TILE_SIZE);
        let num_tiles = tiles_x * tiles_y;

        let light_buffer_size =
            (MAX_POINT_LIGHTS as u64) * (std::mem::size_of::<PointLightGPU>() as u64);
        let tile_index_size = (num_tiles as u64) * (MAX_LIGHTS_PER_TILE as u64) * 4;
        let tile_count_size = (num_tiles as u64) * 4;
        let frame_data_size = std::mem::size_of::<LightCullFrameData>() as u64;

        info!(
            "Creating Metal light culling: {}x{}, {}x{} tiles, \
             light={}KB, tile_idx={}KB, tile_cnt={}KB, frame_data={}B",
            screen_width,
            screen_height,
            tiles_x,
            tiles_y,
            light_buffer_size / 1024,
            tile_index_size / 1024,
            tile_count_size / 1024,
            frame_data_size,
        );

        let light_buffer = context.create_buffer(light_buffer_size, true)?;
        let tile_index_buffer = context.create_buffer(tile_index_size, true)?;
        let tile_count_buffer = context.create_buffer(tile_count_size, true)?;
        let frame_data_buffer = context.create_buffer(frame_data_size, true)?;

        // Zero all buffers initially
        {
            let ptr = light_buffer.map();
            unsafe {
                std::ptr::write_bytes(ptr, 0, light_buffer_size as usize);
            }
            light_buffer.unmap();
        }
        {
            let ptr = tile_index_buffer.map();
            unsafe {
                std::ptr::write_bytes(ptr, 0, tile_index_size as usize);
            }
            tile_index_buffer.unmap();
        }
        {
            let ptr = tile_count_buffer.map();
            unsafe {
                std::ptr::write_bytes(ptr, 0, tile_count_size as usize);
            }
            tile_count_buffer.unmap();
        }

        Ok(Self {
            light_buffer: [
                light_buffer,
                context.create_buffer(light_buffer_size, true)?,
                context.create_buffer(light_buffer_size, true)?,
            ],
            tile_index_buffer: [
                tile_index_buffer,
                context.create_buffer(tile_index_size, true)?,
                context.create_buffer(tile_index_size, true)?,
            ],
            tile_count_buffer: [
                tile_count_buffer,
                context.create_buffer(tile_count_size, true)?,
                context.create_buffer(tile_count_size, true)?,
            ],
            frame_data_buffer: [
                frame_data_buffer,
                context.create_buffer(frame_data_size, true)?,
                context.create_buffer(frame_data_size, true)?,
            ],
            active_slot: 0,
            tiles_x,
            tiles_y,
            screen_width,
            screen_height,
            light_count: 0,
            prev_light_count: 0,
        })
    }

    pub fn light_buffer(&self) -> &MetalBuffer {
        &self.light_buffer[self.active_slot]
    }

    pub fn tile_index_buffer(&self) -> &MetalBuffer {
        &self.tile_index_buffer[self.active_slot]
    }

    pub fn tile_count_buffer(&self) -> &MetalBuffer {
        &self.tile_count_buffer[self.active_slot]
    }

    /// Upload point light data to the GPU.
    ///
    /// Call once per frame before dispatching light culling.
    /// Stale entries beyond the new count are zeroed when the light list shrinks.
    pub fn upload_lights(&mut self, lights: &[PointLightGPU]) {
        let new_count = lights.len().min(MAX_POINT_LIGHTS as usize) as u32;
        self.light_count = new_count;

        let ptr = self.light_buffer[self.active_slot].map() as *mut PointLightGPU;
        unsafe {
            std::ptr::write_bytes(ptr, 0, MAX_POINT_LIGHTS as usize);
        }

        // Zero stale entries when the light list shrinks
        if new_count < self.prev_light_count {
            let dst = unsafe { std::slice::from_raw_parts_mut(ptr, MAX_POINT_LIGHTS as usize) };
            for item in dst
                .iter_mut()
                .take(self.prev_light_count as usize)
                .skip(new_count as usize)
            {
                *item = PointLightGPU {
                    position: [0.0; 3],
                    range: 0.0,
                    color: [0.0; 3],
                    intensity: 0.0,
                };
            }
        }

        // Copy active lights
        if new_count > 0 {
            let dst = unsafe { std::slice::from_raw_parts_mut(ptr, MAX_POINT_LIGHTS as usize) };
            dst[..new_count as usize].copy_from_slice(&lights[..new_count as usize]);
        }

        self.light_buffer[self.active_slot].unmap();
        self.prev_light_count = new_count;
    }

    /// Run the light culling compute pass.
    ///
    /// Clears tile counters, writes frame data, and dispatches the compute shader.
    /// Call after uploading lights, before the geometry pass.
    pub(crate) fn prepare_frame(&mut self, view_matrix: &[f32; 16], proj_matrix: &[f32; 16]) {
        // Zero the tile count buffer
        {
            let ptr = self.tile_count_buffer[self.active_slot].map();
            let count_size = (self.tiles_x * self.tiles_y) as usize * 4;
            unsafe {
                std::ptr::write_bytes(ptr, 0, count_size);
            }
            self.tile_count_buffer[self.active_slot].unmap();
        }

        // Write frame data
        {
            let frame_data = LightCullFrameData {
                view_matrix: *view_matrix,
                proj_matrix: *proj_matrix,
                light_count: self.light_count,
                tiles_x: self.tiles_x,
                tiles_y: self.tiles_y,
                screen_width: self.screen_width,
                screen_height: self.screen_height,
                _pad0: 0,
                _pad1: 0,
                _pad2: 0,
            };
            let ptr = self.frame_data_buffer[self.active_slot].map();
            unsafe {
                std::ptr::copy_nonoverlapping(
                    &frame_data as *const LightCullFrameData as *const u8,
                    ptr,
                    std::mem::size_of::<LightCullFrameData>(),
                );
            }
            self.frame_data_buffer[self.active_slot].unmap();
        }
    }

    pub(crate) fn select_slot(&mut self, slot: usize) {
        self.active_slot = slot;
    }
    pub(crate) fn frame_buffer(&self) -> &MetalBuffer {
        &self.frame_data_buffer[self.active_slot]
    }
    pub(crate) fn dispatch_size(&self) -> [u32; 3] {
        [self.tiles_x, self.tiles_y, 1]
    }

    /// Recreate tile buffers for new screen dimensions.
    ///
    /// Call after window resize to keep tile grid in sync.
    pub fn resize(
        &mut self,
        context: &MetalContext,
        screen_width: u32,
        screen_height: u32,
    ) -> Result<(), RendererError> {
        let tiles_x = screen_width.div_ceil(TILE_SIZE);
        let tiles_y = screen_height.div_ceil(TILE_SIZE);
        let num_tiles = tiles_x * tiles_y;

        let tile_index_size = (num_tiles as u64) * (MAX_LIGHTS_PER_TILE as u64) * 4;
        let tile_count_size = (num_tiles as u64) * 4;

        for slot in 0..FRAMES_IN_FLIGHT {
            self.tile_index_buffer[slot] = context.create_buffer(tile_index_size, true)?;
        }
        for slot in 0..FRAMES_IN_FLIGHT {
            self.tile_count_buffer[slot] = context.create_buffer(tile_count_size, true)?;
        }

        {
            let ptr = self.tile_index_buffer[self.active_slot].map();
            unsafe {
                std::ptr::write_bytes(ptr, 0, tile_index_size as usize);
            }
            self.tile_index_buffer[self.active_slot].unmap();
        }
        {
            let ptr = self.tile_count_buffer[self.active_slot].map();
            unsafe {
                std::ptr::write_bytes(ptr, 0, tile_count_size as usize);
            }
            self.tile_count_buffer[self.active_slot].unmap();
        }

        self.tiles_x = tiles_x;
        self.tiles_y = tiles_y;
        self.screen_width = screen_width;
        self.screen_height = screen_height;

        info!(
            "Metal light culling resized: {}x{}, {}x{} tiles",
            screen_width, screen_height, tiles_x, tiles_y,
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::command::{GpuCommandBuffer, GpuComputeEncoder};

    fn create_context() -> MetalContext {
        MetalContext::init_headless().expect("Failed to create headless context")
    }

    #[test]
    fn test_metal_light_culling_init() {
        let ctx = create_context();
        let lc = MetalLightCulling::new(&ctx, 1920, 1080);
        assert!(lc.is_ok(), "Failed to init light culling: {:?}", lc.err());
        let lc = lc.unwrap();
        assert_eq!(lc.tiles_x, 120);
        assert_eq!(lc.tiles_y, 68);
        assert_eq!(lc.screen_width, 1920);
        assert_eq!(lc.screen_height, 1080);
    }

    #[test]
    fn test_metal_light_culling_upload() {
        let ctx = create_context();
        let mut lc = MetalLightCulling::new(&ctx, 800, 600).unwrap();

        let lights = vec![
            PointLightGPU {
                position: [1.0, 2.0, 3.0],
                range: 10.0,
                color: [1.0, 1.0, 1.0],
                intensity: 50.0,
            },
            PointLightGPU {
                position: [4.0, 5.0, 6.0],
                range: 20.0,
                color: [1.0, 0.0, 0.0],
                intensity: 100.0,
            },
        ];

        lc.upload_lights(&lights);
        assert_eq!(lc.light_count, 2);

        // Verify data was written correctly
        let ptr = lc.light_buffer[lc.active_slot].map() as *const PointLightGPU;
        let read_lights = unsafe { std::slice::from_raw_parts(ptr, 2) };
        assert_eq!(read_lights[0].position, [1.0, 2.0, 3.0]);
        assert_eq!(read_lights[0].range, 10.0);
        assert_eq!(read_lights[1].color, [1.0, 0.0, 0.0]);
        assert_eq!(read_lights[1].intensity, 100.0);
        lc.light_buffer[lc.active_slot].unmap();
    }

    #[test]
    fn test_metal_light_culling_resize() {
        let ctx = create_context();
        let mut lc = MetalLightCulling::new(&ctx, 800, 600).unwrap();
        assert_eq!(lc.tiles_x, 50);
        assert_eq!(lc.tiles_y, 38);

        let result = lc.resize(&ctx, 1920, 1080);
        assert!(result.is_ok(), "Resize failed: {:?}", result.err());
        assert_eq!(lc.tiles_x, 120);
        assert_eq!(lc.tiles_y, 68);
        assert_eq!(lc.screen_width, 1920);
        assert_eq!(lc.screen_height, 1080);
    }

    #[test]
    fn test_metal_light_culling_dispatch_writes_tile_lists() {
        use crate::render_graph::{BuiltinComputeKernel, ComputeKernel, RenderGraphBackend};
        let mut renderer =
            super::super::metal_renderer::MetalRenderer::new(create_context()).unwrap();
        let descriptor = ComputeKernel::Builtin(BuiltinComputeKernel::LightCulling).descriptor();
        renderer.prepare_compute_pipeline(&descriptor).unwrap();
        let ctx = &renderer.context;
        let pipeline = renderer.compute_pipelines.get(&descriptor).unwrap();
        let mut lc = MetalLightCulling::new(ctx, 32, 32).unwrap();
        lc.upload_lights(&[PointLightGPU {
            position: [0.0; 3],
            range: 100.0,
            color: [1.0; 3],
            intensity: 1.0,
        }]);
        let identity = [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ];
        lc.prepare_frame(&identity, &identity);
        let mut command = ctx.create_command_buffer();
        command.begin();
        let mut encoder = command.begin_compute_pass();
        encoder.bind_compute_pipeline(pipeline);
        encoder.bind_storage_buffer(lc.light_buffer(), 0, 0);
        encoder.bind_storage_buffer(lc.tile_index_buffer(), 0, 1);
        encoder.bind_storage_buffer(lc.tile_count_buffer(), 0, 2);
        encoder.bind_storage_buffer(lc.frame_buffer(), 0, 3);
        let [x, y, z] = lc.dispatch_size();
        encoder.dispatch(x, y, z);
        encoder.end_encoding();
        command.end();
        command.submit(ctx);
        command.wait_until_completed().unwrap();
        let counts =
            unsafe { std::slice::from_raw_parts(lc.tile_count_buffer().map().cast::<u32>(), 4) };
        assert_eq!(counts, &[1; 4]);
        lc.tile_count_buffer().unmap();
        let indices = unsafe {
            std::slice::from_raw_parts(
                lc.tile_index_buffer().map().cast::<u32>(),
                4 * MAX_LIGHTS_PER_TILE as usize,
            )
        };
        for tile in 0..4 {
            assert_eq!(indices[tile * MAX_LIGHTS_PER_TILE as usize], 0);
        }
        lc.tile_index_buffer().unmap();
    }
}
