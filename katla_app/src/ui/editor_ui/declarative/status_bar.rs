use katla_math::Vec2;
use katla_ui::ColorScheme;
use katla_ui::declarative::{
    Alignment, Build, BuildContext, Padding, StackDescriptor, StatusBarDescriptor, ViewDescriptor,
};

/// Snapshot of data needed to render the status bar each frame.
#[derive(Clone)]
pub(crate) struct StatusBarData {
    pub screen_size: Vec2,
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
    fn build(&self, ctx: &mut BuildContext) -> ViewDescriptor {
        let Some(data) = ctx.env::<StatusBarData>() else {
            return ViewDescriptor::Empty;
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
            ViewDescriptor::Text {
                content: format!("FPS: {:.0}", data.fps),
                color: Some(fps_color),
                font_size: None,
            },
            ViewDescriptor::Text {
                content: format!("{:.2} ms", data.frame_time_ms),
                color: Some(frame_time_color),
                font_size: None,
            },
            ViewDescriptor::Text {
                content: format!("Frame: {}", data.frame_count),
                color: Some(data.theme.text_secondary),
                font_size: None,
            },
            ViewDescriptor::Text {
                content: format!("Entities: {}", data.entity_count),
                color: Some(data.theme.text_secondary),
                font_size: None,
            },
            ViewDescriptor::Text {
                content: format!("Draws: {}", data.draw_call_count),
                color: Some(data.theme.text_secondary),
                font_size: None,
            },
            ViewDescriptor::Text {
                content: selection_text,
                color: Some(selection_color),
                font_size: None,
            },
        ];

        let right_items = vec![
            ViewDescriptor::Text {
                content: format!("ColorScheme: {}", data.theme.name),
                color: Some(data.theme.text_muted),
                font_size: None,
            },
            ViewDescriptor::Text {
                content: mode_text.to_string(),
                color: Some(mode_color),
                font_size: None,
            },
        ];

        let mut content_children = vec![ViewDescriptor::HStack(Box::new(StackDescriptor {
            children: left_items,
            spacing: 8.0,
            padding: Padding::all(4.0),
            alignment: Alignment::Center,
        }))];

        if data.save_confirmation_timer > 0.0 {
            content_children.push(ViewDescriptor::Text {
                content: "✓ Scene saved".to_string(),
                color: Some(data.theme.success),
                font_size: None,
            });
        }

        content_children.push(ViewDescriptor::HStack(Box::new(StackDescriptor {
            children: right_items,
            spacing: 8.0,
            padding: Padding::horizontal(16.0),
            alignment: Alignment::Trailing,
        })));

        ViewDescriptor::StatusBar(Box::new(StatusBarDescriptor {
            height: data.height,
            content: Box::new(ViewDescriptor::HStack(Box::new(StackDescriptor {
                children: content_children,
                spacing: 0.0,
                padding: Padding::zero(),
                alignment: Alignment::Leading,
            }))),
        }))
    }
}
