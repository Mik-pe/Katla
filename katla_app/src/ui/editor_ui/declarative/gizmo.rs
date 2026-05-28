use katla_math::{Rect2D, Vec2};
use katla_ui::declarative::{Build, BuildContext, ViewDescriptor};
use katla_ui::{UiContext, widgets::RadioButton};

use crate::ui::EditorAction;

#[derive(Clone)]
pub(crate) struct GizmoDrawCtx {
    pub gizmo_mode: u8,
    pub viewport_bounds: Rect2D,
    pub actions: Vec<EditorAction>,
}

pub struct GizmoButtonsView;

impl Build for GizmoButtonsView {
    fn build(&self, _ctx: &mut BuildContext) -> ViewDescriptor {
        ViewDescriptor::Custom(draw_gizmo_buttons)
    }
}

fn draw_gizmo_buttons(ui: &mut UiContext, _bounds: Rect2D) {
    let mut ctx = match ui.get_scratch::<GizmoDrawCtx>().cloned() {
        Some(c) => c,
        None => return,
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

    ui.set_scratch(ctx);
}
