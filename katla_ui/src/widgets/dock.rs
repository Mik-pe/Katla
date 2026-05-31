//! Dockable panel system — data structures, tab bar widget, drag-drop, and rendering.
//!
//! Provides the foundational types for a dockable panel layout:
//! - [`DockNode`] / [`DockLayout`] — tree-based split/tab layout
//! - [`DockTabBar`] — widget for rendering tabs within a dock leaf
//! - [`DockArea`] — recursive renderer with resize handles, tab drag, dock zones
//! - [`DockDragState`] — tracks in-progress tab drag across frames

use crate::context::UiContext;
use katla_math::{Color, Rect2D, Vec2};

// ---------------------------------------------------------------------------
// ResizeHandle Widget (internal, used by DockArea)
// ---------------------------------------------------------------------------

/// Direction of resize for a [`ResizeHandle`].
pub(crate) enum ResizeDirection {
    Horizontal,
    Vertical,
}

/// A thin invisible hit-region that drives panel-edge resizing.
///
/// Returns the new clamped dimension after each frame. Cursor changes and
/// drag tracking are handled internally so callers only need to feed the
/// returned value back into their layout.
pub(crate) struct ResizeHandle {
    bounds: Rect2D,
    direction: ResizeDirection,
    current_value: f32,
    min_value: f32,
    max_value: f32,
    inverted: bool,
}

impl ResizeHandle {
    /// Create a horizontal resize handle (left/right drag changes width).
    pub(crate) fn horizontal(bounds: Rect2D, current_value: f32) -> Self {
        Self {
            bounds,
            direction: ResizeDirection::Horizontal,
            current_value,
            min_value: 0.0,
            max_value: f32::MAX,
            inverted: false,
        }
    }

    /// Create a vertical resize handle (up/down drag changes height).
    pub(crate) fn vertical(bounds: Rect2D, current_value: f32) -> Self {
        Self {
            bounds,
            direction: ResizeDirection::Vertical,
            current_value,
            min_value: 0.0,
            max_value: f32::MAX,
            inverted: false,
        }
    }

    /// Set the minimum allowed value.
    pub(crate) fn min_value(mut self, min: f32) -> Self {
        self.min_value = min;
        self
    }

    /// Set the maximum allowed value.
    pub(crate) fn max_value(mut self, max: f32) -> Self {
        self.max_value = max;
        self
    }

    /// Process the resize interaction and return the new clamped dimension.
    pub(crate) fn show(self, ui: &mut UiContext) -> f32 {
        let id = ui.generate_id("resize_handle");
        let hovered = ui.input.is_hovered(self.bounds);

        if hovered {
            match self.direction {
                ResizeDirection::Horizontal => {
                    ui.set_mouse_cursor(crate::input::MouseCursor::ResizeHorizontal)
                }
                ResizeDirection::Vertical => {
                    ui.set_mouse_cursor(crate::input::MouseCursor::ResizeVertical)
                }
            }
        }

        let is_active = ui.active_id == Some(id);

        if hovered && ui.input.mouse_pressed[crate::input::mouse_button::LEFT] && !is_active {
            ui.active_id = Some(id);
        }

        if is_active {
            let raw_delta = match self.direction {
                ResizeDirection::Horizontal => ui.input.mouse_delta.x(),
                ResizeDirection::Vertical => ui.input.mouse_delta.y(),
            };
            let delta = if self.inverted { -raw_delta } else { raw_delta };
            let new_value = (self.current_value + delta).clamp(self.min_value, self.max_value);

            if !ui.input.mouse_down[crate::input::mouse_button::LEFT] {
                ui.active_id = None;
            }

            new_value
        } else {
            self.current_value
        }
    }
}

// ---------------------------------------------------------------------------
// Dock system
// ---------------------------------------------------------------------------

/// Unique identifier for a dockable panel.
pub type DockPanelId = u64;

// ---------------------------------------------------------------------------
// DockTree data structures
// ---------------------------------------------------------------------------

/// A node in the dock tree — either a split or a leaf with tabs.
#[derive(Debug, Clone)]
pub enum DockNode {
    /// A horizontal or vertical split between two children.
    Split {
        direction: SplitDirection,
        /// 0.0–1.0, position of the splitter.
        ratio: f32,
        children: Box<[DockNode; 2]>,
    },
    /// A leaf node containing one or more tabbed panels.
    Leaf {
        tabs: Vec<DockPanelId>,
        active_tab: usize,
        /// When true, the leaf is collapsed — only the tab bar is visible.
        collapsed: bool,
    },
}

/// Direction of a dock split.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitDirection {
    /// Left / right.
    Horizontal,
    /// Top / bottom.
    Vertical,
}

/// A floating (undocked) window.
#[derive(Debug, Clone)]
pub struct FloatingDockWindow {
    pub node: DockNode,
    pub position: Vec2,
    pub size: Vec2,
}

/// The complete dock layout.
#[derive(Debug, Clone)]
pub struct DockLayout {
    pub root: DockNode,
    pub floating: Vec<FloatingDockWindow>,
}

/// Persistent drag state for tab tear-off and dock zone interaction.
/// Stored externally (on EditorUI) and passed to DockArea each frame.
#[derive(Debug, Clone, Default)]
pub struct DockDragState {
    /// The panel ID being dragged (if any).
    pub dragging_panel: Option<DockPanelId>,
    /// Current mouse position during drag.
    pub mouse_pos: Vec2,
    /// The source leaf bounds (used to compute tear-off threshold).
    pub source_bounds: Option<Rect2D>,
    /// Whether the tab has been torn off (moved far enough from source).
    pub torn_off: bool,
    /// The dock zone being hovered during drag (if any).
    pub target_zone: Option<DockZone>,
    /// The leaf bounds that the zone applies to.
    pub target_leaf_bounds: Option<Rect2D>,
}

/// A dock zone where a dragged panel can be dropped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockZone {
    /// Drop onto center — add as tab.
    Center,
    /// Drop onto left edge — split left.
    Left,
    /// Drop onto right edge — split right.
    Right,
    /// Drop onto top edge — split top.
    Top,
    /// Drop onto bottom edge — split bottom.
    Bottom,
}

impl DockNode {
    pub fn leaf(tab: DockPanelId) -> Self {
        DockNode::Leaf {
            tabs: vec![tab],
            active_tab: 0,
            collapsed: false,
        }
    }

    pub fn leaf_with_tabs(tabs: Vec<DockPanelId>) -> Self {
        DockNode::Leaf {
            active_tab: 0,
            collapsed: false,
            tabs,
        }
    }

    pub fn split(direction: SplitDirection, ratio: f32, left: DockNode, right: DockNode) -> Self {
        DockNode::Split {
            direction,
            ratio: ratio.clamp(0.1, 0.9),
            children: Box::new([left, right]),
        }
    }

    /// Find the leaf node containing the given panel ID.
    pub fn find_leaf_with_panel(&self, panel_id: DockPanelId) -> Option<&DockNode> {
        match self {
            DockNode::Split { children, .. } => children[0]
                .find_leaf_with_panel(panel_id)
                .or_else(|| children[1].find_leaf_with_panel(panel_id)),
            DockNode::Leaf { tabs, .. } => {
                if tabs.contains(&panel_id) {
                    Some(self)
                } else {
                    None
                }
            }
        }
    }

    /// Find the leaf node containing the given panel ID (mutable).
    pub fn find_leaf_with_panel_mut(&mut self, panel_id: DockPanelId) -> Option<&mut DockNode> {
        match self {
            DockNode::Split { children, .. } => {
                if children[0].find_leaf_with_panel(panel_id).is_some() {
                    children[0].find_leaf_with_panel_mut(panel_id)
                } else {
                    children[1].find_leaf_with_panel_mut(panel_id)
                }
            }
            DockNode::Leaf { tabs, .. } => {
                if tabs.contains(&panel_id) {
                    Some(self)
                } else {
                    None
                }
            }
        }
    }

    /// Remove a panel from the tree. Returns `true` if the panel was found.
    ///
    /// After removal, collapses any Split nodes whose children become empty
    /// leaves, replacing the split with the non-empty sibling.
    pub fn remove_panel(&mut self, panel_id: DockPanelId) -> bool {
        match self {
            DockNode::Split { children, .. } => {
                let found_in_first = children[0].remove_panel(panel_id);
                let found_in_second = children[1].remove_panel(panel_id);
                let found = found_in_first || found_in_second;
                if found {
                    self.collapse_empty_splits();
                }
                found
            }
            DockNode::Leaf {
                tabs, active_tab, ..
            } => {
                if let Some(pos) = tabs.iter().position(|&t| t == panel_id) {
                    tabs.remove(pos);
                    if *active_tab >= tabs.len() {
                        *active_tab = tabs.len().saturating_sub(1);
                    }
                    true
                } else {
                    false
                }
            }
        }
    }

    /// Collapse splits where one or both children are empty leaves.
    /// Replaces the split with its non-empty child, or becomes an empty leaf.
    fn collapse_empty_splits(&mut self) {
        if let DockNode::Split { children, .. } = self {
            children[0].collapse_empty_splits();
            children[1].collapse_empty_splits();

            let first_empty = children[0].is_empty_leaf();
            let second_empty = children[1].is_empty_leaf();

            if first_empty && second_empty {
                *self = DockNode::Leaf {
                    tabs: Vec::new(),
                    active_tab: 0,
                    collapsed: false,
                };
            } else if first_empty {
                let replacement = std::mem::replace(
                    &mut children[1],
                    DockNode::Leaf {
                        tabs: Vec::new(),
                        active_tab: 0,
                        collapsed: false,
                    },
                );
                *self = replacement;
            } else if second_empty {
                let replacement = std::mem::replace(
                    &mut children[0],
                    DockNode::Leaf {
                        tabs: Vec::new(),
                        active_tab: 0,
                        collapsed: false,
                    },
                );
                *self = replacement;
            }
        }
    }

    /// Returns true if this is a leaf with no tabs.
    fn is_empty_leaf(&self) -> bool {
        matches!(self, DockNode::Leaf { tabs, .. } if tabs.is_empty())
    }

    /// Add a panel as a new tab to the leaf containing `target_panel_id`.
    pub fn add_tab_to_leaf(
        &mut self,
        target_panel_id: DockPanelId,
        new_panel_id: DockPanelId,
    ) -> bool {
        if let Some(leaf) = self.find_leaf_with_panel_mut(target_panel_id)
            && let DockNode::Leaf {
                tabs, active_tab, ..
            } = leaf
        {
            tabs.push(new_panel_id);
            *active_tab = tabs.len() - 1;
            return true;
        }
        false
    }

    /// Split a leaf containing `target_panel_id` by placing `new_panel_id`
    /// on the given side, creating a new Split node.
    pub fn split_leaf(
        &mut self,
        target_panel_id: DockPanelId,
        new_panel_id: DockPanelId,
        zone: DockZone,
    ) -> bool {
        // Find the leaf and its parent
        let (direction, new_on_first) = match zone {
            DockZone::Left => (SplitDirection::Horizontal, true),
            DockZone::Right => (SplitDirection::Horizontal, false),
            DockZone::Top => (SplitDirection::Vertical, true),
            DockZone::Bottom => (SplitDirection::Vertical, false),
            DockZone::Center => return self.add_tab_to_leaf(target_panel_id, new_panel_id),
        };

        // Find and replace the leaf
        self.split_leaf_inner(target_panel_id, new_panel_id, direction, new_on_first)
    }

    fn split_leaf_inner(
        &mut self,
        target_panel_id: DockPanelId,
        new_panel_id: DockPanelId,
        direction: SplitDirection,
        new_on_first: bool,
    ) -> bool {
        match self {
            DockNode::Split { children, .. } => {
                if children[0].find_leaf_with_panel(target_panel_id).is_some() {
                    children[0].split_leaf_inner(
                        target_panel_id,
                        new_panel_id,
                        direction,
                        new_on_first,
                    )
                } else if children[1].find_leaf_with_panel(target_panel_id).is_some() {
                    children[1].split_leaf_inner(
                        target_panel_id,
                        new_panel_id,
                        direction,
                        new_on_first,
                    )
                } else {
                    false
                }
            }
            DockNode::Leaf {
                tabs,
                active_tab,
                collapsed,
            } => {
                if !tabs.contains(&target_panel_id) {
                    return false;
                }

                // Keep all existing tabs in the old leaf
                let old_tabs = std::mem::take(tabs);
                let old_collapsed = *collapsed;

                let old_leaf = DockNode::Leaf {
                    tabs: old_tabs,
                    active_tab: *active_tab,
                    collapsed: old_collapsed,
                };
                let new_leaf = DockNode::Leaf {
                    tabs: vec![new_panel_id],
                    active_tab: 0,
                    collapsed: false,
                };

                let (first, second) = if new_on_first {
                    (new_leaf, old_leaf)
                } else {
                    (old_leaf, new_leaf)
                };

                *self = DockNode::split(direction, 0.5, first, second);
                true
            }
        }
    }

    /// Find the first leaf panel ID in the tree (non-allocating traversal).
    pub fn first_panel_id(&self) -> Option<DockPanelId> {
        match self {
            DockNode::Split { children, .. } => children[0]
                .first_panel_id()
                .or_else(|| children[1].first_panel_id()),
            DockNode::Leaf { tabs, .. } => tabs.first().copied(),
        }
    }
}

impl DockLayout {
    pub fn new(root: DockNode) -> Self {
        DockLayout {
            root,
            floating: Vec::new(),
        }
    }

    /// Create a default layout with a single root leaf.
    pub fn single(tab: DockPanelId) -> Self {
        DockLayout::new(DockNode::leaf(tab))
    }
}

// ---------------------------------------------------------------------------
// DockTabBar widget
// ---------------------------------------------------------------------------

/// Height of the tab bar rendered at the top of each leaf node.
const TAB_BAR_HEIGHT: f32 = 28.0;

/// Width of the visual separator line drawn between split children.
const SPLITTER_THICKNESS: f32 = 2.0;

/// Size of the dock zone indicator square (center + 4 cardinal triangles).
/// Distance a tab must be dragged before it tears off from its source leaf.
const TEAR_OFF_THRESHOLD: f32 = 20.0;

/// Response returned by [`DockTabBar`].
#[derive(Debug, Clone, Copy)]
pub struct DockTabBarResponse {
    /// Index of the clicked tab, if any.
    pub clicked_tab: Option<usize>,
    /// Index of the close button that was pressed, if any.
    pub closed_tab: Option<usize>,
    /// Index of the tab being dragged (mouse down + moved), if any.
    pub drag_started_tab: Option<usize>,
}

/// A tab bar widget specialised for dock panel leaves.
///
/// Renders evenly distributed tabs with a bottom separator. The active tab
/// blends with the content panel below (no bottom border). Optionally shows
/// a small "x" close button on each tab.
pub struct DockTabBar<'a> {
    tabs: &'a [DockPanelId],
    active: usize,
    bounds: Rect2D,
    close_buttons: bool,
    id_base: Option<&'a str>,
    label_fn: Option<&'a dyn Fn(DockPanelId) -> &'static str>,
}

impl<'a> DockTabBar<'a> {
    pub fn new(tabs: &'a [DockPanelId], active: usize) -> Self {
        Self {
            tabs,
            active,
            bounds: Rect2D::from_size(Vec2::new(200.0, 28.0)),
            close_buttons: false,
            id_base: None,
            label_fn: None,
        }
    }

    pub fn bounds(mut self, bounds: Rect2D) -> Self {
        self.bounds = bounds;
        self
    }

    pub fn close_buttons(mut self, enabled: bool) -> Self {
        self.close_buttons = enabled;
        self
    }

    pub fn id(mut self, id: &'a str) -> Self {
        self.id_base = Some(id);
        self
    }

    pub fn labels(mut self, f: &'a dyn Fn(DockPanelId) -> &'static str) -> Self {
        self.label_fn = Some(f);
        self
    }
}

impl DockTabBar<'_> {
    pub fn show(self, ui: &mut UiContext) -> DockTabBarResponse {
        let tab_count = self.tabs.len();
        if tab_count == 0 {
            return DockTabBarResponse {
                clicked_tab: None,
                closed_tab: None,
                drag_started_tab: None,
            };
        }

        let active_bg = ui.style.window_bg;
        let inactive_bg = ui.style.window_title_bg;
        let hover_bg = ui.style.button_hovered;
        let active_text = ui.style.text_color;
        let inactive_text = ui.style.text_disabled;
        let separator_color = ui.style.separator;
        let font_size = ui.style.font_size;

        let tab_width = self.bounds.width() / tab_count as f32;
        let separator_y = self.bounds.max.y();

        ui.draw_line(
            Vec2::new(self.bounds.min.x(), separator_y),
            Vec2::new(self.bounds.max.x(), separator_y),
            separator_color,
            1.0,
        );

        let mut clicked_tab: Option<usize> = None;
        let mut closed_tab: Option<usize> = None;
        let mut drag_started_tab: Option<usize> = None;

        let id_prefix = self.id_base.unwrap_or("dock_tab");

        for (i, _panel_id) in self.tabs.iter().enumerate() {
            let tab_min_x = self.bounds.min.x() + tab_width * i as f32;
            let tab_bounds = Rect2D::from_origin_size(
                Vec2::new(tab_min_x, self.bounds.min.y()),
                Vec2::new(tab_width, self.bounds.height()),
            );
            let tab_id = ui.generate_id(&format!("{}_{}", id_prefix, i));

            ui.register_focusable(tab_id, tab_bounds, &format!("{}_{}", id_prefix, i));
            let tab_hovered = ui.update_hover(tab_id, tab_bounds);
            let is_active = i == self.active;

            let bg_color = if is_active {
                active_bg
            } else if tab_hovered {
                hover_bg
            } else {
                inactive_bg
            };
            ui.draw_rect(tab_bounds, bg_color);

            if is_active {
                if i > 0 {
                    ui.draw_line(
                        Vec2::new(self.bounds.min.x(), separator_y),
                        Vec2::new(tab_bounds.min.x(), separator_y),
                        separator_color,
                        1.0,
                    );
                }
                if i < tab_count - 1 {
                    ui.draw_line(
                        Vec2::new(tab_bounds.max.x(), separator_y),
                        Vec2::new(self.bounds.max.x(), separator_y),
                        separator_color,
                        1.0,
                    );
                }
            }

            let label = self
                .label_fn
                .map(|f| f(self.tabs[i]))
                .unwrap_or_else(|| "?");
            let text_color = if is_active {
                active_text
            } else {
                inactive_text
            };

            let close_btn_width = if self.close_buttons { 16.0 } else { 0.0 };
            let label_area_width = (tab_width - close_btn_width).max(0.0);
            let text_size = ui.measure_text(label, font_size);
            let text_pos = Vec2::new(
                tab_bounds.min.x() + (label_area_width - text_size.x()) * 0.5,
                tab_bounds.center().y() - text_size.y() * 0.5,
            );
            ui.draw_text(label, text_pos, text_color, font_size);

            if self.close_buttons {
                let close_x = tab_bounds.max.x() - close_btn_width;
                let close_bounds = Rect2D::from_origin_size(
                    Vec2::new(close_x, tab_bounds.min.y()),
                    Vec2::new(close_btn_width, tab_bounds.height()),
                );
                let close_id = ui.generate_id(&format!("{}_close_{}", id_prefix, i));
                ui.register_focusable(
                    close_id,
                    close_bounds,
                    &format!("{}_close_{}", id_prefix, i),
                );
                let close_hovered = ui.update_hover(close_id, close_bounds);

                let close_text_color = if close_hovered {
                    active_text
                } else {
                    inactive_text
                };
                let close_size = ui.measure_text("x", font_size);
                let close_pos = Vec2::new(
                    close_bounds.center().x() - close_size.x() * 0.5,
                    close_bounds.center().y() - close_size.y() * 0.5,
                );
                ui.draw_text("x", close_pos, close_text_color, font_size);

                let close_click = ui.click_interaction(
                    close_id,
                    close_hovered,
                    close_bounds,
                    crate::context::interaction::ClickConfig::POPUP_AWARE,
                );
                if close_click.is_clicked() {
                    closed_tab = Some(i);
                }
            }

            // Detect drag start: mouse down on tab + mouse moved
            let tab_drag_id = ui.generate_id(&format!("{}_drag_{}", id_prefix, i));
            if tab_hovered && ui.input.mouse_pressed[crate::input::mouse_button::LEFT] {
                ui.active_id = Some(tab_drag_id);
            }
            if ui.active_id == Some(tab_drag_id) {
                if ui.input.mouse_down[crate::input::mouse_button::LEFT] {
                    let delta = ui.input.mouse_delta;
                    if delta.x().abs() > 3.0 || delta.y().abs() > 3.0 {
                        drag_started_tab = Some(i);
                    }
                } else {
                    ui.active_id = None;
                }
            }

            let click_result = ui.click_interaction(
                tab_id,
                tab_hovered,
                tab_bounds,
                crate::context::interaction::ClickConfig::POPUP_AWARE,
            );
            if click_result.is_clicked() && !is_active {
                clicked_tab = Some(i);
            }

            if tab_hovered {
                ui.input.set_cursor(crate::input::MouseCursor::Hand);
            }
        }

        DockTabBarResponse {
            clicked_tab,
            closed_tab,
            drag_started_tab,
        }
    }
}

// ---------------------------------------------------------------------------
// DockArea widget
// ---------------------------------------------------------------------------

/// Response from rendering a [`DockArea`].
#[derive(Debug, Clone, Default)]
pub struct DockAreaResponse {
    /// Panel closed via tab close button.
    pub closed_panel: Option<DockPanelId>,
    /// A tab drag started this frame.
    pub drag_started: Option<(DockPanelId, Rect2D)>,
    /// A drop occurred this frame (drag ended over a dock zone).
    pub dropped: Option<(DockPanelId, DockZone, DockPanelId)>,
    /// The currently active (visible) panel IDs and their content bounds.
    pub visible_panels: Vec<(DockPanelId, Rect2D)>,
}

/// Extended render callback that receives panel label for tab rendering.
pub type RenderPanelFn<'a> = &'a mut dyn FnMut(&mut UiContext, Rect2D, DockPanelId);

pub struct DockArea<'a> {
    layout: &'a mut DockLayout,
    drag_state: &'a mut DockDragState,
    bounds: Rect2D,
    /// Maps panel IDs to human-readable labels for tab rendering.
    panel_label_fn: Option<&'a dyn Fn(DockPanelId) -> &'static str>,
}

impl<'a> DockArea<'a> {
    pub fn new(layout: &'a mut DockLayout, drag_state: &'a mut DockDragState) -> Self {
        Self {
            layout,
            drag_state,
            bounds: Rect2D::from_size(Vec2::new(800.0, 600.0)),
            panel_label_fn: None,
        }
    }

    pub fn bounds(mut self, bounds: Rect2D) -> Self {
        self.bounds = bounds;
        self
    }

    /// Provide a function that maps panel IDs to display labels for tab rendering.
    pub fn panel_labels(mut self, f: &'a dyn Fn(DockPanelId) -> &'static str) -> Self {
        self.panel_label_fn = Some(f);
        self
    }

    /// Render the dock area and return a response with interaction results.
    pub fn show(mut self, ui: &mut UiContext, render_panel: RenderPanelFn<'_>) -> DockAreaResponse {
        let mut response = DockAreaResponse::default();

        // If a drag is in progress, render the drag overlay
        if self.drag_state.dragging_panel.is_some() {
            self.handle_drag_drop(ui, &mut response);
        }

        render_node(
            render_panel,
            ui,
            &mut self.layout.root,
            self.bounds,
            self.panel_label_fn,
            self.drag_state,
            &mut response,
        );

        // Render floating windows
        for floating in &mut self.layout.floating {
            render_node(
                render_panel,
                ui,
                &mut floating.node,
                Rect2D::from_origin_size(floating.position, floating.size),
                self.panel_label_fn,
                self.drag_state,
                &mut response,
            );
        }

        // Render drag preview (floating tab at mouse position)
        if let Some(_panel_id) = self.drag_state.dragging_panel
            && self.drag_state.torn_off
        {
            self.render_drag_preview(ui);
        }

        response
    }

    /// Compute the content bounds for every visible leaf panel without rendering anything.
    /// Returns a list of (panel_id, content_bounds) for each active tab.
    /// Also processes resize handles so ratio changes are reflected.
    pub fn compute_leaf_bounds(
        layout: &mut DockLayout,
        ui: &mut UiContext,
        bounds: Rect2D,
    ) -> Vec<(DockPanelId, Rect2D)> {
        let mut result = Vec::new();
        compute_leaf_bounds_recursive(&mut layout.root, bounds, ui, &mut result);
        result
    }

    /// Render only the dock chrome (tab bars, splitters, drag overlay) without
    /// invoking the panel render callback. Used when panel content is rendered
    /// separately via the declarative view tree.
    pub fn show_chrome(mut self, ui: &mut UiContext) -> DockAreaResponse {
        let mut response = DockAreaResponse::default();

        // Handle drag interactions
        if self.drag_state.dragging_panel.is_some() {
            self.handle_drag_drop(ui, &mut response);
        }

        // Render chrome (tabs + splitters) — no panel content
        render_chrome_recursive(
            ui,
            &mut self.layout.root,
            self.bounds,
            self.panel_label_fn,
            self.drag_state,
            &mut response,
        );

        // Render drag preview
        if self.drag_state.dragging_panel.is_some() && self.drag_state.torn_off {
            self.render_drag_preview(ui);
        }

        response
    }
}

fn compute_leaf_bounds_recursive(
    node: &mut DockNode,
    bounds: Rect2D,
    ui: &mut UiContext,
    result: &mut Vec<(DockPanelId, Rect2D)>,
) {
    match node {
        DockNode::Split {
            direction,
            ratio,
            children,
        } => {
            let (first_bounds, _) = compute_split_bounds(*direction, *ratio, bounds);

            compute_leaf_bounds_recursive(&mut children[0], first_bounds, ui, result);

            let handle_bounds = match direction {
                SplitDirection::Horizontal => Rect2D::from_origin_size(
                    Vec2::new(first_bounds.max.x(), bounds.min.y()),
                    Vec2::new(SPLITTER_THICKNESS, bounds.height()),
                ),
                SplitDirection::Vertical => Rect2D::from_origin_size(
                    Vec2::new(bounds.min.x(), first_bounds.max.y()),
                    Vec2::new(bounds.width(), SPLITTER_THICKNESS),
                ),
            };

            let new_ratio = match direction {
                SplitDirection::Horizontal => {
                    let current_width = *ratio * bounds.width();
                    let new_width = ResizeHandle::horizontal(handle_bounds, current_width)
                        .min_value(bounds.width() * 0.05)
                        .max_value(bounds.width() * 0.95)
                        .show(ui);
                    (new_width / bounds.width()).clamp(0.05, 0.95)
                }
                SplitDirection::Vertical => {
                    let current_height = *ratio * bounds.height();
                    let new_height = ResizeHandle::vertical(handle_bounds, current_height)
                        .min_value(bounds.height() * 0.05)
                        .max_value(bounds.height() * 0.95)
                        .show(ui);
                    (new_height / bounds.height()).clamp(0.05, 0.95)
                }
            };
            *ratio = new_ratio;

            let (_, second_bounds) = compute_split_bounds(*direction, new_ratio, bounds);
            compute_leaf_bounds_recursive(&mut children[1], second_bounds, ui, result);
        }
        DockNode::Leaf {
            tabs,
            active_tab,
            collapsed,
        } => {
            if tabs.is_empty() {
                return;
            }
            let active_idx = (*active_tab).min(tabs.len() - 1);
            if let Some(&panel_id) = tabs.get(active_idx) {
                let content_y = bounds.min.y() + TAB_BAR_HEIGHT;
                let content_bounds = if *collapsed {
                    Rect2D::from_origin_size(
                        Vec2::new(bounds.min.x(), content_y),
                        Vec2::new(bounds.width(), 0.0),
                    )
                } else {
                    Rect2D::from_origin_size(
                        Vec2::new(bounds.min.x(), content_y),
                        Vec2::new(bounds.width(), (bounds.height() - TAB_BAR_HEIGHT).max(0.0)),
                    )
                };
                result.push((panel_id, content_bounds));
            }
        }
    }
}

/// Render dock chrome (tab bars, separator lines) without panel content.
/// Used when panel content is rendered by the declarative view tree.
#[allow(clippy::too_many_arguments)]
fn render_chrome_recursive(
    ui: &mut UiContext,
    node: &mut DockNode,
    bounds: Rect2D,
    label_fn: Option<&dyn Fn(DockPanelId) -> &'static str>,
    drag_state: &mut DockDragState,
    response: &mut DockAreaResponse,
) {
    match node {
        DockNode::Split {
            direction,
            ratio,
            children,
        } => {
            let (first_bounds, _) = compute_split_bounds(*direction, *ratio, bounds);

            render_chrome_recursive(
                ui,
                &mut children[0],
                first_bounds,
                label_fn,
                drag_state,
                response,
            );

            // Draw separator line
            let separator_color = ui.style.separator;
            match direction {
                SplitDirection::Horizontal => {
                    let sep_x = first_bounds.max.x() + SPLITTER_THICKNESS * 0.5;
                    ui.draw_line(
                        Vec2::new(sep_x, bounds.min.y()),
                        Vec2::new(sep_x, bounds.max.y()),
                        separator_color,
                        SPLITTER_THICKNESS,
                    );
                }
                SplitDirection::Vertical => {
                    let sep_y = first_bounds.max.y() + SPLITTER_THICKNESS * 0.5;
                    ui.draw_line(
                        Vec2::new(bounds.min.x(), sep_y),
                        Vec2::new(bounds.max.x(), sep_y),
                        separator_color,
                        SPLITTER_THICKNESS,
                    );
                }
            }

            let (_, second_bounds) = compute_split_bounds(*direction, *ratio, bounds);
            render_chrome_recursive(
                ui,
                &mut children[1],
                second_bounds,
                label_fn,
                drag_state,
                response,
            );
        }
        DockNode::Leaf {
            tabs,
            active_tab,
            collapsed,
        } => {
            if tabs.is_empty() {
                return;
            }

            // Draw tab bar
            let tab_bar_bounds =
                Rect2D::from_origin_size(bounds.min, Vec2::new(bounds.width(), TAB_BAR_HEIGHT));
            let tab_response = DockTabBar::new(tabs, *active_tab)
                .bounds(tab_bar_bounds)
                .labels(label_fn.unwrap_or(&|_| "?"))
                .show(ui);

            if let Some(clicked) = tab_response.clicked_tab {
                *active_tab = clicked;
            }

            if let Some(closed_idx) = tab_response.closed_tab
                && let Some(&panel_id) = tabs.get(closed_idx)
            {
                response.closed_panel = Some(panel_id);
            }

            if let Some(drag_idx) = tab_response.drag_started_tab
                && let Some(&panel_id) = tabs.get(drag_idx)
                && drag_state.dragging_panel.is_none()
            {
                drag_state.dragging_panel = Some(panel_id);
                drag_state.source_bounds = Some(bounds);
                drag_state.torn_off = false;
                response.drag_started = Some((panel_id, bounds));
            }

            let active_idx = tab_response
                .clicked_tab
                .unwrap_or(*active_tab)
                .min(tabs.len() - 1);
            *active_tab = active_idx;

            if !*collapsed {
                let content_bounds = Rect2D::from_origin_size(
                    Vec2::new(bounds.min.x(), tab_bar_bounds.max.y()),
                    Vec2::new(bounds.width(), (bounds.height() - TAB_BAR_HEIGHT).max(0.0)),
                );
                if let Some(&panel_id) = tabs.get(active_idx) {
                    ui.register_panel(panel_id, content_bounds);
                    response.visible_panels.push((panel_id, content_bounds));
                }
            }
        }
    }
}

impl<'a> DockArea<'a> {
    fn handle_drag_drop(&mut self, ui: &mut UiContext, response: &mut DockAreaResponse) {
        let mouse_pos = ui.input.mouse_pos;
        self.drag_state.mouse_pos = mouse_pos;

        // Check if mouse is still down
        if !ui.input.mouse_down[crate::input::mouse_button::LEFT] {
            // Drop!
            if let (Some(panel_id), Some(zone)) =
                (self.drag_state.dragging_panel, self.drag_state.target_zone)
            {
                let target_panel = self.layout.root.first_panel_id().unwrap_or(panel_id);

                response.dropped = Some((panel_id, zone, target_panel));
            }
            *self.drag_state = DockDragState::default();
            return;
        }

        // Check tear-off distance
        if let Some(source_bounds) = self.drag_state.source_bounds
            && !source_bounds.contains(mouse_pos)
        {
            let dist = mouse_pos - source_bounds.center();
            if dist.x().abs() > TEAR_OFF_THRESHOLD || dist.y().abs() > TEAR_OFF_THRESHOLD {
                self.drag_state.torn_off = true;
            }
        }

        // Detect dock zones by walking the tree
        self.drag_state.target_zone = None;
        self.drag_state.target_leaf_bounds = None;
        let root = self.layout.root.clone();
        self.detect_dock_zone(&root, self.bounds);
    }

    fn detect_dock_zone(&mut self, node: &DockNode, bounds: Rect2D) {
        match node {
            DockNode::Split {
                direction,
                ratio,
                children,
            } => {
                let (first_bounds, second_bounds) =
                    compute_split_bounds(*direction, *ratio, bounds);
                self.detect_dock_zone(&children[0], first_bounds);
                self.detect_dock_zone(&children[1], second_bounds);
            }
            DockNode::Leaf { tabs, .. } => {
                if tabs.is_empty() {
                    return;
                }
                let mouse_pos = self.drag_state.mouse_pos;
                if bounds.contains(mouse_pos) {
                    self.drag_state.target_leaf_bounds = Some(bounds);
                    self.drag_state.target_zone = Some(detect_zone_in_bounds(mouse_pos, bounds));
                }
            }
        }
    }

    fn render_drag_preview(&self, ui: &mut UiContext) {
        let mouse = self.drag_state.mouse_pos;
        let preview_size = Vec2::new(120.0, TAB_BAR_HEIGHT);
        let preview_bounds =
            Rect2D::from_origin_size(mouse - Vec2::new(0.0, preview_size.y()), preview_size);

        ui.draw_rect(preview_bounds, ui.style.window_title_bg);
        ui.draw_rect_border(
            preview_bounds,
            ui.style.window_title_bg,
            ui.style.window_border,
            1.0,
        );

        if let Some(label_fn) = self.panel_label_fn
            && let Some(panel_id) = self.drag_state.dragging_panel
        {
            let label = label_fn(panel_id);
            let font_size = ui.style.font_size;
            let text_size = ui.measure_text(label, font_size);
            let text_pos = Vec2::new(
                preview_bounds.min.x() + (preview_bounds.width() - text_size.x()) * 0.5,
                preview_bounds.center().y() - text_size.y() * 0.5,
            );
            ui.draw_text(label, text_pos, ui.style.text_color, font_size);
        }

        // Draw dock zone overlay on target leaf
        if let (Some(zone), Some(leaf_bounds)) = (
            self.drag_state.target_zone,
            self.drag_state.target_leaf_bounds,
        ) {
            render_dock_zone_overlay(ui, zone, leaf_bounds);
        }
    }
}

fn compute_split_bounds(direction: SplitDirection, ratio: f32, bounds: Rect2D) -> (Rect2D, Rect2D) {
    match direction {
        SplitDirection::Horizontal => {
            let first = Rect2D::from_origin_size(
                bounds.min,
                Vec2::new(bounds.width() * ratio, bounds.height()),
            );
            let split_x = bounds.min.x() + bounds.width() * ratio;
            let second = Rect2D::from_origin_size(
                Vec2::new(split_x + SPLITTER_THICKNESS, bounds.min.y()),
                Vec2::new(
                    (bounds.width() * (1.0 - ratio) - SPLITTER_THICKNESS).max(0.0),
                    bounds.height(),
                ),
            );
            (first, second)
        }
        SplitDirection::Vertical => {
            let first = Rect2D::from_origin_size(
                bounds.min,
                Vec2::new(bounds.width(), bounds.height() * ratio),
            );
            let split_y = bounds.min.y() + bounds.height() * ratio;
            let second = Rect2D::from_origin_size(
                Vec2::new(bounds.min.x(), split_y + SPLITTER_THICKNESS),
                Vec2::new(
                    bounds.width(),
                    (bounds.height() * (1.0 - ratio) - SPLITTER_THICKNESS).max(0.0),
                ),
            );
            (first, second)
        }
    }
}

fn detect_zone_in_bounds(mouse_pos: Vec2, bounds: Rect2D) -> DockZone {
    let cx = bounds.center().x();
    let cy = bounds.center().y();
    let hw = bounds.width() * 0.25;
    let hh = bounds.height() * 0.25;

    // Center zone: within 25% of center
    let dx = (mouse_pos.x() - cx).abs();
    let dy = (mouse_pos.y() - cy).abs();
    if dx < hw && dy < hh {
        return DockZone::Center;
    }

    // Determine which edge is closest
    let dist_left = mouse_pos.x() - bounds.min.x();
    let dist_right = bounds.max.x() - mouse_pos.x();
    let dist_top = mouse_pos.y() - bounds.min.y();
    let dist_bottom = bounds.max.y() - mouse_pos.y();

    let min_dist = dist_left.min(dist_right).min(dist_top).min(dist_bottom);
    if min_dist == dist_left {
        DockZone::Left
    } else if min_dist == dist_right {
        DockZone::Right
    } else if min_dist == dist_top {
        DockZone::Top
    } else {
        DockZone::Bottom
    }
}

fn render_dock_zone_overlay(ui: &mut UiContext, zone: DockZone, bounds: Rect2D) {
    let highlight = Color::new(0.3, 0.6, 1.0, 0.25);
    let border_color = Color::new(0.3, 0.6, 1.0, 0.6);

    let zone_bounds = match zone {
        DockZone::Center => bounds,
        DockZone::Left => {
            Rect2D::from_origin_size(bounds.min, Vec2::new(bounds.width() * 0.5, bounds.height()))
        }
        DockZone::Right => Rect2D::from_origin_size(
            Vec2::new(bounds.min.x() + bounds.width() * 0.5, bounds.min.y()),
            Vec2::new(bounds.width() * 0.5, bounds.height()),
        ),
        DockZone::Top => {
            Rect2D::from_origin_size(bounds.min, Vec2::new(bounds.width(), bounds.height() * 0.5))
        }
        DockZone::Bottom => Rect2D::from_origin_size(
            Vec2::new(bounds.min.x(), bounds.min.y() + bounds.height() * 0.5),
            Vec2::new(bounds.width(), bounds.height() * 0.5),
        ),
    };

    ui.draw_rect(zone_bounds, highlight);
    ui.draw_rect_border(zone_bounds, highlight, border_color, 2.0);
}

#[allow(clippy::too_many_arguments)]
fn render_node(
    render_panel: &mut dyn FnMut(&mut UiContext, Rect2D, DockPanelId),
    ui: &mut UiContext,
    node: &mut DockNode,
    bounds: Rect2D,
    label_fn: Option<&dyn Fn(DockPanelId) -> &'static str>,
    drag_state: &mut DockDragState,
    response: &mut DockAreaResponse,
) {
    match node {
        DockNode::Split {
            direction,
            ratio,
            children,
        } => {
            let (first_bounds, _) = compute_split_bounds(*direction, *ratio, bounds);

            render_node(
                render_panel,
                ui,
                &mut children[0],
                first_bounds,
                label_fn,
                drag_state,
                response,
            );

            let separator_color = ui.style.separator;
            let handle_bounds = match direction {
                SplitDirection::Horizontal => Rect2D::from_origin_size(
                    Vec2::new(first_bounds.max.x(), bounds.min.y()),
                    Vec2::new(SPLITTER_THICKNESS, bounds.height()),
                ),
                SplitDirection::Vertical => Rect2D::from_origin_size(
                    Vec2::new(bounds.min.x(), first_bounds.max.y()),
                    Vec2::new(bounds.width(), SPLITTER_THICKNESS),
                ),
            };

            let new_ratio = match direction {
                SplitDirection::Horizontal => {
                    let current_width = *ratio * bounds.width();
                    let new_width = ResizeHandle::horizontal(handle_bounds, current_width)
                        .min_value(bounds.width() * 0.05)
                        .max_value(bounds.width() * 0.95)
                        .show(ui);
                    (new_width / bounds.width()).clamp(0.05, 0.95)
                }
                SplitDirection::Vertical => {
                    let current_height = *ratio * bounds.height();
                    let new_height = ResizeHandle::vertical(handle_bounds, current_height)
                        .min_value(bounds.height() * 0.05)
                        .max_value(bounds.height() * 0.95)
                        .show(ui);
                    (new_height / bounds.height()).clamp(0.05, 0.95)
                }
            };
            *ratio = new_ratio;

            let (_, second_bounds) = compute_split_bounds(*direction, new_ratio, bounds);

            match direction {
                SplitDirection::Horizontal => {
                    let sep_x = first_bounds.max.x() + SPLITTER_THICKNESS * 0.5;
                    ui.draw_line(
                        Vec2::new(sep_x, bounds.min.y()),
                        Vec2::new(sep_x, bounds.max.y()),
                        separator_color,
                        SPLITTER_THICKNESS,
                    );
                }
                SplitDirection::Vertical => {
                    let sep_y = first_bounds.max.y() + SPLITTER_THICKNESS * 0.5;
                    ui.draw_line(
                        Vec2::new(bounds.min.x(), sep_y),
                        Vec2::new(bounds.max.x(), sep_y),
                        separator_color,
                        SPLITTER_THICKNESS,
                    );
                }
            }

            render_node(
                render_panel,
                ui,
                &mut children[1],
                second_bounds,
                label_fn,
                drag_state,
                response,
            );
        }
        DockNode::Leaf {
            tabs,
            active_tab,
            collapsed,
        } => {
            if tabs.is_empty() {
                return;
            }

            // Tab bar
            let tab_bar_bounds =
                Rect2D::from_origin_size(bounds.min, Vec2::new(bounds.width(), TAB_BAR_HEIGHT));
            let tab_response = DockTabBar::new(tabs, *active_tab)
                .bounds(tab_bar_bounds)
                .labels(label_fn.unwrap_or(&|_| "?"))
                .show(ui);

            // Handle tab click
            if let Some(clicked) = tab_response.clicked_tab {
                *active_tab = clicked;
            }

            // Handle close button
            if let Some(closed_idx) = tab_response.closed_tab
                && let Some(&panel_id) = tabs.get(closed_idx)
            {
                response.closed_panel = Some(panel_id);
            }

            // Handle drag start
            if let Some(drag_idx) = tab_response.drag_started_tab
                && let Some(&panel_id) = tabs.get(drag_idx)
                && drag_state.dragging_panel.is_none()
            {
                drag_state.dragging_panel = Some(panel_id);
                drag_state.source_bounds = Some(bounds);
                drag_state.torn_off = false;
                response.drag_started = Some((panel_id, bounds));
            }

            // Collapse toggle: double-click on tab bar background
            // (just use the collapse state directly for now)

            let active_idx = tab_response
                .clicked_tab
                .unwrap_or(*active_tab)
                .min(tabs.len() - 1);
            *active_tab = active_idx;

            if *collapsed {
                // When collapsed, only show tab bar
                return;
            }

            let content_bounds = Rect2D::from_origin_size(
                Vec2::new(bounds.min.x(), tab_bar_bounds.max.y()),
                Vec2::new(bounds.width(), (bounds.height() - TAB_BAR_HEIGHT).max(0.0)),
            );

            ui.draw_rect(content_bounds, ui.style.window_bg);

            if let Some(&panel_id) = tabs.get(active_idx) {
                ui.register_panel(panel_id, content_bounds);
                response.visible_panels.push((panel_id, content_bounds));
                render_panel(ui, content_bounds, panel_id);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Serialization stubs
// ---------------------------------------------------------------------------

impl DockLayout {
    pub fn to_string(&self) -> Result<String, String> {
        Err("DockLayout::to_string — serde not enabled for katla_ui".into())
    }

    pub fn from_string(_s: &str) -> Result<Self, String> {
        Err("DockLayout::from_string — serde not enabled for katla_ui".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_leaf_creates_leaf_with_tab_and_active_zero() {
        let node = DockNode::leaf(42);
        match node {
            DockNode::Leaf {
                tabs, active_tab, ..
            } => {
                assert_eq!(tabs, vec![42]);
                assert_eq!(active_tab, 0);
            }
            _ => panic!("expected Leaf"),
        }
    }

    #[test]
    fn test_split_creates_split_with_clamped_ratio() {
        let left = DockNode::leaf(1);
        let right = DockNode::leaf(2);
        let node = DockNode::split(SplitDirection::Horizontal, 0.5, left, right);
        match node {
            DockNode::Split {
                direction,
                ratio,
                children,
            } => {
                assert_eq!(direction, SplitDirection::Horizontal);
                assert!((ratio - 0.5f32).abs() < f32::EPSILON);
                assert!(matches!(&children[0], DockNode::Leaf { tabs, .. } if tabs == &[1]));
                assert!(matches!(&children[1], DockNode::Leaf { tabs, .. } if tabs == &[2]));
            }
            _ => panic!("expected Split"),
        }
    }

    #[test]
    fn test_find_leaf_with_panel_nested_tree() {
        let tree = DockNode::split(
            SplitDirection::Horizontal,
            0.5,
            DockNode::leaf(10),
            DockNode::leaf(20),
        );

        let found = tree.find_leaf_with_panel(10).expect("should find panel 10");
        match found {
            DockNode::Leaf { tabs, .. } => assert!(tabs.contains(&10)),
            _ => panic!("expected Leaf"),
        }

        assert!(tree.find_leaf_with_panel(99).is_none());
    }

    #[test]
    fn test_find_leaf_with_panel_mut_allows_mutation() {
        let mut tree = DockNode::split(
            SplitDirection::Horizontal,
            0.5,
            DockNode::leaf(10),
            DockNode::leaf(20),
        );

        {
            let leaf = tree
                .find_leaf_with_panel_mut(10)
                .expect("should find panel 10");
            if let DockNode::Leaf { active_tab, .. } = leaf {
                *active_tab = 42;
            }
        }

        match tree
            .find_leaf_with_panel(10)
            .expect("should still find panel 10")
        {
            DockNode::Leaf { active_tab, .. } => assert_eq!(*active_tab, 42),
            _ => panic!("expected Leaf"),
        }
    }

    #[test]
    fn test_remove_panel_from_leaf() {
        let mut node = DockNode::Leaf {
            tabs: vec![1, 2, 3],
            active_tab: 2,
            collapsed: false,
        };
        assert!(node.remove_panel(3));
        match &node {
            DockNode::Leaf {
                tabs, active_tab, ..
            } => {
                assert_eq!(*tabs, vec![1, 2]);
                assert_eq!(*active_tab, 1);
            }
            _ => panic!("expected Leaf"),
        }
    }

    #[test]
    fn test_remove_panel_from_split_collapse() {
        let mut tree = DockNode::split(
            SplitDirection::Horizontal,
            0.5,
            DockNode::leaf(10),
            DockNode::leaf(20),
        );

        assert!(tree.remove_panel(10));

        match &tree {
            DockNode::Leaf { tabs, .. } => assert!(tabs.contains(&20)),
            _ => panic!("expected collapse to a single leaf"),
        }
    }

    #[test]
    fn test_add_tab_to_leaf() {
        let mut node = DockNode::leaf(10);
        assert!(node.add_tab_to_leaf(10, 20));
        match &node {
            DockNode::Leaf {
                tabs, active_tab, ..
            } => {
                assert_eq!(*tabs, vec![10, 20]);
                assert_eq!(*active_tab, 1);
            }
            _ => panic!("expected Leaf"),
        }
    }

    #[test]
    fn test_split_leaf_right() {
        let mut tree = DockNode::leaf(1);
        assert!(tree.split_leaf(1, 2, DockZone::Right));

        match &tree {
            DockNode::Split {
                direction,
                children,
                ..
            } => {
                assert_eq!(*direction, SplitDirection::Horizontal);
                // new_on_first=false, so old (panel 1) is first, new (panel 2) is second
                assert!(matches!(&children[0], DockNode::Leaf { tabs, .. } if tabs == &[1]));
                assert!(matches!(&children[1], DockNode::Leaf { tabs, .. } if tabs == &[2]));
            }
            _ => panic!("expected Split"),
        }
    }

    #[test]
    fn test_split_leaf_left() {
        let mut tree = DockNode::leaf(1);
        assert!(tree.split_leaf(1, 2, DockZone::Left));

        match &tree {
            DockNode::Split {
                direction,
                children,
                ..
            } => {
                assert_eq!(*direction, SplitDirection::Horizontal);
                // new_on_first=true, so new (panel 2) is first, old (panel 1) is second
                assert!(matches!(&children[0], DockNode::Leaf { tabs, .. } if tabs == &[2]));
                assert!(matches!(&children[1], DockNode::Leaf { tabs, .. } if tabs == &[1]));
            }
            _ => panic!("expected Split"),
        }
    }

    #[test]
    fn test_split_leaf_center_adds_tab() {
        let mut tree = DockNode::leaf(1);
        assert!(tree.split_leaf(1, 2, DockZone::Center));
        match &tree {
            DockNode::Leaf {
                tabs, active_tab, ..
            } => {
                assert_eq!(*tabs, vec![1, 2]);
                assert_eq!(*active_tab, 1);
            }
            _ => panic!("expected Leaf with both tabs"),
        }
    }

    #[test]
    fn test_detect_zone_center() {
        let bounds = Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(400.0, 300.0));
        let zone = detect_zone_in_bounds(bounds.center(), bounds);
        assert_eq!(zone, DockZone::Center);
    }

    #[test]
    fn test_detect_zone_edges() {
        let bounds = Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(400.0, 300.0));

        let left = detect_zone_in_bounds(Vec2::new(5.0, 150.0), bounds);
        assert_eq!(left, DockZone::Left);

        let right = detect_zone_in_bounds(Vec2::new(395.0, 150.0), bounds);
        assert_eq!(right, DockZone::Right);

        let top = detect_zone_in_bounds(Vec2::new(200.0, 5.0), bounds);
        assert_eq!(top, DockZone::Top);

        let bottom = detect_zone_in_bounds(Vec2::new(200.0, 295.0), bounds);
        assert_eq!(bottom, DockZone::Bottom);
    }

    #[test]
    fn test_first_panel_id() {
        let tree = DockNode::split(
            SplitDirection::Horizontal,
            0.5,
            DockNode::leaf(1),
            DockNode::leaf(2),
        );
        assert_eq!(tree.first_panel_id(), Some(1));
    }

    #[test]
    fn test_dock_layout_single() {
        let layout = DockLayout::single(7);
        match &layout.root {
            DockNode::Leaf {
                tabs, active_tab, ..
            } => {
                assert_eq!(*tabs, vec![7]);
                assert_eq!(*active_tab, 0);
            }
            _ => panic!("expected Leaf"),
        }
        assert!(layout.floating.is_empty());
    }

    #[test]
    fn test_split_ratio_clamping() {
        let node_lo = DockNode::split(
            SplitDirection::Horizontal,
            0.0,
            DockNode::leaf(1),
            DockNode::leaf(2),
        );
        match node_lo {
            DockNode::Split { ratio, .. } => assert!((ratio - 0.1f32).abs() < f32::EPSILON),
            _ => panic!("expected Split"),
        }

        let node_hi = DockNode::split(
            SplitDirection::Horizontal,
            1.0,
            DockNode::leaf(1),
            DockNode::leaf(2),
        );
        match node_hi {
            DockNode::Split { ratio, .. } => assert!((ratio - 0.9f32).abs() < f32::EPSILON),
            _ => panic!("expected Split"),
        }
    }
}
