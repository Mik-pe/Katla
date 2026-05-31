use std::boxed::Box;
use std::path::PathBuf;

use katla_ecs::EntityId;
use katla_math::Rect2D;
use katla_ui::FontSize;
use katla_ui::declarative::{
    Build, BuildContext, StateId, Widget, WidgetBox, button, empty, panel, scroll,
    separator_horizontal, text, vstack,
};

use crate::ui::editor_ui::ColorScheme;
use crate::ui::editor_ui::types::{EntityInfo, InspectorEditState};

/// Environment data injected before each frame for the inspector panel.
#[derive(Clone)]
pub(crate) struct InspectorDrawCtx {
    pub bounds: Rect2D,
    pub selected_entity: Option<EntityId>,
    pub entities: Vec<EntityInfo>,
    #[expect(dead_code)]
    pub edit: InspectorEditState,
    pub theme: ColorScheme,
    #[expect(dead_code)]
    pub available_components: Vec<&'static str>,
    #[expect(dead_code)]
    pub add_component_open: bool,
    #[expect(dead_code)]
    pub add_component_filter: String,
    #[expect(dead_code)]
    pub focus_script_input: bool,
    pub audio_listener_count: usize,
}

pub(crate) struct InspectorView;

impl Build for InspectorView {
    fn build(&self, ctx: &mut BuildContext) -> Box<dyn Widget> {
        let draw_ctx = ctx.env::<InspectorDrawCtx>().cloned();
        let Some(draw_ctx) = draw_ctx else {
            return empty().boxed();
        };

        let scroll_id: StateId = ctx.state(0.0f32);

        let header_text = if let Some(entity_id) = draw_ctx.selected_entity {
            format!("Inspector: Entity {}", entity_id.id())
        } else {
            "Inspector".to_string()
        };

        let content = if let Some(entity) = draw_ctx
            .entities
            .iter()
            .find(|e| draw_ctx.selected_entity == Some(e.id))
        {
            let mut children = Vec::new();

            // Transform
            children.push(text("Transform").font_size(FontSize::Small).boxed());
            children.push(separator_horizontal().boxed());
            children.push(text(format!("Position: {:?}", entity.position)).boxed());
            children.push(text(format!("Rotation: {:?}", entity.rotation)).boxed());
            children.push(text(format!("Scale: {:?}", entity.scale)).boxed());
            children.push(separator_horizontal().boxed());

            // Type
            children.push(text("Type").font_size(FontSize::Small).boxed());
            children.push(text(&entity.entity_type).boxed());
            children.push(separator_horizontal().boxed());

            // AudioSource
            if let Some(ref src) = entity.audio_source {
                children.push(text("Audio Source").font_size(FontSize::Small).boxed());
                children.push(separator_horizontal().boxed());
                children.push(text(format!("Path: {}", src.path)).boxed());
                if let Some(sr) = src.sample_rate {
                    children.push(text(format!("Sample Rate: {} Hz", sr)).boxed());
                }
                if let Some(ch) = src.channels {
                    children.push(text(format!("Channels: {}", ch)).boxed());
                }
                if let Some(dur) = src.duration_secs {
                    children.push(text(format!("Duration: {:.2}s", dur)).boxed());
                }
                let path_clone = src.path.clone();
                let play_btn = button("▶ Play Preview")
                    .on_click(ctx.on_click(move |actions| {
                        actions.emit(
                            crate::ui::editor_ui::types::EditorAction::AudioPreviewToggle {
                                path: PathBuf::from(&path_clone),
                            },
                        );
                    }))
                    .boxed();
                children.push(play_btn);
                children.push(separator_horizontal().boxed());
            }

            // AudioListener
            if entity.has_audio_listener {
                children.push(text("Audio Listener").font_size(FontSize::Small).boxed());
                children.push(separator_horizontal().boxed());
                children.push(text("Active listener").boxed());
                if draw_ctx.audio_listener_count > 1 {
                    children.push(
                        text(format!(
                            "⚠ {} listeners in scene",
                            draw_ctx.audio_listener_count
                        ))
                        .color(draw_ctx.theme.warning)
                        .boxed(),
                    );
                }
                children.push(separator_horizontal().boxed());
            }

            vstack(children).boxed()
        } else {
            text("No entity selected")
                .color(draw_ctx.theme.text_muted)
                .boxed()
        };

        let panel_content = scroll(content, scroll_id).flex_grow(1.0).boxed();

        panel(header_text, panel_content)
            .flex_width(draw_ctx.bounds.width())
            .flex_height(draw_ctx.bounds.height())
            .boxed()
    }
}
