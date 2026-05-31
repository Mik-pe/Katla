use std::collections::HashMap;

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
        }
    }

    pub fn sync(&mut self, tree: &ViewTree, measure: MeasureFn<'_>) {
        let Some(root_id) = tree.root() else {
            return;
        };

        self.taffy.clear();
        self.mapping.clear();
        self.reverse.clear();

        self.sync_recursive(tree, root_id, measure);
    }

    fn sync_recursive(&mut self, tree: &ViewTree, view_id: ViewId, measure: MeasureFn<'_>) {
        let Some(node) = tree.get(view_id) else {
            return;
        };

        let mut style = node.widget.layout_style(measure);

        // ZStack children need absolute positioning so they stack rather than
        // participate in flex flow.  Do NOT set all-four insets to 0% — that
        // stretches every child to the parent size, which makes
        // resolve_zstack_alignment compute wrong positions (e.g. BottomLeading
        // collapses to y=0).  Each child keeps its natural / explicit size and
        // is repositioned during draw.
        if node.zstack_alignment.is_some() {
            style.position = taffy::Position::Absolute;
        }

        let children = &node.children;

        if children.is_empty() {
            let taffy_id = self.taffy.new_leaf(style).unwrap();
            self.mapping.insert(view_id, taffy_id);
            self.reverse.insert(taffy_id, view_id);
        } else {
            let child_taffy_ids: Vec<TaffyNodeId> = children
                .iter()
                .filter_map(|&child_id| {
                    self.sync_recursive(tree, child_id, measure);
                    self.mapping.get(&child_id).copied()
                })
                .collect();

            let taffy_id = self
                .taffy
                .new_with_children(style, &child_taffy_ids)
                .unwrap();
            self.mapping.insert(view_id, taffy_id);
            self.reverse.insert(taffy_id, view_id);
        }
    }

    pub fn compute(
        &mut self,
        root: ViewId,
        available_size: Vec2,
        tree: &ViewTree,
    ) -> HashMap<ViewId, Rect2D> {
        let Some(&taffy_root) = self.mapping.get(&root) else {
            return HashMap::new();
        };

        let available = Size {
            width: taffy::AvailableSpace::Definite(available_size.x()),
            height: taffy::AvailableSpace::Definite(available_size.y()),
        };

        self.taffy.compute_layout(taffy_root, available).unwrap();

        let mut bounds = HashMap::new();
        self.resolve_bounds_recursive(taffy_root, Vec2::new(0.0, 0.0), tree, &mut bounds);

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
    use crate::declarative::widget::WidgetBox;
    use katla_math::Vec2;

    fn approx_eq(a: f32, b: f32) -> bool {
        (a - b).abs() < 1.0
    }

    fn build_tree(tree: &mut ViewTree, widget: Box<dyn crate::declarative::widget::Widget>) {
        tree.set_root(widget);
    }

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
        layout.sync(&tree, &measure_text_descriptor);

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
        layout.sync(&tree, &measure_text_descriptor);

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
        layout.sync(&tree, &measure_text_descriptor);

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
        layout.sync(&tree, &measure_text_descriptor);

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
        layout.sync(&tree, &measure_text_descriptor);

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
        layout.sync(&tree, &measure_text_descriptor);

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
        layout.sync(&tree, &measure_text_descriptor);

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
        layout.sync(&tree, &measure_text_descriptor);

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
        layout.sync(&tree, &measure_text_descriptor);

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
        layout.sync(&tree, &measure_text_descriptor);

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
        layout.sync(&tree, &measure_text_descriptor);

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
}
