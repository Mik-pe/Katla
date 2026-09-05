use std::boxed::Box;
use std::collections::HashMap;

use katla_ecs::EntityId;
use katla_math::Rect2D;
use katla_ui::FontSize;
use katla_ui::declarative::{
    Alignment, Build, BuildContext, Padding, StateId, Widget, WidgetBox, empty, hstack, icon,
    panel_body, scroll, selectable, text, textfield, vstack, zstack,
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
    ToggleExpanded(EntityId),
}

pub(crate) struct HierarchyView;

/// Chevron/indent column width per nesting level.
const LEVEL_INDENT: f32 = katla_ui::tokens::TREE_INDENT;
const CHEVRON_SLOT: f32 = 12.0;

impl Build for HierarchyView {
    fn build(&self, ctx: &mut BuildContext) -> Box<dyn Widget> {
        let draw_ctx = ctx.env::<HierarchyDrawCtx>().cloned();
        let Some(draw_ctx) = draw_ctx else {
            return empty().boxed();
        };

        let search_id: StateId = ctx.state(draw_ctx.search_filter.clone());
        let scroll_id: StateId = ctx.state(0.0f32);
        let search_filter: String = ctx
            .get_state(search_id)
            .unwrap_or_else(|| draw_ctx.search_filter.clone());

        let filtered_entities: Vec<&EntityInfo> = if search_filter.is_empty() {
            draw_ctx.entities.iter().collect()
        } else {
            let filter_lower = search_filter.to_lowercase();
            draw_ctx
                .entities
                .iter()
                .filter(|e| e.name.to_lowercase().contains(&filter_lower))
                .collect()
        };

        let parent_map: HashMap<EntityId, Option<EntityId>> = draw_ctx
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

        let search_field = textfield("Search entities...", search_id)
            .flex_width((draw_ctx.bounds.width() - 16.0).max(0.0))
            .flex_shrink(0.0)
            .boxed();

        let display_names = build_display_names(&filtered_entities);

        let mut tree_children = Vec::new();
        for (i, entity) in filtered_entities.iter().enumerate() {
            let is_selected = draw_ctx.selected_entity == Some(entity.id);
            let is_expanded = draw_ctx
                .hierarchy_state
                .expanded_entities
                .contains(&entity.id);
            let depth = ancestry_depth(entity.id, &parent_map);
            let has_children = filtered_entities
                .iter()
                .any(|e| e.parent_id == Some(entity.id));

            let (entity_icon, icon_color) =
                entity_icon_for_name(&entity.name, &entity.entity_type, &draw_ctx.theme);

            let entity_id = entity.id;
            let display_name = &display_names[i];
            let name_color = if is_selected {
                draw_ctx.theme.text_primary
            } else {
                draw_ctx.theme.text_secondary
            };

            let row = hstack([
                // Indent rail: one fixed slot per nesting level.
                spacer(LEVEL_INDENT * depth as f32),
                // Disclosure chevron (empty slot keeps icon column aligned).
                if has_children {
                    chevron(is_expanded, draw_ctx.theme.text_muted)
                } else {
                    spacer(CHEVRON_SLOT)
                },
                icon(entity_icon).color(icon_color).boxed(),
                text(display_name)
                    .color(name_color)
                    .font_size(FontSize::Small)
                    .boxed(),
            ])
            .spacing(6.0)
            .align(Alignment::Middle)
            .flex_height(katla_ui::tokens::TREE_ROW_HEIGHT);

            tree_children.push(
                selectable(row.boxed())
                    .selected(is_selected)
                    .on_click(ctx.on_click(move |actions| {
                        // Expand/select are one gesture; the engine keeps a
                        // flat ordered list so children follow on reveal.
                        if has_children {
                            actions.emit(HierarchyAction::ToggleExpanded(entity_id));
                        }
                        actions.emit(HierarchyAction::SelectEntity(entity_id));
                    }))
                    .boxed(),
            );
        }

        let tree_content = if tree_children.is_empty() {
            text(if visible_count == 0 && !search_filter.is_empty() {
                "No matching entities"
            } else {
                "No entities in scene"
            })
            .color(draw_ctx.theme.text_muted)
            .font_size(FontSize::Small)
            .boxed()
        } else {
            vstack(tree_children).spacing(2.0).flex_grow(1.0).boxed()
        };

        let content = vstack([
            search_field,
            scroll(tree_content, scroll_id).flex_grow(1.0).boxed(),
        ])
        .spacing(8.0)
        .padding(Padding::all(8.0))
        .flex_grow(1.0)
        .flex_min_height(0.0)
        .boxed();

        panel_body(content)
            .flex_width(draw_ctx.bounds.width())
            .flex_height(draw_ctx.bounds.height())
            .boxed()
    }
}

fn spacer(width: f32) -> Box<dyn Widget> {
    // Width-only: a zero-height spacer is harmless inside a Middle-aligned
    // row, whereas a fixed height would inflate every row that contains one.
    zstack(Vec::new()).flex_width(width).boxed()
}

fn chevron(expanded: bool, color: katla_math::Color) -> Box<dyn Widget> {
    let glyph = if expanded {
        katla_ui::ForkAwesome::ANGLE_DOWN
    } else {
        katla_ui::ForkAwesome::ANGLE_RIGHT
    };
    icon(glyph).color(color).icon_size(FontSize::XSmall).boxed()
}

/// Number of ancestors above `id` (0 = root level).
fn ancestry_depth(id: EntityId, parent_map: &HashMap<EntityId, Option<EntityId>>) -> usize {
    let mut depth = 0;
    let mut current = parent_map.get(&id).copied().flatten();
    while let Some(parent) = current {
        depth += 1;
        current = parent_map.get(&parent).copied().flatten();
    }
    depth
}

/// Build display names with auto-numbering for duplicates (e.g. "Sphere.001", "Sphere.002").
fn build_display_names(entities: &[&EntityInfo]) -> Vec<String> {
    let mut name_count: HashMap<String, usize> = HashMap::new();
    for entity in entities {
        *name_count.entry(entity.name.clone()).or_default() += 1;
    }

    let mut display_names = Vec::with_capacity(entities.len());
    let mut name_seen: HashMap<String, usize> = HashMap::new();
    for entity in entities {
        let total = name_count[&entity.name];
        if total > 1 {
            let seen = name_seen.entry(entity.name.clone()).or_insert(0);
            *seen += 1;
            display_names.push(format!("{}.{:03}", entity.name, *seen));
        } else {
            display_names.push(entity.name.clone());
        }
    }
    display_names
}

/// Determine icon and color based on entity name and type.
fn entity_icon_for_name(
    name: &str,
    entity_type: &str,
    theme: &ColorScheme,
) -> (char, katla_math::Color) {
    match entity_type {
        "Particle Emitter" => (katla_ui::ForkAwesome::FIRE, theme.entity_particle),
        "Directional Light" => (katla_ui::ForkAwesome::SUN, theme.entity_light),
        "Point Light" => (katla_ui::ForkAwesome::LIGHTBULB, theme.entity_light),
        "Audio Source" | "AudioListener" => (katla_ui::ForkAwesome::VOLUME_UP, theme.highlight),
        "Camera" | "PerspectiveCamera" => (katla_ui::ForkAwesome::CAMERA, theme.highlight),
        "Mesh" => mesh_icon_for_name(name, theme),
        _ => (katla_ui::ForkAwesome::CIRCLE_OUTLINE, theme.entity_empty),
    }
}

/// Pick a per-shape icon for mesh entities based on their name.
fn mesh_icon_for_name(name: &str, theme: &ColorScheme) -> (char, katla_math::Color) {
    let lower = name.to_lowercase();
    if lower.contains("sphere") {
        (katla_ui::ForkAwesome::CIRCLE, theme.entity_mesh)
    } else if lower.contains("cylinder") {
        (katla_ui::ForkAwesome::CUBE, theme.entity_mesh)
    } else if lower.contains("plane") || lower.contains("ground") || lower.contains("floor") {
        (katla_ui::ForkAwesome::SQUARE_OUTLINE, theme.entity_mesh)
    } else if lower.contains("torus") {
        (katla_ui::ForkAwesome::CIRCLE_OUTLINE, theme.entity_mesh)
    } else if lower.contains("light") || lower.contains("lamp") {
        (katla_ui::ForkAwesome::LIGHTBULB, theme.entity_light)
    } else if lower.contains("camera") {
        (katla_ui::ForkAwesome::CAMERA, theme.highlight)
    } else {
        (katla_ui::ForkAwesome::SQUARE, theme.entity_mesh)
    }
}
