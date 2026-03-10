//! Debug overlay UI for displaying stats and controls.

use crate::util::MetricsHistory;
use katla_math::{Color, Rect2D, Vec2};
use katla_ui::{
    widgets::{Button, Checkbox, Label, Slider},
    DrawList, GraphOptions, UiContext,
};

/// Render mode options.
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

/// Quality level options.
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
        &[
            QualityLevel::Low,
            QualityLevel::Medium,
            QualityLevel::High,
            QualityLevel::Ultra,
        ]
    }
}

/// Debug overlay for displaying engine stats and controls.
pub struct DebugOverlay {
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
    /// FPS history for the graph.
    fps_history: MetricsHistory,
    /// Frame time history (in milliseconds) for the graph.
    frame_time_history: MetricsHistory,
    /// Context menu open state.
    context_menu_open: bool,
}

impl DebugOverlay {
    /// Create a new debug overlay.
    pub fn new() -> Self {
        Self {
            settings_visible: true,
            render_mode: RenderMode::Solid,
            quality: QualityLevel::High,
            volume: 0.8,
            sensitivity: 1.0,
            vsync: true,
            show_fps: true,
            ambient_intensity: 0.3,
            context_message: String::new(),
            fps_history: MetricsHistory::new(100),
            frame_time_history: MetricsHistory::new(100),
            context_menu_open: false,
        }
    }

    /// Build the debug overlay UI.
    pub fn build(
        &mut self,
        ui: &mut UiContext,
        fps: f32,
        fps_history: &[f32],
        frame_time_history: &[f32],
        frame_count: usize,
        entity_count: usize,
    ) {
        let padding = 10.0;
        let line_height = 22.0;
        let title_height = 25.0;
        let button_height = ui.style.button_height_small;
        let graph_height = 50.0;
        let graph_spacing = 5.0;
        let window_width = 300.0;

        // === Stats Window ===
        let stats = [
            format!("FPS: {:.1}", fps),
            format!("Frame: {}", frame_count),
            format!("Entities: {}", entity_count),
        ];

        // Calculate content height including graphs
        let stats_height = stats.len() as f32 * line_height;
        let graphs_height = (graph_height + graph_spacing) * 2.0;
        let content_height =
            stats_height + padding * 2.0 + button_height + padding + graphs_height + graph_spacing;
        let window_height = title_height + content_height;

        let window_bounds = Rect2D::from_origin_size(
            Vec2::new(10.0, 10.0),
            Vec2::new(window_width, window_height),
        );

        let window = ui.begin_window_with_title("debug_window", Some("Debug Stats"), window_bounds);

        // Use vertical layout for stats window
        ui.begin_column();
        ui.set_cursor(Vec2::new(
            window.bounds.min.x() + padding,
            window.bounds.min.y() + title_height + padding,
        ));

        // Stats section
        for text in &stats {
            ui.add(Label::new(text).at_cursor_width(ui, window_width - padding * 2.0));
            ui.spacing(line_height);
        }

        // FPS Graph
        ui.spacing(graph_spacing);
        let fps_graph_bounds = Rect2D::from_origin_size(
            ui.cursor(),
            Vec2::new(window_width - padding * 2.0, graph_height),
        );
        ui.graph(
            "fps_graph",
            Some("FPS"),
            fps_history,
            fps_graph_bounds,
            Some(GraphOptions::fps()),
        );

        // Frame Time Graph
        ui.spacing(graph_height + graph_spacing);
        let frame_time_bounds = Rect2D::from_origin_size(
            ui.cursor(),
            Vec2::new(window_width - padding * 2.0, graph_height),
        );
        ui.graph(
            "frame_time_graph",
            Some("Frame Time (ms)"),
            frame_time_history,
            frame_time_bounds,
            Some(GraphOptions::frame_time()),
        );

        // Settings toggle button
        ui.spacing(graph_height + padding);
        let btn_text = if self.settings_visible {
            "[Close Settings]"
        } else {
            "[Settings]"
        };
        let button_bounds = Rect2D::from_origin_size(
            ui.cursor(),
            Vec2::new(window_width - padding * 2.0, button_height),
        );
        if ui.add(Button::new(btn_text).id("settings_btn")).clicked {
            self.settings_visible = !self.settings_visible;
        }

        ui.end_column();
        ui.end_window();

        // === Settings Panel ===
        if self.settings_visible {
            self.build_settings_panel(ui, padding, line_height, title_height, button_height);
        }

        // === Context Menu (right-click anywhere in debug area) ===
        // Open on right-click if no popup already open
        if ui.input.mouse_clicked(katla_ui::input::mouse_button::RIGHT) && !ui.has_open_popup() {
            self.context_menu_open = true;
        }

        ui.context_menu("main_context", &mut self.context_menu_open, |ui, open| {
            if ui.menu_item_clicked("Solid Mode") {
                self.render_mode = RenderMode::Solid;
                self.context_message = "Switched to Solid!".to_string();
                *open = false;
            }
            if ui.menu_item_clicked("Wireframe") {
                self.render_mode = RenderMode::Wireframe;
                self.context_message = "Switched to Wireframe!".to_string();
                *open = false;
            }
            if ui.menu_item_clicked("Points") {
                self.render_mode = RenderMode::Points;
                self.context_message = "Switched to Points!".to_string();
                *open = false;
            }
        });

        // === Toast Message ===
        if !self.context_message.is_empty() {
            let msg_size = ui.measure_text(&self.context_message, ui.style.font_size);
            let toast_width = msg_size.x() + 20.0;
            let toast_height = 30.0;
            let toast_bounds = Rect2D::from_origin_size(
                Vec2::new(ui.screen_size().x() - toast_width - 20.0, 20.0),
                Vec2::new(toast_width, toast_height),
            );
            ui.draw_rect(toast_bounds, Color::new(0.2, 0.6, 0.2, 0.9));
            ui.draw_rect_border(
                toast_bounds,
                Color::TRANSPARENT,
                Color::new(0.3, 0.8, 0.3, 1.0),
                1.0,
            );
            let text_pos = Vec2::new(
                toast_bounds.min.x() + 10.0,
                toast_bounds.center().y() - msg_size.y() * 0.5,
            );
            ui.draw_text(
                &self.context_message,
                text_pos,
                Color::WHITE,
                ui.style.font_size,
            );
        }
    }

    fn build_settings_panel(
        &mut self,
        ui: &mut UiContext,
        padding: f32,
        line_height: f32,
        title_height: f32,
        button_height: f32,
    ) {
        let panel_x = 320.0;
        let panel_width = 320.0;

        // Panel height - make it taller to fit all content
        let panel_height = 480.0;

        let panel_bounds = Rect2D::from_origin_size(
            Vec2::new(panel_x, 10.0),
            Vec2::new(panel_width, panel_height),
        );

        let window = ui.begin_window_with_title("settings_panel", Some("Settings"), panel_bounds);

        // Use begin_column() for vertical layout
        ui.begin_column();
        ui.set_cursor(Vec2::new(
            window.bounds.min.x() + padding,
            window.bounds.min.y() + title_height + padding,
        ));

        // === Render Mode (Selectable buttons) ===
        ui.add(Label::new("Render Mode:").at_cursor_width(ui, panel_width - padding * 2.0));
        ui.spacing(line_height + 4.0);

        // Three buttons in a row for render modes
        ui.begin_row();
        let btn_width = (panel_width - padding * 2.0 - 8.0) / 3.0;
        for (i, mode) in [RenderMode::Solid, RenderMode::Wireframe, RenderMode::Points]
            .iter()
            .enumerate()
        {
            let btn_bounds =
                Rect2D::from_origin_size(ui.cursor(), Vec2::new(btn_width, button_height));
            let is_selected = *mode == self.render_mode;
            if ui
                .selectable(
                    &format!("render_{}", i),
                    mode.as_str(),
                    is_selected,
                    btn_bounds,
                )
                .clicked
            {
                self.render_mode = *mode;
            }
            ui.spacing(btn_width + 4.0);
        }
        ui.end_row();
        ui.spacing(button_height + 12.0);

        // === Quality (Selectable buttons) ===
        ui.add(Label::new("Quality:").at_cursor_width(ui, panel_width - padding * 2.0));
        ui.spacing(line_height + 4.0);

        ui.begin_row();
        let btn_width = (panel_width - padding * 2.0 - 12.0) / 4.0;
        for (i, quality) in QualityLevel::all().iter().enumerate() {
            let btn_bounds =
                Rect2D::from_origin_size(ui.cursor(), Vec2::new(btn_width, button_height));
            let is_selected = *quality == self.quality;
            if ui
                .selectable(
                    &format!("qual_{}", i),
                    quality.as_str(),
                    is_selected,
                    btn_bounds,
                )
                .clicked
            {
                self.quality = *quality;
            }
            ui.spacing(btn_width + 4.0);
        }
        ui.end_row();
        ui.spacing(button_height + 12.0);

        // === Volume Slider ===
        ui.add(
            Label::new(&format!("Volume: {:.0}%", self.volume * 100.0))
                .at_cursor_width(ui, panel_width - padding * 2.0),
        );
        ui.spacing(line_height + 2.0);

        let slider_bounds = Rect2D::from_origin_size(
            ui.cursor(),
            Vec2::new(panel_width - padding * 2.0, ui.style.checkbox_size),
        );
        ui.add(
            Slider::new(&mut self.volume, 0.0..=1.0)
                .bounds(slider_bounds)
                .id("volume_slider"),
        );
        ui.spacing(28.0);

        // === Sensitivity Slider ===
        ui.add(
            Label::new(&format!("Sensitivity: {:.1}", self.sensitivity))
                .at_cursor_width(ui, panel_width - padding * 2.0),
        );
        ui.spacing(line_height + 2.0);

        let slider_bounds = Rect2D::from_origin_size(
            ui.cursor(),
            Vec2::new(panel_width - padding * 2.0, ui.style.checkbox_size),
        );
        ui.add(
            Slider::new(&mut self.sensitivity, 0.1..=3.0)
                .bounds(slider_bounds)
                .id("sens_slider"),
        );
        ui.spacing(28.0);

        // === Ambient Slider ===
        ui.add(
            Label::new(&format!("Ambient: {:.2}", self.ambient_intensity))
                .at_cursor_width(ui, panel_width - padding * 2.0),
        );
        ui.spacing(line_height + 2.0);

        let slider_bounds = Rect2D::from_origin_size(
            ui.cursor(),
            Vec2::new(panel_width - padding * 2.0, ui.style.checkbox_size),
        );
        ui.add(
            Slider::new(&mut self.ambient_intensity, 0.0..=1.0)
                .bounds(slider_bounds)
                .id("ambient_slider"),
        );
        ui.spacing(28.0);

        // === Checkboxes ===
        let checkbox_bounds = Rect2D::from_origin_size(
            ui.cursor(),
            Vec2::new(panel_width - padding * 2.0, line_height),
        );
        ui.add(
            Checkbox::new(&mut self.vsync, "VSync Enabled")
                .bounds(checkbox_bounds)
                .id("vsync_check"),
        );
        ui.spacing(line_height + 4.0);

        let checkbox_bounds = Rect2D::from_origin_size(
            ui.cursor(),
            Vec2::new(panel_width - padding * 2.0, line_height),
        );
        ui.add(
            Checkbox::new(&mut self.show_fps, "Show FPS in Title")
                .bounds(checkbox_bounds)
                .id("fps_check"),
        );
        ui.spacing(12.0);

        // === Close Button ===
        let button_bounds = Rect2D::from_origin_size(
            ui.cursor(),
            Vec2::new(panel_width - padding * 2.0, button_height),
        );
        if ui
            .add(
                Button::new("Close Panel")
                    .bounds(button_bounds)
                    .id("close_settings"),
            )
            .clicked
        {
            self.settings_visible = false;
        }

        ui.end_column();
        ui.end_window();
    }

    /// Render the debug overlay and return the draw list.
    ///
    /// This handles begin/end internally and updates the metrics history.
    pub fn render<'a>(
        &mut self,
        ui: &'a mut UiContext,
        screen_size: Vec2,
        scale_factor: f32,
        fps: f32,
        frame_count: usize,
        entity_count: usize,
    ) -> &'a DrawList {
        // Calculate frame time in milliseconds
        let frame_time_ms = if fps > 0.0 { 1000.0 / fps } else { 0.0 };

        // Update history
        self.fps_history.push(fps);
        self.frame_time_history.push(frame_time_ms);

        // Begin UI frame
        ui.begin(screen_size, scale_factor);

        // Build the UI
        self.build(
            ui,
            fps,
            &self.fps_history.values_vec(),
            &self.frame_time_history.values_vec(),
            frame_count,
            entity_count,
        );

        // End UI frame and return draw list
        ui.end()
    }
}

impl Default for DebugOverlay {
    fn default() -> Self {
        Self::new()
    }
}
