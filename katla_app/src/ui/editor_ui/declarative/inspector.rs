use std::boxed::Box;
use std::path::PathBuf;

use katla_ecs::EntityId;
use katla_math::Rect2D;
use katla_ui::FontSize;
use katla_ui::declarative::{
    Alignment, Build, BuildContext, Padding, StateId, Widget, WidgetBox, button, empty, icon,
    panel_body, property_row, scroll, section, text, vstack,
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

/// Actions emitted by the inspector panel.
#[derive(Clone, Debug)]
pub(crate) enum InspectorAction {
    ToggleAddComponent,
    AddComponent { entity: EntityId, component: String },
}

impl Build for InspectorView {
    fn build(&self, ctx: &mut BuildContext) -> Box<dyn Widget> {
        let draw_ctx = ctx.env::<InspectorDrawCtx>().cloned();
        let Some(draw_ctx) = draw_ctx else {
            return empty().boxed();
        };

        let scroll_id: StateId = ctx.state(0.0f32);

        // Section expansion slots are reserved unconditionally, in a stable
        // order: every sibling view shares this view's positional state-slot
        // counter, so a slot that appears only while an entity is selected
        // would shift every later view's slots frame-to-frame (type confusion
        // and panics downstream).
        let transform_expanded_id: StateId = ctx.state(true);
        let audio_expanded_id: StateId = ctx.state(true);
        let listener_expanded_id: StateId = ctx.state(true);
        let add_component_open_id: StateId = ctx.state(false);

        let content = if let Some(entity) = draw_ctx
            .entities
            .iter()
            .find(|e| draw_ctx.selected_entity == Some(e.id))
        {
            let mut sections: Vec<Box<dyn Widget>> = Vec::new();

            // Entity identity header: name carries the hierarchy's display
            // numbering, the type sits underneath as secondary metadata.
            sections.push(
                vstack([
                    text(&entity.name)
                        .color(draw_ctx.theme.text_primary)
                        .font_size(FontSize::Medium)
                        .boxed(),
                    text(&entity.entity_type)
                        .color(draw_ctx.theme.text_secondary)
                        .font_size(FontSize::Small)
                        .boxed(),
                ])
                .spacing(2.0)
                .boxed(),
            );

            // Transform section (collapsible)
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

            // AudioSource section (collapsible)
            if let Some(ref src) = entity.audio_source {
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

            // Add Component action: a quiet expander listing the component
            // types registered with the scene tool registry.
            if !draw_ctx.available_components.is_empty() {
                let add_open: bool = ctx.get_state(add_component_open_id).unwrap_or(false);
                let mut footer: Vec<Box<dyn Widget>> = Vec::new();
                footer.push(
                    button("+ Add Component")
                        .fill(katla_math::Color::TRANSPARENT)
                        .border(draw_ctx.theme.border)
                        .on_click(ctx.on_click(move |actions| {
                            actions.emit(InspectorAction::ToggleAddComponent);
                        }))
                        .boxed(),
                );
                if add_open {
                    let entity = entity.id;
                    for component in draw_ctx.available_components.clone() {
                        footer.push(
                            button(component)
                                .fill(katla_math::Color::TRANSPARENT)
                                .border(katla_math::Color::TRANSPARENT)
                                .on_click(ctx.on_click(move |actions| {
                                    actions.emit(InspectorAction::AddComponent {
                                        entity,
                                        component: component.to_string(),
                                    });
                                }))
                                .boxed(),
                        );
                    }
                }
                sections.push(vstack(footer).spacing(4.0).boxed());
            }

            vstack(sections)
                .flex_grow(1.0)
                .spacing(12.0)
                .padding(Padding::all(12.0))
                .boxed()
        } else {
            // Intentional, quiet empty state — not an onboarding poster.
            let icon_color = draw_ctx.theme.text_muted;
            vstack([
                icon(katla_ui::ForkAwesome::CUBE)
                    .icon_size(FontSize::XLarge)
                    .color(icon_color)
                    .boxed(),
                text("No entity selected")
                    .color(draw_ctx.theme.text_secondary)
                    .font_size(FontSize::Medium)
                    .boxed(),
                text("Select an entity to inspect it.")
                    .color(draw_ctx.theme.text_muted)
                    .font_size(FontSize::Small)
                    .boxed(),
            ])
            .spacing(8.0)
            .align(Alignment::Center)
            .padding(Padding::all(24.0))
            .boxed()
        };

        let panel_content = scroll(content, scroll_id).flex_grow(1.0).boxed();

        panel_body(panel_content)
            .flex_width(draw_ctx.bounds.width())
            .flex_height(draw_ctx.bounds.height())
            .boxed()
    }
}
