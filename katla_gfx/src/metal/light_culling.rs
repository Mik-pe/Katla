//! Metal Forward+ tile-based light culling subsystem.
//!
//! Manages GPU buffers and compute pipeline for building per-tile light lists
//! from dynamic point lights. Each tile covers a 16×16 pixel region of the screen.
//!
//! The compute shader projects light spheres into screen space and tests tile AABB
//! overlap to determine which lights affect each tile.

use bytemuck::{Pod, Zeroable};
use log::info;
use objc2_metal::MTLCommandBuffer;

use crate::backend::command::{GpuCommandBuffer, GpuComputeEncoder};
use crate::backend::resource::GpuBuffer;
use crate::error::RendererError;
use crate::metal::buffer::MetalBuffer;

use super::context::MetalContext;
use super::pipeline::MetalComputePipeline;
use super::shader;

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
    light_buffer: super::buffer::MetalBuffer,
    /// Storage buffer: per-tile visible light indices (u32 array).
    tile_index_buffer: super::buffer::MetalBuffer,
    /// Storage buffer: per-tile light counts (u32 array, used atomically by shader).
    tile_count_buffer: super::buffer::MetalBuffer,
    /// Uniform buffer: frame data (view/proj matrices, tile params).
    frame_data_buffer: super::buffer::MetalBuffer,
    /// Compute pipeline for light culling.
    pipeline: MetalComputePipeline,
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

        // Compile the light culling compute shader
        let shader_rel = "lighting/light_cull.wgsl";
        let wgsl_source = find_and_read_shader(shader_rel)?;

        let compiled =
            shader::compile_wgsl_to_metal(&context.device, &wgsl_source, &["cs_main"], false)?;

        let cs_function = compiled.module.entry_points.get("cs_main").ok_or_else(|| {
            RendererError::InvalidOperation(
                "cs_main entry point not found in light culling shader".into(),
            )
        })?;

        let pipeline = context.create_compute_pipeline(cs_function, [TILE_SIZE, TILE_SIZE, 1])?;

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
            light_buffer,
            tile_index_buffer,
            tile_count_buffer,
            frame_data_buffer,
            pipeline,
            tiles_x,
            tiles_y,
            screen_width,
            screen_height,
            light_count: 0,
            prev_light_count: 0,
        })
    }

    pub fn light_buffer(&self) -> &MetalBuffer {
        &self.light_buffer
    }

    pub fn tile_index_buffer(&self) -> &MetalBuffer {
        &self.tile_index_buffer
    }

    pub fn tile_count_buffer(&self) -> &MetalBuffer {
        &self.tile_count_buffer
    }

    pub fn light_count(&self) -> u32 {
        self.light_count
    }

    /// Upload point light data to the GPU.
    ///
    /// Call once per frame before dispatching light culling.
    /// Stale entries beyond the new count are zeroed when the light list shrinks.
    pub fn upload_lights(&mut self, lights: &[PointLightGPU]) {
        let new_count = lights.len().min(MAX_POINT_LIGHTS as usize) as u32;
        self.light_count = new_count;

        let ptr = self.light_buffer.map() as *mut PointLightGPU;

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

        self.light_buffer.unmap();
        self.prev_light_count = new_count;
    }

    /// Run the light culling compute pass.
    ///
    /// Clears tile counters, writes frame data, and dispatches the compute shader.
    /// Call after uploading lights, before the geometry pass.
    pub fn dispatch_light_culling(
        &mut self,
        context: &MetalContext,
        view_matrix: &[f32; 16],
        proj_matrix: &[f32; 16],
    ) {
        if self.light_count == 0 {
            return;
        }

        // Zero the tile count buffer
        {
            let ptr = self.tile_count_buffer.map();
            let count_size = (self.tiles_x * self.tiles_y) as usize * 4;
            unsafe {
                std::ptr::write_bytes(ptr, 0, count_size);
            }
            self.tile_count_buffer.unmap();
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
            let ptr = self.frame_data_buffer.map();
            unsafe {
                std::ptr::copy_nonoverlapping(
                    &frame_data as *const LightCullFrameData as *const u8,
                    ptr,
                    std::mem::size_of::<LightCullFrameData>(),
                );
            }
            self.frame_data_buffer.unmap();
        }

        // Dispatch compute
        let mut cmd_buffer = context.create_command_buffer();
        cmd_buffer.begin();

        let mut encoder = cmd_buffer.begin_compute_pass();
        encoder.bind_compute_pipeline(&self.pipeline);

        // Binding 0: point light data (storage buffer, read)
        encoder.bind_storage_buffer(&self.light_buffer, 0, 0);
        // Binding 1: tile light indices (storage buffer, read_write)
        encoder.bind_storage_buffer(&self.tile_index_buffer, 0, 1);
        // Binding 2: tile light counts (storage buffer, read_write)
        encoder.bind_storage_buffer(&self.tile_count_buffer, 0, 2);
        // Binding 3: frame data (uniform buffer)
        encoder.bind_storage_buffer(&self.frame_data_buffer, 0, 3);

        // One workgroup per tile
        encoder.dispatch(self.tiles_x, self.tiles_y, 1);
        encoder.end_encoding();

        cmd_buffer.end();
        cmd_buffer.submit(context);
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

        self.tile_index_buffer = context.create_buffer(tile_index_size, true)?;
        self.tile_count_buffer = context.create_buffer(tile_count_size, true)?;

        {
            let ptr = self.tile_index_buffer.map();
            unsafe {
                std::ptr::write_bytes(ptr, 0, tile_index_size as usize);
            }
            self.tile_index_buffer.unmap();
        }
        {
            let ptr = self.tile_count_buffer.map();
            unsafe {
                std::ptr::write_bytes(ptr, 0, tile_count_size as usize);
            }
            self.tile_count_buffer.unmap();
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

/// Find and read a shader file by probing common relative paths.
///
/// Shader files live at `resources/shaders/` in the workspace root, but the
/// CWD at runtime varies (workspace root, crate dir, etc.). This helper tries
/// multiple relative locations, resolves `#include` directives, and returns
/// the fully expanded source.
fn find_and_read_shader(rel_path: &str) -> Result<String, RendererError> {
    let candidates: Vec<std::path::PathBuf> = [
        format!("resources/shaders/{rel_path}"),
        format!("../resources/shaders/{rel_path}"),
        format!("../../resources/shaders/{rel_path}"),
    ]
    .iter()
    .map(std::path::PathBuf::from)
    .collect();

    for path in &candidates {
        if path.exists() {
            let raw = std::fs::read_to_string(path).map_err(|e| {
                RendererError::InvalidOperation(format!(
                    "Failed to read shader '{}': {}",
                    path.display(),
                    e
                ))
            })?;
            let resolved = resolve_wgsl_includes(&raw, path)?;
            return Ok(resolved);
        }
    }

    Err(RendererError::InvalidOperation(format!(
        "Failed to find shader '{}': tried {:?}",
        rel_path,
        candidates.iter().map(|p| p.display()).collect::<Vec<_>>()
    )))
}

/// Recursively resolve `#include "..."` directives in WGSL source.
fn resolve_wgsl_includes(
    source: &str,
    file_path: &std::path::Path,
) -> Result<String, RendererError> {
    let mut result = String::new();
    let base_dir = file_path.parent().unwrap_or(std::path::Path::new("."));

    for line in source.lines() {
        let trimmed = line.trim();
        if let Some(path_str) = trimmed
            .strip_prefix("//include ")
            .or_else(|| trimmed.strip_prefix("#include "))
        {
            let path_str = path_str.trim();
            let include_rel = path_str
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix('"'))
                .unwrap_or(path_str);
            let include_path = base_dir.join(include_rel);
            let include_source = std::fs::read_to_string(&include_path).map_err(|e| {
                RendererError::InvalidOperation(format!(
                    "Failed to read include '{}': {}",
                    include_path.display(),
                    e
                ))
            })?;
            let expanded = resolve_wgsl_includes(&include_source, &include_path)?;
            result.push_str(&expanded);
        } else {
            result.push_str(line);
        }
        result.push('\n');
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let ptr = lc.light_buffer.map() as *const PointLightGPU;
        let read_lights = unsafe { std::slice::from_raw_parts(ptr, 2) };
        assert_eq!(read_lights[0].position, [1.0, 2.0, 3.0]);
        assert_eq!(read_lights[0].range, 10.0);
        assert_eq!(read_lights[1].color, [1.0, 0.0, 0.0]);
        assert_eq!(read_lights[1].intensity, 100.0);
        lc.light_buffer.unmap();
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
    fn test_metal_light_culling_dispatch_empty() {
        let ctx = create_context();
        let mut lc = MetalLightCulling::new(&ctx, 800, 600).unwrap();

        // No lights uploaded — dispatch should be a no-op
        lc.dispatch_light_culling(&ctx, &[0.0; 16], &[0.0; 16]);
    }
}
