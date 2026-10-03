//! App-owned image drag/drop; retained hit testing supplies visibility and clipping.

use super::super::EditorAction;
use katla_agent::{material::MaterialOp, material_sampling::TextureRole};
use katla_ecs::EntityId;
use katla_math::{Rect2D, Vec2};
use katla_ui::{
    UiContext,
    declarative::{
        Callback, Widget,
        animation::AnimationState,
        diff::DiffAction,
        selectable,
        state::{StateArena, ViewId},
        widget::{ChildWidgets, DrawInfo, InputContext, InputResult, MeasureFn},
        widgets::selectable::Selectable,
    },
    input::mouse_button,
};
use std::{any::Any, cell::RefCell, path::PathBuf, rc::Rc};

#[derive(Clone, Default)]
pub(crate) struct MaterialDrag(pub(crate) Rc<RefCell<Option<ImageDrag>>>);
pub(crate) struct ImageDrag {
    path: PathBuf,
    start: Vec2,
}
pub(crate) enum DragRole {
    Source(PathBuf),
    Target { entity: EntityId, role: TextureRole },
}
pub(crate) struct ImageDragWidget {
    pub(crate) role: DragRole,
    drag: MaterialDrag,
    inner: Selectable,
}
impl ImageDragWidget {
    pub(crate) fn new(
        role: DragRole,
        drag: MaterialDrag,
        child: Box<dyn Widget>,
        callback: Callback,
        selected: bool,
    ) -> Self {
        Self {
            role,
            drag,
            inner: selectable(child).selected(selected).on_click(callback),
        }
    }
}
impl Widget for ImageDragWidget {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn diff_against(&self, prev: &dyn Widget) -> DiffAction {
        if prev.as_any().is::<Self>() {
            DiffAction::Update
        } else {
            DiffAction::Replace
        }
    }
    fn layout_style(&self, measure: MeasureFn<'_>) -> taffy::Style {
        self.inner.layout_style(measure)
    }
    fn handle_input(
        &self,
        ctx: &mut InputContext<'_>,
        state: &mut StateArena,
        bounds: Rect2D,
        children: &[ViewId],
    ) -> InputResult {
        if !bounds.contains(ctx.mouse_pos) {
            return InputResult::Ignore;
        }
        match &self.role {
            DragRole::Source(path) if ctx.input.mouse_clicked(mouse_button::LEFT) => {
                *self.drag.0.borrow_mut() = Some(ImageDrag {
                    path: path.clone(),
                    start: ctx.mouse_pos,
                });
            }
            DragRole::Target { entity, role } if ctx.input.mouse_released[mouse_button::LEFT] => {
                let source = self.drag.0.borrow_mut().take();
                if let Some(source) = source {
                    let delta = ctx.mouse_pos - source.start;
                    if delta.x() * delta.x() + delta.y() * delta.y() >= 16. {
                        ctx.actions
                            .emit(EditorAction::MaterialPreset(MaterialOp::SetTexture {
                                entity_ids: vec![entity.id().to_string()],
                                role: *role,
                                source: serde_json::json!({
                                    "kind": "file",
                                    "asset": crate::scene::AssetRef::File(source.path),
                                }),
                            }));
                        if let Some(callback) = &self.inner.on_click {
                            ctx.callbacks.invoke(callback, ctx.actions);
                        }
                        return InputResult::Consumed;
                    }
                }
            }
            _ => {}
        }
        self.inner.handle_input(ctx, state, bounds, children)
    }
    fn draw(
        &self,
        ctx: &mut UiContext,
        state: &StateArena,
        bounds: Rect2D,
        animation: &AnimationState,
        children: &[ViewId],
        info: &DrawInfo<'_>,
    ) {
        self.inner
            .draw(ctx, state, bounds, animation, children, info);
    }
    fn take_children(&mut self) -> ChildWidgets {
        self.inner.take_children()
    }
    fn children(&self) -> &[ViewId] {
        self.inner.children()
    }
    fn children_mut(&mut self) -> &mut Vec<ViewId> {
        self.inner.children_mut()
    }
    fn interactive(&self, _: &StateArena) -> bool {
        true
    }
    fn focusable(&self) -> bool {
        true
    }
    fn press_action(&self) -> Option<Callback> {
        self.inner.press_action()
    }
}
