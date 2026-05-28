use std::ops::RangeInclusive;

use katla_math::{Color, Rect2D, Vec2};

use crate::context::UiContext;
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

    Slider {
        label: String,
        value_id: StateId,
        range: RangeInclusive<f32>,
        show_value: bool,
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
    },

    ColorPicker {
        label: String,
        value_id: StateId,
    },

    Image {
        texture: TextureId,
        uv: Option<Rect2D>,
        tint: Color,
    },

    HStack(Box<StackDescriptor>),
    VStack(Box<StackDescriptor>),
    ZStack(Box<ZStackDescriptor>),

    ScrollView(Box<ScrollDescriptor>),
    Panel(Box<PanelDescriptor>),
    Overlay(Box<OverlayDescriptor>),

    Custom(CustomDrawFn),
}

#[derive(Clone, Debug)]
pub struct StackDescriptor {
    pub children: Vec<ViewDescriptor>,
    pub spacing: f32,
    pub padding: Padding,
    pub alignment: Alignment,
}

#[derive(Clone, Debug)]
pub struct ZStackDescriptor {
    pub children: Vec<(Alignment, ViewDescriptor)>,
    pub padding: Padding,
}

#[derive(Clone, Debug)]
pub struct ScrollDescriptor {
    pub content: Box<ViewDescriptor>,
    pub scroll_state_id: StateId,
}

#[derive(Clone, Debug)]
pub struct PanelDescriptor {
    pub title: String,
    pub content: Box<ViewDescriptor>,
    pub header_height: f32,
}

#[derive(Clone, Debug)]
pub struct OverlayDescriptor {
    pub anchor: Anchor,
    pub offset: Vec2,
    pub content: Box<ViewDescriptor>,
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

#[derive(Clone, Copy, Debug)]
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

#[derive(Clone, Copy, Debug)]
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

#[derive(Clone)]
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

pub type CustomDrawFn = fn(&mut UiContext, Rect2D);

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
            } => f
                .debug_struct("Progress")
                .field("value", value)
                .field("range", &format_args!("{:?}", range))
                .field("fill_color", fill_color)
                .finish(),
            ViewDescriptor::ColorPicker { label, value_id } => f
                .debug_struct("ColorPicker")
                .field("label", label)
                .field("value_id", value_id)
                .finish(),
            ViewDescriptor::Image { texture, uv, tint } => f
                .debug_struct("Image")
                .field("texture", texture)
                .field("uv", uv)
                .field("tint", tint)
                .finish(),
            ViewDescriptor::HStack(s) => f.debug_tuple("HStack").field(s).finish(),
            ViewDescriptor::VStack(s) => f.debug_tuple("VStack").field(s).finish(),
            ViewDescriptor::ZStack(s) => f.debug_tuple("ZStack").field(s).finish(),
            ViewDescriptor::ScrollView(s) => f.debug_tuple("ScrollView").field(s).finish(),
            ViewDescriptor::Panel(s) => f.debug_tuple("Panel").field(s).finish(),
            ViewDescriptor::Overlay(s) => f.debug_tuple("Overlay").field(s).finish(),
            ViewDescriptor::TransitionContainer { child, .. } => {
                f.debug_tuple("TransitionContainer").field(child).finish()
            }
            ViewDescriptor::Custom(_) => f.debug_tuple("Custom").field(&"..").finish(),
        }
    }
}
