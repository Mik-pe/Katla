use std::boxed::Box;
use std::path::PathBuf;

use katla_ecs::EntityId;
use katla_math::Rect2D;
use katla_ui::FontSize;
use katla_ui::declarative::{
    Build, BuildContext, Padding, StateId, Widget, WidgetBox, button, empty, panel, property_row,
    scroll, section, text, vstack,
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
            let mut sections: Vec<Box<dyn Widget>> = Vec::new();

            // Transform section (collapsible)
            let transform_expanded_id: StateId = ctx.state(true);
            let transform_content = vstack([
                property_row(
                    "Position",
                    format!(
                        "{:.2}, {:.2}, {:.2}",
                        entity.position.x(),
                        entity.position.y(),
                        entity.position.z()
                    ),
                )
                .boxed(),
                property_row(
                    "Rotation",
                    format!(
                        "{:.2}, {:.2}, {:.2}",
                        entity.rotation.x(),
                        entity.rotation.y(),
                        entity.rotation.z()
                    ),
                )
                .boxed(),
                property_row(
                    "Scale",
                    format!(
                        "{:.2}, {:.2}, {:.2}",
                        entity.scale.x(),
                        entity.scale.y(),
                        entity.scale.z()
                    ),
                )
                .boxed(),
            ])
            .spacing(4.0)
            .boxed();
            sections.push(section("Transform", transform_content, transform_expanded_id).boxed());

            // Type section (collapsible)
            let type_expanded_id: StateId = ctx.state(true);
            let type_content = vstack([property_row("Type", &entity.entity_type).boxed()])
                .spacing(4.0)
                .boxed();
            sections.push(section("Type", type_content, type_expanded_id).boxed());

            // AudioSource section (collapsible)
            if let Some(ref src) = entity.audio_source {
                let audio_expanded_id: StateId = ctx.state(true);
                let mut audio_children: Vec<Box<dyn Widget>> = Vec::new();
                audio_children.push(property_row("Path", &src.path).boxed());
                if let Some(sr) = src.sample_rate {
                    audio_children.push(property_row("Sample Rate", format!("{} Hz", sr)).boxed());
                }
                if let Some(ch) = src.channels {
                    audio_children.push(property_row("Channels", ch.to_string()).boxed());
                }
                if let Some(dur) = src.duration_secs {
                    audio_children.push(property_row("Duration", format!("{:.2}s", dur)).boxed());
                }
                let path_clone = src.path.clone();
                audio_children.push(
                    button("▶ Play Preview")
                        .on_click(ctx.on_click(move |actions| {
                            actions.emit(
                                crate::ui::editor_ui::types::EditorAction::AudioPreviewToggle {
                                    path: PathBuf::from(&path_clone),
                                },
                            );
                        }))
                        .boxed(),
                );
                sections.push(
                    section(
                        "Audio Source",
                        vstack(audio_children).spacing(4.0).boxed(),
                        audio_expanded_id,
                    )
                    .boxed(),
                );
            }

            // AudioListener section (collapsible)
            if entity.has_audio_listener {
                let listener_expanded_id: StateId = ctx.state(true);
                let mut listener_children: Vec<Box<dyn Widget>> = Vec::new();
                listener_children.push(text("Active listener").boxed());
                if draw_ctx.audio_listener_count > 1 {
                    listener_children.push(
                        text(format!(
                            "⚠ {} listeners in scene",
                            draw_ctx.audio_listener_count
                        ))
                        .color(draw_ctx.theme.warning)
                        .boxed(),
                    );
                }
                sections.push(
                    section(
                        "Audio Listener",
                        vstack(listener_children).spacing(4.0).boxed(),
                        listener_expanded_id,
                    )
                    .boxed(),
                );
            }

            vstack(sections)
                .spacing(2.0)
                .padding(Padding::all(12.0))
                .boxed()
        } else {
            vstack([text("Select an object to inspect")
                .color(draw_ctx.theme.text_muted)
                .font_size(FontSize::Small)
                .boxed()])
            .padding(Padding::all(12.0))
            .boxed()
        };

        let panel_content = scroll(content, scroll_id).flex_grow(1.0).boxed();

        panel(header_text, panel_content)
            .flex_width(draw_ctx.bounds.width())
            .flex_height(draw_ctx.bounds.height())
            .boxed()
    }
}
