use std::boxed::Box;
use std::collections::{HashMap, HashSet};

use katla_ecs::EntityId;
use katla_math::Rect2D;
use katla_ui::FontSize;
use katla_ui::declarative::{
    Alignment, Build, BuildContext, Padding, StateId, Widget, WidgetBox, empty, hstack, icon,
    panel_body, scroll, selectable, text, textfield, vstack, zstack,
};

use crate::ui::editor_ui::ColorScheme;
use crate::ui::editor_ui::types::{EntityInfo, HierarchyState};

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
        let last_selected_id: StateId = ctx.state(Option::<EntityId>::None);
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

        let rows = prepare_hierarchy_rows(
            &draw_ctx.entities,
            &filtered_entities,
            &draw_ctx.hierarchy_state.expanded_entities,
        );
        let visible_count = rows
            .iter()
            .filter(|row| !search_filter.is_empty() || row.visible)
            .count();

        // Auto-reveal: when the selection changes outside this panel (e.g. a
        // viewport pick), scroll the selected row into view. Clicking a row
        // that is already visible keeps the offset untouched.
        if draw_ctx.selected_entity != ctx.get_state(last_selected_id) {
            ctx.set_state(last_selected_id, draw_ctx.selected_entity);
            if let Some(selected_id) = draw_ctx.selected_entity {
                let row_pitch = katla_ui::tokens::TREE_ROW_HEIGHT + 2.0;
                let mut selected_row: Option<f32> = None;
                let mut visible_rows = 0.0f32;
                for (entity, row) in filtered_entities.iter().zip(&rows) {
                    if !search_filter.is_empty() || row.visible {
                        if entity.id == selected_id {
                            selected_row = Some(visible_rows);
                        }
                        visible_rows += 1.0;
                    }
                }
                if let Some(row_index) = selected_row {
                    let view_height = (draw_ctx.bounds.height() - 52.0).max(0.0);
                    let row_y = row_index * row_pitch;
                    let offset: f32 = ctx.get_state(scroll_id).unwrap_or_default();
                    if row_y < offset || row_y + row_pitch > offset + view_height {
                        let content_height = visible_rows * row_pitch;
                        let max_offset = (content_height - view_height).max(0.0);
                        let target =
                            (row_y - (view_height - row_pitch) * 0.5).clamp(0.0, max_offset);
                        ctx.set_state(scroll_id, target);
                    }
                }
            }
        }

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
            let depth = rows[i].depth;
            let has_children = rows[i].has_children;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Ancestry {
    depth: usize,
    visible: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct HierarchyRow {
    depth: usize,
    visible: bool,
    has_children: bool,
}

/// Resolves each parent link once, retaining the filtered list's row order.
fn prepare_hierarchy_rows(
    entities: &[EntityInfo],
    filtered_entities: &[&EntityInfo],
    expanded: &HashSet<EntityId>,
) -> Vec<HierarchyRow> {
    if filtered_entities
        .iter()
        .all(|entity| entity.parent_id.is_none())
    {
        return vec![
            HierarchyRow {
                depth: 0,
                visible: true,
                has_children: false,
            };
            filtered_entities.len()
        ];
    }
    let parents: HashMap<_, _> = entities.iter().map(|e| (e.id, e.parent_id)).collect();
    let parents_with_children: HashSet<_> = filtered_entities
        .iter()
        .filter_map(|e| e.parent_id)
        .collect();
    let mut ancestry = HashMap::<EntityId, Ancestry>::with_capacity(entities.len());
    let mut path = Vec::new();
    let mut path_indices = HashMap::new();

    filtered_entities
        .iter()
        .map(|entity| {
            let mut current = entity.id;
            while !ancestry.contains_key(&current) {
                if let Some(&cycle_start) = path_indices.get(&current) {
                    let cycle_depth = path.len() - cycle_start;
                    // Cycles cannot be visible. Their length gives every member
                    // the same bounded depth, regardless of traversal order.
                    for &id in &path[cycle_start..] {
                        path_indices.remove(&id);
                        ancestry.insert(
                            id,
                            Ancestry {
                                depth: cycle_depth,
                                visible: false,
                            },
                        );
                    }
                    path.truncate(cycle_start);
                    break;
                }
                let Some(parent) = parents.get(&current).copied().flatten() else {
                    ancestry.insert(
                        current,
                        Ancestry {
                            depth: 0,
                            visible: true,
                        },
                    );
                    break;
                };
                path_indices.insert(current, path.len());
                path.push(current);
                current = parent;
            }
            for id in path.drain(..).rev() {
                path_indices.remove(&id);
                if let Some(parent) = parents.get(&id).copied().flatten()
                    && let Some(parent_ancestry) = ancestry.get(&parent).copied()
                {
                    ancestry.insert(
                        id,
                        Ancestry {
                            depth: parent_ancestry.depth + 1,
                            visible: parent_ancestry.visible && expanded.contains(&parent),
                        },
                    );
                }
            }
            let resolved = ancestry[&entity.id];
            HierarchyRow {
                depth: resolved.depth,
                visible: resolved.visible,
                has_children: parents_with_children.contains(&entity.id),
            }
        })
        .collect()
}

/// Build display names with auto-numbering for duplicates (e.g. "Sphere.001", "Sphere.002").
fn build_display_names(entities: &[&EntityInfo]) -> Vec<String> {
    let mut name_count: HashMap<&str, usize> = HashMap::new();
    for entity in entities {
        *name_count.entry(entity.name.as_str()).or_default() += 1;
    }

    let mut display_names = Vec::with_capacity(entities.len());
    let mut name_seen: HashMap<&str, usize> = HashMap::new();
    for entity in entities {
        let total = name_count[entity.name.as_str()];
        if total > 1 {
            let seen = name_seen.entry(entity.name.as_str()).or_insert(0);
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
        (katla_ui::ForkAwesome::CIRCLE_OUTLINE, theme.entity_mesh)
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
        (katla_ui::ForkAwesome::CUBE, theme.entity_mesh)
    }
}

#[cfg(test)]
#[path = "hierarchy_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "hierarchy_bench.rs"]
mod benchmarks;
