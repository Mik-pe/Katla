use std::collections::HashMap;

use katla_math::{Rect2D, Vec2};
use taffy::{
    Dimension, FlexDirection, LengthPercentage, NodeId as TaffyNodeId, Size, Style, TaffyTree,
};

use crate::style::FontSize;

use super::descriptor::{Alignment, FlexProps, Padding, ViewDescriptor};
use super::state::ViewId;
use super::tree::ViewTree;

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

    pub fn sync(&mut self, tree: &ViewTree) {
        let Some(root_id) = tree.root() else {
            return;
        };

        self.taffy.clear();
        self.mapping.clear();
        self.reverse.clear();

        self.sync_recursive(tree, root_id);
    }

    fn sync_recursive(&mut self, tree: &ViewTree, view_id: ViewId) {
        let Some(node) = tree.get(view_id) else {
            return;
        };

        let style = descriptor_to_style(&node.descriptor);
        let children = &node.children;

        if children.is_empty() {
            let taffy_id = self.taffy.new_leaf(style).unwrap();
            self.mapping.insert(view_id, taffy_id);
            self.reverse.insert(taffy_id, view_id);
        } else {
            let child_taffy_ids: Vec<TaffyNodeId> = children
                .iter()
                .filter_map(|&child_id| {
                    self.sync_recursive(tree, child_id);
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

fn descriptor_to_style(descriptor: &ViewDescriptor) -> Style {
    match descriptor {
        ViewDescriptor::Empty => Style::default(),

        ViewDescriptor::Text {
            content, font_size, ..
        } => {
            let size = measure_text_descriptor(content, *font_size);
            Style {
                size: Size {
                    width: Dimension::Length(size.x()),
                    height: Dimension::Length(size.y()),
                },
                ..Style::default()
            }
        }

        ViewDescriptor::Button { label, .. } => {
            let text_size = measure_text_descriptor(label, None);
            let h_padding = 16.0;
            let v_padding = 8.0;
            Style {
                size: Size {
                    width: Dimension::Length(text_size.x() + h_padding),
                    height: Dimension::Length(text_size.y() + v_padding),
                },
                ..Style::default()
            }
        }

        ViewDescriptor::HStack(stack) => {
            let mut style = Style {
                flex_direction: FlexDirection::Row,
                gap: Size {
                    width: LengthPercentage::Length(stack.spacing),
                    height: LengthPercentage::Length(0.0),
                },
                padding: padding_to_taffy(&stack.padding),
                ..Style::default()
            };
            apply_alignment_to_style(&mut style, stack.alignment);
            style
        }

        ViewDescriptor::VStack(stack) => {
            let mut style = Style {
                flex_direction: FlexDirection::Column,
                gap: Size {
                    width: LengthPercentage::Length(0.0),
                    height: LengthPercentage::Length(stack.spacing),
                },
                padding: padding_to_taffy(&stack.padding),
                ..Style::default()
            };
            apply_alignment_to_style(&mut style, stack.alignment);
            style
        }

        ViewDescriptor::ZStack(zstack) => Style {
            padding: padding_to_taffy(&zstack.padding),
            ..Style::default()
        },

        ViewDescriptor::ScrollView(_) => Style {
            overflow: taffy::Point {
                x: taffy::Overflow::Scroll,
                y: taffy::Overflow::Scroll,
            },
            ..Style::default()
        },

        ViewDescriptor::Panel(_) => Style {
            flex_direction: FlexDirection::Column,
            ..Style::default()
        },

        ViewDescriptor::Overlay(_) => Style {
            position: taffy::Position::Absolute,
            ..Style::default()
        },

        ViewDescriptor::LabeledSlider { label, .. } => {
            let text_size = measure_text_descriptor(label, None);
            Style {
                size: Size {
                    width: Dimension::Length((text_size.x() + 120.0).max(200.0)),
                    height: Dimension::Length(text_size.y() + 12.0),
                },
                ..Style::default()
            }
        }

        ViewDescriptor::Slider { label, .. } | ViewDescriptor::ColorPicker { label, .. } => {
            let text_size = measure_text_descriptor(label, None);
            Style {
                size: Size {
                    width: Dimension::Length((text_size.x() + 40.0).max(100.0)),
                    height: Dimension::Length(text_size.y() + 12.0),
                },
                ..Style::default()
            }
        }

        ViewDescriptor::Toggle { label, .. } => {
            let text_size = measure_text_descriptor(label, None);
            Style {
                size: Size {
                    width: Dimension::Length(text_size.x() + 28.0),
                    height: Dimension::Length(text_size.y() + 8.0),
                },
                ..Style::default()
            }
        }

        ViewDescriptor::TextField { placeholder, .. } => {
            let text_size = measure_text_descriptor(placeholder, None);
            Style {
                size: Size {
                    width: Dimension::Length(text_size.x() + 16.0),
                    height: Dimension::Length(text_size.y() + 12.0),
                },
                ..Style::default()
            }
        }

        ViewDescriptor::Progress { .. } => Style {
            size: Size {
                width: Dimension::Length(100.0),
                height: Dimension::Length(8.0),
            },
            ..Style::default()
        },

        ViewDescriptor::Vec3Slider { label, .. } => {
            let text_size = measure_text_descriptor(label, None);
            Style {
                size: Size {
                    width: Dimension::Length((text_size.x() + 120.0).max(200.0)),
                    height: Dimension::Length(text_size.y() * 3.0 + 20.0),
                },
                ..Style::default()
            }
        }

        ViewDescriptor::ImageButton { .. } => Style {
            size: Size {
                width: Dimension::Length(28.0),
                height: Dimension::Length(28.0),
            },
            ..Style::default()
        },

        ViewDescriptor::RadioButton { label, .. } => {
            let text_size = measure_text_descriptor(label, None);
            Style {
                size: Size {
                    width: Dimension::Length(text_size.x() + 24.0),
                    height: Dimension::Length(text_size.y() + 8.0),
                },
                ..Style::default()
            }
        }

        ViewDescriptor::PropertyRow { label, value } => {
            let label_size = measure_text_descriptor(label, None);
            let value_size = measure_text_descriptor(value, None);
            Style {
                size: Size {
                    width: Dimension::Length(label_size.x() + value_size.x() + 16.0),
                    height: Dimension::Length(label_size.y().max(value_size.y()) + 4.0),
                },
                ..Style::default()
            }
        }

        ViewDescriptor::Image { .. } => Style {
            size: Size {
                width: Dimension::Length(64.0),
                height: Dimension::Length(64.0),
            },
            ..Style::default()
        },

        ViewDescriptor::Custom(_) => Style::default(),

        ViewDescriptor::TransitionContainer { .. } => Style::default(),
    }
}

fn padding_to_taffy(padding: &Padding) -> taffy::Rect<LengthPercentage> {
    taffy::Rect {
        top: LengthPercentage::Length(padding.top),
        right: LengthPercentage::Length(padding.right),
        bottom: LengthPercentage::Length(padding.bottom),
        left: LengthPercentage::Length(padding.left),
    }
}

fn apply_alignment_to_style(style: &mut Style, alignment: Alignment) {
    match alignment {
        Alignment::Leading => {
            style.align_items = Some(taffy::AlignItems::Start);
            style.justify_content = Some(taffy::JustifyContent::Start);
        }
        Alignment::Trailing => {
            style.align_items = Some(taffy::AlignItems::End);
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
            style.align_items = Some(taffy::AlignItems::Start);
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
    use crate::declarative::build::{Build, BuildContext};
    use crate::declarative::descriptor::{StackDescriptor, ViewDescriptor};
    use crate::declarative::tree::ViewTree;
    use katla_math::Vec2;

    fn approx_eq(a: f32, b: f32) -> bool {
        (a - b).abs() < 1.0
    }

    struct StaticDescriptor(ViewDescriptor);

    impl Build for StaticDescriptor {
        fn build(&self, _ctx: &mut BuildContext) -> ViewDescriptor {
            self.0.clone()
        }
    }

    fn build_tree(tree: &mut ViewTree, descriptor: ViewDescriptor) {
        tree.build_from(&StaticDescriptor(descriptor));
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
        let descriptor = ViewDescriptor::HStack(Box::new(StackDescriptor {
            children: vec![
                ViewDescriptor::Text {
                    content: "A".into(),
                    color: None,
                    font_size: None,
                },
                ViewDescriptor::Text {
                    content: "B".into(),
                    color: None,
                    font_size: None,
                },
            ],
            spacing: 10.0,
            padding: Padding::all(8.0),
            alignment: Alignment::Leading,
        }));

        build_tree(&mut tree, descriptor);

        let mut layout = TaffyNodeMap::new();
        layout.sync(&tree);

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
        let descriptor = ViewDescriptor::VStack(Box::new(StackDescriptor {
            children: vec![
                ViewDescriptor::Text {
                    content: "Line 1".into(),
                    color: None,
                    font_size: None,
                },
                ViewDescriptor::Text {
                    content: "Line 2".into(),
                    color: None,
                    font_size: None,
                },
            ],
            spacing: 4.0,
            padding: Padding::zero(),
            alignment: Alignment::Leading,
        }));

        build_tree(&mut tree, descriptor);

        let mut layout = TaffyNodeMap::new();
        layout.sync(&tree);

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
        let descriptor = ViewDescriptor::VStack(Box::new(StackDescriptor {
            children: vec![
                ViewDescriptor::Text {
                    content: "Top".into(),
                    color: None,
                    font_size: None,
                },
                ViewDescriptor::Text {
                    content: "Bottom".into(),
                    color: None,
                    font_size: None,
                },
            ],
            spacing: 0.0,
            padding: Padding::zero(),
            alignment: Alignment::Leading,
        }));

        build_tree(&mut tree, descriptor);

        let mut layout = TaffyNodeMap::new();
        layout.sync(&tree);

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
        let descriptor = ViewDescriptor::HStack(Box::new(StackDescriptor {
            children: vec![ViewDescriptor::Text {
                content: "X".into(),
                color: None,
                font_size: None,
            }],
            spacing: 0.0,
            padding: Padding::all(20.0),
            alignment: Alignment::Leading,
        }));

        build_tree(&mut tree, descriptor);

        let mut layout = TaffyNodeMap::new();
        layout.sync(&tree);

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
}
