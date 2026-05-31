use katla_ecs::EntityId;
use katla_math::Rect2D;
use katla_ui::FontSize;
use katla_ui::declarative::{Build, BuildContext, Padding, StateId, ViewDescriptor};

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
}

pub(crate) struct HierarchyView;

impl Build for HierarchyView {
    fn build(&self, ctx: &mut BuildContext) -> ViewDescriptor {
        use katla_ui::declarative::{hstack, panel, scroll, text, textfield, vstack};

        let draw_ctx = ctx.env::<HierarchyDrawCtx>().cloned();
        let Some(draw_ctx) = draw_ctx else {
            return ViewDescriptor::Empty;
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

        // Search field
        let search_field = textfield("Filter entities...", search_id);

        // Tree view - build a list of entity rows
        let mut tree_children = Vec::new();
        for entity in filtered_entities.iter() {
            let badge_color = match entity.entity_type.as_str() {
                "Mesh" => draw_ctx.theme.entity_mesh,
                "Particle Emitter" => draw_ctx.theme.entity_particle,
                "Directional Light" | "Point Light" => draw_ctx.theme.entity_light,
                _ => draw_ctx.theme.entity_empty,
            };

            let badge_text = &entity.entity_type;

            // Build a row with type badge and name
            let children = vec![
                text(format!("{} ", badge_text))
                    .color(badge_color)
                    .font_size(FontSize::XSmall),
                text(&entity.name).color(draw_ctx.theme.text_secondary),
            ];

            tree_children.push(hstack(children).spacing(8.0).padding_all(2.0));
        }

        let tree_content = if tree_children.is_empty() {
            text("No entities in scene").color(draw_ctx.theme.text_muted)
        } else {
            vstack(tree_children)
        };

        let content = vstack([search_field, scroll(tree_content, scroll_id).flex_grow(1.0)])
            .spacing(4.0)
            .padding(Padding::all(4.0))
            .flex_grow(1.0);

        panel(header_text, content)
            .flex_width(draw_ctx.bounds.width())
            .flex_height(draw_ctx.bounds.height())
    }
}
