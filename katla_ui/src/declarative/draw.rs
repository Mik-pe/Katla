use katla_math::{Color, Rect2D, Vec2};

use crate::context::UiContext;

use super::animation::AnimationState;
use super::descriptor::ViewDescriptor;
use super::state::StateArena;
use super::tree::InteractionState;

pub(crate) fn draw_descriptor_with_id(
    descriptor: &ViewDescriptor,
    ui: &mut UiContext,
    bounds: Rect2D,
    state_arena: &StateArena,
    children_bounds: &[Rect2D],
    interaction: &InteractionState,
    view_id: super::state::ViewId,
    anim_state: &AnimationState,
) {
    let _ = state_arena;
    let is_hovered = interaction.hovered_id == Some(view_id);
    let is_active = interaction.active_id == Some(view_id);

    match descriptor {
        ViewDescriptor::Empty => {}

        ViewDescriptor::TransitionContainer { .. } => {}

        ViewDescriptor::Text {
            content,
            color,
            font_size,
        } => {
            let text_color = color.unwrap_or(ui.style().text_color);
            let size = font_size
                .map(|fs| ui.scaled_font_size(fs))
                .unwrap_or(ui.style().font_size);
            ui.draw_text(
                content,
                bounds.min,
                anim_state.apply_to_color(text_color),
                size,
            );
        }

        ViewDescriptor::Button {
            label,
            fill_color,
            hover_color,
            border_color,
            on_click: _,
        } => {
            let bg = if is_active {
                hover_color.unwrap_or(ui.style().button_active)
            } else if is_hovered {
                hover_color.unwrap_or(ui.style().button_hovered)
            } else {
                fill_color.unwrap_or(ui.style().button_normal)
            };
            let bg = anim_state.apply_to_color(bg);
            let radius = anim_state.apply_to_corner_radius(ui.style().button_rounding);
            ui.draw_rounded_rect(bounds, bg, radius);

            if !is_active {
                let highlight = Color::new(
                    (bg.r + 0.04).min(1.0),
                    (bg.g + 0.04).min(1.0),
                    (bg.b + 0.04).min(1.0),
                    bg.a,
                );
                ui.draw_line(
                    Vec2::new(bounds.min.x() + radius, bounds.min.y() + 0.5),
                    Vec2::new(bounds.max.x() - radius, bounds.min.y() + 0.5),
                    highlight,
                    1.0,
                );
            }

            if let Some(border) = border_color {
                ui.draw_rounded_selection_border(bounds, *border, 1.0, radius);
            }

            let font_size = ui.style().font_size;
            let text_size = ui.measure_text(label, font_size);
            let text_pos = Vec2::new(
                bounds.center().x() - text_size.x() * 0.5,
                bounds.center().y() - text_size.y() * 0.5,
            );
            ui.draw_text(
                label,
                text_pos,
                anim_state.apply_to_color(ui.style().button_text),
                font_size,
            );
        }

        ViewDescriptor::Slider { .. }
        | ViewDescriptor::Toggle { .. }
        | ViewDescriptor::TextField { .. }
        | ViewDescriptor::Progress { .. }
        | ViewDescriptor::ColorPicker { .. } => {
            // These variants are not yet used by any panel. They will be
            // implemented when panels migrate from ViewDescriptor::Custom
            // to native declarative variants.
        }

        ViewDescriptor::Image { texture, uv, tint } => {
            let uv_rect =
                uv.unwrap_or_else(|| Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(1.0, 1.0)));
            ui.draw_image(bounds, uv_rect.min, uv_rect.max, *tint, *texture);
        }

        ViewDescriptor::HStack(_) | ViewDescriptor::VStack(_) | ViewDescriptor::ZStack(_) => {}

        ViewDescriptor::ScrollView(_) => {
            let bg = ui.style().window_bg;
            ui.draw_rect(bounds, bg);
        }

        ViewDescriptor::Panel(desc) => {
            let bg = ui.style().window_bg;
            ui.draw_rect(bounds, bg);

            let header_bounds = Rect2D::new(
                bounds.min,
                Vec2::new(bounds.max.x(), bounds.min.y() + desc.header_height),
            );
            ui.draw_rect(header_bounds, ui.style().window_title_bg);

            let font_size = ui.style().font_size;
            let padding = ui.style().window_padding;
            let text_pos = Vec2::new(
                header_bounds.min.x() + padding,
                header_bounds.min.y() + (desc.header_height - font_size) * 0.5,
            );
            ui.draw_text(
                &desc.title,
                text_pos,
                ui.style().window_title_text,
                font_size,
            );

            if children_bounds.len() > 1 {
                ui.draw_line(
                    Vec2::new(bounds.min.x(), header_bounds.max.y()),
                    Vec2::new(bounds.max.x(), header_bounds.max.y()),
                    ui.style().window_border,
                    1.0,
                );
            }
        }

        ViewDescriptor::Overlay(_) => {}

        ViewDescriptor::Custom(draw_fn) => {
            draw_fn(ui, bounds);
        }
    }
}
