//! Particle inspector widget for runtime particle emitter editing.

use katla_ecs::EntityId;
use katla_gfx::particles::ParticleStats;
use katla_math::{Rect2D, Vec2};
use katla_ui::{
    widgets::{Button, DraggablePanel, DraggablePanelState, DraggablePanelStyle},
    FontSize, Response, UiContext, Widget,
};

use crate::ui::Theme;

/// State for the particle inspector floating panel.
#[derive(Debug, Clone, Default)]
pub struct ParticleInspectorState {
    pub panel: DraggablePanelState,
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
    fn ui(mut self, ui: &mut UiContext) -> Response {
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

        ui.begin_column();
        ui.set_cursor(Vec2::new(content_x, content_start_y));

        let line_height = 20.0;

        ui.draw_text(
            "Emitter:",
            ui.cursor(),
            self.theme.text_primary,
            ui.scaled_font_size(FontSize::Small),
        );
        ui.spacing(line_height);

        if self.data.emitter_entities.is_empty() {
            ui.draw_text(
                "No particle emitters in scene",
                ui.cursor(),
                self.theme.text_muted,
                ui.scaled_font_size(FontSize::Small),
            );
        } else {
            for (idx, entity_id) in self.data.emitter_entities.iter().enumerate() {
                let is_selected = self.selected_emitter == &Some(*entity_id);
                let entity_name = format!("Emitter {}", idx);

                let button_bounds =
                    Rect2D::from_origin_size(ui.cursor(), Vec2::new(content_width, 24.0));

                let button_color = if is_selected {
                    self.theme.highlight
                } else {
                    self.theme.button_bg
                };

                if ui
                    .add(
                        Button::new(&entity_name)
                            .bounds(button_bounds)
                            .fill_color(button_color),
                    )
                    .clicked
                {
                    self.pending_actions
                        .push(ParticleInspectorAction::SelectEmitter(*entity_id));
                }

                ui.spacing(4.0);
            }
        }

        ui.spacing(line_height);
        ui.separator_line();
        ui.spacing(line_height);

        if let Some(ref config) = self.data.selected_emitter_config {
            self.render_emitter_config(ui, config, content_width);
        } else if self.selected_emitter.is_some() {
            ui.draw_text(
                "Selected emitter not found",
                ui.cursor(),
                self.theme.text_muted,
                ui.scaled_font_size(FontSize::Small),
            );
        }

        ui.end_column();

        DraggablePanel::end(&mut self.state.panel, &frame);

        // Return actions to caller
        if frame.close_clicked || frame.clicked_outside {
            self.pending_actions.push(ParticleInspectorAction::Close);
        }

        Response::default()
    }
}

impl<'a> ParticleInspector<'a> {
    fn render_emitter_config(
        &mut self,
        ui: &mut UiContext,
        config: &EmitterConfigView,
        content_width: f32,
    ) {
        let line_height = 20.0;
        let slider_width = content_width - 8.0;

        ui.draw_text(
            "Emitter Shape",
            ui.cursor(),
            self.theme.text_accent,
            ui.scaled_font_size(FontSize::Small),
        );
        ui.spacing(line_height);

        ui.property_row("Shape:", config.shape_name);
        ui.spacing(line_height);

        match config.shape_name {
            "Point" => {
                ui.property_row("Parameters:", "None (point emission)");
            }
            "Line" => {
                ui.property_row("Length:", &format!("{:.2}", config.shape_params[0]));
                ui.property_row("Axis:", "Y (vertical)");
            }
            "Circle" => {
                ui.property_row("Radius:", &format!("{:.2}", config.shape_params[0]));
                ui.property_row("Plane:", "XZ (horizontal)");
            }
            "Sphere" => {
                ui.property_row("Radius:", &format!("{:.2}", config.shape_params[0]));
            }
            "Box" => {
                ui.property_row("Width:", &format!("{:.2}", config.shape_params[0]));
                ui.property_row("Height:", &format!("{:.2}", config.shape_params[1]));
                ui.property_row("Depth:", &format!("{:.2}", config.shape_params[2]));
            }
            _ => {}
        }
        ui.spacing(line_height);

        ui.draw_text(
            "Emission",
            ui.cursor(),
            self.theme.text_accent,
            ui.scaled_font_size(FontSize::Small),
        );
        ui.spacing(line_height);

        ui.property_row("Emit Rate:", &format!("{:.1}/s", config.emit_rate));
        ui.property_row("Base Lifetime:", &format!("{:.2}s", config.base_lifetime));
        ui.property_row(
            "Lifetime Var:",
            &format!("{:.2}", config.lifetime_variation),
        );
        ui.spacing(line_height);

        ui.draw_text(
            "Velocity",
            ui.cursor(),
            self.theme.text_accent,
            ui.scaled_font_size(FontSize::Small),
        );
        ui.spacing(line_height);

        ui.property_row(
            "Magnitude:",
            &format!("{:.2} m/s", config.velocity_magnitude),
        );
        ui.property_row(
            "Cone Angle:",
            &format!("{:.2} rad", config.velocity_cone_angle),
        );
        ui.spacing(line_height);

        ui.draw_text(
            "Scale",
            ui.cursor(),
            self.theme.text_accent,
            ui.scaled_font_size(FontSize::Small),
        );
        ui.spacing(line_height);

        ui.property_row("Base Scale:", &format!("{:.2}", config.base_scale));
        ui.property_row("Scale Var:", &format!("{:.2}", config.scale_variation));
        ui.spacing(line_height);

        ui.draw_text(
            "Color",
            ui.cursor(),
            self.theme.text_accent,
            ui.scaled_font_size(FontSize::Small),
        );
        ui.spacing(line_height);

        ui.property_row(
            "Color:",
            &format!(
                "R:{:.2} G:{:.2} B:{:.2} A:{:.2}",
                config.color[0], config.color[1], config.color[2], config.color[3]
            ),
        );
        ui.property_row("Color Var:", &format!("{:.2}", config.color_variation));
        ui.spacing(line_height);

        // Stats section
        if let Some(ref stats) = self.data.stats {
            ui.draw_text(
                "Statistics",
                ui.cursor(),
                self.theme.text_accent,
                ui.scaled_font_size(FontSize::Small),
            );
            ui.spacing(line_height);

            ui.property_row(
                "Alive Particles:",
                &format!("{} / {}", stats.current_alive_count, stats.max_alive_count),
            );
            ui.spacing(4.0);
            ui.property_row("Dead Particles:", &format!("{}", stats.dead_count));
            ui.spacing(4.0);
            ui.property_row(
                "Buffer Utilization:",
                &format!("{:.1}%", stats.buffer_utilization * 100.0),
            );
            ui.spacing(4.0);
            ui.property_row("Memory Used:", &format!("{:.2} MB", stats.memory_used_mb));
            ui.spacing(line_height);

            ui.draw_text(
                "Performance",
                ui.cursor(),
                self.theme.text_accent,
                ui.scaled_font_size(FontSize::Small),
            );
            ui.spacing(line_height);

            ui.property_row("Compute Time:", &format!("{:.3} ms", stats.compute_time_ms));
            ui.spacing(4.0);
            ui.property_row(
                "Avg Compute:",
                &format!("{:.3} ms", stats.avg_compute_time_ms),
            );
            ui.spacing(4.0);
            ui.property_row(
                "Peak Compute:",
                &format!("{:.3} ms", stats.peak_compute_time_ms),
            );
            ui.spacing(line_height);

            ui.draw_text(
                "Lifetime",
                ui.cursor(),
                self.theme.text_accent,
                ui.scaled_font_size(FontSize::Small),
            );
            ui.spacing(line_height);

            ui.property_row("Total Emitted:", &format!("{}", stats.total_emitted));
            ui.spacing(4.0);
            ui.property_row("Total Died:", &format!("{}", stats.total_died));
            ui.spacing(4.0);
            ui.property_row("Frame Count:", &format!("{}", stats.frame_count));
            ui.spacing(4.0);
            ui.property_row("Total Dispatches:", &format!("{}", stats.total_dispatches));
            ui.spacing(line_height);
        }

        // Controls section
        ui.draw_text(
            "Controls",
            ui.cursor(),
            self.theme.text_accent,
            ui.scaled_font_size(FontSize::Small),
        );
        ui.spacing(line_height);

        let toggle_text = if config.active { "Disable" } else { "Enable" };
        let toggle_bounds = Rect2D::from_origin_size(ui.cursor(), Vec2::new(slider_width, 24.0));
        if ui
            .add(
                Button::new(toggle_text)
                    .bounds(toggle_bounds)
                    .fill_color(if config.active {
                        self.theme.button_bg
                    } else {
                        self.theme.highlight
                    }),
            )
            .clicked
        {
            self.pending_actions
                .push(ParticleInspectorAction::ToggleEmitter);
        }
        ui.spacing(4.0);

        let reset_bounds = Rect2D::from_origin_size(ui.cursor(), Vec2::new(slider_width, 24.0));
        if ui
            .add(Button::new("Reset System").bounds(reset_bounds))
            .clicked
        {
            self.pending_actions
                .push(ParticleInspectorAction::ResetSystem);
        }
    }
}
