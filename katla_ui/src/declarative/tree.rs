use std::collections::HashMap;

use slotmap::SlotMap;
use taffy::NodeId as TaffyNodeId;

use katla_math::{Rect2D, Vec2};

use crate::context::UiContext;

use super::actions::ActionStream;
use super::animation::{AnimatedProperty, Animation, AnimationState, KeyframeAnimation, Tween};
use super::build::{Build as BuildTrait, BuildContext, CallbackTable, Environment};
use super::descriptor::{Alignment, DraggablePanelState};
use super::diff::DiffAction;
use super::focus::{self, Direction, GamepadNavigator};
use super::ime::ImeRequest;
use super::input;
use super::layout::TaffyNodeMap;
use super::state::{StateArena, ViewId};
use super::transition::Transition;
use super::widget::{ChildWidgets, InteractionState, Widget};

pub struct ViewNode {
    pub widget: Box<dyn Widget>,
    pub children: Vec<ViewId>,
    pub parent: Option<ViewId>,
    pub animations: Vec<Animation>,
    pub keyframe_animations: Vec<KeyframeAnimation>,
    pub animation_state: AnimationState,
    pub pending_remove: bool,
    pub bounds: Rect2D,
    pub state_version: u32,
    pub taffy_id: Option<TaffyNodeId>,
    pub key: Option<u64>,
    pub zstack_alignment: Option<Alignment>,
}

pub struct ViewTree {
    nodes: SlotMap<ViewId, ViewNode>,
    state: StateArena,
    root: Option<ViewId>,
    dirty: bool,
    callbacks: CallbackTable,
    actions: ActionStream,
    env: Environment,
    focus: super::focus::FocusManager,
    gamepad: GamepadNavigator,
    taffy: TaffyNodeMap,
    bounds_map: HashMap<ViewId, Rect2D>,
    resolved_bounds: HashMap<ViewId, Rect2D>,
    interaction: InteractionState,
    current_time: f64,
}

impl Default for ViewTree {
    fn default() -> Self {
        Self {
            nodes: SlotMap::with_key(),
            state: StateArena::new(),
            root: None,
            dirty: true,
            callbacks: CallbackTable::new(),
            actions: ActionStream::new(),
            env: Environment::new(),
            focus: super::focus::FocusManager::new(),
            gamepad: GamepadNavigator::new(),
            taffy: TaffyNodeMap::new(),
            bounds_map: HashMap::new(),
            resolved_bounds: HashMap::new(),
            interaction: InteractionState::default(),
            current_time: 0.0,
        }
    }
}

impl ViewTree {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_root(&mut self, widget: Box<dyn Widget>) {
        self.callbacks.clear();

        if let Some(root_id) = self.root {
            // Diff new widget against old, then sync children
            let action = if let Some(node) = self.nodes.get(root_id) {
                widget.diff_against(&*node.widget)
            } else {
                DiffAction::Replace
            };

            match action {
                DiffAction::Update => {
                    if let Some(node) = self.nodes.get_mut(root_id) {
                        node.widget = widget;
                        node.state_version += 1;
                    }
                }
                DiffAction::RecurseChildren | DiffAction::Replace => {
                    let old_children: Vec<ViewId> = self
                        .nodes
                        .get(root_id)
                        .map(|n| n.children.clone())
                        .unwrap_or_default();
                    if action == DiffAction::Replace {
                        for child_id in &old_children {
                            self.remove_node_recursive(*child_id);
                        }
                        if let Some(node) = self.nodes.get_mut(root_id) {
                            node.children.clear();
                            node.state_version += 1;
                        }
                    }
                    if let Some(node) = self.nodes.get_mut(root_id) {
                        node.widget = widget;
                    }
                    self.sync_tree_from_node(root_id);
                }
            }
        } else {
            let root_id = self.insert_node(None, widget, None, None);
            self.root = Some(root_id);
            self.sync_tree_from_node(root_id);
        }

        self.dirty = false;
    }

    pub fn root(&self) -> Option<ViewId> {
        self.root
    }

    pub fn get(&self, id: ViewId) -> Option<&ViewNode> {
        self.nodes.get(id)
    }

    pub fn get_mut(&mut self, id: ViewId) -> Option<&mut ViewNode> {
        self.nodes.get_mut(id)
    }

    pub fn iter_nodes(&self) -> impl Iterator<Item = (ViewId, &ViewNode)> {
        self.nodes.iter()
    }

    pub fn state_arena(&self) -> &StateArena {
        &self.state
    }

    pub fn state_arena_mut(&mut self) -> &mut StateArena {
        &mut self.state
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty || self.state.is_dirty()
    }

    pub fn clear_dirty(&mut self) {
        self.dirty = false;
        self.state.clear_dirty();
    }

    pub fn actions_mut(&mut self) -> &mut ActionStream {
        &mut self.actions
    }

    pub fn env_mut(&mut self) -> &mut Environment {
        &mut self.env
    }

    pub fn interaction(&self) -> &InteractionState {
        &self.interaction
    }

    pub fn interaction_mut(&mut self) -> &mut InteractionState {
        &mut self.interaction
    }

    pub fn ime_request(&self) -> ImeRequest {
        let focused_id = match self.interaction.focused_id {
            Some(id) => id,
            None => return ImeRequest::inactive(),
        };

        let node = match self.nodes.get(focused_id) {
            Some(n) => n,
            None => return ImeRequest::inactive(),
        };

        if node
            .widget
            .as_any()
            .downcast_ref::<super::widgets::textfield::TextField>()
            .is_some()
        {
            let bounds = self
                .resolved_bounds
                .get(&focused_id)
                .copied()
                .unwrap_or(node.bounds);
            ImeRequest::at_cursor(bounds)
        } else {
            ImeRequest::inactive()
        }
    }

    pub fn frame(&mut self, ui: &mut UiContext, root: &dyn BuildTrait, screen_size: Vec2) -> bool {
        self.current_time = ui.time;

        self.build_from(root);

        let root_id = self.root.unwrap();
        let mut taffy = std::mem::take(&mut self.taffy);
        {
            let fonts = ui.fonts.clone();
            let font_id = ui.current_font;
            let scale = ui.scale_factor;
            let measure = |content: &str, font_size: Option<crate::style::FontSize>| {
                let size = font_size
                    .unwrap_or(crate::style::FontSize::Medium)
                    .to_pixels();
                fonts.borrow().measure_text(font_id, content, size, scale)
            };
            taffy.sync(self, &measure);
        }
        let bounds = taffy.compute(root_id, screen_size, self);
        self.taffy = taffy;
        self.bounds_map = bounds;

        for (&id, &b) in &self.bounds_map {
            if let Some(node) = self.nodes.get_mut(id) {
                node.bounds = b;
            }
        }

        self.tick_animations(ui.time);

        self.resolve_positions();

        let (chain, traps) = focus::collect_focus_chain(self, &self.state);
        self.focus.set_focus_chain(chain, traps);
        if ui.input.key_pressed(crate::input::KeyCode::Tab) {
            if ui.input.is_key_down(crate::input::KeyCode::Shift) {
                self.focus.focus_prev();
            } else {
                self.focus.focus_next();
            }
            self.gamepad.set_focused(self.focus.focused());
        }

        // Gamepad directional navigation via arrow keys
        let direction = if ui.input.key_pressed(crate::input::KeyCode::ArrowUp) {
            Some(Direction::Up)
        } else if ui.input.key_pressed(crate::input::KeyCode::ArrowDown) {
            Some(Direction::Down)
        } else if ui.input.key_pressed(crate::input::KeyCode::ArrowLeft) {
            Some(Direction::Left)
        } else if ui.input.key_pressed(crate::input::KeyCode::ArrowRight) {
            Some(Direction::Right)
        } else {
            None
        };
        if let Some(dir) = direction {
            let chain_ids = self.focus.focus_chain_ids();
            let _ = self.gamepad.navigate(dir, &chain_ids, &self.bounds_map);
            self.focus.set_focused(self.gamepad.focused());
        } else {
            self.gamepad.set_focused(self.focus.focused());
        }

        let mut callbacks = std::mem::take(&mut self.callbacks);
        let resolved = std::mem::take(&mut self.resolved_bounds);
        let input_result = input::process_input(self, &ui.input, &mut callbacks, &resolved);
        let input_consumed = input_result.input_consumed;
        self.interaction.hovered_id = input_result.hovered_id;

        // Set focus when a focusable widget is clicked
        if let Some(clicked_id) = input_result.clicked_id {
            if let Some(node) = self.nodes.get(clicked_id) {
                if node.widget.focusable() {
                    self.focus.set_focused(Some(clicked_id));
                }
            }
        }
        self.interaction.focused_id = self.focus.focused();
        self.callbacks = callbacks;
        self.resolved_bounds = resolved;

        self.update_draggable_bounds();
        self.update_dock_child_bounds();

        if let Some(rid) = self.root {
            self.draw_recursive(rid, ui);
        }

        // Step 8: GC — clean orphaned StateArena entries
        {
            let live_ids: std::collections::HashSet<ViewId> =
                self.nodes.iter().map(|(id, _)| id).collect();
            self.state.gc(&live_ids);
        }

        self.clear_dirty();

        input_consumed
    }

    fn tick_animations(&mut self, current_time: f64) {
        let view_ids: Vec<ViewId> = self
            .nodes
            .iter()
            .filter(|(_, node)| !node.animations.is_empty() || !node.keyframe_animations.is_empty())
            .map(|(id, _)| id)
            .collect();

        for id in view_ids {
            let mut opacity: Option<f32> = None;
            let mut offset_x: Option<f32> = None;
            let mut offset_y: Option<f32> = None;
            let mut scale: Option<f32> = None;
            let mut corner_radius: Option<f32> = None;

            let mut completed_callbacks: Vec<u32> = Vec::new();

            let anim_ticks: Vec<(bool, AnimatedProperty, f32, Option<u32>)> = self
                .nodes
                .get(id)
                .map(|node| {
                    let simple: Vec<_> = node
                        .animations
                        .iter()
                        .map(|a| {
                            (
                                a.is_complete(current_time),
                                a.property(),
                                a.value_at(current_time),
                                a.on_complete_id(),
                            )
                        })
                        .collect();
                    let keyframe: Vec<_> = node
                        .keyframe_animations
                        .iter()
                        .map(|a| {
                            (
                                a.is_complete(current_time),
                                a.property(),
                                a.value_at(current_time),
                                a.on_complete_id(),
                            )
                        })
                        .collect();
                    let mut combined = simple;
                    combined.extend(keyframe);
                    combined
                })
                .unwrap_or_default();

            for (is_complete, property, value, cb_id) in &anim_ticks {
                if *is_complete {
                    if let Some(cb) = cb_id {
                        completed_callbacks.push(*cb);
                    }
                } else {
                    match property {
                        AnimatedProperty::Opacity => opacity = Some(*value),
                        AnimatedProperty::OffsetX => offset_x = Some(*value),
                        AnimatedProperty::OffsetY => offset_y = Some(*value),
                        AnimatedProperty::Scale => scale = Some(*value),
                        AnimatedProperty::CornerRadius => corner_radius = Some(*value),
                    }
                }
            }

            if let Some(node) = self.nodes.get_mut(id) {
                node.animations.retain(|a| !a.is_complete(current_time));
                node.keyframe_animations
                    .retain(|a| !a.is_complete(current_time));

                let offset = match (offset_x, offset_y) {
                    (Some(ox), Some(oy)) => Some(Vec2::new(ox, oy)),
                    (Some(ox), None) => Some(Vec2::new(ox, 0.0)),
                    (None, Some(oy)) => Some(Vec2::new(0.0, oy)),
                    _ => None,
                };

                node.animation_state = AnimationState {
                    opacity,
                    offset,
                    scale,
                    corner_radius,
                };
            }

            let mut callbacks = std::mem::take(&mut self.callbacks);
            let mut actions = std::mem::take(&mut self.actions);
            for cb_id in completed_callbacks {
                callbacks.invoke(&super::descriptor::Callback(cb_id), &mut actions);
            }
            self.callbacks = callbacks;
            self.actions = actions;
        }

        let to_remove: Vec<ViewId> = self
            .nodes
            .iter()
            .filter(|(_, node)| {
                node.pending_remove
                    && node.animations.is_empty()
                    && node.keyframe_animations.is_empty()
            })
            .map(|(id, _)| id)
            .collect();

        for id in to_remove {
            self.remove_node_recursive(id);
        }
    }

    // ── Position resolution ──────────────────────────────────────────────

    fn resolve_positions(&mut self) {
        let Some(root_id) = self.root else { return };
        self.resolved_bounds.clear();

        let (traversal, parent_map) = {
            let mut order = Vec::new();
            let mut parents = HashMap::new();
            let mut stack = vec![(root_id, root_id)];
            while let Some((id, parent_id)) = stack.pop() {
                order.push(id);
                parents.insert(id, parent_id);
                if let Some(node) = self.nodes.get(id) {
                    for &child in node.children.iter().rev() {
                        stack.push((child, id));
                    }
                }
            }
            (order, parents)
        };

        let root_bounds = self
            .nodes
            .get(root_id)
            .map(|n| n.animation_state.apply_to_bounds(n.bounds))
            .unwrap_or_default();
        self.resolved_bounds.insert(root_id, root_bounds);

        let mut translations: HashMap<ViewId, Vec2> = HashMap::new();
        translations.insert(root_id, Vec2::new(0.0, 0.0));

        let nodes = &self.nodes;
        let state = &self.state;
        let resolved = &mut self.resolved_bounds;

        for &node_id in traversal.iter().skip(1) {
            let Some(&parent_id) = parent_map.get(&node_id) else {
                continue;
            };
            let accumulated_translation = translations.get(&parent_id).copied().unwrap_or_default();
            let parent_bounds = resolved.get(&parent_id).copied().unwrap_or_default();

            let Some(node) = nodes.get(node_id) else {
                continue;
            };

            let mut bounds = node.animation_state.apply_to_bounds(node.bounds);
            bounds = bounds.translate(accumulated_translation);

            let delta = node.widget.resolve_position_delta(
                bounds,
                parent_bounds,
                node.zstack_alignment,
                state,
            );
            bounds = bounds.translate(delta);

            resolved.insert(node_id, bounds);
            translations.insert(node_id, accumulated_translation + delta);
        }
    }

    fn update_draggable_bounds(&mut self) {
        let patches: Vec<(ViewId, Vec2)> = self
            .nodes
            .iter()
            .filter_map(|(id, node)| {
                let dp = node
                    .widget
                    .as_any()
                    .downcast_ref::<super::widgets::draggable_panel::DraggablePanel>()?;
                let panel_state: DraggablePanelState =
                    self.state.get(dp.state_id).unwrap_or_default();
                if panel_state.visibility.is_visible() {
                    panel_state.position.map(|pos| (id, pos))
                } else {
                    None
                }
            })
            .collect();

        for (id, pos) in patches {
            if let Some(bounds) = self.resolved_bounds.get_mut(&id) {
                let size = Vec2::new(bounds.width(), bounds.height());
                *bounds = Rect2D::new(pos, pos + size);
            }
        }
    }

    fn update_dock_child_bounds(&mut self) {
        use super::widgets::dock_space::{DockSpace, compute_leaf_info};
        use crate::dock::DockTree;

        let patches: Vec<(Vec<ViewId>, Vec<katla_math::Rect2D>)> = self
            .nodes
            .iter()
            .filter_map(|(id, node)| {
                let ds = node.widget.as_any().downcast_ref::<DockSpace<u32>>()?;

                let tree: DockTree<u32> = self.state.get(ds.dock_state_id)?;
                let dock_bounds = self.resolved_bounds.get(&id).copied()?;

                let leaf_info = compute_leaf_info(tree.root(), dock_bounds, ds.tab_bar_height);
                let child_ids: Vec<ViewId> = node.children.clone();
                if child_ids.len() != leaf_info.len() {
                    return None;
                }

                let child_rects: Vec<katla_math::Rect2D> =
                    leaf_info.iter().map(|info| info.content_bounds).collect();

                Some((child_ids, child_rects))
            })
            .collect();

        for (child_ids, child_rects) in patches {
            for (child_id, rect) in child_ids.iter().zip(child_rects.iter()) {
                if let Some(bounds) = self.resolved_bounds.get_mut(child_id) {
                    *bounds = *rect;
                }
            }
        }
    }

    // ── Drawing ─────────────────────────────────────────────────────────

    fn draw_recursive(&self, node_id: ViewId, ui: &mut UiContext) {
        let Some(node) = self.nodes.get(node_id) else {
            return;
        };
        let children: Vec<ViewId> = node.children.clone();
        let anim_state = node.animation_state;

        let bounds = self
            .resolved_bounds
            .get(&node_id)
            .copied()
            .unwrap_or_default();

        let children_bounds: Vec<Rect2D> = children
            .iter()
            .filter_map(|&id| self.resolved_bounds.get(&id).copied())
            .collect();

        let needs_clip = node.widget.needs_clip_children();
        if needs_clip {
            ui.push_clip(bounds);
        }

        let draw_interaction = super::widget::DrawInteraction {
            hovered_id: self.interaction.hovered_id,
            active_id: self.interaction.active_id,
            focused_id: self.interaction.focused_id,
        };

        node.widget.draw(
            ui,
            &self.state,
            bounds,
            &anim_state,
            &children,
            &draw_interaction,
            node_id,
            &children_bounds,
        );

        let scroll_offset = node.widget.scroll_offset(&self.state);
        let skip_children = !node.widget.should_draw_children(&self.state);

        if !skip_children {
            for &child_id in &children {
                self.draw_child_recursive(child_id, ui, bounds, scroll_offset);
            }
        }

        node.widget
            .draw_after_children(ui, &self.state, bounds, &children, &children_bounds);

        if needs_clip {
            ui.pop_clip();
        }
    }

    fn draw_child_recursive(
        &self,
        child_id: ViewId,
        ui: &mut UiContext,
        parent_bounds: Rect2D,
        parent_scroll_offset: f32,
    ) {
        let Some(child_node) = self.nodes.get(child_id) else {
            return;
        };
        let anim_state = child_node.animation_state;

        let child_bounds = self
            .resolved_bounds
            .get(&child_id)
            .copied()
            .unwrap_or_default();

        let draw_bounds = if parent_scroll_offset != 0.0 {
            child_bounds
                .intersection(&parent_bounds)
                .unwrap_or(Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(0.0, 0.0)))
                .translate(Vec2::new(0.0, -parent_scroll_offset))
        } else {
            child_bounds
        };

        let grandchildren: Vec<ViewId> = child_node.children.clone();
        let grandchildren_bounds: Vec<Rect2D> = grandchildren
            .iter()
            .filter_map(|&id| self.resolved_bounds.get(&id).copied())
            .collect();

        let child_needs_clip = child_node.widget.needs_clip_children();
        if child_needs_clip {
            ui.push_clip(draw_bounds);
        }

        let draw_interaction = super::widget::DrawInteraction {
            hovered_id: self.interaction.hovered_id,
            active_id: self.interaction.active_id,
            focused_id: self.interaction.focused_id,
        };

        child_node.widget.draw(
            ui,
            &self.state,
            draw_bounds,
            &anim_state,
            &grandchildren,
            &draw_interaction,
            child_id,
            &grandchildren_bounds,
        );

        let child_scroll = child_node.widget.scroll_offset(&self.state);
        let skip_children = !child_node.widget.should_draw_children(&self.state);

        if !skip_children {
            for &grandchild_id in &grandchildren {
                self.draw_child_recursive(grandchild_id, ui, draw_bounds, child_scroll);
            }
        }

        if child_needs_clip {
            ui.pop_clip();
        }
    }

    pub fn build_from<B: BuildTrait + ?Sized>(&mut self, builder: &B) {
        self.callbacks.clear();
        self.state.reset_slots();

        let root_id = self.root.unwrap_or_else(|| {
            let id = self.nodes.insert(ViewNode {
                widget: Box::new(super::widgets::empty::Empty),
                children: Vec::new(),
                parent: None,
                animations: Vec::new(),
                keyframe_animations: Vec::new(),
                animation_state: AnimationState::empty(),
                pending_remove: false,
                bounds: Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(0.0, 0.0)),
                state_version: 0,
                taffy_id: None,
                key: None,
                zstack_alignment: None,
            });
            self.root = Some(id);
            id
        });

        let mut ctx = BuildContext::new(
            root_id,
            &mut self.state,
            &mut self.callbacks,
            &mut self.actions,
            &self.env,
        );

        let widget = builder.build(&mut ctx);

        // Diff new widget against old, then store and sync children
        let action = if let Some(node) = self.nodes.get(root_id) {
            widget.diff_against(&*node.widget)
        } else {
            DiffAction::Replace
        };

        match action {
            DiffAction::Update => {
                if let Some(node) = self.nodes.get_mut(root_id) {
                    node.widget = widget;
                    node.state_version += 1;
                }
            }
            DiffAction::RecurseChildren => {
                if let Some(node) = self.nodes.get_mut(root_id) {
                    node.widget = widget;
                }
                self.sync_tree_from_node(root_id);
            }
            DiffAction::Replace => {
                let old_children: Vec<ViewId> = self
                    .nodes
                    .get(root_id)
                    .map(|n| n.children.clone())
                    .unwrap_or_default();
                for child_id in &old_children {
                    self.remove_node_recursive(*child_id);
                }
                if let Some(node) = self.nodes.get_mut(root_id) {
                    node.widget = widget;
                    node.children.clear();
                    node.state_version += 1;
                }
                self.sync_tree_from_node(root_id);
            }
        }

        self.dirty = false;
    }

    fn insert_node(
        &mut self,
        parent: Option<ViewId>,
        widget: Box<dyn Widget>,
        key: Option<u64>,
        zstack_alignment: Option<Alignment>,
    ) -> ViewId {
        let id = self.nodes.insert(ViewNode {
            widget,
            children: Vec::new(),
            parent,
            animations: Vec::new(),
            keyframe_animations: Vec::new(),
            animation_state: AnimationState::empty(),
            pending_remove: false,
            bounds: Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(0.0, 0.0)),
            state_version: 0,
            taffy_id: None,
            key,
            zstack_alignment,
        });
        if let Some(pid) = parent
            && let Some(p) = self.nodes.get_mut(pid)
        {
            p.children.push(id);
        }
        id
    }

    fn remove_node_recursive(&mut self, id: ViewId) {
        if let Some(node) = self.nodes.remove(id) {
            for child in node.children {
                self.remove_node_recursive(child);
            }
        }
    }

    fn insert_animation_range(property: &AnimatedProperty) -> (f32, f32) {
        match property {
            AnimatedProperty::Opacity => (0.0, 1.0),
            AnimatedProperty::OffsetY => (20.0, 0.0),
            AnimatedProperty::OffsetX => (20.0, 0.0),
            AnimatedProperty::Scale => (0.8, 1.0),
            AnimatedProperty::CornerRadius => (0.0, 1.0),
        }
    }

    fn start_insert_animation(node: &mut ViewNode, transition: &Transition, start_time: f64) {
        if let Some(ref config) = transition.insert {
            let (from, to) = Self::insert_animation_range(&transition.property);
            node.animations.push(Animation {
                property: transition.property,
                tween: Tween {
                    from,
                    to,
                    duration: config.duration,
                    easing: config.easing.clone(),
                },
                start_time,
                on_complete: None,
            });
        }
    }

    /// Core sync: compare the stored widget at `node_id` with itself,
    /// extract children via `take_children()`, and recurse.
    fn sync_tree_from_node(&mut self, node_id: ViewId) {
        let _old_widget_type = if let Some(node) = self.nodes.get(node_id) {
            node.widget.widget_type()
        } else {
            return;
        };

        // Diff the stored widget against itself to determine action
        // (This seems odd but the widget was just set via set_root or build_from)
        // Actually, we need to compare the NEW widget against the OLD widget that was there before.
        // But since build_from already stored the new widget, we need the old one.
        // Wait - this approach is wrong. Let me rethink.

        // The correct approach: build_from stored the new widget, and we call sync_tree_from_node.
        // But we need to compare new vs old. The old widget is gone since we replaced it.
        //
        // Solution: build_from should NOT store the widget first. Instead, sync_tree should
        // take the new widget, compare with the old, then store and recurse.

        // For now, let's assume the widget was just stored. We still need to extract children.
        // The diff is already done by the caller (set_root or build_from).
        // We just need to extract children and recurse.

        let child_info = {
            let node = self.nodes.get_mut(node_id).unwrap();
            node.widget.take_children()
        };

        self.recurse_children(node_id, child_info);
    }

    /// Recurse children extracted from a node's widget into the tree.
    fn recurse_children(&mut self, node_id: ViewId, child_info: ChildWidgets) {
        let old_children: Vec<ViewId> = self
            .nodes
            .get(node_id)
            .map(|n| n.children.clone())
            .unwrap_or_default();

        match child_info {
            ChildWidgets::None => {
                for child_id in &old_children {
                    self.remove_node_recursive(*child_id);
                }
                if let Some(node) = self.nodes.get_mut(node_id) {
                    node.children.clear();
                }
            }
            ChildWidgets::Single(child) => {
                let transition = self
                    .nodes
                    .get(node_id)
                    .and_then(|n| n.widget.as_transition())
                    .cloned();

                if let Some(ref trans) = transition {
                    self.sync_transition_child(node_id, &old_children, child, trans);
                } else {
                    if let Some(&child_id) = old_children.first() {
                        // Recurse into existing child with new widget
                        if let Some(node) = self.nodes.get_mut(child_id) {
                            node.widget = child;
                            node.state_version += 1;
                        }
                        self.sync_tree_from_node(child_id);
                    } else {
                        // Insert new child node
                        let child_id = self.insert_node(Some(node_id), child, None, None);
                        self.sync_tree_from_node(child_id);
                    }
                }
            }
            ChildWidgets::Multi(children) => {
                self.sync_keyed_multi(node_id, &old_children, children);
            }
            ChildWidgets::ZStack(children) => {
                let owned_children: Vec<(Option<u64>, Box<dyn Widget>)> =
                    children.into_iter().map(|(_, key, w)| (key, w)).collect();

                let alignments: Vec<Alignment> = {
                    let node = self.nodes.get(node_id).unwrap();
                    node.widget
                        .as_any()
                        .downcast_ref::<super::widgets::zstack::ZStack>()
                        .map(|z| z.child_widgets.iter().map(|(a, _)| *a).collect())
                        .unwrap_or_default()
                };

                self.sync_keyed_multi(node_id, &old_children, owned_children);

                // Propagate zstack alignment
                let child_ids: Vec<ViewId> = self
                    .nodes
                    .get(node_id)
                    .map(|n| n.children.clone())
                    .unwrap_or_default();
                for (i, &child_id) in child_ids.iter().enumerate() {
                    if i < alignments.len()
                        && let Some(child) = self.nodes.get_mut(child_id)
                    {
                        child.zstack_alignment = Some(alignments[i]);
                    }
                }
            }
            ChildWidgets::Transition { child, transition } => {
                self.sync_transition_child(node_id, &old_children, child, &transition);
            }
        }
    }

    fn sync_transition_child(
        &mut self,
        node_id: ViewId,
        old_children: &[ViewId],
        new_child: Box<dyn Widget>,
        transition: &Transition,
    ) {
        if let Some(&child_id) = old_children.first() {
            let was_transition = self
                .nodes
                .get(node_id)
                .map(|n| {
                    n.widget
                        .as_any()
                        .downcast_ref::<super::widgets::transition::TransitionContainer>()
                        .is_some()
                })
                .unwrap_or(false);

            if let Some(node) = self.nodes.get_mut(child_id) {
                node.widget = new_child;
                node.state_version += 1;
            }
            self.sync_tree_from_node(child_id);

            if !was_transition && let Some(node) = self.nodes.get_mut(child_id) {
                Self::start_insert_animation(node, transition, self.current_time);
            }
        } else {
            let child_id = self.insert_node(Some(node_id), new_child, None, None);
            self.sync_tree_from_node(child_id);
            if let Some(node) = self.nodes.get_mut(child_id) {
                Self::start_insert_animation(node, transition, self.current_time);
            }
        }
    }

    fn sync_keyed_multi(
        &mut self,
        node_id: ViewId,
        old_children: &[ViewId],
        mut new_children: Vec<(Option<u64>, Box<dyn Widget>)>,
    ) {
        let has_any_key = new_children.iter().any(|(k, _)| k.is_some());

        if !has_any_key {
            self.sync_by_index(node_id, old_children, new_children);
            return;
        }

        let old_keys: Vec<Option<u64>> = old_children
            .iter()
            .map(|&id| self.nodes.get(id).and_then(|n| n.key))
            .collect();

        let mut matched: Vec<Option<usize>> = vec![None; new_children.len()];
        let mut old_matched: Vec<bool> = vec![false; old_children.len()];

        for (new_i, (key, _)) in new_children.iter().enumerate() {
            if let Some(k) = key {
                for (old_i, old_key) in old_keys.iter().enumerate() {
                    if !old_matched[old_i] && *old_key == Some(*k) {
                        matched[new_i] = Some(old_i);
                        old_matched[old_i] = true;
                        break;
                    }
                }
            }
        }

        for (new_i, _) in new_children.iter().enumerate() {
            if matched[new_i].is_none() {
                for (old_i, was_matched) in old_matched.iter_mut().enumerate() {
                    if !*was_matched {
                        matched[new_i] = Some(old_i);
                        *was_matched = true;
                        break;
                    }
                }
            }
        }

        let mut new_child_ids: Vec<ViewId> = Vec::with_capacity(new_children.len());

        for (new_i, (key, widget)) in new_children.drain(..).enumerate() {
            if let Some(old_i) = matched[new_i] {
                let old_id = old_children[old_i];
                if let Some(node) = self.nodes.get_mut(old_id) {
                    node.widget = widget;
                    node.state_version += 1;
                }
                self.sync_tree_from_node(old_id);
                new_child_ids.push(old_id);
            } else {
                let child_id = self.insert_node(Some(node_id), widget, key, None);
                self.sync_tree_from_node(child_id);
                new_child_ids.push(child_id);
            }
        }

        for (old_i, &was_matched) in old_matched.iter().enumerate() {
            if !was_matched {
                self.remove_node_recursive(old_children[old_i]);
            }
        }

        if let Some(node) = self.nodes.get_mut(node_id) {
            node.children = new_child_ids;
        }
    }

    fn sync_by_index(
        &mut self,
        node_id: ViewId,
        old_children: &[ViewId],
        mut new_children: Vec<(Option<u64>, Box<dyn Widget>)>,
    ) {
        let new_count = new_children.len();

        for (i, (key, widget)) in new_children.drain(..).enumerate() {
            if i < old_children.len() {
                if let Some(node) = self.nodes.get_mut(old_children[i]) {
                    node.widget = widget;
                    node.state_version += 1;
                }
                self.sync_tree_from_node(old_children[i]);
            } else {
                let child_id = self.insert_node(Some(node_id), widget, key, None);
                self.sync_tree_from_node(child_id);
            }
        }

        if old_children.len() > new_count {
            if let Some(node) = self.nodes.get_mut(node_id) {
                node.children.truncate(new_count);
            }
            for old_id in old_children[new_count..].iter() {
                self.remove_node_recursive(*old_id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::declarative::constructors::*;
    use crate::declarative::widget::WidgetBox;

    fn child_ids(tree: &ViewTree) -> Vec<ViewId> {
        let root = tree.root().unwrap();
        tree.get(root)
            .map(|n| n.children.clone())
            .unwrap_or_default()
    }

    #[test]
    fn test_sync_tree_mount_unmount_children() {
        let mut tree = ViewTree::new();

        tree.set_root(vstack([text("a").boxed(), text("b").boxed(), text("c").boxed()]).boxed());
        let root = tree.root().unwrap();
        assert_eq!(tree.get(root).unwrap().children.len(), 3);

        tree.set_root(vstack([text("a").boxed(), text("b").boxed()]).boxed());
        assert_eq!(tree.get(root).unwrap().children.len(), 2);

        tree.set_root(
            vstack([
                text("a").boxed(),
                text("b").boxed(),
                text("c").boxed(),
                text("d").boxed(),
            ])
            .boxed(),
        );
        assert_eq!(tree.get(root).unwrap().children.len(), 4);
    }

    #[test]
    fn test_keyed_reorder_preserves_identity() {
        let mut tree = ViewTree::new();

        tree.set_root(
            vstack_keyed(vec![
                keyed(1, text("first").boxed()),
                keyed(2, text("second").boxed()),
                keyed(3, text("third").boxed()),
            ])
            .boxed(),
        );
        let ids_before = child_ids(&tree);
        assert_eq!(ids_before.len(), 3);

        tree.set_root(
            vstack_keyed(vec![
                keyed(3, text("third").boxed()),
                keyed(1, text("first").boxed()),
                keyed(2, text("second").boxed()),
            ])
            .boxed(),
        );
        let ids_after = child_ids(&tree);
        assert_eq!(ids_after.len(), 3);

        assert_eq!(ids_before[0], ids_after[1], "key=1 should map to same node");
        assert_eq!(ids_before[1], ids_after[2], "key=2 should map to same node");
        assert_eq!(ids_before[2], ids_after[0], "key=3 should map to same node");
    }

    #[test]
    fn test_unkeyed_children_use_index_matching() {
        let mut tree = ViewTree::new();

        tree.set_root(
            vstack([
                text("first").boxed(),
                text("second").boxed(),
                text("third").boxed(),
            ])
            .boxed(),
        );
        let ids_before = child_ids(&tree);
        assert_eq!(ids_before.len(), 3);

        tree.set_root(
            vstack([
                text("third").boxed(),
                text("first").boxed(),
                text("second").boxed(),
            ])
            .boxed(),
        );
        let ids_after = child_ids(&tree);
        assert_eq!(ids_after.len(), 3);

        assert_eq!(
            ids_before[0], ids_after[0],
            "unkeyed: index 0 maps to index 0"
        );
        assert_eq!(
            ids_before[1], ids_after[1],
            "unkeyed: index 1 maps to index 1"
        );
        assert_eq!(
            ids_before[2], ids_after[2],
            "unkeyed: index 2 maps to index 2"
        );
    }

    #[test]
    fn test_keyed_add_and_remove() {
        let mut tree = ViewTree::new();

        tree.set_root(
            vstack_keyed(vec![
                keyed(1, text("a").boxed()),
                keyed(2, text("b").boxed()),
            ])
            .boxed(),
        );
        let ids_a = child_ids(&tree);
        assert_eq!(ids_a.len(), 2);

        tree.set_root(
            vstack_keyed(vec![
                keyed(2, text("b").boxed()),
                keyed(3, text("c").boxed()),
            ])
            .boxed(),
        );
        let ids_b = child_ids(&tree);
        assert_eq!(ids_b.len(), 2);

        assert_eq!(
            ids_a[1], ids_b[0],
            "key=2 survives from first to second build"
        );
    }

    fn transition_container(
        child: Box<dyn crate::declarative::widget::Widget>,
        transition: Transition,
    ) -> Box<dyn crate::declarative::widget::Widget> {
        crate::declarative::constructors::wrap_transition_container(child, transition).boxed()
    }

    fn first_child_id(tree: &ViewTree) -> Option<ViewId> {
        let root = tree.root()?;
        let root_node = tree.get(root)?;
        root_node.children.first().copied()
    }

    #[test]
    fn test_transition_container_insert_starts_animation() {
        let mut tree = ViewTree::new();
        let transition = Transition::fade(0.3);

        tree.set_root(transition_container(text("hello").boxed(), transition));

        let child_id = first_child_id(&tree).expect("should have a child");
        let child = tree.get(child_id).expect("child node should exist");
        assert!(
            !child.animations.is_empty(),
            "child should have insert animation"
        );
        assert_eq!(child.animations.len(), 1);
        assert!(
            matches!(child.animations[0].property, AnimatedProperty::Opacity),
            "animation property should be Opacity for fade transition"
        );
        assert_eq!(child.animations[0].tween.from, 0.0);
        assert_eq!(child.animations[0].tween.to, 1.0);
        assert!((child.animations[0].tween.duration - 0.3).abs() < f64::EPSILON);
    }

    #[test]
    fn test_transition_container_child_widget_updated() {
        let mut tree = ViewTree::new();
        let transition = Transition::fade(0.3);

        tree.set_root(transition_container(text("A").boxed(), transition.clone()));

        let child_id = first_child_id(&tree).expect("should have a child");
        let child = tree.get(child_id).unwrap();
        let text_widget = child
            .widget
            .as_any()
            .downcast_ref::<super::super::widgets::text::Text>()
            .expect("should be Text widget");
        assert_eq!(text_widget.content, "A");

        tree.set_root(transition_container(text("B").boxed(), transition));

        let child = tree.get(child_id).expect("same child should persist");
        let text_widget = child
            .widget
            .as_any()
            .downcast_ref::<super::super::widgets::text::Text>()
            .expect("should be Text widget");
        assert_eq!(text_widget.content, "B");
    }

    #[test]
    fn test_transition_container_preserves_child_across_rebuilds() {
        let mut tree = ViewTree::new();
        let transition = Transition::fade(0.3);

        tree.set_root(transition_container(
            text("first").boxed(),
            transition.clone(),
        ));

        let child_id_first = first_child_id(&tree).expect("should have child after first build");

        tree.set_root(transition_container(text("second").boxed(), transition));

        let child_id_second = first_child_id(&tree).expect("should have child after second build");
        assert_eq!(
            child_id_first, child_id_second,
            "child ViewId should be stable across rebuilds"
        );
    }

    #[test]
    fn test_sync_tree_update_preserves_state() {
        let mut tree = ViewTree::new();

        tree.set_root(vstack([text("hello").boxed()]).boxed());
        let root = tree.root().unwrap();
        let text_id = tree.get(root).unwrap().children[0];

        let state_id = tree.state_arena_mut().get_or_create(text_id, 42_i32);

        tree.set_root(vstack([text("world").boxed()]).boxed());

        let root_after = tree.root().unwrap();
        let text_id_after = tree.get(root_after).unwrap().children[0];
        assert_eq!(root, root_after);
        assert_eq!(text_id, text_id_after);
        assert_eq!(tree.state_arena().get::<i32>(state_id).unwrap(), 42);
    }

    #[test]
    fn test_sync_tree_replace_replaces_node() {
        let mut tree = ViewTree::new();

        tree.set_root(text("hello").boxed());
        let root = tree.root().unwrap();
        let v0 = tree.get(root).unwrap().state_version;

        tree.set_root(button("hello").boxed());
        let root_after = tree.root().unwrap();
        assert_eq!(root, root_after);

        let node = tree.get(root).unwrap();
        assert!(
            node.widget
                .as_any()
                .downcast_ref::<super::super::widgets::button::Button>()
                .is_some()
        );
        assert!(node.state_version > v0);
    }

    #[test]
    fn test_sync_tree_empty_to_content_and_back() {
        let mut tree = ViewTree::new();

        tree.set_root(empty().boxed());
        let root = tree.root().unwrap();
        assert!(
            tree.get(root)
                .unwrap()
                .widget
                .as_any()
                .downcast_ref::<super::super::widgets::empty::Empty>()
                .is_some()
        );

        tree.set_root(text("hello").boxed());
        assert_eq!(tree.root().unwrap(), root);
        assert!(
            tree.get(root)
                .unwrap()
                .widget
                .as_any()
                .downcast_ref::<super::super::widgets::text::Text>()
                .is_some()
        );

        tree.set_root(empty().boxed());
        assert!(
            tree.get(root)
                .unwrap()
                .widget
                .as_any()
                .downcast_ref::<super::super::widgets::empty::Empty>()
                .is_some()
        );
    }

    #[test]
    fn test_sync_tree_preserves_node_identity_on_recurse() {
        let mut tree = ViewTree::new();

        tree.set_root(panel("My Panel", text("content").boxed()).boxed());
        let root = tree.root().unwrap();
        let child_id = tree.get(root).unwrap().children[0];

        tree.set_root(panel("My Panel", text("updated").boxed()).boxed());
        let root_after = tree.root().unwrap();
        let child_id_after = tree.get(root_after).unwrap().children[0];

        assert_eq!(root, root_after, "Panel should keep same ViewId");
        assert_eq!(child_id, child_id_after, "Text should keep same ViewId");

        let text_widget = tree
            .get(child_id_after)
            .unwrap()
            .widget
            .as_any()
            .downcast_ref::<super::super::widgets::text::Text>()
            .expect("should be Text widget");
        assert_eq!(text_widget.content, "updated");
    }

    #[test]
    fn test_gc_removes_orphaned_state_after_node_removal() {
        let mut tree = ViewTree::new();

        tree.set_root(vstack([text("a").boxed(), text("b").boxed(), text("c").boxed()]).boxed());
        let root = tree.root().unwrap();
        let children = tree.get(root).unwrap().children.clone();
        assert_eq!(children.len(), 3);

        // Create state for all three children
        let sid_a = tree.state_arena_mut().get_or_create(children[0], 1_i32);
        let sid_b = tree.state_arena_mut().get_or_create(children[1], 2_i32);
        let sid_c = tree.state_arena_mut().get_or_create(children[2], 3_i32);

        // Remove two children by rebuilding with only one
        tree.set_root(vstack([text("a").boxed()]).boxed());

        // Run GC manually (set_root doesn't trigger frame)
        let live_ids: std::collections::HashSet<ViewId> =
            tree.nodes.iter().map(|(id, _)| id).collect();
        tree.state.gc(&live_ids);

        // Only surviving child's state should remain
        let root_after = tree.root().unwrap();
        let surviving_children = tree.get(root_after).unwrap().children.clone();
        assert_eq!(surviving_children.len(), 1);
        assert_eq!(surviving_children[0], children[0]);

        assert!(tree.state.get::<i32>(sid_a).is_some(), "a survives");
        assert!(tree.state.get::<i32>(sid_b).is_none(), "b orphaned");
        assert!(tree.state.get::<i32>(sid_c).is_none(), "c orphaned");
    }

    #[test]
    fn test_gc_no_leak_over_1000_frames() {
        let mut tree = ViewTree::new();

        for i in 0..1000 {
            if i % 2 == 0 {
                tree.set_root(
                    vstack([text("a").boxed(), text("b").boxed(), text("c").boxed()]).boxed(),
                );
            } else {
                tree.set_root(vstack([text("a").boxed()]).boxed());
            }

            // Run GC
            let live_ids: std::collections::HashSet<ViewId> =
                tree.nodes.iter().map(|(id, _)| id).collect();
            tree.state.gc(&live_ids);
        }

        let live_ids: std::collections::HashSet<ViewId> =
            tree.nodes.iter().map(|(id, _)| id).collect();
        let live_count = live_ids.len();
        assert!(
            tree.state.cell_count() <= live_count * 2,
            "arena should stay bounded, got {} cells for {} live nodes",
            tree.state.cell_count(),
            live_count
        );
    }

    // ── Focus chain integration tests ──────────────────────────────────

    #[test]
    fn test_focus_chain_collects_from_widget_tree() {
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

        let (chain, _) = super::super::focus::collect_focus_chain(&tree, tree.state_arena());
        assert_eq!(chain.len(), 2, "only buttons should be focusable");
    }

    #[test]
    fn test_focus_chain_panel_creates_scope() {
        let mut tree = ViewTree::new();
        let mut callbacks = crate::declarative::build::CallbackTable::new();
        let cb1 = callbacks.push(|_| {});
        let cb2 = callbacks.push(|_| {});
        let cb3 = callbacks.push(|_| {});

        tree.set_root(
            vstack([
                button("Outside").on_click(cb1).boxed(),
                panel(
                    "MyPanel",
                    vstack([
                        button("Inside1").on_click(cb2).boxed(),
                        button("Inside2").on_click(cb3).boxed(),
                    ])
                    .boxed(),
                )
                .boxed(),
            ])
            .boxed(),
        );

        let (chain, traps) = super::super::focus::collect_focus_chain(&tree, tree.state_arena());
        assert_eq!(chain.len(), 3, "three focusable buttons total");
        assert!(traps.is_empty(), "panels should not trap");

        let outside_scope = chain
            .iter()
            .find(|(id, _)| {
                tree.get(*id)
                    .unwrap()
                    .widget
                    .as_any()
                    .downcast_ref::<super::super::widgets::button::Button>()
                    .map(|b| b.label == "Outside")
                    .unwrap_or(false)
            })
            .unwrap();
        assert!(outside_scope.1.is_none(), "outside button has no scope");

        let inside_buttons: Vec<_> = chain
            .iter()
            .filter(|(id, _)| {
                tree.get(*id)
                    .unwrap()
                    .widget
                    .as_any()
                    .downcast_ref::<super::super::widgets::button::Button>()
                    .map(|b| b.label.starts_with("Inside"))
                    .unwrap_or(false)
            })
            .collect();
        assert_eq!(inside_buttons.len(), 2);
        assert!(
            inside_buttons[0].1.is_some(),
            "buttons inside panel should have a scope"
        );
    }

    #[test]
    fn test_focus_chain_modal_creates_trap_when_open() {
        let mut tree = ViewTree::new();
        let mut callbacks = crate::declarative::build::CallbackTable::new();
        let cb1 = callbacks.push(|_| {});
        let cb2 = callbacks.push(|_| {});
        let cb3 = callbacks.push(|_| {});

        let root_id = ViewId::from(slotmap::KeyData::from_ffi(0));
        let open_id = tree.state_arena_mut().get_or_create(root_id, true);

        tree.set_root(
            vstack([
                button("Background").on_click(cb1).boxed(),
                modal(
                    400.0,
                    300.0,
                    open_id,
                    vstack([
                        button("ModalBtn1").on_click(cb2).boxed(),
                        button("ModalBtn2").on_click(cb3).boxed(),
                    ])
                    .boxed(),
                )
                .boxed(),
            ])
            .boxed(),
        );

        let (chain, traps) = super::super::focus::collect_focus_chain(&tree, tree.state_arena());
        assert_eq!(chain.len(), 3, "three focusable buttons total");
        assert_eq!(traps.len(), 1, "modal should be a trap when open");

        let modal_btns: Vec<_> = chain
            .iter()
            .filter(|(id, _)| {
                tree.get(*id)
                    .unwrap()
                    .widget
                    .as_any()
                    .downcast_ref::<super::super::widgets::button::Button>()
                    .map(|b| b.label.starts_with("ModalBtn"))
                    .unwrap_or(false)
            })
            .collect();
        assert_eq!(modal_btns.len(), 2);
        let scope_id = modal_btns[0].1.unwrap();
        assert!(traps.contains(&scope_id), "modal scope should be in traps");
    }

    #[test]
    fn test_focus_chain_modal_no_trap_when_closed() {
        let mut tree = ViewTree::new();
        let mut callbacks = crate::declarative::build::CallbackTable::new();
        let cb1 = callbacks.push(|_| {});
        let cb2 = callbacks.push(|_| {});

        let root_id = ViewId::from(slotmap::KeyData::from_ffi(0));
        let open_id = tree.state_arena_mut().get_or_create(root_id, false);

        tree.set_root(
            vstack([
                button("Background").on_click(cb1).boxed(),
                modal(
                    400.0,
                    300.0,
                    open_id,
                    button("ModalBtn").on_click(cb2).boxed(),
                )
                .boxed(),
            ])
            .boxed(),
        );

        let (chain, traps) = super::super::focus::collect_focus_chain(&tree, tree.state_arena());
        assert_eq!(chain.len(), 2);
        assert!(traps.is_empty(), "closed modal should not trap");
    }

    #[test]
    fn test_focus_chain_draggable_panel_creates_scope() {
        let mut tree = ViewTree::new();
        let mut callbacks = crate::declarative::build::CallbackTable::new();
        let cb1 = callbacks.push(|_| {});
        let cb2 = callbacks.push(|_| {});

        let root_id = ViewId::from(slotmap::KeyData::from_ffi(0));
        let state_id = tree.state_arena_mut().get_or_create(
            root_id,
            super::super::descriptor::DraggablePanelState::default(),
        );

        tree.set_root(
            vstack([
                button("Outside").on_click(cb1).boxed(),
                draggable_panel(
                    "Float",
                    200.0,
                    300.0,
                    button("Inside").on_click(cb2).boxed(),
                    state_id,
                )
                .boxed(),
            ])
            .boxed(),
        );

        let (chain, traps) = super::super::focus::collect_focus_chain(&tree, tree.state_arena());
        assert_eq!(chain.len(), 2);
        assert!(traps.is_empty(), "draggable panel should not trap");

        let inside = chain
            .iter()
            .find(|(id, _)| {
                tree.get(*id)
                    .unwrap()
                    .widget
                    .as_any()
                    .downcast_ref::<super::super::widgets::button::Button>()
                    .map(|b| b.label == "Inside")
                    .unwrap_or(false)
            })
            .unwrap();
        assert!(
            inside.1.is_some(),
            "button inside draggable panel should have scope"
        );
    }
}
