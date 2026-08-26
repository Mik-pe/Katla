use katla_math::{Rect2D, Vec2};

use super::state::StateId;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Alignment {
    Leading,
    Trailing,
    Center,
    /// Cross-axis centring only. In a row, children share a vertical centre
    /// line but remain packed to the leading edge; mirrored for columns.
    Middle,
    Top,
    Bottom,
    TopLeading,
    TopTrailing,
    BottomLeading,
    BottomTrailing,
    BottomCenter,
}

#[derive(Clone, Copy, Debug, PartialEq)]
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SeparatorDirection {
    Horizontal,
    Vertical,
}

#[derive(Clone, Debug)]
pub struct Callback(pub u32);

#[derive(Clone, Debug)]
pub struct TabItem {
    pub label: String,
}

#[derive(Clone, Debug)]
pub struct TreeItem {
    pub id: u64,
    pub label: String,
    pub depth: u32,
    pub has_children: bool,
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
pub struct ContextMenuEntry {
    pub label: String,
    pub on_click: Option<Callback>,
    pub disabled: bool,
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
