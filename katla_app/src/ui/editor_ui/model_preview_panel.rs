use katla_math::{Color, Rect2D, Vec2};
use katla_ui::{
    input::mouse_button, widgets::Button, z_index, FontSize, ForkAwesome, Response, UiContext,
    Widget,
};

use super::Theme;
use crate::ui::model_preview::{LoadState, ModelPreviewState};

pub struct ModelPreviewPanel<'a> {
    pub screen_size: Vec2,
    pub state: &'a mut ModelPreviewState,
    pub theme: &'a Theme,
}

impl<'a> ModelPreviewPanel<'a> {
    pub fn new(screen_size: Vec2, state: &'a mut ModelPreviewState, theme: &'a Theme) -> Self {
        Self {
            screen_size,
            state,
            theme,
        }
    }
}

impl<'a> Widget for ModelPreviewPanel<'a> {
    fn ui(self, ui: &mut UiContext) -> Response {
        let panel_width = self.state.panel_width;
        let panel_height = 400.0;
        let title_bar_height = 28.0;
        let padding = 8.0;

        let panel_x = self.screen_size.x() - panel_width - padding;
        let panel_y = 60.0;

        let panel_bounds = Rect2D::from_origin_size(
            Vec2::new(panel_x, panel_y),
            Vec2::new(panel_width, panel_height),
        );

        ui.with_z_index(z_index::POPUP, |ui| {
            ui.draw_rect(panel_bounds, self.theme.panel_bg);
            ui.draw_rect_border(
                panel_bounds,
                self.theme.panel_bg,
                self.theme.panel_border,
                1.0,
            );

            let title_bounds = Rect2D::from_origin_size(
                Vec2::new(panel_x, panel_y),
                Vec2::new(panel_width, title_bar_height),
            );
            ui.draw_rect(title_bounds, self.theme.panel_header);

            let model_name = self
                .state
                .model_path
                .as_ref()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "Model Preview".to_string());
            ui.draw_text(
                &model_name,
                Vec2::new(panel_x + padding, panel_y + 7.0),
                self.theme.text_primary,
                ui.scaled_font_size(FontSize::Small),
            );

            let close_btn_size = 20.0;
            let close_bounds = Rect2D::from_origin_size(
                Vec2::new(panel_x + panel_width - close_btn_size - 4.0, panel_y + 4.0),
                Vec2::new(close_btn_size, close_btn_size),
            );
            if ui
                .add(katla_ui::widgets::ImageButton::new(ForkAwesome::TIMES).bounds(close_bounds))
                .clicked
            {
                self.state.close();
            }

            let content_y = panel_y + title_bar_height + padding;
            let content_width = panel_width - padding * 2.0;
            let preview_height = 200.0;

            let preview_bounds = Rect2D::from_origin_size(
                Vec2::new(panel_x + padding, content_y),
                Vec2::new(content_width, preview_height),
            );

            match &self.state.load_state {
                LoadState::Idle => {
                    ui.draw_rect(preview_bounds, self.theme.background_dark);
                    let text = "No model loaded";
                    let text_size = ui.measure_text(text, ui.scaled_font_size(FontSize::Medium));
                    ui.draw_text(
                        text,
                        Vec2::new(
                            preview_bounds.center().x() - text_size.x() * 0.5,
                            preview_bounds.center().y() - text_size.y() * 0.5,
                        ),
                        self.theme.text_muted,
                        ui.scaled_font_size(FontSize::Medium),
                    );
                }
                LoadState::Loading => {
                    ui.draw_rect(preview_bounds, self.theme.background_dark);

                    let rotation = (std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis()
                        % 1000) as f32
                        / 1000.0
                        * std::f32::consts::TAU;
                    let spinner_chars = ['|', '/', '—', '\\'];
                    let spinner_idx = ((rotation / std::f32::consts::FRAC_PI_2) as usize) % 4;
                    let spinner_char = spinner_chars[spinner_idx];

                    let text = format!("Loading {}", spinner_char);
                    let text_size = ui.measure_text(&text, ui.scaled_font_size(FontSize::Large));
                    ui.draw_text(
                        &text,
                        Vec2::new(
                            preview_bounds.center().x() - text_size.x() * 0.5,
                            preview_bounds.center().y() - text_size.y() * 0.5,
                        ),
                        self.theme.text_secondary,
                        ui.scaled_font_size(FontSize::Large),
                    );

                    let bar_width = content_width * 0.6;
                    let bar_height = 4.0;
                    let bar_x = preview_bounds.center().x() - bar_width * 0.5;
                    let bar_y = preview_bounds.center().y() + 20.0;
                    let bar_bounds = Rect2D::from_origin_size(
                        Vec2::new(bar_x, bar_y),
                        Vec2::new(bar_width, bar_height),
                    );
                    ui.draw_rect(bar_bounds, self.theme.background);
                    let progress = (rotation / std::f32::consts::TAU) * bar_width;
                    let seg_width = bar_width * 0.3;
                    let seg_x = bar_x + (progress * 0.7);
                    ui.draw_rect(
                        Rect2D::from_origin_size(
                            Vec2::new(seg_x.min(bar_x + bar_width - seg_width), bar_y),
                            Vec2::new(seg_width, bar_height),
                        ),
                        self.theme.highlight,
                    );
                }
                LoadState::Loaded => {
                    ui.draw_rect(preview_bounds, self.theme.background_dark);

                    if self.state.model.is_some() {
                        ui.image(
                            self.state.texture_id,
                            preview_bounds,
                            None,
                            Some(Color::OPAQUE_IMAGE),
                        );
                    } else {
                        let text = "Preview Ready";
                        let text_size =
                            ui.measure_text(text, ui.scaled_font_size(FontSize::Medium));
                        ui.draw_text(
                            text,
                            Vec2::new(
                                preview_bounds.center().x() - text_size.x() * 0.5,
                                preview_bounds.center().y() - text_size.y() * 0.5,
                            ),
                            self.theme.text_secondary,
                            ui.scaled_font_size(FontSize::Medium),
                        );
                    }

                    if ui.is_hovered(preview_bounds) {
                        if ui.input.mouse_clicked(mouse_button::LEFT) {
                            self.state.camera.begin_drag(ui.input.mouse_pos);
                        }

                        let scroll = ui.input.scroll_delta.y();
                        if scroll != 0.0 {
                            self.state.camera.zoom(scroll * 0.5);
                        }
                    }

                    if ui.input.is_mouse_down(mouse_button::LEFT) {
                        self.state.camera.update_drag(ui.input.mouse_pos);
                    }

                    if ui.input.mouse_released[mouse_button::LEFT] {
                        self.state.camera.end_drag();
                    }
                }
                LoadState::Failed(error) => {
                    ui.draw_rect(preview_bounds, self.theme.background_dark);
                    let text = "Failed to load".to_string();
                    let text_size = ui.measure_text(&text, ui.scaled_font_size(FontSize::Medium));
                    ui.draw_text(
                        &text,
                        Vec2::new(
                            preview_bounds.center().x() - text_size.x() * 0.5,
                            preview_bounds.center().y() - 30.0,
                        ),
                        self.theme.error,
                        ui.scaled_font_size(FontSize::Medium),
                    );

                    let error_display = if error.len() > 40 {
                        format!("{}...", &error[..40])
                    } else {
                        error.clone()
                    };
                    let error_size =
                        ui.measure_text(&error_display, ui.scaled_font_size(FontSize::XSmall));
                    ui.draw_text(
                        &error_display,
                        Vec2::new(
                            preview_bounds.center().x() - error_size.x() * 0.5,
                            preview_bounds.center().y() + 5.0,
                        ),
                        self.theme.text_muted,
                        ui.scaled_font_size(FontSize::XSmall),
                    );
                }
            }

            let stats_y = content_y + preview_height + padding;
            let mut cursor = Vec2::new(panel_x + padding, stats_y);

            ui.draw_text(
                "Model Statistics",
                cursor,
                self.theme.text_secondary,
                ui.scaled_font_size(FontSize::Small),
            );
            cursor = Vec2::new(cursor.x(), cursor.y() + 20.0);

            if let Some(stats) = &self.state.stats {
                let stat_items = [
                    ("Vertices", format!("{}", stats.vertex_count)),
                    ("Triangles", format!("{}", stats.triangle_count)),
                    ("Meshes", stats.mesh_count.to_string()),
                    ("Primitives", stats.primitive_count.to_string()),
                ];

                for (label, value) in stat_items {
                    ui.draw_text(
                        label,
                        cursor,
                        self.theme.text_muted,
                        ui.scaled_font_size(FontSize::XSmall),
                    );
                    let value_x = panel_x + panel_width
                        - padding
                        - ui.measure_text(&value, ui.scaled_font_size(FontSize::XSmall))
                            .x();
                    ui.draw_text(
                        &value,
                        Vec2::new(value_x, cursor.y()),
                        self.theme.text_primary,
                        ui.scaled_font_size(FontSize::XSmall),
                    );
                    cursor = Vec2::new(cursor.x(), cursor.y() + 16.0);
                }

                if stats.has_animations {
                    cursor = Vec2::new(cursor.x(), cursor.y() + 4.0);
                    ui.draw_text(
                        "Animations",
                        cursor,
                        self.theme.text_secondary,
                        ui.scaled_font_size(FontSize::XSmall),
                    );
                    cursor = Vec2::new(cursor.x(), cursor.y() + 14.0);

                    for anim_name in &stats.animation_names {
                        ui.draw_icon_label(
                            ForkAwesome::VIDEO_CAMERA,
                            anim_name,
                            cursor,
                            ui.scaled_font_size(FontSize::XSmall),
                            ui.scaled_font_size(FontSize::XSmall),
                            self.theme.text_muted,
                        );
                        cursor = Vec2::new(cursor.x(), cursor.y() + 14.0);
                    }

                    cursor = Vec2::new(cursor.x(), cursor.y() + 4.0);
                    let btn_width = 80.0;
                    let btn_height = 24.0;
                    let btn_bounds =
                        Rect2D::from_origin_size(cursor, Vec2::new(btn_width, btn_height));

                    let btn_text = if self.state.animation.playing {
                        "Pause"
                    } else {
                        "Play"
                    };
                    if ui
                        .add(
                            Button::new(btn_text)
                                .bounds(btn_bounds)
                                .fill_color(self.theme.button_bg)
                                .hover_color(self.theme.button_hover),
                        )
                        .clicked
                    {
                        self.state.animation.playing = !self.state.animation.playing;
                    }
                }

                if stats.has_skinning {
                    cursor = Vec2::new(cursor.x(), cursor.y() + 8.0);
                    ui.draw_icon_label(
                        ForkAwesome::USER,
                        "Has Skeleton",
                        cursor,
                        ui.scaled_font_size(FontSize::XSmall),
                        ui.scaled_font_size(FontSize::XSmall),
                        self.theme.info,
                    );
                }
            }
        });

        Response::default()
    }
}
