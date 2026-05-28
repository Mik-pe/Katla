use std::collections::HashMap;

use slotmap::SlotMap;
use taffy::NodeId as TaffyNodeId;

use katla_math::{Rect2D, Vec2};

use crate::context::UiContext;

use super::actions::ActionStream;
use super::animation::{AnimatedProperty, Animation, AnimationState, KeyframeAnimation, Tween};
use super::build::{Build as BuildTrait, BuildContext, CallbackTable, Environment};
use super::descriptor::{Anchor, Callback, ViewDescriptor};
use super::diff::{DiffAction, Patch, diff_descriptor};
use super::draw::draw_descriptor_with_id;
use super::focus::{self, FocusManager};
use super::ime::ImeRequest;
use super::input;
use super::layout::TaffyNodeMap;
use super::state::{StateArena, ViewId};
use super::transition::Transition;
use crate::style::FontSize;

pub struct ViewNode {
    pub descriptor: ViewDescriptor,
    pub children: Vec<ViewId>,
    pub parent: Option<ViewId>,
    pub animations: Vec<Animation>,
    pub keyframe_animations: Vec<KeyframeAnimation>,
    pub animation_state: AnimationState,
    pub pending_remove: bool,
    pub bounds: Rect2D,
    pub state_version: u32,
    pub taffy_id: Option<TaffyNodeId>,
}

/// Tracks interactive state across frames for the declarative view tree.
///
/// Analogous to the immediate mode `active_id`/`hovered_id`/`focused_id` pattern,
/// but stored on the retained tree for cross-frame interactions like slider drags.
#[derive(Default)]
pub struct InteractionState {
    /// Node being actively pressed/dragged (e.g. slider thumb mid-drag).
    pub active_id: Option<ViewId>,
    /// Node currently under the mouse cursor.
    pub hovered_id: Option<ViewId>,
    /// Node with keyboard focus (synced with FocusManager).
    pub focused_id: Option<ViewId>,
    /// For Vec3Slider: which axis (0, 1, 2) is being dragged.
    pub drag_axis: Option<usize>,
}

pub struct ViewTree {
    nodes: SlotMap<ViewId, ViewNode>,
    state: StateArena,
    root: Option<ViewId>,
    dirty: bool,
    callbacks: CallbackTable,
    actions: ActionStream,
    env: Environment,
    focus: FocusManager,
    taffy: TaffyNodeMap,
    bounds_map: HashMap<ViewId, Rect2D>,
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
            focus: FocusManager::new(),
            taffy: TaffyNodeMap::new(),
            bounds_map: HashMap::new(),
            interaction: InteractionState::default(),
            current_time: 0.0,
        }
    }
}

impl ViewTree {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_root(&mut self, descriptor: ViewDescriptor) {
        self.callbacks.clear();

        if let Some(root_id) = self.root {
            let patches = self.diff_against(root_id, &descriptor);
            self.apply_patches(&patches);
            if let Some(node) = self.nodes.get_mut(root_id) {
                node.descriptor = descriptor;
            }
        } else {
            let root_id = self.insert_node(None, ViewDescriptor::Empty);
            self.root = Some(root_id);
            self.sync_tree(root_id, &descriptor);
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

    /// Access the interaction state (read-only).
    pub fn interaction(&self) -> &InteractionState {
        &self.interaction
    }

    /// Access the interaction state (mutable).
    pub fn interaction_mut(&mut self) -> &mut InteractionState {
        &mut self.interaction
    }

    /// Get the current IME request from the focused text field (if any).
    ///
    /// Returns an active ImeRequest with the TextField's bounds as the cursor
    /// position if a TextField is focused, or an inactive request otherwise.
    pub fn ime_request(&self) -> ImeRequest {
        let focused_id = match self.interaction.focused_id {
            Some(id) => id,
            None => return ImeRequest::inactive(),
        };

        let node = match self.nodes.get(focused_id) {
            Some(n) => n,
            None => return ImeRequest::inactive(),
        };

        match &node.descriptor {
            ViewDescriptor::TextField { .. } => ImeRequest::at_cursor(node.bounds),
            _ => ImeRequest::inactive(),
        }
    }

    /// Run one full frame: build → diff → layout → tick animations → input → draw.
    pub fn frame(&mut self, ui: &mut UiContext, root: &dyn BuildTrait, screen_size: Vec2) -> bool {
        // Store time so sync_tree can use it for animation start times
        self.current_time = ui.time;

        // 1. Build the descriptor tree from root
        self.build_from(root);

        // 2. Sync Taffy layout and compute bounds
        let root_id = self.root.unwrap();
        let mut taffy = std::mem::take(&mut self.taffy);
        {
            let fonts = ui.fonts.clone();
            let font_id = ui.current_font;
            let scale = ui.scale_factor;
            let measure = |content: &str, font_size: Option<FontSize>| {
                let size = font_size.unwrap_or(FontSize::Medium).to_pixels();
                fonts.borrow().measure_text(font_id, content, size, scale)
            };
            taffy.sync(self, &measure);
        }
        let bounds = taffy.compute(root_id, screen_size, self);
        self.taffy = taffy;
        self.bounds_map = bounds;

        // 3. Store computed bounds on each ViewNode
        for (&id, &b) in &self.bounds_map {
            if let Some(node) = self.nodes.get_mut(id) {
                node.bounds = b;
            }
        }

        // 4. Tick all animations and compute AnimationState per node
        self.tick_animations(ui.time);

        // 5. Rebuild focus chain and process Tab navigation
        let chain = focus::collect_focus_chain(self);
        self.focus.set_focus_chain(chain);
        if ui.input.key_pressed(crate::input::KeyCode::Tab) {
            if ui.input.is_key_down(crate::input::KeyCode::Shift) {
                self.focus.focus_prev();
            } else {
                self.focus.focus_next();
            }
        }

        // 6. Process input (hit test, dispatch callbacks)
        let mut callbacks = std::mem::take(&mut self.callbacks);
        let bounds_map = self.bounds_map.clone();
        let input_result = input::process_input(self, &ui.input, &mut callbacks, &bounds_map);
        let input_consumed = input_result.input_consumed;
        self.interaction.hovered_id = input_result.hovered_id;
        self.interaction.focused_id = self.focus.focused();
        self.callbacks = callbacks;

        // 7. Walk tree and draw each node
        if let Some(rid) = self.root {
            self.draw_recursive(rid, ui);
        }

        // 8. Clear dirty flags
        self.clear_dirty();

        input_consumed
    }

    /// Tick all animations: compute current values, remove completed, resolve AnimationState.
    fn tick_animations(&mut self, current_time: f64) {
        // Collect all view IDs that have animations
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

            // Collect on_complete callbacks from completed animations
            let mut completed_callbacks: Vec<u32> = Vec::new();

            // Tick simple animations
            let animations = if let Some(node) = self.nodes.get(id) {
                node.animations.clone()
            } else {
                continue;
            };

            for anim in &animations {
                if anim.is_complete(current_time) {
                    if let Some(ref cb) = anim.on_complete {
                        completed_callbacks.push(cb.0);
                    }
                } else {
                    let value = anim.value_at(current_time);
                    match anim.property {
                        AnimatedProperty::Opacity => opacity = Some(value),
                        AnimatedProperty::OffsetX => offset_x = Some(value),
                        AnimatedProperty::OffsetY => offset_y = Some(value),
                        AnimatedProperty::Scale => scale = Some(value),
                        AnimatedProperty::CornerRadius => corner_radius = Some(value),
                    }
                }
            }

            // Tick keyframe animations
            let kf_animations = if let Some(node) = self.nodes.get(id) {
                node.keyframe_animations.clone()
            } else {
                continue;
            };

            for kf_anim in &kf_animations {
                if kf_anim.is_complete(current_time) {
                    if let Some(ref cb) = kf_anim.on_complete {
                        completed_callbacks.push(cb.0);
                    }
                } else {
                    let value = kf_anim.value_at(current_time);
                    match kf_anim.property {
                        AnimatedProperty::Opacity => opacity = Some(value),
                        AnimatedProperty::OffsetX => offset_x = Some(value),
                        AnimatedProperty::OffsetY => offset_y = Some(value),
                        AnimatedProperty::Scale => scale = Some(value),
                        AnimatedProperty::CornerRadius => corner_radius = Some(value),
                    }
                }
            }

            // Remove completed animations and update state
            if let Some(node) = self.nodes.get_mut(id) {
                node.animations.retain(|a| !a.is_complete(current_time));
                node.keyframe_animations
                    .retain(|a| !a.is_complete(current_time));

                // Build combined offset
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

            // Fire on_complete callbacks
            let mut callbacks = std::mem::take(&mut self.callbacks);
            for cb_id in completed_callbacks {
                callbacks.invoke(&Callback(cb_id));
            }
            self.callbacks = callbacks;
        }

        // Remove nodes that are pending_remove and have no active animations
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

    fn draw_recursive(&self, node_id: ViewId, ui: &mut UiContext) {
        let Some(node) = self.nodes.get(node_id) else {
            return;
        };
        let descriptor = &node.descriptor;
        let children: Vec<ViewId> = node.children.clone();
        let anim_state = node.animation_state.clone();

        // Apply animation state to bounds
        let bounds = anim_state.apply_to_bounds(node.bounds);

        // Collect children bounds for container draw calls
        let children_bounds: Vec<Rect2D> = children
            .iter()
            .filter_map(|&id| self.bounds_map.get(&id).copied())
            .collect();

        // Handle ScrollView clipping
        let is_scroll_view = matches!(descriptor, ViewDescriptor::ScrollView(_));
        if is_scroll_view {
            ui.push_clip(bounds);
        }

        // Draw this node
        draw_descriptor_with_id(
            descriptor,
            ui,
            bounds,
            &self.state,
            &children_bounds,
            &self.interaction,
            node_id,
            &anim_state,
        );

        // Compute scroll offset for children
        let scroll_offset = if let ViewDescriptor::ScrollView(desc) = descriptor {
            Some(self.state.get::<f32>(desc.scroll_state_id))
        } else {
            None
        };

        // Skip drawing children for collapsed Section
        let skip_children = if let ViewDescriptor::Section { expanded_id, .. } = descriptor {
            let expanded: bool = self.state.get(*expanded_id);
            !expanded
        } else {
            false
        };

        // Draw children
        if !skip_children {
            for &child_id in &children {
                self.draw_child_recursive(child_id, ui, bounds, descriptor, scroll_offset);
            }
        }

        if is_scroll_view {
            ui.pop_clip();
        }
    }

    fn draw_child_recursive(
        &self,
        child_id: ViewId,
        ui: &mut UiContext,
        parent_bounds: Rect2D,
        parent_descriptor: &ViewDescriptor,
        scroll_offset: Option<f32>,
    ) {
        let Some(child_node) = self.nodes.get(child_id) else {
            return;
        };

        let anim_state = child_node.animation_state.clone();
        let mut child_bounds = anim_state.apply_to_bounds(child_node.bounds);

        // Handle Overlay positioning
        if let ViewDescriptor::Overlay(overlay_desc) = &child_node.descriptor {
            let base_bounds = child_node.bounds;
            child_bounds = Self::resolve_overlay_bounds(
                overlay_desc.anchor,
                overlay_desc.offset,
                parent_bounds,
                base_bounds,
            );
            // Re-apply animation offset/scale on top of overlay positioning
            child_bounds = anim_state.apply_to_bounds(child_bounds);
        }

        // Clip children to parent for ScrollView and apply scroll offset
        let is_scroll_content = matches!(parent_descriptor, ViewDescriptor::ScrollView(_));
        if is_scroll_content {
            child_bounds = child_bounds
                .intersection(&parent_bounds)
                .unwrap_or(Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(0.0, 0.0)));
        }

        // Apply inherited scroll offset from ancestor ScrollView
        if let Some(offset) = scroll_offset {
            child_bounds = child_bounds.translate(Vec2::new(0.0, -offset));
        }

        let grandchildren: Vec<ViewId> = child_node.children.clone();
        let grandchildren_bounds: Vec<Rect2D> = grandchildren
            .iter()
            .filter_map(|&id| self.bounds_map.get(&id).copied())
            .collect();

        let is_scroll_view = matches!(child_node.descriptor, ViewDescriptor::ScrollView(_));
        if is_scroll_view {
            ui.push_clip(child_bounds);
        }

        draw_descriptor_with_id(
            &child_node.descriptor,
            ui,
            child_bounds,
            &self.state,
            &grandchildren_bounds,
            &self.interaction,
            child_id,
            &anim_state,
        );

        // Compute scroll offset for this node's children
        let child_scroll = if let ViewDescriptor::ScrollView(desc) = &child_node.descriptor {
            Some(self.state.get::<f32>(desc.scroll_state_id))
        } else {
            scroll_offset
        };

        for &grandchild_id in &grandchildren {
            self.draw_child_recursive(
                grandchild_id,
                ui,
                child_bounds,
                &child_node.descriptor,
                child_scroll,
            );
        }

        if is_scroll_view {
            ui.pop_clip();
        }
    }

    fn resolve_overlay_bounds(
        anchor: Anchor,
        offset: Vec2,
        parent_bounds: Rect2D,
        content_bounds: Rect2D,
    ) -> Rect2D {
        let pw = parent_bounds.width();
        let ph = parent_bounds.height();
        let cw = content_bounds.width();
        let ch = content_bounds.height();

        let pos = match anchor {
            Anchor::TopLeft => parent_bounds.min,
            Anchor::TopRight => Vec2::new(parent_bounds.max.x() - cw, parent_bounds.min.y()),
            Anchor::BottomLeft => Vec2::new(parent_bounds.min.x(), parent_bounds.max.y() - ch),
            Anchor::BottomRight => {
                Vec2::new(parent_bounds.max.x() - cw, parent_bounds.max.y() - ch)
            }
            Anchor::TopCenter => Vec2::new(
                parent_bounds.min.x() + (pw - cw) * 0.5,
                parent_bounds.min.y(),
            ),
            Anchor::BottomCenter => Vec2::new(
                parent_bounds.min.x() + (pw - cw) * 0.5,
                parent_bounds.max.y() - ch,
            ),
            Anchor::Center => Vec2::new(
                parent_bounds.min.x() + (pw - cw) * 0.5,
                parent_bounds.min.y() + (ph - ch) * 0.5,
            ),
        };

        let origin = pos + offset;
        Rect2D::new(origin, Vec2::new(origin.x() + cw, origin.y() + ch))
    }

    pub fn build_from<B: BuildTrait + ?Sized>(&mut self, builder: &B) {
        self.callbacks.clear();
        self.state.reset_slots();

        let root_id = self.root.unwrap_or_else(|| {
            let id = self.nodes.insert(ViewNode {
                descriptor: ViewDescriptor::Empty,
                children: Vec::new(),
                parent: None,
                animations: Vec::new(),
                keyframe_animations: Vec::new(),
                animation_state: AnimationState::empty(),
                pending_remove: false,
                bounds: Rect2D::new(
                    katla_math::Vec2::new(0.0, 0.0),
                    katla_math::Vec2::new(0.0, 0.0),
                ),
                state_version: 0,
                taffy_id: None,
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

        let descriptor = builder.build(&mut ctx);

        self.sync_tree(root_id, &descriptor);
        self.dirty = false;
    }

    fn insert_node(&mut self, parent: Option<ViewId>, descriptor: ViewDescriptor) -> ViewId {
        let id = self.nodes.insert(ViewNode {
            descriptor,
            children: Vec::new(),
            parent,
            animations: Vec::new(),
            keyframe_animations: Vec::new(),
            animation_state: AnimationState::empty(),
            pending_remove: false,
            bounds: Rect2D::new(
                katla_math::Vec2::new(0.0, 0.0),
                katla_math::Vec2::new(0.0, 0.0),
            ),
            state_version: 0,
            taffy_id: None,
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

    fn collect_children(descriptor: &ViewDescriptor) -> &[ViewDescriptor] {
        match descriptor {
            ViewDescriptor::HStack(s) | ViewDescriptor::VStack(s) => &s.children,
            ViewDescriptor::Grid(s) => &s.children,
            ViewDescriptor::ZStack(_) => &[],
            ViewDescriptor::ScrollView(_) => &[],
            ViewDescriptor::Panel(_) => &[],
            ViewDescriptor::Overlay(_) => &[],
            _ => &[],
        }
    }

    fn get_single_child(descriptor: &ViewDescriptor) -> Option<&ViewDescriptor> {
        match descriptor {
            ViewDescriptor::ScrollView(s) => Some(&s.content),
            ViewDescriptor::Panel(s) => Some(&s.content),
            ViewDescriptor::Overlay(s) => Some(&s.content),
            ViewDescriptor::StatusBar(s) => Some(&s.content),
            ViewDescriptor::DraggablePanel(s) => Some(&s.content),
            ViewDescriptor::Modal(s) => Some(&s.content),
            ViewDescriptor::TransitionContainer { child, .. } => Some(child),
            ViewDescriptor::Selectable { child, .. } => Some(child),
            ViewDescriptor::Section { child, .. } => Some(child),
            ViewDescriptor::TabBar(s) => Some(&s.content),
            _ => None,
        }
    }

    fn get_transition(descriptor: &ViewDescriptor) -> Option<&Transition> {
        match descriptor {
            ViewDescriptor::TransitionContainer { transition, .. } => Some(transition),
            _ => None,
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

    fn remove_animation_range(property: &AnimatedProperty) -> (f32, f32) {
        match property {
            AnimatedProperty::Opacity => (1.0, 0.0),
            AnimatedProperty::OffsetY => (0.0, -20.0),
            AnimatedProperty::OffsetX => (0.0, -20.0),
            AnimatedProperty::Scale => (1.0, 0.8),
            AnimatedProperty::CornerRadius => (1.0, 0.0),
        }
    }

    fn start_insert_animation(node: &mut ViewNode, transition: &Transition, start_time: f64) {
        if let Some(ref config) = transition.insert {
            let (from, to) = Self::insert_animation_range(&transition.property);
            node.animations.push(Animation {
                property: transition.property.clone(),
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

    fn start_remove_animation(node: &mut ViewNode, transition: &Transition, start_time: f64) {
        if let Some(ref config) = transition.remove {
            let (from, to) = Self::remove_animation_range(&transition.property);
            node.animations.push(Animation {
                property: transition.property.clone(),
                tween: Tween {
                    from,
                    to,
                    duration: config.duration,
                    easing: config.easing.clone(),
                },
                start_time,
                on_complete: None,
            });
            node.pending_remove = true;
        }
    }

    fn sync_tree(&mut self, node_id: ViewId, descriptor: &ViewDescriptor) {
        let old_descriptor = if let Some(node) = self.nodes.get(node_id) {
            node.descriptor.clone()
        } else {
            return;
        };

        let action = diff_descriptor(&old_descriptor, descriptor);

        match action {
            DiffAction::Update => {
                if let Some(node) = self.nodes.get_mut(node_id) {
                    node.descriptor = descriptor.clone();
                    node.state_version += 1;
                }
            }
            DiffAction::RecurseChildren => {
                if let Some(node) = self.nodes.get_mut(node_id) {
                    node.descriptor = descriptor.clone();
                }

                // Handle TransitionContainer single-child with animation support
                if let Some(transition) = Self::get_transition(descriptor) {
                    let transition_clone = transition.clone();
                    let new_child = Self::get_single_child(descriptor).unwrap();
                    let old_children: Vec<ViewId> = self
                        .nodes
                        .get(node_id)
                        .map(|n| n.children.clone())
                        .unwrap_or_default();

                    let was_transition =
                        matches!(old_descriptor, ViewDescriptor::TransitionContainer { .. });

                    if let Some(&child_id) = old_children.first() {
                        if was_transition {
                            // Both old and new are TransitionContainer with a child — recurse
                            self.sync_tree(child_id, new_child);
                        } else {
                            // Old was non-transition, now is transition: insert animation
                            self.sync_tree(child_id, new_child);
                            if let Some(node) = self.nodes.get_mut(child_id) {
                                Self::start_insert_animation(
                                    node,
                                    &transition_clone,
                                    self.current_time,
                                );
                            }
                        }
                    } else {
                        // No old child — insert new child with insert animation
                        let child_id = self.insert_node(Some(node_id), new_child.clone());
                        self.sync_tree(child_id, new_child);
                        if let Some(node) = self.nodes.get_mut(child_id) {
                            Self::start_insert_animation(
                                node,
                                &transition_clone,
                                self.current_time,
                            );
                        }
                    }
                    return;
                }

                // Detect removal when going from TransitionContainer → Empty
                if matches!(descriptor, ViewDescriptor::Empty) {
                    if let ViewDescriptor::TransitionContainer { transition, .. } = &old_descriptor
                    {
                        let old_children: Vec<ViewId> = self
                            .nodes
                            .get(node_id)
                            .map(|n| n.children.clone())
                            .unwrap_or_default();
                        if let Some(&child_id) = old_children.first() {
                            if let Some(node) = self.nodes.get_mut(child_id) {
                                Self::start_remove_animation(node, transition, self.current_time);
                            }
                        }
                    }
                }

                // Handle single-child containers
                if let Some(new_child) = Self::get_single_child(descriptor) {
                    let old_children: Vec<ViewId> = self
                        .nodes
                        .get(node_id)
                        .map(|n| n.children.clone())
                        .unwrap_or_default();

                    if let Some(&child_id) = old_children.first() {
                        self.sync_tree(child_id, new_child);
                    } else {
                        let child_id = self.insert_node(Some(node_id), new_child.clone());
                        self.sync_tree(child_id, new_child);
                    }
                    return;
                }

                // Handle multi-child containers
                let old_children: Vec<ViewId> = self
                    .nodes
                    .get(node_id)
                    .map(|n| n.children.clone())
                    .unwrap_or_default();

                // Handle ZStack separately
                if let ViewDescriptor::ZStack(zstack) = descriptor {
                    let new_count = zstack.children.len();
                    for (i, (_, child_desc)) in zstack.children.iter().enumerate() {
                        if i < old_children.len() {
                            self.sync_tree(old_children[i], child_desc);
                        } else {
                            let child_id = self.insert_node(Some(node_id), child_desc.clone());
                            self.sync_tree(child_id, child_desc);
                        }
                    }
                    // Remove excess old children
                    if old_children.len() > new_count {
                        if let Some(node) = self.nodes.get_mut(node_id) {
                            node.children.truncate(new_count);
                        }
                        for old_id in old_children[new_count..].iter() {
                            self.remove_node_recursive(*old_id);
                        }
                    }
                    return;
                }

                let new_children_descs = Self::collect_children(descriptor);
                let new_count = new_children_descs.len();

                for (i, child_desc) in new_children_descs.iter().enumerate() {
                    if i < old_children.len() {
                        self.sync_tree(old_children[i], child_desc);
                    } else {
                        let child_id = self.insert_node(Some(node_id), child_desc.clone());
                        self.sync_tree(child_id, child_desc);
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
            DiffAction::Replace => {
                // Detect TransitionContainer → Empty: start remove animation on child
                if matches!(descriptor, ViewDescriptor::Empty) {
                    if let ViewDescriptor::TransitionContainer { transition, .. } = &old_descriptor
                    {
                        let old_children: Vec<ViewId> = self
                            .nodes
                            .get(node_id)
                            .map(|n| n.children.clone())
                            .unwrap_or_default();
                        if let Some(&child_id) = old_children.first() {
                            if let Some(node) = self.nodes.get_mut(child_id) {
                                Self::start_remove_animation(node, transition, self.current_time);
                            }
                        }
                        // Update descriptor but keep child alive for animation
                        if let Some(node) = self.nodes.get_mut(node_id) {
                            node.descriptor = descriptor.clone();
                            node.state_version += 1;
                        }
                        return;
                    }
                }

                // Detect Empty → TransitionContainer: insert child with insert animation
                if let ViewDescriptor::TransitionContainer { transition, .. } = descriptor {
                    let old_children: Vec<ViewId> = self
                        .nodes
                        .get(node_id)
                        .map(|n| n.children.clone())
                        .unwrap_or_default();
                    // Remove old children (if any from previous state)
                    for child_id in &old_children {
                        self.remove_node_recursive(*child_id);
                    }
                    if let Some(node) = self.nodes.get_mut(node_id) {
                        node.descriptor = descriptor.clone();
                        node.children.clear();
                        node.state_version += 1;
                    }
                    if let Some(new_child) = Self::get_single_child(descriptor) {
                        let child_id = self.insert_node(Some(node_id), new_child.clone());
                        self.sync_tree(child_id, new_child);
                        if let Some(node) = self.nodes.get_mut(child_id) {
                            Self::start_insert_animation(node, transition, self.current_time);
                        }
                    }
                    return;
                }

                let old_children: Vec<ViewId> = self
                    .nodes
                    .get(node_id)
                    .map(|n| n.children.clone())
                    .unwrap_or_default();
                for child_id in &old_children {
                    self.remove_node_recursive(*child_id);
                }
                if let Some(node) = self.nodes.get_mut(node_id) {
                    node.descriptor = descriptor.clone();
                    node.children.clear();
                    node.state_version += 1;
                }

                let new_children_descs = Self::collect_children(descriptor);
                for child_desc in new_children_descs {
                    let child_id = self.insert_node(Some(node_id), child_desc.clone());
                    self.sync_tree(child_id, child_desc);
                }

                if let Some(new_child) = Self::get_single_child(descriptor) {
                    let child_id = self.insert_node(Some(node_id), new_child.clone());
                    self.sync_tree(child_id, new_child);
                }

                if let ViewDescriptor::ZStack(zstack) = descriptor {
                    for (_, child_desc) in &zstack.children {
                        let child_id = self.insert_node(Some(node_id), child_desc.clone());
                        self.sync_tree(child_id, child_desc);
                    }
                }
            }
        }
    }

    fn diff_against(&self, node_id: ViewId, new: &ViewDescriptor) -> Vec<Patch> {
        let mut patches = Vec::new();
        self.diff_recursive(node_id, new, &mut patches);
        patches
    }

    fn diff_recursive(&self, node_id: ViewId, new: &ViewDescriptor, patches: &mut Vec<Patch>) {
        let old = match self.nodes.get(node_id) {
            Some(n) => &n.descriptor,
            None => return,
        };

        match diff_descriptor(old, new) {
            DiffAction::Update => {
                patches.push(Patch::Update {
                    node: node_id,
                    descriptor: new.clone(),
                });
            }
            DiffAction::RecurseChildren => {
                patches.push(Patch::Update {
                    node: node_id,
                    descriptor: new.clone(),
                });
                // Recurse into children (handled by sync_tree in practice)
            }
            DiffAction::Replace => {
                patches.push(Patch::Remove { node: node_id });
                patches.push(Patch::Insert {
                    parent: self.nodes.get(node_id).and_then(|n| n.parent),
                    index: 0,
                    descriptor: new.clone(),
                });
            }
        }
    }

    fn apply_patches(&mut self, patches: &[Patch]) {
        for patch in patches {
            match patch {
                Patch::Insert {
                    parent,
                    index: _,
                    descriptor,
                } => {
                    self.insert_node(*parent, descriptor.clone());
                }
                Patch::Update { node, descriptor } => {
                    if let Some(n) = self.nodes.get_mut(*node) {
                        n.descriptor = descriptor.clone();
                        n.state_version += 1;
                    }
                }
                Patch::Remove { node } => {
                    self.remove_node_recursive(*node);
                }
            }
        }
    }
}
