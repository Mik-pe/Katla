use katla_ecs::EntityId;
use katla_ui::FontSize;
use katla_ui::declarative::{
    Alignment, Build, BuildContext, DraggablePanelState, DraggablePanelVisibility, StateId,
    ViewDescriptor, draggable_panel, hstack, labeled_slider, property_row, radio, scroll, text,
    toggle, vstack,
};

use crate::ui::particle_inspector::EmitterField;
use crate::ui::{ColorScheme, ParticleInspectorAction, ParticleInspectorData, ParticleStats};

/// Environment data injected before each frame.
#[derive(Clone)]
pub(crate) struct ParticleInspectorDrawCtx {
    pub data: ParticleInspectorData,
    pub theme: ColorScheme,
    pub is_open: bool,
}

/// Emitted each frame to sync panel position/visibility back to the app.
#[derive(Clone, Debug)]
pub(crate) struct ParticleInspectorPanelSync {
    pub position: Option<katla_math::Vec2>,
    pub visibility: DraggablePanelVisibility,
}

pub(crate) struct ParticleInspectorView;

impl Build for ParticleInspectorView {
    fn build(&self, ctx: &mut BuildContext) -> ViewDescriptor {
        let draw_ctx = ctx.env::<ParticleInspectorDrawCtx>().cloned();
        let Some(draw_ctx) = draw_ctx else {
            return ViewDescriptor::Empty;
        };

        let panel_id: StateId = ctx.state(DraggablePanelState::default());
        let scroll_id: StateId = ctx.state(0.0f32);
        let mut panel_state: DraggablePanelState = ctx.get_state(panel_id);

        // Sync open state from app
        if draw_ctx.is_open && !panel_state.visibility.is_visible() {
            panel_state.visibility = DraggablePanelVisibility::JustOpened;
            ctx.set_state(panel_id, panel_state.clone());
        } else if !draw_ctx.is_open && panel_state.visibility.is_visible() {
            panel_state.visibility = DraggablePanelVisibility::Hidden;
            ctx.set_state(panel_id, panel_state.clone());
        }

        // Always emit panel sync
        let current_panel: DraggablePanelState = ctx.get_state(panel_id);
        ctx.emit(ParticleInspectorPanelSync {
            position: current_panel.position,
            visibility: current_panel.visibility,
        });

        // Detect close
        if draw_ctx.is_open && !current_panel.visibility.is_visible() {
            ctx.emit(ParticleInspectorAction::Close);
        }

        if !current_panel.visibility.is_visible() {
            return ViewDescriptor::Empty;
        }

        let theme = &draw_ctx.theme;
        let data = &draw_ctx.data;
        let mut children: Vec<ViewDescriptor> = Vec::new();

        // Emitter label
        children.push(
            text("Emitter:")
                .color(theme.text_primary)
                .font_size(FontSize::Small),
        );

        // Emitter selector via RadioButton group
        if data.emitter_entities.is_empty() {
            children.push(
                text("No particle emitters in scene")
                    .color(theme.text_muted)
                    .font_size(FontSize::Small),
            );
        } else {
            let selected_idx = data
                .selected_emitter_entity
                .and_then(|e| data.emitter_entities.iter().position(|&id| id == e))
                .unwrap_or(0);
            let emitter_sel_id: StateId = ctx.state(selected_idx);
            let current_sel: usize = ctx.get_state(emitter_sel_id);

            // Detect emitter selection change
            if current_sel != selected_idx && current_sel < data.emitter_entities.len() {
                ctx.emit(ParticleInspectorAction::SelectEmitter(
                    data.emitter_entities[current_sel],
                ));
            }
            ctx.set_state(emitter_sel_id, selected_idx);

            for (idx, _) in data.emitter_entities.iter().enumerate() {
                children.push(radio(emitter_sel_id, idx, format!("Emitter {}", idx)));
            }
        }

        // Emitter config sections
        if let Some(ref config) = data.selected_emitter_config {
            let entity = data.selected_emitter_entity;
            if let Some(entity) = entity {
                let mut config_children = Vec::new();

                // Shape section — RadioButton group
                config_children.push(heading("Emitter Shape", theme));
                let shape_names = ["Point", "Line", "Circle", "Sphere", "Box"];
                let shape_idx = shape_names
                    .iter()
                    .position(|&s| s == config.shape_name)
                    .unwrap_or(0);
                let shape_sel_id: StateId = ctx.state(shape_idx);
                let current_shape: usize = ctx.get_state(shape_sel_id);

                if current_shape != shape_idx {
                    let field = match current_shape {
                        1 => EmitterField::ShapeLine,
                        2 => EmitterField::ShapeCircle,
                        3 => EmitterField::ShapeSphere,
                        4 => EmitterField::ShapeBox,
                        _ => EmitterField::ShapePoint,
                    };
                    ctx.emit(ParticleInspectorAction::SetEmitterField(entity, field));
                }
                ctx.set_state(shape_sel_id, shape_idx);

                let shape_buttons: Vec<ViewDescriptor> = shape_names
                    .iter()
                    .enumerate()
                    .map(|(i, name)| radio(shape_sel_id, i, *name))
                    .collect();
                config_children.push(hstack(shape_buttons).spacing(2.0));

                // Shape params
                match config.shape_name {
                    "Line" => {
                        config_children.push(scalar_slider(
                            ctx,
                            "Length",
                            config.shape_params[0],
                            0.1..=50.0,
                            entity,
                            EmitterField::ShapeParam0,
                        ));
                        config_children.push(property_row("Axis:", "Y (vertical)"));
                    }
                    "Circle" => {
                        config_children.push(scalar_slider(
                            ctx,
                            "Radius",
                            config.shape_params[0],
                            0.1..=50.0,
                            entity,
                            EmitterField::ShapeParam0,
                        ));
                        config_children.push(property_row("Plane:", "XZ (horizontal)"));
                    }
                    "Sphere" => {
                        config_children.push(scalar_slider(
                            ctx,
                            "Radius",
                            config.shape_params[0],
                            0.1..=50.0,
                            entity,
                            EmitterField::ShapeParam0,
                        ));
                    }
                    "Box" => {
                        config_children.push(scalar_slider(
                            ctx,
                            "Width",
                            config.shape_params[0],
                            0.1..=50.0,
                            entity,
                            EmitterField::ShapeParam0,
                        ));
                        config_children.push(scalar_slider(
                            ctx,
                            "Height",
                            config.shape_params[1],
                            0.1..=50.0,
                            entity,
                            EmitterField::ShapeParam1,
                        ));
                        config_children.push(scalar_slider(
                            ctx,
                            "Depth",
                            config.shape_params[2],
                            0.1..=50.0,
                            entity,
                            EmitterField::ShapeParam2,
                        ));
                    }
                    _ => {}
                }

                // Emission
                config_children.push(heading("Emission", theme));
                config_children.push(scalar_slider(
                    ctx,
                    "Emit Rate",
                    config.emit_rate,
                    0.0..=1000.0,
                    entity,
                    EmitterField::EmitRate,
                ));
                config_children.push(scalar_slider(
                    ctx,
                    "Base Lifetime",
                    config.base_lifetime,
                    0.1..=30.0,
                    entity,
                    EmitterField::BaseLifetime,
                ));
                config_children.push(scalar_slider(
                    ctx,
                    "Lifetime Var",
                    config.lifetime_variation,
                    0.0..=1.0,
                    entity,
                    EmitterField::LifetimeVariation,
                ));

                // Velocity
                config_children.push(heading("Velocity", theme));
                config_children.push(scalar_slider(
                    ctx,
                    "Magnitude",
                    config.velocity_magnitude,
                    0.0..=50.0,
                    entity,
                    EmitterField::VelocityMagnitude,
                ));
                config_children.push(scalar_slider(
                    ctx,
                    "Cone Angle",
                    config.velocity_cone_angle,
                    0.0..=std::f32::consts::FRAC_PI_2,
                    entity,
                    EmitterField::VelocityConeAngle,
                ));

                // Scale
                config_children.push(heading("Scale", theme));
                config_children.push(scalar_slider(
                    ctx,
                    "Base Scale",
                    config.base_scale,
                    0.01..=5.0,
                    entity,
                    EmitterField::BaseScale,
                ));
                config_children.push(scalar_slider(
                    ctx,
                    "Scale Var",
                    config.scale_variation,
                    0.0..=1.0,
                    entity,
                    EmitterField::ScaleVariation,
                ));

                // Color
                config_children.push(heading("Color", theme));
                config_children.push(property_row(
                    "Color:",
                    &format!(
                        "R:{:.2} G:{:.2} B:{:.2} A:{:.2}",
                        config.color[0], config.color[1], config.color[2], config.color[3]
                    ),
                ));
                config_children.push(scalar_slider(
                    ctx,
                    "Color Var",
                    config.color_variation,
                    0.0..=1.0,
                    entity,
                    EmitterField::ColorVariation,
                ));
                config_children.push(property_row(
                    "Color End:",
                    &format!(
                        "R:{:.2} G:{:.2} B:{:.2} A:{:.2}",
                        config.color_end[0],
                        config.color_end[1],
                        config.color_end[2],
                        config.color_end[3]
                    ),
                ));

                // Size Over Lifetime
                config_children.push(heading("Size Over Lifetime", theme));
                config_children.push(scalar_slider(
                    ctx,
                    "Scale End",
                    config.scale_end,
                    0.0..=3.0,
                    entity,
                    EmitterField::ScaleEnd,
                ));

                // Forces
                config_children.push(heading("Forces", theme));
                config_children.push(scalar_slider(
                    ctx,
                    "Gravity",
                    config.gravity,
                    -30.0..=30.0,
                    entity,
                    EmitterField::Gravity,
                ));
                config_children.push(scalar_slider(
                    ctx,
                    "Turb Str",
                    config.turbulence_strength,
                    0.0..=10.0,
                    entity,
                    EmitterField::TurbulenceStrength,
                ));
                config_children.push(scalar_slider(
                    ctx,
                    "Turb Freq",
                    config.turbulence_frequency,
                    0.1..=20.0,
                    entity,
                    EmitterField::TurbulenceFrequency,
                ));

                // Statistics
                if let Some(stats) = &data.stats {
                    config_children.push(statistics_section(stats));
                }

                // Controls — Toggle buttons
                config_children.push(heading("Controls", theme));

                let active_id: StateId = ctx.state(config.active);
                let active_val: bool = ctx.get_state(active_id);
                if active_val != config.active {
                    ctx.emit(ParticleInspectorAction::ToggleEmitter);
                    ctx.set_state(active_id, config.active);
                }
                config_children.push(toggle(
                    if config.active { "Disable" } else { "Enable" },
                    active_id,
                ));

                let reset_id: StateId = ctx.state(false);
                let reset_val: bool = ctx.get_state(reset_id);
                if reset_val {
                    ctx.emit(ParticleInspectorAction::ResetSystem);
                    ctx.set_state(reset_id, false);
                }
                config_children.push(toggle("Reset System", reset_id));

                children.extend(config_children);
            }
        } else if data.selected_emitter_entity.is_some() {
            children.push(
                text("Selected emitter not found")
                    .color(theme.text_muted)
                    .font_size(FontSize::Small),
            );
        }

        draggable_panel(
            "Particle Inspector",
            320.0,
            600.0,
            scroll(
                vstack(children)
                    .spacing(4.0)
                    .padding_all(8.0)
                    .align(Alignment::Leading),
                scroll_id,
            ),
            panel_id,
        )
    }
}

fn heading(label: &str, theme: &ColorScheme) -> ViewDescriptor {
    text(label)
        .color(theme.text_accent)
        .font_size(FontSize::Small)
}

fn scalar_slider(
    ctx: &mut BuildContext,
    label: &str,
    config_value: f32,
    range: std::ops::RangeInclusive<f32>,
    entity: EntityId,
    field: fn(f32) -> EmitterField,
) -> ViewDescriptor {
    let value_id: StateId = ctx.state(config_value);
    let current: f32 = ctx.get_state(value_id);
    if (current - config_value).abs() > 1e-4 {
        ctx.emit(ParticleInspectorAction::SetEmitterField(
            entity,
            field(current),
        ));
    }
    labeled_slider(label, value_id, range)
        .label_width(100.0)
        .show_value(true)
        .precision(2)
}

fn statistics_section(stats: &ParticleStats) -> ViewDescriptor {
    let mut children = Vec::new();

    children.push(text("Statistics").font_size(FontSize::Small));
    children.push(property_row(
        "Alive:",
        &format!("{} / {}", stats.current_alive_count, stats.max_alive_count),
    ));
    children.push(property_row("Dead:", &format!("{}", stats.dead_count)));
    children.push(property_row(
        "Buffer:",
        &format!("{:.1}%", stats.buffer_utilization * 100.0),
    ));
    children.push(property_row(
        "Memory:",
        &format!("{:.2} MB", stats.memory_used_mb),
    ));

    children.push(text("Performance").font_size(FontSize::Small));
    children.push(property_row(
        "Compute:",
        &format!("{:.3} ms", stats.compute_time_ms),
    ));
    children.push(property_row(
        "Avg:",
        &format!("{:.3} ms", stats.avg_compute_time_ms),
    ));
    children.push(property_row(
        "Peak:",
        &format!("{:.3} ms", stats.peak_compute_time_ms),
    ));

    children.push(text("Lifetime").font_size(FontSize::Small));
    children.push(property_row(
        "Emitted:",
        &format!("{}", stats.total_emitted),
    ));
    children.push(property_row("Died:", &format!("{}", stats.total_died)));
    children.push(property_row("Frames:", &format!("{}", stats.frame_count)));
    children.push(property_row(
        "Dispatches:",
        &format!("{}", stats.total_dispatches),
    ));

    vstack(children).spacing(4.0)
}
