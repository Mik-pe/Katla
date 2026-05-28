use katla_ui::declarative::{Build, BuildContext, StateId, ViewDescriptor, hstack, radio};

#[derive(Clone)]
pub(crate) struct GizmoDrawCtx {
    pub gizmo_mode: u8,
}

/// Action emitted when the gizmo mode changes via the declarative radio buttons.
pub(crate) struct GizmoModeChanged(pub u8);

pub struct GizmoButtonsView;

impl Build for GizmoButtonsView {
    fn build(&self, ctx: &mut BuildContext) -> ViewDescriptor {
        let mode_id: StateId = ctx.state(0usize);

        let mode_from_env = ctx
            .env::<GizmoDrawCtx>()
            .map(|c| c.gizmo_mode as usize)
            .unwrap_or(0);

        let current: usize = ctx.get_state(mode_id);

        // If the state arena differs from what the app set, the radio buttons
        // were clicked during the previous frame's input pass. Emit the change.
        if current != mode_from_env {
            ctx.emit(GizmoModeChanged(current as u8));
        }

        // Sync the state arena to the app's authoritative value for this frame.
        ctx.set_state(mode_id, mode_from_env);

        let modes: [(usize, &str); 3] = [(0, "W:Move"), (1, "E:Rotate"), (2, "R:Scale")];

        let children: Vec<ViewDescriptor> = modes
            .map(|(index, label)| radio(mode_id, index, label))
            .to_vec();

        hstack(children).spacing(2.0).padding_all(10.0)
    }
}
