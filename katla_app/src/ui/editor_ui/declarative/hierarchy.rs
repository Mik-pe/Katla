use katla_ecs::EntityId;
use katla_math::{Color, Rect2D, Vec2};
use katla_ui::declarative::{Alignment, Build, BuildContext, Padding, StateId, ViewDescriptor};
use katla_ui::widgets::TextInput;
use katla_ui::{FontSize, ForkAwesome};

use crate::ui::editor_ui::ColorScheme;
use crate::ui::editor_ui::types::{
    EditorAction, EntityInfo, HierarchyState, is_entity_visible_fast,
};

/// Environment data injected before each frame for the hierarchy panel.
#[derive(Clone)]
pub(crate) struct HierarchyDrawCtx {
    pub bounds: Rect2D,
    pub entities: Vec<EntityInfo>,
    pub selected_entity: Option<EntityId>,
    pub hierarchy_state: HierarchyState,
    pub theme: ColorScheme,
    pub pending_actions: Vec<EditorAction>,
    pub search_filter: String,
}

/// Actions emitted by the hierarchy panel to sync state back to the application.
#[derive(Clone, Debug)]
pub(crate) struct HierarchySync {
    pub expanded_entities: Vec<EntityId>,
    pub selected_entity: Option<EntityId>,
    pub search_filter: String,
    pub pending_actions: Vec<EditorAction>,
    pub context_entity: Option<EntityId>,
    pub context_menu_open: bool,
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

        let content = vstack([search_field, scroll(tree_content, scroll_id)])
            .spacing(4.0)
            .padding(Padding::all(4.0));

        panel(header_text, content).header_height(24.0)
    }
}
