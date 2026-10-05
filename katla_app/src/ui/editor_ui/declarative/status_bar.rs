use std::boxed::Box;

use katla_ui::ColorScheme;
use katla_ui::FontSize;
use katla_ui::declarative::{
    Alignment, Build, BuildContext, Padding, Widget, WidgetBox, empty, hstack, statusbar, text,
};

/// Snapshot of data needed to render the status bar each frame.
#[derive(Clone)]
pub(crate) struct StatusBarData {
    pub height: f32,
    pub fps: f32,
    pub frame_time_ms: f32,
    pub entity_count: usize,
    pub draw_call_count: usize,
    pub total_assets: usize,
    pub is_playing: bool,
    pub is_paused: bool,
    pub theme: ColorScheme,
    pub save_confirmation_timer: f32,
}

pub(crate) struct StatusBarView;

impl Build for StatusBarView {
    fn build(&self, ctx: &mut BuildContext) -> Box<dyn Widget> {
        let Some(data) = ctx.env::<StatusBarData>() else {
            return empty().boxed();
        };
        let theme = &data.theme;

        // Left cluster: low-priority telemetry. Everything sits at Small +
        // muted; only a bad frame rate earns attention (warning color).
        let fps_color = if data.fps < 30.0 {
            theme.warning
        } else {
            theme.text_muted
        };
        let left_items = vec![
            text(format!("FPS {:.0}", data.fps))
                .color(fps_color)
                .font_size(FontSize::Small)
                .boxed(),
            text(format!("{:.1} ms", data.frame_time_ms))
                .color(theme.text_muted)
                .font_size(FontSize::Small)
                .boxed(),
            text(format!("{} entities", data.entity_count))
                .color(theme.text_muted)
                .font_size(FontSize::Small)
                .boxed(),
            text(format!("{} draws", data.draw_call_count))
                .color(theme.text_muted)
                .font_size(FontSize::Small)
                .boxed(),
            text(format!("{} assets", data.total_assets))
                .color(theme.text_muted)
                .font_size(FontSize::Small)
                .boxed(),
        ];

        // Right cluster: the mode is the one loud item on the bar.
        let (mode_text, mode_color) = if data.is_playing && !data.is_paused {
            ("PLAYING", theme.success)
        } else if data.is_paused {
            ("PAUSED", theme.warning)
        } else {
            ("EDITING", theme.text_muted)
        };
        let right_items = vec![
            text(mode_text)
                .color(mode_color)
                .font_size(FontSize::Small)
                .boxed(),
        ];

        let mut content_children = vec![
            // flex_height makes the wrapper span the full bar height so its
            // Middle alignment centers content in the 24 px strip, not in
            // a text-height box riding the top edge.
            hstack(left_items)
                .spacing(12.0)
                .padding(Padding::horizontal(12.0))
                .align(Alignment::Middle)
                .flex_height(data.height)
                .boxed(),
        ];

        if data.save_confirmation_timer > 0.0 {
            content_children.push(
                text("✓ Scene saved")
                    .color(theme.success)
                    .font_size(FontSize::Small)
                    .boxed(),
            );
        }

        content_children.push(
            hstack(right_items)
                .spacing(6.0)
                .padding(Padding::horizontal(8.0))
                .align(Alignment::Trailing)
                .flex_grow(1.0)
                .flex_height(data.height)
                .boxed(),
        );

        statusbar(data.height, hstack(content_children).boxed()).boxed()
    }
}
