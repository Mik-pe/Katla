use super::{UiContext, WidgetId};

impl UiContext {
    /// Generate a unique ID for a widget.
    ///
    /// Combines parent ID, label, and a sequential counter to ensure uniqueness.
    /// The counter is reset each frame in `begin()`, so consistent call order
    /// produces consistent IDs across frames.
    pub(crate) fn generate_id(&mut self, label: &str) -> WidgetId {
        let base = self.id_stack.last().copied().unwrap_or(0);
        let counter = self.id_counter;
        self.id_counter += 1;

        let mut hash = base;
        for byte in label.bytes() {
            hash = hash.wrapping_mul(31).wrapping_add(byte as u64);
        }
        hash = hash.wrapping_mul(31).wrapping_add(counter as u64);

        hash
    }
}
