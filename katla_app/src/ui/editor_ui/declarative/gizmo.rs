use std::boxed::Box;

use katla_ui::ForkAwesome;
use katla_ui::declarative::{Build, BuildContext, Widget, WidgetBox, hstack, tool_label_button};

#[derive(Clone)]
pub(crate) struct GizmoDrawCtx {
    pub gizmo_mode: u8,
}

/// Action emitted when the gizmo mode changes via the segmented tool group.
pub(crate) struct GizmoModeChanged(pub u8);

pub struct GizmoButtonsView;

/// Compact editor tool group (Move/Rotate/Scale) overlaid on the viewport.
/// Selected state uses the accent; inactive tools stay fully legible.
impl Build for GizmoButtonsView {
    fn build(&self, ctx: &mut BuildContext) -> Box<dyn Widget> {
        let mode_from_env = ctx
            .env::<GizmoDrawCtx>()
            .map(|c| c.gizmo_mode as usize)
            .unwrap_or(0);

        let modes: [(usize, char, &str); 3] = [
            (0, ForkAwesome::CROSSHAIRS, "Move"),
            (1, ForkAwesome::REFRESH, "Rotate"),
            (2, ForkAwesome::EXPAND, "Scale"),
        ];

        let children: Vec<Box<dyn Widget>> = modes
            .iter()
            .map(|&(index, icon, label)| {
                tool_label_button(icon, label)
                    .selected(mode_from_env == index)
                    .on_click(ctx.on_click(move |actions| {
                        actions.emit(GizmoModeChanged(index as u8));
                    }))
                    .boxed()
            })
            .collect();

        hstack(children).spacing(2.0).boxed()
    }
}
