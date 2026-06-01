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
//! 1. Implement [`Build`](declarative::Build) to produce a `Box<dyn Widget>` tree each frame
//! 2. Drive rendering with [`ViewTree::frame()`](declarative::ViewTree::frame) which handles build, diff, layout,
//!    input, and drawing in one call
//! 3. Drain typed actions from [`ViewTree::actions_mut()`](declarative::ViewTree::actions_mut)
//!
//! ## Available widgets
//!
//! **Leaf widgets:** Text, Button, Slider, LabeledSlider, Vec3Slider, Toggle,
//! TextField, Progress, ColorPicker, ImageButton, RadioButton, Image,
//! PropertyRow, VuMeter, Separator, Icon, Selectable, Section
//!
//! **Layout containers:** HStack, VStack, ZStack, ScrollView, Panel, Overlay,
//! StatusBar, DraggablePanel, MenuBar, TreeView, Modal, ContextMenu, TabBar, Grid,
//! DockSpace, Memoize, TransitionContainer
//!
//! ## State management
//!
//! Use [`BuildContext::state()`](declarative::BuildContext::state) to create persistent state scoped to each view node.
//! State survives across frames and is automatically cleaned up when nodes are removed.
//!
//! ## Actions
//!
//! Use [`BuildContext::emit()`](declarative::BuildContext::emit) to send typed actions from any widget.
//! Drain them after the frame via [`ViewTree::actions_mut()`](declarative::ViewTree::actions_mut).
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
//!     Build, BuildContext, ViewTree, Widget,
//!     text, slider, hstack, Padding, Alignment,
//! };
//!
//! struct MyHud;
//!
//! impl Build for MyHud {
//!     fn build(&self, ctx: &mut BuildContext) -> Box<dyn Widget> {
//!         let health = ctx.state(100.0f32);
//!         hstack([
//!             text("Health").boxed(),
//!             slider("", health, 0.0..=100.0).show_value().precision(0).boxed(),
//!         ])
//!         .spacing(8.0)
//!         .padding(Padding::all(10.0))
//!         .align(Alignment::Leading)
//!         .boxed()
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
mod style;
mod text;
mod types;
mod widget;
pub mod widgets;

pub mod dock;

// Core types that katla_app needs for integration
pub use context::{ScrollAreaState, UiContext, z_index};
pub use draw_list::DrawList;
pub use icons::ForkAwesome;
pub use input::{KeyCode, MouseCursor, UiInputState, mouse_button};
pub use style::{ColorScheme, DEFAULTS, FontSize, UiStyle};
pub use text::FontId;
pub use types::{DrawCmd, InstanceData, TextureId, Vertex};
pub use widget::ClipboardProvider;
