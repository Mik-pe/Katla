//! 3D Model Preview Panel State
//!
//! Provides state management for the model preview panel that renders
//! selected GLTF models to an offscreen texture with an orbit camera.
//!
//! # Current Status
//! - UI panel with loading indicator: ✅
//! - Model stats display: ✅
//! - Animation controls UI: ✅
//! - 3D preview rendering: 🚧 (needs render graph integration)
//!
//! # Future Work
//! The actual 3D rendering should be done via the render graph system,
//! not by adding model-specific code to katla_vulkan. The render graph
//! can support multiple cameras/viewports for different purposes.

use std::path::PathBuf;
use std::rc::Rc;

use katla_math::{Mat4, Vec2, Vec3};
use katla_ui::TextureId;

use crate::util::{GLTFModel, LoadId};

/// Loading state for model preview.
#[derive(Debug, Clone)]
pub enum LoadState {
    /// No model loaded.
    Idle,
    /// Model is currently loading.
    Loading,
    /// Model loaded successfully.
    Loaded,
    /// Model failed to load.
    Failed(String),
}

/// Animation playback state.
#[derive(Debug, Clone, Default)]
pub struct AnimationPlaybackState {
    /// Whether animation is currently playing.
    pub playing: bool,
    /// Current animation index (if model has multiple).
    pub current_animation: usize,
    /// Current playback time in seconds.
    pub time: f32,
    /// Animation speed multiplier.
    pub speed: f32,
}

impl AnimationPlaybackState {
    pub fn new() -> Self {
        Self {
            playing: false,
            current_animation: 0,
            time: 0.0,
            speed: 1.0,
        }
    }
}

/// Orbit camera for interactive model viewing.
#[derive(Debug, Clone)]
pub struct OrbitCamera {
    /// Distance from target point.
    pub distance: f32,
    /// Horizontal rotation angle (yaw) in radians.
    pub yaw: f32,
    /// Vertical rotation angle (pitch) in radians.
    pub pitch: f32,
    /// Point the camera orbits around.
    pub target: Vec3,
    /// Starting position for drag operation.
    pub drag_start: Option<Vec2>,
    /// Camera rotation at drag start.
    pub drag_start_yaw: f32,
    pub drag_start_pitch: f32,
}

impl OrbitCamera {
    /// Create a new orbit camera with default settings.
    pub fn new() -> Self {
        Self {
            distance: 5.0,
            yaw: 0.0,
            pitch: 0.3,
            target: Vec3::new(0.0, 0.0, 0.0),
            drag_start: None,
            drag_start_yaw: 0.0,
            drag_start_pitch: 0.0,
        }
    }

    /// Update camera rotation from mouse drag delta.
    pub fn update_from_drag(&mut self, delta: Vec2) {
        self.yaw -= delta.x() * 0.01;
        self.pitch = (self.pitch + delta.y() * 0.01).clamp(-1.5, 1.5);
    }

    /// Start a drag operation.
    pub fn begin_drag(&mut self, mouse_pos: Vec2) {
        self.drag_start = Some(mouse_pos);
        self.drag_start_yaw = self.yaw;
        self.drag_start_pitch = self.pitch;
    }

    /// Update drag and return true if actively dragging.
    pub fn update_drag(&mut self, mouse_pos: Vec2) -> bool {
        if let Some(start) = self.drag_start {
            let delta = mouse_pos - start;
            self.yaw = self.drag_start_yaw - delta.x() * 0.01;
            self.pitch = (self.drag_start_pitch + delta.y() * 0.01).clamp(-1.5, 1.5);
            return true;
        }
        false
    }

    /// End the drag operation.
    pub fn end_drag(&mut self) {
        self.drag_start = None;
    }

    /// Check if currently dragging.
    pub fn is_dragging(&self) -> bool {
        self.drag_start.is_some()
    }

    /// Zoom the camera (adjust distance).
    pub fn zoom(&mut self, delta: f32) {
        self.distance = (self.distance - delta).clamp(0.5, 50.0);
    }

    /// Get the camera position in world space.
    pub fn position(&self) -> Vec3 {
        // Spherical to Cartesian conversion
        let x = self.distance * self.pitch.cos() * self.yaw.sin();
        let y = self.distance * self.pitch.sin();
        let z = self.distance * self.pitch.cos() * self.yaw.cos();
        self.target + Vec3::new(x, y, z)
    }

    /// Get the view matrix for this camera.
    pub fn view_matrix(&self) -> Mat4 {
        let position = self.position();
        let up = Vec3::new(0.0, 1.0, 0.0);
        Mat4::create_lookat(position, self.target, up)
    }

    /// Reset camera to default view.
    pub fn reset(&mut self) {
        self.distance = 5.0;
        self.yaw = 0.0;
        self.pitch = 0.3;
        self.target = Vec3::new(0.0, 0.0, 0.0);
    }

    /// Fit camera to view a bounding sphere.
    pub fn fit_to_bounds(&mut self, center: Vec3, radius: f32) {
        self.target = center;
        // Distance should be enough to see the whole model
        // FOV is typically 45-60 degrees, use 1.5x radius for padding
        self.distance = radius * 2.5;
        if self.distance < 1.0 {
            self.distance = 1.0;
        }
    }
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics about a loaded model.
#[derive(Debug, Clone, Default)]
pub struct ModelStats {
    /// Total vertex count.
    pub vertex_count: usize,
    /// Total triangle count.
    pub triangle_count: usize,
    /// Number of meshes.
    pub mesh_count: usize,
    /// Number of primitives.
    pub primitive_count: usize,
    /// Whether the model has animations.
    pub has_animations: bool,
    /// Animation names (if any).
    pub animation_names: Vec<String>,
    /// Whether the model has skinning.
    pub has_skinning: bool,
    /// Bounding sphere radius.
    pub bounds_radius: f32,
}

impl ModelStats {
    /// Compute stats from a loaded GLTF model.
    pub fn from_model(model: &GLTFModel) -> Self {
        let vertex_count = model.vertex_data.len().max(model.skinned_vertex_data.len());

        // Calculate triangle count from index data
        let triangle_count = match model.index_stride {
            2 => model.index_data.len() / 6, // 16-bit indices, 2 bytes per index, 3 indices per triangle
            4 => model.index_data.len() / 12, // 32-bit indices, 4 bytes per index, 3 indices per triangle
            _ => vertex_count / 3,           // No indices or unknown, estimate from vertices
        };

        // Count meshes and primitives
        let mut mesh_count = 0;
        let mut primitive_count = 0;
        for node in model.document.nodes() {
            if let Some(mesh) = node.mesh() {
                mesh_count += 1;
                primitive_count += mesh.primitives().count();
            }
        }

        // Get animation info
        let animations: Vec<_> = model.document.animations().collect();
        let has_animations = !animations.is_empty();
        let animation_names: Vec<String> = animations
            .iter()
            .map(|a| a.name().unwrap_or("Unnamed").to_string())
            .collect();

        Self {
            vertex_count,
            triangle_count,
            mesh_count,
            primitive_count,
            has_animations,
            animation_names,
            has_skinning: model.has_skinning,
            bounds_radius: model.bounds.radius,
        }
    }
}

/// State for the model preview panel.
pub struct ModelPreviewState {
    /// Path to the model being previewed.
    pub model_path: Option<PathBuf>,
    /// The loaded model (after background loading completes).
    pub model: Option<Rc<GLTFModel>>,
    /// Orbit camera for interactive viewing.
    pub camera: OrbitCamera,
    /// Statistics about the loaded model.
    pub stats: Option<ModelStats>,
    /// Animation playback state.
    pub animation: AnimationPlaybackState,
    /// Whether the preview panel is visible.
    pub visible: bool,
    /// Texture ID for the preview render target.
    /// Uses TextureId::custom(101) = 101 + 100 = 201 range
    pub texture_id: TextureId,
    /// Current loading state.
    pub load_state: LoadState,
    /// Load ID for tracking background loader requests.
    pub load_id: Option<LoadId>,
    /// Panel width in pixels (when visible).
    pub panel_width: f32,
}

impl ModelPreviewState {
    /// Texture ID base for model preview (101).
    pub const TEXTURE_ID: u64 = 101;

    /// Create a new model preview state.
    pub fn new() -> Self {
        Self {
            model_path: None,
            model: None,
            camera: OrbitCamera::new(),
            stats: None,
            animation: AnimationPlaybackState::new(),
            visible: false,
            texture_id: TextureId::custom(Self::TEXTURE_ID),
            load_state: LoadState::Idle,
            load_id: None,
            panel_width: 300.0,
        }
    }

    /// Request to preview a model.
    pub fn request_preview(&mut self, path: PathBuf) -> LoadId {
        // Reset state for new model
        self.model = None;
        self.stats = None;
        self.model_path = Some(path.clone());
        self.load_state = LoadState::Loading;
        self.visible = true;
        self.camera.reset();
        self.animation = AnimationPlaybackState::new();

        // Return a placeholder - actual LoadId comes from BackgroundLoader
        LoadId(0)
    }

    /// Called when model loading completes successfully.
    pub fn on_model_loaded(&mut self, model: Rc<GLTFModel>) {
        // Compute stats
        self.stats = Some(ModelStats::from_model(&model));

        // Fit camera to model bounds
        self.camera.fit_to_bounds(
            model.bounds.center,
            if model.bounds.radius > 0.0 {
                model.bounds.radius
            } else {
                1.0
            },
        );

        self.model = Some(model);
        self.load_state = LoadState::Loaded;
        self.load_id = None;
    }

    /// Called when model loading fails.
    pub fn on_model_failed(&mut self, error: String) {
        self.load_state = LoadState::Failed(error);
        self.load_id = None;
    }

    /// Close the preview panel.
    pub fn close(&mut self) {
        self.visible = false;
        self.model = None;
        self.model_path = None;
        self.stats = None;
        self.load_state = LoadState::Idle;
        self.load_id = None;
    }

    /// Check if currently loading a model.
    pub fn is_loading(&self) -> bool {
        matches!(self.load_state, LoadState::Loading)
    }

    /// Check if a model is loaded and ready to render.
    pub fn is_ready(&self) -> bool {
        matches!(self.load_state, LoadState::Loaded) && self.model.is_some()
    }

    /// Update animation playback.
    pub fn update_animation(&mut self, delta_time: f32) {
        if self.animation.playing && self.stats.as_ref().map(|s| s.has_animations).unwrap_or(false) {
            self.animation.time += delta_time * self.animation.speed;
        }
    }
}

impl Default for ModelPreviewState {
    fn default() -> Self {
        Self::new()
    }
}
