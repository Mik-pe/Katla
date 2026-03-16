//! Particle inspector widget for runtime particle emitter editing.
//!
//! This module provides a UI panel for inspecting and tweaking particle emitter
//! parameters at runtime without recompilation.

use katla_ecs::{EntityId, World};
use katla_gfx::particles::GlobalParticleSystem;
use katla_math::{Rect2D, Vec2};
use katla_ui::{widgets::Button, FontSize, Response, UiContext, Widget};

use crate::components::ParticleEmitterComponent;

/// Particle inspector panel for runtime emitter editing.
pub struct ParticleInspector<'a> {
    /// Panel bounds
    pub bounds: Rect2D,
    /// Currently selected emitter entity
    pub selected_emitter: &'a mut Option<EntityId>,
    /// Theme colors
    pub theme: &'a super::Theme,
}

impl<'a> ParticleInspector<'a> {
    /// Create a new particle inspector panel.
    pub fn new(
        bounds: Rect2D,
        selected_emitter: &'a mut Option<EntityId>,
        theme: &'a super::Theme,
    ) -> Self {
        Self {
            bounds,
            selected_emitter,
            theme,
        }
    }

    /// Set the selected emitter entity.
    pub fn set_selected_emitter(&mut self, entity: EntityId) {
        *self.selected_emitter = Some(entity);
    }

    /// Get the selected emitter entity.
    pub fn get_selected_emitter(&self) -> Option<EntityId> {
        *self.selected_emitter
    }

    /// Clear the selected emitter.
    pub fn clear_selection(&mut self) {
        *self.selected_emitter = None;
    }

    /// Render the inspector panel.
    pub fn render(
        &mut self,
        ui: &mut UiContext,
        world: &mut World,
        particle_system: &mut GlobalParticleSystem,
    ) {
        // Draw panel background
        ui.draw_rect(self.bounds, self.theme.panel_bg);
        ui.draw_rect_border(
            self.bounds,
            self.theme.panel_bg,
            self.theme.panel_border,
            1.0,
        );

        let header_height = 24.0;
        let header_bounds = Rect2D::from_origin_size(
            self.bounds.min,
            Vec2::new(self.bounds.width(), header_height),
        );
        ui.draw_rect(header_bounds, self.theme.panel_header);

        let header_pos = Vec2::new(self.bounds.min.x() + 8.0, header_bounds.center().y() - 7.0);
        ui.draw_text(
            "Particle Inspector",
            header_pos,
            self.theme.text_primary,
            ui.scaled_font_size(FontSize::Medium),
        );

        // Collect all entities with particle emitters
        let mut emitter_entities: Vec<EntityId> = Vec::new();
        for (entity_id, _emitter) in world.query::<&ParticleEmitterComponent>() {
            emitter_entities.push(entity_id);
        }

        // Begin column layout for content
        ui.begin_column();
        ui.set_cursor(Vec2::new(
            self.bounds.min.x() + 8.0,
            self.bounds.min.y() + header_height + 8.0,
        ));

        let line_height = 20.0;
        let content_width = self.bounds.width() - 16.0;

        // Emitter selector
        ui.draw_text(
            "Emitter:",
            ui.cursor(),
            self.theme.text_primary,
            ui.scaled_font_size(FontSize::Small),
        );
        ui.spacing(line_height);

        if emitter_entities.is_empty() {
            ui.draw_text(
                "No particle emitters in scene",
                ui.cursor(),
                self.theme.text_muted,
                ui.scaled_font_size(FontSize::Small),
            );
        } else {
            // List all emitters
            for (idx, entity_id) in emitter_entities.iter().enumerate() {
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
                    *self.selected_emitter = Some(*entity_id);
                }

                ui.spacing(4.0);
            }
        }

        ui.spacing(line_height);

        // Separator
        ui.separator_line();
        ui.spacing(line_height);

        // Show selected emitter details
        if let Some(selected_id) = *self.selected_emitter {
            if let Some(emitter) = world.get_component_mut::<ParticleEmitterComponent>(selected_id)
            {
                self.render_emitter_config(ui, emitter, particle_system);
            } else {
                ui.draw_text(
                    "Selected emitter not found",
                    ui.cursor(),
                    self.theme.text_muted,
                    ui.scaled_font_size(FontSize::Small),
                );
                *self.selected_emitter = None;
            }
        }

        ui.end_column();
    }

    /// Render emitter configuration controls.
    fn render_emitter_config(
        &mut self,
        ui: &mut UiContext,
        emitter: &mut ParticleEmitterComponent,
        particle_system: &mut GlobalParticleSystem,
    ) {
        let line_height = 20.0;
        let slider_width = self.bounds.width() - 24.0;

        // Section: Emission
        ui.draw_text(
            "Emission",
            ui.cursor(),
            self.theme.text_accent,
            ui.scaled_font_size(FontSize::Small),
        );
        ui.spacing(line_height);

        // Display current values
        ui.property_row("Emit Rate:", &format!("{:.1}/s", emitter.config.emit_rate));
        ui.property_row(
            "Base Lifetime:",
            &format!("{:.2}s", emitter.config.base_lifetime),
        );
        ui.property_row(
            "Lifetime Var:",
            &format!("{:.2}", emitter.config.lifetime_variation),
        );
        ui.spacing(line_height);

        // Section: Velocity
        ui.draw_text(
            "Velocity",
            ui.cursor(),
            self.theme.text_accent,
            ui.scaled_font_size(FontSize::Small),
        );
        ui.spacing(line_height);

        ui.property_row(
            "Magnitude:",
            &format!("{:.2} m/s", emitter.config.velocity_magnitude),
        );
        ui.property_row(
            "Cone Angle:",
            &format!("{:.2} rad", emitter.config.velocity_cone_angle),
        );
        ui.spacing(line_height);

        // Section: Scale
        ui.draw_text(
            "Scale",
            ui.cursor(),
            self.theme.text_accent,
            ui.scaled_font_size(FontSize::Small),
        );
        ui.spacing(line_height);

        ui.property_row("Base Scale:", &format!("{:.2}", emitter.config.base_scale));
        ui.property_row(
            "Scale Var:",
            &format!("{:.2}", emitter.config.scale_variation),
        );
        ui.spacing(line_height);

        // Section: Color
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
                emitter.config.color[0],
                emitter.config.color[1],
                emitter.config.color[2],
                emitter.config.color[3]
            ),
        );
        ui.property_row(
            "Color Var:",
            &format!("{:.2}", emitter.config.color_variation),
        );
        ui.spacing(line_height);

        // Section: Stats
        ui.draw_text(
            "Statistics",
            ui.cursor(),
            self.theme.text_accent,
            ui.scaled_font_size(FontSize::Small),
        );
        ui.spacing(line_height);

        // Get comprehensive statistics
        let stats = particle_system.get_stats();

        // Particle counts
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

        ui.property_row(
            "Memory Used:",
            &format!("{:.2} MB", stats.memory_used_mb),
        );
        ui.spacing(line_height);

        // Performance stats
        ui.draw_text(
            "Performance",
            ui.cursor(),
            self.theme.text_accent,
            ui.scaled_font_size(FontSize::Small),
        );
        ui.spacing(line_height);

        ui.property_row(
            "Compute Time:",
            &format!("{:.3} ms", stats.compute_time_ms),
        );
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

        // Lifetime stats
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

        ui.property_row(
            "Total Dispatches:",
            &format!("{}", stats.total_dispatches),
        );
        ui.spacing(line_height);

        // Section: Controls
        ui.draw_text(
            "Controls",
            ui.cursor(),
            self.theme.text_accent,
            ui.scaled_font_size(FontSize::Small),
        );
        ui.spacing(line_height);

        // Toggle button
        let toggle_text = if emitter.active { "Disable" } else { "Enable" };
        let toggle_bounds = Rect2D::from_origin_size(ui.cursor(), Vec2::new(slider_width, 24.0));

        if ui
            .add(
                Button::new(toggle_text)
                    .bounds(toggle_bounds)
                    .fill_color(if emitter.active {
                        self.theme.button_bg
                    } else {
                        self.theme.highlight
                    }),
            )
            .clicked
        {
            emitter.active = !emitter.active;
        }
        ui.spacing(4.0);

        // Reset button
        let reset_bounds = Rect2D::from_origin_size(ui.cursor(), Vec2::new(slider_width, 24.0));
        if ui
            .add(Button::new("Reset System").bounds(reset_bounds))
            .clicked
        {
            // Reset particle system by reinitializing
            log::info!("Resetting particle system from inspector");
        }

        // Apply any pending updates to the particle system
        if let Some(handle) = emitter.emitter_handle {
            particle_system.update_emitter(handle, emitter.config);
        }
    }
}

impl<'a> Widget for ParticleInspector<'a> {
    fn ui(self, _ui: &mut UiContext) -> Response {
        // Note: The actual rendering is done via render() method
        // This is a placeholder for Widget trait compliance
        Response::default()
    }
}
