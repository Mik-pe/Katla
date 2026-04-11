use std::collections::HashSet;

/// A single item in a tree view.
#[derive(Debug, Clone)]
pub struct TreeItem {
    /// Unique identifier for this item.
    pub id: u64,
    /// Display label.
    pub label: String,
    /// Depth in the tree (0 = root).
    pub depth: u32,
    /// Whether this item has children (used to show expand/collapse toggle).
    pub has_children: bool,
}

/// Persistent state for a tree view widget.
#[derive(Debug, Clone, Default)]
pub struct TreeState {
    /// Set of expanded item IDs.
    pub expanded: HashSet<u64>,
    /// Currently selected item ID.
    pub selected: Option<u64>,
    /// Scroll offset for virtualized rendering.
    pub scroll_offset: f32,
}

impl TreeState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn toggle_expanded(&mut self, id: u64) {
        if self.expanded.contains(&id) {
            self.expanded.remove(&id);
        } else {
            self.expanded.insert(id);
        }
    }

    pub fn is_expanded(&self, id: u64) -> bool {
        self.expanded.contains(&id)
    }

    pub fn expand_all(&mut self, items: &[TreeItem]) {
        for item in items {
            if item.has_children {
                self.expanded.insert(item.id);
            }
        }
    }

    pub fn collapse_all(&mut self) {
        self.expanded.clear();
    }
}
