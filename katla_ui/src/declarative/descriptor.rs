use std::ops::RangeInclusive;

use katla_math::{Color, Rect2D, Vec2};

use crate::style::FontSize;
use crate::types::TextureId;

use super::state::StateId;
use super::transition::Transition;

/// Declarative description of a UI view tree.
///
/// Each variant describes a leaf widget (Text, Button, Slider, etc.) or a
/// layout container (HStack, VStack, ZStack, Panel, ScrollView, Overlay).
///
/// Implement [`Build`](super::Build) to produce a `ViewDescriptor` tree each frame.
/// The [`ViewTree`](super::ViewTree) handles diffing, layout, input, and rendering
/// automatically.
#[derive(Clone)]
pub enum ViewDescriptor {
    Empty,

    /// Invisible wrapper that carries transition config for its child.
    /// During sync_tree, insert/remove animations are applied to the child node.
    TransitionContainer {
        child: Box<ViewDescriptor>,
        transition: Transition,
    },

    Text {
        content: String,
        color: Option<Color>,
        font_size: Option<FontSize>,
    },

    Button {
        label: String,
        fill_color: Option<Color>,
        hover_color: Option<Color>,
        border_color: Option<Color>,
        on_click: Option<Callback>,
    },

    /// A slider with a label prefix and optional value display.
    ///
    /// Layout: `[label (label_width)] [track (fills remaining)] [value (if show_value)]`
    LabeledSlider {
        label: String,
        value_id: StateId,
        range: RangeInclusive<f32>,
        label_width: f32,
        show_value: bool,
        precision: usize,
    },

    /// A basic slider without label prefix or value display.
    Slider {
        label: String,
        value_id: StateId,
        range: RangeInclusive<f32>,
        show_value: bool,
        precision: usize,
    },

    /// A three-axis slider for Vec3/f32[3] values with colored axis labels.
    Vec3Slider {
        label: String,
        value_ids: [StateId; 3],
        range: RangeInclusive<f32>,
        axis_labels: [String; 3],
        axis_colors: [Color; 3],
        precision: usize,
    },

    Toggle {
        label: String,
        value_id: StateId,
    },

    TextField {
        placeholder: String,
        value_id: StateId,
        on_submit: Option<Callback>,
    },

    Progress {
        value: f32,
        range: RangeInclusive<f32>,
        fill_color: Option<Color>,
        label: Option<String>,
    },

    ColorPicker {
        label: String,
        value_id: StateId,
    },

    /// An icon-only clickable button.
    ImageButton {
        icon: char,
        enabled: bool,
        fill_color: Option<Color>,
        on_click: Option<Callback>,
    },

    /// A radio button for selecting one option from a group.
    ///
    /// `value_id` holds the current selection index. `index` is this button's value.
    RadioButton {
        value_id: StateId,
        index: usize,
        label: String,
    },

    Image {
        texture: TextureId,
        uv: Option<Rect2D>,
        tint: Color,
        width: Option<f32>,
        height: Option<f32>,
    },

    /// Read-only property display: `[label] [value]` on a single row.
    PropertyRow {
        label: String,
        value: String,
    },

    /// A horizontal or vertical divider line.
    Separator {
        direction: SeparatorDirection,
        color: Option<Color>,
    },

    /// Render an icon glyph with configurable size and color.
    Icon {
        icon: char,
        size: Option<FontSize>,
        color: Option<Color>,
    },

    /// Wrapper that highlights on hover and fires on_click.
    Selectable {
        child: Box<ViewDescriptor>,
        on_click: Option<Callback>,
        selected: bool,
    },

    /// Collapsible section with header row, optional remove button,
    /// and expand/collapse chevron.
    Section {
        title: String,
        child: Box<ViewDescriptor>,
        expanded_id: StateId,
        on_remove: Option<Callback>,
    },

    /// Tab strip with selectable tabs and content area below.
    /// `selected_id` holds the current tab index. `tabs` provides labels.
    /// `content` is the child shown below the tab strip.
    TabBar(Box<TabBarDescriptor>),

    /// Wrapping grid layout with fixed column count and uniform cell size.
    Grid(Box<GridDescriptor>),

    HStack(Box<StackDescriptor>),
    VStack(Box<StackDescriptor>),
    ZStack(Box<ZStackDescriptor>),

    ScrollView(Box<ScrollDescriptor>),
    Panel(Box<PanelDescriptor>),
    Overlay(Box<OverlayDescriptor>),

    StatusBar(Box<StatusBarDescriptor>),

    DraggablePanel(Box<DraggablePanelDescriptor>),

    MenuBar(Box<MenuBarDescriptor>),

    TreeView(Box<TreeViewDescriptor>),

    Modal(Box<ModalDescriptor>),

    ContextMenu(Box<ContextMenuDescriptor>),

    VuMeter(Box<VuMeterDescriptor>),
}

#[derive(Clone, Debug)]
pub struct ChildDescriptor {
    pub key: Option<u64>,
    pub descriptor: ViewDescriptor,
}

impl From<ViewDescriptor> for ChildDescriptor {
    fn from(descriptor: ViewDescriptor) -> Self {
        Self {
            key: None,
            descriptor,
        }
    }
}

#[derive(Clone, Debug)]
pub struct StackDescriptor {
    pub children: Vec<ChildDescriptor>,
    pub spacing: f32,
    pub padding: Padding,
    pub alignment: Alignment,
    pub flex: FlexProps,
}

#[derive(Clone, Debug)]
pub struct ZStackDescriptor {
    pub children: Vec<(Alignment, ChildDescriptor)>,
    pub padding: Padding,
    pub flex: FlexProps,
}

#[derive(Clone, Debug)]
pub struct ScrollDescriptor {
    pub content: Box<ViewDescriptor>,
    pub scroll_state_id: StateId,
    pub flex: FlexProps,
}

#[derive(Clone, Debug)]
pub struct PanelDescriptor {
    pub title: String,
    pub content: Box<ViewDescriptor>,
    pub header_height: f32,
    pub flex: FlexProps,
}

#[derive(Clone, Debug)]
pub struct OverlayDescriptor {
    pub anchor: Anchor,
    pub offset: Vec2,
    pub content: Box<ViewDescriptor>,
}

#[derive(Clone, Debug)]
pub struct StatusBarDescriptor {
    pub height: f32,
    pub content: Box<ViewDescriptor>,
}

#[derive(Clone, Debug)]
pub struct DraggablePanelDescriptor {
    pub title: String,
    pub width: f32,
    pub height: f32,
    pub content: Box<ViewDescriptor>,
    pub state_id: StateId,
    pub close_on_outside_click: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DraggablePanelState {
    pub position: Option<Vec2>,
    pub visibility: DraggablePanelVisibility,
    pub dragging: bool,
    pub drag_offset: Vec2,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DraggablePanelVisibility {
    #[default]
    Hidden,
    JustOpened,
    Visible,
}

impl DraggablePanelVisibility {
    pub fn is_visible(&self) -> bool {
        !matches!(self, Self::Hidden)
    }

    pub fn is_just_opened(&self) -> bool {
        matches!(self, Self::JustOpened)
    }
}

impl DraggablePanelState {
    pub fn is_visible(&self) -> bool {
        self.visibility.is_visible()
    }

    pub fn open(&mut self) {
        self.visibility = DraggablePanelVisibility::JustOpened;
    }

    pub fn close(&mut self) {
        self.visibility = DraggablePanelVisibility::Hidden;
    }

    pub fn mark_shown(&mut self) {
        if self.visibility == DraggablePanelVisibility::JustOpened {
            self.visibility = DraggablePanelVisibility::Visible;
        }
    }

    pub fn bounds(&self, width: f32, height: f32, screen: Vec2) -> Option<Rect2D> {
        let pos = self.position?;
        Some(Rect2D::from_origin_size(
            pos,
            Vec2::new(
                width.min(screen.x() - pos.x()),
                height.min(screen.y() - pos.y()),
            ),
        ))
    }
}

#[derive(Clone, Debug)]
pub struct MenuBarDescriptor {
    pub groups: Vec<MenuGroup>,
    pub right_content: Option<Box<ViewDescriptor>>,
    pub height: f32,
}

#[derive(Clone, Debug)]
pub struct MenuGroup {
    pub label: String,
    pub open_id: StateId,
    pub items: Vec<MenuEntry>,
}

#[derive(Clone, Debug)]
pub struct MenuEntry {
    pub label: String,
    pub on_click: Option<Callback>,
    pub disabled: bool,
}

#[derive(Clone, Debug)]
pub struct TreeViewDescriptor {
    pub items: Vec<TreeItem>,
    pub expanded_id: StateId,
    pub selected_id: StateId,
    pub scroll_id: StateId,
    pub row_height: f32,
    pub indent_per_level: f32,
    pub on_select: Option<Callback>,
    pub on_right_click: Option<Callback>,
}

#[derive(Clone, Debug)]
pub struct TreeItem {
    pub id: u64,
    pub label: String,
    pub depth: u32,
    pub has_children: bool,
}

#[derive(Clone, Debug)]
pub struct ModalDescriptor {
    pub width: f32,
    pub height: f32,
    pub open_id: StateId,
    pub content: Box<ViewDescriptor>,
    pub on_close: Option<Callback>,
}

#[derive(Clone, Debug)]
pub struct ContextMenuDescriptor {
    pub items: Vec<ContextMenuEntry>,
    pub open_id: StateId,
}

#[derive(Clone, Debug)]
pub struct ContextMenuEntry {
    pub label: String,
    pub on_click: Option<Callback>,
    pub disabled: bool,
}

#[derive(Clone, Debug)]
pub struct VuMeterDescriptor {
    pub peak_db: f32,
    pub rms_db: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SeparatorDirection {
    Horizontal,
    Vertical,
}

#[derive(Clone, Debug)]
pub struct TabBarDescriptor {
    pub tabs: Vec<TabItem>,
    pub selected_id: StateId,
    pub content: Box<ViewDescriptor>,
}

#[derive(Clone, Debug)]
pub struct TabItem {
    pub label: String,
}

#[derive(Clone, Debug)]
pub struct GridDescriptor {
    pub columns: usize,
    pub cell_size: Vec2,
    pub spacing: f32,
    pub children: Vec<ChildDescriptor>,
    pub flex: FlexProps,
}

#[derive(Clone, Copy, Debug)]
pub enum Anchor {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    TopCenter,
    BottomCenter,
    Center,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Padding {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl Padding {
    pub const fn all(v: f32) -> Self {
        Self {
            top: v,
            right: v,
            bottom: v,
            left: v,
        }
    }

    pub const fn horizontal(v: f32) -> Self {
        Self {
            top: 0.0,
            right: v,
            bottom: 0.0,
            left: v,
        }
    }

    pub const fn zero() -> Self {
        Self {
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
            left: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Alignment {
    Leading,
    Trailing,
    Center,
    Top,
    Bottom,
    TopLeading,
    TopTrailing,
    BottomLeading,
    BottomTrailing,
    BottomCenter,
}

#[derive(Clone, Debug)]
pub struct FlexProps {
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub min_width: Option<f32>,
    pub min_height: Option<f32>,
    pub max_width: Option<f32>,
    pub max_height: Option<f32>,
    pub flex_grow: f32,
    pub flex_shrink: f32,
    pub aspect_ratio: Option<f32>,
}

impl Default for FlexProps {
    fn default() -> Self {
        Self {
            width: None,
            height: None,
            min_width: None,
            min_height: None,
            max_width: None,
            max_height: None,
            flex_grow: 0.0,
            flex_shrink: 1.0,
            aspect_ratio: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Callback(pub u32);

impl std::fmt::Debug for ViewDescriptor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ViewDescriptor::Empty => write!(f, "Empty"),
            ViewDescriptor::Text {
                content,
                color,
                font_size,
            } => f
                .debug_struct("Text")
                .field("content", content)
                .field("color", color)
                .field("font_size", font_size)
                .finish(),
            ViewDescriptor::Button {
                label,
                fill_color,
                hover_color,
                border_color,
                on_click,
            } => f
                .debug_struct("Button")
                .field("label", label)
                .field("fill_color", fill_color)
                .field("hover_color", hover_color)
                .field("border_color", border_color)
                .field("on_click", on_click)
                .finish(),
            ViewDescriptor::LabeledSlider {
                label,
                value_id,
                range,
                label_width,
                show_value,
                precision,
            } => f
                .debug_struct("LabeledSlider")
                .field("label", label)
                .field("value_id", value_id)
                .field("range", &format_args!("{:?}", range))
                .field("label_width", label_width)
                .field("show_value", show_value)
                .field("precision", precision)
                .finish(),
            ViewDescriptor::Slider {
                label,
                value_id,
                range,
                show_value,
                precision,
            } => f
                .debug_struct("Slider")
                .field("label", label)
                .field("value_id", value_id)
                .field("range", &format_args!("{:?}", range))
                .field("show_value", show_value)
                .field("precision", precision)
                .finish(),
            ViewDescriptor::Vec3Slider {
                label,
                value_ids,
                range,
                axis_labels,
                axis_colors,
                precision,
            } => f
                .debug_struct("Vec3Slider")
                .field("label", label)
                .field("value_ids", &format_args!("{:?}", value_ids))
                .field("range", &format_args!("{:?}", range))
                .field("axis_labels", axis_labels)
                .field("axis_colors", axis_colors)
                .field("precision", precision)
                .finish(),
            ViewDescriptor::Toggle { label, value_id } => f
                .debug_struct("Toggle")
                .field("label", label)
                .field("value_id", value_id)
                .finish(),
            ViewDescriptor::TextField {
                placeholder,
                value_id,
                on_submit,
            } => f
                .debug_struct("TextField")
                .field("placeholder", placeholder)
                .field("value_id", value_id)
                .field("on_submit", on_submit)
                .finish(),
            ViewDescriptor::Progress {
                value,
                range,
                fill_color,
                label,
            } => f
                .debug_struct("Progress")
                .field("value", value)
                .field("range", &format_args!("{:?}", range))
                .field("fill_color", fill_color)
                .field("label", label)
                .finish(),
            ViewDescriptor::ColorPicker { label, value_id } => f
                .debug_struct("ColorPicker")
                .field("label", label)
                .field("value_id", value_id)
                .finish(),
            ViewDescriptor::ImageButton {
                icon,
                enabled,
                fill_color,
                on_click: _,
            } => f
                .debug_struct("ImageButton")
                .field("icon", icon)
                .field("enabled", enabled)
                .field("fill_color", fill_color)
                .finish(),
            ViewDescriptor::RadioButton {
                value_id,
                index,
                label,
            } => f
                .debug_struct("RadioButton")
                .field("value_id", value_id)
                .field("index", index)
                .field("label", label)
                .finish(),
            ViewDescriptor::Image {
                texture,
                uv,
                tint,
                width: _,
                height: _,
            } => f
                .debug_struct("Image")
                .field("texture", texture)
                .field("uv", uv)
                .field("tint", tint)
                .finish(),
            ViewDescriptor::PropertyRow { label, value } => f
                .debug_struct("PropertyRow")
                .field("label", label)
                .field("value", value)
                .finish(),
            ViewDescriptor::Separator { direction, color } => f
                .debug_struct("Separator")
                .field("direction", direction)
                .field("color", color)
                .finish(),
            ViewDescriptor::Icon { icon, size, color } => f
                .debug_struct("Icon")
                .field("icon", icon)
                .field("size", size)
                .field("color", color)
                .finish(),
            ViewDescriptor::Selectable {
                on_click, selected, ..
            } => f
                .debug_struct("Selectable")
                .field("on_click", on_click)
                .field("selected", selected)
                .finish(),
            ViewDescriptor::Section {
                title,
                expanded_id,
                on_remove,
                ..
            } => f
                .debug_struct("Section")
                .field("title", title)
                .field("expanded_id", expanded_id)
                .field("on_remove", on_remove)
                .finish(),
            ViewDescriptor::TabBar(desc) => {
                f.debug_tuple("TabBar").field(&desc.tabs.len()).finish()
            }
            ViewDescriptor::Grid(desc) => f
                .debug_tuple("Grid")
                .field(&desc.columns)
                .field(&desc.children.len())
                .finish(),
            ViewDescriptor::HStack(s) => f.debug_tuple("HStack").field(s).finish(),
            ViewDescriptor::VStack(s) => f.debug_tuple("VStack").field(s).finish(),
            ViewDescriptor::ZStack(s) => f.debug_tuple("ZStack").field(s).finish(),
            ViewDescriptor::ScrollView(s) => f.debug_tuple("ScrollView").field(s).finish(),
            ViewDescriptor::Panel(s) => f.debug_tuple("Panel").field(s).finish(),
            ViewDescriptor::Overlay(s) => f.debug_tuple("Overlay").field(s).finish(),
            ViewDescriptor::StatusBar(s) => f.debug_tuple("StatusBar").field(s).finish(),
            ViewDescriptor::DraggablePanel(s) => f.debug_tuple("DraggablePanel").field(s).finish(),
            ViewDescriptor::MenuBar(s) => f.debug_tuple("MenuBar").field(s).finish(),
            ViewDescriptor::TreeView(s) => f.debug_tuple("TreeView").field(s).finish(),
            ViewDescriptor::Modal(s) => f.debug_tuple("Modal").field(s).finish(),
            ViewDescriptor::ContextMenu(s) => f.debug_tuple("ContextMenu").field(s).finish(),
            ViewDescriptor::VuMeter(s) => f.debug_tuple("VuMeter").field(s).finish(),
            ViewDescriptor::TransitionContainer { child, .. } => {
                f.debug_tuple("TransitionContainer").field(child).finish()
            }
        }
    }
}
