//! Dockable panel system — data structures, tab bar widget, and serialization.
//!
//! Provides the foundational types for a dockable panel layout:
//! - [`DockNode`] / [`DockLayout`] — tree-based split/tab layout
//! - [`DockTabBar`] — widget for rendering tabs within a dock leaf
//! - Serialization helpers (`to_string` / `from_string`)

use crate::{Response, UiContext, Widget};
use katla_math::{Rect2D, Vec2};

/// Unique identifier for a dockable panel.
pub type DockPanelId = u64;

// ---------------------------------------------------------------------------
// 157a — DockTree data structures
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

impl DockNode {
    pub fn leaf(tab: DockPanelId) -> Self {
        DockNode::Leaf {
            tabs: vec![tab],
            active_tab: 0,
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
    pub fn remove_panel(&mut self, panel_id: DockPanelId) -> bool {
        match self {
            DockNode::Split { children, .. } => {
                children[0].remove_panel(panel_id) || children[1].remove_panel(panel_id)
            }
            DockNode::Leaf { tabs, active_tab } => {
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

    /// Add a panel as a new tab to the leaf containing `target_panel_id`.
    pub fn add_tab_to_leaf(
        &mut self,
        target_panel_id: DockPanelId,
        new_panel_id: DockPanelId,
    ) -> bool {
        if let Some(leaf) = self.find_leaf_with_panel_mut(target_panel_id)
            && let DockNode::Leaf { tabs, active_tab } = leaf
        {
            tabs.push(new_panel_id);
            *active_tab = tabs.len() - 1;
            return true;
        }
        false
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
// 157b — DockTabBar widget
// ---------------------------------------------------------------------------

/// Response returned by [`DockTabBar`].
#[derive(Debug, Clone, Copy)]
pub struct DockTabBarResponse {
    /// Index of the clicked tab, if any.
    pub clicked_tab: Option<usize>,
    /// Index of the close button that was pressed, if any.
    pub closed_tab: Option<usize>,
}

/// A tab bar widget specialised for dock panel leaves.
///
/// Renders evenly distributed tabs with a bottom separator. The active tab
/// blends with the content panel below (no bottom border). Optionally shows
/// a small "×" close button on each tab.
pub struct DockTabBar<'a> {
    tabs: &'a [DockPanelId],
    active: usize,
    bounds: Rect2D,
    close_buttons: bool,
    id_base: Option<&'a str>,
}

impl<'a> DockTabBar<'a> {
    /// Create a new dock tab bar.
    ///
    /// * `tabs` — panel IDs displayed as tabs (callers should map these to
    ///   human-readable labels externally).
    /// * `active` — index of the currently active tab.
    pub fn new(tabs: &'a [DockPanelId], active: usize) -> Self {
        Self {
            tabs,
            active,
            bounds: Rect2D::from_size(Vec2::new(200.0, 28.0)),
            close_buttons: false,
            id_base: None,
        }
    }

    /// Set the tab bar bounds.
    pub fn bounds(mut self, bounds: Rect2D) -> Self {
        self.bounds = bounds;
        self
    }

    /// Enable or disable close buttons on each tab.
    pub fn close_buttons(mut self, enabled: bool) -> Self {
        self.close_buttons = enabled;
        self
    }

    /// Set an ID base for unique widget identification.
    pub fn id(mut self, id: &'a str) -> Self {
        self.id_base = Some(id);
        self
    }
}

impl Widget for DockTabBar<'_> {
    fn ui(self, ui: &mut UiContext) -> Response {
        let tab_count = self.tabs.len();
        if tab_count == 0 {
            return Response::new(self.bounds);
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

        // Bottom separator line across full width
        ui.draw_line(
            Vec2::new(self.bounds.min.x(), separator_y),
            Vec2::new(self.bounds.max.x(), separator_y),
            separator_color,
            1.0,
        );

        let mut clicked_tab: Option<usize> = None;
        let mut closed_tab: Option<usize> = None;
        let mut hovered_any = false;

        let id_prefix = self.id_base.unwrap_or("dock_tab");

        for (i, _panel_id) in self.tabs.iter().enumerate() {
            let tab_min_x = self.bounds.min.x() + tab_width * i as f32;
            let tab_bounds = Rect2D::from_origin_size(
                Vec2::new(tab_min_x, self.bounds.min.y()),
                Vec2::new(tab_width, self.bounds.height()),
            );
            let tab_id = ui.generate_id(&format!("{}_{}", id_prefix, i));

            ui.register_focusable(tab_id, tab_bounds);
            let tab_hovered = ui.update_hover(tab_id, tab_bounds);
            let is_active = i == self.active;

            // Tab background
            let bg_color = if is_active {
                active_bg
            } else if tab_hovered {
                hover_bg
            } else {
                inactive_bg
            };
            ui.draw_rect(tab_bounds, bg_color);

            // Active tab: redraw separator segments with a gap
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

            // Tab label (show panel ID as placeholder — callers can remap)
            let label = format!("{}", self.tabs[i]);
            let text_color = if is_active {
                active_text
            } else {
                inactive_text
            };

            let close_btn_width = if self.close_buttons { 16.0 } else { 0.0 };
            let label_area_width = (tab_width - close_btn_width).max(0.0);
            let text_size = ui.measure_text(&label, font_size);
            let text_pos = Vec2::new(
                tab_bounds.min.x() + (label_area_width - text_size.x()) * 0.5,
                tab_bounds.center().y() - text_size.y() * 0.5,
            );
            ui.draw_text(&label, text_pos, text_color, font_size);

            // Close button
            if self.close_buttons {
                let close_x = tab_bounds.max.x() - close_btn_width;
                let close_bounds = Rect2D::from_origin_size(
                    Vec2::new(close_x, tab_bounds.min.y()),
                    Vec2::new(close_btn_width, tab_bounds.height()),
                );
                let close_id = ui.generate_id(&format!("{}_close_{}", id_prefix, i));
                ui.register_focusable(close_id, close_bounds);
                let close_hovered = ui.update_hover(close_id, close_bounds);

                let close_text_color = if close_hovered {
                    active_text
                } else {
                    inactive_text
                };
                let close_size = ui.measure_text("×", font_size);
                let close_pos = Vec2::new(
                    close_bounds.center().x() - close_size.x() * 0.5,
                    close_bounds.center().y() - close_size.y() * 0.5,
                );
                ui.draw_text("×", close_pos, close_text_color, font_size);

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

            // Tab click detection
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
                hovered_any = true;
                ui.input.set_cursor(crate::input::MouseCursor::Hand);
            }
        }

        // Store the DockTabBarResponse for callers to retrieve via the Response.
        // We encode it in a general Response — callers check clicked_tab / closed_tab
        // via the dock_tab_bar helper method on Response (not added yet to avoid
        // coupling; instead we return a Response and expose a standalone function).
        let mut response = Response::new(self.bounds);
        response.hovered = hovered_any;
        response.clicked = clicked_tab.is_some() || closed_tab.is_some();
        response
    }
}

impl DockTabBar<'_> {
    /// Show the dock tab bar and return a specialised response.
    ///
    /// Use this instead of `ui.add()` when you need the typed
    /// [`DockTabBarResponse`] with `clicked_tab` / `closed_tab`.
    pub fn show(self, ui: &mut UiContext) -> DockTabBarResponse {
        let tab_count = self.tabs.len();
        if tab_count == 0 {
            return DockTabBarResponse {
                clicked_tab: None,
                closed_tab: None,
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

        // Bottom separator line across full width
        ui.draw_line(
            Vec2::new(self.bounds.min.x(), separator_y),
            Vec2::new(self.bounds.max.x(), separator_y),
            separator_color,
            1.0,
        );

        let mut clicked_tab: Option<usize> = None;
        let mut closed_tab: Option<usize> = None;

        let id_prefix = self.id_base.unwrap_or("dock_tab");

        for (i, _panel_id) in self.tabs.iter().enumerate() {
            let tab_min_x = self.bounds.min.x() + tab_width * i as f32;
            let tab_bounds = Rect2D::from_origin_size(
                Vec2::new(tab_min_x, self.bounds.min.y()),
                Vec2::new(tab_width, self.bounds.height()),
            );
            let tab_id = ui.generate_id(&format!("{}_{}", id_prefix, i));

            ui.register_focusable(tab_id, tab_bounds);
            let tab_hovered = ui.update_hover(tab_id, tab_bounds);
            let is_active = i == self.active;

            // Tab background
            let bg_color = if is_active {
                active_bg
            } else if tab_hovered {
                hover_bg
            } else {
                inactive_bg
            };
            ui.draw_rect(tab_bounds, bg_color);

            // Active tab: redraw separator segments with a gap
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

            // Tab label
            let label = format!("{}", self.tabs[i]);
            let text_color = if is_active {
                active_text
            } else {
                inactive_text
            };

            let close_btn_width = if self.close_buttons { 16.0 } else { 0.0 };
            let label_area_width = (tab_width - close_btn_width).max(0.0);
            let text_size = ui.measure_text(&label, font_size);
            let text_pos = Vec2::new(
                tab_bounds.min.x() + (label_area_width - text_size.x()) * 0.5,
                tab_bounds.center().y() - text_size.y() * 0.5,
            );
            ui.draw_text(&label, text_pos, text_color, font_size);

            // Close button
            if self.close_buttons {
                let close_x = tab_bounds.max.x() - close_btn_width;
                let close_bounds = Rect2D::from_origin_size(
                    Vec2::new(close_x, tab_bounds.min.y()),
                    Vec2::new(close_btn_width, tab_bounds.height()),
                );
                let close_id = ui.generate_id(&format!("{}_close_{}", id_prefix, i));
                ui.register_focusable(close_id, close_bounds);
                let close_hovered = ui.update_hover(close_id, close_bounds);

                let close_text_color = if close_hovered {
                    active_text
                } else {
                    inactive_text
                };
                let close_size = ui.measure_text("×", font_size);
                let close_pos = Vec2::new(
                    close_bounds.center().x() - close_size.x() * 0.5,
                    close_bounds.center().y() - close_size.y() * 0.5,
                );
                ui.draw_text("×", close_pos, close_text_color, font_size);

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

            // Tab click detection
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
        }
    }
}

// ---------------------------------------------------------------------------
// 157c — DockArea widget
// ---------------------------------------------------------------------------

/// Height of the tab bar rendered at the top of each leaf node.
const TAB_BAR_HEIGHT: f32 = 28.0;

/// Width of the visual separator line drawn between split children.
const SPLITTER_THICKNESS: f32 = 2.0;

/// A widget that recursively renders a [`DockLayout`].
///
/// Walks the `DockNode` tree, drawing resize separators for `Split` nodes
/// and delegating leaf rendering to a caller-provided callback.
///
/// # Example
///
/// ```ignore
/// use katla_ui::widgets::DockArea;
///
/// ui.add(DockArea::new(&layout, |ui, content_bounds, panel_id| {
///     ui.draw_text(&format!("Panel {}", panel_id), content_bounds.min, Color::WHITE, 14.0);
/// }).bounds(screen_bounds));
/// ```
pub struct DockArea<'a, F>
where
    F: FnMut(&mut UiContext, Rect2D, DockPanelId),
{
    layout: &'a mut DockLayout,
    bounds: Rect2D,
    render_panel: F,
}

impl<'a, F> DockArea<'a, F>
where
    F: FnMut(&mut UiContext, Rect2D, DockPanelId),
{
    /// Create a new dock area.
    ///
    /// * `layout` — the dock layout tree to render (mutable so resize handles
    ///   can update split ratios).
    /// * `render_panel` — callback invoked for each visible leaf panel, receiving
    ///   the content area (below the tab bar) and the active panel ID.
    pub fn new(layout: &'a mut DockLayout, render_panel: F) -> Self {
        Self {
            layout,
            bounds: Rect2D::from_size(Vec2::new(800.0, 600.0)),
            render_panel,
        }
    }

    /// Set the total bounds of the dock area.
    pub fn bounds(mut self, bounds: Rect2D) -> Self {
        self.bounds = bounds;
        self
    }
}

fn render_node<F: FnMut(&mut UiContext, Rect2D, DockPanelId)>(
    render_panel: &mut F,
    ui: &mut UiContext,
    node: &mut DockNode,
    bounds: Rect2D,
) {
    match node {
        DockNode::Split {
            direction,
            ratio,
            children,
        } => {
            let (first_bounds, second_bounds) = match direction {
                SplitDirection::Horizontal => {
                    let split_x = bounds.min.x() + bounds.width() * *ratio;
                    let first = Rect2D::from_origin_size(
                        bounds.min,
                        Vec2::new(bounds.width() * *ratio, bounds.height()),
                    );
                    let second = Rect2D::from_origin_size(
                        Vec2::new(split_x + SPLITTER_THICKNESS, bounds.min.y()),
                        Vec2::new(
                            (bounds.width() * (1.0 - *ratio) - SPLITTER_THICKNESS).max(0.0),
                            bounds.height(),
                        ),
                    );
                    (first, second)
                }
                SplitDirection::Vertical => {
                    let split_y = bounds.min.y() + bounds.height() * *ratio;
                    let first = Rect2D::from_origin_size(
                        bounds.min,
                        Vec2::new(bounds.width(), bounds.height() * *ratio),
                    );
                    let second = Rect2D::from_origin_size(
                        Vec2::new(bounds.min.x(), split_y + SPLITTER_THICKNESS),
                        Vec2::new(
                            bounds.width(),
                            (bounds.height() * (1.0 - *ratio) - SPLITTER_THICKNESS).max(0.0),
                        ),
                    );
                    (first, second)
                }
            };

            render_node(render_panel, ui, &mut children[0], first_bounds);

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
                    let new_width = super::ResizeHandle::horizontal(handle_bounds, current_width)
                        .min_value(bounds.width() * 0.1)
                        .max_value(bounds.width() * 0.9)
                        .show(ui);
                    (new_width / bounds.width()).clamp(0.1, 0.9)
                }
                SplitDirection::Vertical => {
                    let current_height = *ratio * bounds.height();
                    let new_height = super::ResizeHandle::vertical(handle_bounds, current_height)
                        .min_value(bounds.height() * 0.1)
                        .max_value(bounds.height() * 0.9)
                        .show(ui);
                    (new_height / bounds.height()).clamp(0.1, 0.9)
                }
            };
            *ratio = new_ratio;

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

            render_node(render_panel, ui, &mut children[1], second_bounds);
        }
        DockNode::Leaf { tabs, active_tab } => {
            if tabs.is_empty() {
                return;
            }

            let tab_bar_bounds =
                Rect2D::from_origin_size(bounds.min, Vec2::new(bounds.width(), TAB_BAR_HEIGHT));
            let tab_response = DockTabBar::new(tabs, *active_tab)
                .bounds(tab_bar_bounds)
                .show(ui);

            let content_bounds = Rect2D::from_origin_size(
                Vec2::new(bounds.min.x(), tab_bar_bounds.max.y()),
                Vec2::new(bounds.width(), (bounds.height() - TAB_BAR_HEIGHT).max(0.0)),
            );

            let active_idx = tab_response
                .clicked_tab
                .unwrap_or(*active_tab)
                .min(tabs.len() - 1);

            ui.draw_rect(content_bounds, ui.style.window_bg);

            if let Some(&panel_id) = tabs.get(active_idx) {
                ui.register_panel(panel_id, content_bounds);
                render_panel(ui, content_bounds, panel_id);
            }
        }
    }
}

impl<'a, F> crate::Widget for DockArea<'a, F>
where
    F: FnMut(&mut UiContext, Rect2D, DockPanelId),
{
    fn ui(mut self, ui: &mut UiContext) -> crate::Response {
        render_node(
            &mut self.render_panel,
            ui,
            &mut self.layout.root,
            self.bounds,
        );
        crate::Response::new(self.bounds)
    }
}

// ---------------------------------------------------------------------------
// 157e — Serialization
// ---------------------------------------------------------------------------
// serde / toml are not currently dependencies of katla_ui.
// Serialization stubs are provided so callers get a clear error when the
// feature is not enabled. Once `serde` and `toml` are added as optional
// dependencies these can be wired up trivially.

impl DockLayout {
    /// Serialize the layout to a string.
    ///
    /// Returns an error until the `serde` feature is enabled for `katla_ui`.
    pub fn to_string(&self) -> Result<String, String> {
        Err("DockLayout::to_string — serde not enabled for katla_ui".into())
    }

    /// Deserialize a layout from a string.
    ///
    /// Returns an error until the `serde` feature is enabled for `katla_ui`.
    pub fn from_string(_s: &str) -> Result<Self, String> {
        Err("DockLayout::from_string — serde not enabled for katla_ui".into())
    }
}
