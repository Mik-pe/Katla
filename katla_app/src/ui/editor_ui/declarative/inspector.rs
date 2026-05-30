use std::path::PathBuf;

use katla_ecs::EntityId;
use katla_ui::FontSize;
use katla_ui::declarative::{Build, BuildContext, StateId, ViewDescriptor};

use crate::ui::editor_ui::ColorScheme;
use crate::ui::editor_ui::types::{EntityInfo, InspectorEditState};

/// Environment data injected before each frame for the inspector panel.
#[derive(Clone)]
pub(crate) struct InspectorDrawCtx {
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
    fn build(&self, ctx: &mut BuildContext) -> ViewDescriptor {
        use katla_ui::declarative::{button, panel, scroll, separator_horizontal, text, vstack};

        let draw_ctx = ctx.env::<InspectorDrawCtx>().cloned();
        let Some(draw_ctx) = draw_ctx else {
            return ViewDescriptor::Empty;
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
            children.push(text("Transform").font_size(FontSize::Small));
            children.push(separator_horizontal());
            children.push(text(format!("Position: {:?}", entity.position)));
            children.push(text(format!("Rotation: {:?}", entity.rotation)));
            children.push(text(format!("Scale: {:?}", entity.scale)));
            children.push(separator_horizontal());

            // Type
            children.push(text("Type").font_size(FontSize::Small));
            children.push(text(&entity.entity_type));
            children.push(separator_horizontal());

            // AudioSource
            if let Some(ref src) = entity.audio_source {
                children.push(text("Audio Source").font_size(FontSize::Small));
                children.push(separator_horizontal());
                children.push(text(format!("Path: {}", src.path)));
                if let Some(sr) = src.sample_rate {
                    children.push(text(format!("Sample Rate: {} Hz", sr)));
                }
                if let Some(ch) = src.channels {
                    children.push(text(format!("Channels: {}", ch)));
                }
                if let Some(dur) = src.duration_secs {
                    children.push(text(format!("Duration: {:.2}s", dur)));
                }
                let path_clone = src.path.clone();
                let play_btn = button("▶ Play Preview").on_click(ctx.on_click(move |actions| {
                    actions.emit(
                        crate::ui::editor_ui::types::EditorAction::AudioPreviewToggle {
                            path: PathBuf::from(&path_clone),
                        },
                    );
                }));
                children.push(play_btn);
                children.push(separator_horizontal());
            }

            // AudioListener
            if entity.has_audio_listener {
                children.push(text("Audio Listener").font_size(FontSize::Small));
                children.push(separator_horizontal());
                children.push(text("Active listener"));
                if draw_ctx.audio_listener_count > 1 {
                    children.push(
                        text(format!(
                            "⚠ {} listeners in scene",
                            draw_ctx.audio_listener_count
                        ))
                        .color(draw_ctx.theme.warning),
                    );
                }
                children.push(separator_horizontal());
            }

            vstack(children)
        } else {
            text("No entity selected").color(draw_ctx.theme.text_muted)
        };

        let panel_content = scroll(content, scroll_id);

        panel(header_text, panel_content).header_height(24.0)
    }
}
