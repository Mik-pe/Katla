use std::boxed::Box;

use katla_ui::ColorScheme;
use katla_ui::declarative::{
    Alignment, Build, BuildContext, Padding, Widget, WidgetBox, empty, hstack, statusbar, text,
};

/// Snapshot of data needed to render the status bar each frame.
#[derive(Clone)]
pub(crate) struct StatusBarData {
    pub height: f32,
    pub fps: f32,
    pub frame_time_ms: f32,
    pub frame_count: usize,
    pub entity_count: usize,
    pub draw_call_count: usize,
    pub selected_count: usize,
    pub total_assets: usize,
    pub is_playing: bool,
    pub theme: ColorScheme,
    pub save_confirmation_timer: f32,
}

pub(crate) struct StatusBarView;

impl Build for StatusBarView {
    fn build(&self, ctx: &mut BuildContext) -> Box<dyn Widget> {
        let Some(data) = ctx.env::<StatusBarData>() else {
            return empty().boxed();
        };

        let fps_color = if data.fps >= 55.0 {
            data.theme.success
        } else if data.fps >= 30.0 {
            data.theme.warning
        } else {
            data.theme.error
        };

        let frame_time_color = if data.frame_time_ms <= 18.0 {
            data.theme.success
        } else if data.frame_time_ms <= 33.0 {
            data.theme.warning
        } else {
            data.theme.error
        };

        let selection_color = if data.selected_count > 0 {
            data.theme.highlight
        } else {
            data.theme.text_secondary
        };

        let selection_text = if data.selected_count > 0 {
            format!("Selected: {} / {}", data.selected_count, data.total_assets)
        } else {
            format!("Assets: {}", data.total_assets)
        };

        let mode_text = if data.is_playing {
            "PLAYING"
        } else {
            "EDITING"
        };
        let mode_color = if data.is_playing {
            data.theme.success
        } else {
            data.theme.text_secondary
        };

        let left_items = vec![
            text(format!("FPS: {:.0}", data.fps))
                .color(fps_color)
                .boxed(),
            text("·").color(data.theme.text_muted).boxed(),
            text(format!("{:.2} ms", data.frame_time_ms))
                .color(frame_time_color)
                .boxed(),
            text("·").color(data.theme.text_muted).boxed(),
            text(format!("Frame: {}", data.frame_count))
                .color(data.theme.text_secondary)
                .boxed(),
            text("·").color(data.theme.text_muted).boxed(),
            text(format!("Entities: {}", data.entity_count))
                .color(data.theme.text_secondary)
                .boxed(),
            text("·").color(data.theme.text_muted).boxed(),
            text(format!("Draws: {}", data.draw_call_count))
                .color(data.theme.text_secondary)
                .boxed(),
            text("·").color(data.theme.text_muted).boxed(),
            text(selection_text).color(selection_color).boxed(),
        ];

        let right_items = vec![
            text(format!("ColorScheme: {}", data.theme.name))
                .color(data.theme.text_muted)
                .boxed(),
            text("·").color(data.theme.text_muted).boxed(),
            text(mode_text).color(mode_color).boxed(),
        ];

        let mut content_children = vec![
            hstack(left_items)
                .spacing(8.0)
                .padding_all(4.0)
                .align(Alignment::Center)
                .boxed(),
        ];

        if data.save_confirmation_timer > 0.0 {
            content_children.push(text("✓ Scene saved").color(data.theme.success).boxed());
        }

        content_children.push(
            hstack(right_items)
                .spacing(8.0)
                .padding(Padding::horizontal(16.0))
                .align(Alignment::Trailing)
                .flex_grow(1.0)
                .boxed(),
        );

        statusbar(data.height, hstack(content_children).boxed()).boxed()
    }
}
