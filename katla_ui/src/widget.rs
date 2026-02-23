//! Widget trait for composable UI elements.
//!
//! The `Widget` trait allows any type to be used as a UI widget.
//! This enables custom widgets and composition patterns.
//!
//! # Example
//!
//! ```ignore
//! use katla_ui::{Widget, UiContext, Response, widgets::Button};
//!
//! struct MyWidget {
//!     label: &'static str,
//! }
//!
//! impl MyWidget {
//!     pub fn new(label: &'static str) -> Self {
//!         Self { label }
//!     }
//! }
//!
//! impl Widget for MyWidget {
//!     fn ui(self, ui: &mut UiContext) -> Response {
//!         ui.add(Button::new(self.label).bounds(Default::default()))
//!     }
//! }
//!
//! // Usage:
//! ui.add(MyWidget::new("Click me!"));
//! ```
//!
//! # Closures as Widgets
//!
//! Closures that take `&mut UiContext` and return `Response`
//! automatically implement `Widget`:
//!
//! ```ignore
//! ui.add(|ui: &mut UiContext| {
//!     // Custom widget logic
//!     Response::default()
//! });
//! ```

use crate::Response;
use crate::UiContext;

/// Trait for types that can be displayed as a UI widget.
///
/// Implement this trait to create custom widgets that can be used
/// with `ui.add()`.
///
/// # Implementation
///
/// The trait is implemented for:
/// - Custom widget types (structs with builder pattern)
/// - Closures `FnOnce(&mut UiContext) -> Response`
///
/// # Example
///
/// ```ignore
/// struct LabeledValue {
///     label: &'static str,
///     value: f32,
/// }
///
/// impl Widget for LabeledValue {
///     fn ui(self, ui: &mut UiContext) -> Response {
///         let bounds = ui.available_rect();
///         ui.draw_text(self.label, bounds.min, Color::WHITE, 14.0);
///         ui.draw_text(&format!("{:.2}", self.value), bounds.min + Vec2::new(100.0, 0.0), Color::GRAY, 14.0);
///         Response::new(bounds)
///     }
/// }
/// ```
pub trait Widget {
    /// Draw this widget into the given UI context.
    ///
    /// Returns a `Response` describing the interaction state.
    fn ui(self, ui: &mut UiContext) -> Response;
}

/// Implement Widget for closures.
impl<F> Widget for F
where
    F: FnOnce(&mut UiContext) -> Response,
{
    fn ui(self, ui: &mut UiContext) -> Response {
        self(ui)
    }
}
