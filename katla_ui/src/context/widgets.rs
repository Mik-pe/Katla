//! Widget behavior helpers.
//!
//! This module provides:
//! - **ScrollAreaState**: State tracking for scrollable regions
//! - **Hover layer tracking**: Z-index based hover registration

mod scroll_area;
mod utility;

pub use scroll_area::ScrollAreaState;

use katla_math::Rect2D;

use super::UiContext;

impl UiContext {
    /// Register that the mouse is hovering over content at the given z-index.
    ///
    /// Called automatically by `draw_rect` when the current z-index is above
    /// DEFAULT. The highest z-index wins — if multiple regions overlap at
    /// the mouse position, only the highest z-index is remembered.
    pub(crate) fn register_hover_layer(&mut self, z: u32, bounds: Rect2D) {
        if z > self.hover_z_index && bounds.contains(self.input.mouse_pos) {
            self.hover_z_index = z;
        }
    }
}
