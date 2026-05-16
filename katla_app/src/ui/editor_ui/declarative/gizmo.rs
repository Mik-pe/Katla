use std::cell::RefCell;

use katla_math::{Rect2D, Vec2};
use katla_ui::declarative::{Build, BuildContext, ViewDescriptor};
use katla_ui::{UiContext, widgets::RadioButton};

use crate::ui::EditorAction;

thread_local! {
    static GIZMO_CTX: RefCell<Option<GizmoDrawCtx>> = const { RefCell::new(None) };
}

struct GizmoDrawCtx {
    gizmo_mode: u8,
    viewport_bounds: Rect2D,
    actions: Vec<EditorAction>,
}

pub struct GizmoButtonsView;

pub fn set_gizmo_ctx(gizmo_mode: u8, viewport_bounds: Rect2D) {
    GIZMO_CTX.with(|c| {
        *c.borrow_mut() = Some(GizmoDrawCtx {
            gizmo_mode,
            viewport_bounds,
            actions: Vec::new(),
        })
    });
}

pub fn take_gizmo_actions() -> Vec<EditorAction> {
    GIZMO_CTX.with(|c| {
        c.borrow_mut()
            .as_mut()
            .map(|ctx| std::mem::take(&mut ctx.actions))
            .unwrap_or_default()
    })
}

impl Build for GizmoButtonsView {
    fn build(&self, _ctx: &mut BuildContext) -> ViewDescriptor {
        ViewDescriptor::Custom(draw_gizmo_buttons)
    }
}

fn draw_gizmo_buttons(ui: &mut UiContext, _bounds: Rect2D) {
    let ctx = GIZMO_CTX.with(|c| c.borrow_mut().take());
    let Some(mut ctx) = ctx else {
        return;
    };

    let gizmo_modes: &[(usize, &str)] = &[(0, "W:Move"), (1, "E:Rotate"), (2, "R:Scale")];
    let gizmo_button_width = 85.0;
    let gizmo_button_height = 24.0;
    let gizmo_padding = 10.0;
    let gizmo_start_x = ctx.viewport_bounds.min.x() + gizmo_padding;
    let gizmo_start_y = ctx.viewport_bounds.min.y() + gizmo_padding + 16.0;

    let mut selected = ctx.gizmo_mode as usize;

    for &(index, label) in gizmo_modes {
        let btn_x = gizmo_start_x + index as f32 * (gizmo_button_width + 2.0);
        let btn_bounds = Rect2D::from_origin_size(
            Vec2::new(btn_x, gizmo_start_y),
            Vec2::new(gizmo_button_width, gizmo_button_height),
        );

        if ui
            .add(
                RadioButton::new(&mut selected, index, label)
                    .bounds(btn_bounds)
                    .id(&format!("gizmo_{label}")),
            )
            .changed
        {
            ctx.actions.push(EditorAction::SetGizmoMode(selected as u8));
        }
    }

    GIZMO_CTX.with(|c| *c.borrow_mut() = Some(ctx));
}
