//! Katla UI - Declarative UI system for the Katla engine.
//!
//! This crate provides a retained-mode declarative UI system layered on top of
//! an immediate-mode rendering core, suitable for:
//! - In-game HUDs and menus
//! - Debug overlays and development tools
//! - Settings panels
//! - Editor interfaces
//!
//! # Architecture
//!
//! The primary API is the **declarative system** ([`declarative`] module):
//!
//! 1. Implement [`Build`] to produce a [`ViewDescriptor`] tree each frame
//! 2. Drive rendering with [`ViewTree::frame()`] which handles build, diff, layout,
//!    input, and drawing in one call
//! 3. Drain typed actions from [`ViewTree::actions_mut()`]
//!
//! ## Available ViewDescriptor widgets
//!
//! **Leaf widgets:**
//! - [`Text`](declarative::ViewDescriptor::Text) — labeled text display
//! - [`Button`](declarative::ViewDescriptor::Button) — clickable button with callback
//! - [`Slider`](declarative::ViewDescriptor::Slider) — basic numeric slider
//! - [`LabeledSlider`](declarative::ViewDescriptor::LabeledSlider) — slider with label prefix and value display
//! - [`Vec3Slider`](declarative::ViewDescriptor::Vec3Slider) — three-axis slider with colored labels
//! - [`Toggle`](declarative::ViewDescriptor::Toggle) — on/off toggle switch
//! - [`TextField`](declarative::ViewDescriptor::TextField) — text input with placeholder
//! - [`Progress`](declarative::ViewDescriptor::Progress) — progress bar
//! - [`ColorPicker`](declarative::ViewDescriptor::ColorPicker) — color swatch picker
//! - [`ImageButton`](declarative::ViewDescriptor::ImageButton) — icon-only clickable button
//! - [`RadioButton`](declarative::ViewDescriptor::RadioButton) — single-selection radio group
//! - [`Image`](declarative::ViewDescriptor::Image) — textured image display
//! - [`PropertyRow`](declarative::ViewDescriptor::PropertyRow) — read-only label:value row
//!
//! **Layout containers:**
//! - [`HStack`](declarative::ViewDescriptor::HStack) — horizontal flex layout
//! - [`VStack`](declarative::ViewDescriptor::VStack) — vertical flex layout
//! - [`ZStack`](declarative::ViewDescriptor::ZStack) — layered (z-order) layout
//! - [`ScrollView`](declarative::ViewDescriptor::ScrollView) — scrollable content area
//! - [`Panel`](declarative::ViewDescriptor::Panel) — titled panel with header
//! - [`Overlay`](declarative::ViewDescriptor::Overlay) — anchored overlay positioning
//!
//! ## State management
//!
//! Use [`BuildContext::state()`] to create persistent state scoped to each view node.
//! State survives across frames and is automatically cleaned up when nodes are removed.
//!
//! ## Actions
//!
//! Use [`BuildContext::emit()`] to send typed actions from any widget.
//! Drain them after the frame via [`ViewTree::actions_mut()`].
//!
//! # Lower-level API
//!
//! The [`widgets`] module provides immediate-mode builder widgets used internally
//! by the declarative draw pipeline. Use these only when a declarative equivalent
//! does not yet exist.
//!
//! # Example
//!
//! ```ignore
//! use katla_ui::declarative::{
//!     Build, BuildContext, ViewDescriptor, ViewTree,
//!     StackDescriptor, Padding, Alignment,
//! };
//!
//! struct MyHud;
//!
//! impl Build for MyHud {
//!     fn build(&self, ctx: &mut BuildContext) -> ViewDescriptor {
//!         let health = ctx.state(100.0f32);
//!         ViewDescriptor::HStack(Box::new(StackDescriptor {
//!             children: vec![
//!                 ViewDescriptor::Text {
//!                     content: "Health".into(),
//!                     color: None,
//!                     font_size: None,
//!                 },
//!                 ViewDescriptor::Slider {
//!                     label: String::new(),
//!                     value_id: health,
//!                     range: 0.0..=100.0,
//!                     show_value: true,
//!                     precision: 0,
//!                 },
//!             ],
//!             spacing: 8.0,
//!             padding: Padding::all(10.0),
//!             alignment: Alignment::Leading,
//!         }))
//!     }
//! }
//!
//! let mut view_tree = ViewTree::new();
//! let input_consumed = view_tree.frame(&mut ui, &MyHud, screen_size);
//! for action in view_tree.actions_mut().drain::<MyAction>() {
//!     // handle actions
//! }
//! ```

mod context;
pub mod declarative;
mod draw_list;
mod icons;
pub mod input;
pub mod markdown;
pub mod response;
mod style;
mod text;
mod types;
mod widget;
pub mod widgets;

pub use context::{
    CloseBehavior, Popup, PopupPosition, PopupStyle, ScrollArea, ScrollAreaState, UiContext,
    z_index,
};
pub use draw_list::DrawList;
pub use icons::ForkAwesome;
pub use input::{KeyCode, MouseCursor, UiInputState, mouse_button};
pub use response::Response;
pub use style::{ColorScheme, DEFAULTS, FontSize, UiStyle};
pub use text::FontId;
pub use types::{DrawCmd, TextureId, Vertex};
pub use widget::{ClipboardProvider, Widget};
