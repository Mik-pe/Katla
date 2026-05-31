use std::ops::RangeInclusive;
use std::sync::Arc;

use katla_math::Vec2;

use crate::types::TextureId;

use super::descriptor::{
    Alignment, Anchor, Callback, ContextMenuEntry, FlexProps, MenuEntry, MenuGroup, Padding,
    SeparatorDirection, TreeItem,
};
use super::state::StateId;
use super::widget::Widget;
use super::widgets;

// ---------------------------------------------------------------------------
// KeyedChild — child widget with optional stable key for diffing
// ---------------------------------------------------------------------------

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
// Leaf constructors — return concrete widget types
// ---------------------------------------------------------------------------

pub fn empty() -> widgets::empty::Empty {
    widgets::empty::Empty
}

pub fn text(content: impl Into<String>) -> widgets::text::Text {
    widgets::text::Text {
        content: content.into(),
        color: None,
        font_size: None,
    }
}

pub fn button(label: impl Into<String>) -> widgets::button::Button {
    widgets::button::Button {
        label: label.into(),
        fill_color: None,
        hover_color: None,
        border_color: None,
        on_click: None,
    }
}

pub fn image_button(icon: char) -> widgets::image_button::ImageButton {
    widgets::image_button::ImageButton {
        icon,
        enabled: true,
        fill_color: None,
        on_click: None,
    }
}

pub fn slider(
    label: impl Into<String>,
    value_id: StateId,
    range: RangeInclusive<f32>,
) -> widgets::slider::Slider {
    widgets::slider::Slider {
        label: label.into(),
        value_id,
        range,
        show_value: false,
        precision: 2,
    }
}

pub fn labeled_slider(
    label: impl Into<String>,
    value_id: StateId,
    range: RangeInclusive<f32>,
) -> widgets::labeled_slider::LabeledSlider {
    widgets::labeled_slider::LabeledSlider {
        label: label.into(),
        value_id,
        range,
        label_width: 0.0,
        show_value: false,
        precision: 2,
    }
}

pub fn textfield(
    placeholder: impl Into<String>,
    value_id: StateId,
) -> widgets::textfield::TextField {
    widgets::textfield::TextField {
        placeholder: placeholder.into(),
        value_id,
        on_submit: None,
    }
}

pub fn progress(value: f32, range: RangeInclusive<f32>) -> widgets::progress::Progress {
    widgets::progress::Progress {
        value,
        range,
        fill_color: None,
        label: None,
    }
}

pub fn vu_meter(peak_db: f32, rms_db: f32) -> widgets::vu_meter::VuMeter {
    widgets::vu_meter::VuMeter { peak_db, rms_db }
}

pub fn image(texture: TextureId, tint: katla_math::Color) -> widgets::image::Image {
    widgets::image::Image {
        texture,
        uv: None,
        tint,
        width: None,
        height: None,
    }
}

pub fn toggle(label: impl Into<String>, value_id: StateId) -> widgets::toggle::Toggle {
    widgets::toggle::Toggle {
        label: label.into(),
        value_id,
    }
}

pub fn radio(
    value_id: StateId,
    index: usize,
    label: impl Into<String>,
) -> widgets::radio::RadioButton {
    widgets::radio::RadioButton {
        value_id,
        index,
        label: label.into(),
    }
}

pub fn property_row(
    label: impl Into<String>,
    value: impl Into<String>,
) -> widgets::property_row::PropertyRow {
    widgets::property_row::PropertyRow {
        label: label.into(),
        value: value.into(),
    }
}

pub fn color_picker(
    label: impl Into<String>,
    value_id: StateId,
) -> widgets::color_picker::ColorPicker {
    widgets::color_picker::ColorPicker {
        label: label.into(),
        value_id,
    }
}

pub fn vec3_slider(
    label: impl Into<String>,
    value_ids: [StateId; 3],
    range: RangeInclusive<f32>,
) -> widgets::vec3_slider::Vec3Slider {
    widgets::vec3_slider::Vec3Slider {
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
    }
}

pub fn separator(direction: SeparatorDirection) -> widgets::separator::Separator {
    widgets::separator::Separator {
        direction,
        color: None,
    }
}

pub fn separator_horizontal() -> widgets::separator::Separator {
    separator(SeparatorDirection::Horizontal)
}

pub fn separator_vertical() -> widgets::separator::Separator {
    separator(SeparatorDirection::Vertical)
}

pub fn icon(icon: char) -> widgets::icon::Icon {
    widgets::icon::Icon {
        icon,
        size: None,
        color: None,
    }
}

pub fn selectable(child: Box<dyn Widget>) -> widgets::selectable::Selectable {
    widgets::selectable::Selectable {
        on_click: None,
        selected: false,
        child_widget: Some(child),
        children: Vec::new(),
    }
}

pub fn section(
    title: impl Into<String>,
    child: Box<dyn Widget>,
    expanded_id: StateId,
) -> widgets::section::Section {
    widgets::section::Section {
        title: title.into(),
        expanded_id,
        on_remove: None,
        child_widget: Some(child),
        children: Vec::new(),
    }
}

pub fn tab_bar(
    tabs: Vec<super::descriptor::TabItem>,
    selected_id: StateId,
    content: Box<dyn Widget>,
) -> widgets::tab_bar::TabBar {
    widgets::tab_bar::TabBar {
        tabs,
        selected_id,
        child_widget: Some(content),
        children: Vec::new(),
    }
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
) -> widgets::grid::Grid {
    let cw = cell_size.x();
    let ch = cell_size.y();
    let child_widgets: Vec<KeyedChild> = children
        .into_iter()
        .map(|child| {
            let wrapper = widgets::vstack::VStack::new(
                0.0,
                Padding::zero(),
                Alignment::Leading,
                FlexProps {
                    width: Some(cw),
                    height: Some(ch),
                    flex_grow: 0.0,
                    flex_shrink: 0.0,
                    ..FlexProps::default()
                },
                vec![KeyedChild {
                    key: None,
                    widget: child,
                }],
            );
            KeyedChild {
                key: None,
                widget: Box::new(wrapper),
            }
        })
        .collect();
    widgets::grid::Grid::new(columns, cell_size, 0.0, FlexProps::default(), child_widgets)
}

// ---------------------------------------------------------------------------
// Container constructors — return concrete widget types
// ---------------------------------------------------------------------------

pub fn hstack(children: impl IntoIterator<Item = Box<dyn Widget>>) -> widgets::hstack::HStack {
    let child_widgets: Vec<KeyedChild> = children
        .into_iter()
        .map(|c| KeyedChild {
            key: None,
            widget: c,
        })
        .collect();
    widgets::hstack::HStack::new(
        0.0,
        Padding::zero(),
        Alignment::Leading,
        FlexProps::default(),
        child_widgets,
    )
}

pub fn vstack(children: impl IntoIterator<Item = Box<dyn Widget>>) -> widgets::vstack::VStack {
    let child_widgets: Vec<KeyedChild> = children
        .into_iter()
        .map(|c| KeyedChild {
            key: None,
            widget: c,
        })
        .collect();
    widgets::vstack::VStack::new(
        0.0,
        Padding::zero(),
        Alignment::Leading,
        FlexProps::default(),
        child_widgets,
    )
}

pub fn zstack(
    children: impl IntoIterator<Item = (Alignment, Box<dyn Widget>)>,
) -> widgets::zstack::ZStack {
    let child_widgets: Vec<(Alignment, KeyedChild)> = children
        .into_iter()
        .map(|(a, w)| {
            (
                a,
                KeyedChild {
                    key: None,
                    widget: w,
                },
            )
        })
        .collect();
    widgets::zstack::ZStack::new(Padding::zero(), FlexProps::default(), child_widgets)
}

pub fn panel(title: impl Into<String>, content: Box<dyn Widget>) -> widgets::panel::Panel {
    widgets::panel::Panel::new(title.into(), 24.0, FlexProps::default(), Some(content))
}

pub fn scroll(content: Box<dyn Widget>, scroll_state_id: StateId) -> widgets::scroll::ScrollView {
    widgets::scroll::ScrollView::new(scroll_state_id, FlexProps::default(), Some(content))
}

pub fn overlay(
    anchor: Anchor,
    offset: Vec2,
    content: Box<dyn Widget>,
) -> widgets::overlay::Overlay {
    widgets::overlay::Overlay::new(anchor, offset, Some(content))
}

pub fn statusbar(height: f32, content: Box<dyn Widget>) -> widgets::statusbar::StatusBar {
    widgets::statusbar::StatusBar::new(height, Some(content))
}

pub fn draggable_panel(
    title: impl Into<String>,
    width: f32,
    height: f32,
    content: Box<dyn Widget>,
    state_id: StateId,
) -> widgets::draggable_panel::DraggablePanel {
    widgets::draggable_panel::DraggablePanel::new(
        title.into(),
        width,
        height,
        state_id,
        false,
        Some(content),
    )
}

pub fn menubar(groups: Vec<MenuGroup>) -> widgets::menubar::MenuBar {
    widgets::menubar::MenuBar::new(groups, None, 28.0)
}

pub fn tree_view(
    items: Vec<TreeItem>,
    expanded_id: StateId,
    selected_id: StateId,
    scroll_id: StateId,
) -> widgets::tree_view::TreeView {
    widgets::tree_view::TreeView::new(
        items,
        expanded_id,
        selected_id,
        scroll_id,
        20.0,
        16.0,
        None,
        None,
    )
}

pub fn modal(
    width: f32,
    height: f32,
    open_id: StateId,
    content: Box<dyn Widget>,
) -> widgets::modal::Modal {
    widgets::modal::Modal::new(width, height, open_id, None, Some(content))
}

pub fn context_menu(
    items: Vec<ContextMenuEntry>,
    open_id: StateId,
) -> widgets::context_menu::ContextMenu {
    widgets::context_menu::ContextMenu::new(items, open_id)
}

// ---------------------------------------------------------------------------
// Keyed child helpers
// ---------------------------------------------------------------------------

pub fn keyed(key: u64, widget: Box<dyn Widget>) -> KeyedChild {
    KeyedChild {
        key: Some(key),
        widget,
    }
}

pub fn hstack_keyed(children: Vec<KeyedChild>) -> widgets::hstack::HStack {
    widgets::hstack::HStack::new(
        0.0,
        Padding::zero(),
        Alignment::Leading,
        FlexProps::default(),
        children,
    )
}

pub fn vstack_keyed(children: Vec<KeyedChild>) -> widgets::vstack::VStack {
    widgets::vstack::VStack::new(
        0.0,
        Padding::zero(),
        Alignment::Leading,
        FlexProps::default(),
        children,
    )
}

pub fn zstack_keyed(children: Vec<(Alignment, KeyedChild)>) -> widgets::zstack::ZStack {
    widgets::zstack::ZStack::new(Padding::zero(), FlexProps::default(), children)
}

pub fn grid_keyed(
    columns: usize,
    cell_size: katla_math::Vec2,
    children: Vec<KeyedChild>,
) -> widgets::grid::Grid {
    widgets::grid::Grid::new(columns, cell_size, 0.0, FlexProps::default(), children)
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
// Transition container wrapper
// ---------------------------------------------------------------------------

/// Wrap a child widget in a transition container.
pub(crate) fn wrap_transition_container(
    child: Box<dyn Widget>,
    transition: super::transition::Transition,
) -> widgets::transition::TransitionContainer {
    widgets::transition::TransitionContainer::new(transition, Some(child))
}

// ---------------------------------------------------------------------------
// Re-export WidgetBox for .boxed() method
// ---------------------------------------------------------------------------

pub use super::widget::WidgetBox;

// ---------------------------------------------------------------------------
// Memoize — wrapper widget for skipping subtree rebuild
// ---------------------------------------------------------------------------

/// Create a `Memoize<T, W>` widget that skips subtree rebuild when the
/// `Arc<T>` data pointer is unchanged between frames.
///
/// The `factory` closure is called to produce the inner widget when the data
/// changes. When the data is the same (via `Arc::ptr_eq`), the inner subtree
/// is reused without rebuild.
pub fn memoize<T: 'static, W: Widget>(
    data: Arc<T>,
    factory: fn(Arc<T>) -> W,
) -> widgets::memoize::Memoize<T, W> {
    widgets::memoize::Memoize::new(data, factory)
}

#[cfg(test)]
mod tests {
    use super::super::state::{StateArena, ViewId};
    use super::super::widget::WidgetBox;
    use super::*;
    use crate::style::FontSize;

    fn dummy_state_id() -> StateId {
        let mut arena = StateArena::default();
        arena.get_or_create(ViewId::default(), 0usize)
    }

    // -- Leaf constructor tests --

    #[test]
    fn test_text_defaults() {
        let w = text("hello");
        assert_eq!(w.content, "hello");
        assert!(w.color.is_none());
        assert!(w.font_size.is_none());
    }

    #[test]
    fn test_text_modifier_color() {
        let w = text("hi").color(katla_math::Color::RED);
        assert_eq!(w.color, Some(katla_math::Color::RED));
    }

    #[test]
    fn test_text_modifier_font_size() {
        let w = text("hi").font_size(FontSize::Small);
        assert_eq!(w.font_size, Some(FontSize::Small));
    }

    #[test]
    fn test_text_boxed() {
        let w: Box<dyn Widget> = text("hello").boxed();
        assert!(w.as_any().downcast_ref::<widgets::text::Text>().is_some());
    }

    #[test]
    fn test_button_defaults() {
        let w = button("ok");
        assert_eq!(w.label, "ok");
        assert!(w.fill_color.is_none());
        assert!(w.hover_color.is_none());
        assert!(w.border_color.is_none());
        assert!(w.on_click.is_none());
    }

    #[test]
    fn test_button_modifier_fill() {
        let w = button("ok").fill(katla_math::Color::BLUE);
        assert_eq!(w.fill_color, Some(katla_math::Color::BLUE));
    }

    #[test]
    fn test_button_modifier_hover() {
        let w = button("ok").hover(katla_math::Color::RED);
        assert_eq!(w.hover_color, Some(katla_math::Color::RED));
    }

    #[test]
    fn test_button_modifier_border() {
        let w = button("ok").border(katla_math::Color::BLACK);
        assert_eq!(w.border_color, Some(katla_math::Color::BLACK));
    }

    #[test]
    fn test_button_modifier_on_click() {
        let w = button("ok").on_click(Callback(1));
        assert!(w.on_click.is_some());
    }

    #[test]
    fn test_image_button_defaults() {
        let w = image_button('X');
        assert_eq!(w.icon, 'X');
        assert!(w.enabled);
        assert!(w.fill_color.is_none());
        assert!(w.on_click.is_none());
    }

    #[test]
    fn test_image_button_modifier_fill() {
        let w = image_button('X').fill(katla_math::Color::GREEN);
        assert_eq!(w.fill_color, Some(katla_math::Color::GREEN));
    }

    #[test]
    fn test_image_button_modifier_enabled() {
        let w = image_button('X').enabled(false);
        assert!(!w.enabled);
    }

    #[test]
    fn test_image_button_modifier_on_click() {
        let w = image_button('X').on_click(Callback(2));
        assert!(w.on_click.is_some());
    }

    #[test]
    fn test_slider_defaults() {
        let id = dummy_state_id();
        let w = slider("vol", id, 0.0..=1.0);
        assert_eq!(w.label, "vol");
        assert_eq!(w.value_id, id);
        assert!(!w.show_value);
        assert_eq!(w.precision, 2);
    }

    #[test]
    fn test_slider_modifier_show_value() {
        let id = dummy_state_id();
        let w = slider("s", id, 0.0..=1.0).show_value(true);
        assert!(w.show_value);
    }

    #[test]
    fn test_slider_modifier_precision() {
        let id = dummy_state_id();
        let w = slider("s", id, 0.0..=1.0).precision(4);
        assert_eq!(w.precision, 4);
    }

    #[test]
    fn test_labeled_slider_defaults() {
        let id = dummy_state_id();
        let w = labeled_slider("vol", id, 0.0..=1.0);
        assert_eq!(w.label, "vol");
        assert_eq!(w.label_width, 0.0);
        assert!(!w.show_value);
        assert_eq!(w.precision, 2);
    }

    #[test]
    fn test_labeled_slider_modifier_show_value() {
        let id = dummy_state_id();
        let w = labeled_slider("s", id, 0.0..=1.0).show_value(true);
        assert!(w.show_value);
    }

    #[test]
    fn test_labeled_slider_modifier_label_width() {
        let id = dummy_state_id();
        let w = labeled_slider("s", id, 0.0..=1.0).label_width(120.0);
        assert_eq!(w.label_width, 120.0);
    }

    #[test]
    fn test_textfield_defaults() {
        let id = dummy_state_id();
        let w = textfield("type here", id);
        assert_eq!(w.placeholder, "type here");
        assert!(w.on_submit.is_none());
    }

    #[test]
    fn test_textfield_modifier_on_submit() {
        let id = dummy_state_id();
        let w = textfield("ph", id).on_submit(Callback(3));
        assert!(w.on_submit.is_some());
    }

    #[test]
    fn test_progress_defaults() {
        let w = progress(0.5, 0.0..=1.0);
        assert_eq!(w.value, 0.5);
        assert!(w.fill_color.is_none());
        assert!(w.label.is_none());
    }

    #[test]
    fn test_progress_modifier_fill() {
        let w = progress(0.5, 0.0..=1.0).fill(katla_math::Color::WHITE);
        assert_eq!(w.fill_color, Some(katla_math::Color::WHITE));
    }

    #[test]
    fn test_progress_modifier_label() {
        let w = progress(0.5, 0.0..=1.0).progress_label("50%");
        assert_eq!(w.label, Some("50%".to_string()));
    }

    #[test]
    fn test_toggle_constructor() {
        let id = dummy_state_id();
        let w = toggle("on", id);
        assert_eq!(w.label, "on");
    }

    #[test]
    fn test_radio_constructor() {
        let id = dummy_state_id();
        let w = radio(id, 2, "opt");
        assert_eq!(w.index, 2);
        assert_eq!(w.label, "opt");
    }

    #[test]
    fn test_property_row_constructor() {
        let w = property_row("key", "val");
        assert_eq!(w.label, "key");
        assert_eq!(w.value, "val");
    }

    #[test]
    fn test_color_picker_constructor() {
        let id = StateId::test_id();
        let w = color_picker("Pick color", id);
        assert_eq!(w.label, "Pick color");
        assert_eq!(w.value_id, id);
    }

    #[test]
    fn test_vec3_slider_constructor() {
        let ids = [dummy_state_id(), dummy_state_id(), dummy_state_id()];
        let w = vec3_slider("position", ids, -10.0..=10.0);
        assert_eq!(w.label, "position");
        assert_eq!(w.value_ids, ids);
        assert_eq!(w.range, -10.0..=10.0);
    }

    #[test]
    fn test_image_constructor() {
        let w = image(TextureId(42), katla_math::Color::WHITE);
        assert_eq!(w.texture.0, 42);
        assert!(w.uv.is_none());
        assert_eq!(w.tint, katla_math::Color::WHITE);
    }

    #[test]
    fn test_image_modifier_uv() {
        let w = image(TextureId(1), katla_math::Color::WHITE)
            .uv(katla_math::Rect2D::new(Vec2::ZERO, Vec2::new(1.0, 1.0)));
        assert!(w.uv.is_some());
    }

    #[test]
    fn test_image_modifier_image_size() {
        let w = image(TextureId(1), katla_math::Color::WHITE).image_size(32.0, 32.0);
        assert_eq!(w.width, Some(32.0));
        assert_eq!(w.height, Some(32.0));
    }

    // -- Container constructor tests --

    #[test]
    fn test_hstack_defaults() {
        let w = hstack([text("a").boxed(), text("b").boxed()]);
        assert_eq!(w.child_widgets.len(), 2);
        assert_eq!(w.spacing, 0.0);
        assert_eq!(w.padding, Padding::zero());
        assert_eq!(w.alignment, Alignment::Leading);
    }

    #[test]
    fn test_vstack_defaults() {
        let w = vstack([text("a").boxed()]);
        assert_eq!(w.child_widgets.len(), 1);
    }

    #[test]
    fn test_zstack_defaults() {
        let w = zstack([(Alignment::Center, text("c").boxed())]);
        assert_eq!(w.child_widgets.len(), 1);
        assert_eq!(w.padding, Padding::zero());
    }

    #[test]
    fn test_panel_defaults() {
        let w = panel("title", text("body").boxed());
        assert_eq!(w.title, "title");
        assert_eq!(w.header_height, 24.0);
    }

    #[test]
    fn test_scroll_constructor() {
        let id = dummy_state_id();
        let w = scroll(text("c").boxed(), id);
        assert_eq!(w.scroll_state_id, id);
    }

    #[test]
    fn test_overlay_constructor() {
        let w = overlay(Anchor::TopLeft, Vec2::ZERO, text("o").boxed());
        assert_eq!(w.anchor, Anchor::TopLeft);
    }

    #[test]
    fn test_statusbar_constructor() {
        let w = statusbar(24.0, text("s").boxed());
        assert_eq!(w.height, 24.0);
    }

    #[test]
    fn test_draggable_panel_defaults() {
        let id = dummy_state_id();
        let w = draggable_panel("p", 200.0, 300.0, text("c").boxed(), id);
        assert_eq!(w.title, "p");
        assert_eq!(w.width, 200.0);
        assert!(!w.close_on_outside_click);
    }

    #[test]
    fn test_menubar_defaults() {
        let w = menubar(vec![]);
        assert!(w.right_content.is_none());
        assert_eq!(w.height, 28.0);
    }

    #[test]
    fn test_tree_view_defaults() {
        let (e, s, sc) = (dummy_state_id(), dummy_state_id(), dummy_state_id());
        let w = tree_view(vec![], e, s, sc);
        assert_eq!(w.row_height, 20.0);
        assert_eq!(w.indent_per_level, 16.0);
        assert!(w.on_select.is_none());
        assert!(w.on_right_click.is_none());
    }

    #[test]
    fn test_modal_constructor() {
        let id = dummy_state_id();
        let w = modal(400.0, 300.0, id, text("m").boxed());
        assert_eq!(w.width, 400.0);
        assert_eq!(w.height, 300.0);
    }

    #[test]
    fn test_context_menu_constructor() {
        let id = dummy_state_id();
        let w = context_menu(vec![], id);
        assert_eq!(w.open_id, id);
    }

    // -- Separator tests --

    #[test]
    fn test_separator_defaults() {
        let w = separator(SeparatorDirection::Horizontal);
        assert_eq!(w.direction, SeparatorDirection::Horizontal);
        assert!(w.color.is_none());
    }

    #[test]
    fn test_separator_horizontal_shortcut() {
        let w = separator_horizontal();
        assert_eq!(w.direction, SeparatorDirection::Horizontal);
    }

    #[test]
    fn test_separator_vertical_shortcut() {
        let w = separator_vertical();
        assert_eq!(w.direction, SeparatorDirection::Vertical);
    }

    #[test]
    fn test_separator_modifier_color() {
        let w = separator_horizontal().separator_color(katla_math::Color::RED);
        assert_eq!(w.color, Some(katla_math::Color::RED));
    }

    // -- Icon tests --

    #[test]
    fn test_icon_defaults() {
        let w = icon('X');
        assert_eq!(w.icon, 'X');
        assert!(w.size.is_none());
        assert!(w.color.is_none());
    }

    #[test]
    fn test_icon_modifier_size() {
        let w = icon('A').icon_size(FontSize::Large);
        assert_eq!(w.size, Some(FontSize::Large));
    }

    #[test]
    fn test_icon_modifier_color() {
        let w = icon('B').color(katla_math::Color::GREEN);
        assert_eq!(w.color, Some(katla_math::Color::GREEN));
    }

    // -- Selectable tests --

    #[test]
    fn test_selectable_defaults() {
        let w = selectable(text("item").boxed());
        assert!(w.on_click.is_none());
        assert!(!w.selected);
        assert!(w.child_widget.is_some());
    }

    #[test]
    fn test_selectable_modifier_selected() {
        let w = selectable(text("x").boxed()).selected(true);
        assert!(w.selected);
    }

    #[test]
    fn test_selectable_modifier_on_click() {
        let w = selectable(text("x").boxed()).on_click(Callback(42));
        assert!(w.on_click.is_some());
    }

    // -- Section tests --

    #[test]
    fn test_section_defaults() {
        let id = dummy_state_id();
        let w = section("My Section", text("content").boxed(), id);
        assert_eq!(w.title, "My Section");
        assert_eq!(w.expanded_id, id);
        assert!(w.on_remove.is_none());
        assert!(w.child_widget.is_some());
    }

    #[test]
    fn test_section_modifier_on_remove() {
        let id = dummy_state_id();
        let w = section("s", text("c").boxed(), id).on_remove(Callback(99));
        assert!(w.on_remove.is_some());
    }

    // -- TabBar tests --

    #[test]
    fn test_tab_bar_defaults() {
        let id = dummy_state_id();
        let w = tab_bar(
            vec![tab_item("A"), tab_item("B")],
            id,
            text("content").boxed(),
        );
        assert_eq!(w.tabs.len(), 2);
        assert_eq!(w.tabs[0].label, "A");
    }

    // -- Grid tests --

    #[test]
    fn test_grid_defaults() {
        let w = grid(
            3,
            Vec2::new(100.0, 50.0),
            [text("a").boxed(), text("b").boxed(), text("c").boxed()],
        );
        assert_eq!(w.columns, 3);
        assert_eq!(w.cell_size, Vec2::new(100.0, 50.0));
        assert_eq!(w.spacing, 0.0);
    }

    #[test]
    fn test_grid_modifier_spacing() {
        let w = grid(2, Vec2::new(50.0, 50.0), []).grid_spacing(8.0);
        assert_eq!(w.spacing, 8.0);
    }

    // -- Container modifier tests --

    #[test]
    fn test_hstack_modifier_spacing() {
        let w = hstack([text("a").boxed()]).spacing(8.0);
        assert_eq!(w.spacing, 8.0);
    }

    #[test]
    fn test_vstack_modifier_padding() {
        let w = vstack([text("a").boxed()]).padding(Padding::all(10.0));
        assert_eq!(w.padding, Padding::all(10.0));
    }

    #[test]
    fn test_zstack_modifier_padding_all() {
        let w = zstack([]).padding_all(12.0);
        assert_eq!(w.padding, Padding::all(12.0));
    }

    #[test]
    fn test_hstack_modifier_align() {
        let w = hstack([]).align(Alignment::Center);
        assert_eq!(w.alignment, Alignment::Center);
    }

    #[test]
    fn test_panel_modifier_header_height() {
        let w = panel("t", text("c").boxed()).header_height(32.0);
        assert_eq!(w.header_height, 32.0);
    }

    #[test]
    fn test_draggable_panel_modifier_close_on_outside() {
        let id = dummy_state_id();
        let w = draggable_panel("p", 200.0, 300.0, text("c").boxed(), id).close_on_outside(true);
        assert!(w.close_on_outside_click);
    }

    #[test]
    fn test_menubar_modifier_right_content() {
        let w = menubar(vec![]).right_content(text("r").boxed());
        assert!(w.right_content.is_some());
    }

    #[test]
    fn test_menubar_modifier_height() {
        let w = menubar(vec![]).menubar_height(40.0);
        assert_eq!(w.height, 40.0);
    }

    #[test]
    fn test_tree_view_modifier_row_height() {
        let (e, s, sc) = (dummy_state_id(), dummy_state_id(), dummy_state_id());
        let w = tree_view(vec![], e, s, sc).row_height(30.0);
        assert_eq!(w.row_height, 30.0);
    }

    #[test]
    fn test_tree_view_modifier_indent() {
        let (e, s, sc) = (dummy_state_id(), dummy_state_id(), dummy_state_id());
        let w = tree_view(vec![], e, s, sc).indent(24.0);
        assert_eq!(w.indent_per_level, 24.0);
    }

    #[test]
    fn test_tree_view_modifier_on_select() {
        let (e, s, sc) = (dummy_state_id(), dummy_state_id(), dummy_state_id());
        let w = tree_view(vec![], e, s, sc).on_select(Callback(10));
        assert!(w.on_select.is_some());
    }

    #[test]
    fn test_tree_view_modifier_on_right_click() {
        let (e, s, sc) = (dummy_state_id(), dummy_state_id(), dummy_state_id());
        let w = tree_view(vec![], e, s, sc).on_right_click(Callback(11));
        assert!(w.on_right_click.is_some());
    }

    // -- Chained modifier test --

    #[test]
    fn test_chained_modifiers() {
        let w = hstack([text("a").boxed(), text("b").boxed()])
            .spacing(8.0)
            .padding_all(4.0)
            .align(Alignment::Center);

        assert_eq!(w.spacing, 8.0);
        assert_eq!(w.padding, Padding::all(4.0));
        assert_eq!(w.alignment, Alignment::Center);
        assert_eq!(w.child_widgets.len(), 2);
    }

    // -- Keyed constructor tests --

    #[test]
    fn test_keyed_helper() {
        let kc = keyed(42, text("k").boxed());
        assert_eq!(kc.key, Some(42));
    }

    #[test]
    fn test_hstack_keyed_constructor() {
        let children = vec![keyed(1, text("a").boxed()), keyed(2, text("b").boxed())];
        let w = hstack_keyed(children);
        assert_eq!(w.child_widgets.len(), 2);
        assert_eq!(w.child_widgets[0].key, Some(1));
        assert_eq!(w.child_widgets[1].key, Some(2));
    }

    #[test]
    fn test_vstack_keyed_constructor() {
        let children = vec![keyed(10, text("x").boxed())];
        let w = vstack_keyed(children);
        assert_eq!(w.child_widgets.len(), 1);
        assert_eq!(w.child_widgets[0].key, Some(10));
    }

    #[test]
    fn test_zstack_keyed_constructor() {
        let children = vec![(Alignment::Center, keyed(5, text("z").boxed()))];
        let w = zstack_keyed(children);
        assert_eq!(w.child_widgets.len(), 1);
    }

    #[test]
    fn test_grid_keyed_constructor() {
        let children = vec![keyed(1, text("a").boxed()), keyed(2, text("b").boxed())];
        let w = grid_keyed(2, Vec2::new(50.0, 50.0), children);
        assert_eq!(w.columns, 2);
    }

    // -- Menu helper tests --

    #[test]
    fn test_menu_group_constructor() {
        let id = dummy_state_id();
        let mg = menu_group("File", id, vec![menu_entry("Open"), menu_entry("Save")]);
        assert_eq!(mg.label, "File");
        assert_eq!(mg.open_id, id);
        assert_eq!(mg.items.len(), 2);
    }

    #[test]
    fn test_menu_entry_disabled() {
        let me = menu_entry_disabled("Greyed");
        assert!(me.disabled);
    }

    #[test]
    fn test_menu_entry_on_click() {
        let me = menu_entry("Click").on_click(Callback(7));
        assert!(me.on_click.is_some());
    }

    // -- ContextMenuEntry helper tests --

    #[test]
    fn test_context_entry_constructor() {
        let ce = context_entry("Copy");
        assert_eq!(ce.label, "Copy");
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

    // -- Type safety: misapplied modifiers are compile errors --
    // These tests verify that the type system prevents invalid modifier chains.
    // If any of these lines compiled, it would mean the type system failed.

    #[test]
    fn test_type_safe_modifiers() {
        // These should compile:
        let _ = text("hi")
            .color(katla_math::Color::RED)
            .font_size(FontSize::Small);
        let _ = button("ok")
            .fill(katla_math::Color::BLUE)
            .hover(katla_math::Color::RED)
            .border(katla_math::Color::BLACK)
            .on_click(Callback(1));
        let _ = image_button('X')
            .fill(katla_math::Color::GREEN)
            .on_click(Callback(2))
            .enabled(false);
        let _ = separator_horizontal().separator_color(katla_math::Color::RED);
        let _ = icon('A')
            .icon_size(FontSize::Large)
            .color(katla_math::Color::GREEN);
        let _ = hstack([])
            .spacing(8.0)
            .padding_all(4.0)
            .align(Alignment::Center);
        let _ = panel("t", empty().boxed()).header_height(32.0);
        let _ = menubar(vec![]).menubar_height(40.0);
        let _ = grid(2, Vec2::new(50.0, 50.0), []).grid_spacing(8.0);
    }

    // The following would be compile errors (uncomment to verify):
    // text("hi").spacing(4.0);       // error: no method `spacing` on Text
    // text("hi").fill(Color::RED);    // error: no method `fill` on Text
    // button("ok").font_size(Small);  // error: no method `font_size` on Button
    // hstack([]).on_click(cb);        // error: no method `on_click` on HStack
}
