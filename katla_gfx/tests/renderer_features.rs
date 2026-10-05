//! Minimal device implementation without scene or editor feature methods.

use katla_gfx::renderer::texture_readback::{
    GraphTextureSource, TextureReadbackData, TextureReadbackRegion, TextureReadbackTicket,
};
use katla_gfx::{BufferDesc, BufferHandle, BufferMemoryPolicy, BufferUsages};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use katla_gfx::renderer::frame_scope::{FrameAcquisition, FrameToken, PresentOutcome};
use katla_gfx::{
    DrawList, GpuCapabilities, GpuRenderer, GpuVendor, IndexType, MaterialHandle, MeshHandle,
    MeshIndexElement, PipelineDescriptor, RendererError, RendererFeature, Size2D, SkeletonHandle,
    TextureDescriptor, TextureHandle,
};

/// Minimal backend: implements every required operation explicitly with
/// recorded calls, declines every optional feature with a typed error.
struct MockRenderer {
    capabilities: GpuCapabilities,
    calls: RefCell<Vec<&'static str>>,
    /// Bumped only by operations that claim to do real work. Optional
    /// operations must fail before touching it.
    generation: Cell<u64>,
    /// The currently open frame token from `acquire_frame`.
    active_frame: Option<FrameToken>,
    buffers: HashMap<BufferHandle, (BufferDesc, Vec<u8>)>,
    next_buffer: u32,
}

impl MockRenderer {
    fn new() -> Self {
        Self {
            capabilities: GpuCapabilities {
                clip_y_down: false,
                max_texture_size: 512,
                max_bindless_textures: 16,
                supports_compute: false,
                max_frames_in_flight: 1,
                vendor: GpuVendor::Unknown,
            },
            calls: RefCell::new(Vec::new()),
            generation: Cell::new(0),
            active_frame: None,
            buffers: HashMap::new(),
            next_buffer: 0,
        }
    }

    fn record(&self, name: &'static str) {
        self.calls.borrow_mut().push(name);
    }

    fn was_called(&self, name: &str) -> bool {
        self.calls.borrow().contains(&name)
    }
}

fn unsupported_message(error: &RendererError) -> &str {
    match error {
        RendererError::UnsupportedFeature(message) => message,
        other => panic!("expected UnsupportedFeature, got {other:?}"),
    }
}

impl GpuRenderer for MockRenderer {
    fn create_buffer(&mut self, desc: BufferDesc) -> Result<BufferHandle, RendererError> {
        self.record("create_buffer");
        let size = usize::try_from(desc.size)
            .map_err(|_| RendererError::InvalidOperation("buffer size".into()))?;
        let handle = BufferHandle::from_raw(self.next_buffer, 0);
        self.next_buffer += 1;
        self.buffers.insert(handle, (desc, vec![0; size]));
        Ok(handle)
    }
    fn buffer_descriptor(&self, handle: BufferHandle) -> Option<BufferDesc> {
        self.buffers.get(&handle).map(|entry| entry.0)
    }
    fn create_buffer_with_data(
        &mut self,
        desc: BufferDesc,
        data: &[u8],
    ) -> Result<BufferHandle, RendererError> {
        if data.len() as u64 > desc.size {
            return Err(RendererError::InvalidOperation(
                "initial data exceeds allocation".into(),
            ));
        }
        let handle = self.create_buffer(desc)?;
        self.buffers.get_mut(&handle).unwrap().1[..data.len()].copy_from_slice(data);
        Ok(handle)
    }
    fn write_buffer(
        &mut self,
        frame: &FrameToken,
        handle: BufferHandle,
        offset: u64,
        data: &[u8],
    ) -> Result<(), RendererError> {
        if self.active_frame != Some(*frame) {
            return Err(RendererError::InvalidOperation("frame not acquired".into()));
        }
        let entry = self
            .buffers
            .get_mut(&handle)
            .ok_or_else(|| RendererError::StaleHandle {
                resource: "buffer".into(),
                detail: format!("{handle:?}"),
            })?;
        if entry.0.memory != BufferMemoryPolicy::CpuVisible {
            return Err(RendererError::InvalidOperation(
                "buffer not CPU writable".into(),
            ));
        }
        let start = usize::try_from(offset)
            .map_err(|_| RendererError::InvalidOperation("offset overflow".into()))?;
        let end = start
            .checked_add(data.len())
            .ok_or_else(|| RendererError::InvalidOperation("range overflow".into()))?;
        let destination = entry
            .1
            .get_mut(start..end)
            .ok_or_else(|| RendererError::InvalidOperation("range exceeds allocation".into()))?;
        destination.copy_from_slice(data);
        self.record("write_buffer");
        Ok(())
    }
    fn destroy_buffer(&mut self, handle: BufferHandle) -> Result<(), RendererError> {
        self.record("destroy_buffer");
        self.buffers.remove(&handle);
        Ok(())
    }
    fn frame_slot_count(&self) -> usize {
        1
    }
    fn skeleton_buffer_handle(
        &mut self,
        _frame: &FrameToken,
        _skeleton: SkeletonHandle,
    ) -> Result<BufferHandle, RendererError> {
        Err(RendererError::UnsupportedFeature(
            "mock has no skeleton storage".into(),
        ))
    }
    fn read_buffer_completed(
        &mut self,
        handle: BufferHandle,
        range: katla_gfx::render_graph::BufferByteRange,
    ) -> Result<Option<Vec<u8>>, RendererError> {
        let entry = self
            .buffers
            .get(&handle)
            .ok_or_else(|| RendererError::StaleHandle {
                resource: "buffer".into(),
                detail: format!("{handle:?}"),
            })?;
        if entry.0.memory != BufferMemoryPolicy::Readback {
            return Err(RendererError::InvalidOperation(
                "buffer not a readback allocation".into(),
            ));
        }
        let start = usize::try_from(range.offset)
            .map_err(|_| RendererError::InvalidOperation("offset overflow".into()))?;
        let size = usize::try_from(range.size)
            .map_err(|_| RendererError::InvalidOperation("size overflow".into()))?;
        let end = start
            .checked_add(size)
            .ok_or_else(|| RendererError::InvalidOperation("range overflow".into()))?;
        let bytes = entry
            .1
            .get(start..end)
            .ok_or_else(|| RendererError::InvalidOperation("range exceeds allocation".into()))?;
        Ok(Some(bytes.to_vec()))
    }

    fn graph_texture_source(
        &self,
        _resource: katla_gfx::render_graph::ResourceId,
    ) -> Option<GraphTextureSource> {
        None
    }
    fn queue_texture_readback(
        &mut self,
        _source: GraphTextureSource,
        _region: TextureReadbackRegion,
    ) -> Result<TextureReadbackTicket, RendererError> {
        Err(RendererError::UnsupportedFeature(
            "mock has no texture readback".into(),
        ))
    }
    fn poll_texture_readback(
        &mut self,
        _ticket: TextureReadbackTicket,
    ) -> Result<Option<TextureReadbackData>, RendererError> {
        Err(RendererError::UnsupportedFeature(
            "mock has no texture readback".into(),
        ))
    }

    fn swapchain_extent(&self) -> Size2D {
        Size2D::new(64, 48)
    }

    fn current_frame(&self) -> usize {
        0
    }

    fn num_images(&self) -> usize {
        1
    }

    fn wait_for_device(&self) {
        self.record("wait_for_device");
    }

    fn destroy(&mut self) {
        self.record("destroy");
    }

    fn capabilities(&self) -> &GpuCapabilities {
        &self.capabilities
    }

    fn supports_feature(&self, _feature: RendererFeature) -> bool {
        false
    }

    fn acquire_frame(&mut self) -> Result<FrameAcquisition, RendererError> {
        self.record("acquire_frame");
        self.generation.set(self.generation.get() + 1);
        let token = FrameToken::new(0);
        self.active_frame = Some(token);
        Ok(FrameAcquisition::Ready(token))
    }

    fn execute_draw_calls(
        &mut self,
        frame: &FrameToken,
        _draw_list: &DrawList,
    ) -> Result<(), RendererError> {
        if self.active_frame != Some(*frame) {
            return Err(RendererError::InvalidOperation("frame not acquired".into()));
        }
        self.record("execute_draw_calls");
        Ok(())
    }

    fn present(&mut self, frame: FrameToken) -> Result<PresentOutcome, RendererError> {
        if self.active_frame != Some(frame) {
            return Err(RendererError::InvalidOperation("frame not acquired".into()));
        }
        self.record("present");
        self.active_frame = None;
        Ok(PresentOutcome::presented())
    }

    fn abort(&mut self, _frame: FrameToken) -> Result<(), RendererError> {
        self.record("abort");
        self.active_frame = None;
        Ok(())
    }

    fn create_mesh<T, U>(
        &mut self,
        _vertices: &[T],
        _indices: &[U],
        _topology: katla_gfx::PrimitiveTopology,
    ) -> Result<MeshHandle, RendererError>
    where
        T: katla_gfx::Vertex,
        U: MeshIndexElement,
    {
        self.record("create_mesh");
        Ok(MeshHandle::from_raw(0, 0))
    }

    fn create_mesh_dynamic(
        &mut self,
        _descriptor: &katla_gfx::MeshDescriptor,
        _vertex_data: &[u8],
        _indices: &[u32],
    ) -> Result<MeshHandle, RendererError> {
        self.record("create_mesh_dynamic");
        Ok(MeshHandle::from_raw(1, 0))
    }

    fn update_mesh_dynamic(
        &mut self,
        _mesh: MeshHandle,
        _vertex_data: &[u8],
        _vertex_count: u32,
        _indices: &[u32],
    ) -> Result<(), RendererError> {
        self.record("update_mesh_dynamic");
        Ok(())
    }

    fn create_texture(
        &mut self,
        _desc: &TextureDescriptor,
        _data: &[u8],
    ) -> Result<TextureHandle, RendererError> {
        self.record("create_texture");
        Ok(TextureHandle::from_raw(0, 0))
    }

    fn create_texture_solid(&mut self, _color: [u8; 4]) -> Result<TextureHandle, RendererError> {
        self.record("create_texture_solid");
        Ok(TextureHandle::from_raw(1, 0))
    }

    fn get_bindless_slot(&self, _handle: TextureHandle) -> Option<u32> {
        None
    }

    fn get_texture_at_slot(&self, _slot: u32) -> Option<TextureHandle> {
        None
    }

    fn get_texture_bindless_index(&self, _handle: TextureHandle) -> u32 {
        0
    }

    fn default_texture(&self) -> TextureHandle {
        TextureHandle::from_raw(0, 0)
    }

    fn compile_material(
        &mut self,
        _descriptor: &PipelineDescriptor,
    ) -> Result<MaterialHandle, RendererError> {
        self.record("compile_material");
        Ok(MaterialHandle::from_raw(0, 0))
    }

    fn set_material_textures(
        &mut self,
        _material: MaterialHandle,
        _textures: katla_gfx::MaterialTextures,
    ) {
        self.record("set_material_textures");
    }

    fn recompile_materials_for_shader(&mut self, _shader_path: &std::path::Path) -> usize {
        self.record("recompile_materials_for_shader");
        0
    }

    fn destroy_mesh(&mut self, _handle: MeshHandle) {
        self.record("destroy_mesh");
    }

    fn destroy_material(&mut self, _handle: MaterialHandle) {
        self.record("destroy_material");
    }

    fn destroy_texture(&mut self, _handle: TextureHandle) {
        self.record("destroy_texture");
    }

    fn destroy_skeleton(&mut self, _handle: SkeletonHandle) {
        self.record("destroy_skeleton");
    }

    fn resize(&mut self, _width: u32, _height: u32) -> Result<(), RendererError> {
        self.record("resize");
        Ok(())
    }

    fn create_skeleton(&mut self, _joint_count: usize) -> Result<SkeletonHandle, RendererError> {
        self.record("create_skeleton");
        Ok(SkeletonHandle::from_raw(0, 0))
    }
}

#[test]
fn test_minimal_backend_reports_no_optional_features() {
    let renderer = MockRenderer::new();
    for feature in RendererFeature::ALL {
        assert!(
            !renderer.supports_feature(*feature),
            "mock backend must not claim {}",
            feature.name()
        );
    }
}

#[test]
fn test_unsupported_texture_updates_do_not_mutate_backend() {
    let mut renderer = MockRenderer::new();
    let error = renderer
        .update_texture(TextureHandle::from_raw(0, 0), &[])
        .unwrap_err();
    assert!(unsupported_message(&error).contains("update_texture"));
    let error = renderer
        .update_texture_region(
            TextureHandle::from_raw(0, 0),
            katla_gfx::texture::TextureUploadRegion::base(&TextureDescriptor::rgba8_unorm(1, 1)),
            &[],
        )
        .unwrap_err();
    assert!(unsupported_message(&error).contains("subresource"));
    assert_eq!(renderer.generation.get(), 0);
    assert!(renderer.calls.borrow().is_empty());
}

#[test]
fn test_core_resources_work_without_editor_services() {
    let mut renderer = MockRenderer::new();
    let desc = BufferDesc::new(8, BufferUsages::STORAGE, BufferMemoryPolicy::CpuVisible);
    let handle = renderer
        .create_buffer_with_data(desc, &[1, 2, 3, 4])
        .unwrap();
    assert_eq!(renderer.buffer_descriptor(handle), Some(desc));
    let FrameAcquisition::Ready(frame) = renderer.acquire_frame().unwrap() else {
        panic!("frame unavailable")
    };
    renderer
        .write_buffer(&frame, handle, 4, &[5, 6, 7, 8])
        .unwrap();
    assert_eq!(renderer.buffers[&handle].1, [1, 2, 3, 4, 5, 6, 7, 8]);
    assert!(renderer.was_called("create_buffer"));
    assert!(renderer.was_called("write_buffer"));
    assert_eq!(renderer.frame_slot_count(), 1);
    assert_eq!(
        renderer
            .present(frame)
            .unwrap()
            .surface
            .expect("surface presentation"),
        katla_gfx::SurfaceStatus::Presented
    );
    assert!(renderer.write_buffer(&frame, handle, 0, &[9]).is_err());
    renderer.destroy_buffer(handle).unwrap();
    assert_eq!(renderer.buffer_descriptor(handle), None);
    assert!(renderer.capture_submission_snapshot().is_none());
}

#[test]
fn test_buffer_write_rejects_stale_frame_and_bounds_without_mutation() {
    let mut renderer = MockRenderer::new();
    let handle = renderer
        .create_buffer(BufferDesc::new(
            4,
            BufferUsages::UNIFORM,
            BufferMemoryPolicy::CpuVisible,
        ))
        .unwrap();
    let FrameAcquisition::Ready(first) = renderer.acquire_frame().unwrap() else {
        panic!("frame unavailable")
    };
    let FrameAcquisition::Ready(second) = renderer.acquire_frame().unwrap() else {
        panic!("frame unavailable")
    };
    assert!(renderer.present(first).is_err());
    assert_eq!(renderer.active_frame, Some(second));
    assert!(!renderer.was_called("present"));
    assert!(renderer.write_buffer(&first, handle, 0, &[1]).is_err());
    assert!(renderer.write_buffer(&second, handle, 3, &[1, 2]).is_err());
    assert!(
        renderer
            .write_buffer(&second, handle, u64::MAX, &[1])
            .is_err()
    );
    assert_eq!(renderer.buffers[&handle].1, [0; 4]);
    renderer.abort(second).unwrap();
}

#[test]
fn test_completed_readback_requires_explicit_memory_policy_and_live_range() {
    use katla_gfx::render_graph::BufferByteRange;
    let mut renderer = MockRenderer::new();
    let handle = renderer
        .create_buffer_with_data(
            BufferDesc::new(4, BufferUsages::READBACK, BufferMemoryPolicy::Readback),
            &[3, 2, 1, 0],
        )
        .unwrap();
    assert_eq!(
        renderer
            .read_buffer_completed(handle, BufferByteRange::new(1, 2))
            .unwrap(),
        Some(vec![2, 1])
    );
    assert!(
        renderer
            .read_buffer_completed(handle, BufferByteRange::new(3, 2))
            .is_err()
    );
    renderer.destroy_buffer(handle).unwrap();
    assert!(
        renderer
            .read_buffer_completed(handle, BufferByteRange::new(0, 4))
            .is_err()
    );
    let ordinary = renderer
        .create_buffer(BufferDesc::new(
            4,
            BufferUsages::STORAGE,
            BufferMemoryPolicy::CpuVisible,
        ))
        .unwrap();
    assert!(
        renderer
            .read_buffer_completed(ordinary, BufferByteRange::new(0, 4))
            .is_err()
    );
}

#[test]
fn test_timestamp_hooks_stay_silent_when_unsupported() {
    let mut renderer = MockRenderer::new();
    assert!(!renderer.supports_feature(RendererFeature::TimestampQueries));
    renderer.begin_timestamp("frame");
    renderer.end_timestamp("frame");
    assert!(renderer.read_timestamps().is_empty());
}

#[test]
fn test_mesh_index_format_absent_is_explicit() {
    let renderer = MockRenderer::new();
    assert_eq!(renderer.mesh_index_format(MeshHandle::from_raw(0, 0)), None);
    let _: Option<IndexType> = renderer.mesh_index_format(MeshHandle::from_raw(0, 0));
}

#[test]
fn test_frame_tokens_have_unique_acquisition_identity_and_copy_preserves_ownership() {
    let mut first = MockRenderer::new();
    let mut second = MockRenderer::new();
    let FrameAcquisition::Ready(first_frame) = first.acquire_frame().unwrap() else {
        panic!("frame unavailable")
    };
    let FrameAcquisition::Ready(second_frame) = second.acquire_frame().unwrap() else {
        panic!("frame unavailable")
    };
    assert_eq!(first_frame.slot(), second_frame.slot());
    assert_ne!(first_frame, second_frame);
    assert!(second.present(first_frame).is_err());
    assert_eq!(second.active_frame, Some(second_frame));
    assert!(!second.was_called("present"));
    let copied = second_frame;
    assert_eq!(
        second.present(copied).unwrap().surface.unwrap(),
        katla_gfx::SurfaceStatus::Presented
    );
    assert!(second.present(second_frame).is_err());
    first.abort(first_frame).unwrap();
}
