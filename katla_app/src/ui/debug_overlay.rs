//! Debug overlay UI for displaying stats and controls.

use katla_math::Vec2;
use katla_math::Rect2D;
use katla_ui::{DrawList, UiContext};

/// Debug overlay for displaying engine stats and controls.
pub struct DebugOverlay {
    /// Whether the overlay is visible.
    visible: bool,
}

impl DebugOverlay {
    /// Create a new debug overlay.
    pub fn new() -> Self {
        Self { visible: true }
    }

    /// Toggle overlay visibility.
    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }

    /// Check if overlay is visible.
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Build the debug overlay UI.
    ///
    /// Call this after `ui.begin()` and before `ui.end()`.
    pub fn build(
        ui: &mut UiContext,
        fps: f32,
        frame_count: usize,
        entity_count: usize,
    ) {
        // Debug window with stats
        // Height calculation: padding(10) + title(25) + 9 lines × 20px + padding(10) = 225
        let window_bounds = Rect2D::from_origin_size(Vec2::new(10.0, 10.0), Vec2::new(280.0, 230.0));
        let window = ui.begin_window("debug_window", window_bounds);

        // Calculate text area
        let padding = 10.0;
        let line_height = 20.0;
        let mut cursor = Vec2::new(
            window.bounds.min.x() + padding,
            window.bounds.min.y() + padding + 25.0, // Account for title bar
        );

        // Stats labels
        let stats = [
            format!("FPS: {:.1}", fps),
            format!("Frame: {}", frame_count),
            format!("Entities: {}", entity_count),
            String::new(),
            "Controls:".to_string(),
            "  WASD - Move camera".to_string(),
            "  Mouse - Look around".to_string(),
            "  T - Add test meshes".to_string(),
            "  ESC - Exit".to_string(),
        ];

        for text in &stats {
            let label_bounds = Rect2D::from_origin_size(cursor, Vec2::new(260.0, line_height));
            ui.label(text, label_bounds);
            cursor = Vec2::new(cursor.x(), cursor.y() + line_height);
        }

        ui.end_window();
    }

    /// Render the debug overlay and return the draw list.
    ///
    /// This handles begin/end internally.
    pub fn render(
        ui: &mut UiContext,
        screen_size: Vec2,
        fps: f32,
        frame_count: usize,
        entity_count: usize,
    ) -> &DrawList {
        // Begin UI frame
        ui.begin(screen_size);

        // Build the UI
        Self::build(ui, fps, frame_count, entity_count);

        // End UI frame and return draw list
        ui.end()
    }
}

impl Default for DebugOverlay {
    fn default() -> Self {
        Self::new()
    }
}
