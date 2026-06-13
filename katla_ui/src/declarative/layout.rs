use std::collections::{HashMap, HashSet};

use katla_math::{Rect2D, Vec2};
use taffy::{Dimension, LengthPercentage, NodeId as TaffyNodeId, Size, Style, TaffyTree};

use crate::style::FontSize;

use super::descriptor::{Alignment, FlexProps, Padding};
use super::state::ViewId;
use super::tree::ViewTree;

/// Function signature for measuring text dimensions during layout.
pub type MeasureFn<'a> = &'a dyn Fn(&str, Option<FontSize>) -> Vec2;

pub struct TaffyNodeMap {
    taffy: TaffyTree,
    mapping: HashMap<ViewId, TaffyNodeId>,
    reverse: HashMap<TaffyNodeId, ViewId>,
    synced_versions: HashMap<ViewId, u32>,
    last_screen_size: Option<Vec2>,
    cached_bounds: HashMap<ViewId, Rect2D>,
    dirty: bool,
}

impl Default for TaffyNodeMap {
    fn default() -> Self {
        Self::new()
    }
}

impl TaffyNodeMap {
    pub fn new() -> Self {
        Self {
            taffy: TaffyTree::new(),
            mapping: HashMap::new(),
            reverse: HashMap::new(),
            synced_versions: HashMap::new(),
            last_screen_size: None,
            cached_bounds: HashMap::new(),
            dirty: true,
        }
    }

    /// Incrementally sync the Taffy layout tree with the ViewTree.
    ///
    /// On first call, creates all Taffy nodes. On subsequent calls, only
    /// creates new nodes, updates changed nodes (detected via state_version),
    /// and removes orphaned nodes.
    pub fn sync(&mut self, tree: &mut ViewTree, measure: MeasureFn<'_>) {
        let Some(root_id) = tree.root() else {
            if !self.mapping.is_empty() {
                self.taffy.clear();
                self.mapping.clear();
                self.reverse.clear();
                self.synced_versions.clear();
                self.dirty = true;
            }
            return;
        };

        // Collect current view IDs from the tree
        let current_ids: HashSet<ViewId> = tree.iter_nodes().map(|(id, _)| id).collect();

        // Remove orphaned Taffy nodes (ViewIds that no longer exist in the tree)
        let stale: Vec<ViewId> = self
            .mapping
            .keys()
            .filter(|id| !current_ids.contains(id))
            .copied()
            .collect();
        if !stale.is_empty() {
            self.dirty = true;
        }
        for id in stale {
            if let Some(taffy_id) = self.mapping.remove(&id) {
                self.reverse.remove(&taffy_id);
                let _ = self.taffy.remove(taffy_id);
            }
            self.synced_versions.remove(&id);
        }

        // Incrementally sync the tree top-down
        self.sync_node_recursive(tree, root_id, measure);
    }

    fn sync_node_recursive(
        &mut self,
        tree: &mut ViewTree,
        view_id: ViewId,
        measure: MeasureFn<'_>,
    ) {
        // Extract all needed data from the node first to release the borrow
        // before recursing into children (which need &mut ViewTree).
        let node_data = {
            let Some(node) = tree.get(view_id) else {
                return;
            };
            let mut style = node.widget.layout_style(measure);
            if node.zstack_alignment.is_some() {
                style.position = taffy::Position::Absolute;
            }
            let children = node.children.clone();
            let state_version = node.state_version;
            let existing_taffy_id = self.mapping.get(&view_id).copied();
            (style, children, state_version, existing_taffy_id)
        };

        let (style, children, state_version, existing_taffy_id) = node_data;

        let prev_version = self.synced_versions.get(&view_id).copied().unwrap_or(0);
        let is_new = existing_taffy_id.is_none();
        let is_dirty = is_new || state_version != prev_version;

        // Recursively sync all children first (bottom-up for creation)
        for &child_id in &children {
            self.sync_node_recursive(tree, child_id, measure);
        }

        if is_new {
            // Create new Taffy node
            self.dirty = true;
            if children.is_empty() {
                let taffy_id = self.taffy.new_leaf(style).unwrap();
                self.mapping.insert(view_id, taffy_id);
                self.reverse.insert(taffy_id, view_id);
            } else {
                let child_taffy_ids: Vec<TaffyNodeId> = children
                    .iter()
                    .filter_map(|id| self.mapping.get(id).copied())
                    .collect();
                let taffy_id = self
                    .taffy
                    .new_with_children(style, &child_taffy_ids)
                    .unwrap();
                self.mapping.insert(view_id, taffy_id);
                self.reverse.insert(taffy_id, view_id);
            }
            // Write taffy_id back to ViewNode
            if let Some(node) = tree.get_mut(view_id) {
                node.taffy_id = self.mapping.get(&view_id).copied();
            }
        } else if is_dirty {
            // Update style on existing node
            self.dirty = true;
            let taffy_id = existing_taffy_id.unwrap();
            self.taffy.set_style(taffy_id, style).unwrap();
            self.update_children_if_changed(taffy_id, &children);
        } else {
            // Not dirty, but children structure might have changed
            if let Some(taffy_id) = existing_taffy_id {
                self.update_children_if_changed(taffy_id, &children);
            }
        }

        self.synced_versions.insert(view_id, state_version);
    }

    /// Update Taffy children list for a parent if it differs from the current state.
    fn update_children_if_changed(&mut self, taffy_id: TaffyNodeId, children: &[ViewId]) {
        let child_taffy_ids: Vec<TaffyNodeId> = children
            .iter()
            .filter_map(|id| self.mapping.get(id).copied())
            .collect();
        let current_taffy_children = self.taffy.children(taffy_id).unwrap_or_default();
        if current_taffy_children != child_taffy_ids {
            self.dirty = true;
            self.taffy.set_children(taffy_id, &child_taffy_ids).unwrap();
        }
    }

    /// Compute layout for the tree rooted at `root`.
    ///
    /// Skips recomputation when nothing is dirty and screen size is unchanged.
    pub fn compute(
        &mut self,
        root: ViewId,
        available_size: Vec2,
        tree: &ViewTree,
    ) -> HashMap<ViewId, Rect2D> {
        let Some(&taffy_root) = self.mapping.get(&root) else {
            return HashMap::new();
        };

        let size_changed = self.last_screen_size != Some(available_size);

        // Skip recomputation when nothing changed
        if !self.dirty && !size_changed {
            return self.cached_bounds.clone();
        }

        self.last_screen_size = Some(available_size);

        let available = Size {
            width: taffy::AvailableSpace::Definite(available_size.x()),
            height: taffy::AvailableSpace::Definite(available_size.y()),
        };

        self.taffy.compute_layout(taffy_root, available).unwrap();

        let mut bounds = HashMap::new();
        self.resolve_bounds_recursive(taffy_root, Vec2::new(0.0, 0.0), tree, &mut bounds);

        self.cached_bounds = bounds.clone();
        self.dirty = false;
        bounds
    }

    fn resolve_bounds_recursive(
        &self,
        taffy_id: TaffyNodeId,
        parent_offset: Vec2,
        tree: &ViewTree,
        bounds: &mut HashMap<ViewId, Rect2D>,
    ) {
        let layout = self.taffy.layout(taffy_id).unwrap();
        let x = parent_offset.x() + layout.location.x;
        let y = parent_offset.y() + layout.location.y;
        let w = layout.size.width;
        let h = layout.size.height;

        let rect = Rect2D::new(Vec2::new(x, y), Vec2::new(x + w, y + h));

        if let Some(&view_id) = self.reverse.get(&taffy_id) {
            bounds.insert(view_id, rect);

            if let Some(node) = tree.get(view_id) {
                for &child_id in &node.children {
                    if let Some(&child_taffy_id) = self.mapping.get(&child_id) {
                        self.resolve_bounds_recursive(
                            child_taffy_id,
                            Vec2::new(x, y),
                            tree,
                            bounds,
                        );
                    }
                }
            }
        }
    }

    /// Returns the number of Taffy nodes currently allocated.
    pub fn node_count(&self) -> usize {
        self.taffy.total_node_count()
    }

    /// Returns whether any changes were made since the last compute().
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }
}

pub fn padding_to_taffy(padding: &Padding) -> taffy::Rect<LengthPercentage> {
    taffy::Rect {
        top: LengthPercentage::Length(padding.top),
        right: LengthPercentage::Length(padding.right),
        bottom: LengthPercentage::Length(padding.bottom),
        left: LengthPercentage::Length(padding.left),
    }
}

pub fn apply_alignment_to_style(style: &mut Style, alignment: Alignment) {
    match alignment {
        // Single-axis alignments: only set the relevant property.
        // Leading/Trailing affect justify_content (main axis) without
        // overriding align_items, preserving the default Stretch so
        // children with Percent(1.0) width resolve correctly.
        Alignment::Leading => {
            style.justify_content = Some(taffy::JustifyContent::Start);
        }
        Alignment::Trailing => {
            style.justify_content = Some(taffy::JustifyContent::End);
        }
        Alignment::Center => {
            style.align_items = Some(taffy::AlignItems::Center);
            style.justify_content = Some(taffy::JustifyContent::Center);
        }
        Alignment::Top => {
            style.align_items = Some(taffy::AlignItems::Start);
        }
        Alignment::Bottom => {
            style.align_items = Some(taffy::AlignItems::End);
        }
        Alignment::TopLeading => {
            style.align_items = Some(taffy::AlignItems::Start);
            style.justify_content = Some(taffy::JustifyContent::Start);
        }
        Alignment::TopTrailing => {
            style.align_items = Some(taffy::AlignItems::End);
            style.justify_content = Some(taffy::JustifyContent::End);
        }
        Alignment::BottomLeading => {
            style.align_items = Some(taffy::AlignItems::End);
            style.justify_content = Some(taffy::JustifyContent::Start);
        }
        Alignment::BottomTrailing => {
            style.align_items = Some(taffy::AlignItems::End);
            style.justify_content = Some(taffy::JustifyContent::End);
        }
        Alignment::BottomCenter => {
            style.align_items = Some(taffy::AlignItems::End);
            style.justify_content = Some(taffy::JustifyContent::Center);
        }
    }
}

pub fn apply_flex_props(style: &mut Style, props: &FlexProps) {
    if let Some(w) = props.width {
        style.size.width = Dimension::Length(w);
    }
    if let Some(h) = props.height {
        style.size.height = Dimension::Length(h);
    }
    if let Some(w) = props.min_width {
        style.min_size.width = Dimension::Length(w);
    }
    if let Some(h) = props.min_height {
        style.min_size.height = Dimension::Length(h);
    }
    if let Some(w) = props.max_width {
        style.max_size.width = Dimension::Length(w);
    }
    if let Some(h) = props.max_height {
        style.max_size.height = Dimension::Length(h);
    }
    style.flex_grow = props.flex_grow;
    style.flex_shrink = props.flex_shrink;
    style.aspect_ratio = props.aspect_ratio;
}

pub fn measure_text_descriptor(content: &str, font_size: Option<FontSize>) -> Vec2 {
    let size = font_size.unwrap_or(FontSize::Medium);
    let height = size.to_pixels();
    let char_width = height * 0.6;
    let width = char_width * content.chars().count() as f32;
    Vec2::new(width, height)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::declarative::constructors::*;
    use crate::declarative::descriptor::Anchor;
    use crate::declarative::widget::WidgetBox;
    use katla_math::Vec2;

    fn approx_eq(a: f32, b: f32) -> bool {
        (a - b).abs() < 1.0
    }

    fn build_tree(tree: &mut ViewTree, widget: Box<dyn crate::declarative::widget::Widget>) {
        tree.set_root(widget);
    }

    /// Build a tree, sync+compute for incremental layout, and also do a
    /// fresh full-rebuild layout. Assert that bounds match.
    fn assert_incremental_matches_full_rebuild(
        tree: &mut ViewTree,
        incremental: &mut TaffyNodeMap,
        screen_size: Vec2,
    ) -> HashMap<ViewId, Rect2D> {
        incremental.sync(tree, &measure_text_descriptor);
        let incremental_bounds = incremental.compute(tree.root().unwrap(), screen_size, tree);

        // Fresh full-rebuild for comparison
        let mut fresh = TaffyNodeMap::new();
        fresh.sync(tree, &measure_text_descriptor);
        let fresh_bounds = fresh.compute(tree.root().unwrap(), screen_size, tree);

        for id in tree.iter_nodes().map(|(id, _)| id) {
            let inc = incremental_bounds.get(&id);
            let fresh = fresh_bounds.get(&id);
            assert_eq!(
                inc.is_some(),
                fresh.is_some(),
                "ViewId {:?}: incremental has entry={}, fresh has entry={}",
                id,
                inc.is_some(),
                fresh.is_some(),
            );
            if let (Some(inc_b), Some(fresh_b)) = (inc, fresh) {
                assert!(
                    (inc_b.min.x() - fresh_b.min.x()).abs() < 0.5,
                    "ViewId {:?}: min.x mismatch: incremental={} fresh={}",
                    id,
                    inc_b.min.x(),
                    fresh_b.min.x(),
                );
                assert!(
                    (inc_b.min.y() - fresh_b.min.y()).abs() < 0.5,
                    "ViewId {:?}: min.y mismatch: incremental={} fresh={}",
                    id,
                    inc_b.min.y(),
                    fresh_b.min.y(),
                );
                assert!(
                    (inc_b.max.x() - fresh_b.max.x()).abs() < 0.5,
                    "ViewId {:?}: max.x mismatch: incremental={} fresh={}",
                    id,
                    inc_b.max.x(),
                    fresh_b.max.x(),
                );
                assert!(
                    (inc_b.max.y() - fresh_b.max.y()).abs() < 0.5,
                    "ViewId {:?}: max.y mismatch: incremental={} fresh={}",
                    id,
                    inc_b.max.y(),
                    fresh_b.max.y(),
                );
            }
        }

        incremental_bounds
    }

    // --- Existing tests (updated for &mut ViewTree) ---

    #[test]
    fn test_measure_text() {
        let size = measure_text_descriptor("Hello", None);
        assert!(size.x() > 0.0);
        assert!(size.y() > 0.0);
    }

    #[test]
    fn test_hstack_layout() {
        let mut tree = ViewTree::new();
        let descriptor = hstack([text("A").boxed(), text("B").boxed()])
            .spacing(10.0)
            .padding(Padding::all(8.0))
            .align(Alignment::Leading);

        build_tree(&mut tree, descriptor.boxed());

        let mut layout = TaffyNodeMap::new();
        layout.sync(&mut tree, &measure_text_descriptor);

        let bounds = layout.compute(tree.root().unwrap(), Vec2::new(800.0, 600.0), &tree);

        let root_bounds = bounds
            .get(&tree.root().unwrap())
            .copied()
            .unwrap_or_default();

        assert!(root_bounds.width() > 0.0);
        assert!(root_bounds.height() > 0.0);
    }

    #[test]
    fn test_vstack_layout() {
        let mut tree = ViewTree::new();
        let descriptor = vstack([text("Line 1").boxed(), text("Line 2").boxed()])
            .spacing(4.0)
            .padding(Padding::zero())
            .align(Alignment::Leading);

        build_tree(&mut tree, descriptor.boxed());

        let mut layout = TaffyNodeMap::new();
        layout.sync(&mut tree, &measure_text_descriptor);

        let bounds = layout.compute(tree.root().unwrap(), Vec2::new(800.0, 600.0), &tree);

        let root_bounds = bounds
            .get(&tree.root().unwrap())
            .copied()
            .unwrap_or_default();

        assert!(root_bounds.height() > 0.0);

        let children: Vec<_> = tree
            .get(tree.root().unwrap())
            .map(|n| n.children.clone())
            .unwrap_or_default();
        assert_eq!(children.len(), 2);

        if let (Some(b1), Some(b2)) = (bounds.get(&children[0]), bounds.get(&children[1])) {
            assert!(
                b1.min.y() < b2.min.y(),
                "VStack children should be vertically stacked"
            );
        }
    }

    #[test]
    fn test_bounds_accumulation() {
        let mut tree = ViewTree::new();
        let descriptor = vstack([text("Top").boxed(), text("Bottom").boxed()])
            .spacing(0.0)
            .padding(Padding::zero())
            .align(Alignment::Leading);

        build_tree(&mut tree, descriptor.boxed());

        let mut layout = TaffyNodeMap::new();
        layout.sync(&mut tree, &measure_text_descriptor);

        let bounds = layout.compute(tree.root().unwrap(), Vec2::new(800.0, 600.0), &tree);

        let children: Vec<_> = tree
            .get(tree.root().unwrap())
            .map(|n| n.children.clone())
            .unwrap_or_default();
        assert_eq!(children.len(), 2);

        if let (Some(b1), Some(b2)) = (bounds.get(&children[0]), bounds.get(&children[1])) {
            assert!(
                approx_eq(b1.min.x(), 0.0),
                "First child should start at x=0"
            );
            assert!(
                approx_eq(b2.min.x(), 0.0),
                "Second child should start at x=0"
            );
            assert!(
                b2.min.y() >= b1.min.y(),
                "Second child should be below first"
            );
        }
    }

    #[test]
    fn test_padding_applied() {
        let mut tree = ViewTree::new();
        let descriptor = hstack([text("X").boxed()])
            .spacing(0.0)
            .padding(Padding::all(20.0))
            .align(Alignment::Leading);

        build_tree(&mut tree, descriptor.boxed());

        let mut layout = TaffyNodeMap::new();
        layout.sync(&mut tree, &measure_text_descriptor);

        let bounds = layout.compute(tree.root().unwrap(), Vec2::new(800.0, 600.0), &tree);

        let children: Vec<_> = tree
            .get(tree.root().unwrap())
            .map(|n| n.children.clone())
            .unwrap_or_default();
        assert_eq!(children.len(), 1);

        if let (Some(root), Some(child)) =
            (bounds.get(&tree.root().unwrap()), bounds.get(&children[0]))
        {
            assert!(
                child.min.x() >= root.min.x() + 20.0 - 0.5,
                "Child x should account for padding: child_x={} root_x={}",
                child.min.x(),
                root.min.x(),
            );
            assert!(
                child.min.y() >= root.min.y() + 20.0 - 0.5,
                "Child y should account for padding: child_y={} root_y={}",
                child.min.y(),
                root.min.y(),
            );
        }
    }

    #[test]
    fn test_section_layout() {
        use crate::declarative::state::{StateArena, ViewId};

        let mut tree = ViewTree::new();
        let mut arena = StateArena::default();
        let state_id = arena.get_or_create(ViewId::default(), false);
        let descriptor = section("My Section", text("content").boxed(), state_id);

        build_tree(&mut tree, descriptor.boxed());

        let mut layout = TaffyNodeMap::new();
        layout.sync(&mut tree, &measure_text_descriptor);

        let bounds = layout.compute(tree.root().unwrap(), Vec2::new(800.0, 600.0), &tree);

        let root_bounds = bounds
            .get(&tree.root().unwrap())
            .copied()
            .unwrap_or_default();

        assert!(root_bounds.width() > 0.0);
        assert!(root_bounds.height() > 0.0);
    }

    #[test]
    fn test_tab_bar_layout() {
        use crate::declarative::state::{StateArena, ViewId};

        let mut tree = ViewTree::new();
        let mut arena = StateArena::default();
        let state_id = arena.get_or_create(ViewId::default(), 0usize);
        let tabs = vec![tab_item("Tab 1"), tab_item("Tab 2")];
        let descriptor = tab_bar(tabs, state_id, text("content").boxed());

        build_tree(&mut tree, descriptor.boxed());

        let mut layout = TaffyNodeMap::new();
        layout.sync(&mut tree, &measure_text_descriptor);

        let bounds = layout.compute(tree.root().unwrap(), Vec2::new(800.0, 600.0), &tree);

        let root_bounds = bounds
            .get(&tree.root().unwrap())
            .copied()
            .unwrap_or_default();

        assert!(root_bounds.width() > 0.0);
        assert!(root_bounds.height() > 0.0);
    }

    #[test]
    fn test_grid_layout() {
        let mut tree = ViewTree::new();
        let descriptor = grid(
            2,
            Vec2::new(100.0, 50.0),
            vec![text("A").boxed(), text("B").boxed(), text("C").boxed()],
        );

        build_tree(&mut tree, descriptor.boxed());

        let mut layout = TaffyNodeMap::new();
        layout.sync(&mut tree, &measure_text_descriptor);

        let bounds = layout.compute(tree.root().unwrap(), Vec2::new(800.0, 600.0), &tree);

        let root_bounds = bounds
            .get(&tree.root().unwrap())
            .copied()
            .unwrap_or_default();

        assert!(root_bounds.width() > 0.0);
        assert!(root_bounds.height() > 0.0);
    }

    #[test]
    fn test_separator_layout() {
        let mut tree = ViewTree::new();
        let descriptor = separator_horizontal();

        build_tree(&mut tree, descriptor.boxed());

        let mut layout = TaffyNodeMap::new();
        layout.sync(&mut tree, &measure_text_descriptor);

        let bounds = layout.compute(tree.root().unwrap(), Vec2::new(800.0, 600.0), &tree);

        let root_bounds = bounds
            .get(&tree.root().unwrap())
            .copied()
            .unwrap_or_default();

        assert!(root_bounds.width() > 0.0);
        assert!(root_bounds.height() >= 0.0);
    }

    #[test]
    fn test_icon_layout() {
        let mut tree = ViewTree::new();
        let descriptor = icon('X');

        build_tree(&mut tree, descriptor.boxed());

        let mut layout = TaffyNodeMap::new();
        layout.sync(&mut tree, &measure_text_descriptor);

        let bounds = layout.compute(tree.root().unwrap(), Vec2::new(800.0, 600.0), &tree);

        let root_bounds = bounds
            .get(&tree.root().unwrap())
            .copied()
            .unwrap_or_default();

        assert!(root_bounds.width() > 0.0);
        assert!(root_bounds.height() > 0.0);
    }

    #[test]
    fn test_selectable_layout() {
        let mut tree = ViewTree::new();
        let descriptor = selectable(text("Select me").boxed());

        build_tree(&mut tree, descriptor.boxed());

        let mut layout = TaffyNodeMap::new();
        layout.sync(&mut tree, &measure_text_descriptor);

        let bounds = layout.compute(tree.root().unwrap(), Vec2::new(800.0, 600.0), &tree);

        let root_bounds = bounds
            .get(&tree.root().unwrap())
            .copied()
            .unwrap_or_default();

        assert!(root_bounds.width() > 0.0);
        assert!(root_bounds.height() > 0.0);
    }

    #[test]
    fn test_zstack_in_selectable_fills_parent() {
        use crate::types::TextureId;
        use katla_math::Color;

        let cell_w = 800.0;
        let cell_h = 600.0;
        let cell_size = Vec2::new(cell_w, cell_h);

        let img = image(TextureId(0), Color::WHITE);
        let img = img.image_size(cell_w, cell_h);

        let inner_zstack = zstack([(Alignment::Center, img.boxed())]);
        let sel = selectable(inner_zstack.boxed());
        let desc = grid(1, cell_size, [sel.boxed()])
            .flex_width(cell_w)
            .flex_height(cell_h);

        let mut tree = ViewTree::new();
        build_tree(&mut tree, desc.boxed());

        let mut layout = TaffyNodeMap::new();
        layout.sync(&mut tree, &measure_text_descriptor);

        let screen = Vec2::new(1200.0, 800.0);
        let bounds = layout.compute(tree.root().unwrap(), screen, &tree);

        let root = tree.root().unwrap();
        let root_node = tree.get(root).unwrap();
        let vstack_id = root_node.children[0];
        let vstack_node = tree.get(vstack_id).unwrap();
        let selectable_id = vstack_node.children[0];
        let selectable_node = tree.get(selectable_id).unwrap();
        let zstack_id = selectable_node.children[0];
        let zstack_node = tree.get(zstack_id).unwrap();
        let _image_id = zstack_node.children[0];

        let zstack_bounds = bounds.get(&zstack_id).copied().unwrap_or_default();

        assert!(
            approx_eq(zstack_bounds.width(), cell_w),
            "ZStack width should be {} but got {}",
            cell_w,
            zstack_bounds.width()
        );
        assert!(
            approx_eq(zstack_bounds.height(), cell_h),
            "ZStack height should be {} but got {}",
            cell_h,
            zstack_bounds.height()
        );
    }

    // --- Incremental layout caching tests ---

    #[test]
    fn test_incremental_vstack_matches_full_rebuild() {
        let mut tree = ViewTree::new();
        let desc = vstack([text("A").boxed(), text("B").boxed(), text("C").boxed()])
            .spacing(8.0)
            .padding(Padding::all(10.0))
            .align(Alignment::Leading);
        build_tree(&mut tree, desc.boxed());

        let mut layout = TaffyNodeMap::new();
        assert_incremental_matches_full_rebuild(&mut tree, &mut layout, Vec2::new(800.0, 600.0));
    }

    #[test]
    fn test_incremental_hstack_matches_full_rebuild() {
        let mut tree = ViewTree::new();
        let desc = hstack([text("A").boxed(), text("B").boxed(), text("C").boxed()])
            .spacing(12.0)
            .padding(Padding::all(16.0))
            .align(Alignment::Leading);
        build_tree(&mut tree, desc.boxed());

        let mut layout = TaffyNodeMap::new();
        assert_incremental_matches_full_rebuild(&mut tree, &mut layout, Vec2::new(800.0, 600.0));
    }

    #[test]
    fn test_incremental_zstack_matches_full_rebuild() {
        let mut tree = ViewTree::new();
        let desc = zstack([
            (Alignment::TopLeading, text("A").boxed()),
            (Alignment::Center, text("B").boxed()),
            (Alignment::BottomTrailing, text("C").boxed()),
        ]);
        build_tree(&mut tree, desc.boxed());

        let mut layout = TaffyNodeMap::new();
        assert_incremental_matches_full_rebuild(&mut tree, &mut layout, Vec2::new(800.0, 600.0));
    }

    #[test]
    fn test_incremental_grid_matches_full_rebuild() {
        let mut tree = ViewTree::new();
        let desc = grid(
            2,
            Vec2::new(100.0, 50.0),
            vec![
                text("A").boxed(),
                text("B").boxed(),
                text("C").boxed(),
                text("D").boxed(),
            ],
        );
        build_tree(&mut tree, desc.boxed());

        let mut layout = TaffyNodeMap::new();
        assert_incremental_matches_full_rebuild(&mut tree, &mut layout, Vec2::new(800.0, 600.0));
    }

    #[test]
    fn test_dirty_flag_widget_type_change() {
        let mut tree = ViewTree::new();
        build_tree(
            &mut tree,
            vstack([text("A").boxed(), text("B").boxed()]).boxed(),
        );
        let screen = Vec2::new(800.0, 600.0);

        let mut layout = TaffyNodeMap::new();
        layout.sync(&mut tree, &measure_text_descriptor);
        let bounds_before = layout.compute(tree.root().unwrap(), screen, &tree);

        // Replace a Text child with a Button (different widget type → Replace)
        build_tree(
            &mut tree,
            vstack([text("A").boxed(), button("Click me").boxed()]).boxed(),
        );

        layout.sync(&mut tree, &measure_text_descriptor);
        let bounds_after = layout.compute(tree.root().unwrap(), screen, &tree);

        // Bounds should differ because Button has different layout from Text
        let root = tree.root().unwrap();
        let children = tree.get(root).unwrap().children.clone();
        let _child_b_before = bounds_before.get(&children[1]).copied();
        let child_b_after = bounds_after.get(&children[1]).copied();

        // At minimum, the button should exist and have valid bounds
        assert!(child_b_after.is_some(), "Button child should have bounds");
        assert!(child_b_after.unwrap().width() > 0.0);
    }

    #[test]
    fn test_dirty_flag_inplace_update() {
        let mut tree = ViewTree::new();
        build_tree(
            &mut tree,
            vstack([text("Hello").boxed(), text("World").boxed()]).boxed(),
        );
        let screen = Vec2::new(800.0, 600.0);

        let mut layout = TaffyNodeMap::new();
        layout.sync(&mut tree, &measure_text_descriptor);
        let bounds1 = layout.compute(tree.root().unwrap(), screen, &tree);

        // Update with same tree (Text → Text, same content → Update)
        build_tree(
            &mut tree,
            vstack([text("Hello").boxed(), text("World").boxed()]).boxed(),
        );
        layout.sync(&mut tree, &measure_text_descriptor);
        let bounds2 = layout.compute(tree.root().unwrap(), screen, &tree);

        // Bounds should be identical since the widget tree is the same
        for id in tree.iter_nodes().map(|(id, _)| id) {
            let b1 = bounds1.get(&id).copied().unwrap_or_default();
            let b2 = bounds2.get(&id).copied().unwrap_or_default();
            assert!(
                approx_eq(b1.min.x(), b2.min.x()),
                "Bounds should match after same-tree update"
            );
            assert!(
                approx_eq(b1.width(), b2.width()),
                "Width should match after same-tree update"
            );
        }
    }

    #[test]
    fn test_dirty_flag_container_property_change() {
        let mut tree = ViewTree::new();
        build_tree(
            &mut tree,
            vstack([text("A").boxed(), text("B").boxed()])
                .spacing(8.0)
                .boxed(),
        );
        let screen = Vec2::new(800.0, 600.0);

        let mut layout = TaffyNodeMap::new();
        layout.sync(&mut tree, &measure_text_descriptor);
        let bounds1 = layout.compute(tree.root().unwrap(), screen, &tree);

        let root = tree.root().unwrap();
        let children1 = tree.get(root).unwrap().children.clone();

        // Change spacing from 8.0 to 16.0
        build_tree(
            &mut tree,
            vstack([text("A").boxed(), text("B").boxed()])
                .spacing(16.0)
                .boxed(),
        );
        layout.sync(&mut tree, &measure_text_descriptor);
        let bounds2 = layout.compute(tree.root().unwrap(), screen, &tree);

        let children2 = tree.get(root).unwrap().children.clone();

        // Children should be further apart with larger spacing
        let b1_first = bounds1.get(&children1[0]).copied().unwrap_or_default();
        let b1_second = bounds1.get(&children1[1]).copied().unwrap_or_default();
        let gap1 = b1_second.min.y() - b1_first.max.y();

        let b2_first = bounds2.get(&children2[0]).copied().unwrap_or_default();
        let b2_second = bounds2.get(&children2[1]).copied().unwrap_or_default();
        let gap2 = b2_second.min.y() - b2_first.max.y();

        assert!(
            gap2 > gap1,
            "Gap should increase with larger spacing: was={}, now={}",
            gap1,
            gap2
        );
    }

    #[test]
    fn test_layout_skipped_when_unchanged() {
        let mut tree = ViewTree::new();
        build_tree(
            &mut tree,
            vstack([text("A").boxed(), text("B").boxed()]).boxed(),
        );
        let screen = Vec2::new(800.0, 600.0);

        let mut layout = TaffyNodeMap::new();
        layout.sync(&mut tree, &measure_text_descriptor);
        let bounds1 = layout.compute(tree.root().unwrap(), screen, &tree);

        // Second sync+compute with identical tree should skip recomputation
        layout.sync(&mut tree, &measure_text_descriptor);
        assert!(
            !layout.is_dirty(),
            "Should not be dirty after syncing identical tree"
        );
        let bounds2 = layout.compute(tree.root().unwrap(), screen, &tree);

        // Bounds should be identical (returned from cache)
        for id in tree.iter_nodes().map(|(id, _)| id) {
            let b1 = bounds1.get(&id).copied().unwrap_or_default();
            let b2 = bounds2.get(&id).copied().unwrap_or_default();
            assert!(
                (b1.min.x() - b2.min.x()).abs() < 0.01,
                "Cached bounds should be identical"
            );
            assert!(
                (b1.width() - b2.width()).abs() < 0.01,
                "Cached width should be identical"
            );
        }
    }

    #[test]
    fn test_add_child_incremental() {
        let mut tree = ViewTree::new();
        build_tree(
            &mut tree,
            vstack([text("A").boxed(), text("B").boxed()]).boxed(),
        );
        let screen = Vec2::new(800.0, 600.0);

        let mut layout = TaffyNodeMap::new();
        layout.sync(&mut tree, &measure_text_descriptor);
        let _bounds1 = layout.compute(tree.root().unwrap(), screen, &tree);

        let root = tree.root().unwrap();
        assert_eq!(tree.get(root).unwrap().children.len(), 2);

        // Add a third child
        build_tree(
            &mut tree,
            vstack([text("A").boxed(), text("B").boxed(), text("C").boxed()]).boxed(),
        );
        layout.sync(&mut tree, &measure_text_descriptor);
        let bounds2 = layout.compute(tree.root().unwrap(), screen, &tree);

        assert_eq!(tree.get(root).unwrap().children.len(), 3);
        assert_incremental_matches_full_rebuild(&mut tree, &mut layout, screen);

        // Third child should have valid bounds
        let children = tree.get(root).unwrap().children.clone();
        let child_c_bounds = bounds2.get(&children[2]).copied().unwrap_or_default();
        assert!(child_c_bounds.width() > 0.0, "New child should have width");
    }

    #[test]
    fn test_remove_child_incremental() {
        let mut tree = ViewTree::new();
        build_tree(
            &mut tree,
            vstack([text("A").boxed(), text("B").boxed(), text("C").boxed()]).boxed(),
        );
        let screen = Vec2::new(800.0, 600.0);

        let mut layout = TaffyNodeMap::new();
        layout.sync(&mut tree, &measure_text_descriptor);
        let _bounds1 = layout.compute(tree.root().unwrap(), screen, &tree);

        // Remove one child
        build_tree(
            &mut tree,
            vstack([text("A").boxed(), text("C").boxed()]).boxed(),
        );
        layout.sync(&mut tree, &measure_text_descriptor);
        let bounds2 = layout.compute(tree.root().unwrap(), screen, &tree);

        let root = tree.root().unwrap();
        assert_eq!(tree.get(root).unwrap().children.len(), 2);

        // Taffy node count should match tree node count
        let tree_count = tree.iter_nodes().count();
        assert_eq!(
            layout.node_count(),
            tree_count,
            "Taffy node count should match ViewTree node count"
        );

        // Bounds should match full rebuild
        assert_incremental_matches_full_rebuild(&mut tree, &mut layout, screen);

        // Check bounds map has entries for all ViewIds
        assert_eq!(bounds2.len(), tree_count);
    }

    #[test]
    fn test_reorder_keyed_children() {
        let mut tree = ViewTree::new();
        build_tree(
            &mut tree,
            vstack_keyed(vec![
                keyed(1, text("first").boxed()),
                keyed(2, text("second").boxed()),
                keyed(3, text("third").boxed()),
            ])
            .boxed(),
        );
        let screen = Vec2::new(800.0, 600.0);

        let mut layout = TaffyNodeMap::new();
        layout.sync(&mut tree, &measure_text_descriptor);
        let _bounds1 = layout.compute(tree.root().unwrap(), screen, &tree);

        // Reorder children: [1,2,3] → [3,1,2]
        build_tree(
            &mut tree,
            vstack_keyed(vec![
                keyed(3, text("third").boxed()),
                keyed(1, text("first").boxed()),
                keyed(2, text("second").boxed()),
            ])
            .boxed(),
        );
        layout.sync(&mut tree, &measure_text_descriptor);
        let bounds2 = layout.compute(tree.root().unwrap(), screen, &tree);

        let root = tree.root().unwrap();
        let children = tree.get(root).unwrap().children.clone();
        assert_eq!(children.len(), 3);

        // All children should have bounds
        for (i, &child_id) in children.iter().enumerate() {
            let b = bounds2.get(&child_id).copied().unwrap_or_default();
            assert!(b.width() > 0.0, "Child {} should have width", i);
        }

        // Bounds should match full rebuild
        assert_incremental_matches_full_rebuild(&mut tree, &mut layout, screen);
    }

    #[test]
    fn test_unkeyed_children_index_match() {
        let mut tree = ViewTree::new();
        build_tree(
            &mut tree,
            vstack([text("A").boxed(), text("B").boxed(), text("C").boxed()]).boxed(),
        );
        let screen = Vec2::new(800.0, 600.0);

        let mut layout = TaffyNodeMap::new();
        layout.sync(&mut tree, &measure_text_descriptor);
        let _bounds1 = layout.compute(tree.root().unwrap(), screen, &tree);

        // Update with same count but different text (unkeyed, matched by index)
        build_tree(
            &mut tree,
            vstack([text("X").boxed(), text("Y").boxed(), text("Z").boxed()]).boxed(),
        );
        layout.sync(&mut tree, &measure_text_descriptor);
        let _bounds2 = layout.compute(tree.root().unwrap(), screen, &tree);

        let root = tree.root().unwrap();
        let children = tree.get(root).unwrap().children.clone();
        assert_eq!(children.len(), 3);

        // Same ViewIds should be preserved (unkeyed matches by index)
        let _orig_children: Vec<_> = {
            // Rebuild original tree to get original IDs
            let mut tree2 = ViewTree::new();
            build_tree(
                &mut tree2,
                vstack([text("A").boxed(), text("B").boxed(), text("C").boxed()]).boxed(),
            );
            tree2.get(tree2.root().unwrap()).unwrap().children.clone()
        };

        // Root should be same across builds
        // Bounds should match full rebuild
        assert_incremental_matches_full_rebuild(&mut tree, &mut layout, screen);
    }

    #[test]
    fn test_empty_tree() {
        let mut tree = ViewTree::new();
        // No root set → empty tree

        let mut layout = TaffyNodeMap::new();
        layout.sync(&mut tree, &measure_text_descriptor);

        // Should not panic, returns empty bounds
        let bounds = layout.compute(
            ViewId::from(slotmap::KeyData::from_ffi(0)), // dummy id
            Vec2::new(800.0, 600.0),
            &tree,
        );
        assert!(bounds.is_empty());
    }

    #[test]
    fn test_single_node_tree() {
        let mut tree = ViewTree::new();
        build_tree(&mut tree, text("Hello").boxed());
        let screen = Vec2::new(800.0, 600.0);

        let mut layout = TaffyNodeMap::new();
        layout.sync(&mut tree, &measure_text_descriptor);
        let bounds = layout.compute(tree.root().unwrap(), screen, &tree);

        let root = tree.root().unwrap();
        let root_bounds = bounds.get(&root).copied().unwrap_or_default();
        assert!(root_bounds.width() > 0.0, "Single node should have width");
        assert!(root_bounds.height() > 0.0, "Single node should have height");

        // Match full rebuild
        assert_incremental_matches_full_rebuild(&mut tree, &mut layout, screen);
    }

    #[test]
    fn test_deeply_nested_tree() {
        let mut tree = ViewTree::new();
        // Build: panel > panel > ... > text (10 levels)
        let inner = text("Deep").boxed();
        let mut widget: Box<dyn crate::declarative::widget::Widget> = inner;
        for _ in 0..10 {
            widget = panel("Label", widget).boxed();
        }
        build_tree(&mut tree, widget);
        let screen = Vec2::new(800.0, 600.0);

        let mut layout = TaffyNodeMap::new();
        layout.sync(&mut tree, &measure_text_descriptor);
        let bounds = layout.compute(tree.root().unwrap(), screen, &tree);

        // All nodes should have bounds
        let node_count = tree.iter_nodes().count();
        assert!(node_count > 10, "Should have deeply nested nodes");
        assert_eq!(bounds.len(), node_count, "All nodes should have bounds");

        assert_incremental_matches_full_rebuild(&mut tree, &mut layout, screen);
    }

    #[test]
    fn test_screen_size_change_triggers_recomputation() {
        let mut tree = ViewTree::new();
        // Use a container that expands to fill available space
        build_tree(
            &mut tree,
            hstack([text("A").boxed(), text("B").boxed()])
                .flex_width(800.0)
                .flex_height(600.0)
                .boxed(),
        );

        let mut layout = TaffyNodeMap::new();
        layout.sync(&mut tree, &measure_text_descriptor);
        let bounds1 = layout.compute(tree.root().unwrap(), Vec2::new(800.0, 600.0), &tree);

        // Second compute with same size should not be dirty and should return cached bounds
        let bounds2 = layout.compute(tree.root().unwrap(), Vec2::new(800.0, 600.0), &tree);
        for id in tree.iter_nodes().map(|(id, _)| id) {
            let b1 = bounds1.get(&id).copied().unwrap_or_default();
            let b2 = bounds2.get(&id).copied().unwrap_or_default();
            assert!(
                (b1.width() - b2.width()).abs() < 0.01,
                "Same screen size should produce identical bounds"
            );
        }

        // Third compute with different size should recompute
        // Using a smaller size so the fixed-size container overflows or repositions
        let bounds3 = layout.compute(tree.root().unwrap(), Vec2::new(400.0, 300.0), &tree);
        let root_bounds3 = bounds3
            .get(&tree.root().unwrap())
            .copied()
            .unwrap_or_default();
        let _root_bounds1 = bounds1
            .get(&tree.root().unwrap())
            .copied()
            .unwrap_or_default();
        // With fixed 800x600 size, bounds should stay 800x600 regardless of available space
        // But the compute() should have been re-invoked (not cached)
        // The key check is that bounds3 is valid and was freshly computed
        assert!(
            root_bounds3.width() > 0.0,
            "Should have valid bounds after resize"
        );

        // Now test with a container that responds to available space
        let mut tree2 = ViewTree::new();
        build_tree(
            &mut tree2,
            hstack([text("A").boxed(), text("B").boxed()]).boxed(),
        );
        let mut layout2 = TaffyNodeMap::new();
        layout2.sync(&mut tree2, &measure_text_descriptor);
        let b_small = layout2.compute(tree2.root().unwrap(), Vec2::new(100.0, 100.0), &tree2);
        let b_large = layout2.compute(tree2.root().unwrap(), Vec2::new(2000.0, 2000.0), &tree2);
        // Both should produce valid bounds (shrink-wrap doesn't depend on available space)
        assert!(
            b_small
                .get(&tree2.root().unwrap())
                .copied()
                .unwrap_or_default()
                .width()
                > 0.0
        );
        assert!(
            b_large
                .get(&tree2.root().unwrap())
                .copied()
                .unwrap_or_default()
                .width()
                > 0.0
        );
    }

    #[test]
    fn test_screen_size_unchanged_no_recomputation() {
        let mut tree = ViewTree::new();
        let screen = Vec2::new(800.0, 600.0);
        build_tree(
            &mut tree,
            vstack([text("A").boxed(), text("B").boxed()]).boxed(),
        );

        let mut layout = TaffyNodeMap::new();
        layout.sync(&mut tree, &measure_text_descriptor);
        let b1 = layout.compute(tree.root().unwrap(), screen, &tree);

        // Sync same tree again — should not be dirty
        layout.sync(&mut tree, &measure_text_descriptor);
        assert!(!layout.is_dirty());

        let b2 = layout.compute(tree.root().unwrap(), screen, &tree);
        // Bounds should be bit-identical (from cache)
        for id in tree.iter_nodes().map(|(id, _)| id) {
            assert_eq!(
                b1.get(&id),
                b2.get(&id),
                "Bounds should be identical from cache"
            );
        }
    }

    #[test]
    fn test_zstack_children_retain_absolute_positioning() {
        let mut tree = ViewTree::new();
        build_tree(
            &mut tree,
            zstack([
                (Alignment::TopLeading, text("A").boxed()),
                (Alignment::BottomTrailing, text("B").boxed()),
            ])
            .boxed(),
        );
        let screen = Vec2::new(800.0, 600.0);

        let mut layout = TaffyNodeMap::new();
        layout.sync(&mut tree, &measure_text_descriptor);
        let bounds = layout.compute(tree.root().unwrap(), screen, &tree);

        let root = tree.root().unwrap();
        let children = tree.get(root).unwrap().children.clone();
        assert_eq!(children.len(), 2);

        // Both children should have valid bounds
        for (i, &child_id) in children.iter().enumerate() {
            let b = bounds.get(&child_id).copied().unwrap_or_default();
            assert!(b.width() > 0.0, "ZStack child {} should have width", i);
        }

        // Match full rebuild
        assert_incremental_matches_full_rebuild(&mut tree, &mut layout, screen);
    }

    #[test]
    fn test_padding_change_reflected() {
        let mut tree = ViewTree::new();
        build_tree(
            &mut tree,
            hstack([text("X").boxed()])
                .padding(Padding::all(0.0))
                .boxed(),
        );
        let screen = Vec2::new(800.0, 600.0);

        let mut layout = TaffyNodeMap::new();
        layout.sync(&mut tree, &measure_text_descriptor);
        let bounds1 = layout.compute(tree.root().unwrap(), screen, &tree);

        // Change padding from 0 to 20
        build_tree(
            &mut tree,
            hstack([text("X").boxed()])
                .padding(Padding::all(20.0))
                .boxed(),
        );
        layout.sync(&mut tree, &measure_text_descriptor);
        let bounds2 = layout.compute(tree.root().unwrap(), screen, &tree);

        let root = tree.root().unwrap();
        let children = tree.get(root).unwrap().children.clone();
        let child1 = bounds1.get(&children[0]).copied().unwrap_or_default();
        let child2 = bounds2.get(&children[0]).copied().unwrap_or_default();

        assert!(
            child2.min.x() >= child1.min.x() + 19.0,
            "Child should shift right with padding: was={}, now={}",
            child1.min.x(),
            child2.min.x(),
        );
    }

    #[test]
    fn test_alignment_change_reflected() {
        let mut tree = ViewTree::new();
        build_tree(
            &mut tree,
            hstack([text("A").boxed(), text("B").boxed()])
                .align(Alignment::Leading)
                .boxed(),
        );
        let screen = Vec2::new(800.0, 600.0);

        let mut layout = TaffyNodeMap::new();
        layout.sync(&mut tree, &measure_text_descriptor);
        let _bounds1 = layout.compute(tree.root().unwrap(), screen, &tree);

        // Change alignment to Center
        build_tree(
            &mut tree,
            hstack([text("A").boxed(), text("B").boxed()])
                .align(Alignment::Center)
                .boxed(),
        );
        layout.sync(&mut tree, &measure_text_descriptor);
        let _bounds2 = layout.compute(tree.root().unwrap(), screen, &tree);

        // Match full rebuild
        assert_incremental_matches_full_rebuild(&mut tree, &mut layout, screen);
    }

    #[test]
    fn test_flex_property_change() {
        let mut tree = ViewTree::new();
        build_tree(
            &mut tree,
            hstack([
                vstack([text("A").boxed()]).flex_width(100.0).boxed(),
                text("B").boxed(),
            ])
            .boxed(),
        );
        let screen = Vec2::new(800.0, 600.0);

        let mut layout = TaffyNodeMap::new();
        layout.sync(&mut tree, &measure_text_descriptor);
        let bounds1 = layout.compute(tree.root().unwrap(), screen, &tree);

        let root = tree.root().unwrap();
        let children = tree.get(root).unwrap().children.clone();
        let child_a1 = bounds1.get(&children[0]).copied().unwrap_or_default();
        assert!(
            approx_eq(child_a1.width(), 100.0),
            "First child width should be 100, got {}",
            child_a1.width(),
        );

        // Change width from 100 to 200
        build_tree(
            &mut tree,
            hstack([
                vstack([text("A").boxed()]).flex_width(200.0).boxed(),
                text("B").boxed(),
            ])
            .boxed(),
        );
        layout.sync(&mut tree, &measure_text_descriptor);
        let bounds2 = layout.compute(tree.root().unwrap(), screen, &tree);

        let children2 = tree.get(root).unwrap().children.clone();
        let child_a2 = bounds2.get(&children2[0]).copied().unwrap_or_default();
        assert!(
            approx_eq(child_a2.width(), 200.0),
            "First child width should be 200, got {}",
            child_a2.width(),
        );
    }

    #[test]
    fn test_grid_cell_columns_change() {
        let mut tree = ViewTree::new();
        build_tree(
            &mut tree,
            grid(
                2,
                Vec2::new(100.0, 50.0),
                vec![
                    text("A").boxed(),
                    text("B").boxed(),
                    text("C").boxed(),
                    text("D").boxed(),
                ],
            )
            .boxed(),
        );
        let screen = Vec2::new(800.0, 600.0);

        let mut layout = TaffyNodeMap::new();
        layout.sync(&mut tree, &measure_text_descriptor);
        let _bounds1 = layout.compute(tree.root().unwrap(), screen, &tree);

        // Change to 3 columns
        build_tree(
            &mut tree,
            grid(
                3,
                Vec2::new(80.0, 40.0),
                vec![
                    text("A").boxed(),
                    text("B").boxed(),
                    text("C").boxed(),
                    text("D").boxed(),
                ],
            )
            .boxed(),
        );
        layout.sync(&mut tree, &measure_text_descriptor);
        let bounds2 = layout.compute(tree.root().unwrap(), screen, &tree);

        let root = tree.root().unwrap();
        let root_bounds = bounds2.get(&root).copied().unwrap_or_default();
        assert!(root_bounds.width() > 0.0);
        assert!(root_bounds.height() > 0.0);

        assert_incremental_matches_full_rebuild(&mut tree, &mut layout, screen);
    }

    #[test]
    fn test_multiple_syncs_stable_bounds() {
        let mut tree = ViewTree::new();
        build_tree(
            &mut tree,
            vstack([text("A").boxed(), text("B").boxed()]).boxed(),
        );
        let screen = Vec2::new(800.0, 600.0);

        let mut layout = TaffyNodeMap::new();
        layout.sync(&mut tree, &measure_text_descriptor);
        let b1 = layout.compute(tree.root().unwrap(), screen, &tree);

        layout.sync(&mut tree, &measure_text_descriptor);
        let b2 = layout.compute(tree.root().unwrap(), screen, &tree);

        layout.sync(&mut tree, &measure_text_descriptor);
        let b3 = layout.compute(tree.root().unwrap(), screen, &tree);

        // All three should be bit-identical
        for id in tree.iter_nodes().map(|(id, _)| id) {
            assert_eq!(
                b1.get(&id),
                b2.get(&id),
                "Bounds drift between sync 1 and 2"
            );
            assert_eq!(
                b2.get(&id),
                b3.get(&id),
                "Bounds drift between sync 2 and 3"
            );
        }
    }

    #[test]
    fn test_mixed_dirty_clean_subtrees() {
        let mut tree = ViewTree::new();
        build_tree(
            &mut tree,
            hstack([
                vstack([text("A1").boxed(), text("A2").boxed()]).boxed(),
                vstack([text("B1").boxed(), text("B2").boxed()]).boxed(),
            ])
            .boxed(),
        );
        let screen = Vec2::new(800.0, 600.0);

        let mut layout = TaffyNodeMap::new();
        layout.sync(&mut tree, &measure_text_descriptor);
        let _bounds1 = layout.compute(tree.root().unwrap(), screen, &tree);

        let root = tree.root().unwrap();
        let hstack_children = tree.get(root).unwrap().children.clone();
        let _left_branch = hstack_children[0];
        let _right_branch = hstack_children[1];

        // Change only left branch
        build_tree(
            &mut tree,
            hstack([
                vstack([text("CHANGED").boxed(), text("A2").boxed()]).boxed(),
                vstack([text("B1").boxed(), text("B2").boxed()]).boxed(),
            ])
            .boxed(),
        );
        layout.sync(&mut tree, &measure_text_descriptor);
        let _bounds2 = layout.compute(tree.root().unwrap(), screen, &tree);

        // Full rebuild comparison
        assert_incremental_matches_full_rebuild(&mut tree, &mut layout, screen);
    }

    #[test]
    fn test_rapid_add_remove_no_leak() {
        let mut tree = ViewTree::new();
        let screen = Vec2::new(800.0, 600.0);

        let mut layout = TaffyNodeMap::new();

        for _ in 0..100 {
            // Add children
            build_tree(
                &mut tree,
                vstack([text("A").boxed(), text("B").boxed(), text("C").boxed()]).boxed(),
            );
            layout.sync(&mut tree, &measure_text_descriptor);
            let _ = layout.compute(tree.root().unwrap(), screen, &tree);

            let tree_count = tree.iter_nodes().count();
            assert_eq!(
                layout.node_count(),
                tree_count,
                "Taffy node count should match after adding children"
            );

            // Remove children
            build_tree(&mut tree, vstack([text("A").boxed()]).boxed());
            layout.sync(&mut tree, &measure_text_descriptor);
            let _ = layout.compute(tree.root().unwrap(), screen, &tree);

            let tree_count = tree.iter_nodes().count();
            assert_eq!(
                layout.node_count(),
                tree_count,
                "Taffy node count should match after removing children"
            );
        }

        // Final state should be bounded
        let final_count = layout.node_count();
        assert!(
            final_count <= 2,
            "Should have at most 2 nodes, got {}",
            final_count
        );
    }

    #[test]
    fn test_bounds_map_all_view_ids() {
        let mut tree = ViewTree::new();
        build_tree(
            &mut tree,
            vstack([text("A").boxed(), text("B").boxed(), text("C").boxed()]).boxed(),
        );
        let screen = Vec2::new(800.0, 600.0);

        let mut layout = TaffyNodeMap::new();
        layout.sync(&mut tree, &measure_text_descriptor);
        let bounds = layout.compute(tree.root().unwrap(), screen, &tree);

        let tree_count = tree.iter_nodes().count();
        assert_eq!(
            bounds.len(),
            tree_count,
            "Bounds map should have entry for every ViewId"
        );

        for (id, _) in tree.iter_nodes() {
            assert!(
                bounds.contains_key(&id),
                "Missing bounds for ViewId {:?}",
                id
            );
        }
    }

    #[test]
    fn test_text_measurement_change_reflected() {
        let mut tree = ViewTree::new();
        build_tree(
            &mut tree,
            hstack([text("Hi").boxed(), text("Other").boxed()]).boxed(),
        );
        let screen = Vec2::new(800.0, 600.0);

        let mut layout = TaffyNodeMap::new();
        layout.sync(&mut tree, &measure_text_descriptor);
        let bounds1 = layout.compute(tree.root().unwrap(), screen, &tree);

        // Change text content to longer string
        build_tree(
            &mut tree,
            hstack([text("Hello World").boxed(), text("Other").boxed()]).boxed(),
        );
        layout.sync(&mut tree, &measure_text_descriptor);
        let bounds2 = layout.compute(tree.root().unwrap(), screen, &tree);

        let root = tree.root().unwrap();
        let children = tree.get(root).unwrap().children.clone();
        let first1 = bounds1.get(&children[0]).copied().unwrap_or_default();
        let first2 = bounds2.get(&children[0]).copied().unwrap_or_default();

        assert!(
            first2.width() > first1.width(),
            "First child should be wider with longer text: was={}, now={}",
            first1.width(),
            first2.width(),
        );
    }

    #[test]
    fn test_zero_available_space() {
        let mut tree = ViewTree::new();
        build_tree(
            &mut tree,
            vstack([text("A").boxed(), text("B").boxed()]).boxed(),
        );

        let mut layout = TaffyNodeMap::new();
        layout.sync(&mut tree, &measure_text_descriptor);
        let bounds = layout.compute(tree.root().unwrap(), Vec2::new(0.0, 0.0), &tree);

        // Should not panic
        let root = tree.root().unwrap();
        assert!(bounds.contains_key(&root), "Root should have bounds entry");

        // Fresh rebuild should match
        let mut fresh = TaffyNodeMap::new();
        fresh.sync(&mut tree, &measure_text_descriptor);
        let fresh_bounds = fresh.compute(tree.root().unwrap(), Vec2::new(0.0, 0.0), &tree);
        for id in tree.iter_nodes().map(|(id, _)| id) {
            let b1 = bounds.get(&id).copied().unwrap_or_default();
            let b2 = fresh_bounds.get(&id).copied().unwrap_or_default();
            assert!(
                (b1.width() - b2.width()).abs() < 0.5,
                "Zero-space bounds should match full rebuild"
            );
        }
    }

    #[test]
    fn test_incremental_cross_text_measurement_callback() {
        // Verify that changing text content triggers re-measurement
        // through the incremental pipeline
        let mut tree = ViewTree::new();
        build_tree(&mut tree, text("Short").boxed());
        let screen = Vec2::new(800.0, 600.0);

        let mut layout = TaffyNodeMap::new();
        layout.sync(&mut tree, &measure_text_descriptor);
        let bounds1 = layout.compute(tree.root().unwrap(), screen, &tree);

        build_tree(&mut tree, text("Much longer text here").boxed());
        layout.sync(&mut tree, &measure_text_descriptor);
        let bounds2 = layout.compute(tree.root().unwrap(), screen, &tree);

        let root = tree.root().unwrap();
        let b1 = bounds1.get(&root).copied().unwrap_or_default();
        let b2 = bounds2.get(&root).copied().unwrap_or_default();

        assert!(
            b2.width() > b1.width(),
            "Text measurement should reflect longer content: was={}, now={}",
            b1.width(),
            b2.width(),
        );
    }

    #[test]
    fn test_layout_caching_reduces_work() {
        let mut tree = ViewTree::new();
        build_tree(
            &mut tree,
            vstack([
                text("A").boxed(),
                text("B").boxed(),
                text("C").boxed(),
                text("D").boxed(),
            ])
            .boxed(),
        );
        let screen = Vec2::new(800.0, 600.0);

        let mut layout = TaffyNodeMap::new();

        // First sync+compute — full rebuild
        layout.sync(&mut tree, &measure_text_descriptor);
        assert!(layout.is_dirty(), "First sync should be dirty");
        let _ = layout.compute(tree.root().unwrap(), screen, &tree);

        // Second sync+compute — nothing changed
        layout.sync(&mut tree, &measure_text_descriptor);
        assert!(!layout.is_dirty(), "Second sync should not be dirty");
    }

    #[test]
    fn test_taffy_id_populated_in_view_nodes() {
        let mut tree = ViewTree::new();
        build_tree(
            &mut tree,
            vstack([text("A").boxed(), text("B").boxed()]).boxed(),
        );

        let mut layout = TaffyNodeMap::new();
        layout.sync(&mut tree, &measure_text_descriptor);

        // All nodes should have taffy_id set
        for (id, node) in tree.iter_nodes() {
            assert!(
                node.taffy_id.is_some(),
                "ViewNode {:?} should have taffy_id populated",
                id,
            );
        }
    }

    /// Verify that Overlay wrapping a Panel with explicit dimensions
    /// gets proper Taffy bounds (not zero-sized). This is the exact pattern
    /// used for docked panels in the editor.
    #[test]
    fn test_overlay_panel_has_nonzero_bounds() {
        let panel_w = 200.0;
        let panel_h = 400.0;
        let offset = Vec2::new(100.0, 50.0);

        let desc = zstack([(
            Alignment::TopLeading,
            overlay(
                Anchor::TopLeft,
                offset,
                panel("Test", text("content").boxed())
                    .flex_width(panel_w)
                    .flex_height(panel_h)
                    .boxed(),
            )
            .boxed(),
        )]);

        let mut tree = ViewTree::new();
        build_tree(&mut tree, desc.boxed());

        let mut taffy = TaffyNodeMap::new();
        taffy.sync(&mut tree, &measure_text_descriptor);
        let bounds = taffy.compute(tree.root().unwrap(), Vec2::new(800.0, 600.0), &tree);

        let root = tree.root().unwrap();
        let root_node = tree.get(root).unwrap();

        // ZStack should fill the screen
        let zstack_bounds = bounds.get(&root).copied().unwrap_or_default();
        assert!(zstack_bounds.width() > 0.0, "ZStack should have width");
        assert!(zstack_bounds.height() > 0.0, "ZStack should have height");

        // Overlay (ZStack's child)
        let overlay_id = root_node.children[0];
        let overlay_bounds = bounds.get(&overlay_id).copied().unwrap_or_default();
        eprintln!("Overlay bounds: {:?}", overlay_bounds);

        // Panel (Overlay's child)
        let overlay_node = tree.get(overlay_id).unwrap();
        let panel_id = overlay_node.children[0];
        let panel_bounds = bounds.get(&panel_id).copied().unwrap_or_default();
        eprintln!("Panel bounds: {:?}", panel_bounds);

        // Both Overlay and Panel should have non-zero dimensions
        assert!(
            overlay_bounds.width() > 1.0,
            "Overlay should have non-zero width, got {}",
            overlay_bounds.width(),
        );
        assert!(
            overlay_bounds.height() > 1.0,
            "Overlay should have non-zero height, got {}",
            overlay_bounds.height(),
        );
        assert!(
            approx_eq(panel_bounds.width(), panel_w),
            "Panel width should be {}, got {}",
            panel_w,
            panel_bounds.width(),
        );
        assert!(
            approx_eq(panel_bounds.height(), panel_h),
            "Panel height should be {}, got {}",
            panel_h,
            panel_bounds.height(),
        );
    }
}
