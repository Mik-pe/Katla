//! Layout serialization for LLM verification.
//!
//! Walks the widget tree and produces a human-readable text representation
//! showing widget types, text content, bounds, positions, and layout properties.
//! Designed for dump-and-exit workflows where an LLM needs to verify UI layout
//! without screenshots or vision.

use std::any::TypeId;

use katla_math::Rect2D;

use super::state::ViewId;
use super::tree::ViewTree;
use super::widget::Widget;

use super::widgets::button::Button;
use super::widgets::code_editor::CodeEditor;
use super::widgets::color_picker::ColorPicker;
use super::widgets::context_menu::ContextMenu;
use super::widgets::dock_space::DockSpace;
use super::widgets::draggable_panel::DraggablePanel;
use super::widgets::empty::Empty;
use super::widgets::grid::Grid;
use super::widgets::hstack::HStack;
use super::widgets::icon::Icon;
use super::widgets::image::Image;
use super::widgets::image_button::ImageButton;
use super::widgets::labeled_slider::LabeledSlider;
use super::widgets::menubar::MenuBar;
use super::widgets::modal::Modal;
use super::widgets::overlay::Overlay;
use super::widgets::panel::Panel;
use super::widgets::progress::Progress;
use super::widgets::property_row::PropertyRow;
use super::widgets::radio::RadioButton;
use super::widgets::scroll::ScrollView;
use super::widgets::section::Section;
use super::widgets::selectable::Selectable;
use super::widgets::separator::Separator;
use super::widgets::slider::Slider;
use super::widgets::statusbar::StatusBar;
use super::widgets::tab_bar::TabBar;
use super::widgets::text::Text;
use super::widgets::textfield::TextField;
use super::widgets::toggle::Toggle;
use super::widgets::transition::TransitionContainer;
use super::widgets::tree_view::TreeView;
use super::widgets::vec3_slider::Vec3Slider;
use super::widgets::vstack::VStack;
use super::widgets::vu_meter::VuMeter;
use super::widgets::zstack::ZStack;

/// Maximum length for text content before truncation.
const MAX_TEXT_LEN: usize = 50;

/// Describes a widget node for serialization purposes.
struct WidgetInfo {
    type_name: String,
    label: Option<String>,
    details: Vec<String>,
}

fn truncate_text(s: &str) -> String {
    if s.len() <= MAX_TEXT_LEN {
        s.to_string()
    } else {
        format!("{}…", &s[..MAX_TEXT_LEN])
    }
}

fn identify_widget(widget: &dyn Widget) -> WidgetInfo {
    let any = widget.as_any();

    // Try each known widget type. Order doesn't matter since TypeId is exact.
    if let Some(w) = any.downcast_ref::<Text>() {
        return WidgetInfo {
            type_name: "Text".into(),
            label: Some(truncate_text(&w.content)),
            details: w
                .font_size
                .map(|fs| format!("font={:?}", fs))
                .into_iter()
                .collect(),
        };
    }

    if let Some(w) = any.downcast_ref::<Button>() {
        return WidgetInfo {
            type_name: "Button".into(),
            label: Some(truncate_text(&w.label)),
            details: Vec::new(),
        };
    }

    if let Some(w) = any.downcast_ref::<Panel>() {
        return WidgetInfo {
            type_name: "Panel".into(),
            label: Some(truncate_text(&w.title)),
            details: Vec::new(),
        };
    }

    if let Some(w) = any.downcast_ref::<VStack>() {
        let mut details = Vec::new();
        if w.spacing != 0.0 {
            details.push(format!("spacing={}", w.spacing));
        }
        if w.alignment != super::descriptor::Alignment::TopLeading {
            details.push(format!("alignment={:?}", w.alignment));
        }
        let p = &w.padding;
        if p.top != 0.0 || p.right != 0.0 || p.bottom != 0.0 || p.left != 0.0 {
            details.push(format!(
                "padding=({},{},{},{})",
                p.top, p.right, p.bottom, p.left
            ));
        }
        return WidgetInfo {
            type_name: "VStack".into(),
            label: None,
            details,
        };
    }

    if let Some(w) = any.downcast_ref::<HStack>() {
        let mut details = Vec::new();
        if w.spacing != 0.0 {
            details.push(format!("spacing={}", w.spacing));
        }
        if w.alignment != super::descriptor::Alignment::TopLeading {
            details.push(format!("alignment={:?}", w.alignment));
        }
        let p = &w.padding;
        if p.top != 0.0 || p.right != 0.0 || p.bottom != 0.0 || p.left != 0.0 {
            details.push(format!(
                "padding=({},{},{},{})",
                p.top, p.right, p.bottom, p.left
            ));
        }
        return WidgetInfo {
            type_name: "HStack".into(),
            label: None,
            details,
        };
    }

    if let Some(w) = any.downcast_ref::<ZStack>() {
        let mut details = Vec::new();
        let p = &w.padding;
        if p.top != 0.0 || p.right != 0.0 || p.bottom != 0.0 || p.left != 0.0 {
            details.push(format!(
                "padding=({},{},{},{})",
                p.top, p.right, p.bottom, p.left
            ));
        }
        return WidgetInfo {
            type_name: "ZStack".into(),
            label: None,
            details,
        };
    }

    if let Some(w) = any.downcast_ref::<Slider>() {
        return WidgetInfo {
            type_name: "Slider".into(),
            label: if w.label.is_empty() {
                None
            } else {
                Some(truncate_text(&w.label))
            },
            details: vec![format!("range={}..={}", w.range.start(), w.range.end())],
        };
    }

    if let Some(w) = any.downcast_ref::<LabeledSlider>() {
        return WidgetInfo {
            type_name: "LabeledSlider".into(),
            label: Some(truncate_text(&w.label)),
            details: vec![format!("range={}..={}", w.range.start(), w.range.end())],
        };
    }

    if let Some(w) = any.downcast_ref::<Vec3Slider>() {
        return WidgetInfo {
            type_name: "Vec3Slider".into(),
            label: if w.label.is_empty() {
                None
            } else {
                Some(truncate_text(&w.label))
            },
            details: vec![format!("range={}..={}", w.range.start(), w.range.end())],
        };
    }

    if let Some(w) = any.downcast_ref::<TextField>() {
        return WidgetInfo {
            type_name: "TextField".into(),
            label: if w.placeholder.is_empty() {
                None
            } else {
                Some(truncate_text(&w.placeholder))
            },
            details: Vec::new(),
        };
    }

    if let Some(w) = any.downcast_ref::<Toggle>() {
        return WidgetInfo {
            type_name: "Toggle".into(),
            label: Some(truncate_text(&w.label)),
            details: Vec::new(),
        };
    }

    if let Some(w) = any.downcast_ref::<RadioButton>() {
        return WidgetInfo {
            type_name: "RadioButton".into(),
            label: Some(truncate_text(&w.label)),
            details: Vec::new(),
        };
    }

    if let Some(w) = any.downcast_ref::<Progress>() {
        let mut details = vec![format!("value={:.2}", w.value)];
        if let Some(ref label) = w.label {
            details.push(format!("label={}", truncate_text(label)));
        }
        return WidgetInfo {
            type_name: "Progress".into(),
            label: None,
            details,
        };
    }

    if let Some(_w) = any.downcast_ref::<Empty>() {
        return WidgetInfo {
            type_name: "Empty".into(),
            label: None,
            details: Vec::new(),
        };
    }

    if let Some(_w) = any.downcast_ref::<ScrollView>() {
        return WidgetInfo {
            type_name: "ScrollView".into(),
            label: None,
            details: Vec::new(),
        };
    }

    if let Some(w) = any.downcast_ref::<Section>() {
        return WidgetInfo {
            type_name: "Section".into(),
            label: Some(truncate_text(&w.title)),
            details: Vec::new(),
        };
    }

    if let Some(w) = any.downcast_ref::<MenuBar>() {
        return WidgetInfo {
            type_name: "MenuBar".into(),
            label: None,
            details: vec![format!("groups={}", w.groups.len())],
        };
    }

    if let Some(w) = any.downcast_ref::<StatusBar>() {
        return WidgetInfo {
            type_name: "StatusBar".into(),
            label: None,
            details: vec![format!("height={}", w.height)],
        };
    }

    if let Some(w) = any.downcast_ref::<TabBar>() {
        let tab_names: Vec<&str> = w.tabs.iter().map(|t| t.label.as_str()).collect();
        return WidgetInfo {
            type_name: "TabBar".into(),
            label: None,
            details: vec![format!("tabs=[{}]", tab_names.join(", "))],
        };
    }

    if let Some(w) = any.downcast_ref::<TreeView>() {
        return WidgetInfo {
            type_name: "TreeView".into(),
            label: None,
            details: vec![format!("items={}", w.items.len())],
        };
    }

    if let Some(w) = any.downcast_ref::<Overlay>() {
        return WidgetInfo {
            type_name: "Overlay".into(),
            label: None,
            details: vec![format!("anchor={:?}", w.anchor)],
        };
    }

    if let Some(w) = any.downcast_ref::<Modal>() {
        return WidgetInfo {
            type_name: "Modal".into(),
            label: None,
            details: vec![format!("size={}×{}", w.width, w.height)],
        };
    }

    if let Some(w) = any.downcast_ref::<DraggablePanel>() {
        return WidgetInfo {
            type_name: "DraggablePanel".into(),
            label: Some(truncate_text(&w.title)),
            details: vec![format!("size={}×{}", w.width, w.height)],
        };
    }

    if let Some(w) = any.downcast_ref::<Grid>() {
        return WidgetInfo {
            type_name: "Grid".into(),
            label: None,
            details: vec![
                format!("columns={}", w.columns),
                format!("cell={}×{}", w.cell_size.x(), w.cell_size.y()),
                format!("spacing={}", w.spacing),
            ],
        };
    }

    if let Some(w) = any.downcast_ref::<Separator>() {
        return WidgetInfo {
            type_name: "Separator".into(),
            label: None,
            details: vec![format!("dir={:?}", w.direction)],
        };
    }

    if let Some(w) = any.downcast_ref::<Icon>() {
        return WidgetInfo {
            type_name: "Icon".into(),
            label: Some(w.icon.to_string()),
            details: Vec::new(),
        };
    }

    if let Some(w) = any.downcast_ref::<Image>() {
        return WidgetInfo {
            type_name: "Image".into(),
            label: None,
            details: vec![format!("texture={:?}", w.texture)],
        };
    }

    if let Some(w) = any.downcast_ref::<ImageButton>() {
        return WidgetInfo {
            type_name: "ImageButton".into(),
            label: Some(w.icon.to_string()),
            details: vec![format!("enabled={}", w.enabled)],
        };
    }

    if let Some(w) = any.downcast_ref::<PropertyRow>() {
        return WidgetInfo {
            type_name: "PropertyRow".into(),
            label: Some(truncate_text(&w.label)),
            details: vec![format!("value={}", truncate_text(&w.value))],
        };
    }

    if let Some(w) = any.downcast_ref::<ColorPicker>() {
        return WidgetInfo {
            type_name: "ColorPicker".into(),
            label: Some(truncate_text(&w.label)),
            details: Vec::new(),
        };
    }

    if let Some(w) = any.downcast_ref::<VuMeter>() {
        return WidgetInfo {
            type_name: "VuMeter".into(),
            label: None,
            details: vec![format!("peak={:.1}dB rms={:.1}dB", w.peak_db, w.rms_db)],
        };
    }

    if let Some(w) = any.downcast_ref::<Selectable>() {
        return WidgetInfo {
            type_name: "Selectable".into(),
            label: None,
            details: vec![format!("selected={}", w.selected)],
        };
    }

    if let Some(w) = any.downcast_ref::<ContextMenu>() {
        return WidgetInfo {
            type_name: "ContextMenu".into(),
            label: None,
            details: vec![format!("items={}", w.items.len())],
        };
    }

    if any.downcast_ref::<TransitionContainer>().is_some() {
        return WidgetInfo {
            type_name: "TransitionContainer".into(),
            label: None,
            details: Vec::new(),
        };
    }

    if any.downcast_ref::<CodeEditor>().is_some() {
        return WidgetInfo {
            type_name: "CodeEditor".into(),
            label: None,
            details: Vec::new(),
        };
    }

    // Memoize<T, W> is generic — check by TypeId of the concrete type.
    // Since we can't know T and W, check if the type name contains "Memoize".
    let type_id = widget.widget_type();
    let type_name = type_name_of(type_id);
    if type_name.contains("Memoize") {
        return WidgetInfo {
            type_name: "Memoize".into(),
            label: None,
            details: Vec::new(),
        };
    }

    // Check for DockSpace<u64> and DockSpace<u32>
    if any.downcast_ref::<DockSpace<u64>>().is_some() {
        return WidgetInfo {
            type_name: "DockSpace".into(),
            label: None,
            details: Vec::new(),
        };
    }
    if any.downcast_ref::<DockSpace<u32>>().is_some() {
        return WidgetInfo {
            type_name: "DockSpace".into(),
            label: None,
            details: Vec::new(),
        };
    }

    // Fallback: unknown widget type
    WidgetInfo {
        type_name,
        label: None,
        details: Vec::new(),
    }
}

fn type_name_of(type_id: TypeId) -> String {
    // Use std::any::type_name via a trick — we can't get the name from TypeId alone,
    // but we stored the type info during widget construction. Use a fallback.
    // Since Widget::widget_type() returns TypeId, and we need a name,
    // we'll use a mapping approach for known types.
    let known: Vec<(TypeId, &str)> = vec![
        (TypeId::of::<Text>(), "Text"),
        (TypeId::of::<Button>(), "Button"),
        (TypeId::of::<Panel>(), "Panel"),
        (TypeId::of::<VStack>(), "VStack"),
        (TypeId::of::<HStack>(), "HStack"),
        (TypeId::of::<ZStack>(), "ZStack"),
        (TypeId::of::<Slider>(), "Slider"),
        (TypeId::of::<Empty>(), "Empty"),
        (TypeId::of::<Separator>(), "Separator"),
        (TypeId::of::<Icon>(), "Icon"),
    ];

    for (known_id, name) in &known {
        if *known_id == type_id {
            return name.to_string();
        }
    }

    "Unknown".to_string()
}

/// Format a single node line for the tree dump.
fn format_node(info: &WidgetInfo, bounds: Rect2D, parent_bounds: Option<Rect2D>) -> String {
    let mut parts = Vec::new();

    // Widget type
    parts.push(info.type_name.clone());

    // Label/text content
    if let Some(ref label) = info.label {
        parts.push(format!("\"{}\"", label));
    }

    // Bounds
    let w = bounds.width();
    let h = bounds.height();
    parts.push(format!("({}×{}) ({}×{})", w as i32, h as i32, w, h));

    // Position relative to parent
    if let Some(pb) = parent_bounds {
        let rel_x = bounds.min.x() - pb.min.x();
        let rel_y = bounds.min.y() - pb.min.y();
        parts.push(format!("pos=({:.0},{:.0})", rel_x, rel_y));
    }

    // Additional details
    for detail in &info.details {
        parts.push(detail.clone());
    }

    parts.join(" ")
}

/// Serialize the view tree to a human-readable text representation.
///
/// Produces output like:
/// ```text
/// ZStack (1200×800) (1200.0×800.0)
/// ├── Panel "Properties" (300×400) (300.0×400.0) pos=(0,0)
/// │   ├── Text "Transform"
/// │   └── TextField "X: 0.0"
/// └── Panel "Hierarchy" (300×200) (300.0×200.0) pos=(0,400)
/// ```
pub fn serialize_layout(tree: &ViewTree, screen_size: katla_math::Vec2) -> String {
    let mut output = String::new();

    // Header with screen size
    output.push_str(&format!(
        "Window ({}×{})\n",
        screen_size.x() as i32,
        screen_size.y() as i32
    ));

    let Some(root_id) = tree.root() else {
        output.push_str("  (empty tree)\n");
        return output;
    };

    serialize_node(tree, root_id, None, "", "", &mut output);
    output
}

fn serialize_node(
    tree: &ViewTree,
    node_id: ViewId,
    parent_bounds: Option<Rect2D>,
    line_prefix: &str,
    child_prefix: &str,
    output: &mut String,
) {
    let Some(node) = tree.get(node_id) else {
        return;
    };

    let bounds = tree
        .resolved_bounds()
        .get(&node_id)
        .copied()
        .unwrap_or(node.bounds);

    let info = identify_widget(&*node.widget);
    let line = format_node(&info, bounds, parent_bounds);

    output.push_str(&format!("{}{}\n", line_prefix, line));

    let children: Vec<ViewId> = node.children.clone();
    let child_count = children.len();

    for (i, child_id) in children.iter().enumerate() {
        let is_last = i == child_count - 1;
        let connector = if is_last { "└── " } else { "├── " };
        let continuation = if is_last { "    " } else { "│   " };

        let next_line = format!("{}{}", child_prefix, connector);
        let next_child = format!("{}{}", child_prefix, continuation);

        serialize_node(
            tree,
            *child_id,
            Some(bounds),
            &next_line,
            &next_child,
            output,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use katla_math::Vec2;

    #[test]
    fn test_truncate_text_short() {
        assert_eq!(truncate_text("hello"), "hello");
    }

    #[test]
    fn test_truncate_text_long() {
        let long = "a".repeat(100);
        let result = truncate_text(&long);
        // The result is 50 chars of content + 1 char ellipsis (which is multi-byte in UTF-8)
        assert!(result.chars().count() <= MAX_TEXT_LEN + 1);
        assert!(result.ends_with('…'));
    }

    #[test]
    fn test_truncate_text_exact() {
        let exact = "a".repeat(MAX_TEXT_LEN);
        assert_eq!(truncate_text(&exact), exact);
    }

    #[test]
    fn test_identify_widget_text() {
        let widget = super::super::widgets::text::Text {
            content: "Hello".into(),
            color: None,
            font_size: None,
        };
        let info = identify_widget(&widget);
        assert_eq!(info.type_name, "Text");
        assert_eq!(info.label.as_deref(), Some("Hello"));
    }

    #[test]
    fn test_identify_widget_button() {
        let widget = super::super::widgets::button::Button {
            label: "Click".into(),
            fill_color: None,
            border_color: None,
            on_click: None,
        };
        let info = identify_widget(&widget);
        assert_eq!(info.type_name, "Button");
        assert_eq!(info.label.as_deref(), Some("Click"));
    }

    #[test]
    fn test_identify_widget_empty() {
        let widget = super::super::widgets::empty::Empty;
        let info = identify_widget(&widget);
        assert_eq!(info.type_name, "Empty");
        assert!(info.label.is_none());
    }

    #[test]
    fn test_serialize_empty_tree() {
        let tree = ViewTree::new();
        let result = serialize_layout(&tree, Vec2::new(800.0, 600.0));
        assert!(result.contains("Window (800×600)"));
        assert!(result.contains("(empty tree)"));
    }
}
