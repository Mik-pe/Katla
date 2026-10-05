use std::boxed::Box;

use katla_ui::ForkAwesome;
use katla_ui::declarative::{Build, BuildContext, Widget, WidgetBox, hstack, tool_button};

#[derive(Clone)]
pub(crate) struct GizmoDrawCtx {
    pub gizmo_mode: u8,
}

/// Action emitted when the gizmo mode changes via the segmented tool group.
pub(crate) struct GizmoModeChanged(pub u8);

pub struct GizmoButtonsView;

/// Compact icon-only editor tool group (Move/Rotate/Scale) overlaid on the
/// viewport. Shortcuts W/E/R are handled by the application's gizmo input;
/// the tooltips surface them.
impl Build for GizmoButtonsView {
    fn build(&self, ctx: &mut BuildContext) -> Box<dyn Widget> {
        let mode_from_env = ctx
            .env::<GizmoDrawCtx>()
            .map(|c| c.gizmo_mode as usize)
            .unwrap_or(0);

        let modes: [(usize, char, &str); 3] = [
            (0, ForkAwesome::CROSSHAIRS, "Move (W)"),
            (1, ForkAwesome::REFRESH, "Rotate (E)"),
            (2, ForkAwesome::EXPAND, "Scale (R)"),
        ];

        let children: Vec<Box<dyn Widget>> = modes
            .iter()
            .map(|&(index, icon, tooltip)| {
                tool_button(icon)
                    .selected(mode_from_env == index)
                    .tooltip(tooltip)
                    .on_click(ctx.on_click(move |actions| {
                        actions.emit(GizmoModeChanged(index as u8));
                    }))
                    .boxed()
            })
            .collect();

        hstack(children).spacing(2.0).boxed()
    }
}
