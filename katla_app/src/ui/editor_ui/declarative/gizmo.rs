use std::boxed::Box;

use katla_ui::declarative::{Build, BuildContext, StateId, Widget, WidgetBox, hstack, radio};

#[derive(Clone)]
pub(crate) struct GizmoDrawCtx {
    pub gizmo_mode: u8,
}

/// Action emitted when the gizmo mode changes via the declarative radio buttons.
pub(crate) struct GizmoModeChanged(pub u8);

pub struct GizmoButtonsView;

impl Build for GizmoButtonsView {
    fn build(&self, ctx: &mut BuildContext) -> Box<dyn Widget> {
        let mode_id: StateId = ctx.state(0usize);

        let mode_from_env = ctx
            .env::<GizmoDrawCtx>()
            .map(|c| c.gizmo_mode as usize)
            .unwrap_or(0);

        let current: usize = ctx.get_state(mode_id).unwrap();

        if current != mode_from_env {
            ctx.emit(GizmoModeChanged(current as u8));
        }

        ctx.set_state(mode_id, mode_from_env);

        let modes: [(usize, &str); 3] = [(0, "Move"), (1, "Rotate"), (2, "Scale")];

        let children: Vec<Box<dyn Widget>> = modes
            .iter()
            .map(|&(index, label)| radio(mode_id, index, label).boxed())
            .collect();

        hstack(children).spacing(4.0).padding_all(4.0).boxed()
    }
}
