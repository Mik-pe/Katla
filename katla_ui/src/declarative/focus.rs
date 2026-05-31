use std::collections::{HashMap, HashSet};

use katla_math::Rect2D;

use super::state::{StateArena, ViewId};
use super::tree::ViewTree;

/// A focusable widget entry with its associated focus scope.
#[derive(Clone, Copy, Debug)]
struct FocusEntry {
    view_id: ViewId,
    scope_id: Option<ViewId>,
}

/// Manages keyboard focus within the declarative view tree.
///
/// Tracks which node has focus and provides Tab/Shift+Tab navigation
/// through the focus chain. Supports focus scopes for isolating focus
/// to panels, modals, and floating windows.
pub struct FocusManager {
    focused: Option<ViewId>,
    entries: Vec<FocusEntry>,
    trapped_scopes: HashSet<ViewId>,
}

impl FocusManager {
    pub fn new() -> Self {
        Self {
            focused: None,
            entries: Vec::new(),
            trapped_scopes: HashSet::new(),
        }
    }

    /// Set the focus chain from pre-collected entries with scope information.
    ///
    /// Preserves focus if the focused ID is still in the new chain.
    /// Clears focus if the focused widget was removed.
    pub fn set_focus_chain(
        &mut self,
        chain: Vec<(ViewId, Option<ViewId>)>,
        traps: HashSet<ViewId>,
    ) {
        if let Some(focused) = self.focused
            && !chain.iter().any(|(id, _)| *id == focused)
        {
            self.focused = None;
        }
        self.entries = chain
            .into_iter()
            .map(|(view_id, scope_id)| FocusEntry { view_id, scope_id })
            .collect();
        self.trapped_scopes = traps;
    }

    /// Move focus to the next focusable node (Tab order).
    ///
    /// If the current focus is inside a focus scope, navigation wraps
    /// within that scope only. Trapped scopes (open modals) prevent
    /// focus from ever leaving.
    pub fn focus_next(&mut self) {
        if self.entries.is_empty() {
            return;
        }

        let current_scope = self.scope_of_focused();
        let is_trapped = current_scope.is_some_and(|s| self.trapped_scopes.contains(&s));

        let indices: Vec<usize> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| self.entry_in_scope(e, current_scope, is_trapped))
            .map(|(i, _)| i)
            .collect();

        if indices.is_empty() {
            return;
        }

        let current_idx = self
            .focused
            .and_then(|f| indices.iter().position(|&i| self.entries[i].view_id == f));

        let next_idx = match current_idx {
            Some(idx) if idx + 1 < indices.len() => indices[idx + 1],
            Some(_) => indices[0],
            None => indices[0],
        };

        self.focused = Some(self.entries[next_idx].view_id);
    }

    /// Move focus to the previous focusable node (Shift+Tab order).
    ///
    /// Same scope isolation rules as `focus_next()`.
    pub fn focus_prev(&mut self) {
        if self.entries.is_empty() {
            return;
        }

        let current_scope = self.scope_of_focused();
        let is_trapped = current_scope.is_some_and(|s| self.trapped_scopes.contains(&s));

        let indices: Vec<usize> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| self.entry_in_scope(e, current_scope, is_trapped))
            .map(|(i, _)| i)
            .collect();

        if indices.is_empty() {
            return;
        }

        let current_idx = self
            .focused
            .and_then(|f| indices.iter().position(|&i| self.entries[i].view_id == f));

        let prev_idx = match current_idx {
            Some(0) => *indices.last().unwrap(),
            Some(idx) => indices[idx - 1],
            None => *indices.last().unwrap(),
        };

        self.focused = Some(self.entries[prev_idx].view_id);
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

    /// Get the flat list of focusable view IDs (for gamepad navigation).
    pub fn focus_chain_ids(&self) -> Vec<ViewId> {
        self.entries.iter().map(|e| e.view_id).collect()
    }

    fn find_entry(&self, id: ViewId) -> Option<&FocusEntry> {
        self.entries.iter().find(|e| e.view_id == id)
    }

    fn scope_of_focused(&self) -> Option<ViewId> {
        self.focused
            .and_then(|f| self.find_entry(f))
            .and_then(|e| e.scope_id)
    }

    fn entry_in_scope(
        &self,
        entry: &FocusEntry,
        current_scope: Option<ViewId>,
        is_trapped: bool,
    ) -> bool {
        if is_trapped {
            entry.scope_id == Some(current_scope.unwrap())
        } else if let Some(scope) = current_scope {
            entry.scope_id == Some(scope)
        } else {
            !entry
                .scope_id
                .is_some_and(|s| self.trapped_scopes.contains(&s))
        }
    }
}

impl Default for FocusManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Collect all focusable view IDs with scope information from the tree.
///
/// Walks the tree depth-first, tracking which focus scope (if any) each
/// focusable widget belongs to. Also identifies trapped scopes (open modals).
pub(crate) fn collect_focus_chain(
    tree: &ViewTree,
    state: &StateArena,
) -> (Vec<(ViewId, Option<ViewId>)>, HashSet<ViewId>) {
    let mut chain = Vec::new();
    let mut traps = HashSet::new();
    if let Some(root) = tree.root() {
        collect_recursive(tree, state, root, &mut None, &mut chain, &mut traps);
    }
    (chain, traps)
}

fn collect_recursive(
    tree: &ViewTree,
    state: &StateArena,
    node_id: ViewId,
    current_scope: &mut Option<ViewId>,
    chain: &mut Vec<(ViewId, Option<ViewId>)>,
    traps: &mut HashSet<ViewId>,
) {
    let Some(node) = tree.get(node_id) else {
        return;
    };

    let is_scope = node.widget.is_focus_scope();
    let mut entered_scope = false;

    if is_scope {
        current_scope.replace(node_id);
        entered_scope = true;

        if node.widget.focus_scope_trap(state) {
            traps.insert(node_id);
        }
    }

    if node.widget.focusable() {
        chain.push((node_id, *current_scope));
    }

    for &child_id in &node.children {
        collect_recursive(tree, state, child_id, current_scope, chain, traps);
    }

    if entered_scope {
        current_scope.take();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::declarative::constructors::*;
    use crate::declarative::tree::ViewTree;
    use crate::declarative::widget::WidgetBox;
    use katla_math::Vec2;

    fn make_view_id(ffi: u64) -> ViewId {
        ViewId::from(slotmap::KeyData::from_ffi(ffi))
    }

    #[test]
    fn test_focus_chain_collects_focusable_widgets() {
        let mut tree = ViewTree::new();
        let mut callbacks = crate::declarative::build::CallbackTable::new();
        let cb1 = callbacks.push(|_| {});
        let cb2 = callbacks.push(|_| {});

        tree.set_root(
            vstack([
                button("A").on_click(cb1).boxed(),
                text("label").boxed(),
                button("B").on_click(cb2).boxed(),
            ])
            .boxed(),
        );

        let (chain, _) = {
            let state = tree.state_arena();
            collect_focus_chain(&tree, state)
        };
        assert_eq!(chain.len(), 2, "only buttons should be in the chain");
    }

    #[test]
    fn test_tab_moves_forward_and_wraps() {
        let mut fm = FocusManager::new();
        let id0 = make_view_id(0);
        let id1 = make_view_id(1);
        let id2 = make_view_id(2);
        let id3 = make_view_id(3);
        let id4 = make_view_id(4);

        let chain: Vec<(ViewId, Option<ViewId>)> = vec![
            (id0, None),
            (id1, None),
            (id2, None),
            (id3, None),
            (id4, None),
        ];
        fm.set_focus_chain(chain, HashSet::new());

        fm.set_focused(Some(id2));
        fm.focus_next();
        assert_eq!(fm.focused(), Some(id3));

        fm.focus_next();
        assert_eq!(fm.focused(), Some(id4));

        fm.focus_next();
        assert_eq!(fm.focused(), Some(id0), "should wrap to first");
    }

    #[test]
    fn test_shift_tab_moves_backward_and_wraps() {
        let mut fm = FocusManager::new();
        let id0 = make_view_id(0);
        let id1 = make_view_id(1);
        let id4 = make_view_id(4);

        let chain: Vec<(ViewId, Option<ViewId>)> = vec![
            (id0, None),
            (id1, None),
            (make_view_id(2), None),
            (make_view_id(3), None),
            (id4, None),
        ];
        fm.set_focus_chain(chain, HashSet::new());

        fm.set_focused(Some(id0));
        fm.focus_prev();
        assert_eq!(fm.focused(), Some(id4), "should wrap to last");
    }

    #[test]
    fn test_focus_cleared_when_widget_removed() {
        let mut fm = FocusManager::new();
        let id0 = make_view_id(0);
        let id1 = make_view_id(1);
        let id2 = make_view_id(2);

        let chain: Vec<(ViewId, Option<ViewId>)> = vec![(id0, None), (id1, None), (id2, None)];
        fm.set_focus_chain(chain, HashSet::new());
        fm.set_focused(Some(id1));
        assert_eq!(fm.focused(), Some(id1));

        let new_chain: Vec<(ViewId, Option<ViewId>)> = vec![(id0, None), (id2, None)];
        fm.set_focus_chain(new_chain, HashSet::new());
        assert_eq!(
            fm.focused(),
            None,
            "focus should be cleared when widget removed"
        );
    }

    #[test]
    fn test_focus_scope_limits_tab_navigation() {
        let mut fm = FocusManager::new();
        let scope = make_view_id(10);
        let id0 = make_view_id(0);
        let id1 = make_view_id(1);
        let id2 = make_view_id(2);
        let id3 = make_view_id(3);

        // id0 and id1 in scope, id2 and id3 outside
        let chain: Vec<(ViewId, Option<ViewId>)> = vec![
            (id0, Some(scope)),
            (id1, Some(scope)),
            (id2, None),
            (id3, None),
        ];
        fm.set_focus_chain(chain, HashSet::new());

        fm.set_focused(Some(id0));
        fm.focus_next();
        assert_eq!(fm.focused(), Some(id1), "Tab should move within scope");

        fm.focus_next();
        assert_eq!(fm.focused(), Some(id0), "Tab should wrap within scope");
    }

    #[test]
    fn test_focus_scope_modal_trap() {
        let mut fm = FocusManager::new();
        let modal_scope = make_view_id(10);
        let btn0 = make_view_id(0);
        let btn1 = make_view_id(1);
        let bg_btn = make_view_id(2);

        let chain: Vec<(ViewId, Option<ViewId>)> = vec![
            (btn0, Some(modal_scope)),
            (btn1, Some(modal_scope)),
            (bg_btn, None),
        ];
        let traps: HashSet<ViewId> = [modal_scope].into_iter().collect();
        fm.set_focus_chain(chain, traps);

        fm.set_focused(Some(btn0));
        fm.focus_next();
        assert_eq!(fm.focused(), Some(btn1));

        fm.focus_next();
        assert_eq!(
            fm.focused(),
            Some(btn0),
            "Tab should wrap within trapped modal scope"
        );

        fm.focus_prev();
        assert_eq!(
            fm.focused(),
            Some(btn1),
            "Shift+Tab should wrap within trapped modal scope"
        );
    }

    #[test]
    fn test_gamepad_directional_navigation() {
        let mut nav = GamepadNavigator::new();
        let tl = make_view_id(0);
        let tr = make_view_id(1);
        let bl = make_view_id(2);
        let br = make_view_id(3);

        let chain = vec![tl, tr, bl, br];
        let mut bounds = HashMap::new();
        bounds.insert(
            tl,
            Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(100.0, 100.0)),
        );
        bounds.insert(
            tr,
            Rect2D::new(Vec2::new(100.0, 0.0), Vec2::new(200.0, 100.0)),
        );
        bounds.insert(
            bl,
            Rect2D::new(Vec2::new(0.0, 100.0), Vec2::new(100.0, 200.0)),
        );
        bounds.insert(
            br,
            Rect2D::new(Vec2::new(100.0, 100.0), Vec2::new(200.0, 200.0)),
        );

        nav.set_focused(Some(tl));
        let result = nav.navigate(Direction::Right, &chain, &bounds);
        assert_eq!(
            result,
            Some(tr),
            "Right from top-left should go to top-right"
        );

        let result = nav.navigate(Direction::Down, &chain, &bounds);
        assert_eq!(
            result,
            Some(br),
            "Down from top-right should go to bottom-right"
        );
    }

    #[test]
    fn test_gamepad_no_focus_selects_first() {
        let mut nav = GamepadNavigator::new();
        let id0 = make_view_id(0);
        let id1 = make_view_id(1);
        let chain = vec![id0, id1];
        let mut bounds = HashMap::new();
        bounds.insert(
            id0,
            Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(100.0, 100.0)),
        );
        bounds.insert(
            id1,
            Rect2D::new(Vec2::new(0.0, 100.0), Vec2::new(100.0, 200.0)),
        );

        let result = nav.navigate(Direction::Down, &chain, &bounds);
        assert_eq!(
            result,
            Some(id0),
            "no focus + directional should select first"
        );
    }

    #[test]
    fn test_gamepad_no_candidate_keeps_current() {
        let mut nav = GamepadNavigator::new();
        let rightmost = make_view_id(0);
        let chain = vec![rightmost];
        let mut bounds = HashMap::new();
        bounds.insert(
            rightmost,
            Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(100.0, 100.0)),
        );

        nav.set_focused(Some(rightmost));
        let result = nav.navigate(Direction::Right, &chain, &bounds);
        assert_eq!(
            result,
            Some(rightmost),
            "no candidate right should keep current"
        );
    }
}
