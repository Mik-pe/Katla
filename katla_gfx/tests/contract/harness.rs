//! The contract harness: one backend-neutral renderer wrapper shared by every
//! scenario.
//!
//! Platform selection lives here and nowhere else. Linux constructs a headless
//! `VulkanRenderer` (CI runs it on lavapipe with the Khronos validation
//! layers), macOS constructs a headless `MetalRenderer`. Everything above
//! [`ContractRenderer`] is backend-neutral: `AnyRenderer`, the `GpuRenderer`
//! trait, `AnyFrameGraph`, and readback pixels in one shared byte order.
//!
//! ## API validation caveat on Vulkan
//!
//! Compiling a PBR pipeline while the Khronos validation layer is active
//! segfaults the Intel driver on the canonical Linux machine, so scenarios
//! that compile PBR materials open the renderer with
//! [`ContractRenderer::open_without_api_validation`]. UI-material scenarios
//! capture API validation errors through the Vulkan callback
//! (`Capabilities::api_validation_capture`) and assert the log is empty in
//! [`ContractRenderer::finish`]. Metal scenarios always run with backend
//! diagnostics enabled; CI additionally sets `MTL_DEBUG_LAYER=1`.
//!
//! Pixel conventions: readback bytes are BGRA on both backends and row 0 is
//! the top row. Scenarios should prefer flip-robust probes (center pixels,
//! left/right splits, whole-frame scans).

use std::ffi::CString;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[cfg(target_os = "macos")]
use katla_gfx::MetalRenderer;
#[cfg(not(target_os = "macos"))]
use katla_gfx::VulkanRenderer;
use katla_gfx::render_graph::any_frame::AnyFrame;
use katla_gfx::render_graph::any_frame_graph::AnyFrameGraph;
use katla_gfx::render_graph::{FrameGraphBuilder, PassId};
use katla_gfx::renderer::features::RendererFeature;
use katla_gfx::renderer::pipeline_descriptor::CullMode;
use katla_gfx::renderer::pipeline_descriptor::{BlendMode, DepthState, PipelineDescriptor};
use katla_gfx::texture::ImageFormat;
use katla_gfx::vertex::VertexPBR;
use katla_gfx::{FrameUniforms, GpuRenderer, MaterialHandle, TextureHandle, ValidationMode};

pub const WIDTH: u32 = 64;
pub const HEIGHT: u32 = 48;

/// What the platform's backend can do, stated once per target triple.
///
/// Scenarios branch on these values (never on backend names or `cfg!`) when
/// the backends legitimately differ. Both current backends can read back
/// pixels and report the feature table, so most entries are true everywhere;
/// the entries exist so a future backend with a real limitation extends this
/// table instead of sprinkling conditionals through scenarios.
pub struct Capabilities {
    pub backend_name: &'static str,
    /// Vulkan-layer error capture through the validation callback.
    pub api_validation_capture: bool,
    /// `pending_retirements()` snapshots are available.
    pub retirement_diagnostics: bool,
    /// `mesh_index_format` reports the created width; Metal normalizes u16
    /// indices to u32 at upload and reports the converted width.
    pub preserves_index_width: bool,
    /// Metal clip space is y-up while Vulkan's is y-down, and the app's
    /// tonemap pass performs the flip for the drawable. The suite renders
    /// straight to the drawable with no tonemap, so identical NDC geometry
    /// lands on vertically mirrored rows: Metal flips the probe mapping.
    pub flips_direct_ndc_y: bool,
    /// Expected `supports_feature` answer for every optional feature.
    pub feature_support: fn(RendererFeature) -> bool,
}

#[cfg(not(target_os = "macos"))]
fn platform_features(feature: RendererFeature) -> bool {
    // Vulkan composites UI through the frame graph, not a direct UI pass.
    feature != RendererFeature::DirectUiPass
}

#[cfg(target_os = "macos")]
fn platform_features(_feature: RendererFeature) -> bool {
    // Metal implements every optional renderer feature.
    true
}

#[cfg(not(target_os = "macos"))]
pub const CAPS: Capabilities = Capabilities {
    backend_name: "vulkan",
    api_validation_capture: true,
    retirement_diagnostics: true,
    preserves_index_width: true,
    flips_direct_ndc_y: false,
    feature_support: platform_features,
};

#[cfg(target_os = "macos")]
pub const CAPS: Capabilities = Capabilities {
    backend_name: "metal",
    api_validation_capture: false,
    retirement_diagnostics: false,
    preserves_index_width: false,
    flips_direct_ndc_y: true,
    feature_support: platform_features,
};

pub struct ContractRenderer {
    renderer: katla_gfx::AnyRenderer,
    validation_errors: Option<Arc<Mutex<Vec<String>>>>,
    /// Monotonic Vulkan readback ids (Metal reads back through its drawable).
    #[cfg(not(target_os = "macos"))]
    readback_index: usize,
}

impl ContractRenderer {
    /// Open a headless renderer with API validation capture enabled.
    ///
    /// Use for UI-material scenarios; PBR pipelines must use
    /// [`ContractRenderer::open_without_api_validation`] (driver caveat in
    /// the module docs).
    pub fn open(label: &str) -> Self {
        Self::open_with(label, true)
    }

    /// Open a headless renderer without Vulkan-layer validation capture.
    pub fn open_without_api_validation(label: &str) -> Self {
        Self::open_with(label, false)
    }

    fn open_with(label: &str, api_validation: bool) -> Self {
        let validation_mode = if api_validation {
            ValidationMode::Enabled
        } else {
            ValidationMode::Disabled
        };
        #[cfg(target_os = "macos")]
        let renderer = katla_gfx::AnyRenderer::new_metal_headless(
            WIDTH,
            HEIGHT,
            validation_mode,
            CString::new(label).expect("label"),
            CString::new("Katla").expect("engine"),
        )
        .expect("headless Metal renderer");
        #[cfg(not(target_os = "macos"))]
        let mut renderer = katla_gfx::AnyRenderer::new_vulkan_headless(
            WIDTH,
            HEIGHT,
            validation_mode,
            CString::new(label).expect("label"),
            CString::new("Katla").expect("engine"),
        )
        .expect("headless Vulkan renderer");

        // The log always exists; only the Vulkan callback feeds it ( Metal
        // surfaces API misuse through command-buffer failures instead).
        let validation_errors = Arc::new(Mutex::new(Vec::new()));
        #[cfg(not(target_os = "macos"))]
        if let Some(vulkan) = renderer.as_vulkan().filter(|_| api_validation) {
            let captured = validation_errors.clone();
            vulkan
                .context()
                .set_validation_callback(move |message, level| {
                    if level == katla_gfx::ValidationLevel::Error {
                        captured
                            .lock()
                            .expect("validation log")
                            .push(message.to_owned());
                    }
                });
        }

        Self {
            renderer,
            validation_errors: Some(validation_errors),
            #[cfg(not(target_os = "macos"))]
            readback_index: 0,
        }
    }

    /// The backend-neutral renderer for resource creation and queries.
    pub fn gfx(&mut self) -> &mut katla_gfx::AnyRenderer {
        &mut self.renderer
    }

    /// Deferred-retirement snapshot where the backend exposes it.
    pub fn pending_retirements(&mut self) -> Option<katla_gfx::RetirementSnapshot> {
        #[cfg(not(target_os = "macos"))]
        {
            Some(
                self.renderer
                    .as_vulkan()
                    .expect("vulkan backend")
                    .pending_retirements(),
            )
        }
        #[cfg(target_os = "macos")]
        {
            let _ = &mut self.renderer;
            None
        }
    }

    /// Compile the shared light-culling resources every graph frame needs.
    pub fn init_frame_pipelines(&mut self) {
        self.renderer
            .init_light_culling(WIDTH, HEIGHT, &shaders().join("lighting/light_cull.wgsl"))
            .expect("light culling init");
    }

    /// Compile the shadow descriptor layouts PBR pipelines require before
    /// they can be compiled.
    pub fn init_shadow_layouts(&mut self) {
        self.renderer
            .init_shadow_resources()
            .expect("shadow resource init");
    }

    /// The platform capability table.
    pub fn caps(&self) -> &'static Capabilities {
        &CAPS
    }

    /// Render one frame, submit it, and read the backbuffer back as BGRA8.
    ///
    /// Follows the canonical order (wait → uniforms → object data → render)
    /// and blocks until the pixels are readable. The graph must already be
    /// compiled; pass ids come from [`pass_id`].
    pub fn render_frame(
        &mut self,
        graph: &mut AnyFrameGraph,
        uniforms: Option<&FrameUniforms>,
        draw_list: Option<&katla_gfx::DrawList>,
        submit: impl FnOnce(&mut AnyFrame),
    ) -> Vec<u8> {
        self.renderer.wait_for_frame().expect("wait_for_frame");
        if let Some(uniforms) = uniforms {
            self.renderer.set_frame_uniforms(uniforms.clone());
        }
        if let Some(draw_list) = draw_list {
            self.renderer
                .execute_draw_calls(draw_list)
                .expect("execute_draw_calls");
        }

        #[cfg(target_os = "macos")]
        let drawable = {
            // A fresh Shared-storage texture per frame, kept alive by this
            // clone: the renderer replaces its drawable each frame and the
            // texture persists until readback (same flow as the app's
            // headless captures).
            let texture = self.renderer.create_offscreen_texture(WIDTH, HEIGHT);
            let keep = texture.clone();
            self.renderer.set_headless_drawable(texture);
            keep
        };

        self.renderer.render(graph, submit).expect("render");

        #[cfg(target_os = "macos")]
        {
            self.renderer
                .wait_for_frame()
                .expect("wait before headless readback");
            katla_gfx::AnyRenderer::readback_bgra_texture(&drawable, WIDTH, HEIGHT)
        }
        #[cfg(not(target_os = "macos"))]
        {
            let vulkan = self.renderer.as_vulkan().expect("vulkan backend");
            vulkan
                .queue_async_readback(self.readback_index)
                .expect("queue_async_readback");
            let (_, pixels) = vulkan
                .wait_for_pending_readback()
                .expect("wait_for_pending_readback")
                .expect("headless readback");
            self.readback_index += 1;
            pixels
        }
    }

    /// Tear the scenario down and assert the captured API validation log is
    /// empty where capture exists. Clean up every graph with [`cleanup_graph`]
    /// before calling this.
    pub fn finish(mut self) {
        GpuRenderer::destroy(&mut self.renderer);
        if let Some(errors) = self
            .validation_errors
            .as_ref()
            .filter(|_| CAPS.api_validation_capture)
        {
            let errors = errors.lock().expect("validation log");
            assert!(
                errors.is_empty(),
                "{} backend reported validation errors: {errors:?}",
                CAPS.backend_name
            );
        }
    }
}

/// Compile a backend-neutral frame graph from the platform's backend.
pub fn build_graph(build: impl FnOnce(FrameGraphBuilder) -> FrameGraphBuilder) -> AnyFrameGraph {
    let builder = build(FrameGraphBuilder::new());
    #[cfg(target_os = "macos")]
    {
        AnyFrameGraph::from_metal(builder.build::<MetalRenderer>().expect("graph compiles"))
    }
    #[cfg(not(target_os = "macos"))]
    {
        AnyFrameGraph::from_vulkan(builder.build::<VulkanRenderer>().expect("graph compiles"))
    }
}

/// Release a scenario graph's GPU resources before the renderer is destroyed.
pub fn cleanup_graph(mut graph: AnyFrameGraph) {
    graph.cleanup();
}

/// Look up a compiled pass id by name.
pub fn pass_id(graph: &AnyFrameGraph, name: &str) -> PassId {
    graph.pass_id(name).expect("pass exists in contract graph")
}

/// Path to the shared WGSL shader sources.
pub fn shaders() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../resources/shaders")
}

/// The shared unculled, depth-free PBR material for geometry scenarios.
pub fn compile_pbr_material(renderer: &mut katla_gfx::AnyRenderer) -> MaterialHandle {
    let descriptor = PipelineDescriptor::pbr(shaders().join("model_pbr.wgsl").to_string_lossy())
        .with_blend(BlendMode::Opaque)
        .with_cull(CullMode::None)
        .with_depth(DepthState::disabled())
        .with_color_format(ImageFormat::B8G8R8A8Srgb);
    renderer
        .compile_material(&descriptor)
        .expect("PBR contract material compiles")
}

/// The UI material used by composition scenarios (alpha-blended, unculled,
/// no depth, sRGB output).
pub fn compile_ui_material(renderer: &mut katla_gfx::AnyRenderer) -> MaterialHandle {
    let descriptor = PipelineDescriptor::ui(shaders().join("ui/ui.wgsl").to_string_lossy());
    renderer
        .compile_material(&descriptor)
        .expect("UI contract material compiles")
}

/// Resources every UI composition scenario needs: the UI material, a white
/// sample texture with its bindless slot, and the required font atlas.
pub struct UiScene {
    pub material: MaterialHandle,
    pub white_slot: u32,
    _atlas: TextureHandle,
}

pub fn init_ui_scene(renderer: &mut katla_gfx::AnyRenderer) -> UiScene {
    let atlas = renderer
        .create_ui_font_atlas(1, 1, &[255, 0, 0, 255])
        .expect("test font atlas creation");
    let white = renderer
        .create_texture(&katla_gfx::TextureDescriptor::rgba8_unorm(1, 1), &[255; 4])
        .expect("test texture creation");
    let white_slot = renderer.get_bindless_slot(white).expect("bindless slot");
    let material = compile_ui_material(renderer);
    UiScene {
        material,
        white_slot,
        _atlas: atlas,
    }
}

/// BGRA channel constants for [`dominant_channel`].
pub const CHANNEL_BLUE: usize = 0;
pub const CHANNEL_GREEN: usize = 1;
pub const CHANNEL_RED: usize = 2;

/// The channel (blue/green/red) that is bright while the other two are dark,
/// if any — enough to tell the test tints apart from dark backgrounds.
pub fn dominant_channel(pixels: &[u8], at: usize) -> Option<usize> {
    let bgra = &pixels[at..at + 4];
    for channel in 0..3 {
        let dominant = bgra[channel] > 120;
        let others_dark = (0..3).all(|other| other == channel || bgra[other] < 120);
        if dominant && others_dark {
            return Some(channel);
        }
    }
    None
}

/// Byte offset of the pixel at NDC coordinates (row 0 is the top row on both
/// backends' readback; the mapping accounts for the platform's clip-space
/// direction via [`Capabilities::flips_direct_ndc_y`]).
pub fn pixel_offset(ndc_x: f32, ndc_y: f32) -> usize {
    let ndc_y = if CAPS.flips_direct_ndc_y {
        -ndc_y
    } else {
        ndc_y
    };
    let col = ((ndc_x + 1.0) * 0.5 * WIDTH as f32) as usize;
    let row = ((ndc_y + 1.0) * 0.5 * HEIGHT as f32) as usize;
    (row * WIDTH as usize + col) * 4
}

/// Byte offset of the pixel at UI pixel coordinates. UI space is top-down
/// pixels on both backends (the UI shader maps it per platform), so this is
/// the direct row/column mapping.
pub fn ui_pixel_offset(x: u32, y: u32) -> usize {
    (y as usize * WIDTH as usize + x as usize) * 4
}

/// True when no pixel in the frame is dominated by `channel` — a whole-frame
/// scan, immune to geometry placement and vertical flips.
pub fn no_pixel_dominates(pixels: &[u8], channel: usize) -> bool {
    !pixels
        .as_chunks::<4>()
        .0
        .iter()
        .any(|px| dominant_channel(px, 0) == Some(channel))
}

/// A column-major identity matrix for clip-space scenarios.
pub fn identity() -> [f32; 16] {
    let mut m = [0.0f32; 16];
    m[0] = 1.0;
    m[5] = 1.0;
    m[10] = 1.0;
    m[15] = 1.0;
    m
}

/// A column-major 2D translation.
pub fn translation(offset: [f32; 2]) -> [f32; 16] {
    let mut m = identity();
    m[12] = offset[0];
    m[13] = offset[1];
    m
}

/// A clip-space triangle spanning ±0.5 around the origin at z = 0.5, sized
/// for identity view/projection matrices.
pub fn clip_triangle() -> Vec<VertexPBR> {
    vec![
        VertexPBR {
            position: [-0.5, -0.5, 0.5],
            normal: [0.0, 0.0, 1.0],
            tangent: [1.0, 0.0, 0.0, 1.0],
            tex_coord0: [0.0, 0.0],
        },
        VertexPBR {
            position: [0.5, -0.5, 0.5],
            normal: [0.0, 0.0, 1.0],
            tangent: [1.0, 0.0, 0.0, 1.0],
            tex_coord0: [1.0, 0.0],
        },
        VertexPBR {
            position: [0.0, 0.5, 0.5],
            normal: [0.0, 0.0, 1.0],
            tangent: [1.0, 0.0, 0.0, 1.0],
            tex_coord0: [0.5, 1.0],
        },
    ]
}

/// Standard identity-camera uniforms for clip-space scenarios.
pub fn clip_uniforms() -> FrameUniforms {
    FrameUniforms {
        view_matrix: identity(),
        proj_matrix: identity(),
        inv_view_proj_matrix: identity(),
        ..Default::default()
    }
}
