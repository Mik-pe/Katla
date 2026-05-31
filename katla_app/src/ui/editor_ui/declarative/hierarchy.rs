use std::boxed::Box;

use katla_ecs::EntityId;
use katla_math::Rect2D;
use katla_ui::FontSize;
use katla_ui::declarative::{
    Build, BuildContext, Padding, StateId, Widget, WidgetBox, empty, hstack, icon, panel, scroll,
    selectable, text, textfield, vstack,
};

use crate::ui::editor_ui::ColorScheme;
use crate::ui::editor_ui::types::{EntityInfo, HierarchyState, is_entity_visible_fast};

/// Environment data injected before each frame for the hierarchy panel.
#[derive(Clone)]
pub(crate) struct HierarchyDrawCtx {
    pub bounds: Rect2D,
    pub entities: Vec<EntityInfo>,
    pub hierarchy_state: HierarchyState,
    pub theme: ColorScheme,
    pub search_filter: String,
    pub selected_entity: Option<EntityId>,
}

/// Actions emitted by the hierarchy panel.
#[derive(Clone, Debug)]
pub(crate) enum HierarchyAction {
    SelectEntity(EntityId),
}

pub(crate) struct HierarchyView;

impl Build for HierarchyView {
    fn build(&self, ctx: &mut BuildContext) -> Box<dyn Widget> {
        let draw_ctx = ctx.env::<HierarchyDrawCtx>().cloned();
        let Some(draw_ctx) = draw_ctx else {
            return empty().boxed();
        };

        let search_id: StateId = ctx.state(draw_ctx.search_filter.clone());
        let scroll_id: StateId = ctx.state(0.0f32);

        let filtered_entities: Vec<&EntityInfo> = if draw_ctx.search_filter.is_empty() {
            draw_ctx.entities.iter().collect()
        } else {
            let filter_lower = draw_ctx.search_filter.to_lowercase();
            draw_ctx
                .entities
                .iter()
                .filter(|e| e.name.to_lowercase().contains(&filter_lower))
                .collect()
        };

        let parent_map: std::collections::HashMap<EntityId, Option<EntityId>> = draw_ctx
            .entities
            .iter()
            .map(|e| (e.id, e.parent_id))
            .collect();

        let visible_count = filtered_entities
            .iter()
            .filter(|e| {
                is_entity_visible_fast(e, &parent_map, &draw_ctx.hierarchy_state.expanded_entities)
            })
            .count();

        let header_text = format!("Hierarchy ({} entities)", visible_count);

        let search_field = textfield("Filter entities...", search_id).boxed();

        let mut tree_children = Vec::new();
        for entity in filtered_entities.iter() {
            let is_selected = draw_ctx.selected_entity == Some(entity.id);

            let (entity_icon, icon_color) =
                entity_icon_for_type(&entity.entity_type, &draw_ctx.theme);

            let entity_id = entity.id;
            let row = hstack([
                icon(entity_icon)
                    .color(icon_color)
                    .icon_size(FontSize::Small)
                    .boxed(),
                text(&entity.name)
                    .color(draw_ctx.theme.text_secondary)
                    .font_size(FontSize::Small)
                    .boxed(),
            ])
            .spacing(6.0)
            .padding(Padding::all(4.0));

            tree_children.push(
                selectable(row.boxed())
                    .selected(is_selected)
                    .on_click(ctx.on_click(move |actions| {
                        actions.emit(HierarchyAction::SelectEntity(entity_id));
                    }))
                    .boxed(),
            );
        }

        let tree_content = if tree_children.is_empty() {
            text("No entities in scene")
                .color(draw_ctx.theme.text_muted)
                .font_size(FontSize::Small)
                .boxed()
        } else {
            vstack(tree_children).spacing(2.0).boxed()
        };

        let content = vstack([
            search_field,
            scroll(tree_content, scroll_id).flex_grow(1.0).boxed(),
        ])
        .spacing(4.0)
        .padding(Padding::all(4.0))
        .flex_grow(1.0)
        .boxed();

        panel(header_text, content)
            .flex_width(draw_ctx.bounds.width())
            .flex_height(draw_ctx.bounds.height())
            .boxed()
    }
}

fn entity_icon_for_type(entity_type: &str, theme: &ColorScheme) -> (char, katla_math::Color) {
    match entity_type {
        "Mesh" => (katla_ui::ForkAwesome::CUBE, theme.entity_mesh),
        "Particle Emitter" => (katla_ui::ForkAwesome::FIRE, theme.entity_particle),
        "Directional Light" => (katla_ui::ForkAwesome::SUN, theme.entity_light),
        "Point Light" => (katla_ui::ForkAwesome::LIGHTBULB, theme.entity_light),
        "Audio Source" | "AudioListener" => (katla_ui::ForkAwesome::VOLUME_UP, theme.highlight),
        "Camera" | "PerspectiveCamera" => (katla_ui::ForkAwesome::CAMERA, theme.highlight),
        "Script" => (katla_ui::ForkAwesome::FILE_CODE, theme.success),
        _ => (katla_ui::ForkAwesome::CIRCLE_OUTLINE, theme.entity_empty),
    }
}
