use std::cell::RefCell;

use katla_ecs::EntityId;
use katla_math::{Rect2D, Vec2};
use katla_ui::declarative::{Build, BuildContext, ViewDescriptor};
use katla_ui::widgets::{Panel, RowInfo, TextInput, TreeItem, TreeState, TreeView};
use katla_ui::{FontId, FontSize, ForkAwesome, Response, UiContext};

use crate::ui::editor_ui::ColorScheme;
use crate::ui::editor_ui::types::{
    EditorAction, EntityInfo, HierarchyState, is_entity_visible_fast,
};

thread_local! {
    static HIERARCHY_CTX: RefCell<Option<HierarchyDrawCtx>> = const { RefCell::new(None) };
}

pub(crate) struct HierarchyDrawCtx {
    pub bounds: Rect2D,
    pub entities: Vec<EntityInfo>,
    pub selected_entity: Option<EntityId>,
    pub hierarchy_state: HierarchyState,
    pub theme: ColorScheme,
    pub pending_actions: Vec<EditorAction>,
    pub search_filter: String,
}

pub(crate) fn set_hierarchy_ctx(ctx: HierarchyDrawCtx) {
    HIERARCHY_CTX.with(|c| *c.borrow_mut() = Some(ctx));
}

pub(crate) fn take_hierarchy_ctx() -> Option<HierarchyDrawCtx> {
    HIERARCHY_CTX.with(|c| c.borrow_mut().take())
}

pub(crate) struct HierarchyView;

impl Build for HierarchyView {
    fn build(&self, _ctx: &mut BuildContext) -> ViewDescriptor {
        ViewDescriptor::Custom(draw_hierarchy)
    }
}

fn draw_hierarchy(ui: &mut UiContext, _bounds: Rect2D) {
    let mut ctx = match take_hierarchy_ctx() {
        Some(ctx) => ctx,
        None => return,
    };

    let search_field_height = 26.0;
    let search_margin = ui.style().item_inner_spacing;

    let filtered_entities: Vec<&EntityInfo> = if ctx.search_filter.is_empty() {
        ctx.entities.iter().collect()
    } else {
        let filter_lower = ctx.search_filter.to_lowercase();
        ctx.entities
            .iter()
            .filter(|e| e.name.to_lowercase().contains(&filter_lower))
            .collect()
    };

    let parent_map: std::collections::HashMap<EntityId, Option<EntityId>> =
        ctx.entities.iter().map(|e| (e.id, e.parent_id)).collect();

    let visible_count = filtered_entities
        .iter()
        .filter(|e| is_entity_visible_fast(e, &parent_map, &ctx.hierarchy_state.expanded_entities))
        .count();

    let header_text = format!("Hierarchy ({} entities)", visible_count);
    let content_bounds = {
        let guard = Panel::new(&header_text)
            .bounds(ctx.bounds)
            .header_height(24.0)
            .show(ui);
        guard.content_bounds()
    };

    let search_bounds = Rect2D::from_origin_size(
        Vec2::new(
            content_bounds.min.x() + search_margin,
            content_bounds.min.y() + 2.0,
        ),
        Vec2::new(
            content_bounds.width() - search_margin * 2.0,
            search_field_height,
        ),
    );
    ui.add(
        TextInput::new("hierarchy_search", &mut ctx.search_filter)
            .bounds(search_bounds)
            .placeholder("Filter entities...")
            .show_clear(true),
    );

    let tree_bounds = Rect2D::from_origin_size(
        Vec2::new(
            content_bounds.min.x(),
            content_bounds.min.y() + search_field_height + search_margin,
        ),
        Vec2::new(
            content_bounds.width(),
            (content_bounds.height() - search_field_height - search_margin).max(0.0),
        ),
    );

    let entities = &ctx.entities;
    let theme = &ctx.theme;
    let bounds_width = ctx.bounds.width();

    let items: Vec<TreeItem> = filtered_entities
        .iter()
        .map(|e| TreeItem {
            id: e.id.id(),
            label: e.name.clone(),
            depth: e.depth,
            has_children: e.has_children,
        })
        .collect();

    let mut tree_state = TreeState {
        expanded: ctx
            .hierarchy_state
            .expanded_entities
            .iter()
            .map(|id| id.id())
            .collect(),
        selected: ctx.selected_entity.map(|id| id.id()),
        scroll_offset: ctx.hierarchy_state.scroll_state.scroll_offset,
        content_height: ctx.hierarchy_state.scroll_state.content_height,
    };

    let response = if !items.is_empty() {
        let entities_clone = entities.clone();
        let theme_clone = theme.clone();

        ui.add(
            TreeView::new("hierarchy_tree", &mut tree_state)
                .bounds(tree_bounds)
                .data(items)
                .row_height(22.0)
                .indent_per_level(16.0)
                .render_item(move |ui: &mut UiContext, item: &TreeItem, info: &RowInfo| {
                    let entity = entities_clone
                        .iter()
                        .find(|e| e.id.id() == item.id)
                        .expect("TreeItem id must correspond to an EntityInfo");

                    if entity.depth > 0 {
                        let line_x = info.bounds.min.x() - 8.0;
                        ui.draw_line(
                            Vec2::new(line_x, info.bounds.min.y()),
                            Vec2::new(line_x, info.bounds.max.y()),
                            theme_clone.separator,
                            1.0,
                        );
                    }

                    let entity_icon = match entity.entity_type.as_str() {
                        "Mesh" => ForkAwesome::CUBE,
                        "Particle Emitter" => ForkAwesome::STAR,
                        "Directional Light" => ForkAwesome::SUN,
                        "Point Light" => ForkAwesome::LIGHTBULB,
                        "Camera" => ForkAwesome::CAMERA,
                        "Empty" => ForkAwesome::CIRCLE,
                        _ => ForkAwesome::CUBE,
                    };
                    let entity_icon_color = match entity.entity_type.as_str() {
                        "Mesh" => theme_clone.entity_mesh,
                        "Particle Emitter" => theme_clone.entity_particle,
                        "Directional Light" | "Point Light" => theme_clone.entity_light,
                        _ => theme_clone.text_secondary,
                    };

                    ui.draw_icon_aligned(
                        entity_icon,
                        Vec2::new(info.content_x, info.bounds.min.y() + 3.0),
                        ui.scaled_font_size(FontSize::Medium),
                        entity_icon_color,
                        FontId::DEFAULT,
                    );

                    let badge_color = match entity.entity_type.as_str() {
                        "Mesh" => theme_clone.entity_mesh,
                        "Particle Emitter" => theme_clone.entity_particle,
                        "Directional Light" | "Point Light" => theme_clone.entity_light,
                        _ => theme_clone.entity_empty,
                    };
                    let badge_text = &entity.entity_type;
                    let badge_size =
                        ui.measure_text(badge_text, ui.scaled_font_size(FontSize::XSmall));
                    let badge_x = info.bounds.min.x() + bounds_width
                        - badge_size.x()
                        - ui.style().panel_padding;

                    let name_x = info.content_x + ui.style().indent_spacing;
                    let name_font_size = ui.scaled_font_size(FontSize::Medium);
                    let max_name_width =
                        (badge_x - name_x - ui.style().item_inner_spacing).max(0.0);
                    let display_name =
                        ui.truncate_text(&entity.name, max_name_width, name_font_size);

                    let name_pos = Vec2::new(name_x, info.bounds.min.y() + 3.0);
                    ui.draw_text(
                        &display_name,
                        name_pos,
                        theme_clone.text_secondary,
                        name_font_size,
                    );

                    let badge_pos = Vec2::new(badge_x, info.bounds.min.y() + 5.0);
                    ui.draw_text(
                        badge_text,
                        badge_pos,
                        badge_color,
                        ui.scaled_font_size(FontSize::XSmall),
                    );
                }),
        )
    } else {
        ui.draw_empty_state(ctx.bounds, "No entities in scene");
        Response::default()
    };

    ctx.hierarchy_state.expanded_entities = tree_state
        .expanded
        .iter()
        .map(|&id| EntityId::from_raw(id))
        .collect();
    ctx.hierarchy_state.scroll_state.scroll_offset = tree_state.scroll_offset;
    ctx.hierarchy_state.scroll_state.content_height = tree_state.content_height;

    if let Some(selected_u64) = tree_state.selected {
        let new_selected = EntityId::from_raw(selected_u64);
        if ctx.selected_entity != Some(new_selected) {
            ctx.selected_entity = Some(new_selected);
            ctx.pending_actions
                .push(EditorAction::SelectEntity(new_selected));
        }
    }

    if response.right_clicked
        && let Some(selected_u64) = tree_state.selected
    {
        let entity_id = EntityId::from_raw(selected_u64);
        ctx.selected_entity = Some(entity_id);
        ctx.hierarchy_state.context_entity = Some(entity_id);
        ctx.hierarchy_state.context_menu_open = true;
    }

    let mut clicked_action: Option<&str> = None;
    ui.context_menu(
        "hierarchy_context",
        &mut ctx.hierarchy_state.context_menu_open,
        |ui, open| {
            if ui.menu_item_clicked_with_icon_and_shortcut(
                "Duplicate",
                ForkAwesome::COPY,
                true,
                "Ctrl+D",
            ) {
                clicked_action = Some("Duplicate");
                *open = false;
                return;
            }
            if ui.menu_item_clicked_with_icon_and_shortcut(
                "Rename",
                ForkAwesome::PENCIL,
                true,
                "F2",
            ) {
                clicked_action = Some("Rename");
                *open = false;
                return;
            }
            ui.menu_separator();
            if ui.menu_item_clicked_with_icon_and_shortcut(
                "Delete",
                ForkAwesome::TRASH,
                true,
                "Del",
            ) {
                clicked_action = Some("Delete");
                *open = false;
            }
        },
    );

    if !ctx.hierarchy_state.context_menu_open {
        ctx.hierarchy_state.context_entity = None;
    }

    if let Some(action) = clicked_action {
        match action {
            "Duplicate" => {
                if let Some(entity_id) = ctx.hierarchy_state.context_entity {
                    ctx.pending_actions
                        .push(EditorAction::DuplicateEntity(entity_id));
                }
            }
            "Rename" => {}
            "Delete" => {
                if let Some(entity_id) = ctx.hierarchy_state.context_entity {
                    ctx.pending_actions
                        .push(EditorAction::DeleteEntity(entity_id));
                }
            }
            _ => {}
        }
    }

    set_hierarchy_ctx(ctx);
}
