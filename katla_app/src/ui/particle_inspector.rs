//! Particle inspector widget for runtime particle emitter editing.

use katla_ecs::EntityId;
use katla_gfx::particles::ParticleStats;
use katla_math::{Rect2D, Vec2};
use katla_ui::{
    widgets::{Button, DraggablePanel, DraggablePanelState, DraggablePanelStyle},
    FontSize, Response, ScrollArea, ScrollAreaState, UiContext, Widget,
};

use crate::ui::Theme;

/// State for the particle inspector floating panel.
#[derive(Debug, Clone, Default)]
pub struct ParticleInspectorState {
    pub panel: DraggablePanelState,
    pub scroll_state: ScrollAreaState,
}

/// Pre-collected data for the particle inspector, gathered from World + GlobalParticleSystem.
#[derive(Debug, Clone, Default)]
pub struct ParticleInspectorData {
    pub emitter_entities: Vec<EntityId>,
    pub selected_emitter_config: Option<EmitterConfigView>,
    pub stats: Option<ParticleStats>,
}

/// Read-only view of emitter config for display in the inspector.
#[derive(Debug, Clone)]
pub struct EmitterConfigView {
    pub active: bool,
    pub shape_name: &'static str,
    pub shape_params: [f32; 3],
    pub emit_rate: f32,
    pub base_lifetime: f32,
    pub lifetime_variation: f32,
    pub velocity_magnitude: f32,
    pub velocity_cone_angle: f32,
    pub base_scale: f32,
    pub scale_variation: f32,
    pub color: [f32; 4],
    pub color_variation: f32,
    pub gravity: f32,
    pub turbulence_strength: f32,
    pub turbulence_frequency: f32,
}

/// Actions emitted by the particle inspector.
#[derive(Debug, Clone)]
pub enum ParticleInspectorAction {
    SelectEmitter(EntityId),
    ToggleEmitter,
    ResetSystem,
    Close,
}

/// Particle inspector panel for runtime emitter editing.
pub struct ParticleInspector<'a> {
    pub state: &'a mut ParticleInspectorState,
    pub selected_emitter: &'a mut Option<EntityId>,
    pub theme: &'a Theme,
    pub data: &'a ParticleInspectorData,
    pub pending_actions: &'a mut Vec<ParticleInspectorAction>,
}

impl<'a> ParticleInspector<'a> {
    pub fn new(
        state: &'a mut ParticleInspectorState,
        selected_emitter: &'a mut Option<EntityId>,
        theme: &'a Theme,
        data: &'a ParticleInspectorData,
        pending_actions: &'a mut Vec<ParticleInspectorAction>,
    ) -> Self {
        Self {
            state,
            selected_emitter,
            theme,
            data,
            pending_actions,
        }
    }
}

impl<'a> Widget for ParticleInspector<'a> {
    fn ui(self, ui: &mut UiContext) -> Response {
        let panel_width = 320.0;
        let panel_height = 600.0;
        let screen_size = ui.screen_size();
        let style = DraggablePanelStyle {
            panel_bg: self.theme.panel_bg,
            panel_border: self.theme.panel_border,
            panel_header: self.theme.panel_header,
            background_light: self.theme.background_light,
            text_primary: self.theme.text_primary,
            text_muted: self.theme.text_muted,
        };

        let frame = DraggablePanel::begin(
            ui,
            "particle_inspector",
            "Particle Inspector",
            panel_width,
            panel_height,
            screen_size,
            &mut self.state.panel,
            &style,
        );

        let title_bar_height = DraggablePanel::title_bar_height();
        let panel_min = frame.panel_bounds.min;
        let content_x = panel_min.x() + 8.0;
        let content_start_y = panel_min.y() + title_bar_height + 8.0;
        let content_width = frame.panel_bounds.width() - 16.0;
        let content_height = panel_height - title_bar_height - 8.0;

        let scroll_bounds = Rect2D::from_origin_size(
            Vec2::new(panel_min.x(), content_start_y - 8.0),
            Vec2::new(panel_width, content_height),
        );

        let theme = self.theme;
        let data = self.data;
        let selected_emitter = self.selected_emitter;
        let mut scroll_actions: Vec<ParticleInspectorAction> = Vec::new();

        self.state.scroll_state = ui.scroll_area(
            ScrollArea::new("particle_inspector_scroll").max_height(content_height),
            self.state.scroll_state,
            scroll_bounds,
            |ui| {
                let scroll_offset = ui.scroll_offset();
                let cursor_x = content_x;
                let mut cursor_y = content_start_y - scroll_offset;

                let line_height = 20.0;

                ui.draw_text(
                    "Emitter:",
                    Vec2::new(cursor_x, cursor_y),
                    theme.text_primary,
                    ui.scaled_font_size(FontSize::Small),
                );
                cursor_y += line_height;

                if data.emitter_entities.is_empty() {
                    ui.draw_text(
                        "No particle emitters in scene",
                        Vec2::new(cursor_x, cursor_y),
                        theme.text_muted,
                        ui.scaled_font_size(FontSize::Small),
                    );
                    cursor_y += line_height;
                } else {
                    for (idx, entity_id) in data.emitter_entities.iter().enumerate() {
                        let is_selected = selected_emitter == &Some(*entity_id);
                        let entity_name = format!("Emitter {}", idx);

                        let button_bounds = Rect2D::from_origin_size(
                            Vec2::new(cursor_x, cursor_y),
                            Vec2::new(content_width, 24.0),
                        );

                        let button_color = if is_selected {
                            theme.highlight
                        } else {
                            theme.button_bg
                        };

                        if ui
                            .add(
                                Button::new(&entity_name)
                                    .bounds(button_bounds)
                                    .fill_color(button_color),
                            )
                            .clicked
                        {
                            scroll_actions.push(ParticleInspectorAction::SelectEmitter(*entity_id));
                        }

                        cursor_y += 28.0;
                    }
                }

                cursor_y += line_height;
                ui.draw_line(
                    Vec2::new(cursor_x, cursor_y),
                    Vec2::new(cursor_x + content_width, cursor_y),
                    theme.separator,
                    1.0,
                );
                cursor_y += line_height;

                if let Some(ref config) = data.selected_emitter_config {
                    let layout = InspectorLayout {
                        cursor_x,
                        content_width,
                        theme,
                    };
                    cursor_y = render_emitter_config(
                        ui,
                        config,
                        &data.stats,
                        &layout,
                        cursor_y,
                        &mut scroll_actions,
                    );
                } else if selected_emitter.is_some() {
                    ui.draw_text(
                        "Selected emitter not found",
                        Vec2::new(cursor_x, cursor_y),
                        theme.text_muted,
                        ui.scaled_font_size(FontSize::Small),
                    );
                    cursor_y += line_height;
                }

                cursor_y - content_start_y + scroll_offset
            },
        );

        self.pending_actions.extend(scroll_actions);

        DraggablePanel::end(&mut self.state.panel, &frame);

        // Return actions to caller
        if frame.close_clicked || frame.clicked_outside {
            self.pending_actions.push(ParticleInspectorAction::Close);
        }

        Response::default()
    }
}

/// Layout context for drawing inside the particle inspector scroll area.
struct InspectorLayout<'a> {
    cursor_x: f32,
    content_width: f32,
    theme: &'a Theme,
}

fn render_emitter_config(
    ui: &mut UiContext,
    config: &EmitterConfigView,
    stats: &Option<ParticleStats>,
    layout: &InspectorLayout,
    mut y: f32,
    pending_actions: &mut Vec<ParticleInspectorAction>,
) -> f32 {
    let lh = 20.0;
    let btn_w = layout.content_width - 8.0;
    let cx = layout.cursor_x;
    let theme = layout.theme;

    macro_rules! row {
        ($label:expr, $value:expr) => {
            ui.set_cursor(Vec2::new(cx, y));
            ui.property_row($label, $value);
            y += lh;
        };
    }

    macro_rules! heading {
        ($text:expr) => {
            ui.set_cursor(Vec2::new(cx, y));
            ui.draw_text(
                $text,
                ui.cursor(),
                theme.text_accent,
                ui.scaled_font_size(FontSize::Small),
            );
            y += lh;
        };
    }

    heading!("Emitter Shape");
    row!("Shape:", config.shape_name);

    match config.shape_name {
        "Point" => {
            row!("Parameters:", "None (point emission)");
        }
        "Line" => {
            row!("Length:", &format!("{:.2}", config.shape_params[0]));
            row!("Axis:", "Y (vertical)");
        }
        "Circle" => {
            row!("Radius:", &format!("{:.2}", config.shape_params[0]));
            row!("Plane:", "XZ (horizontal)");
        }
        "Sphere" => {
            row!("Radius:", &format!("{:.2}", config.shape_params[0]));
        }
        "Box" => {
            row!("Width:", &format!("{:.2}", config.shape_params[0]));
            row!("Height:", &format!("{:.2}", config.shape_params[1]));
            row!("Depth:", &format!("{:.2}", config.shape_params[2]));
        }
        _ => {}
    }

    heading!("Emission");
    row!("Emit Rate:", &format!("{:.1}/s", config.emit_rate));
    row!("Base Lifetime:", &format!("{:.2}s", config.base_lifetime));
    row!(
        "Lifetime Var:",
        &format!("{:.2}", config.lifetime_variation)
    );

    heading!("Velocity");
    row!(
        "Magnitude:",
        &format!("{:.2} m/s", config.velocity_magnitude)
    );
    row!(
        "Cone Angle:",
        &format!("{:.2} rad", config.velocity_cone_angle)
    );

    heading!("Scale");
    row!("Base Scale:", &format!("{:.2}", config.base_scale));
    row!("Scale Var:", &format!("{:.2}", config.scale_variation));

    heading!("Color");
    {
        ui.set_cursor(Vec2::new(cx, y));
        ui.property_row(
            "Color:",
            &format!(
                "R:{:.2} G:{:.2} B:{:.2} A:{:.2}",
                config.color[0], config.color[1], config.color[2], config.color[3]
            ),
        );
        y += lh;
    }
    row!("Color Var:", &format!("{:.2}", config.color_variation));

    heading!("Forces");
    row!("Gravity:", &format!("{:.1} m/s^2", config.gravity));
    row!(
        "Turbulence:",
        &format!(
            "str={:.1} freq={:.1}",
            config.turbulence_strength, config.turbulence_frequency
        )
    );

    if let Some(ref stats) = stats {
        heading!("Statistics");
        row!(
            "Alive Particles:",
            &format!("{} / {}", stats.current_alive_count, stats.max_alive_count)
        );
        row!("Dead Particles:", &format!("{}", stats.dead_count));
        row!(
            "Buffer Utilization:",
            &format!("{:.1}%", stats.buffer_utilization * 100.0)
        );
        row!("Memory Used:", &format!("{:.2} MB", stats.memory_used_mb));

        heading!("Performance");
        row!("Compute Time:", &format!("{:.3} ms", stats.compute_time_ms));
        row!(
            "Avg Compute:",
            &format!("{:.3} ms", stats.avg_compute_time_ms)
        );
        row!(
            "Peak Compute:",
            &format!("{:.3} ms", stats.peak_compute_time_ms)
        );

        heading!("Lifetime");
        row!("Total Emitted:", &format!("{}", stats.total_emitted));
        row!("Total Died:", &format!("{}", stats.total_died));
        row!("Frame Count:", &format!("{}", stats.frame_count));
        row!("Total Dispatches:", &format!("{}", stats.total_dispatches));
    }

    heading!("Controls");

    let toggle_text = if config.active { "Disable" } else { "Enable" };
    let toggle_bounds = Rect2D::from_origin_size(Vec2::new(cx, y), Vec2::new(btn_w, 24.0));
    if ui
        .add(
            Button::new(toggle_text)
                .bounds(toggle_bounds)
                .fill_color(if config.active {
                    theme.button_bg
                } else {
                    theme.highlight
                }),
        )
        .clicked
    {
        pending_actions.push(ParticleInspectorAction::ToggleEmitter);
    }
    y += 28.0;

    let reset_bounds = Rect2D::from_origin_size(Vec2::new(cx, y), Vec2::new(btn_w, 24.0));
    if ui
        .add(Button::new("Reset System").bounds(reset_bounds))
        .clicked
    {
        pending_actions.push(ParticleInspectorAction::ResetSystem);
    }
    y += 28.0;

    y
}
