//! Debug overlay UI for displaying stats and controls.

use katla_math::{Color, Rect2D, Vec2};
use katla_ui::{DrawList, UiContext};

/// Render mode options for dropdown demo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderMode {
    Solid,
    Wireframe,
    Points,
}

impl RenderMode {
    fn as_str(&self) -> &'static str {
        match self {
            RenderMode::Solid => "Solid",
            RenderMode::Wireframe => "Wireframe",
            RenderMode::Points => "Points",
        }
    }
}

/// Quality level for combo demo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QualityLevel {
    Low,
    Medium,
    High,
    Ultra,
}

impl QualityLevel {
    fn as_str(&self) -> &'static str {
        match self {
            QualityLevel::Low => "Low",
            QualityLevel::Medium => "Medium",
            QualityLevel::High => "High",
            QualityLevel::Ultra => "Ultra",
        }
    }

    fn all() -> &'static [QualityLevel] {
        &[QualityLevel::Low, QualityLevel::Medium, QualityLevel::High, QualityLevel::Ultra]
    }
}

/// Debug overlay for displaying engine stats and controls.
pub struct DebugOverlay {
    /// Whether the overlay is visible.
    visible: bool,
    /// Settings panel visible.
    settings_visible: bool,
    /// Current render mode.
    pub render_mode: RenderMode,
    /// Current quality level.
    pub quality: QualityLevel,
    /// Master volume (slider demo).
    pub volume: f32,
    /// Mouse sensitivity (slider demo).
    pub sensitivity: f32,
    /// VSync enabled (checkbox demo).
    pub vsync: bool,
    /// Show FPS (checkbox demo).
    pub show_fps: bool,
    /// Ambient light intensity.
    pub ambient_intensity: f32,
    /// Right-click context menu message.
    context_message: String,
}

impl DebugOverlay {
    /// Create a new debug overlay.
    pub fn new() -> Self {
        Self {
            visible: true,
            settings_visible: false,
            render_mode: RenderMode::Solid,
            quality: QualityLevel::High,
            volume: 0.8,
            sensitivity: 1.0,
            vsync: true,
            show_fps: true,
            ambient_intensity: 0.3,
            context_message: String::new(),
        }
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
        &mut self,
        ui: &mut UiContext,
        fps: f32,
        frame_count: usize,
        entity_count: usize,
    ) {
        let padding = 10.0;
        let line_height = 22.0;
        let title_height = 25.0;
        let button_height = 28.0;
        let window_width = 300.0;

        // === Stats Window ===
        let stats = [
            format!("FPS: {:.1}", fps),
            format!("Frame: {}", frame_count),
            format!("Entities: {}", entity_count),
        ];

        let content_height = stats.len() as f32 * line_height + padding * 2.0 + button_height + padding;
        let window_height = title_height + content_height;

        let window_bounds = Rect2D::from_origin_size(
            Vec2::new(10.0, 10.0),
            Vec2::new(window_width, window_height),
        );

        let window = ui.begin_window_with_title("debug_window", Some("Debug Stats"), window_bounds);

        let mut cursor = Vec2::new(
            window.bounds.min.x() + padding,
            window.bounds.min.y() + title_height + padding,
        );

        // Stats
        for text in &stats {
            let label_bounds = Rect2D::from_origin_size(cursor, Vec2::new(window_width - padding * 2.0, line_height));
            ui.label(text, label_bounds);
            cursor = Vec2::new(cursor.x(), cursor.y() + line_height);
        }

        // Settings button
        cursor = Vec2::new(cursor.x(), cursor.y() + padding);
        let button_bounds = Rect2D::from_origin_size(cursor, Vec2::new(window_width - padding * 2.0, button_height));
        if ui.button("settings_btn", "⚙ Settings", button_bounds) {
            self.settings_visible = !self.settings_visible;
        }

        ui.end_window();

        // === Settings Panel ===
        if self.settings_visible {
            self.build_settings_panel(ui, window_width, padding, line_height, title_height, button_height);
        }

        // === Context Menu Demo ===
        // Open context menu on right-click anywhere
        ui.open_context_menu("main_context");

        if ui.begin_context_menu("main_context") {
            let item_height = ui.menu_item_height();
            let item_cursor = Vec2::new(0.0, 0.0);

            let item1_bounds = Rect2D::from_origin_size(item_cursor, Vec2::new(150.0, item_height));
            if ui.menu_item("ctx_solid", "Solid Mode", item1_bounds) {
                self.render_mode = RenderMode::Solid;
                self.context_message = "Switched to Solid mode!".to_string();
                ui.close_current_popup();
            }

            let item2_bounds = Rect2D::from_origin_size(
                Vec2::new(0.0, item_height),
                Vec2::new(150.0, item_height)
            );
            if ui.menu_item("ctx_wire", "Wireframe Mode", item2_bounds) {
                self.render_mode = RenderMode::Wireframe;
                self.context_message = "Switched to Wireframe mode!".to_string();
                ui.close_current_popup();
            }

            let item3_bounds = Rect2D::from_origin_size(
                Vec2::new(0.0, item_height * 2.0),
                Vec2::new(150.0, item_height)
            );
            if ui.menu_item("ctx_points", "Points Mode", item3_bounds) {
                self.render_mode = RenderMode::Points;
                self.context_message = "Switched to Points mode!".to_string();
                ui.close_current_popup();
            }

            ui.end_context_menu();
        }

        // === Context Message Toast ===
        if !self.context_message.is_empty() {
            let msg_size = ui.measure_text(&self.context_message, ui.style.font_size);
            let toast_width = msg_size.x() + 20.0;
            let toast_height = 30.0;
            let toast_bounds = Rect2D::from_origin_size(
                Vec2::new(ui.screen_size().x() - toast_width - 20.0, 20.0),
                Vec2::new(toast_width, toast_height),
            );
            ui.draw_rect(toast_bounds, Color::new(0.2, 0.6, 0.2, 0.9));
            ui.draw_rect_border(toast_bounds, Color::TRANSPARENT, Color::new(0.3, 0.8, 0.3, 1.0), 1.0);
            let text_pos = Vec2::new(
                toast_bounds.min.x() + 10.0,
                toast_bounds.center().y() - msg_size.y() * 0.5,
            );
            ui.draw_text(&self.context_message, text_pos, Color::WHITE, ui.style.font_size);
        }
    }

    fn build_settings_panel(
        &mut self,
        ui: &mut UiContext,
        _window_width: f32,
        padding: f32,
        line_height: f32,
        title_height: f32,
        button_height: f32,
    ) {
        let panel_x = 320.0;
        let panel_width = 320.0;

        // Calculate panel height based on content
        let items = 9; // render mode + quality + 2 sliders + 2 checkboxes + ambient + close button + padding
        let content_height = items as f32 * (line_height + 8.0) + padding * 2.0;
        let panel_height = title_height + content_height;

        let panel_bounds = Rect2D::from_origin_size(
            Vec2::new(panel_x, 10.0),
            Vec2::new(panel_width, panel_height),
        );

        let window = ui.begin_window_with_title("settings_panel", Some("⚙ Settings"), panel_bounds);

        let mut cursor = Vec2::new(
            window.bounds.min.x() + padding,
            window.bounds.min.y() + title_height + padding,
        );

        // Render Mode Dropdown
        {
            let label_bounds = Rect2D::from_origin_size(cursor, Vec2::new(panel_width - padding * 2.0, line_height));
            ui.label("Render Mode:", label_bounds);
            cursor = Vec2::new(cursor.x(), cursor.y() + line_height + 4.0);

            let dropdown_bounds = Rect2D::from_origin_size(cursor, Vec2::new(panel_width - padding * 2.0, button_height));
            if ui.begin_dropdown("render_mode", self.render_mode.as_str(), dropdown_bounds) {
                let item_height = ui.menu_item_height();
                for (i, mode) in [RenderMode::Solid, RenderMode::Wireframe, RenderMode::Points].iter().enumerate() {
                    let item_bounds = Rect2D::from_origin_size(
                        Vec2::new(cursor.x(), cursor.y() + button_height + i as f32 * item_height),
                        Vec2::new(panel_width - padding * 2.0, item_height),
                    );
                    if ui.menu_item(&format!("mode_{}", i), mode.as_str(), item_bounds) {
                        self.render_mode = *mode;
                        ui.close_current_popup();
                    }
                }
                ui.end_dropdown();
            }
            cursor = Vec2::new(cursor.x(), cursor.y() + button_height + 8.0);
        }

        // Quality Combo Box
        {
            let label_bounds = Rect2D::from_origin_size(cursor, Vec2::new(panel_width - padding * 2.0, line_height));
            ui.label("Quality:", label_bounds);
            cursor = Vec2::new(cursor.x(), cursor.y() + line_height + 4.0);

            let combo_bounds = Rect2D::from_origin_size(cursor, Vec2::new(panel_width - padding * 2.0, button_height));
            if ui.begin_combo("quality_combo", self.quality.as_str(), combo_bounds) {
                let item_height = ui.menu_item_height();
                for (i, quality) in QualityLevel::all().iter().enumerate() {
                    let item_bounds = Rect2D::from_origin_size(
                        Vec2::new(cursor.x(), cursor.y() + button_height + i as f32 * item_height),
                        Vec2::new(panel_width - padding * 2.0, item_height),
                    );
                    if ui.selectable(&format!("qual_{}", i), quality.as_str(), *quality == self.quality, item_bounds) {
                        self.quality = *quality;
                        ui.close_current_popup();
                    }
                }
                ui.end_combo();
            }
            cursor = Vec2::new(cursor.x(), cursor.y() + button_height + 8.0);
        }

        // Volume Slider
        {
            let label_bounds = Rect2D::from_origin_size(cursor, Vec2::new(panel_width - padding * 2.0, line_height));
            ui.label(&format!("Volume: {:.0}%", self.volume * 100.0), label_bounds);
            cursor = Vec2::new(cursor.x(), cursor.y() + line_height + 2.0);

            let slider_bounds = Rect2D::from_origin_size(cursor, Vec2::new(panel_width - padding * 2.0, 20.0));
            ui.slider("volume_slider", &mut self.volume, 0.0, 1.0, slider_bounds);
            cursor = Vec2::new(cursor.x(), cursor.y() + 28.0);
        }

        // Sensitivity Slider
        {
            let label_bounds = Rect2D::from_origin_size(cursor, Vec2::new(panel_width - padding * 2.0, line_height));
            ui.label(&format!("Sensitivity: {:.1}", self.sensitivity), label_bounds);
            cursor = Vec2::new(cursor.x(), cursor.y() + line_height + 2.0);

            let slider_bounds = Rect2D::from_origin_size(cursor, Vec2::new(panel_width - padding * 2.0, 20.0));
            ui.slider("sens_slider", &mut self.sensitivity, 0.1, 3.0, slider_bounds);
            cursor = Vec2::new(cursor.x(), cursor.y() + 28.0);
        }

        // Ambient Intensity Slider
        {
            let label_bounds = Rect2D::from_origin_size(cursor, Vec2::new(panel_width - padding * 2.0, line_height));
            ui.label(&format!("Ambient: {:.2}", self.ambient_intensity), label_bounds);
            cursor = Vec2::new(cursor.x(), cursor.y() + line_height + 2.0);

            let slider_bounds = Rect2D::from_origin_size(cursor, Vec2::new(panel_width - padding * 2.0, 20.0));
            ui.slider("ambient_slider", &mut self.ambient_intensity, 0.0, 1.0, slider_bounds);
            cursor = Vec2::new(cursor.x(), cursor.y() + 28.0);
        }

        // VSync Checkbox
        {
            let checkbox_bounds = Rect2D::from_origin_size(cursor, Vec2::new(panel_width - padding * 2.0, line_height));
            ui.checkbox("vsync_check", "VSync", &mut self.vsync, checkbox_bounds);
            cursor = Vec2::new(cursor.x(), cursor.y() + line_height + 4.0);
        }

        // Show FPS Checkbox
        {
            let checkbox_bounds = Rect2D::from_origin_size(cursor, Vec2::new(panel_width - padding * 2.0, line_height));
            ui.checkbox("fps_check", "Show FPS in Title", &mut self.show_fps, checkbox_bounds);
            cursor = Vec2::new(cursor.x(), cursor.y() + line_height + 8.0);
        }

        // Close Button
        {
            let button_bounds = Rect2D::from_origin_size(cursor, Vec2::new(panel_width - padding * 2.0, button_height));
            if ui.button("close_settings", "Close Settings", button_bounds) {
                self.settings_visible = false;
            }
        }

        ui.end_window();
    }

    /// Render the debug overlay and return the draw list.
    ///
    /// This handles begin/end internally.
    pub fn render<'a>(
        &mut self,
        ui: &'a mut UiContext,
        screen_size: Vec2,
        fps: f32,
        frame_count: usize,
        entity_count: usize,
    ) -> &'a DrawList {
        // Begin UI frame
        ui.begin(screen_size);

        // Build the UI
        self.build(ui, fps, frame_count, entity_count);

        // End UI frame and return draw list
        ui.end()
    }

    /// Get the current render mode.
    pub fn render_mode(&self) -> RenderMode {
        self.render_mode
    }

    /// Get whether to show FPS in title.
    pub fn show_fps_in_title(&self) -> bool {
        self.show_fps
    }

    /// Clear the context message.
    pub fn clear_context_message(&mut self) {
        self.context_message.clear();
    }
}

impl Default for DebugOverlay {
    fn default() -> Self {
        Self::new()
    }
}
