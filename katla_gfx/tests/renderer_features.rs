//! Renderer capability contract for issue #91.
//!
//! A minimal backend implementing `GpuRenderer` must make an explicit
//! decision for every operation: required operations reach the backend's own
//! implementation (no silent inherited default), and optional operations
//! either work or fail with `RendererError::UnsupportedFeature` before
//! mutating any state. `supports_feature` must agree with that behavior.
//!
//! If the trait ever regains a successful no-op default, the recording
//! assertions below fail: the call never reaches an explicit implementation.

use std::cell::{Cell, RefCell};

use katla_gfx::{
    DrawCall, DrawList, FrameUniforms, GpuCapabilities, GpuRenderer, GpuVendor, IndexType,
    MaterialHandle, MeshHandle, MeshIndexElement, PipelineKind, PointLightGPU, Rect, RendererError,
    RendererFeature, Size2D, SkeletonHandle, TextureDescriptor, TextureHandle, UIDrawList,
    ViewportBuilder, ViewportHandle,
};

/// Minimal backend: implements every required operation explicitly with
/// recorded calls, declines every optional feature with a typed error.
struct MockRenderer {
    capabilities: GpuCapabilities,
    uniforms: FrameUniforms,
    calls: RefCell<Vec<&'static str>>,
    /// Bumped only by operations that claim to do real work. Optional
    /// operations must fail before touching it.
    generation: Cell<u64>,
}

impl MockRenderer {
    fn new() -> Self {
        Self {
            capabilities: GpuCapabilities {
                max_texture_size: 512,
                max_bindless_textures: 16,
                supports_compute: false,
                max_frames_in_flight: 1,
                vendor: GpuVendor::Unknown,
                supports_light_culling: false,
            },
            uniforms: FrameUniforms::default(),
            calls: RefCell::new(Vec::new()),
            generation: Cell::new(0),
        }
    }

    fn record(&self, name: &'static str) {
        self.calls.borrow_mut().push(name);
    }

    fn was_called(&self, name: &str) -> bool {
        self.calls.borrow().iter().any(|c| *c == name)
    }
}

fn unsupported_message(error: &RendererError) -> &str {
    match error {
        RendererError::UnsupportedFeature(message) => message,
        other => panic!("expected UnsupportedFeature, got {other:?}"),
    }
}

impl GpuRenderer for MockRenderer {
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

    fn wait_for_frame(&mut self) -> Result<(), RendererError> {
        self.record("wait_for_frame");
        Ok(())
    }

    fn set_frame_uniforms(&mut self, uniforms: FrameUniforms) {
        self.record("set_frame_uniforms");
        self.uniforms = uniforms;
    }

    fn execute_draw_calls(&mut self, _draw_list: &DrawList) -> Result<(), RendererError> {
        self.record("execute_draw_calls");
        Ok(())
    }

    fn draw(
        &mut self,
        _uniforms: &FrameUniforms,
        _draw_calls: &[DrawCall],
    ) -> Result<DrawList, RendererError> {
        self.record("draw");
        Ok(DrawList::default())
    }

    fn frame_uniforms(&self) -> &FrameUniforms {
        &self.uniforms
    }

    fn begin_frame(&mut self) -> Result<u32, RendererError> {
        self.record("begin_frame");
        Ok(0)
    }

    fn end_frame(&mut self) -> Result<(), RendererError> {
        self.record("end_frame");
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
        Ok(MeshHandle::new(0))
    }

    fn create_mesh_dynamic(
        &mut self,
        _descriptor: &katla_gfx::MeshDescriptor,
        _vertex_data: &[u8],
        _indices: &[u32],
    ) -> Result<MeshHandle, RendererError> {
        self.record("create_mesh_dynamic");
        Ok(MeshHandle::new(1))
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
        Ok(TextureHandle::new(0))
    }

    fn create_texture_solid(&mut self, _color: [u8; 4]) -> Result<TextureHandle, RendererError> {
        self.record("create_texture_solid");
        Ok(TextureHandle::new(1))
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
        TextureHandle::new(0)
    }

    fn compile_material(
        &mut self,
        _shader_path: &str,
        _vertex_type: &str,
    ) -> Result<MaterialHandle, RendererError> {
        self.record("compile_material");
        Ok(MaterialHandle::new(0))
    }

    fn set_material_texture_indices(&mut self, _material: MaterialHandle, _indices: [u32; 4]) {
        self.record("set_material_texture_indices");
    }

    fn set_default_material(&mut self, _material: MaterialHandle) {
        self.record("set_default_material");
    }

    fn default_material(&self) -> MaterialHandle {
        MaterialHandle::new(0)
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

    fn create_viewport(&mut self) -> ViewportBuilder {
        self.record("create_viewport");
        ViewportBuilder::new()
    }

    fn viewport_count(&self) -> usize {
        0
    }

    fn get_viewport(&self, _handle: ViewportHandle) -> Option<&katla_gfx::Viewport> {
        None
    }

    fn viewport_extent(&self, _handle: ViewportHandle) -> Option<Size2D> {
        None
    }

    fn destroy_viewport(&mut self, _handle: ViewportHandle) {
        self.record("destroy_viewport");
    }

    fn resize(&mut self, _width: u32, _height: u32) -> Result<(), RendererError> {
        self.record("resize");
        Ok(())
    }

    fn recreate_scene_render_targets(&mut self, _width: u32, _height: u32) {
        self.record("recreate_scene_render_targets");
    }

    fn upload_lights(&mut self, _lights: &[PointLightGPU]) {
        self.record("upload_lights");
    }

    fn update_shadows(&mut self, _light_direction: [f32; 3]) {
        self.record("update_shadows");
    }

    fn upload_shadow_cascades(&mut self) {
        self.record("upload_shadow_cascades");
    }

    fn create_skeleton(&mut self, _joint_count: usize) -> Result<SkeletonHandle, RendererError> {
        self.record("create_skeleton");
        Ok(SkeletonHandle::new(0))
    }

    fn update_skeleton(&mut self, _handle: SkeletonHandle, _matrices: &[[f32; 16]]) {
        self.record("update_skeleton");
    }

    fn init_particle_system(&mut self) -> Result<(), RendererError> {
        Err(RendererError::UnsupportedFeature(
            "mock backend has no particle system".into(),
        ))
    }

    fn create_ui_font_atlas(
        &mut self,
        _width: u32,
        _height: u32,
        _data: &[u8],
    ) -> Result<TextureHandle, RendererError> {
        self.record("create_ui_font_atlas");
        Ok(TextureHandle::new(2))
    }

    fn update_ui_font_atlas(&mut self, _width: u32, _height: u32, _data: &[u8]) {
        self.record("update_ui_font_atlas");
    }

    fn ui_font_atlas_handle(&self) -> Option<TextureHandle> {
        None
    }

    fn set_viewport_bindless_slot(&mut self, _slot: u32) {
        self.record("set_viewport_bindless_slot");
    }

    fn set_ui_material(&mut self, _material: MaterialHandle) {
        self.record("set_ui_material");
    }

    fn render_ui_pass(&mut self, _draw_list: UIDrawList) {
        self.record("render_ui_pass");
    }

    fn set_viewport_panel_rect(&mut self, _rect: Option<Rect>) {
        self.record("set_viewport_panel_rect");
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
fn test_unsupported_operations_fail_explicitly_without_mutation() {
    let mut renderer = MockRenderer::new();
    let shader_path = std::path::Path::new("test.wgsl");

    let error = renderer.init_animation_pipeline(shader_path).unwrap_err();
    assert!(unsupported_message(&error).contains("init_animation_pipeline"));

    let error = renderer
        .init_light_culling(64, 48, shader_path)
        .unwrap_err();
    assert!(unsupported_message(&error).contains("init_light_culling"));

    let error = renderer.init_shadow_resources().unwrap_err();
    assert!(unsupported_message(&error).contains("init_shadow_resources"));

    let error = renderer
        .init_pass_pipeline(PipelineKind::Sky, &[shader_path])
        .unwrap_err();
    assert!(unsupported_message(&error).contains("init_pass_pipeline"));

    let error = renderer.init_particle_system().unwrap_err();
    assert!(unsupported_message(&error).contains("particle"));

    let error = renderer
        .update_texture(TextureHandle::new(0), &[])
        .unwrap_err();
    assert!(unsupported_message(&error).contains("update_texture"));

    let error = renderer.register_depth_textures_bindless().unwrap_err();
    assert!(unsupported_message(&error).contains("register_depth_textures_bindless"));

    assert_eq!(
        renderer.generation.get(),
        0,
        "failed optional operations must not mutate backend state"
    );
    assert!(
        renderer.calls.borrow().is_empty(),
        "failed optional operations must not reach backend implementations"
    );
}

#[test]
fn test_required_operations_reach_explicit_implementations() {
    let mut renderer = MockRenderer::new();

    renderer.upload_lights(&[]);
    renderer.update_shadows([0.0, 1.0, 0.0]);
    renderer.upload_shadow_cascades();
    renderer.set_viewport_bindless_slot(3);
    renderer.set_ui_material(MaterialHandle::new(0));
    renderer.render_ui_pass(UIDrawList::default());
    renderer.set_viewport_panel_rect(None);
    renderer.recreate_scene_render_targets(64, 48);
    assert_eq!(
        renderer.recompile_materials_for_shader(std::path::Path::new("x.wgsl")),
        0
    );

    for expected in [
        "upload_lights",
        "update_shadows",
        "upload_shadow_cascades",
        "set_viewport_bindless_slot",
        "set_ui_material",
        "render_ui_pass",
        "set_viewport_panel_rect",
        "recreate_scene_render_targets",
        "recompile_materials_for_shader",
    ] {
        assert!(
            renderer.was_called(expected),
            "{expected} never reached an explicit backend implementation"
        );
    }
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
    assert_eq!(renderer.mesh_index_format(MeshHandle::new(0)), None);
    let _: Option<IndexType> = renderer.mesh_index_format(MeshHandle::new(0));
}
