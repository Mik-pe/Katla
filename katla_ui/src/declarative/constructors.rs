use std::ops::RangeInclusive;

use katla_math::Vec2;

use crate::style::FontSize;
use crate::types::TextureId;

use super::descriptor::{
    Alignment, Anchor, Callback, ChildDescriptor, ContextMenuDescriptor, ContextMenuEntry,
    DraggablePanelDescriptor, FlexProps, MenuBarDescriptor, MenuEntry, MenuGroup, ModalDescriptor,
    OverlayDescriptor, Padding, PanelDescriptor, ScrollDescriptor, SeparatorDirection,
    StackDescriptor, StatusBarDescriptor, TreeItem, TreeViewDescriptor, ViewDescriptor,
    ZStackDescriptor,
};
use super::state::StateId;
use super::widget::{DescriptorWidget, Widget};

// ---------------------------------------------------------------------------
// KeyedChild — replaces ChildDescriptor in the public API
// ---------------------------------------------------------------------------

/// A child widget with an optional stable key for diffing.
pub struct KeyedChild {
    pub key: Option<u64>,
    pub widget: Box<dyn Widget>,
}

impl From<Box<dyn Widget>> for KeyedChild {
    fn from(widget: Box<dyn Widget>) -> Self {
        Self { key: None, widget }
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn wrap(descriptor: ViewDescriptor) -> Box<dyn Widget> {
    Box::new(DescriptorWidget::new(descriptor))
}

fn extract_descriptor(widget: &Box<dyn Widget>) -> ViewDescriptor {
    widget
        .as_any()
        .downcast_ref::<DescriptorWidget>()
        .expect("expected DescriptorWidget")
        .descriptor()
        .clone()
}

fn extract_descriptor_mut(widget: &mut Box<dyn Widget>) -> &mut ViewDescriptor {
    widget
        .as_any_mut()
        .downcast_mut::<DescriptorWidget>()
        .expect("expected DescriptorWidget")
        .descriptor_mut()
}

// ---------------------------------------------------------------------------
// Leaf constructors
// ---------------------------------------------------------------------------

pub fn empty() -> Box<dyn Widget> {
    wrap(ViewDescriptor::Empty)
}

pub fn text(content: impl Into<String>) -> Box<dyn Widget> {
    wrap(ViewDescriptor::Text {
        content: content.into(),
        color: None,
        font_size: None,
    })
}

pub fn button(label: impl Into<String>) -> Box<dyn Widget> {
    wrap(ViewDescriptor::Button {
        label: label.into(),
        fill_color: None,
        hover_color: None,
        border_color: None,
        on_click: None,
    })
}

pub fn image_button(icon: char) -> Box<dyn Widget> {
    wrap(ViewDescriptor::ImageButton {
        icon,
        enabled: true,
        fill_color: None,
        on_click: None,
    })
}

pub fn slider(
    label: impl Into<String>,
    value_id: StateId,
    range: RangeInclusive<f32>,
) -> Box<dyn Widget> {
    wrap(ViewDescriptor::Slider {
        label: label.into(),
        value_id,
        range,
        show_value: false,
        precision: 2,
    })
}

pub fn labeled_slider(
    label: impl Into<String>,
    value_id: StateId,
    range: RangeInclusive<f32>,
) -> Box<dyn Widget> {
    wrap(ViewDescriptor::LabeledSlider {
        label: label.into(),
        value_id,
        range,
        label_width: 0.0,
        show_value: false,
        precision: 2,
    })
}

pub fn textfield(placeholder: impl Into<String>, value_id: StateId) -> Box<dyn Widget> {
    wrap(ViewDescriptor::TextField {
        placeholder: placeholder.into(),
        value_id,
        on_submit: None,
    })
}

pub fn progress(value: f32, range: RangeInclusive<f32>) -> Box<dyn Widget> {
    wrap(ViewDescriptor::Progress {
        value,
        range,
        fill_color: None,
        label: None,
    })
}

pub fn vu_meter(peak_db: f32, rms_db: f32) -> Box<dyn Widget> {
    wrap(ViewDescriptor::VuMeter(Box::new(
        super::descriptor::VuMeterDescriptor { peak_db, rms_db },
    )))
}

pub fn image(texture: TextureId, tint: katla_math::Color) -> Box<dyn Widget> {
    wrap(ViewDescriptor::Image {
        texture,
        uv: None,
        tint,
        width: None,
        height: None,
    })
}

pub fn toggle(label: impl Into<String>, value_id: StateId) -> Box<dyn Widget> {
    wrap(ViewDescriptor::Toggle {
        label: label.into(),
        value_id,
    })
}

pub fn radio(value_id: StateId, index: usize, label: impl Into<String>) -> Box<dyn Widget> {
    wrap(ViewDescriptor::RadioButton {
        value_id,
        index,
        label: label.into(),
    })
}

pub fn property_row(label: impl Into<String>, value: impl Into<String>) -> Box<dyn Widget> {
    wrap(ViewDescriptor::PropertyRow {
        label: label.into(),
        value: value.into(),
    })
}

pub fn color_picker(label: impl Into<String>, value_id: StateId) -> Box<dyn Widget> {
    wrap(ViewDescriptor::ColorPicker {
        label: label.into(),
        value_id,
    })
}

pub fn vec3_slider(
    label: impl Into<String>,
    value_ids: [StateId; 3],
    range: RangeInclusive<f32>,
) -> Box<dyn Widget> {
    wrap(ViewDescriptor::Vec3Slider {
        label: label.into(),
        value_ids,
        range,
        axis_labels: ["X".to_string(), "Y".to_string(), "Z".to_string()],
        axis_colors: [
            katla_math::Color::RED,
            katla_math::Color::GREEN,
            katla_math::Color::BLUE,
        ],
        precision: 2,
    })
}

pub fn separator(direction: SeparatorDirection) -> Box<dyn Widget> {
    wrap(ViewDescriptor::Separator {
        direction,
        color: None,
    })
}

pub fn separator_horizontal() -> Box<dyn Widget> {
    separator(SeparatorDirection::Horizontal)
}

pub fn separator_vertical() -> Box<dyn Widget> {
    separator(SeparatorDirection::Vertical)
}

pub fn icon(icon: char) -> Box<dyn Widget> {
    wrap(ViewDescriptor::Icon {
        icon,
        size: None,
        color: None,
    })
}

pub fn selectable(child: Box<dyn Widget>) -> Box<dyn Widget> {
    wrap(ViewDescriptor::Selectable {
        child: Box::new(extract_descriptor(&child)),
        on_click: None,
        selected: false,
    })
}

pub fn section(
    title: impl Into<String>,
    child: Box<dyn Widget>,
    expanded_id: StateId,
) -> Box<dyn Widget> {
    wrap(ViewDescriptor::Section {
        title: title.into(),
        child: Box::new(extract_descriptor(&child)),
        expanded_id,
        on_remove: None,
    })
}

pub fn tab_bar(
    tabs: Vec<super::descriptor::TabItem>,
    selected_id: StateId,
    content: Box<dyn Widget>,
) -> Box<dyn Widget> {
    wrap(ViewDescriptor::TabBar(Box::new(
        super::descriptor::TabBarDescriptor {
            tabs,
            selected_id,
            content: Box::new(extract_descriptor(&content)),
        },
    )))
}

pub fn tab_item(label: impl Into<String>) -> super::descriptor::TabItem {
    super::descriptor::TabItem {
        label: label.into(),
    }
}

pub fn grid(
    columns: usize,
    cell_size: katla_math::Vec2,
    children: impl IntoIterator<Item = Box<dyn Widget>>,
) -> Box<dyn Widget> {
    let cw = cell_size.x();
    let ch = cell_size.y();
    let sized_children: Vec<ChildDescriptor> = children
        .into_iter()
        .map(|child| {
            let sized = ViewDescriptor::VStack(Box::new(StackDescriptor {
                children: vec![ChildDescriptor::from(extract_descriptor(&child))],
                spacing: 0.0,
                padding: Padding::zero(),
                alignment: Alignment::Leading,
                flex: FlexProps {
                    width: Some(cw),
                    height: Some(ch),
                    flex_grow: 0.0,
                    flex_shrink: 0.0,
                    ..FlexProps::default()
                },
            }));
            ChildDescriptor::from(sized)
        })
        .collect();
    wrap(ViewDescriptor::Grid(Box::new(
        super::descriptor::GridDescriptor {
            columns,
            cell_size,
            spacing: 0.0,
            children: sized_children,
            flex: FlexProps::default(),
        },
    )))
}

// ---------------------------------------------------------------------------
// Container constructors
// ---------------------------------------------------------------------------

pub fn hstack(children: impl IntoIterator<Item = Box<dyn Widget>>) -> Box<dyn Widget> {
    wrap(ViewDescriptor::HStack(Box::new(StackDescriptor {
        children: children
            .into_iter()
            .map(|c| ChildDescriptor::from(extract_descriptor(&c)))
            .collect(),
        spacing: 0.0,
        padding: Padding::zero(),
        alignment: Alignment::Leading,
        flex: FlexProps::default(),
    })))
}

pub fn vstack(children: impl IntoIterator<Item = Box<dyn Widget>>) -> Box<dyn Widget> {
    wrap(ViewDescriptor::VStack(Box::new(StackDescriptor {
        children: children
            .into_iter()
            .map(|c| ChildDescriptor::from(extract_descriptor(&c)))
            .collect(),
        spacing: 0.0,
        padding: Padding::zero(),
        alignment: Alignment::Leading,
        flex: FlexProps::default(),
    })))
}

pub fn zstack(children: impl IntoIterator<Item = (Alignment, Box<dyn Widget>)>) -> Box<dyn Widget> {
    wrap(ViewDescriptor::ZStack(Box::new(ZStackDescriptor {
        children: children
            .into_iter()
            .map(|(a, w)| (a, ChildDescriptor::from(extract_descriptor(&w))))
            .collect(),
        padding: Padding::zero(),
        flex: FlexProps::default(),
    })))
}

pub fn panel(title: impl Into<String>, content: Box<dyn Widget>) -> Box<dyn Widget> {
    wrap(ViewDescriptor::Panel(Box::new(PanelDescriptor {
        title: title.into(),
        content: Box::new(extract_descriptor(&content)),
        header_height: 24.0,
        flex: FlexProps::default(),
    })))
}

pub fn scroll(content: Box<dyn Widget>, scroll_state_id: StateId) -> Box<dyn Widget> {
    wrap(ViewDescriptor::ScrollView(Box::new(ScrollDescriptor {
        content: Box::new(extract_descriptor(&content)),
        scroll_state_id,
        flex: FlexProps::default(),
    })))
}

pub fn overlay(anchor: Anchor, offset: Vec2, content: Box<dyn Widget>) -> Box<dyn Widget> {
    wrap(ViewDescriptor::Overlay(Box::new(OverlayDescriptor {
        anchor,
        offset,
        content: Box::new(extract_descriptor(&content)),
    })))
}

pub fn statusbar(height: f32, content: Box<dyn Widget>) -> Box<dyn Widget> {
    wrap(ViewDescriptor::StatusBar(Box::new(StatusBarDescriptor {
        height,
        content: Box::new(extract_descriptor(&content)),
    })))
}

pub fn draggable_panel(
    title: impl Into<String>,
    width: f32,
    height: f32,
    content: Box<dyn Widget>,
    state_id: StateId,
) -> Box<dyn Widget> {
    wrap(ViewDescriptor::DraggablePanel(Box::new(
        DraggablePanelDescriptor {
            title: title.into(),
            width,
            height,
            content: Box::new(extract_descriptor(&content)),
            state_id,
            close_on_outside_click: false,
        },
    )))
}

pub fn menubar(groups: Vec<MenuGroup>) -> Box<dyn Widget> {
    wrap(ViewDescriptor::MenuBar(Box::new(MenuBarDescriptor {
        groups,
        right_content: None,
        height: 28.0,
    })))
}

pub fn tree_view(
    items: Vec<TreeItem>,
    expanded_id: StateId,
    selected_id: StateId,
    scroll_id: StateId,
) -> Box<dyn Widget> {
    wrap(ViewDescriptor::TreeView(Box::new(TreeViewDescriptor {
        items,
        expanded_id,
        selected_id,
        scroll_id,
        row_height: 20.0,
        indent_per_level: 16.0,
        on_select: None,
        on_right_click: None,
    })))
}

pub fn modal(
    width: f32,
    height: f32,
    open_id: StateId,
    content: Box<dyn Widget>,
) -> Box<dyn Widget> {
    wrap(ViewDescriptor::Modal(Box::new(ModalDescriptor {
        width,
        height,
        open_id,
        content: Box::new(extract_descriptor(&content)),
        on_close: None,
    })))
}

pub fn context_menu(items: Vec<ContextMenuEntry>, open_id: StateId) -> Box<dyn Widget> {
    wrap(ViewDescriptor::ContextMenu(Box::new(
        ContextMenuDescriptor { items, open_id },
    )))
}

// ---------------------------------------------------------------------------
// Keyed child helper
// ---------------------------------------------------------------------------

pub fn keyed(key: u64, widget: Box<dyn Widget>) -> KeyedChild {
    KeyedChild {
        key: Some(key),
        widget,
    }
}

pub fn hstack_keyed(children: Vec<KeyedChild>) -> Box<dyn Widget> {
    wrap(ViewDescriptor::HStack(Box::new(StackDescriptor {
        children: children
            .into_iter()
            .map(|kc| ChildDescriptor {
                key: kc.key,
                descriptor: extract_descriptor(&kc.widget),
            })
            .collect(),
        spacing: 0.0,
        padding: Padding::zero(),
        alignment: Alignment::Leading,
        flex: FlexProps::default(),
    })))
}

pub fn vstack_keyed(children: Vec<KeyedChild>) -> Box<dyn Widget> {
    wrap(ViewDescriptor::VStack(Box::new(StackDescriptor {
        children: children
            .into_iter()
            .map(|kc| ChildDescriptor {
                key: kc.key,
                descriptor: extract_descriptor(&kc.widget),
            })
            .collect(),
        spacing: 0.0,
        padding: Padding::zero(),
        alignment: Alignment::Leading,
        flex: FlexProps::default(),
    })))
}

pub fn zstack_keyed(children: Vec<(Alignment, KeyedChild)>) -> Box<dyn Widget> {
    wrap(ViewDescriptor::ZStack(Box::new(ZStackDescriptor {
        children: children
            .into_iter()
            .map(|(a, kc)| {
                (
                    a,
                    ChildDescriptor {
                        key: kc.key,
                        descriptor: extract_descriptor(&kc.widget),
                    },
                )
            })
            .collect(),
        padding: Padding::zero(),
        flex: FlexProps::default(),
    })))
}

pub fn grid_keyed(
    columns: usize,
    cell_size: katla_math::Vec2,
    children: Vec<KeyedChild>,
) -> Box<dyn Widget> {
    wrap(ViewDescriptor::Grid(Box::new(
        super::descriptor::GridDescriptor {
            columns,
            cell_size,
            spacing: 0.0,
            children: children
                .into_iter()
                .map(|kc| ChildDescriptor {
                    key: kc.key,
                    descriptor: extract_descriptor(&kc.widget),
                })
                .collect(),
            flex: FlexProps::default(),
        },
    )))
}

// ---------------------------------------------------------------------------
// Box<dyn Widget> modifier methods
// ---------------------------------------------------------------------------

/// Extension trait providing builder-pattern modifier methods on `Box<dyn Widget>`.
///
/// Import [`WidgetExt`] to chain modifiers like `.color(...)`, `.fill(...)`, etc.
pub trait WidgetExt {
    // -- Leaf modifiers --
    fn color(self, color: impl Into<katla_math::Color>) -> Box<dyn Widget>;
    fn font_size(self, fs: FontSize) -> Box<dyn Widget>;
    fn fill(self, color: impl Into<katla_math::Color>) -> Box<dyn Widget>;
    fn hover(self, color: impl Into<katla_math::Color>) -> Box<dyn Widget>;
    fn border(self, color: impl Into<katla_math::Color>) -> Box<dyn Widget>;
    fn on_click(self, cb: Callback) -> Box<dyn Widget>;
    fn on_close(self, cb: Callback) -> Box<dyn Widget>;
    fn enabled(self, e: bool) -> Box<dyn Widget>;
    fn on_submit(self, cb: Callback) -> Box<dyn Widget>;
    fn show_value(self, show: bool) -> Box<dyn Widget>;
    fn precision(self, p: usize) -> Box<dyn Widget>;
    fn label_width(self, w: f32) -> Box<dyn Widget>;
    fn uv(self, rect: katla_math::Rect2D) -> Box<dyn Widget>;

    // -- Container modifiers --
    fn spacing(self, s: f32) -> Box<dyn Widget>;
    fn padding(self, p: Padding) -> Box<dyn Widget>;
    fn padding_all(self, v: f32) -> Box<dyn Widget>;
    fn align(self, a: Alignment) -> Box<dyn Widget>;
    fn header_height(self, h: f32) -> Box<dyn Widget>;
    fn close_on_outside(self, close: bool) -> Box<dyn Widget>;
    fn right_content(self, content: Box<dyn Widget>) -> Box<dyn Widget>;
    fn menubar_height(self, h: f32) -> Box<dyn Widget>;
    fn row_height(self, h: f32) -> Box<dyn Widget>;
    fn indent(self, i: f32) -> Box<dyn Widget>;
    fn on_select(self, cb: Callback) -> Box<dyn Widget>;
    fn on_right_click(self, cb: Callback) -> Box<dyn Widget>;

    // -- Separator / Icon / Selectable / Section modifiers --
    fn separator_color(self, color: impl Into<katla_math::Color>) -> Box<dyn Widget>;
    fn icon_size(self, size: FontSize) -> Box<dyn Widget>;
    fn selected(self, sel: bool) -> Box<dyn Widget>;
    fn on_remove(self, cb: Callback) -> Box<dyn Widget>;

    // -- Progress / Grid / Image modifiers --
    fn image_size(self, width: f32, height: f32) -> Box<dyn Widget>;
    fn progress_label(self, label: impl Into<String>) -> Box<dyn Widget>;
    fn grid_spacing(self, spacing: f32) -> Box<dyn Widget>;
    fn flex_width(self, w: f32) -> Box<dyn Widget>;
    fn flex_height(self, h: f32) -> Box<dyn Widget>;
    fn flex_grow(self, grow: f32) -> Box<dyn Widget>;
}

impl WidgetExt for Box<dyn Widget> {
    // -- Leaf modifiers --

    fn color(mut self, color: impl Into<katla_math::Color>) -> Box<dyn Widget> {
        let c = color.into();
        let d = extract_descriptor_mut(&mut self);
        match d {
            ViewDescriptor::Text { color: co, .. } | ViewDescriptor::Icon { color: co, .. } => {
                *co = Some(c)
            }
            _ => {
                debug_assert!(false, "color() modifier applied to non-Text/Icon variant");
            }
        }
        self
    }

    fn font_size(mut self, fs: FontSize) -> Box<dyn Widget> {
        let d = extract_descriptor_mut(&mut self);
        if let ViewDescriptor::Text { font_size: f, .. } = d {
            *f = Some(fs);
        } else {
            debug_assert!(false, "font_size() modifier applied to non-Text variant");
        }
        self
    }

    fn fill(mut self, color: impl Into<katla_math::Color>) -> Box<dyn Widget> {
        let c = color.into();
        let d = extract_descriptor_mut(&mut self);
        match d {
            ViewDescriptor::Button { fill_color: f, .. }
            | ViewDescriptor::ImageButton { fill_color: f, .. }
            | ViewDescriptor::Progress { fill_color: f, .. } => *f = Some(c),
            _ => {
                debug_assert!(false, "fill() modifier applied to unsupported variant");
            }
        }
        self
    }

    fn hover(mut self, color: impl Into<katla_math::Color>) -> Box<dyn Widget> {
        let c = color.into();
        let d = extract_descriptor_mut(&mut self);
        if let ViewDescriptor::Button { hover_color: h, .. } = d {
            *h = Some(c);
        } else {
            debug_assert!(false, "hover() modifier applied to non-Button variant");
        }
        self
    }

    fn border(mut self, color: impl Into<katla_math::Color>) -> Box<dyn Widget> {
        let c = color.into();
        let d = extract_descriptor_mut(&mut self);
        if let ViewDescriptor::Button {
            border_color: b, ..
        } = d
        {
            *b = Some(c);
        } else {
            debug_assert!(false, "border() modifier applied to non-Button variant");
        }
        self
    }

    fn on_click(mut self, cb: Callback) -> Box<dyn Widget> {
        let d = extract_descriptor_mut(&mut self);
        match d {
            ViewDescriptor::Button { on_click: c, .. }
            | ViewDescriptor::ImageButton { on_click: c, .. }
            | ViewDescriptor::Selectable { on_click: c, .. } => *c = Some(cb),
            _ => {
                debug_assert!(false, "on_click() modifier applied to unsupported variant");
            }
        }
        self
    }

    fn on_close(mut self, cb: Callback) -> Box<dyn Widget> {
        let d = extract_descriptor_mut(&mut self);
        if let ViewDescriptor::Modal(desc) = d {
            desc.on_close = Some(cb);
        } else {
            debug_assert!(false, "on_close() modifier applied to non-Modal variant");
        }
        self
    }

    fn enabled(mut self, e: bool) -> Box<dyn Widget> {
        let d = extract_descriptor_mut(&mut self);
        if let ViewDescriptor::ImageButton { enabled: en, .. } = d {
            *en = e;
        } else {
            debug_assert!(
                false,
                "enabled() modifier applied to non-ImageButton variant"
            );
        }
        self
    }

    fn on_submit(mut self, cb: Callback) -> Box<dyn Widget> {
        let d = extract_descriptor_mut(&mut self);
        if let ViewDescriptor::TextField { on_submit: c, .. } = d {
            *c = Some(cb);
        } else {
            debug_assert!(
                false,
                "on_submit() modifier applied to non-TextField variant"
            );
        }
        self
    }

    fn show_value(mut self, show: bool) -> Box<dyn Widget> {
        let d = extract_descriptor_mut(&mut self);
        match d {
            ViewDescriptor::Slider { show_value: s, .. }
            | ViewDescriptor::LabeledSlider { show_value: s, .. } => *s = show,
            _ => {
                debug_assert!(
                    false,
                    "show_value() modifier applied to unsupported variant"
                );
            }
        }
        self
    }

    fn precision(mut self, p: usize) -> Box<dyn Widget> {
        let d = extract_descriptor_mut(&mut self);
        match d {
            ViewDescriptor::Slider { precision: pr, .. }
            | ViewDescriptor::LabeledSlider { precision: pr, .. } => *pr = p,
            _ => {
                debug_assert!(false, "precision() modifier applied to unsupported variant");
            }
        }
        self
    }

    fn label_width(mut self, w: f32) -> Box<dyn Widget> {
        let d = extract_descriptor_mut(&mut self);
        if let ViewDescriptor::LabeledSlider {
            label_width: lw, ..
        } = d
        {
            *lw = w;
        } else {
            debug_assert!(
                false,
                "label_width() modifier applied to non-LabeledSlider variant"
            );
        }
        self
    }

    fn uv(mut self, rect: katla_math::Rect2D) -> Box<dyn Widget> {
        let d = extract_descriptor_mut(&mut self);
        if let ViewDescriptor::Image { uv: u, .. } = d {
            *u = Some(rect);
        } else {
            debug_assert!(false, "uv() modifier applied to non-Image variant");
        }
        self
    }

    // -- Container modifiers --

    fn spacing(mut self, s: f32) -> Box<dyn Widget> {
        let d = extract_descriptor_mut(&mut self);
        match d {
            ViewDescriptor::HStack(desc) | ViewDescriptor::VStack(desc) => desc.spacing = s,
            _ => {
                debug_assert!(false, "spacing() modifier applied to non-stack variant");
            }
        }
        self
    }

    fn padding(mut self, p: Padding) -> Box<dyn Widget> {
        let d = extract_descriptor_mut(&mut self);
        match d {
            ViewDescriptor::HStack(desc) | ViewDescriptor::VStack(desc) => desc.padding = p,
            ViewDescriptor::ZStack(desc) => desc.padding = p,
            _ => {
                debug_assert!(false, "padding() modifier applied to non-stack variant");
            }
        }
        self
    }

    fn padding_all(mut self, v: f32) -> Box<dyn Widget> {
        let p = Padding::all(v);
        let d = extract_descriptor_mut(&mut self);
        match d {
            ViewDescriptor::HStack(desc) | ViewDescriptor::VStack(desc) => desc.padding = p,
            ViewDescriptor::ZStack(desc) => desc.padding = p,
            _ => {
                debug_assert!(false, "padding_all() modifier applied to non-stack variant");
            }
        }
        self
    }

    fn align(mut self, a: Alignment) -> Box<dyn Widget> {
        let d = extract_descriptor_mut(&mut self);
        match d {
            ViewDescriptor::HStack(desc) | ViewDescriptor::VStack(desc) => desc.alignment = a,
            _ => {
                debug_assert!(false, "align() modifier applied to non-stack variant");
            }
        }
        self
    }

    fn header_height(mut self, h: f32) -> Box<dyn Widget> {
        let d = extract_descriptor_mut(&mut self);
        if let ViewDescriptor::Panel(desc) = d {
            desc.header_height = h;
        } else {
            debug_assert!(
                false,
                "header_height() modifier applied to non-Panel variant"
            );
        }
        self
    }

    fn close_on_outside(mut self, close: bool) -> Box<dyn Widget> {
        let d = extract_descriptor_mut(&mut self);
        if let ViewDescriptor::DraggablePanel(desc) = d {
            desc.close_on_outside_click = close;
        } else {
            debug_assert!(
                false,
                "close_on_outside() modifier applied to non-DraggablePanel variant"
            );
        }
        self
    }

    fn right_content(mut self, content: Box<dyn Widget>) -> Box<dyn Widget> {
        let d = extract_descriptor_mut(&mut self);
        if let ViewDescriptor::MenuBar(desc) = d {
            desc.right_content = Some(Box::new(extract_descriptor(&content)));
        } else {
            debug_assert!(
                false,
                "right_content() modifier applied to non-MenuBar variant"
            );
        }
        self
    }

    fn menubar_height(mut self, h: f32) -> Box<dyn Widget> {
        let d = extract_descriptor_mut(&mut self);
        if let ViewDescriptor::MenuBar(desc) = d {
            desc.height = h;
        } else {
            debug_assert!(
                false,
                "menubar_height() modifier applied to non-MenuBar variant"
            );
        }
        self
    }

    fn row_height(mut self, h: f32) -> Box<dyn Widget> {
        let d = extract_descriptor_mut(&mut self);
        if let ViewDescriptor::TreeView(desc) = d {
            desc.row_height = h;
        } else {
            debug_assert!(
                false,
                "row_height() modifier applied to non-TreeView variant"
            );
        }
        self
    }

    fn indent(mut self, i: f32) -> Box<dyn Widget> {
        let d = extract_descriptor_mut(&mut self);
        if let ViewDescriptor::TreeView(desc) = d {
            desc.indent_per_level = i;
        } else {
            debug_assert!(false, "indent() modifier applied to non-TreeView variant");
        }
        self
    }

    fn on_select(mut self, cb: Callback) -> Box<dyn Widget> {
        let d = extract_descriptor_mut(&mut self);
        if let ViewDescriptor::TreeView(desc) = d {
            desc.on_select = Some(cb);
        } else {
            debug_assert!(
                false,
                "on_select() modifier applied to non-TreeView variant"
            );
        }
        self
    }

    fn on_right_click(mut self, cb: Callback) -> Box<dyn Widget> {
        let d = extract_descriptor_mut(&mut self);
        if let ViewDescriptor::TreeView(desc) = d {
            desc.on_right_click = Some(cb);
        } else {
            debug_assert!(
                false,
                "on_right_click() modifier applied to non-TreeView variant"
            );
        }
        self
    }

    // -- Separator / Icon / Selectable / Section modifiers --

    fn separator_color(mut self, color: impl Into<katla_math::Color>) -> Box<dyn Widget> {
        let d = extract_descriptor_mut(&mut self);
        if let ViewDescriptor::Separator { color: c, .. } = d {
            *c = Some(color.into());
        } else {
            debug_assert!(
                false,
                "separator_color() modifier applied to non-Separator variant"
            );
        }
        self
    }

    fn icon_size(mut self, size: FontSize) -> Box<dyn Widget> {
        let d = extract_descriptor_mut(&mut self);
        if let ViewDescriptor::Icon { size: s, .. } = d {
            *s = Some(size);
        } else {
            debug_assert!(false, "icon_size() modifier applied to non-Icon variant");
        }
        self
    }

    fn selected(mut self, sel: bool) -> Box<dyn Widget> {
        let d = extract_descriptor_mut(&mut self);
        if let ViewDescriptor::Selectable { selected: s, .. } = d {
            *s = sel;
        } else {
            debug_assert!(
                false,
                "selected() modifier applied to non-Selectable variant"
            );
        }
        self
    }

    fn on_remove(mut self, cb: Callback) -> Box<dyn Widget> {
        let d = extract_descriptor_mut(&mut self);
        if let ViewDescriptor::Section { on_remove: r, .. } = d {
            *r = Some(cb);
        } else {
            debug_assert!(false, "on_remove() modifier applied to non-Section variant");
        }
        self
    }

    // -- Progress / Grid / Image modifiers --

    fn image_size(mut self, width: f32, height: f32) -> Box<dyn Widget> {
        let d = extract_descriptor_mut(&mut self);
        if let ViewDescriptor::Image {
            width: w,
            height: h,
            ..
        } = d
        {
            *w = Some(width);
            *h = Some(height);
        } else {
            debug_assert!(false, "image_size() modifier applied to non-Image variant");
        }
        self
    }

    fn progress_label(mut self, label: impl Into<String>) -> Box<dyn Widget> {
        let d = extract_descriptor_mut(&mut self);
        if let ViewDescriptor::Progress { label: l, .. } = d {
            *l = Some(label.into());
        } else {
            debug_assert!(
                false,
                "progress_label() modifier applied to non-Progress variant"
            );
        }
        self
    }

    fn grid_spacing(mut self, spacing: f32) -> Box<dyn Widget> {
        let d = extract_descriptor_mut(&mut self);
        if let ViewDescriptor::Grid(desc) = d {
            desc.spacing = spacing;
        } else {
            debug_assert!(false, "grid_spacing() modifier applied to non-Grid variant");
        }
        self
    }

    fn flex_width(mut self, w: f32) -> Box<dyn Widget> {
        let d = extract_descriptor_mut(&mut self);
        match d {
            ViewDescriptor::HStack(desc) | ViewDescriptor::VStack(desc) => {
                desc.flex.width = Some(w)
            }
            ViewDescriptor::ZStack(desc) => desc.flex.width = Some(w),
            ViewDescriptor::Panel(desc) => desc.flex.width = Some(w),
            ViewDescriptor::Grid(desc) => desc.flex.width = Some(w),
            ViewDescriptor::ScrollView(desc) => desc.flex.width = Some(w),
            _ => {
                debug_assert!(
                    false,
                    "flex_width() modifier applied to unsupported variant"
                );
            }
        }
        self
    }

    fn flex_height(mut self, h: f32) -> Box<dyn Widget> {
        let d = extract_descriptor_mut(&mut self);
        match d {
            ViewDescriptor::HStack(desc) | ViewDescriptor::VStack(desc) => {
                desc.flex.height = Some(h)
            }
            ViewDescriptor::ZStack(desc) => desc.flex.height = Some(h),
            ViewDescriptor::Panel(desc) => desc.flex.height = Some(h),
            ViewDescriptor::Grid(desc) => desc.flex.height = Some(h),
            ViewDescriptor::ScrollView(desc) => desc.flex.height = Some(h),
            _ => {
                debug_assert!(
                    false,
                    "flex_height() modifier applied to unsupported variant"
                );
            }
        }
        self
    }

    fn flex_grow(mut self, grow: f32) -> Box<dyn Widget> {
        let d = extract_descriptor_mut(&mut self);
        match d {
            ViewDescriptor::HStack(desc) | ViewDescriptor::VStack(desc) => {
                desc.flex.flex_grow = grow
            }
            ViewDescriptor::ZStack(desc) => desc.flex.flex_grow = grow,
            ViewDescriptor::Panel(desc) => desc.flex.flex_grow = grow,
            ViewDescriptor::Grid(desc) => desc.flex.flex_grow = grow,
            ViewDescriptor::ScrollView(desc) => desc.flex.flex_grow = grow,
            _ => {
                debug_assert!(false, "flex_grow() modifier applied to unsupported variant");
            }
        }
        self
    }
}

// ---------------------------------------------------------------------------
// MenuGroup / MenuEntry helpers
// ---------------------------------------------------------------------------

pub fn menu_group(label: impl Into<String>, open_id: StateId, items: Vec<MenuEntry>) -> MenuGroup {
    MenuGroup {
        label: label.into(),
        open_id,
        items,
    }
}

pub fn menu_entry(label: impl Into<String>) -> MenuEntry {
    MenuEntry {
        label: label.into(),
        on_click: None,
        disabled: false,
    }
}

pub fn menu_entry_disabled(label: impl Into<String>) -> MenuEntry {
    MenuEntry {
        label: label.into(),
        on_click: None,
        disabled: true,
    }
}

impl MenuEntry {
    pub fn on_click(mut self, cb: Callback) -> Self {
        self.on_click = Some(cb);
        self
    }
}

// ---------------------------------------------------------------------------
// ContextMenuEntry helpers
// ---------------------------------------------------------------------------

pub fn context_entry(label: impl Into<String>) -> ContextMenuEntry {
    ContextMenuEntry {
        label: label.into(),
        on_click: None,
        disabled: false,
    }
}

pub fn context_entry_disabled(label: impl Into<String>) -> ContextMenuEntry {
    ContextMenuEntry {
        label: label.into(),
        on_click: None,
        disabled: true,
    }
}

impl ContextMenuEntry {
    pub fn on_click(mut self, cb: Callback) -> Self {
        self.on_click = Some(cb);
        self
    }
}

// ---------------------------------------------------------------------------
// Descriptor access utility
// ---------------------------------------------------------------------------

/// Extract the [`ViewDescriptor`] from a `Box<dyn Widget>` that was produced
/// by a constructor. Panics if the widget is not a `DescriptorWidget`.
pub fn into_descriptor(widget: Box<dyn Widget>) -> ViewDescriptor {
    extract_descriptor(&widget)
}

/// Wrap a [`ViewDescriptor`] in a [`DescriptorWidget`] and return it as `Box<dyn Widget>`.
pub fn into_descriptor_owned(descriptor: ViewDescriptor) -> Box<dyn Widget> {
    wrap(descriptor)
}

/// Wrap a child widget in a transition container.
pub(crate) fn wrap_transition_container(
    child: Box<dyn Widget>,
    transition: super::transition::Transition,
) -> Box<dyn Widget> {
    wrap(ViewDescriptor::TransitionContainer {
        child: Box::new(extract_descriptor(&child)),
        transition,
    })
}

#[cfg(test)]
mod tests {
    use super::super::state::{StateArena, ViewId};
    use super::*;

    fn dummy_state_id() -> StateId {
        let mut arena = StateArena::default();
        arena.get_or_create(ViewId::default(), 0usize)
    }

    fn desc(w: Box<dyn Widget>) -> ViewDescriptor {
        w.as_any()
            .downcast_ref::<DescriptorWidget>()
            .expect("expected DescriptorWidget")
            .descriptor()
            .clone()
    }

    // -- Leaf constructor tests --

    #[test]
    fn test_empty() {
        assert!(matches!(desc(empty()), ViewDescriptor::Empty));
    }

    #[test]
    fn test_text_defaults() {
        let ViewDescriptor::Text {
            content,
            color,
            font_size,
        } = desc(text("hello"))
        else {
            panic!("expected Text");
        };
        assert_eq!(content, "hello");
        assert!(color.is_none());
        assert!(font_size.is_none());
    }

    #[test]
    fn test_button_defaults() {
        let ViewDescriptor::Button {
            label,
            fill_color,
            hover_color,
            border_color,
            on_click,
        } = desc(button("ok"))
        else {
            panic!("expected Button");
        };
        assert_eq!(label, "ok");
        assert!(fill_color.is_none());
        assert!(hover_color.is_none());
        assert!(border_color.is_none());
        assert!(on_click.is_none());
    }

    #[test]
    fn test_image_button_defaults() {
        let ViewDescriptor::ImageButton {
            icon,
            enabled,
            fill_color,
            on_click,
        } = desc(image_button('X'))
        else {
            panic!("expected ImageButton");
        };
        assert_eq!(icon, 'X');
        assert!(enabled);
        assert!(fill_color.is_none());
        assert!(on_click.is_none());
    }

    #[test]
    fn test_slider_defaults() {
        let id = dummy_state_id();
        let ViewDescriptor::Slider {
            label,
            value_id,
            range: _,
            show_value,
            precision,
        } = desc(slider("vol", id, 0.0..=1.0))
        else {
            panic!("expected Slider")
        };
        assert_eq!(label, "vol");
        assert_eq!(value_id, id);
        assert!(!show_value);
        assert_eq!(precision, 2);
    }

    #[test]
    fn test_labeled_slider_defaults() {
        let id = dummy_state_id();
        let ViewDescriptor::LabeledSlider {
            label,
            value_id: _,
            range: _,
            label_width,
            show_value,
            precision,
        } = desc(labeled_slider("vol", id, 0.0..=1.0))
        else {
            panic!("expected LabeledSlider")
        };
        assert_eq!(label, "vol");
        assert_eq!(label_width, 0.0);
        assert!(!show_value);
        assert_eq!(precision, 2);
    }

    #[test]
    fn test_textfield_defaults() {
        let id = dummy_state_id();
        let ViewDescriptor::TextField {
            placeholder,
            value_id: _,
            on_submit,
        } = desc(textfield("type here", id))
        else {
            panic!("expected TextField")
        };
        assert_eq!(placeholder, "type here");
        assert!(on_submit.is_none());
    }

    #[test]
    fn test_progress_defaults() {
        let ViewDescriptor::Progress {
            value,
            range: _,
            fill_color,
            ..
        } = desc(progress(0.5, 0.0..=1.0))
        else {
            panic!("expected Progress")
        };
        assert_eq!(value, 0.5);
        assert!(fill_color.is_none());
    }

    #[test]
    fn test_toggle_constructor() {
        let id = dummy_state_id();
        let ViewDescriptor::Toggle { label, value_id: _ } = desc(toggle("on", id)) else {
            panic!("expected Toggle");
        };
        assert_eq!(label, "on");
    }

    #[test]
    fn test_radio_constructor() {
        let id = dummy_state_id();
        let ViewDescriptor::RadioButton {
            value_id: _,
            index,
            label,
        } = desc(radio(id, 2, "opt"))
        else {
            panic!("expected RadioButton");
        };
        assert_eq!(index, 2);
        assert_eq!(label, "opt");
    }

    #[test]
    fn test_property_row_constructor() {
        let ViewDescriptor::PropertyRow { label, value } = desc(property_row("key", "val")) else {
            panic!("expected PropertyRow");
        };
        assert_eq!(label, "key");
        assert_eq!(value, "val");
    }

    #[test]
    fn test_color_picker_constructor() {
        let id = StateId::test_id();
        let ViewDescriptor::ColorPicker { label, value_id } = desc(color_picker("Pick color", id))
        else {
            panic!("expected ColorPicker");
        };
        assert_eq!(label, "Pick color");
        assert_eq!(value_id, id);
    }

    #[test]
    fn test_color_picker_diff_update() {
        use crate::declarative::diff::{DiffAction, diff_descriptor};
        let id = StateId::test_id();
        let a = color_picker("Color A", id);
        let b = color_picker("Color B", id);
        assert_eq!(diff_descriptor(&desc(a), &desc(b)), DiffAction::Update);
    }

    #[test]
    fn test_color_picker_diff_replace() {
        use crate::declarative::diff::{DiffAction, diff_descriptor};
        let id = StateId::test_id();
        let a = color_picker("Color", id);
        let b = text("Not a color picker");
        assert_eq!(diff_descriptor(&desc(a), &desc(b)), DiffAction::Replace);
    }

    #[test]
    fn test_color_picker_state_round_trip() {
        let mut arena = StateArena::new();
        let view_id = ViewId::default();
        let state_id = arena.get_or_create(view_id, 0.5f32);
        let _picker = color_picker("Color", state_id);
        arena.set(state_id, 0.8f32);
        let value: f32 = arena.get(state_id).unwrap();
        assert!((value - 0.8).abs() < f32::EPSILON);
    }

    #[test]
    fn test_vec3_slider_constructor() {
        let ids = [dummy_state_id(), dummy_state_id(), dummy_state_id()];
        let ViewDescriptor::Vec3Slider {
            label,
            value_ids,
            range,
            axis_labels,
            axis_colors,
            precision,
        } = desc(vec3_slider("position", ids, -10.0..=10.0))
        else {
            panic!("expected Vec3Slider");
        };
        assert_eq!(label, "position");
        assert_eq!(value_ids, ids);
        assert_eq!(range, -10.0..=10.0);
        assert_eq!(axis_labels, ["X", "Y", "Z"]);
        assert_eq!(
            axis_colors,
            [
                katla_math::Color::RED,
                katla_math::Color::GREEN,
                katla_math::Color::BLUE
            ]
        );
        assert_eq!(precision, 2);
    }

    #[test]
    fn test_image_constructor() {
        let vd = image(TextureId(42), katla_math::Color::WHITE);
        let ViewDescriptor::Image {
            texture, uv, tint, ..
        } = desc(vd)
        else {
            panic!("expected Image")
        };
        assert_eq!(texture.0, 42);
        assert!(uv.is_none());
        assert_eq!(tint, katla_math::Color::WHITE);
    }

    // -- Container constructor tests --

    #[test]
    fn test_hstack_defaults() {
        let ViewDescriptor::HStack(d) = desc(hstack([text("a"), text("b")])) else {
            panic!("expected HStack");
        };
        assert_eq!(d.children.len(), 2);
        assert_eq!(d.spacing, 0.0);
        assert_eq!(d.padding, Padding::zero());
        assert_eq!(d.alignment, Alignment::Leading);
    }

    #[test]
    fn test_vstack_defaults() {
        let ViewDescriptor::VStack(d) = desc(vstack([text("a")])) else {
            panic!("expected VStack");
        };
        assert_eq!(d.children.len(), 1);
    }

    #[test]
    fn test_zstack_defaults() {
        let vd = zstack([(Alignment::Center, text("c"))]);
        let ViewDescriptor::ZStack(d) = desc(vd) else {
            panic!("expected ZStack")
        };
        assert_eq!(d.children.len(), 1);
        assert_eq!(d.padding, Padding::zero());
    }

    #[test]
    fn test_panel_defaults() {
        let ViewDescriptor::Panel(d) = desc(panel("title", text("body"))) else {
            panic!("expected Panel");
        };
        assert_eq!(d.title, "title");
        assert_eq!(d.header_height, 24.0);
    }

    #[test]
    fn test_scroll_constructor() {
        let id = dummy_state_id();
        assert!(matches!(
            desc(scroll(text("c"), id)),
            ViewDescriptor::ScrollView(_)
        ));
    }

    #[test]
    fn test_overlay_constructor() {
        assert!(matches!(
            desc(overlay(Anchor::TopLeft, Vec2::ZERO, text("o"))),
            ViewDescriptor::Overlay(_)
        ));
    }

    #[test]
    fn test_statusbar_constructor() {
        assert!(matches!(
            desc(statusbar(24.0, text("s"))),
            ViewDescriptor::StatusBar(_)
        ));
    }

    #[test]
    fn test_draggable_panel_defaults() {
        let id = dummy_state_id();
        let ViewDescriptor::DraggablePanel(d) =
            desc(draggable_panel("p", 200.0, 300.0, text("c"), id))
        else {
            panic!("expected DraggablePanel");
        };
        assert_eq!(d.title, "p");
        assert_eq!(d.width, 200.0);
        assert!(!d.close_on_outside_click);
    }

    #[test]
    fn test_menubar_defaults() {
        let ViewDescriptor::MenuBar(d) = desc(menubar(vec![])) else {
            panic!("expected MenuBar");
        };
        assert!(d.right_content.is_none());
        assert_eq!(d.height, 28.0);
    }

    #[test]
    fn test_tree_view_defaults() {
        let (e, s, sc) = (dummy_state_id(), dummy_state_id(), dummy_state_id());
        let ViewDescriptor::TreeView(d) = desc(tree_view(vec![], e, s, sc)) else {
            panic!("expected TreeView");
        };
        assert_eq!(d.row_height, 20.0);
        assert_eq!(d.indent_per_level, 16.0);
        assert!(d.on_select.is_none());
        assert!(d.on_right_click.is_none());
    }

    #[test]
    fn test_modal_constructor() {
        let id = dummy_state_id();
        assert!(matches!(
            desc(modal(400.0, 300.0, id, text("m"))),
            ViewDescriptor::Modal(_)
        ));
    }

    #[test]
    fn test_context_menu_constructor() {
        let id = dummy_state_id();
        assert!(matches!(
            desc(context_menu(vec![], id)),
            ViewDescriptor::ContextMenu(_)
        ));
    }

    // -- Modifier tests (via WidgetExt) --

    #[test]
    fn test_color_modifier() {
        let vd = text("hi").color(katla_math::Color::RED);
        let ViewDescriptor::Text { color, .. } = desc(vd) else {
            panic!("expected Text")
        };
        assert_eq!(color, Some(katla_math::Color::RED));
    }

    #[test]
    fn test_font_size_modifier() {
        let vd = text("hi").font_size(FontSize::Small);
        let ViewDescriptor::Text { font_size, .. } = desc(vd) else {
            panic!("expected Text")
        };
        assert_eq!(font_size, Some(FontSize::Small));
    }

    #[test]
    fn test_fill_modifier_button() {
        let vd = button("ok").fill(katla_math::Color::BLUE);
        let ViewDescriptor::Button { fill_color, .. } = desc(vd) else {
            panic!("expected Button")
        };
        assert_eq!(fill_color, Some(katla_math::Color::BLUE));
    }

    #[test]
    fn test_fill_modifier_image_button() {
        let vd = image_button('X').fill(katla_math::Color::GREEN);
        let ViewDescriptor::ImageButton { fill_color, .. } = desc(vd) else {
            panic!("expected ImageButton")
        };
        assert_eq!(fill_color, Some(katla_math::Color::GREEN));
    }

    #[test]
    fn test_fill_modifier_progress() {
        let vd = progress(0.5, 0.0..=1.0).fill(katla_math::Color::WHITE);
        let ViewDescriptor::Progress { fill_color, .. } = desc(vd) else {
            panic!("expected Progress")
        };
        assert_eq!(fill_color, Some(katla_math::Color::WHITE));
    }

    #[test]
    fn test_hover_modifier() {
        let vd = button("ok").hover(katla_math::Color::RED);
        let ViewDescriptor::Button { hover_color, .. } = desc(vd) else {
            panic!("expected Button")
        };
        assert_eq!(hover_color, Some(katla_math::Color::RED));
    }

    #[test]
    fn test_border_modifier() {
        let vd = button("ok").border(katla_math::Color::BLACK);
        let ViewDescriptor::Button { border_color, .. } = desc(vd) else {
            panic!("expected Button")
        };
        assert_eq!(border_color, Some(katla_math::Color::BLACK));
    }

    #[test]
    fn test_enabled_modifier() {
        let vd = image_button('X').enabled(false);
        let ViewDescriptor::ImageButton { enabled, .. } = desc(vd) else {
            panic!("expected ImageButton")
        };
        assert!(!enabled);
    }

    #[test]
    fn test_show_value_modifier_slider() {
        let id = dummy_state_id();
        let vd = slider("s", id, 0.0..=1.0).show_value(true);
        let ViewDescriptor::Slider { show_value, .. } = desc(vd) else {
            panic!("expected Slider")
        };
        assert!(show_value);
    }

    #[test]
    fn test_show_value_modifier_labeled_slider() {
        let id = dummy_state_id();
        let vd = labeled_slider("s", id, 0.0..=1.0).show_value(true);
        let ViewDescriptor::LabeledSlider { show_value, .. } = desc(vd) else {
            panic!("expected LabeledSlider")
        };
        assert!(show_value);
    }

    #[test]
    fn test_precision_modifier() {
        let id = dummy_state_id();
        let vd = slider("s", id, 0.0..=1.0).precision(4);
        let ViewDescriptor::Slider { precision, .. } = desc(vd) else {
            panic!("expected Slider")
        };
        assert_eq!(precision, 4);
    }

    #[test]
    fn test_label_width_modifier() {
        let id = dummy_state_id();
        let vd = labeled_slider("s", id, 0.0..=1.0).label_width(120.0);
        let ViewDescriptor::LabeledSlider { label_width, .. } = desc(vd) else {
            panic!("expected LabeledSlider")
        };
        assert_eq!(label_width, 120.0);
    }

    #[test]
    fn test_uv_modifier() {
        let vd = image(TextureId(1), katla_math::Color::WHITE)
            .uv(katla_math::Rect2D::new(Vec2::ZERO, Vec2::new(1.0, 1.0)));
        let ViewDescriptor::Image { uv, .. } = desc(vd) else {
            panic!("expected Image")
        };
        assert!(uv.is_some());
    }

    // -- Container modifier tests --

    #[test]
    fn test_spacing_modifier() {
        let vd = hstack([text("a")]).spacing(8.0);
        let ViewDescriptor::HStack(d) = desc(vd) else {
            panic!("expected HStack")
        };
        assert_eq!(d.spacing, 8.0);
    }

    #[test]
    fn test_padding_modifier() {
        let vd = vstack([text("a")]).padding(Padding::all(10.0));
        let ViewDescriptor::VStack(d) = desc(vd) else {
            panic!("expected VStack")
        };
        assert_eq!(d.padding, Padding::all(10.0));
    }

    #[test]
    fn test_padding_all_modifier() {
        let vd = zstack([]).padding_all(12.0);
        let ViewDescriptor::ZStack(d) = desc(vd) else {
            panic!("expected ZStack")
        };
        assert_eq!(d.padding, Padding::all(12.0));
    }

    #[test]
    fn test_align_modifier() {
        let vd = hstack([]).align(Alignment::Center);
        let ViewDescriptor::HStack(d) = desc(vd) else {
            panic!("expected HStack")
        };
        assert_eq!(d.alignment, Alignment::Center);
    }

    #[test]
    fn test_header_height_modifier() {
        let vd = panel("t", text("c")).header_height(32.0);
        let ViewDescriptor::Panel(d) = desc(vd) else {
            panic!("expected Panel")
        };
        assert_eq!(d.header_height, 32.0);
    }

    #[test]
    fn test_close_on_outside_modifier() {
        let id = dummy_state_id();
        let vd = draggable_panel("p", 200.0, 300.0, text("c"), id).close_on_outside(true);
        let ViewDescriptor::DraggablePanel(d) = desc(vd) else {
            panic!("expected DraggablePanel")
        };
        assert!(d.close_on_outside_click);
    }

    #[test]
    fn test_right_content_modifier() {
        let vd = menubar(vec![]).right_content(text("r"));
        let ViewDescriptor::MenuBar(d) = desc(vd) else {
            panic!("expected MenuBar")
        };
        assert!(d.right_content.is_some());
    }

    #[test]
    fn test_menubar_height_modifier() {
        let vd = menubar(vec![]).menubar_height(40.0);
        let ViewDescriptor::MenuBar(d) = desc(vd) else {
            panic!("expected MenuBar")
        };
        assert_eq!(d.height, 40.0);
    }

    #[test]
    fn test_row_height_modifier() {
        let (e, s, sc) = (dummy_state_id(), dummy_state_id(), dummy_state_id());
        let vd = tree_view(vec![], e, s, sc).row_height(30.0);
        let ViewDescriptor::TreeView(d) = desc(vd) else {
            panic!("expected TreeView")
        };
        assert_eq!(d.row_height, 30.0);
    }

    #[test]
    fn test_indent_modifier() {
        let (e, s, sc) = (dummy_state_id(), dummy_state_id(), dummy_state_id());
        let vd = tree_view(vec![], e, s, sc).indent(24.0);
        let ViewDescriptor::TreeView(d) = desc(vd) else {
            panic!("expected TreeView")
        };
        assert_eq!(d.indent_per_level, 24.0);
    }

    // -- Chained modifier test --

    #[test]
    fn test_chained_modifiers() {
        let vd = hstack([text("a"), text("b")])
            .spacing(8.0)
            .padding_all(4.0)
            .align(Alignment::Center);

        let ViewDescriptor::HStack(d) = desc(vd) else {
            panic!("expected HStack")
        };
        assert_eq!(d.spacing, 8.0);
        assert_eq!(d.padding, Padding::all(4.0));
        assert_eq!(d.alignment, Alignment::Center);
        assert_eq!(d.children.len(), 2);
    }

    // -- Separator tests --

    #[test]
    fn test_separator_defaults() {
        let ViewDescriptor::Separator { direction, color } =
            desc(separator(SeparatorDirection::Horizontal))
        else {
            panic!("expected Separator")
        };
        assert_eq!(direction, SeparatorDirection::Horizontal);
        assert!(color.is_none());
    }

    #[test]
    fn test_separator_horizontal_shortcut() {
        let ViewDescriptor::Separator { direction, .. } = desc(separator_horizontal()) else {
            panic!("expected Separator")
        };
        assert_eq!(direction, SeparatorDirection::Horizontal);
    }

    #[test]
    fn test_separator_vertical_shortcut() {
        let ViewDescriptor::Separator { direction, .. } = desc(separator_vertical()) else {
            panic!("expected Separator")
        };
        assert_eq!(direction, SeparatorDirection::Vertical);
    }

    #[test]
    fn test_separator_color_modifier() {
        let vd = separator_horizontal().separator_color(katla_math::Color::RED);
        let ViewDescriptor::Separator { color, .. } = desc(vd) else {
            panic!("expected Separator")
        };
        assert_eq!(color, Some(katla_math::Color::RED));
    }

    // -- Icon tests --

    #[test]
    fn test_icon_defaults() {
        let ViewDescriptor::Icon { icon, size, color } = desc(super::icon('X')) else {
            panic!("expected Icon")
        };
        assert_eq!(icon, 'X');
        assert!(size.is_none());
        assert!(color.is_none());
    }

    #[test]
    fn test_icon_size_modifier() {
        let vd = super::icon('A').icon_size(FontSize::Large);
        let ViewDescriptor::Icon { size, .. } = desc(vd) else {
            panic!("expected Icon")
        };
        assert_eq!(size, Some(FontSize::Large));
    }

    #[test]
    fn test_icon_color_modifier() {
        let vd = super::icon('B').color(katla_math::Color::GREEN);
        let ViewDescriptor::Icon { color, .. } = desc(vd) else {
            panic!("expected Icon")
        };
        assert_eq!(color, Some(katla_math::Color::GREEN));
    }

    // -- Selectable tests --

    #[test]
    fn test_selectable_defaults() {
        let ViewDescriptor::Selectable {
            on_click,
            selected,
            child,
        } = desc(selectable(text("item")))
        else {
            panic!("expected Selectable")
        };
        assert!(on_click.is_none());
        assert!(!selected);
        assert!(matches!(*child, ViewDescriptor::Text { .. }));
    }

    #[test]
    fn test_selectable_selected_modifier() {
        let vd = selectable(text("x")).selected(true);
        let ViewDescriptor::Selectable { selected, .. } = desc(vd) else {
            panic!("expected Selectable")
        };
        assert!(selected);
    }

    #[test]
    fn test_selectable_on_click_modifier() {
        let vd = selectable(text("x")).on_click(Callback(42));
        let ViewDescriptor::Selectable { on_click, .. } = desc(vd) else {
            panic!("expected Selectable")
        };
        assert!(on_click.is_some());
    }

    // -- Section tests --

    #[test]
    fn test_section_defaults() {
        let id = dummy_state_id();
        let ViewDescriptor::Section {
            title,
            expanded_id,
            on_remove,
            child,
        } = desc(section("My Section", text("content"), id))
        else {
            panic!("expected Section")
        };
        assert_eq!(title, "My Section");
        assert_eq!(expanded_id, id);
        assert!(on_remove.is_none());
        assert!(matches!(*child, ViewDescriptor::Text { .. }));
    }

    #[test]
    fn test_section_on_remove_modifier() {
        let id = dummy_state_id();
        let vd = section("s", text("c"), id).on_remove(Callback(99));
        let ViewDescriptor::Section { on_remove, .. } = desc(vd) else {
            panic!("expected Section")
        };
        assert!(on_remove.is_some());
    }

    // -- TabBar tests --

    #[test]
    fn test_tab_bar_defaults() {
        let id = dummy_state_id();
        let ViewDescriptor::TabBar(d) = desc(tab_bar(
            vec![tab_item("A"), tab_item("B")],
            id,
            text("content"),
        )) else {
            panic!("expected TabBar")
        };
        assert_eq!(d.tabs.len(), 2);
        assert_eq!(d.tabs[0].label, "A");
        assert_eq!(d.selected_id, id);
        assert!(matches!(*d.content, ViewDescriptor::Text { .. }));
    }

    // -- Grid tests --

    #[test]
    fn test_grid_defaults() {
        let vd = grid(3, Vec2::new(100.0, 50.0), [text("a"), text("b"), text("c")]);
        let ViewDescriptor::Grid(d) = desc(vd) else {
            panic!("expected Grid")
        };
        assert_eq!(d.columns, 3);
        assert_eq!(d.cell_size, Vec2::new(100.0, 50.0));
        assert_eq!(d.spacing, 0.0);
        assert_eq!(d.children.len(), 3);
    }

    #[test]
    fn test_grid_spacing_modifier() {
        let vd = grid(2, Vec2::new(50.0, 50.0), []).grid_spacing(8.0);
        let d = desc(vd);
        let ViewDescriptor::Grid { .. } = d else {
            panic!("expected Grid")
        };
        let ViewDescriptor::Grid(d) = d else {
            panic!("expected Grid")
        };
        assert_eq!(d.spacing, 8.0);
    }

    // -- Progress label tests --

    #[test]
    fn test_progress_defaults_no_label() {
        let ViewDescriptor::Progress { label, .. } = desc(progress(0.5, 0.0..=1.0)) else {
            panic!("expected Progress")
        };
        assert!(label.is_none());
    }

    #[test]
    fn test_progress_label_modifier() {
        let vd = progress(0.5, 0.0..=1.0).progress_label("50%");
        let ViewDescriptor::Progress { label, .. } = desc(vd) else {
            panic!("expected Progress")
        };
        assert_eq!(label, Some("50%".to_string()));
    }

    // -- on_click modifier on Button / ImageButton --

    #[test]
    fn test_on_click_modifier_button() {
        let vd = button("ok").on_click(Callback(1));
        let ViewDescriptor::Button { on_click, .. } = desc(vd) else {
            panic!("expected Button")
        };
        assert!(on_click.is_some());
    }

    #[test]
    fn test_on_click_modifier_image_button() {
        let vd = image_button('X').on_click(Callback(2));
        let ViewDescriptor::ImageButton { on_click, .. } = desc(vd) else {
            panic!("expected ImageButton")
        };
        assert!(on_click.is_some());
    }

    // -- on_submit modifier on TextField --

    #[test]
    fn test_on_submit_modifier() {
        let id = dummy_state_id();
        let vd = textfield("ph", id).on_submit(Callback(3));
        let ViewDescriptor::TextField { on_submit, .. } = desc(vd) else {
            panic!("expected TextField")
        };
        assert!(on_submit.is_some());
    }

    // -- on_select / on_right_click on TreeView --

    #[test]
    fn test_on_select_modifier() {
        let (e, s, sc) = (dummy_state_id(), dummy_state_id(), dummy_state_id());
        let vd = tree_view(vec![], e, s, sc).on_select(Callback(10));
        let ViewDescriptor::TreeView(d) = desc(vd) else {
            panic!("expected TreeView")
        };
        assert!(d.on_select.is_some());
    }

    #[test]
    fn test_on_right_click_modifier() {
        let (e, s, sc) = (dummy_state_id(), dummy_state_id(), dummy_state_id());
        let vd = tree_view(vec![], e, s, sc).on_right_click(Callback(11));
        let ViewDescriptor::TreeView(d) = desc(vd) else {
            panic!("expected TreeView")
        };
        assert!(d.on_right_click.is_some());
    }

    // -- Keyed constructor tests --

    #[test]
    fn test_keyed_helper() {
        let kc = keyed(42, text("k"));
        assert_eq!(kc.key, Some(42));
        assert!(matches!(desc(kc.widget), ViewDescriptor::Text { .. }));
    }

    #[test]
    fn test_hstack_keyed_constructor() {
        let children = vec![keyed(1, text("a")), keyed(2, text("b"))];
        let ViewDescriptor::HStack(d) = desc(hstack_keyed(children)) else {
            panic!("expected HStack")
        };
        assert_eq!(d.children.len(), 2);
        assert_eq!(d.children[0].key, Some(1));
        assert_eq!(d.children[1].key, Some(2));
    }

    #[test]
    fn test_vstack_keyed_constructor() {
        let children = vec![keyed(10, text("x"))];
        let ViewDescriptor::VStack(d) = desc(vstack_keyed(children)) else {
            panic!("expected VStack")
        };
        assert_eq!(d.children.len(), 1);
        assert_eq!(d.children[0].key, Some(10));
    }

    #[test]
    fn test_zstack_keyed_constructor() {
        let children = vec![(Alignment::Center, keyed(5, text("z")))];
        let ViewDescriptor::ZStack(d) = desc(zstack_keyed(children)) else {
            panic!("expected ZStack")
        };
        assert_eq!(d.children.len(), 1);
        assert_eq!(d.children[0].0, Alignment::Center);
        assert_eq!(d.children[0].1.key, Some(5));
    }

    #[test]
    fn test_grid_keyed_constructor() {
        let children = vec![keyed(1, text("a")), keyed(2, text("b"))];
        let ViewDescriptor::Grid(d) = desc(grid_keyed(2, Vec2::new(50.0, 50.0), children)) else {
            panic!("expected Grid")
        };
        assert_eq!(d.columns, 2);
        assert_eq!(d.children.len(), 2);
        assert_eq!(d.children[0].key, Some(1));
    }

    // -- Menu helper tests --

    #[test]
    fn test_menu_group_constructor() {
        let id = dummy_state_id();
        let mg = menu_group("File", id, vec![menu_entry("Open"), menu_entry("Save")]);
        assert_eq!(mg.label, "File");
        assert_eq!(mg.open_id, id);
        assert_eq!(mg.items.len(), 2);
        assert_eq!(mg.items[0].label, "Open");
        assert!(mg.items[0].on_click.is_none());
        assert!(!mg.items[0].disabled);
    }

    #[test]
    fn test_menu_entry_disabled() {
        let me = menu_entry_disabled("Greyed");
        assert!(me.disabled);
        assert!(me.on_click.is_none());
    }

    #[test]
    fn test_menu_entry_on_click() {
        let me = menu_entry("Click").on_click(Callback(7));
        assert!(me.on_click.is_some());
        assert!(!me.disabled);
    }

    // -- ContextMenuEntry helper tests --

    #[test]
    fn test_context_entry_constructor() {
        let ce = context_entry("Copy");
        assert_eq!(ce.label, "Copy");
        assert!(ce.on_click.is_none());
        assert!(!ce.disabled);
    }

    #[test]
    fn test_context_entry_disabled() {
        let ce = context_entry_disabled("Paste");
        assert!(ce.disabled);
    }

    #[test]
    fn test_context_entry_on_click() {
        let ce = context_entry("Cut").on_click(Callback(8));
        assert!(ce.on_click.is_some());
    }

    // -- Misapplied modifier no-op tests (release only) --

    #[cfg(not(debug_assertions))]
    #[test]
    fn test_color_on_hstack_is_noop() {
        let vd = hstack([text("a")]).color(katla_math::Color::RED);
        let ViewDescriptor::HStack(d) = desc(vd) else {
            panic!("expected HStack")
        };
        assert_eq!(d.children.len(), 1);
    }

    #[cfg(not(debug_assertions))]
    #[test]
    fn test_fill_on_text_is_noop() {
        let vd = text("hi").fill(katla_math::Color::BLUE);
        let ViewDescriptor::Text {
            color, font_size, ..
        } = desc(vd)
        else {
            panic!("expected Text")
        };
        assert!(color.is_none());
        assert!(font_size.is_none());
    }

    #[cfg(not(debug_assertions))]
    #[test]
    fn test_spacing_on_text_is_noop() {
        let vd = text("hi").spacing(10.0);
        assert!(matches!(desc(vd), ViewDescriptor::Text { .. }));
    }

    #[cfg(not(debug_assertions))]
    #[test]
    fn test_padding_all_on_button_is_noop() {
        let vd = button("ok").padding_all(5.0);
        let ViewDescriptor::Button {
            fill_color,
            on_click,
            ..
        } = desc(vd)
        else {
            panic!("expected Button")
        };
        assert!(fill_color.is_none());
        assert!(on_click.is_none());
    }

    #[cfg(not(debug_assertions))]
    #[test]
    fn test_font_size_on_button_is_noop() {
        let vd = button("ok").font_size(FontSize::Small);
        assert!(matches!(desc(vd), ViewDescriptor::Button { .. }));
    }
}
