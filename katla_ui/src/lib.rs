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
//! The primary API is the **declarative system** (`declarative` module):
//!
//! 1. Implement [`Build`] to produce a [`ViewDescriptor`] tree each frame
//! 2. Drive rendering with [`ViewTree::frame()`] which handles build, diff, layout,
//!    input, and drawing in one call
//! 3. Drain typed actions from [`ViewTree::actions_mut()`]
//!
//! The [`widgets`] module provides lower-level builder widgets used internally
//! by declarative views and as escape hatches for complex custom rendering.
//!
//! # Example
//!
//! ```ignore
//! use katla_ui::declarative::{
//!     Build, BuildContext, ViewDescriptor, ViewTree,
//!     HStack, StackDescriptor, Padding, Alignment,
//! };
//!
//! // Define a view
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
//!         })
//!     }
//! }
//!
//! // Per-frame rendering
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
    CloseBehavior, Popup, PopupPosition, PopupStyle, ScrollArea, ScrollAreaState, TextInputState,
    UiContext, z_index,
};
pub use draw_list::DrawList;
pub use icons::ForkAwesome;
pub use input::{KeyCode, MouseCursor, UiInputState, mouse_button};
pub use response::Response;
pub use style::{ColorScheme, DEFAULTS, FontSize, UiStyle};
pub use text::FontId;
pub use types::{DrawCmd, TextureId, Vertex};
pub use widget::{ClipboardProvider, Widget};
