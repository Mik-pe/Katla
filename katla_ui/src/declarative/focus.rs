use std::collections::HashMap;

use katla_math::Rect2D;

use super::state::ViewId;
use super::tree::ViewTree;

/// Manages keyboard focus within the declarative view tree.
///
/// Tracks which node has focus and provides Tab/Shift+Tab navigation
/// through the focus chain.
pub struct FocusManager {
    focused: Option<ViewId>,
    focus_chain: Vec<ViewId>,
}

impl FocusManager {
    pub fn new() -> Self {
        Self {
            focused: None,
            focus_chain: Vec::new(),
        }
    }

    /// Rebuild the focus chain by walking the tree and collecting
    /// all focusable nodes (those with callbacks or focusable widgets).
    pub fn rebuild_focus_chain(&mut self, tree: &ViewTree) {
        self.focus_chain.clear();
        if let Some(root) = tree.root() {
            self.collect_focusable(tree, root);
        }
    }

    /// Rebuild focus chain using a pre-collected list of focusable IDs.
    /// This avoids borrow-check issues when the tree is also mutably borrowed.
    pub fn set_focus_chain(&mut self, chain: Vec<ViewId>) {
        // Preserve focus if the focused ID is still in the new chain
        if let Some(focused) = self.focused
            && !chain.contains(&focused)
        {
            self.focused = None;
        }
        self.focus_chain = chain;
    }

    /// Move focus to the next focusable node (Tab order).
    pub fn focus_next(&mut self) {
        if self.focus_chain.is_empty() {
            return;
        }

        let current_index = self
            .focused
            .and_then(|f| self.focus_chain.iter().position(|&id| id == f))
            .unwrap_or(self.focus_chain.len());

        let next_index = if current_index >= self.focus_chain.len() - 1 {
            0
        } else {
            current_index + 1
        };

        self.focused = Some(self.focus_chain[next_index]);
    }

    /// Move focus to the previous focusable node (Shift+Tab order).
    pub fn focus_prev(&mut self) {
        if self.focus_chain.is_empty() {
            return;
        }

        let current_index = self
            .focused
            .and_then(|f| self.focus_chain.iter().position(|&id| id == f))
            .unwrap_or(self.focus_chain.len());

        let prev_index = if current_index == 0 || current_index >= self.focus_chain.len() {
            self.focus_chain.len() - 1
        } else {
            current_index - 1
        };

        self.focused = Some(self.focus_chain[prev_index]);
    }

    /// Check if a specific node is currently focused.
    pub fn is_focused(&self, id: ViewId) -> bool {
        self.focused == Some(id)
    }

    /// Get the currently focused node, if any.
    pub fn focused(&self) -> Option<ViewId> {
        self.focused
    }

    /// Set focus to a specific node.
    pub fn set_focused(&mut self, id: Option<ViewId>) {
        self.focused = id;
    }

    fn collect_focusable(&mut self, tree: &ViewTree, node_id: ViewId) {
        let Some(node) = tree.get(node_id) else {
            return;
        };

        if node.widget.focusable() {
            self.focus_chain.push(node_id);
        }

        for &child_id in &node.children {
            self.collect_focusable(tree, child_id);
        }
    }
}

impl Default for FocusManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Collect all focusable view IDs from the tree (read-only).
pub(crate) fn collect_focus_chain(tree: &ViewTree) -> Vec<ViewId> {
    let mut chain = Vec::new();
    if let Some(root) = tree.root() {
        collect_focusable_recursive(tree, root, &mut chain);
    }
    chain
}

fn collect_focusable_recursive(tree: &ViewTree, node_id: ViewId, chain: &mut Vec<ViewId>) {
    let Some(node) = tree.get(node_id) else {
        return;
    };

    if node.widget.focusable() {
        chain.push(node_id);
    }

    for &child_id in &node.children {
        collect_focusable_recursive(tree, child_id, chain);
    }
}

/// Cardinal direction for gamepad-driven focus navigation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

/// Gamepad-driven directional navigation using 2D spatial comparison on node bounds.
///
/// Actual gamepad input mapping (D-pad / left stick → Direction) happens at the
/// application layer. This struct only handles the spatial focus movement logic.
pub struct GamepadNavigator {
    focused: Option<ViewId>,
}

impl GamepadNavigator {
    pub fn new() -> Self {
        Self { focused: None }
    }

    /// Move focus to the nearest focusable node in the given direction.
    ///
    /// Uses 2D spatial comparison on node bounds centers. Nodes whose center
    /// is not in the correct direction relative to the current focus are excluded,
    /// then the closest remaining node is selected.
    pub fn navigate(
        &mut self,
        direction: Direction,
        focus_chain: &[ViewId],
        bounds_map: &HashMap<ViewId, Rect2D>,
    ) -> Option<ViewId> {
        if focus_chain.is_empty() {
            return None;
        }

        // If nothing is focused, focus the first node
        let current = match self.focused {
            Some(id) if focus_chain.contains(&id) => id,
            _ => {
                self.focused = Some(focus_chain[0]);
                return self.focused;
            }
        };

        let current_bounds = bounds_map.get(&current)?;
        let current_center = current_bounds.center();

        let mut best: Option<(ViewId, f32)> = None;

        for &candidate_id in focus_chain {
            if candidate_id == current {
                continue;
            }

            let candidate_bounds = match bounds_map.get(&candidate_id) {
                Some(b) => b,
                None => continue,
            };
            let candidate_center = candidate_bounds.center();

            let dx = candidate_center.x() - current_center.x();
            let dy = candidate_center.y() - current_center.y();

            let in_direction = match direction {
                Direction::Up => dy < 0.0,
                Direction::Down => dy > 0.0,
                Direction::Left => dx < 0.0,
                Direction::Right => dx > 0.0,
            };

            if !in_direction {
                continue;
            }

            let dist = (dx * dx + dy * dy).sqrt();

            match best {
                Some((_, best_dist)) if dist < best_dist => {
                    best = Some((candidate_id, dist));
                }
                None => best = Some((candidate_id, dist)),
                _ => {}
            }
        }

        if let Some((id, _)) = best {
            self.focused = Some(id);
        }

        self.focused
    }

    /// Get the currently focused node.
    pub fn focused(&self) -> Option<ViewId> {
        self.focused
    }

    /// Set the focused node externally (e.g. when FocusManager changes focus).
    pub fn set_focused(&mut self, id: Option<ViewId>) {
        self.focused = id;
    }
}

impl Default for GamepadNavigator {
    fn default() -> Self {
        Self::new()
    }
}
