use std::cell::RefCell;

use katla_ecs::EntityId;
use katla_math::{Rect2D, Vec2};
use katla_ui::declarative::{Build, BuildContext, ViewDescriptor};
use katla_ui::widgets::{
    Button, DraggablePanel, DraggablePanelConfig, DraggablePanelState, LabeledSlider,
};
use katla_ui::{FontSize, ScrollArea, ScrollAreaState, UiContext};

use crate::ui::particle_inspector::EmitterField;
use crate::ui::{ColorScheme, ParticleInspectorAction, ParticleInspectorData, ParticleStats};

thread_local! {
    static PARTICLE_INSPECTOR_CTX: RefCell<Option<ParticleInspectorDrawCtx>> = const { RefCell::new(None) };
}

struct ParticleInspectorDrawCtx {
    panel: DraggablePanelState,
    scroll_state: ScrollAreaState,
    selected_emitter: Option<EntityId>,
    theme: ColorScheme,
    data: ParticleInspectorData,
    pending_actions: Vec<ParticleInspectorAction>,
}

pub(crate) fn set_particle_inspector_ctx(
    panel: DraggablePanelState,
    scroll_state: ScrollAreaState,
    selected_emitter: Option<EntityId>,
    theme: &ColorScheme,
    data: &ParticleInspectorData,
) {
    PARTICLE_INSPECTOR_CTX.with(|c| {
        *c.borrow_mut() = Some(ParticleInspectorDrawCtx {
            panel,
            scroll_state,
            selected_emitter,
            theme: theme.clone(),
            data: data.clone(),
            pending_actions: Vec::new(),
        })
    });
}

pub(crate) fn take_particle_inspector_ctx() -> Option<(
    DraggablePanelState,
    ScrollAreaState,
    Option<EntityId>,
    Vec<ParticleInspectorAction>,
)> {
    PARTICLE_INSPECTOR_CTX.with(|c| {
        c.borrow_mut().take().map(|ctx| {
            (
                ctx.panel,
                ctx.scroll_state,
                ctx.selected_emitter,
                ctx.pending_actions,
            )
        })
    })
}

pub(crate) struct ParticleInspectorView;

impl Build for ParticleInspectorView {
    fn build(&self, _ctx: &mut BuildContext) -> ViewDescriptor {
        ViewDescriptor::Custom(draw_particle_inspector)
    }
}

fn draw_particle_inspector(ui: &mut UiContext, _bounds: Rect2D) {
    let ctx = PARTICLE_INSPECTOR_CTX.with(|c| c.borrow_mut().take());
    let Some(mut ctx) = ctx else {
        return;
    };

    let panel_width = 320.0;
    let panel_height = 600.0;
    let screen_size = ui.screen_size();

    let was_visible = ctx.panel.is_visible();

    DraggablePanel::show(
        ui,
        &mut ctx.panel,
        DraggablePanelConfig::new("particle_inspector", "Particle Inspector")
            .size(panel_width, panel_height)
            .screen_size(screen_size),
        |ui, frame| {
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

            let theme = &ctx.theme;
            let emitter_entities = &ctx.data.emitter_entities;
            let selected_emitter_entity = ctx.data.selected_emitter_entity;
            let mut selected_emitter_config = ctx.data.selected_emitter_config.take();
            let stats = &ctx.data.stats;
            let selected_emitter = &ctx.selected_emitter;
            let mut scroll_actions: Vec<ParticleInspectorAction> = Vec::new();

            ctx.scroll_state = ui.scroll_area(
                ScrollArea::new("particle_inspector_scroll").max_height(content_height),
                ctx.scroll_state,
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

                    if emitter_entities.is_empty() {
                        ui.draw_text(
                            "No particle emitters in scene",
                            Vec2::new(cursor_x, cursor_y),
                            theme.text_muted,
                            ui.scaled_font_size(FontSize::Small),
                        );
                        cursor_y += line_height;
                    } else {
                        for (idx, entity_id) in emitter_entities.iter().enumerate() {
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
                                scroll_actions
                                    .push(ParticleInspectorAction::SelectEmitter(*entity_id));
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

                    if let Some(ref mut config) = selected_emitter_config {
                        let layout = InspectorLayout {
                            cursor_x,
                            content_width,
                            theme,
                        };
                        cursor_y = render_emitter_config(
                            ui,
                            config,
                            selected_emitter_entity,
                            stats,
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

            ctx.pending_actions.extend(scroll_actions);
        },
    );

    if was_visible && !ctx.panel.is_visible() {
        ctx.pending_actions.push(ParticleInspectorAction::Close);
    }

    PARTICLE_INSPECTOR_CTX.with(|c| *c.borrow_mut() = Some(ctx));
}

/// Layout context for drawing inside the particle inspector scroll area.
struct InspectorLayout<'a> {
    cursor_x: f32,
    content_width: f32,
    theme: &'a ColorScheme,
}

fn render_emitter_config(
    ui: &mut UiContext,
    config: &mut super::super::super::particle_inspector::EmitterConfigView,
    emitter_entity: Option<EntityId>,
    stats: &Option<ParticleStats>,
    layout: &InspectorLayout,
    mut y: f32,
    pending_actions: &mut Vec<ParticleInspectorAction>,
) -> f32 {
    let slider_h = ui.style().slider_default_height;
    let btn_w = layout.content_width - 8.0;
    let cx = layout.cursor_x;
    let w = layout.content_width;
    let theme = layout.theme;
    let entity = match emitter_entity {
        Some(id) => id,
        None => return y,
    };

    macro_rules! heading {
        ($text:expr) => {
            ui.set_cursor(Vec2::new(cx, y));
            ui.draw_text(
                $text,
                ui.cursor(),
                theme.text_accent,
                ui.scaled_font_size(FontSize::Small),
            );
            y += 20.0;
        };
    }

    macro_rules! scalar_slider {
        ($label:expr, $value:expr, $range:expr, $field:expr) => {{
            let old_val = $value;
            let bounds = Rect2D::from_origin_size(Vec2::new(cx, y), Vec2::new(w, slider_h));
            ui.add(
                LabeledSlider::new($label, &mut $value, $range)
                    .bounds(bounds)
                    .label_width(100.0)
                    .show_value(true)
                    .precision(2),
            );
            if ($value - old_val).abs() > 1e-4 {
                pending_actions.push(ParticleInspectorAction::SetEmitterField(
                    entity,
                    $field($value),
                ));
            }
            y += slider_h + 2.0;
        }};
    }

    macro_rules! read_only_row {
        ($label:expr, $value:expr) => {
            ui.set_cursor(Vec2::new(cx, y));
            ui.property_row($label, $value);
            y += 20.0;
        };
    }

    heading!("Emitter Shape");

    let shapes = ["Point", "Line", "Circle", "Sphere", "Box"];
    let shape_btn_w = w / shapes.len() as f32;
    for (i, shape_name) in shapes.iter().enumerate() {
        let is_active = config.shape_name == *shape_name;
        let btn_bounds = Rect2D::from_origin_size(
            Vec2::new(cx + i as f32 * shape_btn_w, y),
            Vec2::new(shape_btn_w - 2.0, 20.0),
        );
        let color = if is_active {
            theme.highlight
        } else {
            theme.button_bg
        };
        if ui
            .add(Button::new(shape_name).bounds(btn_bounds).fill_color(color))
            .clicked
        {
            let field = match *shape_name {
                "Point" => EmitterField::ShapePoint,
                "Line" => EmitterField::ShapeLine,
                "Circle" => EmitterField::ShapeCircle,
                "Sphere" => EmitterField::ShapeSphere,
                "Box" => EmitterField::ShapeBox,
                _ => EmitterField::ShapePoint,
            };
            pending_actions.push(ParticleInspectorAction::SetEmitterField(entity, field));
        }
    }
    y += 24.0;

    match config.shape_name {
        "Line" => {
            scalar_slider!(
                "Length",
                config.shape_params[0],
                0.1..=50.0,
                EmitterField::ShapeParam0
            );
            read_only_row!("Axis:", "Y (vertical)");
        }
        "Circle" => {
            scalar_slider!(
                "Radius",
                config.shape_params[0],
                0.1..=50.0,
                EmitterField::ShapeParam0
            );
            read_only_row!("Plane:", "XZ (horizontal)");
        }
        "Sphere" => {
            scalar_slider!(
                "Radius",
                config.shape_params[0],
                0.1..=50.0,
                EmitterField::ShapeParam0
            );
        }
        "Box" => {
            scalar_slider!(
                "Width",
                config.shape_params[0],
                0.1..=50.0,
                EmitterField::ShapeParam0
            );
            scalar_slider!(
                "Height",
                config.shape_params[1],
                0.1..=50.0,
                EmitterField::ShapeParam1
            );
            scalar_slider!(
                "Depth",
                config.shape_params[2],
                0.1..=50.0,
                EmitterField::ShapeParam2
            );
        }
        _ => {}
    }

    heading!("Emission");
    scalar_slider!(
        "Emit Rate",
        config.emit_rate,
        0.0..=1000.0,
        EmitterField::EmitRate
    );
    scalar_slider!(
        "Base Lifetime",
        config.base_lifetime,
        0.1..=30.0,
        EmitterField::BaseLifetime
    );
    scalar_slider!(
        "Lifetime Var",
        config.lifetime_variation,
        0.0..=1.0,
        EmitterField::LifetimeVariation
    );

    heading!("Velocity");
    scalar_slider!(
        "Magnitude",
        config.velocity_magnitude,
        0.0..=50.0,
        EmitterField::VelocityMagnitude
    );
    scalar_slider!(
        "Cone Angle",
        config.velocity_cone_angle,
        0.0..=std::f32::consts::FRAC_PI_2,
        EmitterField::VelocityConeAngle
    );

    heading!("Scale");
    scalar_slider!(
        "Base Scale",
        config.base_scale,
        0.01..=5.0,
        EmitterField::BaseScale
    );
    scalar_slider!(
        "Scale Var",
        config.scale_variation,
        0.0..=1.0,
        EmitterField::ScaleVariation
    );

    heading!("Color");
    {
        read_only_row!(
            "Color:",
            &format!(
                "R:{:.2} G:{:.2} B:{:.2} A:{:.2}",
                config.color[0], config.color[1], config.color[2], config.color[3]
            )
        );
    }
    scalar_slider!(
        "Color Var",
        config.color_variation,
        0.0..=1.0,
        EmitterField::ColorVariation
    );
    {
        read_only_row!(
            "Color End:",
            &format!(
                "R:{:.2} G:{:.2} B:{:.2} A:{:.2}",
                config.color_end[0], config.color_end[1], config.color_end[2], config.color_end[3]
            )
        );
    }

    heading!("Size Over Lifetime");
    scalar_slider!(
        "Scale End",
        config.scale_end,
        0.0..=3.0,
        EmitterField::ScaleEnd
    );

    heading!("Forces");
    scalar_slider!(
        "Gravity",
        config.gravity,
        -30.0..=30.0,
        EmitterField::Gravity
    );
    scalar_slider!(
        "Turb Str",
        config.turbulence_strength,
        0.0..=10.0,
        EmitterField::TurbulenceStrength
    );
    scalar_slider!(
        "Turb Freq",
        config.turbulence_frequency,
        0.1..=20.0,
        EmitterField::TurbulenceFrequency
    );

    if let Some(stats) = stats {
        heading!("Statistics");
        read_only_row!(
            "Alive:",
            &format!("{} / {}", stats.current_alive_count, stats.max_alive_count)
        );
        read_only_row!("Dead:", &format!("{}", stats.dead_count));
        read_only_row!(
            "Buffer:",
            &format!("{:.1}%", stats.buffer_utilization * 100.0)
        );
        read_only_row!("Memory:", &format!("{:.2} MB", stats.memory_used_mb));

        heading!("Performance");
        read_only_row!("Compute:", &format!("{:.3} ms", stats.compute_time_ms));
        read_only_row!("Avg:", &format!("{:.3} ms", stats.avg_compute_time_ms));
        read_only_row!("Peak:", &format!("{:.3} ms", stats.peak_compute_time_ms));

        heading!("Lifetime");
        read_only_row!("Emitted:", &format!("{}", stats.total_emitted));
        read_only_row!("Died:", &format!("{}", stats.total_died));
        read_only_row!("Frames:", &format!("{}", stats.frame_count));
        read_only_row!("Dispatches:", &format!("{}", stats.total_dispatches));
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
