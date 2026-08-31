use std::any::Any;

use katla_math::{Rect2D, Vec2};
use taffy::{Dimension, Size as TaffySize, Style};

use super::super::animation::AnimationState;
use super::super::descriptor::FlexProps;
use super::super::diff::DiffAction;
use super::super::state::{StateArena, StateId, ViewId};
use super::super::widget::{ChildWidgets, DrawInfo, InputContext, InputResult, MeasureFn, Widget};
use crate::context::UiContext;
use crate::dock::{DockNode, DockPath, DockTree, DockZone, SplitDirection};
use crate::input::mouse_button;

const TAB_DRAG_THRESHOLD: f32 = 3.0;

/// Actions emitted by DockSpace through ActionStream.
#[derive(Debug, Clone, PartialEq)]
pub enum DockAction<T: Clone + PartialEq> {
    TabMoved {
        from_path: DockPath,
        to_path: DockPath,
        zone: DockZone,
        tab: T,
    },
    TabClosed {
        path: DockPath,
        tab: T,
    },
    SplitResized {
        path: DockPath,
        ratio: f32,
    },
    TabActivated {
        path: DockPath,
        tab: T,
    },
}

/// Drag state for tab and splitter drag operations, stored in StateArena.
#[derive(Debug, Clone, PartialEq)]
pub struct DockDragState<T: Clone + PartialEq> {
    pub dragging: bool,
    pub drag_started: bool,
    pub source_is_splitter: bool,
    pub source_path: DockPath,
    pub source_tab: T,
    pub drag_start: Vec2,
    pub drag_pos: Vec2,
    pub target_zone: Option<DockZone>,
    pub target_path: Option<DockPath>,
}

impl<T: Clone + PartialEq + Default> Default for DockDragState<T> {
    fn default() -> Self {
        Self {
            dragging: false,
            drag_started: false,
            source_is_splitter: false,
            source_path: DockPath::root(),
            source_tab: T::default(),
            drag_start: Vec2::ZERO,
            drag_pos: Vec2::ZERO,
            target_zone: None,
            target_path: None,
        }
    }
}

/// Information about a leaf node computed during layout.
pub struct LeafInfo<T: Clone + PartialEq> {
    pub path: DockPath,
    pub full_bounds: Rect2D,
    pub content_bounds: Rect2D,
    pub tabs: Vec<T>,
    pub active: usize,
}

/// Information about a splitter handle.
pub struct SplitInfo {
    pub path: DockPath,
    pub direction: SplitDirection,
    pub node_bounds: Rect2D,
    pub handle_rect: Rect2D,
    /// The 1 px visual line centered inside [`Self::handle_rect`].
    pub line_rect: Rect2D,
}

/// Determine which DockZone a position falls within, relative to a leaf area.
pub fn zone_from_pos(leaf_area: Rect2D, pos: Vec2) -> DockZone {
    if !leaf_area.contains(pos) {
        return DockZone::Center;
    }

    let center = leaf_area.center();
    let half_w = leaf_area.width() * 0.25;
    let half_h = leaf_area.height() * 0.25;

    let center_rect = Rect2D::new(
        Vec2::new(center.x() - half_w, center.y() - half_h),
        Vec2::new(center.x() + half_w, center.y() + half_h),
    );

    if center_rect.contains(pos) {
        return DockZone::Center;
    }

    let dx = pos.x() - center.x();
    let dy = pos.y() - center.y();

    if dx.abs() > dy.abs() {
        if dx < 0.0 {
            DockZone::Left
        } else {
            DockZone::Right
        }
    } else if dy < 0.0 {
        DockZone::Top
    } else {
        DockZone::Bottom
    }
}

/// Hit/draw width of one dock tab. Multi-tab strips stack capped-width tabs
/// from the left so they read as one tab group; a single-tab leaf fills the
/// strip and doubles as the panel header. Draw and hit-testing MUST share
/// this function so the visible tabs are exactly the clickable tabs.
fn tab_hit_width(bar_width: f32, tab_count: usize) -> f32 {
    if tab_count <= 1 {
        bar_width
    } else {
        (bar_width / tab_count as f32).min(crate::tokens::TAB_MAX_WIDTH)
    }
}

/// Compute leaf information for a DockNode tree within the given area.
pub fn compute_leaf_info<T: Clone + PartialEq>(
    node: &DockNode<T>,
    area: Rect2D,
    tab_bar_height: f32,
) -> Vec<LeafInfo<T>> {
    compute_leaf_info_recursive(node, area, tab_bar_height, &mut DockPath::root())
}

fn compute_leaf_info_recursive<T: Clone + PartialEq>(
    node: &DockNode<T>,
    area: Rect2D,
    tab_bar_height: f32,
    path: &mut DockPath,
) -> Vec<LeafInfo<T>> {
    match node {
        DockNode::Empty => {
            let content_bounds = Rect2D::new(area.min + Vec2::new(0.0, tab_bar_height), area.max);
            vec![LeafInfo {
                path: path.clone(),
                full_bounds: area,
                content_bounds,
                tabs: vec![],
                active: 0,
            }]
        }
        DockNode::Leaf { tabs, active } => {
            let content_bounds = if tabs.is_empty() {
                area
            } else {
                Rect2D::new(area.min + Vec2::new(0.0, tab_bar_height), area.max)
            };
            vec![LeafInfo {
                path: path.clone(),
                full_bounds: area,
                content_bounds,
                tabs: tabs.clone(),
                active: *active,
            }]
        }
        DockNode::Split {
            direction,
            ratio,
            children,
        } => {
            let clamped = ratio.clamp(0.0, 1.0);
            let (area0, area1) = split_area(area, *direction, clamped);

            let mut result = Vec::new();
            path.push(0);
            result.extend(compute_leaf_info_recursive(
                &children[0],
                area0,
                tab_bar_height,
                path,
            ));
            path.pop();

            path.push(1);
            result.extend(compute_leaf_info_recursive(
                &children[1],
                area1,
                tab_bar_height,
                path,
            ));
            path.pop();

            result
        }
    }
}

/// Compute splitter handle rectangles for a DockNode tree.
pub fn compute_split_info<T: Clone + PartialEq>(
    node: &DockNode<T>,
    area: Rect2D,
    splitter_width: f32,
) -> Vec<SplitInfo> {
    compute_split_info_recursive(node, area, splitter_width, &mut DockPath::root())
}

fn compute_split_info_recursive<T: Clone + PartialEq>(
    node: &DockNode<T>,
    area: Rect2D,
    splitter_width: f32,
    path: &mut DockPath,
) -> Vec<SplitInfo> {
    match node {
        DockNode::Split {
            direction,
            ratio,
            children,
        } => {
            let clamped = ratio.clamp(0.0, 1.0);
            let handle_rect = split_handle_rect(area, *direction, clamped, splitter_width);
            let (area0, area1) = split_area(area, *direction, clamped);
            let line_rect =
                split_line_rect(handle_rect, *direction, crate::tokens::SPLITTER_LINE_WIDTH);

            let mut result = vec![SplitInfo {
                path: path.clone(),
                direction: *direction,
                node_bounds: area,
                handle_rect,
                line_rect,
            }];

            path.push(0);
            result.extend(compute_split_info_recursive(
                &children[0],
                area0,
                splitter_width,
                path,
            ));
            path.pop();

            path.push(1);
            result.extend(compute_split_info_recursive(
                &children[1],
                area1,
                splitter_width,
                path,
            ));
            path.pop();

            result
        }
        _ => vec![],
    }
}

fn split_area(area: Rect2D, direction: SplitDirection, ratio: f32) -> (Rect2D, Rect2D) {
    match direction {
        SplitDirection::Horizontal => {
            let split_x = area.min.x() + area.width() * ratio;
            let area0 = Rect2D::new(area.min, Vec2::new(split_x, area.max.y()));
            let area1 = Rect2D::new(Vec2::new(split_x, area.min.y()), area.max);
            (area0, area1)
        }
        SplitDirection::Vertical => {
            let split_y = area.min.y() + area.height() * ratio;
            let area0 = Rect2D::new(area.min, Vec2::new(area.max.x(), split_y));
            let area1 = Rect2D::new(Vec2::new(area.min.x(), split_y), area.max);
            (area0, area1)
        }
    }
}

fn split_handle_rect(
    area: Rect2D,
    direction: SplitDirection,
    ratio: f32,
    splitter_width: f32,
) -> Rect2D {
    let half = splitter_width * 0.5;
    match direction {
        SplitDirection::Horizontal => {
            let split_x = area.min.x() + area.width() * ratio;
            Rect2D::new(
                Vec2::new(split_x - half, area.min.y()),
                Vec2::new(split_x + half, area.max.y()),
            )
        }
        SplitDirection::Vertical => {
            let split_y = area.min.y() + area.height() * ratio;
            Rect2D::new(
                Vec2::new(area.min.x(), split_y - half),
                Vec2::new(area.max.x(), split_y + half),
            )
        }
    }
}

/// The thin visual line drawn inside a splitter's (wider) hit rect.
fn split_line_rect(handle_rect: Rect2D, direction: SplitDirection, line_width: f32) -> Rect2D {
    let half = line_width * 0.5;
    let center = handle_rect.center();
    match direction {
        SplitDirection::Horizontal => Rect2D::new(
            Vec2::new(center.x() - half, handle_rect.min.y()),
            Vec2::new(center.x() + half, handle_rect.max.y()),
        ),
        SplitDirection::Vertical => Rect2D::new(
            Vec2::new(handle_rect.min.x(), center.y() - half),
            Vec2::new(handle_rect.max.x(), center.y() + half),
        ),
    }
}

fn splitter_ratio_from_pos(split: &SplitInfo, pos: Vec2) -> f32 {
    match split.direction {
        SplitDirection::Horizontal => {
            ((pos.x() - split.node_bounds.min.x()) / split.node_bounds.width()).clamp(0.05, 0.95)
        }
        SplitDirection::Vertical => {
            ((pos.y() - split.node_bounds.min.y()) / split.node_bounds.height()).clamp(0.05, 0.95)
        }
    }
}

/// Widget that renders a DockTree with tab bars and splitter handles.
///
/// Reads the `DockTree<T>` from `StateArena` via `dock_state_id`.
/// Draws children (panel content) first, then chrome (tab bars, splitters)
/// on top via `draw_after_children`.
///
/// When placed in a full-screen ZStack, use `content_inset_top` and
/// `content_inset_bottom` to exclude regions occupied by overlays like
/// toolbar and status bar.
pub struct DockSpace<T: Clone + PartialEq + 'static> {
    pub dock_state_id: StateId,
    pub drag_state_id: StateId,
    pub panel_labels: Vec<(T, String)>,
    pub tab_bar_height: f32,
    pub splitter_width: f32,
    /// Pixels to exclude from the top of the widget's bounds (e.g. toolbar).
    pub content_inset_top: f32,
    /// Pixels to exclude from the bottom of the widget's bounds (e.g. status bar).
    pub content_inset_bottom: f32,
    pub flex: FlexProps,
    pub(crate) child_widgets: Vec<Option<Box<dyn Widget>>>,
    children: Vec<ViewId>,
}

impl<T: Clone + PartialEq + std::fmt::Debug + 'static> DockSpace<T> {
    pub fn new(
        dock_state_id: StateId,
        drag_state_id: StateId,
        panel_labels: Vec<(T, String)>,
        child_widgets: Vec<Box<dyn Widget>>,
        flex: FlexProps,
    ) -> Self {
        Self {
            dock_state_id,
            drag_state_id,
            panel_labels,
            tab_bar_height: crate::tokens::TAB_BAR_HEIGHT,
            splitter_width: crate::tokens::SPLITTER_HIT_WIDTH,
            content_inset_top: 0.0,
            content_inset_bottom: 0.0,
            flex,
            child_widgets: child_widgets.into_iter().map(Some).collect(),
            children: Vec::new(),
        }
    }

    /// Compute the effective dock bounds after applying content insets.
    pub fn effective_bounds(&self, bounds: Rect2D) -> Rect2D {
        Rect2D::new(
            bounds.min + Vec2::new(0.0, self.content_inset_top),
            bounds.max - Vec2::new(0.0, self.content_inset_bottom),
        )
    }

    fn get_label(&self, tab_val: &T) -> String {
        self.panel_labels
            .iter()
            .find(|(id, _)| id == tab_val)
            .map(|(_, label)| label.clone())
            .unwrap_or_else(|| format!("{:?}", tab_val))
    }

    fn is_empty_tree(&self, state: &StateArena) -> bool {
        let tree: Option<DockTree<T>> = state.get(self.dock_state_id);
        match tree {
            None => true,
            Some(tree) => matches!(tree.root(), DockNode::Empty),
        }
    }

    fn read_tree(&self, state: &StateArena) -> Option<DockTree<T>> {
        state.get(self.dock_state_id)
    }
}

impl<T: Clone + PartialEq + Default + std::fmt::Debug + 'static> Widget for DockSpace<T> {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn diff_against(&self, prev: &dyn Widget) -> DiffAction {
        if prev.as_any().downcast_ref::<DockSpace<T>>().is_some() {
            DiffAction::RecurseChildren
        } else {
            DiffAction::Replace
        }
    }

    fn layout_style(&self, _measure: MeasureFn<'_>) -> Style {
        let mut style = Style {
            size: TaffySize {
                width: Dimension::Percent(1.0),
                height: Dimension::Percent(1.0),
            },
            ..Style::default()
        };
        crate::declarative::layout::apply_flex_props(&mut style, &self.flex);
        style.flex_grow = 1.0;
        style.flex_shrink = 1.0;
        style
    }

    fn handle_input(
        &self,
        ctx: &mut InputContext<'_>,
        state: &mut StateArena,
        bounds: Rect2D,
        _children: &[ViewId],
    ) -> InputResult {
        let tree = match self.read_tree(state) {
            Some(t) => t,
            None => return InputResult::Ignore,
        };

        if matches!(tree.root(), DockNode::Empty) {
            return InputResult::Ignore;
        }

        let dock_bounds = self.effective_bounds(bounds);
        let mut drag_state: DockDragState<T> = state.get(self.drag_state_id).unwrap_or_default();

        if drag_state.dragging {
            return self.handle_drag(ctx, state, dock_bounds, &tree, &mut drag_state);
        }

        if !ctx.input.mouse_clicked(mouse_button::LEFT) {
            return InputResult::Ignore;
        }

        let splits = compute_split_info(tree.root(), dock_bounds, self.splitter_width);
        if let Some(split) = splits
            .iter()
            .find(|split| split.handle_rect.contains(ctx.mouse_pos))
        {
            drag_state.dragging = true;
            drag_state.drag_started = true;
            drag_state.source_is_splitter = true;
            drag_state.source_path = split.path.clone();
            drag_state.drag_start = ctx.mouse_pos;
            drag_state.drag_pos = ctx.mouse_pos;
            drag_state.target_zone = None;
            drag_state.target_path = None;
            state.set(self.drag_state_id, drag_state);
            return InputResult::Consumed;
        }

        let leaf_info = compute_leaf_info(tree.root(), dock_bounds, self.tab_bar_height);
        for leaf in &leaf_info {
            if leaf.tabs.is_empty() {
                continue;
            }

            let tab_bar_bounds = Rect2D::new(
                leaf.full_bounds.min,
                Vec2::new(
                    leaf.full_bounds.max.x(),
                    leaf.full_bounds.min.y() + self.tab_bar_height,
                ),
            );

            if !tab_bar_bounds.contains(ctx.mouse_pos) {
                continue;
            }

            let tab_count = leaf.tabs.len();
            let tab_width = tab_hit_width(tab_bar_bounds.width(), tab_count);
            let tab_index = ((ctx.mouse_pos.x() - tab_bar_bounds.min.x()) / tab_width)
                .clamp(0.0, tab_count as f32 - 0.01) as usize;

            if tab_index < leaf.tabs.len() {
                let source_tab = leaf.tabs[tab_index].clone();
                let delta = ctx.input.mouse_delta;
                let delta_sq = delta.x() * delta.x() + delta.y() * delta.y();

                ctx.actions.emit(DockAction::TabActivated {
                    path: leaf.path.clone(),
                    tab: source_tab.clone(),
                });

                drag_state.dragging = true;
                drag_state.drag_started = delta_sq > TAB_DRAG_THRESHOLD * TAB_DRAG_THRESHOLD;
                drag_state.source_is_splitter = false;
                drag_state.source_path = leaf.path.clone();
                drag_state.source_tab = source_tab;
                drag_state.drag_start = ctx.mouse_pos;
                drag_state.drag_pos = ctx.mouse_pos;
                drag_state.target_zone = None;
                drag_state.target_path = None;
                state.set(self.drag_state_id, drag_state);
                return InputResult::Consumed;
            }
        }

        InputResult::Ignore
    }

    fn draw(
        &self,
        _ctx: &mut UiContext,
        state: &StateArena,
        _bounds: Rect2D,
        _animation: &AnimationState,
        _children: &[ViewId],
        _info: &DrawInfo,
    ) {
        if self.is_empty_tree(state) {}
    }

    fn draw_after_children(
        &self,
        ctx: &mut UiContext,
        state: &StateArena,
        bounds: Rect2D,
        _children: &[ViewId],
        _children_bounds: &[Rect2D],
    ) {
        let tree = match self.read_tree(state) {
            Some(t) => t,
            None => return,
        };

        if matches!(tree.root(), DockNode::Empty) {
            return;
        }

        let dock_bounds = self.effective_bounds(bounds);
        let leaf_info = compute_leaf_info(tree.root(), dock_bounds, self.tab_bar_height);
        let splits = compute_split_info(tree.root(), dock_bounds, self.splitter_width);

        for leaf in &leaf_info {
            if leaf.tabs.is_empty() {
                continue;
            }

            let tab_bar_bounds = Rect2D::new(
                leaf.full_bounds.min,
                Vec2::new(
                    leaf.full_bounds.max.x(),
                    leaf.full_bounds.min.y() + self.tab_bar_height,
                ),
            );

            let tab_count = leaf.tabs.len();
            let tab_width = tab_hit_width(tab_bar_bounds.width(), tab_count);
            let font_size = ctx.style().font_size;

            for (i, tab_val) in leaf.tabs.iter().enumerate() {
                let tab_bounds = Rect2D::from_origin_size(
                    Vec2::new(
                        tab_bar_bounds.min.x() + i as f32 * tab_width,
                        tab_bar_bounds.min.y(),
                    ),
                    Vec2::new(tab_width, tab_bar_bounds.height()),
                );

                let is_active = i == leaf.active;
                let is_hovered = tab_bounds.contains(ctx.mouse_pos());

                let bg = if is_active {
                    ctx.style().tab_active_bg
                } else if is_hovered {
                    ctx.style().tab_hover_bg
                } else {
                    ctx.style().tab_inactive_bg
                };
                ctx.draw_rect(tab_bounds, bg);

                let label = self.get_label(tab_val);
                let label_size = ctx.measure_text(&label, font_size);
                let text_pos = Vec2::new(
                    tab_bounds.min.x() + crate::tokens::TAB_LABEL_LEADING,
                    tab_bounds.center().y() - label_size.y() * 0.5,
                );
                let text_color = if is_active {
                    ctx.style().tab_active_text
                } else {
                    ctx.style().tab_text
                };
                ctx.draw_text(&label, text_pos, text_color, font_size);
            }

            // Hairline closing the tab strip; under the active tab it picks
            // up the tab surface so the tab reads as connected to its panel.
            let divider_color = if ctx.style().tab_border.a > 0.0 {
                ctx.style().tab_border
            } else {
                ctx.style().separator
            };
            if divider_color.a > 0.0 {
                let divider_rect = Rect2D::new(
                    Vec2::new(
                        tab_bar_bounds.min.x(),
                        tab_bar_bounds.max.y() - crate::tokens::DIVIDER_THICKNESS,
                    ),
                    Vec2::new(tab_bar_bounds.max.x(), tab_bar_bounds.max.y()),
                );
                ctx.draw_rect(divider_rect, divider_color);
                if let Some(active) = leaf.tabs.get(leaf.active) {
                    let label = self.get_label(active);
                    let label_size = ctx.measure_text(&label, font_size);
                    let label_width = label_size.x() + crate::tokens::TAB_LABEL_LEADING * 2.0;
                    let active_rect = Rect2D::new(
                        Vec2::new(
                            tab_bar_bounds.min.x() + leaf.active as f32 * tab_width,
                            tab_bar_bounds.max.y() - crate::tokens::DIVIDER_THICKNESS,
                        ),
                        Vec2::new(
                            (tab_bar_bounds.min.x() + leaf.active as f32 * tab_width + label_width)
                                .min(tab_bar_bounds.max.x()),
                            tab_bar_bounds.max.y(),
                        ),
                    );
                    ctx.draw_rect(active_rect, ctx.style().tab_active_bg);
                }
            }
        }

        for split in &splits {
            let is_hovered = split.handle_rect.contains(ctx.mouse_pos());
            if is_hovered {
                ctx.draw_rect(split.handle_rect, ctx.style().selectable_selected);
            } else {
                let color = if ctx.style().tab_border.a > 0.0 {
                    ctx.style().tab_border
                } else {
                    ctx.style().separator
                };
                if color.a > 0.0 {
                    ctx.draw_rect(split.line_rect, color);
                }
            }
        }

        let drag_state: DockDragState<T> = state.get(self.drag_state_id).unwrap_or_default();
        if drag_state.dragging && drag_state.drag_started && !drag_state.source_is_splitter {
            self.draw_drag_overlay(ctx, &leaf_info, &drag_state);
        }
    }

    fn children(&self) -> &[ViewId] {
        &self.children
    }

    fn children_mut(&mut self) -> &mut Vec<ViewId> {
        &mut self.children
    }

    fn take_children(&mut self) -> ChildWidgets {
        let children: Vec<(Option<u64>, Box<dyn Widget>)> = self
            .child_widgets
            .iter_mut()
            .filter_map(|opt| opt.take().map(|w| (None, w)))
            .collect();
        if children.is_empty() {
            ChildWidgets::None
        } else {
            ChildWidgets::Multi(children)
        }
    }

    fn interactive(&self) -> bool {
        false
    }

    fn wants_global_input(&self, _state: &StateArena) -> bool {
        true
    }

    fn is_focus_scope(&self) -> bool {
        false
    }
}

impl<T: Clone + PartialEq + Default + std::fmt::Debug + 'static> DockSpace<T> {
    fn handle_drag(
        &self,
        ctx: &mut InputContext<'_>,
        state: &mut StateArena,
        bounds: Rect2D,
        tree: &DockTree<T>,
        drag_state: &mut DockDragState<T>,
    ) -> InputResult {
        if drag_state.source_is_splitter {
            if ctx.input.is_mouse_down(mouse_button::LEFT) {
                let splits = compute_split_info(tree.root(), bounds, self.splitter_width);
                if let Some(split) = splits
                    .iter()
                    .find(|split| split.path == drag_state.source_path)
                {
                    ctx.actions.emit::<DockAction<T>>(DockAction::SplitResized {
                        path: drag_state.source_path.clone(),
                        ratio: splitter_ratio_from_pos(split, ctx.mouse_pos),
                    });
                }
                return InputResult::Consumed;
            }

            *drag_state = DockDragState::default();
            state.set(self.drag_state_id, drag_state.clone());
            return InputResult::Consumed;
        }

        if ctx.input.is_mouse_down(mouse_button::LEFT) {
            drag_state.drag_pos = ctx.mouse_pos;

            if !drag_state.drag_started {
                let delta = ctx.mouse_pos - drag_state.drag_start;
                let delta_sq = delta.x() * delta.x() + delta.y() * delta.y();
                drag_state.drag_started = delta_sq > TAB_DRAG_THRESHOLD * TAB_DRAG_THRESHOLD;
            }

            if drag_state.drag_started {
                drag_state.target_path = None;
                drag_state.target_zone = None;
                let leaf_info = compute_leaf_info(tree.root(), bounds, self.tab_bar_height);
                for leaf in &leaf_info {
                    if leaf.full_bounds.contains(ctx.mouse_pos) {
                        drag_state.target_path = Some(leaf.path.clone());
                        drag_state.target_zone =
                            Some(zone_from_pos(leaf.full_bounds, ctx.mouse_pos));
                        break;
                    }
                }
            }

            state.set(self.drag_state_id, drag_state.clone());
            return InputResult::Consumed;
        }

        if drag_state.drag_started
            && let (Some(to_path), Some(zone)) =
                (drag_state.target_path.clone(), drag_state.target_zone)
            && (to_path != drag_state.source_path || zone != DockZone::Center)
        {
            ctx.actions.emit(DockAction::TabMoved {
                from_path: drag_state.source_path.clone(),
                to_path,
                zone,
                tab: drag_state.source_tab.clone(),
            });
        }

        *drag_state = DockDragState::default();
        state.set(self.drag_state_id, drag_state.clone());
        InputResult::Consumed
    }

    fn draw_drag_overlay(
        &self,
        ctx: &mut UiContext,
        leaf_info: &[LeafInfo<T>],
        drag_state: &DockDragState<T>,
    ) {
        if let (Some(target_path), Some(ref zone)) = (
            drag_state.target_path.as_ref(),
            drag_state.target_zone.as_ref(),
        ) {
            for leaf in leaf_info {
                if leaf.path == *target_path {
                    let zone_rect = match zone {
                        DockZone::Center => {
                            let inset =
                                leaf.full_bounds.width().min(leaf.full_bounds.height()) * 0.15;
                            Rect2D::new(
                                leaf.full_bounds.min + Vec2::new(inset, inset),
                                leaf.full_bounds.max - Vec2::new(inset, inset),
                            )
                        }
                        DockZone::Left => Rect2D::new(
                            leaf.full_bounds.min,
                            Vec2::new(
                                leaf.full_bounds.min.x() + leaf.full_bounds.width() * 0.5,
                                leaf.full_bounds.max.y(),
                            ),
                        ),
                        DockZone::Right => Rect2D::new(
                            Vec2::new(
                                leaf.full_bounds.min.x() + leaf.full_bounds.width() * 0.5,
                                leaf.full_bounds.min.y(),
                            ),
                            leaf.full_bounds.max,
                        ),
                        DockZone::Top => Rect2D::new(
                            leaf.full_bounds.min,
                            Vec2::new(
                                leaf.full_bounds.max.x(),
                                leaf.full_bounds.min.y() + leaf.full_bounds.height() * 0.5,
                            ),
                        ),
                        DockZone::Bottom => Rect2D::new(
                            Vec2::new(
                                leaf.full_bounds.min.x(),
                                leaf.full_bounds.min.y() + leaf.full_bounds.height() * 0.5,
                            ),
                            leaf.full_bounds.max,
                        ),
                    };
                    let overlay_color = ctx.style().check_mark_color.with_alpha(0.3);
                    ctx.draw_rect(zone_rect, overlay_color);
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::declarative::layout::measure_text_descriptor;
    use crate::declarative::widget::{DrawInteraction, WidgetBox};

    fn make_leaf(tabs: Vec<u32>) -> DockNode<u32> {
        DockNode::Leaf { tabs, active: 0 }
    }

    fn make_split(
        direction: SplitDirection,
        ratio: f32,
        left: DockNode<u32>,
        right: DockNode<u32>,
    ) -> DockNode<u32> {
        DockNode::Split {
            direction,
            ratio,
            children: [Box::new(left), Box::new(right)],
        }
    }

    fn make_view_id(ffi: u64) -> ViewId {
        ViewId::from(slotmap::KeyData::from_ffi(ffi))
    }

    fn setup_dock_tree() -> (StateArena, StateId, StateId) {
        let mut arena = StateArena::new();
        let view_id = make_view_id(1);
        let dock_state_id = arena.get_or_create(
            view_id,
            DockTree::new(make_split(
                SplitDirection::Horizontal,
                0.5,
                make_leaf(vec![1, 2]),
                make_leaf(vec![3]),
            )),
        );
        let drag_state_id = arena.get_or_create(view_id, DockDragState::<u32>::default());
        (arena, dock_state_id, drag_state_id)
    }

    fn make_dock_space(dock_state_id: StateId, drag_state_id: StateId) -> DockSpace<u32> {
        DockSpace::new(
            dock_state_id,
            drag_state_id,
            vec![
                (1u32, "Panel A".into()),
                (2u32, "Panel B".into()),
                (3u32, "Panel C".into()),
            ],
            vec![],
            FlexProps::default(),
        )
    }

    #[test]
    fn test_dockspace_layout_style_fills_space() {
        let (_arena, dock_id, drag_id) = setup_dock_tree();
        let ds = make_dock_space(dock_id, drag_id);
        let style = ds.layout_style(&measure_text_descriptor);
        assert_eq!(style.flex_grow, 1.0);
        assert_eq!(style.flex_shrink, 1.0);
    }

    #[test]
    fn test_dockspace_reads_docktree_from_state_arena() {
        let (arena, dock_id, drag_id) = setup_dock_tree();
        let ds = make_dock_space(dock_id, drag_id);
        let tree = ds.read_tree(&arena);
        assert!(tree.is_some());
        assert!(matches!(tree.unwrap().root(), DockNode::Split { .. }));
    }

    #[test]
    fn test_dockspace_missing_tree_renders_nothing() {
        let mut arena = StateArena::new();
        let view_id = make_view_id(99);
        let dock_id = arena.get_or_create::<Option<DockTree<u32>>>(view_id, None);
        let drag_id = arena.get_or_create(view_id, DockDragState::<u32>::default());
        let ds = make_dock_space(dock_id, drag_id);
        assert!(ds.is_empty_tree(&arena));
    }

    #[test]
    fn test_dockspace_empty_tree_renders_nothing() {
        let mut arena = StateArena::new();
        let view_id = make_view_id(1);
        let dock_id = arena.get_or_create(view_id, DockTree::<u32>::new(DockNode::Empty));
        let drag_id = arena.get_or_create(view_id, DockDragState::<u32>::default());
        let ds: DockSpace<u32> =
            DockSpace::new(dock_id, drag_id, vec![], vec![], FlexProps::default());
        assert!(ds.is_empty_tree(&arena));

        let mut ui = crate::context::UiContext::new();
        ui.begin(Vec2::new(1920.0, 1080.0), 1.0);
        let bounds = Rect2D::new(Vec2::ZERO, Vec2::new(1920.0, 1080.0));
        let anim = AnimationState::default();
        let interaction = DrawInteraction {
            hovered_id: None,
            active_id: None,
            focused_id: None,
        };
        let vid = make_view_id(1);
        let info = DrawInfo {
            interaction: &interaction,
            view_id: vid,
            children_bounds: &[],
        };
        ds.draw(&mut ui, &arena, bounds, &anim, &[], &info);
        ds.draw_after_children(&mut ui, &arena, bounds, &[], &[]);
    }

    #[test]
    fn test_dockspace_draws_chrome() {
        let (arena, dock_id, drag_id) = setup_dock_tree();
        let ds = make_dock_space(dock_id, drag_id);
        let mut ui = crate::context::UiContext::new();
        ui.begin(Vec2::new(1920.0, 1080.0), 1.0);
        let bounds = Rect2D::new(Vec2::ZERO, Vec2::new(1920.0, 1080.0));
        ds.draw_after_children(&mut ui, &arena, bounds, &[], &[]);
    }

    #[test]
    fn test_dockspace_draw_order_content_then_chrome() {
        let (arena, dock_id, drag_id) = setup_dock_tree();
        let ds = make_dock_space(dock_id, drag_id);
        let mut ui = crate::context::UiContext::new();
        ui.begin(Vec2::new(1920.0, 1080.0), 1.0);
        let bounds = Rect2D::new(Vec2::ZERO, Vec2::new(1920.0, 1080.0));
        let anim = AnimationState::default();
        let interaction = DrawInteraction {
            hovered_id: None,
            active_id: None,
            focused_id: None,
        };
        let vid = make_view_id(1);
        let info = DrawInfo {
            interaction: &interaction,
            view_id: vid,
            children_bounds: &[],
        };
        ds.draw(&mut ui, &arena, bounds, &anim, &[], &info);
        ds.draw_after_children(&mut ui, &arena, bounds, &[], &[]);
    }

    #[test]
    fn test_dockspace_splitter_drag() {
        let (mut arena, dock_id, drag_id) = setup_dock_tree();
        let ds = make_dock_space(dock_id, drag_id);
        let mut input = crate::input::UiInputState::default();
        input.set_mouse_pos(Vec2::new(960.0, 540.0));
        input.set_mouse_button(mouse_button::LEFT, true);
        let mut callbacks = crate::declarative::build::CallbackTable::new();
        let mut actions = crate::declarative::actions::ActionStream::new();
        let view_id = make_view_id(1);
        let mut ctx = InputContext {
            input: &input,
            mouse_pos: Vec2::new(960.0, 540.0),
            callbacks: &mut callbacks,
            actions: &mut actions,
            view_id,
            active_id: None,
            focused_id: None,
        };
        let bounds = Rect2D::new(Vec2::ZERO, Vec2::new(1920.0, 1080.0));
        let result = ds.handle_input(&mut ctx, &mut arena, bounds, &[]);
        assert_eq!(result, InputResult::Consumed);
        let drag: DockDragState<u32> = arena.get(drag_id).unwrap();
        assert!(drag.dragging);
        assert!(drag.source_is_splitter);
    }

    #[test]
    fn test_dockspace_tab_drag_initiates() {
        let (mut arena, dock_id, drag_id) = setup_dock_tree();
        let ds = make_dock_space(dock_id, drag_id);
        let mut input = crate::input::UiInputState::default();
        input.set_mouse_pos(Vec2::new(80.0, 14.0));
        input.set_mouse_button(mouse_button::LEFT, true);
        input.mouse_delta = Vec2::new(5.0, 0.0);
        let mut callbacks = crate::declarative::build::CallbackTable::new();
        let mut actions = crate::declarative::actions::ActionStream::new();
        let view_id = make_view_id(1);
        let mut ctx = InputContext {
            input: &input,
            mouse_pos: Vec2::new(80.0, 14.0),
            callbacks: &mut callbacks,
            actions: &mut actions,
            view_id,
            active_id: None,
            focused_id: None,
        };
        let bounds = Rect2D::new(Vec2::ZERO, Vec2::new(1920.0, 1080.0));
        let result = ds.handle_input(&mut ctx, &mut arena, bounds, &[]);
        assert_eq!(result, InputResult::Consumed);
        let drag: DockDragState<u32> = arena.get(drag_id).unwrap();
        assert!(drag.dragging);
        assert!(drag.drag_started);
        assert_eq!(drag.source_tab, 1u32);
    }

    #[test]
    fn test_dockspace_emits_tab_activated() {
        let (mut arena, dock_id, drag_id) = setup_dock_tree();
        let ds = make_dock_space(dock_id, drag_id);
        let mut input = crate::input::UiInputState::default();
        input.set_mouse_pos(Vec2::new(600.0, 14.0));
        input.set_mouse_button(mouse_button::LEFT, true);
        let mut callbacks = crate::declarative::build::CallbackTable::new();
        let mut actions = crate::declarative::actions::ActionStream::new();
        let view_id = make_view_id(1);
        let mut ctx = InputContext {
            input: &input,
            mouse_pos: Vec2::new(600.0, 14.0),
            callbacks: &mut callbacks,
            actions: &mut actions,
            view_id,
            active_id: None,
            focused_id: None,
        };
        let bounds = Rect2D::new(Vec2::ZERO, Vec2::new(1920.0, 1080.0));
        ds.handle_input(&mut ctx, &mut arena, bounds, &[]);
        let emitted: Vec<DockAction<u32>> = ctx.actions.drain();
        assert!(
            emitted
                .iter()
                .any(|action| matches!(action, DockAction::TabActivated { tab, .. } if *tab == 2))
        );
    }

    #[test]
    fn test_dockspace_emits_split_resized() {
        let (mut arena, dock_id, drag_id) = setup_dock_tree();
        let ds = make_dock_space(dock_id, drag_id);
        let mut input = crate::input::UiInputState::default();
        input.set_mouse_pos(Vec2::new(960.0, 540.0));
        input.set_mouse_button(mouse_button::LEFT, true);
        let mut callbacks = crate::declarative::build::CallbackTable::new();
        let mut actions = crate::declarative::actions::ActionStream::new();
        let view_id = make_view_id(1);
        let mut ctx = InputContext {
            input: &input,
            mouse_pos: Vec2::new(960.0, 540.0),
            callbacks: &mut callbacks,
            actions: &mut actions,
            view_id,
            active_id: None,
            focused_id: None,
        };
        let bounds = Rect2D::new(Vec2::ZERO, Vec2::new(1920.0, 1080.0));
        ds.handle_input(&mut ctx, &mut arena, bounds, &[]);

        input.set_mouse_pos(Vec2::new(700.0, 540.0));
        let mut ctx2 = InputContext {
            input: &input,
            mouse_pos: Vec2::new(700.0, 540.0),
            callbacks: &mut callbacks,
            actions: &mut actions,
            view_id,
            active_id: None,
            focused_id: None,
        };
        ds.handle_input(&mut ctx2, &mut arena, bounds, &[]);
        let emitted: Vec<DockAction<u32>> = ctx2.actions.drain();
        assert!(
            emitted
                .iter()
                .any(|action| matches!(action, DockAction::SplitResized { .. }))
        );
    }

    #[test]
    fn test_nested_splitter_ratio_uses_node_bounds() {
        let split = SplitInfo {
            path: DockPath(vec![1]),
            direction: SplitDirection::Horizontal,
            node_bounds: Rect2D::new(Vec2::new(960.0, 0.0), Vec2::new(1920.0, 1080.0)),
            handle_rect: Rect2D::default(),
            line_rect: Rect2D::default(),
        };
        assert!((splitter_ratio_from_pos(&split, Vec2::new(1440.0, 540.0)) - 0.5).abs() < 1e-4);
    }

    #[test]
    fn test_dockspace_drag_state_persistence() {
        let (mut arena, _dock_id, drag_id) = setup_dock_tree();
        let drag = DockDragState {
            dragging: true,
            drag_started: true,
            source_is_splitter: false,
            source_path: DockPath(vec![0]),
            source_tab: 1u32,
            drag_start: Vec2::new(80.0, 50.0),
            drag_pos: Vec2::new(100.0, 50.0),
            target_zone: Some(DockZone::Center),
            target_path: Some(DockPath(vec![1])),
        };
        arena.set(drag_id, drag.clone());
        let read_back: DockDragState<u32> = arena.get(drag_id).unwrap();
        assert!(read_back.dragging);
        assert_eq!(read_back.source_tab, 1u32);
        assert_eq!(read_back.target_zone, Some(DockZone::Center));
    }

    #[test]
    fn test_dockzone_all_five_zones() {
        let area = Rect2D::new(Vec2::ZERO, Vec2::new(400.0, 400.0));
        assert_eq!(
            zone_from_pos(area, Vec2::new(200.0, 200.0)),
            DockZone::Center
        );
        assert_eq!(zone_from_pos(area, Vec2::new(50.0, 200.0)), DockZone::Left);
        assert_eq!(
            zone_from_pos(area, Vec2::new(350.0, 200.0)),
            DockZone::Right
        );
        assert_eq!(zone_from_pos(area, Vec2::new(200.0, 50.0)), DockZone::Top);
        assert_eq!(
            zone_from_pos(area, Vec2::new(200.0, 350.0)),
            DockZone::Bottom
        );
    }

    #[test]
    fn test_zone_from_pos_edge_cases() {
        let area = Rect2D::new(Vec2::ZERO, Vec2::new(400.0, 400.0));
        assert_eq!(
            zone_from_pos(area, Vec2::new(500.0, 500.0)),
            DockZone::Center
        );
        assert_eq!(zone_from_pos(area, Vec2::new(50.0, 50.0)), DockZone::Top);
        assert_eq!(zone_from_pos(area, Vec2::new(350.0, 50.0)), DockZone::Top);
    }

    #[test]
    fn test_compute_leaf_info_single_leaf() {
        let tree = DockTree::new(make_leaf(vec![1u32, 2]));
        let area = Rect2D::new(Vec2::ZERO, Vec2::new(1920.0, 1080.0));
        let info = compute_leaf_info(tree.root(), area, 28.0);
        assert_eq!(info.len(), 1);
        assert_eq!(info[0].tabs, vec![1, 2]);
        assert_eq!(info[0].active, 0);
        assert_eq!(info[0].content_bounds.min.y(), 28.0);
    }

    #[test]
    fn test_compute_leaf_info_split() {
        let tree = DockTree::new(make_split(
            SplitDirection::Horizontal,
            0.5,
            make_leaf(vec![1u32]),
            make_leaf(vec![2u32]),
        ));
        let area = Rect2D::new(Vec2::ZERO, Vec2::new(1920.0, 1080.0));
        let info = compute_leaf_info(tree.root(), area, 28.0);
        assert_eq!(info.len(), 2);
        assert_eq!(info[0].tabs, vec![1]);
        assert_eq!(info[1].tabs, vec![2]);
        assert_eq!(info[0].full_bounds.max.x(), 960.0);
        assert_eq!(info[1].full_bounds.min.x(), 960.0);
    }

    #[test]
    fn test_compute_split_info() {
        let tree = DockTree::new(make_split(
            SplitDirection::Horizontal,
            0.5,
            make_leaf(vec![1u32]),
            make_leaf(vec![2u32]),
        ));
        let area = Rect2D::new(Vec2::ZERO, Vec2::new(1920.0, 1080.0));
        let splits = compute_split_info(tree.root(), area, 4.0);
        assert_eq!(splits.len(), 1);
        assert_eq!(splits[0].direction, SplitDirection::Horizontal);
        assert_eq!(splits[0].node_bounds, area);
        assert!((splits[0].handle_rect.center().x() - 960.0).abs() < 1.0);
    }

    #[test]
    fn test_dockspace_diff_same_type() {
        let (_, dock_id, drag_id) = setup_dock_tree();
        let a = make_dock_space(dock_id, drag_id);
        let b = make_dock_space(dock_id, drag_id);
        assert_eq!(b.diff_against(&a), DiffAction::RecurseChildren);
    }

    #[test]
    fn test_dockspace_diff_different_type() {
        let (_, dock_id, drag_id) = setup_dock_tree();
        let ds = make_dock_space(dock_id, drag_id);
        let other = crate::declarative::constructors::text("hello");
        assert_eq!(ds.diff_against(&other), DiffAction::Replace);
    }

    #[test]
    fn test_dockspace_take_children() {
        let (_, dock_id, drag_id) = setup_dock_tree();
        let mut ds: DockSpace<u32> = DockSpace::new(
            dock_id,
            drag_id,
            vec![],
            vec![
                crate::declarative::constructors::text("c1").boxed(),
                crate::declarative::constructors::text("c2").boxed(),
            ],
            FlexProps::default(),
        );
        match ds.take_children() {
            ChildWidgets::Multi(children) => assert_eq!(children.len(), 2),
            _ => panic!("Expected Multi children"),
        }
    }

    #[test]
    fn test_dockspace_interactive() {
        let (_, dock_id, drag_id) = setup_dock_tree();
        let ds = make_dock_space(dock_id, drag_id);
        assert!(!ds.interactive());
        assert!(ds.wants_global_input(&StateArena::new()));
    }

    #[test]
    fn test_dockspace_focus_scope() {
        let (_, dock_id, drag_id) = setup_dock_tree();
        let ds = make_dock_space(dock_id, drag_id);
        assert!(!ds.is_focus_scope());
    }

    #[test]
    fn test_dock_action_variants() {
        let moved = DockAction::TabMoved {
            from_path: DockPath(vec![0]),
            to_path: DockPath(vec![1]),
            zone: DockZone::Center,
            tab: 1u32,
        };
        let closed = DockAction::TabClosed {
            path: DockPath(vec![0]),
            tab: 1u32,
        };
        let resized: DockAction<u32> = DockAction::SplitResized {
            path: DockPath::root(),
            ratio: 0.7,
        };
        let activated = DockAction::TabActivated {
            path: DockPath(vec![0]),
            tab: 1u32,
        };
        assert_eq!(moved, moved.clone());
        assert_ne!(moved, closed);
        let _ = format!("{:?}", closed);
        let _ = format!("{:?}", resized);
        let _ = format!("{:?}", activated);
    }

    #[test]
    fn test_dock_drag_state_default() {
        let state = DockDragState::<u32>::default();
        assert!(!state.dragging);
        assert!(!state.drag_started);
        assert!(!state.source_is_splitter);
        assert_eq!(state.source_tab, 0u32);
    }
}
